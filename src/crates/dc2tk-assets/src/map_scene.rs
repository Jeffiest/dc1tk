//! A map as a scene tree, the glTF layout of docs/assets/maps.md section 8, without the meshes:
//! the root (`map:`), a node per placement (`place:`) holding its template's pieces (`piece:`)
//! and points (`point:`), the map's own points and the fixed cameras. `build_scene` makes it from
//! a .map; `read_scene` reads an edited one back into a `SceneEdit` for `map_edit::rebuild`.
//!
//! Every node carries `extras.c2`. Blender renames repeated nodes (`.001`), reorders children and
//! turns ints into floats in mixed lists, so reading back takes names from `extras.c2.name`,
//! order from `extras.c2.order` and keeps a number's original form wherever its value did not
//! change (section 8.1).

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::map_edit::{PlacementEdit, SceneEdit};
use crate::map_transform::{self, Trs};
use crate::mapfile::{
    AnimeData, ColType, EventData, FireData, FixCamera, InventData, MapFile, Num, Piece, PieceMaterial, Placement,
    PlightData, Point, PointData, PointKind, Settings, SoundData,
};
use crate::{Context, Error, Result};

/// A node of the scene. `model` is set on pieces, whose meshes the exporter adds under them.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneNode {
    pub name: String,
    pub trs: Option<Trs>,
    pub extras: Value,
    pub children: Vec<SceneNode>,
    pub model: Option<String>,
}

impl SceneNode {
    fn group(name: &str, children: Vec<SceneNode>) -> SceneNode {
        SceneNode { name: name.into(), trs: None, extras: json!({"c2": {"role": "group"}}), children, model: None }
    }

    pub fn c2(&self) -> &Value {
        &self.extras["c2"]
    }

    pub fn role(&self) -> &str {
        self.c2()["role"].as_str().unwrap_or("")
    }

    /// The exact name: `extras.c2.name`, else the node's name without its prefix and a Blender
    /// `.NNN` suffix.
    pub fn exact_name(&self) -> String {
        if let Some(name) = self.c2()["name"].as_str() {
            return name.to_string();
        }
        let name = self.name.split_once(':').map_or(self.name.as_str(), |(_, rest)| rest);
        match name.rsplit_once('.') {
            Some((stem, digits)) if digits.len() == 3 && digits.chars().all(|c| c.is_ascii_digit()) => stem.into(),
            _ => name.into(),
        }
    }
}

// ---------- numbers ----------

/// The value a number was written as (for an exponent the written value, not the 0 the engine
/// reads), for comparing.
fn written(num: Num) -> f64 {
    match num {
        Num::Int(value) => value as f64,
        Num::Float(_) => num.text().parse().unwrap_or(0.0),
        Num::Exponent { exponent } => exponent as f64,
    }
}

