//! The game's text in comments: a script's event messages (the message files it loads, quoted
//! where a call shows a message by number) and the names of items, maps, characters, character
//! packs and monster kinds where a call passes their number, all read from the disc in the
//! definition set's language.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use indexmap::IndexMap;
use regex::Regex;

use crate::node::Node;
use crate::sdk::Context;

pub const MESSAGE_KINDS: [&str; 1] = ["message"];
pub const NAME_KINDS: [&str; 5] = ["item_id", "map_id", "npc_id", "chara_pack", "monster_kind"];

/// A name table's key: a number, or a text (a map's short name, a pack's file name).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NameKey {
    Int(i64),
    Text(String),
}

pub type NameTable = IndexMap<NameKey, String>;

fn record_body(data: &[u8], start: usize) -> &[u8] {
    let mut end = start;
    while end < data.len() && data[end] != 0 {
        if data[end] == 0x0A && data.get(end + 1) == Some(&b'@') {
            break;
        }
        end += 1;
    }
    let body = &data[start.min(end)..end];
    body.strip_suffix(b"\r").unwrap_or(body)
}

fn without_comments(body: &[u8]) -> Vec<u8> {
    let text = replace_bytes(body, b"\r\n", b"\n");
    // a line's leading ASCII whitespace, vertical tab and form feed included
    let indent = |line: &[u8]| -> usize { line.iter().take_while(|b| b" \t\n\r\x0b\x0c".contains(b)).count() };
    let kept: Vec<&[u8]> =
        text.split(|&b| b == b'\n').filter(|line| !line[indent(line)..].starts_with(b"//")).collect();
    replace_bytes(&kept.join(&b'\n'), b"\\n", b"\n")
}

fn replace_bytes(data: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut position = 0;
    while position < data.len() {
        if data[position..].starts_with(from) {
            out.extend_from_slice(to);
            position += from.len();
        } else {
            out.push(data[position]);
            position += 1;
        }
    }
    out
}

/// Text in the language's encoding: cp932 for Japanese and English, Latin-1 (with the game's own
/// œ/Œ) for the others.
pub fn decoded(body: &[u8], language: i64) -> String {
    if language == 0 || language == 1 {
        return dc2tk_disc::text::decode_lossy(body);
    }
    body.iter()
        .map(|&byte| match byte {
            0xBD => 'Œ',
            0xBE | 0x9C => 'œ',
            byte => byte as char,
        })
        .collect()
}

