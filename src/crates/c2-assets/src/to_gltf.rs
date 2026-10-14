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

/// The column-major matrix of a translation, rotation (x, y, z, w) and scale.
pub fn compose(t: &[f64], r: &[f64], s: &[f64]) -> Matrix {
    let (x, y, z, w) = (r[0], r[1], r[2], r[3]);
    let columns = [
        [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y + z * w), 2.0 * (x * z - y * w)],
        [2.0 * (x * y - z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z + x * w)],
        [2.0 * (x * z + y * w), 2.0 * (y * z - x * w), 1.0 - 2.0 * (x * x + y * y)],
    ];
    let mut m = IDENT;
    for (c, column) in columns.iter().enumerate() {
        for row in 0..3 {
            m[c * 4 + row] = column[row] * s[c];
        }
    }
    m[12..15].copy_from_slice(&t[..3]);
    m
}

/// .mot rotation keys are (w, x, y, z) fed to QuatToMat, which builds the transpose of the usual
/// matrix; so the glTF rotation is the conjugate.
fn engine_quat_to_gltf(q: &[f64; 4]) -> Vec<f64> {
    let [w, x, y, z] = *q;
    normalize(&[-x, -y, -z, w], &NO_ROTATION)
}

pub(crate) fn slerp(a: &[f64], b: &[f64], t: f64) -> Result<Vec<f64>> {
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

/// A character's info.cfg, model, bind pose, weights and motion sets, read from its unpacked pack.
pub struct Character {
    pub info: CharInfo,
    pub model: Mds,
    pub bbp: Option<Vec<Matrix>>,
    pub wgt: Option<Vec<KeyList>>,
    /// The motion sets whose .mot the pack holds, in the order info.cfg first gives their ids.
    pub motions: Vec<SetMotion>,
}

/// A motion set (`MOTION id, "x.mot", ...`): its id, its .mot's key lists and its KEYs.
#[derive(Clone, Debug)]
pub struct SetMotion {
    pub id: Arg,
    pub lists: Vec<KeyList>,
    pub keys: Vec<info_cfg::Key>,
}

/// The motion sets of an info.cfg whose .mot a folder holds (set 0's must read; another set whose
/// .mot does not read is left out).
pub fn load_motions(folder: &Folder, info: &CharInfo) -> Result<Vec<SetMotion>> {
    let mut out = Vec::new();
    for (id, motion) in &info.motions {
        let zero = info_cfg::same_value(id, &Arg::Int(0));
        let name = match text_arg(&motion.mot) {
            Ok(name) => name,
            Err(error) if zero => return Err(error),
            Err(_) => continue,
        };
        let Some(data) = name.and_then(|name| folder.get(name)) else {
            continue;
        };
        let lists = match mot::parse(data) {
            Ok(lists) => lists,
            Err(error) if zero => return Err(error),
            Err(_) => continue,
        };
        out.push(SetMotion { id: id.clone(), lists, keys: motion.keys.clone() });
    }
    Ok(out)
}

/// A motion set's id as names give it (`7`).
pub fn id_text(id: &Arg) -> String {
    py_str(id)
}

/// The set whose KEYs keep their names in the export: set 0, else the first.
pub fn primary_set(ids: &[&Arg]) -> usize {
    ids.iter().position(|id| info_cfg::same_value(id, &Arg::Int(0))).unwrap_or(0)
}

/// The animation name of a KEY of a motion set: the KEY's own for the primary set (with its set's
/// id in front when it would read as another set's), `<id>:<name>` for the others.
pub fn set_animation_name(id: &Arg, primary: bool, name: &str) -> String {
    if primary && set_of_name(name).is_none() {
        name.to_string()
    } else {
        format!("{}:{name}", py_str(id))
    }
}

/// The set id and KEY name an animation name gives (`7:walk`), None for a name without one.
pub fn set_of_name(name: &str) -> Option<(i64, &str)> {
    let (text, rest) = name.split_once(':')?;
    let id: i64 = text.parse().ok().filter(|id| (0..8).contains(id))?;
    (id.to_string() == text).then_some((id, rest))
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
    let motions = load_motions(folder, &info)?;
    Ok(Character { info, model, bbp, wgt, motions })
}

/// A material key: equal as the Python's tuple (texture, diffuse, ambient) is.
struct MaterialKey {
    texture: String,
    diffuse: Vec<f64>,
    ambient: Vec<f64>,
    origin: (usize, usize),
    /// The frame's blend flags, when they become material settings.
    flags: String,
}

impl MaterialKey {
    fn same(&self, other: &MaterialKey) -> bool {
        // the same Material object's colours compare equal even holding a NaN
        self.texture == other.texture
            && self.flags == other.flags
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
    export_parts(&character, &character.model, out, base, fps, &Options::default())?;
    Ok(())
}

/// Exports a pack of motion sets alone (`chara/c01_fishing.chr`: `MOTION 7`, no model) on the
/// model of another pack (`skeleton`, whose motions it plays): that pack's skeleton, skin and
/// meshes with an animation per KEY of the motion pack's sets.
pub fn export_motions(motion_folder: &Folder, skeleton: &Folder, out: &mut Folder, base: &str, fps: f64) -> Result<()> {
    let Some(info) = motion_folder.get("info.cfg") else {
        bail!("the motion pack has no info.cfg");
    };
    let info = info_cfg::read(info);
    let mut character = load_char(skeleton)?;
    character.motions = load_motions(motion_folder, &info)?;
    if character.motions.is_empty() {
        bail!("the pack has no motion set to export");
    }
    export_parts(&character, &character.model, out, base, fps, &Options::default())?;
    Ok(())
}

/// Exports one model (a map piece's .mds: no skin, no motion) to `<base>.gltf` and `<base>.bin` in
/// `out`, with its textures in textures/, as the options say.
pub fn export_model(model: &Mds, out: &mut Folder, base: &str, options: &Options) -> Result<Exported> {
    let character =
        Character { info: CharInfo::default(), model: Mds::new(), bbp: None, wgt: None, motions: Vec::new() };
    export_parts(&character, model, out, base, 30.0, options)
}

/// How a model is exported. The default is the character export's: nodes and meshes named by
/// their frames' base names, placeholder textures, materials from the MDT fields alone.
#[derive(Clone, Copy, Default)]
pub struct Options<'a> {
    /// Name glTF nodes and meshes by the frames' whole names, `--` flags included (map meshes tell
    /// frames apart by them: `grid54--f2appz0n1t-5_1` and `grid54--f2appz0n1t-6_1`).
    pub full_names: bool,
    /// Take each material's picture from these texture files (by picture name, the first file
    /// holding it), with its GS CLAMP as the sampler's wrap; names none of them hold get a
    /// placeholder.
    pub textures: Option<&'a TextureSearch>,
    /// Turn the frames' name flags (blend, alpha test, lighting) into glTF material settings, the
    /// flags kept in the material's extras.
    pub blend_flags: bool,
    /// The model is collision (`-a` floor and wall, `-c` camera): materials are named for the
    /// per-triangle attributes their diffuse colour encodes (`col_ground0_foot2_area6_mask12`) and
    /// coloured by them, untextured.
    pub collision: bool,
}

impl Options<'_> {
    /// The options for a map's drawn mesh (`-m`): whole names, its textures, its blend flags.
    pub fn map(textures: Option<&TextureSearch>) -> Options<'_> {
        Options { full_names: true, textures, blend_flags: true, collision: false }
    }

    /// The options for a map's collision mesh (`-a`, `-c`): whole names, attribute materials.
    pub fn collision() -> Options<'static> {
        Options { full_names: true, textures: None, blend_flags: false, collision: true }
    }
}

