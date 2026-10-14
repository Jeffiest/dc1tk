//! The monster kinds' stats (docs/assets/data.md, "Monsters") as one JSON table with named
//! fields, and edits of it patched into the boot executable.
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

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::{bail, Result};

/// The boot executable, as the disc's root names it.
pub const EXECUTABLE: &str = "SCES_511.90";
/// base_monster_define's address and the most records its 0xF740 bytes hold.
pub const TABLE_ADDRESS: u32 = 0x341DA0;
pub const RECORD: usize = 0xB8;
pub const MOST: usize = 0xF740 / RECORD;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    F32,
    /// A NUL-ended string in this many bytes.
    Text(usize),
}

impl Kind {
    fn size(self) -> usize {
        match self {
            Kind::I8 | Kind::U8 => 1,
            Kind::I16 | Kind::U16 => 2,
            Kind::I32 | Kind::U32 | Kind::F32 => 4,
            Kind::Text(size) => size,
        }
    }

    fn range(self) -> (i64, i64) {
        match self {
            Kind::I8 => (-128, 127),
            Kind::U8 => (0, 255),
            Kind::I16 => (-32768, 32767),
            Kind::U16 => (0, 65535),
            Kind::I32 => (i32::MIN as i64, i32::MAX as i64),
            _ => (0, u32::MAX as i64),
        }
    }
}

/// A field: its JSON name (a `group.name` puts it in a nested object), its offset in the record,
/// its kind and how many in a row (a list when more than one).
#[derive(Clone, Copy, Debug)]
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

/// BASE_MONSTER_TBL (monster.hpp), its padding left out. `id` is the key and `name` a label.
const SPECS: [Spec; 41] = [
    one("grade", 0x02, Kind::I16),
    one("model", 0x24, Kind::Text(16)),
    one("script", 0x34, Kind::Text(16)),
    one("gift_type", 0x44, Kind::I16),
    one("sound_bank", 0x48, Kind::I32),
    one("unknown_4c", 0x4C, Kind::I32),
    one("hp", 0x50, Kind::I32),
    one("family", 0x54, Kind::I8),
    one("abs", 0x56, Kind::U16),
    one("money", 0x58, Kind::U16),
    one("unknown_5a", 0x5A, Kind::I16),
    one("weapon_wear", 0x5C, Kind::F32),
    one("rage_hits", 0x60, Kind::I16),
    one("guard_rate", 0x62, Kind::I8),
    many("dodge_rates", 0x63, Kind::I8, 2),
    one("attack", 0x66, Kind::I16),
    one("defence", 0x68, Kind::U8),
    one("stagger", 0x69, Kind::I8),
    one("boss", 0x6A, Kind::I8),
    one("sword_after_images", 0x6B, Kind::I8),
    one("elements.flame", 0x6C, Kind::I16),
    one("elements.chill", 0x6E, Kind::I16),
    one("elements.lightning", 0x70, Kind::I16),
    one("elements.cyclone", 0x72, Kind::I16),
    one("elements.smash", 0x74, Kind::I16),
    one("elements.exorcism", 0x76, Kind::I16),
    one("elements.beast", 0x78, Kind::I16),
    one("elements.scale", 0x7A, Kind::I16),
    many("attack_kind_rates", 0x7C, Kind::I16, 12),
    one("unknown_94", 0x94, Kind::U32),
    one("flags", 0x98, Kind::U32),
    one("companion", 0x9C, Kind::I32),
    many("drops", 0xA0, Kind::I16, 2),
    one("rare_drop", 0xA4, Kind::I16),
    one("status_immunity", 0xA8, Kind::U32),
    one("status_rate", 0xAC, Kind::I16),
    one("percent_damage_scale", 0xAE, Kind::I16),
    one("book_area", 0xB0, Kind::I8),
    one("book_index", 0xB2, Kind::I16),
    one("shared_model", 0xB4, Kind::I16),
    // kept last: the name, a label
    one("name", 0x04, Kind::Text(32)),
];

/// Where a vaddr is in an ELF file: by the program header that loads it from the file.
pub fn file_offset(elf: &[u8], vaddr: u32) -> Option<usize> {
    if elf.len() < 0x34 || &elf[..4] != b"\x7fELF" {
        return None;
    }
    let u16_at = |at: usize| u16::from_le_bytes([elf[at], elf[at + 1]]) as usize;
    let u32_at = |at: usize| elf.get(at..at + 4).map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()));
    let (table, size, count) = (u32_at(0x1C)? as usize, u16_at(0x2A), u16_at(0x2C));
    for number in 0..count {
        let header = table + number * size;
        let (kind, offset, address, file_size) =
            (u32_at(header)?, u32_at(header + 4)?, u32_at(header + 8)?, u32_at(header + 16)?);
        if kind == 1 && vaddr >= address && vaddr - address < file_size {
            return Some((offset + (vaddr - address)) as usize);
        }
    }
    None
}

