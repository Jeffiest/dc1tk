//! The game's item master data (docs/assets/data.md, "Items") as one JSON table with named fields,
//! and edits of that table written back into the scripts it comes from.
//!
//! CGameData::LoadData (gamedata.cpp) runs seven cfg scripts of `menu/cfg7/` over the tag table
//! `gamedata_tag`: `comdat.cfg` (`COM`, the data every item number has), `wepdat.cfg` (weapons,
//! blocks of `WEP` .. `WEP_BUILD`, the n-th block for the weapon whose `COM` list number is n),
//! `itemdat.cfg` (`ITEM`, usable item effects), `atdat.cfg` (attachments, blocks of `AT_ST` ..
//! `AT_ST_SP` by list number), `robodat.cfg` (`RB_PARTS`, ridepod parts), `fishdat.cfg` (`FISH`)
//! and `grddat.cfg` (`GRD`, a table nothing reads). The display names come from
//! `comdatmes<language>.cfg`; the English one labels the table.
//!
//! The table holds one record per `COM` statement, in file order, with the families' fields
//! nested under `weapon`, `effect`, `attachment`, `ridepod` and `fish`. Each field is one argument
//! of one statement ([`cfg_text`]), so [`apply`] changes only the arguments whose values a mod
//! changes and every other byte stays the disc's: an unedited table writes the files back
//! unchanged. A mod's table may hold only the records and fields it changes; several mods' tables
//! merge, and two mods giving one field different values are refused.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::cfg_text::{self, Arg, Edit, Script, To};
use crate::{bail, Result};

/// Where the engine reads the tables (`menu/cfg7/%s`, LoadGameDataAnalyze).
pub const FOLDER: &str = "menu/cfg7";
/// The scripts, in the order LoadData runs them; the guard table is left out of the JSON.
pub const FILES: [&str; 7] =
    ["comdat.cfg", "wepdat.cfg", "itemdat.cfg", "atdat.cfg", "robodat.cfg", "fishdat.cfg", "grddat.cfg"];
/// The English item names (LoadItemSystemMes(1)).
pub const NAMES: &str = "comdatmes1.cfg";

const COMMON: usize = 0;
const WEAPON: usize = 1;
const EFFECT: usize = 2;
const ATTACH: usize = 3;
const RIDEPOD: usize = 4;
const FISH: usize = 5;

/// What a field holds and the values the engine's field takes.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Int(i64, i64),
    /// Read with spiGetStackFloat.
    Float,
    /// A string of at most this many bytes.
    Text(usize),
}

/// A byte: -1 is kept as 255, as the engine stores it.
const U8: Kind = Kind::Int(-128, 255);
const S16: Kind = Kind::Int(-32768, 32767);
const U16: Kind = Kind::Int(-32768, 65535);
/// A u32 of bits, as atoi can give it.
const BITS: Kind = Kind::Int(0, i32::MAX as i64);

/// A field: its name, the statement and argument it is, and how many arguments from there (an
/// array in the JSON when more than one).
#[derive(Clone, Copy, Debug)]
struct Spec {
    name: &'static str,
    tag: &'static str,
    arg: usize,
    len: usize,
    kind: Kind,
}

const fn one(name: &'static str, tag: &'static str, arg: usize, kind: Kind) -> Spec {
    Spec { name, tag, arg, len: 1, kind }
}

const fn many(name: &'static str, tag: &'static str, arg: usize, len: usize, kind: Kind) -> Spec {
    Spec { name, tag, arg, len, kind }
}

/// The eight attribute parameters, in the order of the weapon and attachment arrays (the
/// attachment crystals give them in this order).
const ELEMENTS: [&str; 8] = ["flame", "chill", "lightning", "cyclone", "smash", "exorcism", "beast", "scale"];
const ELEMENTS_MAX: [&str; 8] =
    ["flame_max", "chill_max", "lightning_max", "cyclone_max", "smash_max", "exorcism_max", "beast_max", "scale_max"];

