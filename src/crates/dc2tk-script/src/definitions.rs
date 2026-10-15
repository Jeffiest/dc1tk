//! Definition sets (definitions/pal): what the tools know about one disc's scripts. A set's
//! definitions.json can extend other sets, which load first; a later set's entries win.

use std::fs;
use std::path::{Path, PathBuf};

use dc2tk_disc::files;
use indexmap::IndexMap;
use serde::Deserialize;

use crate::names::{self, Entries};
use crate::node::{Map, Node};
use crate::{notes, paths, Error, Result};

pub const MANIFEST: &str = "definitions.json";
/// The scripts of the disc a set was made from, by path and hash.
pub const DISC: &str = "disc.json";
/// The event sound bank of each sound set (column 15 of snd2/MAP_index.txt), which load_sound(n)
/// loads: {"2237": "EV_283_000"}.
pub const SOUNDS: &str = "sounds.json";

#[derive(Debug, Default, Deserialize)]
struct Manifest {
    #[serde(default)]
    extends: Vec<String>,
    #[serde(default)]
    tables: Vec<TableRule>,
    language: Option<String>,
    #[serde(default)]
    include: Vec<String>,
    #[serde(default)]
    executable: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    text: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct TableRule {
    prefix: String,
    table: String,
}

fn read_json(path: &Path) -> Result<serde_json::Value> {
    serde_json::from_str(&files::read_to_string(path)?)
        .map_err(|error| Error::invalid(format!("{}: {error}", path.display())))
}

fn manifest(directory: &Path) -> Result<Manifest> {
    let path = directory.join(MANIFEST);
    if !path.is_file() {
        return Ok(Manifest::default());
    }
    serde_json::from_value(read_json(&path)?).map_err(|error| Error::invalid(format!("{}: {error}", path.display())))
}

/// A path made absolute, its symlinks and ".." resolved (as far as it exists), written
/// with "/".
pub fn resolved_posix(path: &Path) -> String {
    let full = fs::canonicalize(path).or_else(|_| std::path::absolute(path)).unwrap_or_else(|_| path.to_path_buf());
    let text = full.to_string_lossy().into_owned();
    let text = if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else {
        text.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(text)
    };
    text.replace('\\', "/")
}

fn ordered_sets(directory: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let directory = PathBuf::from(resolved_posix(directory));
    if out.contains(&directory) {
        return Ok(());
    }
    for parent in manifest(&directory)?.extends {
        ordered_sets(&directory.join(parent), out)?;
    }
    out.push(directory);
    Ok(())
}

/// The sets to load, parents before the sets that extend them.
pub fn sets(directories: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut order = Vec::new();
    for directory in directories {
        ordered_sets(directory, &mut order)?;
    }
    Ok(order)
}

/// (path prefix, native table) rules, a later set's before an earlier one's.
pub fn table_rules(directories: &[PathBuf]) -> Result<Vec<(String, String)>> {
    let mut rules: Vec<(String, String)> = Vec::new();
    for directory in sets(directories)? {
        let mut found: Vec<(String, String)> =
            manifest(&directory)?.tables.into_iter().map(|rule| (rule.prefix, rule.table)).collect();
        found.extend(rules);
        rules = found;
    }
    Ok(rules)
}

/// Everything the loaded sets know.
#[derive(Clone, Debug, Default)]
pub struct Definitions {
    pub sets: Vec<PathBuf>,
    /// Each native table's names, by fingerprint.
    pub names: IndexMap<String, Entries>,
    /// Translations of the disc's strings, by string_key.
    pub strings: IndexMap<String, String>,
    pub tables: Vec<(String, String)>,
    pub language: String,
    pub include: Vec<String>,
    /// Script entries (globals, doc), by disc path.
    pub scripts: IndexMap<String, Node>,
    pub executable: Map,
    pub text: Map,
    pub libraries: IndexMap<String, Node>,
    pub paths: IndexMap<String, Node>,
    pub disc: IndexMap<String, String>,
    pub sound_set_banks: IndexMap<String, String>,
}

impl Definitions {
    /// No definitions (--no-defs).
    pub fn empty() -> Definitions {
        Definitions { language: "en".into(), ..Definitions::default() }
    }
}

/// old with new's keys set over it: a key both have keeps old's place, new's others come after.
fn updated(old: Option<&Node>, new: Node) -> Node {
    match (old.and_then(Node::as_object), new) {
        (Some(old), Node::Map(new)) => {
            let mut out = old.clone();
            for (key, value) in new {
                out.insert(key, value);
            }
            Node::Map(out)
        }
        (_, new) => new,
    }
}

fn object(value: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    match value {
        serde_json::Value::Object(object) => object,
        _ => serde_json::Map::new(),
    }
}

/// Every .json under a folder, sorted by their parts (lowercased on Windows, whose names ignore case).
fn json_files(directory: &Path) -> Result<Vec<String>> {
    let mut found =
        paths::walk(directory, |entry| entry.is_dir || entry.path.extension().is_some_and(|ext| ext == "json"))?;
    paths::sort_by_parts(&mut found);
    Ok(found)
}

/// A set's script entries, by disc path (scripts/map/s/s51/s51_1.json is map/s/s51/s51_1.stb).
fn load_scripts(directory: &Path) -> Result<Vec<(String, Node)>> {
    let mut found = Vec::new();
    for relative in json_files(directory)? {
        let entry = Node::from(read_json(&paths::join_relative(directory, &relative))?);
        let stem = relative.strip_suffix(".json").unwrap_or(&relative);
        found.push((format!("{stem}.stb"), entry));
    }
    Ok(found)
}

/// The sets, merged.
pub fn load(directories: &[PathBuf]) -> Result<Definitions> {
    let mut found = Definitions::empty();
    let order = sets(directories)?;
    for directory in &order {
        let settings = manifest(directory)?;
        found.sets.push(directory.clone());
        if let Some(language) = settings.language {
            found.language = language;
        }
        for (key, value) in settings.executable {
            found.executable.insert(key, Node::from(value));
        }
        for (key, value) in settings.text {
            let value = Node::from(value);
            let merged = if value.as_object().is_some() { updated(found.text.get(&key), value) } else { value };
            found.text.insert(key, merged);
        }
        let mut rules: Vec<(String, String)> =
            settings.tables.into_iter().map(|rule| (rule.prefix, rule.table)).collect();
        rules.append(&mut found.tables);
        found.tables = rules;
        for name in settings.include {
            if !found.include.contains(&name) {
                found.include.push(name);
            }
        }
        for (table, entries) in names::load(&directory.join("names"))? {
            let into = found.names.entry(table).or_default();
            for (print, entry) in entries {
                into.insert(print, entry);
            }
        }
        let paths = directory.join("paths.json");
        if paths.is_file() {
            for (path, name) in object(read_json(&paths)?["names"].take()) {
                found.paths.insert(path, Node::from(name));
            }
        }
        let libraries = directory.join("libraries.json");
        if libraries.is_file() {
            for (key, entry) in object(read_json(&libraries)?["libraries"].take()) {
                let merged = updated(found.libraries.get(&key), Node::from(entry));
                found.libraries.insert(key, merged);
            }
        }
        for (path, entry) in load_scripts(&directory.join("scripts"))? {
            let merged = updated(found.scripts.get(&path), entry);
            found.scripts.insert(path, merged);
        }
        let sounds = directory.join(SOUNDS);
        if sounds.is_file() {
            for (set, bank) in object(read_json(&sounds)?["sound_set_event_banks"].take()) {
                found.sound_set_banks.insert(set, bank.as_str().unwrap_or_default().to_string());
            }
        }
        let disc = directory.join(DISC);
        if disc.is_file() {
            for (path, hash) in object(read_json(&disc)?["scripts"].take()) {
                found.disc.insert(path, hash.as_str().unwrap_or_default().to_string());
            }
        }
    }
    for directory in &order {
        found.strings.extend(notes::load(&directory.join("strings"), &found.language)?);
    }
    Ok(found)
}

/// "x_1.stb" → "x.stb": a map script's language variants share a base.
pub fn variant_base(path: &str) -> String {
    let bytes = path.as_bytes();
    if path.ends_with(".stb")
        && bytes.len() >= 6
        && bytes[bytes.len() - 6] == b'_'
        && bytes[bytes.len() - 5].is_ascii_digit()
    {
        format!("{}.stb", &path[..path.len() - 6])
    } else {
        path.to_string()
    }
}

/// The script entry for a script or its source file: by its disc path, else a language variant's.
pub fn script_entry<'a>(defs: &'a Definitions, path: &str) -> Option<&'a Node> {
    let mut text = path.replace('\\', "/");
    if let Some(stem) = text.strip_suffix(crate::source::EXTENSION) {
        text = format!("{stem}.stb");
    }
    let mut sibling = None;
    let mine = variant_base(&text);
    for (disc_path, entry) in &defs.scripts {
        if text == *disc_path || text.ends_with(&format!("/{disc_path}")) {
            return Some(entry);
        }
        let base = variant_base(disc_path);
        if sibling.is_none() && (mine == base || mine.ends_with(&format!("/{base}"))) {
            sibling = Some(entry);
        }
    }
    sibling
}

