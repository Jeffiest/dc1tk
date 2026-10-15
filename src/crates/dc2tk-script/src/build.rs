//! What build compiles: the scripts of a tree whose compiled form differs from the disc, with mods'
//! files, their patches and build --start-at's script standing in.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

use dc2tk_disc::archive::{Archive, Entry};
use dc2tk_disc::{rebuild, Image};
use indexmap::IndexMap;
use rayon::prelude::*;

use crate::compile::Toolchain;
use crate::patches::{self, Patch};
use crate::{definitions, index, library, natives, parse, pathnames, source, tree, Error, Result};

/// The mod patches that change one script of a tree, merged in once its libraries are.
pub struct Patched<'a> {
    pub tree: &'a Path,
    pub relative: &'a str,
    pub patches: Vec<&'a Patch>,
}

/// Compiles a script of the tree; text, when given, stands in for the file's own (build
/// --start-at, build --mod), table_path, its path without readable names, picks the native table,
/// and overlay holds mod files that stand in for the tree's libraries.
pub fn compiled(
    source_path: &Path,
    tools: &Toolchain,
    text: Option<&str>,
    table_path: Option<&str>,
    overlay: Option<&IndexMap<String, String>>,
    patched: Option<&Patched>,
) -> Result<Vec<u8>> {
    let owned;
    let text = match text {
        Some(text) => text,
        None => {
            owned = dc2tk_py::read_text(source_path)?;
            &owned
        }
    };
    let mut headers = tools.headers.clone();
    headers.extend(definitions::source_headers(source_path)?);
    let libraries = library::include_texts(text, source_path, overlay, tools.sdk_dir, &[])?;
    let defines = std::collections::HashMap::new();
    let options = parse::ParseOptions {
        defines: Some(&defines),
        headers: Some(&headers),
        libraries: Some(&libraries),
        ..Default::default()
    };
    let program = parse::parse_with(text, &options)?;
    let includes = library::read_includes(&program, source_path, overlay, tools.sdk_dir, &[])?;
    let mut program = library::expand(program, &includes, &headers)?;
    if let Some(patched) = patched {
        program = patches::applied(
            program,
            &patched.patches,
            patched.relative,
            patched.tree,
            &headers,
            overlay,
            tools.sdk_dir,
        )?;
    }
    if program.get("asm").is_none() && program["natives"].is_null() {
        let table_path = match table_path {
            Some(path) => path.to_string(),
            None => definitions::resolved_posix(source_path),
        };
        program = program.with("natives", natives::table_for(&table_path, tools.rules));
    }
    crate::compile::compile_program(&program, tools.tables)
}

/// The tree's scripts (outside lib/), by path from it, sorted.
pub fn source_scripts(tree: &Path) -> BTreeSet<String> {
    index::source_files(tree).into_iter().filter(|path| !index::is_library(path)).collect()
}

/// Where a script is on the disc: the file of DATA.DAT and the pack members down to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub top: String,
    pub members: Vec<String>,
}

pub fn disc_location(relative: &str, entries: &HashMap<&str, &Entry>) -> Option<Location> {
    let stem = relative.strip_suffix(source::EXTENSION).unwrap_or(relative);
    let parts: Vec<&str> = stem.split(".d/").collect();
    if parts.len() == 1 {
        return rebuild::SCRIPT_SUFFIXES
            .iter()
            .map(|suffix| format!("{stem}{suffix}"))
            .find(|top| entries.contains_key(top.as_str()))
            .map(|top| Location { top, members: Vec::new() });
    }
    if !entries.contains_key(parts[0]) {
        return None;
    }
    Some(Location { top: parts[0].to_string(), members: parts[1..].iter().map(|part| part.to_string()).collect() })
}

/// Scripts that cannot differ from the disc: unchanged since the decompiler wrote them, as are the
/// libraries they include, and replaced by no override. Every retail script compiles back to its
/// own bytes (verify), so build need not compile these. Empty for a tree with no record, with why.
pub fn settled_and_why(
    tree_dir: &Path,
    overrides: &IndexMap<String, String>,
    headers: &IndexMap<String, String>,
) -> Result<(HashSet<String>, Option<String>)> {
    let Some(record) =
        tree::read_record(tree_dir)?.filter(|record| record.as_object().is_some_and(|map| !map.is_empty()))
    else {
        return Ok((
            HashSet::new(),
            Some("the tree has no record of what was decompiled; decompile it again for fast builds".into()),
        ));
    };
    if record["headers"].as_str() != Some(tree::headers_hash(headers).as_str()) {
        return Ok((
            HashSet::new(),
            Some("the SDK or definition headers differ from the ones the tree was decompiled with".into()),
        ));
    }
    let recorded: IndexMap<String, String> = record["files"]
        .as_object()
        .map(|files| files.iter().filter_map(|(path, hash)| Some((path.clone(), hash.as_str()?.to_string()))).collect())
        .unwrap_or_default();
    for (path, hash) in &recorded {
        if path.starts_with("include/") {
            let file = tree_dir.join(path);
            if !file.is_file() || tree::file_hash(&file)? != *hash {
                return Ok((HashSet::new(), Some(format!("the tree's own header {path} was edited"))));
            }
        }
    }
    Ok((settled_scripts(tree_dir, overrides, &recorded)?, None))
}