/// Text with the game's [UNIxxxx] escapes as their characters (a few remapped as the game draws
/// them).
pub fn expanded(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("[UNI") {
        let digits = rest[at + 4..].get(..4).filter(|digits| digits.bytes().all(|b| b.is_ascii_hexdigit()));
        match digits {
            Some(digits) if rest[at + 8..].starts_with(']') => {
                out.push_str(&rest[..at]);
                let code = digits.to_lowercase();
                let code = match code.as_str() {
                    "011d" => "00e8",
                    "0129" => "00ea",
                    "0155" => "00e0",
                    "0171" => "00fb",
                    "017f" => "00f9",
                    other => other,
                };
                if let Some(c) = char::from_u32(u32::from_str_radix(code, 16).unwrap()) {
                    out.push(c);
                }
                rest = &rest[at + 9..];
            }
            _ => {
                out.push_str(&rest[..at + 1]);
                rest = &rest[at + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// A message file's messages, by number (the first of a number wins).
pub fn read_messages(path: &Path, language: i64) -> IndexMap<i64, String> {
    let Ok(data) = fs::read(path) else {
        return IndexMap::new();
    };
    let mut found = IndexMap::new();
    // each record number: @ and an integer (@12, @-1), anywhere in the file
    let mut position = 0;
    while let Some(at) = data[position..].iter().position(|&b| b == b'@').map(|at| position + at) {
        let sign = usize::from(data.get(at + 1) == Some(&b'-'));
        let digits = data[at + 1 + sign..].iter().take_while(|b| b.is_ascii_digit()).count();
        if digits == 0 {
            position = at + 1;
            continue;
        }
        let end = at + 1 + sign + digits;
        position = end;
        let Some(number) = std::str::from_utf8(&data[at + 1..end]).ok().and_then(|text| text.parse::<i64>().ok())
        else {
            continue;
        };
        let Some(newline) = data[end..].iter().position(|&b| b == b'\n').map(|at| end + at) else {
            continue;
        };
        if found.contains_key(&number) {
            continue;
        }
        let text = expanded(&decoded(&without_comments(record_body(&data, newline + 1)), language));
        found.insert(number, text.trim_matches('\n').to_string());
    }
    found
}

/// A message file's name for a language: e50_1.txt → e50_<code>.txt.
pub fn localized(path: &str, code: i64) -> String {
    let bytes = path.as_bytes();
    if path.ends_with(".txt")
        && bytes.len() >= 6
        && bytes[bytes.len() - 6] == b'_'
        && bytes[bytes.len() - 5].is_ascii_digit()
    {
        return format!("{}_{code}.txt", &path[..path.len() - 6]);
    }
    if let Some(stem) = path.strip_suffix(".txt") {
        return format!("{stem}_{code}.txt");
    }
    path.to_string()
}

fn kind_positions(declared: &Node, kinds: &[&str]) -> BTreeSet<usize> {
    declared
        .list("params")
        .iter()
        .enumerate()
        .filter(|(_, param)| param.get("kind").and_then(Node::as_str).is_some_and(|kind| kinds.contains(&kind)))
        .map(|(position, _)| position)
        .collect()
}

/// Every statement, nested ones after the statement holding them.
pub fn statements_in<'a>(nodes: &'a [Node], out: &mut Vec<&'a Node>) {
    for node in nodes {
        out.push(node);
        match node.kind() {
            "if" => {
                statements_in(node.list("then"), out);
                statements_in(node.list("else"), out);
            }
            "while" => statements_in(node.list("body"), out),
            _ => {}
        }
    }
}

/// The call a statement makes: ("native" or "call", name, arguments).
pub fn calls_in(node: &Node) -> Option<(&'static str, &str, &[Node])> {
    if node.kind() == "native" {
        return Some(("native", node["name"].as_str().unwrap_or(""), node.list("args")));
    }
    let mut expression = node.get("expr").filter(|expression| !expression.is_null())?;
    if expression.kind() == "assign" {
        expression = &expression["value"];
    }
    if expression.kind() == "call" {
        return Some(("call", expression["name"].as_str().unwrap_or(""), expression.list("args")));
    }
    None
}

/// Each function's parameters that reach a parameter of the given kinds, by position.
pub type ParamsByFunction = HashMap<String, BTreeSet<usize>>;

fn callee_positions(
    kind: &str,
    name: &str,
    found: &Context,
    params_by_function: &ParamsByFunction,
    kinds: &[&str],
) -> BTreeSet<usize> {
    if kind == "native" {
        return found.by_original.get(name).map(|declared| kind_positions(declared, kinds)).unwrap_or_default();
    }
    params_by_function.get(name).cloned().unwrap_or_default()
}

fn param_names(function: &Node) -> Vec<String> {
    function.list("params").iter().map(|param| param["name"].as_str().unwrap_or("").to_string()).collect()
}

fn string_arg(args: &[Node], positions: &BTreeSet<usize>) -> Option<String> {
    positions.iter().find_map(|&position| {
        let arg = args.get(position)?;
        (arg.kind() == "string").then(|| arg["text"].as_str().unwrap_or("").to_string())
    })
}

/// The message files the functions load, with the directory the calls before set.
pub fn loaded_files(functions: &[Node], found: &Context) -> Vec<String> {
    let mut files: Vec<String> = Vec::new();
    for function in functions.iter().filter(|function| function.kind() == "function") {
        let mut directory = String::new();
        let mut statements = Vec::new();
        statements_in(function.list("body"), &mut statements);
        for node in statements {
            if node.kind() != "native" {
                continue;
            }
            let Some(declared) = found.by_original.get(node["name"].as_str().unwrap_or("")) else {
                continue;
            };
            let args = node.list("args");
            let directories = kind_positions(declared, &["directory"]);
            let changed = string_arg(args, &directories);
            if changed.is_some() || (!directories.is_empty() && args.is_empty()) {
                directory = changed.unwrap_or_default();
            }
            if let Some(loaded) = string_arg(args, &kind_positions(declared, &["message_file"])) {
                let file = format!("{directory}{loaded}");
                if !files.contains(&file) {
                    files.push(file);
                }
            }
        }
    }
    files
}

/// The messages of the files, in the language, the first of a number winning.
pub fn texts_for(
    files: &[String],
    root: &Path,
    code: i64,
    cache: &mut HashMap<String, IndexMap<i64, String>>,
) -> IndexMap<i64, String> {
    let mut found = IndexMap::new();
    for file in files {
        let path = localized(file, code);
        let messages = cache.entry(path.clone()).or_insert_with(|| read_messages(&root.join(&path), code));
        for (number, text) in messages.iter() {
            found.entry(*number).or_insert_with(|| text.clone());
        }
    }
    found
}

fn quoted(text: &str) -> String {
    let replaced = text.replace("<page>", "\n");
    let parts: Vec<&str> = replaced.split('\n').map(str::trim).filter(|part| !part.is_empty()).collect();
    format!("\"{}\"", parts.join(" "))
}

/// The names the disc's tables give items, maps, characters and character packs.
pub fn disc_names(root: &Path, code: i64) -> IndexMap<String, NameTable> {
    let mut found: IndexMap<String, NameTable> =
        ["item_id", "map_id", "npc_id", "chara_pack"].iter().map(|kind| (kind.to_string(), NameTable::new())).collect();
    let read = |path: &Path| fs::read(path).ok().map(|data| expanded(&decoded(&data, code)));
    if let Some(text) = read(&root.join(format!("menu/cfg7/comdatmes{code}.cfg"))) {
        static MES_SYS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"MES_SYS\s+(\d+)\s*,\s*"([^"]*)""#).unwrap());
        for capture in MES_SYS.captures_iter(&text) {
            if let Ok(number) = capture[1].parse::<i64>() {
                found["item_id"].entry(NameKey::Int(number)).or_insert_with(|| capture[2].to_string());
            }
        }
    }
    let table = |path: String| fs::read(root.join(path)).ok();
    if let Some(cfg) = table(crate::startat::map_table_path(Some(code))) {
        for row in crate::maptable::rows(&cfg).into_iter().filter(|row| !row.code.is_empty()) {
            let name = expanded(&decoded(&row.title, code));
            let names = &mut found["map_id"];
            names.entry(NameKey::Int(row.number)).or_insert_with(|| name.clone());
            names.entry(NameKey::Text(row.code)).or_insert(name);
        }
    }
    if let Some(cfg) = table(format!("npc{code}.cfg")) {
        for npc in crate::villagers::npc_info(&cfg) {
            let (name, pack) = (expanded(&decoded(&npc.name, code)), NameKey::Text(npc.pack));
            if let Some(number) = npc.number {
                found["npc_id"].entry(NameKey::Int(number)).or_insert_with(|| name.clone());
            }
            let packs = &mut found["chara_pack"];
            match packs.get(&pack).cloned() {
                None => {
                    packs.insert(pack, name);
                }
                Some(known) if !known.split(" or ").any(|part| part == name) => {
                    packs.insert(pack, format!("{known} or {name}"));
                }
                _ => {}
            }
        }
    }
    found
}

fn pack_name(text: &str) -> &str {
    let file = text.rsplit('/').next().unwrap_or(text);
    file.rsplit_once('.').map_or(file, |(stem, _)| stem)
}

fn looked_up<'a>(arg: &Node, table: &'a NameTable) -> Option<&'a String> {
    match arg.kind() {
        "int" => table.get(&NameKey::Int(arg["value"].as_i64()?)),
        "string" => {
            let text = arg["text"].as_str()?;
            table
                .get(&NameKey::Text(text.to_string()))
                .filter(|found| !found.is_empty())
                .or_else(|| table.get(&NameKey::Text(pack_name(text).to_string())))
        }
        _ => None,
    }
}

fn named_by_enum(arg: &Node, kinds: &[&str], found: &Context) -> bool {
    if arg.kind() != "int" {
        return false;
    }
    let Some(value) = arg["value"].as_i64() else {
        return false;
    };
    found.enums.iter().any(|(name, members)| {
        members.contains_key(&value) && kinds.iter().any(|kind| name == kind || name.starts_with(&format!("{kind}_")))
    })
}

/// One lookup of the notes: the kinds of parameter it reads, which functions pass them on, the
/// table, and whether its text is quoted (messages are).
pub struct Lookup<'a> {
    pub kinds: Vec<&'static str>,
    pub params_by_function: ParamsByFunction,
    pub table: &'a NameTable,
    pub quote: bool,
}

fn statement_notes(node: &Node, found: &Context, lookups: &[Lookup]) -> String {
    let mut lines = Vec::new();
    if let Some((kind, name, args)) = calls_in(node) {
        for lookup in lookups {
            for position in callee_positions(kind, name, found, &lookup.params_by_function, &lookup.kinds) {
                if position < args.len() && named_by_enum(&args[position], &lookup.kinds, found) {
                    continue;
                }
                if let Some(text) = args.get(position).and_then(|arg| looked_up(arg, lookup.table)) {
                    if !text.is_empty() {
                        lines.push(if lookup.quote { quoted(text) } else { text.clone() });
                    }
                }
            }
        }
    }
    lines.join("; ")
}

fn annotated(nodes: &[Node], found: &Context, lookups: &[Lookup]) -> Node {
    nodes
        .iter()
        .map(|node| match node.kind() {
            "if" => {
                let otherwise = match node["else"].as_array() {
                    Some(items) => annotated(items, found, lookups),
                    None => Node::Null,
                };
                node.clone().with("then", annotated(node.list("then"), found, lookups)).with("else", otherwise)
            }
            "while" => node.clone().with("body", annotated(node.list("body"), found, lookups)),
            _ => {
                let comment = statement_notes(node, found, lookups);
                if comment.is_empty() {
                    return node.clone();
                }
                let old = node.get("comment").filter(|old| old.is_set()).map(Node::plain_text);
                let joined: Vec<String> =
                    [old, Some(comment)].into_iter().flatten().filter(|part| !part.is_empty()).collect();
                node.clone().with("comment", joined.join("; "))
            }
        })
        .collect()
}

/// The declared kind (or enum) of each of a native's parameters that is not an out parameter.
fn declared_kinds(declared: &Node) -> IndexMap<usize, String> {
    declared
        .list("params")
        .iter()
        .enumerate()
        .filter_map(|(position, param)| {
            let kind = param
                .get("kind")
                .filter(|kind| kind.is_set())
                .or_else(|| param.get("enum").filter(|kind| kind.is_set()))?;
            (!param["out"].is_set()).then(|| (position, kind.plain_text()))
        })
        .collect()
}

/// The kind each function's parameters carry to the natives (and functions) they reach.
pub fn param_kinds(functions: &[Node], found: &Context) -> HashMap<String, IndexMap<usize, String>> {
    let mut result: HashMap<String, IndexMap<usize, String>> = HashMap::new();
    let mut changed = true;
    while changed {
        changed = false;
        for function in functions.iter().filter(|function| function.kind() == "function") {
            let params = param_names(function);
            let name = function["name"].as_str().unwrap_or("").to_string();
            let mut known = result.get(&name).cloned().unwrap_or_default();
            let mut statements = Vec::new();
            statements_in(function.list("body"), &mut statements);
            for node in statements {
                let Some((kind, callee, args)) = calls_in(node) else {
                    continue;
                };
                let declared = if kind == "native" { found.by_original.get(callee) } else { None };
                let callee_kinds = match declared {
                    Some(declared) => declared_kinds(declared),
                    None => result.get(callee).cloned().unwrap_or_default(),
                };
                for (position, value_kind) in callee_kinds {
                    let Some(arg) = args.get(position) else {
                        continue;
                    };
                    if arg.kind() == "var" {
                        if let Some(index) = params.iter().position(|param| arg["name"] == param.as_str()) {
                            if !known.contains_key(&index) {
                                known.insert(index, value_kind);
                                changed = true;
                            }
                        }
                    }
                }
            }
            if !known.is_empty() {
                result.insert(name, known);
            }
        }
    }
    result
}

/// The functions' parameters by the kind they carry: {kind (messages as "message"): {function:
/// positions}}.
pub fn kind_params(functions: &[Node], found: &Context) -> HashMap<String, ParamsByFunction> {
    let mut params: HashMap<String, ParamsByFunction> = HashMap::new();
    for (name, positions) in param_kinds(functions, found) {
        for (position, kind) in positions {
            let group = if MESSAGE_KINDS.contains(&kind.as_str()) { "message".to_string() } else { kind };
            params.entry(group).or_default().entry(name.clone()).or_default().insert(position);
        }
    }
    params
}

/// The lookups for the messages and each kind of name.
pub fn lookups_for<'a>(
    params_by_kind: &HashMap<String, ParamsByFunction>,
    texts: &'a NameTable,
    names: &'a IndexMap<String, NameTable>,
    empty: &'a NameTable,
) -> Vec<Lookup<'a>> {
    let mut found = vec![Lookup {
        kinds: MESSAGE_KINDS.to_vec(),
        params_by_function: params_by_kind.get("message").cloned().unwrap_or_default(),
        table: texts,
        quote: true,
    }];
    for kind in NAME_KINDS {
        found.push(Lookup {
            kinds: vec![kind],
            params_by_function: params_by_kind.get(kind).cloned().unwrap_or_default(),
            table: names.get(kind).unwrap_or(empty),
            quote: false,
        });
    }
    found
}

/// The functions with message and name comments on the statements that pass them.
pub fn annotate_functions(
    functions: Vec<Node>,
    found: &Context,
    params_by_kind: &HashMap<String, ParamsByFunction>,
    texts: &IndexMap<i64, String>,
    names: &IndexMap<String, NameTable>,
) -> Vec<Node> {
    if texts.is_empty() && !names.values().any(|table| !table.is_empty()) {
        return functions;
    }
    let texts: NameTable = texts.iter().map(|(number, text)| (NameKey::Int(*number), text.clone())).collect();
    let empty = NameTable::new();
    let lookups = lookups_for(params_by_kind, &texts, names, &empty);
    functions
        .into_iter()
        .map(|function| {
            if function.kind() == "function" {
                let body = annotated(function.list("body"), found, &lookups);
                function.with("body", body)
            } else {
                function
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir().join(format!("dc2tk-messages-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn a_message_file_is_read_by_record_number() {
        let folder = temp("records");
        let path = folder.join("e50_1.txt");
        let text = b"@1\r\nHello[UNI00E9]\r\n@2\n// a note for the translator\nOne\\nTwo\n@1\nAgain\n@-3\nneg\x00ignored\n@x\n@4";
        fs::write(&path, text).unwrap();
        let found = read_messages(&path, 1);
        let found: Vec<(i64, &str)> = found.iter().map(|(number, text)| (*number, text.as_str())).collect();
        // the first of a number wins; a record ends at the next @ line or a NUL; comment lines go
        // and \n is a line break; a number with no line after it has no record
        assert_eq!(found, [(1, "Hello\u{e9}"), (2, "One\nTwo"), (-3, "neg")]);
        assert!(read_messages(&folder.join("missing.txt"), 1).is_empty());
        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn text_is_decoded_in_its_languages_encoding() {
        assert_eq!(decoded(b"c\x9cur \xbd", 2), "c\u{153}ur \u{152}");
        assert_eq!(decoded(b"caf\xe9", 3), "caf\u{e9}");
        assert_eq!(decoded(b"\x83p\x81[\x83\x8b", 0), "\u{30d1}\u{30fc}\u{30eb}");
        // the escapes the game draws otherwise, and one that is no escape
        assert_eq!(expanded("[UNI011D]t[UNI00e9] [UNIZZ] [UNI00"), "\u{e8}t\u{e9} [UNIZZ] [UNI00");
    }

    #[test]
    fn a_message_file_is_named_for_the_language() {
        assert_eq!(localized("event/e50_1.txt", 3), "event/e50_3.txt");
        assert_eq!(localized("names.txt", 3), "names_3.txt");
        assert_eq!(localized("e50.bin", 3), "e50.bin");
        assert_eq!(quoted("Hello <page> there\n  you "), "\"Hello there you\"");
    }

    #[test]
    fn the_discs_tables_name_items_maps_characters_and_packs() {
        let root = temp("tables");
        fs::create_dir_all(root.join("menu/cfg7")).unwrap();
        fs::create_dir_all(root.join("map")).unwrap();
        fs::write(root.join("menu/cfg7/comdatmes1.cfg"), b"MES_SYS 5, \"Bread\";\nMES_SYS 6 , \"Cheese\";\n").unwrap();
        let maps =
            b"MAP_NAME_NUM 3;\nMAP_NAME \"g01\",\"Sindain\",\"g00/g00\",1,1;\n// MAP_NAME \"old\",\"Old\",\"\",1,1;\n\
MAP_NAME \"\",\"\",\"\",0,0;\nMAP_NAME \"m05\",\"Park\",\"\";\n";
        fs::write(root.join("map/map1.cfg"), maps).unwrap();
        let npcs = b"NPC_INFO 1,\"Cedric\",\"p01_03a\",2;\nNPC_INFO 2,\"Donny\",\"p01_04a\",1;\nNPC_INFO 3,\"Donny's twin\",\"p01_04a\",1;\n";
        fs::write(root.join("npc1.cfg"), npcs).unwrap();
        let found = disc_names(&root, 1);
        let text = |table: &str, key: NameKey| found[table].get(&key).cloned();
        assert_eq!(text("item_id", NameKey::Int(6)), Some("Cheese".into()));
        // a map by its row number (the commented row is no row) and by its code
        assert_eq!(text("map_id", NameKey::Int(2)), Some("Park".into()));
        assert_eq!(text("map_id", NameKey::Text("g01".into())), Some("Sindain".into()));
        assert_eq!(found["map_id"].len(), 4);
        assert_eq!(text("npc_id", NameKey::Int(3)), Some("Donny's twin".into()));
        assert_eq!(text("chara_pack", NameKey::Text("p01_04a".into())), Some("Donny or Donny's twin".into()));
        fs::remove_dir_all(&root).unwrap();
    }
}