fn num_json(num: Num) -> Value {
    match num {
        Num::Int(value) => json!(value),
        _ => json!(written(num)),
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-5 * a.abs().max(b.abs()).max(1.0)
}

/// A number read back: the original when its value did not change, else what the file will hold.
fn num_from(value: &Value, original: Option<Num>, what: &str) -> Result<Num> {
    let Some(number) = value.as_f64() else {
        return Err(Error::invalid(format!("{what}: {value} is not a number")));
    };
    if let Some(original) = original {
        if close(written(original), number) {
            return Ok(original);
        }
    }
    match value.as_i64() {
        Some(int) if i32::try_from(int).is_ok() => Ok(Num::Int(int as i32)),
        _ => Ok(Num::from(number as f32)),
    }
}

fn nums_json<const N: usize>(nums: &[Num; N]) -> Value {
    Value::Array(nums.iter().map(|num| num_json(*num)).collect())
}

fn nums_from<const N: usize>(value: &Value, original: Option<&[Num; N]>, what: &str) -> Result<[Num; N]> {
    let items = value.as_array().filter(|items| items.len() == N);
    let Some(items) = items else {
        return Err(Error::invalid(format!("{what}: {value} is not a list of {N} numbers")));
    };
    let mut out = [Num::Int(0); N];
    for (at, item) in items.iter().enumerate() {
        out[at] = num_from(item, original.map(|original| original[at]), what)?;
    }
    Ok(out)
}

fn text_from(value: &Value, what: &str) -> Result<String> {
    value.as_str().map(str::to_string).ok_or_else(|| Error::invalid(format!("{what}: {value} is not text")))
}

/// Keeps the original form of every number of `edited` whose value is the original's (objects by
/// key, lists by position), so JSON that went through Blender reads back as it was.
pub fn reconcile(original: &Value, edited: &Value) -> Value {
    match (original, edited) {
        (Value::Number(a), Value::Number(b)) => match (a.as_f64(), b.as_f64()) {
            (Some(x), Some(y)) if close(x, y) => original.clone(),
            _ => edited.clone(),
        },
        (Value::Object(a), Value::Object(b)) => Value::Object(
            b.iter()
                .map(|(key, value)| (key.clone(), a.get(key).map_or(value.clone(), |old| reconcile(old, value))))
                .collect(),
        ),
        (Value::Array(a), Value::Array(b)) => Value::Array(
            b.iter()
                .enumerate()
                .map(|(at, value)| a.get(at).map_or(value.clone(), |old| reconcile(old, value)))
                .collect(),
        ),
        _ => edited.clone(),
    }
}

// ---------- point data ----------

fn data_json(data: &PointData) -> Value {
    match data {
        PointData::None => json!({}),
        PointData::Invent(d) => json!({
            "id": num_json(d.id), "min": nums_json(&d.min), "max": nums_json(&d.max),
            "unknown_int": num_json(d.int_param), "unknown_float": num_json(d.float_param), "angle": num_json(d.angle),
        }),
        PointData::Event(d) => json!({
            "kind": d.kind, "event_no": num_json(d.event_no), "program": num_json(d.program),
            "args": nums_json(&d.args), "target": d.target,
            "flags": nums_json(&[d.action_button, d.use_item, d.flag_100]),
        }),
        PointData::Fire(d) => {
            let mut out = json!({"color": nums_json(&d.color), "draw": num_json(d.draw)});
            if let Some(extra) = &d.extra {
                out["extra"] = nums_json(extra);
            }
            out
        }
        PointData::Plight(d) => json!({
            "power": num_json(d.power), "color": nums_json(&d.color), "mode": num_json(d.mode),
            "args": nums_json(&d.ints), "flicker": num_json(d.flicker_type),
            "depth": num_json(d.flicker_depth), "period": num_json(d.flicker_period),
        }),
        PointData::Effect(name) => json!({"effect": name}),
        PointData::Sound(d) => json!({
            "number": num_json(d.number), "near": num_json(d.near), "far": num_json(d.far),
            "unknown_int": num_json(d.int_param), "shape": num_json(d.shape),
            "start": nums_json(&d.start), "end": nums_json(&d.end),
        }),
        PointData::Anime(d) => {
            let mut out = json!({
                "placement": d.placement, "piece": d.piece, "frame": d.frame, "drives": num_json(d.drives),
                "mode": num_json(d.mode), "start": nums_json(&d.start), "step": nums_json(&d.step),
                "end": nums_json(&d.end),
            });
            if let Some([uniform, relative]) = d.flags {
                out["uniform"] = num_json(uniform);
                out["relative"] = num_json(relative);
            }
            out
        }
    }
}

/// A point's data read back; fields the scene leaves out keep the original's (or zero).
fn data_from(kind: PointKind, value: &Value, original: Option<&PointData>) -> Result<PointData> {
    let field = |name: &str| &value[name];
    let num = |name: &str, old: Option<Num>| -> Result<Num> {
        match field(name) {
            Value::Null => Ok(old.unwrap_or(Num::Int(0))),
            found => num_from(found, old, name),
        }
    };
    fn nums<const N: usize>(found: &Value, name: &str, old: Option<&[Num; N]>) -> Result<[Num; N]> {
        match found {
            Value::Null => Ok(old.copied().unwrap_or([Num::Int(0); N])),
            found => nums_from(found, old, name),
        }
    }
    let text = |name: &str, old: Option<&String>| -> Result<String> {
        match field(name) {
            Value::Null => Ok(old.cloned().unwrap_or_default()),
            found => text_from(found, name),
        }
    };
    let extra = |old: Option<&Option<[Num; 2]>>| -> Result<Option<[Num; 2]>> {
        match field("extra") {
            Value::Null => Ok(None),
            found => Ok(Some(nums_from(found, old.and_then(Option::as_ref), "extra")?)),
        }
    };
    Ok(match kind {
        PointKind::Pos => PointData::None,
        PointKind::Invent => {
            let o = match original {
                Some(PointData::Invent(o)) => Some(o),
                _ => None,
            };
            PointData::Invent(InventData {
                id: num("id", o.map(|o| o.id))?,
                min: nums(field("min"), "min", o.map(|o| &o.min))?,
                max: nums(field("max"), "max", o.map(|o| &o.max))?,
                int_param: num("unknown_int", o.map(|o| o.int_param))?,
                float_param: num("unknown_float", o.map(|o| o.float_param))?,
                angle: num("angle", o.map(|o| o.angle))?,
            })
        }
        PointKind::Event => {
            let o = match original {
                Some(PointData::Event(o)) => Some(o),
                _ => None,
            };
            let flags = nums(field("flags"), "flags", o.map(|o| [o.action_button, o.use_item, o.flag_100]).as_ref())?;
            PointData::Event(EventData {
                kind: text("kind", o.map(|o| &o.kind))?,
                event_no: num("event_no", o.map(|o| o.event_no))?,
                program: num("program", o.map(|o| o.program))?,
                args: nums(field("args"), "args", o.map(|o| &o.args))?,
                target: text("target", o.map(|o| &o.target))?,
                action_button: flags[0],
                use_item: flags[1],
                flag_100: flags[2],
            })
        }
        PointKind::Fire | PointKind::Flare => {
            let o = match original {
                Some(PointData::Fire(o)) => Some(o),
                _ => None,
            };
            PointData::Fire(FireData {
                color: nums(field("color"), "color", o.map(|o| &o.color))?,
                draw: num("draw", o.map(|o| o.draw))?,
                extra: extra(o.map(|o| &o.extra))?,
            })
        }
        PointKind::Plight => {
            let o = match original {
                Some(PointData::Plight(o)) => Some(o),
                _ => None,
            };
            PointData::Plight(PlightData {
                power: num("power", o.map(|o| o.power))?,
                color: nums(field("color"), "color", o.map(|o| &o.color))?,
                mode: num("mode", o.map(|o| o.mode))?,
                ints: nums(field("args"), "args", o.map(|o| &o.ints))?,
                flicker_type: num("flicker", o.map(|o| o.flicker_type))?,
                flicker_depth: num("depth", o.map(|o| o.flicker_depth))?,
                flicker_period: num("period", o.map(|o| o.flicker_period))?,
            })
        }
        PointKind::Effect => {
            let o = match original {
                Some(PointData::Effect(o)) => Some(o),
                _ => None,
            };
            PointData::Effect(text("effect", o)?)
        }
        PointKind::Sound => {
            let o = match original {
                Some(PointData::Sound(o)) => Some(o),
                _ => None,
            };
            PointData::Sound(SoundData {
                number: num("number", o.map(|o| o.number))?,
                near: num("near", o.map(|o| o.near))?,
                far: num("far", o.map(|o| o.far))?,
                int_param: num("unknown_int", o.map(|o| o.int_param))?,
                shape: num("shape", o.map(|o| o.shape))?,
                start: nums(field("start"), "start", o.map(|o| &o.start))?,
                end: nums(field("end"), "end", o.map(|o| &o.end))?,
            })
        }
        PointKind::Anime => {
            let o = match original {
                Some(PointData::Anime(o)) => Some(o),
                _ => None,
            };
            // Scenes exported before the fields were decoded name them kind, unknown_int, v0, v1,
            // v2 and extra.
            let named = |name: &str, old: &str| match field(name) {
                Value::Null => field(old),
                found => found,
            };
            let num = |name: &str, old_name: &str, old: Option<Num>| match named(name, old_name) {
                Value::Null => Ok(old.unwrap_or(Num::Int(0))),
                found => num_from(found, old, name),
            };
            let flags = o.and_then(|o| o.flags);
            let flags = match (field("uniform"), field("relative"), field("extra")) {
                (Value::Null, Value::Null, Value::Null) => None,
                (Value::Null, Value::Null, extra) => Some(nums_from(extra, flags.as_ref(), "extra")?),
                (uniform, relative, _) => {
                    let one = |found: &Value, name: &str, at: usize| match found {
                        Value::Null => Ok(flags.map_or(Num::Int(0), |flags| flags[at])),
                        found => num_from(found, flags.map(|flags| flags[at]), name),
                    };
                    Some([one(uniform, "uniform", 0)?, one(relative, "relative", 1)?])
                }
            };
            PointData::Anime(AnimeData {
                placement: text("placement", o.map(|o| &o.placement))?,
                piece: text("piece", o.map(|o| &o.piece))?,
                frame: text("frame", o.map(|o| &o.frame))?,
                drives: num("drives", "kind", o.map(|o| o.drives))?,
                mode: num("mode", "unknown_int", o.map(|o| o.mode))?,
                start: nums(named("start", "v0"), "start", o.map(|o| &o.start))?,
                step: nums(named("step", "v1"), "step", o.map(|o| &o.step))?,
                end: nums(named("end", "v2"), "end", o.map(|o| &o.end))?,
                flags,
            })
        }
    })
}

// ---------- building the scene ----------

fn point_node(point: &Point, order: usize) -> SceneNode {
    SceneNode {
        name: format!("point:{}", point.name),
        trs: Some(map_transform::to_trs(&point.pos, &point.rot, &point.scale)),
        extras: json!({"c2": {
            "role": "point", "name": point.name, "order": order, "kind": point.kind.name(),
            "enable": num_json(point.enable), "flag": nums_json(&point.flag), "data": data_json(&point.data),
        }}),
        children: Vec::new(),
        model: None,
    }
}

fn col_type_json(col_type: &Option<ColType>) -> Value {
    match col_type {
        None => Value::Null,
        Some(ColType { kind, param: None }) => num_json(*kind),
        Some(ColType { kind, param: Some(param) }) => json!([num_json(*kind), num_json(*param)]),
    }
}

fn materials_json(materials: &Option<Vec<PieceMaterial>>) -> Value {
    match materials {
        None => Value::Null,
        Some(list) => Value::Array(
            list.iter()
                .map(|m| json!({"frame": m.frame, "material": num_json(m.material), "color": nums_json(&m.color), "flag": num_json(m.flag)}))
                .collect(),
        ),
    }
}

fn piece_node(piece: &Piece, order: usize, kind: &str) -> SceneNode {
    SceneNode {
        name: format!("piece:{}", piece.name),
        trs: Some(map_transform::to_trs(&piece.pos, &piece.rot, &piece.scale)),
        extras: json!({"c2": {
            "role": "piece", "name": piece.name, "order": order, "model": piece.model, "kind": kind,
            "show": num_json(piece.show), "time": nums_json(&piece.time),
            "col_type": col_type_json(&piece.col_type), "materials": materials_json(&piece.materials),
        }}),
        children: Vec::new(),
        model: Some(piece.model.clone()),
    }
}

/// How far apart template previews stand: a dungeon cell (320) and half again.
pub const PREVIEW_SPACING: i32 = 480;
/// Previews a row holds before the next row of the same family.
pub const PREVIEW_ROW: usize = 10;

/// A template library (a map placing none of its templates: the dungeon floors' parts, which the
/// floor generator places at run time, and the add maps) shown as one placement of each template,
/// marked `preview`, so its pieces can be seen and edited; reading back folds a preview's pieces
/// and points into its template and never makes it a placement. Templates are laid out on the
/// ground in rows by family (the name before its digits: `way`, `room`, `door`, ...), the first of
/// a repeated name only, as the engine finds it.
fn template_previews(map: &MapFile, kind_of: &dyn Fn(&str) -> String) -> Vec<SceneNode> {
    let mut families: Vec<(String, usize)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut row = 0usize;
    let mut out = Vec::new();
    for template in &map.templates {
        if !seen.insert(template.name.as_str()) {
            continue;
        }
        let family = template.name.trim_end_matches(|c: char| c.is_ascii_digit()).to_string();
        let column = match families.iter_mut().find(|(known, _)| *known == family) {
            Some((_, count)) => {
                *count += 1;
                *count - 1
            }
            None => {
                if !families.is_empty() {
                    row += families.last().map_or(1, |(_, count)| count.div_ceil(PREVIEW_ROW));
                }
                families.push((family, 1));
                0
            }
        };
        let (x, z) = ((column % PREVIEW_ROW) as i32, (row + column / PREVIEW_ROW) as i32);
        let pos = [Num::Int(x * PREVIEW_SPACING), Num::Int(0), Num::Int(z * PREVIEW_SPACING)];
        let mut children: Vec<SceneNode> = template
            .pieces
            .iter()
            .enumerate()
            .map(|(order, piece)| piece_node(piece, order, &kind_of(&piece.model)))
            .collect();
        children.extend(template.points.iter().flatten().enumerate().map(|(order, point)| point_node(point, order)));
        out.push(SceneNode {
            name: format!("place:{}", template.name),
            trs: Some(map_transform::to_trs(&pos, &[Num::Int(0); 3], &[Num::Int(1); 3])),
            extras: json!({"c2": {
                "role": "placement", "name": template.name, "template": template.name, "preview": true,
                "show": 1, "group": "", "far_clip": Value::Null,
            }}),
            children,
            model: None,
        });
    }
    out
}

/// The scene of a map. `kind_of` gives a piece model's kind (model, collision, camera, chara) from
/// its pack's info.cfg TYPE.
pub fn build_scene(map: &MapFile, map_name: &str, kind_of: &dyn Fn(&str) -> String) -> Result<SceneNode> {
    let mut templates: HashMap<&str, usize> = HashMap::new();
    for (at, template) in map.templates.iter().enumerate() {
        templates.entry(template.name.as_str()).or_insert(at);
    }
    let mut placements = Vec::new();
    for (slot, placement) in map.placements.iter().enumerate() {
        let template = templates.get(placement.template.as_str()).map(|&at| &map.templates[at]);
        let mut children: Vec<SceneNode> = Vec::new();
        if let Some(template) = template {
            children.extend(
                template
                    .pieces
                    .iter()
                    .enumerate()
                    .map(|(order, piece)| piece_node(piece, order, &kind_of(&piece.model))),
            );
            children
                .extend(template.points.iter().flatten().enumerate().map(|(order, point)| point_node(point, order)));
        }
        placements.push(SceneNode {
            name: format!("place:{}", placement.name),
            trs: Some(map_transform::to_trs(&placement.pos, &placement.rot, &placement.scale)),
            extras: json!({"c2": {
                "role": "placement", "name": placement.name, "slot": slot, "template": placement.template,
                "show": num_json(placement.show), "group": placement.group,
                "far_clip": placement.far_clip.as_ref().map_or(Value::Null, nums_json),
            }}),
            children,
            model: None,
        });
    }
    if map.placements.is_empty() {
        placements.extend(template_previews(map, kind_of));
    }
    let points = map.points.iter().flatten().enumerate().map(|(order, point)| point_node(point, order)).collect();
    let cameras = map
        .cameras
        .iter()
        .flatten()
        .enumerate()
        .map(|(index, camera)| SceneNode {
            name: format!("camera:{index}"),
            trs: Some(map_transform::to_trs(&camera.pos, &[Num::Int(0); 3], &[Num::Int(1); 3])),
            extras: json!({"c2": {"role": "camera", "name": format!("{index}"), "index": index}}),
            children: Vec::new(),
            model: None,
        })
        .collect();
    let tree = json!({
        "images": map.images, "packs": map.packs, "header_order": map.header_order,
        "settings": map.settings, "cameras": map.cameras,
    });
    Ok(SceneNode {
        name: format!("map:{map_name}"),
        trs: None,
        extras: json!({"c2": {"role": "map", "name": map_name, "map": tree}}),
        children: vec![
            SceneNode::group("placements", placements),
            SceneNode::group("points", points),
            SceneNode::group("cameras", cameras),
        ],
        model: None,
    })
}

// ---------- reading it back ----------

fn transform_of(
    node: &SceneNode,
    original: Option<(&[Num; 3], &[Num; 3], &[Num; 3])>,
) -> ([Num; 3], [Num; 3], [Num; 3]) {
    match (&node.trs, original) {
        (Some(trs), original) => map_transform::from_trs(trs, original),
        (None, Some((pos, rot, scale))) => (*pos, *rot, *scale),
        (None, None) => ([Num::Int(0); 3], [Num::Int(0); 3], [Num::Int(1); 3]),
    }
}

/// Children of a role in their scene order: by `order`, new ones (no order) after, by name.
fn ordered<'a>(children: &'a [SceneNode], role: &str) -> Vec<&'a SceneNode> {
    let mut found: Vec<&SceneNode> = children.iter().filter(|child| child.role() == role).collect();
    found.sort_by(|a, b| {
        let key = |node: &SceneNode| (node.c2()["order"].as_u64().unwrap_or(u64::MAX), node.exact_name());
        key(a).cmp(&key(b))
    });
    found
}