/// Whether a map model's file name is a collision mesh's (`-a` floor and wall, `-c` camera).
pub fn is_collision_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with("-a.mds") || lower.ends_with("-c.mds")
}

/// What an export found, of the textures it gave placeholders: the names no texture file holds,
/// and those registered without pixels (no_image pictures a texture animation fills at run time).
#[derive(Clone, Debug, Default)]
pub struct Exported {
    pub missing_textures: Vec<String>,
    pub no_image_textures: Vec<String>,
}

/// Pictures by name across a list of texture files (.img), the first file holding a name winning:
/// the search path a map's materials find their pictures in (its own .img files, then shared ones).
pub struct TextureSearch {
    pictures: Vec<(String, im3::Image)>,
    by_name: std::collections::HashMap<String, usize>,
    pngs: std::sync::Mutex<std::collections::HashMap<usize, Vec<u8>>>,
}

impl TextureSearch {
    /// The pictures of the texture files (file name, contents), searched in this order.
    pub fn new(files: &[(String, Vec<u8>)]) -> Result<TextureSearch> {
        let mut search = TextureSearch { pictures: Vec::new(), by_name: Default::default(), pngs: Default::default() };
        for (name, data) in files {
            search.add(name, data)?;
        }
        Ok(search)
    }

    /// Adds a texture file's pictures at the end of the search path.
    pub fn add(&mut self, file: &str, data: &[u8]) -> Result<()> {
        for image in im3::read(data)? {
            if !self.by_name.contains_key(&image.name) {
                self.by_name.insert(image.name.clone(), self.pictures.len());
            }
            self.pictures.push((file.to_string(), image));
        }
        Ok(())
    }

    /// The texture file holding a picture of this name first, and the picture (case-sensitive, as
    /// the engine's texture manager compares names).
    pub fn find(&self, name: &str) -> Option<(&str, &im3::Image)> {
        self.by_name.get(name).map(|&i| (self.pictures[i].0.as_str(), &self.pictures[i].1))
    }

    /// The PNG of a picture of this name, None when no file holds it or it has no pixels.
    pub fn png(&self, name: &str) -> Result<Option<Vec<u8>>> {
        let Some(&i) = self.by_name.get(name) else {
            return Ok(None);
        };
        let image = &self.pictures[i].1;
        if image.no_image {
            return Ok(None);
        }
        if let Some(png) = self.pngs.lock().unwrap().get(&i) {
            return Ok(Some(png.clone()));
        }
        let png = im3::to_png(image, true)?;
        self.pngs.lock().unwrap().insert(i, png.clone());
        Ok(Some(png))
    }

    /// The PNGs of every picture of this name with pixels, in search order: several maps' texture
    /// files hold different pictures under one name, and which the game shows depends on what it
    /// has loaded.
    pub fn pngs_named(&self, name: &str) -> Result<Vec<Vec<u8>>> {
        let mut out = Vec::new();
        for (i, (_, image)) in self.pictures.iter().enumerate() {
            if image.name != name || image.no_image {
                continue;
            }
            let cached = self.pngs.lock().unwrap().get(&i).cloned();
            let png = match cached {
                Some(png) => png,
                None => {
                    let png = im3::to_png(image, true)?;
                    self.pngs.lock().unwrap().insert(i, png.clone());
                    png
                }
            };
            out.push(png);
        }
        Ok(out)
    }
}

const REPEAT: i64 = 10497;
const CLAMP_TO_EDGE: i64 = 33071;

/// The glTF wrap modes (s, t) of a GS CLAMP register value: WMS in bits 0-1, WMT in bits 2-3
/// (0 repeat, 1 clamp, 2 region clamp, 3 region repeat). IM2 pictures have no CLAMP: repeat.
pub fn wrap_of(clamp: Option<u64>) -> (i64, i64) {
    let mode = |bits: u64| if bits == 1 || bits == 2 { CLAMP_TO_EDGE } else { REPEAT };
    match clamp {
        Some(c) => (mode(c & 3), mode((c >> 2) & 3)),
        None => (REPEAT, REPEAT),
    }
}

