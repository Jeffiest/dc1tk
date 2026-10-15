//! A table of game data (docs/assets/data.md) kept where the engine reads it: arguments of cfg
//! script statements ([`cfg_text`]) and bytes of the boot executable's `.data`, as one JSON table
//! of named records. The kinds of data built on it (inventions, Georama, floor medals, monster
//! placement, fishing, character tables, spectrumising, Donny's shop) say where each field is;
//! this module reads the values, writes the JSON and turns a mod's JSON into edits and patches.
//!
//! The JSON is `{"section": [record, ...], ...}`. A record is named by its keys (`{"dungeon": 0,
//! "floor": 3}`); labels (names, which are text) may be given but not changed; every other member
//! is a field, `a.b` names nested as `{"a": {"b": ...}}`. A field is one argument or several in a
//! row (a list), a list inside a statement (DATASET's materials), a list over several statements
//! (FL lines), or values in the executable. A field kept in several files (the copies of
//! `inv6.lst`, a Georama part in each language's `.gpi`) is read from the first and written to
//! every copy, so they stay in step.
//!
//! As for the item table ([`crate::item_data`]), a mod's table holds only what it changes: a value
//! the same as the disc's changes nothing, so an unedited table writes the files back unchanged
//! and the executable keeps every byte. Several mods' tables merge; two giving one value different
//! numbers are refused when the edits are made ([`cfg_text::apply_edits`],
//! [`crate::elf::apply_patches`]).

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::cfg_text::{self, Arg, Edit, Script, To};
use crate::elf::Patch;
use crate::{bail, Result};

/// What a value is and the values the engine's field takes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Int(i64, i64),
    /// spiGetStackFloat in a script, an f32 in the executable.
    Float,
    /// A string of at most this many bytes (in the executable, the bytes it has, NUL included).
    Text(usize),
}

pub const S8: Kind = Kind::Int(-128, 127);
/// A byte a script fills: -1 is kept as 255, as the engine stores it.
pub const B8: Kind = Kind::Int(-128, 255);
pub const U8: Kind = Kind::Int(0, 255);
pub const S16: Kind = Kind::Int(-32768, 32767);
pub const B16: Kind = Kind::Int(-32768, 65535);
pub const S32: Kind = Kind::Int(i32::MIN as i64, i32::MAX as i64);

