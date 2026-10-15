//! Learning a definition set from edited source: the names, docs, parameter and local names, value
//! kinds, return types and line notes of each function (by fingerprint, from compiling the
//! source), each script's globals and doc, and the paths, docs and global names of the libraries
//! the set already knows.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use indexmap::IndexMap;
use regex::Regex;

use crate::natives::Table;
use crate::node;
use crate::node::{Map, Node};
use crate::source::truthy;
use crate::{
    asm, compile, decompile, definitions, fingerprint, library, names, natives, parse, pathnames, sdk, stb, Error,
    Result,
};

static DEFAULT_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(f_[0-9a-f]+|func_\d+|program_\d+|shared_\d+|start|[a-z0-9_]+_\d+_\d+)$").unwrap());
static DEFAULT_VARIABLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[av]\d+$").unwrap());
static DEFAULT_GLOBAL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^g\d+$").unwrap());
static ALIAS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(.+?)(?:_for_program_\d+|_\d+)$").unwrap());

const BASE_TYPES: [&str; 3] = ["int", "float", "string"];

/// A name the decompiler would have given anyway: f_04d4, func_3, start, on_<a program's name>.
pub fn is_default_name(name: &str, program_names: &IndexMap<String, i64>) -> bool {
    DEFAULT_NAME.is_match(name) || name.strip_prefix("on_").is_some_and(|rest| program_names.contains_key(rest))
}

fn names_of(variables: &[Node]) -> Vec<Node> {
    variables.iter().map(|variable| variable["name"].clone()).collect()
}

fn any_named(names: &[Node], default: &Regex) -> bool {
    names.iter().any(|name| !default.is_match(name.as_str().unwrap_or("")))
}

/// What a source function says that the decompiler would not: its value kinds, a return type it
/// would not choose (inferred is what it chose), its name, its parameters' and locals' names, its
/// doc.
pub fn learned_entry(function: &Node, program_names: &IndexMap<String, i64>, inferred: Option<&Node>) -> Node {
    let mut entry = Map::new();
    let variables: Vec<&Node> = function.list("params").iter().chain(function.list("locals")).collect();
    let mut types = Map::new();
    for variable in variables {
        let kind = &variable["type"];
        if !kind.as_str().is_some_and(|kind| BASE_TYPES.contains(&kind)) {
            types.insert(variable.text("name").to_string(), kind.clone());
        }
    }
    if !types.is_empty() {
        entry.insert("types", Node::Map(types));
    }
    if let Some(inferred) = inferred {
        let returns = &function["returns"];
        if returns.as_str().is_some_and(|kind| ["void", "int", "float"].contains(&kind))
            && !returns.same_value(inferred)
        {
            entry.insert("returns", returns.clone());
        }
    }
    let name = function.text("name");
    if !is_default_name(name, program_names) {
        entry.insert("name", function["name"].clone());
    }
    let params = names_of(function.list("params"));
    if any_named(&params, &DEFAULT_VARIABLE) {
        entry.insert("params", Node::List(params));
    }
    let locals = names_of(function.list("locals"));
    if any_named(&locals, &DEFAULT_VARIABLE) {
        entry.insert("locals", Node::List(locals));
    }
    if function.get("doc").is_some_and(truthy) {
        entry.insert("doc", function["doc"].clone());
    }
    Node::Map(entry)
}

/// A script's own globals' names, when it names any, and its doc.
pub fn script_entry(program: &Node) -> Node {
    let mut entry = Map::new();
    let globals = names_of(program.list("globals"));
    if any_named(&globals, &DEFAULT_GLOBAL) {
        entry.insert("globals", Node::List(globals));
    }
    if program.get("doc").is_some_and(truthy) {
        entry.insert("doc", program["doc"].clone());
    }
    Node::Map(entry)
}

fn is_empty(node: &Node) -> bool {
    node.as_object().is_none_or(Map::is_empty)
}

/// What one source file teaches: its native table (None for an assembly file), its functions'
/// entries by exact fingerprint, its script entry and its libraries' entries.
pub struct ScriptEntries {
    pub table: Option<String>,
    pub entries: IndexMap<String, Node>,
    pub script: Node,
    pub libraries: IndexMap<String, Node>,
}