fn read_point(node: &SceneNode, original: Option<&Point>) -> Result<Point> {
    let c2 = node.c2();
    let name = node.exact_name();
    let what = format!("point {name}");
    let kind = c2["kind"]
        .as_str()
        .and_then(PointKind::from_name)
        .ok_or_else(|| Error::invalid(format!("{what}: unknown kind {}", c2["kind"])))?;
    let original = original.filter(|point| point.kind == kind);
    let (pos, rot, scale) = transform_of(node, original.map(|p| (&p.pos, &p.rot, &p.scale)));
    Ok(Point {
        kind,
        enable: match &c2["enable"] {
            Value::Null => original.map_or(Num::Int(1), |p| p.enable),
            value => num_from(value, original.map(|p| p.enable), &what)?,
        },
        name,
        flag: match &c2["flag"] {
            Value::Null => original.map_or([Num::Int(0); 4], |p| p.flag),
            value => nums_from(value, original.map(|p| &p.flag), &what)?,
        },
        data: data_from(kind, &c2["data"], original.map(|p| &p.data))?,
        pos,
        rot,
        scale,
    })
}

fn read_piece(node: &SceneNode, original: Option<&Piece>) -> Result<Piece> {
    let c2 = node.c2();
    let name = node.exact_name();
    let what = format!("piece {name}");
    let (pos, rot, scale) = transform_of(node, original.map(|p| (&p.pos, &p.rot, &p.scale)));
    let col_type = match &c2["col_type"] {
        Value::Null => None,
        Value::Array(pair) if pair.len() == 2 => Some(ColType {
            kind: num_from(&pair[0], original.and_then(|p| p.col_type.as_ref()).map(|c| c.kind), &what)?,
            param: Some(num_from(&pair[1], original.and_then(|p| p.col_type.as_ref()).and_then(|c| c.param), &what)?),
        }),
        value => Some(ColType {
            kind: num_from(value, original.and_then(|p| p.col_type.as_ref()).map(|c| c.kind), &what)?,
            param: None,
        }),
    };
    let materials = match &c2["materials"] {
        Value::Null => None,
        Value::Array(list) => {
            let old = original.and_then(|p| p.materials.as_ref());
            let mut out = Vec::new();
            for (at, item) in list.iter().enumerate() {
                let o = old.and_then(|old| old.get(at));
                out.push(PieceMaterial {
                    frame: text_from(&item["frame"], &what)?,
                    material: num_from(&item["material"], o.map(|o| o.material), &what)?,
                    color: nums_from(&item["color"], o.map(|o| &o.color), &what)?,
                    flag: num_from(&item["flag"], o.map(|o| o.flag), &what)?,
                });
            }
            Some(out)
        }
        value => return Err(Error::invalid(format!("{what}: materials {value} is not a list"))),
    };
    Ok(Piece {
        model: c2["model"]
            .as_str()
            .map(str::to_string)
            .or_else(|| original.map(|p| p.model.clone()))
            .unwrap_or_default(),
        show: match &c2["show"] {
            Value::Null => original.map_or(Num::Int(1), |p| p.show),
            value => num_from(value, original.map(|p| p.show), &what)?,
        },
        name,
        pos,
        rot,
        scale,
        materials,
        col_type,
        time: match &c2["time"] {
            Value::Null => original.map_or([Num::Int(0); 2], |p| p.time),
            value => nums_from(value, original.map(|p| &p.time), &what)?,
        },
    })
}

