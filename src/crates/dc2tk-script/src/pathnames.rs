//! Readable names in a decompiled tree. A folder or script the game names with a code keeps the
//! code and gains a name after it: map/s/s51_raybrandt_castle/s51_1.c,
//! dungeon/monster/e33a_golem.c, lib/map/s/s51_raybrandt_castle/... The game's own names contain
//! underscores too (s06b_1, d01_sfida_0), so a name cannot be stripped off by rule: the tree
//! records, in UNNAMED, the path each renamed file would have without names, which is its disc
//! path for a script. Tools that need a script's disc path (build, defs learn, xref, build
//! --start-at) read it from there.
//!
//! Names come from the disc in the definition set's language: map folders from the map table
//! (map/map<language>.cfg, by the folder's map code: s51, or i01h01 for map/i/i01/h01), map
//! scripts outside a map folder the same way, and monster AI scripts from the monster table (the
//! kind whose model is the script, else the lowest kind that runs it). A definition set's
//! paths.json names the rest and wins over the disc.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use indexmap::IndexMap;
use regex::Regex;
use unicode_normalization::UnicodeNormalization;

use crate::definitions::Definitions;
use crate::disc;
use crate::messages::{self, NameKey};
use crate::source::EXTENSION;
use crate::{Error, Result};

pub const UNNAMED: &str = "unnamed-paths.json";
const MAX_NAME: usize = 40;

/// A name as a path part: lower-case words joined with "_", accents folded away.
pub fn slug(name: &str) -> String {
    let plain: String = name.nfkd().filter(char::is_ascii).collect::<String>().replace('\'', "");
    let words: Vec<String> =
        Regex::new("[A-Za-z0-9]+").unwrap().find_iter(&plain).map(|word| word.as_str().to_lowercase()).collect();
    let joined = words.join("_");
    joined[..joined.len().min(MAX_NAME)].trim_end_matches('_').to_string()
}

/// The names a disc and definition set give, for naming a tree decompiled from them.
pub struct Lookups {
    pub maps: HashMap<String, String>,
    pub ais: HashMap<String, String>,
    pub titles: HashMap<String, String>,
}

pub fn lookups(root: &Path, defs: &Definitions) -> Lookups {
    let mut maps = HashMap::new();
    if let Some(code) = disc::language_code(defs) {
        if let Some(table) = messages::disc_names(root, code).get("map_id") {
            for (key, name) in table {
                if let NameKey::Text(key) = key {
                    maps.insert(key.clone(), name.clone());
                }
            }
        }
    }
    let layout = defs.executable.get("monster_table").filter(|layout| crate::source::truthy(layout));
    let elf =
        defs.executable.get("name").and_then(|name| name.as_str()).and_then(|name| std::fs::read(root.join(name)).ok());
    let rows = match (elf, layout) {
        (Some(elf), Some(layout)) => disc::monster_table(&elf, layout),
        _ => IndexMap::new(),
    };
    let titles =
        defs.paths.iter().filter_map(|(path, name)| Some((path.clone(), name.as_str()?.to_string()))).collect();
    Lookups { maps, ais: ai_names(&rows), titles }
}

/// Each AI script's name: the kind whose model is the script, else the lowest kind that runs it.
pub fn ai_names(rows: &IndexMap<i64, disc::MonsterRow>) -> HashMap<String, String> {
    let sorted: BTreeMap<i64, &disc::MonsterRow> = rows.iter().map(|(kind, row)| (*kind, row)).collect();
    let mut ais: HashMap<String, (bool, String)> = HashMap::new();
    for row in sorted.values() {
        let script = if row.script.is_empty() { row.model.clone() } else { row.script.clone() };
        let own = row.model == script;
        if !row.name.is_empty() && ais.get(&script).is_none_or(|(model, _)| own && !model) {
            ais.insert(script, (own, row.name.clone()));
        }
    }
    ais.into_iter().map(|(script, (_, name))| (script, name)).collect()
}

fn map_code(parts: &[&str], index: usize) -> Option<String> {
    (parts[0] == "map" && (2..=3).contains(&index)).then(|| parts[2..=index].concat())
}

fn with_name(code: &str, name: Option<&String>) -> String {
    let text = name.map(|name| slug(name)).unwrap_or_default();
    if !text.is_empty() && text != code.to_lowercase() {
        format!("{code}_{text}")
    } else {
        code.to_string()
    }
}

/// The named path of a disc script, given as the tree path it has without names
/// (map/s/s51/s51_1.c).
pub fn named_script(path: &str, found: &Lookups) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    let mut out = Vec::new();
    for (index, part) in parts[..parts.len() - 1].iter().enumerate() {
        let mut name = found.titles.get(&parts[..=index].join("/"));
        let code = map_code(&parts, index);
        if name.is_none() {
            if let Some(code) = &code {
                name = found.maps.get(code);
            }
        }
        out.push(with_name(part, name));
    }
    let last = parts[parts.len() - 1];
    let stem = &last[..last.len() - EXTENSION.len()];
    let mut key: Vec<String> = parts[..parts.len() - 1].iter().map(|part| part.to_string()).collect();
    key.push(format!("{stem}.stb"));
    let mut name = found.titles.get(&key.join("/"));
    if name.is_none() && parts[0] == "map" && parts.len() <= 3 {
        name = found.maps.get(stem);
    }
    if name.is_none() && parts.len() == 3 && parts[..2] == ["dungeon", "monster"] {
        name = found.ais.get(stem);
    }
    out.push(with_name(stem, name) + EXTENSION);
    out.join("/")
}

