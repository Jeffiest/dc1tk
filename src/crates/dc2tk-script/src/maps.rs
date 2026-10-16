//! A catalog of the game's maps for mod makers: how the game uses each one (a town map it walks
//! into, or a map loaded dungeon-style for a scene or a floor), whether the player plays in it, the
//! programs its scripts define and which numbers are free for a mod, the trigger points in its .map
//! file and the program each starts, and where its scripts place the player. The trigger points
//! also say what starts each program, which the decompiler notes on PROGRAM lines.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

use dc2tk_assets::mapfile::{self, MapFile, Num, Placement, PointData};
use dc2tk_disc::{files, DiscSource};
use indexmap::IndexMap;

use crate::definitions::Definitions;
use crate::disc::DiscFiles;
use crate::index::{self, Folded, Outline};
use crate::messages::NameKey;
use crate::xref::{self, Entry, Users};
use crate::{additions, disc, mapedit, messages, pathnames, Result};

pub const TOWN_CALLS: [&str; 4] = ["jump_to_map", "go_to_interior", "go_to_outside", "enter_interior"];
pub const MOD_PROGRAMS: std::ops::Range<i128> = 9000..10000;

/// A trigger point of a .map file, where the game has it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Point {
    pub kind: String,
    /// The placement that carries it, which scripts name to reach it; "" for the map's own points
    /// and for the points of templates the file does not place.
    pub part: String,
    /// The template it comes from; "" for the map's own points.
    pub template: String,
    pub name: String,
    pub program: Option<i64>,
    /// How an event point is set off: "on the action button", "on using an item" or "on touch".
    pub check: Option<String>,
    /// In the world: the placement's transform applied to FUNC_POS's position.
    pub position: Option<Vec<f64>>,
    /// FUNC_POS's scale.
    pub size: Option<Vec<f64>>,
    /// FUNC_EVENT_DATA's fields as written: kind, event number, program, three arguments, target,
    /// flags.
    pub event: Option<Vec<String>>,
    pub item: Option<i64>,
}

/// A number as the file writes it, read as a decimal (an exponent, which the engine reads as 0, as
/// 0).
fn decimal(value: Num) -> f64 {
    match value {
        Num::Exponent { .. } => 0.0,
        _ => value.text().parse().unwrap_or(0.0),
    }
}

/// A placement's transform applied to a point: scale, then rotations about X, Y and Z (radians,
/// T·Rz·Ry·Rx·S as mgRotMatrixXYZ builds it), then translation.
fn placed(placement: &Placement, local: [f64; 3]) -> [f64; 3] {
    let [sx, sy, sz] = placement.scale.map(decimal);
    let [rx, ry, rz] = placement.rot.map(decimal);
    let [x, y, z] = [local[0] * sx, local[1] * sy, local[2] * sz];
    let [x, y, z] = [x, y * rx.cos() - z * rx.sin(), y * rx.sin() + z * rx.cos()];
    let [x, y, z] = [x * ry.cos() + z * ry.sin(), y, -x * ry.sin() + z * ry.cos()];
    let [x, y, z] = [x * rz.cos() - y * rz.sin(), x * rz.sin() + y * rz.cos(), z];
    let [tx, ty, tz] = placement.pos.map(decimal);
    [x + tx, y + ty, z + tz]
}

/// One point of the file as the game has it: in a placement (part and transform), in a template
/// nothing places, or the map's own.
fn described(point: &mapfile::Point, template: &str, placement: Option<&Placement>) -> Point {
    let local = point.pos.map(decimal);
    let position = placement.map_or(local, |placement| placed(placement, local));
    let mut found = Point {
        kind: point.kind.name().to_string(),
        part: placement.map(|placement| placement.name.clone()).unwrap_or_default(),
        template: template.to_string(),
        name: point.name.clone(),
        position: Some(position.to_vec()),
        size: Some(point.scale.map(decimal).to_vec()),
        ..Point::default()
    };
    match &point.data {
        PointData::Event(event) => {
            // the action button and using an item set it off (mapFUNC_EVENT_DATA in mapload.cpp); with
            // neither, touching it
            if event.kind == "event" {
                found.program = Some(i64::from(event.program.int()));
                found.check = Some(
                    if event.action_button.int() != 0 {
                        "on the action button"
                    } else if event.use_item.int() != 0 {
                        "on using an item"
                    } else {
                        "on touch"
                    }
                    .to_string(),
                );
            }
            let quoted = |text: &str| format!("\"{text}\"");
            let mut fields = vec![quoted(&event.kind), event.event_no.text(), event.program.text()];
            fields.extend(event.args.map(Num::text));
            fields.push(quoted(&event.target));
            fields.extend([event.action_button, event.use_item, event.flag_100].map(Num::text));
            found.event = Some(fields);
        }
        PointData::Invent(invent) => found.item = Some(i64::from(invent.id.int())),
        _ => {}
    }
    found
}

