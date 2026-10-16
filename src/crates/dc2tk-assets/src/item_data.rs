//! The game's item master data (docs/game/data.md, "Items") as one JSON table with named fields,
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
use crate::data_table::{self, same, set_piece, value_text, values_of, Kind, B16, B8, S16};
use crate::lang::{converted_len, name_bytes, ENGLISH, LANGUAGE_NAMES};
use crate::{bail, Context, Result};

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
/// aligned to 64 bytes first (MenuCalcBufAlignment), which loses at most 0x40 of it (see
/// georama_data::DOWNLOAD_BUFFER, which counts on a 16-byte-aligned start).
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

/// The scripts, by their place in FILES.
const COMDAT: usize = 0;
const WEPDAT: usize = 1;
const ITEMDAT: usize = 2;
const ATDAT: usize = 3;
const ROBODAT: usize = 4;
const FISHDAT: usize = 5;
const GRDDAT: usize = 6;

/// The groups of a record's fields, by their place in GROUPS: the common data and each family's.
const COMMON: usize = 0;
const WEAPON: usize = 1;
const EFFECT: usize = 2;
const ATTACH: usize = 3;
const RIDEPOD: usize = 4;
const FISH: usize = 5;

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

/// The COM arguments a new item's are set from: its family table's list number, its messages'.
const COM_LIST: usize = 2;
const COM_MESSAGE: usize = 8;

/// `COM item_no, type, list_no, active_set, stack_num, max_num, icon_sheet, icon_no, message_no,
/// "file", attribute` (_DATACOM, into CDataCommon).
const COMMON_SPECS: [Spec; 10] = [
    one("type", "COM", 1, B8),
    one("list", "COM", COM_LIST, S16),
    one("active_set", "COM", 3, B8),
    one("stack", "COM", 4, S16),
    one("max", "COM", 5, B16),
    one("icon_sheet", "COM", 6, B8),
    one("icon", "COM", 7, S16),
    one("message", "COM", COM_MESSAGE, S16),
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
        one("synthesis_points", "WEP_SPE", 0, B8),
        one("palette", "WEP_SPE", 1, B8),
        one("unknown_47", "WEP_SPE", 2, B8),
        one("synthesis_per_level", "WEP_SPE", 3, B8),
        one("special", "WEP_SPE", 4, BITS),
        one("attack_type", "WEP_SPE", 5, B8),
        one("model_no", "WEP_SPE", 6, B8),
        many("build_up", "WEP_BUILD", 0, 3, S16),
        many("build_up_kills", "WEP_BUILD", 3, 3, S16),
    ]);
    specs
}

