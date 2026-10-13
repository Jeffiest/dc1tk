//! Character slots a map folder's scripts always fill with the same character, so the slot can be
//! named for that folder. The engine fills a few slots itself (enum chara_slot_engine); villagers
//! and the rest are loaded by the scripts with _LOAD_CHARA and _LOAD_MOTION (load_chara, and
//! helpers such as load_scene_chara and add_chara_motions), whose second argument is the character
//! pack and whose fourth is the slot. A slot loaded with different packs in one folder holds
//! different characters from scene to scene and keeps its number. See
//! definitions/pal/notes/characters.md.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::OnceLock;

use indexmap::IndexMap;
use regex::Regex;

use crate::messages::{NameKey, NameTable};
use crate::node::Node;
use crate::sdk::Context;

const LOADS: [&str; 2] = ["_LOAD_CHARA", "_LOAD_MOTION"];
const ENGINE_SLOTS: [i64; 5] = [0, 120, 121, 122, 123];

fn list<'a>(node: &'a Node, key: &str) -> &'a [Node] {
    node[key].as_array().map(Vec::as_slice).unwrap_or(&[])
}

fn loads(nodes: &[Node], out: &mut Vec<(i64, String)>) {
    for node in nodes {
        let args = list(node, "args");
        if node.kind() == "native" && LOADS.contains(&node["name"].as_str().unwrap_or("")) && args.len() > 3 {
            let (pack, slot) = (&args[1], &args[3]);
            if pack.kind() == "string" && slot.kind() == "int" {
                if let (Some(value), Some(text)) = (slot["value"].as_i64(), pack["text"].as_str()) {
                    out.push((value, text.to_string()));
                }
            }
        }
        for key in ["then", "else", "body"] {
            if let Some(items) = node.get(key).and_then(Node::as_array) {
                loads(items, out);
            }
        }
    }
}

/// A path's folder (PurePosixPath(path).parent).
pub fn folder(path: &str) -> String {
    match path.rfind('/') {
        Some(at) if at > 0 => path[..at].to_string(),
        Some(_) => "/".to_string(),
        None => ".".to_string(),
    }
}

/// {folder: {slot: {pack, ...}}}: the packs each folder's scripts load into each slot.
pub type Survey = BTreeMap<String, BTreeMap<i64, BTreeSet<String>>>;

/// Adds a program's literal pack loads to the survey.
pub fn survey(found: &mut Survey, path: &str, program: &Node) {
    if program.get("asm").is_some() {
        return;
    }
    for function in list(program, "functions").iter().filter(|function| function.kind() == "function") {
        let mut loaded = Vec::new();
        loads(list(function, "body"), &mut loaded);
        for (slot, pack) in loaded {
            found.entry(folder(path)).or_default().entry(slot).or_default().insert(pack);
        }
    }
}

fn identifier(text: &str) -> String {
    static CELL: OnceLock<Regex> = OnceLock::new();
    let separators = CELL.get_or_init(|| Regex::new("[^A-Z0-9]+").unwrap());
    separators.replace_all(&text.to_uppercase(), "_").trim_matches('_').to_string()
}

/// A character's name for its pack: the villager the disc's NPC table names for it when there is
/// exactly one, Max or Monica for their packs, otherwise the pack's file name without its event
/// prefix (e109_starbre is starbre). None for a villager model shared by several villagers.
fn character_name(pack: &str, pack_names: &NameTable) -> Option<String> {
    static EVENT_PREFIX: OnceLock<Regex> = OnceLock::new();
    static VILLAGER_MODEL: OnceLock<Regex> = OnceLock::new();
    if let Some(known) = pack_names.get(&NameKey::Text(pack.to_string())).filter(|known| !known.is_empty()) {
        if !known.contains(" or ") {
            return Some(identifier(known));
        }
    }
    let path = pack.replace('\\', "/");
    let name = path.rsplit('/').next().unwrap_or("");
    let stem = name.split('.').next().unwrap_or("");
    let stem = EVENT_PREFIX.get_or_init(|| Regex::new(r"^e\d+[a-z]?_").unwrap()).replace(stem, "").into_owned();
    if VILLAGER_MODEL.get_or_init(|| Regex::new(r"^p\d+_\d+[a-z]$").unwrap()).is_match(&stem) {
        return None;
    }
    let (head, rest) = stem.split_once('_').unwrap_or((stem.as_str(), ""));
    let player = match head {
        "c01" => Some("MAX"),
        "c02" => Some("MONICA"),
        _ => None,
    };
    if let Some(player) = player {
        let suffix = if !rest.is_empty() && rest != "base" { format!("_{}", identifier(rest)) } else { String::new() };
        return Some(format!("{player}{suffix}"));
    }
    let named = identifier(&stem);
    (!named.is_empty()).then_some(named)
}