/// A frame's blend mode from its name flags (`aPP`, `aNN`, `aOF`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blend {
    Add,
    Subtract,
    Opaque,
}

/// The draw settings a frame's name flags give it, as mgSetFrameAttr reads them, those of its
/// ancestors' flags marked with a '.' first. None where no flag sets one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrameFlags {
    pub blend: Option<Blend>,
    /// The alpha test reference (`aXX`, hex; 0x80 is opaque).
    pub alpha_ref: Option<i64>,
    /// `c0` draws without lighting, `c1` with it.
    pub lit: Option<bool>,
    /// `z0` writes no depth.
    pub z_write: Option<bool>,
    /// `fN`.
    pub fog: Option<i64>,
}

impl FrameFlags {
    /// The flags after the name's `--` applied over these; with `dotted` only those marked with a
    /// '.' (the ones the engine pushes down to the frame's subtree).
    fn apply(&mut self, name: &str, dotted: bool) {
        let Some((_, flags)) = name.split_once("--") else {
            return;
        };
        let mut text: Vec<u8> = flags.bytes().collect();
        text.extend([0, 0, 0]);
        let digit = |c: u8| c as i64 - b'0' as i64;
        let (mut i, mut apply) = (0, false);
        while i <= flags.len() {
            let before = self.clone();
            let mut set = true;
            match text[i].to_ascii_lowercase() {
                b'a' => {
                    let code = [text[i + 1].to_ascii_uppercase(), text[i + 2].to_ascii_uppercase()];
                    i += 2;
                    match &code {
                        b"PP" => self.blend = Some(Blend::Add),
                        b"NN" => self.blend = Some(Blend::Subtract),
                        b"OF" => self.blend = Some(Blend::Opaque),
                        _ => {
                            let hex = |c: u8| (c as char).to_digit(16).unwrap_or(0) as i64;
                            // htoi of the two characters, stopping at a NUL
                            self.alpha_ref =
                                Some(if code[1] == 0 { hex(code[0]) } else { hex(code[0]) * 16 + hex(code[1]) });
                        }
                    }
                }
                b'c' => {
                    i += 1;
                    self.lit = Some(digit(text[i]) != 0);
                }
                b'z' => {
                    i += 1;
                    if text[i].eq_ignore_ascii_case(&b'p') {
                        i += 1;
                    } else {
                        self.z_write = Some(digit(text[i]) != 0);
                    }
                }
                b'f' => {
                    i += 1;
                    self.fog = Some(digit(text[i]));
                }
                b'v' | b'm' | b's' | b'n' | b'o' | b'b' => {
                    i += 1;
                    if text[i - 1].eq_ignore_ascii_case(&b'v') && text[i].eq_ignore_ascii_case(&b'c') {
                        i += 1;
                    }
                    set = false;
                }
                _ => set = false,
            }
            if dotted && !(apply && set) {
                *self = before;
            }
            apply = text.get(i) == Some(&b'.');
            i += 1;
        }
    }

    /// Every frame's flags, its ancestors' '.' flags first (parents come before their children).
    pub fn of_model(model: &Mds) -> Vec<FrameFlags> {
        let mut inherited: Vec<FrameFlags> = Vec::with_capacity(model.nodes.len());
        let mut out = Vec::with_capacity(model.nodes.len());
        for node in &model.nodes {
            let parent = usize::try_from(node.parent).ok().filter(|&p| p < inherited.len());
            let base = parent.map(|p| inherited[p].clone()).unwrap_or_default();
            let mut own = base.clone();
            own.apply(&node.name, false);
            let mut down = base;
            down.apply(&node.name, true);
            inherited.push(down);
            out.push(own);
        }
        out
    }

    /// The glTF alpha mode (and cutoff) these settings come closest to: additive and subtractive
    /// blending are BLEND, an alpha test MASK, `aOF` OPAQUE; otherwise BLEND when the material's
    /// diffuse alpha is below 1.
    pub fn alpha_mode(&self, diffuse_alpha: f64) -> Option<(&'static str, Option<f64>)> {
        match (self.blend, self.alpha_ref) {
            (Some(Blend::Add | Blend::Subtract), _) => Some(("BLEND", None)),
            (_, Some(r)) if r > 0 => Some(("MASK", Some(py::min(1.0, r as f64 / 128.0)))),
            (Some(Blend::Opaque), _) => None,
            _ if diffuse_alpha < 1.0 => Some(("BLEND", None)),
            _ => None,
        }
    }

    fn extras(&self, own: &str, inherited: &str) -> Value {
        let mut out = object! {"mds_flags" => own};
        if !inherited.is_empty() {
            out.set("inherited_flags", Value::from(inherited));
        }
        if let Some(blend) = self.blend {
            let name = match blend {
                Blend::Add => "add",
                Blend::Subtract => "subtract",
                Blend::Opaque => "opaque",
            };
            out.set("blend", Value::from(name));
        }
        if let Some(r) = self.alpha_ref {
            out.set("alpha_ref", Value::from(r));
        }
        if let Some(lit) = self.lit {
            out.set("lit", Value::Bool(lit));
        }
        if let Some(z) = self.z_write {
            out.set("z_write", Value::Bool(z));
        }
        if let Some(fog) = self.fog {
            out.set("fog", Value::from(fog));
        }
        out
    }
}

/// The per-triangle attributes a collision material's diffuse colour encodes (LoadCollisionFile):
/// ground = (int)(r*0.7+0.01), foot sound = (int)(g*0.7+0.01), area = (int)(b*0.7+0.01), ignore
/// mask = (int)(1-a), in the engine's float32 arithmetic.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CollisionAttributes {
    pub ground: i64,
    pub foot: i64,
    pub area: i64,
    pub mask: i64,
}

