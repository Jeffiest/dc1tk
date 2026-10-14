//! Renaming a constant of a definition set's headers (a story flag, a music file): its declaration
//! in the set's include/ folder, what the set says about it (docs and line notes that name it, and
//! the variables named after it: the decompiler calls what get_flag(FLAG_TRAIN_RUNNING) reads
//! train_running, and a set that recorded that name would otherwise keep the old one), and its uses
//! in decompiled trees and mods. A tree's files that held no edits are recorded again, so changes
//! and build still tell them from edited ones. Names never change what a script compiles to.

use std::fs;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use regex::Regex;

use crate::{names, tree, Error, Result};

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
fn files_under(folder: &Path, extensions: &[&str]) -> Vec<String> {
    let mut found = Vec::new();
    let mut folders = vec![folder.to_path_buf()];
    while let Some(current) = folders.pop() {
        let Ok(listing) = fs::read_dir(&current) else {
            continue;
        };
        for entry in listing.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                folders.push(path);
            } else if path.extension().is_some_and(|ext| extensions.iter().any(|wanted| ext == *wanted)) {
                found.push(path.strip_prefix(folder).unwrap().to_string_lossy().replace('\\', "/"));
            }
        }
    }
    found.sort();
    found
}

/// Rewrites a file with edit; true when it changed. Line endings are kept as they are.
fn rewrite(path: &Path, edit: impl Fn(&str) -> String) -> Result<bool> {
    let text = fs::read_to_string(path)?;
    let changed = edit(&text);
    if changed == text {
        return Ok(false);
    }
    fs::write(path, changed)?;
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
        if !names::is_identifier(name) {
            return Err(Error::invalid(format!("{name} is not a valid name")));
        }
    }
    let old_word = word(old);
    let new_word = word(new);
    if let Some((header, _)) = headers.iter().find(|(_, text)| new_word.is_match(text)) {
        return Err(Error::invalid(format!("{new} is already used in {header}")));
    }
    let include = set.join("include");
    let own = files_under(&include, &["h"]);
    let mut declared = false;
    for path in &own {
        let text = fs::read_to_string(include.join(path))?;
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
    let variable_word = variables.as_ref().map(|(from, _)| word(from));
    let mut renamed = Renamed::default();
    for path in own {
        let file = include.join(path);
        if rewrite(&file, |text| old_word.replace_all(text, new).into_owned())? {
            renamed.set_files.push(file);
        }
    }
    for path in files_under(set, &["json", "md"]) {
        if path.starts_with("include/") || path.starts_with("strings/") || path == crate::definitions::DISC {
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
    for folder in folders {
        let unedited: Vec<String> = match tree::status(folder)? {
            Some(status) => {
                let recorded = tree::originals(folder)?.unwrap_or_default();
                recorded.keys().filter(|path| !status.edited.contains(path)).cloned().collect()
            }
            None => Vec::new(),
        };
        let mut changed = Vec::new();
        for path in files_under(folder, &["c", "h"]) {
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
        renamed.folders.push((folder.clone(), changed));
    }
    Ok(renamed)
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