/// `COM item_no, type, list_no, active_set, stack_num, max_num, icon_sheet, icon_no, message_no,
/// "file", attribute` (_DATACOM, into CDataCommon).
const COMMON_SPECS: [Spec; 10] = [
    one("type", "COM", 1, U8),
    one("list", "COM", 2, S16),
    one("active_set", "COM", 3, U8),
    one("stack", "COM", 4, S16),
    one("max", "COM", 5, U16),
    one("icon_sheet", "COM", 6, U8),
    one("icon", "COM", 7, S16),
    one("message", "COM", 8, S16),
    one("model", "COM", 9, Kind::Text(15)),
    one("attribute", "COM", 10, BITS),
];

fn weapon_specs() -> Vec<Spec> {
    let mut specs = vec![
        one("whp", "WEP", 0, S16),
        one("abs", "WEP", 1, S16),
        one("attack", "WEP_ST", 0, S16),
        one("durability", "WEP_ST", 1, S16),
        one("attack_max", "WEP_ST_L", 0, S16),
        one("durability_max", "WEP_ST_L", 1, S16),
    ];
    specs.extend(ELEMENTS.iter().enumerate().map(|(at, name)| one(name, "WEP_ST2", at, S16)));
    specs.extend(ELEMENTS_MAX.iter().enumerate().map(|(at, name)| one(name, "WEP_ST2_L", at, S16)));
    specs.extend([
        one("synthesis_points", "WEP_SPE", 0, U8),
        one("palette", "WEP_SPE", 1, U8),
        one("unknown_47", "WEP_SPE", 2, U8),
        one("synthesis_per_level", "WEP_SPE", 3, U8),
        one("special", "WEP_SPE", 4, BITS),
        one("attack_type", "WEP_SPE", 5, U8),
        one("model_no", "WEP_SPE", 6, U8),
        many("build_up", "WEP_BUILD", 0, 3, S16),
        many("build_up_kills", "WEP_BUILD", 3, 3, S16),
    ]);
    specs
}

const EFFECT_SPECS: [Spec; 4] = [
    one("use_flags", "ITEM", 1, BITS),
    one("status_flags", "ITEM", 2, BITS),
    one("target_flags", "ITEM", 3, U16),
    many("values", "ITEM", 4, 3, S16),
];

fn attach_specs() -> Vec<Spec> {
    let mut specs = vec![one("attack", "AT_ST", 0, S16), one("durability", "AT_ST", 1, S16)];
    specs.extend(ELEMENTS.iter().enumerate().map(|(at, name)| one(name, "AT_ST2", at, S16)));
    specs.push(one("special", "AT_ST_SP", 0, BITS));
    specs
}

/// `RB_PARTS item_no, kind, capacity, offset_no, ...` (_DATAROBO_ANALYZE): the rest by the kind,
/// 0 a body, 1 an arm, 2 legs, 3 an energy pack.
fn ridepod_specs(kind: i32) -> Vec<Spec> {
    let mut specs =
        vec![one("kind", "RB_PARTS", 1, U8), one("capacity", "RB_PARTS", 2, S16), one("file_no", "RB_PARTS", 3, U8)];
    match kind {
        0 => specs.extend([one("defence", "RB_PARTS", 4, S16), one("label", "RB_PARTS", 5, Kind::Text(255))]),
        1 => {
            specs.extend([
                one("whp", "RB_PARTS", 4, S16),
                one("attack", "RB_PARTS", 5, S16),
                one("durability", "RB_PARTS", 6, S16),
            ]);
            specs.extend(ELEMENTS.iter().enumerate().map(|(at, name)| one(name, "RB_PARTS", 7 + at, S16)));
            specs.extend([one("attack_type", "RB_PARTS", 15, S16), one("label", "RB_PARTS", 16, Kind::Text(255))]);
        }
        2 => specs.extend([one("unknown_4", "RB_PARTS", 4, S16), one("attack_type", "RB_PARTS", 5, S16)]),
        3 => specs.push(one("energy", "RB_PARTS", 4, S16)),
        _ => {}
    }
    specs
}

