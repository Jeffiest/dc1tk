//! Tables compiled into the boot executable's `.data` (docs/game/data.md, "Characters and other
//! executable tables"), as [`data_table`] tables patched in place like the monster table
//! ([`crate::monster_data`]): spectrumising (in `data:items`), Donny's shop (in `data:shops`) and
//! the character tables (`data:characters`). Addresses are those of `config/pal/main.symbols.txt`.

use serde_json::Value;

use crate::bytes::u16_at;
use crate::data_table::{self, Kind, Record, Table, S16, S8};
use crate::elf::table_offset;
use crate::item_data::ITEM_MOST;
use crate::Result;

/// etcitem_spectol_table: s8 {slot, value} by item number - 1 (SetItemSpectolPoint).
pub const SPECTRUM_TABLE: u32 = 0x339820;
pub const SPECTRUM_ITEMS: usize = 425;
/// dony_shoplist: {s16 item, s8 level, pad} until an item of 0 or less (GetDonyShopLineUp).
pub const DONNY_TABLE: u32 = 0x358620;
pub const DONNY_ROWS: usize = 7;
pub const DONNY_RECORD: usize = 4;
/// lifetbl_2854: f32 per character, weptbl_4503: s16 [language][character × 5 + slot],
/// htbl_1662: s8 [10], mos_henge_param: 0x1C × 57, monster_progress_tbl: s16 [19][5].
pub const LIFE_TABLE: u32 = 0x37C790;
pub const WEAPON_TABLE: u32 = 0x33A510;
/// The starting equipment's slots a character has, and the bytes of a language's table (two
/// characters of five s16 slots; the Japanese first).
pub const WEAPON_SLOTS: usize = 5;
pub const WEAPON_LANGUAGE: usize = CHARACTERS.len() * WEAPON_SLOTS * 2;
pub const WHP_TABLE: u32 = 0x33A490;
pub const WHP_LEVELS: usize = 10;
pub const HENGE_TABLE: u32 = 0x339D70;
pub const HENGE_ROWS: usize = 57;
pub const HENGE_RECORD: usize = 0x1C;
pub const PROGRESS_TABLE: u32 = 0x358C90;
pub const PROGRESS_ROWS: usize = 19;
/// A badge row: the badge and its four forms, s16 each.
pub const PROGRESS_RECORD: usize = 10;

pub const CHARACTERS: [&str; 2] = ["max", "monica"];
/// gift_item_tbl: s16 {gift type, item, unread} rows up to one of gift type -1 (CheckGiftPack).
pub const GIFT_TABLE: u32 = 0x351660;
pub const GIFT_ROWS: usize = 14;
pub const GIFT_RECORD: usize = 6;
/// GolfClubDef: {f32 power, f32, s32} for Spheda clubs 9 to 14 (GetSphidaClubDef: club - 9); a
/// seventh row, of zeros, no club reaches.
pub const CLUB_TABLE: u32 = 0x361340;
pub const CLUBS: std::ops::RangeInclusive<usize> = 9..=14;
pub const CLUB_ROWS: usize = 7;
pub const CLUB_RECORD: usize = 0xC;
/// aquafish_mixTable: s8 {parent, parent, child} × 171, each a fish item less 0x136
/// (GetChildFishNo, a fixed count of rows).
pub const BREEDING_TABLE: u32 = 0x3535A0;
pub const BREEDING_ROWS: usize = 171;
/// The fish item a breeding row's numbers count from.
pub const FIRST_FISH: i64 = 0x136;

/// Spectrumising: what each item number adds to the attachment it becomes. `slot` 0-7 is an
/// attribute (flame to scale), 10 attack, 11 durability, any other nothing; `value` the points,
/// times the item's count or level. As the `items` section of `data:items`, by `item`.
pub fn spectrumise(elf: &[u8]) -> Result<Table> {
    let start = table_offset(elf, SPECTRUM_TABLE, SPECTRUM_ITEMS * 2, "spectrumising table")?;
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
    let start = table_offset(elf, DONNY_TABLE, (DONNY_ROWS + 1) * DONNY_RECORD, "Donny's shop")?;
    let mut records = Vec::new();
    for row in 0..DONNY_ROWS {
        let at = start + row * DONNY_RECORD;
        let mut record = Record::new(&[("row", Value::from(row))]);
        // an item of 0 or less would end the list there
        record.bytes("item", Kind::Int(1, ITEM_MOST), at, 1);
        record.bytes("level", S8, at + 2, 1);
        records.push(record);
    }
    let mut table = Table::new("a shop table", Vec::new(), elf.to_vec());
    table.section("donny", &["row"], records);
    Ok(table)
}