/// The trigger points of a map, as the game has them: each template's points once for every
/// placement of it (the engine places the first template of a name), the points of templates the
/// file does not place (placed at run time, as dungeon floors are), then the map's own points.
pub fn map_points(map: &MapFile) -> Vec<Point> {
    let mut first: HashMap<&str, &mapfile::Template> = HashMap::new();
    for template in &map.templates {
        first.entry(template.name.as_str()).or_insert(template);
    }
    let mut found = Vec::new();
    let mut placed_names = HashSet::new();
    for placement in &map.placements {
        if let Some(template) = first.get(placement.template.as_str()) {
            placed_names.insert(template.name.as_str());
            for point in template.points.iter().flatten() {
                found.push(described(point, &template.name, Some(placement)));
            }
        }
    }
    for template in &map.templates {
        let unplaced = !placed_names.contains(template.name.as_str());
        if unplaced && std::ptr::eq(first[template.name.as_str()], template) {
            for point in template.points.iter().flatten() {
                found.push(described(point, &template.name, None));
            }
        }
    }
    for point in map.points.iter().flatten() {
        found.push(described(point, "", None));
    }
    found
}

/// The trigger points of a map file (`map_points`).
pub fn trigger_points(path: &Path) -> Result<Vec<Point>> {
    Ok(map_points(&mapfile::parse(&files::read(path)?)?))
}

/// The programs a trigger point starts, with a note saying what it is. An event point starts its
/// program; doors and treasure boxes also run the program in their last argument when used, the
/// one the edit loop calls (kEventDataCallFlag, editloop.cpp).
pub fn point_programs(point: &Point) -> Vec<(i64, String)> {
    let empty = Vec::new();
    let fields = point.event.as_ref().unwrap_or(&empty);
    let kind = fields.first().map_or("", |field| field.trim_matches('"'));
    if let Some(program) = point.program {
        let how = match point.check.as_deref().unwrap_or_default() {
            "on the action button" => "action button",
            "on using an item" => "using an item",
            "on touch" => "touch",
            other => other,
        };
        let name = if point.name.is_empty() { String::new() } else { format!("\"{}\" ", point.name) };
        return vec![(program, format!("trigger point {name}({how})"))];
    }
    if fields.len() < 6 || !matches!(kind, "door" | "ed_door" | "close_door" | "t_box") {
        return Vec::new();
    }
    let target = fields.get(6).map(|field| field.trim_matches('"').to_string()).unwrap_or_default();
    let label = if kind == "t_box" {
        "treasure box".to_string()
    } else if !target.is_empty() {
        format!("door to {target}")
    } else {
        "door".to_string()
    };
    let mut found = Vec::new();
    for field in [&fields[2], &fields[5]] {
        let unsigned = field.trim_start_matches('-');
        let number = if !unsigned.is_empty() && unsigned.chars().all(|c| c.is_ascii_digit()) {
            field.parse().unwrap_or(0)
        } else {
            0
        };
        if number > 0 && !found.contains(&(number, label.clone())) {
            found.push((number, label.clone()));
        }
    }
    found
}

/// What starts each program, from the trigger points of the .map files under an extracted disc:
/// {disc folder: {program: ["\"exit\" point (action button)", ...]}}. A map's script sits in the
/// folder of its .map file.
pub fn folder_triggers(extracted: &Path) -> Result<BTreeMap<String, IndexMap<i64, Vec<String>>>> {
    let mut found: BTreeMap<String, IndexMap<i64, Vec<String>>> = BTreeMap::new();
    for relative in crate::sdk::files_under(extracted, "map")? {
        let folder = match relative.rfind('/') {
            Some(at) => relative[..at].to_string(),
            None => ".".to_string(),
        };
        // a map file that does not read gives no notes rather than stopping the decompile
        let Ok(points) = trigger_points(&extracted.join(&relative)) else {
            continue;
        };
        for point in points {
            for (program, note) in point_programs(&point) {
                let notes = found.entry(folder.clone()).or_default().entry(program).or_default();
                if !notes.contains(&note) {
                    notes.push(note);
                }
            }
        }
    }
    Ok(found)
}

