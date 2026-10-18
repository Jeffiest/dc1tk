//! Dark Cloud 1's text as upstream Chronicle's `lang/` files: one flat JSON per language
//! (`en_gb.json`, ...) of "<file key>.<id>": text, and `hashes/<language>.json` of the same keys with
//! each text's FNV-1a hash, which upstream's patches use. A port of `LocalizeExport`
//! (port/src/localize.cpp), so a dc1tk export is what `darkcloud --export-text` writes, without the
//! port's own `options.` strings (they are the port's, not the disc's).

use std::collections::BTreeMap;
use std::path::Path;

use crate::{text_dc1, Error, Result};

/// The language files, by the game's language number.
pub const LANGUAGE_FILES: [&str; 7] = ["ja_jp", "en_us", "en_gb", "fr_fr", "de_de", "it_it", "es_es"];
const LANGUAGE_DIRS: [&str; 7] = ["a_jpn", "a_usa", "a_eng", "a_fre", "a_ger", "a_ita", "a_spa"];
const END: i16 = -0xFF;
const PACK_ENTRY: usize = 0x50;
const PACK_NAME: usize = 0x40;

fn i16_at(data: &[u8], word: usize) -> i16 {
    i16::from_le_bytes([data[word * 2], data[word * 2 + 1]])
}

/// A retail message file's messages, by id (the first of an id wins), as text (ReadRetail and
/// LocalizeReadMessages). None when it is not a message file.
pub fn read_messages(data: &[u8]) -> Option<BTreeMap<i32, String>> {
    let words = data.len() / 2;
    if words < 2 || i16_at(data, 0) < 0 {
        return None;
    }
    let count = i16_at(data, 0) as usize;
    if 2 + count * 2 > words {
        return None;
    }
    let base = 1 + count as i64;
    let mut out = BTreeMap::new();
    for i in 0..count {
        let id = i16_at(data, 2 + i * 2) as i32;
        let start = base + i16_at(data, 3 + i * 2) as i64;
        if start < 0 || start as usize >= words {
            continue;
        }
        let mut codes = Vec::new();
        let mut at = start as usize;
        while at < words && i16_at(data, at) != END {
            codes.push(i16_at(data, at) as u16);
            at += 1;
        }
        if at >= words {
            continue;
        }
        out.entry(id).or_insert_with(|| text_dc1::decode(&codes));
    }
    Some(out)
}

/// The language a message file's path names: an a_xxx/ folder or a name ending _N.mes; -1 for none.
fn file_language(key: &str) -> i32 {
    for (code, dir) in LANGUAGE_DIRS.iter().enumerate() {
        if key.contains(&format!("{dir}/")) {
            return code as i32;
        }
    }
    let name = key.rsplit('/').next().unwrap_or(key).as_bytes();
    let n = name.len();
    if n >= 6 && name[n - 6] == b'_' && (b'0'..=b'6').contains(&name[n - 5]) && &name[n - 4..] == b".mes" {
        return (name[n - 5] - b'0') as i32;
    }
    -1
}

/// A message file's key: its path without the language folder, the .mes and the language number,
/// slashes made dots (LocalizeMessageKey).
pub fn message_key(path: &str) -> String {
    let mut key = path.to_string();
    for dir in LANGUAGE_DIRS {
        let folder = format!("{dir}/");
        if key.starts_with(&folder) {
            key.drain(..folder.len());
        } else if let Some(at) = key.find(&format!("/{folder}")) {
            key.replace_range(at + 1..at + 1 + folder.len(), "");
        }
    }
    if key.ends_with("opmes.bin") {
        key.truncate(key.len() - 4);
    }
    if key.ends_with(".mes") {
        key.truncate(key.len() - 4);
        let b = key.as_bytes();
        if b.len() >= 2 && b[b.len() - 2] == b'_' && (b'0'..=b'6').contains(&b[b.len() - 1]) {
            key.truncate(key.len() - 2);
        }
    }
    key.replace('/', ".")
}

/// Japanese text is codes from 0 up, written "{N}"; every other language's are negative.
fn is_japanese(messages: &BTreeMap<i32, String>) -> bool {
    messages.values().any(|text| text.as_bytes().windows(2).any(|w| w[0] == b'{' && w[1].is_ascii_digit()))
}

/// The FNV-1a hash of a text, as upstream's patches name it (LocalizeTextHash).
pub fn text_hash(text: &str) -> String {
    let mut hash: u64 = 0xCBF29CE484222325;
    for &b in text.as_bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001B3);
    }
    format!("{hash:016x}")
}

struct Found {
    key: String,
    messages: BTreeMap<i32, String>,
    language: i32,
    derived: bool,
}