/// The character tables (`data:characters`): starting HP and equipment, the WHP a weapon gains
/// at a level-up, Monica's monster forms and the badges' forms.
pub fn characters(elf: &[u8]) -> Result<Table> {
    let life = table_offset(elf, LIFE_TABLE, CHARACTERS.len() * 4, "starting HP table")?;
    let weapons = table_offset(elf, WEAPON_TABLE, 2 * WEAPON_LANGUAGE, "starting weapon table")?;
    let mut table = Table::new("a character table", Vec::new(), elf.to_vec());
    let mut characters = Vec::new();
    for (number, name) in CHARACTERS.iter().enumerate() {
        let mut record = Record::new(&[("character", Value::from(*name))]);
        let slots = number * WEAPON_SLOTS * 2;
        record.bytes("starting_hp", Kind::Float, life + number * 4, 1);
        record.bytes("starting_equipment", S16, weapons + WEAPON_LANGUAGE + slots, WEAPON_SLOTS);
        record.bytes("starting_equipment_japanese", S16, weapons + slots, WEAPON_SLOTS);
        characters.push(record);
    }
    table.section("characters", &["character"], characters);
    let whp = table_offset(elf, WHP_TABLE, WHP_LEVELS, "WHP gain table")?;
    let mut level_up = Record::new(&[]);
    level_up.bytes("whp_gain", S8, whp, WHP_LEVELS);
    table.section("weapon_level_up", &[], vec![level_up]);
    let henge = table_offset(elf, HENGE_TABLE, HENGE_ROWS * HENGE_RECORD, "monster form table")?;
    let mut forms = Vec::new();
    for row in 0..HENGE_ROWS {
        let at = henge + row * HENGE_RECORD;
        let mut record = Record::new(&[("monster", Value::from(u16_at(elf, at)? as i16))]);
        record.bytes("attack", S16, at + 2, 1).bytes("defence", S16, at + 4, 1);
        forms.push(record);
    }
    table.section("monster_forms", &["monster"], forms);
    let progress = table_offset(elf, PROGRESS_TABLE, PROGRESS_ROWS * PROGRESS_RECORD, "badge table")?;
    let mut badges = Vec::new();
    for row in 0..PROGRESS_ROWS {
        let at = progress + row * PROGRESS_RECORD;
        let mut record = Record::new(&[("row", Value::from(row))]);
        record.bytes("badge", S16, at, 1).bytes("forms", S16, at + 2, 4);
        badges.push(record);
    }
    table.section("badges", &["row"], badges);
    Ok(table)
}

/// The gifts that win a monster's badge (`data:gifts`, gift_item_tbl): by `row`, the
/// `gift_type` a monster kind names (data:monsters' `gift_type`), the `item` given and a third
/// value nothing reads (`unread`). CheckGiftPack (monster.cpp) finds the row of the monster's
/// gift type and takes a gift whose three gift slots all hold the item. The rows are the
/// table's; none is added (the row after the last ends it).
pub fn gifts(elf: &[u8]) -> Result<Table> {
    let start = table_offset(elf, GIFT_TABLE, (GIFT_ROWS + 1) * GIFT_RECORD, "gift table")?;
    let mut records = Vec::new();
    for row in 0..GIFT_ROWS {
        let at = start + row * GIFT_RECORD;
        let mut record = Record::new(&[("row", Value::from(row))]);
        // a gift type of -1 would end the table there
        record.bytes("gift_type", Kind::Int(0, i16::MAX as i64), at, 1);
        record.bytes("item", Kind::Int(0, ITEM_MOST), at + 2, 1).bytes("unread", S16, at + 4, 1);
        records.push(record);
    }
    let mut table = Table::new("a gift table", Vec::new(), elf.to_vec());
    table.section("gifts", &["row"], records);
    Ok(table)
}