/// The maps' names in the definitions' language, by code (s17), from the disc.
pub fn map_names(disc: &(impl DiscFiles + ?Sized), defs: &Definitions) -> IndexMap<String, String> {
    let Some(code) = disc::language_code(defs) else {
        return IndexMap::new();
    };
    let mut found = messages::disc_names(disc, code);
    let names = found.swap_remove("map_id").unwrap_or_default();
    names
        .into_iter()
        .filter_map(|(key, name)| match key {
            NameKey::Text(code) => Some((code, name)),
            NameKey::Int(_) => None,
        })
        .collect()
}

/// The trigger points of a map's .map file, which the engine finds by the map's code
/// (`mapedit::map_files`), after a heading that names the file.
fn disc_points(disc: Option<&DiscSource>, code: &str, lines: &mut Vec<String>) -> Vec<Point> {
    let found = disc.map(|disc| (disc, mapedit::map_files(disc.paths(), code)));
    let path = match &found {
        Some((_, Ok(files))) => Some(files.map.as_str()),
        _ => None,
    };
    let file_name = path.map(|path| path.rsplit('/').next().unwrap_or(path));
    lines.extend([String::new(), format!("trigger points ({}):", file_name.unwrap_or("no .map file found"))]);
    let (disc, files) = match found {
        None => return Vec::new(),
        Some((_, Err(error))) => {
            lines.push(format!("  ({error})"));
            return Vec::new();
        }
        Some((disc, Ok(files))) => (disc, files),
    };
    let read = || -> Result<Vec<Point>> { Ok(map_points(&mapfile::parse(&disc.read(&files.map)?)?)) };
    read().unwrap_or_else(|error| {
        lines.push(format!("  (the .map file could not be read: {error})"));
        Vec::new()
    })
}

/// The programs the scripts define: {number: the names it is written as}.
pub fn program_numbers(
    outline: &IndexMap<String, Outline>,
    consts: &IndexMap<String, i64>,
    paths: &[String],
) -> BTreeMap<i128, BTreeSet<String>> {
    let mut found: BTreeMap<i128, BTreeSet<String>> = BTreeMap::new();
    for path in paths {
        for (written, _) in &outline[path].programs {
            if let Some(Folded::Int(number)) = index::value(written, consts) {
                found.entry(number).or_default().insert(written.clone());
            }
        }
    }
    found
}

/// Everything the catalog says about one map.
pub struct MapInfo {
    pub code: String,
    pub name: String,
    pub scripts: Vec<String>,
    pub programs: BTreeMap<i128, BTreeSet<String>>,
    pub entries: Vec<Entry>,
    pub kinds: Vec<&'static str>,
    pub playable: bool,
}

/// The catalog: every map by code, and the scripts that include each library.
pub struct Catalog {
    pub maps: BTreeMap<String, MapInfo>,
    pub users: Users,
}

/// Everything the catalog says about each map; the disc, when there is one, names them.
pub fn catalog(
    tree: &Path,
    disc: Option<&DiscSource>,
    defs: &Definitions,
    consts: &IndexMap<String, i64>,
) -> Result<Catalog> {
    let names = disc.map(|disc| map_names(disc, defs)).unwrap_or_default();
    let outline = index::outlines(tree)?;
    let mut plain = Vec::new();
    for path in outline.keys().filter(|path| !index::is_library(path)) {
        plain.push((path.clone(), pathnames::unnamed(tree, path)?));
    }
    let users = index::includers(&outline);
    let mut entries: IndexMap<String, Vec<Entry>> = IndexMap::new();
    for call in xref::map_entries(tree, consts)? {
        if let Some(Folded::Text(map)) = &call.map {
            entries.entry(map.clone()).or_default().push(call);
        }
    }
    let codes: BTreeSet<String> = names.keys().chain(entries.keys()).cloned().collect();
    let player_dead = consts.get("player_dead").map(|value| *value as i128);
    let mut maps = BTreeMap::new();
    for code in codes {
        let mut scripts: Vec<String> = plain
            .iter()
            .filter(|(_, unnamed)| additions::in_map(unnamed, &code))
            .map(|(path, _)| path.clone())
            .collect();
        scripts.sort();
        let programs = program_numbers(&outline, consts, &scripts);
        let calls = entries.swap_remove(&code).unwrap_or_default();
        let mut kinds = Vec::new();
        if calls.iter().any(|entry| TOWN_CALLS.contains(&entry.call.name.as_str())) {
            kinds.push("town");
        }
        if calls.iter().any(|entry| entry.call.name == "load_dungeon_map") {
            kinds.push("dungeon-style");
        }
        let playable = player_dead.is_some_and(|number| programs.contains_key(&number));
        let name = names.get(&code).cloned().unwrap_or_default();
        maps.insert(code.clone(), MapInfo { code, name, scripts, programs, entries: calls, kinds, playable });
    }
    Ok(Catalog { maps, users })
}

