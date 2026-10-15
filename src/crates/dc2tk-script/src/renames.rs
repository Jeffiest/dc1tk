//! Overrides that survive renames. A patch overrides a function by name, and a later decompile with
//! better definitions can give the function another. So the fingerprints (fingerprint) of each
//! function a patch overrides are kept in LOCK in the mod, {patch: {function: {exact, loose}}},
//! recorded by mod override, mod check and build when the name is still the script's. When a
//! target no longer has the name, follow finds the function by its exact fingerprint (else by its
//! loose one, as in another language's script) and the patch is merged as if it said the new name;
//! rename writes the new names into the mod's files.
//!
//! A script's fingerprints are those of the bytes the tree compiles it to, which are the disc's
//! for a script nobody edited, so they match the definitions' wherever the decompile names them.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use regex::Regex;
use serde_json::Value;

use crate::compile::Toolchain;
use crate::patches::{self, Plan};
use crate::{definitions, fingerprint, library, mods, natives, parse, pathnames, paths, source, stb, Error, Result};

pub const LOCK: &str = "fingerprints.json";

/// A function's exact and loose fingerprints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Print {
    pub exact: String,
    pub loose: String,
}

/// {patch file, by path from the mod: {function: fingerprints}}.
pub type Recorded = BTreeMap<String, BTreeMap<String, Print>>;

pub fn read_lock(mod_dir: &Path) -> Result<Recorded> {
    let path = mod_dir.join(LOCK);
    let mut found = Recorded::new();
    if !path.is_file() {
        return Ok(found);
    }
    let value: Value = serde_json::from_str(&dc2tk_disc::textfile::read_text(&path)?)
        .map_err(|error| Error::invalid(format!("{}: {error}", path.display())))?;
    for (patch, functions) in value.as_object().into_iter().flatten() {
        for (name, prints) in functions.as_object().into_iter().flatten() {
            if let (Some(exact), Some(loose)) = (prints["exact"].as_str(), prints["loose"].as_str()) {
                let print = Print { exact: exact.into(), loose: loose.into() };
                found.entry(patch.clone()).or_default().insert(name.clone(), print);
            }
        }
    }
    Ok(found)
}

pub fn write_lock(mod_dir: &Path, recorded: &Recorded) -> Result<()> {
    let mut body = serde_json::Map::new();
    body.insert(
        "note".into(),
        "The fingerprints of the functions this mod's patches override, so that build finds a function \
         the decompiler has renamed since. dc2tk writes it."
            .into(),
    );
    for (patch, functions) in recorded.iter().filter(|(_, functions)| !functions.is_empty()) {
        let mut entries = serde_json::Map::new();
        for (name, print) in functions {
            entries.insert(name.clone(), serde_json::json!({"exact": print.exact, "loose": print.loose}));
        }
        body.insert(patch.clone(), Value::Object(entries));
    }
    let text = dc2tk_disc::textfile::json_text(&Value::Object(body));
    dc2tk_disc::textfile::write_text(&mod_dir.join(LOCK), &text)?;
    Ok(())
}

/// The fingerprints of every function of a tree script as build compiles it without patches,
/// by name. overlay holds mod files that stand in for the tree's.
pub fn script_prints(
    tree: &Path,
    script: &str,
    tools: &Toolchain,
    overlay: &IndexMap<String, String>,
) -> Result<IndexMap<String, Print>> {
    let path = tree.join(script);
    let text = match overlay.get(script) {
        Some(text) => text.clone(),
        None => dc2tk_disc::textfile::read_text(&path)?,
    };
    let program = parse::parse(&text)?;
    let mut headers = tools.headers.clone();
    headers.extend(definitions::source_headers(&path)?);
    let includes = library::read_includes(&program, &path, Some(overlay), tools.sdk_dir, &[])?;
    let mut program = library::expand(program, &includes, &headers)?;
    if program.get("asm").is_some() {
        return Ok(IndexMap::new());
    }
    if program["natives"].is_null() {
        program = program.with("natives", natives::table_for(&pathnames::unnamed(tree, script)?, tools.rules));
    }
    let table = program["natives"].plain_text();
    let names: Vec<String> = program["functions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|function| matches!(function.kind(), "function" | "asm"))
        .map(|function| function["name"].plain_text())
        .collect();
    let compiled = stb::read(&crate::compile::compile_program(&program, tools.tables)?);
    let prints = fingerprint::both_fingerprints(&compiled, &table);
    if prints.order.len() != names.len() {
        return Err(Error::invalid(format!(
            "{script}: {} functions compiled from {} in the source",
            prints.order.len(),
            names.len()
        )));
    }
    Ok(names
        .into_iter()
        .zip(&prints.order)
        .map(|(name, offset)| {
            (name, Print { exact: prints.exact[offset].clone(), loose: prints.loose[offset].clone() })
        })
        .collect())
}

/// The names in a script whose code is the recorded function's: those with its exact fingerprint,
/// else those with its loose one.
pub fn matches(prints: &IndexMap<String, Print>, wanted: &Print) -> Vec<String> {
    let exact: Vec<String> =
        prints.iter().filter(|(_, print)| print.exact == wanted.exact).map(|(name, _)| name.clone()).collect();
    if !exact.is_empty() {
        return exact;
    }
    prints.iter().filter(|(_, print)| print.loose == wanted.loose).map(|(name, _)| name.clone()).collect()
}

/// An override whose function the tree has renamed: the mod, its patch (by path from the mod),
/// the name the patch gives and the script's name now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Followed {
    pub module: String,
    pub patch: String,
    pub old: String,
    pub new: String,
}

