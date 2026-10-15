//! MDS meshes (and .wgt skin weights and the .bbp bind pose) rebuilt from a glTF, with the original
//! MDS as the template: the frame hierarchy stays, and mesh blocks are replaced by name (a glTF mesh
//! node "X" or "X_skin" replaces the MDT of frame X; Blender's ".001" suffixes are ignored).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use crate::gltf::{AccessorType, ComponentType};
use crate::math::{compose, decompose, mat_inv, mat_mul, normalize, xform_dir, xform_point, Mat4, Quat, IDENT, Z};
use crate::mds::{self, Material, Mds, Mdt, Node, Prim};
use crate::mot::{self, KeyList};
use crate::numeric::{self, Num};
use crate::to_gltf::CollisionAttributes;
use crate::{bail, Context, Error, Result};
use serde_json::{json, Value};

/// A glTF's JSON and its buffers (shared: the models of a map scene are each a glTF over the
/// scene's one buffer).
pub struct Gltf {
    pub json: Value,
    pub buffers: Vec<Arc<Vec<u8>>>,
}

fn missing(key: &str) -> Error {
    Error::invalid(format!("the glTF has no \"{key}\" where it needs one"))
}

/// Whether a value gives something: it is not null, false, zero or empty (a string, list or
/// object). A glTF leaves out what it has nothing for, or writes it so.
pub fn is_set(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(number) => number.as_f64() != Some(0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
    }
}

/// value[key], or an error naming the key.
pub fn item<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
    value.get(key).ok_or_else(|| missing(key))
}

/// list[at], at a JSON int; an index the list does not have is an error.
pub fn index<'a>(list: &'a Value, at: &Value) -> Result<&'a Value> {
    let Some(items) = list.as_array() else {
        bail!("the glTF has something else where it needs a list");
    };
    let found = at.as_u64().and_then(|i| usize::try_from(i).ok()).and_then(|i| items.get(i));
    found.ok_or_else(|| Error::invalid(format!("the glTF's index {at} is not one of a list of {}", items.len())))
}

/// The glTF's JSON.
pub fn load_json(text: &str) -> Result<Value> {
    serde_json::from_str(text).context("the glTF's JSON does not read")
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
    std::str::from_utf8(data).map_err(|_| Error::invalid("the glTF is not UTF-8"))
}

/// A .gltf (with its buffers read through `read`, by path relative to the glTF's folder) or a .glb.
pub fn load_gltf(raw: &[u8], read: &dyn Fn(&str) -> Result<Vec<u8>>) -> Result<Gltf> {
    if raw.starts_with(b"glTF") {
        let mut o = 12;
        let (mut found, mut bin) = (None, Vec::new());
        while o < raw.len() {
            if o + 8 > raw.len() {
                bail!("the .glb ends inside the header of a chunk at {o}");
            }
            let length = crate::bytes::u32_at(raw, o)? as usize;
            let chunk = crate::bytes::clamped(raw, o + 8, o + 8 + length);
            match &raw[o + 4..o + 8] {
                b"JSON" => found = Some(load_json(utf8(chunk)?)?),
                b"BIN\0" => bin = chunk.to_vec(),
                _ => {}
            }
            o += 8 + length;
        }
        let Some(json) = found else {
            bail!("the .glb has no JSON chunk");
        };
        return Ok(Gltf { json, buffers: vec![Arc::new(bin)] });
    }
    with_buffers(load_json(utf8(raw)?)?, &|uri| read(uri).map(Arc::new))
}

/// A .gltf's JSON with its buffers: data: URIs decoded, files read through `read`.
pub fn with_buffers(json: Value, read: &dyn Fn(&str) -> Result<Arc<Vec<u8>>>) -> Result<Gltf> {
    let mut buffers = Vec::new();
    let Some(list) = item(&json, "buffers")?.as_array() else {
        bail!("the glTF's buffers are not a list");
    };
    for buffer in list {
        let Some(uri) = item(buffer, "uri")?.as_str() else {
            bail!("the buffer's uri is not text");
        };
        if uri.starts_with("data:") {
            let Some((_, data)) = uri.split_once(',') else {
                bail!("a buffer's data: URI has no comma before its data");
            };
            buffers.push(Arc::new(base64(data)?));
        } else {
            buffers.push(read(uri)?);
        }
    }
    Ok(Gltf { json, buffers })
}

/// An accessor's values: `n` components each, ints or floats as its component type has them.
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

    /// The c-th value's components; an index the accessor does not have is an error.
    pub fn at(&self, c: i64) -> Result<&[Num]> {
        let Some(i) = usize::try_from(c).ok().filter(|&i| i < self.len()) else {
            bail!("the glTF's index {c} is not one of an accessor's {} values", self.len());
        };
        let start = i * self.n;
        Ok(&self.values[start..(start + self.n).min(self.values.len())])
    }
}

/// A JSON int (or a bool, as 0 or 1).
fn json_int(value: &Value, what: &str) -> Result<i64> {
    match value.as_i64().or_else(|| value.as_bool().map(i64::from)) {
        Some(int) => Ok(int),
        None => bail!("the glTF's {what} is {value}, not an integer"),
    }
}

/// An accessor's componentType, which must be one glTF has.
fn component_type(value: &Value, what: &str) -> Result<ComponentType> {
    let code = json_int(value, what)?;
    ComponentType::from_code(code)
        .ok_or_else(|| Error::invalid(format!("an accessor's {what} is {code}, which glTF does not have")))
}

/// The values of accessor i.
pub fn accessor(gltf: &Gltf, i: &Value) -> Result<Values> {
    let a = index(item(&gltf.json, "accessors")?, i)?;
    let kind = item(a, "type")?.as_str().unwrap_or_default();
    let Some(kind) = AccessorType::from_name(kind) else {
        bail!("an accessor of type {kind:?}, which glTF does not have");
    };
    let n = kind.components();
    let component = component_type(item(a, "componentType")?, "componentType")?;
    let count = json_int(item(a, "count")?, "count")?.max(0) as usize;
    let mut values = match a.get("bufferView") {
        // zeros (a sparse accessor's base): as many as a model can have, not what a count says
        None if count.checked_mul(n).is_none_or(|values| values > MAX_ZEROS) => {
            bail!("an accessor of {count} values without a buffer view is more than a model can hold")
        }
        None => vec![Num::Int(0); count * n],
        Some(view) => view_values(gltf, view, a.get("byteOffset"), count, n, component)?,
    };
    // a sparse accessor: its values at the indices it lists replaced
    if let Some(sparse) = a.get("sparse") {
        let changed = json_int(item(sparse, "count")?, "count")?.max(0) as usize;
        let indices = item(sparse, "indices")?;
        let index_type = component_type(item(indices, "componentType")?, "sparse indices' componentType")?;
        if !index_type.is_unsigned() {
            bail!("an accessor's sparse indices are of component type {}, not an unsigned integer", index_type.code());
        }
        let at = view_values(gltf, item(indices, "bufferView")?, indices.get("byteOffset"), changed, 1, index_type)?;
        let given = item(sparse, "values")?;
        let replaced = view_values(gltf, item(given, "bufferView")?, given.get("byteOffset"), changed, n, component)?;
        for (k, place) in at.iter().enumerate() {
            let place = place.f() as usize;
            if place >= count {
                bail!("a sparse index {place} is past the accessor's {count} values");
            }
            values[place * n..(place + 1) * n].copy_from_slice(&replaced[k * n..(k + 1) * n]);
        }
    }
    if a.get("normalized").is_some_and(|normalized| normalized.as_bool() == Some(true)) {
        let Some(divisor) = component.divisor() else {
            bail!(
                "a normalized accessor of component type {}, which only integers of 8 or 16 bits can be",
                component.code()
            );
        };
        values = values.into_iter().map(|v| Num::Float(numeric::max(-1.0, v.f() / divisor))).collect();
    }
    Ok(Values { n, values })
}

/// The most values an accessor without a buffer view (all zeros) may have.
const MAX_ZEROS: usize = 1 << 26;

/// `count` values of `n` components of a component type from a buffer view, at an offset in it.
fn view_values(
    gltf: &Gltf,
    view: &Value,
    offset: Option<&Value>,
    count: usize,
    n: usize,
    component: ComponentType,
) -> Result<Vec<Num>> {
    let size = component.size();
    let view = index(item(&gltf.json, "bufferViews")?, view)?;
    let buffer_number = view.get("buffer").map_or(Ok(0), |b| json_int(b, "buffer"))?;
    let Some(buffer) = usize::try_from(buffer_number).ok().and_then(|b| gltf.buffers.get(b)) else {
        bail!("the glTF has no buffer {buffer_number}");
    };
    // in i128, which no sum or product of a count, stride and offsets of JSON ints overflows
    let base = view.get("byteOffset").map_or(Ok(0), |o| json_int(o, "byteOffset"))? as i128
        + offset.map_or(Ok(0), |o| json_int(o, "byteOffset"))? as i128;
    // a stride of 0 (or none) means the values are packed
    let stride = match view.get("byteStride").map(|s| json_int(s, "byteStride")).transpose()? {
        Some(stride) if stride != 0 => stride as i128,
        _ => (n * size) as i128,
    };
    // the first and last values in the buffer, before anything is allocated for them (the values
    // between lie between them)
    let value_size = (n * size) as i128;
    for k in [0, count.saturating_sub(1)] {
        let at = base + k as i128 * stride;
        if count > 0 && (at < 0 || at + value_size > buffer.len() as i128) {
            bail!(
                "an accessor's value {k} (of {count}) runs past the end of buffer {buffer_number} ({} bytes)",
                buffer.len()
            );
        }
    }
    let mut values = Vec::with_capacity(count * n);
    for k in 0..count {
        let at = (base + k as i128 * stride) as usize;
        for j in 0..n {
            values.push(component.read(&buffer[at + j * size..at + (j + 1) * size]));
        }
    }
    Ok(values)
}

/// The node's name without Blender's ".001" duplicate suffix.
pub fn clean(name: Option<&Value>) -> Result<String> {
    let name = match name {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(other) => bail!("the glTF has a name that is not text: {other}"),
    };
    let chars: Vec<char> = name.chars().collect();
    let n = chars.len();
    if n > 4 && chars[n - 4] == '.' && chars[n - 3..].iter().all(char::is_ascii_digit) {
        return Ok(chars[..n - 4].iter().collect());
    }
    Ok(name)
}