fn uses(entry: &MapInfo) -> String {
    if entry.kinds.is_empty() {
        "(entered another way)".into()
    } else {
        entry.kinds.join(", ")
    }
}

pub fn summary(found: &Catalog) -> String {
    let mut lines = vec![format!("{:<9} {:<30} {:<20} {}", "map", "name", "the game uses it as", "player plays there")];
    for (code, entry) in &found.maps {
        if entry.scripts.is_empty() {
            continue;
        }
        let name: String = entry.name.chars().take(30).collect();
        let plays = if entry.playable { "yes" } else { "" };
        lines.push(format!("{code:<9} {name:<30} {:<20} {plays}", uses(entry)));
    }
    lines.extend([
        String::new(),
        "\"player plays there\": its scripts handle the player dying (a player_dead program), so the game lets the \
         player move and fight there."
            .into(),
        "Maps used dungeon-style, and towns, can be a --start-at target.".into(),
    ]);
    lines.join("\n")
}

/// One map in full; the disc, when there is one, gives its .map file's trigger points.
pub fn detail(
    code: &str,
    found: &Catalog,
    tree: &Path,
    disc: Option<&DiscSource>,
    programs: &BTreeSet<i64>,
) -> Result<String> {
    let Some(entry) = found.maps.get(code) else {
        return Ok(format!("no map {code}"));
    };
    let named = if entry.name.is_empty() { String::new() } else { format!(" ({})", entry.name) };
    let mut lines = vec![format!("map {code}{named}")];
    lines.push(format!("  the game uses it as: {}", uses(entry)));
    lines.push(format!(
        "  player plays there: {}",
        if entry.playable {
            "yes, its scripts handle the player dying"
        } else {
            "not as far as its scripts show (no player_dead program)"
        }
    ));
    lines.extend([String::new(), "scripts:".into()]);
    lines.extend(entry.scripts.iter().map(|path| format!("  {path}")));
    lines.extend([String::new(), "programs (number, name):".into()]);
    for (number, written) in &entry.programs {
        lines.push(format!("  {number:>6}  {}", written.iter().cloned().collect::<Vec<_>>().join(", ")));
    }
    let free: Vec<String> = MOD_PROGRAMS
        .filter(|number| {
            !entry.programs.contains_key(number)
                && !i64::try_from(*number).is_ok_and(|number| programs.contains(&number))
        })
        .take(5)
        .map(|number| number.to_string())
        .collect();
    lines.push(format!("  free for a mod: {} ...", free.join(", ")));
    let points = disc_points(disc, code, &mut lines);
    for point in &points {
        let place = match &point.position {
            Some(position) if !position.is_empty() => {
                // to the hundredth of a unit, where + 0.0 writes -0 as 0
                let values: Vec<String> =
                    position.iter().map(|value| ((value * 100.0).round() / 100.0 + 0.0).to_string()).collect();
                format!("at {}", values.join(", "))
            }
            _ => String::new(),
        };
        let started = point_programs(point);
        let what = if let Some(program) = point.program {
            format!("starts program {program} {}", point.check.as_deref().unwrap_or_default())
        } else if !started.is_empty() {
            let numbers: Vec<String> = started.iter().map(|(number, _)| number.to_string()).collect();
            format!("{}, runs program {}", started[0].1, numbers.join(" and "))
        } else if let Some(item) = point.item {
            format!("item use point for item {item}")
        } else {
            point.kind.clone()
        };
        let name = if point.name.is_empty() { "(unnamed)" } else { &point.name };
        let part = if !point.part.is_empty() {
            format!("part {}", point.part)
        } else if !point.template.is_empty() {
            format!("template {} (not placed here)", point.template)
        } else {
            "map's own".to_string()
        };
        lines.push(format!("  {name:<18} {part:<20} {what} {place}"));
    }
    lines.push("  (switch one off with set_func_point_enabled(part, name, 0); \"\" for the map's own points)".into());
    lines.extend([
        String::new(),
        "where its scripts place the player (place_chara(0, x, y, z, yaw) and the like):".into(),
    ]);
    let mut seen = HashSet::new();
    for call in index::calls(tree, &BTreeSet::from(["place_chara".to_string()]))? {
        if entry.scripts.contains(&call.path) && call.args.first().is_some_and(|first| first == "0") {
            let key: Vec<String> = call.args.iter().skip(1).take(4).cloned().collect();
            if seen.insert(key) {
                let function = call.function.as_deref().unwrap_or("no function: before the first");
                lines.push(format!("  {}  (in {function})", call.text));
            }
        }
    }
    lines.extend([String::new(), "entered by:".into()]);
    let entered = xref::grouped(&entry.entries, &found.users, Some(&xref::starts_program))?;
    if entered.is_empty() {
        lines.push("  none found".into());
    } else {
        lines.extend(entered);
    }
    Ok(lines.join("\n"))
}