/// `FISH item_no, size, unknown, power, speed[3], stamina[, unknown]` (_DATAFISH): power, the
/// speeds and stamina are the bred fish's race parameters (CopyDataFish).
const FISH_SPECS: [Spec; 7] = [
    one("size", "FISH", 1, Kind::Float),
    one("unknown_4", "FISH", 2, S16),
    one("power", "FISH", 3, S16),
    many("speed", "FISH", 4, 3, S16),
    one("stamina", "FISH", 7, S16),
    one("unknown_10", "FISH", 8, S16),
    // a last spec only to keep the array's length fixed
    one("", "", 0, S16),
];

const GROUPS: [&str; 6] = ["", "weapon", "effect", "attachment", "ridepod", "fish"];

/// A field of a record and where its arguments are.
#[derive(Clone, Debug)]
struct Located {
    group: usize,
    spec: Spec,
    file: usize,
    statement: usize,
}

/// The tables read from the scripts, each field with the place it was read from.
#[derive(Clone, Debug)]
pub struct Items {
    scripts: Vec<Script>,
    records: Vec<Record>,
}

#[derive(Clone, Debug)]
struct Record {
    item: i32,
    name: Option<String>,
    fields: Vec<Located>,
}

/// ConvertUsedItemType (gamedata.cpp): the family of an item type, 3 weapon, 4 clothing, 5
/// ridepod part, 2 attachment, 1 item, 7 gift box, 6 fish, 8 boiled.
pub fn family(item_type: i32) -> i32 {
    let mut family = match item_type {
        1..=4 => 3,
        5..=10 => 4,
        12..=15 => 5,
        16..=19 | 0x22 => 2,
        11 => 1,
        t if t >= 20 => 1,
        _ => 0,
    };
    match item_type {
        0x1C => family = 7,
        0x1E => family = 6,
        0x23 => family = 8,
        _ => {}
    }
    family
}

/// Blocks of statements that fill one entry, ended by the tag that moves the engine's cursor on:
/// {tag: statement index} each.
fn blocks(script: &Script, tags: &[&str], last: &str) -> Vec<BTreeMap<&'static str, usize>> {
    let mut out = Vec::new();
    let mut current: BTreeMap<&'static str, usize> = BTreeMap::new();
    for (at, statement) in script.statements.iter().enumerate() {
        let Some(tag) = tags.iter().find(|tag| **tag == statement.tag) else {
            continue;
        };
        // the tag tables are 'static, so key by them
        let tag: &'static str = Box::leak(tag.to_string().into_boxed_str());
        current.insert(tag, at);
        if statement.tag == last {
            out.push(std::mem::take(&mut current));
        }
    }
    out
}

/// The count an `*INIT`/`*NUM` statement gives, or none.
fn count(script: &Script, tag: &str) -> Option<i32> {
    script
        .statements
        .iter()
        .rev()
        .find(|statement| statement.tag == tag)
        .map(|statement| statement.values().first().map_or(0, Arg::int))
}

/// {item number: statement index} of the statements that name their item first; a later one
/// fills the entry again, so it wins.
fn keyed(script: &Script, tag: &str) -> BTreeMap<i32, usize> {
    let mut out = BTreeMap::new();
    for (at, statement) in script.statements.iter().enumerate() {
        if statement.tag == tag {
            if let Some(first) = statement.values().first() {
                out.insert(first.int(), at);
            }
        }
    }
    out
}

