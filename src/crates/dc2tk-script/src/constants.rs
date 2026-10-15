//! Renaming a constant of a definition set's headers (a story flag, a music file): its declaration
//! in the set's include/ folder, what the set says about it (docs and line notes that name it, and
//! the variables named after it: the decompiler calls what get_flag(FLAG_TRAIN_RUNNING) reads
//! train_running, and a set that recorded that name would otherwise keep the old one), and its uses
//! in decompiled trees and mods. A tree's files that held no edits are recorded again, so changes
//! and build still tell them from edited ones. Names never change what a script compiles to.

use std::path::{Path, PathBuf};

use dc2tk_disc::files;
use indexmap::IndexMap;
use regex::Regex;

use crate::{names, paths, tree, Error, Result};

/// The prefixes of the constants the decompiler names variables after, with the prefix dropped
/// and the rest in lower case (sdk::subject_prefix).
const SUBJECT_PREFIXES: [&str; 2] = ["FLAG_", "CHARA_"];

/// The variable name the decompiler gives what a call with this constant reads, if any.
fn subject(constant: &str) -> Option<String> {
    SUBJECT_PREFIXES
        .iter()
        .find_map(|prefix| constant.strip_prefix(prefix))
        .filter(|rest| !rest.is_empty())
        .map(str::to_lowercase)
}

fn word(name: &str) -> Regex {
    Regex::new(&format!(r"\b{}\b", regex::escape(name))).unwrap()
}

/// What rename changed: files in the set, and per tree or mod folder the files rewritten.
#[derive(Debug, Default)]
pub struct Renamed {
    pub set_files: Vec<PathBuf>,
    pub folders: Vec<(PathBuf, Vec<String>)>,
}

/// Every file under a folder with one of the extensions, as paths relative to it with "/".
/// Hidden files and folders are left out.
fn files_under(folder: &Path, extensions: &[&str]) -> Result<Vec<String>> {
    let mut found = paths::walk(folder, |entry| {
        !entry.name.starts_with('.')
            && (entry.is_dir
                || entry.path.extension().is_some_and(|ext| extensions.iter().any(|wanted| ext == *wanted)))
    })?;
    found.sort();
    Ok(found)
}

/// Rewrites a file with edit; true when it changed. Line endings are kept as they are.
fn rewrite(path: &Path, edit: impl Fn(&str) -> String) -> Result<bool> {
    let text = files::read_to_string(path)?;
    let changed = edit(&text);
    if changed == text {
        return Ok(false);
    }
    files::write(path, changed)?;
    Ok(true)
}

