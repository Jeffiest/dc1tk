//! The SDK: the headers (sdk/, and each definition set's include/) that declare the natives,
//! programs, inline helpers, constants and value kinds the source uses. Compiling resolves the
//! readable source to what the VM runs (resolve); decompiling turns the VM's calls back into it
//! (present): natives by their SDK names, helpers folded back from the statements they expand to,
//! constants for numbers, results as assignments, locals named for what they hold, and the sound
//! entries play_se plays named for the bank they come from.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;

use crate::definitions::Definitions;
use crate::natives::Table;
use crate::node;
use crate::node::{Map, Node};
use crate::{assets, names, Error, Result};

pub use crate::soundpass::{library_sounds, Passed, Seed, Sounds};

/// The prelude every script includes; it declares no natives.
pub const PRELUDE: &str = "c2s.h";
pub const KEYWORDS: [&str; 28] = [
    "if", "else", "while", "return", "break", "continue", "wait", "end", "skip_end", "print", "sin", "cos", "int",
    "float", "void", "string", "any", "extern", "static", "inline", "enum", "program", "NATIVE", "OPTIONAL", "PROGRAM",
    "MAIN", "AT", "DIRECT",
];
const DECLARATION_KEYS: [&str; 5] = ["natives", "programs", "inlines", "constants", "kinds"];

/// Files under a folder with an extension, sorted by their parts (lowercased on Windows, whose
/// names ignore case), as paths from the folder with "/".
pub fn files_under(base: &Path, extension: &str) -> Vec<String> {
    let mut found: Vec<PathBuf> = Vec::new();
    let mut folders = vec![base.to_path_buf()];
    while let Some(folder) = folders.pop() {
        let Ok(listing) = fs::read_dir(&folder) else {
            continue;
        };
        for entry in listing.flatten() {
            let path = entry.path();
            if path.is_dir() {
                folders.push(path);
            } else if path.extension().is_some_and(|ext| ext == extension) {
                found.push(path);
            }
        }
    }
    let mut relative: Vec<String> =
        found.iter().map(|path| path.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/")).collect();
    relative.sort_by_key(|path| -> Vec<String> {
        let path = if cfg!(windows) { path.to_lowercase() } else { path.clone() };
        path.split('/').map(str::to_string).collect()
    });
    relative
}

/// Path.read_text(): UTF-8 with \r\n and \r read as \n.
pub fn read_text(path: &Path) -> Result<String> {
    Ok(dc2tk_disc::textfile::read_text(path)?)
}

/// Every header under a folder, by path from it.
pub fn load_texts(directory: &Path) -> Result<IndexMap<String, String>> {
    let mut texts = IndexMap::new();
    for relative in files_under(directory, "h") {
        let text = read_text(&directory.join(&relative))?;
        texts.insert(relative, text);
    }
    Ok(texts)
}

/// The SDK's headers and each definition set's, a later one's winning.
pub fn header_texts(defs: &Definitions, sdk_dir: &Path) -> Result<IndexMap<String, String>> {
    let mut texts = load_texts(sdk_dir)?;
    for directory in &defs.sets {
        let include = directory.join("include");
        if include.is_dir() {
            texts.extend(load_texts(&include)?);
        }
    }
    Ok(texts)
}

fn ext_number(name: &str) -> Option<i64> {
    let digits = name.strip_prefix("_EXT_")?;
    (!digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())).then(|| digits.parse().ok()).flatten()
}

/// The declarations headers make: lists of natives, programs, inlines and constants, and the
/// value kinds.
#[derive(Clone, Debug, Default)]
pub struct Declarations {
    pub natives: Vec<Node>,
    pub programs: Vec<Node>,
    pub inlines: Vec<Node>,
    pub constants: Vec<Node>,
    pub kinds: Map,
}

impl Declarations {
    /// A program's declarations ({"natives": [...], ...}).
    pub fn of(node: &Node) -> Declarations {
        Declarations {
            natives: node.list("natives").to_vec(),
            programs: node.list("programs").to_vec(),
            inlines: node.list("inlines").to_vec(),
            constants: node.list("constants").to_vec(),
            kinds: node["kinds"].as_object().cloned().unwrap_or_default(),
        }
    }

    /// The declarations as a program tree holds them.
    pub fn to_node(&self) -> Node {
        node!({
            "natives": self.natives.clone(),
            "programs": self.programs.clone(),
            "inlines": self.inlines.clone(),
            "constants": self.constants.clone(),
            "kinds": Node::Map(self.kinds.clone()),
        })
    }

    /// Adds another file's declarations: lists after this one's, kinds over it.
    pub fn add_from(&mut self, other: &Node) {
        self.add(other);
    }

    fn add(&mut self, other: &Node) {
        for key in DECLARATION_KEYS {
            match key {
                "kinds" => {
                    if let Some(kinds) = other.get(key).and_then(Node::as_object) {
                        for (name, value) in kinds.iter() {
                            self.kinds.insert(name.to_string(), value.clone());
                        }
                    }
                }
                _ => {
                    let into = match key {
                        "natives" => &mut self.natives,
                        "programs" => &mut self.programs,
                        "inlines" => &mut self.inlines,
                        _ => &mut self.constants,
                    };
                    into.extend(other.list(key).iter().cloned());
                }
            }
        }
    }
}

/// Reads a header's declarations, and those of the SDK headers it includes; the native table the
/// headers name.
pub fn header_declarations(
    path: &str,
    headers: &IndexMap<String, String>,
    declarations: &mut Declarations,
    seen: &mut HashSet<String>,
    parse: &dyn Fn(&str) -> Result<Node>,
) -> Result<Option<String>> {
    if path == PRELUDE || seen.contains(path) {
        return Ok(None);
    }
    let Some(text) = headers.get(path) else {
        return Err(Error::invalid(format!("cannot find SDK header <{path}>")));
    };
    seen.insert(path.to_string());
    let header = parse(text)?;
    let mut table = header["natives"].as_str().map(str::to_string);
    declarations.add(&header["declarations"]);
    for item in header.list("functions") {
        if item.kind() == "include" && item["system"].is_set() {
            let nested = header_declarations(item.text("path"), headers, declarations, seen, parse)?;
            if table.is_none() {
                table = nested;
            }
        }
    }
    Ok(table)
}

pub fn header_name(table: &str) -> String {
    format!("{table}.h")
}

/// What a script's headers declare, as the decompiler and compiler use it.
#[derive(Clone, Debug, Default)]
pub struct Context {
    pub table: String,
    /// Natives by their SDK name, each with the canonical (engine) name it stands for.
    pub natives: IndexMap<String, Node>,
    /// The same natives by canonical name.
    pub by_original: HashMap<String, Node>,
    /// The engine's names of the table's natives.
    pub originals: HashSet<String>,
    pub programs: IndexMap<String, i64>,
    pub program_names: HashMap<i64, String>,
    pub inlines: IndexMap<String, Node>,
    /// Constants by name: their value as a tree node.
    pub constants: IndexMap<String, Node>,
    /// Each enum's members: {value: name}.
    pub enums: IndexMap<String, IndexMap<i64, String>>,
    /// Float constants by their bits.
    pub float_names: IndexMap<i64, String>,
    pub old_groups: HashMap<i64, i64>,
    pub old_direct: HashMap<i64, i64>,
    pub old_table_groups: HashMap<i64, i64>,
    pub old_table: bool,
    pub raw_table: bool,
    pub by_number: HashMap<i64, String>,
    /// The headers a decompiled script includes.
    pub includes: Vec<String>,
    /// The event bank of each sound set, by number.
    pub sound_set_banks: HashMap<i64, String>,
    /// Names for the game's asset strings: {(prefix, text): name}.
    pub asset_names: IndexMap<(String, String), String>,
    /// The constant a slot takes while a load has put a pack in it: {(slot, pack): name}, for the
    /// folder's scripts (slots::loaded_names).
    pub slot_packs: HashMap<(i64, String), String>,
    /// The function being resolved was written under #pragma std c89: a native in an operand C
    /// may not evaluate runs only when C would evaluate it.
    pub strict: bool,
    reserved: HashSet<String>,
}

fn numbered_groups(found: &IndexMap<String, i64>) -> HashMap<i64, i64> {
    found.iter().filter_map(|(key, value)| Some((key.parse::<i64>().ok()?, *value))).collect()
}