impl Items {
    /// The tables from the seven scripts (by [`FILES`]) and the English names, as the engine
    /// fills them.
    pub fn read(files: &[Vec<u8>], names: Option<&[u8]>) -> Result<Items> {
        if files.len() != FILES.len() {
            bail!("the item data is {} scripts", FILES.len());
        }
        let mut scripts = Vec::new();
        for (file, data) in FILES.iter().zip(files) {
            scripts.push(Script::parse(data).map_err(|error| crate::Error::invalid(format!("{file}: {error}")))?);
        }
        let mut shown: BTreeMap<i32, String> = BTreeMap::new();
        if let Some(names) = names {
            for statement in Script::parse(names)?.statements.iter().filter(|statement| statement.tag == "MES_SYS") {
                if let [number, Arg::Str(text)] = &statement.values()[..] {
                    shown.insert(number.int(), c2_disc::text::decode_lossy(text));
                }
            }
        }
        let weapon_tags = ["WEP", "WEP_ST", "WEP_ST_L", "WEP_ST2", "WEP_ST2_L", "WEP_SPE", "WEP_BUILD"];
        let weapons = blocks(&scripts[1], &weapon_tags, "WEP_BUILD");
        let weapon_count = count(&scripts[1], "WEPNUM").unwrap_or(0).max(0) as usize;
        let attachments = blocks(&scripts[3], &["AT_ST", "AT_ST2", "AT_ST_SP"], "AT_ST_SP");
        let attach_count = count(&scripts[3], "AT_INIT").unwrap_or(0).max(0) as usize;
        let effects = keyed(&scripts[2], "ITEM");
        let ridepod = keyed(&scripts[4], "RB_PARTS");
        let fish = keyed(&scripts[5], "FISH");
        let mut records = Vec::new();
        let weapon_specs = weapon_specs();
        let attach_specs = attach_specs();
        for (statement_at, statement) in scripts[0].statements.iter().enumerate() {
            if statement.tag != "COM" {
                continue;
            }
            let values = statement.values();
            let item = values.first().map_or(0, Arg::int);
            let item_type = values.get(1).map_or(0, Arg::int);
            let list = values.get(2).map_or(-1, Arg::int);
            let mut fields = Vec::new();
            let add = |fields: &mut Vec<Located>, group: usize, file: usize, statement: usize, specs: &[Spec]| {
                for spec in specs.iter().filter(|spec| !spec.name.is_empty()) {
                    let args = scripts[file].statements[statement].args().len();
                    if spec.arg + spec.len <= args {
                        fields.push(Located { group, spec: *spec, file, statement });
                    }
                }
            };
            add(&mut fields, COMMON, 0, statement_at, &COMMON_SPECS);
            let block_fields = |fields: &mut Vec<Located>,
                                group: usize,
                                file: usize,
                                block: &BTreeMap<&str, usize>,
                                specs: &[Spec]| {
                for spec in specs {
                    if let Some(&statement) = block.get(spec.tag) {
                        if spec.arg + spec.len <= scripts[file].statements[statement].args().len() {
                            fields.push(Located { group, spec: *spec, file, statement });
                        }
                    }
                }
            };
            let family = family(item_type);
            let list_index = usize::try_from(list).ok();
            match family {
                3 => {
                    if let Some(block) = list_index.filter(|&at| at < weapon_count).and_then(|at| weapons.get(at)) {
                        block_fields(&mut fields, WEAPON, 1, block, &weapon_specs);
                    }
                }
                2 => {
                    if let Some(block) = list_index.filter(|&at| at < attach_count).and_then(|at| attachments.get(at)) {
                        block_fields(&mut fields, ATTACH, 3, block, &attach_specs);
                    }
                }
                _ => {}
            }
            if matches!(family, 1 | 7 | 8) {
                if let Some(&at) = effects.get(&item) {
                    add(&mut fields, EFFECT, 2, at, &EFFECT_SPECS);
                }
            }
            if let Some(&at) = ridepod.get(&item) {
                let kind = scripts[4].statements[at].values().get(1).map_or(-1, Arg::int);
                add(&mut fields, RIDEPOD, 4, at, &ridepod_specs(kind));
            }
            if family == 6 {
                if let Some(&at) = fish.get(&item) {
                    add(&mut fields, FISH, 5, at, &FISH_SPECS);
                }
            }
            records.push(Record { item, name: shown.get(&item).cloned(), fields });
        }
        Ok(Items { scripts, records })
    }

    /// The number of records (COM statements).
    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    fn value(&self, field: &Located) -> Value {
        let args = self.scripts[field.file].statements[field.statement].args();
        let one = |at: usize| -> Value {
            let arg = args[at].value();
            match field.spec.kind {
                Kind::Int(..) => Value::from(arg.int()),
                Kind::Float => match arg {
                    Arg::Float(value) => serde_json::Number::from_f64(value as f64).map_or(Value::Null, Value::Number),
                    other => Value::from(other.int()),
                },
                Kind::Text(_) => match arg {
                    Arg::Str(text) => Value::from(c2_disc::text::decode_lossy(&text)),
                    _ => Value::Null,
                },
            }
        };
        if field.spec.len == 1 {
            one(field.spec.arg)
        } else {
            Value::Array((field.spec.arg..field.spec.arg + field.spec.len).map(one).collect())
        }
    }

