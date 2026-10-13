//! A character (an unpacked .chr) exported to glTF 2.0 (.gltf and .bin): one node per MDS frame,
//! rigid meshes on their frames, skinned meshes (VERTEX_ANIME frames with weights) on extra nodes
//! with one skin over every frame, a material per MDT material with a placeholder texture, and an
//! animation per KEY clip of motion set 0.

use crate::folder::Folder;
use crate::info_cfg::{self, CharInfo};
use crate::json::Value as Arg;
use crate::json::Value;
use crate::mds::{self, Material, Mds, Slot, IDENT};
use crate::mot::{self, KeyList};
use crate::{bail, im3, object, py, Result};

/// A column-major 4x4 matrix.
pub type Matrix = [f64; 16];

pub fn mat_mul(a: &Matrix, b: &Matrix) -> Matrix {
    mds::mat_mul(a, b)
}

/// The inverse by Gauss-Jordan elimination, or None when singular.
pub fn mat_inv(m: &Matrix) -> Option<Matrix> {
    let mut a: Vec<[f64; 8]> = (0..4)
        .map(|r| {
            let mut row = [0.0; 8];
            for c in 0..4 {
                row[c] = m[c * 4 + r];
                row[4 + c] = if r == c { 1.0 } else { 0.0 };
            }
            row
        })
        .collect();
    for col in 0..4 {
        // max(range(col, 4), key=abs): the first of the largest
        let mut pivot = col;
        for r in col + 1..4 {
            if a[r][col].abs() > a[pivot][col].abs() {
                pivot = r;
            }
        }
        if a[pivot][col].abs() < 1e-12 {
            return None;
        }
        a.swap(col, pivot);
        let p = a[col][col];
        for x in a[col].iter_mut() {
            *x /= p;
        }
        for r in 0..4 {
            if r != col && a[r][col] != 0.0 {
                let f = a[r][col];
                let pivot_row = a[col];
                for (x, y) in a[r].iter_mut().zip(pivot_row) {
                    *x -= f * y;
                }
            }
        }
    }
    let mut out = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            out[c * 4 + r] = a[r][4 + c];
        }
    }
    Some(out)
}

pub fn xform_point(m: &Matrix, v: &[f64]) -> [f64; 3] {
    [0, 1, 2].map(|r| m[r] * v[0] + m[4 + r] * v[1] + m[8 + r] * v[2] + m[12 + r])
}

pub fn xform_dir(m: &Matrix, v: &[f64]) -> [f64; 3] {
    [0, 1, 2].map(|r| m[r] * v[0] + m[4 + r] * v[1] + m[8 + r] * v[2])
}

/// The vector divided by its length, or the fallback when it has none.
pub fn normalize(v: &[f64], fallback: &[f64]) -> Vec<f64> {
    let length = py::sum(v.iter().map(|x| x * x)).sqrt();
    if length > 1e-12 {
        v.iter().map(|x| x / length).collect()
    } else {
        fallback.to_vec()
    }
}

const NO_ROTATION: [f64; 4] = [0.0, 0.0, 0.0, 1.0];
const Z: [f64; 3] = [0.0, 0.0, 1.0];

/// The quaternion (x, y, z, w) of the rotation whose columns are c0, c1, c2.
fn quat_from_rot(c0: &[f64], c1: &[f64], c2: &[f64]) -> Result<Vec<f64>> {
    let (m00, m10, m20) = (c0[0], c0[1], c0[2]);
    let (m01, m11, m21) = (c1[0], c1[1], c1[2]);
    let (m02, m12, m22) = (c2[0], c2[1], c2[2]);
    let tr = m00 + m11 + m22;
    let q = if tr > 0.0 {
        let s = py::sqrt(tr + 1.0)? * 2.0;
        [py::div(m21 - m12, s)?, py::div(m02 - m20, s)?, py::div(m10 - m01, s)?, 0.25 * s]
    } else if m00 > m11 && m00 > m22 {
        let s = py::sqrt(1.0 + m00 - m11 - m22)? * 2.0;
        [0.25 * s, py::div(m01 + m10, s)?, py::div(m02 + m20, s)?, py::div(m21 - m12, s)?]
    } else if m11 > m22 {
        let s = py::sqrt(1.0 + m11 - m00 - m22)? * 2.0;
        [py::div(m01 + m10, s)?, 0.25 * s, py::div(m12 + m21, s)?, py::div(m02 - m20, s)?]
    } else {
        let s = py::sqrt(1.0 + m22 - m00 - m11)? * 2.0;
        [py::div(m02 + m20, s)?, py::div(m12 + m21, s)?, 0.25 * s, py::div(m10 - m01, s)?]
    };
    Ok(normalize(&q, &NO_ROTATION))
}

