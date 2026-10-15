//! Fishing (docs/assets/data.md, "Fishing"): the fishing spots, the fish race opponents, the
//! race and tournament prizes, and the two fish tables of the executable, as one [`data_table`]
//! table (`data:fishing`).
//!
//! LoadFishPlaceData (fishing.cpp) reads `sg/fish/fishing.cfg`: `FISH_MAP map[, exclusive];`,
//! `FISH_PLACE area, "name", 5 floats;`, up to eight `FISH fish, rate, wait_bias;` and
//! `FISH_MAP_END;` (a spot, named here by its place in the file). CGyoraceFishData::LoadData
//! (menuaqua.cpp) reads `sg/gyo/rfd<language>.cfg`: `NUM class, count;` then `DAT item, "name",
//! colour, power, speed × 3, stamina, size;` (_GYORACE_DATA, into a fish of CopyDataFish(item));
//! the languages' files have the same fish, and an edit goes into each. LoadFishPrize reads
//! `gyop.cfg` (the fish race) or `uofp.cfg` (the tournament): `GRP count, values...;` then
//! `count` × `PRIZE value, item, count, item, count, item, count;` (first to third place).
//! RefreshFishPrize takes only the first PRIZE of the group the player's wins pick and reads
//! neither GRP's values nor PRIZE's first; they are shown, as `values` and `unread`.
//!
//! `FishParam` (0x361A50, FISH_PARAM 0x54 × 19, row 0 none) is each species caught: its item,
//! sizes, weight and points per size, pull and liking of each bait and time of day. `fish_data`
//! (0x3630C0, grFISH_DATA 0x1C × 18) is each kind's race figures in percent.

use serde_json::Value;

use crate::cfg_text::Arg;
use crate::data_table::{self, Adding, Kind, Place, Record, Table, B8, S16, S32};
use crate::elf::table_offset;
use crate::Result;

pub const LANGUAGES: usize = 7;
/// kFishPlaceMaxFish. FISH reads its arguments unchecked, so a spot keeps at least the FISH lines
/// it has (an emptied line would add a fish of whatever the stack held); more are added up to this.
pub const SPOT_FISH_MOST: usize = 8;
/// No array bounds the spots (FISH_MAP_NUM allocates them); this keeps a mod within reason.
pub const SPOTS_MOST: usize = 256;
pub const SPECIES_TABLE: u32 = 0x361A50;
pub const SPECIES: usize = 19;
pub const SPECIES_RECORD: usize = 0x54;
pub const RACE_TABLE: u32 = 0x3630C0;
pub const RACE_KINDS: usize = 18;
pub const RACE_RECORD: usize = 0x1C;
pub const CONTESTS: [&str; 2] = ["race", "tournament"];
/// The files, by paths(): the spots, the race fish of each language, then each contest's prizes.
const SPOTS: usize = 0;
const RACERS: usize = 1;
const PRIZES: [usize; 2] = [RACERS + LANGUAGES, RACERS + LANGUAGES + 1];

pub fn paths() -> Vec<String> {
    let mut out = vec!["sg/fish/fishing.cfg".to_string()];
    out.extend((0..LANGUAGES).map(|language| format!("sg/gyo/rfd{language}.cfg")));
    out.extend(["gyop.cfg".to_string(), "uofp.cfg".to_string()]);
    out
}

/// The NUL-ended text a pointer of the executable points at.
fn text_at(elf: &[u8], pointer: u32) -> Option<Value> {
    let start = crate::elf::file_offset(elf, pointer)?;
    let end = start + elf.get(start..)?.iter().position(|&byte| byte == 0)?;
    Some(Value::from(dc2tk_disc::text::decode_lossy(&elf[start..end])))
}

