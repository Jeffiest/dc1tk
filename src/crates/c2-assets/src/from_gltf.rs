//! MDS meshes (and .wgt skin weights and the .bbp bind pose) rebuilt from a glTF, with the original
//! MDS as the template: the frame hierarchy stays, and mesh blocks are replaced by name (a glTF mesh
//! node "X" or "X_skin" replaces the MDT of frame X; Blender's ".001" suffixes are ignored).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::json::{self, Value};
use crate::mds::{self, Material, Mds, Mdt, Node, Prim, IDENT};
use crate::mot::{self, KeyList};
use crate::py::{self, Num};
use crate::to_gltf::{mat_inv, mat_mul, normalize, xform_dir, xform_point, CollisionAttributes, Matrix};
use crate::{bail, Error, Result};

/// A glTF's JSON and its buffers.
pub struct Gltf {
    pub json: Value,
    pub buffers: Vec<Vec<u8>>,
}

fn missing(key: &str) -> Error {
    Error::invalid(format!("'{key}'"))
}

/// d[key].
pub fn item<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
    value.get(key).ok_or_else(|| missing(key))
}

/// list[index] with Python's negative indexing; the index a JSON int.
pub fn index<'a>(list: &'a Value, at: &Value) -> Result<&'a Value> {
    let Some(items) = list.as_list() else {
        bail!("the value is not a list");
    };
    let Some(i) = at.as_int() else {
        bail!("list indices must be integers or slices");
    };
    let i = if i < 0 { items.len() as i64 + i } else { i };
    if i < 0 || i >= items.len() as i64 {
        bail!("list index out of range");
    }
    Ok(&items[i as usize])
}

fn base64(text: &str) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for c in text.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            _ => continue,
        };
        acc = (acc << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Ok(out)
}

fn utf8(data: &[u8]) -> Result<&str> {
    std::str::from_utf8(data).map_err(|_| Error::invalid("'utf-8' codec can't decode bytes"))
}

/// A .gltf (with its buffers read through `read`, by path relative to the glTF's folder) or a .glb.
pub fn load_gltf(raw: &[u8], read: &dyn Fn(&str) -> Result<Vec<u8>>) -> Result<Gltf> {
    if raw.starts_with(b"glTF") {
        let mut o = 12;
        let (mut found, mut bin) = (None, Vec::new());
        while o < raw.len() {
            if o + 8 > raw.len() {
                bail!("unpack_from requires a buffer of at least {} bytes", o + 8);
            }
            let length = u32::from_le_bytes(raw[o..o + 4].try_into().unwrap()) as usize;
            let chunk = &raw[o + 8..(o + 8 + length).min(raw.len())];
            match &raw[o + 4..o + 8] {
                b"JSON" => found = Some(json::loads(utf8(chunk)?)?),
                b"BIN\0" => bin = chunk.to_vec(),
                _ => {}
            }
            o += 8 + length;
        }
        let Some(json) = found else {
            bail!("'NoneType' object is not subscriptable");
        };
        return Ok(Gltf { json, buffers: vec![bin] });
    }
    let json = json::loads(utf8(raw)?)?;
    let mut buffers = Vec::new();
    let Some(list) = item(&json, "buffers")?.as_list() else {
        bail!("'buffers' is not a list");
    };
    for buffer in list {
        let Some(uri) = item(buffer, "uri")?.as_str() else {
            bail!("the buffer's uri is not text");
        };
        if uri.starts_with("data:") {
            let Some((_, data)) = uri.split_once(',') else {
                bail!("list index out of range");
            };
            buffers.push(base64(data)?);
        } else {
            buffers.push(read(uri)?);
        }
    }
    Ok(Gltf { json, buffers })
}

/// An accessor's values: `n` components each, as the Python reads them (ints or floats).
pub struct Values {
    pub n: usize,
    pub values: Vec<Num>,
}

