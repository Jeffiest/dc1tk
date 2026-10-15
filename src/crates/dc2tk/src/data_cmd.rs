//! Game data tables a mod edits as JSON (`"data:items": "data/items.json"` in mod.json; see
//! docs/assets/data.md): `mod asset ISO NAME data:items` writes a table, and `build` turns the
//! chosen mods' tables into the disc files they come from. Most are cfg scripts in DATA.DAT (some
//! kept in several copies, some inside packs); monsters, spectrumising, Donny's shop, the
//! character tables and parts of the inventions and fishing tables are in the boot executable,
//! patched in place in the built image, every table's patches together. The inventions table
//! carries the ideas' names (`ideas`, the copies of `neta2.lst`).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{Map, Value};

use dc2tk_assets::cfg_text::Edit;
use dc2tk_assets::chest_data::{self, Chests};
use dc2tk_assets::data_table::Table as DataTable;
use dc2tk_assets::dungeon_layout::{self, Layouts};
use dc2tk_assets::elf::{self, Patch};
use dc2tk_assets::idea_data::{self, Ideas};
use dc2tk_assets::item_data::{self, ItemText, Items};
use dc2tk_assets::shop_data::{self, Shops};
use dc2tk_assets::{exe_tables, game_data, json, text};
use dc2tk_disc::{iso, rebuild, Image};
use dc2tk_script::discfiles::{self, Files};
use dc2tk_script::replace::{self, Line};

use crate::map_cmd::Disc;

/// The boot executable's bytes, from the image's directory tree or an extracted disc's folder.
pub fn executable(iso_path: &Path) -> Result<Vec<u8>> {
    if iso_path.is_file() {
        let image = Image::open(iso_path)?;
        let record = iso::find(image.bytes(), elf::EXECUTABLE)?
            .with_context(|| format!("the disc has no {}", elf::EXECUTABLE))?;
        return Ok(iso::read_file(image.bytes(), &record).to_vec());
    }
    let path = iso_path.join(elf::EXECUTABLE);
    fs::read(&path).with_context(|| format!("cannot read {}", path.display()))
}

/// The item tables and every language's item names.
fn item_paths() -> Vec<String> {
    item_data::FILES.iter().chain(&item_data::NAME_FILES).map(|file| format!("{}/{file}", item_data::FOLDER)).collect()
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
    /// A game_data kind's table, by the paths it read (none for one of the executable alone).
    Data(DataTable, Vec<String>),
    /// The inventions (recipes and scoops) and the ideas, by the paths of both, the recipes' first.
    Inventions(DataTable, Ideas, Vec<String>),
    /// Dungeon floor layouts, which add and rewrite whole rooms rather than edit arguments.
    Layouts(Layouts),
    /// What dungeon chests hold, rewritten by whole groups and floors as layouts are.
    Chests(Chests),
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
            let items = Items::read(&read_all(&paths)?, None)?;
            Table::Items(items, paths, exe_tables::spectrumise(elf)?)
        }
        "shops" => {
            let paths = shop_paths();
            let names = current(&names_path()).ok();
            Table::Shops(Shops::read(&read_all(&paths)?, names.as_deref())?, paths, exe_tables::donny(elf)?)
        }
        "layouts" => Table::Layouts(Layouts::read(&read_all(&dungeon_layout::paths())?, dungeon_layout::parts(elf)?)?),
        "chests" => Table::Chests(Chests::read(&read_all(&chest_data::paths())?)?),
        _ => {
            let Ok(kind) = game_data::kind(table) else {
                bail!("no data table {table} (the tables are {})", replace::DATA_TABLES.join(", "));
            };
            let mut paths = (kind.paths)();
            let read = (kind.read)(&read_all(&paths)?, elf)?;
            if table == "inventions" {
                let ideas = Ideas::read(&read_all(&idea_data::paths())?)?;
                paths.extend(idea_data::paths());
                Table::Inventions(read, ideas, paths)
            } else {
                Table::Data(read, paths)
            }
        }
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