    /// The tables as JSON: `{"items": [{"item": 1, "name": ..., "type": ..., "weapon": {...}}]}`.
    pub fn to_json(&self) -> Value {
        let mut items = Vec::new();
        for record in &self.records {
            let mut object = Map::new();
            object.insert("item".into(), Value::from(record.item));
            if let Some(name) = &record.name {
                object.insert("name".into(), Value::from(name.clone()));
            }
            for field in record.fields.iter().filter(|field| field.group == COMMON) {
                object.insert(field.spec.name.into(), self.value(field));
            }
            for (group, key) in GROUPS.iter().enumerate().skip(1) {
                let mut nested = Map::new();
                for field in record.fields.iter().filter(|field| field.group == group) {
                    nested.insert(field.spec.name.into(), self.value(field));
                }
                if !nested.is_empty() {
                    object.insert((*key).into(), Value::Object(nested));
                }
            }
            items.push(Value::Object(object));
        }
        let mut top = Map::new();
        top.insert("items".into(), Value::Array(items));
        Value::Object(top)
    }

    /// The scripts as they stand, by [`FILES`].
    pub fn write(&self) -> Vec<Vec<u8>> {
        self.scripts.iter().map(Script::write).collect()
    }
}

fn value_text(spec: &Spec, value: &Value, what: &str) -> Result<Vec<u8>> {
    match spec.kind {
        Kind::Int(low, high) => {
            let Some(number) = value.as_i64() else {
                bail!("{what} is a whole number");
            };
            if number < low || number > high {
                bail!("{what} is {number}; the game keeps {low} to {high}");
            }
            Ok(cfg_text::int_text(number))
        }
        Kind::Float => {
            if let Some(number) = value.as_i64().filter(|number| i32::try_from(*number).is_ok()) {
                return Ok(cfg_text::int_text(number));
            }
            let Some(number) = value.as_f64().filter(|number| number.is_finite()) else {
                bail!("{what} is a number");
            };
            let mut text = format!("{}", number as f32);
            if !text.contains('.') {
                text.push_str(".0");
            }
            Ok(text.into_bytes())
        }
        Kind::Text(limit) => {
            let Some(text) = value.as_str() else {
                bail!("{what} is a string");
            };
            let Some(bytes) = c2_disc::text::encode(text) else {
                bail!("{what}: {text:?} has characters the game's text cannot hold");
            };
            if bytes.len() > limit {
                bail!("{what}: {text:?} is {} bytes; the game keeps {limit}", bytes.len());
            }
            Ok(cfg_text::str_text(&bytes))
        }
    }
}

/// Whether two values are the same to the engine (an int and the float of it are).
fn same(old: &Value, new: &Value) -> bool {
    match (old.as_f64(), new.as_f64()) {
        (Some(a), Some(b)) if old.is_number() && new.is_number() => (a as f32) == (b as f32),
        _ => old == new,
    }
}