/// Translation, rotation and scale of a matrix.
pub fn decompose(m: &Matrix) -> Result<([f64; 3], Vec<f64>, [f64; 3])> {
    let t = [m[12], m[13], m[14]];
    let cols = [&m[0..3], &m[4..7], &m[8..11]];
    let mut s = cols.map(|c| py::sum(c.iter().map(|x| x * x)).sqrt());
    let det = cols[0][0] * (cols[1][1] * cols[2][2] - cols[1][2] * cols[2][1])
        - cols[1][0] * (cols[0][1] * cols[2][2] - cols[0][2] * cols[2][1])
        + cols[2][0] * (cols[0][1] * cols[1][2] - cols[0][2] * cols[1][1]);
    if det < 0.0 {
        s[0] = -s[0];
    }
    let rc: Vec<Vec<f64>> = (0..3)
        .map(|i| {
            let d = if s[i] == 0.0 { 1.0 } else { s[i] };
            cols[i].iter().map(|x| x / d).collect()
        })
        .collect();
    Ok((t, quat_from_rot(&rc[0], &rc[1], &rc[2])?, s))
}

/// .mot rotation keys are (w, x, y, z) fed to QuatToMat, which builds the transpose of the usual
/// matrix; so the glTF rotation is the conjugate.
fn engine_quat_to_gltf(q: &[f64; 4]) -> Vec<f64> {
    let [w, x, y, z] = *q;
    normalize(&[-x, -y, -z, w], &NO_ROTATION)
}

fn slerp(a: &[f64], b: &[f64], t: f64) -> Result<Vec<f64>> {
    let mut b = b.to_vec();
    let mut d = py::sum(a.iter().zip(&b).map(|(x, y)| x * y));
    if d < 0.0 {
        b = b.iter().map(|x| -x).collect();
        d = -d;
    }
    if d > 0.9995 {
        let mixed: Vec<f64> = a.iter().zip(&b).map(|(x, y)| x + (y - x) * t).collect();
        return Ok(normalize(&mixed, &Z));
    }
    let th = py::acos(py::min(1.0, d))?;
    let s = th.sin();
    let (fa, fb) = (py::div(((1.0 - t) * th).sin(), s)?, py::div((t * th).sin(), s)?);
    let mixed: Vec<f64> = a.iter().zip(&b).map(|(x, y)| fa * x + fb * y).collect();
    Ok(normalize(&mixed, &Z))
}

