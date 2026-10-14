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
    villagers: &Changing,
) -> Option<String> {
    let generated = generated?;
    if folder_names.is_empty() && villagers.header.is_empty() && villagers.loaded.is_empty() {
        return Some(generated);
    }
    let position = generated.rfind("#endif").expect("disc.h ends with #endif");
    Some(format!(
        "{}\n{}\n{}",
        generated[..position].trim_end_matches('\n'),
        header_text(folder_names) + &villager_header_text(villagers),
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

/// Villager slots per map: the villager the engine loads into a slot no script of the map's folder
/// loads itself (see villagers.rs): a constant where it is always the same villager, a note where
/// the chapter and time of day decide among several.
#[derive(Clone, Debug, Default)]
pub struct Changing {
    /// map code → slot → constant
    pub names: BTreeMap<String, BTreeMap<i64, String>>,
    /// map code → slot → the villagers it can hold
    pub notes: BTreeMap<String, BTreeMap<i64, String>>,
    /// script path → the map it runs on, for maps with villager slots
    pub maps: BTreeMap<String, String>,
    /// folder code → constant → slot, for disc.h
    pub header: BTreeMap<String, BTreeMap<String, i64>>,
    /// folder → (slot, pack) → constant: the characters a folder's scripts load into slots that
    /// hold others at other times, named where a load reaches a use (slotflow.rs)
    pub loaded: BTreeMap<String, BTreeMap<(i64, String), String>>,
}

/// The characters a folder's scripts load into a slot that holds others at other times:
/// {folder: {(slot, pack): constant}}, CHARA_<folder>_<character> (with the slot after it where the
/// name is another slot's).
pub fn loaded_names(
    found: &Survey,
    folder_names: &BTreeMap<String, BTreeMap<i64, String>>,
    pack_names: &NameTable,
) -> BTreeMap<String, BTreeMap<(i64, String), String>> {
    let mut out = BTreeMap::new();
    for (path, by_slot) in found {
        let code = folder_code(path);
        if code.is_empty() {
            continue;
        }
        let constant = folder_names.get(path);
        let mut taken: std::collections::HashMap<String, i64> =
            constant.into_iter().flatten().map(|(slot, name)| (name.clone(), *slot)).collect();
        let mut chosen = BTreeMap::new();
        for (slot, packs) in by_slot {
            if ENGINE_SLOTS.contains(slot) || constant.is_some_and(|constant| constant.contains_key(slot)) {
                continue;
            }
            for pack in packs {
                let Some(character) = character_name(pack, pack_names) else {
                    continue;
                };
                let mut name = format!("CHARA_{code}_{character}");
                if taken.get(&name).is_some_and(|other| other != slot) {
                    name = format!("{name}_{slot}");
                }
                if taken.get(&name).is_some_and(|other| other != slot) {
                    continue;
                }
                taken.insert(name.clone(), *slot);
                chosen.insert((*slot, pack.clone()), name);
            }
        }
        if !chosen.is_empty() {
            out.insert(path.clone(), chosen);
        }
    }
    out
}

/// The villager slots of the maps the scripts run on.
pub fn villagers(
    found: &Survey,
    folder_names: &BTreeMap<String, BTreeMap<i64, String>>,
    tables: Option<&crate::villagers::Tables>,
    names: &IndexMap<String, NameTable>,
    paths: &[String],
) -> Changing {
    let empty = NameTable::new();
    let npc_names = names.get("npc_id").unwrap_or(&empty);
    let loaded = loaded_names(found, folder_names, names.get("chara_pack").unwrap_or(&empty));
    let mut out = Changing { loaded, ..Changing::default() };
    let Some(tables) = tables else {
        return out;
    };
    let by_map = tables.slots();
    let mut folders: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for path in paths {
        if let Some(map) = tables.map_of(path).filter(|map| by_map.contains_key(map)) {
            out.maps.insert(path.clone(), map.clone());
            folders.entry(map).or_default().insert(folder(path));
        }
    }
    let mut taken: HashSet<String> = folder_names
        .values()
        .flat_map(|chosen| chosen.values().cloned())
        .chain(out.loaded.values().flat_map(|chosen| chosen.values().cloned()))
        .collect();
    for (map, map_folders) in &folders {
        let Some(code) = map_folders.iter().map(|path| folder_code(path)).find(|code| !code.is_empty()) else {
            continue;
        };
        for (slot, villagers) in &by_map[map] {
            // a slot the folder's scripts load with characters of their own is not the villager's
            let loaded = map_folders.iter().any(|path| {
                found.get(path).is_some_and(|slots| slots.contains_key(slot))
                    || folder_names.get(path).is_some_and(|chosen| chosen.contains_key(slot))
            });
            if loaded {
                continue;
            }
            let named: Vec<String> =
                villagers.iter().map(|villager| crate::villagers::villager_name(*villager, npc_names)).collect();
            if let [only] = &named[..] {
                let mut name = format!("CHARA_{code}_{}", identifier(only));
                let header = out.header.entry(code.clone()).or_default();
                if taken.contains(&name) && header.get(&name) != Some(slot) {
                    name = format!("{name}_{slot}");
                }
                taken.insert(name.clone());
                header.insert(name.clone(), *slot);
                out.names.entry(map.clone()).or_default().insert(*slot, name);
            } else {
                let listed = match &named[..] {
                    [rest @ .., last] => format!("{} or {last}", rest.join(", ")),
                    [] => continue,
                };
                out.notes
                    .entry(map.clone())
                    .or_default()
                    .insert(*slot, format!("villager slot {slot}: {listed}, by chapter and time of day"));
            }
        }
    }
    out
}

/// disc.h's enums of the villager slots that always hold one villager.
pub fn villager_header_text(villagers: &Changing) -> String {
    let mut lines = Vec::new();
    let mut enums = |doc: &str, kind: &str, by_code: BTreeMap<String, BTreeSet<(i64, String)>>| {
        if by_code.is_empty() {
            return;
        }
        lines.extend([String::new(), doc.to_string()]);
        for (code, names) in by_code {
            lines.push(format!("enum {kind}_slots_{} {{", code.to_lowercase()));
            lines.extend(names.into_iter().map(|(slot, name)| format!("    {name} = {slot},")));
            lines.push("};".into());
        }
    };
    enums(
        "// Characters a folder's scripts load into a slot that holds others at other times, named where\n\
         // a load reaches the code that uses the slot.",
        "loaded",
        villagers
            .loaded
            .iter()
            .map(|(path, chosen)| {
                (folder_code(path), chosen.iter().map(|((slot, _), name)| (*slot, name.clone())).collect())
            })
            .collect(),
    );
    enums(
        "// Villager slots: the villager the engine always loads into a slot of a map (npc_place4.cfg).",
        "villager",
        villagers
            .header
            .iter()
            .map(|(code, names)| (code.clone(), names.iter().map(|(name, slot)| (*slot, name.clone())).collect()))
            .collect(),
    );
    if lines.is_empty() {
        return String::new();
    }
    lines.join("\n") + "\n"
}

/// The context a script (or a library its map's scripts alone include) is presented with on a
/// map: the map's villager slot names are the kind's.
pub fn on_map(context: Option<Context>, map: Option<&str>, villagers: &Changing) -> Option<Context> {
    let mut context = context?;
    if let Some(chosen) = map.and_then(|map| villagers.names.get(map)).filter(|chosen| !chosen.is_empty()) {
        let members: IndexMap<i64, String> = chosen.iter().map(|(slot, name)| (*slot, name.clone())).collect();
        context.enums.insert("chara_slot_villager".into(), members);
    }
    Some(context)
}

/// The context of a script in a folder (or a library only that folder's scripts include): the
/// characters its loads put in slots, for slotflow.
pub fn with_loaded(context: Option<Context>, folder: &str, villagers: &Changing) -> Option<Context> {
    let mut context = context?;
    if let Some(chosen) = villagers.loaded.get(folder) {
        context.slot_packs = chosen.iter().map(|(key, name)| (key.clone(), name.clone())).collect();
    }
    Some(context)
}

/// Notes on the natives that address a villager slot several villagers can hold.
pub fn villager_notes(nodes: &[Node], found: &Context, notes: &BTreeMap<i64, String>) -> Vec<Node> {
    nodes
        .iter()
        .map(|node| match node.kind() {
            "if" => {
                let otherwise = match node["else"].as_array() {
                    Some(items) => Node::List(villager_notes(items, found, notes)),
                    None => Node::Null,
                };
                node.clone()
                    .with("then", Node::List(villager_notes(list(node, "then"), found, notes)))
                    .with("else", otherwise)
            }
            "while" => node.clone().with("body", Node::List(villager_notes(list(node, "body"), found, notes))),
            "native" => {
                let Some(declared) = found.natives.get(node["name"].as_str().unwrap_or("")) else {
                    return node.clone();
                };
                let params = list(declared, "params");
                let mut added: Vec<&String> = Vec::new();
                for (position, arg) in list(node, "args").iter().enumerate() {
                    let slot = params.get(position).filter(|param| param["kind"].as_str() == Some("chara_slot"));
                    if let (Some(_), Some(note)) = (
                        slot,
                        arg["value"].as_i64().filter(|_| arg.kind() == "int").and_then(|value| notes.get(&value)),
                    ) {
                        if !added.contains(&note) {
                            added.push(note);
                        }
                    }
                }
                if added.is_empty() {
                    return node.clone();
                }
                let mut comment: Vec<String> = added.into_iter().cloned().collect();
                if let Some(existing) = node["comment"].as_str().filter(|existing| !existing.is_empty()) {
                    comment.insert(0, existing.to_string());
                }
                node.clone().with("comment", comment.join("; "))
            }
            _ => node.clone(),
        })
        .collect()
}
