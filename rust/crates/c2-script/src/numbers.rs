//! Numbers a mod asks for by name instead of choosing them: NEW_FLAG(name) for a save flag,
//! NEW_PROGRAM(name) for a map program number and NEW_MONSTER_VAR(name) for a monster variable
//! every monster on a floor shares. build gives each name a number that nothing on the disc, in the
//! engine or in the chosen mods' own code uses, so mods built together never share one.
//!
//! The numbers are kept in LOCK in the mods folder, so a name keeps its number from build to
//! build: a flag is saved with the game, and a save made with one set of mods must mean the same
//! after another mod is added. A number stays reserved while its mod is left out of a build, so it
//! is never handed to another mod's name; delete its line from the file to free it.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::path::Path;
use std::sync::OnceLock;

use indexmap::IndexMap;
use regex::Regex;

use crate::index::{self, Folded};
use crate::{flags, Error, Result};

pub const LOCK: &str = "numbers.json";
pub const KINDS: [&str; 3] = ["flag", "program", "monster_var"];

/// Where each kind's numbers come from. Flags start above every flag the disc and the engine use
/// (they stop at 824) with room to spare; 2047 is build --start-at's. Program numbers are the ones
/// cli.py map offers mods. Monster variables 8 to 135 are shared by every monster on a floor.
pub fn pool(kind: &str) -> Range<i64> {
    match kind {
        "flag" => 1024..flags::FLAG_COUNT - 1,
        "program" => 9000..10000,
        _ => 8..136,
    }
}

pub fn word(kind: &str) -> &'static str {
    match kind {
        "flag" => "save flag",
        "program" => "program",
        _ => "monster variable",
    }
}

fn declaration() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| Regex::new(r"(?m)^[ \t]*NEW_(FLAG|PROGRAM|MONSTER_VAR)\(\s*([A-Za-z_]\w*)\s*\)\s*;").unwrap())
}

/// Calls whose first argument is a number of the kind: what a mod or the tree may have written by
/// hand.
fn calls(kind: &str) -> Vec<&'static str> {
    match kind {
        "flag" => flags::FLAG_FUNCTIONS.iter().map(|(name, _)| *name).collect(),
        "program" => vec!["run_event_script", "queue_dungeon_event", "load_event_script"],
        _ => vec!["set_monster_var", "get_monster_var", "set_monster_var2", "get_monster_var2"],
    }
}

fn literal_calls(text: &str, names: &[&str]) -> BTreeSet<i64> {
    let alternatives: Vec<String> = names.iter().map(|name| regex::escape(name)).collect();
    let pattern = Regex::new(&format!(r#"\b(?:{})\s*\(\s*(?:"[^"]*"\s*,\s*)?(\d+)\s*[,)]"#, alternatives.join("|"))).unwrap();
    pattern.captures_iter(&index::without_comments(text)).filter_map(|found| found[1].parse().ok()).collect()
}

/// The names a mod's files ask numbers for: {kind: {name}}.
pub fn requests<'a>(texts: impl IntoIterator<Item = &'a String>) -> BTreeMap<String, BTreeSet<String>> {
    let mut found: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for text in texts {
        for capture in declaration().captures_iter(&index::without_comments(text)) {
            found.entry(capture[1].to_lowercase()).or_default().insert(capture[2].to_string());
        }
    }
    found
}

/// {kind: {number}}.
pub type Used = BTreeMap<String, BTreeSet<i64>>;

fn empty_used() -> Used {
    KINDS.iter().map(|kind| (kind.to_string(), BTreeSet::new())).collect()
}

/// Numbers written by hand in texts (mods' files).
pub fn written<'a>(texts: impl IntoIterator<Item = &'a String>) -> Used {
    static PROGRAM: OnceLock<Regex> = OnceLock::new();
    let literal_program = PROGRAM.get_or_init(|| Regex::new(r"(?m)^PROGRAM\(\s*(\d+)\s*,").unwrap());
    let mut found = empty_used();
    for text in texts {
        for kind in KINDS {
            found.get_mut(kind).unwrap().extend(literal_calls(text, &calls(kind)));
        }
        found.get_mut("program").unwrap().extend(literal_program.captures_iter(text).filter_map(|found| found[1].parse::<i64>().ok()));
    }
    found
}