/// A mod's invention table split into the recipes and scoops and the ideas (a list).
fn split_inventions(given: &Value) -> (Value, Value) {
    let mut inventions = given.clone();
    let ideas = inventions.as_object_mut().and_then(|object| object.remove("ideas"));
    (inventions, ideas.unwrap_or_else(|| Value::Array(Vec::new())))
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
            Table::Data(table, _) => table.to_json(),
            Table::Inventions(table, ideas, _) => {
                let mut json = table.to_json();
                if let Some(object) = json.as_object_mut() {
                    object.insert("ideas".into(), ideas.to_json());
                }
                json
            }
            Table::Layouts(layouts) => layouts.to_json(),
            Table::Chests(chests) => chests.to_json(),
        }
    }

    /// The edits of the table's cfg files, the patches of the executable and the item messages
    /// the mods' tables make, (mod, table) in order: records several mods add are counted
    /// together.
    fn changes(&self, given: &[(String, Value)]) -> Result<(Vec<Edit>, Vec<Patch>, Vec<ItemText>)> {
        let mut patches = Vec::new();
        Ok(match self {
            Table::Items(items, _, spectrum) => {
                let mut tables = Vec::new();
                for (module, table) in given {
                    let (table, extra) = split_items(table);
                    patches.extend(spectrum.changes(&extra, module).with_context(|| format!("mod {module}"))?.1);
                    tables.push((module.clone(), table));
                }
                let (edits, texts) = items.changes_all(&tables)?;
                (edits, patches, texts)
            }
            Table::Shops(shops, _, donny) => {
                let mut tables = Vec::new();
                for (module, table) in given {
                    let (table, extra) = split_shops(table);
                    patches.extend(donny.changes(&extra, module).with_context(|| format!("mod {module}"))?.1);
                    tables.push((module.clone(), table));
                }
                (shops.changes_all(&tables)?, patches, Vec::new())
            }
            Table::Data(table, _) => {
                let (edits, patches) = table.changes_all(given)?;
                (edits, patches, Vec::new())
            }
            Table::Inventions(table, ideas, paths) => {
                let (mut tables, mut lists) = (Vec::new(), Vec::new());
                for (module, given) in given {
                    let (inventions, ideas) = split_inventions(given);
                    tables.push((module.clone(), inventions));
                    lists.push((module.clone(), ideas));
                }
                let (mut edits, patches) = table.changes_all(&tables)?;
                // the ideas' files follow the recipes'
                let first = paths.len() - idea_data::paths().len();
                edits.extend(
                    ideas.changes_all(&lists)?.into_iter().map(|edit| Edit { file: edit.file + first, ..edit }),
                );
                (edits, patches, Vec::new())
            }
            Table::Layouts(_) | Table::Chests(_) => bail!("layouts and chests are changed whole, in apply_data"),
        })
    }

    /// The table's files with the edits made, by its paths.
    fn apply(&self, edits: &[Edit]) -> Result<(Vec<Vec<u8>>, &[String])> {
        Ok(match self {
            Table::Items(items, paths, _) => (items.apply(edits)?, paths),
            Table::Shops(shops, paths, _) => (shops.apply(edits)?, paths),
            Table::Layouts(_) | Table::Chests(_) => (Vec::new(), &[]),
            Table::Data(table, paths) => (table.apply(edits)?, paths),
            Table::Inventions(table, ideas, paths) => {
                let first = paths.len() - idea_data::paths().len();
                let (mine, theirs): (Vec<Edit>, Vec<Edit>) = edits.iter().cloned().partition(|edit| edit.file < first);
                let theirs: Vec<Edit> =
                    theirs.into_iter().map(|edit| Edit { file: edit.file - first, ..edit }).collect();
                let mut written = table.apply(&mine)?;
                written.extend(ideas.apply(&theirs)?);
                (written, paths)
            }
        })
    }
}

/// The table as the JSON text `mod asset` writes.
pub fn export(iso_path: &Path, table: &str) -> Result<String> {
    let disc = Disc::open(iso_path)?;
    let loaded = load(table, &executable(iso_path)?, &|path| disc.read(path))?;
    Ok(json::json_text(&loaded.to_json()))
}