fn settled_scripts(
    tree_dir: &Path,
    overrides: &IndexMap<String, String>,
    recorded: &IndexMap<String, String>,
) -> Result<HashSet<String>> {
    let outlines = index::outlines(tree_dir)?;
    let mut paths: BTreeSet<&String> = BTreeSet::new();
    for (path, entry) in &outlines {
        if !index::is_library(path) {
            paths.insert(path);
            paths.extend(&entry.includes);
        }
    }
    let known: HashMap<&String, bool> = paths
        .into_par_iter()
        .map(|path| {
            let file = tree_dir.join(path);
            let unchanged = recorded.get(path).is_some_and(|hash| {
                !overrides.contains_key(path)
                    && file.is_file()
                    && tree::file_hash(&file).is_ok_and(|found| found == *hash)
            });
            (path, unchanged)
        })
        .collect();
    Ok(outlines
        .iter()
        .filter(|(path, entry)| {
            !index::is_library(path) && known[path] && entry.includes.iter().all(|item| known[item])
        })
        .map(|(path, _)| path.clone())
        .collect())
}

/// A script whose compiled form differs from the disc's.
#[derive(Clone, Debug)]
pub struct Change {
    pub top: String,
    pub members: Vec<String>,
    pub source: String,
    pub data: Vec<u8>,
}

pub struct Changes {
    pub scripts: Vec<Change>,
    /// The DATA.DAT files rebuilt with the changed scripts in them.
    pub files: BTreeMap<String, Vec<u8>>,
    pub skipped: Vec<String>,
    pub compiled: usize,
    pub all_because: Option<String>,
}

/// The scripts whose compiled form differs from the disc. overrides holds whole-file texts that
/// stand in for tree files, mods' and build --start-at's, keyed by tree path; scripts only a mod
/// has are compiled too. plan holds the mod patches that change each script.
pub fn changes(
    tree_dir: &Path,
    iso_path: &Path,
    tools: &Toolchain,
    overrides: &IndexMap<String, String>,
    full: bool,
    plan: Option<&patches::Plan>,
) -> Result<Changes> {
    let (mut skip, why) = if full {
        (HashSet::new(), Some("--full".to_string()))
    } else {
        settled_and_why(tree_dir, overrides, tools.headers)?
    };
    if let Some(plan) = plan {
        skip.retain(|path| !plan.scripts.contains_key(path));
    }
    let image = Image::open(iso_path)?;
    let disc = Archive::open(image.bytes())?;
    let entries: HashMap<&str, &Entry> = disc.entries.iter().map(|entry| (entry.path.as_str(), entry)).collect();
    let mut scripts = source_scripts(tree_dir);
    scripts
        .extend(overrides.keys().filter(|path| path.ends_with(source::EXTENSION) && !index::is_library(path)).cloned());
    let mut skipped = Vec::new();
    let mut wanted = Vec::new();
    for relative in &scripts {
        let plain = pathnames::unnamed(tree_dir, relative)?;
        let Some(location) = disc_location(&plain, &entries) else {
            skipped.push(relative.clone());
            continue;
        };
        if skip.contains(relative) {
            continue;
        }
        let top = disc.read(image.bytes(), entries[location.top.as_str()]);
        if rebuild::read_member(top, &location.members).is_none() {
            skipped.push(relative.clone());
            continue;
        }
        wanted.push((relative, plain, location));
    }
    let results: Vec<Result<Option<Change>>> = wanted
        .par_iter()
        .map(|(relative, plain, location)| {
            let patched = plan.and_then(|plan| plan.for_script(relative)).map(|found| Patched {
                tree: tree_dir,
                relative,
                patches: found,
            });
            let overlay = Some(overrides);
            let new = compiled(
                &tree_dir.join(relative),
                tools,
                overrides.get(*relative).map(String::as_str),
                Some(plain),
                overlay,
                patched.as_ref(),
            )
            .map_err(|error| Error::invalid(format!("{relative}: {error}")))?;
            let top = disc.read(image.bytes(), entries[location.top.as_str()]);
            let original = rebuild::read_member(top, &location.members).unwrap();
            Ok((new != original).then(|| Change {
                top: location.top.clone(),
                members: location.members.clone(),
                source: (*relative).clone(),
                data: new,
            }))
        })
        .collect();
    let mut found = Vec::new();
    for result in results {
        if let Some(change) = result? {
            found.push(change);
        }
    }
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for change in &found {
        let data = match files.get(&change.top) {
            Some(data) => data.clone(),
            None => disc.read(image.bytes(), entries[change.top.as_str()]).to_vec(),
        };
        files.insert(change.top.clone(), rebuild::replaced_member(&data, &change.members, &change.data)?);
    }
    Ok(Changes { compiled: scripts.len() - skip.len(), scripts: found, files, skipped, all_because: why })
}

/// The changed scripts that read back out of a built image differently.
pub fn check(out_path: &Path, found: &[Change]) -> Result<Vec<String>> {
    let image = Image::open(out_path)?;
    let disc = Archive::open(image.bytes())?;
    Ok(found
        .iter()
        .filter(|change| {
            rebuild::read_back(image.bytes(), &disc, &change.top, &change.members).as_deref() != Some(&change.data[..])
        })
        .map(|change| change.source.clone())
        .collect())
}