/// Every message file under a data folder (the game's own, as the PC port reads it), and the
/// message entries of its .pak and .pac packs.
fn walk(root: &Path, folder: &Path, out: &mut Vec<Found>) -> Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(folder)
        .map_err(|e| Error::invalid(format!("cannot read {}: {e}", folder.display())))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            walk(root, &path, out)?;
            continue;
        }
        let key = path.strip_prefix(root).expect("below the root").to_string_lossy().replace('\\', "/").to_lowercase();
        let mes = key.ends_with(".mes");
        if !mes && !key.ends_with(".pak") && !key.ends_with(".pac") {
            continue;
        }
        let bytes = std::fs::read(&path).map_err(|e| Error::invalid(format!("cannot read {}: {e}", path.display())))?;
        if mes {
            add(&key, &bytes, out);
            continue;
        }
        let mut at = 0;
        while at + PACK_ENTRY <= bytes.len() && bytes[at] != 0 {
            let int = |o: usize| i32::from_le_bytes(bytes[at + o..at + o + 4].try_into().unwrap());
            let (offset, size, next) = (int(PACK_NAME), int(PACK_NAME + 4), int(PACK_NAME + 8));
            let name_end = bytes[at..at + PACK_NAME].iter().position(|&b| b == 0).unwrap_or(PACK_NAME);
            let name = String::from_utf8_lossy(&bytes[at..at + name_end]).to_lowercase();
            if offset < 0 || size < 0 || next <= 0 || at + offset as usize + size as usize > bytes.len() {
                break;
            }
            if name.ends_with(".mes") || name == "opmes.bin" {
                let start = at + offset as usize;
                add(&format!("{key}/{name}"), &bytes[start..start + size as usize], out);
            }
            at += next as usize;
        }
    }
    Ok(())
}

fn add(key: &str, data: &[u8], out: &mut Vec<Found>) {
    let Some(messages) = read_messages(data).filter(|m| !m.is_empty()) else {
        return;
    };
    let mut language = file_language(key);
    let mut derived = false;
    if language != 0 && is_japanese(&messages) {
        language = 0;
        derived = true;
    }
    out.push(Found { key: key.to_string(), messages, language, derived });
}

fn json_text(map: &BTreeMap<String, String>) -> String {
    let mut out = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
    let mut ser = serde_json::Serializer::with_formatter(&mut out, formatter);
    serde::Serialize::serialize(map, &mut ser).expect("strings serialize");
    String::from_utf8(out).expect("JSON is UTF-8") + "\n"
}

/// Writes `<language>.json` and `hashes/<language>.json` for every language into `out` from the data
/// folder `root` (folders starting `mods` are a port's mods, not the disc's, and are skipped). Gives
/// the number of message files read and the key collisions found.
pub fn export(root: &Path, out: &Path) -> Result<(usize, Vec<String>)> {
    let mut found = Vec::new();
    walk(root, root, &mut found)?;
    found.retain(|item| !item.key.starts_with("mods/"));
    std::fs::create_dir_all(out.join("hashes")).map_err(|e| Error::invalid(format!("cannot create {}: {e}", out.display())))?;
    let mut collisions = Vec::new();
    let rank = |item: &Found| if item.language == -1 { 0 } else if item.derived { 1 } else { 2 };
    for (code, file) in LANGUAGE_FILES.iter().enumerate() {
        let mut chosen: BTreeMap<String, &Found> = BTreeMap::new();
        for item in found.iter().filter(|item| item.language == code as i32 || item.language == -1) {
            let key = message_key(&item.key);
            match chosen.get(&key) {
                Some(was) if rank(item) <= rank(was) => {
                    if rank(item) == rank(was) && was.key != item.key {
                        collisions.push(format!("{} and {} are both \"{key}\"", was.key, item.key));
                    }
                }
                _ => {
                    chosen.insert(key, item);
                }
            }
        }
        let mut texts = BTreeMap::new();
        let mut hashes = BTreeMap::new();
        for (key, item) in &chosen {
            for (id, text) in &item.messages {
                texts.insert(format!("{key}.{id}"), text.clone());
                hashes.insert(format!("{key}.{id}"), text_hash(text));
            }
        }
        let write = |path: std::path::PathBuf, map: &BTreeMap<String, String>| {
            std::fs::write(&path, json_text(map)).map_err(|e| Error::invalid(format!("cannot write {}: {e}", path.display())))
        };
        write(out.join(format!("{file}.json")), &texts)?;
        write(out.join("hashes").join(format!("{file}.json")), &hashes)?;
    }
    Ok((found.len(), collisions))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_drop_language_folders_and_numbers() {
        assert_eq!(message_key("dun/script/d01/d01_3.mes"), "dun.script.d01.d01");
        assert_eq!(message_key("meswin/system14_2.mes"), "meswin.system14");
        assert_eq!(message_key("commenu/a_fre/dungeon/dunmenu5.pak/allmenu.mes"), "commenu.dungeon.dunmenu5.pak.allmenu");
        assert_eq!(message_key("noda_w/dun00_eng.mes"), "noda_w.dun00_eng");
        assert_eq!(file_language("commenu/a_fre/x.pak/allmenu.mes"), 3);
        assert_eq!(file_language("dun/script/d01/d01_3.mes"), 3);
        assert_eq!(file_language("noda_w/dun00_eng.mes"), -1);
    }
}
