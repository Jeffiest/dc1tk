//! Character motions rebuilt from glTF animations. The export writes one animation per KEY of
//! motion set 0, sampled every motion frame (`FPS` frames a second); build reads the animations
//! back and writes into the pack's .mot and info.cfg only those that differ from the export by
//! value (a tool that resamples or adds float noise changes nothing), and those the export does not
//! have:
//! - an edited motion of the same length is written over its own frames of the .mot timeline, its
//!   KEY unchanged, when no other KEY shares a frame it changes;
//! - otherwise (a retimed motion, or one whose frames another KEY uses) its frames are added at
//!   the end of the timeline and its KEY points there;
//! - a new animation is added at the end of the timeline as a new KEY, after the motion set's last.
//!
//! Every key list gets values at every frame a motion is written to: from the animation (or the
//! node's rest when it does not animate it) for rotation, scale and translation, and from the
//! original motion (or the one an animation copied, "walk.001" from "walk") for the channels glTF
//! has no place for (vertex positions, material alpha and colour, visibility). Lists the animations
//! do not change keep their bytes.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::from_gltf::{accessor, clean, index, item, Gltf};
use crate::info_cfg::{self, Statement};
use crate::json::Value;
use crate::mds::Mds;
use crate::mot::{self, KeyList};
use crate::to_gltf::{decompose, slerp};
use crate::{bail, Result};

/// Motion frames a second at step 1.0: the PAL game advances a KEY by 1.2 × step frames each
/// 25 Hz game frame (CCharacter2::NormalDrive), the NTSC one by step each 30 Hz frame.
pub const FPS: f64 = 30.0;

/// The channels a key list of each kind (0, 1, 2) drives.
const PATHS: [&str; 3] = ["rotation", "scale", "translation"];
const ROTATION: usize = 0;

/// Largest rotation (radians) and translation or scale differences that are no edit: Blender's
/// round trip moves rotations by up to 7e-4 and translations by a few millionths.
const ROTATION_TOLERANCE: f64 = 3e-3;
const DISTANCE_TOLERANCE: f64 = 1e-3;
const RELATIVE_TOLERANCE: f64 = 2e-5;

/// The longest KEY name: CHRINFO_KEY_SET's char[0x24] with its NUL.
const KEY_NAME: usize = 35;

type Vec4 = [f64; 4];

#[derive(Clone, Copy, PartialEq)]
enum Interpolation {
    Linear,
    Step,
    Cubic,
}

/// One animation channel: key times, values (vec3 padded with 0, or a quaternion x, y, z, w) and,
/// for cubic splines, each key's in and out tangents.
struct Curve {
    times: Vec<f64>,
    values: Vec<Vec4>,
    tangents: Vec<(Vec4, Vec4)>,
    interpolation: Interpolation,
}

fn lerp(a: &Vec4, b: &Vec4, t: f64) -> Vec4 {
    [0, 1, 2, 3].map(|i| a[i] + (b[i] - a[i]) * t)
}

fn normalized(q: &Vec4) -> Vec4 {
    let length = q.iter().map(|x| x * x).sum::<f64>().sqrt();
    if length > 1e-12 {
        q.map(|x| x / length)
    } else {
        [0.0, 0.0, 0.0, 1.0]
    }
}

fn quat_slerp(a: &Vec4, b: &Vec4, t: f64) -> Vec4 {
    match slerp(a, b, t) {
        Ok(q) if q.len() == 4 => [q[0], q[1], q[2], q[3]],
        _ => normalized(&lerp(a, b, t)),
    }
}

impl Curve {
    /// The value at time t, held before the first key and after the last.
    fn at(&self, t: f64, rotation: bool) -> Vec4 {
        let n = self.times.len();
        if n == 1 || t <= self.times[0] {
            return self.values[0];
        }
        if t >= self.times[n - 1] {
            return self.values[n - 1];
        }
        let k = self.times.partition_point(|&x| x <= t) - 1;
        let dt = self.times[k + 1] - self.times[k];
        let u = if dt > 0.0 { (t - self.times[k]) / dt } else { 0.0 };
        let (a, b) = (&self.values[k], &self.values[k + 1]);
        match self.interpolation {
            Interpolation::Step => *a,
            Interpolation::Linear if rotation => quat_slerp(a, b, u),
            Interpolation::Linear => lerp(a, b, u),
            Interpolation::Cubic => {
                let (u2, u3) = (u * u, u * u * u);
                let out = &self.tangents[k].1;
                let into = &self.tangents[k + 1].0;
                let value = [0, 1, 2, 3].map(|i| {
                    (2.0 * u3 - 3.0 * u2 + 1.0) * a[i]
                        + (u3 - 2.0 * u2 + u) * dt * out[i]
                        + (-2.0 * u3 + 3.0 * u2) * b[i]
                        + (u3 - u2) * dt * into[i]
                });
                if rotation {
                    normalized(&value)
                } else {
                    value
                }
            }
        }
    }
}

/// An animation's channels by MDS frame and kind (0 rotation, 1 scale, 2 translation).
struct Animation {
    name: String,
    extras: Option<Value>,
    curves: BTreeMap<(usize, usize), Curve>,
    /// The times of the first and last key of any channel: the motion runs between them.
    start: f64,
    end: f64,
}

impl Animation {
    /// Its length in motion frames.
    fn frames(&self) -> i64 {
        ((self.end - self.start) * FPS).round() as i64
    }
}

/// What a glTF says about a model's motion: the rest of the node standing for each frame (rotation,
/// scale, translation) and the animations.
struct Motions {
    rest: Vec<Option<[Vec4; 3]>>,
    animations: Vec<Animation>,
}

impl Motions {
    /// The value an animation gives a frame's channel t seconds into it: its curve's, or the node's
    /// rest.
    fn value(&self, animation: &Animation, frame: usize, kind: usize, t: f64) -> Option<Vec4> {
        match animation.curves.get(&(frame, kind)) {
            Some(curve) => Some(curve.at(animation.start + t, kind == ROTATION)),
            None => self.rest.get(frame).copied().flatten().map(|rest| rest[kind]),
        }
    }
}

