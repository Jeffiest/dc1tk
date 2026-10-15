//! How much of a disc a definition set names (defs check), the functions it still leaves unnamed,
//! most called first (defs todo), and the record of which disc a set is for (defs disc).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use dc2tk_disc::files;
use indexmap::IndexMap;
use rayon::prelude::*;

use crate::definitions::{self, Definitions};
use crate::natives::{self, Table};
use crate::node::Node;
use crate::{fingerprint, library, names, stb, Result};

/// One function of a disc script: its table and fingerprints.
pub struct DiscFunction {
    pub table: String,
    pub exact: String,
    pub loose: String,
}

/// Every function of the disc's scripts (paths from paths::scripts_under), script by script.
pub fn disc_functions(root: &Path, paths: &[String], defs: &Definitions) -> Result<Vec<DiscFunction>> {
    let per_script: Vec<Vec<DiscFunction>> = paths
        .par_iter()
        .map(|path| -> Result<Vec<DiscFunction>> {
            let table = natives::table_for(path, &defs.tables);
            let prints = fingerprint::both_fingerprints(&stb::read(&files::read(root.join(path))?), table);
            Ok(prints
                .exact
                .iter()
                .map(|(offset, exact)| DiscFunction {
                    table: table.to_string(),
                    exact: exact.clone(),
                    loose: prints.loose[offset].clone(),
                })
                .collect())
        })
        .collect::<Result<_>>()?;
    Ok(per_script.into_iter().flatten().collect())
}

/// The counts of one native table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TableCounts {
    pub functions: usize,
    pub named_exact: usize,
    pub named_loose: usize,
    pub classes: usize,
    pub named_classes: usize,
}

/// What defs check reports.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub tables: BTreeMap<String, TableCounts>,
    pub stale_names: Vec<String>,
    pub stale_scripts: Vec<String>,
    pub scripts_documented: usize,
    pub scripts: usize,
}

/// How much of a disc the definitions name.
pub fn check(root: &Path, paths: &[String], defs: &Definitions) -> Result<Report> {
    let functions = disc_functions(root, paths, defs)?;
    let entries = names::merged(&defs.names);
    let named_loose: HashSet<&str> = entries
        .values()
        .filter(|entry| entry.get("name").is_some())
        .filter_map(|entry| entry.get("loose").and_then(Node::as_str))
        .collect();
    let exact_on_disc: HashSet<&str> = functions.iter().map(|function| function.exact.as_str()).collect();
    let loose_on_disc: HashSet<&str> = functions.iter().map(|function| function.loose.as_str()).collect();
    let mut by_table: BTreeMap<String, (TableCounts, HashSet<&str>, HashSet<&str>)> = BTreeMap::new();
    for function in &functions {
        let (counts, classes, named) = by_table.entry(function.table.clone()).or_default();
        counts.functions += 1;
        classes.insert(&function.loose);
        if entries.get(&function.exact).is_some_and(|entry| entry.get("name").is_some()) {
            counts.named_exact += 1;
            named.insert(&function.loose);
        } else if named_loose.contains(function.loose.as_str()) {
            counts.named_loose += 1;
            named.insert(&function.loose);
        }
    }
    let tables = by_table
        .into_iter()
        .map(|(table, (counts, classes, named))| {
            (table, TableCounts { classes: classes.len(), named_classes: named.len(), ..counts })
        })
        .collect();
    let on_disc: HashSet<&String> = paths.iter().collect();
    let mut stale_names: Vec<String> = entries
        .iter()
        .filter(|(key, entry)| {
            !exact_on_disc.contains(key.as_str())
                && !entry.get("loose").and_then(Node::as_str).is_some_and(|loose| loose_on_disc.contains(loose))
        })
        .map(|(key, _)| key.clone())
        .collect();
    stale_names.sort();
    let mut stale_scripts: Vec<String> = defs.scripts.keys().filter(|path| !on_disc.contains(path)).cloned().collect();
    stale_scripts.sort();
    let scripts_documented = on_disc
        .iter()
        .filter(|path| {
            definitions::script_entry(defs, path).is_some_and(|entry| entry.get("doc").is_some_and(Node::is_set))
        })
        .count();
    Ok(Report { tables, stale_names, stale_scripts, scripts_documented, scripts: on_disc.len() })
}

pub fn format_check(report: &Report) -> String {
    let mut lines = vec![format!("{:<8} {:>9} {:>9} {:>9} {:>9}", "table", "functions", "named", "classes", "named")];
    for (table, counts) in &report.tables {
        lines.push(format!(
            "{:<8} {:>9} {:>9} {:>9} {:>9}",
            table,
            counts.functions,
            counts.named_exact + counts.named_loose,
            counts.classes,
            counts.named_classes
        ));
    }
    lines.push(String::new());
    lines.push(format!("scripts with a doc comment: {} of {}", report.scripts_documented, report.scripts));
    lines.push(format!("name entries matching nothing on this disc: {}", report.stale_names.len()));
    lines.push(format!("script entries for paths not on this disc: {}", report.stale_scripts.len()));
    lines.join("\n")
}

/// One unnamed function: how often the disc's scripts call it, and the first script and name it
/// has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unnamed {
    pub calls: usize,
    pub path: String,
    pub name: String,
}