/// Refuses a constant declared twice: an enum member whose name another enum member or a #define
/// already has (C rejects it, and a number could print as either), or a #define with two values.
pub fn check_constants(constants: &[Node]) -> Result<()> {
    let shown = |declared: &Node| {
        let value = &declared["value"];
        let value = match value.kind() {
            "int" => value["value"].as_i64().map_or_else(String::new, |number| number.to_string()),
            "string" => format!("{:?}", value["value"].as_str().unwrap_or_default()),
            kind => kind.to_string(),
        };
        match declared["enum"].as_str() {
            Some(enumeration) => format!("= {value} in enum {enumeration}"),
            None => format!("#define {value}"),
        }
    };
    let mut first: HashMap<&str, &Node> = HashMap::new();
    for declared in constants {
        let name = declared.text("name");
        let Some(earlier) = first.insert(name, declared) else {
            continue;
        };
        let member = |node: &Node| node["enum"].as_str().is_some();
        if member(earlier) || member(declared) || earlier["value"] != declared["value"] {
            return Err(Error::invalid(format!(
                "{name} is declared twice ({}, then {}); give one of them another name",
                shown(earlier),
                shown(declared)
            )));
        }
    }
    Ok(())
}

/// The context of a set of declarations for a native table.
pub fn context(declarations: &Declarations, table: &str, tables: &BTreeMap<String, Table>) -> Result<Context> {
    check_constants(&declarations.constants)?;
    let rows = tables.get(table);
    let original_by_number: IndexMap<i64, String> =
        rows.map(|rows| rows.natives.iter().map(|row| (row.no as i64, row.name.clone())).collect()).unwrap_or_default();
    let mut found = Context { table: table.to_string(), ..Context::default() };
    for declared in &declarations.natives {
        let no = declared["no"].as_i64().unwrap_or(0);
        let canonical = original_by_number.get(&no).cloned().unwrap_or_else(|| format!("_EXT_{no}"));
        let with = declared.clone().with("canonical", canonical.clone());
        found.natives.insert(declared.text("name").to_string(), with.clone());
        found.by_original.insert(canonical, with);
    }
    for declared in &declarations.programs {
        found.programs.insert(declared.text("name").to_string(), declared["no"].as_i64().unwrap_or(0));
    }
    for declared in &declarations.constants {
        found.constants.insert(declared.text("name").to_string(), declared["value"].clone());
    }
    for declared in &declarations.programs {
        let (name, no) = (declared.text("name").to_string(), declared["no"].as_i64().unwrap_or(0));
        if !found.constants.contains_key(&name) {
            found.constants.insert(name.clone(), node!({"kind": "int", "value": no}));
        }
        found.enums.entry("program".into()).or_default().entry(no).or_insert(name);
    }
    for declared in &declarations.constants {
        let value = &declared["value"];
        let name = declared.text("name").to_string();
        if let Some(enum_name) = declared["enum"].as_str() {
            if let Some(member) = value["value"].as_i64() {
                found.enums.entry(enum_name.to_string()).or_default().entry(member).or_insert(name);
            }
        } else if value.kind() == "float" {
            if let Some(bits) = value["bits"].as_i64() {
                found.float_names.entry(bits).or_insert(name);
            }
        }
    }
    found.originals = original_by_number.values().cloned().collect();
    found.program_names = found.programs.iter().map(|(name, number)| (*number, name.clone())).collect();
    if let Some(rows) = rows {
        found.old_groups = numbered_groups(&rows.old_groups);
        found.old_direct = numbered_groups(&rows.old_direct);
        found.old_table_groups = numbered_groups(&rows.old_table_groups);
    }
    found.by_number = declarations
        .natives
        .iter()
        .map(|declared| (declared["no"].as_i64().unwrap_or(0), declared.text("name").to_string()))
        .collect();
    found.refresh_reserved();
    for inline in &declarations.inlines {
        let shadowed: HashSet<String> =
            inline.list("params").iter().map(|param| param.text("name").to_string()).collect();
        let body = with_constants(&inline["body"], &found.constants, &shadowed);
        let resolved = statements(body.as_array().map(Vec::as_slice).unwrap_or(&[]), &found, &mut Vec::new())?;
        found.inlines.insert(inline.text("name").to_string(), inline.clone().with("body", resolved));
        found.refresh_reserved();
    }
    Ok(found)
}

impl Context {
    fn refresh_reserved(&mut self) {
        let mut names: HashSet<String> = KEYWORDS.iter().map(|word| word.to_string()).collect();
        names.extend(self.natives.keys().cloned());
        names.extend(self.inlines.keys().cloned());
        names.extend(self.programs.keys().cloned());
        names.extend(self.constants.keys().cloned());
        self.reserved = names;
    }
}

/// The context for a native table's header and any extra includes; None when the SDK has no
/// header for the table.
pub fn header_context(
    table: &str,
    headers: &IndexMap<String, String>,
    tables: &BTreeMap<String, Table>,
    extra: &[String],
    parse: &dyn Fn(&str) -> Result<Node>,
) -> Result<Option<Context>> {
    let name = header_name(table);
    if !headers.contains_key(&name) {
        return Ok(None);
    }
    let mut declarations = Declarations::default();
    let mut seen = HashSet::new();
    header_declarations(&name, headers, &mut declarations, &mut seen, parse)?;
    for include in extra {
        header_declarations(include, headers, &mut declarations, &mut seen, parse)?;
    }
    let mut found = context(&declarations, table, tables)?;
    found.includes = std::iter::once(name).chain(extra.iter().cloned()).collect();
    Ok(Some(found))
}

/// The names a script's own names must not take.
pub fn reserved(found: Option<&Context>) -> HashSet<String> {
    match found {
        None => KEYWORDS.iter().map(|word| word.to_string()).collect(),
        Some(found) => found.reserved.clone(),
    }
}

/// A tree with the variables that name constants replaced by the constants' values, except the
/// shadowed ones.
pub fn with_constants(node: &Node, constants: &IndexMap<String, Node>, shadowed: &HashSet<String>) -> Node {
    match node {
        Node::List(items) => Node::List(items.iter().map(|item| with_constants(item, constants, shadowed)).collect()),
        Node::Map(map) => {
            if node.kind() == "var" {
                let name = node.text("name");
                if let Some(value) = constants.get(name).filter(|_| !shadowed.contains(name)) {
                    return value.clone();
                }
            }
            Node::Map(
                map.entries().map(|(key, value)| (key.clone(), with_constants(value, constants, shadowed))).collect(),
            )
        }
        other => other.clone(),
    }
}

fn variable_names(function: &Node, global_names: &HashSet<String>) -> HashSet<String> {
    let mut found: HashSet<String> =
        function.list("params").iter().map(|param| param.text("name").to_string()).collect();
    found.extend(function.list("locals").iter().map(|declared| declared.text("name").to_string()));
    found.extend(global_names.iter().cloned());
    found
}

pub fn is_native(name: &str, found: &Context) -> bool {
    found.natives.contains_key(name) || found.originals.contains(name) || ext_number(name).is_some()
}

pub fn has_result(declared: &Node) -> bool {
    declared.list("params").last().is_some_and(|param| param["result"].is_set())
}

/// A native call as the VM makes it: its canonical name and group base.
fn canonical_native(name: &str, at: &Node, args: Node, found: &Context) -> Node {
    if let Some(declared) = found.natives.get(name) {
        let base = if at.is_null() {
            declared["base"].clone()
        } else if at == "direct" {
            Node::Null
        } else {
            at.clone()
        };
        return node!({"kind": "native", "name": declared["canonical"].clone(), "base": base, "args": args});
    }
    let base = if at.is_null() || at == "direct" { Node::Null } else { at.clone() };
    node!({"kind": "native", "name": name, "base": base, "args": args})
}

fn address_of(target: Node) -> Node {
    if target.kind() == "deref" {
        return target["operand"].clone();
    }
    node!({"kind": "addr", "target": target})
}

fn substituted(node: &Node, bindings: &HashMap<String, Node>, outs: &HashSet<String>) -> Node {
    match node {
        Node::List(items) => Node::List(items.iter().map(|item| substituted(item, bindings, outs)).collect()),
        Node::Map(map) => {
            if node.kind() == "var" {
                if let Some(bound) = bindings.get(node.text("name")) {
                    return bound.clone();
                }
            }
            if node.kind() == "deref" && node["operand"].kind() == "var" && outs.contains(node["operand"].text("name"))
            {
                return bindings[node["operand"].text("name")]["target"].clone();
            }
            Node::Map(
                map.entries()
                    .filter(|(key, _)| key != "line" && key != "start")
                    .map(|(key, value)| (key.clone(), substituted(value, bindings, outs)))
                    .collect(),
            )
        }
        other => other.clone(),
    }
}