#[derive(Clone, Debug)]
pub struct Monsters {
    elf: Vec<u8>,
    /// Where the table starts in the file, and each record's id by index.
    start: usize,
    ids: Vec<i16>,
}

fn text_of(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&byte| byte == 0).unwrap_or(bytes.len());
    c2_disc::text::decode_lossy(&bytes[..end])
}

impl Monsters {
    /// The table in the executable's bytes: the records up to the first with an empty name.
    pub fn read(elf: &[u8]) -> Result<Monsters> {
        let Some(start) = file_offset(elf, TABLE_ADDRESS).filter(|start| start + RECORD <= elf.len()) else {
            bail!("{EXECUTABLE}: not the PAL executable (no monster table at 0x{TABLE_ADDRESS:X})");
        };
        let mut ids = Vec::new();
        for number in (0..MOST).take_while(|number| start + (number + 1) * RECORD <= elf.len()) {
            let record = &elf[start + number * RECORD..start + (number + 1) * RECORD];
            if record[4] == 0 {
                break;
            }
            ids.push(i16::from_le_bytes([record[0], record[1]]));
        }
        let first = &elf[start..start + RECORD];
        if ids.first() != Some(&0) || text_of(&first[0x24..0x34]).is_empty() {
            bail!("{EXECUTABLE}: not the PAL executable (its monster table does not start with monster 0)");
        }
        Ok(Monsters { elf: elf.to_vec(), start, ids })
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    fn at(&self, index: usize, spec: &Spec, number: usize) -> usize {
        self.start + index * RECORD + spec.offset + number * spec.kind.size()
    }

    fn scalar(&self, place: usize, kind: Kind) -> Value {
        let bytes = &self.elf[place..place + kind.size()];
        match kind {
            Kind::I8 => Value::from(bytes[0] as i8),
            Kind::U8 => Value::from(bytes[0]),
            Kind::I16 => Value::from(i16::from_le_bytes([bytes[0], bytes[1]])),
            Kind::U16 => Value::from(u16::from_le_bytes([bytes[0], bytes[1]])),
            Kind::I32 => Value::from(i32::from_le_bytes(bytes.try_into().unwrap())),
            Kind::U32 => Value::from(u32::from_le_bytes(bytes.try_into().unwrap())),
            Kind::F32 => {
                let value = f32::from_le_bytes(bytes.try_into().unwrap());
                // the shortest text that reads back as the same f32
                let text = format!("{value}");
                serde_json::from_str(&text).unwrap_or(Value::Null)
            }
            Kind::Text(_) => Value::from(text_of(bytes)),
        }
    }

    fn value(&self, index: usize, spec: &Spec) -> Value {
        if spec.count == 1 {
            return self.scalar(self.at(index, spec, 0), spec.kind);
        }
        Value::Array((0..spec.count).map(|number| self.scalar(self.at(index, spec, number), spec.kind)).collect())
    }

    /// The table as JSON: `{"monsters": [{"id": 0, "name": ..., "hp": ..., "elements": {...}}]}`.
    pub fn to_json(&self) -> Value {
        let mut monsters = Vec::new();
        let name_spec = SPECS[SPECS.len() - 1];
        for (index, &id) in self.ids.iter().enumerate() {
            let mut object = Map::new();
            object.insert("id".into(), Value::from(id));
            object.insert("name".into(), self.value(index, &name_spec));
            for spec in &SPECS[..SPECS.len() - 1] {
                match spec.name.split_once('.') {
                    Some((group, name)) => {
                        let nested = object.entry(group).or_insert_with(|| Value::Object(Map::new()));
                        nested.as_object_mut().unwrap().insert(name.into(), self.value(index, spec));
                    }
                    None => {
                        object.insert(spec.name.into(), self.value(index, spec));
                    }
                }
            }
            monsters.push(Value::Object(object));
        }
        let mut top = Map::new();
        top.insert("monsters".into(), Value::Array(monsters));
        Value::Object(top)
    }

    /// The bytes of one value of a field, from a mod's JSON.
    fn bytes(spec: &Spec, value: &Value, what: &str) -> Result<Vec<u8>> {
        match spec.kind {
            Kind::F32 => {
                let Some(number) = value.as_f64().filter(|number| number.is_finite()) else {
                    bail!("{what} is a number");
                };
                Ok((number as f32).to_le_bytes().to_vec())
            }
            Kind::Text(size) => {
                let Some(text) = value.as_str() else {
                    bail!("{what} is a string");
                };
                let Some(mut bytes) = c2_disc::text::encode(text) else {
                    bail!("{what}: {text:?} has characters the game's text cannot hold");
                };
                if bytes.len() >= size || bytes.contains(&0) {
                    bail!("{what}: {text:?} is {} bytes; the game keeps {}", bytes.len(), size - 1);
                }
                bytes.resize(size, 0);
                Ok(bytes)
            }
            kind => {
                let (low, high) = kind.range();
                let Some(number) = value.as_i64() else {
                    bail!("{what} is a whole number");
                };
                if number < low || number > high {
                    bail!("{what} is {number}; the game keeps {low} to {high}");
                }
                Ok(number.to_le_bytes()[..kind.size()].to_vec())
            }
        }
    }

    /// The patches a mod's table makes: only fields whose values differ from this table's. A
    /// monster is named by `id`; `name` is a label; a field the record does not have, or a value
    /// its engine field cannot hold, is refused.
    pub fn changes(&self, table: &Value, module: &str) -> Result<Vec<Patch>> {
        let Some(records) = table.get("monsters").and_then(Value::as_array) else {
            bail!("a monster table is {{\"monsters\": [records]}}, as mod asset writes it");
        };
        let mut out = Vec::new();
        for record in records {
            let Some(object) = record.as_object() else {
                bail!("each monster is an object with \"id\" and the fields it changes");
            };
            let Some(id) = object.get("id").and_then(Value::as_i64) else {
                bail!("each monster names its id: {{\"id\": 0, ...}}");
            };
            let Some(index) = self.ids.iter().position(|&known| known as i64 == id) else {
                bail!("monster {id}: the table has no such monster");
            };
            let name = self.value(index, &SPECS[SPECS.len() - 1]);
            let label = format!("monster {id} ({})", name.as_str().unwrap_or_default());
            let mut given: Vec<(String, &Value)> = Vec::new();
            for (key, value) in object {
                match (key.as_str(), value) {
                    ("id", _) => {}
                    ("name", _) if *value == name => {}
                    ("name", _) => {
                        bail!("{label}: \"name\" is a label; monster names are text (dungeon/cfg_file/mosdata*.cfg)")
                    }
                    ("elements", Value::Object(nested)) => {
                        given.extend(nested.iter().map(|(name, value)| (format!("elements.{name}"), value)))
                    }
                    _ => given.push((key.clone(), value)),
                }
            }
            for (name, value) in given {
                let what = format!("{label} {name}");
                let Some(spec) = SPECS[..SPECS.len() - 1].iter().find(|spec| spec.name == name) else {
                    bail!("{what}: no such field");
                };
                let values: Vec<&Value> = if spec.count == 1 {
                    vec![value]
                } else {
                    match value.as_array() {
                        Some(list) if list.len() == spec.count => list.iter().collect(),
                        _ => bail!("{what} is a list of {}", spec.count),
                    }
                };
                for (number, value) in values.into_iter().enumerate() {
                    let bytes = Self::bytes(spec, value, &what)?;
                    let place = self.at(index, spec, number);
                    if self.elf[place..place + bytes.len()] == bytes[..] {
                        continue;
                    }
                    // a string the same up to its NUL is the same string
                    if matches!(spec.kind, Kind::Text(_)) && self.scalar(place, spec.kind) == *value {
                        continue;
                    }
                    out.push(Patch {
                        offset: place,
                        bytes,
                        what: if spec.count == 1 { what.clone() } else { format!("{what}[{number}]") },
                        module: module.to_string(),
                    });
                }
            }
        }
        Ok(out)
    }

    /// The executable with the patches made; two that give one field different bytes are refused.
    pub fn apply(&self, patches: &[Patch]) -> Result<Vec<u8>> {
        let mut elf = self.elf.clone();
        let mut done: BTreeMap<usize, &Patch> = BTreeMap::new();
        for patch in patches {
            if let Some(other) = done.get(&patch.offset) {
                if other.bytes != patch.bytes {
                    bail!("{} is given two values, by mod {} and mod {}", patch.what, other.module, patch.module);
                }
                continue;
            }
            done.insert(patch.offset, patch);
            elf[patch.offset..patch.offset + patch.bytes.len()].copy_from_slice(&patch.bytes);
        }
        Ok(elf)
    }
}

/// Bytes a mod changes in the executable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Patch {
    pub offset: usize,
    pub bytes: Vec<u8>,
    pub what: String,
    pub module: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// An ELF with one PT_LOAD of the table's page: two monsters and the empty-named end.
    fn sample() -> Vec<u8> {
        let base = 0x341000u32;
        let mut elf = vec![0u8; 0x100 + 0x2000];
        elf[..4].copy_from_slice(b"\x7fELF");
        elf[0x1C..0x20].copy_from_slice(&0x34u32.to_le_bytes());
        elf[0x2A..0x2C].copy_from_slice(&32u16.to_le_bytes());
        elf[0x2C..0x2E].copy_from_slice(&1u16.to_le_bytes());
        let header = [1u32, 0x100, base, base, 0x2000, 0x2000];
        for (at, word) in header.iter().enumerate() {
            elf[0x34 + at * 4..0x38 + at * 4].copy_from_slice(&word.to_le_bytes());
        }
        let start = 0x100 + (TABLE_ADDRESS - base) as usize;
        for (number, (id, name, model, hp)) in
            [(0i16, "Sewer Rat", "e201a", 30i32), (5, "Bat", "e205a", 12)].into_iter().enumerate()
        {
            let record = &mut elf[start + number * RECORD..start + (number + 1) * RECORD];
            record[..2].copy_from_slice(&id.to_le_bytes());
            record[4..4 + name.len()].copy_from_slice(name.as_bytes());
            record[0x24..0x24 + model.len()].copy_from_slice(model.as_bytes());
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
        assert_eq!(file_offset(&elf, TABLE_ADDRESS), Some(0x100 + 0xDA0));
        let monsters = Monsters::read(&elf).unwrap();
        assert_eq!(monsters.len(), 2);
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
        assert!(monsters.changes(&json, "m").unwrap().is_empty());
        assert!(Monsters::read(&[0; 64]).is_err());
    }

    #[test]
    fn edits_patch_only_their_bytes() {
        let elf = sample();
        let monsters = Monsters::read(&elf).unwrap();
        let edit = json!({"monsters": [{"id": 5, "hp": 500, "elements": {"chill": 150}, "drops": [281, 7], "model": "e201a", "weapon_wear": 0.7}]});
        let patches = monsters.changes(&edit, "a").unwrap();
        assert_eq!(patches.len(), 4);
        let patched = monsters.apply(&patches).unwrap();
        assert_eq!(patched.len(), elf.len());
        let differing = elf.iter().zip(&patched).filter(|(a, b)| a != b).count();
        assert!(differing <= 2 + 1 + 1 + 5, "{differing}");
        let json = Monsters::read(&patched).unwrap().to_json();
        assert_eq!(
            (json["monsters"][1]["hp"].as_i64(), json["monsters"][1]["model"].as_str()),
            (Some(500), Some("e201a"))
        );
        assert_eq!(json["monsters"][1]["elements"]["chill"], 150);
        let mut both = patches.clone();
        both.extend(monsters.changes(&json!({"monsters": [{"id": 5, "hp": 600}]}), "b").unwrap());
        assert!(monsters.apply(&both).unwrap_err().to_string().contains("mod a and mod b"));
        let refused = |edit: Value| monsters.changes(&edit, "m").unwrap_err().to_string();
        assert!(refused(json!({"monsters": [{"id": 9}]})).contains("no such monster"));
        assert!(refused(json!({"monsters": [{"id": 0, "defence": 300}]})).contains("0 to 255"));
        assert!(refused(json!({"monsters": [{"id": 0, "name": "Rat"}]})).contains("label"));
        assert!(refused(json!({"monsters": [{"id": 0, "model": "a_model_name_too_long"}]})).contains("bytes"));
        assert!(refused(json!({"monsters": [{"id": 0, "speed": 1}]})).contains("no such field"));
    }

    /// The executable of an extracted disc (C2S_CORPUS): its 343 monsters read, every field's
    /// bytes are the ones its value writes, and the JSON changes nothing.
    #[test]
    fn corpus_monsters_round_trip() {
        let Some(corpus) = std::env::var_os("C2S_CORPUS") else {
            return;
        };
        let elf = std::fs::read(std::path::Path::new(&corpus).join(EXECUTABLE)).unwrap();
        let monsters = Monsters::read(&elf).unwrap();
        assert_eq!(monsters.len(), 343);
        for index in 0..monsters.len() {
            for spec in &SPECS {
                for number in 0..spec.count {
                    let place = monsters.at(index, spec, number);
                    let value = monsters.scalar(place, spec.kind);
                    let bytes = Monsters::bytes(spec, &value, spec.name).unwrap();
                    let wanted = &elf[place..place + bytes.len()];
                    let same = match spec.kind {
                        Kind::Text(_) => text_of(&bytes) == text_of(wanted),
                        _ => bytes == wanted,
                    };
                    assert!(same, "monster {} {}", monsters.ids[index], spec.name);
                }
            }
        }
        let json: Value = serde_json::from_str(&crate::item_data::json_text(&monsters.to_json())).unwrap();
        assert!(monsters.changes(&json, "m").unwrap().is_empty());
        assert_eq!(monsters.apply(&[]).unwrap(), elf);
    }
}
