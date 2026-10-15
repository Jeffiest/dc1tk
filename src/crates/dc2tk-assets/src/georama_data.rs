//! Georama (docs/assets/data.md, "Georama"): the parts each town's editor can place, from
//! `map/g/gNN/gNN<language>.gpi`, and the parts a town starts with, from `map/g/gNN/gNN.cfg`, as
//! one [`data_table`] table (`data:georama`).
//!
//! CEditInfoMngr::LoadEditInfo (editinfo.cpp) reads blocks `EDIT_PARTS "name";` .. `EDIT_PARTS_END;`
//! into CEditPartsInfo: `ID`, `PARTS_NAME` (the map part), `PARTS_ATR` (attribute bits),
//! `PARTS_MATERIAL item, count` (one a statement, at most four), `PARTS_COMMENT`, `CPOINT` (two
//! culture point values), `WEIGHT`, `GEO_STONE`, `MAX_NUM`, `PAINT_NUM`, `PAINT_USED`, `PARTS_TYPE`,
//! `PLACE_EPS` (a float), `POLYN` (up to three) and `MAP_NO`. Each language has its own file with
//! the same parts; a part is named by its town and `ID`, and an edit goes into every language's
//! file. The name and comment are text, labels here. CEditMap::LoadEditInfo (editmap.cpp) reads
//! `FIX_EPARTS id, x, y, z, angle;` and `INIT_EPARTS ...;` (ePlaceData) from `gNN.cfg`: the parts
//! the map always has and those a town begun from nothing has, by their place in the list.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::cfg_text::{Arg, Script};
use crate::data_table::{self, Adding, Kind, Place, Record, Table, S32};
use crate::{lang, Result};

pub const TOWNS: usize = 5;
pub const LANGUAGES: usize = 7;
/// The language whose names label the parts (English).
pub const LABELS: usize = lang::ENGLISH;
/// EDIT_PARTS_MATERIAL_MAX.
pub const MATERIALS_MOST: usize = 4;
/// GEORAMA_PARTS_LIST_MAX: the editor's lists (placed, stock, make) hold 0x180 parts, and
/// MakeDownLoadAnaunce goes over the first 0x180 definitions.
pub const PARTS_MOST: usize = 0x180;
/// The stock list (CMenuGeorama::UpdateGeoramaPartsList) goes over IDs 0 to 0x7F; the save's
/// stock (CSaveData::build_parts_num[0x100]) is by ID for all towns: a part of the ID of another
/// town's part shares its stock, as the towns' own houses and fences do.
pub const PART_ID_MOST: i64 = 0x7F;
/// GEORAMA_PARTS_LIST_ITEM's name[0x30], filled by strcpy.
pub const NAME_MOST: usize = 0x2F;
/// CEditMap::CreateTable(main_data, 0x100, ...): the parts placed in a town.
pub const PLACED_MOST: usize = 0x100;
/// The most bytes of g01's parts files: MakeDownLoadAnaunce reads each into a 0x7800-byte stack
/// buffer aligned to 64 bytes first, which loses at most 0x30 of it from a 16-byte-aligned stack
/// (the bound docs/assets/data.md gives, 0x77D0). item_data::SCRIPT_BUFFER allows for 0x40, the
/// most an alignment of any start loses; both hold every file of the disc.
pub const DOWNLOAD_BUFFER: usize = 0x7800 - 0x30;

fn town(number: usize) -> String {
    format!("g{:02}", number + 1)
}

/// The parts files, town by town and language by language, then each town's cfg.
pub fn paths() -> Vec<String> {
    let mut out = Vec::new();
    for number in 0..TOWNS {
        let town = town(number);
        out.extend((0..LANGUAGES).map(|language| format!("map/g/{town}/{town}{language}.gpi")));
    }
    out.extend((0..TOWNS).map(|number| format!("map/g/{0}/{0}.cfg", town(number))));
    out
}

/// A block of a parts file, EDIT_PARTS to EDIT_PARTS_END: the part's name, its statements by tag
/// and the block's first and last statements.
struct Block {
    name: Option<Value>,
    tags: BTreeMap<String, Vec<usize>>,
    first: usize,
    last: usize,
}

/// A parts file's blocks by the part's ID.
type Blocks = BTreeMap<i32, Block>;