fn expanded_inline(inline: &Node, args: &[Node]) -> Result<Vec<Node>> {
    let params = inline.list("params");
    let name = inline.text("name");
    if args.len() != params.len() {
        return Err(Error::invalid(format!("{} takes {} arguments", name, params.len())));
    }
    let bindings: HashMap<String, Node> =
        params.iter().zip(args).map(|(param, arg)| (param.text("name").to_string(), arg.clone())).collect();
    let outs: HashSet<String> =
        params.iter().filter(|param| param["out"].is_set()).map(|param| param.text("name").to_string()).collect();
    for param in params.iter().filter(|param| param["out"].is_set()) {
        let out = param.text("name");
        if bindings[out].kind() != "addr" {
            return Err(Error::invalid(format!("{name}: pass &variable for pointer parameter {out}")));
        }
    }
    Ok(substituted(&inline["body"], &bindings, &outs).as_array().cloned().unwrap_or_default())
}

fn native_values(node: &Node, found: &Context, out: &mut Vec<String>) {
    match node {
        Node::List(items) => items.iter().for_each(|item| native_values(item, found, out)),
        Node::Map(map) => {
            if node.kind() == "call" && is_native(node.text("name"), found) {
                out.push(node.text("name").to_string());
            }
            map.values().for_each(|value| native_values(value, found, out));
        }
        _ => {}
    }
}

fn checked(node: &Node, found: &Context) -> Result<Node> {
    let mut misplaced = Vec::new();
    native_values(node, found, &mut misplaced);
    if let Some(first) = misplaced.first() {
        return Err(Error::invalid(format!(
            "native {first} has no value; call it as a statement or assign its result to a variable"
        )));
    }
    Ok(node.clone())
}

/// Whether an expression calls a native with a result, which hoisting runs before the expression.
fn calls_native(node: &Node, found: &Context) -> bool {
    match node {
        Node::List(items) => items.iter().any(|item| calls_native(item, found)),
        Node::Map(map) => {
            (node.kind() == "call" && is_native(node.text("name"), found))
                || map.values().any(|value| calls_native(value, found))
        }
        _ => false,
    }
}

fn hidden_value(temps: &mut Vec<String>) -> String {
    let value = format!("__value_{}", temps.len());
    temps.push(value.clone());
    value
}

fn assigned(name: &str, value: Node) -> Node {
    node!({"kind": "expr", "expr": {"kind": "assign", "target": {"kind": "var", "name": name}, "value": value}})
}

fn truth(value: Node) -> Node {
    node!({"kind": "unary", "op": "!", "operand": {"kind": "unary", "op": "!", "operand": value}})
}

/// An operand C evaluates only some of the time, holding a native call, in strict mode: the
/// operands it depends on are worked out first into a hidden local, and the native is called in an
/// if, so it runs only when C would evaluate it. a && b is t = !!a; if (t) { t = !!b; } and a || b
/// the same with if (!t); c ? x : y is if (c) { t = x; } else { t = y; }; in a, b the a is a
/// statement of its own before b's natives. None when the node is none of these.
fn sequenced(node: &Node, found: &Context, temps: &mut Vec<String>, before: &mut Vec<Node>) -> Result<Option<Node>> {
    let branch = |part: &Node, temps: &mut Vec<String>, value: &str, made: fn(Node) -> Node| -> Result<Vec<Node>> {
        let mut inner = Vec::new();
        let part = hoisted(part, found, temps, &mut inner)?;
        inner.push(assigned(value, made(part)));
        Ok(inner)
    };
    match node.kind() {
        "logical" if calls_native(&node["right"], found) => {
            let left = hoisted(&node["left"], found, temps, before)?;
            let value = hidden_value(temps);
            before.push(assigned(&value, truth(left)));
            let then = branch(&node["right"], temps, &value, truth)?;
            let tested = node!({"kind": "var", "name": value.clone()});
            let cond = if node["op"] == "&&" { tested } else { node!({"kind": "unary", "op": "!", "operand": tested}) };
            before.push(node!({"kind": "if", "cond": cond, "then": then, "else": null}));
            Ok(Some(node!({"kind": "var", "name": value})))
        }
        "ternary" if calls_native(&node["then"], found) || calls_native(&node["else"], found) => {
            let cond = hoisted(&node["cond"], found, temps, before)?;
            let value = hidden_value(temps);
            let then = branch(&node["then"], temps, &value, |part| part)?;
            let otherwise = branch(&node["else"], temps, &value, |part| part)?;
            before.push(node!({"kind": "if", "cond": cond, "then": then, "else": otherwise}));
            Ok(Some(node!({"kind": "var", "name": value})))
        }
        "comma" if calls_native(&node["right"], found) => {
            let left = hoisted(&node["left"], found, temps, before)?;
            before.push(node!({"kind": "expr", "expr": left}));
            Ok(Some(hoisted(&node["right"], found, temps, before)?))
        }
        _ => Ok(None),
    }
}

/// Native calls inside an expression. The VM's natives push no value, so each one is called
/// first, into a hidden local (__value_N), and the expression reads that local. The calls are
/// added to before, in the order the expression would make them; one in a condition therefore runs
/// before the condition is tested, even on the right of && or || or in a branch of ?:, except in
/// strict mode (sequenced).
fn hoisted(node: &Node, found: &Context, temps: &mut Vec<String>, before: &mut Vec<Node>) -> Result<Node> {
    if found.strict {
        if let Some(lowered) = sequenced(node, found, temps, before)? {
            return Ok(lowered);
        }
    }
    match node {
        Node::List(items) => {
            Ok(Node::List(items.iter().map(|item| hoisted(item, found, temps, before)).collect::<Result<_>>()?))
        }
        Node::Map(map) => {
            let name = node.text("name");
            if node.kind() == "call" && found.inlines.contains_key(name) {
                return Err(Error::invalid(format!("helper {name} is a statement; call it on a line of its own")));
            }
            if node.kind() == "call" && is_native(name, found) {
                if !found.natives.get(name).is_some_and(has_result) {
                    return Err(Error::invalid(format!("native {name} returns nothing, so it has no value to use")));
                }
                let mut args = hoisted(&node["args"], found, temps, before)?;
                let value = hidden_value(temps);
                let target = node!({"kind": "addr", "target": {"kind": "var", "name": value.clone()}});
                if let Some(items) = args.as_array_mut() {
                    items.push(target);
                }
                before.push(canonical_native(name, &Node::Null, args, found));
                return Ok(node!({"kind": "var", "name": value}));
            }
            let mut out = Map::with_capacity(map.len());
            for (key, value) in map.entries() {
                out.insert(key.clone(), hoisted(value, found, temps, before)?);
            }
            Ok(Node::Map(out))
        }
        other => Ok(other.clone()),
    }
}

fn resolved_statement(
    node: &Node,
    found: &Context,
    temps: &mut Vec<String>,
    before: &mut Vec<Node>,
) -> Result<Vec<Node>> {
    let kind = node.kind();
    let at = node.get("at").cloned().unwrap_or(Node::Null);
    let mut plain = node.clone();
    if let Some(map) = plain.as_object_mut() {
        map.remove("at");
    }
    let expression = if kind == "expr" { node.get("expr").filter(|expression| !expression.is_null()) } else { None };
    if let Some(expression) = expression.filter(|expression| expression.kind() == "call") {
        let name = expression.text("name");
        if let Some(inline) = found.inlines.get(name) {
            if !at.is_null() {
                return Err(Error::invalid(format!("AT and DIRECT apply to native calls, not to helper {name}")));
            }
            let args = hoisted(&expression["args"], found, temps, before)?;
            return expanded_inline(inline, args.as_array().map(Vec::as_slice).unwrap_or(&[]));
        }
        if is_native(name, found) {
            let args = hoisted(&expression["args"], found, temps, before)?;
            return Ok(vec![canonical_native(name, &at, args, found)]);
        }
    }
    if let Some(expression) = expression {
        if expression.kind() == "assign"
            && expression["value"].kind() == "call"
            && is_native(expression["value"].text("name"), found)
        {
            let name = expression["value"].text("name");
            if !found.natives.get(name).is_some_and(has_result) {
                return Err(Error::invalid(format!(
                    "native {name} returns nothing; pass &variable instead of assigning"
                )));
            }
            let mut args = hoisted(&expression["value"]["args"], found, temps, before)?;
            let target = address_of(hoisted(&expression["target"], found, temps, before)?);
            if let Some(items) = args.as_array_mut() {
                items.push(target);
            }
            return Ok(vec![canonical_native(name, &at, args, found)]);
        }
    }
    if !at.is_null() {
        return Err(Error::invalid("AT and DIRECT apply only to native calls"));
    }
    match kind {
        "native" => {
            let args = hoisted(&node["args"], found, temps, before)?;
            Ok(vec![canonical_native(node.text("name"), &node["base"], args, found)])
        }
        "if" => {
            let otherwise = match node["else"].as_array() {
                Some(items) => statements(items, found, temps)?,
                None => Node::Null,
            };
            let cond = hoisted(&node["cond"], found, temps, before)?;
            let then = statements(node.list("then"), found, temps)?;
            Ok(vec![plain.with("cond", cond).with("then", then).with("else", otherwise)])
        }
        "while" => {
            let mut inner = Vec::new();
            let cond = hoisted(&node["cond"], found, temps, &mut inner)?;
            let body = statements(node.list("body"), found, temps)?;
            if inner.is_empty() {
                return Ok(vec![plain.with("cond", cond).with("body", body)]);
            }
            // The condition's native calls run at the top of every iteration.
            let leave = node!({"kind": "if", "cond": {"kind": "unary", "op": "!", "operand": cond}, "then": [{"kind": "break"}], "else": null});
            inner.push(leave);
            inner.extend(body.as_array().cloned().unwrap_or_default());
            Ok(vec![plain.with("cond", node!({"kind": "int", "value": 1})).with("body", inner)])
        }
        "switch" => {
            let mut cases = Vec::new();
            for case in node.list("cases") {
                let value = if case["value"].is_null() { Node::Null } else { checked(&case["value"], found)? };
                let body = statements(case.list("body"), found, temps)?;
                cases.push(case.clone().with("value", value).with("body", body));
            }
            let value = hoisted(&node["value"], found, temps, before)?;
            Ok(vec![plain.with("value", value).with("cases", cases)])
        }
        _ => Ok(vec![hoisted(&plain, found, temps, before)?]),
    }
}

