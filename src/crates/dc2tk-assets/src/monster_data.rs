//! The monster kinds' stats (docs/assets/data.md, "Monsters") as one JSON table with named
//! fields, and edits of it patched into the boot executable: a [`data_table`] table of the
//! executable, as the other executable tables are ([`crate::exe_tables`]).
//!
//! The stats are not on the disc's DATA.DAT: they are `base_monster_define`, a table of 0xB8-byte
//! `BASE_MONSTER_TBL` records in the `.data` of `SCES_511.90` (the monster unit, vaddr 0x341DA0),
//! looked up by id (GetMonsterTable, a linear search that ends at an empty name). The engine copies
//! a record into each monster it places (SetActiveMonster); monster scripts may change some values
//! after that. Names are overwritten at boot by `dungeon/cfg_file/mosdata<language>.cfg`, so the
//! table's own name is only a label here.
//!
//! An edit writes only the bytes of the fields a mod changes, in place: the executable keeps its
//! size and layout, and an unedited table changes nothing. A mod's table holds only the monsters
//! and fields it changes; several mods' tables merge, and two giving one field different values
//! are refused.

use serde_json::Value;

use crate::data_table::{Kind, Record, Table, S16, S32, S8, U16, U32, U8};
use crate::elf::{table_offset, EXECUTABLE};
use crate::{bail, Result};

/// base_monster_define's address and the most records its 0xF740 bytes hold.
pub const TABLE_ADDRESS: u32 = 0x341DA0;
pub const RECORD: usize = 0xB8;
pub const MOST: usize = 0xF740 / RECORD;
/// A record's name (char[32], a label; an empty one ends the table) and model (char[16]).
const NAME: usize = 0x04;
const NAME_SIZE: usize = 32;
const MODEL: usize = 0x24;

/// A field: its JSON name (a `group.name` puts it in a nested object), its offset in the record,
/// its kind and how many in a row (a list when more than one).
struct Spec {
    name: &'static str,
    offset: usize,
    kind: Kind,
    count: usize,
}

const fn one(name: &'static str, offset: usize, kind: Kind) -> Spec {
    Spec { name, offset, kind, count: 1 }
}

const fn many(name: &'static str, offset: usize, kind: Kind, count: usize) -> Spec {
    Spec { name, offset, kind, count }
}

/// BASE_MONSTER_TBL (monster.hpp), its padding left out. `id` (0x00) is the key and `name` a
/// label.
const SPECS: [Spec; 40] = [
    one("grade", 0x02, S16),
    one("model", MODEL, Kind::Text(16)),
    one("script", 0x34, Kind::Text(16)),
    one("gift_type", 0x44, S16),
    one("sound_bank", 0x48, S32),
    one("unknown_4c", 0x4C, S32),
    one("hp", 0x50, S32),
    one("family", 0x54, S8),
    one("abs", 0x56, U16),
    one("money", 0x58, U16),
    one("unknown_5a", 0x5A, S16),
    one("weapon_wear", 0x5C, Kind::Float),
    one("rage_hits", 0x60, S16),
    one("guard_rate", 0x62, S8),
    many("dodge_rates", 0x63, S8, 2),
    one("attack", 0x66, S16),
    one("defence", 0x68, U8),
    one("stagger", 0x69, S8),
    one("boss", 0x6A, S8),
    one("sword_after_images", 0x6B, S8),
    one("elements.flame", 0x6C, S16),
    one("elements.chill", 0x6E, S16),
    one("elements.lightning", 0x70, S16),
    one("elements.cyclone", 0x72, S16),
    one("elements.smash", 0x74, S16),
    one("elements.exorcism", 0x76, S16),
    one("elements.beast", 0x78, S16),
    one("elements.scale", 0x7A, S16),
    many("attack_kind_rates", 0x7C, S16, 12),
    one("reaction_flags", 0x94, U32),
    one("flags", 0x98, U32),
    one("companion", 0x9C, S32),
    many("drops", 0xA0, S16, 2),
    one("rare_drop", 0xA4, S16),
    one("status_immunity", 0xA8, U32),
    one("status_rate", 0xAC, S16),
    one("percent_damage_scale", 0xAE, S16),
    one("book_area", 0xB0, S8),
    one("book_index", 0xB2, S16),
    one("shared_model", 0xB4, S16),
];

fn text_of(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&byte| byte == 0).unwrap_or(bytes.len());
    dc2tk_disc::text::decode_lossy(&bytes[..end])
}

