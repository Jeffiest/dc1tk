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
/// The item names of every language, by LanguageCode (LoadItemSystemMes, `comdatmes%d.cfg`):
/// files 7 to 13 of a table read with them.
pub const NAME_FILES: [&str; 7] = [
    "comdatmes0.cfg",
    "comdatmes1.cfg",
    "comdatmes2.cfg",
    "comdatmes3.cfg",
    "comdatmes4.cfg",
    "comdatmes5.cfg",
    "comdatmes6.cfg",
];
/// local_com_itemdata[432]: the COM records the engine holds (InitItemMes clears 0x1B0).
pub const COMMON_MOST: usize = 432;
/// local_itemdatano_converttable[512]; GetCommonData takes 1 to 0x1FF.
pub const ITEM_MOST: i64 = 0x1FF;
/// LoadGameDataAnalyze and LoadItemSystemMes read each script into a 0x7800-byte stack buffer,
/// aligned to 64 bytes first (MenuCalcBufAlignment).
pub const SCRIPT_BUFFER: usize = 0x7800 - 0x40;
/// gamedata_sysword_buffer_1073: the names, each copied in 16-byte units (mgCopyString), into an
/// mgCMemory of 0x280 units that refuses the allocation reaching its end.
pub const NAME_UNITS: usize = 0x280;
/// The languages the PAL game plays (LanguageCode 1 to 5); Japanese and Korean files are kept in
/// step but not checked against the name buffer (the Korean one is over it on the disc).
pub const PLAYED: std::ops::RangeInclusive<usize> = 1..=5;
/// Item names are system messages 10000 + message (GetItemMessageNo kind 1, msg_offsettbl_1363);
/// descriptions are allmenu.mes messages of the same number.
pub const MESSAGE_BASE: i64 = 10000;

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
    /// The COM statement.
    com: usize,
    /// The family's statements: (file, first, last), a block or one statement.
    block: Option<(usize, usize, usize)>,
}

/// The family tables a new item's list number is counted in: (families, file, count tag, the
/// tag that ends an entry, the most the engine holds: local_weapondata[116], local_attachdata[38],
/// local_itemdata[162], local_robodata[68], local_fishdata[20], local_guarddata[40]).
const FAMILY_TABLES: [(&[i32], usize, &str, &str, usize); 6] = [
    (&[3], 1, "WEPNUM", "WEP_BUILD", 116),
    (&[2], 3, "AT_INIT", "AT_ST_SP", 38),
    (&[1, 7, 8], 2, "ITEMINIT", "ITEM", 162),
    (&[5], 4, "ROBOINIT", "RB_PARTS", 68),
    (&[6], 5, "FISHINIT", "FISH", 20),
    (&[4], 6, "GRDNUM", "GRD", 40),
];

/// A message an item's name or description puts into the message files: `file` "system"
/// (meswin/system_N.mes, the names item slots and menus show) or "allmenu" (each menu pack's
/// allmenu.mes, the descriptions), `language` the LanguageCode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemText {
    pub file: &'static str,
    pub language: usize,
    pub id: u16,
    /// The message in the text form of docs/assets/text.md; None copies the message `copy` of
    /// the same file (a new item's description, from the item it is like).
    pub text: Option<String>,
    pub copy: u16,
    pub what: String,
    pub module: String,
}

