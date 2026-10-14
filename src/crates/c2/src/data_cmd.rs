//! Game data tables a mod edits as JSON (`"data:items": "data/items.json"` in mod.json; see
//! docs/assets/data.md): `mod asset ISO NAME data:items` writes a table, and `build` turns the
//! chosen mods' tables into the disc files they come from. Most are cfg scripts in DATA.DAT (some
//! kept in several copies, some inside packs); monsters, spectrumising, Donny's shop, the
//! character tables and parts of the inventions and fishing tables are in the boot executable,
//! patched in place in the built image, every table's patches together.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde_json::{Map, Value};

use c2_assets::cfg_text::Edit;
use c2_assets::data_table::{self, Table as DataTable};
use c2_assets::item_data::{self, Items};
use c2_assets::monster_data::{self, Monsters, Patch};
use c2_assets::shop_data::{self, Shops};
use c2_assets::{exe_tables, game_data};
use c2_disc::{iso, rebuild, Image};
use c2_script::discfiles::{self, Files};
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

/// A table read from the disc, with the DATA.DAT files it came from. Items and shops carry the
/// executable tables merged into them: spectrumising (each item's `spectrumise`) and Donny's shop
/// (`donny`).
enum Table {
    Items(Items, Vec<String>, DataTable),
    Shops(Shops, Vec<String>, DataTable),
    Monsters(Monsters),
    Data(DataTable, Vec<String>),
}

/// A file by its DATA.DAT path, a pack's member as `pack.d/member`.
fn read_path(current: &dyn Fn(&str) -> Result<Vec<u8>>, path: &str) -> Result<Vec<u8>> {
    let (top, members) = discfiles::location(path);
    let data = current(&top)?;
    match rebuild::read_member(&data, &members) {
        Some(member) => Ok(member.to_vec()),
        None => bail!("{top} has no member {}", members.join("/")),
    }
}

/// The table, from the files `current` gives (what an earlier step of the build rewrote, or the
/// disc's own).
fn load(table: &str, elf: &[u8], current: &dyn Fn(&str) -> Result<Vec<u8>>) -> Result<Table> {
    let read_all =
        |paths: &[String]| -> Result<Vec<Vec<u8>>> { paths.iter().map(|path| read_path(current, path)).collect() };
    Ok(match table {
        "items" => {
            let paths = item_paths();
            let names = current(&names_path()).ok();
            let items = Items::read(&read_all(&paths)?, names.as_deref())?;
            Table::Items(items, paths, exe_tables::spectrumise(elf)?)
        }
        "shops" => {
            let paths = shop_paths();
            let names = current(&names_path()).ok();
            Table::Shops(Shops::read(&read_all(&paths)?, names.as_deref())?, paths, exe_tables::donny(elf)?)
        }
        "monsters" => Table::Monsters(Monsters::read(elf)?),
        kind if game_data::KINDS.contains(&kind) => {
            let paths = game_data::paths(kind);
            Table::Data(game_data::read(kind, &read_all(&paths)?, elf)?, paths)
        }
        _ => bail!("no data table {table} (the tables are {})", replace::DATA_TABLES.join(", ")),
    })
}

