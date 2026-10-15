//! dc2tk decompile EXTRACTED -o TREE: every script of an extracted disc (or a folder of one) as C,
//! with the functions scripts share as libraries, readable names for folders and scripts, disc.h
//! and the record of what was written.

use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use indexmap::IndexMap;
use rayon::prelude::*;

use dc2tk_script::definitions::{self, Definitions};
use dc2tk_script::{assets, disc, library, pathnames, sdk, slots, source, tree};

use crate::files;

/// Whether an extracted disc is the one the definition sets were made for.
pub fn disc_match_text(defs: &Definitions, differences: &definitions::DiscDifferences) -> String {
    if defs.disc.is_empty() {
        return "the definition sets record no disc to compare with".into();
    }
    if differences.differ.is_empty() && differences.unknown.is_empty() && differences.missing.is_empty() {
        return format!("the disc is the one the definition sets were made for ({} scripts match)", defs.disc.len());
    }
    let first = differences
        .differ
        .iter()
        .chain(&differences.unknown)
        .chain(&differences.missing)
        .next()
        .cloned()
        .unwrap_or_default();
    format!(
        "the disc is not the one the definition sets were made for: {} scripts differ, {} are new, {} are missing (first: {}); \
         names and docs may not fit",
        differences.differ.len(),
        differences.unknown.len(),
        differences.missing.len(),
        first
    )
}

fn write(out: &Path, relative: &str, text: &str) -> Result<()> {
    let target = out.join(relative);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    dc2tk_py::write_text(&target, text)?;
    Ok(())
}

/// The path of a script's source in a tree without readable names: map/s/s51/s51_1.c.
fn plain_source(path: &str) -> String {
    let stem = path.rsplit_once('.').map_or(path, |(stem, _)| stem);
    format!("{stem}{}", source::EXTENSION)
}

pub fn run(
    stb: &Path,
    output: Option<&Path>,
    force: bool,
    defs: Definitions,
    tables: std::collections::BTreeMap<String, dc2tk_script::natives::Table>,
    sdk_dir: &Path,
) -> Result<()> {
    let Some(out) = output else {
        bail!("decompiling a directory needs -o OUT_DIR");
    };
    let out = out.to_path_buf();
    let root = dc2tk_script::decompiler::disc_root(&stb.join("manifest.json")).unwrap_or_else(|| stb.to_path_buf());
    let resolved = PathBuf::from(definitions::resolved_posix(stb));
    let resolved_root = PathBuf::from(definitions::resolved_posix(&root));
    let prefix =
        resolved.strip_prefix(&resolved_root).map(|rest| rest.to_string_lossy().replace('\\', "/")).unwrap_or_default();
    let prefix = if prefix.is_empty() { ".".to_string() } else { prefix };
    let found = files::find_scripts(stb)?;
    // a folder of the disc is compared with that folder of the set's disc
    let scripts: Vec<(String, Vec<u8>)> =
        found.par_iter().map(|path| Ok((path.clone(), std::fs::read(stb.join(path))?))).collect::<Result<_>>()?;
    let folder = if prefix == "." { String::new() } else { format!("{prefix}/") };
    let differences = definitions::disc_differences(&scripts, &defs, &folder);
    if !differences.differ.is_empty() || !differences.unknown.is_empty() {
        println!("warning: {}", disc_match_text(&defs, &differences));
    }
    let paths: Vec<String> =
        found.iter().map(|path| if prefix == "." { path.clone() } else { format!("{prefix}/{path}") }).collect();
    let headers_used = sdk::header_texts(&defs, sdk_dir)?;
    let (defs, headers, generated) = disc::with_header(&root, defs, headers_used.clone());
    let mut model = library::model_for(&root, &paths, &tables, &defs, &headers)?;
    let generated = slots::with_header(generated, &model.folder_slots, &model.villagers);
    let generated = assets::with_header(generated, &model.asset_names);
    let naming = pathnames::lookups(&root, &defs);
    let mut renamed: IndexMap<String, String> = IndexMap::new();
    let libraries: Vec<(String, String)> = model
        .libraries
        .values()
        .map(|library| (library.path.clone(), pathnames::named_library(&library.path, &naming)))
        .collect();
    for (plain, named) in &libraries {
        renamed.insert(plain.clone(), named.clone());
    }
    // the scripts include libraries by their named paths
    for library in model.libraries.values_mut() {
        library.path = pathnames::named_library(&library.path, &naming);
    }
    for path in &paths {
        let plain = plain_source(path);
        renamed.insert(plain.clone(), pathnames::named_script(&plain, &naming));
    }
    let library_paths: Vec<String> = libraries.iter().map(|(_, named)| named.clone()).collect();
    let mut written: Vec<String> = library_paths.clone();
    written.extend(paths.iter().map(|path| renamed[&plain_source(path)].clone()));
    written.push(source::EDITOR_FLAGS.to_string());
    if generated.is_some() {
        written.push(format!("include/{}", disc::HEADER));
    }
    let blocked = if out.is_dir() { tree::overwrites(&out, &written)? } else { Vec::new() };
    if !blocked.is_empty() && !force {
        bail!(
            "{} holds {} edited file{} this would overwrite, such as:\n  {}\nSave them first (dc2tk mod save {} NAME), write somewhere else, or pass --force.",
            out.display(),
            blocked.len(),
            if blocked.len() == 1 { "" } else { "s" },
            blocked.iter().take(10).cloned().collect::<Vec<_>>().join("\n  "),
            out.display()
        );
    }
    if let Some(text) = &generated {
        write(&out, &format!("include/{}", disc::HEADER), text)?;
    }
    paths.par_iter().try_for_each(|path| -> Result<()> {
        let program = &model.programs[path];
        let text = source::text(&model.script_program(program, path)?)?;
        write(&out, &renamed[&plain_source(path)], &text)
    })?;
    // libraries last: the scripts that include one tell its sound pass what they pass it
    for (path, text) in model.library_texts()? {
        write(&out, &path, &text)?;
    }
    write(&out, source::EDITOR_FLAGS, &source::editor_flags(sdk_dir, &model.defs.sets, None))?;
    let mut removed = Vec::new();
    if prefix == "." {
        // a whole-disc decompile replaces the tree: files the last one wrote and this one does not
        // (a library the definitions moved) go, unless someone edited them
        let (unedited, edited) = tree::stale(&out, &written)?;
        tree::remove_files(&out, &unedited)?;
        for path in &edited {
            println!("kept {path}: the decompiler no longer writes it, but it was edited");
        }
        if !unedited.is_empty() {
            println!(
                "removed {} file{} the decompiler no longer writes",
                unedited.len(),
                if unedited.len() == 1 { "" } else { "s" }
            );
        }
        removed = unedited;
    }
    tree::record(&out, &written, Some(&tree::headers_hash(&headers_used)), &removed)?;
    pathnames::record(&out, &renamed)?;
    println!(
        "{} scripts and {} library files written to {}",
        paths.len(),
        library_paths.len(),
        files::python_path(&out.to_string_lossy())
    );
    Ok(())
}