fn blocks(script: &Script) -> Blocks {
    let mut out = Blocks::new();
    let mut current: Option<Block> = None;
    for (at, statement) in script.statements.iter().enumerate() {
        match statement.tag.as_str() {
            "EDIT_PARTS" => {
                let name = match statement.values().first() {
                    Some(Arg::Str(text)) => Some(Value::from(dc2tk_disc::text::decode_lossy(text))),
                    _ => None,
                };
                current = Some(Block { name, tags: BTreeMap::new(), first: at, last: at });
            }
            "EDIT_PARTS_END" => {
                if let Some(block) = current.take() {
                    if let Some(&id) = block.tags.get("ID").and_then(|places| places.last()) {
                        let id = script.statements[id].values().first().map_or(0, Arg::int);
                        out.insert(id, Block { last: at, ..block });
                    }
                }
            }
            tag => {
                if let Some(block) = current.as_mut() {
                    block.tags.entry(tag.to_string()).or_default().push(at);
                }
            }
        }
    }
    out
}

pub fn read(files: &[Vec<u8>]) -> Result<Table> {
    let scripts = data_table::parse_all(&paths(), files)?;
    let parts: Vec<Record> = (0..TOWNS).flat_map(|number| town_parts(&scripts, number)).collect();
    let [fixed, starting] = placed_parts(&scripts);
    let rivers = river_sections(&scripts);
    let mut table = Table::new("a Georama table", scripts, Vec::new());
    let count_of = |table: &Table, tag: &str, files: std::ops::Range<usize>| -> Vec<(usize, usize, usize)> {
        files.filter_map(|file| data_table::tagged(&table.scripts[file], tag).last().map(|&at| (file, at, 0))).collect()
    };
    let languages = (0..TOWNS * LANGUAGES).map(|file| (file, file % LANGUAGES)).collect();
    table.section("parts", &["town", "part"], parts);
    table.adding(Adding {
        counts: count_of(&table, "EDIT_PARTS_NUM", 0..TOWNS * LANGUAGES),
        most: PARTS_MOST,
        why: "GEORAMA_PARTS_LIST_MAX, the editor's lists of parts",
        key_args: vec![("part", "ID", 0, Kind::Int(0, PART_ID_MOST))],
        label_args: vec![("name", "EDIT_PARTS", 0, NAME_MOST), ("comment", "PARTS_COMMENT", 0, 0xFF)],
        languages,
        ..Adding::default()
    });
    let configs = TOWNS * LANGUAGES..TOWNS * LANGUAGES + TOWNS;
    for (name, tag, records) in
        [("fixed_parts", "FIX_EPARTS_START", fixed), ("starting_parts", "INIT_EPARTS_START", starting)]
    {
        let counts = count_of(&table, tag, configs.clone());
        table.section(name, &["town", "number"], records);
        table.adding(Adding {
            counts,
            most: PLACED_MOST,
            why: "the parts placed in a town (CEditMap::CreateTable)",
            place: Some("number"),
            ..Adding::default()
        });
    }
    for (name, keys, records) in rivers {
        table.section(name, &keys, records);
    }
    table.limits = (0..LANGUAGES).map(|file| (file, DOWNLOAD_BUFFER, "MakeDownLoadAnaunce's stack buffer")).collect();
    Ok(table)
}