fn statement(node: &Node, found: &Context, temps: &mut Vec<String>) -> Result<Vec<Node>> {
    let mut before = Vec::new();
    let resolved = resolved_statement(node, found, temps, &mut before)?;
    let mut result = before;
    result.extend(resolved);
    if let Some(note) = node.get("note").filter(|note| note.is_set()) {
        if let Some(first) = result.first_mut() {
            if !first.get("note").is_some_and(Node::is_set) {
                *first = std::mem::take(first).with("note", note.clone());
            }
        }
    }
    let line = &node["line"];
    if line.is_null() {
        return Ok(result);
    }
    Ok(result
        .into_iter()
        .map(|item| if item.get("line").is_some() { item } else { item.with("line", line.clone()) })
        .collect())
}

/// Statements resolved to what the VM runs: helpers expanded, natives by their canonical names,
/// native results and calls in expressions hoisted into statements of their own.
pub fn statements(nodes: &[Node], found: &Context, temps: &mut Vec<String>) -> Result<Node> {
    let mut out = Vec::new();
    for node in nodes {
        let items = statement(node, found, temps).map_err(|error| error.at(&node["line"]))?;
        out.extend(items);
    }
    Ok(Node::List(out))
}

/// Locals whose array size names a constant get its value.
fn sized_locals(function: &Node, found: &Context) -> Result<Vec<Node>> {
    let mut sized = Vec::new();
    for declared in function.list("locals") {
        if let Some(size) = declared["size"].as_str() {
            let value = found.constants.get(size).filter(|value| value.is_set() && value.kind() == "int");
            let Some(value) = value else {
                return Err(Error::invalid(format!(
                    "{}: array size {} is not a known integer constant",
                    function.text("name"),
                    size
                )));
            };
            sized.push(declared.clone().with("size", value["value"].clone()));
        } else {
            sized.push(declared.clone());
        }
    }
    Ok(sized)
}

const BASE_TYPES: [&str; 3] = ["int", "float", "string"];

/// A type a variable can have as it is: int, float, string, or one of C's other integer types.
fn is_base_type(kind: &str) -> bool {
    BASE_TYPES.contains(&kind) || crate::parse::INTEGER_TYPES.contains(&kind)
}

/// A variable declared with a value kind (bgm_file) or an enum (enum se_bank) has the kind's base
/// type, int for an enum; the kind stays on it, for the decompiler's types.
fn typed(variable: &Node, kinds: &Map) -> Result<Node> {
    let kind = variable.text("type");
    // void *p: a pointer parameter the function never reads through
    if is_base_type(kind) || (kind == "void" && variable["array"].is_set()) {
        return Ok(variable.clone());
    }
    let base = if kind.starts_with("enum ") { Some("int") } else { kinds.get(kind).and_then(Node::as_str) };
    match base {
        Some(base) if is_base_type(base) => Ok(variable.clone().with("type", base).with("kind", kind)),
        _ => Err(Error::invalid(format!("{}: unknown type {}", variable.text("name"), kind))),
    }
}

/// The program as the VM runs it, ready for compile::to_asm.
pub fn resolve(program: &Node, tables: &BTreeMap<String, Table>) -> Result<Node> {
    if program.get("asm").is_some() {
        return Ok(program.clone());
    }
    let declarations = program.get("declarations").map(Declarations::of).unwrap_or_default();
    let mut found = context(&declarations, program.text("natives"), tables)?;
    let program = crate::lower::initialised_globals(program.clone(), &found)?;
    let program = crate::composite::lower(&program, &declarations.kinds, &found)?;
    let program = &crate::lower::integer_types(program, &declarations.kinds, &found)?;
    let global_names: HashSet<String> =
        program.list("globals").iter().map(|declared| declared.text("name").to_string()).collect();
    let mut functions = Vec::new();
    for function in program.list("functions") {
        let mut function = function.clone();
        if function.kind() == "function" {
            let params: Vec<Node> =
                function.list("params").iter().map(|param| typed(param, &declarations.kinds)).collect::<Result<_>>()?;
            let locals: Vec<Node> = function
                .list("locals")
                .iter()
                .map(|declared| typed(declared, &declarations.kinds))
                .collect::<Result<_>>()?;
            function = function.with("params", params).with("locals", locals);
            let body = with_constants(&function["body"], &found.constants, &variable_names(&function, &global_names));
            found.strict = crate::lower::is_strict(&function);
            let mut temps = Vec::new();
            let body = statements(body.as_array().map(Vec::as_slice).unwrap_or(&[]), &found, &mut temps)?;
            let mut locals = sized_locals(&function, &found)?;
            for name in temps {
                locals.push(node!({"name": name, "type": "int", "size": null}));
            }
            function = function.with("body", body).with("locals", locals);
        }
        functions.push(function);
    }
    let mut programs = Vec::new();
    for entry in program.list("programs") {
        let number = match &entry["no"] {
            Node::Str(name) => match found.programs.get(name) {
                Some(number) => Node::Int(*number),
                None => return Err(Error::invalid(format!("unknown program name {name}"))),
            },
            other => other.clone(),
        };
        programs.push(entry.clone().with("no", number));
    }
    Ok(program.clone().with("functions", functions).with("programs", programs))
}

// ---- presenting a decompiled program -------------------------------------------------------

/// What a helper's parameter matches: an out-parameter only an address, an "any" anything, an in
/// parameter anything but an address.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Param {
    Out,
    Any,
    In,
}

fn matches(pattern: &Node, node: &Node, params: &HashMap<String, Param>, bindings: &mut HashMap<String, Node>) -> bool {
    match pattern {
        Node::List(items) => {
            let Some(others) = node.as_array() else {
                return false;
            };
            items.len() == others.len()
                && items.iter().zip(others).all(|(item, other)| matches(item, other, params, bindings))
        }
        Node::Map(map) => {
            let Some(other) = node.as_object() else {
                return false;
            };
            if pattern.kind() == "var" {
                if let Some(param) = params.get(pattern.text("name")) {
                    if *param == Param::Out && node.get("kind").and_then(Node::as_str) != Some("addr") {
                        return false;
                    }
                    if *param == Param::In && node.get("kind").and_then(Node::as_str) == Some("addr") {
                        return false;
                    }
                    return bound(pattern.text("name"), node, bindings);
                }
            }
            if pattern.kind() == "deref"
                && pattern["operand"].kind() == "var"
                && params.get(pattern["operand"].text("name")) == Some(&Param::Out)
            {
                if !matches!(node.get("kind").and_then(Node::as_str), Some("var") | Some("index")) {
                    return false;
                }
                return bound(
                    pattern["operand"].text("name"),
                    &node!({"kind": "addr", "target": node.clone()}),
                    bindings,
                );
            }
            // the same keys, line and start aside
            let counted = |map: &Map| map.keys().filter(|key| *key != "line" && *key != "start").count();
            if counted(map) != counted(other)
                || !map.keys().filter(|key| *key != "line" && *key != "start").all(|key| other.contains_key(key))
            {
                return false;
            }
            map.iter()
                .filter(|(key, _)| *key != "line" && *key != "start")
                .all(|(key, value)| matches(value, other.get(key).unwrap(), params, bindings))
        }
        other => other.same_value(node),
    }
}