/// The table in the executable's bytes, `{"monsters": [{"id": 0, "name": ..., "hp": ...,
/// "elements": {...}}]}`: the records up to the first with an empty name.
pub fn read(elf: &[u8]) -> Result<Table> {
    let start = table_offset(elf, TABLE_ADDRESS, RECORD, "monster table")?;
    let first = &elf[start..start + RECORD];
    if first[NAME] == 0 || first[..2] != [0, 0] || text_of(&first[MODEL..MODEL + 16]).is_empty() {
        bail!("{EXECUTABLE}: not the PAL executable (its monster table does not start with monster 0)");
    }
    let mut records = Vec::new();
    for number in (0..MOST).take_while(|number| start + (number + 1) * RECORD <= elf.len()) {
        let at = start + number * RECORD;
        let bytes = &elf[at..at + RECORD];
        if bytes[NAME] == 0 {
            break;
        }
        let id = i16::from_le_bytes([bytes[0], bytes[1]]);
        let mut record = Record::new(&[("id", Value::from(id))]);
        record.label("name", Value::from(text_of(&bytes[NAME..NAME + NAME_SIZE])));
        for spec in &SPECS {
            record.bytes(spec.name, spec.kind, at + spec.offset, spec.count);
        }
        records.push(record);
    }
    let mut table = Table::new("a monster table", Vec::new(), elf.to_vec());
    table.section("monsters", &["id"], records);
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elf::apply_patches;
    use serde_json::json;

    /// An ELF with one PT_LOAD of the table's page: two monsters and the empty-named end.
    fn sample() -> Vec<u8> {
        let mut elf = crate::elf::tests::sample();
        let start = crate::elf::file_offset(&elf, TABLE_ADDRESS).unwrap();
        for (number, (id, name, model, hp)) in
            [(0i16, "Sewer Rat", "e201a", 30i32), (5, "Bat", "e205a", 12)].into_iter().enumerate()
        {
            let record = &mut elf[start + number * RECORD..start + (number + 1) * RECORD];
            record[..2].copy_from_slice(&id.to_le_bytes());
            record[NAME..NAME + name.len()].copy_from_slice(name.as_bytes());
            record[MODEL..MODEL + model.len()].copy_from_slice(model.as_bytes());
            record[0x50..0x54].copy_from_slice(&hp.to_le_bytes());
            record[0x5C..0x60].copy_from_slice(&0.7f32.to_le_bytes());
            record[0x6C..0x6E].copy_from_slice(&100i16.to_le_bytes());
            record[0xA0..0xA2].copy_from_slice(&281i16.to_le_bytes());
        }
        elf
    }

    #[test]
    fn the_table_reads_by_id() {
        let elf = sample();
        let monsters = read(&elf).unwrap();
        assert_eq!(monsters.sections[0].records.len(), 2);
        let json = monsters.to_json();
        let rat = &json["monsters"][0];
        assert_eq!(
            (rat["name"].as_str(), rat["model"].as_str(), rat["hp"].as_i64()),
            (Some("Sewer Rat"), Some("e201a"), Some(30))
        );
        assert_eq!(rat["weapon_wear"], json!(0.7));
        assert_eq!(rat["elements"]["flame"], 100);
        assert_eq!(rat["drops"], json!([281, 0]));
        assert_eq!(json["monsters"][1]["id"], 5);
        // the key, the label, then the fields in the record's order, the elements nested
        let names: Vec<&String> = rat.as_object().unwrap().keys().take(5).collect();
        assert_eq!(names, ["id", "name", "grade", "model", "script"]);
        assert_eq!(monsters.check(), Vec::<String>::new());
        assert!(read(&[0; 64]).is_err());
    }

    #[test]
    fn edits_patch_only_their_bytes() {
        let elf = sample();
        let monsters = read(&elf).unwrap();
        let edit = json!({"monsters": [{"id": 5, "hp": 500, "elements": {"chill": 150}, "drops": [281, 7], "model": "e201a", "weapon_wear": 0.7}]});
        let (edits, patches) = monsters.changes(&edit, "a").unwrap();
        assert!(edits.is_empty());
        assert_eq!(patches.len(), 4);
        let patched = apply_patches(&elf, &patches).unwrap();
        assert_eq!(patched.len(), elf.len());
        let differing = elf.iter().zip(&patched).filter(|(a, b)| a != b).count();
        assert!(differing <= 2 + 1 + 1 + 5, "{differing}");
        let json = read(&patched).unwrap().to_json();
        assert_eq!(
            (json["monsters"][1]["hp"].as_i64(), json["monsters"][1]["model"].as_str()),
            (Some(500), Some("e201a"))
        );
        assert_eq!(json["monsters"][1]["elements"]["chill"], 150);
        let mut both = patches.clone();
        both.extend(monsters.changes(&json!({"monsters": [{"id": 5, "hp": 600}]}), "b").unwrap().1);
        assert!(apply_patches(&elf, &both).unwrap_err().to_string().contains("mod a and mod b"));
        let refused = |edit: Value| monsters.changes(&edit, "m").unwrap_err().to_string();
        assert!(refused(json!({"monsters": [{"id": 9}]})).contains("no record with id 9"));
        assert!(refused(json!({"monsters": [{"id": 0, "defence": 300}]})).contains("0 to 255"));
        assert!(refused(json!({"monsters": [{"id": 0, "name": "Rat"}]})).contains("label"));
        assert!(refused(json!({"monsters": [{"id": 0, "model": "a_model_name_too_long"}]})).contains("bytes"));
        assert!(refused(json!({"monsters": [{"id": 0, "speed": 1}]})).contains("no such field"));
        assert!(refused(json!({"monsters": [{"id": 0, "drops": [1]}]})).contains("list of 2"));
    }

    /// The executable of an extracted disc (C2S_CORPUS): its 343 monsters read, every field's
    /// bytes are the ones its value writes, and the JSON changes nothing.
    #[test]
    fn corpus_monsters_round_trip() {
        let Some(corpus) = std::env::var_os("C2S_CORPUS") else {
            return;
        };
        let elf = std::fs::read(std::path::Path::new(&corpus).join(EXECUTABLE)).unwrap();
        let monsters = read(&elf).unwrap();
        assert_eq!(monsters.sections[0].records.len(), 343);
        assert_eq!(monsters.check(), Vec::<String>::new());
        let json: Value = serde_json::from_str(&crate::json::json_text(&monsters.to_json())).unwrap();
        assert_eq!(monsters.changes(&json, "m").unwrap(), (Vec::new(), Vec::new()));
    }
}
