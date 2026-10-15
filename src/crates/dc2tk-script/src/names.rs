//! Names from the definition set: functions are found by fingerprint (exact, else loose for a
//! translated script) and get their names, docs, parameter and local names, value kinds, return
//! types and line notes; the rest get names from the programs they handle or func_N. Every name is
//! made unique within its scope, avoiding the SDK's own names.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use indexmap::IndexMap;

use crate::node;
use crate::node::Node;
use crate::source::truthy;
use crate::{Error, Result};

/// The definition set's entries by fingerprint.
pub type Entries = IndexMap<String, Node>;

pub fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic()) && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn strings(node: &Node) -> Vec<String> {
    node.as_array()
        .map(|items| items.iter().filter_map(|item| item.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

/// Each table's entries (names/<table>.json), tables in file name order.
pub fn load(directory: &Path) -> Result<IndexMap<String, Entries>> {
    let mut paths: Vec<_> = match fs::read_dir(directory) {
        Ok(listing) => listing
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .collect(),
        Err(_) => Vec::new(),
    };
    paths.sort();
    let mut tables = IndexMap::new();
    for path in paths {
        let file = path.file_name().unwrap().to_string_lossy().into_owned();
        let mut value: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path)?)
            .map_err(|error| Error::invalid(format!("{file}: {error}")))?;
        let serde_json::Value::Object(functions) = value["functions"].take() else {
            return Err(Error::invalid(format!("{file} has no functions")));
        };
        let mut entries = Entries::with_capacity(functions.len());
        for (fingerprint, entry) in functions {
            let entry = Node::from(entry);
            let mut all = vec![entry.get("name").and_then(Node::as_str).unwrap_or("x").to_string()];
            all.extend(strings(&entry["params"]));
            all.extend(strings(&entry["locals"]));
            for name in all {
                if !is_identifier(&name) {
                    return Err(Error::invalid(format!("{file}: {name} is not a valid name for {fingerprint}")));
                }
            }
            entries.insert(fingerprint, entry);
        }
        tables.insert(path.file_stem().unwrap().to_string_lossy().into_owned(), entries);
    }
    Ok(tables)
}

/// Every table's entries in one, a later table's winning.
pub fn merged(tables: &IndexMap<String, Entries>) -> Entries {
    let mut found = Entries::new();
    for table in tables.values() {
        for (fingerprint, entry) in table {
            found.insert(fingerprint.clone(), entry.clone());
        }
    }
    found
}

fn walk_calls(node: &Node, out: &mut Vec<String>) {
    match node {
        Node::List(items) => items.iter().for_each(|item| walk_calls(item, out)),
        Node::Map(map) => {
            if node.kind() == "call" {
                if let Some(name) = node["name"].as_str() {
                    out.push(name.to_string());
                }
            }
            map.values().for_each(|value| walk_calls(value, out));
        }
        _ => {}
    }
}

pub fn callees(node: &Node) -> Vec<String> {
    let mut out = Vec::new();
    walk_calls(node, &mut out);
    out
}

fn walk_variables(node: &Node, out: &mut Vec<String>) {
    match node {
        Node::List(items) => items.iter().for_each(|item| walk_variables(item, out)),
        Node::Map(map) => {
            if matches!(node.kind(), "var" | "index") {
                if let Some(name) = node["name"].as_str() {
                    out.push(name.to_string());
                }
            }
            map.values().for_each(|value| walk_variables(value, out));
        }
        _ => {}
    }
}

pub fn variables(node: &Node) -> Vec<String> {
    let mut out = Vec::new();
    walk_variables(node, &mut out);
    out
}

/// A copy of a tree with its calls renamed.
pub fn mapped(node: &Node, mapping: &HashMap<String, String>) -> Result<Node> {
    Ok(match node {
        Node::List(items) => Node::List(items.iter().map(|item| mapped(item, mapping)).collect::<Result<_>>()?),
        Node::Map(map) => {
            let mut out = crate::node::Map::with_capacity(map.len());
            for (key, value) in map.entries() {
                out.insert(key.clone(), mapped(value, mapping)?);
            }
            if node.kind() == "call" {
                let name = node["name"].as_str().unwrap_or("");
                let to = mapping
                    .get(name)
                    .ok_or_else(|| Error::invalid(format!("a call to {name}, which is not a function")))?;
                out.insert("name", Node::from(to));
            }
            Node::Map(out)
        }
        other => other.clone(),
    })
}

/// A copy of a tree with its variables renamed.
pub fn renamed_variables(node: &Node, mapping: &HashMap<String, String>) -> Node {
    match node {
        Node::List(items) => Node::List(items.iter().map(|item| renamed_variables(item, mapping)).collect()),
        Node::Map(map) => {
            let mut out = crate::node::Map::with_capacity(map.len());
            for (key, value) in map.entries() {
                out.insert(key.clone(), renamed_variables(value, mapping));
            }
            if matches!(node.kind(), "var" | "index") {
                if let Some(to) = node["name"].as_str().and_then(|name| mapping.get(name)) {
                    out.insert("name", Node::from(to));
                }
            }
            Node::Map(out)
        }
        other => other.clone(),
    }
}

/// Each function's exact fingerprint (None for assembly), by name.
pub fn fingerprints(program: &Node) -> IndexMap<String, Option<String>> {
    let known = &program["fingerprints"];
    list(program, "functions")
        .iter()
        .map(|function| {
            let name = function["name"].as_str().unwrap_or("").to_string();
            let found =
                if function.kind() == "function" { known[name.as_str()].as_str().map(str::to_string) } else { None };
            (name, found)
        })
        .collect()
}

fn list<'a>(node: &'a Node, key: &str) -> &'a [Node] {
    node[key].as_array().map(Vec::as_slice).unwrap_or(&[])
}