/// Where a field's values are.
#[derive(Clone, Debug)]
pub enum Place {
    /// `len` arguments from `arg` of a statement, in every copy: (file, statement) each, the
    /// first read.
    Args { at: Vec<(usize, usize)>, arg: usize, len: usize },
    /// A list of entries from argument `arg` to the last `tail` arguments, `entry` arguments to
    /// an entry, from `count.0` to `count.1` entries; in every copy.
    List { at: Vec<(usize, usize)>, arg: usize, tail: usize, entry: Vec<(&'static str, Kind)>, count: (usize, usize) },
    /// A list of entries over every argument of these statements in turn, in each copy (the first
    /// read). A changed list fills them in order: with `per_line` 0 each but the last as many
    /// entries as it had and the last the rest; otherwise `per_line` entries a statement (the
    /// engine reads no more), statements left over emptied. No statement is added.
    /// With `grow`, a list past the room of the lines adds lines of the last one's tag after it,
    /// `per_line` entries each.
    Lines {
        copies: Vec<Vec<(usize, usize)>>,
        entry: Vec<(&'static str, Kind)>,
        count: (usize, usize),
        per_line: usize,
        grow: bool,
    },
    /// `len` values of `size` bytes from `offset` in the executable's file.
    Bytes { offset: usize, size: usize, len: usize },
}

#[derive(Clone, Debug)]
pub struct Field {
    pub name: String,
    pub kind: Kind,
    pub place: Place,
    /// Why the field may not change (a number that ties records together), or None.
    pub fixed: Option<&'static str>,
}

#[derive(Clone, Debug, Default)]
pub struct Record {
    pub keys: Vec<(String, Value)>,
    pub labels: Vec<(String, Value)>,
    pub fields: Vec<Field>,
    /// The statements the record is, in each copy: (file, first, last). A record a mod adds is
    /// a copy of these from the record it is `like`.
    pub block: Vec<(usize, usize, usize)>,
}

/// How a section takes new records ([`Section::adding`]): each a copy of the statements of the
/// record it is `like`, put after the section's last record in each of that record's files.
#[derive(Clone, Debug, Default)]
pub struct Adding {
    /// Arguments that count the records, raised by those added to their file: (file, statement,
    /// argument).
    pub counts: Vec<(usize, usize, usize)>,
    /// The most records a file (a set of copies) holds, and what holds them.
    pub most: usize,
    pub why: &'static str,
    /// The key that is a record's place among its file's records (new ones continue from the
    /// last); the other keys are written into the copy at (key, tag, argument), or, when not
    /// there either, are the like record's own (a town).
    pub place: Option<&'static str>,
    pub key_args: Vec<(&'static str, &'static str, usize, Kind)>,
    /// Labels a new record gives, written at (label, tag, argument) as the text of each copy's
    /// language (at most the last number of bytes once converted): one string, or {"en": ...,
    /// "fr": ...}.
    pub label_args: Vec<(&'static str, &'static str, usize, usize)>,
    /// The language of each file that has one (file, LanguageCode), for the labels' text.
    pub languages: Vec<(usize, usize)>,
}

impl Record {
    pub fn new(keys: &[(&str, Value)]) -> Record {
        Record {
            keys: keys.iter().map(|(name, value)| (name.to_string(), value.clone())).collect(),
            ..Record::default()
        }
    }

    pub fn label(&mut self, name: &str, value: Value) -> &mut Record {
        self.labels.push((name.to_string(), value));
        self
    }

    pub fn field(&mut self, name: &str, kind: Kind, place: Place) -> &mut Record {
        self.fields.push(Field { name: name.to_string(), kind, place, fixed: None });
        self
    }

    /// One argument of a statement (and its copies).
    pub fn arg(&mut self, name: &str, kind: Kind, at: &[(usize, usize)], arg: usize) -> &mut Record {
        self.field(name, kind, Place::Args { at: at.to_vec(), arg, len: 1 })
    }

    /// `len` values of `kind` in a row in the executable.
    pub fn bytes(&mut self, name: &str, kind: Kind, offset: usize, len: usize) -> &mut Record {
        let size = match kind {
            Kind::Int(low, high) if low >= -128 && high <= 255 => 1,
            Kind::Int(low, high) if low >= -32768 && high <= 65535 => 2,
            Kind::Text(size) => size,
            _ => 4,
        };
        self.field(name, kind, Place::Bytes { offset, size, len })
    }

    /// The last field added may not change.
    pub fn fixed(&mut self, why: &'static str) -> &mut Record {
        if let Some(field) = self.fields.last_mut() {
            field.fixed = Some(why);
        }
        self
    }

    /// The record's name in messages: `recipe 3`, `parts town g01 part 1 (Straw House)`.
    fn label_text(&self, section: &str) -> String {
        let mut words = Vec::new();
        if !self.keys.first().is_some_and(|(name, _)| section.starts_with(name.as_str())) {
            words.push(section.to_string());
        }
        for (name, value) in &self.keys {
            words.push(format!("{name} {}", shown(value)));
        }
        let mut out = words.join(" ");
        if let Some((_, Value::String(name))) = self.labels.iter().find(|(label, _)| label == "name") {
            out.push_str(&format!(" ({name})"));
        }
        out
    }
}

fn shown(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

#[derive(Clone, Debug)]
pub struct Section {
    pub name: String,
    /// The names of the keys every record of the section has.
    pub keys: Vec<String>,
    pub records: Vec<Record>,
    /// How the section takes new records, when it does.
    pub adding: Option<Adding>,
}

/// The table: the scripts and executable it reads, and its sections.
#[derive(Clone, Debug)]
pub struct Table {
    /// What the table is, for messages (`an invention table`).
    pub what: String,
    pub scripts: Vec<Script>,
    pub elf: Vec<u8>,
    pub sections: Vec<Section>,
    /// Files the engine reads into a buffer of a size: (file, most bytes, the buffer). A file an
    /// edit grows past it is refused.
    pub limits: Vec<(usize, usize, &'static str)>,
    /// Where the engine copies every string argument of a file, each in 16-byte units, into a
    /// heap of this many units that refuses the allocation reaching its end: (units, the heap).
    pub string_units: Option<(usize, &'static str)>,
}

/// Whether two values are the same to the engine (an int and the float of it are).
fn same(old: &Value, new: &Value) -> bool {
    match (old, new) {
        (Value::Number(a), Value::Number(b)) => match (a.as_i64(), b.as_i64()) {
            (Some(a), Some(b)) => a == b,
            _ => a.as_f64().map(|a| a as f32) == b.as_f64().map(|b| b as f32),
        },
        (Value::Array(a), Value::Array(b)) => a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(a, b)),
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len() && a.iter().all(|(key, a)| b.get(key).is_some_and(|b| same(a, b)))
        }
        _ => old == new,
    }
}

/// The JSON number of a float argument: as float_value, a whole one with its ".0" (a float
/// in the script, as it is written).
fn float_arg_value(value: f32) -> Value {
    let mut text = format!("{value}");
    if value.is_finite() && !text.contains('.') && !text.contains('e') {
        text.push_str(".0");
    }
    serde_json::from_str(&text).unwrap_or(Value::Null)
}

/// The JSON number of an f32: the shortest text that reads back as the same f32.
pub fn float_value(value: f32) -> Value {
    serde_json::from_str(&format!("{value}")).unwrap_or(Value::Null)
}

fn arg_value(arg: Arg, kind: Kind) -> Value {
    match kind {
        Kind::Int(..) => Value::from(arg.int()),
        Kind::Float => match arg {
            Arg::Float(value) => float_arg_value(value),
            other => Value::from(other.int()),
        },
        Kind::Text(_) => match arg {
            Arg::Str(text) => Value::from(dc2tk_disc::text::decode_lossy(&text)),
            _ => Value::Null,
        },
    }
}

/// The text an argument is written as.
pub fn value_text(kind: Kind, value: &Value, what: &str) -> Result<Vec<u8>> {
    match kind {
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

/// The bytes of a value in the executable.
fn value_bytes(kind: Kind, size: usize, value: &Value, what: &str) -> Result<Vec<u8>> {
    match kind {
        Kind::Float => {
            let Some(number) = value.as_f64().filter(|number| number.is_finite()) else {
                bail!("{what} is a number");
            };
            Ok((number as f32).to_le_bytes().to_vec())
        }
        Kind::Text(_) => {
            let Some(text) = value.as_str() else {
                bail!("{what} is a string");
            };
            let Some(mut bytes) = dc2tk_disc::text::encode(text) else {
                bail!("{what}: {text:?} has characters the game's text cannot hold");
            };
            if bytes.len() >= size || bytes.contains(&0) {
                bail!("{what}: {text:?} is {} bytes; the game keeps {}", bytes.len(), size - 1);
            }
            bytes.resize(size, 0);
            Ok(bytes)
        }
        Kind::Int(low, high) => {
            let Some(number) = value.as_i64() else {
                bail!("{what} is a whole number");
            };
            if number < low || number > high {
                bail!("{what} is {number}; the game keeps {low} to {high}");
            }
            Ok(number.to_le_bytes()[..size].to_vec())
        }
    }
}

fn bytes_value(kind: Kind, bytes: &[u8]) -> Value {
    match kind {
        Kind::Float => float_value(f32::from_le_bytes(bytes.try_into().unwrap_or_default())),
        Kind::Text(_) => {
            let end = bytes.iter().position(|&byte| byte == 0).unwrap_or(bytes.len());
            Value::from(dc2tk_disc::text::decode_lossy(&bytes[..end]))
        }
        Kind::Int(low, _) => {
            let mut wide = [0u8; 8];
            wide[..bytes.len()].copy_from_slice(bytes);
            let mut number = i64::from_le_bytes(wide);
            let bits = bytes.len() * 8;
            if low < 0 && bits < 64 && number >= 1 << (bits - 1) {
                number -= 1 << bits;
            }
            Value::from(number)
        }
    }
}

/// An entry of a list as JSON: a number, or an object of the entry's names.
fn entry_value(entry: &[(&str, Kind)], values: Vec<Value>) -> Value {
    if entry.len() == 1 {
        return values.into_iter().next().unwrap_or(Value::Null);
    }
    Value::Object(entry.iter().zip(values).map(|((name, _), value)| (name.to_string(), value)).collect())
}

/// The texts of a list's entries from a mod's JSON.
fn entry_texts(
    entry: &[(&'static str, Kind)],
    (least, most): (usize, usize),
    value: &Value,
    what: &str,
) -> Result<Vec<Vec<u8>>> {
    let names: Vec<&str> = entry.iter().map(|(name, _)| *name).collect();
    let Some(list) = value.as_array() else {
        if entry.len() == 1 {
            bail!("{what} is a list of numbers");
        }
        bail!("{what} is a list of {{{}}}", names.join(", "));
    };
    if list.len() < least || list.len() > most {
        bail!("{what}: {} entries; the game keeps {least} to {most}", list.len());
    }
    let mut out = Vec::new();
    for item in list {
        if entry.len() == 1 {
            out.push(value_text(entry[0].1, item, what)?);
            continue;
        }
        let Some(object) = item.as_object().filter(|object| object.keys().all(|key| names.contains(&key.as_str())))
        else {
            bail!("{what}: each entry is {{{}}}", names.join(", "));
        };
        for (name, kind) in entry {
            let Some(value) = object.get(*name) else {
                bail!("{what}: an entry has no \"{name}\"");
            };
            out.push(value_text(*kind, value, &format!("{what} {name}"))?);
        }
    }
    Ok(out)
}

impl Table {
    pub fn new(what: &str, scripts: Vec<Script>, elf: Vec<u8>) -> Table {
        Table { what: what.to_string(), scripts, elf, sections: Vec::new(), limits: Vec::new(), string_units: None }
    }

    /// Adds a section of records with these keys.
    pub fn section(&mut self, name: &str, keys: &[&str], records: Vec<Record>) {
        self.sections.push(Section {
            name: name.to_string(),
            keys: keys.iter().map(|key| key.to_string()).collect(),
            records,
            adding: None,
        });
    }

    /// Lets the last section added take new records.
    pub fn adding(&mut self, adding: Adding) {
        if let Some(section) = self.sections.last_mut() {
            section.adding = Some(adding);
        }
    }

    fn args(&self, file: usize, statement: usize) -> &[cfg_text::Piece] {
        self.scripts[file].statements[statement].args()
    }

    fn entries(&self, file: usize, statement: usize, from: usize, to: usize, entry: &[(&str, Kind)]) -> Vec<Value> {
        let args = self.args(file, statement);
        let values: Vec<Vec<Value>> = args[from.min(to)..to]
            .chunks(entry.len())
            .filter(|chunk| chunk.len() == entry.len())
            .map(|chunk| chunk.iter().zip(entry).map(|(piece, (_, kind))| arg_value(piece.value(), *kind)).collect())
            .collect();
        values.into_iter().map(|values| entry_value(entry, values)).collect()
    }

    /// A field's value as the engine reads it (from the first copy).
    pub fn value(&self, field: &Field) -> Value {
        match &field.place {
            Place::Args { at, arg, len } => {
                let args = self.args(at[0].0, at[0].1);
                let one = |at: usize| args.get(at).map_or(Value::Null, |piece| arg_value(piece.value(), field.kind));
                if *len == 1 {
                    one(*arg)
                } else {
                    Value::Array((*arg..arg + len).map(one).collect())
                }
            }
            Place::List { at, arg, tail, entry, .. } => {
                let count = self.args(at[0].0, at[0].1).len();
                Value::Array(self.entries(at[0].0, at[0].1, *arg, count.saturating_sub(*tail), entry))
            }
            Place::Lines { copies, entry, .. } => Value::Array(
                copies[0]
                    .iter()
                    .flat_map(|&(file, statement)| {
                        let count = self.args(file, statement).len();
                        self.entries(file, statement, 0, count, entry)
                    })
                    .collect(),
            ),
            Place::Bytes { offset, size, len } => {
                let one = |number: usize| {
                    let at = offset + number * size;
                    bytes_value(field.kind, &self.elf[at..at + size])
                };
                if *len == 1 {
                    one(0)
                } else {
                    Value::Array((0..*len).map(one).collect())
                }
            }
        }
    }

    pub fn to_json(&self) -> Value {
        let mut top = Map::new();
        for section in &self.sections {
            let mut records = Vec::new();
            for record in &section.records {
                let mut object = Map::new();
                for (name, value) in record.keys.iter().chain(&record.labels) {
                    object.insert(name.clone(), value.clone());
                }
                for field in &record.fields {
                    insert(&mut object, &field.name, self.value(field));
                }
                records.push(Value::Object(object));
            }
            top.insert(section.name.clone(), Value::Array(records));
        }
        Value::Object(top)
    }

    /// The edits and patches a mod's table makes: only values that differ from these (see
    /// [`Table::changes_all`]).
    pub fn changes(&self, table: &Value, module: &str) -> Result<(Vec<Edit>, Vec<Patch>)> {
        self.changes_all(&[(module.to_string(), table.clone())])
    }

    /// The edits and patches every mod's table makes, (mod, table) in order: only values that
    /// differ from these. A record with keys the section lacks is a new record, where the section
    /// takes them ([`Adding`]): a copy of the record its `like` names (that record's keys: a
    /// number, or an object of them), with the fields given. Records several mods add are
    /// counted together; one record added two ways is refused.
    pub fn changes_all(&self, tables: &[(String, Value)]) -> Result<(Vec<Edit>, Vec<Patch>)> {
        let mut edits = Vec::new();
        let mut patches = Vec::new();
        let mut added: Vec<Vec<Added>> = vec![Vec::new(); self.sections.len()];
        for (module, table) in tables {
            self.changes_of(table, module, &mut edits, &mut patches, &mut added)
                .map_err(|error| crate::Error::invalid(format!("mod {module}: {error}")))?;
        }
        for (number, records) in added.iter().enumerate() {
            if !records.is_empty() {
                self.additions(&self.sections[number], records, &mut edits)?;
            }
        }
        Ok((edits, patches))
    }

    fn changes_of<'a>(
        &self,
        table: &'a Value,
        module: &'a str,
        edits: &mut Vec<Edit>,
        patches: &mut Vec<Patch>,
        added: &mut [Vec<Added<'a>>],
    ) -> Result<()> {
        let names: Vec<&str> = self.sections.iter().map(|section| section.name.as_str()).collect();
        let Some(object) = table.as_object() else {
            bail!(
                "{} is {{{}}}, as mod asset writes it",
                self.what,
                names.iter().map(|name| format!("\"{name}\": [...]")).collect::<Vec<_>>().join(", ")
            );
        };
        for (key, list) in object {
            let Some(number) = self.sections.iter().position(|section| &section.name == key) else {
                bail!("{} has {}, not \"{key}\"", self.what, names.join(", "));
            };
            let section = &self.sections[number];
            let Some(list) = list.as_array() else {
                bail!("\"{key}\" is a list of records");
            };
            for given in list {
                let Some(given) = given.as_object() else {
                    bail!("each of \"{key}\" is an object");
                };
                let mut wanted = Vec::new();
                for name in &section.keys {
                    let Some(value) = given.get(name) else {
                        bail!("each of \"{key}\" names its {}", section.keys.join(" and "));
                    };
                    wanted.push(value);
                }
                let Some(record) = section
                    .records
                    .iter()
                    .rev()
                    .find(|record| record.keys.iter().zip(&wanted).all(|((_, have), want)| same(have, want)))
                else {
                    let keys = keys_text(&section.keys, &wanted.iter().copied().cloned().collect::<Vec<_>>());
                    if section.adding.is_none() {
                        bail!("{key}: no record with {keys} (the {key} the game has are edited, not added to)");
                    }
                    if !given.contains_key("like") {
                        bail!(
                            "{key}: no record with {keys}; a new one names the record it is like (\"like\": its {})",
                            section.keys.join(" and ")
                        );
                    }
                    let values: Vec<Value> = wanted.into_iter().cloned().collect();
                    match added[number].iter().find(|known| known.keys == values) {
                        Some(known) if known.given != given => {
                            bail!(
                                "{key}: new record {keys} is given two ways, by mod {} and mod {module}",
                                known.module
                            )
                        }
                        Some(_) => {}
                        None => added[number].push(Added { keys: values, module, given }),
                    }
                    continue;
                };
                let label = record.label_text(key);
                if given.contains_key("like") {
                    bail!("{label} is the game's own; \"like\" makes a new record from it");
                }
                let mut flat = Vec::new();
                for (name, value) in given {
                    if section.keys.contains(name) {
                        continue;
                    }
                    if let Some((_, have)) = record.labels.iter().find(|(label, _)| label == name) {
                        if !same(have, value) {
                            bail!("{label}: \"{name}\" is a label (text, not data) and does not change");
                        }
                        continue;
                    }
                    flatten(record, name, value, &mut flat, &label)?;
                }
                for (name, value) in flat {
                    let field = record.fields.iter().find(|field| field.name == name).unwrap();
                    self.field_changes(field, value, &format!("{label} {name}"), module, edits, patches)?;
                }
            }
        }
        Ok(())
    }

    /// The record a new one's `like` names: an object of the keys, or the last key's value (the
    /// other keys the new record's own: another part of its town).
    fn like_of<'a>(&self, section: &'a Section, like: &Value, keys: &[Value], label: &str) -> Result<&'a Record> {
        let wanted: Vec<Value> = match like {
            Value::Object(object) => {
                section.keys.iter().map(|key| object.get(key).cloned().unwrap_or(Value::Null)).collect()
            }
            value => {
                let mut wanted = keys.to_vec();
                *wanted.last_mut().expect("a section has keys") = value.clone();
                wanted
            }
        };
        section
            .records
            .iter()
            .rev()
            .find(|record| record.keys.iter().zip(&wanted).all(|((_, have), want)| same(have, want)))
            .filter(|record| !record.block.is_empty())
            .ok_or_else(|| crate::Error::invalid(format!("{label}: like {like}: no such record to copy")))
    }

    /// The new records of a section: each a copy of its like record's statements with its keys,
    /// labels and the fields given, after the section's last record in each file; the files'
    /// counts raised.
    fn additions(&self, section: &Section, records: &[Added], edits: &mut Vec<Edit>) -> Result<()> {
        let adding = section.adding.as_ref().expect("a section that takes records");
        let name = &section.name;
        let first_file = |record: &Record| record.block.first().map_or(usize::MAX, |&(file, ..)| file);
        let place_at = adding.place.and_then(|place| section.keys.iter().position(|key| key == place));
        let mut new: Vec<(&Added, &Record, String)> = Vec::new();
        for added in records {
            let label = format!("{name} {} (new)", keys_text(&section.keys, &added.keys));
            let like = self.like_of(section, &added.given["like"], &added.keys, &label)?;
            new.push((added, like, label));
        }
        // places continue from the last, in order
        new.sort_by_key(|(added, ..)| place_at.map_or(0, |at| added.keys[at].as_i64().unwrap_or(0)));
        // a record's group: its first file and the keys a new record takes from the one it is like
        // (its town), which its place counts in and new ones go after
        let group = |record: &Record| -> String {
            let kept: Vec<String> = record
                .keys
                .iter()
                .filter(|(key, _)| {
                    Some(key.as_str()) != adding.place && !adding.key_args.iter().any(|(known, ..)| known == key)
                })
                .map(|(_, value)| shown(value))
                .collect();
            format!("{} {}", first_file(record), kept.join(" "))
        };
        let mut added_to: BTreeMap<String, usize> = BTreeMap::new();
        for &(added, like, ref label) in &new {
            let had = section.records.iter().filter(|record| group(record) == group(like)).count();
            let count = added_to.entry(group(like)).or_insert(0);
            *count += 1;
            if had + *count > adding.most {
                bail!("{label}: {} {name} in its file; the game holds {} ({})", had + *count, adding.most, adding.why);
            }
            // keys: the place, written ones, or the like record's own
            let mut key_texts: Vec<(&str, usize, Vec<u8>)> = Vec::new();
            for ((key, value), (_, like_value)) in section.keys.iter().zip(&added.keys).zip(&like.keys) {
                if Some(key.as_str()) == adding.place {
                    let wanted = (had + *count - 1) as i64;
                    if value.as_i64() != Some(wanted) {
                        bail!("{label}: a new record's {key} is the next one, {wanted}");
                    }
                    // a place the statements also hold (a request's number)
                    if let Some(&(_, tag, arg, kind)) = adding.key_args.iter().find(|(known, ..)| known == key) {
                        key_texts.push((tag, arg, value_text(kind, value, &format!("{label} {key}"))?));
                    }
                } else if let Some(&(_, tag, arg, kind)) = adding.key_args.iter().find(|(known, ..)| known == key) {
                    key_texts.push((tag, arg, value_text(kind, value, &format!("{label} {key}"))?));
                } else if !same(value, like_value) {
                    bail!("{label}: a new record has the {key} of the record it is like ({})", shown(like_value));
                }
            }
            // the copy: the like record's statements in each of its files
            let mut scripts = Vec::new();
            for &(file, first, last) in &like.block {
                let mut statements = self.scripts[file].statements[first..=last].to_vec();
                statements[0].before = b"\r\n".to_vec();
                let language = adding.languages.iter().find(|(known, _)| *known == file).map(|&(_, language)| language);
                for statement in statements.iter_mut() {
                    for (tag, arg, text) in &key_texts {
                        if statement.tag == *tag {
                            set_piece(statement, *arg, text.clone(), label)?;
                        }
                    }
                    for &(label_name, tag, arg, most) in &adding.label_args {
                        if statement.tag != tag {
                            continue;
                        }
                        let Some(value) = added.given.get(label_name) else {
                            continue;
                        };
                        let text = label_in(value, language, label_name, label)?;
                        let Some(bytes) = crate::lang::name_bytes(&text, language.unwrap_or(1)) else {
                            bail!("{label}: {label_name} {text:?} has characters the game's text cannot hold");
                        };
                        if crate::lang::converted_len(&bytes, language.unwrap_or(1)) > most {
                            bail!("{label}: {label_name} {text:?} is longer than the {most} bytes the game keeps");
                        }
                        set_piece(statement, arg, cfg_text::str_text(&bytes), label)?;
                    }
                }
                scripts.push(Script { statements, tail: Vec::new() });
            }
            // the like record's fields, moved onto the copy
            let moved = |&(file, statement): &(usize, usize)| -> Option<(usize, usize)> {
                like.block
                    .iter()
                    .position(|&(known, first, last)| known == file && (first..=last).contains(&statement))
                    .map(|copy| (copy, statement - like.block[copy].1))
            };
            let copy = Table::new(&self.what, scripts, Vec::new());
            let mut fields = Vec::new();
            for field in &like.fields {
                let place = match &field.place {
                    Place::Args { at, arg, len } => {
                        Place::Args { at: at.iter().filter_map(moved).collect(), arg: *arg, len: *len }
                    }
                    Place::List { at, arg, tail, entry, count } => Place::List {
                        at: at.iter().filter_map(moved).collect(),
                        arg: *arg,
                        tail: *tail,
                        entry: entry.clone(),
                        count: *count,
                    },
                    Place::Lines { copies, entry, count, per_line, grow } => Place::Lines {
                        copies: copies
                            .iter()
                            .map(|lines| lines.iter().filter_map(moved).collect::<Vec<_>>())
                            .filter(|lines| !lines.is_empty())
                            .collect(),
                        entry: entry.clone(),
                        count: *count,
                        per_line: *per_line,
                        grow: *grow,
                    },
                    Place::Bytes { .. } => continue,
                };
                fields.push(Field { name: field.name.clone(), kind: field.kind, place, fixed: None });
            }
            let moved_record = Record { fields, ..Record::default() };
            let mut flat = Vec::new();
            for (key, value) in added.given.iter() {
                if section.keys.contains(key)
                    || key == "like"
                    || adding.label_args.iter().any(|(known, ..)| known == key)
                {
                    continue;
                }
                if like.labels.iter().any(|(known, _)| known == key) {
                    bail!("{label}: \"{key}\" is text the copy keeps from the record it is like");
                }
                flatten(&moved_record, key, value, &mut flat, label)?;
            }
            let mut copy_edits = Vec::new();
            let mut unused = Vec::new();
            for (field_name, value) in flat {
                let field = moved_record.fields.iter().find(|field| field.name == field_name).unwrap();
                let outside = match &field.place {
                    Place::Args { at, .. } | Place::List { at, .. } => at.is_empty(),
                    Place::Lines { copies, .. } => copies.is_empty(),
                    Place::Bytes { .. } => true,
                };
                if outside {
                    bail!("{label} {field_name}: the record it is like keeps it outside its statements");
                }
                let what = format!("{label} {field_name}");
                copy.field_changes(field, value, &what, added.module, &mut copy_edits, &mut unused)?;
            }
            let texts = cfg_text::apply_edits(&copy.scripts, &copy_edits)?;
            for (&(file, ..), text) in like.block.iter().zip(texts) {
                let after = section
                    .records
                    .iter()
                    .filter(|record| group(record) == group(like))
                    .flat_map(|record| &record.block)
                    .filter(|(known, ..)| *known == file)
                    .map(|&(_, _, last)| last)
                    .max()
                    .expect("the like record's own file");
                edits.push(Edit {
                    file,
                    statement: after,
                    arg: 0,
                    to: To::Insert(text),
                    what: label.clone(),
                    module: added.module.to_string(),
                });
            }
        }
        // the counts of the files records were added to
        for &(file, statement, arg) in &adding.counts {
            let more = new.iter().filter(|(_, like, _)| like.block.iter().any(|&(known, ..)| known == file)).count();
            if more == 0 {
                continue;
            }
            let Some(piece) = self.args(file, statement).get(arg) else {
                bail!("{name}: the count statement has no argument {arg}");
            };
            edits.push(Edit {
                file,
                statement,
                arg,
                to: To::Arg(cfg_text::int_text(piece.value().int() as i64 + more as i64)),
                what: format!("{name} count (file {file})"),
                module: "build".to_string(),
            });
        }
        Ok(())
    }

    fn field_changes(
        &self,
        field: &Field,
        value: &Value,
        what: &str,
        module: &str,
        edits: &mut Vec<Edit>,
        patches: &mut Vec<Patch>,
    ) -> Result<()> {
        let old = self.value(field);
        if same(&old, value) {
            return Ok(());
        }
        if let Some(why) = field.fixed {
            bail!("{what}: {why}, and does not change");
        }
        let edit = |file: usize, statement: usize, arg: usize, to: To, what: String| Edit {
            file,
            statement,
            arg,
            to,
            what,
            module: module.to_string(),
        };
        match &field.place {
            Place::Args { at, arg, len } => {
                let values: Vec<&Value> = if *len == 1 {
                    vec![value]
                } else {
                    match value.as_array() {
                        Some(list) if list.len() == *len => list.iter().collect(),
                        _ => bail!("{what} is a list of {len}"),
                    }
                };
                for (number, value) in values.into_iter().enumerate() {
                    let what = if *len == 1 { what.to_string() } else { format!("{what}[{number}]") };
                    let text = value_text(field.kind, value, &what)?;
                    for &(file, statement) in at {
                        let Some(piece) = self.args(file, statement).get(arg + number) else {
                            bail!("{what}: the statement has no such argument");
                        };
                        if same(&arg_value(piece.value(), field.kind), value) {
                            continue;
                        }
                        edits.push(edit(file, statement, arg + number, To::Arg(text.clone()), what.clone()));
                    }
                }
            }
            Place::List { at, arg, tail, entry, count } => {
                let texts = entry_texts(entry, *count, value, what)?;
                for &(file, statement) in at {
                    let count = self.args(file, statement).len();
                    if same(
                        &Value::Array(self.entries(file, statement, *arg, count.saturating_sub(*tail), entry)),
                        value,
                    ) {
                        continue;
                    }
                    edits.push(edit(file, statement, *arg, To::Span(texts.clone(), *tail), what.to_string()));
                }
            }
            Place::Lines { copies, entry, count, per_line, grow } => {
                let texts = entry_texts(entry, *count, value, what)?;
                for at in copies {
                    let room = if *per_line == 0 || *grow { usize::MAX } else { at.len() * per_line };
                    if at.is_empty() || texts.len() / entry.len() > room {
                        bail!(
                            "{what}: {} entries; the file has {} lines of {per_line} for them (lines are not added)",
                            texts.len() / entry.len(),
                            at.len()
                        );
                    }
                    let mut rest = &texts[..];
                    for (number, &(file, statement)) in at.iter().enumerate() {
                        let had = self.args(file, statement).len() / entry.len() * entry.len();
                        let take = match *per_line {
                            0 if number + 1 == at.len() => rest.len(),
                            0 => had.min(rest.len()),
                            per_line => (per_line * entry.len()).min(rest.len()),
                        };
                        let (mine, after) = rest.split_at(take);
                        rest = after;
                        let old: Vec<&[u8]> =
                            self.args(file, statement).iter().map(|piece| piece.text.as_slice()).collect();
                        if old == mine.iter().map(Vec::as_slice).collect::<Vec<_>>() {
                            continue;
                        }
                        edits.push(edit(file, statement, 0, To::Rest(mine.to_vec()), what.to_string()));
                    }
                    // what the lines had no room for: new lines of the last one's tag after it
                    let &(file, last) = at.last().expect("lines");
                    let tag = self.scripts[file].statements[last].tag.clone();
                    for (number, chunk) in rest.chunks((*per_line).max(1) * entry.len()).enumerate() {
                        let text = cfg_text::statement_text(&tag, chunk);
                        edits.push(edit(file, last, 0, To::Insert(text), format!("{what} (new line {})", number + 1)));
                    }
                }
            }
            Place::Bytes { offset, size, len } => {
                let values: Vec<&Value> = if *len == 1 {
                    vec![value]
                } else {
                    match value.as_array() {
                        Some(list) if list.len() == *len => list.iter().collect(),
                        _ => bail!("{what} is a list of {len}"),
                    }
                };
                for (number, value) in values.into_iter().enumerate() {
                    let what = if *len == 1 { what.to_string() } else { format!("{what}[{number}]") };
                    let bytes = value_bytes(field.kind, *size, value, &what)?;
                    let at = offset + number * size;
                    let have = &self.elf[at..at + size];
                    if have == bytes.as_slice() || same(&bytes_value(field.kind, have), value) {
                        continue;
                    }
                    patches.push(Patch { offset: at, bytes, what, module: module.to_string() });
                }
            }
        }
        Ok(())
    }

    /// The scripts with the edits made.
    pub fn apply(&self, edits: &[Edit]) -> Result<Vec<Vec<u8>>> {
        let written = cfg_text::apply_edits(&self.scripts, edits)?;
        for &(file, most, why) in &self.limits {
            if written[file].len() > most && written[file] != self.scripts[file].write() {
                bail!(
                    "{} file {file} would be {} bytes; the game reads it into {most} ({why})",
                    self.what,
                    written[file].len()
                );
            }
        }
        if let Some((most, why)) = self.string_units {
            for (file, data) in written.iter().enumerate() {
                if *data == self.scripts[file].write() {
                    continue;
                }
                let units = string_units(&Script::parse(data)?);
                if units >= most {
                    bail!("{} file {file}: its strings take {units} of the {most} 16-byte units of {why}", self.what);
                }
            }
        }
        Ok(written)
    }

    pub fn write(&self) -> Vec<Vec<u8>> {
        self.scripts.iter().map(Script::write).collect()
    }

    /// The fields whose values do not write back as the text or bytes they were read from (in the
    /// first copy: copies may differ, and the first is the one shown; what
    /// the corpus tests check is empty), and whether the JSON changes anything.
    pub fn check(&self) -> Vec<String> {
        let mut wrong = Vec::new();
        for section in &self.sections {
            for record in &section.records {
                let label = record.label_text(&section.name);
                for field in &record.fields {
                    let what = format!("{label} {}", field.name);
                    let value = self.value(field);
                    match &field.place {
                        Place::Args { at, arg, len } => {
                            let values =
                                if *len == 1 { vec![value] } else { value.as_array().cloned().unwrap_or_default() };
                            for (number, value) in values.iter().enumerate() {
                                for &(file, statement) in &at[..1] {
                                    let text = value_text(field.kind, value, &what);
                                    let have = self.args(file, statement).get(arg + number).map(|piece| &piece.text);
                                    if text.as_ref().ok() != have {
                                        wrong.push(format!("{what}: {value} in file {file}"));
                                    }
                                }
                            }
                        }
                        Place::List { entry, count, .. } | Place::Lines { entry, count, .. } => {
                            if entry_texts(entry, *count, &value, &what).is_err() {
                                wrong.push(format!("{what}: {value}"));
                            }
                        }
                        Place::Bytes { offset, size, len } => {
                            let values =
                                if *len == 1 { vec![value] } else { value.as_array().cloned().unwrap_or_default() };
                            for (number, value) in values.iter().enumerate() {
                                let at = offset + number * size;
                                let bytes = value_bytes(field.kind, *size, value, &what);
                                let same_bytes = match (&bytes, field.kind) {
                                    (Ok(bytes), Kind::Text(_)) => bytes_value(field.kind, bytes) == *value,
                                    (Ok(bytes), _) => bytes.as_slice() == &self.elf[at..at + size],
                                    _ => false,
                                };
                                if !same_bytes {
                                    wrong.push(format!("{what}: {value}"));
                                }
                            }
                        }
                    }
                }
            }
        }
        let json: Value = serde_json::from_str(&crate::json::json_text(&self.to_json())).unwrap();
        match self.changes(&json, "check") {
            Ok((edits, patches)) if edits.is_empty() && patches.is_empty() => {}
            Ok((edits, patches)) => wrong.push(format!(
                "the exported JSON changes {} arguments and {} values of the executable: {:?}",
                edits.len(),
                patches.len(),
                edits
                    .iter()
                    .map(|edit| &edit.what)
                    .chain(patches.iter().map(|patch| &patch.what))
                    .take(5)
                    .collect::<Vec<_>>()
            )),
            Err(error) => wrong.push(format!("the exported JSON is refused: {error}")),
        }
        wrong
    }
}

/// A record a mod adds: its keys, the mod and what it gives.
#[derive(Clone, Debug)]
struct Added<'a> {
    keys: Vec<Value>,
    module: &'a str,
    given: &'a Map<String, Value>,
}

fn keys_text(names: &[String], values: &[Value]) -> String {
    names.iter().zip(values).map(|(name, value)| format!("{name} {}", shown(value))).collect::<Vec<_>>().join(", ")
}

/// A label's text in a copy's language: one string, or {"en": ..., "fr": ...} (English for a
/// language not given).
fn label_in(value: &Value, language: Option<usize>, name: &str, what: &str) -> Result<String> {
    match (value, language) {
        (Value::String(text), _) => Ok(text.clone()),
        (Value::Object(by_language), language) => {
            let short = crate::lang::LANGUAGE_NAMES[language.unwrap_or(1)];
            match by_language.get(short).or_else(|| by_language.get("en")) {
                Some(Value::String(text)) => Ok(text.clone()),
                _ => bail!("{what}: \"{name}\" has no text for {short} (nor en)"),
            }
        }
        _ => bail!("{what}: \"{name}\" is a string or {{\"en\": ..., \"fr\": ...}}"),
    }
}

/// The 16-byte units a script's string arguments take, each copied whole with its NUL.
pub fn string_units(script: &Script) -> usize {
    script
        .statements
        .iter()
        .flat_map(|statement| statement.values())
        .filter_map(|value| match value {
            Arg::Str(text) => Some((text.len() + 16) / 16),
            _ => None,
        })
        .sum()
}

/// Gives a statement's argument new text.
fn set_piece(statement: &mut cfg_text::Statement, arg: usize, text: Vec<u8>, what: &str) -> Result<()> {
    let Some(piece) = statement.pieces.get(arg) else {
        bail!("{what}: the copied {} has no argument {arg}", statement.tag);
    };
    statement.pieces[arg] = piece.with_text(text);
    Ok(())
}

/// Puts a value at a dotted name (`a.b` into `{"a": {"b": ...}}`).
fn insert(object: &mut Map<String, Value>, name: &str, value: Value) {
    match name.split_once('.') {
        Some((group, rest)) => {
            let nested = object.entry(group).or_insert_with(|| Value::Object(Map::new()));
            if let Value::Object(nested) = nested {
                insert(nested, rest, value);
            }
        }
        None => {
            object.insert(name.to_string(), value);
        }
    }
}

/// A mod's member as (field name, value) pairs: an object of a group's fields is opened up.
fn flatten<'a>(
    record: &Record,
    name: &str,
    value: &'a Value,
    out: &mut Vec<(String, &'a Value)>,
    label: &str,
) -> Result<()> {
    if record.fields.iter().any(|field| field.name == name) {
        out.push((name.to_string(), value));
        return Ok(());
    }
    let prefix = format!("{name}.");
    if record.fields.iter().any(|field| field.name.starts_with(&prefix)) {
        let Some(nested) = value.as_object() else {
            bail!("{label}: \"{name}\" is an object of fields");
        };
        for (inner, value) in nested {
            flatten(record, &format!("{name}.{inner}"), value, out, label)?;
        }
        return Ok(());
    }
    bail!("{label} {name}: no such field");
}

/// {key: statement indices} of the statements with this tag, by their first argument.
pub fn by_first(script: &Script, tag: &str) -> BTreeMap<i32, Vec<usize>> {
    let mut out: BTreeMap<i32, Vec<usize>> = BTreeMap::new();
    for (at, statement) in script.statements.iter().enumerate().filter(|(_, statement)| statement.tag == tag) {
        out.entry(statement.values().first().map_or(0, Arg::int)).or_default().push(at);
    }
    out
}

/// The statements with this tag, in order.
pub fn tagged(script: &Script, tag: &str) -> Vec<usize> {
    script.statements.iter().enumerate().filter(|(_, statement)| statement.tag == tag).map(|(at, _)| at).collect()
}

/// Parses each file, naming it in the error.
pub fn parse_all(names: &[String], files: &[Vec<u8>]) -> Result<Vec<Script>> {
    if names.len() != files.len() {
        bail!("{} files given for {}", files.len(), names.len());
    }
    names
        .iter()
        .zip(files)
        .map(|(name, data)| Script::parse(data).map_err(|error| crate::Error::invalid(format!("{name}: {error}"))))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elf::apply_patches;
    use serde_json::json;

    fn sample() -> Table {
        let scripts = vec![
            Script::parse(
                b"RF_INFO 1,0, 9300 ,0;\r\nDATASET 309,4,210,5,216,4,0,0.99;\r\nFL 0,1,0,2;\r\nFL 8,800;\r\n",
            )
            .unwrap(),
            Script::parse(b"RF_INFO 1,0,9300,0;\r\n").unwrap(),
        ];
        let mut elf = vec![0u8; 16];
        elf[4..6].copy_from_slice(&(-3i16).to_le_bytes());
        elf[8..12].copy_from_slice(&1.5f32.to_le_bytes());
        let mut table = Table::new("a test table", scripts, elf);
        let mut floor = Record::new(&[("floor", json!(1))]);
        floor.label("name", json!("Sewer"));
        floor.arg("fast_time", S32, &[(0, 0), (1, 0)], 2);
        floor.field(
            "materials",
            Kind::Int(0, 0),
            Place::List { at: vec![(0, 1)], arg: 2, tail: 2, entry: vec![("item", S16), ("count", U8)], count: (1, 4) },
        );
        floor.field("scale", Kind::Float, Place::Args { at: vec![(0, 1)], arg: 7, len: 1 });
        floor.field(
            "monsters",
            Kind::Int(0, 0),
            Place::Lines {
                copies: vec![vec![(0, 2), (0, 3)]],
                entry: vec![("id", S16), ("param", S16)],
                count: (0, 32),
                per_line: 0,
                grow: false,
            },
        );
        floor.bytes("elf.value", S16, 4, 1).bytes("elf.rate", Kind::Float, 8, 1);
        floor.bytes("elf.id", S16, 0, 1).fixed("the id ties it to the rest");
        table.section("floors", &["floor"], vec![floor]);
        table
    }

    #[test]
    fn values_read_and_unedited_tables_change_nothing() {
        let table = sample();
        let json = table.to_json();
        let floor = &json["floors"][0];
        assert_eq!(floor["fast_time"], 9300);
        assert_eq!(floor["materials"], json!([{"item": 210, "count": 5}, {"item": 216, "count": 4}]));
        assert_eq!(floor["scale"], json!(0.99));
        assert_eq!(floor["monsters"][2], json!({"id": 8, "param": 800}));
        assert_eq!(floor["elf"], json!({"value": -3, "rate": 1.5, "id": 0}));
        assert!(table.check().is_empty(), "{:?}", table.check());
        assert_eq!(table.apply(&[]).unwrap(), table.write());
    }

    #[test]
    fn edits_reach_every_copy_and_lists_change_length() {
        let table = sample();
        let edit = json!({"floors": [{"floor": 1, "fast_time": 6000, "materials": [{"item": 1, "count": 2}],
            "monsters": [{"id": 5, "param": 1}, {"id": 6, "param": 2}, {"id": 7, "param": 3}], "elf": {"value": 9}}]});
        let (edits, patches) = table.changes(&edit, "m").unwrap();
        let files = table.apply(&edits).unwrap();
        assert_eq!(
            String::from_utf8_lossy(&files[0]),
            "RF_INFO 1,0, 6000 ,0;\r\nDATASET 309,4,1,2,0,0.99;\r\nFL 5,1,6,2;\r\nFL 7,3;\r\n"
        );
        assert_eq!(files[1], b"RF_INFO 1,0,6000,0;\r\n");
        let patched = apply_patches(&table.elf, &patches).unwrap();
        assert_eq!(&patched[4..6], &9i16.to_le_bytes());
        let refused = |edit: Value| table.changes(&edit, "m").unwrap_err().to_string();
        assert!(refused(json!({"floors": [{"floor": 2}]})).contains("no record"));
        assert!(refused(json!({"floors": [{"floor": 1, "name": "Pipe"}]})).contains("label"));
        assert!(refused(json!({"floors": [{"floor": 1, "speed": 1}]})).contains("no such field"));
        assert!(refused(json!({"floors": [{"floor": 1, "elf": {"id": 4}}]})).contains("does not change"));
        assert!(
            refused(json!({"floors": [{"floor": 1, "materials": [{"item": 1, "count": 300}]}]})).contains("0 to 255")
        );
        assert!(refused(json!({"rooms": []})).contains("not \"rooms\""));
        let (mut both, _) = table.changes(&json!({"floors": [{"floor": 1, "fast_time": 1}]}), "a").unwrap();
        both.extend(table.changes(&json!({"floors": [{"floor": 1, "fast_time": 2}]}), "b").unwrap().0);
        assert!(table.apply(&both).unwrap_err().to_string().contains("mod a and mod b"));
    }
}