/// What a source file teaches. headers are the SDK's and the sets'; rules pick the native table of
/// a file that names none; sdk_dir is where its includes are looked for last.
pub fn script_entries(
    path: &Path,
    tables: &BTreeMap<String, Table>,
    headers: &IndexMap<String, String>,
    rules: &[(String, String)],
    sdk_dir: &Path,
) -> Result<ScriptEntries> {
    let program = parse::parse(&dc2tk_py::read_text(path)?)?;
    if program.get("asm").is_some() {
        return Ok(ScriptEntries {
            table: None,
            entries: IndexMap::new(),
            script: Node::Map(Map::new()),
            libraries: IndexMap::new(),
        });
    }
    let mut all_headers = headers.clone();
    all_headers.extend(definitions::source_headers(path)?);
    let includes = library::read_includes(&program, path, None, sdk_dir, &[])?;
    let mut expanded = library::expand(program.clone(), &includes, &all_headers)?;
    let table = match expanded["natives"].as_str().filter(|table| !table.is_empty()) {
        Some(table) => table.to_string(),
        None => natives::table_for(&definitions::resolved_posix(path), rules).to_string(),
    };
    expanded = expanded.with("natives", table.as_str());
    let mut notes: IndexMap<String, Node> = IndexMap::new();
    let resolved = sdk::resolve(&expanded, tables)?;
    let data = asm::assemble(&compile::to_asm(&resolved, tables, Some(&mut notes))?, tables)?;
    let script = stb::read(&data);
    let prints = fingerprint::both_fingerprints(&script, &table);
    let found = &prints.exact;
    let decompiled = decompile::decompile(&script, &table, tables);
    let assembly = decompiled.get("asm").is_some();
    let inferred: Vec<Node> = if assembly {
        Vec::new()
    } else {
        decompiled
            .list("functions")
            .iter()
            .filter(|function| matches!(function.kind(), "function" | "asm"))
            .map(|function| function.get("returns").cloned().unwrap_or(Node::Null))
            .collect()
    };
    let context = sdk::context(&sdk::Declarations::of(&expanded["declarations"]), &table, tables)?;
    let functions: Vec<&Node> =
        expanded.list("functions").iter().filter(|function| matches!(function.kind(), "function" | "asm")).collect();
    let mut entries = IndexMap::new();
    let mut seen = HashSet::new();
    for (position, (function, (offset, exact))) in functions.iter().zip(found).enumerate() {
        if function.kind() != "function" {
            continue;
        }
        // A function the file holds twice decompiles once under its name and again under an alias
        // (start_event_for_program_506, unused_name, name_for_caller, name_2); only the first copy
        // says what to call it.
        if !seen.insert(exact.clone()) {
            continue;
        }
        let returned = inferred.get(position).filter(|node| !node.is_null());
        let mut entry = learned_entry(function, &context.programs, returned);
        if let Some(noted) = notes.get(function.text("name")).filter(|noted| truthy(noted)) {
            entry = entry.with("notes", noted.clone());
        }
        if !is_empty(&entry) {
            let loose = prints.loose.get(offset).cloned().unwrap_or_default();
            entries.insert(exact.clone(), entry.with("loose", loose));
        }
    }
    let mut slots = HashMap::new();
    if !assembly && decompiled.list("globals").len() == expanded.list("globals").len() {
        for (declared, shown) in expanded.list("globals").iter().zip(decompiled.list("globals")) {
            slots.insert(declared.text("name").to_string(), shown.text("name").to_string());
        }
    }
    let prints: Vec<String> = found.values().cloned().collect();
    let docs = expanded["library_docs"].as_object().cloned().unwrap_or_default();
    let libraries = library_entries(&functions, &prints, &table, &docs, &slots);
    Ok(ScriptEntries { table: Some(table), entries, script: script_entry(&program), libraries })
}

