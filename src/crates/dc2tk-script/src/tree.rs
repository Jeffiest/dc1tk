//! A decompiled tree records what the decompiler wrote, so the tools can tell your edits from its
//! output: setup and decompile refuse to overwrite edited files, changes lists them, build says
//! which edited libraries reach scripts you did not touch, and mod save copies them into a mod.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use indexmap::IndexMap;
use sha2::{Digest, Sha256};

use crate::pathnames::UNNAMED;
use crate::{Error, Result};

pub const ORIGINALS: &str = "original-hashes.json";

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A file's hash; line endings do not count, since an editor that rewrites them has not edited the
/// file.
pub fn file_hash(path: &Path) -> Result<String> {
    let data = fs::read(path)?;
    let mut plain = Vec::with_capacity(data.len());
    let mut at = 0;
    while at < data.len() {
        if data[at] == b'\r' && data.get(at + 1) == Some(&b'\n') {
            at += 1;
            continue;
        }
        plain.push(data[at]);
        at += 1;
    }
    Ok(hex(&Sha256::digest(&plain)))
}

/// Files of the tree, leaving out the record itself and hidden files and folders (clangd keeps its
/// index in .cache/ beside compile_flags.txt).
pub fn tracked(tree: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut folders = vec![tree.to_path_buf()];
    while let Some(folder) = folders.pop() {
        let Ok(listing) = fs::read_dir(&folder) else {
            continue;
        };
        for entry in listing.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                folders.push(path);
            } else if name != ORIGINALS && name != UNNAMED {
                found.push(path.strip_prefix(tree).unwrap().to_string_lossy().replace('\\', "/"));
            }
        }
    }
    found.sort();
    found
}

/// The tree's record: {"note", "files": {path: hash}, "headers": hash}, or None.
pub fn read_record(tree: &Path) -> Result<Option<serde_json::Value>> {
    let path = tree.join(ORIGINALS);
    if !path.is_file() {
        return Ok(None);
    }
    let value = serde_json::from_str(&dc2tk_disc::textfile::read_text(&path)?)
        .map_err(|error| Error::invalid(format!("{}: {error}", path.display())))?;
    Ok(Some(value))
}

/// The hashes of the files the decompiler wrote, or None for a tree with no record.
pub fn originals(tree: &Path) -> Result<Option<IndexMap<String, String>>> {
    Ok(read_record(tree)?.map(|record| {
        record["files"]
            .as_object()
            .map(|files| {
                files.iter().filter_map(|(path, hash)| Some((path.clone(), hash.as_str()?.to_string()))).collect()
            })
            .unwrap_or_default()
    }))
}

/// The headers the tree was decompiled with, as one hash: the SDK and definition headers decide
/// what every script compiles to, so a change to them means every script must be compiled again.
pub fn headers_hash(texts: &IndexMap<String, String>) -> String {
    let sorted: BTreeMap<&String, &String> = texts.iter().collect();
    let mut digest = Sha256::new();
    for (name, text) in sorted {
        digest.update(name.as_bytes());
        digest.update(b"\0");
        digest.update(text.replace("\r\n", "\n").as_bytes());
        digest.update(b"\0");
    }
    hex(&digest.finalize())
}