const EFFECT_SPECS: [Spec; 4] = [
    one("use_flags", "ITEM", 1, BITS),
    one("status_flags", "ITEM", 2, BITS),
    one("target_flags", "ITEM", 3, B16),
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
        vec![one("kind", "RB_PARTS", 1, B8), one("capacity", "RB_PARTS", 2, S16), one("file_no", "RB_PARTS", 3, B8)];
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
const FISH_SPECS: [Spec; 6] = [
    one("size", "FISH", 1, Kind::Float),
    one("unknown_4", "FISH", 2, S16),
    one("power", "FISH", 3, S16),
    many("speed", "FISH", 4, 3, S16),
    one("stamina", "FISH", 7, S16),
    one("unknown_10", "FISH", 8, S16),
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

/// A family table a new item's list number is counted in.
struct FamilyTable {
    families: &'static [Family],
    file: usize,
    /// The statement that counts the entries, and the tag that ends an entry.
    count: &'static str,
    last: &'static str,
    /// The most entries the engine holds.
    most: usize,
    /// Whether an entry names its item first (an effect, ridepod part, fish or guard); a weapon's
    /// or attachment's block is the n-th, for list number n, and names none.
    by_item: bool,
}

/// The family tables: local_weapondata[116], local_attachdata[38], local_itemdata[162],
/// local_robodata[68], local_fishdata[20], local_guarddata[40].
const FAMILY_TABLES: [FamilyTable; 6] = [
    FamilyTable {
        families: &[Family::Weapon],
        file: WEPDAT,
        count: "WEPNUM",
        last: "WEP_BUILD",
        most: 116,
        by_item: false,
    },
    FamilyTable {
        families: &[Family::Attachment],
        file: ATDAT,
        count: "AT_INIT",
        last: "AT_ST_SP",
        most: 38,
        by_item: false,
    },
    FamilyTable {
        families: &[Family::Item, Family::GiftBox, Family::Boiled],
        file: ITEMDAT,
        count: "ITEMINIT",
        last: "ITEM",
        most: 162,
        by_item: true,
    },
    FamilyTable {
        families: &[Family::Ridepod],
        file: ROBODAT,
        count: "ROBOINIT",
        last: "RB_PARTS",
        most: 68,
        by_item: true,
    },
    FamilyTable { families: &[Family::Fish], file: FISHDAT, count: "FISHINIT", last: "FISH", most: 20, by_item: true },
    FamilyTable { families: &[Family::Clothing], file: GRDDAT, count: "GRDNUM", last: "GRD", most: 40, by_item: true },
];

/// A new item a mod gives: its number and label, the mod, what it gives and the item it is like.
struct NewItem<'a> {
    item: i64,
    label: String,
    module: &'a str,
    object: &'a Map<String, Value>,
    like: &'a Record,
    like_number: i64,
}

/// The common fields a new item's other parts follow: its message number, attribute and type.
struct NewCommon {
    message: i64,
    attribute: i64,
    item_type: i32,
}

/// What an item's names and descriptions are written for: its number, type and message number,
/// its label in messages, and for a new item the message number of the item it is like.
struct TextChange<'a> {
    item: i32,
    item_type: i32,
    message: i64,
    label: &'a str,
    like: Option<i64>,
}

/// A message an item's name or description puts into the message files: `file` "system"
/// (meswin/system_N.mes, the names item slots and menus show) or "allmenu" (each menu pack's
/// allmenu.mes, the descriptions), `language` the LanguageCode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemText {
    pub file: &'static str,
    pub language: usize,
    pub id: u16,
    /// The message in the text form of docs/modding/text.md; None copies the message `copy` of
    /// the same file (a new item's description, from the item it is like).
    pub text: Option<String>,
    pub copy: u16,
    pub what: String,
    pub module: String,
}

/// ConvertUsedItemType's families of item types (gamedata.cpp): which family table holds a type's
/// entry, and how long its name may be. The engine numbers them as the comments say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// 0: a type of no family.
    None,
    /// 1: usable items (ITEM).
    Item,
    /// 2: attachments (AT_ST blocks).
    Attachment,
    /// 3: weapons (WEP blocks).
    Weapon,
    /// 4: clothing (GRD, the guard table nothing reads).
    Clothing,
    /// 5: ridepod parts (RB_PARTS).
    Ridepod,
    /// 6: fish (FISH).
    Fish,
    /// 7: gift boxes, usable items.
    GiftBox,
    /// 8: boiled fish, usable items.
    Boiled,
}

/// ConvertUsedItemType (gamedata.cpp): the family of an item type.
pub fn family(item_type: i32) -> Family {
    match item_type {
        0x1C => Family::GiftBox,
        0x1E => Family::Fish,
        0x23 => Family::Boiled,
        1..=4 => Family::Weapon,
        5..=10 => Family::Clothing,
        12..=15 => Family::Ridepod,
        16..=19 | 0x22 => Family::Attachment,
        11 => Family::Item,
        t if t >= 20 => Family::Item,
        _ => Family::None,
    }
}