fn bound(name: &str, node: &Node, bindings: &mut HashMap<String, Node>) -> bool {
    match bindings.get(name) {
        Some(known) => known.same_value(node),
        None => {
            bindings.insert(name.to_string(), node.clone());
            true
        }
    }
}

/// The helpers, longest body first (then by name), as folding tries them.
fn ordered_inlines(found: &Context) -> Vec<&Node> {
    let mut ranked: Vec<(i64, &String)> =
        found.inlines.iter().map(|(name, inline)| (-(inline.list("body").len() as i64), name)).collect();
    ranked.sort();
    ranked.into_iter().map(|(_, name)| &found.inlines[name]).collect()
}

fn helper_call(inline: &Node, nodes: &[Node], position: usize) -> Option<(Node, Vec<Node>)> {
    let body = inline.list("body");
    let params: HashMap<String, Param> = inline
        .list("params")
        .iter()
        .map(|param| {
            let kind = if param["out"].is_set() {
                Param::Out
            } else if param["type"] == "any" {
                Param::Any
            } else {
                Param::In
            };
            (param.text("name").to_string(), kind)
        })
        .collect();
    let mut bindings = HashMap::new();
    let window = &nodes[position..(position + body.len()).min(nodes.len())];
    let matched = body.len() == window.len()
        && body.iter().zip(window).all(|(pattern, node)| matches(pattern, node, &params, &mut bindings));
    if !matched || bindings.len() != params.len() {
        return None;
    }
    let args: Vec<Node> = inline.list("params").iter().map(|param| bindings[param.text("name")].clone()).collect();
    let call = node!({"kind": "expr", "expr": {"kind": "call", "name": inline["name"].clone(), "args": args}});
    Some((call, inline.list("params").to_vec()))
}

/// Statements with runs that a helper expands to folded back into calls of the helper, choosing
/// the fewest statements (then the fewest native calls).
fn folded(nodes: &[Node], found: &Context, inlines: &[&Node], pointers: &HashSet<String>) -> Vec<Node> {
    let mut best: Vec<(usize, usize)> = vec![(0, 0); nodes.len() + 1];
    let mut choice: Vec<Option<(Node, Vec<Node>, usize)>> = vec![None; nodes.len()];
    // a helper can only start where its first statement's kind (and native) is
    let first: Vec<Option<(&str, &Node)>> = inlines
        .iter()
        .map(|inline| {
            let pattern = inline.list("body").first()?;
            let kind = pattern.get("kind")?.as_str()?;
            (!matches!(kind, "var" | "deref")).then(|| (kind, &pattern["name"]))
        })
        .collect();
    for position in (0..nodes.len()).rev() {
        let (statements_after, natives_after) = best[position + 1];
        best[position] = (statements_after + 1, natives_after + usize::from(nodes[position].kind() == "native"));
        let here = &nodes[position];
        for (inline, first) in inlines.iter().zip(&first) {
            if let Some((kind, name)) = first {
                if here.get("kind").and_then(Node::as_str) != Some(kind)
                    || (*kind == "native" && !name.same_value(&here["name"]))
                {
                    continue;
                }
            }
            let size = inline.list("body").len();
            if position + size > nodes.len() {
                continue;
            }
            let cost = (best[position + size].0 + 1, best[position + size].1);
            if cost >= best[position] {
                continue;
            }
            if let Some((call, params)) = helper_call(inline, nodes, position) {
                best[position] = cost;
                choice[position] = Some((call, params, size));
            }
        }
    }
    let mut out = Vec::new();
    let mut position = 0;
    while position < nodes.len() {
        match choice[position].take() {
            None => {
                out.push(nested(&nodes[position], found, inlines, pointers));
                position += 1;
            }
            Some((call, params, size)) => {
                let args = named_args(call["expr"].list("args"), &params, found);
                let expr = call["expr"].clone().with("args", args);
                let mut call = call.with("expr", expr);
                let comments: Vec<String> = nodes[position..position + size]
                    .iter()
                    .filter_map(|node| node.get("comment").filter(|comment| comment.is_set()).map(Node::plain_text))
                    .collect();
                if !comments.is_empty() {
                    call = call.with("comment", comments.join("; "));
                }
                out.push(call);
                position += size;
            }
        }
    }
    out
}

fn nested(node: &Node, found: &Context, inlines: &[&Node], pointers: &HashSet<String>) -> Node {
    match node.kind() {
        "if" => {
            let otherwise = match node["else"].as_array() {
                Some(items) => Node::List(folded(items, found, inlines, pointers)),
                None => Node::Null,
            };
            let then = folded(node.list("then"), found, inlines, pointers);
            node.clone().with("then", then).with("else", otherwise)
        }
        "while" => {
            let body = folded(node.list("body"), found, inlines, pointers);
            node.clone().with("body", body)
        }
        "native" => readable_native(node, found, pointers),
        _ => node.clone(),
    }
}

fn result_target(arg: &Node, pointers: &HashSet<String>) -> Option<Node> {
    if arg.kind() == "addr" && matches!(arg["target"].kind(), "var" | "index") {
        return Some(arg["target"].clone());
    }
    if arg.kind() == "var" && pointers.contains(arg.text("name")) {
        return Some(node!({"kind": "deref", "operand": arg.clone()}));
    }
    None
}

/// The members of an enum and of its sub-enums (name_*), the first name of a value winning.
pub fn enum_values(name: &str, found: &Context) -> IndexMap<i64, String> {
    let mut values = IndexMap::new();
    let mut enums: Vec<(&String, &IndexMap<i64, String>)> = found.enums.iter().collect();
    enums.sort_by(|a, b| a.0.cmp(b.0));
    let prefix = format!("{name}_");
    for (enum_name, members) in enums {
        if enum_name == name || enum_name.starts_with(&prefix) {
            for (value, constant) in members {
                values.entry(*value).or_insert_with(|| constant.clone());
            }
        }
    }
    values
}

/// A number written as the enum's constant for it (also inside | + - * /).
pub fn named_literal(node: &Node, enum_name: &str, found: &Context) -> Node {
    let names = enum_values(enum_name, found);
    if node.kind() == "int" {
        if let Some(name) = node["value"].as_i64().and_then(|value| names.get(&value)) {
            return node!({"kind": "var", "name": name.clone()});
        }
    }
    if node.kind() == "binary" && matches!(node.text("op"), "|" | "+" | "-" | "*" | "/") {
        let left = named_literal(&node["left"], enum_name, found);
        let right = named_literal(&node["right"], enum_name, found);
        return node.clone().with("left", left).with("right", right);
    }
    node.clone()
}

/// Arguments written with the constants of the enum (or kind) each parameter takes.
fn named_args(args: &[Node], params: &[Node], found: &Context) -> Vec<Node> {
    args.iter()
        .enumerate()
        .map(|(position, arg)| {
            let param = params.get(position);
            let kind = param.and_then(|param| {
                param
                    .get("enum")
                    .filter(|value| value.is_set())
                    .or_else(|| param.get("kind").filter(|value| value.is_set()))
            });
            let enumeration = param.and_then(|param| param.get("enum")).filter(|value| value.is_set());
            match (kind, enumeration) {
                // an address where the native takes an enum (a call written for an older native
                // table): C needs the conversion written out; it compiles to nothing
                (_, Some(enumeration)) if arg.kind() == "addr" => {
                    node!({"kind": "cast", "type": format!("enum {}", enumeration.plain_text()), "operand": arg.clone()})
                }
                (Some(kind), _) => named_literal(arg, &kind.plain_text(), found),
                (None, _) => arg.clone(),
            }
        })
        .collect()
}

fn named_floats(node: &Node, found: &Context) -> Node {
    match node {
        Node::List(items) => Node::List(items.iter().map(|item| named_floats(item, found)).collect()),
        Node::Map(map) => {
            if node.kind() == "float" {
                if let Some(name) = node["bits"].as_i64().and_then(|bits| found.float_names.get(&bits)) {
                    return node!({"kind": "var", "name": name.clone()});
                }
            }
            Node::Map(map.entries().map(|(key, value)| (key.clone(), named_floats(value, found))).collect())
        }
        other => other.clone(),
    }
}