/// The libraries a script includes, keyed as the decompiler keys them (the fingerprints of the
/// functions each holds, in order), with the path the script includes them by, their doc and the
/// names of the globals they use, by the slot the decompiler names them for (g0). prints are the
/// functions' exact fingerprints in order; slots maps each global's name to that slot name.
pub fn library_entries(
    functions: &[&Node],
    prints: &[String],
    table: &str,
    docs: &Map,
    slots: &HashMap<String, String>,
) -> IndexMap<String, Node> {
    let mut runs: Vec<(String, Vec<String>, Vec<&Node>)> = Vec::new();
    for (function, print) in functions.iter().zip(prints) {
        let Some(path) = function.get("library").and_then(Node::as_str) else {
            continue;
        };
        match runs.last_mut() {
            Some(run) if run.0 == path => {
                run.1.push(print.clone());
                run.2.push(function);
            }
            _ => runs.push((path.to_string(), vec![print.clone()], vec![function])),
        }
    }
    let mut out = IndexMap::new();
    for (path, prints, members) in runs {
        let mut entry = Map::new();
        entry.insert("path", Node::from(path.as_str()));
        if let Some(doc) = docs.get(&path).filter(|doc| truthy(doc)) {
            entry.insert("doc", doc.clone());
        }
        let mut used = HashSet::new();
        for function in &members {
            let own: HashSet<&str> = function
                .list("params")
                .iter()
                .chain(function.list("locals"))
                .map(|variable| variable.text("name"))
                .collect();
            for name in names::variables(&function["body"]) {
                if !own.contains(name.as_str()) {
                    used.insert(name);
                }
            }
        }
        let mut globals: Vec<(&String, &String)> = used
            .iter()
            .filter(|name| !DEFAULT_GLOBAL.is_match(name))
            .filter_map(|name| slots.get(name).map(|slot| (slot, name)))
            .collect();
        globals.sort_by_key(|(slot, _)| slot[1..].parse::<i64>().unwrap_or(0));
        if !globals.is_empty() {
            let globals: Map = globals.into_iter().map(|(slot, name)| (slot.clone(), Node::from(name))).collect();
            entry.insert("globals", Node::Map(globals));
        }
        out.insert(library::library_fingerprint(table, &prints), Node::Map(entry));
    }
    out
}

/// One entry from the copies of a function in several scripts. For each field a copy that differs
/// from what the set already says is an edit and wins; otherwise the first copy's. A name that is
/// another copy's name plus the suffix the decompiler adds when that name is taken in a file
/// (_for_program_506, _2) is an alias, not a name.
pub fn resolved(copies: &[Node], existing: &Node) -> Node {
    let mut fields: Vec<&str> = Vec::new();
    for copy in copies {
        for key in copy.as_object().map(|map| map.keys().collect::<Vec<_>>()).unwrap_or_default() {
            if !fields.contains(&key) {
                fields.push(key);
            }
        }
    }
    let mut out = Map::new();
    for field in fields {
        let mut values: Vec<&Node> = copies.iter().filter_map(|copy| copy.get(field)).collect();
        if field == "name" {
            let plain: Vec<&Node> = values
                .iter()
                .copied()
                .filter(|value| {
                    let alias = value.as_str().and_then(|name| ALIAS.captures(name));
                    !alias.is_some_and(|found| values.iter().any(|other| other.as_str() == Some(&found[1])))
                })
                .collect();
            if !plain.is_empty() {
                values = plain;
            }
        }
        let known = existing.get(field);
        let edited: Vec<&Node> =
            values.iter().copied().filter(|value| !known.is_some_and(|known| value.same_value(known))).collect();
        let chosen = if edited.is_empty() { values[0] } else { edited[0] };
        out.insert(field.to_string(), chosen.clone());
    }
    Node::Map(out)
}

/// The source files to learn from: the file, or every .c of a tree outside its lib/ folder, sorted
/// as Python sorts Windows paths.
pub fn source_files(tree: &Path) -> Vec<PathBuf> {
    if tree.is_file() {
        return vec![tree.to_path_buf()];
    }
    let mut found: Vec<String> = Vec::new();
    let mut folders = vec![tree.to_path_buf()];
    while let Some(folder) = folders.pop() {
        let Ok(listing) = fs::read_dir(&folder) else {
            continue;
        };
        for entry in listing.flatten() {
            let path = entry.path();
            if path.is_dir() {
                folders.push(path);
            } else if path.extension().is_some_and(|ext| {
                let ext = ext.to_string_lossy();
                if cfg!(windows) {
                    ext.eq_ignore_ascii_case("c")
                } else {
                    ext == "c"
                }
            }) {
                found.push(path.strip_prefix(tree).unwrap().to_string_lossy().replace('\\', "/"));
            }
        }
    }
    found.retain(|path| path.split('/').next() != Some("lib"));
    found.sort_by_key(|path| -> Vec<String> {
        let path = if cfg!(windows) { path.to_lowercase() } else { path.clone() };
        path.split('/').map(str::to_string).collect()
    });
    found.into_iter().map(|relative| tree.join(relative)).collect()
}

