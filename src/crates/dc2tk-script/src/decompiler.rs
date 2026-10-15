//! Decompiling a script to readable source: the program tree (decompile), named from the
//! definition set (names), commented with the game's text (messages, notes) and shown through the
//! SDK (sdk::present). What it needs that does not change between scripts is read once (Setup).
//! A whole extracted disc, or a folder of one, decompiles into a source tree (decompile_tree).

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use indexmap::IndexMap;
use rayon::prelude::*;

use dc2tk_disc::extract::MANIFEST;

use crate::definitions::{self, Definitions};
use crate::messages::{self, NameTable};
use crate::natives::{self, Table};
use crate::node::Node;
use crate::sdk::{self, Context};
use crate::{
    assets, decompile, disc, library, names, notes, parse, pathnames, paths, slots, source, stb, tree, Context as _,
    Error, Result,
};

/// What decompiling scripts needs that does not change between them.
pub struct Setup {
    pub defs: Definitions,
    pub tables: BTreeMap<String, Table>,
    pub headers: IndexMap<String, String>,
    pub known: names::Known,
    /// Each native table's header context, with the definition set's sound sets.
    contexts: HashMap<String, Option<Context>>,
    /// The name tables of each disc root.
    lookups: Mutex<HashMap<PathBuf, Arc<IndexMap<String, NameTable>>>>,
}

impl Setup {
    pub fn new(defs: Definitions, tables: BTreeMap<String, Table>, sdk_dir: &Path) -> Result<Setup> {
        let headers = sdk::header_texts(&defs, sdk_dir)?;
        let known = names::Known::new(names::merged(&defs.names));
        let mut contexts = HashMap::new();
        for table in natives::TABLES {
            let found = sdk::header_context(table, &headers, &tables, &defs.include, &|text| parse::parse(text))?;
            contexts.insert(table.to_string(), sdk::with_sound_sets(found, &defs));
        }
        Ok(Setup { defs, tables, headers, known, contexts, lookups: Mutex::new(HashMap::new()) })
    }

    pub fn context(&self, table: &str) -> Option<&Context> {
        self.contexts.get(table).and_then(Option::as_ref)
    }

    fn lookups(&self, root: &Path) -> Arc<IndexMap<String, NameTable>> {
        let mut cache = self.lookups.lock().unwrap();
        cache.entry(root.to_path_buf()).or_insert_with(|| Arc::new(disc::tables(root, &self.defs))).clone()
    }
}

/// The extracted disc a script is in: the nearest folder above it with manifest.json.
pub fn disc_root(path: &Path) -> Option<PathBuf> {
    let full = PathBuf::from(definitions::resolved_posix(path));
    full.ancestors().skip(1).find(|folder| folder.join(MANIFEST).is_file()).map(Path::to_path_buf)
}

fn bodies(program: &Node) -> Node {
    program
        .list("functions")
        .iter()
        .map(|function| function.get("body").cloned().unwrap_or(Node::List(Vec::new())))
        .collect()
}

/// The English the same script in the definition set's language gives this one's strings
/// (s51_1.stb read beside s51_0.stb).
fn sibling_strings(
    program: &Node,
    path: &Path,
    code: i64,
    found: &Context,
    tables: &BTreeMap<String, Table>,
) -> HashMap<String, String> {
    let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let base = definitions::variant_base(&name);
    if base == name {
        return HashMap::new();
    }
    let wanted = format!("_{code}.stb");
    if name.ends_with(&wanted) {
        return HashMap::new();
    }
    let sibling = path.with_file_name(format!("{}{}", &base[..base.len() - ".stb".len()], wanted));
    let Ok(data) = std::fs::read(&sibling) else {
        return HashMap::new();
    };
    let other = decompile::decompile(&stb::read(&data), &found.table, tables);
    if other.get("asm").is_some() {
        return HashMap::new();
    }
    notes::sibling_strings(&notes::strings_in(&bodies(program)), &notes::strings_in(&bodies(&other)))
}

