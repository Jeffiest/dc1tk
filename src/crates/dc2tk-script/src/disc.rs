//! disc.h: names read from the disc a tree was decompiled from (monster kinds from the game
//! executable's monster table, items and characters from the language's tables), as enums the
//! decompiled source uses. Generated, never committed, since the names are the game's own text.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

use indexmap::IndexMap;
use regex::Regex;
use unicode_normalization::UnicodeNormalization;

use crate::definitions::Definitions;
use crate::messages::{self, NameKey, NameTable};
use crate::node::Node;

pub const HEADER: &str = "disc.h";
const MONSTER_ID: usize = 0;
const MONSTER_NAME: (usize, usize) = (4, 36);
const MONSTER_MODEL: (usize, usize) = (36, 52);
const MONSTER_SCRIPT: (usize, usize) = (52, 68);

fn text_at(record: &[u8], span: (usize, usize)) -> String {
    let field = &record[span.0..span.1];
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    dc2tk_disc::text::decode_lossy(&field[..end])
}

fn u32_at(data: &[u8], offset: usize) -> Option<u32> {
    data.get(offset..offset + 4).map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
}

/// Where a virtual address of an ELF is in its file.
pub fn file_offset(elf: &[u8], address: u64) -> Option<usize> {
    let table = u32_at(elf, 28)? as usize;
    let count = u16::from_le_bytes(elf.get(44..46)?.try_into().unwrap()) as usize;
    for number in 0..count {
        let at = table + 32 * number;
        let (kind, offset, virtual_address, size) =
            (u32_at(elf, at)?, u32_at(elf, at + 4)?, u32_at(elf, at + 8)?, u32_at(elf, at + 16)?);
        if kind == 1 && virtual_address as u64 <= address && address < virtual_address as u64 + size as u64 {
            return Some((offset as u64 + address - virtual_address as u64) as usize);
        }
    }
    None
}

pub struct MonsterRow {
    pub name: String,
    pub model: String,
    pub script: String,
}

/// The executable's monster table: each kind's name, model pack and AI script.
pub fn monster_table(elf: &[u8], layout: &Node) -> IndexMap<i64, MonsterRow> {
    let mut rows = IndexMap::new();
    let (Some(address), Some(size)) = (layout["address"].as_i64(), layout["record_size"].as_i64()) else {
        return rows;
    };
    let Some(mut position) = file_offset(elf, address as u64) else {
        return rows;
    };
    let size = size as usize;
    while position + size <= elf.len() {
        let record = &elf[position..position + size];
        let model = text_at(record, MONSTER_MODEL);
        if model.is_empty() {
            break;
        }
        let id = i16::from_le_bytes(record[MONSTER_ID..MONSTER_ID + 2].try_into().unwrap()) as i64;
        rows.entry(id).or_insert_with(|| MonsterRow {
            name: text_at(record, MONSTER_NAME),
            model,
            script: text_at(record, MONSTER_SCRIPT),
        });
        position += size;
    }
    rows
}

/// The game executable named by the definitions, read from the extracted disc.
pub fn executable(root: &Path, defs: &Definitions) -> Option<Vec<u8>> {
    let name = defs.executable.get("name").and_then(Node::as_str)?;
    fs::read(root.join(name)).ok()
}

/// The language's number for disc files (0 Japanese, 1 English, ...), from the definitions.
pub fn language_code(defs: &Definitions) -> Option<i64> {
    defs.text.get("languages")?.get(&defs.language)?.as_i64()
}

/// Where a monster kind's files are: its model pack and the AI script it runs, both under
/// dungeon/monster/. Kinds can share an AI script, and an event-only kind can have its own.
pub fn monster_note(row: &MonsterRow) -> String {
    format!("model {}, AI {}", row.model, if row.script.is_empty() { &row.model } else { &row.script })
}

/// The name tables: the language's (items, maps, characters, packs) and the monster kinds.
pub fn tables(root: &Path, defs: &Definitions) -> IndexMap<String, NameTable> {
    let mut found = match language_code(defs) {
        Some(code) => messages::disc_names(root, code),
        None => IndexMap::new(),
    };
    let layout = defs.executable.get("monster_table").filter(|layout| layout.is_set());
    let monsters = match (executable(root, defs), layout) {
        (Some(elf), Some(layout)) => monster_table(&elf, layout),
        _ => IndexMap::new(),
    };
    let named = || monsters.iter().filter(|(_, row)| !row.name.is_empty());
    found.insert(
        "monster_kind".into(),
        named().map(|(number, row)| (NameKey::Int(*number), row.name.clone())).collect(),
    );
    found.insert(
        "monster_kind_notes".into(),
        named().map(|(number, row)| (NameKey::Int(*number), monster_note(row))).collect(),
    );
    let packs = found.entry("chara_pack".into()).or_default();
    for (_, row) in named() {
        packs.entry(NameKey::Text(row.model.clone())).or_insert_with(|| row.name.clone());
    }
    found
}

