//! The engine's native function tables (event, monster, action, effect, arg), generated from the
//! ChronicleTwo decompilation into JSON, and which table a script calls into.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::Result;

pub const TABLES: [&str; 5] = ["event", "monster", "action", "effect", "arg"];

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Native {
    pub no: i32,
    pub name: String,
    pub accessors: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Table {
    pub table: String,
    pub source: String,
    pub array: String,
    pub natives: Vec<Native>,
}

impl Table {
    /// The natives by number.
    pub fn numbered(&self) -> BTreeMap<i32, &Native> {
        self.natives.iter().map(|native| (native.no, native)).collect()
    }
}

/// The tables in a folder, by name; a table without a file is left out.
pub fn load(directory: &Path) -> Result<BTreeMap<String, Table>> {
    let mut tables = BTreeMap::new();
    for name in TABLES {
        let path = directory.join(format!("{name}.json"));
        if path.exists() {
            let table: Table = serde_json::from_str(&fs::read_to_string(&path)?)
                .map_err(|error| crate::Error::invalid(format!("{}: {error}", path.display())))?;
            tables.insert(name.to_string(), table);
        }
    }
    Ok(tables)
}

/// The table a script calls into, by the first rule (path prefix, table) its path fits; "event"
/// when none does.
pub fn table_for<'a>(path: &str, rules: &'a [(String, String)]) -> &'a str {
    let text = path.replace('\\', "/");
    for (prefix, table) in rules {
        if text.starts_with(prefix.as_str()) || text.contains(&format!("/{prefix}")) {
            return table;
        }
    }
    "event"
}
