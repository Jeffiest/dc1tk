//! Georama requests and townspeople (docs/assets/data.md, "Georama" and "Townspeople"), as
//! [`data_table`] tables: `data:requests` from `geo<language>.cfg` and `data:townspeople` from
//! `npc<language>.cfg`, one file per language with the same records, an edit going into each.
//!
//! LoadEditAnalyzeData (editdata.cpp) reads `GEO_ANALYZE town;` (0 to 4), `CONDITION number,
//! "text"[, geostone];` (0 to 63) and blocks `ANALYZE request, unread, "title"[, geostone];`
//! (0 to 15, from 0 without a gap) .. `END_ANALYZE;` holding `CON_NO condition, ...;` (at most
//! seven), `ON_PARTS "parts";`, `OFF_PARTS "parts";` (map parts shown and hidden, `;` between,
//! at most 16) and `PERCENT points;`, into EditAnalyzeSrc[5]; every string is copied into
//! buff_1271, 0x300 16-byte units. Whether a condition is met is code (AnalyzeSharlot and the
//! rest, editanalyze.cpp, with the parts, counts and distances as literals): a request may name
//! the conditions the code sets, and a new condition number is never met.
//!
//! LoadNPCCfg (npccfg.cpp) reads `NPC_INFO number, "name", "model", kind, abilities, points,
//! cost, cost, cost, cost, debug;` into NpcBaseData[180] (NPC_BASE_DATA 0x36: name char[0x1C],
//! model char[0x10], the message kind, the number of abilities, the most points, each ability's
//! cost and the debug menu's flag); NPC_NUM is set to the count read. The file is read into
//! 0x8000 bytes, aligned to 64 first.

use serde_json::Value;

use crate::cfg_text::{Arg, Script};
use crate::data_table::{self, Adding, Kind, Place, Record, Table, S16, S32};
use crate::Result;

pub const LANGUAGES: usize = 7;
/// The language whose texts label the records (English).
pub const LABELS: usize = 1;
/// AnalyzeSrc's data[16]: the requests of a town.
pub const REQUESTS_MOST: usize = 16;
/// CON_NO takes at most seven conditions (an s8 list ended by -1).
pub const CONDITIONS_MOST: usize = 7;
/// buff_1271: 0x3000 bytes of strings.
pub const STRING_UNITS: usize = 0x300;
/// NpcBaseData[180], filled by an unchecked u8 index.
pub const TOWNSPEOPLE_MOST: usize = 180;
/// NPC_BASE_DATA's name char[0x1C] and model char[0x10], filled by strcpy.
pub const NAME_MOST: usize = 0x1B;
pub const MODEL_MOST: usize = 0x0F;
/// LoadNPCCfg's u_long128 work[2048], aligned to 64 bytes first.
pub const NPC_BUFFER: usize = 0x8000 - 0x30;

pub fn request_paths() -> Vec<String> {
    (0..LANGUAGES).map(|language| format!("geo{language}.cfg")).collect()
}

pub fn townspeople_paths() -> Vec<String> {
    (0..LANGUAGES).map(|language| format!("npc{language}.cfg")).collect()
}

fn text(script: &Script, statement: usize, arg: usize) -> Option<Value> {
    match script.statements[statement].values().get(arg) {
        Some(Arg::Str(text)) => Some(Value::from(dc2tk_disc::text::decode_lossy(text))),
        _ => None,
    }
}

/// A request block: {tag: statement} and its first and last statements.
#[derive(Clone, Debug, Default)]
struct Block {
    tags: std::collections::BTreeMap<String, usize>,
    first: usize,
    last: usize,
}

/// (town, condition) and (town, request) of a requests file, to their statements.
type Found = (std::collections::BTreeMap<(i32, i32), usize>, std::collections::BTreeMap<(i32, i32), Block>);