/// The language codes (text.rs' short names) by LanguageCode.
pub const LANGUAGE_NAMES: [&str; 7] = ["ja", "en", "fr", "de", "it", "es", "ko"];

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
        if files.len() != FILES.len() && files.len() != FILES.len() + NAME_FILES.len() {
            bail!("the item data is {} scripts (and the {} name files)", FILES.len(), NAME_FILES.len());
        }
        let mut scripts = Vec::new();
        for (file, data) in FILES.iter().chain(&NAME_FILES).zip(files) {
            scripts.push(Script::parse(data).map_err(|error| crate::Error::invalid(format!("{file}: {error}")))?);
        }
        let names = names.or_else(|| files.get(FILES.len() + 1).map(Vec::as_slice));
        let mut shown: BTreeMap<i32, String> = BTreeMap::new();
        if let Some(names) = names {
            for statement in Script::parse(names)?.statements.iter().filter(|statement| statement.tag == "MES_SYS") {
                if let [number, Arg::Str(text)] = &statement.values()[..] {
                    shown.insert(number.int(), dc2tk_disc::text::decode_lossy(text));
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
        let guards = keyed(&scripts[6], "GRD");
        let span = |block: &BTreeMap<&str, usize>| -> Option<(usize, usize)> {
            Some((*block.values().min()?, *block.values().max()?))
        };
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
            let mut block = None;
            match family {
                3 => {
                    if let Some(found) = list_index.filter(|&at| at < weapon_count).and_then(|at| weapons.get(at)) {
                        block_fields(&mut fields, WEAPON, 1, found, &weapon_specs);
                        block = span(found).map(|(first, last)| (1, first, last));
                    }
                }
                2 => {
                    if let Some(found) = list_index.filter(|&at| at < attach_count).and_then(|at| attachments.get(at)) {
                        block_fields(&mut fields, ATTACH, 3, found, &attach_specs);
                        block = span(found).map(|(first, last)| (3, first, last));
                    }
                }
                4 => block = guards.get(&item).map(|&at| (6, at, at)),
                _ => {}
            }
            if matches!(family, 1 | 7 | 8) {
                if let Some(&at) = effects.get(&item) {
                    add(&mut fields, EFFECT, 2, at, &EFFECT_SPECS);
                    block = Some((2, at, at));
                }
            }
            if let Some(&at) = ridepod.get(&item) {
                let kind = scripts[4].statements[at].values().get(1).map_or(-1, Arg::int);
                add(&mut fields, RIDEPOD, 4, at, &ridepod_specs(kind));
                block = Some((4, at, at));
            }
            if family == 6 {
                if let Some(&at) = fish.get(&item) {
                    add(&mut fields, FISH, 5, at, &FISH_SPECS);
                    block = Some((5, at, at));
                }
            }
            records.push(Record { item, name: shown.get(&item).cloned(), fields, com: statement_at, block });
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
                    Arg::Str(text) => Value::from(dc2tk_disc::text::decode_lossy(&text)),
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
            let Some(bytes) = dc2tk_disc::text::encode(text) else {
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

/// The members of an item record that are not fields: the key, the item a new one copies, and
/// its text.
const TEXT_KEYS: [&str; 4] = ["name", "names", "description", "descriptions"];

/// An item name as a `comdatmes` string of a language: the engine's own characters (Shift-JIS),
/// and in French, German, Italian and Spanish the Latin-1 letters as the `[UNI00xx]` codes
/// ConvertFontCode turns into one byte. In English a letter Shift-JIS lacks that the build draws
/// into the English font for the menus' names (font::drawable: é, ŷ) is its plain spelling
/// (font::folded: e, y), as nothing turns codes into those glyphs there.
pub fn name_bytes(text: &str, language: usize) -> Option<Vec<u8>> {
    if language == 1 {
        let mut out = Vec::new();
        for c in text.chars() {
            match dc2tk_disc::text::encode(c.encode_utf8(&mut [0; 4])) {
                Some(bytes) => out.extend(bytes),
                None if crate::font::drawable(c) => out.extend(crate::font::folded(c)?.bytes()),
                None => return None,
            }
        }
        return Some(out);
    }
    if !(2..=5).contains(&language) {
        return dc2tk_disc::text::encode(text);
    }
    let mut out = Vec::new();
    for c in text.chars() {
        match c as u32 {
            0x20..=0x7E => out.push(c as u8),
            code @ 0xA0..=0xFF => out.extend(format!("[UNI00{code:02x}]").bytes()),
            _ => return None,
        }
    }
    Some(out)
}

/// The longest name an item keeps: _MES_SYS converts it in 256 bytes, and the copies an owned
/// item takes (CopyDataWeapon, CopyDataRoboPart, CopyDataFish, CopyDataAttach) keep it in
/// fixed fields: 0x20 bytes for weapons and ridepod parts, 0x1A attachments, 0x15 fish, 0x16
/// boiled fish, the last a NUL.
pub fn name_most(item_type: i32) -> usize {
    match family(item_type) {
        3 | 5 => 0x1F,
        2 => 0x19,
        6 => 0x14,
        8 => 0x15,
        _ => 0xFF,
    }
}

/// The bytes a name takes once ConvertFontCode has turned each 9-character code into one byte.
pub fn converted_len(bytes: &[u8], language: usize) -> usize {
    if !(2..=5).contains(&language) {
        return bytes.len();
    }
    let mut length = 0;
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'[' && bytes.get(at + 8) == Some(&b']') && !bytes[at + 1..at + 8].contains(&b']') {
            at += 9;
        } else {
            at += 1;
        }
        length += 1;
    }
    length
}

/// Per language, the text a record gives: `one` (a string, for `default_all` languages or English
/// alone) and `many` ({"fr": ...}) over it.
fn texts_by_language(
    object: &Map<String, Value>,
    one: &str,
    many: &str,
    default_all: bool,
    label: &str,
) -> Result<BTreeMap<usize, String>> {
    let mut out = BTreeMap::new();
    if let Some(value) = object.get(one) {
        let Some(text) = value.as_str() else {
            bail!("{label}: \"{one}\" is a string");
        };
        if default_all {
            for language in 0..LANGUAGE_NAMES.len() {
                out.insert(language, text.to_string());
            }
        } else {
            out.insert(1, text.to_string());
        }
    }
    if let Some(value) = object.get(many) {
        let Some(map) = value.as_object() else {
            bail!("{label}: \"{many}\" is {{\"en\": ..., \"fr\": ...}}, by language");
        };
        for (short, text) in map {
            let Some(language) = LANGUAGE_NAMES.iter().position(|name| name == short) else {
                bail!("{label}: \"{many}\" has {short}; the languages are {}", LANGUAGE_NAMES.join(", "));
            };
            let Some(text) = text.as_str() else {
                bail!("{label}: \"{many}\".{short} is a string");
            };
            out.insert(language, text.to_string());
        }
    }
    Ok(out)
}

/// The id of an item's name and description messages: MESSAGE_BASE + its `message`.
fn message_id(message: i64, label: &str) -> Result<u16> {
    match u16::try_from(MESSAGE_BASE + message) {
        Ok(id) if message >= 0 => Ok(id),
        _ => bail!("{label}: message {message} gives no message id (10000 + message is at most 65535)"),
    }
}

/// A description as an allmenu.mes message: the item slot line the menu fills, then the text.
fn description_text(text: &str) -> String {
    format!("[String1]\n{text}")
}

impl Items {
    /// Whether the table was read with the name files (comdatmes0..6, files 7 to 13).
    fn has_names(&self) -> bool {
        self.scripts.len() == FILES.len() + NAME_FILES.len()
    }

    /// The MES_SYS statement naming an item in a language's file (the last, which wins).
    fn name_statement(&self, language: usize, item: i32) -> Option<usize> {
        let script = self.scripts.get(FILES.len() + language)?;
        script
            .statements
            .iter()
            .rposition(|statement| statement.tag == "MES_SYS" && statement.values().first().map(Arg::int) == Some(item))
    }

    /// The last statement with a tag in a file.
    fn last_tagged(&self, file: usize, tag: &str) -> Option<usize> {
        self.scripts[file].statements.iter().rposition(|statement| statement.tag == tag)
    }

    /// The changes a mod's table makes to the cfg scripts (see [`Items::changes_all`]).
    pub fn changes(&self, table: &Value, module: &str) -> Result<Vec<Edit>> {
        Ok(self.changes_all(&[(module.to_string(), table.clone())])?.0)
    }

    /// The changes every mod's table makes, in order: the edits of the scripts and the messages
    /// of the message files. A record names its item by `item`. For an item the game has, only
    /// fields whose values differ change; a field the item does not have, or a value its engine
    /// field cannot hold, is refused; `name` (English) and `names` (by language) rename it,
    /// `description` and `descriptions` describe it. An item number the game lacks is a new item:
    /// a copy of the item `like` names with the fields given, its list number in its family's
    /// table given by build, its name in every language. Two mods adding one item differently are
    /// refused, as are additions past what the engine holds.
    pub fn changes_all(&self, tables: &[(String, Value)]) -> Result<(Vec<Edit>, Vec<ItemText>)> {
        let mut edits = Vec::new();
        let mut texts = Vec::new();
        let mut added: BTreeMap<i64, (&str, &Map<String, Value>)> = BTreeMap::new();
        for (module, table) in tables {
            let Some(records) = table.get("items").and_then(Value::as_array) else {
                bail!("an item table is {{\"items\": [records]}}, as mod asset writes it");
            };
            for record in records {
                let Some(object) = record.as_object() else {
                    bail!("each item is an object with \"item\" and the fields it changes");
                };
                let Some(item) = object.get("item").and_then(Value::as_i64) else {
                    bail!("each item names its item number: {{\"item\": 1, ...}}");
                };
                match self.records.iter().rev().find(|found| found.item as i64 == item) {
                    Some(found) => self.record_changes(found, object, module, &mut edits, &mut texts)?,
                    None => {
                        if let Some((other, given)) = added.get(&item) {
                            if *given != object {
                                bail!("new item {item} is given two ways, by mod {other} and mod {module}");
                            }
                            continue;
                        }
                        added.insert(item, (module, object));
                    }
                }
            }
        }
        if !added.is_empty() {
            self.additions(&added, &mut edits, &mut texts)?;
        }
        Ok((edits, texts))
    }

    fn record_changes(
        &self,
        found: &Record,
        object: &Map<String, Value>,
        module: &str,
        out: &mut Vec<Edit>,
        texts: &mut Vec<ItemText>,
    ) -> Result<()> {
        let item = found.item;
        let label = match &found.name {
            Some(name) => format!("item {item} ({name})"),
            None => format!("item {item}"),
        };
        if object.contains_key("like") {
            bail!("{label} is the game's own; \"like\" makes a new item from it");
        }
        let mut names = texts_by_language(object, "name", "names", false, &label)?;
        if names.get(&1) == found.name.as_ref() && !object.contains_key("names") {
            names.clear();
        }
        let message = self.common_value(found, "message").as_i64().unwrap_or(item as i64);
        let item_type = self.common_value(found, "type").as_i64().unwrap_or(0) as i32;
        self.text_changes(item, item_type, message, &label, &names, object, None, module, out, texts)?;
        for (key, value) in object {
            if key == "item" || TEXT_KEYS.contains(&key.as_str()) {
                continue;
            }
            let (group, fields): (usize, Vec<(&String, &Value)>) = match GROUPS.iter().position(|group| group == key) {
                Some(group) if group > 0 => {
                    let Some(nested) = value.as_object() else {
                        bail!("{label}: \"{key}\" is an object of fields");
                    };
                    (group, nested.iter().collect())
                }
                _ => (COMMON, vec![(key, value)]),
            };
            for (name, value) in fields {
                let what = if group == COMMON { format!("{label} {name}") } else { format!("{label} {key}.{name}") };
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
        Ok(())
    }

    fn common_value(&self, record: &Record, name: &str) -> Value {
        record
            .fields
            .iter()
            .find(|field| field.group == COMMON && field.spec.name == name)
            .map_or(Value::Null, |field| self.value(field))
    }

    /// The edits and messages of an item's names and descriptions: each language's MES_SYS (a
    /// changed one rewritten, a missing one added) and system message, and its allmenu message;
    /// `like`, the message number of the item a new one is like, copies that item's description
    /// where none is given.
    #[allow(clippy::too_many_arguments)]
    fn text_changes(
        &self,
        item: i32,
        item_type: i32,
        message: i64,
        label: &str,
        names: &BTreeMap<usize, String>,
        object: &Map<String, Value>,
        like: Option<i64>,
        module: &str,
        out: &mut Vec<Edit>,
        texts: &mut Vec<ItemText>,
    ) -> Result<()> {
        let descriptions = texts_by_language(object, "description", "descriptions", like.is_some(), label)?;
        if names.is_empty() && descriptions.is_empty() && like.is_none() {
            return Ok(());
        }
        let id = message_id(message, label)?;
        if !names.is_empty() && !self.has_names() {
            bail!("{label}: names need the name files (comdatmes0..6)");
        }
        for (&language, name) in names {
            let what = format!("{label} name ({})", LANGUAGE_NAMES[language]);
            let Some(bytes) = name_bytes(name, language) else {
                bail!("{what}: {name:?} has characters the item names of that language cannot hold");
            };
            let most = name_most(item_type);
            if converted_len(&bytes, language) > most {
                bail!("{what}: {name:?} is longer than the {most} bytes the game keeps for the name of an item of type {item_type}");
            }
            let file = FILES.len() + language;
            let text = cfg_text::str_text(&bytes);
            match self.name_statement(language, item) {
                Some(at) => {
                    if self.scripts[file].statements[at].args().get(1).map(|piece| &piece.text) != Some(&text) {
                        out.push(Edit { file, statement: at, arg: 1, to: To::Arg(text), what, module: module.into() });
                    }
                }
                None => {
                    let Some(after) = self.last_tagged(file, "MES_SYS") else {
                        bail!("{what}: {} has no MES_SYS to add one after", NAME_FILES[language]);
                    };
                    let line = cfg_text::statement_text("MES_SYS", &[cfg_text::int_text(item as i64), text]);
                    out.push(Edit {
                        file,
                        statement: after,
                        arg: 0,
                        to: To::Insert(line),
                        what,
                        module: module.into(),
                    });
                }
            }
            texts.push(ItemText {
                file: "system",
                language,
                id,
                text: Some(name.clone()),
                copy: id,
                what: format!("{label} name ({}, system message {id})", LANGUAGE_NAMES[language]),
                module: module.into(),
            });
        }
        for (language, short) in LANGUAGE_NAMES.iter().enumerate() {
            let what = format!("{label} description ({short}, allmenu message {id})");
            match (descriptions.get(&language), like) {
                (Some(text), _) => texts.push(ItemText {
                    file: "allmenu",
                    language,
                    id,
                    text: Some(description_text(text)),
                    copy: id,
                    what,
                    module: module.into(),
                }),
                (None, Some(like)) => texts.push(ItemText {
                    file: "allmenu",
                    language,
                    id,
                    text: None,
                    copy: message_id(like, label)?,
                    what,
                    module: module.into(),
                }),
                (None, None) => {}
            }
        }
        Ok(())
    }

    /// New items: each a copy of the item it is `like`, with the fields given, appended to every
    /// table it belongs in, counted in its family's table.
    fn additions(
        &self,
        added: &BTreeMap<i64, (&str, &Map<String, Value>)>,
        out: &mut Vec<Edit>,
        texts: &mut Vec<ItemText>,
    ) -> Result<()> {
        let total = self.records.len() + added.len();
        if total > COMMON_MOST {
            bail!(
                "{} new items make {total}; the game holds {COMMON_MOST} items (local_com_itemdata), {} more than the disc's",
                added.len(),
                COMMON_MOST - self.records.len()
            );
        }
        let Some(last_com) = self.last_tagged(0, "COM") else {
            bail!("comdat.cfg has no COM statement to add after");
        };
        // the next list number of each family table, and the edits of its count
        let mut next: Vec<(i64, usize)> = Vec::new();
        for (_, file, count_tag, _, _) in FAMILY_TABLES {
            let count = count(&self.scripts[file], count_tag).unwrap_or(0) as i64;
            next.push((count, 0));
        }
        for (&item, &(module, object)) in added {
            let label = format!("new item {item}");
            if !(1..=ITEM_MOST).contains(&item) {
                bail!("{label}: item numbers go from 1 to {ITEM_MOST} (local_itemdatano_converttable)");
            }
            let Some(like_number) = object.get("like").and_then(Value::as_i64) else {
                bail!("{label}: the game has no item {item}; a new item names the item it is like: {{\"item\": {item}, \"like\": 1, \"name\": ...}}");
            };
            let Some(like) = self.records.iter().rev().find(|found| found.item as i64 == like_number) else {
                bail!("{label}: like {like_number}: the game has no such item");
            };
            let like_type = self.common_value(like, "type").as_i64().unwrap_or(0) as i32;
            let like_family = family(like_type);
            // the COM statement: the like item's, with the new number, list and message
            let mut com = self.scripts[0].statements[like.com].clone();
            let set = |statement: &mut cfg_text::Statement, arg: usize, text: Vec<u8>, what: &str| -> Result<()> {
                let Some(piece) = statement.pieces.get(arg) else {
                    bail!("{what}: the copied statement has no argument {arg}");
                };
                statement.pieces[arg] = piece.with_text(text);
                Ok(())
            };
            set(&mut com, 0, cfg_text::int_text(item), &label)?;
            let mut message = item;
            let mut attribute = self.common_value(like, "attribute").as_i64().unwrap_or(0);
            let mut item_type = like_type;
            for (key, value) in object {
                if key == "item" || key == "like" || TEXT_KEYS.contains(&key.as_str()) || GROUPS.contains(&key.as_str())
                {
                    continue;
                }
                let what = format!("{label} {key}");
                let Some(spec) = COMMON_SPECS.iter().find(|spec| spec.name == key) else {
                    bail!("{what}: no such field");
                };
                if key == "list" {
                    bail!("{what}: build gives a new item its list number");
                }
                set(&mut com, spec.arg, value_text(spec, value, &what)?, &what)?;
                match key.as_str() {
                    "message" => message = value.as_i64().unwrap_or(item),
                    "attribute" => attribute = value.as_i64().unwrap_or(0),
                    "type" => item_type = value.as_i64().unwrap_or(0) as i32,
                    _ => {}
                }
            }
            if family(item_type) != like_family {
                bail!("{label} type {item_type}: an item of another family than item {like_number}'s (type {like_type}) is made like one of that family");
            }
            if item > exe_spectrum_items() && attribute & 2 != 0 && !matches!(like_family, 2 | 3) {
                bail!(
                    "{label}: spectrumising an item reads etcitem_spectol_table, which has items 1 to {}; clear the attribute's 2 (spectrumisable) or use a number up to {}",
                    exe_spectrum_items(),
                    exe_spectrum_items()
                );
            }
            // the family's entry, copied
            let table = FAMILY_TABLES.iter().position(|(families, ..)| families.contains(&like_family));
            let mut list = self.common_value(like, "list").as_i64().unwrap_or(0);
            if let Some(table) = table {
                let (_, file, count_tag, last_tag, most) = FAMILY_TABLES[table];
                list = next[table].0 + next[table].1 as i64;
                next[table].1 += 1;
                if list as usize >= most {
                    bail!("{label}: {} entries in {}; the game holds {most} ({count_tag})", list + 1, FILES[file]);
                }
                if let Some((block_file, first, last)) = like.block {
                    let mut block: Vec<cfg_text::Statement> =
                        self.scripts[block_file].statements[first..=last].to_vec();
                    if last_tag != "WEP_BUILD" && last_tag != "AT_ST_SP" {
                        set(&mut block[0], 0, cfg_text::int_text(item), &label)?;
                    }
                    for (group, key) in GROUPS.iter().enumerate().skip(1) {
                        let Some(given) = object.get(*key) else {
                            continue;
                        };
                        let Some(nested) = given.as_object() else {
                            bail!("{label}: \"{key}\" is an object of fields");
                        };
                        for (name, value) in nested {
                            let what = format!("{label} {key}.{name}");
                            let Some(field) =
                                like.fields.iter().find(|field| field.group == group && field.spec.name == name)
                            else {
                                if !like.fields.iter().any(|field| field.group == group) {
                                    bail!("{label}: item {like_number}, which it is like, has no {key} data");
                                }
                                bail!("{what}: no such field");
                            };
                            if group == RIDEPOD && name == "kind" {
                                bail!("{what}: the kind says which fields a part has; make the part like one of that kind");
                            }
                            let values: Vec<&Value> = if field.spec.len == 1 {
                                vec![value]
                            } else {
                                match value.as_array() {
                                    Some(list) if list.len() == field.spec.len => list.iter().collect(),
                                    _ => bail!("{what} is a list of {}", field.spec.len),
                                }
                            };
                            for (offset, value) in values.into_iter().enumerate() {
                                let text = value_text(&field.spec, value, &what)?;
                                set(&mut block[field.statement - first], field.spec.arg + offset, text, &what)?;
                            }
                        }
                    }
                    let Some(after) = self.last_tagged(block_file, last_tag) else {
                        bail!("{label}: {} has no {last_tag} to add after", FILES[block_file]);
                    };
                    out.push(Edit {
                        file: block_file,
                        statement: after,
                        arg: 0,
                        to: To::Insert(cfg_text::block_text(&block)),
                        what: format!("{label} {}", FILES[block_file]),
                        module: module.into(),
                    });
                }
            } else if GROUPS.iter().skip(1).any(|key| object.contains_key(*key)) {
                bail!("{label}: item {like_number}, which it is like, has no family data");
            }
            set(&mut com, 2, cfg_text::int_text(list), &label)?;
            set(&mut com, 8, cfg_text::int_text(message), &label)?;
            out.push(Edit {
                file: 0,
                statement: last_com,
                arg: 0,
                to: To::Insert(cfg_text::block_text(std::slice::from_ref(&com))),
                what: format!("{label} common data"),
                module: module.into(),
            });
            let names = texts_by_language(object, "name", "names", true, &label)?;
            if names.len() < LANGUAGE_NAMES.len() {
                bail!(
                    "{label}: a new item has a \"name\" (every language) or \"names\" in all of {}",
                    LANGUAGE_NAMES.join(", ")
                );
            }
            // the description the like item shows is the message its own `message` numbers (the
            // game's tables give every item its item number, but a table need not)
            let like_message = self.common_value(like, "message").as_i64().unwrap_or(like_number);
            let like = Some(like_message);
            self.text_changes(item as i32, item_type, message, &label, &names, object, like, module, out, texts)?;
        }
        for (table, &(count_old, added)) in next.iter().enumerate() {
            if added == 0 {
                continue;
            }
            let (_, file, count_tag, ..) = FAMILY_TABLES[table];
            let Some(at) = self.last_tagged(file, count_tag) else {
                bail!("{} has no {count_tag} to count the new entries in", FILES[file]);
            };
            out.push(Edit {
                file,
                statement: at,
                arg: 0,
                to: To::Arg(cfg_text::int_text(count_old + added as i64)),
                what: format!("{} {count_tag}", FILES[file]),
                module: "build".into(),
            });
        }
        Ok(())
    }

    /// The scripts with the edits made, by [`FILES`] and [`NAME_FILES`] (cfg_text::apply_edits);
    /// refused when a script outgrows the buffer the engine reads it into, or the names of a
    /// language the game plays outgrow the buffer they are copied into.
    pub fn apply(&self, edits: &[Edit]) -> Result<Vec<Vec<u8>>> {
        let written = cfg_text::apply_edits(&self.scripts, edits)?;
        for ((name, data), old) in FILES.iter().chain(&NAME_FILES).zip(&written).zip(&self.scripts) {
            if data.len() > SCRIPT_BUFFER && *data != old.write() {
                bail!("{name} would be {} bytes; the game reads it into {SCRIPT_BUFFER}", data.len());
            }
        }
        if written.len() == FILES.len() + NAME_FILES.len() && !edits.is_empty() {
            let common = Script::parse(&written[0])?;
            let items: std::collections::BTreeSet<i32> = common
                .statements
                .iter()
                .filter(|statement| statement.tag == "COM")
                .filter_map(|statement| statement.values().first().map(Arg::int))
                .collect();
            for language in PLAYED {
                let units = name_units(&Script::parse(&written[FILES.len() + language])?, &items, language);
                let before = name_units(&self.scripts[FILES.len() + language], &items, language);
                if units >= NAME_UNITS && units != before {
                    bail!(
                        "the {} item names take {units} of the {NAME_UNITS} 16-byte units the game copies them into (gamedata_sysword_buffer, one less than that fits)",
                        LANGUAGE_NAMES[language]
                    );
                }
            }
        }
        Ok(written)
    }
}

/// The 16-byte units the names of the items that have common data take (_MES_SYS copies a name
/// only for an item GetCommonData finds; a later name for one item takes units again).
fn name_units(script: &Script, items: &std::collections::BTreeSet<i32>, language: usize) -> usize {
    script
        .statements
        .iter()
        .filter(|statement| statement.tag == "MES_SYS")
        .filter_map(|statement| match &statement.values()[..] {
            [number, Arg::Str(text)] if items.contains(&number.int()) => {
                Some((converted_len(text, language) + 16) / 16)
            }
            _ => None,
        })
        .sum()
}

/// The items etcitem_spectol_table has (exe_tables::SPECTRUM_ITEMS).
fn exe_spectrum_items() -> i64 {
    crate::exe_tables::SPECTRUM_ITEMS as i64
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
        // a small entry of a list ({"item": 210, "count": 5}) on one line
        Value::Object(map)
            if indent > 2 && map.len() <= 3 && map.values().all(|item| !item.is_object() && !item.is_array()) =>
        {
            let members: Vec<String> = map
                .iter()
                .map(|(key, item)| {
                    format!("{}: {}", serde_json::to_string(key).unwrap(), serde_json::to_string(item).unwrap())
                })
                .collect();
            out.push_str(&format!("{{{}}}", members.join(", ")));
        }
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
        // rows of text, one a line (a dungeon layout's cells, dungeon_layout)
        Value::Array(list)
            if list.iter().any(|item| item.is_object() || item.is_array())
                || (list.len() > 1 && list.iter().all(|item| item.as_str().is_some_and(|text| text.contains(' ')))) =>
        {
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
        assert!(refused(serde_json::json!({"items": [{"item": 2}]})).contains("the game has no item 2"));
        assert!(refused(serde_json::json!({"items": [{"item": 1, "weapon": {"attack": 40000}}]}))
            .contains("-32768 to 32767"));
        assert!(refused(serde_json::json!({"items": [{"item": 1, "fish": {"size": 1}}]})).contains("no fish data"));
        assert!(refused(serde_json::json!({"items": [{"item": 1, "name": "Wrench"}]})).contains("name files"));
        assert!(refused(serde_json::json!({"items": [{"item": 1, "list": 3}]})).contains("list number"));
        assert!(refused(serde_json::json!({"items": [{"item": 1, "model": "a_name_far_too_long"}]})).contains("bytes"));
        assert!(refused(serde_json::json!({"items": [{"item": 1, "weapon": {"build_up": [1]}}]})).contains("list of 3"));
    }

    /// The sample with a name file for every language.
    fn sample_named() -> Vec<Vec<u8>> {
        let mut files = sample();
        files.extend(std::iter::repeat_n(NAMES_SAMPLE.to_vec(), NAME_FILES.len()));
        files
    }

    #[test]
    fn new_items_copy_the_item_they_are_like() {
        use serde_json::json;
        let items = Items::read(&sample_named(), None).unwrap();
        assert_eq!(items.to_json()["items"][0]["name"], "Battle Wrench");
        let edit = json!({"items": [{"item": 429, "like": 1, "name": "Mega Wrench", "description": "Hits hard.",
            "weapon": {"attack": 200, "attack_max": 999}, "names": {"fr": "Clé géante"}}]});
        let (edits, texts) = items.changes_all(&[("m".into(), edit)]).unwrap();
        let files = items.apply(&edits).unwrap();
        let text = |file: usize| String::from_utf8_lossy(&files[file]).to_string();
        assert!(text(0).ends_with("\"rb\",0;\r\nCOM 429,1,1,0,1,300,0,0,429,\"wm001\",3;\r\n"), "{}", text(0));
        assert!(text(1).starts_with("WEPNUM 2;"));
        let block = "WEP_BUILD 2,90,0,-1,-1,-1;\r\nWEP 36,32;\r\nWEP_ST 200,10;\r\nWEP_ST_L 999,19;\r\nWEP_ST2 2,0,0,0, 5 ,0,0,0;\r\nWEP_ST2_L 19,19,19,19,19,19,19,19;\r\nWEP_SPE 1,1,0,3,0,0,0;\r\nWEP_BUILD 2,90,0,-1,-1,-1;\r\n";
        assert!(text(1).ends_with(block), "{}", text(1));
        assert!(text(8).contains("MES_SYS 175,\"Flame Crystal\";\r\nMES_SYS 429,\"Mega Wrench\";"));
        assert!(text(9).contains("MES_SYS 429,\"Cl[UNI00e9] g[UNI00e9]ante\";"));
        let again = Items::read(&files, None).unwrap().to_json();
        let new = again["items"].as_array().unwrap().iter().find(|item| item["item"] == 429).unwrap().clone();
        assert_eq!(
            (&new["name"], &new["list"], &new["weapon"]["attack"]),
            (&json!("Mega Wrench"), &json!(1), &json!(200))
        );
        let system: Vec<_> = texts.iter().filter(|text| text.file == "system").collect();
        assert_eq!(system.len(), 7);
        assert!(system.iter().all(|text| text.id == 10429));
        let allmenu: Vec<_> = texts.iter().filter(|text| text.file == "allmenu").collect();
        assert_eq!(allmenu[1].text.as_deref(), Some("[String1]\nHits hard."));
        // two mods adding it alike add it once; differently, they are refused
        let same = json!({"items": [{"item": 429, "like": 1, "name": "Mega Wrench"}]});
        let (edits, _) = items.changes_all(&[("a".into(), same.clone()), ("b".into(), same)]).unwrap();
        assert!(items.apply(&edits).is_ok());
        let one = |name: &str| json!({"items": [{"item": 429, "like": 1, "name": name}]});
        let error =
            items.changes_all(&[("a".into(), one("Mega")), ("b".into(), one("Other"))]).unwrap_err().to_string();
        assert!(error.contains("mod a and mod b"), "{error}");
        let refused = |edit: Value| items.changes_all(&[("m".into(), edit)]).unwrap_err().to_string();
        assert!(refused(json!({"items": [{"item": 429, "max": 5}]})).contains("like"));
        assert!(refused(json!({"items": [{"item": 512, "like": 1, "name": "x"}]})).contains("1 to 511"));
        assert!(refused(json!({"items": [{"item": 429, "like": 1}]})).contains("\"name\""));
        assert!(refused(json!({"items": [{"item": 429, "like": 1, "name": "x", "type": 16}]})).contains("family"));
        assert!(refused(json!({"items": [{"item": 429, "like": 1, "name": "x", "list": 3}]})).contains("list number"));
        assert!(refused(json!({"items": [{"item": 429, "like": 246, "name": "x", "attribute": 2}]}))
            .contains("etcitem_spectol_table"));
        assert!(
            refused(json!({"items": [{"item": 429, "like": 1, "name": "x", "fish": {"size": 1}}]})).contains("no fish")
        );
        let many: Vec<Value> = (390..510).map(|item| json!({"item": item, "like": 1, "name": "x"})).collect();
        assert!(refused(json!({"items": many})).contains("116"));
        // renaming one of the game's items: English, and a language of its own
        let rename = json!({"items": [{"item": 1, "name": "Big Wrench", "names": {"de": "Schlüssel"}}]});
        let (edits, texts) = items.changes_all(&[("m".into(), rename)]).unwrap();
        let files = items.apply(&edits).unwrap();
        assert!(String::from_utf8_lossy(&files[8]).contains("MES_SYS 1,\"Big Wrench\";"));
        assert!(String::from_utf8_lossy(&files[10]).contains("MES_SYS 1,\"Schl[UNI00fc]ssel\";"));
        assert_eq!(texts.len(), 2);
    }

    #[test]
    fn a_new_item_copies_the_description_its_like_items_message_numbers() {
        let mut files = sample_named();
        // the wrench's message is 7, not its item number
        files[0] = String::from_utf8(files[0].clone()).unwrap().replace("0,0,1,\"wm001\"", "0,0,7,\"wm001\"").into();
        let items = Items::read(&files, None).unwrap();
        let edit = serde_json::json!({"items": [{"item": 429, "like": 1, "name": "Mega Wrench"}]});
        let (_, texts) = items.changes_all(&[("m".into(), edit)]).unwrap();
        let copies: Vec<(u16, u16)> =
            texts.iter().filter(|text| text.file == "allmenu").map(|text| (text.id, text.copy)).collect();
        assert_eq!(copies, vec![(10429, 10007); LANGUAGE_NAMES.len()]);
    }

    #[test]
    fn renaming_an_item_whose_name_line_has_no_name_is_refused_not_a_panic() {
        let mut files = sample_named();
        files[FILES.len() + 1] = b"MES_SYS 1;\r\n".to_vec();
        let items = Items::read(&files, None).unwrap();
        let edits = items.changes(&serde_json::json!({"items": [{"item": 1, "name": "Big Wrench"}]}), "m").unwrap();
        let error = items.apply(&edits).unwrap_err().to_string();
        assert!(error.contains("item 1 name (en)") && error.contains("no argument 1"), "{error}");
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

    /// The game's item tables with their name files (C2S_CORPUS): read back unchanged, and items
    /// added up to what the engine holds (432 records: here 4 weapons, 2 usable items, 2
    /// attachments and a fish, each family's own limit met too), a tenth refused.
    #[test]
    fn corpus_items_are_added_up_to_the_engine_limits() {
        let Some(corpus) = std::env::var_os("C2S_CORPUS") else {
            return;
        };
        let folder = std::path::Path::new(&corpus).join(FOLDER);
        let files: Vec<Vec<u8>> =
            FILES.iter().chain(&NAME_FILES).map(|file| std::fs::read(folder.join(file)).unwrap()).collect();
        let items = Items::read(&files, None).unwrap();
        assert_eq!(items.write(), files);
        assert_eq!(items.apply(&[]).unwrap(), files);
        let json = items.to_json();
        let like = |wanted: i32| -> i64 {
            json["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|record| family(record["type"].as_i64().unwrap() as i32) == wanted)
                .unwrap()["item"]
                .as_i64()
                .unwrap()
        };
        let likes = [like(3), like(3), like(3), like(3), like(1), like(1), like(2), like(2), like(6)];
        let mut records: Vec<Value> = likes
            .iter()
            .enumerate()
            .map(|(at, like)| serde_json::json!({"item": 429 + at, "like": like, "name": format!("New {at}")}))
            .collect();
        // past etcitem_spectol_table, the usable items and the fish may not be spectrumised
        for record in &mut records[4..] {
            record["attribute"] = serde_json::json!(1);
        }
        let (edits, texts) = items.changes_all(&[("m".into(), serde_json::json!({ "items": records }))]).unwrap();
        let written = items.apply(&edits).unwrap();
        let again = Items::read(&written, None).unwrap();
        assert_eq!(again.len(), COMMON_MOST);
        assert_eq!(again.to_json()["items"][431]["name"], "New 8");
        assert_eq!(count(&again.scripts[1], "WEPNUM"), Some(116));
        assert_eq!(texts.iter().filter(|text| text.file == "system").count(), 9 * 7);
        records.push(serde_json::json!({"item": 500, "like": like(4), "name": "One too many"}));
        let error = items.changes_all(&[("m".into(), serde_json::json!({ "items": records }))]).unwrap_err();
        assert!(error.to_string().contains("432"), "{error}");
    }
}