/// A script's hash in a set's disc.json: the first 16 hex digits of its SHA-1.
pub fn script_hash(data: &[u8]) -> String {
    sha1_smol::Sha1::from(data).digest().to_string()[..16].to_string()
}

/// How an extracted disc (or a folder of one) differs from the one the sets were made from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiscDifferences {
    pub differ: Vec<String>,
    pub missing: Vec<String>,
    pub unknown: Vec<String>,
}

/// The scripts that differ, that the disc lacks, and that the sets do not know; empty when no set
/// records a disc. prefix is the folder of the disc the scripts are under ("map/s/s51/", or "").
pub fn disc_differences(scripts: &[(String, Vec<u8>)], defs: &Definitions, prefix: &str) -> DiscDifferences {
    if defs.disc.is_empty() {
        return DiscDifferences::default();
    }
    let found: IndexMap<String, String> =
        scripts.iter().map(|(path, data)| (format!("{prefix}{path}"), script_hash(data))).collect();
    let expected: IndexMap<&String, &String> = defs.disc.iter().filter(|(path, _)| path.starts_with(prefix)).collect();
    let mut differ: Vec<String> = found
        .iter()
        .filter(|(path, value)| expected.get(path).is_some_and(|known| known != value))
        .map(|(path, _)| path.clone())
        .collect();
    let mut missing: Vec<String> =
        expected.keys().filter(|path| !found.contains_key(**path)).map(|path| (*path).clone()).collect();
    let mut unknown: Vec<String> = found.keys().filter(|path| !expected.contains_key(path)).cloned().collect();
    differ.sort();
    missing.sort();
    unknown.sort();
    DiscDifferences { differ, missing, unknown }
}

/// The headers in the include folders above a source file (a tree's own disc.h), the nearest
/// folder's winning.
pub fn source_headers(path: &Path) -> Result<IndexMap<String, String>> {
    let full = PathBuf::from(resolved_posix(path));
    let folders: Vec<&Path> = full.ancestors().skip(1).collect();
    let mut texts = IndexMap::new();
    for folder in folders.into_iter().rev() {
        let include = folder.join("include");
        if include.is_dir() {
            texts.extend(crate::sdk::load_texts(&include)?);
        }
    }
    Ok(texts)
}
