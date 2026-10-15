//! Decompiling a script to readable source: the program tree (decompile), named from the
//! definition set (names), commented with the game's text (messages, notes) and shown through the
//! SDK (sdk::present). What it needs that does not change between scripts is read once (Setup).

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use indexmap::IndexMap;

use crate::definitions::{self, Definitions};
use crate::messages::{self, NameTable};
use crate::natives::{self, Table};
use crate::node::Node;
use crate::sdk::{self, Context};
use crate::{decompile, disc, names, notes, parse, source, stb, Result};

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
    full.ancestors().skip(1).find(|folder| folder.join("manifest.json").is_file()).map(Path::to_path_buf)
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
