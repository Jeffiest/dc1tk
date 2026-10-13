//! The SDK: the headers (sdk/, and each definition set's include/) that declare the natives,
//! programs, inline helpers, constants and value kinds the decompiled source uses. So far: loading
//! their texts; reading them comes with the C parser.

use std::fs;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;

use crate::definitions::Definitions;
use crate::Result;

/// The prelude every script includes; it declares no natives.
pub const PRELUDE: &str = "c2s.h";

/// Files under a folder with an extension, sorted as Python sorts Windows paths (by lowercased
/// parts), as paths from the folder with "/".
pub fn files_under(base: &Path, extension: &str) -> Vec<String> {
    let mut found: Vec<PathBuf> = Vec::new();
    let mut folders = vec![base.to_path_buf()];
    while let Some(folder) = folders.pop() {
        let Ok(listing) = fs::read_dir(&folder) else {
            continue;
        };
        for entry in listing.flatten() {
            let path = entry.path();
            if path.is_dir() {
                folders.push(path);
            } else if path.extension().is_some_and(|ext| ext == extension) {
                found.push(path);
            }
        }
    }
    let mut relative: Vec<String> =
        found.iter().map(|path| path.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/")).collect();
    relative.sort_by_key(|path| -> Vec<String> {
        let path = if cfg!(windows) { path.to_lowercase() } else { path.clone() };
        path.split('/').map(str::to_string).collect()
    });
    relative
}

/// Path.read_text(): UTF-8 with \r\n and \r read as \n.
pub fn read_text(path: &Path) -> Result<String> {
    Ok(fs::read_to_string(path)?.replace("\r\n", "\n").replace('\r', "\n"))
}

/// Every header under a folder, by path from it.
pub fn load_texts(directory: &Path) -> Result<IndexMap<String, String>> {
    let mut texts = IndexMap::new();
    for relative in files_under(directory, "h") {
        let text = read_text(&directory.join(&relative))?;
        texts.insert(relative, text);
    }
    Ok(texts)
}

/// The SDK's headers and each definition set's, a later one's winning.
pub fn header_texts(defs: &Definitions, sdk_dir: &Path) -> Result<IndexMap<String, String>> {
    let mut texts = load_texts(sdk_dir)?;
    for directory in &defs.sets {
        let include = directory.join("include");
        if include.is_dir() {
            texts.extend(load_texts(&include)?);
        }
    }
    Ok(texts)
}