impl CollisionAttributes {
    pub fn decode(diffuse: &[f64]) -> CollisionAttributes {
        let d = |i: usize| diffuse.get(i).copied().unwrap_or(1.0) as f32;
        let k = |x: f32| (x * 0.7f32 + 0.01f32) as i64;
        CollisionAttributes { ground: k(d(0)), foot: k(d(1)), area: k(d(2)), mask: (1.0f32 - d(3)) as i64 }
    }

    /// The diffuse colour that encodes these attributes: k/0.7 as float32, alpha 1-mask.
    pub fn encode(&self) -> Vec<f64> {
        let k = |v: i64| (v as f64 / 0.7) as f32 as f64;
        vec![k(self.ground), k(self.foot), k(self.area), (1.0 - self.mask as f64) as f32 as f64]
    }

    /// The material name: `col_ground0_foot2_area6_mask12`.
    pub fn name(&self) -> String {
        format!("col_ground{}_foot{}_area{}_mask{}", self.ground, self.foot, self.area, self.mask)
    }

    /// The attributes a material name gives (Blender's ".001" suffix allowed), or None.
    pub fn parse(name: &str) -> Option<CollisionAttributes> {
        let name = match name.rsplit_once('.') {
            Some((stem, digits)) if !digits.is_empty() && digits.bytes().all(|c| c.is_ascii_digit()) => stem,
            _ => name,
        };
        let rest = name.strip_prefix("col_")?;
        let mut values = [None; 4];
        for part in rest.split('_') {
            let (slot, digits) = [("ground", 0), ("foot", 1), ("area", 2), ("mask", 3)]
                .iter()
                .find_map(|(label, slot)| part.strip_prefix(label).map(|digits| (*slot, digits)))?;
            if values[slot].is_some() {
                return None;
            }
            values[slot] = Some(digits.parse::<i64>().ok()?);
        }
        Some(CollisionAttributes {
            ground: values[0].unwrap_or(0),
            foot: values[1].unwrap_or(0),
            area: values[2].unwrap_or(0),
            mask: values[3].unwrap_or(0),
        })
    }

    /// A colour to show the attributes by (water areas 1 and 7 blue; others by foot sound and area).
    fn display_colour(&self) -> Vec<f64> {
        if self.area == 1 || self.area == 7 {
            return vec![0.2, 0.4, 0.9, 1.0];
        }
        let mut hasher = flate2::Crc::new();
        hasher.update(format!("{} {} {} {}", self.ground, self.foot, self.area, self.mask).as_bytes());
        let h = hasher.sum();
        let c = |shift: u32| 0.25 + ((h >> shift) & 0xFF) as f64 / 255.0 * 0.6;
        vec![c(0), c(8), c(16), 1.0]
    }
}

impl SetMotion {
    /// The list of a frame, kind and target, with keys.
    pub fn list(&self, frame: usize, kind: i32, target: i32) -> Option<&KeyList> {
        self.lists
            .iter()
            .find(|l| l.frame as i64 == frame as i64 && l.kind == kind && l.target == target && !l.keys.is_empty())
    }

    /// The frames its KEYs play.
    fn key_frames(&self) -> std::collections::BTreeSet<i64> {
        let mut out = std::collections::BTreeSet::new();
        for key in &self.keys {
            if let (Arg::Int(s), Arg::Int(e)) = (&key.start, &key.end) {
                out.extend(*s..=(*e).max(*s));
            }
        }
        out
    }
}

/// A list's value at a frame with the keys between interpolated (lerp), as MotionProc reads
/// vertex and material lists.
fn lerped_value(list: &KeyList, f: i64) -> [f64; 4] {
    let keys = &list.keys;
    let after = keys.partition_point(|(k, _)| *k <= f);
    if after == 0 {
        return keys[0].1;
    }
    let (f0, a) = keys[after - 1];
    if f0 == f || after == keys.len() {
        return a;
    }
    let (f1, b) = keys[after];
    let t = (f - f0) as f64 / (f1 - f0) as f64;
    [0, 1, 2, 3].map(|i| a[i] + (b[i] - a[i]) * t)
}

/// A list's value at a frame held from the key at or before it, as visibility lists are read.
fn held_value(list: &KeyList, f: i64) -> [f64; 4] {
    let after = list.keys.partition_point(|(k, _)| *k <= f);
    list.keys[after.saturating_sub(1)].1
}

/// A morph target of a frame's mesh: a frame of its vertex lists in a motion set (weight 1 at
/// that frame, 0 at others), or a carrier of a list glTF has no channel for that Blender keeps
/// (no displacement; its weight is the list's value): a material's fade (`c2_fade_<k>`, value.x
/// of a type 40 list, alpha being 1 - it), colour (`c2_red_<k>`, `c2_green_<k>`, `c2_blue_<k>`,
/// type 41) or the frame's visibility (`c2_visible`, value.x of a type 50 list; `c2_visible_51`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Target {
    Frame { set: usize, frame: i64 },
    Fade(usize),
    Colour(usize, usize),
    Visible(i32),
}

impl Target {
    /// The target's name (`targetNames`): `c2_frame_<frame>` for the primary set's frames,
    /// `c2_set<id>_frame_<frame>` for another's.
    pub fn name(&self, ids: &[String], primary: usize) -> String {
        match *self {
            Target::Frame { set, frame } if set == primary => format!("c2_frame_{frame}"),
            Target::Frame { set, frame } => format!("c2_set{}_frame_{frame}", ids[set]),
            Target::Fade(k) => format!("c2_fade_{k}"),
            Target::Colour(k, c) => format!("c2_{}_{k}", ["red", "green", "blue"][c]),
            Target::Visible(50) => "c2_visible".to_string(),
            Target::Visible(kind) => format!("c2_visible_{kind}"),
        }
    }