/// A native call by its SDK name, with named arguments and its result as an assignment.
fn readable_native(node: &Node, found: &Context, pointers: &HashSet<String>) -> Node {
    let Some(declared) = found.by_original.get(node.text("name")) else {
        return node.clone().with("result", Node::Null);
    };
    let params = declared.list("params");
    let args = named_args(node.list("args"), params, found);
    let base = if node["base"].same_value(&declared["base"]) {
        Node::Null
    } else if node["base"].is_null() {
        Node::from("direct")
    } else {
        node["base"].clone()
    };
    let mut args = args;
    let mut result = Node::Null;
    if has_result(declared) && args.len() == params.len() {
        if let Some(target) = result_target(args.last().unwrap(), pointers) {
            result = target;
            args.pop();
        }
    }
    node.clone().with("args", args).with("name", declared["name"].clone()).with("base", base).with("result", result)
}

/// Kinds whose constant, when a native is called with one, names what the native's result is
/// about: get_flag(FLAG_GRIFFON_DEFEATED) reads griffon_defeated, get_chara_pos(CHARA_PLAYER, ...)
/// the player's x, y and z.
fn subject_prefix(kind: &str) -> Option<&'static str> {
    match kind {
        "flag" => Some("FLAG_"),
        "chara_slot" => Some("CHARA_"),
        _ => None,
    }
}

/// What a vector getter reads (home for get_home_pos), as get_([a-z][a-z_]*?)_(pos|rot|vec|dir|scale)
/// matches it.
fn vector_subject(name: &str) -> Option<&str> {
    let rest = name.strip_prefix("get_")?;
    let (thing, part) = rest.rsplit_once('_')?;
    let thing_ok = thing.starts_with(|c: char| c.is_ascii_lowercase())
        && thing.bytes().all(|b| b.is_ascii_lowercase() || b == b'_');
    (thing_ok && ["pos", "rot", "vec", "dir", "scale"].contains(&part)).then_some(thing)
}

fn call_subject(node: &Node, declared: &Node, found: &Context) -> Option<(String, String)> {
    let params = declared.list("params");
    for (position, arg) in node.list("args").iter().enumerate() {
        if position >= params.len() || arg.kind() != "int" {
            continue;
        }
        let kind = params[position].get("kind").and_then(Node::as_str).unwrap_or("");
        if let Some(prefix) = subject_prefix(kind) {
            let constant = arg["value"].as_i64().and_then(|value| enum_values(kind, found).get(&value).cloned());
            if let Some(constant) = constant.filter(|constant| constant.starts_with(prefix)) {
                return Some((kind.to_string(), constant[prefix.len()..].to_lowercase()));
            }
        }
    }
    None
}

/// (variable, wanted name, the parameter's own name) for each variable a native's out-parameter
/// fills.
fn out_uses(nodes: &[Node], found: &Context, out: &mut Vec<(String, String, String)>) {
    for node in nodes {
        match node.kind() {
            "native" => {
                let Some(declared) = found.by_original.get(node.text("name")) else {
                    continue;
                };
                let params = declared.list("params");
                let subject = call_subject(node, declared, found);
                let outs: Vec<(usize, &Node)> = node
                    .list("args")
                    .iter()
                    .enumerate()
                    .filter(|(position, arg)| {
                        *position < params.len()
                            && params[*position]["out"].is_set()
                            && arg.kind() == "addr"
                            && arg["target"].kind() == "var"
                    })
                    .collect();
                // get_home_pos(&x, &y, &z) reads the home's position: home_x, home_y, home_z
                let thing = vector_subject(declared.text("name")).map(str::to_string);
                for (position, arg) in &outs {
                    let generic = params[*position].text("name").to_string();
                    let mut wanted = generic.clone();
                    match &subject {
                        Some((kind, subject)) if kind == "flag" && outs.len() == 1 => wanted = subject.clone(),
                        Some((kind, subject)) if kind == "chara_slot" => wanted = format!("{subject}_{wanted}"),
                        _ => {
                            if let Some(thing) = thing
                                .as_ref()
                                .filter(|thing| !["chara", "actor", "handle", "object"].contains(&thing.as_str()))
                            {
                                if ["x", "y", "z"].contains(&wanted.as_str()) {
                                    wanted = format!("{thing}_{wanted}");
                                }
                            }
                        }
                    }
                    out.push((arg["target"].text("name").to_string(), wanted, generic));
                }
            }
            "if" => {
                out_uses(node.list("then"), found, out);
                out_uses(node.list("else"), found, out);
            }
            "while" => out_uses(node.list("body"), found, out),
            _ => {}
        }
    }
}

/// The native parameter each variable is first passed to as a plain value: {variable: parameter}.
fn in_uses(nodes: &[Node], found: &Context, out: &mut HashMap<String, String>) {
    for node in nodes {
        if node.kind() == "native" {
            let declared = found.by_original.get(node.text("name"));
            for (position, arg) in node.list("args").iter().enumerate() {
                if let Some(declared) = declared {
                    let params = declared.list("params");
                    if position < params.len() && !params[position]["out"].is_set() && arg.kind() == "var" {
                        out.entry(arg.text("name").to_string())
                            .or_insert_with(|| params[position].text("name").to_string());
                    }
                }
            }
        }
        for key in ["then", "else", "body"] {
            if let Some(items) = node.get(key).and_then(Node::as_array) {
                in_uses(items, found, out);
            }
        }
    }
}

/// The variables a body reads and writes. A write is an assignment's target, a native's result, or
/// an argument passed by address (an out-parameter); anything else is a read.
fn variable_access(node: &Node, reads: &mut HashSet<String>, writes: &mut HashSet<String>) {
    match node {
        Node::List(items) => items.iter().for_each(|item| variable_access(item, reads, writes)),
        Node::Map(map) => {
            let kind = node.kind();
            if kind == "var" {
                reads.insert(node.text("name").to_string());
                return;
            }
            if (kind == "addr" || kind == "assign") && matches!(node["target"].kind(), "var" | "index") {
                writes.insert(node["target"].text("name").to_string());
                if let Some(index) = node["target"].get("index") {
                    variable_access(index, reads, writes);
                }
                if kind == "assign" {
                    variable_access(&node["value"], reads, writes);
                }
                return;
            }
            if kind == "native" && node.get("result").is_some_and(|result| !result.is_null() && result.kind() == "var")
            {
                writes.insert(node["result"].text("name").to_string());
            }
            for (key, value) in map.iter() {
                if key != "result" {
                    variable_access(value, reads, writes);
                }
            }
        }
        _ => {}
    }
}

fn digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}

/// unused, unused3, unused_value or unused_2.
fn is_unused_local(name: &str) -> bool {
    match name.strip_prefix("unused") {
        Some("") | Some("_value") => true,
        Some(rest) => digits(rest) || rest.strip_prefix('_').is_some_and(digits),
        None => false,
    }
}

/// The name, or the name with _N after it.
fn is_name_or_numbered(name: &str, base: &str) -> bool {
    match name.strip_prefix(base) {
        Some("") => true,
        Some(rest) => rest.strip_prefix('_').is_some_and(digits),
        None => false,
    }
}

fn default_local(name: &str) -> bool {
    name.len() > 1 && name.starts_with('v') && name[1..].bytes().all(|b| b.is_ascii_digit())
}

