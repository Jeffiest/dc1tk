//! Character motions rebuilt from glTF animations. The export writes one animation per KEY of every
//! motion set the pack holds a .mot for, sampled every motion frame (`FPS` frames a second): its
//! frames' rotations, scales and translations as node channels, and its vertex, material and
//! visibility lists as the morph weights of the frames' meshes (to_gltf::Target). Build reads the
//! animations back and writes into the pack's .mot files and info.cfg only those that differ from
//! the export by value (a tool that resamples or adds float noise changes nothing), those the
//! export does not have, and those it no longer has:
//! - an edited motion of the same length is written over its own frames of its set's timeline, its
//!   KEY unchanged, when no other KEY shares a frame it changes;
//! - otherwise (a retimed motion, or one whose frames another KEY uses) its frames are added at
//!   the end of the timeline and its KEY points there;
//! - a new animation is added at the end of the timeline as a new KEY, after the motion set's last;
//! - a KEY whose animation is gone is taken out of info.cfg, with its frames no other KEY plays.
//!
//! Every key list gets values at every frame a motion is written to: from the animation, or (where
//! the animation leaves a channel out) the frame's rest, or the original motion's (or the one an
//! animation copied, "walk.001" from "walk") values for lists the glTF carries nothing of. Lists
//! the animations do not change keep their bytes.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::from_gltf::{self, accessor, clean, index, item, Gltf};
use crate::info_cfg::{self, Arg, Statement};
use crate::math::{decompose, slerp, Mixing};
use crate::mds::Mds;
use crate::mot::{self, KeyList};
use crate::to_gltf::Target;
use crate::{bail, Result};
use serde_json::Value;

/// Motion frames a second at step 1.0: the PAL game advances a KEY by 1.2 × step frames each
/// 25 Hz game frame (CCharacter2::NormalDrive), the NTSC one by step each 30 Hz frame.
pub const FPS: f64 = 30.0;

/// The channels a key list of each kind (0, 1, 2) drives.
const PATHS: [&str; 3] = ["rotation", "scale", "translation"];
const ROTATION: i32 = 0;
const SCALE: i32 = 1;
const TRANSLATION: i32 = 2;
/// A vertex position list (target = vertex + 1), a material's fade and colour (target = the
/// material), a frame's visibility.
const VERTEX: i32 = 12;
const FADE: i32 = 40;
const COLOUR: i32 = 41;
const VISIBLE: i32 = 50;
const VISIBLE_CHILDREN: i32 = 51;

/// Largest rotation (radians) and translation or scale differences that are no edit: Blender's
/// round trip moves rotations by up to 7e-4 and translations by a few millionths.
const ROTATION_TOLERANCE: f64 = 3e-3;
const DISTANCE_TOLERANCE: f64 = 1e-3;
const RELATIVE_TOLERANCE: f64 = 2e-5;
/// Largest difference in a material's fade or colour or a visibility value that is no edit (the
/// morph weights carrying them come back as written).
const VALUE_TOLERANCE: f64 = 1e-3;

/// The longest KEY name: CHRINFO_KEY_SET's char[0x24] with its NUL.
const KEY_NAME: usize = 35;

type Vec4 = [f64; 4];

/// The key list a channel stands for: its frame, kind and target. Rotation, scale and translation
/// channels have target 0 and stand for their frame's list of that kind whatever its target.
type Channel = (usize, i32, i32);

fn is_trs(kind: i32) -> bool {
    (ROTATION..=TRANSLATION).contains(&kind)
}

#[derive(Clone, Copy, PartialEq)]
enum Interpolation {
    Linear,
    Step,
    Cubic,
}

/// One animation channel: key times, values (vec3 padded with 0, a quaternion x, y, z, w, or a
/// list's value as the .mot holds it) and, for cubic splines, each key's in and out tangents.
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

/// math::slerp, or the straight mix normalised where its numbers fail (a NaN).
fn quat_slerp(a: &Vec4, b: &Vec4, t: f64) -> Vec4 {
    slerp(a, b, t).unwrap_or_else(|_| normalized(&lerp(a, b, t)))
}

impl Curve {
    /// The value at time t, held before the first key and after the last.
    fn at(&self, t: f64, mixing: Mixing) -> Vec4 {
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
            Interpolation::Linear if mixing == Mixing::Spherical => quat_slerp(a, b, u),
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
                match mixing {
                    Mixing::Spherical => normalized(&value),
                    Mixing::Straight => value,
                }
            }
        }
    }
}

/// An animation's channels.
struct Animation {
    name: String,
    extras: Option<Value>,
    curves: BTreeMap<Channel, Curve>,
    /// The frames whose meshes' morph weights it drives: their vertex, material and visibility
    /// channels it leaves out hold their rests.
    morphs: BTreeSet<usize>,
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
/// scale, translation) and the animations; `model` is the model whose meshes its morph targets
/// are read against.
struct Motions<'m> {
    model: &'m Mds,
    rest: Vec<Option<[Vec4; 3]>>,
    animations: Vec<Animation>,
}

impl Motions<'_> {
    /// The value an animation gives a channel t seconds into it: its curve's, else a frame's rest
    /// (its node's for rotation, scale and translation; for the others when the animation drives
    /// the frame's mesh's morph weights), else None.
    fn value(&self, animation: &Animation, channel: Channel, t: f64) -> Option<Vec4> {
        if let Some(curve) = animation.curves.get(&channel) {
            let mixing = if channel.1 == ROTATION { Mixing::Spherical } else { Mixing::Straight };
            return Some(curve.at(animation.start + t, mixing));
        }
        if is_trs(channel.1) {
            return self.rest.get(channel.0).copied().flatten().map(|rest| rest[channel.1 as usize]);
        }
        if animation.morphs.contains(&channel.0) {
            return channel_rest(self.model, channel);
        }
        None
    }
}

/// A vertex, material or visibility channel's value where no list drives it, as the .mot holds
/// values: the vertex's position (w 1), the fade 1 - the material's alpha, its colour, shown.
fn channel_rest(model: &Mds, (frame, kind, target): Channel) -> Option<Vec4> {
    let t = model.nodes.get(frame)?.mdt.as_ref()?;
    let diffuse = |k: i32, c: usize| -> Option<f64> {
        let m = t.materials.get(usize::try_from(k).ok()?)?;
        Some(m.diffuse.get(c).copied().unwrap_or(1.0))
    };
    match kind {
        VERTEX => {
            let v = t.vertices.get(usize::try_from(target - 1).ok()?)?;
            Some([v[0], v[1], v[2], 1.0])
        }
        FADE => Some([1.0 - diffuse(target, 3)?, 0.0, 0.0, 0.0]),
        COLOUR => Some([diffuse(target, 0)?, diffuse(target, 1)?, diffuse(target, 2)?, 0.0]),
        VISIBLE | VISIBLE_CHILDREN => Some([1.0, 0.0, 0.0, 0.0]),
        _ => None,
    }
}

