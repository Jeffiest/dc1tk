//! Game data tables a mod edits as JSON (`"data:items": "data/items.json"` in mod.json; see
//! docs/assets/data.md): `mod asset ISO NAME data:items` writes a table, and `build` turns the
//! chosen mods' tables into the disc files they come from. Items (menu/cfg7) and shops (the shop
//! lists of menu/) are cfg scripts in DATA.DAT; monsters are a table in the boot executable,
//! patched in place in the built image.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde_json::Value;

use c2_assets::cfg_text::Edit;
use c2_assets::item_data::{self, Items};
use c2_assets::monster_data::{self, Monsters, Patch};
use c2_assets::shop_data::{self, Shops};
use c2_disc::{iso, Image};
use c2_script::discfiles::Files;
use c2_script::replace::{self, Line};

use crate::map_cmd::Disc;

/// The boot executable's bytes, from the image's directory tree or an extracted disc's folder.
pub fn executable(iso_path: &Path) -> Result<Vec<u8>> {
    if iso_path.is_file() {
        let image = Image::open(iso_path)?;
        let record = iso::find(image.bytes(), monster_data::EXECUTABLE)?
            .with_context(|| format!("the disc has no {}", monster_data::EXECUTABLE))?;
        return Ok(iso::read_file(image.bytes(), &record).to_vec());
    }
    let path = iso_path.join(monster_data::EXECUTABLE);
    fs::read(&path).with_context(|| format!("cannot read {}", path.display()))
}

fn item_paths() -> Vec<String> {
    item_data::FILES.iter().map(|file| format!("{}/{file}", item_data::FOLDER)).collect()
}

fn shop_paths() -> Vec<String> {
    shop_data::files().iter().map(|file| format!("{}/{file}", shop_data::FOLDER)).collect()
}

fn names_path() -> String {
    format!("{}/{}", item_data::FOLDER, item_data::NAMES)
}

/// A table read from the disc, with the DATA.DAT files it came from.
enum Table {
    Items(Items, Vec<String>),
    Shops(Shops, Vec<String>),
    Monsters(Monsters),
}

/// The table, from the files `current` gives (what an earlier step of the build rewrote, or the
/// disc's own).
fn load(iso_path: &Path, table: &str, current: &dyn Fn(&str) -> Result<Vec<u8>>) -> Result<Table> {
    let read_all = |paths: &[String]| -> Result<Vec<Vec<u8>>> { paths.iter().map(|path| current(path)).collect() };
    Ok(match table {
        "items" => {
            let paths = item_paths();
            let names = current(&names_path()).ok();
            Table::Items(Items::read(&read_all(&paths)?, names.as_deref())?, paths)
        }
        "shops" => {
            let paths = shop_paths();
            let names = current(&names_path()).ok();
            Table::Shops(Shops::read(&read_all(&paths)?, names.as_deref())?, paths)
        }
        "monsters" => Table::Monsters(Monsters::read(&executable(iso_path)?)?),
        _ => bail!("no data table {table} (the tables are {})", replace::DATA_TABLES.join(", ")),
    })
}

impl Table {
    fn to_json(&self) -> Value {
        match self {
            Table::Items(items, _) => items.to_json(),
            Table::Shops(shops, _) => shops.to_json(),
            Table::Monsters(monsters) => monsters.to_json(),
        }
    }
}

/// The table as the JSON text `mod asset` writes.
pub fn export(iso_path: &Path, table: &str) -> Result<String> {
    let disc = Disc::open(iso_path)?;
    let loaded = load(iso_path, table, &|path| disc.read(path))?;
    Ok(item_data::json_text(&loaded.to_json()))
}

/// Writes each data: key's table into the mod (data/<table>.json) and adds its line to mod.json;
/// a table the mod has a line for already is left as it is. Returns (key, file, whether written).
pub fn copy_out(iso_path: &Path, mod_dir: &Path, keys: &[String]) -> Result<Vec<(String, PathBuf, bool)>> {
    let known: Vec<Line> =
        if mod_dir.join(c2_script::mods::INDEX).is_file() { replace::lines(mod_dir)? } else { Vec::new() };
    let mut out = Vec::new();
    let mut new_lines = Vec::new();
    for key in keys {
        let table = key[replace::DATA.len()..].trim().to_lowercase();
        if !replace::DATA_TABLES.contains(&table.as_str()) {
            bail!(
                "{key}: the data tables are {}",
                replace::DATA_TABLES.map(|table| format!("data:{table}")).join(", ")
            );
        }
        let id = format!("{}{table}", replace::DATA);
        if let Some(line) = known.iter().find(|line| line.is_data() && line.data_table() == table) {
            out.push((id, line.file.clone(), false));
            continue;
        }
        let relative = format!("data/{table}.json");
        let file = mod_dir.join("data").join(format!("{table}.json"));
        fs::create_dir_all(file.parent().unwrap())?;
        fs::write(&file, export(iso_path, &table)?)?;
        new_lines.push((id.clone(), Value::String(relative)));
        out.push((id, file, true));
    }
    replace::add_lines(mod_dir, &new_lines)?;
    Ok(out)
}

/// What the chosen mods' data: lines changed in one table.
pub struct DataChange {
    pub table: String,
    /// What each mod's line changes, one per changed value: (mod, what).
    pub changed: Vec<(String, String)>,
    /// The files rewritten: (path, new size, old size).
    pub files: Vec<(String, usize, usize)>,
}

fn read_table(line: &Line) -> Result<Value> {
    let text = fs::read_to_string(&line.file).with_context(|| line.file.display().to_string())?;
    serde_json::from_str(&text).with_context(|| format!("{} (mod {}): {}", line.key, line.module, line.shown))
}