/// Numbers the tree, the engine and these tools use.
pub fn used_by_tree(tree: &Path, consts: &IndexMap<String, i64>, kinds: &BTreeSet<String>) -> Result<Used> {
    let mut found = empty_used();
    if kinds.contains("flag") {
        found.insert("flag".into(), flags::used_numbers(&flags::scan(tree, consts)?));
    }
    if kinds.contains("program") {
        for outline in index::outlines(tree)?.values() {
            for (number, _) in &outline.programs {
                if let Some(Folded::Int(value)) = index::value(number, consts) {
                    found.get_mut("program").unwrap().insert(value as i64);
                }
            }
        }
    }
    if kinds.contains("monster_var") {
        let names: BTreeSet<String> = calls("monster_var").iter().map(|name| name.to_string()).collect();
        for call in index::calls(tree, &names)? {
            if let Some(Some(Folded::Int(value))) = call.args.first().map(|arg| index::value(arg, consts)) {
                found.get_mut("monster_var").unwrap().insert(value as i64);
            }
        }
    }
    Ok(found)
}

/// {kind: {"mod/name": number}}.
pub type Lock = BTreeMap<String, IndexMap<String, i64>>;

pub fn read_lock(path: Option<&Path>) -> Result<Lock> {
    let value: serde_json::Value = match path.filter(|path| path.is_file()) {
        Some(path) => serde_json::from_str(&c2_py::read_text(path)?).map_err(|error| Error::invalid(format!("{}: {error}", path.display())))?,
        None => serde_json::Value::Null,
    };
    Ok(KINDS
        .iter()
        .map(|kind| {
            let held = value[*kind]
                .as_object()
                .map(|entries| entries.iter().filter_map(|(key, number)| Some((key.clone(), number.as_i64()?))).collect())
                .unwrap_or_default();
            (kind.to_string(), held)
        })
        .collect())
}

pub fn write_lock(path: &Path, lock: &Lock) -> Result<()> {
    let note = "Numbers build gave mods' NEW_FLAG, NEW_PROGRAM and NEW_MONSTER_VAR names, kept so a name keeps its number and \
                saves stay valid. Delete a line to free its number.";
    let mut body = serde_json::Map::new();
    for kind in KINDS {
        if !lock[kind].is_empty() {
            let sorted: BTreeMap<&String, &i64> = lock[kind].iter().collect();
            body.insert(kind.to_string(), serde_json::to_value(sorted).unwrap());
        }
    }
    body.insert("note".into(), note.into());
    let text = c2_py::json::dumps(&serde_json::Value::Object(body), &c2_py::json::Options::indented(1)) + "\n";
    c2_py::write_text(path, &text)?;
    Ok(())
}

/// {mod: {kind: {name}}}: what each mod asks for.
pub type Wanted = BTreeMap<String, BTreeMap<String, BTreeSet<String>>>;

/// Gives every (mod, kind, name) asked for a number: the one the lock holds, or the lowest free
/// one, which is added to the lock. used holds the numbers nothing may be given. Returns ({mod:
/// {name: number}}, whether the lock changed).
pub fn assign(wanted: &Wanted, lock: &mut Lock, used: &Used) -> Result<(BTreeMap<String, IndexMap<String, i64>>, bool)> {
    let mut result: BTreeMap<String, IndexMap<String, i64>> = BTreeMap::new();
    let mut changed = false;
    for (module, kinds) in wanted {
        for (kind, names) in kinds {
            let held = lock.get_mut(kind).expect("a known kind");
            for name in names {
                let key = format!("{module}/{name}");
                let mut number = held.get(&key).copied();
                if let Some(given) = number {
                    if used[kind].contains(&given) {
                        return Err(Error::invalid(format!(
                            "{} {} of mod {} has number {} in {}, which something else now uses; delete its line to give it \
                             another (a save that set it will then mean nothing)",
                            word(kind),
                            name,
                            module,
                            given,
                            LOCK
                        )));
                    }
                }
                if number.is_none() {
                    let taken: BTreeSet<i64> = used[kind].iter().copied().chain(held.values().copied()).collect();
                    let Some(free) = pool(kind).find(|candidate| !taken.contains(candidate)) else {
                        return Err(Error::invalid(format!(
                            "no {} is left for {} of mod {}; {} holds every free one",
                            word(kind),
                            name,
                            module,
                            LOCK
                        )));
                    };
                    held.insert(key, free);
                    changed = true;
                    number = Some(free);
                }
                result.entry(module.clone()).or_default().insert(name.clone(), number.unwrap());
            }
        }
    }
    Ok((result, changed))
}