impl Values {
    pub fn len(&self) -> usize {
        self.values.len().div_ceil(self.n)
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// The c-th value (Python's negative indexing).
    pub fn at(&self, c: i64) -> Result<&[Num]> {
        let count = self.len() as i64;
        let i = if c < 0 { count + c } else { c };
        if i < 0 || i >= count {
            bail!("list index out of range");
        }
        let start = i as usize * self.n;
        Ok(&self.values[start..(start + self.n).min(self.values.len())])
    }

    /// repr() of the list the Python's accessor() returns.
    pub fn repr(&self) -> String {
        let number = |v: &Num| match v {
            Num::Int(i) => i.to_string(),
            Num::Float(f) => crate::py::float_repr(*f),
        };
        let mut out = String::from("[");
        for (k, chunk) in self.values.chunks(self.n.max(1)).enumerate() {
            if k > 0 {
                out.push_str(", ");
            }
            if self.n == 1 {
                out.push_str(&number(&chunk[0]));
            } else {
                out.push('(');
                out.push_str(&chunk.iter().map(number).collect::<Vec<_>>().join(", "));
                out.push(')');
            }
        }
        out.push(']');
        out
    }
}

fn json_int(value: &Value, what: &str) -> Result<i64> {
    match value {
        Value::Int(v) => Ok(*v),
        Value::Bool(b) => Ok(*b as i64),
        _ => bail!("{what} must be an integer"),
    }
}

/// The values of accessor i.
pub fn accessor(gltf: &Gltf, i: &Value) -> Result<Values> {
    let a = index(item(&gltf.json, "accessors")?, i)?;
    let kind = item(a, "type")?.as_str().unwrap_or_default();
    let n = match kind {
        "SCALAR" => 1,
        "VEC2" => 2,
        "VEC3" => 3,
        "VEC4" => 4,
        "MAT4" => 16,
        _ => bail!("'{kind}'"),
    };
    let component = json_int(item(a, "componentType")?, "componentType")?;
    let size = match component {
        5120 | 5121 => 1,
        5122 | 5123 => 2,
        5125 | 5126 => 4,
        _ => bail!("{component}"),
    };
    let count = json_int(item(a, "count")?, "count")?.max(0) as usize;
    let mut values = Vec::with_capacity(count * n);
    match a.get("bufferView") {
        None => values.resize(count * n, Num::Int(0)),
        Some(view) => {
            let view = index(item(&gltf.json, "bufferViews")?, view)?;
            let buffer_number = view.get("buffer").map_or(Ok(0), |b| json_int(b, "buffer"))?;
            let Some(buffer) = usize::try_from(buffer_number).ok().and_then(|b| gltf.buffers.get(b)) else {
                bail!("list index out of range");
            };
            let base = view.get("byteOffset").map_or(Ok(0), |o| json_int(o, "byteOffset"))?
                + a.get("byteOffset").map_or(Ok(0), |o| json_int(o, "byteOffset"))?;
            let stride = match view.get("byteStride") {
                Some(s) if s.truthy() => json_int(s, "byteStride")?,
                _ => (n * size) as i64,
            };
            for k in 0..count as i64 {
                let at = base + k * stride;
                if at < 0 || at as usize + n * size > buffer.len() {
                    bail!("unpack_from requires a buffer of at least {} bytes", at + (n * size) as i64);
                }
                let at = at as usize;
                for j in 0..n {
                    let b = &buffer[at + j * size..at + (j + 1) * size];
                    values.push(match component {
                        5120 => Num::Int(b[0] as i8 as i64),
                        5121 => Num::Int(b[0] as i64),
                        5122 => Num::Int(i16::from_le_bytes([b[0], b[1]]) as i64),
                        5123 => Num::Int(u16::from_le_bytes([b[0], b[1]]) as i64),
                        5125 => Num::Int(u32::from_le_bytes(b.try_into().unwrap()) as i64),
                        _ => Num::Float(py::unpack_f32(u32::from_le_bytes(b.try_into().unwrap()))),
                    });
                }
            }
        }
    }
    if a.get("normalized").is_some_and(Value::truthy) {
        let divisor = match component {
            5120 => 127.0,
            5121 => 255.0,
            5122 => 32767.0,
            5123 => 65535.0,
            _ => bail!("{component}"),
        };
        values = values.into_iter().map(|v| Num::Float(py::max(-1.0, v.f() / divisor))).collect();
    }
    if a.has("sparse") {
        bail!("sparse accessors are not supported");
    }
    Ok(Values { n, values })
}

/// The node's name without Blender's ".001" duplicate suffix.
pub fn clean(name: Option<&Value>) -> Result<String> {
    let name = match name {
        Some(value) if value.truthy() => match value.as_str() {
            Some(text) => text.to_string(),
            None => bail!("object of type '{}' has no len()", "int"),
        },
        _ => String::new(),
    };
    let chars: Vec<char> = name.chars().collect();
    let n = chars.len();
    if n > 4 && chars[n - 4] == '.' && chars[n - 3..].iter().all(char::is_ascii_digit) {
        return Ok(chars[..n - 4].iter().collect());
    }
    Ok(name)
}

fn num(value: &Value) -> Result<Num> {
    match value {
        Value::Int(v) => Ok(Num::Int(*v)),
        Value::Bool(b) => Ok(Num::Int(*b as i64)),
        Value::Float(v) => Ok(Num::Float(*v)),
        _ => bail!("unsupported operand type(s)"),
    }
}

fn nums(value: Option<&Value>, default: &[i64]) -> Result<Vec<Num>> {
    match value {
        None => Ok(default.iter().map(|&v| Num::Int(v)).collect()),
        Some(Value::List(items)) => items.iter().map(num).collect(),
        Some(_) => bail!("the value is not a list"),
    }
}

type TypedMatrix = Vec<Num>;

/// A node's local matrix from its matrix or its translation, rotation and scale.
fn node_matrix(node: &Value) -> Result<TypedMatrix> {
    if let Some(matrix) = node.get("matrix") {
        let m = nums(Some(matrix), &[])?;
        if m.len() < 16 {
            bail!("tuple index out of range");
        }
        return Ok(m);
    }
    let t = nums(node.get("translation"), &[0, 0, 0])?;
    let r = nums(node.get("rotation"), &[0, 0, 0, 1])?;
    let s = nums(node.get("scale"), &[1, 1, 1])?;
    if r.len() != 4 {
        bail!("not enough values to unpack (expected 4, got {})", r.len());
    }
    if t.len() < 3 || s.len() < 3 {
        bail!("tuple index out of range");
    }
    let (x, y, z, w) = (r[0], r[1], r[2], r[3]);
    let (one, two, zero) = (Num::Int(1), Num::Int(2), Num::Float(0.0));
    let rot = [
        one - two * (y * y + z * z),
        two * (x * y + w * z),
        two * (x * z - w * y),
        two * (x * y - w * z),
        one - two * (x * x + z * z),
        two * (y * z + w * x),
        two * (x * z + w * y),
        two * (y * z - w * x),
        one - two * (x * x + y * y),
    ];
    Ok(vec![
        rot[0] * s[0],
        rot[1] * s[0],
        rot[2] * s[0],
        zero,
        rot[3] * s[1],
        rot[4] * s[1],
        rot[5] * s[1],
        zero,
        rot[6] * s[2],
        rot[7] * s[2],
        rot[8] * s[2],
        zero,
        t[0],
        t[1],
        t[2],
        Num::Float(1.0),
    ])
}

fn typed_mul(a: &TypedMatrix, b: &TypedMatrix) -> TypedMatrix {
    let mut out = Vec::with_capacity(16);
    for c in 0..4 {
        for r in 0..4 {
            out.push(py::sum_nums((0..4).map(|k| a[k * 4 + r] * b[c * 4 + k])));
        }
    }
    out
}

fn floats(m: &TypedMatrix) -> Matrix {
    let mut out = [0.0; 16];
    for (slot, v) in out.iter_mut().zip(m) {
        *slot = v.f();
    }
    out
}

fn close(a: &[Num], b: &Matrix) -> bool {
    a.iter().zip(b).all(|(x, y)| (x.f() - y).abs() <= 1e-5 * py::max(1.0, y.abs()))
}

/// A float as a dictionary key compares: -0.0 equals 0.0, a NaN equals nothing.
#[derive(Clone, Copy, Debug)]
struct KeyFloat(f64);

impl PartialEq for KeyFloat {
    fn eq(&self, other: &KeyFloat) -> bool {
        self.0 == other.0
    }
}

impl Eq for KeyFloat {}

impl std::hash::Hash for KeyFloat {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let canonical = if self.0 == 0.0 { 0.0f64 } else { self.0 };
        canonical.to_bits().hash(state);
    }
}

type Skin = Option<Vec<(i64, KeyFloat)>>;

/// One triangle corner: position, normal, uv, colour, and the key telling MDT vertices apart
/// (position and skin).
struct Corner {
    position: [f64; 3],
    normal: Option<Vec<f64>>,
    uv: Option<[f64; 2]>,
    colour: Option<Vec<f64>>,
    skin: Skin,
}

struct Primitive {
    material: usize,
    corners: Vec<Corner>,
}

fn key_of(values: &[f64]) -> Vec<KeyFloat> {
    values.iter().map(|&v| KeyFloat(v)).collect()
}

/// An MDT from triangles (corners in threes); returns it and each vertex's skin.
/// Vertices (position, alpha) by the cell of one unit they are in.
type Cells = HashMap<(i64, i64, i64), Vec<([f64; 3], f64)>>;

/// A mesh's vertex alphas by position, as glTF colours give them.
struct Alphas {
    cells: Cells,
}

fn original_alphas(mdt: Option<&Mdt>) -> Alphas {
    let mut cells = Cells::new();
    for prim in mdt.map(|t| t.prims.as_slice()).unwrap_or_default() {
        let (Some(v), Some(c)) = (prim.slot(mds::Slot::Vertex), prim.slot(mds::Slot::Colour)) else {
            continue;
        };
        for corner in &prim.verts {
            let mdt = mdt.unwrap();
            let (Some(p), Some(colour)) = (mdt.vertices.get(corner[v] as usize), mdt.colours.get(corner[c] as usize)) else {
                continue;
            };
            let position = [p[0], p[1], p[2]];
            cells.entry(Alphas::cell(&position)).or_default().push((position, colour[3].clamp(0.0, 1.0)));
        }
    }
    Alphas { cells }
}

impl Alphas {
    fn cell(p: &[f64]) -> (i64, i64, i64) {
        (p[0].floor() as i64, p[1].floor() as i64, p[2].floor() as i64)
    }

    /// The alpha of the nearest vertex: in the cells around first, then anywhere; 1 for none.
    fn near(&self, p: &[f64]) -> f64 {
        let distance = |q: &[f64; 3]| (0..3).map(|i| (q[i] - p[i]).powi(2)).sum::<f64>();
        let nearest = |found: &mut dyn Iterator<Item = &([f64; 3], f64)>| {
            found.min_by(|a, b| distance(&a.0).total_cmp(&distance(&b.0))).map(|(_, alpha)| *alpha)
        };
        let (x, y, z) = Alphas::cell(p);
        let around = (-1..=1).flat_map(|dx| (-1..=1).flat_map(move |dy| (-1..=1).map(move |dz| (x + dx, y + dy, z + dz))));
        nearest(&mut around.filter_map(|key| self.cells.get(&key)).flatten())
            .or_else(|| nearest(&mut self.cells.values().flatten()))
            .unwrap_or(1.0)
    }
}