/// The frame each glTF node stands for, for motions: its `extras.mds_index`, or else by its name (a
/// frame's whole name, or its name without flags) among the frames under the frame its parent
/// stands for; nodes of one name under one parent take that name's frames in order, by Blender's
/// ".NNN" (which numbers them in the order it made them) and then in glTF order. Skinned mesh
/// nodes stand for none.
fn node_frames(json: &Value, model: &Mds) -> Result<Vec<Option<usize>>> {
    let empty = Vec::new();
    let nodes = json.get("nodes").and_then(Value::as_list).unwrap_or(&empty);
    let mut whole: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut base: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, node) in model.nodes.iter().enumerate() {
        whole.entry(node.name.as_str()).or_default().push(i);
        base.entry(node.base_name()).or_default().push(i);
    }
    let children = |gi: usize| -> Vec<usize> {
        let list = nodes[gi].get("children").and_then(Value::as_list).unwrap_or(&empty);
        list.iter()
            .filter_map(|c| c.as_int().and_then(|c| usize::try_from(c).ok()))
            .filter(|&c| c < nodes.len())
            .collect()
    };
    let mut parent: Vec<Option<usize>> = vec![None; nodes.len()];
    for gi in 0..nodes.len() {
        for c in children(gi) {
            parent[c] = Some(gi);
        }
    }
    // each node's name without the suffix, and its place among the nodes of that name beside it
    let mut stems: Vec<Option<String>> = Vec::with_capacity(nodes.len());
    // (parent, name, .NNN, node) of every named node
    let mut named: Vec<(Option<usize>, String, usize, usize)> = Vec::new();
    for (gi, n) in nodes.iter().enumerate() {
        let stem = match n.get("name").and_then(Value::as_str) {
            Some(name) if !n.has("skin") => {
                let stem = clean(n.get("name"))?;
                let number = if stem.len() < name.len() { name[stem.len() + 1..].parse().unwrap_or(0) } else { 0 };
                named.push((parent[gi], stem.clone(), number, gi));
                Some(stem)
            }
            _ => None,
        };
        stems.push(stem);
    }
    named.sort();
    let mut rank = vec![0; nodes.len()];
    for (k, (p, stem, _, gi)) in named.iter().enumerate() {
        if let Some((q, other, _, before)) = k.checked_sub(1).map(|k| &named[k]) {
            if (q, other) == (p, stem) {
                rank[*gi] = rank[*before] + 1;
            }
        }
    }
    // parents before their children
    let mut order = Vec::with_capacity(nodes.len());
    let mut pending: Vec<usize> = (0..nodes.len()).rev().filter(|&gi| parent[gi].is_none()).collect();
    let mut reached = vec![false; nodes.len()];
    while let Some(gi) = pending.pop() {
        if !std::mem::replace(&mut reached[gi], true) {
            order.push(gi);
            pending.extend(children(gi).into_iter().rev());
        }
    }
    let mut out = vec![None; nodes.len()];
    for gi in order {
        let n = &nodes[gi];
        let index = n.get("extras").and_then(|e| e.get("mds_index")).and_then(Value::as_int);
        if let Some(i) = index.and_then(|i| usize::try_from(i).ok()).filter(|&i| i < model.nodes.len()) {
            out[gi] = (!n.has("skin")).then_some(i);
            continue;
        }
        let Some(stem) = &stems[gi] else {
            continue;
        };
        // a name with flags is a frame's whole name; one without, the name of every frame of it
        let frames = match stem.split_once("--") {
            Some((flagless, _)) => whole.get(stem.as_str()).or_else(|| base.get(flagless)),
            None => base.get(stem.as_str()),
        };
        let Some(frames) = frames else {
            continue;
        };
        let under = parent[gi].and_then(|p| out[p]);
        let beside: Vec<usize> =
            frames.iter().copied().filter(|&f| under.is_some_and(|p| model.nodes[f].parent == p as i32)).collect();
        let pool = if beside.is_empty() { frames } else { &beside };
        out[gi] = Some(pool.get(rank[gi]).copied().unwrap_or(pool[0]));
    }
    Ok(out)
}

fn floats_of(value: Option<&Value>, default: &[f64]) -> Vec<f64> {
    match value.and_then(Value::as_list) {
        Some(items) => items.iter().map(|v| v.as_f64().unwrap_or(0.0)).collect(),
        None => default.to_vec(),
    }
}

/// A node's local rotation, scale and translation.
fn node_rest(node: &Value) -> Result<[Vec4; 3]> {
    if let Some(matrix) = node.get("matrix") {
        let m = floats_of(Some(matrix), &[]);
        if m.len() < 16 {
            bail!("a node's matrix has {} numbers", m.len());
        }
        let (t, r, s) = decompose(&std::array::from_fn(|i| m[i]))?;
        return Ok([[r[0], r[1], r[2], r[3]], [s[0], s[1], s[2], 0.0], [t[0], t[1], t[2], 0.0]]);
    }
    let r = floats_of(node.get("rotation"), &[0.0, 0.0, 0.0, 1.0]);
    let s = floats_of(node.get("scale"), &[1.0, 1.0, 1.0]);
    let t = floats_of(node.get("translation"), &[0.0, 0.0, 0.0]);
    if r.len() != 4 || s.len() != 3 || t.len() != 3 {
        bail!("a node's rotation, scale or translation has the wrong number of values");
    }
    Ok([normalized(&[r[0], r[1], r[2], r[3]]), [s[0], s[1], s[2], 0.0], [t[0], t[1], t[2], 0.0]])
}

fn vec4(values: &[crate::py::Num]) -> Vec4 {
    std::array::from_fn(|i| values.get(i).map_or(0.0, |v| v.f()))
}