/// The frame each glTF node stands for, for motions: its `extras.mds_index`, or else by its name (a
/// frame's whole name, or its name without flags) among the frames under the frame its parent
/// stands for; nodes of one name under one parent take that name's frames in order, by Blender's
/// ".NNN" (which numbers them in the order it made them) and then in glTF order. Skinned mesh
/// nodes stand for none.
fn node_frames(json: &Value, model: &Mds) -> Result<Vec<Option<usize>>> {
    let empty = Vec::new();
    let nodes = json.get("nodes").and_then(Value::as_array).unwrap_or(&empty);
    let mut whole: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut base: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, node) in model.nodes.iter().enumerate() {
        whole.entry(node.name.as_str()).or_default().push(i);
        base.entry(node.base_name()).or_default().push(i);
    }
    let children = |gi: usize| -> Vec<usize> {
        let list = nodes[gi].get("children").and_then(Value::as_array).unwrap_or(&empty);
        list.iter()
            .filter_map(|c| c.as_i64().and_then(|c| usize::try_from(c).ok()))
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
            Some(name) if n.get("skin").is_none() => {
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
        let index = n.get("extras").and_then(|e| e.get("mds_index")).and_then(Value::as_i64);
        if let Some(i) = index.and_then(|i| usize::try_from(i).ok()).filter(|&i| i < model.nodes.len()) {
            out[gi] = n.get("skin").is_none().then_some(i);
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
    match value.and_then(Value::as_array) {
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

fn vec4(values: &[crate::numeric::Num]) -> Vec4 {
    std::array::from_fn(|i| values.get(i).map_or(0.0, |v| v.f()))
}

/// What the morph targets of a glTF's meshes stand for: the motion sets' ids (as the targets'
/// names give them) and the one whose frames are named without one.
struct TargetNames<'a> {
    ids: &'a [String],
    primary: usize,
}

/// The glTF's rests and animations by MDS frame. A frame several nodes stand for (Blender's bone
/// and the mesh object on it) takes the rest and channels of the one that is not a mesh. The
/// meshes' morph weights give the vertex channels of the vertices their frame targets move (in
/// `model`'s meshes, matched by place; `binds` the pack's bind matrices) and the carriers' material
/// and visibility channels.
fn read<'m>(
    gltf: &Gltf,
    model: &'m Mds,
    binds: &HashMap<usize, crate::math::Mat4>,
    names: &TargetNames,
) -> Result<Motions<'m>> {
    let j = &gltf.json;
    let frames = node_frames(j, model)?;
    let empty = Vec::new();
    let nodes = j.get("nodes").and_then(Value::as_array).unwrap_or(&empty);
    let mut chosen: Vec<Option<usize>> = vec![None; model.nodes.len()];
    for (gi, frame) in frames.iter().enumerate() {
        if let Some(f) = *frame {
            let better = match chosen[f] {
                None => true,
                Some(other) => nodes[other].get("mesh").is_some() && nodes[gi].get("mesh").is_none(),
            };
            if better {
                chosen[f] = Some(gi);
            }
        }
    }
    let rest = chosen.iter().map(|gi| gi.map(|gi| node_rest(&nodes[gi])).transpose()).collect::<Result<_>>()?;
    let morph_meshes = from_gltf::morph_meshes(gltf, model, binds)?;
    let mesh_of: HashMap<usize, usize> = morph_meshes.iter().map(|(&frame, morph)| (morph.node, frame)).collect();
    let mut animations = Vec::new();
    for (number, a) in j.get("animations").and_then(Value::as_array).unwrap_or(&empty).iter().enumerate() {
        let name = a.get("name").and_then(Value::as_str).map_or(format!("animation{number}"), str::to_string);
        let samplers = item(a, "samplers")?;
        let mut curves = BTreeMap::new();
        let mut morphs = BTreeSet::new();
        let (mut start, mut end) = (f64::INFINITY, 0.0f64);
        for channel in item(a, "channels")?.as_array().unwrap_or(&empty) {
            let target = item(channel, "target")?;
            let path = target.get("path").and_then(Value::as_str);
            let Some(gi) = target.get("node").and_then(Value::as_i64).and_then(|n| usize::try_from(n).ok()) else {
                continue;
            };
            let sampler = index(samplers, item(channel, "sampler")?)?;
            let interpolation = match sampler.get("interpolation").and_then(Value::as_str) {
                Some("STEP") => Interpolation::Step,
                Some("CUBICSPLINE") => Interpolation::Cubic,
                _ => Interpolation::Linear,
            };
            if path == Some("weights") {
                let Some((&frame, morph)) = mesh_of.get(&gi).and_then(|f| morph_meshes.get_key_value(f)) else {
                    continue;
                };
                let times: Vec<f64> = accessor(gltf, item(sampler, "input")?)?.values.iter().map(|v| v.f()).collect();
                let output: Vec<f64> = accessor(gltf, item(sampler, "output")?)?.values.iter().map(|v| v.f()).collect();
                let count = morph.names.len();
                let per_key = if interpolation == Interpolation::Cubic { 3 * count } else { count };
                if times.is_empty() || count == 0 || output.len() != times.len() * per_key {
                    bail!("animation {name}: a weights channel has {} values for {} keys", output.len(), times.len());
                }
                // the values at the keys (a cubic spline's middle ones), linearly between
                let offset = if interpolation == Interpolation::Cubic { count } else { 0 };
                let weights: Vec<&[f64]> =
                    (0..times.len()).map(|k| &output[k * per_key + offset..k * per_key + offset + count]).collect();
                let interpolation =
                    if interpolation == Interpolation::Step { Interpolation::Step } else { Interpolation::Linear };
                for (channel, curve) in morph_curves(model, frame, morph, &times, &weights, interpolation, names) {
                    curves.insert(channel, curve);
                }
                morphs.insert(frame);
                start = start.min(times[0]);
                end = end.max(times[times.len() - 1]);
                continue;
            }
            let Some(kind) = path.and_then(|p| PATHS.iter().position(|q| *q == p)) else {
                continue;
            };
            let Some(frame) = frames.get(gi).copied().flatten() else {
                continue;
            };
            if chosen[frame] != Some(gi) {
                continue;
            }
            let times: Vec<f64> = accessor(gltf, item(sampler, "input")?)?.values.iter().map(|v| v.f()).collect();
            let output = accessor(gltf, item(sampler, "output")?)?;
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
            let kind = kind as i32;
            let values = if kind == ROTATION { values.iter().map(normalized).collect() } else { values };
            start = start.min(times[0]);
            end = end.max(times[times.len() - 1]);
            curves.insert((frame, kind, 0), Curve { times, values, tangents, interpolation });
        }
        let start = if start.is_finite() { start.min(end) } else { 0.0 };
        animations.push(Animation { name, extras: a.get("extras").cloned(), curves, morphs, start, end });
    }
    Ok(Motions { model, rest, animations })
}

/// The channels a mesh's morph weights give (`weights[k]` the targets' weights at `times[k]`): the
/// position of every vertex its vertex frame targets move (its place plus each such target's move
/// times its weight), and the carriers' fades, colours and visibility (their weights).
fn morph_curves(
    model: &Mds,
    frame: usize,
    morph: &from_gltf::MorphMesh,
    times: &[f64],
    weights: &[&[f64]],
    interpolation: Interpolation,
    names: &TargetNames,
) -> Vec<(Channel, Curve)> {
    let targets: Vec<Option<Target>> =
        morph.names.iter().map(|name| Target::parse(name, names.ids, names.primary)).collect();
    let curve = |values: Vec<Vec4>| Curve { times: times.to_vec(), values, tangents: Vec::new(), interpolation };
    let mut out = Vec::new();
    let shapes: Vec<usize> = (0..targets.len()).filter(|&k| matches!(targets[k], Some(Target::Frame { .. }))).collect();
    if !shapes.is_empty() {
        for (&v, (base, moved)) in &morph.vertices {
            let deltas: Vec<(usize, [f64; 3])> = shapes
                .iter()
                .map(|&k| (k, [0, 1, 2].map(|i| moved[k][i] - base[i])))
                .filter(|(_, d)| d.iter().any(|x| x.abs() > 1e-6))
                .collect();
            if deltas.is_empty() {
                continue;
            }
            let values = weights
                .iter()
                .map(|w| {
                    let mut p = [base[0], base[1], base[2], 1.0];
                    for (k, d) in &deltas {
                        for i in 0..3 {
                            p[i] += w[*k] * d[i];
                        }
                    }
                    p
                })
                .collect();
            out.push(((frame, VERTEX, v as i32 + 1), curve(values)));
        }
    }
    // the carriers: a colour from its three targets, a component without one at the material's
    let mut colours: BTreeMap<usize, [Option<usize>; 3]> = BTreeMap::new();
    for (k, target) in targets.iter().enumerate() {
        match *target {
            Some(Target::Fade(m)) => {
                out.push(((frame, FADE, m as i32), curve(weights.iter().map(|w| [w[k], 0.0, 0.0, 0.0]).collect())));
            }
            Some(Target::Colour(m, c)) => colours.entry(m).or_default()[c] = Some(k),
            Some(Target::Visible(kind)) => {
                out.push(((frame, kind, 0), curve(weights.iter().map(|w| [w[k], 0.0, 0.0, 0.0]).collect())));
            }
            _ => {}
        }
    }
    for (m, parts) in colours {
        let rest = channel_rest(model, (frame, COLOUR, m as i32)).unwrap_or([1.0, 1.0, 1.0, 0.0]);
        let values = weights
            .iter()
            .map(|w| {
                let part = |c: usize| parts[c].map_or(rest[c], |k| w[k]);
                [part(0), part(1), part(2), 0.0]
            })
            .collect();
        out.push(((frame, COLOUR, m as i32), curve(values)));
    }
    out
}

/// Whether two values of a kind are the same within what a round trip through another tool moves.
fn close(kind: i32, a: &Vec4, b: &Vec4) -> bool {
    close_within(kind, a, b, 1.0)
}

/// `close`, translations allowed `slack` times the distance tolerance: a frame under ancestors
/// shrunk to a scale s moves in the model by s times its translation, and a tool working in model
/// space (Blender's exporter takes each bone's local matrix from float32 armature-space matrices)
/// gives its translation back only to about 1/s times its float error.
fn close_within(kind: i32, a: &Vec4, b: &Vec4, slack: f64) -> bool {
    match kind {
        ROTATION => {
            let dot = a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>().abs();
            2.0 * dot.min(1.0).acos() <= ROTATION_TOLERANCE
        }
        SCALE | TRANSLATION | VERTEX => {
            let slack = if kind == TRANSLATION { slack } else { 1.0 };
            (0..3).all(|i| {
                (a[i] - b[i]).abs() <= slack * DISTANCE_TOLERANCE + RELATIVE_TOLERANCE * a[i].abs().max(b[i].abs())
            })
        }
        _ => (0..3).all(|i| (a[i] - b[i]).abs() <= VALUE_TOLERANCE),
    }
}

/// The scale a frame's ancestors give it in the model at t seconds into an animation: the product
/// of each ancestor's smallest scale (as the animation or its rest has it).
fn ancestor_scale(motions: &Motions, animation: &Animation, model: &Mds, frame: usize, t: f64) -> f64 {
    let mut scale = 1.0;
    let mut at = model.nodes[frame].parent;
    for _ in 0..model.nodes.len() {
        let Some(parent) = usize::try_from(at).ok().filter(|&p| p < model.nodes.len()) else {
            break;
        };
        if let Some(value) = motions.value(animation, (parent, SCALE, 0), t) {
            scale *= value[..3].iter().map(|x| x.abs()).fold(f64::INFINITY, f64::min);
        }
        at = model.nodes[parent].parent;
    }
    scale
}

/// How many times the distance tolerance a translation of a frame gets: 1 under ancestors of
/// scale 1 or more, 1/s under ancestors shrunk to s in both animations (the larger s of the two,
/// so that a frame its edit brings back to full size is compared as any other).
fn slack(before: f64, after: f64) -> f64 {
    let scale = before.max(after);
    if scale.is_finite() && scale < 1.0 {
        1.0 / scale.max(1e-9)
    } else {
        1.0
    }
}

/// A glTF value as a .mot key: rotations as the engine's (w, x, y, z) of the conjugate (QuatToMat
/// builds the transpose), translations and scales with w 0; the others are already the .mot's.
fn engine_key(kind: i32, v: &Vec4) -> Vec4 {
    match kind {
        ROTATION => {
            let q = normalized(v);
            [q[3], -q[0], -q[1], -q[2]]
        }
        SCALE | TRANSLATION => [v[0], v[1], v[2], 0.0],
        VERTEX => [v[0], v[1], v[2], 1.0],
        _ => *v,
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
        ROTATION => quat_slerp(&a, &b, t),
        VISIBLE | VISIBLE_CHILDREN => a,
        _ => lerp(&a, &b, t),
    }
}

/// A KEY and its statement in the info.cfg.
struct Key {
    statement: Statement,
    range: Option<(i64, i64)>,
}

/// A motion set as info.cfg gives it (CharInfo's reading: an id given again keeps its place and
/// starts its KEYs again): its id, its .mot and its KEYs.
struct SetInfo {
    id: Arg,
    mot: Arg,
    keys: Vec<Key>,
}

/// The motion sets of an info.cfg, in the order their ids are first given.
fn motion_sets(info: &[u8]) -> Vec<SetInfo> {
    let mut out: Vec<SetInfo> = Vec::new();
    let mut current = None;
    for statement in info_cfg::statements(info) {
        match statement.tag.as_str() {
            "MOTION" if statement.args.len() >= 4 => {
                let id = statement.args[0].0.clone();
                let set = SetInfo { id: id.clone(), mot: statement.args[1].0.clone(), keys: Vec::new() };
                match out.iter().position(|known| info_cfg::same_value(&known.id, &id)) {
                    Some(at) => {
                        out[at] = set;
                        current = Some(at);
                    }
                    None => {
                        out.push(set);
                        current = Some(out.len() - 1);
                    }
                }
            }
            "KEY" if statement.args.len() >= 4 => {
                if let Some(at) = current {
                    let range = match (&statement.args[1].0, &statement.args[2].0) {
                        (Arg::Int(s), Arg::Int(e)) => Some((*s, *e.max(s))),
                        _ => None,
                    };
                    out[at].keys.push(Key { statement, range });
                }
            }
            _ => {}
        }
    }
    out
}

/// The KEY statements of motion set 0.
#[cfg(test)]
fn motion0_keys(info: &[u8]) -> Vec<Key> {
    motion_sets(info)
        .into_iter()
        .find(|set| info_cfg::same_value(&set.id, &Arg::Int(0)))
        .map(|set| set.keys)
        .unwrap_or_default()
}

/// A motion set of the pack with its .mot read (as to_gltf::load_motions reads them).
struct LoadedSet {
    info: SetInfo,
    member: String,
    lists: Vec<KeyList>,
}

impl LoadedSet {
    /// Whether the export has an animation for each of its KEYs.
    fn exported(&self) -> bool {
        !self.lists.is_empty() && !self.info.keys.is_empty()
    }
}

/// What became of a motion.
enum Written {
    /// Written over its own frames.
    InPlace,
    /// Added at the end of the timeline at these frames.
    Moved(i64, i64),
}

/// The .mot files (by member) and info.cfg rebuilt, and the names of the motions a glTF's
/// animations changed or added and of the KEYs taken out.
pub struct Rebuilt {
    pub mots: Vec<(String, Vec<u8>)>,
    pub info: Vec<u8>,
    pub changed: Vec<String>,
    pub removed: Vec<String>,
}

impl Rebuilt {
    /// The rebuilt .mot of a member, if it changed.
    pub fn mot(&self, member: &str) -> Option<&[u8]> {
        self.mots.iter().find(|(name, _)| name == member).map(|(_, data)| data.as_slice())
    }
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
        let rotation = self.original.kind == ROTATION;
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

/// The channel a key list stands for, among a model's frames: rotation, scale and translation by
/// frame and kind; vertex lists by vertex, material lists by material, visibility by kind.
fn channel_of(list: &KeyList, model: &Mds) -> Option<Channel> {
    let frame = usize::try_from(list.frame).ok().filter(|&f| f < model.nodes.len())?;
    if list.keys.is_empty() {
        return None;
    }
    match list.kind {
        kind if is_trs(kind) => Some((frame, kind, 0)),
        VERTEX | FADE | COLOUR | VISIBLE | VISIBLE_CHILDREN => Some((frame, list.kind, list.target)),
        _ => None,
    }
}

/// An edit of the info.cfg's bytes: the range replaced and what replaces it.
type Edit = (std::ops::Range<usize>, Vec<u8>);

/// The motion sets the pack holds a .mot of, as the export reads them (set 0's must read).
fn loaded_sets(info: &[u8], member: &dyn Fn(&str) -> Option<Vec<u8>>) -> Result<Vec<LoadedSet>> {
    let mut sets: Vec<LoadedSet> = Vec::new();
    for set in motion_sets(info) {
        let zero = info_cfg::same_value(&set.id, &Arg::Int(0));
        let name = match &set.mot {
            Arg::Str(name) if !name.is_empty() => name.clone(),
            value if !value.is_set() => continue,
            _ if zero => bail!("info.cfg names motion set 0's .mot with {:?}, which is not a string", set.mot),
            _ => continue,
        };
        let Some(data) = member(&name) else {
            continue;
        };
        let lists = match mot::parse(&data) {
            Ok(lists) => lists,
            Err(error) if zero => return Err(error),
            Err(_) => continue,
        };
        sets.push(LoadedSet { info: set, member: name, lists });
    }
    Ok(sets)
}

/// The sets' animations as the export wrote them, in order: each exported set's KEYs, as (set,
/// KEY).
fn exported_keys(sets: &[LoadedSet]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (s, set) in sets.iter().enumerate() {
        if set.exported() {
            out.extend((0..set.info.keys.len()).map(|k| (s, k)));
        }
    }
    out
}

/// Each given animation in its set: an exported one's, a copy's ("7:walk.001" of "7:walk"), the
/// set its name gives ("7:jump"), else the primary set.
fn given_by_set<'a>(
    before: &Motions,
    after: &'a Motions,
    sets: &[LoadedSet],
    exported_keys: &[(usize, usize)],
    names: &TargetNames,
) -> Result<Vec<Vec<Given<'a>>>> {
    let by_name: HashMap<&str, usize> =
        before.animations.iter().enumerate().map(|(i, e)| (e.name.as_str(), i)).collect();
    let mut given_of: Vec<Vec<Given>> = (0..sets.len()).map(|_| Vec::new()).collect();
    for g in &after.animations {
        if let Some(&i) = by_name.get(g.name.as_str()) {
            if let Some(&(s, k)) = exported_keys.get(i) {
                given_of[s].push(Given {
                    animation: g,
                    key: Some(k),
                    exported: Some(i),
                    copied: None,
                    name: String::new(),
                });
            }
            continue;
        }
        if sets.is_empty() {
            bail!("the pack has no motion set to add the animation {} to", g.name);
        }
        let stem = clean(Some(&Value::String(g.name.clone())))?;
        let copied = by_name.get(stem.as_str()).and_then(|&i| exported_keys.get(i).map(|&(s, k)| (s, k, i)));
        // the set its name gives (`7:jump`), and the KEY's name in it
        let named = crate::to_gltf::set_of_name(&g.name).and_then(|(id, rest)| {
            names.ids.iter().position(|known| *known == id.to_string()).map(|s| (s, rest.to_string()))
        });
        let (s, name) = match (copied, named) {
            (Some((s, _, _)), Some((n, rest))) if n == s => (s, rest),
            (Some((s, _, _)), _) => (s, g.name.clone()),
            (None, Some(found)) => found,
            (None, None) => (names.primary, g.name.clone()),
        };
        given_of[s].push(Given {
            animation: g,
            key: None,
            exported: None,
            copied: copied.map(|(_, k, i)| (k, i)),
            name,
        });
    }
    Ok(given_of)
}