/// Path.with_suffix(suffix) for a path with "/".
fn with_suffix(path: &str, suffix: &str) -> String {
    let (folder, name) = match path.rsplit_once('/') {
        Some((folder, name)) => (Some(folder), name),
        None => (None, path),
    };
    let stem = match name.rfind('.') {
        Some(at) if at > 0 => &name[..at],
        _ => name,
    };
    match folder {
        Some(folder) => format!("{folder}/{stem}{suffix}"),
        None => format!("{stem}{suffix}"),
    }
}

/// The disc path a learned source file stands for: its name alone when the source is a file, else
/// its path in the tree without readable names.
fn disc_path(path: &Path, source: &Path) -> Result<String> {
    let relative = if source.is_file() {
        path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
    } else {
        let inner = path.strip_prefix(source).unwrap_or(path).to_string_lossy().replace('\\', "/");
        pathnames::unnamed(source, &inner)?
    };
    Ok(with_suffix(&relative, ".stb"))
}

fn read_json(path: &Path) -> Result<Node> {
    let value: serde_json::Value = serde_json::from_str(&dc2tk_py::read_text(path)?)
        .map_err(|error| Error::invalid(format!("{}: {error}", path.display())))?;
    Ok(Node::from(value))
}

fn write_json(path: &Path, value: &Node) -> Result<()> {
    let text = dc2tk_py::json::dumps(value, &dc2tk_py::json::Options::indented(1).unicode()) + "\n";
    Ok(dc2tk_py::write_text(path, &text)?)
}

fn write_script(target: &Path, path: &str, entry: &Node) -> Result<()> {
    let file = target.join("scripts").join(with_suffix(path, ".json"));
    let mut merged = if file.is_file() { read_json(&file)? } else { Node::Map(Map::new()) };
    for (key, value) in entry.as_object().into_iter().flat_map(Map::iter) {
        merged = merged.with(key.to_string(), value.clone());
    }
    fs::create_dir_all(file.parent().unwrap())?;
    write_json(&file, &merged)
}

/// What learn did: functions learned per native table, scripts with globals or docs written,
/// libraries changed, and the files skipped with why.
#[derive(Debug, Default, PartialEq)]
pub struct Learned {
    pub learned: IndexMap<String, usize>,
    pub scripts: usize,
    pub libraries: usize,
    pub failures: Vec<(PathBuf, String)>,
}

/// Records the names and docs an edited source file or tree gives into a definition set.
pub fn learn(
    source: &Path,
    target: &Path,
    tables: &BTreeMap<String, Table>,
    headers: &IndexMap<String, String>,
    rules: &[(String, String)],
    sdk_dir: &Path,
) -> Result<Learned> {
    let mut collected: IndexMap<String, IndexMap<String, Vec<Node>>> = IndexMap::new();
    let mut found_libraries: IndexMap<String, Node> = IndexMap::new();
    let mut result = Learned::default();
    for path in source_files(source) {
        let found = match script_entries(&path, tables, headers, rules, sdk_dir) {
            Ok(found) => found,
            Err(error) if error.is_input() => {
                result.failures.push((path, error.to_string()));
                continue;
            }
            Err(error) => return Err(error),
        };
        if let Some(table) = found.table {
            let copies = collected.entry(table).or_default();
            for (key, entry) in found.entries {
                copies.entry(key).or_default().push(entry);
            }
        }
        if !is_empty(&found.script) {
            write_script(target, &disc_path(&path, source)?, &found.script)?;
            result.scripts += 1;
        }
        for (key, mut entry) in found.libraries {
            // the set keeps library paths under the disc's folder names, as the decompiler makes them
            if source.is_dir() {
                let path = pathnames::unnamed(source, entry.text("path"))?;
                entry = entry.with("path", path);
            }
            found_libraries.entry(key).or_insert(entry);
        }
    }
    let directory = target.join("names");
    fs::create_dir_all(&directory)?;
    let existing = names::load(&directory)?;
    for (table, entries) in &collected {
        let mut merged = existing.get(table).cloned().unwrap_or_default();
        for (key, copies) in entries {
            let known = merged.get(key).cloned().unwrap_or_else(|| Node::Map(Map::new()));
            let mut entry = known.clone();
            for (field, value) in resolved(copies, &known).as_object().into_iter().flat_map(Map::iter) {
                entry = entry.with(field.to_string(), value.clone());
            }
            merged.insert(key.clone(), entry);
        }
        merged.sort_keys();
        let functions: Map = merged.into_iter().collect();
        write_json(&directory.join(format!("{table}.json")), &node!({"functions": Node::Map(functions)}))?;
        result.learned.insert(table.clone(), entries.len());
    }
    result.libraries = write_libraries(target, &found_libraries)?;
    Ok(result)
}

