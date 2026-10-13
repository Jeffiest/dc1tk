//! Definition sets (definitions/pal): what the tools know about one disc's scripts. A set's
//! definitions.json can extend other sets, which load first. So far this reads the rules for
//! which native table a script calls; the names, docs and libraries come with the decompiler.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{Error, Result};

pub const MANIFEST: &str = "definitions.json";

#[derive(Debug, Default, Deserialize)]
struct Manifest {
    #[serde(default)]
    extends: Vec<String>,
    #[serde(default)]
    tables: Vec<TableRule>,
}

#[derive(Debug, Deserialize)]
struct TableRule {
    prefix: String,
    table: String,
}

fn manifest(directory: &Path) -> Result<Manifest> {
    let path = directory.join(MANIFEST);
    if !path.is_file() {
        return Ok(Manifest::default());
    }
    serde_json::from_str(&fs::read_to_string(&path)?)
        .map_err(|error| Error::invalid(format!("{}: {error}", path.display())))
}

/// A path made absolute with symlinks and "..", resolved as Python's Path.resolve() does, written
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