/// What follow found: the renamed overrides it follows, and notes on those it cannot (the code
/// is in more than one function).
#[derive(Debug, Default)]
pub struct Following {
    pub followed: Vec<Followed>,
    pub notes: Vec<String>,
}

/// Records the fingerprints of the functions the chosen mods' patches override, where their
/// LOCK lacks them, and sets each patch's follow for the overridden names a target no longer has
/// but has the code of under another name. A name found neither way is left for the merge to
/// report.
pub fn follow(
    mod_dirs: &[PathBuf],
    plan: &mut Plan,
    tree: &Path,
    tools: &Toolchain,
    overlay: &IndexMap<String, String>,
) -> Result<Following> {
    let mut found = Following::default();
    let mut cache: HashMap<String, Option<IndexMap<String, Print>>> = HashMap::new();
    let mut prints_of = |script: &str| -> Option<IndexMap<String, Print>> {
        // a script that does not compile has its errors reported by the build itself
        cache.entry(script.to_string()).or_insert_with(|| script_prints(tree, script, tools, overlay).ok()).clone()
    };
    for mod_dir in mod_dirs {
        let module = paths::file_name(mod_dir);
        let mut recorded = read_lock(mod_dir)?;
        let before = recorded.clone();
        for patch in plan.patches.iter_mut().filter(|patch| patch.module == module) {
            let names = patches::overridden(&patch.text);
            if names.is_empty() {
                continue;
            }
            let relative = patch.path.strip_prefix(mod_dir).unwrap_or(&patch.path).to_string_lossy().replace('\\', "/");
            let mine = recorded.entry(relative.clone()).or_default();
            let mut renamed: IndexMap<String, Vec<String>> = IndexMap::new();
            for script in patch.scripts.clone() {
                let present = patches::function_texts(tree, &script, &names)?;
                for name in names.iter().filter(|name| !present.contains_key(*name)) {
                    let Some(wanted) = mine.get(name) else {
                        continue;
                    };
                    let Some(prints) = prints_of(&script) else {
                        continue;
                    };
                    if prints.contains_key(name) {
                        continue;
                    }
                    let candidates = matches(&prints, wanted);
                    match candidates.as_slice() {
                        [] => {}
                        [new] => {
                            patch.follow.entry(script.clone()).or_default().insert(name.clone(), new.clone());
                            let seen = renamed.entry(name.clone()).or_default();
                            if !seen.contains(new) {
                                seen.push(new.clone());
                            }
                        }
                        several => found.notes.push(format!(
                            "{relative} (mod {module}): {script} has no {name}; its code is in {}, so put the one meant \
                             after #pragma override",
                            several.join(", ")
                        )),
                    }
                }
            }
            for (old, news) in renamed {
                for new in news {
                    found.followed.push(Followed {
                        module: module.clone(),
                        patch: relative.clone(),
                        old: old.clone(),
                        new,
                    });
                }
            }
            let unrecorded: Vec<&String> = names.iter().filter(|name| !mine.contains_key(*name)).collect();
            if unrecorded.is_empty() || patch.scripts.is_empty() {
                continue;
            }
            let model = patch
                .scripts
                .iter()
                .find(|script| patch.plain[*script].ends_with(&format!("_1{}", source::EXTENSION)))
                .unwrap_or(&patch.scripts[0])
                .clone();
            if let Some(prints) = prints_of(&model) {
                for name in unrecorded {
                    if let Some(print) = prints.get(name) {
                        mine.insert(name.clone(), print.clone());
                    }
                }
            }
        }
        recorded.retain(|_, functions| !functions.is_empty());
        if recorded != before {
            write_lock(mod_dir, &recorded)?;
        }
    }
    Ok(found)
}

/// One line for a followed override.
pub fn describe(followed: &Followed) -> String {
    format!(
        "{} (mod {}) overrides {}, which the tree now calls {} (same fingerprint); merged as {}",
        followed.patch, followed.module, followed.old, followed.new, followed.new
    )
}

/// Writes the new names of followed overrides into the mod's own source files (every whole word
/// of the old name) and moves their fingerprints to the new names. Returns the files changed.
pub fn rename(mod_dir: &Path, followed: &[Followed]) -> Result<Vec<PathBuf>> {
    let module = paths::file_name(mod_dir);
    let mine: Vec<&Followed> = followed.iter().filter(|item| item.module == module).collect();
    let mut changed = Vec::new();
    if mine.is_empty() {
        return Ok(changed);
    }
    // an old name the code of two functions now has is not renamed; the note says which
    let mut targets: HashMap<&str, Vec<&str>> = HashMap::new();
    for item in &mine {
        let news = targets.entry(item.old.as_str()).or_default();
        if !news.contains(&item.new.as_str()) {
            news.push(item.new.as_str());
        }
    }
    targets.retain(|_, news| news.len() == 1);
    for (path, text) in mods::sources(mod_dir)? {
        let mut out = text.clone();
        for (old, news) in &targets {
            let word = Regex::new(&format!(r"\b{}\b", regex::escape(old))).unwrap();
            out = word.replace_all(&out, news[0]).into_owned();
        }
        if out != text {
            dc2tk_disc::textfile::write_text(&path, &out)?;
            changed.push(path);
        }
    }
    let mut recorded = read_lock(mod_dir)?;
    for item in &mine {
        if targets.contains_key(item.old.as_str()) {
            if let Some(print) = recorded.get_mut(&item.patch).and_then(|functions| functions.remove(&item.old)) {
                recorded.get_mut(&item.patch).unwrap().insert(item.new.clone(), print);
            }
        }
    }
    write_lock(mod_dir, &recorded)?;
    Ok(changed)
}