/// A JSON number (a bool as 0 or 1), an int kept apart from a float.
fn num(value: &Value) -> Result<Num> {
    match value {
        Value::Bool(b) => Ok(Num::Int(*b as i64)),
        _ => Num::of(value).ok_or_else(|| Error::invalid(format!("the glTF has {value} where it needs a number"))),
    }
}

fn nums(value: Option<&Value>, default: &[i64]) -> Result<Vec<Num>> {
    match value {
        None => Ok(default.iter().map(|&v| Num::Int(v)).collect()),
        Some(Value::Array(items)) => items.iter().map(num).collect(),
        Some(other) => bail!("the glTF has {other} where it needs a list of numbers"),
    }
}

type TypedMatrix = Vec<Num>;

/// A node's local matrix from its matrix or its translation, rotation and scale.
fn node_matrix(node: &Value) -> Result<TypedMatrix> {
    if let Some(matrix) = node.get("matrix") {
        let m = nums(Some(matrix), &[])?;
        if m.len() < 16 {
            bail!("a node's matrix has {} numbers, not 16", m.len());
        }
        return Ok(m);
    }
    let t = nums(node.get("translation"), &[0, 0, 0])?;
    let r = nums(node.get("rotation"), &[0, 0, 0, 1])?;
    let s = nums(node.get("scale"), &[1, 1, 1])?;
    if r.len() != 4 {
        bail!("a node's rotation has {} numbers, not 4", r.len());
    }
    if t.len() < 3 || s.len() < 3 {
        bail!("a node's translation or scale has fewer than 3 numbers");
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
            out.push(numeric::sum_nums((0..4).map(|k| a[k * 4 + r] * b[c * 4 + k])));
        }
    }
    out
}

fn floats(m: &TypedMatrix) -> Mat4 {
    let mut out = [0.0; 16];
    for (slot, v) in out.iter_mut().zip(m) {
        *slot = v.f();
    }
    out
}

/// Whether a bind matrix (the inverse of a glTF's inverse bind matrix) is the one exported, within
/// what Blender's round trip moves it: it rests bones at the bind pose and writes their inverses
/// back in 32 bits, a bone's direction only as good as its length allows (head and tail in 32
/// bits: 3e-4 radians on a bone 0.07 long 200 units out, which its inverse turns into 0.07 in the
/// translation): 1e-3 of the larger of 1 and the value, the translation (12-14, the bone's place)
/// within 0.005 and 2e-5 of the value (Blender chains the bones' rests down the skeleton in 32 bits).
fn close_bind(a: &Mat4, b: &Mat4) -> bool {
    a.iter().zip(b).enumerate().all(|(at, (x, y))| {
        let difference = (x - y).abs();
        match at {
            12..=14 => difference <= 5e-3 + 2e-5 * y.abs(),
            _ => difference <= 1e-3 * numeric::max(1.0, y.abs()),
        }
    })
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
            let (Some(p), Some(colour)) = (mdt.vertices.get(corner[v] as usize), mdt.colours.get(corner[c] as usize))
            else {
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
        let around =
            (-1..=1).flat_map(|dx| (-1..=1).flat_map(move |dy| (-1..=1).map(move |dz| (x + dx, y + dy, z + dz))));
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
        Value::Array(items) => items
            .iter()
            .map(|v| v.as_f64().ok_or_else(|| Error::invalid(format!("the glTF has {v} where it needs a number"))))
            .collect(),
        other => bail!("the glTF has {other} where it needs a list of numbers"),
    }
}

/// The MDT material for a glTF material (its extras when to_gltf wrote them).
fn material_from_gltf(gltf: &Value, number: &Value) -> Result<Material> {
    let mut m = Material::new("");
    if matches!(number, Value::Null) {
        return Ok(m);
    }
    let gm = index(item(gltf, "materials")?, number)?;
    let empty = json!({});
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
        m.power =
            value.as_f64().ok_or_else(|| Error::invalid(format!("a material's mdt_power is {value}, not a number")))?;
    }
    let mut texture = extras.get("texture").filter(|v| !matches!(v, Value::Null)).cloned();
    if texture.is_none() {
        if let Some(base) = pbr.get("baseColorTexture") {
            let tx = index(item(gltf, "textures")?, item(base, "index")?)?;
            let image = match tx.get("source") {
                Some(source) => index(item(gltf, "images")?, source)?.clone(),
                None => json!({}),
            };
            let name = image.get("name").filter(|v| is_set(v)).cloned();
            texture = Some(match name {
                Some(name) => name,
                None => {
                    let uri = image.get("uri").and_then(Value::as_str).unwrap_or("");
                    let base_name = uri.rsplit(['/', '\\']).next().unwrap_or("");
                    Value::String(crate::to_gltf::strip_extension(base_name).to_string())
                }
            });
        }
    }
    let texture = texture.unwrap_or_else(|| gm.get("name").cloned().unwrap_or(Value::String(String::new())));
    let Some(texture) = texture.as_str() else {
        bail!("the material's texture name is not text");
    };
    let encoded_length: usize =
        texture.chars().map(|c| dc2tk_disc::text::encode(&c.to_string()).map_or(1, |b| b.len())).sum();
    if encoded_length > 31 {
        bail!("texture name too long: {texture:?}");
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

/// How import reads the glTF, and the template's files besides its MDS.
#[derive(Clone, Copy, Debug, Default)]
pub struct ImportOptions<'a> {
    /// The template's bind pose (.bbp); without one, its frames' rests.
    pub bbp: Option<&'a [u8]>,
    /// The template's skin weights (.wgt).
    pub wgt: Option<&'a [u8]>,
    /// The template meshes to replace (by what each glTF mesh node stands for, mesh_ids); every
    /// one when None.
    pub meshes: Option<&'a BTreeSet<MeshId>>,
    /// The model is collision: each mesh is written as type 0x213 triangle lists of positions alone
    /// (normals, uvs and colours dropped), each material's diffuse colour encoding the attributes
    /// its name gives (`col_ground0_foot2_area6_mask12`, see `collision_material`).
    pub collision: bool,
}

/// The template frame each glTF node stands for: the node's `extras.mds_index` when it has one
/// within the template, otherwise its name (Blender's ".001" and a skinned mesh node's "_skin"
/// dropped) as a frame's whole name, else as a frame's name without its `--` flags; the k-th node
/// of a name given more than once takes the k-th frame of that name, counting by Blender's ".NNN"
/// (which numbers names in the order it made them, the template's) and then in glTF order; a name
/// without flags takes the frames of that whole name first, then those of that name with flags. A
/// rigid mesh node under a node of its own name stands, in a skinned model, for its parent's frame
/// (Blender makes every frame of a skinned model a bone, and a mesh on one an object of the same
/// name parented to it), else for the frame of its name under the parent's. None for a node no
/// frame has the name of.
pub fn frames_of(json: &Value, model: &Mds) -> Result<Vec<Option<usize>>> {
    let empty = Vec::new();
    let nodes = match json.get("nodes") {
        Some(nodes) => nodes.as_array().ok_or_else(|| Error::invalid("the glTF's nodes are not a list"))?,
        None => &empty,
    };
    let mut whole: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut base: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, node) in model.nodes.iter().enumerate() {
        whole.entry(node.name.as_str()).or_default().push(i);
        base.entry(node.base_name()).or_default().push(i);
    }
    let parent = parents_of(nodes);
    let skinned = json.get("skins").and_then(Value::as_array).is_some_and(|skins| !skins.is_empty());
    let index_of = |n: &Value| {
        let index = n.get("extras").and_then(|e| e.get("mds_index")).and_then(Value::as_i64);
        index.and_then(|i| usize::try_from(i).ok()).filter(|&i| i < model.nodes.len())
    };
    let names: Vec<String> = nodes.iter().map(|n| frame_name(json, n)).collect::<Result<_>>()?;
    // rigid mesh nodes under a node of their own name: placed after the others, from their parents
    let split: Vec<Option<usize>> = (0..nodes.len())
        .map(|gi| {
            let n = &nodes[gi];
            let up = parent.get(&(gi as i64)).copied()?;
            let lone = n.get("mesh").is_some()
                && !n.get("skin").is_some()
                && index_of(n).is_none()
                && !nodes[up].get("mesh").is_some();
            (lone && names[up] == names[gi]).then_some(up)
        })
        .collect();
    // the nodes of each name, by Blender's number and then in glTF order
    let mut order: Vec<usize> = (0..nodes.len()).collect();
    let suffix = |gi: usize| -> u32 {
        let name = nodes[gi].get("name").and_then(Value::as_str).unwrap_or("");
        match name.rsplit_once('.') {
            Some((_, digits)) if digits.len() == 3 && digits.chars().all(|c| c.is_ascii_digit()) => {
                digits.parse().unwrap_or(0)
            }
            _ => 0,
        }
    };
    order.sort_by_key(|&gi| (suffix(gi), gi));
    let mut taken: HashMap<(bool, String), usize> = HashMap::new();
    let mut out = vec![None; nodes.len()];
    // the frames a name may stand for, in the order its nodes take them: those of that whole name
    // where the glTF names nodes by whole names (a map's), else every frame of the name in file
    // order (a character's nodes are named without flags, and Blender numbers them in that order)
    let whole_names = names.iter().any(|name| name.contains("--"));
    let by_name = |gi: usize, taken: &mut HashMap<(bool, String), usize>| -> Option<usize> {
        let name = &names[gi];
        let stem = name.split_once("--").map_or(name.as_str(), |(stem, _)| stem);
        let frames = match (whole.get(name.as_str()), base.get(name.as_str()), base.get(stem)) {
            (Some(frames), None, _) => frames,
            (Some(frames), Some(_), _) if whole_names => frames,
            (_, Some(frames), _) | (None, None, Some(frames)) => frames,
            _ => return None,
        };
        let k = taken.entry((nodes[gi].get("skin").is_some(), name.clone())).or_insert(0);
        *k += 1;
        Some(frames[(*k - 1).min(frames.len() - 1)])
    };
    for &gi in &order {
        out[gi] = match (index_of(&nodes[gi]), split[gi]) {
            (Some(i), _) => Some(i),
            (None, Some(_)) => continue,
            (None, None) => by_name(gi, &mut taken),
        };
    }
    for gi in order {
        let Some(up) = split[gi] else {
            continue;
        };
        // a skinned model's: the bone's frame; a rigid model's mesh is a frame of its own, the
        // one of its name under the parent's frame
        let name = &names[gi];
        let under = |p: usize, same: &dyn Fn(&Node) -> bool| {
            model.nodes.iter().position(|node| node.parent == p as i32 && same(node))
        };
        out[gi] = match out[up] {
            Some(p) if skinned => Some(p),
            Some(p) => under(p, &|node| node.name == *name).or_else(|| under(p, &|node| node.base_name() == name)),
            None => None,
        };
        if out[gi].is_none() {
            out[gi] = by_name(gi, &mut taken);
        }
    }
    Ok(out)
}