/// The glTF's rests and animations by MDS frame. A frame several nodes stand for (Blender's bone
/// and the mesh object on it) takes the rest and channels of the one that is not a mesh.
fn read(gltf: &Gltf, model: &Mds) -> Result<Motions> {
    let j = &gltf.json;
    let frames = node_frames(j, model)?;
    let empty = Vec::new();
    let nodes = j.get("nodes").and_then(Value::as_list).unwrap_or(&empty);
    let mut chosen: Vec<Option<usize>> = vec![None; model.nodes.len()];
    for (gi, frame) in frames.iter().enumerate() {
        if let Some(f) = *frame {
            let better = match chosen[f] {
                None => true,
                Some(other) => nodes[other].has("mesh") && !nodes[gi].has("mesh"),
            };
            if better {
                chosen[f] = Some(gi);
            }
        }
    }
    let rest = chosen.iter().map(|gi| gi.map(|gi| node_rest(&nodes[gi])).transpose()).collect::<Result<_>>()?;
    let mut animations = Vec::new();
    for (number, a) in j.get("animations").and_then(Value::as_list).unwrap_or(&empty).iter().enumerate() {
        let name = a.get("name").and_then(Value::as_str).map_or(format!("animation{number}"), str::to_string);
        let samplers = item(a, "samplers")?;
        let mut curves = BTreeMap::new();
        let (mut start, mut end) = (f64::INFINITY, 0.0f64);
        for channel in item(a, "channels")?.as_list().unwrap_or(&empty) {
            let target = item(channel, "target")?;
            let Some(kind) =
                target.get("path").and_then(Value::as_str).and_then(|p| PATHS.iter().position(|q| *q == p))
            else {
                continue;
            };
            let Some(gi) = target.get("node").and_then(Value::as_int).and_then(|n| usize::try_from(n).ok()) else {
                continue;
            };
            let Some(frame) = frames.get(gi).copied().flatten() else {
                continue;
            };
            if chosen[frame] != Some(gi) {
                continue;
            }
            let sampler = index(samplers, item(channel, "sampler")?)?;
            let times: Vec<f64> = accessor(gltf, item(sampler, "input")?)?.values.iter().map(|v| v.f()).collect();
            let output = accessor(gltf, item(sampler, "output")?)?;
            let interpolation = match sampler.get("interpolation").and_then(Value::as_str) {
                Some("STEP") => Interpolation::Step,
                Some("CUBICSPLINE") => Interpolation::Cubic,
                _ => Interpolation::Linear,
            };
            let all: Vec<Vec4> = (0..output.len()).map(|k| output.at(k as i64).map(vec4)).collect::<Result<_>>()?;
            let (values, tangents) = if interpolation == Interpolation::Cubic {
                if all.len() != 3 * times.len() {
                    bail!("animation {name}: a cubic spline has {} values for {} keys", all.len(), times.len());
                }
                (all.chunks(3).map(|c| c[1]).collect(), all.chunks(3).map(|c| (c[0], c[2])).collect())
            } else {
                (all, Vec::new())
            };
            if times.is_empty() || values.len() != times.len() {
                bail!("animation {name}: a channel has {} keys and {} values", times.len(), values.len());
            }
            let values = if kind == ROTATION { values.iter().map(normalized).collect() } else { values };
            start = start.min(times[0]);
            end = end.max(times[times.len() - 1]);
            curves.insert((frame, kind), Curve { times, values, tangents, interpolation });
        }
        let start = if start.is_finite() { start.min(end) } else { 0.0 };
        animations.push(Animation { name, extras: a.get("extras").cloned(), curves, start, end });
    }
    Ok(Motions { rest, animations })
}

/// Whether two values of a kind are the same within what a round trip through another tool moves.
fn close(kind: usize, a: &Vec4, b: &Vec4) -> bool {
    if kind == ROTATION {
        let dot = a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>().abs();
        return 2.0 * dot.min(1.0).acos() <= ROTATION_TOLERANCE;
    }
    (0..3).all(|i| (a[i] - b[i]).abs() <= DISTANCE_TOLERANCE + RELATIVE_TOLERANCE * a[i].abs().max(b[i].abs()))
}

/// A glTF value as a .mot key: rotations as the engine's (w, x, y, z) of the conjugate (QuatToMat
/// builds the transpose), translations and scales with w 0.
fn engine_key(kind: usize, v: &Vec4) -> Vec4 {
    if kind == ROTATION {
        let q = normalized(v);
        [q[3], -q[0], -q[1], -q[2]]
    } else {
        [v[0], v[1], v[2], 0.0]
    }
}

/// A key list's value at a frame as MotionProc gives it: the key there, or between the keys
/// around it (slerp for rotations, held for visibility, lerp otherwise); the first key before the
/// first, the last after the last.
fn list_value(list: &KeyList, frame: i64) -> Vec4 {
    let keys = &list.keys;
    let after = keys.partition_point(|(f, _)| *f <= frame);
    if after == 0 {
        return keys[0].1;
    }
    let (f0, a) = keys[after - 1];
    if f0 == frame || after == keys.len() {
        return a;
    }
    let (f1, b) = keys[after];
    let t = (frame - f0) as f64 / (f1 - f0) as f64;
    match list.kind {
        0 => quat_slerp(&a, &b, t),
        50 | 51 => a,
        _ => lerp(&a, &b, t),
    }
}

/// A KEY of motion set 0 and its statement in the info.cfg.
struct Key {
    statement: Statement,
    range: Option<(i64, i64)>,
}

/// The KEY statements of motion set 0 (as CharInfo reads them: a MOTION 0 given again starts its
/// list again).
fn motion0_keys(info: &[u8]) -> Vec<Key> {
    let mut out = Vec::new();
    let mut in_set0 = false;
    for statement in info_cfg::statements(info) {
        match statement.tag.as_str() {
            "MOTION" if statement.args.len() >= 4 => {
                in_set0 = info_cfg::same_value(&statement.args[0].0, &Value::Int(0));
                if in_set0 {
                    out.clear();
                }
            }
            "KEY" if statement.args.len() >= 4 && in_set0 => {
                let range = match (&statement.args[1].0, &statement.args[2].0) {
                    (Value::Int(s), Value::Int(e)) => Some((*s, *e.max(s))),
                    _ => None,
                };
                out.push(Key { statement, range });
            }
            _ => {}
        }
    }
    out
}

/// What became of a motion.
enum Written {
    /// Written over its own frames.
    InPlace,
    /// Added at the end of the timeline at these frames.
    Moved(i64, i64),
}

/// The .mot, info.cfg and the names of the motions a glTF's animations changed or added.
pub struct Rebuilt {
    pub mot: Vec<u8>,
    pub info: Vec<u8>,
    pub changed: Vec<String>,
}

/// One key list being rebuilt: its original keys and, once written to, its keys by frame.
struct Track {
    original: KeyList,
    keys: Option<BTreeMap<i64, Vec4>>,
    /// A list the animations add (a channel no list drives).
    new: bool,
}

impl Track {
    fn keys(&mut self) -> &mut BTreeMap<i64, Vec4> {
        let original = &self.original;
        self.keys.get_or_insert_with(|| original.keys.iter().copied().collect())
    }

    /// Writes a key, a rotation turned to the side of the key before it (or of the key it
    /// replaces) so that no step flips.
    fn set(&mut self, frame: i64, mut value: Vec4) {
        let rotation = self.original.kind == ROTATION as i32;
        let keys = self.keys();
        if rotation {
            let near = keys.get(&(frame - 1)).or_else(|| keys.get(&frame)).copied();
            if near.is_some_and(|n| n.iter().zip(&value).map(|(x, y)| x * y).sum::<f64>() < 0.0) {
                value = value.map(|x| -x);
            }
        }
        keys.insert(frame, value);
    }
}