/// Names locals after the natives that fill them: a slot-numbered local takes the name of the
/// result it first holds, and a local with only that generic name (flag, flag_2) takes the name of
/// what it reads when every read agrees (griffon_defeated for get_flag(FLAG_GRIFFON_DEFEATED)).
pub fn named_locals(function: Node, found: &Context, function_names: &HashSet<String>) -> Node {
    let mut uses = Vec::new();
    out_uses(function.list("body"), found, &mut uses);
    let mut by_variable: HashMap<&str, HashSet<(&str, &str)>> = HashMap::new();
    for (variable, wanted, generic) in &uses {
        by_variable.entry(variable).or_default().insert((wanted, generic));
    }
    let locals = function.list("locals");
    let mut generic_names = HashSet::new();
    for declared in locals {
        let name = declared.text("name");
        if let Some(seen) = by_variable.get(name).filter(|seen| seen.len() == 1) {
            let (wanted, generic) = *seen.iter().next().unwrap();
            if wanted != generic && is_name_or_numbered(name, generic) {
                generic_names.insert(name.to_string());
            }
        }
    }
    // a local called unused that the function reads after all: unset when nothing writes it (the
    // function reads a slot it never sets), otherwise named like any other
    let (mut reads, mut writes) = (HashSet::new(), HashSet::new());
    variable_access(&function["body"], &mut reads, &mut writes);
    let misnamed: HashSet<String> = locals
        .iter()
        .map(|declared| declared.text("name"))
        .filter(|name| is_unused_local(name) && reads.contains(*name))
        .map(str::to_string)
        .collect();
    let mut unnamed: HashSet<String> = locals
        .iter()
        .map(|declared| declared.text("name"))
        .filter(|name| default_local(name))
        .map(str::to_string)
        .collect();
    unnamed.extend(generic_names);
    unnamed.extend(misnamed.iter().cloned());
    if unnamed.is_empty() {
        return function;
    }
    let mut taken: HashSet<String> =
        function.list("params").iter().map(|param| param.text("name").to_string()).collect();
    taken.extend(locals.iter().map(|declared| declared.text("name").to_string()));
    taken.extend(names::variables(&function["body"]));
    // the SDK's names are taken too; checked in place, as there are thousands
    taken.extend(function_names.iter().cloned());
    let mut mapping: IndexMap<String, String> = IndexMap::new();
    for (variable, wanted, _) in &uses {
        if !unnamed.contains(variable) || mapping.contains_key(variable) {
            continue;
        }
        let mut candidate = wanted.clone();
        let mut copy = 1;
        while taken.contains(&candidate) || found.reserved.contains(&candidate) {
            copy += 1;
            candidate = format!("{wanted}_{copy}");
        }
        taken.insert(candidate.clone());
        mapping.insert(variable.clone(), candidate);
    }
    let mut passed = HashMap::new();
    in_uses(function.list("body"), found, &mut passed);
    let mut leftover: Vec<&String> = misnamed.iter().filter(|name| !mapping.contains_key(*name)).collect();
    leftover.sort();
    for variable in leftover {
        // named for the parameter it is first passed to (sound_id for play_se), or by what it is
        let mut wanted = passed
            .get(variable)
            .cloned()
            .unwrap_or_else(|| if writes.contains(variable) { "value" } else { "unset" }.to_string());
        if !writes.contains(variable) && wanted != "unset" {
            wanted = format!("unset_{wanted}");
        }
        let mut candidate = wanted.clone();
        let mut copy = 1;
        while taken.contains(&candidate) || found.reserved.contains(&candidate) {
            copy += 1;
            candidate = format!("{wanted}_{copy}");
        }
        taken.insert(candidate.clone());
        mapping.insert(variable.clone(), candidate);
    }
    if mapping.is_empty() {
        return function;
    }
    let mapping: HashMap<String, String> = mapping.into_iter().collect();
    let locals: Node = locals
        .iter()
        .map(|declared| {
            let name = declared.text("name");
            declared.clone().with("name", mapping.get(name).cloned().unwrap_or_else(|| name.to_string()))
        })
        .collect();
    let body = names::renamed_variables(&function["body"], &mapping);
    function.with("locals", locals).with("body", body)
}

fn call_named(node: &Node, found: &Context, function_kinds: &HashMap<String, BTreeMap<usize, String>>) -> Node {
    match node {
        Node::List(items) => Node::List(items.iter().map(|item| call_named(item, found, function_kinds)).collect()),
        Node::Map(map) => {
            let mut out = Node::Map(
                map.entries().map(|(key, value)| (key.clone(), call_named(value, found, function_kinds))).collect(),
            );
            if node.kind() == "call" {
                if let Some(kinds) = function_kinds.get(node.text("name")) {
                    let args: Vec<Node> = out
                        .list("args")
                        .iter()
                        .enumerate()
                        .map(|(position, arg)| match kinds.get(&position) {
                            Some(kind) => named_literal(arg, kind, found),
                            None => arg.clone(),
                        })
                        .collect();
                    out = out.with("args", args);
                }
            }
            out
        }
        other => other.clone(),
    }
}

/// The name an enum type's constants are looked up by: enum se_bank reads as the kind se_bank.
fn kind_name(kind: &str) -> String {
    kind.strip_prefix("enum ").unwrap_or(kind).to_string()
}

fn variable_kinds(function: &Node) -> HashMap<String, String> {
    function
        .list("params")
        .iter()
        .chain(function.list("locals"))
        .filter(|variable| variable.get("kind").is_some_and(Node::is_set))
        .map(|variable| (variable.text("name").to_string(), kind_name(&variable["kind"].plain_text())))
        .collect()
}

/// Names the numbers assigned to a variable with a value kind, and compared with it.
fn typed_literals(node: &Node, kinds: &HashMap<String, String>, found: &Context) -> Node {
    if kinds.is_empty() {
        return node.clone();
    }
    match node {
        Node::List(items) => Node::List(items.iter().map(|item| typed_literals(item, kinds, found)).collect()),
        Node::Map(map) => {
            let mut out = Node::Map(
                map.entries().map(|(key, value)| (key.clone(), typed_literals(value, kinds, found))).collect(),
            );
            match out.kind() {
                "assign" if out["target"].kind() == "var" => {
                    if let Some(kind) = kinds.get(out["target"].text("name")) {
                        let value = named_literal(&out["value"], kind, found);
                        out = out.with("value", value);
                    }
                }
                "binary" if matches!(out.text("op"), "==" | "!=" | "<" | "<=" | ">" | ">=") => {
                    for (side, other) in [("left", "right"), ("right", "left")] {
                        if out[side].kind() == "var" {
                            if let Some(kind) = kinds.get(out[side].text("name")) {
                                let value = named_literal(&out[other], kind, found);
                                out = out.with(other, value);
                            }
                        }
                    }
                }
                _ => {}
            }
            out
        }
        other => other.clone(),
    }
}

/// The parameters of each function that carry a kind: {function: {position: kind}}.
pub fn function_kinds_of(
    params_by_kind: &HashMap<String, crate::messages::ParamsByFunction>,
) -> HashMap<String, BTreeMap<usize, String>> {
    let mut found: HashMap<String, BTreeMap<usize, String>> = HashMap::new();
    // a parameter carries one kind, so the order the kinds are taken in does not matter
    for kind in params_by_kind.keys() {
        for (name, positions) in &params_by_kind[kind] {
            for position in positions {
                found.entry(name.clone()).or_default().insert(*position, kind.clone());
            }
        }
    }
    found
}

fn calls_made(node: &Node, out: &mut HashSet<(String, String)>) {
    match node {
        Node::List(items) => items.iter().for_each(|item| calls_made(item, out)),
        Node::Map(map) => {
            if matches!(node.kind(), "native" | "call") {
                out.insert((node.kind().to_string(), node.text("name").to_string()));
            }
            map.values().for_each(|value| calls_made(value, out));
        }
        _ => {}
    }
}

/// The hidden functions the sound pass needs: those that call a sound native, or call a function
/// (shown or hidden) that does, directly or further down.
fn sound_related<'a>(hidden: &'a [Node], shown: &[Node], found: &Context) -> Vec<&'a Node> {
    let functions: Vec<&Node> = hidden.iter().chain(shown).filter(|function| function.kind() == "function").collect();
    let calls: Vec<(String, HashSet<(String, String)>)> = functions
        .iter()
        .map(|function| {
            let mut made = HashSet::new();
            calls_made(&function["body"], &mut made);
            (function.text("name").to_string(), made)
        })
        .collect();
    let mut related: HashSet<String> = calls
        .iter()
        .filter(|(_, made)| {
            made.iter().any(|(kind, callee)| {
                let name =
                    found.by_original.get(callee).and_then(|declared| declared["name"].as_str()).unwrap_or(callee);
                kind == "native" && crate::soundpass::is_sound_native(name)
            })
        })
        .map(|(name, _)| name.clone())
        .collect();
    let mut grown = true;
    while grown {
        grown = false;
        for (name, made) in &calls {
            if !related.contains(name) && made.iter().any(|(kind, callee)| kind == "call" && related.contains(callee)) {
                related.insert(name.clone());
                grown = true;
            }
        }
    }
    hidden.iter().filter(|function| function.kind() == "function" && related.contains(function.text("name"))).collect()
}

fn prepared_function(function: &Node, found: &Context, function_names: &HashSet<String>, inlines: &[&Node]) -> Node {
    if function.kind() != "function" {
        return function.clone();
    }
    let body = assets::named(&old_numbered(function.list("body"), found), found);
    let function = named_locals(function.clone().with("body", body), found, function_names);
    let body = folded(function.list("body"), found, inlines, &HashSet::new());
    function.with("body", body)
}

/// The context for a script its definition entry marks as built for an older native table, or a
/// library only such scripts use. "old": groups that are current groups too (old_table_groups) are
/// read as old there. "raw": every call by number without a group comes from the older, smaller
/// table, also those that land on a native the header gives no group (old_direct).
pub fn numbered(found: Option<Context>, numbering: Option<&str>) -> Option<Context> {
    let mut found = found?;
    match numbering {
        Some("raw") => found.raw_table = true,
        Some("old") => {
            let groups: Vec<(i64, i64)> = found.old_table_groups.iter().map(|(base, new)| (*base, *new)).collect();
            found.old_groups.extend(groups);
            found.old_table = true;
        }
        _ => {}
    }
    Some(found)
}

