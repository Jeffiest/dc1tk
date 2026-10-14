//! A catalog of the game's maps for mod makers: how the game uses each one (a town map it walks
//! into, or a map loaded dungeon-style for a scene or a floor), whether the player plays in it, the
//! programs its scripts define and which numbers are free for a mod, the trigger points in its .map
//! file and the program each starts, and where its scripts place the player. The trigger points
//! also say what starts each program, which the decompiler notes on PROGRAM lines.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use c2_assets::mapfile::{self, MapFile, Num, Placement, PointData};
use indexmap::IndexMap;

use crate::definitions::Definitions;
use crate::index::{self, Folded, Outline};
use crate::messages::NameKey;
use crate::xref::{self, Entry, Users};
use crate::{additions, disc, messages, pathnames, Result};

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
    Ok(map_points(&mapfile::parse(&std::fs::read(path)?)?))
}

/// The programs a trigger point starts, with a note saying what it is. An event point starts its
/// program; doors and treasure boxes also run the program in their last argument when used, the
/// one the edit loop calls (kEventDataCallFlag, editloop.cpp).
pub fn point_programs(point: &Point) -> Vec<(i64, String)> {
    let empty = Vec::new();
    let fields = point.event.as_ref().unwrap_or(&empty);
    let kind = fields.first().map(|field| field.trim_matches('"')).unwrap_or("");
    if let Some(program) = point.program {
        let check = point.check.clone().unwrap_or_else(|| "None".into());
        let how = match check.as_str() {
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
    for relative in crate::sdk::files_under(extracted, "map") {
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

/// The maps' names in the definitions' language, by code (s17), from the extracted disc.
pub fn map_names(extracted: &Path, defs: &Definitions) -> IndexMap<String, String> {
    let Some(code) = disc::language_code(defs) else {
        return IndexMap::new();
    };
    let mut found = messages::disc_names(extracted, code);
    let names = found.swap_remove("map_id").unwrap_or_default();
    names
        .into_iter()
        .filter_map(|(key, name)| match key {
            NameKey::Text(code) => Some((code, name)),
            NameKey::Int(_) => None,
        })
        .collect()
}

/// Path(root).rglob("*.map") on Windows, in its order: the matches in each folder in the order the
/// folder lists them, the folders taken from a stack.
fn map_paths(root: &Path) -> Vec<PathBuf> {
    fn listed(folder: &Path) -> Vec<std::fs::DirEntry> {
        std::fs::read_dir(folder).map(|entries| entries.filter_map(|entry| entry.ok()).collect()).unwrap_or_default()
    }
    fn matching(folder: &Path, found: &mut Vec<PathBuf>) {
        for entry in listed(folder) {
            if entry.file_name().to_string_lossy().to_lowercase().ends_with(".map") {
                found.push(entry.path());
            }
        }
    }
    let mut found = Vec::new();
    if !root.is_dir() {
        return found;
    }
    matching(root, &mut found);
    let mut stack = vec![root.to_path_buf()];
    while let Some(folder) = stack.pop() {
        for entry in listed(&folder) {
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                let path = entry.path();
                matching(&path, &mut found);
                stack.push(path);
            }
        }
    }
    found
}

/// Each .map file under the extracted disc by its name without the extension (the last found of
/// a name wins).
pub fn map_files(extracted: &Path) -> HashMap<String, PathBuf> {
    let mut found = HashMap::new();
    for path in map_paths(extracted) {
        if path.is_file() {
            found.insert(path_stem(&path), path);
        }
    }
    found
}

/// Path.stem.
fn path_stem(path: &Path) -> String {
    let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    match name.rfind('.') {
        Some(at) if at > 0 && at < name.len() - 1 => name[..at].to_string(),
        _ => name,
    }
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

/// Everything the catalog says about each map.
pub fn catalog(tree: &Path, extracted: &Path, defs: &Definitions, consts: &IndexMap<String, i64>) -> Result<Catalog> {
    let names = map_names(extracted, defs);
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

pub fn detail(code: &str, found: &Catalog, tree: &Path, extracted: &Path, programs: &BTreeSet<i64>) -> Result<String> {
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
    let path = map_files(extracted).remove(code);
    let file_name = path.as_ref().and_then(|path| path.file_name()).map(|name| name.to_string_lossy().into_owned());
    lines
        .extend([String::new(), format!("trigger points ({}):", file_name.as_deref().unwrap_or("no .map file found"))]);
    let points = match path.as_deref().map(trigger_points) {
        Some(Ok(points)) => points,
        Some(Err(error)) => {
            lines.push(format!("  (the .map file could not be read: {error})"));
            Vec::new()
        }
        None => Vec::new(),
    };
    for point in &points {
        let place = match &point.position {
            Some(position) if !position.is_empty() => {
                // + 0.0 writes -0 as 0
                let values: Vec<String> = position.iter().map(|value| c2_py::format_g(value + 0.0, 6)).collect();
                format!("at {}", values.join(", "))
            }
            _ => String::new(),
        };
        let started = point_programs(point);
        let what = if let Some(program) = point.program {
            format!("starts program {program} {}", point.check.as_deref().unwrap_or("None"))
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
                lines.push(format!("  {}  (in {})", call.text, call.function.as_deref().unwrap_or("None")));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xref::tests::tree;

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
        let no_disc = root.parent().unwrap().join("no-disc");
        let consts = IndexMap::from([("map_start".to_string(), 100), ("player_dead".to_string(), 1200)]);
        let found = catalog(&root, &no_disc, &Definitions::empty(), &consts).unwrap();
        assert!(found.maps["s98"].kinds == ["dungeon-style"] && found.maps["s98"].playable);
        assert!(found.maps["s97"].kinds == ["town"] && !found.maps["s97"].playable);
        let text = detail("s98", &found, &root, &no_disc, &BTreeSet::from([9002])).unwrap();
        assert!(text.contains("free for a mod: 9001, 9003") && text.contains("starts program 9000"));
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