/// The exact fingerprint to the loose one, for every function that has both.
pub fn loose_map(program: &Node) -> IndexMap<String, String> {
    let mut out = IndexMap::new();
    let (Some(exact), Some(loose)) = (program["fingerprints"].as_object(), program["loose_fingerprints"].as_object())
    else {
        return out;
    };
    for (name, print) in exact.iter() {
        if let (Some(print), Some(found)) = (print.as_str(), loose.get(name).and_then(Node::as_str)) {
            out.insert(print.to_string(), found.to_string());
        }
    }
    out
}

/// The merged entries and an index of their loose fingerprints, built once for many scripts.
pub struct Known {
    pub entries: Entries,
    /// The first named entry with each loose fingerprint, by its index in entries.
    loose: HashMap<String, usize>,
    /// Loose fingerprints named entries give different names.
    ambiguous: HashSet<String>,
}

impl Known {
    pub fn new(entries: Entries) -> Known {
        let mut loose: HashMap<String, usize> = HashMap::new();
        let mut ambiguous = HashSet::new();
        for (at, entry) in entries.values().enumerate() {
            if let (Some(print), Some(name)) = (entry.get("loose").and_then(Node::as_str), entry.get("name")) {
                let first = *loose.entry(print.to_string()).or_insert(at);
                if entries[first]["name"] != *name {
                    ambiguous.insert(print.to_string());
                }
            }
        }
        Known { entries, loose, ambiguous }
    }

    /// The entries for one program: with its functions known only by their loose fingerprint
    /// added, where that names one function only.
    pub fn with_loose(&self, exact_to_loose: &IndexMap<String, String>) -> Lookup<'_> {
        let mut added = IndexMap::new();
        for (exact, loose) in exact_to_loose {
            if !self.entries.contains_key(exact) && !self.ambiguous.contains(loose) {
                if let Some(&at) = self.loose.get(loose) {
                    added.insert(exact.clone(), &self.entries[at]);
                }
            }
        }
        Lookup { known: self, added }
    }
}

/// The entries one program's functions are looked up in.
pub struct Lookup<'a> {
    known: &'a Known,
    added: IndexMap<String, &'a Node>,
}