/// The KEYs of each set whose animations are gone (when the glTF has any): those that animate
/// something.
fn gone_keys(before: &Motions, after: &Motions, sets: usize, exported_keys: &[(usize, usize)]) -> Vec<Vec<usize>> {
    let mut gone: Vec<Vec<usize>> = (0..sets).map(|_| Vec::new()).collect();
    if !after.animations.is_empty() {
        for (i, e) in before.animations.iter().enumerate() {
            let animates = !e.curves.is_empty() || !e.morphs.is_empty();
            if animates && !after.animations.iter().any(|g| g.name == e.name) {
                if let Some(&(s, k)) = exported_keys.get(i) {
                    gone[s].push(k);
                }
            }
        }
    }
    gone
}

/// The edits taking out the sound cues and sequence steps that name a KEY taken out (one no set
/// still has), and a sequence left with no step.
fn cue_edits(info: &[u8], sets: &[LoadedSet], removed: &[(String, Statement)]) -> Vec<Edit> {
    let remaining: BTreeSet<String> = sets
        .iter()
        .flat_map(|set| set.info.keys.iter())
        .filter(|key| !removed.iter().any(|(_, statement)| statement.start == key.statement.start))
        .filter_map(|key| key.statement.args[0].0.as_str().map(str::to_string))
        .collect();
    let gone_names: BTreeSet<String> =
        removed.iter().map(|(name, _)| name.clone()).filter(|name| !remaining.contains(name)).collect();
    let names_gone = |statement: &Statement| {
        statement.args.first().and_then(|(value, _)| value.as_str()).is_some_and(|n| gone_names.contains(n))
    };
    let mut edits = Vec::new();
    let statements = info_cfg::statements(info);
    for (at, statement) in statements.iter().enumerate() {
        if !(matches!(statement.tag.as_str(), "SE" | "SELP" | "SEQ") && names_gone(statement)) {
            continue;
        }
        edits.push((info_cfg::statement_line(info, statement), Vec::new()));
        // a sequence left with no step goes too
        if statement.tag == "SEQ" {
            let open = statements[..at].iter().rposition(|s| s.tag == "SEQ_START");
            let close = statements[at..].iter().position(|s| s.tag == "SEQ_END").map(|p| p + at);
            if let (Some(open), Some(close)) = (open, close) {
                let steps = &statements[open + 1..close];
                let left = steps.iter().any(|s| s.tag == "SEQ" && !names_gone(s));
                if !left && steps.iter().rfind(|s| s.tag == "SEQ").is_some_and(|s| s.start == statement.start) {
                    edits.push((info_cfg::statement_line(info, &statements[open]), Vec::new()));
                    edits.push((info_cfg::statement_line(info, &statements[close]), Vec::new()));
                }
            }
        }
    }
    edits
}

/// The info.cfg with the edits made, from the last back (a range taken out twice once).
fn edited(info: &[u8], mut edits: Vec<Edit>) -> Vec<u8> {
    edits.sort_by(|a, b| b.0.start.cmp(&a.0.start).then(b.0.end.cmp(&a.0.end)));
    edits.dedup_by(|a, b| a.0 == b.0 && a.1.is_empty() && b.1.is_empty());
    let mut out = info.to_vec();
    for (range, text) in edits {
        out.splice(range, text);
    }
    out
}

/// The pack's motions with the given glTF's animations written in (see the module), or None when
/// none differs from the export's (`exported`, the pack's own export) and none is gone. `model` is
/// the pack's model (for a pack of motions alone, the model whose skeleton they play on), `member`
/// gives the pack's members by name, `rebuilt` the model with the meshes the build rebuilt and
/// their frames (whose vertex lists are made again for their new vertices).
pub fn build(
    exported: &Gltf,
    given: &Gltf,
    model: &Mds,
    member: &dyn Fn(&str) -> Option<Vec<u8>>,
    info: &[u8],
    rebuilt: Option<(&Mds, &BTreeSet<usize>)>,
) -> Result<Option<Rebuilt>> {
    let sets = loaded_sets(info, member)?;
    let ids: Vec<String> = sets.iter().map(|set| crate::to_gltf::id_text(&set.info.id)).collect();
    let primary = crate::to_gltf::primary_set(&sets.iter().map(|set| &set.info.id).collect::<Vec<_>>());
    let names = TargetNames { ids: &ids, primary };
    let binds = from_gltf::bind_matrices(exported, model)?;
    let after_model = rebuilt.map_or(model, |(m, _)| m);
    let rebuilt_frames: BTreeSet<usize> = rebuilt.map(|(_, frames)| frames.clone()).unwrap_or_default();
    let before = read(exported, model, &binds, &names)?;
    let after = read(given, after_model, &binds, &names)?;
    let exported_keys = exported_keys(&sets);
    if before.animations.len() != exported_keys.len() && !before.animations.is_empty() {
        bail!("the export has {} animations for {} KEY lines", before.animations.len(), exported_keys.len());
    }
    let mut seen = BTreeSet::new();
    for animation in &after.animations {
        if !seen.insert(animation.name.as_str()) {
            bail!("two animations are named {}", animation.name);
        }
    }
    let given_of = given_by_set(&before, &after, &sets, &exported_keys, &names)?;
    let gone = gone_keys(&before, &after, sets.len(), &exported_keys);
    let context = Context { before: &before, after: &after, model, after_model, rebuilt: &rebuilt_frames, info };
    let mut mots = Vec::new();
    let mut edits: Vec<Edit> = Vec::new();
    let mut changed = Vec::new();
    let mut removed: Vec<(String, Statement)> = Vec::new();
    for (s, set) in sets.iter().enumerate() {
        let Some(out) = build_set(&context, set, &given_of[s], &gone[s])? else {
            continue;
        };
        if let Some(mot) = out.mot {
            mots.push((set.member.clone(), mot));
        }
        edits.extend(out.edits);
        changed.extend(out.changed);
        removed.extend(out.removed);
    }
    if mots.is_empty() && edits.is_empty() && removed.is_empty() {
        return Ok(None);
    }
    edits.extend(cue_edits(info, &sets, &removed));
    for (_, statement) in &removed {
        edits.push((info_cfg::statement_line(info, statement), Vec::new()));
    }
    let info = edited(info, edits);
    Ok(Some(Rebuilt { mots, info, changed, removed: removed.into_iter().map(|(name, _)| name).collect() }))
}

/// A given animation and what it stands for in its set: the KEY it is the export's of (and that
/// animation's number), or for a new one the KEY and export animation it copies and its KEY name.
struct Given<'a> {
    animation: &'a Animation,
    key: Option<usize>,
    exported: Option<usize>,
    copied: Option<(usize, usize)>,
    name: String,
}

/// What every set's build reads: the export's motions and the given ones, the model (the given's
/// meshes are read against `after_model`), the frames whose meshes were rebuilt, the info.cfg.
struct Context<'a, 'm> {
    before: &'a Motions<'m>,
    after: &'a Motions<'m>,
    model: &'m Mds,
    after_model: &'m Mds,
    rebuilt: &'a BTreeSet<usize>,
    info: &'a [u8],
}

/// One set's rebuilt .mot (None when no list changed), its info.cfg edits, the motions changed
/// and the KEYs taken out (name, statement).
struct SetBuilt {
    mot: Option<Vec<u8>>,
    edits: Vec<(std::ops::Range<usize>, Vec<u8>)>,
    changed: Vec<String>,
    removed: Vec<(String, Statement)>,
}

/// An animation of a set that differs from the export (or that the export does not have), and how
/// it is written.
struct Change<'a> {
    animation: &'a Animation,
    name: String,
    key: Option<usize>,
    frames: i64,
    retimed: bool,
    channels: BTreeSet<Channel>,
    in_place: bool,
    /// The KEY whose frames give the channels the glTF has nothing of.
    source: Option<usize>,
    /// The KEY a new animation is a copy of.
    copied: Option<usize>,
}

/// The key lists of a set being rebuilt, and the list standing for each channel.
struct Tracks {
    tracks: Vec<Track>,
    track_of: HashMap<Channel, usize>,
    /// The last frame any list keys or any KEY plays.
    last: i64,
}

/// One set's rebuild (build_set): what it reads, and the questions its phases ask of it.
struct SetPlan<'p, 'm> {
    c: &'p Context<'p, 'm>,
    set: &'p LoadedSet,
    given: &'p [Given<'p>],
    gone: &'p [usize],
    keys: &'p [Key],
    /// The channels a list of the set drives.
    listed: BTreeSet<Channel>,
}

impl SetPlan<'_, '_> {
    /// Whether a KEY other than k plays a frame.
    fn shared(&self, k: usize, frame: i64) -> bool {
        self.keys
            .iter()
            .enumerate()
            .any(|(other, key)| other != k && key.range.is_some_and(|(s, e)| s <= frame && frame <= e))
    }