/// Records the path, doc and global names of each library the set already knows (a key the
/// decompiler made), when they differ from what the set says. Returns how many changed.
pub fn write_libraries(target: &Path, found: &IndexMap<String, Node>) -> Result<usize> {
    let file = target.join("libraries.json");
    if !file.is_file() {
        return Ok(0);
    }
    let mut data = read_json(&file)?;
    let mut detached = Node::Map(Map::new());
    let known = match data.as_object_mut().and_then(|map| map.get_mut("libraries")) {
        Some(known) => known,
        None => &mut detached,
    };
    let mut taken: HashSet<Option<String>> = known
        .as_object()
        .into_iter()
        .flat_map(Map::values)
        .map(|entry| entry.get("path").and_then(Node::as_str).map(str::to_string))
        .collect();
    let mut changed = 0;
    for (key, entry) in found {
        let Some(old) = known.get(key).cloned() else {
            continue;
        };
        let mut updated = old.clone();
        let path = entry.text("path").to_string();
        let current = updated.get("path").and_then(Node::as_str).map(str::to_string);
        if current.as_deref() != Some(path.as_str()) && !taken.contains(&Some(path.clone())) {
            taken.remove(&current);
            updated = updated.with("path", path.as_str());
            taken.insert(Some(path));
        }
        if let Some(doc) = entry.get("doc").filter(|doc| truthy(doc)) {
            if !updated.get("doc").is_some_and(|known| known.same_value(doc)) {
                updated = updated.with("doc", doc.clone());
            }
        }
        if let Some(globals) = entry.get("globals").filter(|globals| truthy(globals)) {
            let mut merged = updated.get("globals").cloned().unwrap_or_else(|| Node::Map(Map::new()));
            for (slot, name) in globals.as_object().into_iter().flat_map(Map::iter) {
                merged = merged.with(slot.to_string(), name.clone());
            }
            updated = updated.with("globals", merged);
        }
        if !updated.same_value(&old) {
            known[key.as_str()] = updated;
            changed += 1;
        }
    }
    if changed > 0 {
        write_json(&file, &data)?;
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{decompile, source};

    const EVENT: &str = r#"#pragma natives event

int g0;
float g1;

PROGRAM(100, f_0000);
PROGRAM(200, f_04d4);
MAIN(f_0000);

void f_0000(void)
{
    int v0;
    float v1;
    int v2[4];

    v0 = 3;
    v1 = 0.5;
    v2[v0] = v0 = 2;
    g0 = g0 + 1;
    g1 = sin(v1) * -2.0;
    _GET_FLAG(12, &v0);
    if (v0 == 1 && g0 > 2 || !v0) {
        print("hello", v0);
    } else if (v0 < 0) {
        wait;
    } else {
        skip_end;
    }
    while (1) {
        if (v2[0] >= 4) {
            break;
        }
        if (v2[1] != 4) {
            continue;
        }
        v2[0] = f_04d4(&v2[0], (float)v0) - (v0 - 1);
    }
    end;
}

int f_04d4(int a0[], float a1)
{
    a0[0] = (int)a1 % 7;
    return a0[0] & 1 | 2;
}
"#;

    /// EVENT with its names, and its start documented and named greet_visitor.
    fn documented() -> String {
        EVENT
            .replace("PROGRAM(100, f_0000);", "PROGRAM(100, greet_visitor);")
            .replace("MAIN(f_0000);", "MAIN(greet_visitor);")
            .replace("void f_0000(void)", "// Greets the visitor and counts the visit.\nvoid greet_visitor(void)")
            .replace("f_04d4", "helper")
            .replace("v0", "count")
            .replace("v1", "angle")
            .replace("v2", "slots")
            .replace("g0", "visits")
            .replace("g1", "speed")
            .replace("a0", "values")
            .replace("a1", "scale")
    }

    const TYPED_HEADER: &str = "#include <c2s.h>
#pragma natives event
typedef int music;
enum music_pal {
    MUSIC_TOWN = 22,
};
extern void load_bgm(int file) NATIVE(610);
";

    const TYPED: &str = "#include <event.h>

PROGRAM(100, on_init);
MAIN(on_init);

void on_init(void)
{
    music track;

    track = 22;
    if (track == 22) {
        load_bgm(track);
    }
}
";

    fn tables() -> BTreeMap<String, Table> {
        natives::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../src/natives")).unwrap()
    }

    fn typed_headers() -> IndexMap<String, String> {
        [("event.h", TYPED_HEADER), ("c2s.h", "#define NATIVE(...)\n")]
            .into_iter()
            .map(|(name, text)| (name.to_string(), text.to_string()))
            .collect()
    }

    /// An empty folder for one test.
    fn folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("dc2tk-learn-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    /// Learns one source text into folder/defs.
    fn learn_text(folder: &Path, file: &str, text: &str, headers: &IndexMap<String, String>) -> Learned {
        let script = folder.join(file);
        fs::write(&script, text).unwrap();
        learn(&script, &folder.join("defs"), &tables(), headers, &[], &folder.join("no-sdk")).unwrap()
    }

    fn learned_entries(folder: &Path) -> names::Entries {
        names::merged(&names::load(&folder.join("defs/names")).unwrap())
    }

    fn build(text: &str, headers: &IndexMap<String, String>) -> Vec<u8> {
        let program = library::expand(parse::parse(text).unwrap(), &IndexMap::new(), headers).unwrap();
        compile::compile_program(&program, &tables()).unwrap()
    }

    /// A script decompiled with the entries' names, as source.
    fn named_source(data: &[u8], entries: names::Entries, found: Option<&sdk::Context>) -> String {
        let program = decompile::decompile(&stb::read(data), "event", &tables());
        let known = names::Known::new(entries);
        let lookup = known.with_loose(&names::loose_map(&program));
        let program_names = found.map(|found| found.program_names.clone()).unwrap_or_default();
        let named = names::apply(program, &lookup, &sdk::reserved(found), &program_names).unwrap();
        let shown = sdk::present(named, found, &HashMap::new(), &[], &mut sdk::Sounds::default());
        source::text(&shown).unwrap()
    }

    fn header_context(headers: &IndexMap<String, String>) -> sdk::Context {
        sdk::header_context("event", headers, &tables(), &[], &|text| parse::parse(text)).unwrap().unwrap()
    }

    fn named(entry: &Node, name: &str) -> bool {
        entry.get("name").and_then(Node::as_str) == Some(name)
    }

    #[test]
    fn learn_records_names_and_docs() {
        let folder = folder("records");
        let result = learn_text(&folder, "greet.c", &documented(), &IndexMap::new());
        assert_eq!(result.learned, IndexMap::from([("event".to_string(), 2)]));
        assert_eq!((result.scripts, result.libraries, result.failures.len()), (1, 0, 0));
        let table = learned_entries(&folder);
        assert!(table.values().all(|entry| entry.get("loose").is_some()));
        let mut entries: Vec<Node> = table
            .values()
            .map(|entry| {
                let mut entry = entry.clone();
                entry.as_object_mut().unwrap().remove("loose");
                entry
            })
            .collect();
        entries.sort_by_key(|entry| entry.text("name").to_string());
        assert!(entries[0].same_value(&node!({"name": "greet_visitor", "locals": ["count", "angle", "slots"],
                                         "doc": "Greets the visitor and counts the visit."})));
        assert!(entries[1].same_value(&node!({"name": "helper", "params": ["values", "scale"]})));
        let data = build(EVENT, &IndexMap::new());
        let shown = named_source(&data, table, None);
        assert!(shown.contains("// Greets the visitor and counts the visit.\nvoid greet_visitor(void)"), "{shown}");
        assert!(shown.contains("int helper(int values[], float scale)"), "{shown}");
        assert_eq!(build(&shown, &IndexMap::new()), data);
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn learn_skips_default_names() {
        let folder = folder("defaults");
        let result = learn_text(&folder, "plain.c", EVENT, &IndexMap::new());
        assert_eq!(result.learned, IndexMap::from([("event".to_string(), 0)]));
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn definition_sets_layer() {
        let folder = folder("layer");
        let (base, layered) = (folder.join("base"), folder.join("mod"));
        for (directory, language, extends) in [(&base, "en", "[]"), (&layered, "de", r#"["../base"]"#)] {
            fs::create_dir_all(directory.join("names")).unwrap();
            let name = directory.file_name().unwrap().to_string_lossy();
            let manifest = format!(
                r#"{{"language": "{language}", "extends": {extends}, "tables": [{{"prefix": "{name}/", "table": "monster"}}]}}"#
            );
            fs::write(directory.join(definitions::MANIFEST), manifest).unwrap();
        }
        fs::write(base.join("names/event.json"), r#"{"functions": {"a": {"name": "one"}, "b": {"name": "two"}}}"#)
            .unwrap();
        fs::write(layered.join("names/event.json"), r#"{"functions": {"b": {"name": "deux"}}}"#).unwrap();
        let defs = definitions::load(std::slice::from_ref(&layered)).unwrap();
        let sets: Vec<String> =
            defs.sets.iter().map(|path| path.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(sets, ["base", "mod"]);
        assert_eq!(defs.language, "de");
        let event = &defs.names["event"];
        assert_eq!(event.keys().collect::<Vec<_>>(), ["a", "b"]);
        assert!(event["a"].same_value(&node!({"name": "one"})) && event["b"].same_value(&node!({"name": "deux"})));
        let rules = [("mod/".to_string(), "monster".to_string()), ("base/".to_string(), "monster".to_string())];
        assert_eq!(defs.tables, rules);
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn names_reach_variants_that_differ_only_in_strings() {
        let folder = folder("variants");
        learn_text(&folder, "greet.c", &documented(), &IndexMap::new());
        let variant = EVENT.replace(r#"print("hello", v0);"#, r#"print("hallo", v0);"#);
        let shown = named_source(&build(&variant, &IndexMap::new()), learned_entries(&folder), None);
        assert!(shown.contains("void greet_visitor(void)"), "{shown}");
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn docs_wrapped_at_a_hyphen_join_without_a_space() {
        let comment =
            "// Pushes backwards while the flip is mid-\n// motion, then stops - briefly.\nvoid f(void)\n{\n}\n";
        let program = parse::parse(comment).unwrap();
        let function = program.list("functions").iter().find(|item| item.kind() == "function").unwrap();
        assert_eq!(function.text("doc"), "Pushes backwards while the flip is mid-motion, then stops - briefly.");
    }

    #[test]
    fn a_handler_takes_the_name_of_the_program_it_handles() {
        let program = node!({"functions": [{"name": "f_0000", "kind": "function"}, {"name": "f_0010", "kind": "function"}],
                             "programs": [{"no": 102, "function": "f_0000"}], "main": "f_0010"});
        let found: IndexMap<String, Option<String>> =
            [("f_0000".to_string(), Some("abc".to_string())), ("f_0010".to_string(), None)].into_iter().collect();
        let class_names = HashMap::from([("abc".to_string(), "on_dodge_shot".to_string())]);
        let program_names = HashMap::from([(101, "dodge_shot".to_string()), (102, "dodge_melee".to_string())]);
        let mapping = names::assign(&program, &found, &class_names, &HashSet::new(), &program_names);
        assert_eq!(mapping["f_0000"], "on_dodge_melee");
    }

    #[test]
    fn types_declared_in_source_come_back_with_their_names() {
        let headers = typed_headers();
        let folder = folder("types");
        learn_text(&folder, "typed.c", TYPED, &headers);
        let entries = learned_entries(&folder);
        let types: Vec<&Node> = entries.values().map(|entry| &entry["types"]).collect();
        assert_eq!(types.len(), 1);
        assert!(types[0].same_value(&node!({"track": "music"})));
        let data = build(TYPED, &headers);
        assert_eq!(data, build(&TYPED.replace("music track", "int track"), &headers));
        let found = header_context(&headers);
        let shown = named_source(&data, entries, Some(&found));
        assert!(shown.contains("    music track;\n"), "{shown}");
        assert!(shown.contains("track = MUSIC_TOWN;") && shown.contains("track == MUSIC_TOWN"), "{shown}");
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_declared_return_type_is_learned_when_it_differs() {
        let headers = typed_headers();
        let source =
            TYPED.replace("void on_init(void)", "float scale(void)\n{\n    return 2;\n}\n\nvoid on_init(void)");
        let folder = folder("returns");
        learn_text(&folder, "typed.c", &source, &headers);
        let entries = learned_entries(&folder);
        let scale: Vec<&Node> =
            entries.values().filter(|entry| named(entry, "scale")).map(|entry| &entry["returns"]).collect();
        assert_eq!(scale, [&Node::from("float")]);
        assert!(entries.values().filter(|entry| named(entry, "on_init")).all(|entry| entry.get("returns").is_none()));
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn line_notes_come_back_above_their_statements() {
        let headers = typed_headers();
        let source =
            TYPED.replace("    if (track == 22) {", "    // note: 22 is the town theme.\n    if (track == 22) {");
        let folder = folder("notes");
        learn_text(&folder, "noted.c", &source, &headers);
        let entries = learned_entries(&folder);
        let notes: Vec<&Node> = entries.values().map(|entry| &entry["notes"]).collect();
        assert_eq!(notes.len(), 1);
        assert!(notes[0].same_value(&node!({"4": "22 is the town theme."})));
        let data = build(&source, &headers);
        let found = header_context(&headers);
        let shown = named_source(&data, entries, Some(&found));
        assert!(shown.contains("    // note: 22 is the town theme.\n    if (track == MUSIC_TOWN) {"), "{shown}");
        assert_eq!(build(&shown, &headers), data);
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn copies_of_a_function_resolve_to_its_edit_not_its_alias() {
        let existing = node!({"name": "start_event", "doc": "Old doc."});
        let copies = vec![
            node!({"name": "start_event", "doc": "Old doc."}),
            node!({"name": "start_event_for_program_506", "doc": "New doc."}),
            node!({"name": "start_event_2"}),
        ];
        assert!(resolved(&copies, &existing).same_value(&node!({"name": "start_event", "doc": "New doc."})));
        let copies = vec![node!({"name": "start_event"}), node!({"name": "begin_event"})];
        assert!(resolved(&copies, &existing).same_value(&node!({"name": "begin_event"})));
    }

    #[test]
    fn libraries_are_keyed_by_the_functions_they_hold() {
        let var = |name: &str| node!({"kind": "expr", "expr": {"kind": "var", "name": name}});
        let function = |name: &str, body: Vec<Node>, path: Option<&str>, locals: &[&str]| {
            let locals: Vec<Node> = locals.iter().map(|local| node!({"name": *local})).collect();
            node!({"name": name, "library": path, "params": [], "body": body, "locals": locals})
        };
        let functions = [
            function("a", vec![var("visits")], None, &[]),
            function("b", vec![var("visits")], Some("lib/x.c"), &[]),
            function("c", vec![var("g1"), var("talks")], Some("lib/x.c"), &["talks"]),
            function("d", vec![], Some("lib/y.c"), &[]),
        ];
        let functions: Vec<&Node> = functions.iter().collect();
        let prints: Vec<String> = ["f0", "f1", "f2", "f3"].iter().map(|print| print.to_string()).collect();
        let docs: Map = [("lib/x.c", Node::from("Shared helpers."))].into_iter().collect();
        // the script's globals, visits and talks, decompile as g0 and g1; talks is a local in c
        let slots: HashMap<String, String> =
            [("visits", "g0"), ("talks", "g1")].iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
        let entries = library_entries(&functions, &prints, "event", &docs, &slots);
        let x = library::library_fingerprint("event", &["f1".into(), "f2".into()]);
        let y = library::library_fingerprint("event", &["f3".into()]);
        assert_eq!(entries.keys().cloned().collect::<Vec<_>>(), vec![x.clone(), y.clone()]);
        assert!(
            entries[&x].same_value(&node!({"path": "lib/x.c", "doc": "Shared helpers.", "globals": {"g0": "visits"}}))
        );
        assert!(entries[&y].same_value(&node!({"path": "lib/y.c"})));
    }

    #[test]
    fn default_names() {
        let programs: IndexMap<String, i64> = [("dodge".to_string(), 101)].into_iter().collect();
        for name in ["f_04d4", "func_3", "program_100", "shared_2", "start", "town_1_2", "on_dodge"] {
            assert!(is_default_name(name, &programs), "{name}");
        }
        for name in ["greet", "on_jump", "start_2", "F_00"] {
            assert!(!is_default_name(name, &programs), "{name}");
        }
    }

    #[test]
    fn suffixes() {
        assert_eq!(with_suffix("map/s/s51.c", ".stb"), "map/s/s51.stb");
        assert_eq!(with_suffix("a.b/c", ".json"), "a.b/c.json");
        assert_eq!(with_suffix("x.y.stb", ".json"), "x.y.json");
    }
}