/// The mesh nodes of a glTF with the template mesh each stands for.
pub fn mesh_ids(json: &Value, model: &Mds) -> Result<Vec<(usize, MeshId)>> {
    let frames = frames_of(json, model)?;
    let empty = Vec::new();
    let nodes = json.get("nodes").and_then(Value::as_array).unwrap_or(&empty);
    let mut out = Vec::new();
    for (gi, n) in nodes.iter().enumerate() {
        if n.get("mesh").is_some() {
            let id = match frames[gi] {
                Some(frame) => MeshId::Frame(frame),
                None => MeshId::New(frame_name(json, n)?),
            };
            out.push((gi, id));
        }
    }
    Ok(out)
}

/// Where a corner's values are in Geometry's lists: position, UVs, colour.
const UV: usize = 3;
const COLOUR: usize = 5;
const CORNER: usize = 9;

/// A mesh as values: its node's matrix and, per material (by name, without a `.NNN` suffix, and
/// the picture it shows), its triangles' corners: position (0-2), first UVs (3-4) and colour
/// (5-8), NaN where the primitive has none (and alpha NaN where the colour has none), then for a
/// skinned mesh its skin: (frame, weight) pairs by frame, the weights summing to 1.
pub struct Geometry {
    pub matrix: Vec<f64>,
    pub groups: BTreeMap<String, Vec<[Vec<f64>; 3]>>,
}

/// {mesh: its geometry}, for comparing a glTF that went through another tool (Blender), which
/// splits and reorders vertices, recomputes normals, adds float noise and moves colours by a step.
pub fn mesh_geometry(gltf: &Gltf, model: &Mds) -> Result<BTreeMap<MeshId, Geometry>> {
    geometry(gltf, model, false, None)
}

/// mesh_geometry for a character: each mesh in the space import_gltf builds it in, so that a tool
/// that moves meshes between nodes (Blender makes a rigid mesh an object parented to its bone, and
/// rests bones at the bind pose) still matches. A rigid mesh's corners are in its frame's space
/// (through the glTF hierarchy, as `rigid_to_local` gives it), a skinned mesh's in model space
/// with each corner's skin (joints as the frames they stand for); the matrix is the identity.
pub fn frame_geometry(gltf: &Gltf, model: &Mds) -> Result<BTreeMap<MeshId, Geometry>> {
    geometry(gltf, model, true, None)
}

/// The parent of every glTF node that has one.
fn parents_of(node_list: &[Value]) -> HashMap<i64, usize> {
    let mut parent = HashMap::new();
    for (i, n) in node_list.iter().enumerate() {
        for c in n.get("children").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default() {
            match Num::of(c) {
                Some(Num::Int(c)) => {
                    parent.insert(c, i);
                }
                Some(Num::Float(c)) if c.fract() == 0.0 => {
                    parent.insert(c as i64, i);
                }
                _ => {}
            }
        }
    }
    parent
}

/// The matrix taking rigid mesh node `gi`'s positions into the space of the frame it stands for
/// (`resolved`, frames_of): through the glTF node of that frame without a mesh (Blender's bone, the
/// mesh an object parented to it), else, when the mesh node is the frame's own node (an export's),
/// from the glTF's world to the frame's in the template (`template_world`), so a node left where
/// it was changes nothing and one moved carries its mesh. None for a mesh no frame stands for.
fn rigid_to_local(
    gi: usize,
    resolved: &[Option<usize>],
    nodes: &Value,
    parent: &HashMap<i64, usize>,
    world: &mut HashMap<i64, TypedMatrix>,
    template_world: &[Mat4],
) -> Result<Option<Mat4>> {
    let Some(frame) = resolved[gi] else {
        return Ok(None);
    };
    let node_list = nodes.as_array().map(Vec::as_slice).unwrap_or_default();
    let mut frame_world = None;
    for (g2, n2) in node_list.iter().enumerate() {
        if !n2.get("mesh").is_some() && resolved[g2] == Some(frame) {
            frame_world = Some(floats(&world_of(g2 as i64, nodes, parent, world, 0)?));
        }
    }
    let frame_world = frame_world.or_else(|| template_world.get(frame).copied()).unwrap_or(IDENT);
    let mesh_world = floats(&world_of(gi as i64, nodes, parent, world, 0)?);
    Ok(Some(mat_mul(&mat_inv(&frame_world).unwrap_or(IDENT), &mesh_world)))
}

/// A corner's skin as Geometry keeps it: up to 4 (frame, weight) pairs, the largest weights,
/// normalised and summed by frame, in frame order; joints no frame stands for are numbered past
/// the model's frames.
fn skin_tail(joints: &[Num], weights: &[Num], joint_frames: &[i64]) -> Vec<f64> {
    let mut pairs: Vec<(f64, i64)> = joints
        .iter()
        .zip(weights)
        .filter(|(_, w)| w.f() > 0.0)
        .map(|(j, w)| {
            let j = j.f() as i64;
            let frame = usize::try_from(j).ok().and_then(|j| joint_frames.get(j)).copied().unwrap_or(-1);
            (w.f(), frame)
        })
        .collect();
    pairs.sort_by(|a, b| b.0.total_cmp(&a.0));
    pairs.truncate(4);
    let total: f64 = pairs.iter().map(|(w, _)| w).sum();
    let mut by_frame: BTreeMap<i64, f64> = BTreeMap::new();
    for (w, frame) in pairs {
        *by_frame.entry(frame).or_default() += if total > 0.0 { w / total } else { 0.0 };
    }
    by_frame.into_iter().flat_map(|(frame, w)| [frame as f64, w]).collect()
}