fn build_mdt(prims: &[Primitive], materials: Vec<Material>) -> (Mdt, Vec<Skin>) {
    let mut t = Mdt::default();
    let mut vertex_of: HashMap<(Vec<KeyFloat>, Skin), usize> = HashMap::new();
    let mut normal_of: HashMap<Vec<KeyFloat>, usize> = HashMap::new();
    let mut uv_of: HashMap<Vec<KeyFloat>, usize> = HashMap::new();
    let mut colour_of: HashMap<Vec<KeyFloat>, usize> = HashMap::new();
    let mut skins = Vec::new();
    fn number(map: &mut HashMap<Vec<KeyFloat>, usize>, list: &mut Vec<[f64; 4]>, value: [f64; 4]) -> i32 {
        let next = list.len();
        let at = *map.entry(key_of(&value)).or_insert(next);
        if at == next {
            list.push(value);
        }
        at as i32
    }
    for p in prims {
        let first = &p.corners[0];
        let (hn, hu, hc) = (first.normal.is_some(), first.uv.is_some(), first.colour.is_some());
        let kind = 3
            | if hn { 0 } else { mds::F_NO_NORMAL }
            | if hu { 0 } else { mds::F_NO_UV }
            | if hc { mds::F_COLOUR } else { 0 };
        let stride = Prim::stride_of(kind);
        let limit = (32767 / stride) / 3 * 3;
        for chunk in p.corners.chunks(limit) {
            let mut verts = Vec::with_capacity(chunk.len());
            for corner in chunk {
                let key = (key_of(&corner.position), corner.skin.clone());
                let next = t.vertices.len();
                let vertex = *vertex_of.entry(key).or_insert(next);
                if vertex == next {
                    let [x, y, z] = corner.position;
                    t.vertices.push([x, y, z, 1.0]);
                    skins.push(corner.skin.clone());
                }
                let mut tuple = vec![vertex as i32];
                if hn {
                    let n = corner.normal.as_ref().unwrap();
                    tuple.push(number(&mut normal_of, &mut t.normals, [n[0], n[1], n[2], 1.0]));
                }
                if hu {
                    let [u, v] = corner.uv.unwrap();
                    tuple.push(number(&mut uv_of, &mut t.uvs, [u, v, 1.0, 0.0]));
                }
                if hc {
                    let c = corner.colour.as_ref().unwrap();
                    tuple.push(number(&mut colour_of, &mut t.colours, [c[0], c[1], c[2], c[3]]));
                }
                verts.push(tuple);
            }
            t.prims.push(Prim { kind, material: p.material as u32, verts });
        }
    }
    if t.normals.is_empty() {
        t.normals.push([0.0, 0.0, 1.0, 1.0]); // the corpus always has at least one normal
    }
    t.materials = materials;
    (t, skins)
}

fn floats_of(value: &Value) -> Result<Vec<f64>> {
    match value {
        Value::List(items) => {
            items.iter().map(|v| v.as_f64().ok_or_else(|| Error::invalid("required argument is not a float"))).collect()
        }
        _ => bail!("the value is not a list"),
    }
}

/// The MDT material for a glTF material (its extras when to_gltf wrote them).
fn material_from_gltf(gltf: &Value, number: &Value) -> Result<Material> {
    let mut m = Material::new("");
    if matches!(number, Value::Null) {
        return Ok(m);
    }
    let gm = index(item(gltf, "materials")?, number)?;
    let empty = Value::object();
    let extras = gm.get("extras").unwrap_or(&empty);
    let pbr = gm.get("pbrMetallicRoughness").unwrap_or(&empty);
    m.diffuse = match extras.get("mdt_diffuse").or_else(|| pbr.get("baseColorFactor")) {
        Some(value) => floats_of(value)?,
        None => vec![1.0; 4],
    };
    if let Some(value) = extras.get("mdt_ambient") {
        m.ambient = floats_of(value)?;
    }
    if let Some(value) = extras.get("mdt_specular") {
        m.specular = floats_of(value)?;
    }
    if let Some(value) = extras.get("mdt_power") {
        m.power = value.as_f64().ok_or_else(|| Error::invalid("required argument is not a float"))?;
    }
    let mut texture = extras.get("texture").filter(|v| !matches!(v, Value::Null)).cloned();
    if texture.is_none() {
        if let Some(base) = pbr.get("baseColorTexture") {
            let tx = index(item(gltf, "textures")?, item(base, "index")?)?;
            let image = match tx.get("source") {
                Some(source) => index(item(gltf, "images")?, source)?.clone(),
                None => Value::object(),
            };
            let name = image.get("name").filter(|v| v.truthy()).cloned();
            texture = Some(match name {
                Some(name) => name,
                None => {
                    let uri = image.get("uri").and_then(Value::as_str).unwrap_or("");
                    let base_name = uri.rsplit(['/', '\\']).next().unwrap_or("");
                    Value::Str(crate::to_gltf::strip_extension(base_name).to_string())
                }
            });
        }
    }
    let texture = texture.unwrap_or_else(|| gm.get("name").cloned().unwrap_or(Value::Str(String::new())));
    let Some(texture) = texture.as_str() else {
        bail!("the material's texture name is not text");
    };
    let encoded_length: usize =
        texture.chars().map(|c| c2_disc::text::encode(&c.to_string()).map_or(1, |b| b.len())).sum();
    if encoded_length > 31 {
        bail!("texture name too long: {}", crate::im3::str_repr(texture));
    }
    m.texture = texture.to_string();
    Ok(m)
}

/// The MDT material for a collision mesh's glTF material: the attributes its name gives
/// (`col_ground0_foot2_area6_mask12`) encoded in the diffuse colour, or the material as exported
/// (its extras) when they still encode the same attributes, so an untouched material keeps its
/// bytes. A material not named so keeps the attributes its extras give.
fn collision_material(gltf: &Value, number: &Value) -> Result<Material> {
    if matches!(number, Value::Null) {
        let mut m = Material::new("");
        m.diffuse = CollisionAttributes::default().encode();
        return Ok(m);
    }
    let gm = index(item(gltf, "materials")?, number)?;
    let name = gm.get("name").and_then(Value::as_str).unwrap_or("");
    let extras = gm.get("extras");
    let exported = match extras.and_then(|e| e.get("mdt_diffuse")) {
        Some(diffuse) => Some(floats_of(diffuse)?),
        None => None,
    };
    let Some(attributes) =
        CollisionAttributes::parse(name).or_else(|| exported.as_deref().map(CollisionAttributes::decode))
    else {
        bail!("collision material {name:?} is not named col_ground<n>_foot<n>_area<n>_mask<n>");
    };
    if exported.as_deref().is_some_and(|d| d.len() == 4 && CollisionAttributes::decode(d) == attributes) {
        return material_from_gltf(gltf, number);
    }
    let texture = extras.and_then(|e| e.get("texture")).and_then(Value::as_str).unwrap_or("");
    let mut m = Material::new(texture);
    m.diffuse = attributes.encode();
    Ok(m)
}

/// What a model's collision breaks of the engine's rules (LoadCollisionFile reads positions and
/// triangle lists only, each corner a vertex index: a prim must be of type 0x213; a strip or a
/// colour-flagged prim leaves its frame without collision, normal or uv indices are misread).
pub fn collision_problems(model: &Mds) -> Vec<String> {
    let mut out = Vec::new();
    for node in &model.nodes {
        let Some(t) = &node.mdt else {
            continue;
        };
        for (k, p) in t.prims.iter().enumerate() {
            if p.kind != COLLISION_PRIM {
                out.push(format!("{}: prim {k} is of type 0x{:x}, not 0x{COLLISION_PRIM:x}", node.name, p.kind));
            }
            if p.verts.len() % 3 != 0 {
                out.push(format!("{}: prim {k} has {} corners, not whole triangles", node.name, p.verts.len()));
            }
            if p.verts.iter().flatten().any(|&v| v < 0 || v as usize >= t.vertices.len()) {
                out.push(format!("{}: prim {k} names a vertex the mesh does not have", node.name));
            }
            if p.material as usize >= t.materials.len() {
                out.push(format!("{}: prim {k} names a material the mesh does not have", node.name));
            }
        }
    }
    out
}