    /// How far a translation may move and be the same: translations under shrunk ancestors are
    /// compared as far as they move the frame in the model.
    fn slack_at(&self, e: Option<&Animation>, g: &Animation, channel: Channel, t: f64) -> f64 {
        if channel.1 != TRANSLATION {
            return 1.0;
        }
        let model = self.c.model;
        let given = ancestor_scale(self.c.after, g, model, channel.0, t);
        slack(e.map_or(given, |e| ancestor_scale(self.c.before, e, model, channel.0, t)), given)
    }

    /// Whether a channel no list drives holds its rest at time t: the frame's own (the export's
    /// node), or the given node's, since Blender rests a bone at its bind pose (from the inverse
    /// bind matrices) and writes that for every bone an action leaves alone; a vertex, material or
    /// visibility channel its mesh's.
    fn at_rest(&self, e: Option<&Animation>, g: &Animation, channel: Channel, t: f64) -> bool {
        let Some(value) = self.c.after.value(g, channel, t) else {
            return true;
        };
        let slack = self.slack_at(e, g, channel, t);
        if !is_trs(channel.1) {
            let rests = [channel_rest(self.c.model, channel), channel_rest(self.c.after_model, channel)];
            return rests.into_iter().flatten().any(|rest| close_within(channel.1, &rest, &value, slack));
        }
        let kind = channel.1 as usize;
        let rests = [self.c.before.rest.get(channel.0), self.c.after.rest.get(channel.0)];
        rests.into_iter().flatten().flatten().any(|rest| close_within(channel.1, &rest[kind], &value, slack))
    }

    /// Whether an animation leaves a channel as the export has it at time t.
    fn same_at(&self, e: &Animation, g: &Animation, channel: Channel, t: f64) -> bool {
        if is_trs(channel.1) && !e.curves.contains_key(&channel) {
            return self.at_rest(Some(e), g, channel, t);
        }
        let old = self.c.before.value(e, channel, t).or_else(|| channel_rest(self.c.model, channel));
        match (old, self.c.after.value(g, channel, t)) {
            (Some(x), Some(y)) => close_within(channel.1, &x, &y, self.slack_at(Some(e), g, channel, t)),
            _ => true,
        }
    }

    /// A channel this set's motions write: not the vertex channels of a rebuilt mesh, which are
    /// made again whole.
    fn wanted(&self, channel: &Channel) -> bool {
        !(channel.1 == VERTEX && self.c.rebuilt.contains(&channel.0))
    }

    /// Whether an animation drives a channel: one a list drives, or one it moves off the rest.
    fn moves(&self, g: &Animation, channel: Channel) -> bool {
        self.listed.contains(&channel) || (0..=g.frames()).any(|r| !self.at_rest(None, g, channel, r as f64 / FPS))
    }

