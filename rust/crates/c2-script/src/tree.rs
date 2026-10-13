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
            if name.starts_with('.') || name == "__pycache__" {
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
    let value = serde_json::from_str(&c2_py::read_text(&path)?)
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
    let text = c2_py::json::dumps(&serde_json::Value::Object(entry), &c2_py::json::Options::indented(1)) + "\n";
    c2_py::write_text(&tree.join(ORIGINALS), &text)?;
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