/// Records the files just written, keeping the record of any others (a folder decompiled into a
/// tree that holds more), and the hash of the headers they were written with.
pub fn record(tree: &Path, paths: &[String], headers: Option<&str>, removed: &[String]) -> Result<()> {
    let found = read_record(tree)?.unwrap_or_else(|| serde_json::json!({}));
    let mut files: BTreeMap<String, String> = found["files"]
        .as_object()
        .map(|files| {
            files
                .iter()
                .filter(|(path, _)| !removed.contains(path))
                .filter_map(|(path, hash)| Some((path.clone(), hash.as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default();
    for path in paths {
        files.insert(path.clone(), file_hash(&tree.join(path))?);
    }
    let mut entry = serde_json::Map::new();
    entry.insert(
        "note".into(),
        "What the decompiler wrote, so the tools can tell edits from its output. Do not edit.".into(),
    );
    entry.insert("files".into(), serde_json::to_value(files).unwrap());
    match headers {
        Some(headers) => {
            entry.insert("headers".into(), headers.into());
        }
        None => {
            if let Some(old) = found.get("headers") {
                entry.insert("headers".into(), old.clone());
            }
        }
    }
    let text = dc2tk_disc::textfile::json_text(&serde_json::Value::Object(entry));
    dc2tk_disc::textfile::write_text(&tree.join(ORIGINALS), &text)?;
    Ok(())
}

/// Files an earlier decompile wrote that this one does not, such as a library whose definition set
/// now gives it another path: (unedited, edited). The unedited ones can go.
pub fn stale(tree: &Path, written: &[String]) -> Result<(Vec<String>, Vec<String>)> {
    let recorded = originals(tree)?.unwrap_or_default();
    let written: BTreeSet<&String> = written.iter().collect();
    let (mut unedited, mut edited) = (Vec::new(), Vec::new());
    let leftover: BTreeSet<&String> = recorded.keys().filter(|path| !written.contains(path)).collect();
    for path in leftover {
        if tree.join(path).is_file() {
            if file_hash(&tree.join(path))? == recorded[path] {
                unedited.push(path.clone());
            } else {
                edited.push(path.clone());
            }
        }
    }
    Ok((unedited, edited))
}

/// Deletes files, and the folders they leave empty.
pub fn remove_files(tree: &Path, paths: &[String]) -> Result<()> {
    for path in paths {
        let file = tree.join(path);
        fs::remove_file(&file)?;
        let mut folder = file.parent().map(Path::to_path_buf);
        while let Some(current) = folder {
            if current == tree || !current.is_dir() || fs::read_dir(&current)?.next().is_some() {
                break;
            }
            fs::remove_dir(&current)?;
            folder = current.parent().map(Path::to_path_buf);
        }
    }
    Ok(())
}

/// What changed in the tree since it was decompiled.
pub struct Status {
    pub edited: Vec<String>,
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

pub fn status(tree: &Path) -> Result<Option<Status>> {
    let Some(recorded) = originals(tree)? else {
        return Ok(None);
    };
    let present: BTreeSet<String> = tracked(tree).into_iter().collect();
    let mut edited = Vec::new();
    for (path, hash) in &recorded {
        if present.contains(path) && file_hash(&tree.join(path))? != *hash {
            edited.push(path.clone());
        }
    }
    edited.sort();
    let added = present.iter().filter(|path| !recorded.contains_key(*path)).cloned().collect();
    let mut removed: Vec<String> = recorded.keys().filter(|path| !present.contains(*path)).cloned().collect();
    removed.sort();
    Ok(Some(Status { edited, added, removed }))
}

/// The files a decompile into tree would overwrite that hold edits (or that it cannot vouch for,
/// when the tree has no record of what was written).
pub fn overwrites(tree: &Path, paths: &[String]) -> Result<Vec<String>> {
    let existing: Vec<String> = paths.iter().filter(|path| tree.join(path).is_file()).cloned().collect();
    let Some(found) = status(tree)? else {
        return Ok(existing);
    };
    let changed: BTreeSet<&String> = found.edited.iter().chain(&found.added).collect();
    Ok(existing.into_iter().filter(|path| changed.contains(path)).collect())
}

/// Notes for build on edited libraries: how many scripts include each, and how many of those were
/// not edited themselves.
pub fn library_notes(tree: &Path, found: &Status) -> Result<Vec<String>> {
    let libraries: Vec<&String> =
        found.edited.iter().chain(&found.added).filter(|path| crate::index::is_library(path)).collect();
    if libraries.is_empty() {
        return Ok(Vec::new());
    }
    let users = crate::index::includers(&crate::index::outlines(tree)?);
    let touched: BTreeSet<&String> = found.edited.iter().chain(&found.added).collect();
    let mut lines = Vec::new();
    for path in libraries {
        let scripts = users.get(path).cloned().unwrap_or_default();
        let others = scripts.iter().filter(|script| !touched.contains(script)).count();
        lines.push(format!(
            "edited library {} is included by {} script{}{}",
            path,
            scripts.len(),
            if scripts.len() == 1 { "" } else { "s" },
            if others > 0 { format!(", {others} of which you did not edit") } else { String::new() }
        ));
    }
    Ok(lines)
}

/// What changed in the tree since it was decompiled, as changes prints it.
pub fn changes_text(tree: &Path) -> Result<String> {
    let Some(found) = status(tree)? else {
        return Ok(format!(
            "{} has no {}, so edits cannot be told apart from decompiler output",
            tree.display(),
            ORIGINALS
        ));
    };
    let mut lines = Vec::new();
    for (label, paths) in [("edited", &found.edited), ("added", &found.added), ("removed", &found.removed)] {
        lines.extend(paths.iter().map(|path| format!("{label:<8} {path}")));
    }
    if lines.is_empty() {
        return Ok("no changes since the tree was decompiled".into());
    }
    let notes = library_notes(tree, &found)?;
    if !notes.is_empty() {
        lines.push(String::new());
        lines.extend(notes);
    }
    Ok(lines.join("\n"))
}

/// Gives some scripts their own copy of a library, so an edit to it reaches only them. Returns
/// the scripts that still include the original.
pub fn fork(tree: &Path, library: &str, copy: &str, scripts: &[String]) -> Result<Vec<String>> {
    if !tree.join(library).is_file() {
        return Err(Error::invalid(format!("no library {} in {}", library, tree.display())));
    }
    if tree.join(copy).exists() {
        return Err(Error::invalid(format!("{copy} already exists")));
    }
    if !copy.starts_with("lib/") || !copy.ends_with(crate::source::EXTENSION) {
        return Err(Error::invalid(format!("the copy must be a lib/...{} path", crate::source::EXTENSION)));
    }
    let includes_library = |line: &str| {
        crate::index::include_pattern()
            .captures(line)
            .is_some_and(|found| found.get(0).unwrap().start() == 0 && &found[1] == library)
    };
    let mut missing = Vec::new();
    for script in scripts {
        if !dc2tk_disc::textfile::read_text(&tree.join(script))?.split('\n').any(includes_library) {
            missing.push(script.as_str());
        }
    }
    if !missing.is_empty() {
        return Err(Error::invalid(format!(
            "these scripts do not include {} directly:\n  {}",
            library,
            missing.join("\n  ")
        )));
    }
    let text = dc2tk_disc::textfile::read_text(&tree.join(library))?;
    let header = format!(
        "// Copy of {} for {}, made by dc2tk fork. Other scripts still include the original.\n\n",
        library,
        scripts.join(", ")
    );
    fs::create_dir_all(tree.join(copy).parent().unwrap())?;
    dc2tk_disc::textfile::write_text(&tree.join(copy), &(header + &text))?;
    for script in scripts {
        let path = tree.join(script);
        let lines: Vec<String> = dc2tk_disc::textfile::read_text(&path)?
            .split('\n')
            .map(|line| {
                if includes_library(line) {
                    line.replacen(&format!("\"{library}\""), &format!("\"{copy}\""), 1)
                } else {
                    line.to_string()
                }
            })
            .collect();
        dc2tk_disc::textfile::write_text(&path, &lines.join("\n"))?;
    }
    Ok(crate::index::includers(&crate::index::outlines(tree)?).get(library).cloned().unwrap_or_default())
}