pub fn numbering_of(entry: &Node) -> Option<String> {
    entry.get("numbering").and_then(Node::as_str).map(str::to_string)
}

/// The native an old-numbered call was written for. A call with an old group base selects the
/// native that many numbers on (old_groups); a call by raw number where the header gives the
/// native a group (shown as DIRECT) comes from an older table, and old_direct maps the numbers
/// whose same-shaped twins elsewhere on the disc show what they stand for.
pub fn meant_native(node: &Node, found: &Context) -> Option<String> {
    let name = node.text("name");
    let (no, declared_base) = match found.by_original.get(name) {
        Some(declared) => (declared["no"].as_i64()?, declared["base"].clone()),
        // a number the engine does not register, as the decompiler names it
        None => (ext_number(name)?, Node::Null),
    };
    if node["base"].is_null() && found.old_table {
        if let Some(group) = found.old_groups.get(&no) {
            // a call to an old group's first number, made without a group
            return found.by_number.get(group).cloned();
        }
    }
    if node["base"].is_null() {
        if (declared_base.is_null() && !found.raw_table) || !found.old_direct.contains_key(&no) {
            return None;
        }
        return found.by_number.get(&found.old_direct[&no]).cloned();
    }
    let base = node["base"].as_i64()?;
    let group = found.old_groups.get(&base)?;
    found.by_number.get(&(no - base + group)).cloned()
}

fn old_numbered(nodes: &[Node], found: &Context) -> Node {
    nodes
        .iter()
        .map(|node| match node.kind() {
            "if" => {
                let otherwise = match node["else"].as_array() {
                    Some(items) => old_numbered(items, found),
                    None => Node::Null,
                };
                node.clone().with("then", old_numbered(node.list("then"), found)).with("else", otherwise)
            }
            "while" => node.clone().with("body", old_numbered(node.list("body"), found)),
            "native" => {
                let mut notes = Vec::new();
                if let Some(meant) = meant_native(node, found) {
                    notes.push(format!("old numbering, meant as {meant}"));
                }
                if node.text("name").starts_with("_EXT_") {
                    // the engine's table has no function at this number; the VM prints "not found
                    // ext" and goes on (CRunScript::ext, runscript.cpp)
                    notes.push("no native at this number, so the call does nothing".into());
                }
                if notes.is_empty() {
                    return node.clone();
                }
                if let Some(comment) = node.get("comment").filter(|comment| comment.is_set()) {
                    notes.push(comment.plain_text());
                }
                node.clone().with("comment", notes.join("; "))
            }
            _ => node.clone(),
        })
        .collect()
}

/// A context with the definition set's event bank of each sound set, keyed by number.
pub fn with_sound_sets(found: Option<Context>, defs: &Definitions) -> Option<Context> {
    let mut found = found?;
    if !defs.sound_set_banks.is_empty() {
        found.sound_set_banks = defs
            .sound_set_banks
            .iter()
            .filter_map(|(number, bank)| Some((number.parse().ok()?, bank.clone())))
            .collect();
    }
    Some(found)
}

fn shadowing(function: &Node, names_used: &[&String]) -> bool {
    let declared: HashSet<&str> =
        function.list("params").iter().chain(function.list("locals")).map(|variable| variable.text("name")).collect();
    names_used.iter().any(|name| declared.contains(name.as_str()))
}

pub fn present_inline(inline: &Node, found: &Context) -> Node {
    let pointers: HashSet<String> = inline
        .list("params")
        .iter()
        .filter(|param| param["out"].is_set())
        .map(|param| param.text("name").to_string())
        .collect();
    let body = folded(inline.list("body"), found, &[], &pointers);
    inline.clone().with("body", body)
}

/// The functions as the decompiler shows them. hidden: the included library functions of a
/// script, which are not shown but whose sound flow counts. sounds: what the scripts that include
/// a library pass it, and where to put what this script passes the hidden functions.
pub fn present_functions(
    functions: &[Node],
    found: &Context,
    params_by_kind: &HashMap<String, crate::messages::ParamsByFunction>,
    hidden: &[Node],
    sounds: &mut Sounds,
) -> Vec<Node> {
    let mut function_kinds = function_kinds_of(params_by_kind);
    // parameters a definition set gives a value kind (types)
    for function in functions.iter().filter(|function| function.kind() == "function") {
        for (position, param) in function.list("params").iter().enumerate() {
            if let Some(kind) = param.get("kind").filter(|kind| kind.is_set()) {
                function_kinds
                    .entry(function.text("name").to_string())
                    .or_default()
                    .insert(position, kind_name(&kind.plain_text()));
            }
        }
    }
    let functions: Vec<Node> = if function_kinds.is_empty() {
        functions.to_vec()
    } else {
        functions
            .iter()
            .map(|function| {
                if function.kind() == "function" {
                    function.clone().with("body", call_named(&function["body"], found, &function_kinds))
                } else {
                    function.clone()
                }
            })
            .collect()
    };
    let inlines = ordered_inlines(found);
    let function_names: HashSet<String> = functions
        .iter()
        .filter(|function| function.kind() == "function")
        .map(|function| function.text("name").to_string())
        .collect();
    let prepared: Vec<Node> =
        functions.iter().map(|function| prepared_function(function, found, &function_names, &inlines)).collect();
    // included code is the same in every script that includes it, so it is prepared once per set
    // of global names (sounds.keys, into sounds.cache)
    let mut prepared_hidden = Vec::new();
    for function in sound_related(hidden, &functions, found) {
        let name = function.text("name");
        match sounds.keys.get(name).cloned() {
            None => prepared_hidden.push(prepared_function(function, found, &function_names, &inlines)),
            Some(key) => {
                if !sounds.cache.contains_key(&key) {
                    let ready = prepared_function(function, found, &function_names, &inlines);
                    sounds.cache.insert(key.clone(), ready);
                }
                prepared_hidden.push(sounds.cache[&key].clone());
            }
        }
    }
    let all: Vec<Node> = prepared.iter().chain(&prepared_hidden).cloned().collect();
    let (known, seen) = crate::soundpass::program_sounds(&all, found, sounds.seed.as_ref(), sounds.entries.as_ref());
    if sounds.want_out {
        sounds.out = Some(crate::soundpass::passed_sounds(&prepared_hidden, &known, &seen));
    }
    let float_names: Vec<&String> = found.float_names.values().collect();
    let presented: Vec<Node> = prepared
        .into_iter()
        .map(|function| {
            if function.kind() != "function" {
                return function;
            }
            let mut banks = crate::soundpass::sound_banks(&function, &known);
            let flow = crate::soundpass::sound_flow(&function, &known);
            let body = crate::soundpass::named_sounds(function.list("body"), found, &mut banks, Some(&flow), None);
            let mut body = typed_literals(&body, &variable_kinds(&function), found);
            if !found.float_names.is_empty() && !shadowing(&function, &float_names) {
                body = named_floats(&body, found);
            }
            function.with("body", body)
        })
        .collect();
    crate::slotflow::named_slots(presented, hidden, found)
}

/// A decompiled program as readable source: natives by their SDK names, helpers, constants,
/// named results, locals and sounds; the SDK header included in place of the native table.
pub fn present(
    program: Node,
    found: Option<&Context>,
    params_by_kind: &HashMap<String, crate::messages::ParamsByFunction>,
    hidden: &[Node],
    sounds: &mut Sounds,
) -> Node {
    let Some(found) = found else {
        return program;
    };
    if program.get("asm").is_some() {
        return program;
    }
    let programs: Vec<Node> = program
        .list("programs")
        .iter()
        .map(|entry| {
            let number = entry["no"]
                .as_i64()
                .and_then(|no| found.program_names.get(&no))
                .map(Node::from)
                .unwrap_or_else(|| entry["no"].clone());
            entry.clone().with("no", number)
        })
        .collect();
    // the functions the engine starts, which find the event port as the last event left it
    let mut entries: HashSet<String> =
        program.list("programs").iter().map(|entry| entry.text("function").to_string()).collect();
    if let Some(main) = program["main"].as_str() {
        entries.insert(main.to_string());
    }
    sounds.entries = Some(entries);
    let mut functions: Vec<Node> =
        found.includes.iter().map(|path| node!({"kind": "include", "path": path.clone(), "system": true})).collect();
    functions.extend(present_functions(program.list("functions"), found, params_by_kind, hidden, sounds));
    program.with("natives", Node::Null).with("programs", programs).with("functions", functions)
}