/// What one call names: (placement, piece or point, the argument giving the placement, the
/// argument giving the name).
type Naming = (&'static str, Option<usize>, Option<usize>);

/// What a script call names of a map: (the native, what it names). A placement is given by its
/// name or its slot (MAP_PARTS order); "" for a point is the map's own points; a piece or a point
/// without a part (get_parts_point_pos) is looked for in every placement.
const NAMING: [(&str, &[Naming]); 13] = [
    ("set_func_point_enabled", &[("point", Some(0), Some(1))]),
    ("bind_handle_map_part", &[("placement", Some(1), None), ("piece", Some(1), Some(2))]),
    ("bind_handle_func_point", &[("point", Some(1), Some(2))]),
    ("set_map_part_motion", &[("placement", Some(0), None), ("piece", Some(0), Some(1))]),
    ("set_func_point_pos", &[("point", Some(0), Some(1))]),
    ("get_func_point_pos", &[("point", Some(0), Some(1))]),
    ("get_func_point_rot", &[("point", Some(0), Some(1))]),
    ("show_map_part", &[("placement", Some(0), None), ("piece", Some(0), Some(1))]),
    ("link_map_part", &[("placement", Some(0), None)]),
    ("ride_map_piece", &[("placement", Some(0), None), ("piece", Some(0), Some(1))]),
    ("get_parts_pos", &[("placement", Some(0), None)]),
    ("compare_part_name", &[("placement", Some(1), None)]),
    ("get_parts_point_pos", &[("point", None, Some(4))]),
];

/// A placement, piece or point of the map that one of its scripts names, which must keep its
/// name (or slot) for the call to find it.
#[derive(Clone, Debug, PartialEq)]
pub struct NameUse {
    pub what: &'static str,
    /// The placement: its name, its slot, or None when the call looks in every placement (or the
    /// argument is not a constant).
    pub part: Option<Folded>,
    pub name: Option<String>,
    pub call: index::Call,
}

/// Whether a call sits in one of `scripts` or in a library one of them includes.
fn in_scripts(call: &index::Call, scripts: &[String], users: &Users) -> bool {
    scripts.contains(&call.path)
        || users.get(&call.path).is_some_and(|including| including.iter().any(|path| scripts.contains(path)))
}

/// Every name the map's scripts (and the libraries they include) give a placement, piece or
/// point of a map.
pub fn name_uses(tree: &Path, entry: &MapInfo, users: &Users, consts: &IndexMap<String, i64>) -> Result<Vec<NameUse>> {
    let natives: BTreeSet<String> = NAMING.iter().map(|(name, _)| name.to_string()).collect();
    let mut found = Vec::new();
    for call in index::calls(tree, &natives)? {
        if !in_scripts(&call, &entry.scripts, users) {
            continue;
        }
        let Some((_, named)) = NAMING.iter().find(|(name, _)| *name == call.name) else { continue };
        for (what, part_at, name_at) in named.iter() {
            let part = part_at.and_then(|at| call.args.get(at)).and_then(|arg| index::value(arg, consts));
            let name = match name_at {
                Some(at) => match call.args.get(*at).and_then(|arg| index::value(arg, consts)) {
                    Some(Folded::Text(text)) => Some(text),
                    // an optional piece left out, or a name made at run time
                    _ => continue,
                },
                None => None,
            };
            // a piece of "" is the whole placement, which the placement entry covers
            if *what == "piece" && name.as_deref() == Some("") {
                continue;
            }
            found.push(NameUse { what, part, name, call: call.clone() });
        }
    }
    Ok(found)
}

/// A position a script stands the player at (place_chara(0, x, y, z, yaw)), when its numbers are
/// written out: (x, y, z, yaw, the call).
pub fn player_positions(tree: &Path, entry: &MapInfo, users: &Users) -> Result<Vec<([f64; 4], index::Call)>> {
    let number = |text: &str| text.trim().parse::<f64>().ok();
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    for call in index::calls(tree, &BTreeSet::from(["place_chara".to_string()]))? {
        if !in_scripts(&call, &entry.scripts, users) || call.args.len() < 5 {
            continue;
        }
        if !matches!(call.args[0].trim(), "0" | "CHARA_PLAYER") {
            continue;
        }
        let values: Option<Vec<f64>> = call.args[1..5].iter().map(|arg| number(arg)).collect();
        let Some(values) = values else { continue };
        let key: Vec<u64> = values.iter().map(|value| value.to_bits()).collect();
        if seen.insert(key) {
            found.push(([values[0], values[1], values[2], values[3]], call));
        }
    }
    Ok(found)
}

fn folded_json(value: &Option<Folded>) -> serde_json::Value {
    match value {
        Some(Folded::Text(text)) => serde_json::Value::from(text.as_str()),
        Some(Folded::Int(number)) => i64::try_from(*number).map_or(serde_json::Value::Null, serde_json::Value::from),
        None => serde_json::Value::Null,
    }
}

/// One map as JSON, for tools (the Blender add-on): its programs and the numbers free for a
/// mod, where its scripts place the player, and every placement, piece and point they name.
pub fn detail_json(
    code: &str,
    found: &Catalog,
    tree: &Path,
    programs: &BTreeSet<i64>,
    consts: &IndexMap<String, i64>,
) -> Result<serde_json::Value> {
    use serde_json::json;

    let Some(entry) = found.maps.get(code) else {
        return Ok(json!({"map": code, "found": false}));
    };
    let listed: Vec<serde_json::Value> = entry
        .programs
        .iter()
        .filter_map(|(number, written)| {
            let number = i64::try_from(*number).ok()?;
            Some(json!({"number": number, "names": written.iter().collect::<Vec<_>>()}))
        })
        .collect();
    let free: Vec<i128> = MOD_PROGRAMS
        .filter(|number| {
            !entry.programs.contains_key(number)
                && !i64::try_from(*number).is_ok_and(|number| programs.contains(&number))
        })
        .take(20)
        .collect();
    let at = |call: &index::Call| json!({"path": call.path, "line": call.line, "function": call.function, "call": call.text});
    let positions: Vec<serde_json::Value> = player_positions(tree, entry, &found.users)?
        .into_iter()
        .map(|([x, y, z, yaw], call)| {
            let mut spot = at(&call);
            spot["x"] = x.into();
            spot["y"] = y.into();
            spot["z"] = z.into();
            spot["yaw"] = yaw.into();
            spot
        })
        .collect();
    let names: Vec<serde_json::Value> = name_uses(tree, entry, &found.users, consts)?
        .into_iter()
        .map(|used| {
            let mut out = at(&used.call);
            out["what"] = used.what.into();
            out["part"] = folded_json(&used.part);
            out["name"] = used.name.map_or(serde_json::Value::Null, serde_json::Value::from);
            out
        })
        .collect();
    Ok(json!({
        "map": code,
        "found": true,
        "name": entry.name,
        "uses": entry.kinds,
        "playable": entry.playable,
        "scripts": entry.scripts,
        "programs": listed,
        "free_programs": free,
        "player_positions": positions,
        "names": names,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xref::tests::tree;

    #[test]
    fn the_json_names_what_scripts_reach_and_where_they_stand_the_player() {
        let script = "PROGRAM(map_start, start);\n#include \"lib/map/s/s96/doors.c\"\n\
                      void start(void)\n{\n    place_chara(0, 68.04, 41.85, -1180.5, 3.14);\n    \
                      place_chara(CHARA_PLAYER, 1.0, 2.0, 3.0, 0.0);\n    place_chara(2, 9.0, 9.0, 9.0, 0.0);\n    \
                      set_func_point_enabled(\"\", \"talk\", 0);\n    bind_handle_map_part(3, \"well01\", \"well-m\", \"rope\");\n    \
                      bind_handle_map_part(4, 12);\n}\n";
        let library = "void open(void)\n{\n    set_func_point_enabled(\"door01\", \"door\", 1);\n    \
                       get_parts_point_pos(0.0, 0.0, 0.0, 50.0, \"kaifuku\", &a, &x, &y, &z);\n}\n";
        let warp = "void warp(void)\n{\n    jump_to_map(0, \"s96\", map_start);\n}\n";
        let root = tree(
            "maps-json",
            &[("map/s/s96/s96_1.c", script), ("lib/map/s/s96/doors.c", library), ("map/d/d01/d01_1.c", warp)],
        );
        let consts = IndexMap::from([("map_start".to_string(), 100), ("CHARA_PLAYER".to_string(), 0)]);
        let found = catalog(&root, None, &Definitions::empty(), &consts).unwrap();
        let report = detail_json("s96", &found, &root, &BTreeSet::from([9000]), &consts).unwrap();
        assert_eq!(report["programs"][0]["number"], 100);
        assert_eq!(report["free_programs"][0], 9001);
        let spots = report["player_positions"].as_array().unwrap();
        assert_eq!(spots.len(), 2, "{spots:?}");
        assert_eq!((spots[0]["x"].as_f64(), spots[0]["z"].as_f64()), (Some(68.04), Some(-1180.5)));
        let mut names: Vec<String> = report["names"]
            .as_array()
            .unwrap()
            .iter()
            .map(|used| format!("{} {} {}", used["what"], used["part"], used["name"]))
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "\"piece\" \"well01\" \"well-m\"",
                "\"placement\" \"well01\" null",
                "\"placement\" 12 null",
                "\"point\" \"\" \"talk\"",
                "\"point\" \"door01\" \"door\"",
                "\"point\" null \"kaifuku\"",
            ]
        );
        assert_eq!(detail_json("x00", &found, &root, &BTreeSet::new(), &consts).unwrap()["found"], false);
    }

    /// A template with a point placed twice (once turned a quarter about Y and moved), a template
    /// nothing places, and the map's own point.
    const MAP_FILE: &str = "PCP \"s99_0.pcp\";
PARTS \"p01_s99\";
\tLIGHT_FLAG 0,0;
\tMOVE_FLAG 1,1,1,1;
\tFUNC_POINT 1;
\t\tFUNC_DATA \"event\",0;
\t\t\tFUNC_NAME \"step\";
\t\t\tFUNC_FLAG 0, 0, 0,0;
\t\t\tFUNC_EVENT_DATA \"event\",1,520,0,0,0,\"\",0,0,0;
\t\t\tFUNC_POS -259.7,0,792.4, 0,0,0, 20,20,20;
\t\tFUNC_DATA_END;
\tFUNC_POINT_END;
PARTS_END;
PARTS \"door99\";
\tLIGHT_FLAG 0,0;
\tMOVE_FLAG 0,0,0,0;
\tFUNC_POINT 1;
\t\tFUNC_DATA \"event\",1;
\t\t\tFUNC_NAME \"door\";
\t\t\tFUNC_FLAG 0, 0, 0,0;
\t\t\tFUNC_EVENT_DATA \"door\",0,200,0,3,210,\"i01h01\",1,0,1;
\t\t\tFUNC_POS 0,0,0, 0,0,0, 15,15,15;
\t\tFUNC_DATA_END;
\tFUNC_POINT_END;
PARTS_END;


ACTIVE_LIGHT_SET 0;
FLOOR -10000;
CHARA_POS 0,0,0;
DEF_FOOT 0;
TIME_FLAG 0,0;
TIME_LIGHT_NUM 4;
SKY_INFO 0,0,0;

MAP_PARTS \"p01_s99\",1;
\tPARTS_NAME \"p01_s99\";
\tPARTS_GROUP \"\";
\tPARTS_POS 0,0,0;
\tPARTS_ROT 0,0,0;
\tPARTS_SCALE 1,1,1;
MAP_PARTS_END;
MAP_PARTS \"p01_s99-1\",1;
\tPARTS_NAME \"p01_s99\";
\tPARTS_GROUP \"\";
\tPARTS_POS 100,0,0;
\tPARTS_ROT 0,1.5708,0;
\tPARTS_SCALE 1,1,1;
MAP_PARTS_END;




FUNC_POINT 1;
\tFUNC_DATA \"event\",1;
\t\tFUNC_NAME \"talk\";
\t\tFUNC_FLAG 0, 0, 0,0;
\t\tFUNC_EVENT_DATA \"event\",0,500,0,0,0,\"\",1,0,0;
\t\tFUNC_POS 68.04,41.85,1180.61, 0,0,0, 30,30,30;
\tFUNC_DATA_END;
FUNC_POINT_END;



";

    #[test]
    fn trigger_points_belong_to_their_placements_or_the_map() {
        let folder = tree("maps-points", &[("s99.map", MAP_FILE)]);
        let points = trigger_points(&folder.join("s99.map")).unwrap();
        let summary: Vec<String> = points
            .iter()
            .map(|point| {
                let check = point.check.as_deref().unwrap_or("-");
                format!("{} {:?} {:?} {:?} {check}", point.name, point.part, point.template, point.program)
            })
            .collect();
        assert_eq!(
            summary,
            [
                "step \"p01_s99\" \"p01_s99\" Some(520) on touch",
                "step \"p01_s99-1\" \"p01_s99\" Some(520) on touch",
                "door \"\" \"door99\" None -",
                "talk \"\" \"\" Some(500) on the action button",
            ]
        );
        assert_eq!(points[0].position, Some(vec![-259.7, 0.0, 792.4]));
        // turned a quarter about Y: x' = z, z' = -x; then moved
        let turned = points[1].position.as_ref().unwrap();
        assert!((turned[0] - 892.4).abs() < 0.01 && turned[1] == 0.0 && (turned[2] - 259.7).abs() < 0.01, "{turned:?}");
        assert_eq!(points[1].size, Some(vec![20.0, 20.0, 20.0]));
        assert_eq!(
            point_programs(&points[2]),
            [(200, "door to i01h01".to_string()), (210, "door to i01h01".to_string())]
        );
        assert_eq!(points[3].position, Some(vec![68.04, 41.85, 1180.61]));
    }

    #[test]
    fn the_catalog_says_how_the_game_uses_a_map_and_what_is_free() {
        let warp =
            "void warp(void)\n{\n    load_dungeon_map(\"s98\");\n    load_event_script(\"s98/s98.stb\", 9000);\n    \
                    jump_to_map(0, \"s97\", map_start);\n}\n";
        let root = tree(
            "maps-catalog",
            &[
                (
                    "map/s/s98/s98_1.c",
                    "PROGRAM(map_start, start);\nPROGRAM(player_dead, lose);\nPROGRAM(9000, mine);\n",
                ),
                ("map/s/s97/s97_1.c", "PROGRAM(map_start, start);\n"),
                ("map/d/d01/d01_1.c", warp),
            ],
        );
        let consts = IndexMap::from([("map_start".to_string(), 100), ("player_dead".to_string(), 1200)]);
        let found = catalog(&root, None, &Definitions::empty(), &consts).unwrap();
        assert!(found.maps["s98"].kinds == ["dungeon-style"] && found.maps["s98"].playable);
        assert!(found.maps["s97"].kinds == ["town"] && !found.maps["s97"].playable);
        let text = detail("s98", &found, &root, None, &BTreeSet::from([9002])).unwrap();
        assert!(text.contains("free for a mod: 9001, 9003") && text.contains("starts program 9000"));
        assert!(text.contains("trigger points (no .map file found):"), "{text}");
    }

    #[test]
    fn a_disc_image_gives_a_maps_trigger_points() {
        use dc2tk_disc::samples;
        let warp = "void warp(void)\n{\n    load_dungeon_map(\"s99\");\n}\n";
        let root = tree("maps-image", &[("map/s/s99/s99_1.c", "PROGRAM(520, step);\n"), ("map/d/d01/d01_1.c", warp)]);
        let (index, dat) = samples::archive_files(&[("\\map\\s\\s99\\s99.map", MAP_FILE.as_bytes())]);
        let image = root.parent().unwrap().join("disc.iso");
        std::fs::write(&image, samples::iso_image(&[("DATA.HD4", &index), ("DATA.DAT", &dat)])).unwrap();
        let disc = DiscSource::open(&image).unwrap();
        let found = catalog(&root, Some(&disc), &Definitions::empty(), &IndexMap::new()).unwrap();
        let text = detail("s99", &found, &root, Some(&disc), &BTreeSet::new()).unwrap();
        assert!(text.contains("trigger points (s99.map):"), "{text}");
        assert!(text.contains("starts program 520"), "{text}");
        let text = detail("s99", &found, &root, None, &BTreeSet::new()).unwrap();
        assert!(text.contains("trigger points (no .map file found):"), "{text}");
    }

    #[test]
    fn doors_and_boxes_name_the_programs_they_run() {
        let point = |fields: [&str; 10]| Point {
            event: Some(fields.iter().map(|field| field.to_string()).collect()),
            ..Point::default()
        };
        let door = point(["\"door\"", "0", "200", "0", "3", "210", "\"i01h01\"", "1", "0", "1"]);
        let chest = point(["\"t_box\"", "0", "0", "388", "1", "105", "\"\"", "1", "0", "1"]);
        assert_eq!(point_programs(&door), [(200, "door to i01h01".to_string()), (210, "door to i01h01".to_string())]);
        assert_eq!(point_programs(&chest), [(105, "treasure box".to_string())]);
    }

    #[test]
    fn program_lines_carry_their_trigger_as_a_comment() {
        let mut program = crate::parse::parse(
            "#pragma natives event\n\nPROGRAM(520, leave);\nMAIN(leave);\n\nvoid leave(void)\n{\n}\n",
        )
        .unwrap();
        let note = "trigger point \"exit\" (action button)";
        program["programs"][0] = program["programs"][0].clone().with("note", note);
        let text = crate::source::text(&program).unwrap();
        assert!(text.contains("PROGRAM(520, leave);  // trigger point \"exit\" (action button)"));
    }
}