/// The functions the definitions leave unnamed (language variants counted once, by loose
/// fingerprint), most called first, up to limit; and how many there are.
pub fn todo(
    root: &Path,
    paths: &[String],
    defs: &Definitions,
    tables: &BTreeMap<String, Table>,
    headers: &IndexMap<String, String>,
    limit: usize,
) -> Result<(Vec<Unnamed>, usize)> {
    let model = library::model_for(root, paths, tables, defs, headers)?;
    let known_entries = names::Known::new(names::merged(&defs.names));
    let entries = known_entries.with_loose(&model.survey.exact_to_loose);
    let mut known: HashMap<String, String> =
        model.shared.own_names.iter().map(|(print, name)| (print.clone(), name.clone())).collect();
    known.extend(model.shared.class_names.iter().map(|(print, name)| (print.clone(), name.clone())));
    let nobody = HashMap::new();
    let mut calls: HashMap<String, usize> = HashMap::new();
    let mut seen: IndexMap<String, (String, String)> = IndexMap::new();
    for path in paths {
        let table = natives::table_for(path, &defs.tables);
        let program = &model.survey.programs[path];
        if program.get("asm").is_some() {
            continue;
        }
        let found = names::fingerprints(program);
        let context = model.naming.contexts.get(table).and_then(Option::as_ref);
        let program_names = context.map_or(&nobody, |context| &context.program_names);
        let mapping = names::assign(program, &found, &known, model.reserved_names(), program_names);
        for function in program["functions"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
            let body = function.get("body").cloned().unwrap_or(Node::List(vec![]));
            for callee in names::callees(&body) {
                if let Some(Some(exact)) = found.get(&callee) {
                    *calls.entry(model.survey.exact_to_loose[exact].clone()).or_default() += 1;
                }
            }
        }
        for (name, exact) in &found {
            let Some(exact) = exact else {
                continue;
            };
            if entries.get(exact).is_none_or(|entry| entry.get("name").is_none()) {
                let loose = model.survey.exact_to_loose[exact].clone();
                seen.entry(loose).or_insert_with(|| (path.clone(), mapping[name].clone()));
            }
        }
    }
    let count = |loose: &String| calls.get(loose).copied().unwrap_or(0);
    let mut ranked: Vec<&String> = seen.keys().collect();
    ranked.sort_by(|a, b| count(b).cmp(&count(a)).then_with(|| seen[*a].cmp(&seen[*b])));
    let rows = ranked
        .iter()
        .take(limit)
        .map(|loose| Unnamed { calls: count(loose), path: seen[*loose].0.clone(), name: seen[*loose].1.clone() })
        .collect();
    Ok((rows, seen.len()))
}

/// The hashes of an extracted disc's scripts (paths from paths::scripts_under), by path.
pub fn disc_hashes(root: &Path, paths: &[String]) -> Result<Vec<(String, String)>> {
    paths.par_iter().map(|path| Ok((path.clone(), definitions::script_hash(&files::read(root.join(path))?)))).collect()
}

/// Writes a set's disc.json: the hashes of the disc's scripts. Returns how many.
pub fn write_disc(root: &Path, paths: &[String], directory: &Path) -> Result<usize> {
    let mut found = disc_hashes(root, paths)?;
    found.sort();
    let scripts: serde_json::Map<String, serde_json::Value> =
        found.iter().map(|(path, hash)| (path.clone(), serde_json::Value::from(hash.as_str()))).collect();
    let text = dc2tk_disc::textfile::json_text(&serde_json::json!({"scripts": scripts}));
    dc2tk_disc::textfile::write_text(&directory.join(definitions::DISC), &text)?;
    Ok(found.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scripts(disc: &Path, paths: &[&str]) -> Vec<(String, Vec<u8>)> {
        paths.iter().map(|path| (path.to_string(), std::fs::read(disc.join(path)).unwrap())).collect()
    }

    #[test]
    fn a_set_knows_which_disc_it_was_made_for() {
        let folder = std::env::temp_dir().join(format!("dc2tk-coverage-disc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let (disc, set) = (folder.join("disc"), folder.join("set"));
        std::fs::create_dir_all(disc.join("map")).unwrap();
        std::fs::create_dir_all(&set).unwrap();
        std::fs::write(disc.join("map/a.stb"), b"one").unwrap();
        std::fs::write(disc.join("map/b.stb"), b"two").unwrap();
        let paths = ["map/a.stb".to_string(), "map/b.stb".to_string()];
        assert_eq!(write_disc(&disc, &paths, &set).unwrap(), 2);
        let defs = definitions::load(std::slice::from_ref(&set)).unwrap();
        let differences = definitions::disc_differences(&scripts(&disc, &["map/a.stb", "map/b.stb"]), &defs, "");
        assert_eq!(differences, definitions::DiscDifferences::default());
        std::fs::write(disc.join("map/a.stb"), b"changed").unwrap();
        std::fs::write(disc.join("map/c.stb"), b"new").unwrap();
        let differences = definitions::disc_differences(&scripts(&disc, &["map/a.stb", "map/c.stb"]), &defs, "");
        let want = definitions::DiscDifferences {
            differ: vec!["map/a.stb".into()],
            missing: vec!["map/b.stb".into()],
            unknown: vec!["map/c.stb".into()],
        };
        assert_eq!(differences, want);
        let _ = std::fs::remove_dir_all(&folder);
    }
}