/// The program with the game's text in comments: the messages its calls show, the names of what
/// they pass, and the English of its strings from the same script in the set's language.
fn with_messages(program: Node, found: Option<&Context>, setup: &Setup, path: &Path) -> Node {
    let (Some(found), Some(root), Some(code)) = (found, disc_root(path), disc::language_code(&setup.defs)) else {
        return program;
    };
    if program.get("asm").is_some() {
        return program;
    }
    let functions = program.list("functions");
    let texts = messages::texts_for(&messages::loaded_files(functions, found), &root, code, &mut HashMap::new());
    let params = messages::kind_params(functions, found);
    let lookups = setup.lookups(&root);
    let functions = messages::annotate_functions(functions.to_vec(), found, &params, &texts, &lookups);
    let siblings = sibling_strings(&program, path, code, found, &setup.tables);
    program.with("functions", notes::annotate_siblings(functions, &siblings))
}

/// A script's readable source, as dc2tk decompile writes it for one script.
pub fn script_source(setup: &Setup, path: &Path, table: &str) -> Result<String> {
    let program = decompile::decompile(&stb::read(&std::fs::read(path)?), table, &setup.tables);
    let found = setup.context(table);
    let program_names = found.map(|found| found.program_names.clone()).unwrap_or_default();
    let lookup = setup.known.with_loose(&names::loose_map(&program));
    let named = names::apply(program, &lookup, &sdk::reserved(found), &program_names)?;
    let named = with_messages(named, found, setup, path);
    let entry = definitions::script_entry(&setup.defs, &path.to_string_lossy()).cloned().unwrap_or_default();
    let scripted = names::with_script(named.clone(), &entry, &Default::default(), &Default::default());
    let params = match found {
        Some(found) if named.get("asm").is_none() => messages::kind_params(named.list("functions"), found),
        _ => HashMap::new(),
    };
    let found = sdk::numbered(found.cloned(), sdk::numbering_of(&entry).as_deref());
    let presented = sdk::present(scripted, found.as_ref(), &params, &[], &mut sdk::Sounds::default());
    source::text(&notes::annotate(presented, &setup.defs.strings))
}

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

/// The scripts under a folder of an extracted disc, with their bytes, by path from the folder.
pub fn read_scripts(folder: &Path, paths: &[String]) -> Result<Vec<(String, Vec<u8>)>> {
    paths
        .par_iter()
        .map(|path| {
            let file = folder.join(path);
            let data = std::fs::read(&file).with_context(|| format!("cannot read {}", file.display()))?;
            Ok((path.clone(), data))
        })
        .collect()
}

/// What decompile_tree wrote, for the command to say.
pub struct TreeWritten {
    /// What differs from the disc the definition sets were made for, when it is not this one.
    pub warning: Option<String>,
    pub scripts: usize,
    pub libraries: usize,
    /// Files the last decompile wrote and this one does not: removed, and kept since they were
    /// edited (a whole-disc decompile only).
    pub removed: Vec<String>,
    pub kept: Vec<String>,
}

fn write_file(out: &Path, relative: &str, text: &str) -> Result<()> {
    let target = out.join(relative);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("cannot create {}", parent.display()))?;
    }
    dc2tk_disc::textfile::write_text(&target, text).with_context(|| format!("cannot write {}", target.display()))
}

/// The path of a script's source in a tree without readable names: map/s/s51/s51_1.c.
fn plain_source(path: &str) -> String {
    let stem = path.rsplit_once('.').map_or(path, |(stem, _)| stem);
    format!("{stem}{}", source::EXTENSION)
}

