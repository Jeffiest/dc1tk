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

use serde_json::Value;

use crate::cfg_text::{Arg, Script};
use crate::data_table::{self, Kind, Place, Record, Table, S32};
use crate::Result;

pub const TOWNS: usize = 5;
pub const LANGUAGES: usize = 7;
/// The language whose names label the parts (English).
pub const LABELS: usize = 1;
/// EDIT_PARTS_MATERIAL_MAX.
pub const MATERIALS_MOST: usize = 4;

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

/// {part ID: {tag: statement indices}} of a parts file's blocks, and each block's name.
/// A block: its name and {tag: statement indices}.
type Block = (Option<Value>, std::collections::BTreeMap<String, Vec<usize>>);
type Blocks = std::collections::BTreeMap<i32, Block>;

fn blocks(script: &Script) -> Blocks {
    let mut out = Blocks::new();
    let mut current: Option<Block> = None;
    for (at, statement) in script.statements.iter().enumerate() {
        match statement.tag.as_str() {
            "EDIT_PARTS" => {
                let name = match statement.values().first() {
                    Some(Arg::Str(text)) => Some(Value::from(c2_disc::text::decode_lossy(text))),
                    _ => None,
                };
                current = Some((name, Default::default()));
            }
            "EDIT_PARTS_END" => {
                if let Some((name, tags)) = current.take() {
                    if let Some(&id) = tags.get("ID").and_then(|places| places.last()) {
                        let id = script.statements[id].values().first().map_or(0, Arg::int);
                        out.insert(id, (name, tags));
                    }
                }
            }
            tag => {
                if let Some((_, tags)) = current.as_mut() {
                    tags.entry(tag.to_string()).or_default().push(at);
                }
            }
        }
    }
    out
}

pub fn read(files: &[Vec<u8>]) -> Result<Table> {
    let scripts = data_table::parse_all(&paths(), files)?;
    let mut parts = Vec::new();
    for number in 0..TOWNS {
        let files: Vec<usize> = (0..LANGUAGES).map(|language| number * LANGUAGES + language).collect();
        let found: Vec<Blocks> = files.iter().map(|&file| blocks(&scripts[file])).collect();
        for (&id, (name, tags)) in &found[LABELS] {
            let mut record = Record::new(&[("town", Value::from(town(number))), ("part", Value::from(id))]);
            if let Some(name) = name {
                record.label("name", name.clone());
            }
            // the last statement of a tag in each language's block of this part
            let at = |tag: &str| -> Vec<(usize, usize)> {
                let mut out: Vec<(usize, usize)> = Vec::new();
                if let Some(places) = tags.get(tag) {
                    out.push((files[LABELS], *places.last().unwrap()));
                }
                for (language, blocks) in found.iter().enumerate().filter(|(language, _)| *language != LABELS) {
                    if let Some(places) = blocks.get(&id).and_then(|(_, tags)| tags.get(tag)) {
                        out.push((files[language], *places.last().unwrap()));
                    }
                }
                out
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
                record.field("culture_points", S32, Place::Args { at: at("CPOINT"), arg: 0, len: 2 });
            }
            let polygons = count("POLYN").min(3);
            if polygons > 0 {
                record.field("polyn", S32, Place::Args { at: at("POLYN"), arg: 0, len: polygons });
            }
            // every language's material statements, in order
            let copies: Vec<Vec<(usize, usize)>> = std::iter::once(LABELS)
                .chain((0..LANGUAGES).filter(|&language| language != LABELS))
                .filter_map(|language| {
                    let tags =
                        if language == LABELS { Some(tags) } else { found[language].get(&id).map(|(_, tags)| tags) }?;
                    Some(tags.get("PARTS_MATERIAL").map_or(Vec::new(), |places| {
                        places.iter().map(|&statement| (files[language], statement)).collect()
                    }))
                })
                .collect();
            record.field(
                "materials",
                S32,
                Place::Lines {
                    copies,
                    entry: vec![("item", S32), ("count", S32)],
                    count: (0, MATERIALS_MOST),
                    per_line: 1,
                },
            );
            parts.push(record);
        }
    }
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
                record.arg("part", S32, &at, 0);
                record.field("position", Kind::Float, Place::Args { at: at.to_vec(), arg: 1, len: 3 });
                record.arg("angle", S32, &at, 4);
                list.push(record);
            }
        }
    }
    let [fixed, starting] = placed;
    let mut table = Table::new("a Georama table", scripts, Vec::new());
    table.section("parts", &["town", "part"], parts);
    table.section("fixed_parts", &["town", "number"], fixed);
    table.section("starting_parts", &["town", "number"], starting);
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Every town's parts in every language and its fixed and starting parts (C2S_CORPUS) read,
    /// write back unchanged, and their JSON changes nothing; an edit reaches every language.
    #[test]
    fn corpus_georama_round_trips() {
        let Some(corpus) = std::env::var_os("C2S_CORPUS") else {
            return;
        };
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
        eprintln!(
            "{} parts, {} fixed and {} starting parts round-trip",
            table.sections[0].records.len(),
            table.sections[1].records.len(),
            table.sections[2].records.len()
        );
    }
}
