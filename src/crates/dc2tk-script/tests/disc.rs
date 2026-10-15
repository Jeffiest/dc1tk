//! disc.h: the monster table read from the executable, the lines it gives each kind, and the
//! identifiers it makes.
use std::collections::HashSet;

use dc2tk_script::disc::{self, MonsterRow};
use dc2tk_script::messages::{NameKey, NameTable};
use dc2tk_script::node;
use indexmap::IndexMap;

fn elf_with(table_address: u32, records: &[Vec<u8>]) -> Vec<u8> {
    let mut header = vec![0u8; 0x100];
    header[0..4].copy_from_slice(b"\x7fELF");
    header[28..32].copy_from_slice(&0x34u32.to_le_bytes());
    header[44..46].copy_from_slice(&1u16.to_le_bytes());
    let mut data: Vec<u8> = records.concat();
    data.extend([0u8; 184]);
    for (at, value) in [1, 0x100, table_address, table_address, data.len() as u32].into_iter().enumerate() {
        header[0x34 + 4 * at..0x38 + 4 * at].copy_from_slice(&value.to_le_bytes());
    }
    header.extend(data);
    header
}

fn record(number: i16, name: &str, model: &str, script: &str) -> Vec<u8> {
    let mut out = vec![0u8; 184];
    out[0..2].copy_from_slice(&number.to_le_bytes());
    out[4..4 + name.len()].copy_from_slice(name.as_bytes());
    out[36..36 + model.len()].copy_from_slice(model.as_bytes());
    out[52..52 + script.len()].copy_from_slice(script.as_bytes());
    out
}

fn names(pairs: &[(i64, &str)]) -> NameTable {
    pairs.iter().map(|(number, name)| (NameKey::Int(*number), name.to_string())).collect()
}

#[test]
fn monster_table_reads_records_until_the_empty_one() {
    let elf =
        elf_with(0x1000, &[record(0, "Sewer Rat", "e201a", "e201a"), record(523, "Griffon Soldier", "e254a", "b254a")]);
    let table = disc::monster_table(&elf, &node!({"address": 0x1000, "record_size": 184}));
    let rows: Vec<(i64, &str, &str, &str)> =
        table.iter().map(|(kind, row)| (*kind, row.name.as_str(), row.model.as_str(), row.script.as_str())).collect();
    assert_eq!(rows, [(0, "Sewer Rat", "e201a", "e201a"), (523, "Griffon Soldier", "e254a", "b254a")]);
}

#[test]
fn monster_lines_say_which_model_and_ai_script_each_kind_uses() {
    let row = MonsterRow { name: String::new(), model: "e254a".into(), script: "b254a".into() };
    let notes: NameTable = [(NameKey::Int(523), disc::monster_note(&row))].into_iter().collect();
    let lines = disc::enum_lines(
        "monster_kind",
        "MONSTER",
        &names(&[(523, "Griffon Soldier")]),
        &mut HashSet::new(),
        Some(&notes),
    );
    assert!(lines.contains(&"    MONSTER_GRIFFON_SOLDIER = 523,  // model e254a, AI b254a".to_string()));
}

#[test]
fn identifiers_are_ascii_and_repeats_get_their_number() {
    assert_eq!(disc::identifier("Dr.Dell", "NPC").as_deref(), Some("NPC_DR_DELL"));
    assert_eq!(disc::identifier("Émile's Hat", "ITEM").as_deref(), Some("ITEM_EMILES_HAT"));
    let lines = disc::enum_lines(
        "monster_kind",
        "MONSTER",
        &names(&[(37, "Griffon Soldier"), (523, "Griffon Soldier")]),
        &mut HashSet::new(),
        None,
    );
    assert!(lines.contains(&"    MONSTER_GRIFFON_SOLDIER = 37,".to_string()));
    assert!(lines.contains(&"    MONSTER_GRIFFON_SOLDIER_523 = 523,".to_string()));
}

#[test]
fn header_skips_names_the_sdk_already_uses() {
    let found: IndexMap<String, NameTable> = [("item_id".to_string(), names(&[(1, "Attrib")]))].into_iter().collect();
    let text = disc::header(&found, &HashSet::from(["ITEM_ATTRIB".to_string()])).unwrap();
    assert!(text.contains("ITEM_ATTRIB_1 = 1,"));
    assert_eq!(disc::header(&IndexMap::new(), &HashSet::new()), None);
}