/// The prim type a collision mesh is made of: a triangle list of positions only (stride 1).
pub const COLLISION_PRIM: u32 = mds::PRIM_TRIANGLES | mds::F_NO_UV | mds::F_NO_NORMAL;

/// What import_gltf writes: the MDS, the .wgt and the .bbp.
pub struct Imported {
    pub mds: Vec<u8>,
    pub wgt: Vec<u8>,
    pub bbp: Vec<u8>,
}

/// Which mesh of a template a glTF mesh node stands for: the frame it replaces the mesh of, or a
/// new frame of this name.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MeshId {
    Frame(usize),
    New(String),
}

/// How import reads the glTF.
#[derive(Clone, Copy, Debug, Default)]
pub struct ImportOptions {
    /// The model is collision: each mesh is written as type 0x213 triangle lists of positions alone
    /// (normals, uvs and colours dropped), each material's diffuse colour encoding the attributes
    /// its name gives (`col_ground0_foot2_area6_mask12`, see `collision_material`).
    pub collision: bool,
}

/// The template frame each glTF node stands for: the node's `extras.mds_index` when it has one
/// within the template, otherwise its name (Blender's ".001" and a skinned mesh node's "_skin"
/// dropped) as a frame's whole name, else as a frame's name without its `--` flags; the k-th node
/// of a name given more than once takes the k-th frame of that name. None for a node no frame has
/// the name of.
pub fn frames_of(json: &Value, model: &Mds) -> Result<Vec<Option<usize>>> {
    let empty = Vec::new();
    let nodes = match json.get("nodes") {
        Some(nodes) => nodes.as_list().ok_or_else(|| Error::invalid("'nodes' is not a list"))?,
        None => &empty,
    };
    let mut whole: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut base: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, node) in model.nodes.iter().enumerate() {
        whole.entry(node.name.as_str()).or_default().push(i);
        base.entry(node.base_name()).or_default().push(i);
    }
    let mut taken: HashMap<(bool, bool, String), usize> = HashMap::new();
    let mut out = Vec::with_capacity(nodes.len());
    for n in nodes {
        let index = n.get("extras").and_then(|e| e.get("mds_index")).and_then(Value::as_int);
        if let Some(i) = index.and_then(|i| usize::try_from(i).ok()).filter(|&i| i < model.nodes.len()) {
            out.push(Some(i));
            continue;
        }
        let name = frame_name(json, n)?;
        let stem = name.split_once("--").map_or(name.as_str(), |(stem, _)| stem);
        let (in_whole, frames) = match (whole.get(name.as_str()), base.get(name.as_str()), base.get(stem)) {
            (Some(frames), _, _) => (true, frames),
            (None, Some(frames), _) | (None, None, Some(frames)) => (false, frames),
            _ => {
                out.push(None);
                continue;
            }
        };
        let k = taken.entry((n.has("skin"), in_whole, name.clone())).or_insert(0);
        out.push(Some(frames[(*k).min(frames.len() - 1)]));
        *k += 1;
    }
    Ok(out)
}

/// The mesh nodes of a glTF with the template mesh each stands for.
pub fn mesh_ids(json: &Value, model: &Mds) -> Result<Vec<(usize, MeshId)>> {
    let frames = frames_of(json, model)?;
    let empty = Vec::new();
    let nodes = json.get("nodes").and_then(Value::as_list).unwrap_or(&empty);
    let mut out = Vec::new();
    for (gi, n) in nodes.iter().enumerate() {
        if n.has("mesh") {
            let id = match frames[gi] {
                Some(frame) => MeshId::Frame(frame),
                None => MeshId::New(frame_name(json, n)?),
            };
            out.push((gi, id));
        }
    }
    Ok(out)
}

/// {mesh: what the mesh holds}: its node's transform and skin, and its primitives' vertex data and
/// materials by value, so a tool that writes the same mesh another way still matches. Meshes are
/// told apart by the template mesh they stand for.
pub fn mesh_contents(gltf: &Gltf, model: &Mds) -> Result<BTreeMap<MeshId, String>> {
    let mut out = BTreeMap::new();
    let nodes = item(&gltf.json, "nodes")?.as_list().ok_or_else(|| Error::invalid("'nodes' is not a list"))?;
    for (gi, id) in mesh_ids(&gltf.json, model)? {
        let node = &nodes[gi];
        let mesh = index(item(&gltf.json, "meshes")?, item(node, "mesh")?)?;
        let transform = Value::Object(
            ["translation", "rotation", "scale", "matrix", "skin"]
                .iter()
                .map(|key| (key.to_string(), node.get(key).cloned().unwrap_or(Value::Null)))
                .collect(),
        );
        let mut parts = vec![transform.sorted_text()];
        let Some(primitives) = item(mesh, "primitives")?.as_list() else {
            bail!("'primitives' is not a list");
        };
        for primitive in primitives {
            let Value::Object(attributes) = item(primitive, "attributes")? else {
                bail!("'attributes' is not an object");
            };
            let mut sorted: Vec<&(String, Value)> = attributes.iter().collect();
            sorted.sort_by(|a, b| a.0.cmp(&b.0));
            let mut indices: Vec<&Value> = sorted.iter().map(|(_, index)| index).collect();
            if let Some(index) = primitive.get("indices") {
                indices.push(index);
            }
            for index in indices {
                if !matches!(index, Value::Null) {
                    parts.push(accessor(gltf, index)?.repr());
                }
            }
            let material = match primitive.get("material") {
                Some(number) => index(item(&gltf.json, "materials")?, number)?.clone(),
                None => Value::Null,
            };
            parts.push(material.sorted_text());
        }
        out.insert(id, parts.join("\n"));
    }
    Ok(out)
}

/// A mesh as values: its node's matrix and, per material (by name, without a `.NNN` suffix, and the picture it shows), its
/// triangles' corners (position, then first UVs and colour when it has them).
pub struct Geometry {
    pub matrix: Vec<f64>,
    pub groups: BTreeMap<String, Vec<[Vec<f64>; 3]>>,
}