impl Lookup<'_> {
    pub fn get(&self, fingerprint: &str) -> Option<&Node> {
        self.known.entries.get(fingerprint).or_else(|| self.added.get(fingerprint).copied())
    }

    /// Every entry, the added ones after the rest (as Python's dict copy with them set).
    pub fn entries(&self) -> Entries {
        let mut out = self.known.entries.clone();
        for (exact, entry) in &self.added {
            out.insert(exact.clone(), (*entry).clone());
        }
        out
    }
}

/// The entries with the functions of a script's language variants (s51_0.stb to s51_5.stb) named
/// where another variant's function at the same position is: by loose fingerprint, for variants
/// with the same number of functions.
pub fn with_positions(
    entries: &Entries,
    script_prints: &IndexMap<String, Vec<Option<String>>>,
    exact_to_loose: &IndexMap<String, String>,
) -> Entries {
    let mut groups: IndexMap<String, Vec<&Vec<Option<String>>>> = IndexMap::new();
    for (path, prints) in script_prints {
        groups.entry(crate::definitions::variant_base(path)).or_default().push(prints);
    }
    let mut out = entries.clone();
    for variants in groups.values() {
        let lengths: HashSet<usize> = variants.iter().map(|prints| prints.len()).collect();
        if lengths.len() != 1 {
            continue;
        }
        for column in 0..variants[0].len() {
            let exacts: Vec<&Option<String>> = variants.iter().map(|prints| &prints[column]).collect();
            let loose_of = |exact: &Option<String>| exact.as_ref().and_then(|exact| exact_to_loose.get(exact)).cloned();
            let mut named: HashMap<Option<String>, &Node> = HashMap::new();
            for exact in &exacts {
                if let Some(entry) =
                    exact.as_ref().and_then(|exact| entries.get(exact)).filter(|entry| entry.get("name").is_some())
                {
                    named.insert(loose_of(exact), entry);
                }
            }
            for exact in &exacts {
                let Some(name) = exact else {
                    continue;
                };
                if !out.contains_key(name) {
                    if let Some(entry) = named.get(&loose_of(exact)) {
                        out.insert(name.clone(), (*entry).clone());
                    }
                }
            }
        }
    }
    out
}

/// The names an entry gives a function's parameters and locals, made unique and kept off the
/// SDK's names.
fn variable_mapping(function: &Node, entry: &Node, reserved: &HashSet<String>) -> HashMap<String, String> {
    let params = list(function, "params");
    let locals = list(function, "locals");
    let wanted: Vec<(&Node, String)> =
        params.iter().zip(strings(&entry["params"])).chain(locals.iter().zip(strings(&entry["locals"]))).collect();
    let declared_name = |declared: &Node| declared["name"].as_str().unwrap_or("").to_string();
    let mut kept: HashSet<String> = params.iter().chain(locals).map(declared_name).collect();
    kept.extend(variables(&function["body"]));
    for (declared, _) in &wanted {
        kept.remove(&declared_name(declared));
    }
    let mut taken = kept;
    taken.extend(callees(&function["body"]));
    let mut mapping = HashMap::new();
    for (declared, name) in wanted {
        let base = if reserved.contains(&name) { format!("{name}_value") } else { name };
        let mut name = base.clone();
        let mut copy = 1;
        while taken.contains(&name) {
            copy += 1;
            name = format!("{base}_{copy}");
        }
        taken.insert(name.clone());
        mapping.insert(declared_name(declared), name);
    }
    mapping
}

const RETURN_TYPES: [&str; 3] = ["void", "int", "float"];

fn returns_value(nodes: &[Node]) -> bool {
    nodes.iter().any(|node| match node.kind() {
        "return" => !node["value"].is_null(),
        "if" => returns_value(list(node, "then")) || returns_value(list(node, "else")),
        "while" => returns_value(list(node, "body")),
        _ => false,
    })
}