/// The value of a key list at an integer frame, as MotionProc interpolates (slerp or lerp).
fn sample(list: &KeyList, frames: &[i64], f: i64, rotation: bool) -> Result<Vec<f64>> {
    // bisect.bisect_right
    let (mut lo, mut hi) = (0, frames.len());
    while lo < hi {
        let mid = (lo + hi) / 2;
        if f < frames[mid] {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    let k = lo.saturating_sub(1);
    let n = lo.min(frames.len() - 1);
    let (a, b) = (&list.keys[k].1, &list.keys[n].1);
    let t = if frames[n] == frames[k] { 0.0 } else { (f - frames[k]) as f64 / (frames[n] - frames[k]) as f64 };
    if rotation {
        let (qa, qb) = (engine_quat_to_gltf(a), engine_quat_to_gltf(b));
        return if 0.0 < t && t < 1.0 {
            slerp(&qa, &qb, t)
        } else if t <= 0.0 {
            Ok(qa)
        } else {
            Ok(qb)
        };
    }
    Ok((0..3).map(|i| a[i] + (b[i] - a[i]) * t).collect())
}

/// The glTF being built: its JSON parts and its buffer.
struct Gltf {
    bin: Vec<u8>,
    scene_nodes: Vec<usize>,
    nodes: Vec<Value>,
    meshes: Vec<Value>,
    materials: Vec<Value>,
    textures: Vec<Value>,
    images: Vec<Value>,
    samplers: Vec<Value>,
    accessors: Vec<Value>,
    buffer_views: Vec<Value>,
    skins: Vec<Value>,
    animations: Vec<Value>,
}

const ARRAY_BUFFER: i64 = 34962;
const ELEMENT_ARRAY_BUFFER: i64 = 34963;
const FLOAT: i64 = 5126;
const UNSIGNED_SHORT: i64 = 5123;
const UNSIGNED_INT: i64 = 5125;

fn components(kind: &str) -> usize {
    match kind {
        "SCALAR" => 1,
        "VEC2" => 2,
        "VEC3" => 3,
        "VEC4" => 4,
        _ => 16,
    }
}

impl Gltf {
    fn view(&mut self, data: &[u8], target: Option<i64>) -> usize {
        while !self.bin.len().is_multiple_of(4) {
            self.bin.push(0);
        }
        let mut view = object! {"buffer" => 0i64, "byteOffset" => self.bin.len(), "byteLength" => data.len()};
        if let Some(target) = target {
            view.set("target", Value::Int(target));
        }
        self.bin.extend(data);
        self.buffer_views.push(view);
        self.buffer_views.len() - 1
    }

    fn add_accessor(&mut self, data: Vec<u8>, count: usize, kind: &str, ctype: i64, target: Option<i64>) -> Value {
        let view = self.view(&data, target);
        object! {"bufferView" => view, "componentType" => ctype, "count" => count, "type" => kind}
    }

    /// A float accessor of values (each a vector, or one number for SCALAR), with min and max
    /// when asked.
    fn floats(&mut self, values: &[Vec<f64>], kind: &str, target: Option<i64>, minmax: bool) -> Result<usize> {
        let mut data = Vec::new();
        for v in values {
            mds::pack_floats(v, &mut data)?;
        }
        let mut accessor = self.add_accessor(data, values.len(), kind, FLOAT, target);
        if minmax {
            let n = components(kind);
            let mut low = Vec::with_capacity(n);
            let mut high = Vec::with_capacity(n);
            for i in 0..n {
                let column: Vec<f64> = values
                    .iter()
                    .map(|v| v.get(i).copied().ok_or_else(|| crate::Error::invalid("tuple index out of range")))
                    .collect::<Result<_>>()?;
                if column.is_empty() {
                    bail!("min() iterable argument is empty");
                }
                low.push(py::min_of(column.iter().copied()));
                high.push(py::max_of(column.iter().copied()));
            }
            accessor.set("min", Value::floats(&low));
            accessor.set("max", Value::floats(&high));
        }
        self.accessors.push(accessor);
        Ok(self.accessors.len() - 1)
    }

    /// An unsigned accessor (unsigned short or int) of values.
    fn unsigned(&mut self, values: &[Vec<i64>], kind: &str, ctype: i64, target: Option<i64>) -> Result<usize> {
        let mut data = Vec::new();
        for &x in values.iter().flatten() {
            if ctype == UNSIGNED_SHORT {
                let Ok(x) = u16::try_from(x) else {
                    bail!("'H' format requires 0 <= number <= 65535");
                };
                data.extend(x.to_le_bytes());
            } else {
                let Ok(x) = u32::try_from(x) else {
                    bail!("'I' format requires 0 <= number <= 4294967295");
                };
                data.extend(x.to_le_bytes());
            }
        }
        let accessor = self.add_accessor(data, values.len(), kind, ctype, target);
        self.accessors.push(accessor);
        Ok(self.accessors.len() - 1)
    }

    /// The JSON, keys in the Python's order, empty lists left out.
    fn json(self, asset: Value, base: &str) -> (Value, Vec<u8>) {
        let buffers = vec![object! {"byteLength" => self.bin.len(), "uri" => format!("{base}.bin")}];
        let mut items = vec![
            ("asset".to_string(), asset),
            ("scene".to_string(), Value::Int(0)),
            (
                "scenes".to_string(),
                Value::List(vec![
                    object! {"nodes" => Value::List(self.scene_nodes.iter().map(|&n| Value::from(n)).collect())},
                ]),
            ),
        ];
        let lists = [
            ("nodes", self.nodes),
            ("meshes", self.meshes),
            ("materials", self.materials),
            ("textures", self.textures),
            ("images", self.images),
            ("samplers", self.samplers),
            ("accessors", self.accessors),
            ("bufferViews", self.buffer_views),
            ("buffers", buffers),
            ("skins", self.skins),
            ("animations", self.animations),
        ];
        for (name, list) in lists {
            if !list.is_empty() {
                items.push((name.to_string(), Value::List(list)));
            }
        }
        (Value::Object(items), self.bin)
    }
}

/// An 8x8 checker in two colours from the name's CRC, standing for a texture.
pub fn placeholder_png(name: &str) -> Vec<u8> {
    let mut hasher = flate2::Crc::new();
    hasher.update(name.as_bytes());
    let h = hasher.sum();
    let c1 = [(h & 0xFF) as u8, ((h >> 8) & 0xFF) as u8, ((h >> 16) & 0xFF) as u8];
    let c2 = c1.map(|x| 255 - x);
    let mut rows = Vec::new();
    for y in 0..8 {
        rows.push(0);
        for x in 0..8 {
            rows.extend(if (x / 2 + y / 2) % 2 == 1 { c1 } else { c2 });
        }
    }
    let chunk = |tag: &[u8], data: &[u8]| {
        let mut out = (data.len() as u32).to_be_bytes().to_vec();
        out.extend(tag);
        out.extend(data);
        let mut hasher = flate2::Crc::new();
        hasher.update(tag);
        hasher.update(data);
        out.extend(hasher.sum().to_be_bytes());
        out
    };
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    out.extend(chunk(b"IHDR", &[0, 0, 0, 8, 0, 0, 0, 8, 8, 2, 0, 0, 0]));
    out.extend(chunk(b"IDAT", &im3::compress(&rows, 6)));
    out.extend(chunk(b"IEND", b""));
    out
}

/// Per vertex up to 4 (bone frame, weight) pairs, in file order, weights normalised; a vertex no
/// block mentions gets weight 1 on the first bone the blocks list (CreateVertexWeight). None when
/// no block skins the frame.
pub fn vertex_weights(lists: &[KeyList], frame: usize, vertex_num: usize) -> Option<Vec<Vec<(i64, f64)>>> {
    let mut out: Vec<Vec<(i64, f64)>> = vec![Vec::new(); vertex_num];
    let mut first_bone = None;
    for list in lists {
        if list.frame as i64 != frame as i64 || list.keys.is_empty() {
            continue;
        }
        first_bone.get_or_insert(list.target as i64);
        for (v, value) in &list.keys {
            if 0 <= *v && (*v as usize) < vertex_num && out[*v as usize].len() < 4 && value[0] != 0.0 {
                out[*v as usize].push((list.target as i64, value[0] / 100.0));
            }
        }
    }
    let first_bone = first_bone?;
    for l in &mut out {
        let s = py::sum(l.iter().map(|(_, w)| *w));
        if s == 0.0 {
            *l = vec![(first_bone, 1.0)];
        } else if s != 1.0 {
            for p in l.iter_mut() {
                p.1 /= s;
            }
        }
    }
    Some(out)
}

/// list[i] with Python's negative indexing.
pub fn py_index<T>(items: &[T], i: i64) -> Result<&T> {
    let index = if i < 0 { items.len() as i64 + i } else { i };
    if index < 0 || index >= items.len() as i64 {
        bail!("list index out of range");
    }
    Ok(&items[index as usize])
}

/// A character's info.cfg, model, bind pose, weights and motion, read from its unpacked pack.
pub struct Character {
    pub info: CharInfo,
    pub model: Mds,
    pub bbp: Option<Vec<Matrix>>,
    pub wgt: Option<Vec<KeyList>>,
    pub motion: Option<Vec<KeyList>>,
}

fn text_arg(value: &Arg) -> Result<Option<&str>> {
    if !value.truthy() {
        return Ok(None);
    }
    match value {
        Arg::Str(text) => Ok(Some(text)),
        _ => bail!("expected str, bytes or os.PathLike object"),
    }
}

/// os.path.splitext(path)[0].
pub fn strip_extension(path: &str) -> &str {
    let base_start = path.rfind(['/', '\\']).map_or(0, |i| i + 1);
    let base = &path[base_start..];
    let leading = base.len() - base.trim_start_matches('.').len();
    match base[leading..].rfind('.') {
        Some(dot) => &path[..base_start + leading + dot],
        None => path,
    }
}

pub fn load_char(folder: &Folder) -> Result<Character> {
    let Some(info) = folder.get("info.cfg") else {
        bail!("[Errno 2] No such file or directory: 'info.cfg'");
    };
    let info = info_cfg::read(info);
    let Some(model_name) = text_arg(info.model.as_ref().unwrap_or(&Arg::Null))? else {
        bail!("info.cfg has no MODEL");
    };
    let Some(model) = folder.get(model_name) else {
        bail!("[Errno 2] No such file or directory: {}", im3::str_repr(model_name));
    };
    let model = mds::parse(model)?;
    let motion0 = info.motion0();
    let mut bbp = None;
    if let Some(name) = motion0.map(|m| text_arg(&m.bbp)).transpose()?.flatten() {
        if let Some(data) = folder.get(name) {
            bbp = Some(mot::parse_bbp(data)?);
        }
    }
    let mut wgt = None;
    if let Some(data) = folder.get(&format!("{}.wgt", strip_extension(model_name))) {
        if !data.is_empty() {
            wgt = Some(mot::parse(data)?);
        }
    }
    let mut motion = None;
    if let Some(name) = motion0.map(|m| text_arg(&m.mot)).transpose()?.flatten() {
        if let Some(data) = folder.get(name) {
            motion = Some(mot::parse(data)?);
        }
    }
    Ok(Character { info, model, bbp, wgt, motion })
}

/// A material key: equal as the Python's tuple (texture, diffuse, ambient) is.
struct MaterialKey {
    texture: String,
    diffuse: Vec<f64>,
    ambient: Vec<f64>,
    origin: (usize, usize),
}

impl MaterialKey {
    fn same(&self, other: &MaterialKey) -> bool {
        // the same Material object's colours compare equal even holding a NaN
        self.texture == other.texture
            && (self.origin == other.origin || (self.diffuse == other.diffuse && self.ambient == other.ambient))
    }
}

/// Python's str() of an argument value, for '%s'.
fn py_str(value: &Arg) -> String {
    match value {
        Arg::Null => "None".into(),
        Arg::Bool(b) => if *b { "True" } else { "False" }.into(),
        Arg::Int(v) => v.to_string(),
        Arg::Float(v) => crate::py::float_repr(*v),
        Arg::Str(s) => s.clone(),
        _ => String::new(),
    }
}

/// Exports the character in a folder to `<base>.gltf` and `<base>.bin` in `out`, with placeholder
/// textures in textures/.
pub fn export(folder: &Folder, out: &mut Folder, base: &str, fps: f64) -> Result<()> {
    let character = load_char(folder)?;
    let (info, model) = (&character.info, &character.model);
    let deform: Vec<&str> = info.vertex_anime.iter().map(String::as_str).collect();
    let mut g = Gltf {
        bin: Vec::new(),
        scene_nodes: Vec::new(),
        nodes: Vec::new(),
        meshes: Vec::new(),
        materials: Vec::new(),
        textures: Vec::new(),
        images: Vec::new(),
        samplers: Vec::new(),
        accessors: Vec::new(),
        buffer_views: Vec::new(),
        skins: Vec::new(),
        animations: Vec::new(),
    };
    let world = model.world_matrices()?;
    let nodes = &model.nodes;

    for (i, node) in nodes.iter().enumerate() {
        let (t, r, s) = decompose(&node.matrix)?;
        let name = if node.base_name().is_empty() { format!("node{i}") } else { node.base_name().to_string() };
        let mut gn = object! {"name" => name};
        if t.iter().any(|x| x.abs() > 0.0) {
            gn.set("translation", Value::floats(&t));
        }
        if r != NO_ROTATION {
            gn.set("rotation", Value::floats(&r));
        }
        if s.iter().any(|x| (x - 1.0).abs() > 1e-6) {
            gn.set("scale", Value::floats(&s));
        }
        gn.set("extras", object! {"mds_index" => i, "mds_name" => node.name.as_str()});
        g.nodes.push(gn);
    }
    for (i, node) in nodes.iter().enumerate() {
        if node.parent >= 0 {
            let Some(parent) = g.nodes.get_mut(node.parent as usize) else {
                bail!("list index out of range");
            };
            match parent.get_mut("children") {
                Some(Value::List(children)) => children.push(Value::from(i)),
                _ => parent.set("children", Value::List(vec![Value::from(i)])),
            }
        } else {
            g.scene_nodes.push(i);
        }
    }
    if g.scene_nodes.len() > 1 {
        // 4 of 16,873 MDS files have two roots; glTF skins want one common root
        let roots = Value::List(g.scene_nodes.iter().map(|&n| Value::from(n)).collect());
        g.nodes.push(object! {"name" => "_mds_roots", "children" => roots, "extras" => object! {"synthetic" => true}});
        g.scene_nodes = vec![g.nodes.len() - 1];
    }
    let Some(&skeleton_root) = g.scene_nodes.first() else {
        bail!("list index out of range");
    };

    let mut texture_index: Vec<(String, usize)> = Vec::new();
    let mut material_index: Vec<(MaterialKey, usize)> = Vec::new();
    // a texture name the file system cannot hold fails the export, as writing its file does
    let mut failed: Option<String> = None;
    let mut material = |g: &mut Gltf,
                        out: &mut Folder,
                        m: &Material,
                        origin: (usize, usize),
                        textured: bool|
     -> usize {
        let key = MaterialKey {
            texture: if textured { m.texture.clone() } else { String::new() },
            diffuse: m.diffuse.clone(),
            ambient: m.ambient.clone(),
            origin,
        };
        if let Some((_, index)) = material_index.iter().find(|(k, _)| k.same(&key)) {
            return *index;
        }
        let factor: Vec<f64> = m.diffuse.iter().map(|&x| py::max(0.0, py::min(1.0, x))).collect();
        let mut pbr =
            object! {"baseColorFactor" => Value::floats(&factor), "metallicFactor" => 0.0, "roughnessFactor" => 1.0};
        if textured && !m.texture.is_empty() {
            let index = match texture_index.iter().find(|(name, _)| *name == m.texture) {
                Some((_, index)) => *index,
                None => {
                    if m.texture.chars().any(|c| "<>:\"/\\|?*".contains(c) || (c as u32) < 0x20) {
                        failed = Some(format!("cannot write a texture named {}", im3::str_repr(&m.texture)));
                    }
                    let file = format!("{}.png", m.texture);
                    out.write(&format!("textures/{file}"), placeholder_png(&m.texture));
                    if g.samplers.is_empty() {
                        g.samplers.push(object! {"magFilter" => 9729i64, "minFilter" => 9729i64});
                    }
                    g.images.push(object! {"uri" => format!("textures/{file}"), "name" => m.texture.as_str()});
                    g.textures.push(
                        object! {"source" => g.images.len() - 1, "sampler" => 0i64, "name" => m.texture.as_str()},
                    );
                    texture_index.push((m.texture.clone(), g.textures.len() - 1));
                    g.textures.len() - 1
                }
            };
            pbr.set("baseColorTexture", object! {"index" => index});
        }
        let name = format!(
            "{}{}",
            if m.texture.is_empty() { "untextured" } else { &m.texture },
            if textured { "" } else { "_notex" }
        );
        let mut gm = object! {
            "name" => name,
            "pbrMetallicRoughness" => pbr,
            "extras" => object! {
                "mdt_diffuse" => Value::floats(&m.diffuse),
                "mdt_ambient" => Value::floats(&m.ambient),
                "mdt_specular" => Value::floats(&m.specular),
                "mdt_power" => m.power,
                "texture" => m.texture.as_str(),
            },
        };
        if m.diffuse.get(3).is_some_and(|&a| a < 1.0) {
            gm.set("alphaMode", Value::from("BLEND"));
        }
        g.materials.push(gm);
        material_index.push((key, g.materials.len() - 1));
        g.materials.len() - 1
    };

    let mut any_skinned = false;
    for (i, node) in nodes.iter().enumerate() {
        let Some(t) = &node.mdt else {
            continue;
        };
        let mut weights = None;
        if let Some(wgt) = character.wgt.as_ref().filter(|w| !w.is_empty()) {
            if deform.contains(&node.base_name()) {
                weights = vertex_weights(wgt, i, t.vertices.len());
            }
        }
        let skinned = weights.is_some();
        let mut bind = world[i];
        if skinned {
            if let Some(bbp) = character.bbp.as_ref().filter(|b| !b.is_empty() && i < b.len()) {
                if mat_inv(&bbp[i]).is_some() {
                    bind = bbp[i];
                }
            }
        }
        // (material, has normals, has uvs, has colours) -> corner keys, their numbers, indices
        type Group = (Vec<[i64; 4]>, std::collections::HashMap<[i64; 4], usize>, Vec<i64>);
        let mut groups: std::collections::BTreeMap<(u32, bool, bool, bool), Group> = Default::default();
        for p in &t.prims {
            let (sv, sn, su, sc) =
                (p.slot(Slot::Vertex).unwrap(), p.slot(Slot::Normal), p.slot(Slot::Uv), p.slot(Slot::Colour));
            let group = groups.entry((p.material, sn.is_some(), su.is_some(), sc.is_some())).or_default();
            for triangle in p.triangles() {
                let corners: Vec<&Vec<i32>> = triangle.iter().map(|&c| &p.verts[c]).collect();
                let distinct: std::collections::HashSet<i32> = corners.iter().map(|c| c[sv]).collect();
                if distinct.len() < 3 {
                    continue; // degenerate (strip joins)
                }
                for c in corners {
                    let slot = |s: Option<usize>| s.map_or(-1, |s| c[s] as i64);
                    let k = [c[sv] as i64, slot(sn), slot(su), slot(sc)];
                    let next = group.0.len();
                    let number = *group.1.entry(k).or_insert(next);
                    if number == next {
                        group.0.push(k);
                    }
                    group.2.push(number as i64);
                }
            }
        }
        let mut prims = Vec::new();
        for ((mat, hn, hu, hc), (vs, _, indices)) in &groups {
            if indices.is_empty() {
                continue;
            }
            let mut pos: Vec<Vec<f64>> =
                vs.iter().map(|k| Ok(py_index(&t.vertices, k[0])?[..3].to_vec())).collect::<Result<_>>()?;
            if skinned {
                pos = pos.iter().map(|v| xform_point(&bind, v).to_vec()).collect();
            }
            let mut attributes = object! {"POSITION" => g.floats(&pos, "VEC3", Some(ARRAY_BUFFER), true)?};
            if *hn {
                let mut normals: Vec<Vec<f64>> =
                    vs.iter().map(|k| Ok(normalize(&py_index(&t.normals, k[1])?[..3], &Z))).collect::<Result<_>>()?;
                if skinned {
                    normals = normals.iter().map(|v| normalize(&xform_dir(&bind, v), &Z)).collect();
                }
                attributes.set("NORMAL", Value::from(g.floats(&normals, "VEC3", Some(ARRAY_BUFFER), false)?));
            }
            if *hu {
                let uvs: Vec<Vec<f64>> =
                    vs.iter().map(|k| Ok(py_index(&t.uvs, k[2])?[..2].to_vec())).collect::<Result<_>>()?;
                attributes.set("TEXCOORD_0", Value::from(g.floats(&uvs, "VEC2", Some(ARRAY_BUFFER), false)?));
            }
            if *hc {
                let colours: Vec<Vec<f64>> = vs
                    .iter()
                    .map(|k| Ok(py_index(&t.colours, k[3])?.iter().map(|&x| py::max(0.0, py::min(1.0, x))).collect()))
                    .collect::<Result<_>>()?;
                attributes.set("COLOR_0", Value::from(g.floats(&colours, "VEC4", Some(ARRAY_BUFFER), false)?));
            }
            if let Some(weights) = &weights {
                let (mut joints, mut amounts) = (Vec::new(), Vec::new());
                for k in vs {
                    let mut l = py_index(weights, k[0])?.clone();
                    l.resize(4.max(l.len()), (0, 0.0));
                    joints.push(l.iter().map(|(b, _)| *b).collect::<Vec<i64>>());
                    amounts.push(l.iter().map(|(_, w)| *w).collect::<Vec<f64>>());
                }
                attributes
                    .set("JOINTS_0", Value::from(g.unsigned(&joints, "VEC4", UNSIGNED_SHORT, Some(ARRAY_BUFFER))?));
                attributes.set("WEIGHTS_0", Value::from(g.floats(&amounts, "VEC4", Some(ARRAY_BUFFER), false)?));
            }
            let big = vs.len() > 65535;
            let index_values: Vec<Vec<i64>> = indices.iter().map(|&x| vec![x]).collect();
            let indices = g.unsigned(
                &index_values,
                "SCALAR",
                if big { UNSIGNED_INT } else { UNSIGNED_SHORT },
                Some(ELEMENT_ARRAY_BUFFER),
            )?;
            let mut gp = object! {"attributes" => attributes, "indices" => indices, "mode" => 4i64};
            if (*mat as usize) < t.materials.len() {
                gp.set(
                    "material",
                    Value::from(material(&mut g, out, &t.materials[*mat as usize], (i, *mat as usize), *hu)),
                );
            }
            prims.push(gp);
        }
        if prims.is_empty() {
            continue;
        }
        g.meshes.push(object! {
            "name" => node.base_name(),
            "primitives" => Value::List(prims),
            "extras" => object! {"mdt_style" => t.style.to_string()},
        });
        let mesh = g.meshes.len() - 1;
        if skinned {
            any_skinned = true;
            g.nodes.push(object! {
                "name" => format!("{}_skin", node.base_name()),
                "mesh" => mesh,
                "skin" => 0i64,
                "extras" => object! {"mds_index" => i, "skinned_mesh_of" => node.name.as_str()},
            });
            g.scene_nodes.push(g.nodes.len() - 1);
        } else {
            g.nodes[i].set("mesh", Value::from(mesh));
        }
    }

    if let Some(message) = failed {
        bail!("{message}");
    }
    if any_skinned {
        let mut inverses = Vec::with_capacity(nodes.len());
        for (i, w) in world.iter().enumerate() {
            let m = character.bbp.as_ref().filter(|b| !b.is_empty() && i < b.len()).map(|b| b[i]);
            let inverse = m.and_then(|m| mat_inv(&m)).or_else(|| mat_inv(w)).unwrap_or(IDENT);
            inverses.push(inverse.to_vec());
        }
        let ibm = g.floats(&inverses, "MAT4", None, false)?;
        g.skins.push(object! {
            "name" => "skeleton",
            "joints" => Value::List((0..nodes.len()).map(Value::from).collect()),
            "skeleton" => skeleton_root,
            "inverseBindMatrices" => ibm,
        });
    }

    let clips = info.motion0().map(|m| m.keys.as_slice()).unwrap_or_default();
    if let Some(motion) = character.motion.as_ref().filter(|m| !m.is_empty() && !clips.is_empty()) {
        let mut by_name: Vec<(String, usize)> = Vec::new();
        for (i, node) in nodes.iter().enumerate() {
            let lower = node.base_name().to_lowercase();
            if !by_name.iter().any(|(n, _)| *n == lower) {
                by_name.push((lower, i));
            }
        }
        let mut channels_of: std::collections::BTreeMap<(usize, &str), &KeyList> = Default::default();
        for list in motion {
            if !(0..=2).contains(&list.kind) || list.keys.is_empty() || list.frame as i64 >= nodes.len() as i64 {
                continue;
            }
            let lower = py_index(nodes, list.frame as i64)?.base_name().to_lowercase();
            let Some(&(_, target)) = by_name.iter().find(|(n, _)| *n == lower) else {
                continue;
            };
            let path = ["rotation", "scale", "translation"][list.kind as usize];
            channels_of.insert((target, path), list);
        }
        let mut seen: Vec<Arg> = Vec::new();
        for clip in clips {
            let mut name = clip.name.clone();
            if seen.iter().any(|s| info_cfg::same_value(s, &name)) {
                let start = match clip.start {
                    Arg::Int(v) => v,
                    Arg::Float(v) => v.trunc() as i64,
                    _ => bail!("%d format: a real number is required"),
                };
                name = Arg::Str(format!("{}_{}", py_str(&name), start));
            }
            seen.push(name.clone());
            let Arg::Int(s) = clip.start else {
                bail!("'float' object cannot be interpreted as an integer");
            };
            let e = match clip.end {
                Arg::Int(end) => end.max(s),
                Arg::Float(end) if end > s as f64 => bail!("'float' object cannot be interpreted as an integer"),
                Arg::Float(_) => s,
                _ => bail!("'>' not supported between instances"),
            };
            let frames: Vec<i64> = (s..=e).collect();
            let times: Vec<Vec<f64>> = frames.iter().map(|f| vec![(f - s) as f64 / fps]).collect();
            let times = g.floats(&times, "SCALAR", None, true)?;
            let (mut samplers, mut channels) = (Vec::new(), Vec::new());
            for (&(node, path), list) in &channels_of {
                let key_frames: Vec<i64> = list.keys.iter().map(|(k, _)| *k).collect();
                let rotation = path == "rotation";
                let mut values =
                    frames.iter().map(|&f| sample(list, &key_frames, f, rotation)).collect::<Result<Vec<_>>>()?;
                if rotation {
                    for a in 1..values.len() {
                        if py::sum(values[a - 1].iter().zip(&values[a]).map(|(x, y)| x * y)) < 0.0 {
                            values[a] = values[a].iter().map(|x| -x).collect();
                        }
                    }
                }
                let output = g.floats(&values, if rotation { "VEC4" } else { "VEC3" }, None, false)?;
                samplers.push(object! {"input" => times, "output" => output, "interpolation" => "LINEAR"});
                channels.push(object! {
                    "sampler" => samplers.len() - 1,
                    "target" => object! {"node" => node, "path" => path},
                });
            }
            g.animations.push(object! {
                "name" => name,
                "samplers" => Value::List(samplers),
                "channels" => Value::List(channels),
                "extras" => object! {"start" => clip.start.clone(), "end" => clip.end.clone(), "step" => clip.step.clone()},
            });
        }
    }
    let asset = object! {
        "version" => "2.0",
        "generator" => "ChronicleTwo-Script to_gltf.py",
        "extras" => object! {
            "source" => object! {"base" => "pack"},
            "fps_assumed" => fps,
            "note" => "textures are placeholders; MDS node i = glTF node i",
        },
    };
    let (json, bin) = g.json(asset, base);
    out.write(&format!("{base}.bin"), bin);
    out.write(&format!("{base}.gltf"), crate::json::dumps_compact(&json).into_bytes());
    Ok(())
}