fn requests_of(script: &Script) -> Found {
    let mut conditions = std::collections::BTreeMap::new();
    let mut requests = std::collections::BTreeMap::new();
    let mut town = None;
    let mut current: Option<(i32, Block)> = None;
    for (at, statement) in script.statements.iter().enumerate() {
        let first = statement.values().first().map(Arg::int);
        match statement.tag.as_str() {
            "GEO_ANALYZE" => town = first,
            "CONDITION" => {
                if let (Some(town), Some(number)) = (town, first) {
                    conditions.insert((town, number), at);
                }
            }
            "ANALYZE" => current = first.map(|number| (number, Block { first: at, last: at, ..Block::default() })),
            "END_ANALYZE" => {
                if let (Some(town), Some((number, mut block))) = (town, current.take()) {
                    block.last = at;
                    requests.insert((town, number), block);
                }
            }
            tag => {
                if let Some((_, block)) = current.as_mut() {
                    block.tags.insert(tag.to_string(), at);
                }
            }
        }
    }
    (conditions, requests)
}

/// The Georama requests (`data:requests`): `conditions` by `town` (0 to 4) and `condition`, the
/// text a label, `geostone` (the floor whose geostone shows it); `requests` by `town` and
/// `request`, the title a label, with `unread`, `geostone`, `conditions`, `on_parts`,
/// `off_parts` and `percent`. New requests copy one of their town's (`like`), with `title`.
pub fn requests(files: &[Vec<u8>]) -> Result<Table> {
    let scripts = data_table::parse_all(&request_paths(), files)?;
    let found: Vec<Found> = scripts.iter().map(requests_of).collect();
    // the statement of a key in every language, the English first
    let order: Vec<usize> =
        std::iter::once(LABELS).chain((0..LANGUAGES).filter(|&language| language != LABELS)).collect();
    let mut conditions = Vec::new();
    for (&(town, number), &at) in &found[LABELS].0 {
        let mut record = Record::new(&[("town", Value::from(town)), ("condition", Value::from(number))]);
        if let Some(label) = text(&scripts[LABELS], at, 1) {
            record.label("text", label);
        }
        let places: Vec<(usize, usize)> = order
            .iter()
            .filter_map(|&file| found[file].0.get(&(town, number)).map(|&at| (file, at)))
            .filter(|&(file, at)| scripts[file].statements[at].args().len() >= 3)
            .collect();
        if places.first().is_some_and(|&(file, _)| file == LABELS) {
            record.arg("geostone", S16, &places, 2);
        }
        conditions.push(record);
    }
    let mut requests = Vec::new();
    for ((town, number), block) in &found[LABELS].1 {
        let mut record = Record::new(&[("town", Value::from(*town)), ("request", Value::from(*number))]);
        if let Some(label) = text(&scripts[LABELS], block.first, 2) {
            record.label("title", label);
        }
        let blocks: Vec<(usize, &Block)> =
            order.iter().filter_map(|&file| found[file].1.get(&(*town, *number)).map(|block| (file, block))).collect();
        let at = |tag: Option<&str>, least: usize| -> Vec<(usize, usize)> {
            let places: Vec<(usize, usize)> = blocks
                .iter()
                .filter_map(|(file, block)| match tag {
                    None => Some((*file, block.first)),
                    Some(tag) => block.tags.get(tag).map(|&at| (*file, at)),
                })
                .filter(|&(file, at)| scripts[file].statements[at].args().len() >= least)
                .collect();
            if places.first().is_some_and(|&(file, _)| file == LABELS) {
                places
            } else {
                Vec::new()
            }
        };
        if !at(None, 2).is_empty() {
            record.arg("unread", S32, &at(None, 2), 1);
        }
        if !at(None, 4).is_empty() {
            record.arg("geostone", S16, &at(None, 4), 3);
        }
        if !at(Some("CON_NO"), 1).is_empty() {
            record.field(
                "conditions",
                Place::List {
                    at: at(Some("CON_NO"), 1),
                    arg: 0,
                    tail: 0,
                    entry: vec![("condition", Kind::Int(0, 63))],
                    count: (1, CONDITIONS_MOST),
                },
            );
        }
        for (name, tag) in [("on_parts", "ON_PARTS"), ("off_parts", "OFF_PARTS")] {
            if !at(Some(tag), 1).is_empty() {
                record.arg(name, Kind::Text(16 * 0x40), &at(Some(tag), 1), 0);
            }
        }
        if !at(Some("PERCENT"), 1).is_empty() {
            record.arg("percent", S16, &at(Some("PERCENT"), 1), 0);
        }
        record.block = blocks.iter().map(|(file, block)| (*file, block.first, block.last)).collect();
        requests.push(record);
    }
    let mut table = Table::new("a request table", scripts, Vec::new());
    table.section("conditions", &["town", "condition"], conditions);
    table.section("requests", &["town", "request"], requests);
    table.adding(Adding {
        most: REQUESTS_MOST,
        why: "AnalyzeSrc's 16 requests a town",
        place: Some("request"),
        key_args: vec![("request", "ANALYZE", 0, Kind::Int(0, REQUESTS_MOST as i64 - 1))],
        label_args: vec![("title", "ANALYZE", 2, 0xFF)],
        languages: (0..LANGUAGES).map(|file| (file, file)).collect(),
        ..Adding::default()
    });
    table.string_units = Some((STRING_UNITS, "buff_1271, where LoadEditAnalyzeData copies them"));
    Ok(table)
}