    /// The target a name stands for, given the sets' ids (frames of a set the pack lacks are None).
    pub fn parse(name: &str, ids: &[String], primary: usize) -> Option<Target> {
        let number = |text: &str| -> Option<i64> { text.parse().ok().filter(|_| !text.starts_with('+')) };
        if let Some(rest) = name.strip_prefix("c2_frame_") {
            return Some(Target::Frame { set: primary, frame: number(rest)? });
        }
        if let Some(rest) = name.strip_prefix("c2_set") {
            let (id, frame) = rest.split_once("_frame_")?;
            let set = ids.iter().position(|known| known == id)?;
            return Some(Target::Frame { set, frame: number(frame)? });
        }
        if let Some(rest) = name.strip_prefix("c2_fade_") {
            return Some(Target::Fade(usize::try_from(number(rest)?).ok()?));
        }
        for (c, colour) in ["red", "green", "blue"].iter().enumerate() {
            if let Some(rest) = name.strip_prefix(&format!("c2_{colour}_")) {
                return Some(Target::Colour(usize::try_from(number(rest)?).ok()?, c));
            }
        }
        match name {
            "c2_visible" => Some(Target::Visible(50)),
            "c2_visible_51" => Some(Target::Visible(51)),
            _ => None,
        }
    }
}

/// The morph targets of a frame's mesh for its motion sets' vertex, material and visibility lists.
struct MeshMotion<'a> {
    frame: usize,
    sets: &'a [SetMotion],
    targets: Vec<Target>,
    names: Vec<String>,
}