/// The edits of the chosen mods' data: lines, table by table, checked against the disc: what
/// `mod check --disc` and `build` both run. `files` holds what the build already rewrote.
pub fn apply_data(
    iso_path: &Path,
    mod_dirs: &[PathBuf],
    files: &Files,
) -> Result<(Files, Option<Vec<u8>>, Vec<DataChange>)> {
    let lines: Vec<Line> = replace::all_lines(mod_dirs)?.into_iter().filter(Line::is_data).collect();
    let mut out = files.clone();
    let mut executable_out = None;
    let mut changes = Vec::new();
    if lines.is_empty() {
        return Ok((out, executable_out, changes));
    }
    let disc = Disc::open(iso_path)?;
    for table in replace::DATA_TABLES {
        let mine: Vec<&Line> = lines.iter().filter(|line| line.data_table() == table).collect();
        if mine.is_empty() {
            continue;
        }
        let current = |path: &str| -> Result<Vec<u8>> {
            match out.iter().find(|(known, _)| known.eq_ignore_ascii_case(path)) {
                Some((_, data)) => Ok(data.clone()),
                None => disc.read(path),
            }
        };
        let loaded = load(iso_path, table, &current)?;
        let mut change = DataChange { table: table.to_string(), changed: Vec::new(), files: Vec::new() };
        let mut rewritten: Vec<(String, Vec<u8>)> = Vec::new();
        let context = |line: &Line| format!("{} (mod {})", line.key, line.module);
        match &loaded {
            Table::Items(_, paths) | Table::Shops(_, paths) => {
                let mut edits: Vec<Edit> = Vec::new();
                for line in &mine {
                    let given = read_table(line)?;
                    let found = match &loaded {
                        Table::Items(items, _) => items.changes(&given, &line.module),
                        Table::Shops(shops, _) => shops.changes(&given, &line.module),
                        Table::Monsters(_) => unreachable!(),
                    }
                    .with_context(|| context(line))?;
                    change.changed.extend(found.iter().map(|edit| (edit.module.clone(), edit.what.clone())));
                    edits.extend(found);
                }
                let written = match &loaded {
                    Table::Items(items, _) => items.apply(&edits),
                    Table::Shops(shops, _) => shops.apply(&edits),
                    Table::Monsters(_) => unreachable!(),
                }?;
                for (path, data) in paths.iter().zip(written) {
                    let old = current(path)?;
                    if data != old {
                        change.files.push((path.clone(), data.len(), old.len()));
                        rewritten.push((path.clone(), data));
                    }
                }
            }
            Table::Monsters(monsters) => {
                let mut patches: Vec<Patch> = Vec::new();
                for line in &mine {
                    let found = monsters.changes(&read_table(line)?, &line.module).with_context(|| context(line))?;
                    change.changed.extend(found.iter().map(|patch| (patch.module.clone(), patch.what.clone())));
                    patches.extend(found);
                }
                if !patches.is_empty() {
                    let patched = monsters.apply(&patches)?;
                    change.files.push((monster_data::EXECUTABLE.to_string(), patched.len(), patched.len()));
                    executable_out = Some(patched);
                }
            }
        }
        out.extend(rewritten);
        changes.push(change);
    }
    Ok((out, executable_out, changes))
}

/// The data files that read back from the built image other than as written.
pub fn check(out_path: &Path, changes: &[DataChange], files: &Files, executable: Option<&[u8]>) -> Result<Vec<String>> {
    let mut wrong = Vec::new();
    if changes.iter().all(|change| change.files.is_empty()) {
        return Ok(wrong);
    }
    let disc = Disc::open(out_path)?;
    for change in changes {
        for (path, _, _) in &change.files {
            let wanted = if path == monster_data::EXECUTABLE {
                executable.map(<[u8]>::to_vec)
            } else {
                files.get(path).cloned()
            };
            let read =
                if path == monster_data::EXECUTABLE { self::executable(out_path).ok() } else { disc.read(path).ok() };
            if wanted.is_none() || read != wanted {
                wrong.push(path.clone());
            }
        }
    }
    Ok(wrong)
}

/// The lines build and mod check print for a table's changes.
pub fn described(change: &DataChange) -> Vec<String> {
    let mut out = Vec::new();
    let mut modules: Vec<&String> = Vec::new();
    for (module, _) in &change.changed {
        if !modules.contains(&module) {
            modules.push(module);
        }
    }
    for module in modules {
        // one value can be in several files (a chapter's shop list in both languages' files)
        let mut whats: Vec<&String> = Vec::new();
        for (_, what) in change.changed.iter().filter(|(owner, _)| owner == module) {
            if !whats.contains(&what) {
                whats.push(what);
            }
        }
        let shown: Vec<&str> = whats.iter().take(6).map(|what| what.as_str()).collect();
        let more =
            if whats.len() > shown.len() { format!(" and {} more", whats.len() - shown.len()) } else { String::new() };
        out.push(format!(
            "{module} (data:{}): {} values changed: {}{more}",
            change.table,
            whats.len(),
            shown.join("; ")
        ));
    }
    if change.changed.is_empty() {
        out.push(format!("data:{}: the tables given change nothing", change.table));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_paths_are_where_the_engine_reads() {
        assert_eq!(item_paths()[1], "menu/cfg7/wepdat.cfg");
        assert_eq!(shop_paths()[0], "menu/shopusa1.cfg");
        assert_eq!(shop_paths()[15], "menu/shoplst8.cfg");
        assert_eq!(names_path(), "menu/cfg7/comdatmes1.cfg");
    }
}