/// {mesh: its geometry}, for comparing a glTF that went through another tool (Blender), which
/// splits and reorders vertices, recomputes normals, adds float noise and moves colours by a step.
pub fn mesh_geometry(gltf: &Gltf, model: &Mds) -> Result<BTreeMap<MeshId, Geometry>> {
    fn strip_suffix(name: &str) -> &str {
        match name.rsplit_once('.') {
            Some((stem, digits)) if digits.len() == 3 && digits.chars().all(|c| c.is_ascii_digit()) => stem,
            _ => name,
        }
    }
    let mut out = BTreeMap::new();
    let nodes = item(&gltf.json, "nodes")?.as_list().ok_or_else(|| Error::invalid("'nodes' is not a list"))?;
    for (gi, id) in mesh_ids(&gltf.json, model)? {
        let node = &nodes[gi];
        let list = |key: &str| -> Option<Vec<f64>> {
            node.get(key).and_then(Value::as_list).map(|l| l.iter().map(|v| v.as_f64().unwrap_or(0.0)).collect())
        };
        let matrix = match list("matrix").filter(|m| m.len() == 16) {
            Some(m) => m,
            None => {
                let t = list("translation").filter(|v| v.len() == 3).unwrap_or(vec![0.0; 3]);
                let s = list("scale").filter(|v| v.len() == 3).unwrap_or(vec![1.0; 3]);
                let q = list("rotation").filter(|v| v.len() == 4).unwrap_or(vec![0.0, 0.0, 0.0, 1.0]);
                let (x, y, z, w) = (q[0], q[1], q[2], q[3]);
                let r = [
                    [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - z * w), 2.0 * (x * z + y * w)],
                    [2.0 * (x * y + z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - x * w)],
                    [2.0 * (x * z - y * w), 2.0 * (y * z + x * w), 1.0 - 2.0 * (x * x + y * y)],
                ];
                // column-major, as glTF writes a matrix
                let mut m = vec![0.0; 16];
                for col in 0..3 {
                    for row in 0..3 {
                        m[col * 4 + row] = r[row][col] * s[col];
                    }
                }
                m[12] = t[0];
                m[13] = t[1];
                m[14] = t[2];
                m[15] = 1.0;
                m
            }
        };
        let mesh = index(item(&gltf.json, "meshes")?, item(node, "mesh")?)?;
        let Some(primitives) = item(mesh, "primitives")?.as_list() else {
            bail!("'primitives' is not a list");
        };
        let mut groups: BTreeMap<String, Vec<[Vec<f64>; 3]>> = BTreeMap::new();
        for primitive in primitives {
            let material = match primitive.get("material") {
                Some(number) => {
                    let found = index(item(&gltf.json, "materials")?, number)?;
                    // with the picture it shows, so a material given another picture differs
                    let texture = found.get("extras").and_then(|extras| extras.get("texture")).and_then(Value::as_str);
                    let name = strip_suffix(found.get("name").and_then(Value::as_str).unwrap_or(""));
                    format!("{name}\u{0}{}", texture.unwrap_or(""))
                }
                None => String::new(),
            };
            let attributes = item(primitive, "attributes")?;
            let read = |key: &str| -> Result<Option<Values>> {
                match attributes.get(key) {
                    Some(at) if !matches!(at, Value::Null) => Ok(Some(accessor(gltf, at)?)),
                    _ => Ok(None),
                }
            };
            let Some(positions) = read("POSITION")? else {
                continue;
            };
            let (uvs, colors) = (read("TEXCOORD_0")?, read("COLOR_0")?);
            let corner = |at: i64| -> Result<Vec<f64>> {
                let mut values: Vec<f64> = positions.at(at)?.iter().map(|v| v.f()).collect();
                if let Some(uvs) = &uvs {
                    values.extend(uvs.at(at)?.iter().map(|v| v.f()));
                }
                if let Some(colors) = &colors {
                    values.extend(colors.at(at)?.iter().map(|v| v.f()));
                }
                Ok(values)
            };
            let order: Vec<i64> = match primitive.get("indices") {
                Some(at) if !matches!(at, Value::Null) => {
                    let values = accessor(gltf, at)?;
                    (0..values.len()).map(|i| values.at(i as i64).map(|v| v[0].f() as i64)).collect::<Result<_>>()?
                }
                _ => (0..positions.len() as i64).collect(),
            };
            let triangles = groups.entry(material).or_default();
            for triangle in order.chunks(3).filter(|chunk| chunk.len() == 3) {
                let corners = [corner(triangle[0])?, corner(triangle[1])?, corner(triangle[2])?];
                let same = |a: &Vec<f64>, b: &Vec<f64>| a.iter().zip(b).take(3).all(|(x, y)| (x - y).abs() < 1e-9);
                if same(&corners[0], &corners[1]) || same(&corners[1], &corners[2]) || same(&corners[0], &corners[2]) {
                    continue;
                }
                triangles.push(corners);
            }
        }
        out.insert(id, Geometry { matrix, groups });
    }
    Ok(out)
}

/// Whether two meshes are the same within what another tool's round trip moves: matrices within
/// 1e-3 (of the larger of 1 and the value), and every triangle of one matched by a triangle of the
/// other in any corner order (winding kept): positions within 0.005, UVs within 0.001, colours
/// within 4/255.
fn same_geometry(a: &Geometry, b: &Geometry) -> bool {
    let close = |x: f64, y: f64, tolerance: f64| (x - y).abs() <= tolerance * x.abs().max(y.abs()).max(1.0);
    if a.matrix.len() != b.matrix.len() || !a.matrix.iter().zip(&b.matrix).all(|(x, y)| close(*x, *y, 1e-3)) {
        return false;
    }
    if a.groups.keys().ne(b.groups.keys()) {
        return false;
    }
    // Blender writes some vertex colours without alpha, and gives every primitive of a mesh with
    // colours a colour, white where it had none: a corner that lost only its alpha, or gained
    // only a white colour, compares on the rest
    let corner_close = |x: &[f64], y: &[f64]| -> bool {
        let (short, long) = if x.len() <= y.len() { (x, y) } else { (y, x) };
        let extra = &long[short.len()..];
        let fits = match extra.len() {
            0 | 1 => true,
            3 | 4 => extra.iter().all(|value| (value - 1.0).abs() <= 4.0 / 255.0),
            _ => false,
        };
        // positions, then UVs when there are (lengths 5, 8 and 9), then colour
        let uvs = matches!(long.len(), 5 | 8 | 9);
        fits && short.iter().zip(long).enumerate().all(|(at, (p, q))| {
            let tolerance = match at {
                0..=2 => 0.005,
                3 | 4 if uvs => 0.001,
                _ => 4.0 / 255.0,
            };
            close(*p, *q, tolerance)
        })
    };
    let tri_close = |x: &[Vec<f64>; 3], y: &[Vec<f64>; 3]| -> bool {
        (0..3).any(|r| (0..3).all(|k| corner_close(&x[k], &y[(k + r) % 3])))
    };
    // triangles found by their centroid in cells of one unit, looking in the cells around it too
    let cell = |tri: &[Vec<f64>; 3]| -> (i64, i64, i64) {
        let c = |axis: usize| ((tri[0][axis] + tri[1][axis] + tri[2][axis]) / 3.0).floor() as i64;
        (c(0), c(1), c(2))
    };
    for (material, xs) in &a.groups {
        let ys = &b.groups[material];
        if xs.len() != ys.len() {
            return false;
        }
        let mut cells: HashMap<(i64, i64, i64), Vec<usize>> = HashMap::new();
        for (at, y) in ys.iter().enumerate() {
            cells.entry(cell(y)).or_default().push(at);
        }
        let mut used = vec![false; ys.len()];
        for x in xs {
            let (cx, cy, cz) = cell(x);
            let mut found = false;
            'search: for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        for &at in cells.get(&(cx + dx, cy + dy, cz + dz)).into_iter().flatten() {
                            if !used[at] && tri_close(x, &ys[at]) {
                                used[at] = true;
                                found = true;
                                break 'search;
                            }
                        }
                    }
                }
            }
            if !found {
                return false;
            }
        }
    }
    true
}

/// The meshes whose geometry (mesh_geometry) differs from the export's, or that it does not have.
pub fn changed_geometry(exported: &Gltf, given: &Gltf, model: &Mds) -> Result<BTreeSet<MeshId>> {
    let before = mesh_geometry(exported, model)?;
    let after = mesh_geometry(given, model)?;
    Ok(after
        .into_iter()
        .filter(|(id, geometry)| !before.get(id).is_some_and(|old| same_geometry(old, geometry)))
        .map(|(id, _)| id)
        .collect())
}

/// The meshes of the given glTF that differ from the export's, or that the export does not have.
pub fn changed_meshes(exported: &Gltf, given: &Gltf, model: &Mds) -> Result<BTreeSet<MeshId>> {
    let before = mesh_contents(exported, model)?;
    let after = mesh_contents(given, model)?;
    Ok(after.into_iter().filter(|(id, content)| before.get(id) != Some(content)).map(|(id, _)| id).collect())
}

/// The model rebuilt from a glTF, the template's meshes replaced by the glTF's: each glTF mesh
/// node replaces the mesh of the frame it stands for (`frames_of`), only those in `meshes` when
/// given.
pub fn import_gltf(
    gltf: &Gltf,
    template: &[u8],
    bbp_data: Option<&[u8]>,
    wgt_data: Option<&[u8]>,
    meshes: Option<&BTreeSet<MeshId>>,
) -> Result<Imported> {
    import_with(gltf, template, bbp_data, wgt_data, meshes, &ImportOptions::default())
}