/// The value kinds an entry gives variables ("types": {"track": "bgm_file"}), keyed by the names
/// the entry gives them, or their default names; each lands on the variable in that position.
fn with_types(function: Node, entry: &Node) -> Node {
    let Some(types) = entry.get("types").and_then(Node::as_object).filter(|types| !types.is_empty()) else {
        return function;
    };
    let kinds = |variables: &[Node], names: Vec<String>| -> Node {
        variables
            .iter()
            .enumerate()
            .map(|(position, variable)| {
                let name =
                    names.get(position).cloned().unwrap_or_else(|| variable["name"].as_str().unwrap_or("").to_string());
                match types.get(&name) {
                    Some(kind) => variable.clone().with("kind", kind.clone()),
                    None => variable.clone(),
                }
            })
            .collect()
    };
    let params = kinds(list(&function, "params"), strings(&entry["params"]));
    let locals = kinds(list(&function, "locals"), strings(&entry["locals"]));
    function.with("params", params).with("locals", locals)
}

/// Puts line notes ({instruction index: text}) on the statements that start at those
/// instructions.
fn with_notes(nodes: &[Node], notes: &crate::node::Map) -> Node {
    nodes
        .iter()
        .map(|node| {
            let mut node = node.clone();
            let start = crate::source::py_str(node.get("start").unwrap_or(&Node::Null));
            if let Some(note) = notes.get(&start) {
                node = node.with("note", note.clone());
            }
            match node.kind() {
                "if" => {
                    let then = with_notes(list(&node, "then"), notes);
                    let otherwise = match node["else"].as_array() {
                        Some(items) => with_notes(items, notes),
                        None => Node::Null,
                    };
                    node.with("then", then).with("else", otherwise)
                }
                "while" => {
                    let body = with_notes(list(&node, "body"), notes);
                    node.with("body", body)
                }
                _ => node,
            }
        })
        .collect()
}

pub fn with_variables(mut function: Node, entry: &Node, reserved: &HashSet<String>) -> Node {
    if let Some(doc) = entry.get("doc").filter(|doc| truthy(doc)) {
        function = function.with("doc", doc.clone());
    }
    // a declared return type; void only where no return gives a value
    if let Some(returns) = entry.get("returns").and_then(Node::as_str).filter(|returns| RETURN_TYPES.contains(returns))
    {
        if returns != "void" || !returns_value(list(&function, "body")) {
            function = function.with("returns", returns);
        }
    }
    function = with_types(function, entry);
    if let Some(notes) = entry.get("notes").filter(|notes| truthy(notes)).and_then(Node::as_object) {
        let body = with_notes(list(&function, "body"), notes);
        function = function.with("body", body);
    }
    let mapping = variable_mapping(&function, entry, reserved);
    if mapping.is_empty() {
        return function;
    }
    let rename = |declared: &Node| {
        let name = declared["name"].as_str().unwrap_or("");
        declared.clone().with("name", mapping.get(name).cloned().unwrap_or_else(|| name.to_string()))
    };
    let params: Node = list(&function, "params").iter().map(rename).collect();
    let locals: Node = list(&function, "locals").iter().map(rename).collect();
    let body = renamed_variables(&function["body"], &mapping);
    function.with("params", params).with("locals", locals).with("body", body)
}

pub fn described(program: Node, found: &[Option<String>], entries: &Lookup, reserved: &HashSet<String>) -> Node {
    let functions: Node = list(&program, "functions")
        .iter()
        .zip(found)
        .map(|(function, fingerprint)| match fingerprint.as_ref().and_then(|print| entries.get(print)) {
            Some(entry) if function.kind() == "function" => with_variables(function.clone(), entry, reserved),
            _ => function.clone(),
        })
        .collect();
    program.with("functions", functions)
}