/// A library's path names its folders the way the scripts it serves are named
/// (lib/map/s/s51/x.c becomes lib/map/s/s51_raybrandt_castle/x.c); its file name is the
/// library's own.
pub fn named_library(path: &str, found: &Lookups) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    if parts[0] != "lib" || parts.len() < 3 {
        return path.to_string();
    }
    let mut inner: Vec<&str> = parts[1..parts.len() - 1].to_vec();
    let placeholder = format!("placeholder{EXTENSION}");
    inner.push(&placeholder);
    let named = named_script(&inner.join("/"), found);
    let mut folders: Vec<&str> = named.split('/').collect();
    folders.pop();
    std::iter::once("lib").chain(folders).chain(std::iter::once(parts[parts.len() - 1])).collect::<Vec<_>>().join("/")
}

fn read_paths(path: &Path) -> Result<IndexMap<String, String>> {
    if !path.is_file() {
        return Ok(IndexMap::new());
    }
    let value: serde_json::Value = serde_json::from_str(&dc2tk_disc::textfile::read_text(path)?)
        .map_err(|error| Error::invalid(format!("{}: {error}", path.display())))?;
    Ok(value["paths"]
        .as_object()
        .map(|paths| paths.iter().filter_map(|(key, value)| Some((key.clone(), value.as_str()?.to_string()))).collect())
        .unwrap_or_default())
}

/// Records the paths files would have without names, keeping what the tree recorded before.
pub fn record(tree: &Path, renamed: &IndexMap<String, String>) -> Result<()> {
    let path = tree.join(UNNAMED);
    let mut known: BTreeMap<String, String> = read_paths(&path)?.into_iter().collect();
    for (unnamed, named) in renamed {
        if named != unnamed {
            known.insert(named.clone(), unnamed.clone());
        }
    }
    let note =
        "The path each renamed file would have without readable names; for a script, its disc path. Do not edit.";
    let value = serde_json::json!({"note": note, "paths": known});
    dc2tk_disc::textfile::write_text(&path, &(dc2tk_disc::textfile::json_text(&value)))?;
    Ok(())
}

/// What a tree records about its names: the unnamed path of each renamed file, the same the
/// other way round, and the folder renames those imply, both ways.
pub struct Tables {
    pub unnamed: IndexMap<String, String>,
    pub named: HashMap<String, String>,
    pub named_folders: HashMap<String, String>,
    pub unnamed_folders: HashMap<String, String>,
}

/// A path's folders, nearest first (Path.parents, without ".").
fn parents(path: &str) -> Vec<String> {
    let parts: Vec<&str> = path.split('/').collect();
    (1..parts.len()).rev().map(|cut| parts[..cut].join("/")).collect()
}

fn build_tables(path: &Path) -> Result<Tables> {
    let unnamed = read_paths(path)?;
    let mut named_folders = HashMap::new();
    for (named_path, plain) in &unnamed {
        for (plain_folder, named_folder) in parents(plain).into_iter().zip(parents(named_path)) {
            named_folders.insert(plain_folder, named_folder);
        }
    }
    Ok(Tables {
        named: unnamed.iter().map(|(named_path, plain)| (plain.clone(), named_path.clone())).collect(),
        unnamed_folders: named_folders.iter().map(|(plain, named)| (named.clone(), plain.clone())).collect(),
        named_folders,
        unnamed,
    })
}

type Cache = Mutex<HashMap<(String, u128), Arc<Tables>>>;

/// The tree's tables, read once per version of its record.
pub fn tree_tables(tree: &Path) -> Result<Arc<Tables>> {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let path = tree.join(UNNAMED);
    let modified = std::fs::metadata(&path)
        .and_then(|meta| meta.modified())
        .map(|time| time.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos())
        .unwrap_or(0);
    let key = (crate::definitions::resolved_posix(tree), modified);
    let cache = CACHE.get_or_init(Default::default);
    if let Some(found) = cache.lock().unwrap().get(&key) {
        return Ok(found.clone());
    }
    let found = Arc::new(build_tables(&path)?);
    cache.lock().unwrap().insert(key, found.clone());
    Ok(found)
}

fn moved(path: &str, renames: &HashMap<String, String>) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    for cut in (1..parts.len()).rev() {
        if let Some(to) = renames.get(&parts[..cut].join("/")) {
            return std::iter::once(to.as_str()).chain(parts[cut..].iter().copied()).collect::<Vec<_>>().join("/");
        }
    }
    path.to_string()
}

/// The path a tree file would have without names (its disc path, for a script). A file the
/// record does not list, such as one a mod added, is placed by its folders.
pub fn unnamed(tree: &Path, path: &str) -> Result<String> {
    let found = tree_tables(tree)?;
    Ok(found.unnamed.get(path).cloned().unwrap_or_else(|| moved(path, &found.unnamed_folders)))
}

/// The tree path of a file given its path without names.
pub fn named(tree: &Path, path: &str) -> Result<String> {
    let found = tree_tables(tree)?;
    Ok(found.named.get(path).cloned().unwrap_or_else(|| moved(path, &found.named_folders)))
}