/// import_gltf as the options say.
pub fn import_with(
    gltf: &Gltf,
    template: &[u8],
    bbp_data: Option<&[u8]>,
    wgt_data: Option<&[u8]>,
    meshes: Option<&BTreeSet<MeshId>>,
    options: &ImportOptions,
) -> Result<Imported> {
    let j = &gltf.json;
    let mut model: Mds = mds::parse(template)?;
    let resolved = frames_of(j, &model)?;
    let mut bbp: Vec<Matrix> = match bbp_data {
        Some(data) => mot::parse_bbp(data)?,
        None => model.world_matrices()?,
    };
    if bbp.len() < model.nodes.len() {
        bbp.resize(model.nodes.len(), IDENT);
    }
    let old_wgt = match wgt_data {
        Some(data) if !data.is_empty() => mot::parse(data)?,
        _ => Vec::new(),
    };
    // frames added for glTF mesh nodes no template frame stands for, by name
    let mut by_name: HashMap<String, usize> = HashMap::new();
    let empty = Value::List(Vec::new());
    let nodes = item(j, "nodes")?;
    let Some(node_list) = nodes.as_list() else {
        bail!("'nodes' is not a list");
    };
    let mut parent: HashMap<i64, usize> = HashMap::new();
    for (i, n) in node_list.iter().enumerate() {
        for c in n.get("children").unwrap_or(&empty).as_list().unwrap_or(&Vec::new()) {
            match c {
                Value::Int(c) => {
                    parent.insert(*c, i);
                }
                Value::Float(c) if c.fract() == 0.0 => {
                    parent.insert(*c as i64, i);
                }
                _ => {}
            }
        }
    }
    let mut world: HashMap<i64, TypedMatrix> = HashMap::new();
    let mds_index_of = |by_name: &HashMap<String, usize>, gi: &Value| -> Result<Option<usize>> {
        let n = index(nodes, gi)?;
        let at = gi.as_int().unwrap_or(-1);
        let at = if at < 0 { node_list.len() as i64 + at } else { at };
        match resolved.get(at as usize).copied().flatten() {
            Some(frame) => Ok(Some(frame)),
            None => Ok(by_name.get(&clean(n.get("name"))?).copied()),
        }
    };

    // bind matrices of joints from the inverse bind matrices (keeps edits to the bind pose)
    let template_world = model.world_matrices()?;
    for skin in j.get("skins").unwrap_or(&empty).as_list().unwrap_or(&Vec::new()) {
        let Some(ibm_index) = skin.get("inverseBindMatrices") else {
            continue;
        };
        let ibm = accessor(gltf, ibm_index)?;
        let Some(joints) = item(skin, "joints")?.as_list() else {
            bail!("'joints' is not a list");
        };
        for (k, gi) in joints.iter().enumerate() {
            let mi = mds_index_of(&by_name, gi)?;
            let matrix = ibm.at(k as i64)?;
            if matrix.len() < 16 {
                bail!("tuple index out of range");
            }
            let values: Vec<f64> = matrix.iter().map(|v| v.f()).collect();
            let inverse = mat_inv(&values[..16].try_into().unwrap());
            let (Some(mi), Some(inverse)) = (mi, inverse) else {
                continue;
            };
            // the template's bind matrix stays unless the glTF's differs from what to_gltf writes
            // for it: its inverse, or the frame's rest matrix when it has none
            let exported = mat_inv(&bbp[mi]).or_else(|| template_world.get(mi).and_then(mat_inv));
            if !exported.is_some_and(|e| close(matrix, &e)) {
                bbp[mi] = inverse;
            }
        }
    }

    let mut replaced = BTreeSet::new();
    let mut new_wgt = Vec::new();
    let mesh_list = j.get("meshes");
    for (gi, n) in node_list.iter().enumerate() {
        let Some(mesh_index) = n.get("mesh") else {
            continue;
        };
        let name_value = match n.get("name").filter(|v| v.truthy()) {
            Some(name) => Some(name.clone()),
            None => index(mesh_list.ok_or_else(|| missing("meshes"))?, mesh_index)?.get("name").cloned(),
        };
        let mut nm = clean(name_value.as_ref())?;
        if let Some(stem) = nm.strip_suffix("_skin") {
            nm = stem.to_string();
        }
        let id = match resolved[gi] {
            Some(frame) => MeshId::Frame(frame),
            None => MeshId::New(nm.clone()),
        };
        if meshes.is_some_and(|wanted| !wanted.contains(&id)) {
            continue;
        }
        let target = match resolved[gi].or_else(|| by_name.get(&nm).copied()) {
            Some(target) => target,
            None => {
                // a new node, under the root
                let mut node = Node::new(&nm, if model.nodes.is_empty() { -1 } else { 0 });
                if let Some(&pj) = parent.get(&(gi as i64)) {
                    if let Some(p) = mds_index_of(&by_name, &Value::Int(pj as i64))? {
                        if !n.has("skin") {
                            node.parent = p as i32;
                        }
                    }
                }
                model.nodes.push(node);
                let target = model.nodes.len() - 1;
                by_name.insert(nm.clone(), target);
                bbp.push(IDENT);
                target
            }
        };
        let skin = match n.get("skin") {
            Some(_) if options.collision => bail!("collision meshes are rigid; mesh {nm} has a skin"),
            Some(s) => Some(index(item(j, "skins")?, s)?).filter(|s| s.truthy()),
            None => None,
        };
        let to_local: Matrix;
        let mut joint_map = Vec::new();
        if let Some(skin) = skin {
            // skinned: glTF positions are model space (the mesh node's transform ignored)
            if mat_inv(&bbp[target]).is_none() {
                bbp[target] = model.world_matrices()?[target];
            }
            to_local = mat_inv(&bbp[target]).unwrap_or(IDENT);
            let Some(joints) = item(skin, "joints")?.as_list() else {
                bail!("'joints' is not a list");
            };
            for g in joints {
                joint_map.push(mds_index_of(&by_name, g)?);
            }
        } else {
            // rigid: vertices into the frame's own space through the glTF hierarchy
            let mut frame_world = None;
            for (g2, n2) in node_list.iter().enumerate() {
                let same = match resolved[gi] {
                    Some(_) => resolved[g2] == Some(target),
                    None => resolved[g2].is_none() && clean(n2.get("name"))? == nm,
                };
                if !n2.has("mesh") && same {
                    frame_world = Some(world_of(g2 as i64, nodes, &parent, &mut world, 0)?);
                }
            }
            let mesh_world = floats(&world_of(gi as i64, nodes, &parent, &mut world, 0)?);
            to_local = match frame_world {
                Some(fw) => mat_mul(&mat_inv(&floats(&fw)).unwrap_or(IDENT), &mesh_world),
                // a mesh no frame stands for (one made in Blender): into the space of the frame
                // it was added under, keeping where the object was put
                None => {
                    let under = model.nodes[target].parent;
                    let frame = if under >= 0 { bbp[under as usize] } else { IDENT };
                    mat_mul(&mat_inv(&frame).unwrap_or(IDENT), &mesh_world)
                }
            };
        }
        // Blender writes some meshes' colours without alpha: the fades come from the mesh it
        // replaces, each corner taking the alpha of the nearest vertex there
        let alphas = original_alphas(model.nodes[target].mdt.as_ref());
        let mut materials: Vec<Material> = Vec::new();
        let mut material_of: Vec<(Value, usize)> = Vec::new();
        let mut prims = Vec::new();
        let mesh = index(mesh_list.ok_or_else(|| missing("meshes"))?, mesh_index)?;
        let Some(primitives) = item(mesh, "primitives")?.as_list() else {
            bail!("'primitives' is not a list");
        };
        for p in primitives {
            if p.get("mode").map_or(4.0, |m| m.as_f64().unwrap_or(f64::NAN)) != 4.0 {
                bail!("only triangle primitives are supported");
            }
            let attributes = item(p, "attributes")?;
            let pos = accessor(gltf, item(attributes, "POSITION")?)?;
            let optional = |name: &str| attributes.get(name).map(|i| accessor(gltf, i)).transpose();
            // collision keeps positions alone
            let shaded = |name: &str| if options.collision { Ok(None) } else { optional(name) };
            let nrm = shaded("NORMAL")?;
            let uv = shaded("TEXCOORD_0")?;
            let col = shaded("COLOR_0")?;
            let (jts, wts) =
                if skin.is_some() { (optional("JOINTS_0")?, optional("WEIGHTS_0")?) } else { (None, None) };
            let indices: Vec<Num> = match p.get("indices") {
                Some(i) => {
                    let values = accessor(gltf, i)?;
                    if values.n != 1 {
                        bail!("list indices must be integers or slices, not tuple");
                    }
                    values.values
                }
                None => (0..pos.len() as i64).map(Num::Int).collect(),
            };
            let mk = p.get("material").cloned().unwrap_or(Value::Null);
            let material = match material_of.iter().find(|(k, _)| crate::info_cfg::same_value(k, &mk)) {
                Some((_, at)) => *at,
                None => {
                    materials.push(if options.collision {
                        collision_material(j, &mk)?
                    } else {
                        material_from_gltf(j, &mk)?
                    });
                    material_of.push((mk.clone(), materials.len() - 1));
                    materials.len() - 1
                }
            };
            let mut corners = Vec::with_capacity(indices.len());
            for c in &indices {
                let Num::Int(c) = *c else {
                    bail!("list indices must be integers or slices, not float");
                };
                let point = pos.at(c)?;
                if point.len() < 3 {
                    bail!("tuple index out of range");
                }
                let point: Vec<f64> = point.iter().map(|v| v.f()).collect();
                let position = xform_point(&to_local, &point);
                let normal = match nrm.as_ref().filter(|v| !v.is_empty()) {
                    Some(nrm) => {
                        let v: Vec<f64> = nrm.at(c)?.iter().map(|v| v.f()).collect();
                        if v.len() < 3 {
                            bail!("tuple index out of range");
                        }
                        Some(normalize(&xform_dir(&to_local, &v), &[0.0, 0.0, 1.0]))
                    }
                    None => None,
                };
                let uv = match uv.as_ref().filter(|v| !v.is_empty()) {
                    Some(uv) => {
                        let v = uv.at(c)?;
                        if v.len() < 2 {
                            bail!("tuple index out of range");
                        }
                        Some([v[0].f(), v[1].f()])
                    }
                    None => None,
                };
                let colour = match col.as_ref().filter(|v| !v.is_empty()) {
                    Some(col) => {
                        let mut v: Vec<f64> = col.at(c)?.iter().map(|v| v.f()).collect();
                        v.push(if col.n == 3 { alphas.near(&position) } else { 1.0 });
                        v.truncate(4);
                        if v.len() < 4 {
                            bail!("tuple index out of range");
                        }
                        Some(v)
                    }
                    None => None,
                };
                let mut skin_key = None;
                if let Some(jts) = jts.as_ref().filter(|v| !v.is_empty()) {
                    let Some(wts) = wts.as_ref() else {
                        bail!("'NoneType' object is not subscriptable");
                    };
                    let mut pairs: Vec<(Num, i64)> = Vec::new();
                    for (jj, w) in jts.at(c)?.iter().zip(wts.at(c)?) {
                        let Num::Int(jj) = *jj else {
                            bail!("list indices must be integers or slices, not float");
                        };
                        if w.f() > 0.0 {
                            if let Some(bone) = *crate::to_gltf::py_index(&joint_map, jj)? {
                                pairs.push((*w, bone as i64));
                            }
                        }
                    }
                    pairs.sort_by(|a, b| b.0.f().partial_cmp(&a.0.f()).unwrap().then(b.1.cmp(&a.1)));
                    pairs.truncate(4);
                    let total = py::sum_nums(pairs.iter().map(|(w, _)| *w));
                    let total = if total.f() == 0.0 { 1.0 } else { total.f() };
                    skin_key = Some(
                        pairs.iter().map(|(w, b)| (*b, KeyFloat(py::round_to(w.f() / total * 100.0, 4)))).collect(),
                    );
                }
                corners.push(Corner { position, normal, uv, colour, skin: skin_key });
            }
            if !corners.is_empty() {
                prims.push(Primitive { material, corners });
            }
        }
        let (t, skins) = build_mdt(&prims, materials);
        model.nodes[target].mdt = Some(t);
        replaced.insert(target as i32);
        if skin.is_some() {
            let mut per_bone: BTreeMap<i64, Vec<(i64, [f64; 4])>> = BTreeMap::new();
            for (vi, skin) in skins.iter().enumerate() {
                for (b, w) in skin.iter().flatten() {
                    per_bone.entry(*b).or_default().push((vi as i64, [w.0, 0.0, 0.0, 0.0]));
                }
            }
            for (b, keys) in per_bone {
                let Ok(b) = i32::try_from(b) else {
                    bail!("'i' format requires -2147483648 <= number <= 2147483647");
                };
                new_wgt.push(KeyList::new(target as i32, b, 20, keys));
            }
        }
    }
    let mut wgt: Vec<KeyList> = old_wgt.into_iter().filter(|list| !replaced.contains(&list.frame)).collect();
    wgt.extend(new_wgt);
    Ok(Imported { mds: mds::build(&model)?, wgt: mot::build(&wgt)?, bbp: mot::build_bbp(&bbp)? })
}