/// The names programs give the functions they start: on_<name> for a named program, program_N
/// otherwise, and start for main.
fn entry_names(program: &Node, program_names: &HashMap<i64, String>) -> HashMap<String, String> {
    let mut found = HashMap::new();
    for entry in list(program, "programs") {
        let function = entry["function"].as_str().unwrap_or("").to_string();
        let no = entry["no"].as_i64();
        let name = match no.and_then(|no| program_names.get(&no)) {
            Some(name) => format!("on_{name}"),
            None => format!("program_{}", crate::source::py_str(&entry["no"])),
        };
        found.entry(function).or_insert(name);
    }
    found.entry(program["main"].as_str().unwrap_or("").to_string()).or_insert_with(|| "start".to_string());
    found
}

fn entry_alias(base: &str, entry: &str) -> String {
    if entry.starts_with("program_") {
        format!("{base}_for_{entry}")
    } else {
        entry.to_string()
    }
}

pub fn assign(
    program: &Node,
    found: &IndexMap<String, Option<String>>,
    class_names: &HashMap<String, String>,
    reserved: &HashSet<String>,
    program_names: &HashMap<i64, String>,
) -> HashMap<String, String> {
    let entries = entry_names(program, program_names);
    let handler_names: HashSet<String> = program_names.values().map(|name| format!("on_{name}")).collect();
    let mut mapping = HashMap::new();
    let mut taken: HashSet<String> = reserved.clone();
    let mut own = 0;
    // the fingerprints named so far, and the later copies of one the numbering named
    let mut prints: HashSet<String> = HashSet::new();
    let mut copies: Vec<(String, String)> = Vec::new();
    for function in list(program, "functions") {
        let name = function["name"].as_str().unwrap_or("");
        let fingerprint = found.get(name).cloned().flatten();
        let mut base = match fingerprint.as_ref().and_then(|print| class_names.get(print)) {
            Some(class_name) => {
                let mut base = class_name.clone();
                // A handler named for its program elsewhere (on_dodge_shot) takes the name of the
                // program it handles here, as the same code can be bound to another one.
                if handler_names.contains(&base) && handler_names.contains(entries.get(name).unwrap_or(&base)) {
                    if let Some(entry) = entries.get(name) {
                        base = entry.clone();
                    }
                }
                base
            }
            None => match entries.get(name) {
                Some(entry) => entry.clone(),
                None => {
                    own += 1;
                    format!("func_{own}")
                }
            },
        };
        if taken.contains(&base) {
            if let Some(entry) = entries.get(name) {
                base = entry_alias(&base, entry);
            }
        }
        let unique = numbered(&base, &taken);
        let repeated = fingerprint.is_some_and(|print| !prints.insert(print));
        if repeated && unique != base && !entries.contains_key(name) {
            copies.push((name.to_string(), base));
        }
        taken.insert(unique.clone());
        mapping.insert(name.to_string(), unique);
    }
    named_copies(program, &copies, &mut mapping, &mut taken);
    mapping
}

/// base, or base_2, base_3 ... whichever is free.
fn numbered(base: &str, taken: &HashSet<String>) -> String {
    let mut unique = base.to_string();
    let mut copy = 1;
    while taken.contains(&unique) {
        copy += 1;
        unique = format!("{base}_{copy}");
    }
    unique
}

/// Renames the later copies of code a file holds twice or more (identical stubs such as
/// do_nothing, which a fingerprint cannot tell apart), which the numbering named base_2: one that
/// a single other function calls is named for it (unlock_interrupts_for_recoil_from_hit, without
/// the caller's on_), one nothing calls or starts is unused_<base>. Copies called from several
/// places, or only from another copy, keep their number.
fn named_copies(
    program: &Node,
    copies: &[(String, String)],
    mapping: &mut HashMap<String, String>,
    taken: &mut HashSet<String>,
) {
    if copies.is_empty() {
        return;
    }
    let renamed: HashSet<&str> = copies.iter().map(|(name, _)| name.as_str()).collect();
    let mut callers: HashMap<String, Vec<String>> = HashMap::new();
    for function in list(program, "functions") {
        let caller = function["name"].as_str().unwrap_or("").to_string();
        for callee in callees(&function["body"]) {
            let found = callers.entry(callee).or_default();
            if !found.contains(&caller) {
                found.push(caller.clone());
            }
        }
    }
    for (name, base) in copies {
        let wanted = match callers.get(name).map(Vec::as_slice).unwrap_or(&[]) {
            [] => format!("unused_{base}"),
            [caller] if caller != name && !renamed.contains(caller.as_str()) => {
                let shown = &mapping[caller];
                format!("{base}_for_{}", shown.strip_prefix("on_").unwrap_or(shown))
            }
            _ => continue,
        };
        let unique = numbered(&wanted, taken);
        taken.insert(unique.clone());
        mapping.insert(name.clone(), unique);
    }
}