/// A town's parts, one record per part of its English file, each field in every language's file
/// that has the part (the English first: it is the one read).
fn town_parts(scripts: &[Script], number: usize) -> Vec<Record> {
    let files: Vec<usize> = (0..LANGUAGES).map(|language| number * LANGUAGES + language).collect();
    let found: Vec<Blocks> = files.iter().map(|&file| blocks(&scripts[file])).collect();
    let mut parts = Vec::new();
    for (&id, block) in &found[LABELS] {
        let copies: Vec<(usize, &Block)> = lang::english_first(LANGUAGES)
            .into_iter()
            .filter_map(|language| found[language].get(&id).map(|block| (files[language], block)))
            .collect();
        let mut record = Record::new(&[("town", Value::from(town(number))), ("part", Value::from(id))]);
        if let Some(name) = &block.name {
            record.label("name", name.clone());
        }
        // the last statement of a tag in each language's block of this part
        let at = |tag: &str| -> Vec<(usize, usize)> {
            copies
                .iter()
                .filter_map(|(file, block)| block.tags.get(tag).and_then(|places| places.last()).map(|&at| (*file, at)))
                .collect()
        };
        let count = |tag: &str| {
            at(tag).first().map_or(0, |&(file, statement)| scripts[file].statements[statement].args().len())
        };
        if count("PARTS_NAME") >= 1 {
            record.arg("model", Kind::Text(63), &at("PARTS_NAME"), 0);
        }
        for (name, tag) in [
            ("attribute", "PARTS_ATR"),
            ("weight", "WEIGHT"),
            ("geostone", "GEO_STONE"),
            ("max", "MAX_NUM"),
            ("paint_colours", "PAINT_NUM"),
            ("paint_used", "PAINT_USED"),
            ("type", "PARTS_TYPE"),
            ("map", "MAP_NO"),
        ] {
            if count(tag) >= 1 {
                record.arg(name, S32, &at(tag), 0);
            }
        }
        if count("PLACE_EPS") >= 1 {
            record.arg("place_eps", Kind::Float, &at("PLACE_EPS"), 0);
        }
        if count("CPOINT") >= 2 {
            record.args("culture_points", S32, &at("CPOINT"), 0, 2);
        }
        let polygons = count("POLYN").min(3);
        if polygons > 0 {
            record.args("polyn", S32, &at("POLYN"), 0, polygons);
        }
        // every language's material statements, in order
        let materials = copies
            .iter()
            .map(|(file, block)| {
                let places = block.tags.get("PARTS_MATERIAL").map(Vec::as_slice).unwrap_or_default();
                places.iter().map(|&statement| (*file, statement)).collect()
            })
            .collect();
        record.field(
            "materials",
            Place::Lines {
                copies: materials,
                entry: vec![("item", S32), ("count", S32)],
                count: (0, MATERIALS_MOST),
                per_line: 1,
                grow: true,
            },
        );
        record.block = copies.iter().map(|(file, block)| (*file, block.first, block.last)).collect();
        parts.push(record);
    }
    parts
}

/// The parts each town's map always has and those a town begun from nothing has, by their place
/// in its cfg's FIX_EPARTS and INIT_EPARTS lists.
fn placed_parts(scripts: &[Script]) -> [Vec<Record>; 2] {
    let mut placed = [Vec::new(), Vec::new()];
    for number in 0..TOWNS {
        let file = TOWNS * LANGUAGES + number;
        for (list, tag) in placed.iter_mut().zip(["FIX_EPARTS", "INIT_EPARTS"]) {
            for (index, statement) in data_table::tagged(&scripts[file], tag).into_iter().enumerate() {
                if scripts[file].statements[statement].args().len() < 5 {
                    continue;
                }
                let at = [(file, statement)];
                let mut record = Record::new(&[("town", Value::from(town(number))), ("number", Value::from(index))]);
                record.block = vec![(file, statement, statement)];
                record.arg("part", S32, &at, 0);
                record.args("position", Kind::Float, &at, 1, 3);
                record.arg("angle", S32, &at, 4);
                list.push(record);
            }
        }
    }
    placed
}