/// Renames a constant declared in the headers of the definition set at set to new. headers are the
/// texts of every header in use (the SDK's and the sets'), which must not declare new already;
/// folders are decompiled trees or mods whose source uses the constant.
pub fn rename(
    set: &Path,
    old: &str,
    new: &str,
    headers: &IndexMap<String, String>,
    folders: &[PathBuf],
) -> Result<Renamed> {
    for name in [old, new] {
        if !names::is_name(name) {
            return Err(Error::invalid(format!("{name} is not a valid name")));
        }
    }
    let old_word = word(old);
    let new_word = word(new);
    if let Some((header, _)) = headers.iter().find(|(_, text)| new_word.is_match(text)) {
        return Err(Error::invalid(format!("{new} is already used in {header}")));
    }
    let include = set.join("include");
    let own = files_under(&include, &["h"])?;
    let mut declared = false;
    for path in &own {
        let text = files::read_to_string(include.join(path))?;
        if new_word.is_match(&text) {
            return Err(Error::invalid(format!("{new} is already used in {path}")));
        }
        declared |= old_word.is_match(&text);
    }
    if !declared {
        return Err(Error::invalid(format!("{old} is not in the headers of {}", set.display())));
    }
    // the variables named after it, as the set records them ("train_running") and as source
    // declares them
    let variables = match (subject(old), subject(new)) {
        (Some(from), Some(to)) if from != to => Some((from, to)),
        _ => None,
    };
    let quoted = variables.as_ref().map(|(from, _)| Regex::new(&format!(r#""{}""#, regex::escape(from))).unwrap());
    let mut renamed = Renamed::default();
    for path in own {
        let file = include.join(path);
        if rewrite(&file, |text| old_word.replace_all(text, new).into_owned())? {
            renamed.set_files.push(file);
        }
    }
    for path in files_under(set, &["json", "md"])? {
        if path.starts_with("include/")
            || path.starts_with("strings/")
            || path == crate::definitions::DISC
            || path == RENAMED
        {
            continue;
        }
        let file = set.join(&path);
        let json = path.ends_with(".json");
        let changed = rewrite(&file, |text| {
            let mut out = old_word.replace_all(text, new).into_owned();
            if let (true, Some(quoted), Some((_, to))) = (json, &quoted, &variables) {
                // a variable's or global's name: a whole JSON string, never a function's name
                out = replace_unless_named(&out, quoted, &format!("\"{to}\""));
            }
            out
        })?;
        if changed {
            renamed.set_files.push(file);
        }
    }
    renamed.folders = rename_in_folders(old, new, folders)?;
    record(set, old, new)?;
    renamed.set_files.push(set.join(RENAMED));
    Ok(renamed)
}

/// Renames a constant's uses in decompiled trees and mods, and the variables named after it, and
/// records again a tree's files that held no edits: (folder, files changed) per folder.
pub fn rename_in_folders(old: &str, new: &str, folders: &[PathBuf]) -> Result<Vec<(PathBuf, Vec<String>)>> {
    let old_word = word(old);
    let variables = match (subject(old), subject(new)) {
        (Some(from), Some(to)) if from != to => Some((from, to)),
        _ => None,
    };
    let variable_word = variables.as_ref().map(|(from, _)| word(from));
    let mut done = Vec::new();
    for folder in folders {
        let unedited: Vec<String> = match tree::status(folder)? {
            Some(status) => {
                let recorded = tree::originals(folder)?.unwrap_or_default();
                recorded.keys().filter(|path| !status.edited.contains(path)).cloned().collect()
            }
            None => Vec::new(),
        };
        let mut changed = Vec::new();
        for path in files_under(folder, &["c", "h"])? {
            let edited = rewrite(&folder.join(&path), |text| {
                let mut out = old_word.replace_all(text, new).into_owned();
                if let (Some(variable), Some((_, to))) = (&variable_word, &variables) {
                    out = replace_variables(&out, variable, to);
                }
                out
            })?;
            if edited {
                changed.push(path);
            }
        }
        let rerecord: Vec<String> = changed.iter().filter(|path| unedited.contains(path)).cloned().collect();
        if !rerecord.is_empty() {
            tree::record(folder, &rerecord, None, &[])?;
        }
        done.push((folder.clone(), changed));
    }
    Ok(done)
}

/// The file of a definition set that lists the constants renamed in it, old name to new, in the
/// order they were renamed, so trees and mods made before can follow (migrate).
pub const RENAMED: &str = "renamed.json";

/// The renames a definition set records, oldest first.
pub fn renamed(set: &Path) -> Result<Vec<(String, String)>> {
    let path = set.join(RENAMED);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let map: IndexMap<String, String> = serde_json::from_str(&files::read_to_string(&path)?)
        .map_err(|error| Error::invalid(format!("{}: {error}", path.display())))?;
    Ok(map.into_iter().collect())
}

/// Adds old -> new to the set's list of renames.
fn record(set: &Path, old: &str, new: &str) -> Result<()> {
    let mut map: IndexMap<String, String> = renamed(set)?.into_iter().collect();
    map.shift_remove(old);
    map.insert(old.to_string(), new.to_string());
    let mut text = serde_json::to_string_pretty(&map).map_err(|error| Error::invalid(error.to_string()))?;
    text.push('\n');
    files::write(set.join(RENAMED), text)?;
    Ok(())
}

/// The name a constant has now, when one of the sets renamed it (following a chain of renames).
pub fn renamed_to(sets: &[PathBuf], name: &str) -> Option<String> {
    let renames: Vec<(String, String)> = sets.iter().flat_map(|set| renamed(set).unwrap_or_default()).collect();
    let mut current = name.to_string();
    let mut moved = false;
    for (old, new) in &renames {
        if *old == current {
            current = new.clone();
            moved = true;
        }
    }
    moved.then_some(current)
}

/// Gives trees and mods made before the sets' renames the new names: every recorded rename, in
/// order. (folder, files changed) per folder, a file listed once.
pub fn migrate(sets: &[PathBuf], folders: &[PathBuf]) -> Result<Vec<(PathBuf, Vec<String>)>> {
    let mut changed: Vec<(PathBuf, Vec<String>)> = folders.iter().map(|folder| (folder.clone(), Vec::new())).collect();
    for set in sets {
        for (old, new) in renamed(set)? {
            for (index, (_, files)) in rename_in_folders(&old, &new, folders)?.into_iter().enumerate() {
                for file in files {
                    if !changed[index].1.contains(&file) {
                        changed[index].1.push(file);
                    }
                }
            }
        }
    }
    for (_, files) in &mut changed {
        files.sort();
    }
    Ok(changed)
}

/// Replaces a variable's name in source, leaving a call to a function of that name alone.
fn replace_variables(text: &str, pattern: &Regex, to: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for found in pattern.find_iter(text) {
        out.push_str(&text[last..found.start()]);
        let call = text[found.end()..].trim_start().starts_with('(');
        out.push_str(if call { found.as_str() } else { to });
        last = found.end();
    }
    out.push_str(&text[last..]);
    out
}

/// Replaces every match of pattern in text, except where the JSON key before it is "name".
fn replace_unless_named(text: &str, pattern: &Regex, to: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for found in pattern.find_iter(text) {
        out.push_str(&text[last..found.start()]);
        let before = text[..found.start()].trim_end();
        if before.ends_with("\"name\":") {
            out.push_str(found.as_str());
        } else {
            out.push_str(to);
        }
        last = found.end();
    }
    out.push_str(&text[last..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subjects_drop_the_prefix() {
        assert_eq!(subject("FLAG_TRAIN_RUNNING").as_deref(), Some("train_running"));
        assert_eq!(subject("CHARA_PLAYER").as_deref(), Some("player"));
        assert_eq!(subject("BGM_SHOP"), None);
    }

    #[test]
    fn calls_keep_a_variable_name() {
        let pattern = word("seen");
        assert_eq!(replace_variables("seen = seen (1); x = seen;", &pattern, "met"), "met = seen (1); x = met;");
    }

    #[test]
    fn function_names_keep_a_variable_name() {
        let pattern = Regex::new(r#""seen""#).unwrap();
        let text = "{\"name\": \"seen\", \"locals\": [\"seen\"], \"types\": {\"seen\": \"flag\"}}";
        assert_eq!(
            replace_unless_named(text, &pattern, "\"met\""),
            "{\"name\": \"seen\", \"locals\": [\"met\"], \"types\": {\"met\": \"flag\"}}"
        );
    }
}