pub fn rename(program: Node, mapping: &HashMap<String, String>) -> Result<Node> {
    let lookup = |name: &Node| -> Result<String> {
        let name = name.as_str().unwrap_or("");
        mapping.get(name).cloned().ok_or_else(|| Error::invalid(format!("{name} is not a function")))
    };
    let mut functions = Vec::new();
    for function in list(&program, "functions") {
        let mut renamed = function.clone().with("name", lookup(&function["name"])?);
        if function.kind() == "function" {
            renamed = renamed.with("body", mapped(&function["body"], mapping)?);
        }
        functions.push(renamed);
    }
    let mut programs = Vec::new();
    for entry in list(&program, "programs") {
        programs.push(node!({"no": entry["no"].clone(), "function": lookup(&entry["function"])?}));
    }
    let main = lookup(&program["main"])?;
    Ok(program.with("functions", functions).with("programs", programs).with("main", main))
}

pub fn global_mapping(
    program: &Node,
    entry: &Node,
    protected: &HashSet<String>,
    forced: &HashMap<String, String>,
) -> HashMap<String, String> {
    let globals = list(program, "globals");
    let name_of = |declared: &Node| declared["name"].as_str().unwrap_or("").to_string();
    let mut mapping: HashMap<String, String> = globals
        .iter()
        .filter_map(|declared| forced.get(&name_of(declared)).map(|to| (name_of(declared), to.clone())))
        .collect();
    let mut taken: HashSet<String> = mapping.values().cloned().collect();
    for (declared, wanted) in globals.iter().zip(list(entry, "globals")) {
        let name = match wanted.as_str() {
            Some(name) => name.to_string(),
            None => crate::source::py_str(&wanted["name"]),
        };
        let current = name_of(declared);
        if protected.contains(&current)
            || mapping.contains_key(&current)
            || taken.contains(&name)
            || !is_identifier(&name)
        {
            continue;
        }
        mapping.insert(current, name.clone());
        taken.insert(name);
    }
    mapping
}

/// The program with the script entry's global names and doc.
pub fn with_script(program: Node, entry: &Node, protected: &HashSet<String>, forced: &HashMap<String, String>) -> Node {
    if program.get("asm").is_some() || !(truthy(entry) || !forced.is_empty()) {
        return program;
    }
    let mapping = global_mapping(&program, entry, protected, forced);
    let functions: Node = list(&program, "functions")
        .iter()
        .map(|function| {
            if function.kind() == "function" {
                function.clone().with("body", renamed_variables(&function["body"], &mapping))
            } else {
                function.clone()
            }
        })
        .collect();
    let globals: Node = list(&program, "globals")
        .iter()
        .map(|declared| {
            let name = declared["name"].as_str().unwrap_or("");
            declared.clone().with("name", mapping.get(name).cloned().unwrap_or_else(|| name.to_string()))
        })
        .collect();
    let mut out = program.with("functions", functions).with("globals", globals);
    if let Some(doc) = entry.get("doc").filter(|doc| truthy(doc)) {
        out = out.with("doc", doc.clone());
    }
    out
}

/// The globals (g<N>) the functions use.
pub fn global_uses(functions: &[Node]) -> HashSet<String> {
    let mut out = HashSet::new();
    for function in functions.iter().filter(|function| function.kind() == "function") {
        for name in variables(&function["body"]) {
            if name.len() > 1 && name.starts_with('g') && name[1..].bytes().all(|b| b.is_ascii_digit()) {
                out.insert(name);
            }
        }
    }
    out
}