/// The townspeople (`data:townspeople`): by `npc` (the character number), the English name a
/// label, with `model`, `message_kind`, `abilities`, `points`, `ability_costs` [4] and `debug`.
/// New ones copy another (`like`), with `name`.
pub fn townspeople(files: &[Vec<u8>]) -> Result<Table> {
    let scripts = data_table::parse_all(&townspeople_paths(), files)?;
    let keyed: Vec<std::collections::BTreeMap<i32, Vec<usize>>> =
        scripts.iter().map(|script| data_table::by_first(script, "NPC_INFO")).collect();
    let order: Vec<usize> =
        std::iter::once(LABELS).chain((0..LANGUAGES).filter(|&language| language != LABELS)).collect();
    let mut records = Vec::new();
    for (&number, places) in &keyed[LABELS] {
        let first = *places.last().expect("a statement");
        if scripts[LABELS].statements[first].args().len() < 11 {
            continue;
        }
        let at: Vec<(usize, usize)> = order
            .iter()
            .filter_map(|&file| keyed[file].get(&number).and_then(|places| places.last()).map(|&at| (file, at)))
            .filter(|&(file, at)| scripts[file].statements[at].args().len() >= 11)
            .collect();
        let mut record = Record::new(&[("npc", Value::from(number))]);
        if let Some(label) = text(&scripts[LABELS], first, 1) {
            record.label("name", label);
        }
        record.arg("model", Kind::Text(MODEL_MOST), &at, 2);
        record.arg("message_kind", S32, &at, 3).arg("abilities", S32, &at, 4).arg("points", S32, &at, 5);
        record.args("ability_costs", S32, &at, 6, 4);
        record.arg("debug", S32, &at, 10);
        record.block = at.iter().map(|&(file, at)| (file, at, at)).collect();
        records.push(record);
    }
    let mut table = Table::new("a townspeople table", scripts, Vec::new());
    table.section("townspeople", &["npc"], records);
    table.adding(Adding {
        most: TOWNSPEOPLE_MOST,
        why: "NpcBaseData[180]",
        key_args: vec![("npc", "NPC_INFO", 0, Kind::Int(1, i16::MAX as i64))],
        label_args: vec![("name", "NPC_INFO", 1, NAME_MOST)],
        languages: (0..LANGUAGES).map(|file| (file, file)).collect(),
        ..Adding::default()
    });
    table.limits = (0..LANGUAGES).map(|file| (file, NPC_BUFFER, "LoadNPCCfg's work buffer")).collect();
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn corpus(paths: &[String]) -> Option<Vec<Vec<u8>>> {
        let corpus = std::env::var_os("C2S_CORPUS")?;
        Some(paths.iter().map(|path| std::fs::read(std::path::Path::new(&corpus).join(path)).unwrap()).collect())
    }

    /// Every language's requests (C2S_CORPUS) read, write back unchanged, their JSON changes
    /// nothing, the strings fit buff_1271; a request is edited in every language and added to
    /// a town.
    #[test]
    fn corpus_requests_round_trip() {
        let Some(files) = corpus(&request_paths()) else {
            return;
        };
        let table = requests(&files).unwrap();
        assert_eq!(table.write(), files);
        assert_eq!(table.check(), Vec::<String>::new());
        let json = table.to_json();
        let (conditions, requests_found) =
            (json["conditions"].as_array().unwrap().len(), json["requests"].as_array().unwrap().len());
        assert_eq!(requests_found, 49);
        for script in &table.scripts {
            assert!(data_table::string_units(script) < STRING_UNITS);
        }
        let town0 = json["requests"].as_array().unwrap().iter().filter(|request| request["town"] == 0).count();
        let edit = json!({"requests": [{"town": 0, "request": 0, "conditions": [0, 1], "percent": 20},
            {"town": 0, "request": town0, "like": 0, "title": "Pond dug", "conditions": [4]}]});
        let (edits, _) = table.changes(&edit, "m").unwrap();
        let written = table.apply(&edits).unwrap();
        let again = requests(&written).unwrap().to_json();
        assert_eq!(again["requests"][0]["conditions"], json!([0, 1]), "{}", again["requests"][0]);
        let new = again["requests"]
            .as_array()
            .unwrap()
            .iter()
            .find(|request| request["town"] == 0 && request["request"] == town0)
            .unwrap();
        assert_eq!((&new["title"], &new["conditions"], &new["percent"]), (&json!("Pond dug"), &json!([4]), &json!(10)));
        for data in &written {
            assert!(String::from_utf8_lossy(data).contains("\"Pond dug\""));
        }
        // in town 0's block, not after the last town's
        let english = String::from_utf8_lossy(&written[LABELS]).into_owned();
        assert!(english.find("Pond dug").unwrap() < english.find("GEO_ANALYZE  1").unwrap());
        let refused = |edit: Value| table.changes(&edit, "m").unwrap_err().to_string();
        assert!(refused(json!({"requests": [{"town": 0, "request": 0, "conditions": [0, 1, 2, 3, 4, 5, 6, 7]}]}))
            .contains("1 to 7"));
        assert!(refused(json!({"conditions": [{"town": 0, "condition": 99, "like": 0}]})).contains("not added to"));
        let many: Vec<Value> =
            (town0..=REQUESTS_MOST).map(|request| json!({"town": 0, "request": request, "like": 0})).collect();
        assert!(refused(json!({ "requests": many })).contains("16 requests"));
        eprintln!("{conditions} conditions and {requests_found} requests in {LANGUAGES} languages round-trip");
    }

    /// Every language's townspeople (C2S_CORPUS) read, write back unchanged and their JSON
    /// changes nothing; one is added in every language, up to NpcBaseData's 180.
    #[test]
    fn corpus_townspeople_round_trip() {
        let Some(files) = corpus(&townspeople_paths()) else {
            return;
        };
        let table = townspeople(&files).unwrap();
        assert_eq!(table.write(), files);
        assert_eq!(table.check(), Vec::<String>::new());
        let json = table.to_json();
        let people = json["townspeople"].as_array().unwrap();
        assert_eq!(people.len(), 177);
        assert_eq!(people[0]["name"], "Cedric");
        let edit = json!({"townspeople": [{"npc": 1, "points": 99}, {"npc": 2000, "like": 1, "name": "Newcomer", "model": "p01_04a"}]});
        let (edits, _) = table.changes(&edit, "m").unwrap();
        let written = table.apply(&edits).unwrap();
        let again = townspeople(&written).unwrap().to_json();
        let new = again["townspeople"].as_array().unwrap().iter().find(|person| person["npc"] == 2000).unwrap();
        assert_eq!((&new["name"], &new["model"], &new["points"]), (&json!("Newcomer"), &json!("p01_04a"), &json!(64)));
        assert_eq!(again["townspeople"][0]["points"], 99);
        let refused = |edit: Value| table.changes(&edit, "m").unwrap_err().to_string();
        let many: Vec<Value> = (2000..2004).map(|npc| json!({"npc": npc, "like": 1, "name": "x"})).collect();
        assert!(refused(json!({ "townspeople": many })).contains("NpcBaseData"));
        assert!(
            refused(json!({"townspeople": [{"npc": 2000, "like": 1, "name": "x".repeat(28)}]})).contains("27 bytes")
        );
        assert!(refused(json!({"townspeople": [{"npc": 1, "model": "a_model_too_long"}]})).contains("15"));
    }
}
