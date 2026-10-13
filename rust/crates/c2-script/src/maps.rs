//! The game's maps for mod makers. So far: the trigger points of the .map files (name, map part,
//! kind, the program an event point starts and how, position and size), and what starts each
//! program, which the decompiler notes on PROGRAM lines.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;

use indexmap::IndexMap;
use regex::Regex;

use crate::Result;

/// A trigger point of a .map file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Point {
    pub kind: String,
    pub part: String,
    pub name: String,
    pub program: Option<i64>,
    /// How an event point is set off: "on the action button", "on using an item" or "on touch".
    pub check: Option<String>,
    pub position: Option<Vec<f64>>,
    pub size: Option<Vec<f64>>,
    /// FUNC_EVENT_DATA's fields: kind, event number, program, three arguments, target, flags.
    pub event: Option<Vec<String>>,
    pub item: Option<i64>,
}

fn func_field() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| Regex::new(r"^(FUNC_\w+|PARTS_NAME)\s*(.*?);?\s*$").unwrap())
}

/// The numbers in a text, as floats.
pub fn numbers(text: &str) -> Vec<f64> {
    static CELL: OnceLock<Regex> = OnceLock::new();
    let pattern = CELL.get_or_init(|| Regex::new(r"-?\d+(?:\.\d+)?(?:[eE]-?\d+)?").unwrap());
    pattern.find_iter(text).filter_map(|found| found.as_str().parse().ok()).collect()
}

fn unquoted(text: &str) -> String {
    text.trim_matches('"').to_string()
}

/// The trigger points of a map file.
pub fn trigger_points(path: &Path) -> Result<Vec<Point>> {
    let text = c2_disc::text::decode_lossy(&std::fs::read(path)?);
    let (mut found, mut part, mut point): (Vec<Point>, String, Option<Point>) = (Vec::new(), String::new(), None);
    for line in text.split(['\r', '\n']) {
        let line = c2_py::strip(line);
        let Some(capture) = func_field().captures(line) else {
            continue;
        };
        let (key, value) = (&capture[1], capture.get(2).map_or("", |value| value.as_str()));
        if key == "PARTS_NAME" {
            part = unquoted(value);
        } else if key == "FUNC_DATA" {
            point = Some(Point {
                kind: unquoted(value.split(',').next().unwrap_or("")),
                part: part.clone(),
                ..Point::default()
            });
        } else if let Some(current) = point.as_mut() {
            match key {
                "FUNC_NAME" => current.name = unquoted(value),
                "FUNC_EVENT_DATA" => {
                    // kind, event number, program, three arguments, target, then flags for the action
                    // button and for using an item (mapFUNC_EVENT_DATA in mapload.cpp); with neither,
                    // touching it.
                    let fields: Vec<String> = value.split(',').map(|field| c2_py::strip(field).to_string()).collect();
                    if fields[0].trim_matches('"') == "event" && fields.len() > 2 {
                        current.program =
                            c2_py::int_base(&fields[2], 10).ok().and_then(|value| i64::try_from(value).ok());
                        let action = fields.len() > 7 && !matches!(fields[7].as_str(), "0" | "");
                        let item = fields.len() > 8 && !matches!(fields[8].as_str(), "0" | "");
                        current.check = Some(
                            if action {
                                "on the action button"
                            } else if item {
                                "on using an item"
                            } else {
                                "on touch"
                            }
                            .to_string(),
                        );
                    }
                    current.event = Some(fields);
                }
                "FUNC_INVENT_DATA" => current.item = numbers(value).first().map(|value| *value as i64),
                "FUNC_POS" => {
                    let values = numbers(value);
                    current.position = Some(values.iter().take(3).copied().collect());
                    current.size = Some(values.iter().skip(6).take(3).copied().collect());
                }
                "FUNC_DATA_END" => found.push(point.take().unwrap()),
                _ => {}
            }
        }
    }
    Ok(found)
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
        for point in trigger_points(&extracted.join(&relative))? {
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
