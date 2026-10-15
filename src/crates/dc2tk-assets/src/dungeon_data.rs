//! Dungeon floors (docs/assets/data.md, "Floor medals" and "Monsters"): the medal conditions of
//! `menu/dngmap/dflr<dungeon>.cfg` (`data:medals`) and the monsters of
//! `dungeon/cfg_file/mos_place<dungeon>.cfg` (`data:floors`), as [`data_table`] tables named by
//! dungeon and floor.
//!
//! CDngFloorManager::LoadDataTable (dngfloor.cpp) reads `RF_INFO floor, geostone, fast_time,
//! fishing, fishing_record, seal, spheda, [item, count] × 3;` (_ROOM_FLOOR_INFO, into
//! DNGMAP_ROOM_INFO: s8, s32, s8, s16, s8, s8, then s16 items and s8 counts) and `RF_INFO2 floor,
//! practice_type, practice_param;` (_ROOM_FLOOR_INFO2: s8, s32). LoadMonsterFile (dng_event.cpp)
//! runs `mos_place` over `FLS floor, count;` (the count is read and dropped), `FL monster, param,
//! ...;` (pairs appended to CMonsterLocateInfo's 32 slots, unchecked) and `FLE;`, for the floor
//! being entered; a later block for one floor starts it again. The param becomes the placed
//! monster's `locate_param`, the talk number (-1: it cannot be talked to; CheckMonsterTolk), shown
//! as `talk`.

use serde_json::Value;

use crate::cfg_text::Arg;
use crate::data_table::{self, Place, Record, Table, S16, S32, S8};
use crate::Result;

pub const DUNGEONS: usize = 7;
/// MONSTER_LOCATE_MAX: the monsters one floor's list holds.
pub const MONSTERS_MOST: usize = 32;

pub fn medal_paths() -> Vec<String> {
    (0..DUNGEONS).map(|dungeon| format!("menu/dngmap/dflr{dungeon}.cfg")).collect()
}

pub fn floor_paths() -> Vec<String> {
    (0..DUNGEONS).map(|dungeon| format!("dungeon/cfg_file/mos_place{dungeon}.cfg")).collect()
}

fn keys(dungeon: usize, floor: i32) -> Record {
    Record::new(&[("dungeon", Value::from(dungeon)), ("floor", Value::from(floor))])
}

/// The floor medals: one record per RF_INFO, with its RF_INFO2.
pub fn medals(files: &[Vec<u8>]) -> Result<Table> {
    let scripts = data_table::parse_all(&medal_paths(), files)?;
    let mut records = Vec::new();
    for (dungeon, script) in scripts.iter().enumerate() {
        let practice = data_table::by_first(script, "RF_INFO2");
        for (floor, places) in data_table::by_first(script, "RF_INFO") {
            let at = [(dungeon, *places.last().unwrap())];
            if script.statements[at[0].1].args().len() < 13 {
                continue;
            }
            let mut record = keys(dungeon, floor);
            record
                .arg("geostone", S8, &at, 1)
                .arg("fast_time", S32, &at, 2)
                .arg("fishing", S8, &at, 3)
                .arg("fishing_record", S16, &at, 4)
                .arg("seal", S8, &at, 5)
                .arg("spheda", S8, &at, 6);
            record.field(
                "spheda_prizes",
                Place::List {
                    at: at.to_vec(),
                    arg: 7,
                    tail: 0,
                    entry: vec![("item", S16), ("count", S8)],
                    count: (3, 3),
                },
            );
            if let Some(places) = practice.get(&floor) {
                let at = [(dungeon, *places.last().unwrap())];
                if script.statements[at[0].1].args().len() >= 3 {
                    record.arg("practice_type", S8, &at, 1).arg("practice_param", S32, &at, 2);
                }
            }
            records.push(record);
        }
    }
    let mut table = Table::new("a medal table", scripts, Vec::new());
    table.section("floors", &["dungeon", "floor"], records);
    Ok(table)
}

/// The monsters of each floor: the FL statements of its last FLS .. FLE block.
pub fn floors(files: &[Vec<u8>]) -> Result<Table> {
    let scripts = data_table::parse_all(&floor_paths(), files)?;
    let mut blocks: std::collections::BTreeMap<(usize, i32), Vec<(usize, usize)>> = Default::default();
    for (dungeon, script) in scripts.iter().enumerate() {
        let mut current: Option<i32> = None;
        for (at, statement) in script.statements.iter().enumerate() {
            match statement.tag.as_str() {
                "FLS" => {
                    let floor = statement.values().first().map_or(0, Arg::int);
                    blocks.insert((dungeon, floor), Vec::new());
                    current = Some(floor);
                }
                "FL" => {
                    if let Some(floor) = current {
                        blocks.entry((dungeon, floor)).or_default().push((dungeon, at));
                    }
                }
                "FLE" => current = None,
                _ => {}
            }
        }
    }
    let mut records = Vec::new();
    for ((dungeon, floor), at) in blocks {
        let mut record = keys(dungeon, floor);
        record.field(
            "monsters",
            Place::Lines {
                copies: vec![at],
                entry: vec![("monster", S16), ("talk", S16)],
                count: (0, MONSTERS_MOST),
                per_line: 0,
                grow: false,
            },
        );
        records.push(record);
    }
    let mut table = Table::new("a floor table", scripts, Vec::new());
    table.section("floors", &["dungeon", "floor"], records);
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

    #[test]
    fn floor_lists_fill_their_lines() {
        let files: Vec<Vec<u8>> = (0..DUNGEONS)
            .map(|dungeon| {
                let text = if dungeon == 0 { "FLS 1,3;\r\nFL 0,0,0,1;\r\nFL 8,800;\r\nFLE;\r\n" } else { "" };
                text.as_bytes().to_vec()
            })
            .collect();
        let table = floors(&files).unwrap();
        assert_eq!(table.to_json()["floors"][0]["monsters"][2], json!({"monster": 8, "talk": 800}));
        let edit = json!({"floors": [{"dungeon": 0, "floor": 1, "monsters": [{"monster": 4, "talk": 400}]}]});
        let written = table.apply(&table.changes(&edit, "m").unwrap().0).unwrap();
        assert_eq!(written[0], b"FLS 1,3;\r\nFL 4,400;\r\nFL;\r\nFLE;\r\n");
    }

    /// The medal and floor files of an extracted disc (C2S_CORPUS) read, write back unchanged,
    /// and their JSON changes nothing.
    #[test]
    fn corpus_floors_and_medals_round_trip() {
        let (Some(medal_files), Some(floor_files)) = (corpus(&medal_paths()), corpus(&floor_paths())) else {
            return;
        };
        let medals = medals(&medal_files).unwrap();
        assert_eq!(medals.write(), medal_files);
        assert_eq!(medals.check(), Vec::<String>::new());
        let floors = floors(&floor_files).unwrap();
        assert_eq!(floors.write(), floor_files);
        assert_eq!(floors.check(), Vec::<String>::new());
        eprintln!(
            "{} floor medals and {} floors' monsters round-trip",
            medals.sections[0].records.len(),
            floors.sections[0].records.len()
        );
    }
}