/// A name as a constant: PREFIX_WORDS_IN_CAPITALS, accents folded away; None when nothing is
/// left.
pub fn identifier(text: &str, prefix: &str) -> Option<String> {
    let plain: String = text.nfkd().filter(char::is_ascii).collect::<String>().replace('\'', "");
    let words: Vec<String> =
        Regex::new("[A-Za-z0-9]+").unwrap().find_iter(&plain).map(|word| word.as_str().to_uppercase()).collect();
    if words.is_empty() {
        return None;
    }
    Some(std::iter::once(prefix.to_string()).chain(words).collect::<Vec<_>>().join("_"))
}

pub fn enum_lines(
    name: &str,
    prefix: &str,
    values: &NameTable,
    taken: &mut HashSet<String>,
    notes: Option<&NameTable>,
) -> Vec<String> {
    let mut lines = vec![format!("enum {name} {{")];
    let numbered: BTreeMap<i64, &String> = values
        .iter()
        .filter_map(|(key, text)| if let NameKey::Int(number) = key { Some((*number, text)) } else { None })
        .collect();
    for (number, text) in numbered {
        let Some(mut constant) = identifier(text, prefix) else {
            continue;
        };
        if taken.contains(&constant) {
            constant = format!("{constant}_{number}");
        }
        if taken.contains(&constant) {
            continue;
        }
        taken.insert(constant.clone());
        let note = notes.and_then(|notes| notes.get(&NameKey::Int(number))).filter(|note| !note.is_empty());
        lines.push(format!(
            "    {} = {},{}",
            constant,
            number,
            note.map(|note| format!("  // {note}")).unwrap_or_default()
        ));
    }
    lines.extend(["};".to_string(), String::new()]);
    lines
}

const ENUMS: [(&str, &str, &str); 3] = [
    ("monster_kind", "MONSTER", "Monster kinds, from the game executable's monster table."),
    ("item_id", "ITEM", "Items, from the item names in menu/cfg7/comdatmes<language>.cfg."),
    ("npc_id", "NPC", "Characters, from npc<language>.cfg."),
];

/// disc.h's text, or None when the disc gives no names.
pub fn header(found: &IndexMap<String, NameTable>, taken: &HashSet<String>) -> Option<String> {
    let mut lines: Vec<String> = [
        "// Names read from the disc this source was decompiled from. Generated by dc2tk; not part of",
        "// the repository, since the names are the game's own text.",
        "#ifndef DISC_H",
        "#define DISC_H",
        "",
    ]
    .iter()
    .map(|line| line.to_string())
    .collect();
    let mut used = taken.clone();
    let mut present = false;
    for (name, prefix, doc) in ENUMS {
        if let Some(values) = found.get(name).filter(|values| !values.is_empty()) {
            lines.push(format!("// {doc}"));
            lines.extend(enum_lines(name, prefix, values, &mut used, found.get(&format!("{name}_notes"))));
            present = true;
        }
    }
    lines.extend(["#endif".to_string(), String::new()]);
    present.then(|| lines.join("\n"))
}

/// The constants headers define (enum members and #defines), which disc.h's names must not take.
pub fn header_constants(headers: &IndexMap<String, String>) -> HashSet<String> {
    let constant = Regex::new(r"(?m)^\s*(?:#define\s+)?([A-Z][A-Z0-9_]*)\b").unwrap();
    headers.values().flat_map(|text| constant.captures_iter(text).map(|found| found[1].to_string())).collect()
}

/// The definitions and headers with disc.h added, and its text; unchanged when the disc gives no
/// names.
pub fn with_header(
    root: &Path,
    mut defs: Definitions,
    mut headers: IndexMap<String, String>,
) -> (Definitions, IndexMap<String, String>, Option<String>) {
    let taken = header_constants(&headers);
    let Some(text) = header(&tables(root, &defs), &taken) else {
        return (defs, headers, None);
    };
    if !defs.include.iter().any(|name| name == HEADER) {
        defs.include.push(HEADER.to_string());
    }
    headers.insert(HEADER.to_string(), text.clone());
    (defs, headers, Some(text))
}