/// The .mot and info.cfg with the glTF's animations that differ from the export's written in (see
/// the module), or None when none differs. `exported` is the pack's own export, `model` its MDS.
pub fn build(exported: &Gltf, given: &Gltf, model: &Mds, mot_data: &[u8], info: &[u8]) -> Result<Option<Rebuilt>> {
    let keys = motion0_keys(info);
    let before = read(exported, model)?;
    let after = read(given, model)?;
    if before.animations.len() != keys.len() && !before.animations.is_empty() {
        bail!("the export has {} animations for {} KEY lines", before.animations.len(), keys.len());
    }
    let mut seen = BTreeSet::new();
    for animation in &after.animations {
        if !seen.insert(animation.name.as_str()) {
            bail!("two animations are named {}", animation.name);
        }
    }
    let shared = |k: usize, frame: i64| {
        keys.iter()
            .enumerate()
            .any(|(other, key)| other != k && key.range.is_some_and(|(s, e)| s <= frame && frame <= e))
    };
    // A channel no list drives holds a rest: the frame's own (the export's node), or the given
    // node's, since Blender rests a bone at its bind pose (from the inverse bind matrices) and
    // writes that for every bone an action leaves alone.
    let at_rest = |g: &Animation, frame: usize, kind: usize, t: f64| {
        let Some(value) = after.value(g, frame, kind, t) else {
            return true;
        };
        let rests = [before.rest.get(frame), after.rest.get(frame)];
        rests.into_iter().flatten().flatten().any(|rest| close(kind, &rest[kind], &value))
    };
    // Whether an animation leaves a channel as the export has it at time t.
    let same_at = |e: &Animation, g: &Animation, frame: usize, kind: usize, t: f64| {
        if !e.curves.contains_key(&(frame, kind)) {
            return at_rest(g, frame, kind, t);
        }
        match (before.value(e, frame, kind, t), after.value(g, frame, kind, t)) {
            (Some(x), Some(y)) => close(kind, &x, &y),
            _ => true,
        }
    };
    // Whether an animation drives a channel: one a list drives, or one it moves off the rest.
    let listed: BTreeSet<(usize, usize)> = before.animations.iter().flat_map(|e| e.curves.keys().copied()).collect();
    let moves = |g: &Animation, frame: usize, kind: usize| {
        listed.contains(&(frame, kind)) || (0..=g.frames()).any(|r| !at_rest(g, frame, kind, r as f64 / FPS))
    };

    // which animations changed, and how
    struct Change<'a> {
        animation: &'a Animation,
        key: Option<usize>,
        frames: i64,
        retimed: bool,
        channels: BTreeSet<(usize, usize)>,
        in_place: bool,
        /// The KEY whose frames give the channels glTF has no place for.
        source: Option<usize>,
        /// The KEY a new animation is a copy of, by its name.
        copied: Option<usize>,
    }
    let mut changes: Vec<Change> = Vec::new();
    for g in &after.animations {
        match before.animations.iter().position(|e| e.name == g.name) {
            Some(k) => {
                let e = &before.animations[k];
                let Some((s, end)) = keys[k].range else {
                    continue;
                };
                let (old, new) = (end - s, g.frames());
                let mut channels = BTreeSet::new();
                for &(frame, kind) in e.curves.keys().chain(g.curves.keys()) {
                    if (0..=old).any(|r| !same_at(e, g, frame, kind, r as f64 / FPS)) {
                        channels.insert((frame, kind));
                    }
                }
                if new <= old && channels.is_empty() {
                    continue;
                }
                let retimed = new != old;
                if retimed {
                    channels.extend(g.curves.keys().filter(|&&(frame, kind)| moves(g, frame, kind)));
                }
                // in place unless a frame another KEY uses would change
                let in_place = !retimed
                    && (s..=end).filter(|&f| shared(k, f)).all(|f| {
                        channels.iter().all(|&(frame, kind)| same_at(e, g, frame, kind, (f - s) as f64 / FPS))
                    });
                changes.push(Change {
                    animation: g,
                    key: Some(k),
                    frames: new,
                    retimed,
                    channels,
                    in_place,
                    source: Some(k),
                    copied: None,
                });
            }
            None => {
                if g.curves.is_empty() {
                    continue; // animates nothing of the model
                }
                // a copy of a motion ("walk.001") takes the rest from it, others from the first
                let stem = clean(Some(&Value::Str(g.name.clone())))?;
                let copied = before.animations.iter().position(|e| e.name == stem);
                changes.push(Change {
                    animation: g,
                    key: None,
                    frames: g.frames(),
                    retimed: true,
                    channels: g.curves.keys().copied().filter(|&(frame, kind)| moves(g, frame, kind)).collect(),
                    in_place: false,
                    source: copied.or((!keys.is_empty()).then_some(0)),
                    copied,
                });
            }
        }
    }
    if changes.is_empty() {
        return Ok(None);
    }

    let lists = mot::parse(mot_data)?;
    if lists.is_empty() {
        bail!("the pack's motion has no key lists to add the animations to");
    }
    let mut tracks: Vec<Track> = lists.into_iter().map(|original| Track { original, keys: None, new: false }).collect();
    let mut track_of: HashMap<(usize, usize), usize> = HashMap::new();
    for (at, track) in tracks.iter().enumerate() {
        let list = &track.original;
        if let (Ok(frame), true) = (usize::try_from(list.frame), (0..3).contains(&list.kind) && !list.keys.is_empty()) {
            if frame < model.nodes.len() {
                track_of.insert((frame, list.kind as usize), at);
            }
        }
    }
    // the export's rest, as the engine holds a frame no list drives
    let rest_key = |frame: usize, kind: usize| -> Vec4 {
        let rest =
            before.rest.get(frame).copied().flatten().unwrap_or([[0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 0.0], [0.0; 4]]);
        engine_key(kind, &rest[kind])
    };
    let last = tracks
        .iter()
        .filter_map(|t| t.original.keys.last().map(|k| k.0))
        .chain(keys.iter().filter_map(|k| k.range.map(|r| r.1)))
        .max()
        .unwrap_or(0);
    // lists for the channels the animations change that no list drives: the rest everywhere else
    for change in &changes {
        for &(frame, kind) in &change.channels {
            if frame < model.nodes.len() && !track_of.contains_key(&(frame, kind)) {
                let rest = rest_key(frame, kind);
                let original = KeyList::new(frame as i32, 0, kind as i32, vec![(0, rest), (last.max(1), rest)]);
                track_of.insert((frame, kind), tracks.len());
                tracks.push(Track { original, keys: None, new: true });
            }
        }
    }
    let tracked: HashMap<usize, (usize, usize)> = track_of.iter().map(|(&channel, &at)| (at, channel)).collect();

    let mut written: Vec<(usize, Written)> = Vec::new();
    let mut base = last + 1;
    for (number, change) in changes.iter().enumerate() {
        let g = change.animation;
        let given_key = |frame: usize, kind: usize, r: i64| {
            after.value(g, frame, kind, r as f64 / FPS).map(|v| engine_key(kind, &v))
        };
        if change.in_place {
            let k = change.key.unwrap_or_default();
            let (s, e) = keys[k].range.unwrap_or_default();
            for &(frame, kind) in &change.channels {
                let Some(&at) = track_of.get(&(frame, kind)) else {
                    continue;
                };
                let track = &mut tracks[at];
                let original = track.original.clone();
                let first = if track.new { 0 } else { original.keys[0].0 };
                for f in s..=e {
                    if shared(k, f) {
                        // another KEY's frame keeps its value
                        if !track.keys().contains_key(&f) {
                            track.set(f, list_value(&original, f));
                        }
                    } else if let Some(value) = given_key(frame, kind, f - s) {
                        track.set(f, value);
                    }
                }
                // the frames just outside keep their values
                for f in [s - 1, e + 1] {
                    if (first..=last).contains(&f) && !track.keys().contains_key(&f) {
                        let value = if track.new { rest_key(frame, kind) } else { list_value(&original, f) };
                        track.keys().insert(f, value);
                    }
                }
            }
            written.push((number, Written::InPlace));
            continue;
        }
        let (b, len) = (base, change.frames.max(0));
        let source = change.source.and_then(|k| keys[k].range);
        for (at, track) in tracks.iter_mut().enumerate() {
            if track.original.keys.is_empty() {
                continue;
            }
            let original = track.original.clone();
            // the frames up to the last any KEY uses keep the value they held
            let held = original.keys[original.keys.len() - 1];
            if held.0 < last && !track.keys().contains_key(&last) {
                track.keys().insert(last, held.1);
            }
            for r in 0..=len {
                let value = match tracked.get(&at) {
                    Some(&(frame, kind)) => {
                        let animated = (change.retimed && !track.new) || change.channels.contains(&(frame, kind));
                        match (animated, source) {
                            (false, Some((s, _))) if !track.new => list_value(&original, s + r),
                            (false, _) => rest_key(frame, kind),
                            (true, _) => given_key(frame, kind, r).unwrap_or_else(|| match source {
                                Some((s, e)) if !track.new => list_value(&original, s + scaled(r, len, e - s)),
                                _ => rest_key(frame, kind),
                            }),
                        }
                    }
                    // vertex, material and visibility lists: from the source motion's frames
                    None => match source {
                        Some((s, e)) => list_value(&original, s + scaled(r, len, e - s)),
                        None => list_value(&original, 0),
                    },
                };
                track.set(b + r, value);
            }
        }
        written.push((number, Written::Moved(b, b + len)));
        base = b + len + 1;
    }

    // the .mot: lists in their places, new ones among their frame's
    let mut out: Vec<KeyList> = Vec::new();
    let mut added: Vec<KeyList> = Vec::new();
    for track in tracks {
        let mut list = track.original;
        if let Some(keys) = track.keys {
            let pads: BTreeMap<i64, Vec<u8>> = list
                .key_pad
                .take()
                .into_iter()
                .flatten()
                .filter_map(|(k, pad)| list.keys.get(k).map(|key| (key.0, pad)))
                .collect();
            list.keys = keys.into_iter().collect();
            let kept: BTreeMap<usize, Vec<u8>> = list
                .keys
                .iter()
                .enumerate()
                .filter_map(|(k, key)| pads.get(&key.0).map(|pad| (k, pad.clone())))
                .collect();
            list.key_pad = (!kept.is_empty()).then_some(kept);
        }
        if track.new {
            added.push(list);
        } else {
            out.push(list);
        }
    }
    for list in added {
        out.insert(insertion_point(&out, &list), list);
    }
    let mot = mot::build(&out)?;

    // the info.cfg: moved KEYs' frames, new KEYs after the set's last
    let mut edits: Vec<(std::ops::Range<usize>, Vec<u8>)> = Vec::new();
    let mut lines: Vec<(String, Vec<u8>)> = Vec::new();
    for (number, how) in &written {
        let change = &changes[*number];
        let Written::Moved(s, e) = how else {
            continue;
        };
        match change.key {
            Some(k) => {
                let args = &keys[k].statement.args;
                edits.push((args[1].1.clone(), s.to_string().into_bytes()));
                edits.push((args[2].1.clone(), e.to_string().into_bytes()));
            }
            None => {
                let name = &change.animation.name;
                match c2_disc::text::encode(name) {
                    Some(bytes) if !bytes.is_empty() && bytes.len() <= KEY_NAME => {}
                    _ => bail!("motion {name}: a KEY name is 1 to {KEY_NAME} bytes of Shift-JIS"),
                }
                let step = match change.animation.extras.as_ref().and_then(|x| x.get("step")).and_then(Value::as_f64) {
                    Some(step) => step_text(step).into_bytes(),
                    None => match change.copied {
                        Some(k) => info[keys[k].statement.args[3].1.clone()].to_vec(),
                        None => b"1.00".to_vec(),
                    },
                };
                lines.push((format!("KEY \"{}\", {s}, {e}, ", name.replace('"', "\\\"")), step));
            }
        }
    }
    let mut info_out = info.to_vec();
    if !lines.is_empty() {
        let Some(last_key) = keys.last() else {
            bail!("the info.cfg has no KEY line to add the new motions after");
        };
        let newline: &[u8] = if info.windows(2).any(|w| w == b"\r\n") { b"\r\n" } else { b"\n" };
        let line_start = info[..last_key.statement.start].iter().rposition(|&b| b == b'\n').map_or(0, |p| p + 1);
        let indent = &info[line_start..last_key.statement.start];
        let indent: &[u8] = if indent.iter().all(|b| *b == b' ' || *b == b'\t') { indent } else { b"\t" };
        let at = info[last_key.statement.end..]
            .iter()
            .position(|&b| b == b'\n')
            .map_or(info.len(), |p| last_key.statement.end + p + 1);
        let mut text = Vec::new();
        if at == info.len() && !info.ends_with(b"\n") {
            text.extend(newline);
        }
        for (line, step) in &lines {
            text.extend(indent);
            text.extend(c2_disc::text::encode(line).unwrap_or_default());
            text.extend(step);
            text.extend(b";");
            text.extend(newline);
        }
        edits.push((at..at, text));
    }
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    for (range, text) in edits {
        info_out.splice(range, text);
    }
    let changed = changes.iter().map(|c| c.animation.name.clone()).collect();
    Ok(Some(Rebuilt { mot, info: info_out, changed }))
}

