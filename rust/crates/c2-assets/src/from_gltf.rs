//! MDS meshes (and .wgt skin weights and the .bbp bind pose) rebuilt from a glTF, with the original
//! MDS as the template: the frame hierarchy stays, and mesh blocks are replaced by name (a glTF mesh
//! node "X" or "X_skin" replaces the MDT of frame X; Blender's ".001" suffixes are ignored).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::json::{self, Value};
use crate::mds::{self, Material, Mds, Mdt, Node, Prim, IDENT};
use crate::mot::{self, KeyList};
use crate::py::{self, Num};
use crate::to_gltf::{mat_inv, mat_mul, normalize, xform_dir, xform_point, Matrix};
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

/// What import_gltf writes: the MDS, the .wgt and the .bbp.
pub struct Imported {
    pub mds: Vec<u8>,
    pub wgt: Vec<u8>,
    pub bbp: Vec<u8>,
}

/// The model rebuilt from a glTF, the template's meshes replaced by the glTF's of the same names
/// (only those in `meshes` when given).
pub fn import_gltf(
    gltf: &Gltf,
    template: &[u8],
    bbp_data: Option<&[u8]>,
    wgt_data: Option<&[u8]>,
    meshes: Option<&BTreeSet<String>>,
) -> Result<Imported> {
    let j = &gltf.json;
    let mut model: Mds = mds::parse(template)?;
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
    let mut by_name: HashMap<String, usize> = HashMap::new();
    for (i, node) in model.nodes.iter().enumerate() {
        by_name.insert(node.base_name().to_string(), i);
    }
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
        Ok(by_name.get(&clean(n.get("name"))?).copied())
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
        if meshes.is_some_and(|wanted| !wanted.contains(&nm)) {
            continue;
        }
        let target = match by_name.get(&nm) {
            Some(&target) => target,
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
                if !n2.has("mesh") && clean(n2.get("name"))? == nm {
                    frame_world = Some(world_of(g2 as i64, nodes, &parent, &mut world, 0)?);
                }
            }
            to_local = match frame_world {
                Some(fw) => {
                    let mesh_world = floats(&world_of(gi as i64, nodes, &parent, &mut world, 0)?);
                    mat_mul(&mat_inv(&floats(&fw)).unwrap_or(IDENT), &mesh_world)
                }
                None => IDENT,
            };
        }
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
            let nrm = optional("NORMAL")?;
            let uv = optional("TEXCOORD_0")?;
            let col = optional("COLOR_0")?;
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
                    materials.push(material_from_gltf(j, &mk)?);
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
                        v.push(1.0);
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