    /// Which given animations changed, and how.
    fn changes(&self) -> Vec<Change<'_>> {
        let mut changes = Vec::new();
        for given in self.given {
            let g = given.animation;
            match (given.key, given.exported) {
                (Some(k), Some(i)) => {
                    let e = &self.c.before.animations[i];
                    let Some((s, end)) = self.keys[k].range else {
                        continue;
                    };
                    let (old, new) = (end - s, g.frames());
                    let mut channels = BTreeSet::new();
                    for &channel in e.curves.keys().chain(g.curves.keys()).filter(|ch| self.wanted(ch)) {
                        if (0..=old).any(|r| !self.same_at(e, g, channel, r as f64 / FPS)) {
                            channels.insert(channel);
                        }
                    }
                    if new <= old && channels.is_empty() {
                        continue;
                    }
                    let retimed = new != old;
                    if retimed {
                        channels.extend(g.curves.keys().copied().filter(|ch| self.wanted(ch) && self.moves(g, *ch)));
                    }
                    // in place unless a frame another KEY uses would change
                    let in_place = !retimed
                        && (s..=end)
                            .filter(|&f| self.shared(k, f))
                            .all(|f| channels.iter().all(|&channel| self.same_at(e, g, channel, (f - s) as f64 / FPS)));
                    changes.push(Change {
                        animation: g,
                        name: g.name.clone(),
                        key: Some(k),
                        frames: new,
                        retimed,
                        channels,
                        in_place,
                        source: Some(k),
                        copied: None,
                    });
                }
                _ => {
                    if g.curves.is_empty() {
                        continue; // animates nothing of the model
                    }
                    let copied = given.copied.map(|(k, _)| k);
                    changes.push(Change {
                        animation: g,
                        name: given.name.clone(),
                        key: None,
                        frames: g.frames(),
                        retimed: true,
                        channels: g.curves.keys().copied().filter(|ch| self.wanted(ch) && self.moves(g, *ch)).collect(),
                        in_place: false,
                        source: copied.or((!self.keys.is_empty()).then_some(0)),
                        copied,
                    });
                }
            }
        }
        changes
    }

    /// The frames of rebuilt meshes with vertex lists, or vertex shapes now: their lists are made
    /// again.
    fn remade(&self) -> BTreeSet<usize> {
        let lists = &self.set.lists;
        self.c
            .rebuilt
            .iter()
            .copied()
            .filter(|&frame| {
                lists.iter().any(|l| l.frame as i64 == frame as i64 && l.kind == VERTEX && !l.keys.is_empty())
                    || self.given.iter().any(|g| g.animation.curves.keys().any(|ch| ch.0 == frame && ch.1 == VERTEX))
            })
            .collect()
    }

    /// The export's rest, as the engine holds a channel no list drives.
    fn rest_key(&self, channel: Channel) -> Vec4 {
        if !is_trs(channel.1) {
            return channel_rest(self.c.model, channel).unwrap_or([0.0; 4]);
        }
        let rest = self.c.before.rest.get(channel.0).copied().flatten().unwrap_or([
            [0.0, 0.0, 0.0, 1.0],
            [1.0, 1.0, 1.0, 0.0],
            [0.0; 4],
        ]);
        engine_key(channel.1, &rest[channel.1 as usize])
    }

    /// The set's lists to write into: its own, and for the channels the animations change that
    /// no list drives, new lists holding the rest everywhere else.
    fn tracks(&self, changes: &[Change]) -> Tracks {
        let (model, lists) = (self.c.model, &self.set.lists);
        let mut tracks: Vec<Track> =
            lists.iter().cloned().map(|original| Track { original, keys: None, new: false }).collect();
        let mut track_of: HashMap<Channel, usize> = HashMap::new();
        for (at, track) in tracks.iter().enumerate() {
            if let Some(channel) = channel_of(&track.original, model) {
                track_of.insert(channel, at);
            }
        }
        let last = tracks
            .iter()
            .filter_map(|t| t.original.keys.last().map(|k| k.0))
            .chain(self.keys.iter().filter_map(|k| k.range.map(|r| r.1)))
            .max()
            .unwrap_or(0);
        // the frames each frame's vertex lists share
        let group_frames = |frame: usize| -> BTreeSet<i64> {
            lists
                .iter()
                .filter(|l| l.frame as i64 == frame as i64 && l.kind == VERTEX)
                .flat_map(|l| l.keys.iter().map(|k| k.0))
                .collect()
        };
        for change in changes {
            for &channel in &change.channels {
                if channel.0 < model.nodes.len() && !track_of.contains_key(&channel) {
                    let rest = self.rest_key(channel);
                    let frames = if channel.1 == VERTEX { group_frames(channel.0) } else { BTreeSet::new() };
                    let keys = if frames.is_empty() {
                        vec![(0, rest), (last.max(1), rest)]
                    } else {
                        frames.iter().map(|&f| (f, rest)).collect()
                    };
                    let original = KeyList::new(channel.0 as i32, channel.2, channel.1, keys);
                    track_of.insert(channel, tracks.len());
                    tracks.push(Track { original, keys: None, new: true });
                }
            }
        }
        Tracks { tracks, track_of, last }
    }

    /// Each change's frames written: over its own frames (in place), or at the end of the
    /// timeline, one after another. Returns where each went.
    fn place(&self, changes: &[Change], tracks: &mut Tracks) -> Vec<(usize, Written)> {
        let after = self.c.after;
        let last = tracks.last;
        let tracked: HashMap<usize, Channel> = tracks.track_of.iter().map(|(&channel, &at)| (at, channel)).collect();
        let mut written: Vec<(usize, Written)> = Vec::new();
        let mut base = last + 1;
        for (number, change) in changes.iter().enumerate() {
            let g = change.animation;
            let given_key =
                |channel: Channel, r: i64| after.value(g, channel, r as f64 / FPS).map(|v| engine_key(channel.1, &v));
            if change.in_place {
                let k = change.key.unwrap_or_default();
                let (s, e) = self.keys[k].range.unwrap_or_default();
                for &channel in &change.channels {
                    let Some(&at) = tracks.track_of.get(&channel) else {
                        continue;
                    };
                    let track = &mut tracks.tracks[at];
                    let original = track.original.clone();
                    let first = if track.new { 0 } else { original.keys[0].0 };
                    for f in s..=e {
                        if self.shared(k, f) {
                            // another KEY's frame keeps its value
                            if !track.keys().contains_key(&f) {
                                track.set(f, list_value(&original, f));
                            }
                        } else if let Some(value) = given_key(channel, f - s) {
                            track.set(f, value);
                        }
                    }
                    // the frames just outside keep their values
                    for f in [s - 1, e + 1] {
                        if (first..=last).contains(&f) && !track.keys().contains_key(&f) {
                            let value = if track.new { self.rest_key(channel) } else { list_value(&original, f) };
                            track.keys().insert(f, value);
                        }
                    }
                }
                written.push((number, Written::InPlace));
                continue;
            }
            let (b, len) = (base, change.frames.max(0));
            let source = change.source.and_then(|k| self.keys[k].range);
            for (at, track) in tracks.tracks.iter_mut().enumerate() {
                if track.original.keys.is_empty() {
                    continue;
                }
                let original = track.original.clone();
                // the frames up to the last any KEY uses keep the value they held
                let held = original.keys[original.keys.len() - 1];
                if held.0 < last && !track.keys().contains_key(&last) {
                    track.keys().insert(last, held.1);
                }
                let stretched = |r: i64| match source {
                    Some((s, e)) => list_value(&original, s + scaled(r, len, e - s)),
                    None => list_value(&original, 0),
                };
                for r in 0..=len {
                    let value = match tracked.get(&at) {
                        Some(&channel) => {
                            let animated = (change.retimed && !track.new) || change.channels.contains(&channel);
                            match (animated, source) {
                                (false, Some((s, _))) if !track.new => list_value(&original, s + r),
                                (false, _) => self.rest_key(channel),
                                (true, _) => given_key(channel, r).unwrap_or_else(|| match source {
                                    Some(_) if !track.new => stretched(r),
                                    _ => self.rest_key(channel),
                                }),
                            }
                        }
                        // lists of no channel: from the source motion's frames
                        None => stretched(r),
                    };
                    track.set(b + r, value);
                }
            }
            written.push((number, Written::Moved(b, b + len)));
            base = b + len + 1;
        }
        written
    }

    /// Every KEY's frames once the changes are written: a moved KEY's new ones.
    fn ranges(&self, changes: &[Change], written: &[(usize, Written)]) -> Vec<Option<(i64, i64)>> {
        let mut ranges: Vec<Option<(i64, i64)>> = self.keys.iter().map(|key| key.range).collect();
        for (number, how) in written {
            if let (Some(k), Written::Moved(s, e)) = (changes[*number].key, how) {
                ranges[k] = Some((*s, *e));
            }
        }
        ranges
    }

    /// The frames of the KEYs taken out that no other KEY plays, taken out of every list; the
    /// frames around each run taken out keep their values.
    fn drop_gone(&self, changes: &[Change], written: &[(usize, Written)], tracks: &mut Tracks) {
        let ranges = self.ranges(changes, written);
        let new_ranges: Vec<(i64, i64)> = written
            .iter()
            .filter(|(number, _)| changes[*number].key.is_none())
            .filter_map(|(_, how)| match how {
                Written::Moved(s, e) => Some((*s, *e)),
                Written::InPlace => None,
            })
            .collect();
        let kept = |frame: i64| {
            self.keys.iter().enumerate().any(|(k, key)| {
                !self.gone.contains(&k)
                    && [key.range, ranges[k]].iter().flatten().any(|&(s, e)| s <= frame && frame <= e)
            }) || new_ranges.iter().any(|&(s, e)| s <= frame && frame <= e)
        };
        let mut dropped: BTreeSet<i64> = BTreeSet::new();
        for &k in self.gone {
            if let Some((s, e)) = self.keys[k].range {
                dropped.extend((s..=e).filter(|&f| !kept(f)));
            }
        }
        if dropped.is_empty() {
            return;
        }
        for track in &mut tracks.tracks {
            let original = track.original.clone();
            let present: BTreeMap<i64, Vec4> = track.keys().clone();
            let left: Vec<i64> = present.keys().copied().filter(|f| !dropped.contains(f)).collect();
            if left.is_empty() || left.len() == present.len() {
                continue; // a list keyed only there keeps its keys
            }
            // each run taken out: the frames around it keep their values
            let value_at = |f: i64| -> Vec4 {
                let list = KeyList { keys: present.iter().map(|(&k, &v)| (k, v)).collect(), ..original.clone() };
                list_value(&list, f)
            };
            let mut runs: Vec<(i64, i64)> = Vec::new();
            for &f in &dropped {
                match runs.last_mut() {
                    Some(run) if run.1 + 1 == f => run.1 = f,
                    _ => runs.push((f, f)),
                }
            }
            let (first, end) = (*present.keys().next().unwrap(), *present.keys().last().unwrap());
            let mut fixes = Vec::new();
            for &(a, b) in &runs {
                if a > first && !present.contains_key(&(a - 1)) && present.range(a..=b).next().is_some() {
                    fixes.push((a - 1, value_at(a - 1)));
                }
                if b < end && !present.contains_key(&(b + 1)) && present.range(a..=b).next().is_some() {
                    fixes.push((b + 1, value_at(b + 1)));
                }
            }
            let keys = track.keys();
            keys.retain(|f, _| !dropped.contains(f));
            keys.extend(fixes);
        }
    }

    /// A rebuilt mesh's vertex lists made again: a list for each vertex its shapes move, keyed at
    /// every frame a KEY plays, with the values of that KEY's animation (its place where none).
    fn remade_lists(
        &self,
        remade: &BTreeSet<usize>,
        changes: &[Change],
        written: &[(usize, Written)],
    ) -> BTreeMap<usize, Vec<KeyList>> {
        let ranges = self.ranges(changes, written);
        let mut remade_lists: BTreeMap<usize, Vec<KeyList>> = BTreeMap::new();
        for &frame in remade {
            let Some(t) = self.c.after_model.nodes.get(frame).and_then(|n| n.mdt.as_ref()) else {
                continue;
            };
            // (start, end, the given animation playing there) of every KEY the set keeps
            let mut plays: Vec<(i64, i64, Option<&Animation>)> = Vec::new();
            for (k, range) in ranges.iter().enumerate() {
                if self.gone.contains(&k) {
                    continue;
                }
                let animation = self.given.iter().find(|g| g.key == Some(k)).map(|g| g.animation);
                if let Some((s, e)) = *range {
                    plays.push((s, e, animation));
                }
            }
            for (number, how) in written {
                if let (None, Written::Moved(s, e)) = (changes[*number].key, how) {
                    plays.push((*s, *e, Some(changes[*number].animation)));
                }
            }
            let frames: BTreeSet<i64> = plays.iter().flat_map(|&(s, e, _)| s..=e).collect();
            let moved: BTreeSet<usize> = self
                .given
                .iter()
                .flat_map(|g| g.animation.curves.keys())
                .filter(|ch| ch.0 == frame && ch.1 == VERTEX)
                .filter_map(|ch| usize::try_from(ch.2 - 1).ok())
                .filter(|&v| v < t.vertices.len())
                .collect();
            let mut made = Vec::new();
            for v in moved {
                let channel = (frame, VERTEX, v as i32 + 1);
                let rest = channel_rest(self.c.after_model, channel).unwrap_or([0.0, 0.0, 0.0, 1.0]);
                let keys = frames
                    .iter()
                    .map(|&f| {
                        let value = plays
                            .iter()
                            .find(|&&(s, e, _)| s <= f && f <= e)
                            .and_then(|&(s, _, animation)| {
                                animation.and_then(|a| self.c.after.value(a, channel, (f - s) as f64 / FPS))
                            })
                            .unwrap_or(rest);
                        (f, engine_key(VERTEX, &value))
                    })
                    .collect();
                made.push(KeyList::new(frame as i32, v as i32 + 1, VERTEX, keys));
            }
            remade_lists.insert(frame, made);
        }
        remade_lists
    }

    /// The .mot, or None when it is the original's: lists in their places, new ones among their
    /// frame's, every frame's vertex lists keyed at the same frames.
    fn mot(&self, tracks: Tracks, remade_lists: BTreeMap<usize, Vec<KeyList>>) -> Result<Option<Vec<u8>>> {
        let mut out: Vec<KeyList> = Vec::new();
        let mut added: Vec<KeyList> = Vec::new();
        for track in tracks.tracks {
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
        for (frame, made) in remade_lists {
            let at = out.iter().position(|l| l.frame as i64 == frame as i64 && l.kind == VERTEX);
            out.retain(|l| !(l.frame as i64 == frame as i64 && l.kind == VERTEX));
            added.retain(|l| !(l.frame as i64 == frame as i64 && l.kind == VERTEX));
            let at = at.unwrap_or_else(|| out.iter().position(|l| l.frame > frame as i32).unwrap_or(out.len()));
            let at = at.min(out.len());
            out.splice(at..at, made);
        }
        for list in added {
            out.insert(insertion_point(&out, &list), list);
        }
        share_vertex_frames(&mut out);
        let mot = mot::build(&out)?;
        Ok((mot != mot::build(&self.set.lists)?).then_some(mot))
    }

    /// The info.cfg's edits: moved KEYs' frames, and a KEY line for each new motion after the
    /// set's last KEY, indented as it is and in the file's line endings.
    fn info_edits(
        &self,
        changes: &[Change],
        written: &[(usize, Written)],
    ) -> Result<Vec<(std::ops::Range<usize>, Vec<u8>)>> {
        let (info, keys) = (self.c.info, self.keys);
        let mut edits: Vec<(std::ops::Range<usize>, Vec<u8>)> = Vec::new();
        let mut lines: Vec<(String, Vec<u8>)> = Vec::new();
        for (number, how) in written {
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
                    let name = &change.name;
                    match dc2tk_disc::text::encode(name) {
                        Some(bytes) if !bytes.is_empty() && bytes.len() <= KEY_NAME => {}
                        _ => bail!("motion {name}: a KEY name is 1 to {KEY_NAME} bytes of Shift-JIS"),
                    }
                    let step =
                        match change.animation.extras.as_ref().and_then(|x| x.get("step")).and_then(Value::as_f64) {
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
        if !lines.is_empty() {
            let Some(last_key) = keys.last() else {
                bail!("the info.cfg has no KEY line to add the new motions after");
            };
            let text: Vec<Vec<u8>> = lines
                .into_iter()
                .map(|(line, step)| {
                    let mut text = dc2tk_disc::text::encode(&line).unwrap_or_default();
                    text.extend(step);
                    text.push(b';');
                    text
                })
                .collect();
            edits.push(info_cfg::lines_after(info, &last_key.statement, &text));
        }
        Ok(edits)
    }
}

fn build_set(c: &Context, set: &LoadedSet, given: &[Given], gone: &[usize]) -> Result<Option<SetBuilt>> {
    let listed: BTreeSet<Channel> = set.lists.iter().filter_map(|list| channel_of(list, c.model)).collect();
    let plan = SetPlan { c, set, given, gone, keys: &set.info.keys, listed };
    let changes = plan.changes();
    let remade = plan.remade();
    if changes.is_empty() && gone.is_empty() && remade.is_empty() {
        return Ok(None);
    }
    if set.lists.is_empty() {
        bail!("the pack's motion has no key lists to add the animations to");
    }
    let mut tracks = plan.tracks(&changes);
    let written = plan.place(&changes, &mut tracks);
    plan.drop_gone(&changes, &written, &mut tracks);
    let remade_lists = plan.remade_lists(&remade, &changes, &written);
    let mot = plan.mot(tracks, remade_lists)?;
    let edits = plan.info_edits(&changes, &written)?;
    let removed: Vec<(String, Statement)> = gone
        .iter()
        .map(|&k| {
            let statement = plan.keys[k].statement.clone();
            let name = statement.args[0].0.as_str().unwrap_or_default().to_string();
            (name, statement)
        })
        .collect();
    let changed = changes.iter().map(|c| c.animation.name.clone()).collect();
    Ok(Some(SetBuilt { mot, edits, changed, removed }))
}

/// What an info.cfg's KEYs became, for build's report: a line for each KEY added, taken out, or
/// given other frames or another step, by motion set, against the original info.cfg. A KEY is
/// matched by its set and name (a name a set gives twice, in order); a new KEY's number is its
/// place among its set's KEYs, counted from 0.
pub fn key_changes(before: &[u8], after: &[u8]) -> Vec<String> {
    let (old_sets, new_sets) = (motion_sets(before), motion_sets(after));
    let arg = |info: &[u8], key: &Key, at: usize| {
        key.statement.args.get(at).map(|(_, bytes)| String::from_utf8_lossy(&info[bytes.clone()]).into_owned())
    };
    let name = |key: &Key| key.statement.args[0].0.text();
    let set_name = |set: &SetInfo| set.id.text();
    let frames = |info: &[u8], key: &Key| {
        format!(
            "frames {} to {}, step {}",
            arg(info, key, 1).unwrap_or_default(),
            arg(info, key, 2).unwrap_or_default(),
            arg(info, key, 3).unwrap_or_default()
        )
    };
    let mut lines = Vec::new();
    for new_set in &new_sets {
        let old_set = old_sets.iter().find(|set| info_cfg::same_value(&set.id, &new_set.id));
        let mut matched: Vec<bool> = old_set.map_or_else(Vec::new, |set| vec![false; set.keys.len()]);
        for (number, key) in new_set.keys.iter().enumerate() {
            let found = old_set.and_then(|set| {
                set.keys.iter().enumerate().position(|(at, old)| !matched[at] && name(old) == name(key))
            });
            let Some(at) = found else {
                lines.push(format!(
                    "KEY \"{}\" added to motion set {} as number {number}: {}",
                    name(key),
                    set_name(new_set),
                    frames(after, key)
                ));
                continue;
            };
            matched[at] = true;
            let old = &old_set.expect("a matched KEY has its set").keys[at];
            let (was, now) = (frames(before, old), frames(after, key));
            if was != now {
                lines.push(format!("KEY \"{}\" of motion set {}: {now} (was {was})", name(key), set_name(new_set)));
            }
        }
        if let Some(set) = old_set {
            for (at, key) in set.keys.iter().enumerate().filter(|(at, _)| !matched[*at]) {
                lines.push(format!("KEY \"{}\" taken out of motion set {} (number {at})", name(key), set_name(set)));
            }
        }
    }
    for set in old_sets.iter().filter(|old| !new_sets.iter().any(|new| info_cfg::same_value(&new.id, &old.id))) {
        lines.push(format!("motion set {} taken out", set_name(set)));
    }
    lines
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

/// Where a new list goes: a vertex list after its frame's last (MotionProc runs a frame's vertex
/// lists as one group, keyed alike); another before the first list of a later frame, or of its
/// frame driving a later channel (the files keep each frame's lists together, material lists
/// before rotation before scale before translation), and never right after a frame's vertex
/// lists, which MotionProc would take into the group.
fn insertion_point(lists: &[KeyList], list: &KeyList) -> usize {
    if list.kind == VERTEX {
        if let Some(at) = lists.iter().rposition(|l| l.frame == list.frame && l.kind == VERTEX) {
            return at + 1;
        }
    }
    let rank = |kind: i32| match kind {
        VERTEX => 0,
        FADE | COLOUR => 1,
        kind if is_trs(kind) => 2 + kind,
        _ => 5,
    };
    let mut at = lists
        .iter()
        .position(|l| l.frame > list.frame || (l.frame == list.frame && rank(l.kind) > rank(list.kind)))
        .unwrap_or(lists.len());
    if list.kind != VERTEX {
        while at > 0 && lists[at - 1].frame == list.frame && lists[at - 1].kind == VERTEX {
            at -= 1;
        }
    }
    at
}

/// Keys every frame's vertex lists at the same frames (MotionProc reads the group with the first
/// list's key numbers): each gets a key, of the value it held there, at every frame another has.
fn share_vertex_frames(lists: &mut [KeyList]) {
    let mut frames_of: BTreeMap<i32, BTreeSet<i64>> = BTreeMap::new();
    for list in lists.iter().filter(|l| l.kind == VERTEX && !l.keys.is_empty()) {
        frames_of.entry(list.frame).or_default().extend(list.keys.iter().map(|k| k.0));
    }
    for list in lists.iter_mut().filter(|l| l.kind == VERTEX && !l.keys.is_empty()) {
        let frames = &frames_of[&list.frame];
        if frames.len() == list.keys.len() {
            continue;
        }
        let original = list.clone();
        let mut keys: BTreeMap<i64, Vec4> = list.keys.iter().copied().collect();
        for &f in frames {
            keys.entry(f).or_insert_with(|| list_value(&original, f));
        }
        list.keys = keys.into_iter().collect();
        // the keys' padding follows them
        if let Some(pads) = original.key_pad {
            let by_frame: BTreeMap<i64, Vec<u8>> =
                pads.into_iter().filter_map(|(k, pad)| original.keys.get(k).map(|key| (key.0, pad))).collect();
            let kept: BTreeMap<usize, Vec<u8>> = list
                .keys
                .iter()
                .enumerate()
                .filter_map(|(k, key)| by_frame.get(&key.0).map(|pad| (k, pad.clone())))
                .collect();
            list.key_pad = (!kept.is_empty()).then_some(kept);
        }
    }
}

/// A new character's motion set: its .mot and its KEYs (name, start, end, step).
pub struct NewMotion {
    pub mot: Vec<u8>,
    pub keys: Vec<(String, i64, i64, f64)>,
}

/// The motion of a model made from a glTF alone (from_gltf::new_model, whose frames the glTF's
/// nodes are named after): every animation that moves a frame, or fades, colours, hides or shows
/// a mesh, becomes a KEY, in glTF order, on one timeline from frame 1 (motion r of an animation at
/// frame start + r, r/30 s into it; step `extras.step`, else 1.00, the speed Blender plays it at).
/// A channel any animation moves off its rest gets a key list with a key every frame from 0 (the
/// rest) to the last, the rest where an animation leaves it; channels no animation moves get
/// none, so their frames hold the model's own matrices and materials. The rests: a node's
/// rotation, scale and translation; a material's own fade and colour; shown. Fades, colours and
/// visibility are the carrier shape keys of the frames' meshes (`c2_fade_<k>`, `c2_red_<k>`,
/// `c2_green_<k>`, `c2_blue_<k>`, `c2_visible`, `c2_visible_51`; to_gltf::Target), as an export
/// writes them; a frame's lists run material, rotation, scale, translation, visibility. Vertex
/// animation (`c2_frame_<f>` shape keys) is refused. None when no animation moves anything.
pub fn new_motion(given: &Gltf, model: &Mds) -> Result<Option<NewMotion>> {
    let motions = read(given, model, &HashMap::new(), &TargetNames { ids: &[], primary: 0 })?;
    let rest = |(frame, kind, target): Channel| -> Option<Vec4> {
        if !is_trs(kind) {
            return channel_rest(model, (frame, kind, target));
        }
        let rest = motions.rest.get(frame).copied().flatten();
        let kind = kind as usize;
        Some(rest.map_or([[0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 0.0], [0.0; 4]][kind], |rest| rest[kind]))
    };
    let moves = |a: &Animation, channel: Channel| -> Result<bool> {
        let (frame, kind, _) = channel;
        let Some(at_rest) = rest(channel) else {
            return Ok(false);
        };
        let moved = (0..=a.frames().max(0))
            .any(|r| motions.value(a, channel, r as f64 / FPS).is_some_and(|value| !close(kind, &at_rest, &value)));
        if moved && kind == VERTEX {
            bail!(
                "motion {}: the mesh of {} has vertex animation (c2_frame_ shape keys), which a new character \
                 cannot have",
                a.name,
                model.nodes[frame].name
            );
        }
        Ok(moved && kind != VERTEX)
    };
    // the animations that move something, at their frames, and the channels they move
    let mut placed: Vec<(&Animation, i64, i64)> = Vec::new();
    let mut channels: BTreeSet<Channel> = BTreeSet::new();
    let mut start = 1;
    for a in &motions.animations {
        let mut moved: Vec<Channel> = Vec::new();
        for &channel in a.curves.keys() {
            if moves(a, channel)? {
                moved.push(channel);
            }
        }
        if moved.is_empty() {
            continue;
        }
        match dc2tk_disc::text::encode(&a.name) {
            Some(bytes) if !bytes.is_empty() && bytes.len() <= KEY_NAME => {}
            _ => bail!("motion {}: a KEY name is 1 to {KEY_NAME} bytes of Shift-JIS", a.name),
        }
        if placed.iter().any(|(other, _, _)| other.name == a.name) {
            bail!("two animations are named {}", a.name);
        }
        channels.extend(moved);
        let end = start + a.frames().max(0);
        placed.push((a, start, end));
        start = end + 1;
    }
    let Some(&(_, _, last)) = placed.last() else {
        return Ok(None);
    };
    // frames in order, a frame's lists as the disc's run: its materials' fades and colours, then
    // rotation, scale, translation, then visibility
    let rank = |kind: i32| match kind {
        FADE | COLOUR => 0,
        kind if is_trs(kind) => 1 + kind,
        _ => 4,
    };
    let mut channels: Vec<Channel> = channels.into_iter().collect();
    channels.sort_by_key(|&(frame, kind, target)| (frame, rank(kind), kind, target));
    let mut lists = Vec::with_capacity(channels.len());
    for channel in channels {
        let (frame, kind, target) = channel;
        let at_rest = rest(channel).unwrap_or_default();
        let mut track = Track { original: KeyList::new(frame as i32, target, kind, Vec::new()), keys: None, new: true };
        track.set(0, engine_key(kind, &at_rest));
        for &(a, s, e) in &placed {
            for f in s..=e {
                let mut value = motions.value(a, channel, (f - s) as f64 / FPS).unwrap_or(at_rest);
                // the engine hides below 1: a shown frame read a hair under it (a 32-bit key time
                // a hair past the frame's) stays shown
                if matches!(kind, VISIBLE | VISIBLE_CHILDREN) && (value[0] - 1.0).abs() <= VALUE_TOLERANCE {
                    value[0] = 1.0;
                }
                track.set(f, engine_key(kind, &value));
            }
        }
        let mut list = track.original;
        list.keys = track.keys.unwrap_or_default().into_iter().collect();
        debug_assert_eq!(list.keys.len() as i64, last + 1);
        lists.push(list);
    }
    let keys = placed
        .iter()
        .map(|(a, s, e)| {
            let step = a.extras.as_ref().and_then(|x| x.get("step")).and_then(Value::as_f64).unwrap_or(1.0);
            (a.name.clone(), *s, *e, step)
        })
        .collect();
    Ok(Some(NewMotion { mot: mot::build(&lists)?, keys }))
}

/// A KEY line of info.cfg: `KEY "name", start, end, step;`.
pub fn key_line(name: &str, start: i64, end: i64, step: f64) -> String {
    format!("KEY \"{}\", {start}, {end}, {};", name.replace('"', "\\\""), step_text(step))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::from_gltf::load_gltf;
    use crate::math::IDENT;
    use crate::mds::{self, Material, Mdt, Node, Prim, PRIM_TRIANGLES};
    use crate::models;
    use dc2tk_disc::rebuild;
    use std::sync::Arc;

    /// The engine key of a turn about an axis: glTF's quaternion conjugated, w first.
    fn engine_turn(axis: usize, degrees: f64) -> Vec4 {
        let half = degrees.to_radians() / 2.0;
        let mut q = [0.0, 0.0, 0.0, half.cos()];
        q[axis] = half.sin();
        engine_key(ROTATION, &q)
    }

    const INFO: &str = "v2;\r\nMODEL \"c.mds\";\r\nMOTION 0, \"c.mot\", \"\", \"\";\r\nKEY_START;\r\n\tKEY \"stand\", 1, 5, 0.02;\r\n\tKEY \"walk\", 5, 15, 0.50;\r\n\tKEY \"jump\", 16, 20, 0.30;\r\nKEY_END;\r\nMOTION_END;\r\nSE_START 2;\r\n\tSE \"jump\", 0, 0, 0.500;\r\n\tSE \"walk\", 0, 0, 0.500;\r\nSE_END;\r\n";

    /// A character of three frames (root, hip, arm with a triangle) and a motion of 20 frames: the
    /// hip turning and rising, the arm turning, its material fading and its first vertex moving up;
    /// stand and walk share frame 5.
    fn sample_pack() -> Vec<u8> {
        sample_pack_with(INFO, &[])
    }

    /// The sample with another info.cfg and more members (a motion set's .mot).
    fn sample_pack_with(info: &str, more: &[(&str, &[u8])]) -> Vec<u8> {
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
            KeyList::new(2, 1, 12, frames.clone().map(|f| (f, [0.0, f as f64 / 10.0, 0.0, 1.0])).collect()),
            KeyList::new(2, 0, 40, frames.clone().map(|f| (f, [f as f64 / 20.0, 0.0, 0.0, 0.0])).collect()),
            KeyList::new(2, 0, 0, frames.map(|f| (f, engine_turn(0, -2.0 * f as f64))).collect()),
        ];
        let model = mds::build(&model).unwrap();
        let mot = mot::build(&lists).unwrap();
        let mut members: Vec<(&str, &[u8])> = vec![("info.cfg", info.as_bytes()), ("c.mds", &model), ("c.mot", &mot)];
        members.extend(more.iter().copied());
        dc2tk_disc::samples::pack(&members)
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
        let get = |name: &str| rebuild::read_member(pack, &[name.to_string()]).map(<[u8]>::to_vec);
        let out = build(&exported(pack), given, &model, &get, &member(pack, "info.cfg"), None).unwrap()?;
        let mut built = pack.to_vec();
        for (name, mot) in &out.mots {
            built = rebuild::replaced_member(&built, std::slice::from_ref(name), mot).unwrap();
        }
        built = rebuild::replaced_member(&built, &["info.cfg".to_string()], &out.info).unwrap();
        Some((out, built))
    }

    /// The bytes of an accessor, to edit in place.
    fn bytes_mut<'a>(gltf: &'a mut Gltf, number: &Value) -> &'a mut [u8] {
        let accessor = index(item(&gltf.json, "accessors").unwrap(), number).unwrap().clone();
        let view = index(item(&gltf.json, "bufferViews").unwrap(), item(&accessor, "bufferView").unwrap()).unwrap();
        let start = view.get("byteOffset").and_then(Value::as_i64).unwrap_or(0) as usize;
        let length = view.get("byteLength").and_then(Value::as_i64).unwrap() as usize;
        &mut Arc::make_mut(&mut gltf.buffers[0])[start..start + length]
    }

    fn animation<'a>(gltf: &'a Gltf, name: &str) -> &'a Value {
        let list = gltf.json.get("animations").unwrap().as_array().unwrap();
        list.iter().find(|a| a.get("name").and_then(Value::as_str) == Some(name)).unwrap()
    }

    fn animation_mut<'a>(gltf: &'a mut Gltf, name: &str) -> &'a mut Value {
        let Some(Value::Array(list)) = gltf.json.get_mut("animations") else {
            panic!("no animations");
        };
        list.iter_mut().find(|a| a.get("name").and_then(Value::as_str) == Some(name)).unwrap()
    }

    /// The output accessor of an animation's channel of a node and path.
    fn output_of(gltf: &Gltf, name: &str, node: i64, path: &str) -> Value {
        let a = animation(gltf, name).clone();
        let channel = a.get("channels").unwrap().as_array().unwrap().iter().find(|c| {
            let target = c.get("target").unwrap();
            target.get("node").unwrap().as_i64() == Some(node) && target.get("path").unwrap().as_str() == Some(path)
        });
        let sampler = index(a.get("samplers").unwrap(), channel.unwrap().get("sampler").unwrap()).unwrap();
        sampler.get("output").unwrap().clone()
    }

    /// Turns a node's rotation channel in an animation by `degrees` about x.
    fn turn(gltf: &mut Gltf, name: &str, node: i64, degrees: f64) {
        let output = output_of(gltf, name, node, "rotation");
        let half = degrees.to_radians() / 2.0;
        let (s, c) = (half.sin(), half.cos());
        for q in bytes_mut(gltf, &output).chunks_mut(16) {
            let v: Vec<f64> = q.chunks(4).map(|b| f32::from_le_bytes(b.try_into().unwrap()) as f64).collect();
            // q * (sin, 0, 0, cos)
            let turned = [v[3] * s + v[0] * c, v[1] * c + v[2] * s, v[2] * c - v[1] * s, v[3] * c - v[0] * s];
            for (slot, x) in q.chunks_mut(4).zip(turned) {
                slot.copy_from_slice(&(x as f32).to_le_bytes());
            }
        }
    }

    /// Edits the weights an animation gives a mesh node's morph target of this name, by key.
    fn edit_weights(gltf: &mut Gltf, name: &str, node: i64, target: &str, edit: &dyn Fn(usize, f32) -> f32) {
        let mesh = gltf.json.get("nodes").unwrap().as_array().unwrap()[node as usize].get("mesh").unwrap().clone();
        let names = index(gltf.json.get("meshes").unwrap(), &mesh)
            .unwrap()
            .get("extras")
            .unwrap()
            .get("targetNames")
            .unwrap()
            .clone();
        let names: Vec<&str> = names.as_array().unwrap().iter().map(|n| n.as_str().unwrap()).collect();
        let k = names.iter().position(|n| *n == target).unwrap_or_else(|| panic!("no target {target}"));
        let count = names.len();
        let output = output_of(gltf, name, node, "weights");
        for (at, w) in bytes_mut(gltf, &output).chunks_mut(4).enumerate() {
            if at % count == k {
                let x = f32::from_le_bytes(w.try_into().unwrap());
                w.copy_from_slice(&edit(at / count, x).to_le_bytes());
            }
        }
    }

    /// Stretches an animation in time.
    fn retime(gltf: &mut Gltf, name: &str, factor: f32) {
        let a = animation(gltf, name).clone();
        let input = a.get("samplers").unwrap().as_array().unwrap()[0].get("input").unwrap().clone();
        for t in bytes_mut(gltf, &input).chunks_mut(4) {
            let x = f32::from_le_bytes(t.try_into().unwrap());
            t.copy_from_slice(&(x * factor).to_le_bytes());
        }
    }

    fn names() -> TargetNames<'static> {
        static IDS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
        TargetNames { ids: IDS.get_or_init(|| vec!["0".to_string()]), primary: 0 }
    }

    /// Every animation of `wanted` is in `got` with the same values at every motion frame.
    fn assert_motions(got: &Gltf, wanted: &Gltf, model: &Mds) {
        let binds = HashMap::new();
        let (got, wanted) =
            (read(got, model, &binds, &names()).unwrap(), read(wanted, model, &binds, &names()).unwrap());
        for w in &wanted.animations {
            let g = got.animations.iter().find(|g| g.name == w.name).unwrap_or_else(|| panic!("no {}", w.name));
            assert_eq!(g.frames(), w.frames(), "{}", w.name);
            for &channel in g.curves.keys().chain(w.curves.keys()) {
                for r in 0..=w.frames() {
                    let t = r as f64 / FPS;
                    let (x, y) = (got.value(g, channel, t), wanted.value(w, channel, t));
                    let (Some(x), Some(y)) = (x, y) else {
                        panic!("{} {channel:?} at {r}: {x:?} {y:?}", w.name);
                    };
                    assert!(close(channel.1, &x, &y), "{} {channel:?} at {r}: {x:?} {y:?}", w.name);
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
        if let Some(Value::Array(nodes)) = bare.json.get_mut("nodes") {
            for node in nodes {
                if let Value::Object(items) = node {
                    items.retain(|key, _| key != "extras");
                }
            }
        }
        assert!(rebuilt(&pack, &bare).is_none());
    }

    #[test]
    fn the_vertex_and_material_lists_are_morph_targets() {
        let pack = sample_pack();
        let gltf = exported(&pack);
        let mesh = &gltf.json.get("meshes").unwrap().as_array().unwrap()[0];
        let names: Vec<&str> = mesh
            .get("extras")
            .unwrap()
            .get("targetNames")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_str().unwrap())
            .collect();
        let mut wanted: Vec<String> = (1..=20).map(|f| format!("c2_frame_{f}")).collect();
        wanted.push("c2_fade_0".into());
        assert_eq!(names, wanted);
        // walk's weights at its third frame (7): frame 7's shape whole, the fade at 7/20
        let output = output_of(&gltf, "walk", 2, "weights");
        let values = accessor(&gltf, &output).unwrap();
        let at = |k: usize| values.values[2 * 21 + k].f();
        assert_eq!(at(6), 1.0);
        assert_eq!(at(5), 0.0);
        assert!((at(20) - 0.35).abs() < 1e-6);
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
        assert_eq!(after.len(), 5);
        assert_eq!(after[..4], before[..4]);
        let frames_changed: Vec<i64> =
            after[4].keys.iter().zip(&before[4].keys).filter(|(a, b)| a != b).map(|(a, _)| a.0).collect();
        assert_eq!(frames_changed, [16, 17, 18, 19, 20]);
        assert_motions(&exported(&built), &given, &model(&pack));
    }

    #[test]
    fn a_morph_edit_reaches_the_vertex_and_material_lists() {
        let pack = sample_pack();
        let mut given = exported(&pack);
        // jump fades the other way
        edit_weights(&mut given, "jump", 2, "c2_fade_0", &|_, w| 1.0 - w);
        let (out, built) = rebuilt(&pack, &given).unwrap();
        assert_eq!(out.changed, ["jump"]);
        let (before, after) = (lists(&pack), lists(&built));
        let fade: Vec<(i64, i64)> = after[3].keys[15..].iter().map(|k| (k.0, (k.1[0] * 1e5).round() as i64)).collect();
        assert_eq!(fade, (16..=20).map(|f| (f, 100_000 - f * 5_000)).collect::<Vec<_>>());
        assert_eq!(after[2], before[2]);
        assert_motions(&exported(&built), &given, &model(&pack));
    }

    #[test]
    fn a_vertex_moved_in_a_shape_gets_its_list_and_the_group_keeps_its_frames() {
        let pack = sample_pack();
        let mut given = exported(&pack);
        // the third vertex (0, 1, 0) moves 2 along z in frame 18's shape
        let mesh = given.json.get("meshes").unwrap().as_array().unwrap()[0].clone();
        let primitive = &mesh.get("primitives").unwrap().as_array().unwrap()[0];
        let positions = accessor(&given, primitive.get("attributes").unwrap().get("POSITION").unwrap()).unwrap();
        let corner = (0..positions.len()).find(|&j| positions.at(j as i64).unwrap()[1].f() == 1.0).unwrap();
        let target = primitive.get("targets").unwrap().as_array().unwrap()[17].get("POSITION").unwrap().clone();
        let bytes = bytes_mut(&mut given, &target);
        bytes[12 * corner + 8..12 * corner + 12].copy_from_slice(&2.0f32.to_le_bytes());
        let (out, built) = rebuilt(&pack, &given).unwrap();
        assert_eq!(out.changed, ["jump"]);
        let after = lists(&built);
        let vertex: Vec<&KeyList> = after.iter().filter(|l| l.kind == VERTEX).collect();
        assert_eq!(vertex.len(), 2);
        assert_eq!((vertex[1].frame, vertex[1].target), (2, 3));
        assert_eq!(
            vertex[0].keys.iter().map(|k| k.0).collect::<Vec<_>>(),
            vertex[1].keys.iter().map(|k| k.0).collect::<Vec<_>>()
        );
        let at = |f: i64| vertex[1].keys.iter().find(|k| k.0 == f).unwrap().1.map(|x| (x * 1e5).round() / 1e5);
        assert_eq!(at(18), [0.0, 1.0, 2.0, 1.0]);
        assert_eq!(at(17), [0.0, 1.0, 0.0, 1.0]);
        // the group is consecutive: the new list right after the first
        let first = after.iter().position(|l| l.kind == VERTEX).unwrap();
        assert_eq!(after[first + 1].kind, VERTEX);
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
        // every list goes on into the new frames; the fade and the vertex as walk had them
        let after = lists(&built);
        assert!(after.iter().all(|list| list.keys.last().unwrap().0 == 31));
        let fade: Vec<f32> = after[3].keys[20..].iter().map(|k| k.1[0] as f32).collect();
        assert_eq!(fade, (5..=15).map(|f| f as f32 / 20.0).collect::<Vec<_>>());
        let lift: Vec<f32> = after[2].keys[20..].iter().map(|k| k.1[1] as f32).collect();
        assert_eq!(lift, (5..=15).map(|f| f as f32 / 10.0).collect::<Vec<_>>());
        assert_eq!(after[0].garbage, (7, 9));
        // the frames before keep their keys
        assert_eq!(after[4].keys[..20], lists(&pack)[4].keys[..]);
    }

    #[test]
    fn a_retimed_motion_and_a_new_one_get_frames_at_the_end() {
        let pack = sample_pack();
        let mut given = exported(&pack);
        retime(&mut given, "jump", 2.0);
        // a copy of stand, as Blender names a duplicated action
        let mut copy = animation(&given, "stand").clone();
        copy["name"] = Value::String("stand.001".into());
        if let Some(Value::Array(animations)) = given.json.get_mut("animations") {
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
    fn the_report_says_which_keys_were_added_moved_or_taken_out_and_which_members_changed() {
        let pack = sample_pack();
        let mut given = exported(&pack);
        retime(&mut given, "jump", 2.0);
        let mut spin = animation(&given, "stand").clone();
        spin["name"] = Value::String("spin".into());
        spin["extras"] = serde_json::json!({"step": 0.3});
        if let Some(Value::Array(animations)) = given.json.get_mut("animations") {
            animations.retain(|a| a.get("name").and_then(Value::as_str) != Some("walk"));
            animations.push(spin);
        }
        let (_, built) = rebuilt(&pack, &given).unwrap();
        assert_eq!(
            key_changes(&member(&pack, "info.cfg"), &member(&built, "info.cfg")),
            [
                "KEY \"jump\" of motion set 0: frames 21 to 29, step 0.30 (was frames 16 to 20, step 0.30)",
                "KEY \"spin\" added to motion set 0 as number 2: frames 30 to 34, step 0.30",
                "KEY \"walk\" taken out of motion set 0 (number 1)",
            ]
        );
        let changes = models::pack_changes(&pack, &built).unwrap();
        let mot = member(&built, "c.mot").len();
        let info = member(&built, "info.cfg").len();
        assert_eq!(
            changes[3..],
            [
                format!(
                    "members changed: info.cfg ({info} bytes, was {}), c.mot ({mot} bytes, was {})",
                    INFO.len(),
                    member(&pack, "c.mot").len()
                ),
                "members unchanged: c.mds".to_string(),
            ]
        );
        assert_eq!(models::pack_changes(&pack, &pack).unwrap(), ["unchanged"]);
    }

    #[test]
    fn a_channel_no_list_drives_gets_a_list_holding_the_rest_elsewhere() {
        let pack = sample_pack();
        let mut given = exported(&pack);
        // the root has no rotation list: jump turns it as it turns the hip
        let jump = animation_mut(&mut given, "jump");
        let Some(Value::Array(channels)) = jump.get_mut("channels") else {
            panic!("no channels");
        };
        let mut root = channels[0].clone();
        root["target"] = serde_json::json!({"node": 0i64, "path": "rotation"});
        channels.push(root);
        let (_, built) = rebuilt(&pack, &given).unwrap();
        let after = lists(&built);
        assert_eq!(after.len(), 6);
        // first, as the root's: the rest at 0, jump's frames, the rest around them and at the end
        let new = &after[0];
        assert_eq!((new.frame, new.kind), (0, 0));
        let frames: Vec<i64> = new.keys.iter().map(|k| k.0).collect();
        assert_eq!(frames, [0, 15, 16, 17, 18, 19, 20]);
        assert_eq!(new.keys[0].1, [1.0, 0.0, 0.0, 0.0]);
        assert_motions(&exported(&built), &given, &model(&pack));
    }

    #[test]
    fn a_deleted_action_takes_its_key_its_frames_and_its_sounds_out() {
        let pack = sample_pack();
        let mut given = exported(&pack);
        if let Some(Value::Array(animations)) = given.json.get_mut("animations") {
            animations.retain(|a| a.get("name").and_then(Value::as_str) != Some("jump"));
        }
        let (out, built) = rebuilt(&pack, &given).unwrap();
        assert_eq!(out.removed, ["jump"]);
        assert!(out.changed.is_empty());
        let info = String::from_utf8(member(&built, "info.cfg")).unwrap();
        assert_eq!(
            info,
            INFO.replace("\tKEY \"jump\", 16, 20, 0.30;\r\n", "").replace("\tSE \"jump\", 0, 0, 0.500;\r\n", "")
        );
        // frames 16 to 20 are gone from every list, the others as they were
        for (after, before) in lists(&built).iter().zip(lists(&pack)) {
            assert_eq!(after.keys, before.keys[..15]);
        }
        // walk, which shares a frame with stand, goes: stand keeps frame 5
        let mut given = exported(&pack);
        if let Some(Value::Array(animations)) = given.json.get_mut("animations") {
            animations.retain(|a| a.get("name").and_then(Value::as_str) != Some("walk"));
        }
        let (_, built) = rebuilt(&pack, &given).unwrap();
        let frames: Vec<i64> = lists(&built)[0].keys.iter().map(|k| k.0).collect();
        assert_eq!(frames, [1, 2, 3, 4, 5, 16, 17, 18, 19, 20]);
        assert_motions(&exported(&built), &given, &model(&pack));
    }

    /// The sample with a second motion set (3) of its own .mot: the hip bowing over frames 1-10.
    fn two_set_pack() -> Vec<u8> {
        let info = INFO.replace(
            "MOTION_END;\r\n",
            "MOTION_END;\r\nMOTION 3, \"bow.mot\", \"\", \"\";\r\nKEY_START;\r\n\tKEY \"bow\", 1, 10, 0.50;\r\nKEY_END;\r\nMOTION_END;\r\n",
        );
        let bow = mot::build(&[KeyList::new(1, 0, 0, (1..=10).map(|f| (f, engine_turn(0, 5.0 * f as f64))).collect())])
            .unwrap();
        sample_pack_with(&info, &[("bow.mot", &bow)])
    }

    #[test]
    fn other_motion_sets_are_animations_of_their_set() {
        let pack = two_set_pack();
        let gltf = exported(&pack);
        let names: Vec<&str> = gltf
            .json
            .get("animations")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.get("name").unwrap().as_str().unwrap())
            .collect();
        assert_eq!(names, ["stand", "walk", "jump", "3:bow"]);
        assert!(rebuilt(&pack, &gltf).is_none());
        // the bow edited and a new action of set 3: set 3's .mot and KEYs alone change
        let mut given = exported(&pack);
        turn(&mut given, "3:bow", 1, 20.0);
        let mut copy = animation(&given, "3:bow").clone();
        copy["name"] = Value::String("3:curtsy".into());
        if let Some(Value::Array(animations)) = given.json.get_mut("animations") {
            animations.push(copy);
        }
        let (out, built) = rebuilt(&pack, &given).unwrap();
        assert_eq!(out.changed, ["3:bow", "3:curtsy"]);
        assert_eq!(out.mots.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(), ["bow.mot"]);
        assert_eq!(member(&built, "c.mot"), member(&pack, "c.mot"));
        let info = String::from_utf8(member(&built, "info.cfg")).unwrap();
        assert!(
            info.contains(
                "\tKEY \"bow\", 1, 10, 0.50;\r\n\tKEY \"curtsy\", 11, 20, 0.50;\r\nKEY_END;\r\nMOTION_END;\r\n"
            ),
            "{info}"
        );
        assert_motions(&exported(&built), &given, &model(&pack));
    }

    #[test]
    fn a_translation_under_a_shrunk_frame_is_compared_as_it_moves_the_model() {
        let pack = sample_pack();
        let model = model(&pack);
        let motions = read(&exported(&pack), &model, &HashMap::new(), &names()).unwrap();
        let jump = &motions.animations[2];
        // the arm under the hip: scale 1, then the hip shrunk to 1e-4
        assert_eq!(ancestor_scale(&motions, jump, &model, 2, 0.0), 1.0);
        assert_eq!(slack(1e-4, 1e-4), 1e4);
        assert_eq!(slack(1e-4, 1.0), 1.0);
        let (a, b) = ([4.775, 0.0, 0.0, 0.0], [4.775, 0.03, 0.0, 0.0]);
        assert!(close_within(TRANSLATION, &a, &b, slack(1e-4, 1e-4)));
        assert!(!close_within(TRANSLATION, &a, &b, slack(1e-4, 1.0)));
        assert!(!close_within(ROTATION, &engine_turn(0, 0.0), &engine_turn(0, 1.0), 1e4));
    }

    /// Every character pack of an extracted disc (DC2TK_CORPUS) with a motion: its unedited export
    /// changes no motion, one bone of its first motion turned 30 degrees comes back from a fresh
    /// export of the rebuilt pack as given, every other motion as it was, and that motion's
    /// action deleted takes its KEY out and leaves every other motion as it was.
    #[test]
    #[ignore = "needs DC2TK_CORPUS"]
    fn corpus_motions_come_back_from_gltf() {
        use rayon::prelude::*;
        let corpus = dc2tk_disc::samples::corpus();
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
            let Some(info) = folder.get("info.cfg") else {
                return Ok(None);
            };
            if names.motion.is_none() {
                return Ok(None);
            }
            let model = mds::parse(folder.get(&names.model).unwrap()).unwrap();
            let gltf_of = |files: &[(String, Vec<u8>)]| {
                let get = |name: &str| files.iter().find(|(p, _)| p == name).map(|(_, d)| d.clone());
                load_gltf(&get(&format!("{}.gltf", names.stem)).unwrap(), &|uri| Ok(get(uri).unwrap())).unwrap()
            };
            let member = |name: &str| folder.get(name).map(<[u8]>::to_vec);
            let before = gltf_of(&files);
            let fail = |what: String| format!("{}: {what}", path.display());
            let run =
                |given: &Gltf| build(&before, given, &model, &member, info, None).map_err(|e| fail(e.to_string()));
            if let Some(out) = run(&before)? {
                return Err(fail(format!("unedited, changed {:?} removed {:?}", out.changed, out.removed)));
            }
            // nor as Blender names it: no extras, a name given again numbered ".001" on
            let mut bare = gltf_of(&files);
            if let Some(Value::Array(nodes)) = bare.json.get_mut("nodes") {
                let mut seen: HashMap<String, usize> = HashMap::new();
                for node in nodes {
                    if let Value::Object(items) = node {
                        items.retain(|key, _| key != "extras");
                    }
                    if let Some(name) = node.get("name").and_then(Value::as_str).map(str::to_string) {
                        let count = seen.entry(name.clone()).or_default();
                        if *count > 0 {
                            node["name"] = Value::String(format!("{name}.{count:03}"));
                        }
                        *count += 1;
                    }
                }
            }
            if let Some(out) = run(&bare)? {
                return Err(fail(format!(
                    "unedited without extras, changed {:?} removed {:?}",
                    out.changed, out.removed
                )));
            }
            // the first motion turning a bone
            let Some((name, node)) = before.json.get("animations").and_then(Value::as_array).and_then(|list| {
                list.iter().find_map(|a| {
                    let channel = a.get("channels")?.as_array()?.iter().find(|c| {
                        c.get("target").and_then(|t| t.get("path")).and_then(Value::as_str) == Some("rotation")
                    })?;
                    Some((a.get("name")?.as_str()?.to_string(), channel.get("target")?.get("node")?.as_i64()?))
                })
            }) else {
                return Ok(None);
            };
            let mut given = gltf_of(&files);
            turn(&mut given, &name, node, 30.0);
            let Some(out) = run(&given)? else {
                return Err(fail(format!("{name} turned, nothing changed")));
            };
            if out.changed != [name.clone()] || !out.removed.is_empty() {
                return Err(fail(format!("{name} turned, changed {:?} removed {:?}", out.changed, out.removed)));
            }
            let mut built = pack.to_vec();
            for (member, mot) in &out.mots {
                built = rebuild::replaced_member(&built, std::slice::from_ref(member), mot).unwrap();
            }
            built = rebuild::replaced_member(&built, &["info.cfg".to_string()], &out.info).unwrap();
            let again = gltf_of(&models::export(&built).map_err(|e| fail(e.to_string()))?);
            std::panic::catch_unwind(|| assert_motions(&again, &given, &model))
                .map_err(|e| fail(e.downcast_ref::<String>().cloned().unwrap_or_default()))?;
            let in_place = out.info == info;
            // that motion's action deleted: its KEY alone goes, every other motion as it was (a
            // glTF with no animation left deletes none: a tool may have left them all out)
            let count = |g: &Gltf| g.json.get("animations").and_then(Value::as_array).map_or(0, Vec::len);
            // (nor a KEY whose name another repeats: that one's export name, `name_<start>`, would
            // become `name`)
            let names: Vec<&str> = before
                .json
                .get("animations")
                .and_then(Value::as_array)
                .map(|list| list.iter().filter_map(|a| a.get("name").and_then(Value::as_str)).collect())
                .unwrap_or_default();
            if count(&before) < 2 || names.iter().any(|other| other.starts_with(&format!("{name}_"))) {
                return Ok(Some(in_place));
            }
            let mut given = gltf_of(&files);
            if let Some(Value::Array(animations)) = given.json.get_mut("animations") {
                animations.retain(|a| a.get("name").and_then(Value::as_str) != Some(name.as_str()));
            }
            let Some(out) = run(&given)? else {
                return Err(fail(format!("{name} deleted, nothing changed")));
            };
            if !out.changed.is_empty() || out.removed.len() != 1 {
                return Err(fail(format!("{name} deleted, changed {:?} removed {:?}", out.changed, out.removed)));
            }
            let mut built = pack.to_vec();
            for (member, mot) in &out.mots {
                built = rebuild::replaced_member(&built, std::slice::from_ref(member), mot).unwrap();
            }
            built = rebuild::replaced_member(&built, &["info.cfg".to_string()], &out.info).unwrap();
            let again = gltf_of(&models::export(&built).map_err(|e| fail(e.to_string()))?);
            if count(&again) + 1 != count(&before) {
                return Err(fail(format!("{name} deleted, {} animations of {}", count(&again), count(&before))));
            }
            std::panic::catch_unwind(|| assert_motions(&again, &given, &model)).map_err(|e| {
                fail(format!("{name} deleted: {}", e.downcast_ref::<String>().cloned().unwrap_or_default()))
            })?;
            Ok(Some(in_place))
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