/// The original of a piece or point read back: the one at its `order` when the name matches,
/// else the first of its name.
fn original_of<'a, T>(list: &'a [T], node: &SceneNode, name_of: impl Fn(&T) -> &str) -> Option<&'a T> {
    let name = node.exact_name();
    let at = node.c2()["order"].as_u64().map(|order| order as usize);
    at.and_then(|at| list.get(at))
        .filter(|item| name_of(item) == name)
        .or_else(|| list.iter().find(|item| name_of(item) == name))
}

/// The edited scene as a `SceneEdit` against the original map.
pub fn read_scene(root: &SceneNode, original: &MapFile) -> Result<SceneEdit> {
    if root.role() != "map" {
        return Err(Error::invalid(format!("{} is not a map scene (extras.c2.role is not \"map\")", root.name)));
    }
    let group = |name: &str| root.children.iter().find(|child| child.role() == "group" && child.exact_name() == name);
    let mut templates: HashMap<&str, usize> = HashMap::new();
    for (at, template) in original.templates.iter().enumerate() {
        templates.entry(template.name.as_str()).or_insert(at);
    }

    let tree = &root.c2()["map"];
    let settings: Settings = match &tree["settings"] {
        Value::Null => original.settings.clone(),
        value => {
            let reconciled = reconcile(&serde_json::to_value(&original.settings).unwrap_or(Value::Null), value);
            serde_json::from_value(reconciled).context("map settings")?
        }
    };
    let mut cameras: Option<Vec<FixCamera>> = match &tree["cameras"] {
        Value::Null => None,
        value => {
            let reconciled = reconcile(&serde_json::to_value(&original.cameras).unwrap_or(Value::Null), value);
            serde_json::from_value(reconciled).context("map cameras")?
        }
    };
    if let (Some(list), Some(group)) = (cameras.as_mut(), group("cameras")) {
        for node in group.children.iter().filter(|child| child.role() == "camera") {
            let index = node.c2()["index"].as_u64().map(|index| index as usize);
            if let (Some(camera), Some(trs)) = (index.and_then(|index| list.get_mut(index)), &node.trs) {
                camera.pos = map_transform::from_trs(trs, Some((&camera.pos, &[Num::Int(0); 3], &[Num::Int(1); 3]))).0;
            }
        }
    }

    let mut placements = Vec::new();
    for node in group("placements").map(|group| group.children.as_slice()).unwrap_or(&[]) {
        if node.role() != "placement" {
            continue;
        }
        let c2 = node.c2();
        let name = node.exact_name();
        let what = format!("placement {name}");
        let preview = c2["preview"].as_bool().unwrap_or(false);
        let slot =
            c2["slot"].as_u64().map(|slot| slot as usize).filter(|&slot| slot < original.placements.len() && !preview);
        let old = slot.map(|slot| &original.placements[slot]);
        let template_name = c2["template"].as_str().map(str::to_string).or_else(|| old.map(|p| p.template.clone()));
        let template_name = template_name.ok_or_else(|| Error::invalid(format!("{what}: no template")))?;
        let template = templates.get(template_name.as_str()).map(|&at| &original.templates[at]);
        let (pos, rot, scale) = transform_of(node, old.map(|p| (&p.pos, &p.rot, &p.scale)));
        let placement = Placement {
            name,
            show: match &c2["show"] {
                Value::Null => old.map_or(Num::Int(1), |p| p.show),
                value => num_from(value, old.map(|p| p.show), &what)?,
            },
            far_clip: match &c2["far_clip"] {
                Value::Null => None,
                value => Some(nums_from(value, old.and_then(|p| p.far_clip.as_ref()), &what)?),
            },
            template: template_name,
            group: c2["group"]
                .as_str()
                .map(str::to_string)
                .or_else(|| old.map(|p| p.group.clone()))
                .unwrap_or_default(),
            pos,
            rot,
            scale,
        };
        let old_pieces = template.map(|t| t.pieces.as_slice()).unwrap_or(&[]);
        let pieces = ordered(&node.children, "piece")
            .into_iter()
            .map(|piece| read_piece(piece, original_of(old_pieces, piece, |p: &Piece| p.name.as_str())))
            .collect::<Result<Vec<_>>>()?;
        let old_points = template.and_then(|t| t.points.as_deref()).unwrap_or(&[]);
        let point_nodes = ordered(&node.children, "point");
        let points = if point_nodes.is_empty() && template.is_none_or(|t| t.points.is_none()) {
            None
        } else {
            Some(
                point_nodes
                    .into_iter()
                    .map(|point| read_point(point, original_of(old_points, point, |p: &Point| p.name.as_str())))
                    .collect::<Result<Vec<_>>>()?,
            )
        };
        placements.push(PlacementEdit { slot, placement, pieces, points, preview });
    }

    let old_points = original.points.as_deref().unwrap_or(&[]);
    let point_nodes: Vec<&SceneNode> =
        group("points").map(|group| ordered(&group.children, "point")).unwrap_or_default();
    let points = if point_nodes.is_empty() && original.points.is_none() {
        None
    } else {
        Some(
            point_nodes
                .into_iter()
                .map(|point| read_point(point, original_of(old_points, point, |p: &Point| p.name.as_str())))
                .collect::<Result<Vec<_>>>()?,
        )
    };
    Ok(SceneEdit { settings, cameras, placements, points })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{map_edit, mapfile};

    fn kind_of(model: &str) -> String {
        if model.contains("-a") {
            "collision"
        } else if model.contains("-c") {
            "camera"
        } else {
            "model"
        }
        .into()
    }

    /// Every map of an extracted disc (C2S_CORPUS) goes to a scene and back unchanged, also after
    /// a trip through JSON as Blender would leave it (names suffixed, children reversed, ints as
    /// floats).
    #[test]
    fn corpus_scenes_read_back_unchanged() {
        let Some(corpus) = std::env::var_os("C2S_CORPUS") else {
            return;
        };
        fn walk(folder: &std::path::Path, found: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(folder).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, found);
                } else if path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("map")) {
                    found.push(path);
                }
            }
        }
        let mut paths = Vec::new();
        walk(&std::path::Path::new(&corpus).join("map"), &mut paths);
        for path in &paths {
            let bytes = std::fs::read(path).unwrap();
            let map = mapfile::parse(&bytes).unwrap();
            let mut scene = build_scene(&map, "test", &kind_of).unwrap();
            blenderish(&mut scene, 0);
            let edit = read_scene(&scene, &map).unwrap();
            let rebuilt = map_edit::rebuild(&map, &edit).unwrap();
            assert_eq!(mapfile::write(&rebuilt).unwrap(), bytes, "{}", path.display());
            old_anime_names(&mut scene);
            let edit = read_scene(&scene, &map).unwrap();
            let rebuilt = map_edit::rebuild(&map, &edit).unwrap();
            assert_eq!(mapfile::write(&rebuilt).unwrap(), bytes, "{} (old anime names)", path.display());
        }
        println!("{} maps go to scenes and back byte for byte", paths.len());
    }

    /// Anime point data as scenes exported before its fields were decoded name it.
    fn old_anime_names(node: &mut SceneNode) {
        let c2 = &mut node.extras["c2"];
        if c2["kind"] == "anime" {
            let data = c2["data"].as_object_mut().unwrap();
            for (new, old) in
                [("drives", "kind"), ("mode", "unknown_int"), ("start", "v0"), ("step", "v1"), ("end", "v2")]
            {
                let value = data.remove(new).unwrap();
                data.insert(old.into(), value);
            }
            if let (Some(uniform), Some(relative)) = (data.remove("uniform"), data.remove("relative")) {
                data.insert("extra".into(), json!([uniform, relative]));
            }
        }
        node.children.iter_mut().for_each(old_anime_names);
    }

    #[test]
    fn anime_fields_by_name() {
        let original = PointData::Anime(AnimeData {
            drives: Num::Int(2),
            mode: Num::Int(4),
            start: [Num::Int(0), Num::Int(-50), Num::Int(0)],
            step: [Num::Int(0), Num::Int(2), Num::Int(0)],
            end: [Num::Int(0), Num::Int(50), Num::Int(0)],
            flags: Some([Num::Int(0), Num::Int(1)]),
            ..AnimeData::default()
        });
        let data = data_json(&original);
        assert_eq!((data["drives"].clone(), data["mode"].clone()), (json!(2), json!(4)));
        assert_eq!((data["start"].clone(), data["step"].clone()), (json!([0, -50, 0]), json!([0, 2, 0])));
        assert_eq!((data["uniform"].clone(), data["relative"].clone()), (json!(0), json!(1)));
        // an edit sets one flag: the other keeps its value
        let edited = json!({"drives": 3, "mode": 7, "uniform": 1});
        let PointData::Anime(back) = data_from(PointKind::Anime, &edited, Some(&original)).unwrap() else { panic!() };
        assert_eq!((back.drives, back.mode, back.flags), (Num::Int(3), Num::Int(7), Some([Num::Int(1), Num::Int(1)])));
        assert_eq!(back.end, [Num::Int(0), Num::Int(50), Num::Int(0)]);
        // old names, and no flags: the line has 14 arguments
        let old = json!({"kind": 1, "unknown_int": 3, "v0": [1, 2, 3], "v1": [4, 5, 6], "v2": [7, 8, 9]});
        let PointData::Anime(back) = data_from(PointKind::Anime, &old, None).unwrap() else { panic!() };
        assert_eq!((back.drives, back.mode, back.step[2], back.flags), (Num::Int(1), Num::Int(3), Num::Int(6), None));
        let old = json!({"extra": [1, 0]});
        let PointData::Anime(back) = data_from(PointKind::Anime, &old, Some(&original)).unwrap() else { panic!() };
        assert_eq!(back.flags, Some([Num::Int(1), Num::Int(0)]));
    }

    /// What Blender does to a scene: suffixes repeated names, reverses children, writes ints in
    /// mixed lists as floats.
    fn blenderish(node: &mut SceneNode, depth: usize) {
        if depth > 0 {
            node.name = format!("{}.001", node.name);
        }
        node.children.reverse();
        fn floats(value: &mut Value) {
            match value {
                Value::Array(items) if items.iter().any(Value::is_f64) => {
                    for item in items.iter_mut() {
                        if let Some(int) = item.as_i64() {
                            *item = json!(int as f64);
                        }
                    }
                }
                Value::Array(items) => items.iter_mut().for_each(floats),
                Value::Object(map) => map.values_mut().for_each(floats),
                _ => {}
            }
        }
        floats(&mut node.extras);
        for child in &mut node.children {
            blenderish(child, depth + 1);
        }
    }

    #[test]
    fn a_template_library_shows_each_template_once_and_places_none() {
        let mut map = MapFile::default();
        let piece = |model: &str| Piece {
            model: model.into(),
            show: Num::Int(1),
            name: model.trim_end_matches(".mds").into(),
            pos: [Num::Int(0); 3],
            rot: [Num::Int(0); 3],
            scale: [Num::Int(1); 3],
            materials: None,
            col_type: None,
            time: [Num::Int(0); 2],
        };
        for (name, model) in
            [("way00", "w00-m.mds"), ("way01", "w01-a.mds"), ("room00", "r00-m.mds"), ("way00", "x.mds")]
        {
            map.templates.push(crate::mapfile::Template {
                name: name.into(),
                far_clip: None,
                light_flag: [Num::Int(0); 2],
                move_flag: Some([Num::Int(0); 4]),
                pieces: vec![piece(model)],
                points: None,
            });
        }
        let mut scene = build_scene(&map, "d09f01", &kind_of).unwrap();
        let previews = &scene.children[0].children;
        let shown: Vec<(String, [f64; 3])> =
            previews.iter().map(|node| (node.exact_name(), node.trs.as_ref().unwrap().translation)).collect();
        assert_eq!(
            shown,
            [
                ("way00".into(), [0.0, 0.0, 0.0]),
                ("way01".into(), [480.0, 0.0, 0.0]),
                ("room00".into(), [0.0, 0.0, 480.0])
            ]
        );
        blenderish(&mut scene, 0);
        let edit = read_scene(&scene, &map).unwrap();
        assert!(edit.placements.iter().all(|placement| placement.preview && placement.slot.is_none()));
        assert_eq!(map_edit::rebuild(&map, &edit).unwrap(), map);
        // a piece moved in a preview moves in its template; nothing is placed
        let mut edit = edit;
        let way01 = edit.placements.iter_mut().find(|placement| placement.placement.name == "way01").unwrap();
        way01.pieces[0].pos[1] = Num::Int(-20);
        let out = map_edit::rebuild(&map, &edit).unwrap();
        assert!(out.placements.is_empty());
        assert_eq!(out.templates[1].pieces[0].pos[1], Num::Int(-20));
        assert_eq!((out.templates.len(), &out.templates[3]), (4, &map.templates[3]));
    }

    #[test]
    fn names_come_from_extras_or_lose_their_suffix() {
        let node = SceneNode {
            name: "piece:door-m.004".into(),
            trs: None,
            extras: json!({"c2": {"role": "piece"}}),
            children: vec![],
            model: None,
        };
        assert_eq!(node.exact_name(), "door-m");
        let node = SceneNode {
            name: "piece:x.001".into(),
            trs: None,
            extras: json!({"c2": {"name": "x.y"}}),
            children: vec![],
            model: None,
        };
        assert_eq!(node.exact_name(), "x.y");
    }
}