pub fn read(files: &[Vec<u8>], elf: &[u8]) -> Result<Table> {
    let scripts = data_table::parse_all(&paths(), files)?;
    // spots: the statements of each FISH_MAP block
    let mut spots = Vec::new();
    let mut block: Option<(usize, Vec<usize>, Vec<usize>)> = None;
    for (at, statement) in scripts[SPOTS].statements.iter().enumerate() {
        match statement.tag.as_str() {
            "FISH_MAP" => block = Some((at, Vec::new(), Vec::new())),
            "FISH_PLACE" => {
                if let Some((_, places, _)) = block.as_mut() {
                    places.push(at);
                }
            }
            "FISH" => {
                if let Some((_, _, fish)) = block.as_mut() {
                    fish.push(at);
                }
            }
            "FISH_MAP_END" => {
                let Some((head, places, fish)) = block.take() else {
                    continue;
                };
                let mut record = Record::new(&[("spot", Value::from(spots.len()))]);
                let head = [(0, head)];
                record.arg("map", S32, &head, 0);
                if scripts[SPOTS].statements[head[0].1].args().len() >= 2 {
                    record.arg("exclusive", S32, &head, 1);
                }
                if let Some(&place) = places.last().filter(|&&place| scripts[SPOTS].statements[place].args().len() >= 7)
                {
                    let place = [(0, place)];
                    record.arg("area", S32, &place, 0);
                    record.args("area_values", Kind::Float, &place, 2, 5);
                }
                record.field(
                    "fish",
                    Place::Lines {
                        copies: vec![fish.iter().map(|&at| (0, at)).collect()],
                        entry: vec![("fish", S32), ("rate", Kind::Float), ("wait_bias", Kind::Float)],
                        count: (fish.len().min(SPOT_FISH_MOST), SPOT_FISH_MOST),
                        per_line: 1,
                        grow: true,
                    },
                );
                record.block = vec![(0, head[0].1, at)];
                spots.push(record);
            }
            _ => {}
        }
    }
    // race opponents: (class, number) to the statement in each language's file
    let mut racers: std::collections::BTreeMap<(i32, usize), Vec<(usize, usize)>> = Default::default();
    for (file, script) in scripts.iter().enumerate().skip(RACERS).take(LANGUAGES) {
        let mut class = None;
        let mut number = 0;
        for (at, statement) in script.statements.iter().enumerate() {
            match statement.tag.as_str() {
                "NUM" => {
                    class = statement.values().first().map(Arg::int);
                    number = 0;
                }
                "DAT" => {
                    if let Some(class) = class {
                        racers.entry((class, number)).or_default().push((file, at));
                        number += 1;
                    }
                }
                _ => {}
            }
        }
    }
    let mut race_fish = Vec::new();
    for ((class, number), mut at) in racers {
        // the English file first: its name labels the fish
        at.sort_by_key(|&(file, _)| file != RACERS + crate::lang::ENGLISH);
        if scripts[at[0].0].statements[at[0].1].args().len() < 9 {
            continue;
        }
        let mut record = Record::new(&[("class", Value::from(class)), ("number", Value::from(number))]);
        if let Some(Arg::Str(name)) = scripts[at[0].0].statements[at[0].1].values().get(1) {
            record.label("name", Value::from(dc2tk_disc::text::decode_lossy(name)));
        }
        record.arg("item", S32, &at, 0).arg("colour", B8, &at, 2).arg("power", S16, &at, 3);
        record.args("speed", S16, &at, 4, 3);
        record.arg("stamina", S16, &at, 7).arg("size", S16, &at, 8);
        race_fish.push(record);
    }
    let mut groups = Vec::new();
    let mut prizes = Vec::new();
    for (contest, file) in CONTESTS.iter().zip(PRIZES) {
        let mut group = None;
        let mut prize = 0;
        for (at, statement) in scripts[file].statements.iter().enumerate() {
            let place = [(file, at)];
            match statement.tag.as_str() {
                "GRP" => {
                    let number = group.map_or(0, |group: usize| group + 1);
                    group = Some(number);
                    prize = 0;
                    let count = statement.args().len();
                    let mut record = Record::new(&[("contest", Value::from(*contest)), ("group", Value::from(number))]);
                    if count >= 1 {
                        record.arg("count", S32, &place, 0).fixed("the count is how many PRIZE lines follow");
                    }
                    if count >= 2 {
                        record.args("values", S32, &place, 1, (count - 1).min(8));
                    }
                    groups.push(record);
                }
                "PRIZE" if statement.args().len() >= 7 => {
                    let Some(number) = group else {
                        continue;
                    };
                    let mut record = Record::new(&[
                        ("contest", Value::from(*contest)),
                        ("group", Value::from(number)),
                        ("prize", Value::from(prize)),
                    ]);
                    record.arg("unread", S32, &place, 0);
                    record.field(
                        "places",
                        Place::List {
                            at: place.to_vec(),
                            arg: 1,
                            tail: statement.args().len() - 7,
                            entry: vec![("item", S32), ("count", S32)],
                            count: (3, 3),
                        },
                    );
                    prizes.push(record);
                    prize += 1;
                }
                _ => {}
            }
        }
    }
    let mut table = Table::new("a fishing table", scripts, elf.to_vec());
    table.section("spots", &["spot"], spots);
    // FISH_MAP_NUM allocates the spots (FISH_PLACE_MAP, 0x88 each, from the scene's stack); a
    // block past it is dropped
    let counts = data_table::tagged(&table.scripts[SPOTS], "FISH_MAP_NUM").last().map(|&at| vec![(SPOTS, at, 0)]);
    table.adding(Adding {
        counts: counts.unwrap_or_default(),
        most: SPOTS_MOST,
        why: "the spots FISH_MAP_NUM allocates from the scene's stack, a size not fixed",
        place: Some("spot"),
        ..Adding::default()
    });
    table.section("race_fish", &["class", "number"], race_fish);
    table.section("prize_groups", &["contest", "group"], groups);
    table.section("prizes", &["contest", "group", "prize"], prizes);
    let start = table_offset(elf, SPECIES_TABLE, SPECIES * SPECIES_RECORD, "fish species table")?;
    let mut species = Vec::new();
    for row in 1..SPECIES {
        let at = start + row * SPECIES_RECORD;
        let mut record = Record::new(&[("fish", Value::from(row))]);
        let pointer = crate::bytes::u32_at(elf, at)?;
        if let Some(name) = text_at(elf, pointer) {
            record.label("name", name);
        }
        record
            .bytes("item", S32, at + 0x8, 1)
            .bytes("base_size", Kind::Float, at + 0xC, 1)
            .bytes("min_size", Kind::Float, at + 0x10, 1)
            .bytes("max_size", Kind::Float, at + 0x14, 1)
            .bytes("unknown_18", Kind::Float, at + 0x18, 1)
            .bytes("weight_rate", Kind::Float, at + 0x1C, 1)
            .bytes("point_rate", Kind::Float, at + 0x20, 1)
            .bytes("pull_rate", Kind::Float, at + 0x24, 1)
            .bytes("bait_liking", S16, at + 0x28, 18)
            .bytes("time_liking", S16, at + 0x4C, 4);
        species.push(record);
    }
    table.section("species", &["fish"], species);
    let start = table_offset(elf, RACE_TABLE, RACE_KINDS * RACE_RECORD, "fish race table")?;
    let mut kinds = Vec::new();
    for row in 0..RACE_KINDS {
        let at = start + row * RACE_RECORD;
        let mut record = Record::new(&[("row", Value::from(row))]);
        record
            .bytes("item", S32, at, 1)
            .fixed("GetFishData finds a kind by its item")
            .bytes("power", Kind::Float, at + 4, 1)
            .bytes("stamina", Kind::Float, at + 8, 1)
            .bytes("speed", Kind::Float, at + 0xC, 3)
            .bytes("affinity", S32, at + 0x18, 1);
        kinds.push(record);
    }
    table.section("race_kinds", &["row"], kinds);
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The fishing files and tables of an extracted disc (DC2TK_CORPUS) read, write back unchanged,
    /// and their JSON changes nothing; a race fish's edit reaches every language.
    #[test]
    #[ignore = "needs DC2TK_CORPUS"]
    fn corpus_fishing_round_trips() {
        let corpus = dc2tk_disc::samples::corpus();
        let root = std::path::Path::new(&corpus);
        let files: Vec<Vec<u8>> = paths().iter().map(|path| std::fs::read(root.join(path)).unwrap()).collect();
        let elf = std::fs::read(root.join(crate::elf::EXECUTABLE)).unwrap();
        let table = read(&files, &elf).unwrap();
        assert_eq!(table.write(), files);
        assert_eq!(table.check(), Vec::<String>::new());
        let counts: Vec<usize> = table.sections.iter().map(|section| section.records.len()).collect();
        assert_eq!(counts[1], 216);
        let edit = json!({"race_fish": [{"class": 0, "number": 0, "power": 99}],
            "spots": [{"spot": 0, "fish": [{"fish": 3, "rate": 1, "wait_bias": 0}]}]});
        assert!(table.changes(&edit, "m").unwrap_err().to_string().contains("1 entries"));
        // more fish in a spot are new FISH lines, up to eight; a new spot copies another
        let had = table.to_json()["spots"][0]["fish"].as_array().unwrap().len();
        let mut more = table.to_json()["spots"][0]["fish"].clone();
        while more.as_array().unwrap().len() < SPOT_FISH_MOST {
            more.as_array_mut().unwrap().push(json!({"fish": 5, "rate": 2.5, "wait_bias": 0}));
        }
        let spots = counts[0];
        let edit = json!({"spots": [{"spot": 0, "fish": more}, {"spot": spots, "like": 0, "map": 99}]});
        let (edits, _) = table.changes(&edit, "m").unwrap();
        let written = table.apply(&edits).unwrap();
        let again = read(&written, &elf).unwrap().to_json();
        assert_eq!(again["spots"][0]["fish"], more);
        assert_eq!(again["spots"][spots]["map"], 99);
        assert_eq!(again["spots"][spots]["fish"], table.to_json()["spots"][0]["fish"]);
        assert!(String::from_utf8_lossy(&written[0]).contains(&format!("FISH_MAP_NUM  {} ;", spots + 1)));
        assert!(had < SPOT_FISH_MOST);
        more.as_array_mut().unwrap().push(json!({"fish": 5, "rate": 2.5, "wait_bias": 0}));
        let error = table.changes(&json!({"spots": [{"spot": 0, "fish": more}]}), "m").unwrap_err();
        assert!(error.to_string().contains("9 entries"), "{error}");
        let fish = json!([{"fish": 3, "rate": 1, "wait_bias": 0}, {"fish": 3, "rate": 1, "wait_bias": 0},
            {"fish": 4, "rate": 1, "wait_bias": 0}, {"fish": 8, "rate": 1, "wait_bias": 0}, {"fish": 17, "rate": 0.5, "wait_bias": 0}]);
        let edit = json!({"race_fish": [{"class": 0, "number": 0, "power": 99}], "spots": [{"spot": 0, "fish": fish}]});
        let (edits, _) = table.changes(&edit, "m").unwrap();
        let again = read(&table.apply(&edits).unwrap(), &elf).unwrap();
        assert_eq!(again.to_json()["race_fish"][0]["power"], 99);
        assert_eq!(again.to_json()["spots"][0]["fish"], fish);
        assert_eq!(edits.iter().filter(|edit| edit.what.contains("power")).count(), LANGUAGES);
        eprintln!("fishing sections {counts:?} round-trip");
    }
}