/// Blocks of statements that fill one entry, ended by the tag that moves the engine's cursor on:
/// {tag: statement index} each, keyed by the tags given.
fn blocks<'t>(script: &Script, tags: &[&'t str], last: &str) -> Vec<BTreeMap<&'t str, usize>> {
    let mut out = Vec::new();
    let mut current: BTreeMap<&'t str, usize> = BTreeMap::new();
    for (at, statement) in script.statements.iter().enumerate() {
        let Some(&tag) = tags.iter().find(|tag| **tag == statement.tag) else {
            continue;
        };
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

/// The English item names a `comdatmes` file gives (MES_SYS item, "name"), by item number: the
/// labels of the item and shop tables.
pub fn item_names(data: &[u8]) -> Result<BTreeMap<i32, String>> {
    let mut out = BTreeMap::new();
    for statement in
        Script::parse(data).context(NAMES)?.statements.iter().filter(|statement| statement.tag == "MES_SYS")
    {
        if let [number, Arg::Str(text)] = &statement.values()[..] {
            out.insert(number.int(), dc2tk_disc::text::decode_lossy(text));
        }
    }
    Ok(out)
}

/// The family tables' entries as LoadData fills them, to find each item's: the weapons' and
/// attachments' blocks by list number (as many as their counts say), the rest by item number.
struct Entries<'s> {
    scripts: &'s [Script],
    weapons: Vec<BTreeMap<&'static str, usize>>,
    attachments: Vec<BTreeMap<&'static str, usize>>,
    effects: BTreeMap<i32, usize>,
    ridepod: BTreeMap<i32, usize>,
    fish: BTreeMap<i32, usize>,
    guards: BTreeMap<i32, usize>,
    weapon_specs: Vec<Spec>,
    attach_specs: Vec<Spec>,
}