impl<'a> MeshMotion<'a> {
    fn of(frame: usize, t: &mds::Mdt, sets: &'a [SetMotion], primary: usize) -> Option<MeshMotion<'a>> {
        let mut targets = Vec::new();
        let mut carriers = std::collections::BTreeSet::new();
        for (s, set) in sets.iter().enumerate() {
            let lists = set.lists.iter().filter(|l| l.frame as i64 == frame as i64 && !l.keys.is_empty());
            let mut vertices = false;
            for list in lists {
                let k = usize::try_from(list.target).ok().filter(|&k| k < t.materials.len());
                match (list.kind, k) {
                    (12, _) => vertices = true,
                    (40, Some(k)) => {
                        carriers.insert(Target::Fade(k));
                    }
                    (41, Some(k)) => {
                        carriers.extend((0..3).map(|c| Target::Colour(k, c)));
                    }
                    (50 | 51, _) => {
                        carriers.insert(Target::Visible(list.kind));
                    }
                    _ => {}
                }
            }
            if vertices {
                targets.extend(set.key_frames().into_iter().map(|frame| Target::Frame { set: s, frame }));
            }
        }
        targets.extend(carriers);
        if targets.is_empty() {
            return None;
        }
        let ids: Vec<String> = sets.iter().map(|set| py_str(&set.id)).collect();
        let names = targets.iter().map(|target| target.name(&ids, primary)).collect();
        Some(MeshMotion { frame, sets, targets, names })
    }

    /// Whether a motion set has lists for these targets.
    fn animated_by(&self, set: usize) -> bool {
        self.sets[set].lists.iter().any(|l| {
            l.frame as i64 == self.frame as i64 && matches!(l.kind, 12 | 40 | 41 | 50 | 51) && !l.keys.is_empty()
        })
    }

    /// Each target's weight at rest: no vertex frame, the materials' own colours, shown.
    fn rest_weights(&self, t: &mds::Mdt) -> Vec<f64> {
        self.targets.iter().map(|target| self.rest(t, target)).collect()
    }

    fn rest(&self, t: &mds::Mdt, target: &Target) -> f64 {
        let diffuse = |k: usize, c: usize| t.materials.get(k).and_then(|m| m.diffuse.get(c)).copied().unwrap_or(1.0);
        match *target {
            Target::Frame { .. } => 0.0,
            Target::Fade(k) => 1.0 - diffuse(k, 3),
            Target::Colour(k, c) => diffuse(k, c),
            Target::Visible(_) => 1.0,
        }
    }

    /// Each target's weight at a frame of a motion set.
    fn weights(&self, t: &mds::Mdt, set: usize, f: i64) -> Vec<f64> {
        let motion = &self.sets[set];
        self.targets
            .iter()
            .map(|target| match *target {
                Target::Frame { set: s, frame } => f64::from(u8::from(s == set && frame == f)),
                Target::Fade(k) => {
                    motion.list(self.frame, 40, k as i32).map_or(self.rest(t, target), |l| lerped_value(l, f)[0])
                }
                Target::Colour(k, c) => {
                    motion.list(self.frame, 41, k as i32).map_or(self.rest(t, target), |l| lerped_value(l, f)[c])
                }
                Target::Visible(kind) => motion.list(self.frame, kind, 0).map_or(1.0, |l| held_value(l, f)[0]),
            })
            .collect()
    }

    /// The targets of a primitive whose corners are `corners` (vertex, normal, uv, colour numbers):
    /// each vertex frame's displacement of the vertices its lists move (by the bind matrix for a
    /// skinned mesh), the carriers' none.
    fn displacements(
        &self,
        g: &mut Gltf,
        t: &mds::Mdt,
        corners: &[[i64; 4]],
        bind: Option<&Matrix>,
    ) -> Result<Vec<Value>> {
        let place = |v: &[f64]| -> [f64; 3] {
            match bind {
                Some(bind) => xform_point(bind, v),
                None => [v[0], v[1], v[2]],
            }
        };
        let mut zero: Option<usize> = None;
        let mut out = Vec::with_capacity(self.targets.len());
        for target in &self.targets {
            let Target::Frame { set, frame } = *target else {
                let at = match zero {
                    Some(at) => at,
                    None => {
                        let values = vec![vec![0.0; 3]; corners.len()];
                        let at = g.floats(&values, "VEC3", Some(ARRAY_BUFFER), true)?;
                        zero = Some(at);
                        at
                    }
                };
                out.push(object! {"POSITION" => at});
                continue;
            };
            // the vertices this frame moves: vertex index (target - 1) -> position
            let mut moved: std::collections::HashMap<i64, [f64; 4]> = Default::default();
            for list in &self.sets[set].lists {
                if list.frame as i64 == self.frame as i64 && list.kind == 12 && !list.keys.is_empty() {
                    moved.insert(list.target as i64 - 1, lerped_value(list, frame));
                }
            }
            let mut values = Vec::with_capacity(corners.len());
            for k in corners {
                let base = py_index(&t.vertices, k[0])?;
                let index = if k[0] < 0 { t.vertices.len() as i64 + k[0] } else { k[0] };
                let delta = match moved.get(&index) {
                    Some(p) => {
                        let (a, b) = (place(&base[..3]), place(&p[..3]));
                        vec![b[0] - a[0], b[1] - a[1], b[2] - a[2]]
                    }
                    None => vec![0.0; 3],
                };
                values.push(delta);
            }
            out.push(object! {"POSITION" => g.floats(&values, "VEC3", Some(ARRAY_BUFFER), true)?});
        }
        Ok(out)
    }
}

/// export with the character's parts (info, weights, pose, motion) and the model to write.
fn export_parts(
    character: &Character,
    model: &Mds,
    out: &mut Folder,
    base: &str,
    fps: f64,
    options: &Options,
) -> Result<Exported> {
    let info = &character.info;
    let deform: Vec<&str> = info.vertex_anime.iter().map(String::as_str).collect();
    let mut missing_textures: Vec<String> = Vec::new();
    let mut no_image_textures: Vec<String> = Vec::new();
    let mut extensions_used: Vec<&str> = Vec::new();
    let flags_of = if options.blend_flags { FrameFlags::of_model(model) } else { Vec::new() };
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

    let full_names = options.full_names;
    fn name_of(node: &mds::Node, full: bool) -> &str {
        if full {
            node.name.as_str()
        } else {
            node.base_name()
        }
    }
    let name_of = |node| name_of(node, full_names);
    for (i, node) in nodes.iter().enumerate() {
        let (t, r, s) = decompose(&node.matrix)?;
        let name = if name_of(node).is_empty() { format!("node{i}") } else { name_of(node).to_string() };
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

    let primary = primary_set(&character.motions.iter().map(|m| &m.id).collect::<Vec<_>>());
    let mut texture_index: Vec<(String, usize)> = Vec::new();
    let mut material_index: Vec<(MaterialKey, usize)> = Vec::new();
    // the morph targets of each frame's mesh
    let mut mesh_motions: std::collections::BTreeMap<usize, (usize, MeshMotion)> = Default::default();
    // per frame: its own flag text, and its ancestors' flag texts that push flags down ('.')
    let flag_text = |node: &mds::Node| node.name.split_once("--").map_or("", |(_, flags)| flags).to_string();
    let mut inherited_text: Vec<String> = Vec::with_capacity(nodes.len());
    for node in nodes {
        let up = usize::try_from(node.parent).ok().filter(|&p| p < inherited_text.len());
        let mut text = up.map(|p| inherited_text[p].clone()).unwrap_or_default();
        if let Some(p) = up {
            let parent_flags = flag_text(&nodes[p]);
            if parent_flags.contains('.') {
                if !text.is_empty() {
                    text.push(' ');
                }
                text.push_str(&parent_flags);
            }
        }
        inherited_text.push(text);
    }
    // a texture name the file system cannot hold fails the export, as writing its file does
    let mut failed: Option<String> = None;
    let mut material = |g: &mut Gltf,
                        out: &mut Folder,
                        m: &Material,
                        origin: (usize, usize),
                        textured: bool|
     -> Result<usize> {
        let flags = flags_of.get(origin.0);
        let flag_key = match flags {
            Some(_) => format!("{}|{}", flag_text(&nodes[origin.0]), inherited_text[origin.0]),
            None => String::new(),
        };
        let key = MaterialKey {
            texture: if textured { m.texture.clone() } else { String::new() },
            diffuse: m.diffuse.clone(),
            ambient: m.ambient.clone(),
            origin,
            flags: flag_key,
        };
        if let Some((_, index)) = material_index.iter().find(|(k, _)| k.same(&key)) {
            return Ok(*index);
        }
        let mut extras = object! {
            "mdt_diffuse" => Value::floats(&m.diffuse),
            "mdt_ambient" => Value::floats(&m.ambient),
            "mdt_specular" => Value::floats(&m.specular),
            "mdt_power" => m.power,
            "texture" => m.texture.as_str(),
        };
        if options.collision {
            // untextured, named and coloured for the attributes the diffuse colour encodes
            let attributes = CollisionAttributes::decode(&m.diffuse);
            extras.set(
                "collision",
                object! {
                    "ground" => attributes.ground,
                    "foot" => attributes.foot,
                    "area" => attributes.area,
                    "mask" => attributes.mask,
                },
            );
            let pbr = object! {
                "baseColorFactor" => Value::floats(&attributes.display_colour()),
                "metallicFactor" => 0.0,
                "roughnessFactor" => 1.0,
            };
            g.materials.push(object! {"name" => attributes.name(), "pbrMetallicRoughness" => pbr, "extras" => extras});
            material_index.push((key, g.materials.len() - 1));
            return Ok(g.materials.len() - 1);
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
                    let mut texture_extras = None;
                    let sampler = match options.textures {
                        None => {
                            out.write(&format!("textures/{file}"), placeholder_png(&m.texture));
                            None
                        }
                        Some(search) => match (search.png(&m.texture)?, search.find(&m.texture)) {
                            (png, Some((img, image))) => {
                                let mut e = object! {"img" => img};
                                if let Some(clamp) = image.clamp {
                                    e.set("gs_clamp", Value::Int(clamp as i64));
                                }
                                match png {
                                    Some(png) => out.write(&format!("textures/{file}"), png),
                                    None => {
                                        // registered without pixels: a texture animation fills it
                                        out.write(&format!("textures/{file}"), placeholder_png(&m.texture));
                                        e.set("no_image", Value::Bool(true));
                                        if !no_image_textures.contains(&m.texture) {
                                            no_image_textures.push(m.texture.clone());
                                        }
                                    }
                                }
                                texture_extras = Some(e);
                                Some(wrap_of(image.clamp))
                            }
                            (_, None) => {
                                out.write(&format!("textures/{file}"), placeholder_png(&m.texture));
                                if !missing_textures.contains(&m.texture) {
                                    missing_textures.push(m.texture.clone());
                                }
                                texture_extras = Some(object! {"missing" => true});
                                Some((REPEAT, REPEAT))
                            }
                        },
                    };
                    let sampler_index = match sampler {
                        None => {
                            if g.samplers.is_empty() {
                                g.samplers.push(object! {"magFilter" => 9729i64, "minFilter" => 9729i64});
                            }
                            0
                        }
                        Some((s, t)) => {
                            let wanted =
                                object! {"magFilter" => 9729i64, "minFilter" => 9729i64, "wrapS" => s, "wrapT" => t};
                            let text = wanted.sorted_text();
                            match g.samplers.iter().position(|known| known.sorted_text() == text) {
                                Some(at) => at,
                                None => {
                                    g.samplers.push(wanted);
                                    g.samplers.len() - 1
                                }
                            }
                        }
                    };
                    g.images.push(object! {"uri" => format!("textures/{file}"), "name" => m.texture.as_str()});
                    let mut texture = object! {
                        "source" => g.images.len() - 1,
                        "sampler" => sampler_index,
                        "name" => m.texture.as_str(),
                    };
                    if let Some(e) = texture_extras {
                        texture.set("extras", e);
                    }
                    g.textures.push(texture);
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
        let alpha = m.diffuse.get(3).copied().unwrap_or(1.0);
        let mode = match flags {
            Some(flags) => {
                extras.set("frame_flags", flags.extras(&flag_text(&nodes[origin.0]), &inherited_text[origin.0]));
                flags.alpha_mode(alpha)
            }
            None => (alpha < 1.0).then_some(("BLEND", None)),
        };
        let mut gm = object! {"name" => name, "pbrMetallicRoughness" => pbr, "extras" => extras};
        if let Some((mode, cutoff)) = mode {
            gm.set("alphaMode", Value::from(mode));
            if let Some(cutoff) = cutoff {
                gm.set("alphaCutoff", Value::Float(cutoff));
            }
        }
        if flags.is_some_and(|f| f.lit == Some(false)) {
            gm.set("extensions", object! {"KHR_materials_unlit" => Value::object()});
            if !extensions_used.contains(&"KHR_materials_unlit") {
                extensions_used.push("KHR_materials_unlit");
            }
        }
        g.materials.push(gm);
        material_index.push((key, g.materials.len() - 1));
        Ok(g.materials.len() - 1)
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
        let morph = MeshMotion::of(i, t, &character.motions, primary);
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
                    Value::from(material(&mut g, out, &t.materials[*mat as usize], (i, *mat as usize), *hu)?),
                );
            }
            if let Some(morph) = &morph {
                let bind = skinned.then_some(&bind);
                let targets = morph.displacements(&mut g, t, vs, bind)?;
                gp.set("targets", Value::List(targets));
            }
            prims.push(gp);
        }
        if prims.is_empty() {
            continue;
        }
        let mut mesh_extras = object! {"mdt_style" => t.style.to_string()};
        if options.full_names {
            mesh_extras.set("mds_index", Value::from(i));
        }
        let mut gmesh = object! {
            "name" => name_of(node),
            "primitives" => Value::List(prims),
            "extras" => mesh_extras,
        };
        if let Some(morph) = &morph {
            gmesh.set("weights", Value::floats(&morph.rest_weights(t)));
            if let Some(extras) = gmesh.get_mut("extras") {
                extras.set("targetNames", Value::List(morph.names.iter().map(|n| Value::from(n.as_str())).collect()));
            }
        }
        g.meshes.push(gmesh);
        let mesh = g.meshes.len() - 1;
        if skinned {
            any_skinned = true;
            g.nodes.push(object! {
                "name" => format!("{}_skin", name_of(node)),
                "mesh" => mesh,
                "skin" => 0i64,
                "extras" => object! {"mds_index" => i, "skinned_mesh_of" => node.name.as_str()},
            });
            g.scene_nodes.push(g.nodes.len() - 1);
        } else {
            g.nodes[i].set("mesh", Value::from(mesh));
        }
        if let Some(morph) = morph {
            let node = if skinned { g.nodes.len() - 1 } else { i };
            mesh_motions.insert(i, (node, morph));
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

    for (set, motion) in character.motions.iter().enumerate() {
        if motion.lists.is_empty() || motion.keys.is_empty() {
            continue;
        }
        // a list drives the frame its number gives (MotionProc's GetFrame), whatever its name
        let mut channels_of: std::collections::BTreeMap<(usize, &str), &KeyList> = Default::default();
        for list in &motion.lists {
            let Some(frame) = usize::try_from(list.frame).ok().filter(|&f| f < nodes.len()) else {
                continue;
            };
            if !(0..=2).contains(&list.kind) || list.keys.is_empty() {
                continue;
            }
            let path = ["rotation", "scale", "translation"][list.kind as usize];
            channels_of.insert((frame, path), list);
        }
        let mut seen: Vec<Arg> = Vec::new();
        for clip in &motion.keys {
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
            let name = match name {
                Arg::Str(text) => Arg::Str(set_animation_name(&motion.id, set == primary, &text)),
                other if set == primary => other,
                other => Arg::Str(set_animation_name(&motion.id, false, &py_str(&other))),
            };
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
            // the meshes' morph weights: vertex frames and the carriers of the other lists
            for (&frame, (node, morph)) in &mesh_motions {
                let Some(t) = &nodes[frame].mdt else {
                    continue;
                };
                if !morph.animated_by(set) {
                    continue;
                }
                let weights: Vec<Vec<f64>> =
                    frames.iter().flat_map(|&f| morph.weights(t, set, f)).map(|w| vec![w]).collect();
                let output = g.floats(&weights, "SCALAR", None, false)?;
                samplers.push(object! {"input" => times, "output" => output, "interpolation" => "LINEAR"});
                channels.push(object! {
                    "sampler" => samplers.len() - 1,
                    "target" => object! {"node" => *node, "path" => "weights"},
                });
            }
            let mut extras =
                object! {"start" => clip.start.clone(), "end" => clip.end.clone(), "step" => clip.step.clone()};
            if set != primary {
                extras.set("motion_set", motion.id.clone());
            }
            g.animations.push(object! {
                "name" => name,
                "samplers" => Value::List(samplers),
                "channels" => Value::List(channels),
                "extras" => extras,
            });
        }
    }
    let character_export =
        !(options.full_names || options.textures.is_some() || options.blend_flags || options.collision);
    let extras = if character_export {
        object! {
            "source" => object! {"base" => "pack"},
            "fps_assumed" => fps,
            "note" => "textures are placeholders; MDS node i = glTF node i",
        }
    } else {
        let mut extras = object! {
            "source" => object! {"base" => "mds"},
            "note" => "MDS node i = glTF node i (extras.mds_index); names are the MDS names whole",
        };
        if options.collision {
            extras.set("collision", Value::Bool(true));
        }
        extras
    };
    let asset = object! {"version" => "2.0", "generator" => "ChronicleTwo-Script c2", "extras" => extras};
    let (mut json, bin) = g.json(asset, base);
    if !extensions_used.is_empty() {
        if let Value::Object(items) = &mut json {
            let used = Value::List(extensions_used.iter().map(|&e| Value::from(e)).collect());
            items.insert(1, ("extensionsUsed".to_string(), used));
        }
    }
    out.write(&format!("{base}.bin"), bin);
    out.write(&format!("{base}.gltf"), crate::json::dumps_compact(&json).into_bytes());
    Ok(Exported { missing_textures, no_image_textures })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flags(names: &[(&str, i32)]) -> Vec<FrameFlags> {
        let mut model = Mds::new();
        for (name, parent) in names {
            model.nodes.push(mds::Node::new(name, *parent));
        }
        FrameFlags::of_model(&model)
    }

    #[test]
    fn name_flags_read_as_the_engine_reads_them() {
        let found = flags(&[("glow--n1z0c0f2app-", -1), ("leaf--a40aof-", -1), ("plain", -1), ("x--aNN", -1)]);
        assert_eq!(
            found[0],
            FrameFlags {
                blend: Some(Blend::Add),
                alpha_ref: None,
                lit: Some(false),
                z_write: Some(false),
                fog: Some(2)
            }
        );
        assert_eq!(found[1], FrameFlags { blend: Some(Blend::Opaque), alpha_ref: Some(0x40), ..FrameFlags::default() });
        assert_eq!(found[2], FrameFlags::default());
        assert_eq!(found[3].blend, Some(Blend::Subtract));
        assert_eq!(found[0].alpha_mode(1.0), Some(("BLEND", None)));
        assert_eq!(found[1].alpha_mode(1.0), Some(("MASK", Some(0.5))));
        assert_eq!(found[2].alpha_mode(0.5), Some(("BLEND", None)));
        assert_eq!(found[2].alpha_mode(1.0), None);
        // a '.' pushes the flag down the subtree; a child's own flag wins for itself only
        let tree = flags(&[("top--.c0z0", -1), ("mid--c1", 0), ("leaf", 1)]);
        assert_eq!((tree[0].lit, tree[0].z_write), (Some(false), Some(false)));
        assert_eq!((tree[1].lit, tree[1].z_write), (Some(true), None));
        assert_eq!((tree[2].lit, tree[2].z_write), (Some(false), None));
    }

    #[test]
    fn collision_attributes_encode_and_decode_in_float32() {
        for k in 0..=255 {
            let a = CollisionAttributes { ground: k, foot: k, area: k, mask: k };
            assert_eq!(CollisionAttributes::decode(&a.encode()), a);
            assert_eq!(CollisionAttributes::parse(&a.name()), Some(a));
        }
        // as the disc stores them: zero as 1.0, foot sound 2 as 2.8571, area 6, mask 12
        let disc = CollisionAttributes::decode(&[1.0, 2.8571, 8.5714, -11.0]);
        assert_eq!(disc, CollisionAttributes { ground: 0, foot: 2, area: 6, mask: 12 });
        assert_eq!(disc.name(), "col_ground0_foot2_area6_mask12");
        assert_eq!(CollisionAttributes::parse("col_ground0_foot2_area6_mask12.004"), Some(disc));
        assert_eq!(
            CollisionAttributes::parse("col_foot3"),
            Some(CollisionAttributes { foot: 3, ..Default::default() })
        );
        for bad in ["wall", "col_foot", "col_feet2", "col_foot2_foot3"] {
            assert_eq!(CollisionAttributes::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn gs_clamp_gives_the_wrap() {
        assert_eq!(wrap_of(Some(5)), (CLAMP_TO_EDGE, CLAMP_TO_EDGE));
        assert_eq!(wrap_of(Some(0)), (REPEAT, REPEAT));
        assert_eq!(wrap_of(Some(1)), (CLAMP_TO_EDGE, REPEAT));
        assert_eq!(wrap_of(None), (REPEAT, REPEAT));
    }
}