/// Spheda's clubs (`data:spheda`, GolfClubDef): by `club` (9 to 14), the `power` the status
/// shows and two values only event scripts read (_SPHIDA_GET_CULB_DEF), `unknown_4` and
/// `unknown_8`.
pub fn spheda(elf: &[u8]) -> Result<Table> {
    let start = table_offset(elf, CLUB_TABLE, CLUB_ROWS * CLUB_RECORD, "Spheda club table")?;
    let mut records = Vec::new();
    for club in CLUBS {
        let at = start + (club - CLUBS.start()) * CLUB_RECORD;
        let mut record = Record::new(&[("club", Value::from(club))]);
        record.bytes("power", Kind::Float, at, 1).bytes("unknown_4", Kind::Float, at + 4, 1);
        record.bytes("unknown_8", data_table::S32, at + 8, 1);
        records.push(record);
    }
    let mut table = Table::new("a Spheda table", Vec::new(), elf.to_vec());
    table.section("clubs", &["club"], records);
    Ok(table)
}

/// Aquarium breeding (`data:breeding`, aquafish_mixTable): by `row`, the two `parents` and the
/// `child`, fish item numbers (0x136 to 0x1B4: the table keeps each less 0x136 in a byte). The
/// 171 rows are all GetChildFishNo reads, in either order of the parents; a pair no row has
/// breeds item 0x136.
pub fn breeding(elf: &[u8]) -> Result<Table> {
    let start = table_offset(elf, BREEDING_TABLE, BREEDING_ROWS * 3, "breeding table")?;
    let mut records = Vec::new();
    for row in 0..BREEDING_ROWS {
        let at = start + row * 3;
        let mut record = Record::new(&[("row", Value::from(row))]);
        record.bytes("parents", S8, at, 2).bytes("child", S8, at + 2, 1);
        records.push(record);
    }
    let mut table = Table::new("a breeding table", Vec::new(), elf.to_vec());
    table.section("breeding", &["row"], records);
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The executable's tables (DC2TK_CORPUS) read, every value's bytes are the ones it writes, and
    /// their JSON changes nothing.
    #[test]
    #[ignore = "needs DC2TK_CORPUS"]
    fn corpus_executable_tables_round_trip() {
        let corpus = dc2tk_disc::samples::corpus();
        let elf = std::fs::read(std::path::Path::new(&corpus).join(crate::elf::EXECUTABLE)).unwrap();
        for table in [
            spectrumise(&elf).unwrap(),
            donny(&elf).unwrap(),
            characters(&elf).unwrap(),
            gifts(&elf).unwrap(),
            spheda(&elf).unwrap(),
            breeding(&elf).unwrap(),
        ] {
            assert_eq!(table.check(), Vec::<String>::new(), "{}", table.what);
        }
        let characters = characters(&elf).unwrap();
        let json = characters.to_json();
        assert_eq!(json["characters"][0]["starting_equipment"][0], 1);
        let (edits, patches) = characters
            .changes(&json!({"weapon_level_up": [{"whp_gain": [9, 9, 9, 9, 9, 9, 9, 9, 9, 9]}]}), "m")
            .unwrap();
        assert!(edits.is_empty() && !patches.is_empty());
        let patched = crate::elf::apply_patches(&elf, &patches).unwrap();
        assert_eq!(super::characters(&patched).unwrap().to_json()["weapon_level_up"][0]["whp_gain"][3], 9);
        assert_eq!(donny(&elf).unwrap().to_json()["donny"].as_array().unwrap().len(), DONNY_ROWS);
        // the table after the gifts ends at a gift type of -1; the clubs and breeding as found
        let start = table_offset(&elf, GIFT_TABLE, (GIFT_ROWS + 1) * GIFT_RECORD, "gift table").unwrap();
        let end = start + GIFT_ROWS * GIFT_RECORD;
        assert_eq!(&elf[end..end + 2], &(-1i16).to_le_bytes());
        assert_eq!(
            gifts(&elf).unwrap().to_json()["gifts"][0],
            json!({"row": 0, "gift_type": 0, "item": 0x10D, "unread": 1})
        );
        assert_eq!(
            spheda(&elf).unwrap().to_json()["clubs"][0],
            json!({"club": 9, "power": 38, "unknown_4": 0.2, "unknown_8": 6})
        );
        let breeding = breeding(&elf).unwrap();
        assert_eq!(breeding.to_json()["breeding"][170], json!({"row": 170, "parents": [0x1A, 0x1A], "child": 0x1A}));
        let (_, patches) = breeding.changes(&json!({"breeding": [{"row": 0, "child": 5}]}), "m").unwrap();
        let patched = crate::elf::apply_patches(&elf, &patches).unwrap();
        assert_eq!(super::breeding(&patched).unwrap().to_json()["breeding"][0]["child"], 5);
        assert!(gifts(&elf).unwrap().changes(&json!({"gifts": [{"row": 0, "gift_type": -1}]}), "m").is_err());
    }
}