/// The rivers and water of each town's cfg (CEditMap::LoadEditInfo, emap_tag, and CMap::LoadCfgFile):
/// `RIVER_PARTS_NAME slot, "map part", "model.mds";` (slots 0 to 7), `WATER_PARTS_NAME
/// "model.mds";` and `MASK_PARTS_NAME 0, "model.mds";` between `EDIT_RIVER` and `EDIT_RIVER_END`;
/// `WATER_SURFACE_START;` .. `WATER_SURFACE_END;` blocks of `WATER_VERTEX nx, nz, x0, y0, z0, x1,
/// y1, z1;`, `WATER_POS x, y, z;` and `WATER_PARAM a, b, c, d;`; and `WATER_DRAW surface, "part or
/// #followN", x, y, z;`.
fn river_sections(scripts: &[Script]) -> Vec<(&'static str, Vec<&'static str>, Vec<Record>)> {
    let mut rivers = Vec::new();
    let mut water = Vec::new();
    let mut surfaces = Vec::new();
    let mut draws = Vec::new();
    for number in 0..TOWNS {
        let file = TOWNS * LANGUAGES + number;
        let script = &scripts[file];
        let keys = |key: &str, value: Value| Record::new(&[("town", Value::from(town(number))), (key, value)]);
        let args = |at: usize| script.statements[at].args().len();
        for at in data_table::tagged(script, "RIVER_PARTS_NAME") {
            if args(at) < 3 {
                continue;
            }
            let slot = script.statements[at].values()[0].int();
            let mut record = keys("slot", Value::from(slot));
            record.arg("part", Kind::Text(63), &[(file, at)], 1).arg("model", Kind::Text(63), &[(file, at)], 2);
            rivers.push(record);
        }
        let mut record = Record::new(&[("town", Value::from(town(number)))]);
        if let Some(&at) = data_table::tagged(script, "WATER_PARTS_NAME").last().filter(|&&at| args(at) >= 1) {
            record.arg("model", Kind::Text(63), &[(file, at)], 0);
        }
        if let Some(&at) = data_table::tagged(script, "MASK_PARTS_NAME").last().filter(|&&at| args(at) >= 2) {
            record.arg("mask_model", Kind::Text(63), &[(file, at)], 1);
        }
        if !record.fields.is_empty() {
            water.push(record);
        }
        let mut current: Option<Record> = None;
        for (at, statement) in script.statements.iter().enumerate() {
            let place = [(file, at)];
            match statement.tag.as_str() {
                "WATER_SURFACE_START" => {
                    current = Some(keys(
                        "surface",
                        Value::from(
                            surfaces.iter().filter(|record: &&Record| record.keys[0].1 == town(number)).count(),
                        ),
                    ))
                }
                "WATER_VERTEX" if args(at) >= 8 => {
                    if let Some(record) = current.as_mut() {
                        record.args("grid", S32, &place, 0, 2);
                        record.args("corners", Kind::Float, &place, 2, 6);
                    }
                }
                "WATER_POS" if args(at) >= 3 => {
                    if let Some(record) = current.as_mut() {
                        record.args("position", Kind::Float, &place, 0, 3);
                    }
                }
                "WATER_PARAM" if args(at) >= 4 => {
                    if let Some(record) = current.as_mut() {
                        record.args("params", Kind::Float, &place, 0, 4);
                    }
                }
                "WATER_SURFACE_END" => surfaces.extend(current.take()),
                "WATER_DRAW" if args(at) >= 5 => {
                    let index = draws.iter().filter(|record: &&Record| record.keys[0].1 == town(number)).count();
                    let mut record = keys("number", Value::from(index));
                    record.arg("surface", S32, &place, 0).arg("follow", Kind::Text(63), &place, 1);
                    record.args("position", Kind::Float, &place, 2, 3);
                    draws.push(record);
                }
                _ => {}
            }
        }
    }
    vec![
        ("rivers", vec!["town", "slot"], rivers),
        ("water", vec!["town"], water),
        ("water_surfaces", vec!["town", "surface"], surfaces),
        ("water_draws", vec!["town", "number"], draws),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Every town's parts in every language and its fixed and starting parts (DC2TK_CORPUS) read,
    /// write back unchanged, and their JSON changes nothing; an edit reaches every language.
    #[test]
    #[ignore = "needs DC2TK_CORPUS"]
    fn corpus_georama_round_trips() {
        let corpus = dc2tk_disc::samples::corpus();
        let root = std::path::Path::new(&corpus);
        let files: Vec<Vec<u8>> = paths().iter().map(|path| std::fs::read(root.join(path)).unwrap()).collect();
        let table = read(&files).unwrap();
        assert_eq!(table.write(), files);
        assert_eq!(table.check(), Vec::<String>::new());
        let first = table.to_json()["parts"][0].clone();
        let edit = json!({"parts": [{"town": first["town"], "part": first["part"], "max": 99,
            "materials": [{"item": 210, "count": 1}]}]});
        let (edits, _) = table.changes(&edit, "m").unwrap();
        let again = read(&table.apply(&edits).unwrap()).unwrap();
        for language in 0..LANGUAGES {
            let text = String::from_utf8_lossy(&again.write()[language]).into_owned();
            assert!(text.contains("MAX_NUM 99") || text.contains("MAX_NUM  99"), "language {language}");
        }
        assert_eq!(again.to_json()["parts"][0]["materials"], json!([{"item": 210, "count": 1}]));
        // a fifth material is refused, a missing one added as a line
        let five = json!([{"item": 1, "count": 1}, {"item": 2, "count": 1}, {"item": 3, "count": 1}, {"item": 4, "count": 1}, {"item": 5, "count": 1}]);
        let edit = json!({"parts": [{"town": first["town"], "part": first["part"], "materials": five}]});
        assert!(table.changes(&edit, "m").unwrap_err().to_string().contains("5 entries"));
        // a new part: a copy of another in every language, with an ID no town uses
        let json = table.to_json();
        let used: Vec<i64> =
            json["parts"].as_array().unwrap().iter().map(|part| part["part"].as_i64().unwrap()).collect();
        let free = (0..=PART_ID_MOST).find(|id| !used.contains(id)).unwrap();
        let new = json!({"parts": [{"town": first["town"], "part": free, "like": first["part"], "name": {"en": "New Hut", "fr": "Hutte"},
            "comment": "A new hut.", "max": 3}]});
        let (edits, _) = table.changes(&new, "m").unwrap();
        let written = table.apply(&edits).unwrap();
        let again = read(&written).unwrap();
        let part =
            again.to_json()["parts"].as_array().unwrap().iter().find(|part| part["part"] == free).unwrap().clone();
        assert_eq!((&part["name"], &part["max"], &part["model"]), (&json!("New Hut"), &json!(3), &first["model"]));
        for language in 0..LANGUAGES {
            let text = String::from_utf8_lossy(&written[language]).into_owned();
            let count = |data: &[u8]| {
                let script = Script::parse(data).unwrap();
                let at = *data_table::tagged(&script, "EDIT_PARTS_NUM").last().unwrap();
                script.statements[at].values()[0].int()
            };
            assert_eq!(count(&written[language]), count(&files[language]) + 1, "{language}");
            assert!(
                text.contains(if language == 2 { "EDIT_PARTS \"Hutte\";" } else { "EDIT_PARTS \"New Hut\";" }),
                "{language}"
            );
            assert!(text.contains(&format!("ID {free};")) || text.contains(&format!("ID {free} ;")), "{language}");
        }
        let refused = |edit: Value| table.changes(&edit, "m").unwrap_err().to_string();
        let taken = json!({"parts": [{"town": "g02", "part": first["part"], "like": json!({"town": "g02", "part": used[60]})}]});
        assert!(refused(taken).contains("game's own"));
        let high = json!({"parts": [{"town": first["town"], "part": 128, "like": first["part"], "name": "x"}]});
        assert!(refused(high).contains("0 to 127"));
        let long =
            json!({"parts": [{"town": first["town"], "part": free, "like": first["part"], "name": "x".repeat(48)}]});
        assert!(refused(long).contains("47 bytes"));
        // a new starting part, counted in INIT_EPARTS_START
        let starts = json["starting_parts"].as_array().unwrap().iter().filter(|part| part["town"] == "g01").count();
        let start = json!({"starting_parts": [{"town": "g01", "number": starts, "like": {"town": "g01", "number": 0}, "position": [1.5, 0, 2]}]});
        let (edits, _) = table.changes(&start, "m").unwrap();
        let written = table.apply(&edits).unwrap();
        let config = String::from_utf8_lossy(&written[TOWNS * LANGUAGES]).into_owned();
        assert!(config.contains(&format!("INIT_EPARTS_START {};", starts + 1)), "{config}");
        let again = read(&written).unwrap().to_json();
        let added =
            again["starting_parts"].as_array().unwrap().iter().rfind(|part| part["town"] == "g01").unwrap().clone();
        assert_eq!(added["position"], json!([1.5, 0, 2]));
        // rivers and water read and edit
        assert_eq!(json["rivers"].as_array().unwrap().len(), 16);
        let river = json!({"rivers": [{"town": "g01", "slot": 0, "model": "g010501_r-m.mds"}],
            "water_surfaces": [{"town": "g01", "surface": 0, "params": [0.2, 0.0045, 0.0, 16.0]}]});
        let (edits, _) = table.changes(&river, "m").unwrap();
        let config = String::from_utf8_lossy(&table.apply(&edits).unwrap()[TOWNS * LANGUAGES]).into_owned();
        assert!(config.contains("RIVER_PARTS_NAME 0,\"p04_g0105\",\"g010501_r-m.mds\";"));
        assert!(config.contains("WATER_PARAM 0.2,0.0045,0.0,16.0;"), "{config}");
        eprintln!(
            "{} parts, {} fixed and {} starting parts round-trip",
            table.sections[0].records.len(),
            table.sections[1].records.len(),
            table.sections[2].records.len()
        );
    }
}