impl Items {
    /// The changes a mod's table makes: only fields whose values differ from these tables'. A
    /// record names its item by `item`; `name` is a label and may not change; a field the item does
    /// not have, or a value its engine field cannot hold, is refused.
    pub fn changes(&self, table: &Value, module: &str) -> Result<Vec<Edit>> {
        let Some(records) = table.get("items").and_then(Value::as_array) else {
            bail!("an item table is {{\"items\": [records]}}, as mod asset writes it");
        };
        let mut out = Vec::new();
        for record in records {
            let Some(object) = record.as_object() else {
                bail!("each item is an object with \"item\" and the fields it changes");
            };
            let Some(item) = object.get("item").and_then(Value::as_i64) else {
                bail!("each item names its item number: {{\"item\": 1, ...}}");
            };
            let Some(found) = self.records.iter().rev().find(|found| found.item as i64 == item) else {
                bail!("item {item}: the game has no such item");
            };
            let label = match &found.name {
                Some(name) => format!("item {item} ({name})"),
                None => format!("item {item}"),
            };
            for (key, value) in object {
                match key.as_str() {
                    "item" => continue,
                    "name" => {
                        if found.name.as_deref() != value.as_str() {
                            bail!("{label}: \"name\" is a label; item names are text (comdatmes), not data");
                        }
                        continue;
                    }
                    _ => {}
                }
                let (group, fields): (usize, Vec<(&String, &Value)>) =
                    match GROUPS.iter().position(|group| group == key) {
                        Some(group) if group > 0 => {
                            let Some(nested) = value.as_object() else {
                                bail!("{label}: \"{key}\" is an object of fields");
                            };
                            (group, nested.iter().collect())
                        }
                        _ => (COMMON, vec![(key, value)]),
                    };
                for (name, value) in fields {
                    let what =
                        if group == COMMON { format!("{label} {name}") } else { format!("{label} {key}.{name}") };
                    let Some(field) = found.fields.iter().find(|field| field.group == group && field.spec.name == name)
                    else {
                        if group != COMMON && !found.fields.iter().any(|field| field.group == group) {
                            bail!("{label} has no {key} data");
                        }
                        bail!("{what}: no such field");
                    };
                    let old = self.value(field);
                    if same(&old, value) {
                        continue;
                    }
                    if group == COMMON && name == "list" {
                        bail!("{what}: the list number ties the item to its family's entries and does not change");
                    }
                    if group == RIDEPOD && name == "kind" {
                        bail!("{what}: the kind says which fields a part has and does not change");
                    }
                    let values: Vec<&Value> = if field.spec.len == 1 {
                        vec![value]
                    } else {
                        match value.as_array() {
                            Some(list) if list.len() == field.spec.len => list.iter().collect(),
                            _ => bail!("{what} is a list of {}", field.spec.len),
                        }
                    };
                    let olds: Vec<Value> = match &old {
                        Value::Array(list) => list.clone(),
                        other => vec![other.clone()],
                    };
                    for (offset, (value, old)) in values.into_iter().zip(olds).enumerate() {
                        let text = value_text(&field.spec, value, &what)?;
                        if same(&old, value) {
                            continue;
                        }
                        out.push(Edit {
                            file: field.file,
                            statement: field.statement,
                            arg: field.spec.arg + offset,
                            to: To::Arg(text),
                            what: if field.spec.len == 1 { what.clone() } else { format!("{what}[{offset}]") },
                            module: module.to_string(),
                        });
                    }
                }
            }
        }
        Ok(out)
    }

    /// The scripts with the edits made, by [`FILES`] (cfg_text::apply_edits).
    pub fn apply(&self, edits: &[Edit]) -> Result<Vec<Vec<u8>>> {
        cfg_text::apply_edits(&self.scripts, edits)
    }
}

/// The table as text: objects a field a line, lists of numbers on one line, as a person edits it.
pub fn json_text(value: &Value) -> String {
    let mut out = String::new();
    write_value(value, 0, &mut out);
    out.push('\n');
    out
}