/// The name import_gltf gives the frame of glTF node `n`: its name, or for a mesh node without one
/// its mesh's, Blender's ".001" and a skinned mesh's "_skin" dropped.
fn frame_name(j: &Value, n: &Value) -> Result<String> {
    let mut name = match (n.get("name").filter(|v| v.truthy()), n.get("mesh")) {
        (Some(name), _) => clean(Some(name))?,
        (None, Some(mesh)) => clean(index(item(j, "meshes")?, mesh)?.get("name"))?,
        (None, None) => String::new(),
    };
    if n.has("mesh") {
        if let Some(stem) = name.strip_suffix("_skin") {
            name = stem.to_string();
        }
    }
    Ok(name)
}

/// A new MDS from a glTF alone: a frame for every node, named and placed as the glTF has it (local
/// translation, rotation and scale, under its parent; several roots go under one frame named
/// `root`), and each mesh node's triangles as its frame's mesh, in the frame's own space. Mesh
/// nodes keep their own names; names given twice, or none, are made distinct. version is the MDS
/// version written. Materials take their texture names as import_gltf reads them, and one with no
/// texture gets none. Skins are refused: a new model is rigid.
pub fn new_model(gltf: &mut Gltf, version: u32, root: &str) -> Result<Vec<u8>> {
    let empty = Vec::new();
    let node_count = gltf.json.get("nodes").and_then(Value::as_list).map_or(0, Vec::len);
    if gltf.json.get("skins").and_then(Value::as_list).is_some_and(|skins| !skins.is_empty()) {
        bail!("the glTF has a skin; a new model is built from rigid meshes (parent them to nodes instead)");
    }
    // every node named, distinctly, as import_gltf will read the names
    let mut names: Vec<String> = Vec::with_capacity(node_count);
    for i in 0..node_count {
        let n = index(item(&gltf.json, "nodes")?, &Value::Int(i as i64))?;
        let mut name = frame_name(&gltf.json, n)?;
        if name.is_empty() {
            name = format!("node{i}");
        }
        let base = name.clone();
        let mut k = 1;
        while names.iter().any(|known| known.eq_ignore_ascii_case(&name)) || name.eq_ignore_ascii_case(root) {
            name = format!("{base}_{k}");
            k += 1;
        }
        names.push(name);
    }
    if let Some(Value::List(nodes)) = gltf.json.get_mut("nodes") {
        for (node, name) in nodes.iter_mut().zip(&names) {
            node.set("name", Value::Str(name.clone()));
            // the frames are the glTF's own, found by these names: an index into another model
            // (left by an export) names none of them
            if let Some(Value::Object(extras)) = node.get_mut("extras") {
                extras.retain(|(key, _)| key != "mds_index");
            }
        }
    }
    // a material without a texture names none (import_gltf would take the material's name)
    if let Some(Value::List(materials)) = gltf.json.get_mut("materials") {
        for material in materials {
            let textured = material.get("extras").is_some_and(|extras| extras.has("texture"))
                || material.get("pbrMetallicRoughness").is_some_and(|pbr| pbr.has("baseColorTexture"));
            if !textured {
                match material.get_mut("extras") {
                    Some(extras @ Value::Object(_)) => extras.set("texture", Value::Str(String::new())),
                    _ => material.set("extras", crate::object! {"texture" => ""}),
                }
            }
        }
    }
    let j = &gltf.json;
    let node_list = item(j, "nodes")?.as_list().unwrap_or(&empty);
    let mut parent: HashMap<usize, usize> = HashMap::new();
    for (i, n) in node_list.iter().enumerate() {
        for c in n.get("children").and_then(Value::as_list).unwrap_or(&empty) {
            match c.as_int() {
                Some(c) if (0..node_count as i64).contains(&c) => {
                    parent.insert(c as usize, i);
                }
                _ => bail!("node {i} has a child that is not a node"),
            }
        }
    }
    let roots: Vec<usize> = (0..node_count).filter(|i| !parent.contains_key(i)).collect();
    let mut frames: Vec<Node> = Vec::with_capacity(node_count + 1);
    let shared_root = roots.len() > 1;
    if shared_root {
        frames.push(Node::new(root, -1));
    }
    // parents before children, as MDS frames are
    let mut pending: Vec<(usize, i32)> = roots.iter().rev().map(|&r| (r, if shared_root { 0 } else { -1 })).collect();
    let mut placed = vec![false; node_count];
    while let Some((i, up)) = pending.pop() {
        if std::mem::replace(&mut placed[i], true) {
            bail!("node {i} is reached twice in the node hierarchy");
        }
        let n = &node_list[i];
        let mut frame = Node::new(&names[i], up);
        frame.matrix = floats(&node_matrix(n)?);
        frames.push(frame);
        let at = (frames.len() - 1) as i32;
        let children = n.get("children").and_then(Value::as_list).unwrap_or(&empty);
        for c in children.iter().rev() {
            pending.push((c.as_int().unwrap_or(0) as usize, at));
        }
    }
    if placed.iter().any(|placed| !placed) {
        bail!("the glTF's node hierarchy has a cycle");
    }
    if !node_list.iter().any(|n| n.has("mesh")) {
        bail!("the glTF has no mesh");
    }
    let skeleton = mds::build(&Mds { version, nodes: frames })?;
    Ok(import_gltf(gltf, &skeleton, None, None, None)?.mds)
}