/// Frame r of a motion `len` frames long, on a motion `old` frames long.
fn scaled(r: i64, len: i64, old: i64) -> i64 {
    if len == 0 {
        0
    } else {
        (r as f64 * old as f64 / len as f64).round() as i64
    }
}

/// A step as info.cfg writes them (0.50), with more digits when it has them.
fn step_text(step: f64) -> String {
    let short = format!("{step:.2}");
    if short.parse::<f64>() == Ok(step) {
        short
    } else {
        format!("{step}")
    }
}

/// Where a new list goes: before the first list of a later frame, or of its frame driving a later
/// channel (the files keep each frame's lists together, rotation before scale before
/// translation), and never right after a frame's vertex lists, which MotionProc runs as one group
/// while the frame stays the same.
fn insertion_point(lists: &[KeyList], list: &KeyList) -> usize {
    let mut at = lists
        .iter()
        .position(|l| l.frame > list.frame || (l.frame == list.frame && (0..3).contains(&l.kind) && l.kind > list.kind))
        .unwrap_or(lists.len());
    while at > 0 && lists[at - 1].frame == list.frame && lists[at - 1].kind == 12 {
        at -= 1;
    }
    at
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::from_gltf::load_gltf;
    use crate::mds::{self, Material, Mdt, Node, Prim, IDENT, PRIM_TRIANGLES};
    use crate::models;
    use c2_disc::rebuild;

    /// The engine key of a turn about an axis: glTF's quaternion conjugated, w first.
    fn engine_turn(axis: usize, degrees: f64) -> Vec4 {
        let half = degrees.to_radians() / 2.0;
        let mut q = [0.0, 0.0, 0.0, half.cos()];
        q[axis] = half.sin();
        engine_key(ROTATION, &q)
    }

    const INFO: &str = "v2;\r\nMODEL \"c.mds\";\r\nMOTION 0, \"c.mot\", \"\", \"\";\r\nKEY_START;\r\n\tKEY \"stand\", 1, 5, 0.02;\r\n\tKEY \"walk\", 5, 15, 0.50;\r\n\tKEY \"jump\", 16, 20, 0.30;\r\nKEY_END;\r\nMOTION_END;\r\n";

    /// A character of three frames (root, hip, arm with a triangle) and a motion of 20 frames: the
    /// hip turning and rising, the arm turning and its material fading; stand and walk share
    /// frame 5.
    fn sample_pack() -> Vec<u8> {
        let mut model = Mds::new();
        model.nodes.push(Node::new("root", -1));
        let mut hip = Node::new("hip", 0);
        hip.matrix[13] = 10.0;
        model.nodes.push(hip);
        let mesh = Mdt {
            vertices: vec![[0.0, 0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.0, 1.0]],
            normals: vec![[0.0, 0.0, 1.0, 1.0]],
            uvs: vec![[0.0, 0.0, 1.0, 0.0]],
            materials: vec![Material::new("arm01")],
            prims: vec![Prim {
                kind: PRIM_TRIANGLES,
                material: 0,
                verts: vec![vec![0, 0, 0], vec![1, 0, 0], vec![2, 0, 0]],
            }],
            ..Mdt::default()
        };
        model.nodes.push(Node { mdt: Some(mesh), matrix: IDENT, ..Node::new("arm", 1) });
        let frames = 1..=20i64;
        let mut hip_turn = KeyList::new(1, 0, 0, frames.clone().map(|f| (f, engine_turn(2, 3.0 * f as f64))).collect());
        hip_turn.garbage = (7, 9);
        let lists = [
            hip_turn,
            KeyList::new(1, 0, 2, frames.clone().map(|f| (f, [0.0, 10.0 + f as f64, 0.0, 0.0])).collect()),
            KeyList::new(2, 0, 40, frames.clone().map(|f| (f, [f as f64 / 20.0, 0.0, 0.0, 0.0])).collect()),
            KeyList::new(2, 0, 0, frames.map(|f| (f, engine_turn(0, -2.0 * f as f64))).collect()),
        ];
        c2_disc::samples::pack(&[
            ("info.cfg", INFO.as_bytes()),
            ("c.mds", &mds::build(&model).unwrap()),
            ("c.mot", &mot::build(&lists).unwrap()),
        ])
    }

    fn member(pack: &[u8], name: &str) -> Vec<u8> {
        rebuild::read_member(pack, &[name.to_string()]).unwrap().to_vec()
    }

    fn exported(pack: &[u8]) -> Gltf {
        let files = models::export(pack).unwrap();
        let get = |name: &str| files.iter().find(|(path, _)| path == name).map(|(_, data)| data.clone());
        load_gltf(&get("c.gltf").unwrap(), &|uri| Ok(get(uri).unwrap())).unwrap()
    }

    fn rebuilt(pack: &[u8], given: &Gltf) -> Option<(Rebuilt, Vec<u8>)> {
        let model = mds::parse(&member(pack, "c.mds")).unwrap();
        let out = build(&exported(pack), given, &model, &member(pack, "c.mot"), &member(pack, "info.cfg")).unwrap()?;
        let mut built = rebuild::replaced_member(pack, &["c.mot".to_string()], &out.mot).unwrap();
        built = rebuild::replaced_member(&built, &["info.cfg".to_string()], &out.info).unwrap();
        Some((out, built))
    }

    /// The bytes of an accessor, to edit in place.
    fn bytes_mut<'a>(gltf: &'a mut Gltf, number: &Value) -> &'a mut [u8] {
        let accessor = index(item(&gltf.json, "accessors").unwrap(), number).unwrap().clone();
        let view = index(item(&gltf.json, "bufferViews").unwrap(), item(&accessor, "bufferView").unwrap()).unwrap();
        let start = view.get("byteOffset").and_then(Value::as_int).unwrap_or(0) as usize;
        let length = view.get("byteLength").and_then(Value::as_int).unwrap() as usize;
        &mut gltf.buffers[0][start..start + length]
    }

    fn animation<'a>(gltf: &'a Gltf, name: &str) -> &'a Value {
        let list = gltf.json.get("animations").unwrap().as_list().unwrap();
        list.iter().find(|a| a.get("name").and_then(Value::as_str) == Some(name)).unwrap()
    }

    fn animation_mut<'a>(gltf: &'a mut Gltf, name: &str) -> &'a mut Value {
        let Some(Value::List(list)) = gltf.json.get_mut("animations") else {
            panic!("no animations");
        };
        list.iter_mut().find(|a| a.get("name").and_then(Value::as_str) == Some(name)).unwrap()
    }

    /// Turns a node's rotation channel in an animation by `degrees` about x.
    fn turn(gltf: &mut Gltf, name: &str, node: i64, degrees: f64) {
        let a = animation(gltf, name).clone();
        let channel = a.get("channels").unwrap().as_list().unwrap().iter().find(|c| {
            let target = c.get("target").unwrap();
            target.get("node").unwrap().as_int() == Some(node)
                && target.get("path").unwrap().as_str() == Some("rotation")
        });
        let sampler = index(a.get("samplers").unwrap(), channel.unwrap().get("sampler").unwrap()).unwrap();
        let half = degrees.to_radians() / 2.0;
        let (s, c) = (half.sin(), half.cos());
        for q in bytes_mut(gltf, sampler.get("output").unwrap()).chunks_mut(16) {
            let v: Vec<f64> = q.chunks(4).map(|b| f32::from_le_bytes(b.try_into().unwrap()) as f64).collect();
            // q * (sin, 0, 0, cos)
            let turned = [v[3] * s + v[0] * c, v[1] * c + v[2] * s, v[2] * c - v[1] * s, v[3] * c - v[0] * s];
            for (slot, x) in q.chunks_mut(4).zip(turned) {
                slot.copy_from_slice(&(x as f32).to_le_bytes());
            }
        }
    }

    /// Stretches an animation in time.
    fn retime(gltf: &mut Gltf, name: &str, factor: f32) {
        let a = animation(gltf, name).clone();
        let input = a.get("samplers").unwrap().as_list().unwrap()[0].get("input").unwrap().clone();
        for t in bytes_mut(gltf, &input).chunks_mut(4) {
            let x = f32::from_le_bytes(t.try_into().unwrap());
            t.copy_from_slice(&(x * factor).to_le_bytes());
        }
    }

    /// Every animation of `wanted` is in `got` with the same values at every motion frame.
    fn assert_motions(got: &Gltf, wanted: &Gltf, model: &Mds) {
        let (got, wanted) = (read(got, model).unwrap(), read(wanted, model).unwrap());
        for w in &wanted.animations {
            let g = got.animations.iter().find(|g| g.name == w.name).unwrap_or_else(|| panic!("no {}", w.name));
            assert_eq!(g.frames(), w.frames(), "{}", w.name);
            for &(frame, kind) in g.curves.keys().chain(w.curves.keys()) {
                for r in 0..=w.frames() {
                    let t = r as f64 / FPS;
                    let (x, y) = (got.value(g, frame, kind, t).unwrap(), wanted.value(w, frame, kind, t).unwrap());
                    assert!(close(kind, &x, &y), "{} frame {frame} kind {kind} at {r}: {x:?} {y:?}", w.name);
                }
            }
        }
    }

    fn lists(pack: &[u8]) -> Vec<KeyList> {
        mot::parse(&member(pack, "c.mot")).unwrap()
    }

    fn model(pack: &[u8]) -> Mds {
        mds::parse(&member(pack, "c.mds")).unwrap()
    }

    #[test]
    fn an_unedited_export_changes_no_motion() {
        let pack = sample_pack();
        assert!(rebuilt(&pack, &exported(&pack)).is_none());
        // nor one that lost its extras, as Blender writes it
        let mut bare = exported(&pack);
        if let Some(Value::List(nodes)) = bare.json.get_mut("nodes") {
            for node in nodes {
                if let Value::Object(items) = node {
                    items.retain(|(key, _)| key != "extras");
                }
            }
        }
        assert!(rebuilt(&pack, &bare).is_none());
    }

    #[test]
    fn an_edited_motion_is_written_over_its_own_frames() {
        let pack = sample_pack();
        let mut given = exported(&pack);
        turn(&mut given, "jump", 2, 40.0);
        let (out, built) = rebuilt(&pack, &given).unwrap();
        assert_eq!(out.changed, ["jump"]);
        assert_eq!(member(&built, "info.cfg"), INFO.as_bytes());
        let (before, after) = (lists(&pack), lists(&built));
        assert_eq!(after.len(), 4);
        assert_eq!(after[..3], before[..3]);
        let frames_changed: Vec<i64> =
            after[3].keys.iter().zip(&before[3].keys).filter(|(a, b)| a != b).map(|(a, _)| a.0).collect();
        assert_eq!(frames_changed, [16, 17, 18, 19, 20]);
        assert_motions(&exported(&built), &given, &model(&pack));
    }

    #[test]
    fn a_motion_whose_frames_another_uses_is_moved_to_the_end() {
        let pack = sample_pack();
        let mut given = exported(&pack);
        turn(&mut given, "walk", 1, 25.0);
        let (out, built) = rebuilt(&pack, &given).unwrap();
        assert_eq!(out.changed, ["walk"]);
        let info = String::from_utf8(member(&built, "info.cfg")).unwrap();
        assert_eq!(info, INFO.replace("\"walk\", 5, 15,", "\"walk\", 21, 31,"));
        assert_motions(&exported(&built), &given, &model(&pack));
        // every list goes on into the new frames; the fade from walk's own frames
        let after = lists(&built);
        assert!(after.iter().all(|list| list.keys.last().unwrap().0 == 31));
        let fade: Vec<f32> = after[2].keys[20..].iter().map(|k| k.1[0] as f32).collect();
        assert_eq!(fade, (5..=15).map(|f| f as f32 / 20.0).collect::<Vec<_>>());
        assert_eq!(after[0].garbage, (7, 9));
        // the frames before keep their keys
        assert_eq!(after[3].keys[..20], lists(&pack)[3].keys[..]);
    }

    #[test]
    fn a_retimed_motion_and_a_new_one_get_frames_at_the_end() {
        let pack = sample_pack();
        let mut given = exported(&pack);
        retime(&mut given, "jump", 2.0);
        // a copy of stand, as Blender names a duplicated action
        let mut copy = animation(&given, "stand").clone();
        copy.set("name", Value::Str("stand.001".into()));
        if let Some(Value::List(animations)) = given.json.get_mut("animations") {
            animations.push(copy);
        }
        let (out, built) = rebuilt(&pack, &given).unwrap();
        assert_eq!(out.changed, ["jump", "stand.001"]);
        let info = String::from_utf8(member(&built, "info.cfg")).unwrap();
        let wanted = INFO
            .replace("\"jump\", 16, 20,", "\"jump\", 21, 29,")
            .replace("KEY_END", "\tKEY \"stand.001\", 30, 34, 0.02;\r\nKEY_END");
        assert_eq!(info, wanted);
        assert_motions(&exported(&built), &given, &model(&pack));
    }

    #[test]
    fn a_channel_no_list_drives_gets_a_list_holding_the_rest_elsewhere() {
        let pack = sample_pack();
        let mut given = exported(&pack);
        // the root has no rotation list: jump turns it as it turns the hip
        let jump = animation_mut(&mut given, "jump");
        let Some(Value::List(channels)) = jump.get_mut("channels") else {
            panic!("no channels");
        };
        let mut root = channels[0].clone();
        root.set("target", crate::object! {"node" => 0i64, "path" => "rotation"});
        channels.push(root);
        let (_, built) = rebuilt(&pack, &given).unwrap();
        let after = lists(&built);
        assert_eq!(after.len(), 5);
        // first, as the root's: the rest at 0, jump's frames, the rest around them and at the end
        let new = &after[0];
        assert_eq!((new.frame, new.kind), (0, 0));
        let frames: Vec<i64> = new.keys.iter().map(|k| k.0).collect();
        assert_eq!(frames, [0, 15, 16, 17, 18, 19, 20]);
        assert_eq!(new.keys[0].1, [1.0, 0.0, 0.0, 0.0]);
        assert_motions(&exported(&built), &given, &model(&pack));
    }

    /// Every character pack of an extracted disc (C2S_CORPUS) with a motion: its unedited export
    /// changes no motion, and one bone of its first motion turned 30 degrees comes back from a
    /// fresh export of the rebuilt pack as given, every other motion as it was.
    #[test]
    fn corpus_motions_come_back_from_gltf() {
        use rayon::prelude::*;
        let Some(corpus) = std::env::var_os("C2S_CORPUS") else {
            return;
        };
        let packs: Vec<std::path::PathBuf> = crate::files_under(std::path::Path::new(&corpus))
            .unwrap()
            .into_iter()
            .filter(|path| path.extension().is_some_and(|e| e.eq_ignore_ascii_case("chr")))
            .collect();
        let check = |path: &std::path::PathBuf| -> std::result::Result<Option<bool>, String> {
            let data = std::fs::read(path).unwrap();
            let stem = path.file_stem().unwrap().to_string_lossy().to_string();
            let (pack, _) = crate::tree::model_pack(&data, &stem);
            let Ok(files) = models::export(&pack) else {
                return Ok(None);
            };
            let (folder, names) = models::unpacked(&pack).map_err(|e| e.to_string())?;
            let (Some(mot_name), Some(info)) = (names.motion.as_deref(), folder.get("info.cfg")) else {
                return Ok(None);
            };
            let Some(mot_data) = folder.get(mot_name) else {
                return Ok(None);
            };
            let model = mds::parse(folder.get(&names.model).unwrap()).unwrap();
            let gltf_of = |files: &[(String, Vec<u8>)]| {
                let get = |name: &str| files.iter().find(|(p, _)| p == name).map(|(_, d)| d.clone());
                load_gltf(&get(&format!("{}.gltf", names.stem)).unwrap(), &|uri| Ok(get(uri).unwrap())).unwrap()
            };
            let before = gltf_of(&files);
            let fail = |what: String| format!("{}: {what}", path.display());
            if let Some(out) = build(&before, &before, &model, mot_data, info).map_err(|e| fail(e.to_string()))? {
                return Err(fail(format!("unedited, changed {:?}", out.changed)));
            }
            // nor as Blender names it: no extras, a name given again numbered ".001" on
            let mut bare = gltf_of(&files);
            if let Some(Value::List(nodes)) = bare.json.get_mut("nodes") {
                let mut seen: HashMap<String, usize> = HashMap::new();
                for node in nodes {
                    if let Value::Object(items) = node {
                        items.retain(|(key, _)| key != "extras");
                    }
                    if let Some(name) = node.get("name").and_then(Value::as_str).map(str::to_string) {
                        let count = seen.entry(name.clone()).or_default();
                        if *count > 0 {
                            node.set("name", Value::Str(format!("{name}.{count:03}")));
                        }
                        *count += 1;
                    }
                }
            }
            if let Some(out) = build(&before, &bare, &model, mot_data, info).map_err(|e| fail(e.to_string()))? {
                return Err(fail(format!("unedited without extras, changed {:?}", out.changed)));
            }
            // the first motion turning a bone
            let Some((name, node)) = before.json.get("animations").and_then(Value::as_list).and_then(|list| {
                list.iter().find_map(|a| {
                    let channel = a.get("channels")?.as_list()?.iter().find(|c| {
                        c.get("target").and_then(|t| t.get("path")).and_then(Value::as_str) == Some("rotation")
                    })?;
                    Some((a.get("name")?.as_str()?.to_string(), channel.get("target")?.get("node")?.as_int()?))
                })
            }) else {
                return Ok(None);
            };
            let mut given = gltf_of(&files);
            turn(&mut given, &name, node, 30.0);
            let Some(out) = build(&before, &given, &model, mot_data, info).map_err(|e| fail(e.to_string()))? else {
                return Err(fail(format!("{name} turned, nothing changed")));
            };
            if out.changed != [name.clone()] {
                return Err(fail(format!("{name} turned, changed {:?}", out.changed)));
            }
            let mut built = rebuild::replaced_member(&pack, &[mot_name.to_string()], &out.mot).unwrap();
            built = rebuild::replaced_member(&built, &["info.cfg".to_string()], &out.info).unwrap();
            let again = gltf_of(&models::export(&built).map_err(|e| fail(e.to_string()))?);
            std::panic::catch_unwind(|| assert_motions(&again, &given, &model))
                .map_err(|e| fail(e.downcast_ref::<String>().cloned().unwrap_or_default()))?;
            Ok(Some(out.info == info))
        };
        let results: Vec<_> = packs.par_iter().map(check).collect();
        let failures: Vec<&String> = results.iter().filter_map(|r| r.as_ref().err()).collect();
        let in_place = results.iter().filter(|r| matches!(r, Ok(Some(true)))).count();
        let moved = results.iter().filter(|r| matches!(r, Ok(Some(false)))).count();
        eprintln!("{} packs: {in_place} edited in place, {moved} moved, {} failed", packs.len(), failures.len());
        assert!(failures.is_empty(), "{}", failures.iter().take(20).map(|f| f.as_str()).collect::<Vec<_>>().join("\n"));
        assert!(in_place + moved > 1000);
    }

    #[test]
    fn key_statements_know_where_their_numbers_are() {
        let keys = motion0_keys(INFO.as_bytes());
        assert_eq!(keys.len(), 3);
        assert_eq!(keys[1].range, Some((5, 15)));
        assert_eq!(&INFO.as_bytes()[keys[1].statement.args[1].1.clone()], b"5");
        assert_eq!(&INFO.as_bytes()[keys[1].statement.args[0].1.clone()], b"\"walk\"");
        assert_eq!(step_text(0.5), "0.50");
        assert_eq!(step_text(0.125), "0.125");
    }
}