/// `binds`: for a character's skinned meshes, the bind matrix of each frame to bring the glTF's
/// positions into (skin_matrices), as import_gltf does.
fn geometry(
    gltf: &Gltf,
    model: &Mds,
    in_frames: bool,
    binds: Option<&HashMap<usize, Mat4>>,
) -> Result<BTreeMap<MeshId, Geometry>> {
    fn strip_suffix(name: &str) -> &str {
        match name.rsplit_once('.') {
            Some((stem, digits)) if digits.len() == 3 && digits.chars().all(|c| c.is_ascii_digit()) => stem,
            _ => name,
        }
    }
    let mut out = BTreeMap::new();
    let resolved = frames_of(&gltf.json, model)?;
    // a character's animated nodes at their frames' rests (where Blender writes them posed)
    let at_rest;
    let nodes_value = match in_frames {
        true => {
            at_rest = animated_nodes_at_rest(&gltf.json, model, &resolved)?;
            &at_rest
        }
        false => item(&gltf.json, "nodes")?,
    };
    let nodes = nodes_value.as_array().ok_or_else(|| Error::invalid("the glTF's nodes are not a list"))?;
    let parent = parents_of(nodes);
    let mut world: HashMap<i64, TypedMatrix> = HashMap::new();
    let template_world = model.world_matrices()?;
    for (gi, id) in mesh_ids(&gltf.json, model)? {
        let node = &nodes[gi];
        // characters: where the corners go, and the frame each joint stands for
        let mut to_local = None;
        let mut joint_frames: Option<Vec<i64>> = None;
        let mut reskin: Option<Vec<Mat4>> = None;
        if in_frames {
            match node.get("skin") {
                Some(skin) => {
                    if let Some(binds) = binds {
                        reskin = Some(skin_matrices(gltf, skin, &resolved, binds)?);
                    }
                    let skin = index(item(&gltf.json, "skins")?, skin)?;
                    let joints = item(skin, "joints")?
                        .as_array()
                        .ok_or_else(|| Error::invalid("a skin's joints are not a list"))?;
                    let mut frames = Vec::with_capacity(joints.len());
                    for joint in joints {
                        let at = joint.as_i64().and_then(|j| usize::try_from(j).ok());
                        let frame = at.and_then(|j| resolved.get(j).copied().flatten());
                        frames.push(match (frame, at) {
                            (Some(frame), _) => frame as i64,
                            (None, at) => (model.nodes.len() + at.unwrap_or(nodes.len())) as i64,
                        });
                    }
                    joint_frames = Some(frames);
                }
                None => {
                    let fallback = floats(&world_of(gi as i64, nodes_value, &parent, &mut world, 0)?);
                    let found = rigid_to_local(gi, &resolved, nodes_value, &parent, &mut world, &template_world)?;
                    to_local = Some(found.unwrap_or(fallback));
                }
            }
        }
        let list = |key: &str| -> Option<Vec<f64>> {
            node.get(key).and_then(Value::as_array).map(|l| l.iter().map(|v| v.as_f64().unwrap_or(0.0)).collect())
        };
        let matrix = match list("matrix").filter(|m| m.len() == 16) {
            _ if in_frames => IDENT.to_vec(),
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
        let Some(primitives) = item(mesh, "primitives")?.as_array() else {
            bail!("a mesh's primitives are not a list");
        };
        let mut groups: BTreeMap<String, Vec<[Vec<f64>; 3]>> = BTreeMap::new();
        for primitive in primitives {
            let material = match primitive.get("material") {
                Some(number) => {
                    let found = index(item(&gltf.json, "materials")?, number)?;
                    // with the picture it shows, so a material given another picture differs; a
                    // character's by its base colour image alone, since Blender drops extras
                    // unless told and a model made in it names materials its own way
                    let name = strip_suffix(found.get("name").and_then(Value::as_str).unwrap_or(""));
                    match in_frames {
                        true => format!("\u{0}{}", picture_of(&gltf.json, found)?),
                        false => {
                            let extras = found.get("extras");
                            let texture = extras.and_then(|extras| extras.get("texture")).and_then(Value::as_str);
                            format!("{name}\u{0}{}", texture.unwrap_or(""))
                        }
                    }
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
            let skin = match &joint_frames {
                Some(frames) => match (read("JOINTS_0")?, read("WEIGHTS_0")?) {
                    (Some(joints), Some(weights)) => Some((frames, joints, weights)),
                    _ => None,
                },
                None => None,
            };
            let joint_weights = match &reskin {
                Some(_) => match (read("JOINTS_0")?, read("WEIGHTS_0")?) {
                    (Some(joints), Some(weights)) => Some((joints, weights)),
                    _ => None,
                },
                None => None,
            };
            let corner = |at: i64| -> Result<Vec<f64>> {
                let mut values = vec![f64::NAN; CORNER];
                for (slot, value) in positions.at(at)?.iter().take(3).enumerate() {
                    values[slot] = value.f();
                }
                if let (Some(matrices), Some((joints, weights))) = (&reskin, &joint_weights) {
                    let m = blend(matrices, joints.at(at)?, weights.at(at)?);
                    let moved = xform_point(&m, &values[..3]);
                    values[..3].copy_from_slice(&moved);
                }
                if let Some(to_local) = &to_local {
                    let moved = xform_point(to_local, &values[..3]);
                    values[..3].copy_from_slice(&moved);
                }
                if let Some(uvs) = &uvs {
                    for (slot, value) in uvs.at(at)?.iter().take(2).enumerate() {
                        values[UV + slot] = value.f();
                    }
                }
                if let Some(colors) = &colors {
                    for (slot, value) in colors.at(at)?.iter().take(4).enumerate() {
                        values[COLOUR + slot] = value.f();
                    }
                }
                if let Some((frames, joints, weights)) = &skin {
                    values.extend(skin_tail(joints.at(at)?, weights.at(at)?, frames));
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
        // a material with degenerate triangles alone (placeholders) is one a tool may drop
        groups.retain(|_, triangles| !triangles.is_empty());
        out.insert(id, Geometry { matrix, groups });
    }
    Ok(out)
}

/// The bind matrix (the inverse of its inverse bind matrix) of each frame a glTF's skins' joints
/// stand for.
pub fn bind_matrices(gltf: &Gltf, model: &Mds) -> Result<HashMap<usize, Mat4>> {
    let resolved = frames_of(&gltf.json, model)?;
    let mut out = HashMap::new();
    for skin in gltf.json.get("skins").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default() {
        let (Some(ibm), Some(joints)) = (skin.get("inverseBindMatrices"), skin.get("joints").and_then(Value::as_array))
        else {
            continue;
        };
        let ibm = accessor(gltf, ibm)?;
        for (k, joint) in joints.iter().enumerate() {
            let frame =
                joint.as_i64().and_then(|j| usize::try_from(j).ok()).and_then(|j| resolved.get(j).copied().flatten());
            let values: Vec<f64> = ibm.at(k as i64)?.iter().map(|v| v.f()).collect();
            if let (Some(frame), true) = (frame, values.len() >= 16) {
                if let Some(bind) = mat_inv(&values[..16].try_into().unwrap()) {
                    out.entry(frame).or_insert(bind);
                }
            }
        }
    }
    Ok(out)
}

/// The matrix of each joint of a skin that takes a position of the glTF's bind pose to the same
/// place in another's (`binds`, by frame): bind · inverse bind matrix, the identity for a joint no
/// frame of `binds` stands for. Blender rests a bone at a bind pose without scale or shear and
/// moves the vertices to match (skin_into_bind_pose); this undoes it.
fn skin_matrices(
    gltf: &Gltf,
    skin: &Value,
    resolved: &[Option<usize>],
    binds: &HashMap<usize, Mat4>,
) -> Result<Vec<Mat4>> {
    let skin = index(item(&gltf.json, "skins")?, skin)?;
    let Some(joints) = item(skin, "joints")?.as_array() else {
        bail!("a skin's joints are not a list");
    };
    let ibm = skin.get("inverseBindMatrices").map(|at| accessor(gltf, at)).transpose()?;
    let mut out = Vec::with_capacity(joints.len());
    for (k, joint) in joints.iter().enumerate() {
        let frame =
            joint.as_i64().and_then(|j| usize::try_from(j).ok()).and_then(|j| resolved.get(j).copied().flatten());
        let inverse: Option<Mat4> = match &ibm {
            Some(ibm) => {
                let values: Vec<f64> = ibm.at(k as i64)?.iter().map(|v| v.f()).collect();
                (values.len() >= 16).then(|| values[..16].try_into().unwrap())
            }
            None => Some(IDENT),
        };
        out.push(match (frame.and_then(|f| binds.get(&f)), inverse) {
            (Some(bind), Some(inverse)) => mat_mul(bind, &inverse),
            _ => IDENT,
        });
    }
    Ok(out)
}

/// The weighted sum of the joints' matrices a vertex's joints and weights give (the identity for
/// a vertex of no weight).
fn blend(matrices: &[Mat4], joints: &[Num], weights: &[Num]) -> Mat4 {
    let mut out = [0.0; 16];
    let mut total = 0.0;
    for (joint, weight) in joints.iter().zip(weights) {
        let (Some(m), w) = (usize::try_from(joint.f() as i64).ok().and_then(|j| matrices.get(j)), weight.f()) else {
            continue;
        };
        if w > 0.0 {
            for (slot, value) in out.iter_mut().zip(m) {
                *slot += w * value;
            }
            total += w;
        }
    }
    if total > 0.0 {
        out.map(|value| value / total)
    } else {
        IDENT
    }
}

/// A mesh's morph targets as the vertices of the frame's mesh they move.
pub struct MorphMesh {
    /// The glTF node of the mesh (whose `weights` an animation drives).
    pub node: usize,
    /// The targets' names (the mesh's `extras.targetNames`, else `target<i>`).
    pub names: Vec<String>,
    /// Per vertex of the frame's mesh the glTF's vertices stand for: its position in the mesh's
    /// own space, and its position under each target at weight 1.
    pub vertices: BTreeMap<usize, ([f64; 3], Vec<[f64; 3]>)>,
}

/// The morph targets of each frame's mesh in a glTF (by the frame its mesh node stands for), its
/// vertices matched to those of the frame's mesh in `model` by position (within 0.005 and 2e-6 of
/// the value, as same_geometry compares them; every vertex of the mesh at that place), in the
/// space the import brings them into: a rigid mesh's frame's, a skinned mesh's own (`binds`, the
/// bind matrices of the pack's frames, as bind_matrices gives them for its export).
pub fn morph_meshes(gltf: &Gltf, model: &Mds, binds: &HashMap<usize, Mat4>) -> Result<BTreeMap<usize, MorphMesh>> {
    let mut out = BTreeMap::new();
    let resolved = frames_of(&gltf.json, model)?;
    let nodes_value = animated_nodes_at_rest(&gltf.json, model, &resolved)?;
    let nodes = nodes_value.as_array().ok_or_else(|| Error::invalid("the glTF's nodes are not a list"))?;
    let parent = parents_of(nodes);
    let mut world: HashMap<i64, TypedMatrix> = HashMap::new();
    let template_world = model.world_matrices()?;
    for (gi, id) in mesh_ids(&gltf.json, model)? {
        let MeshId::Frame(frame) = id else {
            continue;
        };
        let node = &nodes[gi];
        let mesh = index(item(&gltf.json, "meshes")?, item(node, "mesh")?)?;
        let Some(primitives) = item(mesh, "primitives")?.as_array() else {
            bail!("a mesh's primitives are not a list");
        };
        let count = primitives.iter().filter_map(|p| p.get("targets").and_then(Value::as_array)).map(Vec::len).max();
        let (Some(count), Some(t)) = (count.filter(|&c| c > 0), model.nodes[frame].mdt.as_ref()) else {
            continue;
        };
        let names: Vec<String> = match mesh.get("extras").and_then(|e| e.get("targetNames")).and_then(Value::as_array) {
            Some(list) => (0..count)
                .map(|i| list.get(i).and_then(Value::as_str).map_or(format!("target{i}"), str::to_string))
                .collect(),
            None => (0..count).map(|i| format!("target{i}")).collect(),
        };
        // into the mesh's own space
        let (to_local, reskin) = match node.get("skin") {
            Some(skin) => {
                let bind = binds.get(&frame).copied().or_else(|| template_world.get(frame).copied()).unwrap_or(IDENT);
                (mat_inv(&bind).unwrap_or(IDENT), Some(skin_matrices(gltf, skin, &resolved, binds)?))
            }
            None => {
                let found = rigid_to_local(gi, &resolved, &nodes_value, &parent, &mut world, &template_world)?;
                (found.unwrap_or(IDENT), None)
            }
        };
        // the mesh's vertices by place, in cells of 0.01
        let cell = |p: &[f64]| -> (i64, i64, i64) {
            ((p[0] / 0.01).floor() as i64, (p[1] / 0.01).floor() as i64, (p[2] / 0.01).floor() as i64)
        };
        let mut cells: HashMap<(i64, i64, i64), Vec<usize>> = HashMap::new();
        for (v, vertex) in t.vertices.iter().enumerate() {
            cells.entry(cell(&vertex[..3])).or_default().push(v);
        }
        let near = |p: &[f64; 3]| -> Vec<usize> {
            let (cx, cy, cz) = cell(p);
            let mut found = Vec::new();
            for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        for &v in cells.get(&(cx + dx, cy + dy, cz + dz)).into_iter().flatten() {
                            let q = &t.vertices[v];
                            if (0..3).all(|i| (p[i] - q[i]).abs() <= 5e-3 + 2e-6 * p[i].abs().max(q[i].abs())) {
                                found.push(v);
                            }
                        }
                    }
                }
            }
            found
        };
        let mut vertices: BTreeMap<usize, ([f64; 3], Vec<[f64; 3]>)> = BTreeMap::new();
        for primitive in primitives {
            let attributes = item(primitive, "attributes")?;
            let Some(positions) = attributes.get("POSITION") else {
                continue;
            };
            let positions = accessor(gltf, positions)?;
            let mut targets = Vec::with_capacity(count);
            for k in 0..count {
                let target = primitive.get("targets").and_then(Value::as_array).and_then(|list| list.get(k));
                targets.push(match target.and_then(|target| target.get("POSITION")) {
                    Some(at) => Some(accessor(gltf, at)?),
                    None => None,
                });
            }
            let skin = match &reskin {
                Some(_) => match (attributes.get("JOINTS_0"), attributes.get("WEIGHTS_0")) {
                    (Some(j), Some(w)) => Some((accessor(gltf, j)?, accessor(gltf, w)?)),
                    _ => None,
                },
                None => None,
            };
            for j in 0..positions.len() {
                let point: Vec<f64> = positions.at(j as i64)?.iter().take(3).map(|v| v.f()).collect();
                if point.len() < 3 {
                    bail!("a vertex position has fewer than 3 numbers");
                }
                let m = match (&reskin, &skin) {
                    (Some(matrices), Some((joints, weights))) => {
                        mat_mul(&to_local, &blend(matrices, joints.at(j as i64)?, weights.at(j as i64)?))
                    }
                    _ => to_local,
                };
                let base = xform_point(&m, &point);
                let found = near(&base);
                if found.is_empty() {
                    continue;
                }
                let mut moved = Vec::with_capacity(count);
                for target in &targets {
                    let delta: Vec<f64> = match target {
                        Some(values) => values.at(j as i64)?.iter().take(3).map(|v| v.f()).collect(),
                        None => vec![0.0; 3],
                    };
                    let shifted: Vec<f64> = (0..3).map(|i| point[i] + delta.get(i).copied().unwrap_or(0.0)).collect();
                    moved.push(xform_point(&m, &shifted));
                }
                for v in found {
                    vertices.entry(v).or_insert_with(|| (base, moved.clone()));
                }
            }
        }
        out.insert(frame, MorphMesh { node: gi, names, vertices });
    }
    Ok(out)
}

/// The picture a glTF material shows, as material_from_gltf names it from its base colour image
/// (the image's name, else its file's stem), or "" for none.
fn picture_of(json: &Value, material: &Value) -> Result<String> {
    let Some(base) = material.get("pbrMetallicRoughness").and_then(|pbr| pbr.get("baseColorTexture")) else {
        return Ok(String::new());
    };
    let texture = index(item(json, "textures")?, item(base, "index")?)?;
    let Some(source) = texture.get("source") else {
        return Ok(String::new());
    };
    let image = index(item(json, "images")?, source)?;
    if let Some(name) = image.get("name").and_then(Value::as_str).filter(|name| !name.is_empty()) {
        return Ok(name.to_string());
    }
    let uri = image.get("uri").and_then(Value::as_str).unwrap_or("");
    Ok(crate::to_gltf::strip_extension(uri.rsplit(['/', '\\']).next().unwrap_or("")).to_string())
}

/// The character meshes whose geometry (frame_geometry: skins included) differs from the
/// export's, or that it does not have.
pub fn changed_character_meshes(exported: &Gltf, given: &Gltf, model: &Mds) -> Result<BTreeSet<MeshId>> {
    let before = frame_geometry(exported, model)?;
    // the given skinned meshes brought into the export's bind pose: Blender rests bones without
    // scale or shear and moves the vertices to match
    let after = geometry(given, model, true, Some(&bind_matrices(exported, model)?))?;
    Ok(after
        .into_iter()
        .filter(|(id, geometry)| !before.get(id).is_some_and(|old| same_geometry(old, geometry)))
        .map(|(id, _)| id)
        .collect())
}

/// Whether two corners' skins (skin_tail) are the same within what a tool that renormalises
/// weights moves them: every frame's weight within 0.01, a frame one side lacks weighing 0 there.
fn same_skin(a: &[f64], b: &[f64]) -> bool {
    let weights = |tail: &[f64]| -> BTreeMap<i64, f64> {
        tail.as_chunks::<2>().0.iter().map(|&[frame, weight]| (frame as i64, weight)).collect()
    };
    let (a, b) = (weights(a), weights(b));
    let weight = |w: &BTreeMap<i64, f64>, frame: &i64| w.get(frame).copied().unwrap_or(0.0);
    a.keys().chain(b.keys()).all(|frame| (weight(&a, frame) - weight(&b, frame)).abs() <= SKIN_TOLERANCE)
}

/// How far a weight may move and still be the same (same_skin).
const SKIN_TOLERANCE: f64 = 0.01;

/// Whether two meshes are the same within what another tool's round trip moves: matrices within
/// 1e-3 (of the larger of 1 and the value; translations within 0.005 and 2e-6 of the value), and
/// every triangle of one matched by a triangle of the other in any corner order (winding kept):
/// positions within 0.005 (and 2e-6 of the value), UVs within 0.001, colours within 4/255, skin
/// weights within 0.01 (same_skin).
fn same_geometry(a: &Geometry, b: &Geometry) -> bool {
    let close = |x: f64, y: f64, tolerance: f64| (x - y).abs() <= tolerance * x.abs().max(y.abs()).max(1.0);
    // positions are compared absolutely (a few thousandths, and 32-bit precision far out), so a
    // move far from the origin is still a move
    let near = |x: f64, y: f64| (x - y).abs() <= 5e-3 + 2e-6 * x.abs().max(y.abs());
    // column-major: the translation is 12-14, where Blender's 32-bit world matrices leave a few
    // thousandths on meshes far out
    let matrix_close = |at: usize, x: f64, y: f64| if (12..15).contains(&at) { near(x, y) } else { close(x, y, 1e-3) };
    if a.matrix.len() != b.matrix.len()
        || !a.matrix.iter().zip(&b.matrix).enumerate().all(|(at, (x, y))| matrix_close(at, *x, *y))
    {
        return false;
    }
    if a.groups.keys().ne(b.groups.keys()) {
        return false;
    }
    // What Blender adds where a primitive had nothing is no edit: UVs of 0 and white colours (it
    // gives every primitive of a mesh what any of them has), and colours without alpha.
    let filler =
        |values: &[f64], fill: f64| values.iter().all(|value| value.is_nan() || (value - fill).abs() <= 4.0 / 255.0);
    let corner_close = |x: &[f64], y: &[f64]| -> bool {
        let field = |range: std::ops::Range<usize>, fill: f64, tolerance: f64| -> bool {
            let (p, q) = (&x[range.clone()], &y[range]);
            match (p.iter().all(|v| v.is_nan()), q.iter().all(|v| v.is_nan())) {
                (true, true) => true,
                (true, false) => filler(q, fill),
                (false, true) => filler(p, fill),
                // a value only one side has (alpha) compares as missing
                (false, false) => p.iter().zip(q).all(|(p, q)| p.is_nan() || q.is_nan() || close(*p, *q, tolerance)),
            }
        };
        x.len() >= CORNER
            && y.len() >= CORNER
            && (0..3).all(|at| near(x[at], y[at]))
            && field(UV..COLOUR, 0.0, 0.001)
            && field(COLOUR..CORNER, 1.0, 4.0 / 255.0)
            && same_skin(&x[CORNER..], &y[CORNER..])
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

/// A mesh's place in the model: what takes the glTF's positions into its frame's space, and for a
/// skinned mesh the frame each of its joints stands for and the matrices that bring its positions
/// into the bind pose kept (Blender's are in its own).
struct MeshSpace {
    to_local: Mat4,
    joint_map: Vec<Option<usize>>,
    reskin: Option<Vec<Mat4>>,
}

/// A mesh's materials as its primitives name them: each glTF material once, in the order first
/// named.
#[derive(Default)]
struct MeshMaterials {
    materials: Vec<Material>,
    material_of: Vec<(Value, usize)>,
}

impl MeshMaterials {
    /// The number of the MDT material for a primitive's glTF material (`number`, or Null).
    fn number(&mut self, json: &Value, number: &Value, collision: bool) -> Result<usize> {
        if let Some((_, at)) = self.material_of.iter().find(|(known, _)| known == number) {
            return Ok(*at);
        }
        self.materials.push(if collision {
            collision_material(json, number)?
        } else {
            material_from_gltf(json, number)?
        });
        self.material_of.push((number.clone(), self.materials.len() - 1));
        Ok(self.materials.len() - 1)
    }
}

/// A model being rebuilt from a glTF (import_gltf): the template's frames, their bind pose, and
/// what the glTF's nodes stand for among them.
struct Import<'g> {
    gltf: &'g Gltf,
    options: &'g ImportOptions<'g>,
    model: Mds,
    resolved: Vec<Option<usize>>,
    /// The glTF's nodes, animated ones at their frames' rests (as frame_geometry compares them).
    nodes: &'g Value,
    node_list: &'g [Value],
    parent: HashMap<i64, usize>,
    /// The glTF's world matrices, as world_of works them out.
    world: HashMap<i64, TypedMatrix>,
    /// The template's frames' world matrices.
    template_world: Vec<Mat4>,
    bbp: Vec<Mat4>,
    /// Frames added for glTF mesh nodes no template frame stands for, by name.
    by_name: HashMap<String, usize>,
}

impl Import<'_> {
    /// The model frame glTF node `gi` stands for: the one frames_of gives, or one added for it.
    fn mds_index_of(&self, gi: &Value) -> Result<Option<usize>> {
        let n = index(self.nodes, gi)?;
        let at = gi.as_i64().unwrap_or(-1);
        let at = if at < 0 { self.node_list.len() as i64 + at } else { at };
        match self.resolved.get(at as usize).copied().flatten() {
            Some(frame) => Ok(Some(frame)),
            None => Ok(self.by_name.get(&clean(n.get("name"))?).copied()),
        }
    }

    /// The bind matrices of the skins' joints from their inverse bind matrices (so edits to the
    /// bind pose are kept): the template's stays unless the glTF's differs from what to_gltf writes
    /// for it (its inverse, or the frame's rest matrix's when it has none) and from what Blender
    /// gives back for that (its bones' rests have no scale or shear).
    fn read_binds(&mut self, exported_binds: &Binds, blender_binds: &Binds) -> Result<()> {
        let skins = self.gltf.json.get("skins").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
        for skin in skins {
            let Some(ibm_index) = skin.get("inverseBindMatrices") else {
                continue;
            };
            let ibm = accessor(self.gltf, ibm_index)?;
            let Some(joints) = item(skin, "joints")?.as_array() else {
                bail!("a skin's joints are not a list");
            };
            for (k, gi) in joints.iter().enumerate() {
                let mi = self.mds_index_of(gi)?;
                let matrix = ibm.at(k as i64)?;
                if matrix.len() < 16 {
                    bail!("an inverse bind matrix has fewer than 16 numbers");
                }
                let values: Vec<f64> = matrix.iter().map(|v| v.f()).collect();
                let inverse = mat_inv(&values[..16].try_into().unwrap());
                let (Some(mi), Some(inverse)) = (mi, inverse) else {
                    continue;
                };
                let known = [exported_binds.get(mi), blender_binds.get(mi)];
                if !known.into_iter().flatten().flatten().any(|e| close_bind(&inverse, e)) {
                    self.bbp[mi] = inverse;
                }
            }
        }
        Ok(())
    }

    /// The frame mesh node `gi` (named `name`) replaces the mesh of: the one it stands for, else a
    /// new frame under the root (under its parent's frame, for a rigid mesh).
    fn target_frame(&mut self, gi: usize, n: &Value, name: &str) -> Result<usize> {
        if let Some(target) = self.resolved[gi].or_else(|| self.by_name.get(name).copied()) {
            return Ok(target);
        }
        let mut node = Node::new(name, if self.model.nodes.is_empty() { -1 } else { 0 });
        if let Some(&pj) = self.parent.get(&(gi as i64)) {
            if let Some(p) = self.mds_index_of(&Value::from(pj as i64))? {
                if !n.get("skin").is_some() {
                    node.parent = p as i32;
                }
            }
        }
        self.model.nodes.push(node);
        let target = self.model.nodes.len() - 1;
        self.by_name.insert(name.to_string(), target);
        self.bbp.push(IDENT);
        Ok(target)
    }

    /// Where mesh node `gi`'s positions go in frame `target`: a skinned mesh's are in model space
    /// (the node's transform ignored), into the frame's bind pose; a rigid mesh's into the frame's
    /// own space through the glTF hierarchy.
    fn mesh_space(
        &mut self,
        gi: usize,
        n: &Value,
        name: &str,
        target: usize,
        skin: Option<&Value>,
    ) -> Result<MeshSpace> {
        if let Some(skin) = skin {
            let mut reskin = None;
            if let Some(number) = n.get("skin") {
                let template_world = self.model.world_matrices()?;
                let binds: HashMap<usize, Mat4> = (0..self.model.nodes.len())
                    .filter_map(|f| {
                        let bind = self.bbp.get(f).filter(|m| mat_inv(m).is_some()).or_else(|| template_world.get(f));
                        bind.map(|m| (f, *m))
                    })
                    .collect();
                reskin = Some(skin_matrices(self.gltf, number, &self.resolved, &binds)?);
            }
            if mat_inv(&self.bbp[target]).is_none() {
                self.bbp[target] = self.model.world_matrices()?[target];
            }
            let to_local = mat_inv(&self.bbp[target]).unwrap_or(IDENT);
            let Some(joints) = item(skin, "joints")?.as_array() else {
                bail!("a skin's joints are not a list");
            };
            let joint_map = joints.iter().map(|g| self.mds_index_of(g)).collect::<Result<_>>()?;
            return Ok(MeshSpace { to_local, joint_map, reskin });
        }
        let mut frame_world = None;
        if self.resolved[gi].is_none() {
            for (g2, n2) in self.node_list.iter().enumerate() {
                if !n2.get("mesh").is_some() && self.resolved[g2].is_none() && clean(n2.get("name"))? == name {
                    frame_world = Some(world_of(g2 as i64, self.nodes, &self.parent, &mut self.world, 0)?);
                }
            }
        }
        let mesh_world = floats(&world_of(gi as i64, self.nodes, &self.parent, &mut self.world, 0)?);
        let found =
            rigid_to_local(gi, &self.resolved, self.nodes, &self.parent, &mut self.world, &self.template_world)?;
        let to_local = match (found, frame_world) {
            (Some(to_local), _) => to_local,
            (None, Some(fw)) => mat_mul(&mat_inv(&floats(&fw)).unwrap_or(IDENT), &mesh_world),
            // a mesh no frame stands for (one made in Blender): into the space of the frame it was
            // added under, keeping where the object was put
            (None, None) => {
                let under = self.model.nodes[target].parent;
                let frame = if under >= 0 { self.bbp[under as usize] } else { IDENT };
                mat_mul(&mat_inv(&frame).unwrap_or(IDENT), &mesh_world)
            }
        };
        Ok(MeshSpace { to_local, joint_map: Vec::new(), reskin: None })
    }

    /// The corners of a triangle primitive in its frame's space (None for one with none): each
    /// corner's position, normal, uv, colour (an alpha Blender left out taken from the nearest
    /// vertex of the mesh it replaces) and skin.
    fn corners_of_primitive(
        &self,
        p: &Value,
        mesh: (&str, &MeshSpace, bool),
        alphas: &Alphas,
        materials: &mut MeshMaterials,
    ) -> Result<Option<Primitive>> {
        let (name, space, skinned) = mesh;
        let (gltf, collision) = (self.gltf, self.options.collision);
        if p.get("mode").map_or(4.0, |m| m.as_f64().unwrap_or(f64::NAN)) != 4.0 {
            bail!("only triangle primitives are supported");
        }
        let attributes = item(p, "attributes")?;
        let pos = accessor(gltf, item(attributes, "POSITION")?)?;
        let optional = |name: &str| attributes.get(name).map(|i| accessor(gltf, i)).transpose();
        // collision keeps positions alone
        let shaded = |name: &str| if collision { Ok(None) } else { optional(name) };
        let nrm = shaded("NORMAL")?;
        let uv = shaded("TEXCOORD_0")?;
        let col = shaded("COLOR_0")?;
        let (jts, wts) = if skinned { (optional("JOINTS_0")?, optional("WEIGHTS_0")?) } else { (None, None) };
        let indices: Vec<Num> = match p.get("indices") {
            Some(i) => {
                let values = accessor(gltf, i)?;
                if values.n != 1 {
                    bail!("a primitive's indices have {} components, not 1", values.n);
                }
                values.values
            }
            None => (0..pos.len() as i64).map(Num::Int).collect(),
        };
        let material = materials.number(&gltf.json, p.get("material").unwrap_or(&Value::Null), collision)?;
        let to_local = &space.to_local;
        let mut corners = Vec::with_capacity(indices.len());
        for c in &indices {
            let Num::Int(c) = *c else {
                bail!("a primitive's indices are floats");
            };
            let point = pos.at(c)?;
            if point.len() < 3 {
                bail!("a vertex position has fewer than 3 numbers");
            }
            let point: Vec<f64> = point.iter().map(|v| v.f()).collect();
            let skinning = match (&space.reskin, jts.as_ref(), wts.as_ref()) {
                (Some(matrices), Some(jts), Some(wts)) if !jts.is_empty() && !wts.is_empty() => {
                    blend(matrices, jts.at(c)?, wts.at(c)?)
                }
                _ => IDENT,
            };
            let position = xform_point(to_local, &xform_point(&skinning, &point));
            let normal = match nrm.as_ref().filter(|v| !v.is_empty()) {
                Some(nrm) => {
                    let v: Vec<f64> = nrm.at(c)?.iter().map(|v| v.f()).collect();
                    if v.len() < 3 {
                        bail!("a normal has fewer than 3 numbers");
                    }
                    Some(normalize(&xform_dir(to_local, &xform_dir(&skinning, &v)), Z).to_vec())
                }
                None => None,
            };
            let uv = match uv.as_ref().filter(|v| !v.is_empty()) {
                Some(uv) => {
                    let v = uv.at(c)?;
                    if v.len() < 2 {
                        bail!("a texture coordinate has fewer than 2 numbers");
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
                        bail!("a colour has fewer than 4 numbers");
                    }
                    Some(v)
                }
                None => None,
            };
            let skin = match jts.as_ref().filter(|v| !v.is_empty()) {
                Some(jts) => {
                    let Some(wts) = wts.as_ref() else {
                        bail!("mesh {name}: a primitive has JOINTS_0 but no WEIGHTS_0");
                    };
                    Some(corner_skin(jts.at(c)?, wts.at(c)?, &space.joint_map)?)
                }
                None => None,
            };
            corners.push(Corner { position, normal, uv, colour, skin });
        }
        Ok((!corners.is_empty()).then_some(Primitive { material, corners }))
    }
}

/// A corner's skin as the MDT keeps it: up to 4 (frame, weight) pairs, the largest weights first,
/// as percentages of their sum rounded to 4 places; joints no frame stands for left out.
fn corner_skin(joints: &[Num], weights: &[Num], joint_map: &[Option<usize>]) -> Result<Vec<(i64, KeyFloat)>> {
    let mut pairs: Vec<(Num, i64)> = Vec::new();
    for (joint, w) in joints.iter().zip(weights) {
        let Num::Int(joint) = *joint else {
            bail!("a primitive's joints are floats");
        };
        if w.f() > 0.0 {
            if let Some(bone) = *crate::to_gltf::at_index(joint_map, joint)? {
                pairs.push((*w, bone as i64));
            }
        }
    }
    pairs.sort_by(|a, b| b.0.f().partial_cmp(&a.0.f()).unwrap().then(b.1.cmp(&a.1)));
    pairs.truncate(4);
    let total = numeric::sum_nums(pairs.iter().map(|(w, _)| *w));
    let total = if total.f() == 0.0 { 1.0 } else { total.f() };
    Ok(pairs.iter().map(|(w, b)| (*b, KeyFloat(numeric::round_decimal(w.f() / total * 100.0, 4)))).collect())
}

/// A skinned mesh's weight lists (.wgt), one per bone, from its vertices' skins.
fn skin_weights(target: usize, skins: &[Skin], name: &str) -> Result<Vec<KeyList>> {
    let mut per_bone: BTreeMap<i64, Vec<(i64, [f64; 4])>> = BTreeMap::new();
    for (vi, skin) in skins.iter().enumerate() {
        for (b, w) in skin.iter().flatten() {
            per_bone.entry(*b).or_default().push((vi as i64, [w.0, 0.0, 0.0, 0.0]));
        }
    }
    let mut out = Vec::with_capacity(per_bone.len());
    for (b, keys) in per_bone {
        let Ok(b) = i32::try_from(b) else {
            bail!("mesh {name}: its skin names frame {b}, past what a weight list can name");
        };
        out.push(KeyList::new(target as i32, b, 20, keys));
    }
    Ok(out)
}

/// The weight lists: a rebuilt mesh's take the place of its old ones (blocks with keys alone: the
/// empty blocks the disc's files hold for every bone are skipped by CreateVertexWeight), new
/// meshes' go at the end.
fn merged_weights(old: Vec<KeyList>, replaced: &BTreeSet<i32>, new: Vec<KeyList>) -> Vec<KeyList> {
    let mut wgt: Vec<KeyList> = Vec::with_capacity(old.len());
    for list in old {
        if !replaced.contains(&list.frame) {
            wgt.push(list);
        } else if !wgt.iter().any(|known| known.frame == list.frame) {
            wgt.extend(new.iter().filter(|added| added.frame == list.frame).cloned());
        }
    }
    let placed: BTreeSet<i32> = wgt.iter().map(|list| list.frame).collect();
    wgt.extend(new.into_iter().filter(|added| !placed.contains(&added.frame)));
    wgt
}

/// The model rebuilt from a glTF, the template's meshes replaced by the glTF's: each glTF mesh
/// node replaces the mesh of the frame it stands for (`frames_of`), only those in
/// `options.meshes` when given, the template's bind pose and weights (`options.bbp`, `.wgt`)
/// kept but for what the glTF changes.
pub fn import_gltf(gltf: &Gltf, template: &[u8], options: &ImportOptions) -> Result<Imported> {
    let j = &gltf.json;
    let model: Mds = mds::parse(template)?;
    let resolved = frames_of(j, &model)?;
    let mut bbp: Vec<Mat4> = match options.bbp {
        Some(data) => mot::parse_bbp(data)?,
        None => model.world_matrices()?,
    };
    let (exported_binds, blender_binds) = exported_binds(&model, &bbp)?;
    if bbp.len() < model.nodes.len() {
        bbp.resize(model.nodes.len(), IDENT);
    }
    let old_wgt = match options.wgt {
        Some(data) if !data.is_empty() => mot::parse(data)?,
        _ => Vec::new(),
    };
    let nodes = animated_nodes_at_rest(j, &model, &resolved)?;
    let Some(node_list) = nodes.as_array() else {
        bail!("the glTF's nodes are not a list");
    };
    let template_world = model.world_matrices()?;
    let mut import = Import {
        gltf,
        options,
        model,
        resolved,
        nodes: &nodes,
        node_list,
        parent: parents_of(node_list),
        world: HashMap::new(),
        template_world,
        bbp,
        by_name: HashMap::new(),
    };
    import.read_binds(&exported_binds, &blender_binds)?;

    let mut replaced = BTreeSet::new();
    let mut new_wgt = Vec::new();
    let mesh_list = j.get("meshes");
    for (gi, n) in node_list.iter().enumerate() {
        let Some(mesh_index) = n.get("mesh") else {
            continue;
        };
        let name = frame_name(j, n)?;
        let id = match import.resolved[gi] {
            Some(frame) => MeshId::Frame(frame),
            None => MeshId::New(name.clone()),
        };
        if options.meshes.is_some_and(|wanted| !wanted.contains(&id)) {
            continue;
        }
        let target = import.target_frame(gi, n, &name)?;
        let skin = match n.get("skin") {
            Some(_) if options.collision => bail!("collision meshes are rigid; mesh {name} has a skin"),
            Some(s) => Some(index(item(j, "skins")?, s)?).filter(|s| is_set(s)),
            None => None,
        };
        let space = import.mesh_space(gi, n, &name, target, skin)?;
        // Blender writes some meshes' colours without alpha: the fades come from the mesh it
        // replaces, each corner taking the alpha of the nearest vertex there
        let alphas = original_alphas(import.model.nodes[target].mdt.as_ref());
        let mut materials = MeshMaterials::default();
        let mut prims = Vec::new();
        let mesh = index(mesh_list.ok_or_else(|| missing("meshes"))?, mesh_index)?;
        let Some(primitives) = item(mesh, "primitives")?.as_array() else {
            bail!("a mesh's primitives are not a list");
        };
        for p in primitives {
            let mesh = (name.as_str(), &space, skin.is_some());
            prims.extend(import.corners_of_primitive(p, mesh, &alphas, &mut materials)?);
        }
        let (t, skins) = build_mdt(&prims, materials.materials);
        import.model.nodes[target].mdt = Some(t);
        replaced.insert(target as i32);
        if skin.is_some() {
            new_wgt.extend(skin_weights(target, &skins, &name)?);
        }
    }
    let wgt = merged_weights(old_wgt, &replaced, new_wgt);
    Ok(Imported { mds: mds::build(&import.model)?, wgt: mot::build(&wgt)?, bbp: mot::build_bbp(&import.bbp)? })
}

/// The name import_gltf gives the frame of glTF node `n`: its name, or for a mesh node without one
/// its mesh's, Blender's ".001" and a skinned mesh's "_skin" dropped.
fn frame_name(j: &Value, n: &Value) -> Result<String> {
    let mut name = match (n.get("name").filter(|v| is_set(v)), n.get("mesh")) {
        (Some(name), _) => clean(Some(name))?,
        (None, Some(mesh)) => clean(index(item(j, "meshes")?, mesh)?.get("name"))?,
        (None, None) => String::new(),
    };
    if n.get("mesh").is_some() && n.get("skin").is_some() {
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
/// texture gets none. A skinned mesh is a frame of its node's name with its vertices in that
/// frame's bind space, weighted to the frames of its joints (import_gltf). The glTF's nodes are
/// renamed to their frames' names, so what reads it next (motions::new_motion) finds them.
pub fn new_model(gltf: &mut Gltf, version: u32, root: &str) -> Result<NewModel> {
    let empty = Vec::new();
    let node_count = gltf.json.get("nodes").and_then(Value::as_array).map_or(0, Vec::len);
    // every node named, distinctly, as import_gltf will read the names
    let mut names: Vec<String> = Vec::with_capacity(node_count);
    for i in 0..node_count {
        let n = index(item(&gltf.json, "nodes")?, &Value::from(i as i64))?;
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
    if let Some(Value::Array(nodes)) = gltf.json.get_mut("nodes") {
        for (node, name) in nodes.iter_mut().zip(&names) {
            node["name"] = Value::String(name.clone());
            // the frames are the glTF's own, found by these names: an index into another model
            // (left by an export) names none of them
            if let Some(Value::Object(extras)) = node.get_mut("extras") {
                extras.retain(|key, _| key != "mds_index");
            }
        }
    }
    // a material without a texture names none (import_gltf would take the material's name)
    if let Some(Value::Array(materials)) = gltf.json.get_mut("materials") {
        for material in materials {
            let textured = material.get("extras").is_some_and(|extras| extras.get("texture").is_some())
                || material.get("pbrMetallicRoughness").is_some_and(|pbr| pbr.get("baseColorTexture").is_some());
            if !textured {
                match material.get_mut("extras") {
                    Some(extras @ Value::Object(_)) => extras["texture"] = Value::String(String::new()),
                    _ => material["extras"] = json!({"texture": ""}),
                }
            }
        }
    }
    let j = &gltf.json;
    let node_list = item(j, "nodes")?.as_array().unwrap_or(&empty);
    let mut parent: HashMap<usize, usize> = HashMap::new();
    for (i, n) in node_list.iter().enumerate() {
        for c in n.get("children").and_then(Value::as_array).unwrap_or(&empty) {
            match c.as_i64() {
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
        let children = n.get("children").and_then(Value::as_array).unwrap_or(&empty);
        for c in children.iter().rev() {
            pending.push((c.as_i64().unwrap_or(0) as usize, at));
        }
    }
    if placed.iter().any(|placed| !placed) {
        bail!("the glTF's node hierarchy has a cycle");
    }
    if !node_list.iter().any(|n| n.get("mesh").is_some()) {
        bail!("the glTF has no mesh");
    }
    let skinned: Vec<String> = node_list
        .iter()
        .zip(&names)
        .filter(|(n, _)| n.get("mesh").is_some() && n.get("skin").is_some())
        .map(|(_, name)| name.clone())
        .collect();
    let skeleton = mds::build(&Mds { version, nodes: frames })?;
    let built = import_gltf(gltf, &skeleton, &ImportOptions::default())?;
    Ok(NewModel { mds: built.mds, wgt: built.wgt, bbp: built.bbp, skinned })
}

/// A model new_model makes: its MDS, the .wgt and .bbp of its skin (the bind pose of every frame,
/// from the inverse bind matrices where the glTF gives them), and the frames of its skinned meshes,
/// which info.cfg's VERTEX_ANIME has to name.
pub struct NewModel {
    pub mds: Vec<u8>,
    pub wgt: Vec<u8>,
    pub bbp: Vec<u8>,
    pub skinned: Vec<String>,
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
        bail!("image {} has neither a uri nor a buffer view", i.as_i64().unwrap_or(-1));
    };
    let view = index(item(&gltf.json, "bufferViews")?, view)?;
    let buffer = view.get("buffer").and_then(Value::as_i64).unwrap_or(0);
    let Some(buffer) = usize::try_from(buffer).ok().and_then(|b| gltf.buffers.get(b)) else {
        bail!("the glTF has no buffer {buffer}");
    };
    let start = view.get("byteOffset").and_then(Value::as_i64).unwrap_or(0).max(0) as usize;
    let length = view.get("byteLength").and_then(Value::as_i64).unwrap_or(0).max(0) as usize;
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
    let count = j.get("materials").and_then(Value::as_array).map_or(0, Vec::len);
    for m in 0..count {
        let number = Value::from(m as i64);
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
            Some((_, other)) if other.as_i64() != source.as_i64() => {
                bail!("two different images are named {name}; give each texture its own name")
            }
            Some(_) => {}
            None => out.push((name, source.clone())),
        }
    }
    Ok(out)
}

/// The glTF's nodes with every node an animation moves (a translation, rotation or scale channel)
/// put at the rest of the frame it stands for. Blender writes an animated object's transform as
/// its action leaves it at the scene's frame, not its rest, so a rigid model's meshes would seem
/// moved; an animated node's own transform is its animation's, not something an edit sets.
fn animated_nodes_at_rest(json: &Value, model: &Mds, resolved: &[Option<usize>]) -> Result<Value> {
    let mut nodes = item(json, "nodes")?.clone();
    let mut animated = BTreeSet::new();
    for animation in json.get("animations").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default() {
        for channel in animation.get("channels").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default() {
            let target = channel.get("target");
            let path = target.and_then(|t| t.get("path")).and_then(Value::as_str);
            if matches!(path, Some("translation" | "rotation" | "scale")) {
                if let Some(node) = target.and_then(|t| t.get("node")).and_then(Value::as_i64) {
                    animated.insert(node);
                }
            }
        }
    }
    if let Value::Array(list) = &mut nodes {
        for (gi, node) in list.iter_mut().enumerate() {
            let frame = resolved.get(gi).copied().flatten();
            let (Some(frame), true) = (frame, animated.contains(&(gi as i64))) else {
                continue;
            };
            let (t, r, s) = decompose(&model.nodes[frame].matrix)?;
            if let Value::Object(items) = node {
                items.retain(|key, _| !matches!(key.as_str(), "matrix" | "translation" | "rotation" | "scale"));
            }
            node["translation"] = Value::from(t.to_vec());
            node["rotation"] = Value::from(r.to_vec());
            node["scale"] = Value::from(s.to_vec());
        }
    }
    Ok(nodes)
}

/// Bind matrices by frame, None where there is none.
type Binds = Vec<Option<Mat4>>;

/// The bind matrices to_gltf writes the inverses of for a model's frames (each frame's bind
/// matrix, or its rest where it has none), and the same as Blender gives them back: it makes every
/// joint a bone, whose rest has no scale or shear, from each joint's bind matrix relative to its
/// parent's with the scale dropped, chained down the skeleton.
fn exported_binds(model: &Mds, bbp: &[Mat4]) -> Result<(Binds, Binds)> {
    let world = model.world_matrices()?;
    let exported: Vec<Option<Mat4>> = (0..model.nodes.len())
        .map(|i| match bbp.get(i).filter(|m| mat_inv(m).is_some()) {
            Some(m) => Some(*m),
            None => world.get(i).copied().filter(|m| mat_inv(m).is_some()),
        })
        .collect();
    let mut rigid: Vec<Option<Mat4>> = vec![None; model.nodes.len()];
    for (i, node) in model.nodes.iter().enumerate() {
        let Some(bind) = exported[i] else {
            continue;
        };
        let parent = usize::try_from(node.parent).ok().filter(|&p| p < i);
        let (parent_inverse, parent_rigid) = match parent {
            Some(p) => match (exported[p].as_ref().and_then(mat_inv), rigid[p]) {
                (Some(a), Some(b)) => (a, b),
                _ => continue,
            },
            None => (IDENT, IDENT),
        };
        let local = mat_mul(&parent_inverse, &bind);
        let rigid_local = compose(&[local[12], local[13], local[14]], &blender_rotation(&local), &[1.0, 1.0, 1.0]);
        rigid[i] = Some(mat_mul(&parent_rigid, &rigid_local));
    }
    Ok((exported, rigid))
}

/// The rotation (x, y, z, w) Blender's Mat4.decompose takes from a glTF matrix (the importer's
/// bind pose of a bone): in Blender's axes (glTF's x, -z, y), each column normalised (all negated
/// when they turn the wrong way), then Mike Day's method (mat3_normalized_to_quat_fast), which for
/// a matrix with shear differs from other ways of reading a rotation.
fn blender_rotation(m: &Mat4) -> Quat {
    let g = |row: usize, col: usize| m[col * 4 + row];
    // Blender's rows: (x, -z, y) of glTF's rows and columns
    let b = [[g(0, 0), -g(0, 2), g(0, 1)], [-g(2, 0), g(2, 2), -g(2, 1)], [g(1, 0), -g(1, 2), g(1, 1)]];
    // mat[column][row], columns normalised
    let mut mat = [[0.0f64; 3]; 3];
    for (col, column) in mat.iter_mut().enumerate() {
        let length = (0..3).map(|row| b[row][col] * b[row][col]).sum::<f64>().sqrt();
        for (row, value) in column.iter_mut().enumerate() {
            *value = if length > 0.0 { b[row][col] / length } else { 0.0 };
        }
    }
    let det = mat[0][0] * (mat[1][1] * mat[2][2] - mat[2][1] * mat[1][2])
        - mat[1][0] * (mat[0][1] * mat[2][2] - mat[2][1] * mat[0][2])
        + mat[2][0] * (mat[0][1] * mat[1][2] - mat[1][1] * mat[0][2]);
    if det < 0.0 {
        mat = mat.map(|column| column.map(|v| -v));
    }
    // (w, x, y, z)
    let mut q = [0.0f64; 4];
    if mat[2][2] < 0.0 {
        if mat[0][0] > mat[1][1] {
            let trace = 1.0 + mat[0][0] - mat[1][1] - mat[2][2];
            let mut s = 2.0 * trace.sqrt();
            if mat[1][2] < mat[2][1] {
                s = -s;
            }
            q[1] = 0.25 * s;
            q[0] = (mat[1][2] - mat[2][1]) / s;
            q[2] = (mat[0][1] + mat[1][0]) / s;
            q[3] = (mat[2][0] + mat[0][2]) / s;
        } else {
            let trace = 1.0 - mat[0][0] + mat[1][1] - mat[2][2];
            let mut s = 2.0 * trace.sqrt();
            if mat[2][0] < mat[0][2] {
                s = -s;
            }
            q[2] = 0.25 * s;
            q[0] = (mat[2][0] - mat[0][2]) / s;
            q[1] = (mat[0][1] + mat[1][0]) / s;
            q[3] = (mat[1][2] + mat[2][1]) / s;
        }
    } else if mat[0][0] < -mat[1][1] {
        let trace = 1.0 - mat[0][0] - mat[1][1] + mat[2][2];
        let mut s = 2.0 * trace.sqrt();
        if mat[0][1] < mat[1][0] {
            s = -s;
        }
        q[3] = 0.25 * s;
        q[0] = (mat[0][1] - mat[1][0]) / s;
        q[1] = (mat[2][0] + mat[0][2]) / s;
        q[2] = (mat[1][2] + mat[2][1]) / s;
    } else {
        let trace = 1.0 + mat[0][0] + mat[1][1] + mat[2][2];
        let s = 2.0 * trace.sqrt();
        q[0] = 0.25 * s;
        q[1] = (mat[1][2] - mat[2][1]) / s;
        q[2] = (mat[2][0] - mat[0][2]) / s;
        q[3] = (mat[0][1] - mat[1][0]) / s;
    }
    let length = q.iter().map(|v| v * v).sum::<f64>().sqrt();
    let q = if length > 0.0 && length.is_finite() { q.map(|v| v / length) } else { [1.0, 0.0, 0.0, 0.0] };
    // back to glTF's axes: x = x, y = Blender's z, z = -Blender's y
    [q[1], q[3], -q[2], q[0]]
}

/// The glTF world matrix of node i, memoised.
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
        bail!("the glTF's node {i} is more than 900 parents deep (its hierarchy has a cycle?)");
    }
    let m = node_matrix(index(nodes, &Value::from(i))?)?;
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

    fn corner(position: [f64; 3], uv: Option<[f64; 2]>, colour: Option<[f64; 4]>) -> Vec<f64> {
        let mut out = vec![f64::NAN; CORNER];
        out[..3].copy_from_slice(&position);
        if let Some(uv) = uv {
            out[UV..COLOUR].copy_from_slice(&uv);
        }
        if let Some(colour) = colour {
            out[COLOUR..].copy_from_slice(&colour);
        }
        out
    }

    fn one_triangle(offset: f64, uv: Option<[f64; 2]>, colour: Option<[f64; 4]>) -> Geometry {
        let tri = [
            corner([1549.0 + offset, 488.0, -2122.0], uv, colour),
            corner([1548.0 + offset, 480.0, -2121.0], uv, colour),
            corner([1549.0 + offset, 480.0, -2118.0], uv, colour),
        ];
        let mut matrix = vec![0.0; 16];
        for at in [0, 5, 10, 15] {
            matrix[at] = 1.0;
        }
        Geometry { matrix, groups: BTreeMap::from([("m".to_string(), vec![tri])]) }
    }

    #[test]
    fn what_blender_adds_is_no_edit_but_a_move_far_out_is() {
        let original = one_triangle(0.0, None, None);
        assert!(same_geometry(&original, &one_triangle(0.002, None, None)), "float noise");
        assert!(
            same_geometry(&original, &one_triangle(0.0, Some([0.0, 0.0]), Some([1.0, 1.0, 1.0, f64::NAN]))),
            "zero UVs and white"
        );
        assert!(!same_geometry(&original, &one_triangle(0.5, None, None)), "half a unit far out");
        assert!(!same_geometry(&original, &one_triangle(0.0, None, Some([0.5, 0.5, 0.5, 1.0]))), "a painted colour");
        let faded = one_triangle(0.0, None, Some([0.7, 0.7, 0.7, 0.0]));
        assert!(same_geometry(&faded, &one_triangle(0.0, None, Some([0.7, 0.7, 0.7, f64::NAN]))), "alpha dropped");
    }

    #[test]
    fn skins_compare_by_frame_within_a_renormalisation() {
        let int = |values: &[i64]| values.iter().map(|&v| Num::Int(v)).collect::<Vec<_>>();
        let float = |values: &[f64]| values.iter().map(|&v| Num::Float(v)).collect::<Vec<_>>();
        // joints 0 and 1 stand for frames 7 and 3; a weight of 0 and a fifth weight are dropped
        let frames = [7, 3, 9];
        let exported = skin_tail(&int(&[0, 1, 0, 0]), &float(&[0.6, 0.4, 0.0, 0.0]), &frames);
        assert_eq!(exported, [3.0, 0.4, 7.0, 0.6]);
        // Blender's: joints in another order, renormalised in 32 bits
        let blender = skin_tail(&int(&[1, 0, 2, 0]), &float(&[0.40001, 0.59999, 0.0, 0.0]), &frames);
        assert!(same_skin(&exported, &blender));
        // painted: a weight moved to another bone
        let painted = skin_tail(&int(&[1, 0, 2, 0]), &float(&[0.3, 0.6, 0.1, 0.0]), &frames);
        assert!(!same_skin(&exported, &painted));
        // two joints of one frame add up
        assert_eq!(skin_tail(&int(&[0, 0, 1, 0]), &float(&[0.25, 0.25, 0.5, 0.0]), &frames), [3.0, 0.5, 7.0, 0.5]);
    }

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