fn folder_code(path: &str) -> String {
    let mut parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty() && *part != ".").collect();
    if parts.first() == Some(&"map") {
        parts.remove(0);
    }
    if parts.len() > 1 && parts[0].chars().count() == 1 {
        parts.remove(0);
    }
    identifier(&parts.join("_"))
}

/// {folder: {slot: constant}} for the slots each folder always fills with one pack.
pub fn names(found: &Survey, pack_names: &NameTable) -> BTreeMap<String, BTreeMap<i64, String>> {
    let mut out = BTreeMap::new();
    for (path, by_slot) in found {
        let code = folder_code(path);
        if code.is_empty() {
            continue;
        }
        let mut chosen = BTreeMap::new();
        let mut used = HashSet::new();
        for (slot, packs) in by_slot {
            // packs of one character from different events (e109_starbre, e110_starbre) agree
            let characters: HashSet<Option<String>> =
                packs.iter().map(|pack| character_name(pack, pack_names)).collect();
            if characters.len() != 1 || ENGINE_SLOTS.contains(slot) {
                continue;
            }
            let Some(character) = characters.into_iter().next().unwrap() else {
                continue;
            };
            let mut name = format!("CHARA_{code}_{character}");
            if used.contains(&name) {
                name = format!("{name}_{slot}");
            }
            used.insert(name.clone());
            chosen.insert(*slot, name);
        }
        if !chosen.is_empty() {
            out.insert(path.clone(), chosen);
        }
    }
    out
}

/// The folders' slot names as enums for disc.h, so the scripts that use them compile.
pub fn header_text(folder_names: &BTreeMap<String, BTreeMap<i64, String>>) -> String {
    let mut lines = vec![
        String::new(),
        "// Character slots each map folder's scripts always load with the same character.".to_string(),
    ];
    for (path, chosen) in folder_names {
        lines.push(format!("enum folder_slots_{} {{", folder_code(path).to_lowercase()));
        lines.extend(chosen.iter().map(|(slot, name)| format!("    {name} = {slot},")));
        lines.push("};".into());
    }
    lines.join("\n") + "\n"
}

pub fn with_header(
    generated: Option<String>,
    folder_names: &BTreeMap<String, BTreeMap<i64, String>>,
) -> Option<String> {
    let generated = generated?;
    if folder_names.is_empty() {
        return Some(generated);
    }
    let position = generated.rfind("#endif").expect("disc.h ends with #endif");
    Some(format!(
        "{}\n{}\n{}",
        generated[..position].trim_end_matches('\n'),
        header_text(folder_names),
        &generated[position..]
    ))
}

/// The context a script in the folder is presented with: its folder's slot names are the kind's.
pub fn in_folder(
    context: Option<Context>,
    path: &str,
    folder_names: &BTreeMap<String, BTreeMap<i64, String>>,
) -> Option<Context> {
    let mut context = context?;
    if let Some(chosen) = folder_names.get(&folder(path)).filter(|chosen| !chosen.is_empty()) {
        let members: IndexMap<i64, String> = chosen.iter().map(|(slot, name)| (*slot, name.clone())).collect();
        context.enums.insert("chara_slot_folder".into(), members);
    }
    Some(context)
}