/// Writes each data: key's table into the mod (data/<table>.json) and adds its line to mod.json;
/// a table the mod has a line for already is left as it is. Returns (key, file, whether written).
pub fn copy_out(iso_path: &Path, mod_dir: &Path, keys: &[String]) -> Result<Vec<(String, PathBuf, bool)>> {
    let known: Vec<Line> =
        if mod_dir.join(dc2tk_script::mods::INDEX).is_file() { replace::lines(mod_dir)? } else { Vec::new() };
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

/// The letters past the English font's own table that the chosen mods' data:items English names
/// and descriptions use, for the letters the build draws into the English font
/// (replace::english_letters): ("data:items", mod, letters).
pub fn english_item_letters(iso_path: &Path, mod_dirs: &[PathBuf]) -> Result<Vec<(String, String, BTreeSet<char>)>> {
    let lines: Vec<Line> = replace::all_lines(mod_dirs)?
        .into_iter()
        .filter(|line| line.is_data() && line.data_table() == "items")
        .collect();
    if lines.is_empty() {
        return Ok(Vec::new());
    }
    let disc = Disc::open(iso_path)?;
    let loaded = load("items", &executable(iso_path)?, &|path| disc.read(path))?;
    let mut given = Vec::new();
    for line in &lines {
        given.push((line.module.clone(), read_table(line)?));
    }
    let mine: Vec<&Line> = lines.iter().collect();
    let (_, _, texts) = loaded.changes(&given).with_context(|| format!("data:items ({})", modules_of(&mine)))?;
    let fonts = text::Fonts { english_table: disc.read(text::ENGLISH_FONT).ok(), ..Default::default() };
    let mut out = Vec::new();
    for item_text in texts.iter().filter(|item_text| item_text.language == text::Language::English as usize) {
        let Some(body) = &item_text.text else {
            continue;
        };
        let letters = fonts
            .english_letters(body)
            .map_err(|error| anyhow!("{} (mod {}): {error}", item_text.what, item_text.module))?;
        out.push(("data:items".to_string(), item_text.module.clone(), letters));
    }
    Ok(out)
}

/// The edits of the chosen mods' data: lines, table by table, checked against the disc: what
/// `mod check --disc` and `build` both run. `files` holds what the build already rewrote; the
/// executable comes back patched when any table changes it. `english_extra` is the letters the
/// build draws as the English font's glyphs 288 on (replace::english_letters; empty for
/// font::ENGLISH_EXTRA), which the items' English names and descriptions are written with.
pub fn apply_data(
    iso_path: &Path,
    mod_dirs: &[PathBuf],
    files: &Files,
    english_extra: &[char],
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
        // the layouts name parts as a data:parts line of the build names them
        let renamed =
            if table == "layouts" && !patches.is_empty() { Some(elf::apply_patches(&elf, &patches)?) } else { None };
        let loaded = load(table, renamed.as_deref().unwrap_or(&elf), &current)?;
        let mut change = DataChange { table: table.to_string(), changed: Vec::new(), files: Vec::new() };
        if let Table::Layouts(layouts) = &loaded {
            // whole rooms, written again where they change (no pack holds a room script)
            let mut found = Vec::new();
            for line in &mine {
                let given = layouts
                    .changes(&read_table(line)?, &line.module)
                    .with_context(|| format!("{} (mod {})", line.key, line.module))?;
                change.changed.extend(given.iter().map(|edit| (edit.module.clone(), edit.what.clone())));
                found.extend(given);
            }
            let written = layouts.apply(&found)?;
            let mut rewritten = Vec::new();
            for (path, data) in dungeon_layout::paths().into_iter().zip(written) {
                let old = current(&path)?;
                if data != old {
                    change.files.push((path.clone(), data.len(), old.len()));
                    rewritten.push((path, data));
                }
            }
            out.extend(rewritten);
            changes.push(change);
            continue;
        }
        if let Table::Chests(chests) = &loaded {
            let mut found = Vec::new();
            for line in &mine {
                let given = chests
                    .changes(&read_table(line)?, &line.module)
                    .with_context(|| format!("{} (mod {})", line.key, line.module))?;
                change.changed.extend(given.iter().map(|edit| (edit.module.clone(), edit.what.clone())));
                found.extend(given);
            }
            let mut rewritten = Vec::new();
            for (path, data) in chest_data::paths().into_iter().zip(chests.apply(&found)?) {
                let old = current(&path)?;
                if data != old {
                    change.files.push((path.clone(), data.len(), old.len()));
                    rewritten.push((path, data));
                }
            }
            out.extend(rewritten);
            changes.push(change);
            continue;
        }
        let mut given = Vec::new();
        for line in &mine {
            given.push((line.module.clone(), read_table(line)?));
        }
        let (edits, found_patches, texts) =
            loaded.changes(&given).with_context(|| format!("data:{table} ({})", modules_of(&mine)))?;
        change.changed.extend(edits.iter().map(|edit| (edit.module.clone(), edit.what.clone())));
        change.changed.extend(found_patches.iter().map(|patch| (patch.module.clone(), patch.what.clone())));
        change.changed.extend(texts.iter().map(|text| (text.module.clone(), text.what.clone())));
        let patched = !found_patches.is_empty();
        patches.extend(found_patches);
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
            change.files.push((elf::EXECUTABLE.to_string(), elf.len(), elf.len()));
        }
        out.extend(rewritten);
        if !texts.is_empty() {
            let current = |path: &str| -> Result<Vec<u8>> {
                match out.iter().find(|(known, _)| known.eq_ignore_ascii_case(path)) {
                    Some((_, data)) => Ok(data.clone()),
                    None => disc.read(path),
                }
            };
            let written = crate::item_text::item_messages(&disc, &current, &texts, english_extra)?;
            for (top, data) in &written {
                change.files.push((top.clone(), data.len(), current(top)?.len()));
            }
            out.extend(written);
        }
        changes.push(change);
    }
    let executable_out = if patches.is_empty() { None } else { Some(elf::apply_patches(&elf, &patches)?) };
    Ok((out, executable_out, changes))
}

fn modules_of(lines: &[&Line]) -> String {
    let mut modules: Vec<&str> = Vec::new();
    for line in lines {
        if !modules.contains(&line.module.as_str()) {
            modules.push(&line.module);
        }
    }
    format!("mod {}", modules.join(", mod "))
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
            let wanted =
                if path == elf::EXECUTABLE { executable.map(<[u8]>::to_vec) } else { files.get(path).cloned() };
            let read = if path == elf::EXECUTABLE { self::executable(out_path).ok() } else { disc.read(path).ok() };
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
        let (inventions, ideas) = split_inventions(&json!({"recipes": [], "ideas": [{"idea": 251, "name": "x"}]}));
        assert_eq!((inventions, ideas), (json!({"recipes": []}), json!([{"idea": 251, "name": "x"}])));
        assert_eq!(split_inventions(&json!({"scoops": []})).1, json!([]));
    }
}