/// The image bytes of glTF image `i`: a data: uri, a file by uri (read through `read`, relative to
/// the glTF), or a buffer view.
pub fn image_bytes(gltf: &Gltf, i: &Value, read: &dyn Fn(&str) -> Result<Vec<u8>>) -> Result<Vec<u8>> {
    let image = index(item(&gltf.json, "images")?, i)?;
    if let Some(uri) = image.get("uri").and_then(Value::as_str) {
        if let Some((_, data)) = uri.strip_prefix("data:").and_then(|rest| rest.split_once(',')) {
            return base64(data);
        }
        return read(uri);
    }
    let Some(view) = image.get("bufferView") else {
        bail!("image {} has neither a uri nor a buffer view", i.as_int().unwrap_or(-1));
    };
    let view = index(item(&gltf.json, "bufferViews")?, view)?;
    let buffer = view.get("buffer").and_then(Value::as_int).unwrap_or(0);
    let Some(buffer) = usize::try_from(buffer).ok().and_then(|b| gltf.buffers.get(b)) else {
        bail!("list index out of range");
    };
    let start = view.get("byteOffset").and_then(Value::as_int).unwrap_or(0).max(0) as usize;
    let length = view.get("byteLength").and_then(Value::as_int).unwrap_or(0).max(0) as usize;
    match buffer.get(start..start + length) {
        Some(bytes) => Ok(bytes.to_vec()),
        None => bail!("an image's buffer view runs past its buffer"),
    }
}

/// (texture name, glTF image index) for every material's base colour texture: the name its MDT
/// material gets, so the picture of that name is the one it shows.
pub fn material_textures(gltf: &Gltf) -> Result<Vec<(String, Value)>> {
    let j = &gltf.json;
    let mut out: Vec<(String, Value)> = Vec::new();
    let count = j.get("materials").and_then(Value::as_list).map_or(0, Vec::len);
    for m in 0..count {
        let number = Value::Int(m as i64);
        let material = index(item(j, "materials")?, &number)?;
        let Some(base) = material.get("pbrMetallicRoughness").and_then(|pbr| pbr.get("baseColorTexture")) else {
            continue;
        };
        let texture = index(item(j, "textures")?, item(base, "index")?)?;
        let Some(source) = texture.get("source") else {
            continue;
        };
        let name = material_from_gltf(j, &number)?.texture;
        if name.is_empty() {
            continue;
        }
        match out.iter().find(|(known, _)| known.eq_ignore_ascii_case(&name)) {
            Some((_, other)) if other.as_int() != source.as_int() => {
                bail!("two different images are named {name}; give each texture its own name")
            }
            Some(_) => {}
            None => out.push((name, source.clone())),
        }
    }
    Ok(out)
}

/// The glTF world matrix of node i (W in the Python), memoised.
fn world_of(
    i: i64,
    nodes: &Value,
    parent: &HashMap<i64, usize>,
    world: &mut HashMap<i64, TypedMatrix>,
    depth: usize,
) -> Result<TypedMatrix> {
    if let Some(m) = world.get(&i) {
        return Ok(m.clone());
    }
    if depth > 900 {
        bail!("maximum recursion depth exceeded");
    }
    let m = node_matrix(index(nodes, &Value::Int(i))?)?;
    let m = match parent.get(&i) {
        Some(&p) => typed_mul(&world_of(p as i64, nodes, parent, world, depth + 1)?, &m),
        None => m,
    };
    world.insert(i, m.clone());
    Ok(m)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_alpha_comes_from_the_nearest_original_vertex() {
        let mdt = Mdt {
            vertices: vec![[0.0, 0.0, 0.0, 1.0], [10.0, 0.0, 0.0, 1.0], [0.5, 0.2, 0.0, 1.0]],
            colours: vec![[1.0, 1.0, 1.0, 0.0], [1.0, 1.0, 1.0, 1.0], [1.0, 1.0, 1.0, 0.25]],
            // vertex and colour indices, no normals or UVs
            prims: vec![Prim {
                kind: mds::F_COLOUR | mds::F_NO_NORMAL | mds::F_NO_UV,
                material: 0,
                verts: vec![vec![0, 0], vec![1, 1], vec![2, 2]],
            }],
            ..Mdt::default()
        };
        let alphas = original_alphas(Some(&mdt));
        assert_eq!(alphas.near(&[0.01, 0.0, 0.0]), 0.0);
        assert_eq!(alphas.near(&[0.5, 0.21, 0.0]), 0.25);
        // moved far from every vertex: the nearest anywhere
        assert_eq!(alphas.near(&[40.0, 3.0, 0.0]), 1.0);
        assert_eq!(original_alphas(None).near(&[0.0, 0.0, 0.0]), 1.0);
    }
}
