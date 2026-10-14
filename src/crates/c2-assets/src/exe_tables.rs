//! Tables compiled into the boot executable's `.data` (docs/assets/data.md, "Characters and other
//! executable tables"), as [`data_table`] tables patched in place like the monster table
//! ([`crate::monster_data`]): spectrumising (in `data:items`), Donny's shop (in `data:shops`) and
//! the character tables (`data:characters`). Addresses are those of `config/pal/main.symbols.txt`.

use serde_json::Value;

use crate::data_table::{self, Kind, Record, Table, S16, S8};
use crate::Result;

/// etcitem_spectol_table: s8 {slot, value} by item number - 1 (SetItemSpectolPoint).
pub const SPECTRUM_TABLE: u32 = 0x339820;
pub const SPECTRUM_ITEMS: usize = 425;
/// dony_shoplist: {s16 item, s8 level, pad} until an item of 0 or less (GetDonyShopLineUp).
pub const DONNY_TABLE: u32 = 0x358620;
pub const DONNY_ROWS: usize = 7;
/// lifetbl_2854: f32 per character, weptbl_4503: s16 [language][character × 5 + slot],
/// htbl_1662: s8 [10], mos_henge_param: 0x1C × 57, monster_progress_tbl: s16 [19][5].
pub const LIFE_TABLE: u32 = 0x37C790;
pub const WEAPON_TABLE: u32 = 0x33A510;
pub const WHP_TABLE: u32 = 0x33A490;
pub const HENGE_TABLE: u32 = 0x339D70;
pub const HENGE_ROWS: usize = 57;
pub const HENGE_RECORD: usize = 0x1C;
pub const PROGRESS_TABLE: u32 = 0x358C90;
pub const PROGRESS_ROWS: usize = 19;

pub const CHARACTERS: [&str; 2] = ["max", "monica"];

/// Spectrumising: what each item number adds to the attachment it becomes. `slot` 0-7 is an
/// attribute (flame to scale), 10 attack, 11 durability, any other nothing; `value` the points,
/// times the item's count or level. As the `items` section of `data:items`, by `item`.
pub fn spectrumise(elf: &[u8]) -> Result<Table> {
    let start = data_table::elf_offset(elf, SPECTRUM_TABLE, SPECTRUM_ITEMS * 2, "spectrumising table")?;
    let mut records = Vec::new();
    for number in 0..SPECTRUM_ITEMS {
        let mut record = Record::new(&[("item", Value::from(number + 1))]);
        record.bytes("spectrumise.slot", S8, start + number * 2, 1);
        record.bytes("spectrumise.value", S8, start + number * 2 + 1, 1);
        records.push(record);
    }
    let mut table = Table::new("an item table", Vec::new(), elf.to_vec());
    table.section("items", &["item"], records);
    Ok(table)
}

/// Donny's shop: each row's item and the invention level Donny must be above to offer it. As the
/// `donny` section of `data:shops`, by `row`.
pub fn donny(elf: &[u8]) -> Result<Table> {
    let start = data_table::elf_offset(elf, DONNY_TABLE, (DONNY_ROWS + 1) * 4, "Donny's shop")?;
    let mut records = Vec::new();
    for row in 0..DONNY_ROWS {
        let mut record = Record::new(&[("row", Value::from(row))]);
        // an item of 0 or less would end the list there
        record.bytes("item", Kind::Int(1, 0x1FF), start + row * 4, 1);
        record.bytes("level", S8, start + row * 4 + 2, 1);
        records.push(record);
    }
    let mut table = Table::new("a shop table", Vec::new(), elf.to_vec());
    table.section("donny", &["row"], records);
    Ok(table)
}

/// The character tables (`data:characters`): starting HP and equipment, the WHP a weapon gains
/// at a level-up, Monica's monster forms and the badges' forms.
pub fn characters(elf: &[u8]) -> Result<Table> {
    let life = data_table::elf_offset(elf, LIFE_TABLE, 8, "starting HP table")?;
    let weapons = data_table::elf_offset(elf, WEAPON_TABLE, 40, "starting weapon table")?;
    let mut table = Table::new("a character table", Vec::new(), elf.to_vec());
    let mut characters = Vec::new();
    for (number, name) in CHARACTERS.iter().enumerate() {
        let mut record = Record::new(&[("character", Value::from(*name))]);
        record.bytes("starting_hp", Kind::Float, life + number * 4, 1);
        record.bytes("starting_equipment", S16, weapons + 20 + number * 10, 5);
        record.bytes("starting_equipment_japanese", S16, weapons + number * 10, 5);
        characters.push(record);
    }
    table.section("characters", &["character"], characters);
    let whp = data_table::elf_offset(elf, WHP_TABLE, 10, "WHP gain table")?;
    let mut level_up = Record::new(&[]);
    level_up.bytes("whp_gain", S8, whp, 10);
    table.section("weapon_level_up", &[], vec![level_up]);
    let henge = data_table::elf_offset(elf, HENGE_TABLE, HENGE_ROWS * HENGE_RECORD, "monster form table")?;
    let mut forms = Vec::new();
    for row in 0..HENGE_ROWS {
        let at = henge + row * HENGE_RECORD;
        let mut record = Record::new(&[("monster", Value::from(i16::from_le_bytes([elf[at], elf[at + 1]])))]);
        record.bytes("attack", S16, at + 2, 1).bytes("defence", S16, at + 4, 1);
        forms.push(record);
    }
    table.section("monster_forms", &["monster"], forms);
    let progress = data_table::elf_offset(elf, PROGRESS_TABLE, PROGRESS_ROWS * 10, "badge table")?;
    let mut badges = Vec::new();
    for row in 0..PROGRESS_ROWS {
        let mut record = Record::new(&[("row", Value::from(row))]);
        record.bytes("badge", S16, progress + row * 10, 1).bytes("forms", S16, progress + row * 10 + 2, 4);
        badges.push(record);
    }
    table.section("badges", &["row"], badges);
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The executable's tables (C2S_CORPUS) read, every value's bytes are the ones it writes, and
    /// their JSON changes nothing.
    #[test]
    fn corpus_executable_tables_round_trip() {
        let Some(corpus) = std::env::var_os("C2S_CORPUS") else {
            return;
        };
        let elf = std::fs::read(std::path::Path::new(&corpus).join(crate::monster_data::EXECUTABLE)).unwrap();
        for table in [spectrumise(&elf).unwrap(), donny(&elf).unwrap(), characters(&elf).unwrap()] {
            assert_eq!(table.check(), Vec::<String>::new(), "{}", table.what);
        }
        let characters = characters(&elf).unwrap();
        let json = characters.to_json();
        assert_eq!(json["characters"][0]["starting_equipment"][0], 1);
        let (edits, patches) = characters
            .changes(&json!({"weapon_level_up": [{"whp_gain": [9, 9, 9, 9, 9, 9, 9, 9, 9, 9]}]}), "m")
            .unwrap();
        assert!(edits.is_empty() && !patches.is_empty());
        let patched = data_table::apply_patches(&elf, &patches).unwrap();
        assert_eq!(super::characters(&patched).unwrap().to_json()["weapon_level_up"][0]["whp_gain"][3], 9);
        assert_eq!(donny(&elf).unwrap().to_json()["donny"].as_array().unwrap().len(), DONNY_ROWS);
    }
}