/// The program with the entries' names: functions, parameters, locals; unchanged when any of it
/// is assembly.
pub fn apply(
    program: Node,
    entries: &Lookup,
    reserved: &HashSet<String>,
    program_names: &HashMap<i64, String>,
) -> Result<Node> {
    if program.get("asm").is_some() || list(&program, "functions").iter().any(|function| function.kind() == "asm") {
        return Ok(program);
    }
    let found = fingerprints(&program);
    // the names entries give this program's functions, where they are not the SDK's
    let class_names: HashMap<String, String> = found
        .values()
        .flatten()
        .filter_map(|print| {
            let name = entries.get(print)?.get("name")?.as_str()?;
            (!reserved.contains(name)).then(|| (print.clone(), name.to_string()))
        })
        .collect();
    let order: Vec<Option<String>> = list(&program, "functions")
        .iter()
        .map(|function| found.get(function["name"].as_str().unwrap_or("")).cloned().flatten())
        .collect();
    let mapping = assign(&program, &found, &class_names, reserved, program_names);
    let renamed = rename(program, &mapping)?;
    Ok(described(renamed, &order, entries, reserved))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn function(name: &str, calls: &[&str]) -> Node {
        let body: Vec<Node> = calls.iter().map(|callee| node!({"kind": "call", "name": *callee, "args": []})).collect();
        node!({"name": name, "kind": "function", "body": body})
    }

    fn stubs(count: usize, other: impl Fn(usize) -> bool) -> IndexMap<String, Option<String>> {
        (0..count)
            .map(|at| (format!("f_{at}"), Some(if other(at) { format!("p{at}") } else { "stub".to_string() })))
            .collect()
    }

    #[test]
    fn later_copies_are_named_for_their_caller() {
        let program = node!({
            "functions": [
                function("f_0", &[]),
                function("f_1", &[]),
                function("f_2", &[]),
                function("f_3", &[]),
                function("f_4", &["f_1"]),
                function("f_5", &["f_2", "f_3"]),
                function("f_6", &["f_3"]),
            ],
            "programs": [{"no": 100, "function": "f_5"}],
            "main": "f_6",
        });
        let found = stubs(7, |at| at >= 4);
        let class_names = HashMap::from([
            ("stub".to_string(), "do_nothing".to_string()),
            ("p4".to_string(), "recoil".to_string()),
            ("p5".to_string(), "on_damage".to_string()),
        ]);
        let program_names = HashMap::from([(100, "damage".to_string())]);
        let mapping = assign(&program, &found, &class_names, &HashSet::new(), &program_names);
        assert_eq!(mapping["f_0"], "do_nothing");
        assert_eq!(mapping["f_1"], "do_nothing_for_recoil");
        assert_eq!(mapping["f_2"], "do_nothing_for_damage");
        // called from two functions: the number stays
        assert_eq!(mapping["f_3"], "do_nothing_4");
        assert_eq!(mapping["f_6"], "start");
    }

    #[test]
    fn copies_nothing_calls_are_unused() {
        let program = node!({
            "functions": [function("f_0", &[]), function("f_1", &[]), function("f_2", &[]), function("f_3", &[])],
            "programs": [{"no": 200, "function": "f_3"}],
            "main": "f_0",
        });
        let class_names = HashMap::from([("stub".to_string(), "do_nothing".to_string())]);
        let mapping = assign(&program, &stubs(4, |_| false), &class_names, &HashSet::new(), &HashMap::new());
        assert_eq!(mapping["f_0"], "do_nothing");
        assert_eq!(mapping["f_1"], "unused_do_nothing");
        assert_eq!(mapping["f_2"], "unused_do_nothing_2");
        // an entry point is named for its program, as before
        assert_eq!(mapping["f_3"], "do_nothing_for_program_200");
    }
}