/// Each item record of `items` with the `spectrumise` of the same item from `extra`.
fn merged_items(mut items: Value, extra: &Value) -> Value {
    let spectrum: std::collections::HashMap<i64, &Value> = extra["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|record| Some((record["item"].as_i64()?, record.get("spectrumise")?)))
        .collect();
    for record in items["items"].as_array_mut().into_iter().flatten() {
        if let (Some(item), Some(object)) = (record["item"].as_i64(), record.as_object_mut()) {
            if let Some(&value) = spectrum.get(&item) {
                object.insert("spectrumise".into(), value.clone());
            }
        }
    }
    items
}

/// A mod's item table split into the cfg part and the spectrumising part.
fn split_items(given: &Value) -> (Value, Value) {
    let mut items = given.clone();
    let mut spectrum = Vec::new();
    for record in items.get_mut("items").and_then(Value::as_array_mut).into_iter().flatten() {
        if let Some(object) = record.as_object_mut() {
            if let Some(value) = object.remove("spectrumise") {
                let mut part = Map::new();
                part.insert("item".into(), object.get("item").cloned().unwrap_or(Value::Null));
                part.insert("spectrumise".into(), value);
                spectrum.push(Value::Object(part));
            }
        }
    }
    let mut extra = Map::new();
    extra.insert("items".into(), Value::Array(spectrum));
    (items, Value::Object(extra))
}

/// A mod's shop table split into the cfg part and Donny's shop.
fn split_shops(given: &Value) -> (Value, Value) {
    let mut shops = given.clone();
    let mut extra = Map::new();
    if let Some(donny) = shops.as_object_mut().and_then(|object| object.remove("donny")) {
        extra.insert("donny".into(), donny);
    }
    (shops, Value::Object(extra))
}

impl Table {
    fn to_json(&self) -> Value {
        match self {
            Table::Items(items, _, spectrum) => merged_items(items.to_json(), &spectrum.to_json()),
            Table::Shops(shops, _, donny) => {
                let mut json = shops.to_json();
                if let (Some(object), Value::Object(extra)) = (json.as_object_mut(), donny.to_json()) {
                    object.extend(extra);
                }
                json
            }
            Table::Monsters(monsters) => monsters.to_json(),
            Table::Data(table, _) => table.to_json(),
        }
    }

    /// The edits of the table's cfg files and the patches of the executable a mod's table makes.
    fn changes(&self, given: &Value, module: &str) -> Result<(Vec<Edit>, Vec<Patch>)> {
        Ok(match self {
            Table::Items(items, _, spectrum) => {
                let (given, extra) = split_items(given);
                (items.changes(&given, module)?, spectrum.changes(&extra, module)?.1)
            }
            Table::Shops(shops, _, donny) => {
                let (given, extra) = split_shops(given);
                (shops.changes(&given, module)?, donny.changes(&extra, module)?.1)
            }
            Table::Monsters(monsters) => (Vec::new(), monsters.changes(given, module)?),
            Table::Data(table, _) => table.changes(given, module)?,
        })
    }

    /// The table's files with the edits made, by its paths.
    fn apply(&self, edits: &[Edit]) -> Result<(Vec<Vec<u8>>, &[String])> {
        Ok(match self {
            Table::Items(items, paths, _) => (items.apply(edits)?, paths),
            Table::Shops(shops, paths, _) => (shops.apply(edits)?, paths),
            Table::Monsters(_) => (Vec::new(), &[]),
            Table::Data(table, paths) => (table.apply(edits)?, paths),
        })
    }
}

/// The table as the JSON text `mod asset` writes.
pub fn export(iso_path: &Path, table: &str) -> Result<String> {
    let disc = Disc::open(iso_path)?;
    let loaded = load(table, &executable(iso_path)?, &|path| disc.read(path))?;
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
/// `mod check --disc` and `build` both run. `files` holds what the build already rewrote; the
/// executable comes back patched when any table changes it.
pub fn apply_data(
    iso_path: &Path,
    mod_dirs: &[PathBuf],
    files: &Files,
) -> Result<(Files, Option<Vec<u8>>, Vec<DataChange>)> {
    let lines: Vec<Line> = replace::all_lines(mod_dirs)?.into_iter().filter(Line::is_data).collect();
    let mut out = files.clone();
    let mut changes = Vec::new();
    if lines.is_empty() {
        return Ok((out, None, changes));
    }
    let disc = Disc::open(iso_path)?;
    let elf = executable(iso_path)?;
    let mut patches: Vec<Patch> = Vec::new();
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
        let loaded = load(table, &elf, &current)?;
        let mut change = DataChange { table: table.to_string(), changed: Vec::new(), files: Vec::new() };
        let mut edits: Vec<Edit> = Vec::new();
        let mut patched = false;
        for line in &mine {
            let (found, found_patches) = loaded
                .changes(&read_table(line)?, &line.module)
                .with_context(|| format!("{} (mod {})", line.key, line.module))?;
            change.changed.extend(found.iter().map(|edit| (edit.module.clone(), edit.what.clone())));
            change.changed.extend(found_patches.iter().map(|patch| (patch.module.clone(), patch.what.clone())));
            patched |= !found_patches.is_empty();
            edits.extend(found);
            patches.extend(found_patches);
        }
        // every file of the table, each member put back into its pack
        let (written, paths) = loaded.apply(&edits)?;
        let mut rewritten: Vec<(String, Vec<u8>)> = Vec::new();
        for (path, data) in paths.iter().zip(written) {
            if data == read_path(&current, path)? {
                continue;
            }
            let (top, members) = discfiles::location(path);
            let holder = match rewritten.iter().position(|(known, _)| *known == top) {
                Some(at) => rewritten.remove(at).1,
                None => current(&top)?,
            };
            rewritten.push((top, rebuild::replaced_member(&holder, &members, &data)?));
        }
        for (top, data) in &rewritten {
            change.files.push((top.clone(), data.len(), current(top)?.len()));
        }
        if patched {
            change.files.push((monster_data::EXECUTABLE.to_string(), elf.len(), elf.len()));
        }
        out.extend(rewritten);
        changes.push(change);
    }
    let executable_out = if patches.is_empty() { None } else { Some(data_table::apply_patches(&elf, &patches)?) };
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
    wrong.sort();
    wrong.dedup();
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
    use serde_json::json;

    #[test]
    fn data_paths_are_where_the_engine_reads() {
        assert_eq!(item_paths()[1], "menu/cfg7/wepdat.cfg");
        assert_eq!(shop_paths()[0], "menu/shopusa1.cfg");
        assert_eq!(shop_paths()[15], "menu/shoplst8.cfg");
        assert_eq!(names_path(), "menu/cfg7/comdatmes1.cfg");
        assert_eq!(discfiles::location("menu/0/inv2_bg.pac.d/inv6.lst").0, "menu/0/inv2_bg.pac");
    }

    #[test]
    fn executable_parts_split_from_and_merge_into_their_tables() {
        let given = json!({"items": [{"item": 1, "max": 5, "spectrumise": {"value": 3}}, {"item": 2}]});
        let (items, extra) = split_items(&given);
        assert_eq!(items, json!({"items": [{"item": 1, "max": 5}, {"item": 2}]}));
        assert_eq!(extra, json!({"items": [{"item": 1, "spectrumise": {"value": 3}}]}));
        assert_eq!(merged_items(items, &extra)["items"][0]["spectrumise"]["value"], 3);
        let (shops, extra) = split_shops(&json!({"prices": [], "donny": [{"row": 0, "level": 2}]}));
        assert_eq!((shops, extra["donny"][0]["level"].clone()), (json!({"prices": []}), json!(2)));
    }
}