fn write_value(value: &Value, indent: usize, out: &mut String) {
    let pad = |depth: usize| "  ".repeat(depth);
    match value {
        Value::Object(map) if !map.is_empty() => {
            out.push_str("{\n");
            for (number, (key, item)) in map.iter().enumerate() {
                out.push_str(&pad(indent + 1));
                out.push_str(&serde_json::to_string(key).unwrap());
                out.push_str(": ");
                write_value(item, indent + 1, out);
                if number + 1 < map.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str(&pad(indent));
            out.push('}');
        }
        Value::Array(list) if list.iter().any(|item| item.is_object() || item.is_array()) => {
            out.push_str("[\n");
            for (number, item) in list.iter().enumerate() {
                out.push_str(&pad(indent + 1));
                write_value(item, indent + 1, out);
                if number + 1 < list.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str(&pad(indent));
            out.push(']');
        }
        Value::Array(list) => {
            let items: Vec<String> = list.iter().map(|item| serde_json::to_string(item).unwrap()).collect();
            out.push('[');
            out.push_str(&items.join(", "));
            out.push(']');
        }
        other => out.push_str(&serde_json::to_string(other).unwrap()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Vec<u8>> {
        let files: [&str; 7] = [
            "COM 1,1,0,0,1,300,0,0,1,\"wm001\",3;\r\nCOM 175,16,0,0,99,99,0,10,175,\"at01\",2;\r\nCOM 246,11,0,1,9,9,0,1,246,\"\",0;\r\nCOM 310,30,0,0,1,1,0,2,310,\"f01\",0;\r\nCOM 135,12,0,0,1,1,0,3,135,\"rb\",0;\r\n",
            "WEPNUM 1;\r\nWEP 36,32;\r\nWEP_ST 8,10;\r\nWEP_ST_L 20,19;\r\nWEP_ST2 2,0,0,0, 5 ,0,0,0;\r\nWEP_ST2_L 19,19,19,19,19,19,19,19;\r\nWEP_SPE 1,1,0,3,0,0,0;\r\nWEP_BUILD 2,90,0,-1,-1,-1;\r\n",
            "ITEMINIT 1;\r\nITEM 246,16777216,0,0,60,0,0;\r\n",
            "AT_INIT 1;\r\nAT_ST 0,0;\r\nAT_ST2 3,0,0,0,0,0,0,0;\r\nAT_ST_SP 0;\r\n",
            "ROBOINIT 1;\r\nRB_PARTS 135,0,10,1,6,\"arm1\";\r\n",
            "FISHINIT 1;\r\nFISH 310,1450,54,6,12,8,8,7;\r\n",
            "GRDNUM 0;\r\n",
        ];
        files.iter().map(|text| text.as_bytes().to_vec()).collect()
    }

    const NAMES_SAMPLE: &[u8] = b"MES_SYS 1,\"Battle Wrench\";\r\nMES_SYS 175,\"Flame Crystal\";\r\n";

    #[test]
    fn the_tables_read_as_the_engine_fills_them() {
        let items = Items::read(&sample(), Some(NAMES_SAMPLE)).unwrap();
        let json = items.to_json();
        let wrench = &json["items"][0];
        assert_eq!(wrench["name"], "Battle Wrench");
        assert_eq!(wrench["model"], "wm001");
        assert_eq!(wrench["weapon"]["whp"], 36);
        assert_eq!(wrench["weapon"]["smash"], 5);
        assert_eq!(wrench["weapon"]["build_up"], serde_json::json!([2, 90, 0]));
        assert_eq!(json["items"][1]["attachment"]["flame"], 3);
        assert_eq!(json["items"][2]["effect"]["values"], serde_json::json!([60, 0, 0]));
        assert_eq!(json["items"][3]["fish"]["speed"], serde_json::json!([12, 8, 8]));
        assert_eq!(json["items"][4]["ridepod"]["defence"], 6);
        // unedited, nothing changes
        assert!(items.changes(&json, "m").unwrap().is_empty());
        assert_eq!(items.apply(&[]).unwrap(), sample());
        let text = json_text(&json);
        assert!(text.contains("\"build_up\": [2, 90, 0]"));
        assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), json);
    }

    #[test]
    fn a_change_rewrites_only_its_argument() {
        let items = Items::read(&sample(), None).unwrap();
        let edit =
            serde_json::json!({"items": [{"item": 1, "max": 300, "weapon": {"smash": 12, "build_up": [3, 90, 0]}}]});
        let changes = items.changes(&edit, "m").unwrap();
        assert_eq!(changes.len(), 2);
        let files = items.apply(&changes).unwrap();
        let wanted =
            String::from_utf8(sample()[1].clone()).unwrap().replace(" 5 ,", " 12 ,").replace("BUILD 2,", "BUILD 3,");
        assert_eq!(files[1], wanted.as_bytes());
        assert_eq!(files[0], sample()[0]);
        let again = Items::read(&files, None).unwrap().to_json();
        assert_eq!(again["items"][0]["weapon"]["smash"], 12);
        let fish = serde_json::json!({"items": [{"item": 310, "fish": {"size": 1450.5}}]});
        let files = items.apply(&items.changes(&fish, "m").unwrap()).unwrap();
        assert!(String::from_utf8_lossy(&files[5]).contains("FISH 310,1450.5,"));
    }

    #[test]
    fn mods_merge_and_wrong_edits_are_refused() {
        let items = Items::read(&sample(), Some(NAMES_SAMPLE)).unwrap();
        let mut changes =
            items.changes(&serde_json::json!({"items": [{"item": 1, "weapon": {"attack": 9}}]}), "a").unwrap();
        changes.extend(
            items
                .changes(&serde_json::json!({"items": [{"item": 1, "weapon": {"attack": 9, "whp": 40}}]}), "b")
                .unwrap(),
        );
        assert!(items.apply(&changes).is_ok());
        changes.extend(
            items.changes(&serde_json::json!({"items": [{"item": 1, "weapon": {"attack": 10}}]}), "c").unwrap(),
        );
        let error = items.apply(&changes).unwrap_err().to_string();
        assert!(error.contains("weapon.attack") && error.contains("mod a") && error.contains("mod c"), "{error}");
        let refused = |edit: Value| items.changes(&edit, "m").unwrap_err().to_string();
        assert!(refused(serde_json::json!({"items": [{"item": 2}]})).contains("no such item"));
        assert!(refused(serde_json::json!({"items": [{"item": 1, "weapon": {"attack": 40000}}]}))
            .contains("-32768 to 32767"));
        assert!(refused(serde_json::json!({"items": [{"item": 1, "fish": {"size": 1}}]})).contains("no fish data"));
        assert!(refused(serde_json::json!({"items": [{"item": 1, "name": "Wrench"}]})).contains("label"));
        assert!(refused(serde_json::json!({"items": [{"item": 1, "list": 3}]})).contains("list number"));
        assert!(refused(serde_json::json!({"items": [{"item": 1, "model": "a_name_far_too_long"}]})).contains("bytes"));
        assert!(refused(serde_json::json!({"items": [{"item": 1, "weapon": {"build_up": [1]}}]})).contains("list of 3"));
    }

    #[test]
    fn families_follow_convert_used_item_type() {
        let families: Vec<i32> = [1, 5, 11, 12, 16, 0x1C, 0x1E, 0x22, 0x23, 40].iter().map(|&t| family(t)).collect();
        assert_eq!(families, [3, 4, 1, 5, 2, 7, 6, 2, 8, 1]);
    }

    /// Every version of the item data on an extracted disc (C2S_CORPUS: menu/cfg5, cfg6 and the
    /// cfg7 the game reads): the scripts round-trip byte for byte, every field's text is the one
    /// its value writes, and the JSON goes back to the same files.
    #[test]
    fn corpus_item_data_round_trips() {
        let Some(corpus) = std::env::var_os("C2S_CORPUS") else {
            return;
        };
        let menu = std::path::Path::new(&corpus).join("menu");
        let mut checked = 0;
        for folder in ["cfg5", "cfg6", "cfg7"] {
            let folder = menu.join(folder);
            let files: Vec<Vec<u8>> =
                FILES.iter().map(|file| std::fs::read(folder.join(file)).unwrap_or_default()).collect();
            let names = std::fs::read(folder.join(NAMES)).ok();
            let items = Items::read(&files, names.as_deref()).unwrap();
            assert_eq!(items.write(), files, "{}", folder.display());
            for record in &items.records {
                for field in &record.fields {
                    let args = items.scripts[field.file].statements[field.statement].args();
                    for at in field.spec.arg..field.spec.arg + field.spec.len {
                        let value = items.value(&Located { spec: one("", "", at, field.spec.kind), ..field.clone() });
                        let what = format!("{} item {} {}", folder.display(), record.item, field.spec.name);
                        let text = value_text(&field.spec, &value, &what).unwrap();
                        assert_eq!(
                            text,
                            args[at].text,
                            "{} item {} {}",
                            folder.display(),
                            record.item,
                            field.spec.name
                        );
                    }
                }
            }
            let json: Value = serde_json::from_str(&json_text(&items.to_json())).unwrap();
            assert!(items.changes(&json, "m").unwrap().is_empty());
            checked += items.len();
        }
        eprintln!("{checked} item records in three versions round-trip");
        assert!(checked > 1000);
    }
}