impl<'s> Entries<'s> {
    fn read(scripts: &'s [Script]) -> Entries<'s> {
        let weapon_tags = ["WEP", "WEP_ST", "WEP_ST_L", "WEP_ST2", "WEP_ST2_L", "WEP_SPE", "WEP_BUILD"];
        let mut weapons = blocks(&scripts[WEPDAT], &weapon_tags, "WEP_BUILD");
        weapons.truncate(count(&scripts[WEPDAT], "WEPNUM").unwrap_or(0).max(0) as usize);
        let mut attachments = blocks(&scripts[ATDAT], &["AT_ST", "AT_ST2", "AT_ST_SP"], "AT_ST_SP");
        attachments.truncate(count(&scripts[ATDAT], "AT_INIT").unwrap_or(0).max(0) as usize);
        Entries {
            scripts,
            weapons,
            attachments,
            effects: keyed(&scripts[ITEMDAT], "ITEM"),
            ridepod: keyed(&scripts[ROBODAT], "RB_PARTS"),
            fish: keyed(&scripts[FISHDAT], "FISH"),
            guards: keyed(&scripts[GRDDAT], "GRD"),
            weapon_specs: weapon_specs(),
            attach_specs: attach_specs(),
        }
    }

    /// The fields of specs a statement has the arguments of.
    fn add(&self, fields: &mut Vec<Located>, group: usize, file: usize, statement: usize, specs: &[Spec]) {
        let args = self.scripts[file].statements[statement].args().len();
        for spec in specs.iter().filter(|spec| spec.arg + spec.len <= args) {
            fields.push(Located { group, spec: *spec, file, statement });
        }
    }

    /// The fields of a block's statements, each spec in the statement of its tag; the block's
    /// span (file, first, last).
    fn add_block(
        &self,
        fields: &mut Vec<Located>,
        group: usize,
        file: usize,
        block: &BTreeMap<&str, usize>,
        specs: &[Spec],
    ) -> Option<(usize, usize, usize)> {
        for spec in specs {
            if let Some(&statement) = block.get(spec.tag) {
                self.add(fields, group, file, statement, std::slice::from_ref(spec));
            }
        }
        Some((file, *block.values().min()?, *block.values().max()?))
    }

    /// The record of a COM statement: its common fields, and those of its family's entry.
    fn record(&self, com: usize, names: &BTreeMap<i32, String>) -> Record {
        let values = self.scripts[COMDAT].statements[com].values();
        let item = values.first().map_or(0, Arg::int);
        let item_type = values.get(1).map_or(0, Arg::int);
        let list = usize::try_from(values.get(2).map_or(-1, Arg::int)).ok();
        let mut fields = Vec::new();
        self.add(&mut fields, COMMON, COMDAT, com, &COMMON_SPECS);
        let family = family(item_type);
        let mut block = None;
        match family {
            Family::Weapon => {
                if let Some(found) = list.and_then(|at| self.weapons.get(at)) {
                    block = self.add_block(&mut fields, WEAPON, WEPDAT, found, &self.weapon_specs);
                }
            }
            Family::Attachment => {
                if let Some(found) = list.and_then(|at| self.attachments.get(at)) {
                    block = self.add_block(&mut fields, ATTACH, ATDAT, found, &self.attach_specs);
                }
            }
            Family::Clothing => block = self.guards.get(&item).map(|&at| (GRDDAT, at, at)),
            _ => {}
        }
        if matches!(family, Family::Item | Family::GiftBox | Family::Boiled) {
            if let Some(&at) = self.effects.get(&item) {
                self.add(&mut fields, EFFECT, ITEMDAT, at, &EFFECT_SPECS);
                block = Some((ITEMDAT, at, at));
            }
        }
        if let Some(&at) = self.ridepod.get(&item) {
            let kind = self.scripts[ROBODAT].statements[at].values().get(1).map_or(-1, Arg::int);
            self.add(&mut fields, RIDEPOD, ROBODAT, at, &ridepod_specs(kind));
            block = Some((ROBODAT, at, at));
        }
        if family == Family::Fish {
            if let Some(&at) = self.fish.get(&item) {
                self.add(&mut fields, FISH, FISHDAT, at, &FISH_SPECS);
                block = Some((FISHDAT, at, at));
            }
        }
        Record { item, name: names.get(&item).cloned(), fields, com, block }
    }
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
            scripts.push(Script::parse(data).context(file)?);
        }
        let names = names.or_else(|| files.get(FILES.len() + ENGLISH).map(Vec::as_slice));
        let shown = match names {
            Some(names) => item_names(names)?,
            None => BTreeMap::new(),
        };
        let entries = Entries::read(&scripts);
        let records = scripts[COMDAT]
            .statements
            .iter()
            .enumerate()
            .filter(|(_, statement)| statement.tag == "COM")
            .map(|(com, _)| entries.record(com, &shown))
            .collect();
        Ok(Items { scripts, records })
    }

    /// The number of records (COM statements).
    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// One argument of a field as the JSON shows it: float arguments widened to f64, as this
    /// table has always written them (data_table::widened_float_value), the rest as data_table
    /// reads them.
    fn arg_value(&self, field: &Located, at: usize) -> Value {
        let arg = self.scripts[field.file].statements[field.statement].args()[at].value();
        match (field.spec.kind, arg) {
            (Kind::Float, Arg::Float(value)) => data_table::widened_float_value(value),
            (kind, arg) => data_table::arg_value(arg, kind),
        }
    }

    fn value(&self, field: &Located) -> Value {
        if field.spec.len == 1 {
            self.arg_value(field, field.spec.arg)
        } else {
            Value::Array(
                (field.spec.arg..field.spec.arg + field.spec.len).map(|at| self.arg_value(field, at)).collect(),
            )
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

/// The members of an item record that are not fields: the key, the item a new one copies, and
/// its text.
const TEXT_KEYS: [&str; 4] = ["name", "names", "description", "descriptions"];

/// The longest name an item keeps: _MES_SYS converts it in 256 bytes, and the copies an owned
/// item takes (CopyDataWeapon, CopyDataRoboPart, CopyDataFish, CopyDataAttach) keep it in
/// fixed fields: 0x20 bytes for weapons and ridepod parts, 0x1A attachments, 0x15 fish, 0x16
/// boiled fish, the last a NUL.
pub fn name_most(item_type: i32) -> usize {
    match family(item_type) {
        Family::Weapon | Family::Ridepod => 0x1F,
        Family::Attachment => 0x19,
        Family::Fish => 0x14,
        Family::Boiled => 0x15,
        _ => 0xFF,
    }
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
            out.insert(ENGLISH, text.to_string());
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
            self.module_changes(table, module, &mut edits, &mut texts, &mut added)
                .with_context(|| format!("mod {module}"))?;
        }
        if !added.is_empty() {
            self.additions(&added, &mut edits, &mut texts)?;
        }
        Ok((edits, texts))
    }

    /// One mod's changes of the game's items; the items it adds go into `added`, once each.
    fn module_changes<'a>(
        &self,
        table: &'a Value,
        module: &'a str,
        edits: &mut Vec<Edit>,
        texts: &mut Vec<ItemText>,
        added: &mut BTreeMap<i64, (&'a str, &'a Map<String, Value>)>,
    ) -> Result<()> {
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
                Some(found) => self.record_changes(found, object, module, edits, texts)?,
                None => match added.get(&item) {
                    Some((other, given)) if *given != object => {
                        return Err(crate::given_two_ways(format!("new item {item}"), other, module));
                    }
                    Some(_) => {}
                    None => {
                        added.insert(item, (module, object));
                    }
                },
            }
        }
        Ok(())
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
        if names.get(&ENGLISH) == found.name.as_ref() && !object.contains_key("names") {
            names.clear();
        }
        let message = self.common_value(found, "message").as_i64().unwrap_or(item as i64);
        let item_type = self.common_value(found, "type").as_i64().unwrap_or(0) as i32;
        let text = TextChange { item, item_type, message, label: &label, like: None };
        self.text_changes(&text, &names, object, module, out, texts)?;
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
                let olds: Vec<Value> = match &old {
                    Value::Array(list) => list.clone(),
                    other => vec![other.clone()],
                };
                let values = values_of(value, field.spec.len, &what)?;
                for (offset, ((what, value), old)) in values.into_iter().zip(olds).enumerate() {
                    let text = value_text(field.spec.kind, value, &what)?;
                    if same(&old, value) {
                        continue;
                    }
                    out.push(Edit {
                        file: field.file,
                        statement: field.statement,
                        arg: field.spec.arg + offset,
                        to: To::Arg(text),
                        what,
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
    fn text_changes(
        &self,
        change: &TextChange,
        names: &BTreeMap<usize, String>,
        object: &Map<String, Value>,
        module: &str,
        out: &mut Vec<Edit>,
        texts: &mut Vec<ItemText>,
    ) -> Result<()> {
        let TextChange { item, item_type, message, label, like } = *change;
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
                    let args = [cfg_text::int_text(item as i64), text];
                    let line = cfg_text::statement_text(self.scripts[file].newline(), "MES_SYS", &args);
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
        let Some(last_com) = self.last_tagged(COMDAT, "COM") else {
            bail!("comdat.cfg has no COM statement to add after");
        };
        // each family table's count, and how many entries the new items add to it
        let mut counts: Vec<(i64, usize)> = FAMILY_TABLES
            .iter()
            .map(|table| (count(&self.scripts[table.file], table.count).unwrap_or(0) as i64, 0))
            .collect();
        for (&item, &(module, object)) in added {
            let new = self.new_item(item, object, module)?;
            self.addition(&new, last_com, &mut counts, out, texts).with_context(|| format!("mod {module}"))?;
        }
        self.count_edits(&counts, out)
    }

    /// A new item's number checked, and the item it is like found.
    fn new_item<'a>(&'a self, item: i64, object: &'a Map<String, Value>, module: &'a str) -> Result<NewItem<'a>> {
        let label = format!("new item {item}");
        let found = || -> Result<NewItem<'a>> {
            if !(1..=ITEM_MOST).contains(&item) {
                bail!("{label}: item numbers go from 1 to {ITEM_MOST} (local_itemdatano_converttable)");
            }
            let Some(like_number) = object.get("like").and_then(Value::as_i64) else {
                bail!("{label}: the game has no item {item}; a new item names the item it is like: {{\"item\": {item}, \"like\": 1, \"name\": ...}}");
            };
            let Some(like) = self.records.iter().rev().find(|found| found.item as i64 == like_number) else {
                bail!("{label}: like {like_number}: the game has no such item");
            };
            Ok(NewItem { item, label: label.clone(), module, object, like, like_number })
        };
        found().with_context(|| format!("mod {module}"))
    }

    /// One new item: its COM statement, its family's entry and its names and description.
    fn addition(
        &self,
        new: &NewItem,
        last_com: usize,
        counts: &mut [(i64, usize)],
        out: &mut Vec<Edit>,
        texts: &mut Vec<ItemText>,
    ) -> Result<()> {
        let label = &new.label;
        let (mut com, common) = self.new_common(new)?;
        let like_type = self.common_value(new.like, "type").as_i64().unwrap_or(0) as i32;
        let like_family = family(like_type);
        if family(common.item_type) != like_family {
            bail!(
                "{label} type {}: an item of another family than item {}'s (type {like_type}) is made like one of that family",
                common.item_type,
                new.like_number
            );
        }
        let spectrum_items = crate::exe_tables::SPECTRUM_ITEMS as i64;
        let spectrumised = !matches!(like_family, Family::Attachment | Family::Weapon);
        if new.item > spectrum_items && common.attribute & 2 != 0 && spectrumised {
            bail!(
                "{label}: spectrumising an item reads etcitem_spectol_table, which has items 1 to {spectrum_items}; clear the attribute's 2 (spectrumisable) or use a number up to {spectrum_items}"
            );
        }
        let list = match FAMILY_TABLES.iter().position(|table| table.families.contains(&like_family)) {
            Some(table) => self.family_entry(new, table, counts, out)?,
            None if GROUPS.iter().skip(1).any(|key| new.object.contains_key(*key)) => {
                bail!("{label}: item {}, which it is like, has no family data", new.like_number);
            }
            None => self.common_value(new.like, "list").as_i64().unwrap_or(0),
        };
        set_piece(&mut com, COM_LIST, cfg_text::int_text(list), label)?;
        set_piece(&mut com, COM_MESSAGE, cfg_text::int_text(common.message), label)?;
        out.push(Edit {
            file: COMDAT,
            statement: last_com,
            arg: 0,
            to: To::Insert(cfg_text::block_text(self.scripts[COMDAT].newline(), std::slice::from_ref(&com))),
            what: format!("{label} common data"),
            module: new.module.into(),
        });
        let names = texts_by_language(new.object, "name", "names", true, label)?;
        if names.len() < LANGUAGE_NAMES.len() {
            bail!(
                "{label}: a new item has a \"name\" (every language) or \"names\" in all of {}",
                LANGUAGE_NAMES.join(", ")
            );
        }
        // the description the like item shows is the message its own `message` numbers (the
        // game's tables give every item its item number, but a table need not)
        let like_message = self.common_value(new.like, "message").as_i64().unwrap_or(new.like_number);
        let item = new.item as i32;
        let text =
            TextChange { item, item_type: common.item_type, message: common.message, label, like: Some(like_message) };
        self.text_changes(&text, &names, new.object, new.module, out, texts)
    }

    /// A new item's COM statement: the like item's, with the new number and the common fields
    /// given (its list and message numbers are set once known).
    fn new_common(&self, new: &NewItem) -> Result<(cfg_text::Statement, NewCommon)> {
        let mut com = self.scripts[COMDAT].statements[new.like.com].clone();
        set_piece(&mut com, 0, cfg_text::int_text(new.item), &new.label)?;
        let mut common = NewCommon {
            message: new.item,
            attribute: self.common_value(new.like, "attribute").as_i64().unwrap_or(0),
            item_type: self.common_value(new.like, "type").as_i64().unwrap_or(0) as i32,
        };
        for (key, value) in new.object {
            if key == "item" || key == "like" || TEXT_KEYS.contains(&key.as_str()) || GROUPS.contains(&key.as_str()) {
                continue;
            }
            let what = format!("{} {key}", new.label);
            let Some(spec) = COMMON_SPECS.iter().find(|spec| spec.name == key) else {
                bail!("{what}: no such field");
            };
            if key == "list" {
                bail!("{what}: build gives a new item its list number");
            }
            set_piece(&mut com, spec.arg, value_text(spec.kind, value, &what)?, &what)?;
            match key.as_str() {
                "message" => common.message = value.as_i64().unwrap_or(new.item),
                "attribute" => common.attribute = value.as_i64().unwrap_or(0),
                "type" => common.item_type = value.as_i64().unwrap_or(0) as i32,
                _ => {}
            }
        }
        Ok((com, common))
    }

    /// A new item's entry in its family's table: the next list number, and the like item's entry
    /// copied with the fields given, after the table's last.
    fn family_entry(
        &self,
        new: &NewItem,
        table: usize,
        counts: &mut [(i64, usize)],
        out: &mut Vec<Edit>,
    ) -> Result<i64> {
        let label = &new.label;
        let FamilyTable { file, count, last, most, by_item, .. } = FAMILY_TABLES[table];
        let list = counts[table].0 + counts[table].1 as i64;
        counts[table].1 += 1;
        if list as usize >= most {
            bail!("{label}: {} entries in {}; the game holds {most} ({count})", list + 1, FILES[file]);
        }
        let Some((block_file, first, end)) = new.like.block else {
            return Ok(list);
        };
        let mut block: Vec<cfg_text::Statement> = self.scripts[block_file].statements[first..=end].to_vec();
        if by_item {
            set_piece(&mut block[0], 0, cfg_text::int_text(new.item), label)?;
        }
        for (group, key) in GROUPS.iter().enumerate().skip(1) {
            let Some(given) = new.object.get(*key) else {
                continue;
            };
            let Some(nested) = given.as_object() else {
                bail!("{label}: \"{key}\" is an object of fields");
            };
            for (name, value) in nested {
                let what = format!("{label} {key}.{name}");
                let Some(field) = new.like.fields.iter().find(|field| field.group == group && field.spec.name == name)
                else {
                    if !new.like.fields.iter().any(|field| field.group == group) {
                        bail!("{label}: item {}, which it is like, has no {key} data", new.like_number);
                    }
                    bail!("{what}: no such field");
                };
                if group == RIDEPOD && name == "kind" {
                    bail!("{what}: the kind says which fields a part has; make the part like one of that kind");
                }
                for (offset, (what, value)) in values_of(value, field.spec.len, &what)?.into_iter().enumerate() {
                    let text = value_text(field.spec.kind, value, &what)?;
                    set_piece(&mut block[field.statement - first], field.spec.arg + offset, text, &what)?;
                }
            }
        }
        let Some(after) = self.last_tagged(block_file, last) else {
            bail!("{label}: {} has no {last} to add after", FILES[block_file]);
        };
        out.push(Edit {
            file: block_file,
            statement: after,
            arg: 0,
            to: To::Insert(cfg_text::block_text(self.scripts[block_file].newline(), &block)),
            what: format!("{label} {}", FILES[block_file]),
            module: new.module.into(),
        });
        Ok(list)
    }

    /// The family tables' counts raised by the entries the new items add.
    fn count_edits(&self, counts: &[(i64, usize)], out: &mut Vec<Edit>) -> Result<()> {
        for (table, &(count_old, added)) in counts.iter().enumerate() {
            if added == 0 {
                continue;
            }
            let FamilyTable { file, count, .. } = FAMILY_TABLES[table];
            let Some(at) = self.last_tagged(file, count) else {
                bail!("{} has no {count} to count the new entries in", FILES[file]);
            };
            out.push(Edit {
                file,
                statement: at,
                arg: 0,
                to: To::Arg(cfg_text::int_text(count_old + added as i64)),
                what: format!("{} {count}", FILES[file]),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::json_text;

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
    fn whole_numbers_compare_exactly_not_as_f32() {
        // 16777217 and 16777216 are one f32: the item table compared them so, and dropped the edit
        let mut files = sample();
        files[0] = String::from_utf8(files[0].clone()).unwrap().replace("\"wm001\",3;", "\"wm001\",16777216;").into();
        let items = Items::read(&files, None).unwrap();
        let edits = items.changes(&serde_json::json!({"items": [{"item": 1, "attribute": 16777217}]}), "m").unwrap();
        assert_eq!(edits.len(), 1);
        assert!(String::from_utf8_lossy(&items.apply(&edits).unwrap()[0]).contains("\"wm001\",16777217;"));
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
        use Family::{Attachment, Boiled, Clothing, Fish, GiftBox, Item, Ridepod, Weapon};
        let families: Vec<Family> =
            [0, 1, 5, 11, 12, 16, 0x1C, 0x1E, 0x22, 0x23, 40].iter().map(|&t| family(t)).collect();
        let wanted =
            [Family::None, Weapon, Clothing, Item, Ridepod, Attachment, GiftBox, Fish, Attachment, Boiled, Item];
        assert_eq!(families, wanted);
    }

    /// Every version of the item data on an extracted disc (DC2TK_CORPUS: menu/cfg5, cfg6 and the
    /// cfg7 the game reads): the scripts round-trip byte for byte, every field's text is the one
    /// its value writes, and the JSON goes back to the same files.
    #[test]
    #[ignore = "needs DC2TK_CORPUS"]
    fn corpus_item_data_round_trips() {
        let corpus = dc2tk_disc::samples::corpus();
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
                        let value = items.arg_value(field, at);
                        let what = format!("{} item {} {}", folder.display(), record.item, field.spec.name);
                        let text = value_text(field.spec.kind, &value, &what).unwrap();
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

    /// The game's item tables with their name files (DC2TK_CORPUS): read back unchanged, and items
    /// added up to what the engine holds (432 records: here 4 weapons, 2 usable items, 2
    /// attachments and a fish, each family's own limit met too), a tenth refused.
    #[test]
    #[ignore = "needs DC2TK_CORPUS"]
    fn corpus_items_are_added_up_to_the_engine_limits() {
        use Family::{Attachment, Clothing, Fish, Item, Weapon};
        let corpus = dc2tk_disc::samples::corpus();
        let folder = std::path::Path::new(&corpus).join(FOLDER);
        let files: Vec<Vec<u8>> =
            FILES.iter().chain(&NAME_FILES).map(|file| std::fs::read(folder.join(file)).unwrap()).collect();
        let items = Items::read(&files, None).unwrap();
        assert_eq!(items.write(), files);
        assert_eq!(items.apply(&[]).unwrap(), files);
        let json = items.to_json();
        let like = |wanted: Family| -> i64 {
            json["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|record| family(record["type"].as_i64().unwrap() as i32) == wanted)
                .unwrap()["item"]
                .as_i64()
                .unwrap()
        };
        let likes = [like(Weapon), like(Weapon), like(Weapon), like(Weapon), like(Item), like(Item)]
            .into_iter()
            .chain([like(Attachment), like(Attachment), like(Fish)])
            .collect::<Vec<_>>();
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
        records.push(serde_json::json!({"item": 500, "like": like(Clothing), "name": "One too many"}));
        let error = items.changes_all(&[("m".into(), serde_json::json!({ "items": records }))]).unwrap_err();
        assert!(error.to_string().contains("432"), "{error}");
    }
}