/// dc2tk decompile EXTRACTED -o TREE: every script of an extracted disc (or a folder of one) as C,
/// with the functions scripts share as libraries, readable names for folders and scripts, disc.h
/// and the record of what was written. Files edited since the last decompile are not overwritten
/// unless `force` is set.
pub fn decompile_tree(
    stb: &Path,
    out: &Path,
    force: bool,
    defs: Definitions,
    tables: BTreeMap<String, Table>,
    sdk_dir: &Path,
) -> Result<TreeWritten> {
    let root = disc_root(&stb.join(MANIFEST)).unwrap_or_else(|| stb.to_path_buf());
    let resolved = PathBuf::from(definitions::resolved_posix(stb));
    let resolved_root = PathBuf::from(definitions::resolved_posix(&root));
    let prefix =
        resolved.strip_prefix(&resolved_root).map(|rest| rest.to_string_lossy().replace('\\', "/")).unwrap_or_default();
    let found = paths::scripts_under(stb)?;
    // a folder of the disc is compared with that folder of the set's disc
    let folder = if prefix.is_empty() { String::new() } else { format!("{prefix}/") };
    let differences = definitions::disc_differences(&read_scripts(stb, &found)?, &defs, &folder);
    let warning = (!differences.differ.is_empty() || !differences.unknown.is_empty())
        .then(|| disc_match_text(&defs, &differences));
    let paths: Vec<String> = found.iter().map(|path| format!("{folder}{path}")).collect();
    let headers_used = sdk::header_texts(&defs, sdk_dir)?;
    let (defs, headers, generated) = disc::with_header(&root, defs, headers_used.clone());
    let mut model = library::model_for(&root, &paths, &tables, &defs, &headers)?;
    let generated = slots::with_header(generated, &model.naming.folder_slots, &model.naming.villagers);
    let generated = assets::with_header(generated, &model.naming.asset_names);
    let naming = pathnames::lookups(&root, &defs);
    let mut renamed: IndexMap<String, String> = IndexMap::new();
    let libraries: Vec<(String, String)> = model
        .shared
        .libraries
        .values()
        .map(|library| (library.path.clone(), pathnames::named_library(&library.path, &naming)))
        .collect();
    for (plain, named) in &libraries {
        renamed.insert(plain.clone(), named.clone());
    }
    // the scripts include libraries by their named paths
    for library in model.shared.libraries.values_mut() {
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
    let blocked = if out.is_dir() { tree::overwrites(out, &written)? } else { Vec::new() };
    if !blocked.is_empty() && !force {
        return Err(Error::invalid(format!(
            "{} holds {} edited file{} this would overwrite, such as:\n  {}\nSave them first (dc2tk mod save {} NAME), write somewhere else, or pass --force.",
            out.display(),
            blocked.len(),
            if blocked.len() == 1 { "" } else { "s" },
            blocked.iter().take(10).cloned().collect::<Vec<_>>().join("\n  "),
            out.display()
        )));
    }
    if let Some(text) = &generated {
        write_file(out, &format!("include/{}", disc::HEADER), text)?;
    }
    paths.par_iter().try_for_each(|path| -> Result<()> {
        let program = &model.survey.programs[path];
        let text = source::text(&model.script_program(program, path)?).with_context(|| path.clone())?;
        write_file(out, &renamed[&plain_source(path)], &text)
    })?;
    // libraries last: the scripts that include one tell its sound pass what they pass it
    for (path, text) in model.library_texts()? {
        write_file(out, &path, &text)?;
    }
    write_file(out, source::EDITOR_FLAGS, &source::editor_flags(sdk_dir, &model.source.defs.sets, None))?;
    let (mut removed, mut kept) = (Vec::new(), Vec::new());
    if prefix.is_empty() {
        // a whole-disc decompile replaces the tree: files the last one wrote and this one does not
        // (a library the definitions moved) go, unless someone edited them
        (removed, kept) = tree::stale(out, &written)?;
        tree::remove_files(out, &removed)?;
    }
    tree::record(out, &written, Some(&tree::headers_hash(&headers_used)), &removed)?;
    pathnames::record(out, &renamed)?;
    Ok(TreeWritten { warning, scripts: paths.len(), libraries: library_paths.len(), removed, kept })
}
