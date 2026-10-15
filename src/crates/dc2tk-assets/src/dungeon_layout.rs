//! Dungeon floor layouts (docs/assets/dungeons.md): the room scripts of `dungeon/cfg_file` that
//! `load_dungeon_map(map, cfg, flags)` hands the floor generator (`data:layouts`).
//!
//! CAutoMapGen::SetupRoomInfo (automap.cpp) runs a script over `GRID_SIZE cell_w, cell_d;`
//! (floats) and one block per room layout: `ROOM_ID id;` (read, never used), `ROOM_SIZE w,h;`,
//! `ROOM_RATE percent;`, `ROOM_FIXED n;`, then h lines `RD part,attr, ...;` of w pairs (a line of
//! another count is dropped by _RD, and the table it fills is then short) and `ROOM_END;`. Up to
//! 64 layouts (AUTOMAP_ROOM_INFO[64]); the generator picks them by their order in the file. A part
//! is an index into the executable's `PartsInfoData` (0x33D440, 277 entries of 0x18: the name of a
//! map template, its cell kind and open sides), -1 an empty cell; the attribute is the cell's
//! (bit 0 hides it from the mini map and from SearchMapFlatPosition). A layout given whole as a
//! floor (`AUTOMAP_GEN_FIXED_FLOOR`, `AUTOMAP_GEN_PRESET_FLOOR`) fills a grid of 900 cells, so w ×
//! h is at most 900.
//!
//! The JSON: `{"layouts": [{"layout": "d01f01", "grid_size": [320, 320], "rooms": [{"room": 0,
//! "id": 0, "rate": 100, "fixed": 0, "cells": ["room08 room00 room09", ...]}]}]}`, a room's rows
//! from the first, each a string of cells: the part's template name, `.` for none, `#N` for a part
//! with no name, and `:attr` after it when the attribute is not 0 (`way01:1`, `.:3`). A mod gives
//! only the layouts, rooms and fields it changes (`cells` whole); a room after the last is added.
//! Each changed room is written again in the files' own layout; the rest keep their bytes. Several
//! mods' layouts merge; two giving one field different values are refused.

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use crate::bytes::{u16_at, u32_at};
use crate::cfg_text::{self, Arg, Script, Statement};
use crate::elf::file_offset;
use crate::{bail, json, Context, Error, Result};

/// The room scripts on the disc, by name (`dungeon/cfg_file/<name>.cfg`). d04f01 and d07f02 are
/// named by no script (d04f01's floors load d01f01's); `_d03f03.cfg` is an older copy, left out.
pub const LAYOUTS: [&str; 23] = [
    "d01f01", "d02f01", "d03f01", "d03f02", "d03f03", "d04f00", "d04f01", "d04f02", "d05f01", "d06f01", "d06f02",
    "d06f03", "d07f01", "d07f02", "mstage0", "mstage1", "mstage2", "mstage3", "mstage4", "mstage5", "mstage6",
    "mstage7", "mstage8",
];
pub const FOLDER: &str = "dungeon/cfg_file";
/// PartsInfoData: AUTOMAP_PARTS_INFO {char *name; s16 kind; u8 link; u8 entrance; s16 tile[8]}.
pub const PARTS_TABLE: u32 = 0x33D440;
pub const PARTS_RECORD: usize = 0x18;
/// The room layouts SetupRoomInfo makes room for.
pub const ROOMS_MOST: usize = 64;
/// The cells of the generator's grid (CAutoMapParts[900], dng_main.cpp).
pub const CELLS_MOST: usize = 900;

pub fn paths() -> Vec<String> {
    LAYOUTS.iter().map(|name| format!("{FOLDER}/{name}.cfg")).collect()
}

/// One entry of PartsInfoData.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    pub name: String,
    /// AUTOMAP_PARTS_KIND flags: 1 corridor, 2 and 4 room, 8 entrance, 0x10 closed door, 0x20
    /// step, 0x100 part, 0x200 corridor end, 0x400 healing point.
    pub kind: i16,
    /// The sides it opens onto (AUTOMAP_LINK: 1 -Z, 2 +Z, 4 +X, 8 -X).
    pub link: u8,
    /// An entrance's side the corridor comes in from.
    pub entrance: u8,
}

/// PartsInfoData from the boot executable, up to its NULL name.
pub fn parts(elf: &[u8]) -> Result<Vec<Part>> {
    let at = |vaddr: u32| {
        file_offset(elf, vaddr).ok_or_else(|| Error::invalid(format!("the executable has no address {vaddr:#x}")))
    };
    let start = at(PARTS_TABLE)?;
    let mut out = Vec::new();
    for number in 0.. {
        let record = elf
            .get(start + number * PARTS_RECORD..start + (number + 1) * PARTS_RECORD)
            .ok_or_else(|| Error::invalid("PartsInfoData runs past the executable"))?;
        let pointer = u32_at(record, 0)?;
        if pointer == 0 {
            break;
        }
        let text = at(pointer)?;
        let end = elf[text..].iter().position(|&byte| byte == 0).map_or(elf.len(), |end| text + end);
        out.push(Part {
            name: String::from_utf8_lossy(&elf[text..end]).into_owned(),
            kind: u16_at(record, 4)? as i16,
            link: record[6],
            entrance: record[7],
        });
        if out.len() > 1024 {
            bail!("PartsInfoData has no end");
        }
    }
    Ok(out)
}

/// The entries of PartsInfoData (the last, 276, named "" and placed by nothing).
pub const PARTS_COUNT: usize = 277;
/// Each name's own string, 8 bytes apart from 0x36C708: 7 letters at most.
pub const PART_NAME_SLOT: usize = 8;
/// MiniMapInfoData, right after PartsInfoData and its 8 bytes of padding: MINIMAP_INFO {char
/// name[16]; s16 tile[320];} for 18 maps.
pub const MINIMAP_TABLE: u32 = 0x33EE40;
pub const MINIMAP_RECORD: usize = 0x290;
pub const MINIMAP_MAPS: usize = 18;
pub const MINIMAP_TILES: usize = 320;

/// Why a part's name may not change: the generator finds it by name, not by its entry.
fn name_found_by_the_engine(part: usize) -> Option<&'static str> {
    match part {
        32..=51 => Some("way32-way51 are found by name (\"way%d\") as the ways in and on"),
        136..=139 => Some("room56-room59 are found by name (\"room5%d\") as the healing point's room"),
        _ => None,
    }
}

/// PartsInfoData and MiniMapInfoData as a data table (`data:parts`, docs/assets/dungeons.md
/// section 2): each part's name (its string in place, 7 letters at most), kind, link, entrance and
/// the eight tile numbers no code reads; each mini map's tile for each part.
pub fn parts_table(elf: &[u8]) -> Result<crate::data_table::Table> {
    use crate::data_table::{Kind, Record, Table, S16, U8};
    use crate::elf::table_offset;
    let start = table_offset(elf, PARTS_TABLE, PARTS_COUNT * PARTS_RECORD, "PartsInfoData")?;
    let mut table = Table::new("a parts table", Vec::new(), elf.to_vec());
    let mut parts = Vec::new();
    for number in 0..PARTS_COUNT {
        let at = start + number * PARTS_RECORD;
        let pointer = u32_at(elf, at)?;
        let name_at = table_offset(elf, pointer, PART_NAME_SLOT, "a part's name")?;
        let mut record = Record::new(&[("part", Value::from(number))]);
        record.bytes("name", Kind::Text(PART_NAME_SLOT), name_at, 1);
        if let Some(why) = name_found_by_the_engine(number) {
            record.fixed(why);
        }
        record.bytes("kind", S16, at + 4, 1).bytes("link", U8, at + 6, 1).bytes("entrance", U8, at + 7, 1).bytes(
            "unread_tiles",
            S16,
            at + 8,
            8,
        );
        parts.push(record);
    }
    table.section("parts", &["part"], parts);
    let minimap = table_offset(elf, MINIMAP_TABLE, MINIMAP_MAPS * MINIMAP_RECORD, "MiniMapInfoData")?;
    let mut maps = Vec::new();
    for number in 0..MINIMAP_MAPS {
        let at = minimap + number * MINIMAP_RECORD;
        let end = elf[at..at + 16].iter().position(|&byte| byte == 0).unwrap_or(16);
        let name = String::from_utf8_lossy(&elf[at..at + end]).into_owned();
        let mut record = Record::new(&[("map", Value::from(name))]);
        record.bytes("tiles", S16, at + 16, MINIMAP_TILES);
        maps.push(record);
    }
    table.section("minimap", &["map"], maps);
    Ok(table)
}

/// One room layout as a file has it.
#[derive(Clone, Debug, PartialEq)]
pub struct Room {
    pub id: i32,
    pub rate: i32,
    pub fixed: i32,
    /// Rows from the first: (part, attribute) cells.
    pub cells: Vec<Vec<(i32, i32)>>,
}

/// A room script: its statements, and where its grid size and rooms are among them.
#[derive(Clone, Debug)]
struct Layout {
    script: Script,
    grid: Option<usize>,
    /// Each room's statements: ROOM_ID to ROOM_END.
    rooms: Vec<(usize, usize)>,
}

fn int_of(statement: &Statement, arg: usize) -> i32 {
    statement.values().get(arg).map_or(0, Arg::int)
}

impl Layout {
    fn parse(data: &[u8], what: &str) -> Result<Layout> {
        let script = Script::parse(data).context(what)?;
        let mut grid = None;
        let mut rooms = Vec::new();
        let mut open: Option<usize> = None;
        for (at, statement) in script.statements.iter().enumerate() {
            match statement.tag.as_str() {
                "GRID_SIZE" => grid = Some(at),
                "ROOM_ID" if open.is_none() => open = Some(at),
                "ROOM_END" => {
                    let Some(start) = open.take() else { bail!("{what}: ROOM_END without ROOM_ID") };
                    rooms.push((start, at));
                }
                "VER" | "ROOM_SIZE" | "ROOM_RATE" | "ROOM_FIXED" | "RD" if open.is_some() || statement.tag == "VER" => {
                }
                tag => bail!("{what}: {tag} where the room script has no place for it"),
            }
        }
        if open.is_some() {
            bail!("{what}: a room without ROOM_END");
        }
        let layout = Layout { script, grid, rooms };
        for number in 0..layout.rooms.len() {
            layout.room(number, what)?;
        }
        Ok(layout)
    }

    fn room(&self, number: usize, what: &str) -> Result<Room> {
        let (start, end) = self.rooms[number];
        let mut room = Room { id: 0, rate: 0, fixed: 0, cells: Vec::new() };
        let mut width = 0;
        for statement in &self.script.statements[start..=end] {
            match statement.tag.as_str() {
                "ROOM_ID" => room.id = int_of(statement, 0),
                "ROOM_SIZE" => width = int_of(statement, 0).max(0) as usize,
                "ROOM_RATE" => room.rate = int_of(statement, 0),
                "ROOM_FIXED" => room.fixed = int_of(statement, 0),
                "RD" => {
                    let values: Vec<i32> = statement.values().iter().map(Arg::int).collect();
                    if values.len() != width * 2 {
                        bail!("{what} room {number}: an RD line of {} values in a room {width} wide", values.len());
                    }
                    room.cells.push(values.chunks(2).map(|pair| (pair[0], pair[1])).collect());
                }
                _ => {}
            }
        }
        Ok(room)
    }

    fn grid_size(&self) -> Option<[f32; 2]> {
        self.grid.map(|at| {
            let values = self.script.statements[at].values();
            [values.first().map_or(0.0, Arg::float), values.get(1).map_or(0.0, Arg::float)]
        })
    }
}

/// The text of a cell.
fn cell_text(parts: &[Part], (part, attr): (i32, i32)) -> String {
    let name = match part {
        -1 => ".".to_string(),
        _ => match parts.get(part as usize).filter(|known| part >= 0 && !known.name.is_empty()) {
            Some(known) => known.name.clone(),
            None => format!("#{part}"),
        },
    };
    if attr == 0 {
        name
    } else {
        format!("{name}:{attr}")
    }
}

/// A cell from its text.
fn cell_from(parts: &[Part], text: &str) -> Result<(i32, i32)> {
    let (name, attr) = match text.split_once(':') {
        Some((name, attr)) => {
            let attr: i32 =
                attr.parse().map_err(|_| Error::invalid(format!("cell {text}: the attribute after : is a number")))?;
            if !(-32768..=32767).contains(&attr) {
                bail!("cell {text}: an attribute is -32768 to 32767");
            }
            (name, attr)
        }
        None => (text, 0),
    };
    let part = if name == "." {
        -1
    } else if let Some(number) = name.strip_prefix('#') {
        // #N names a part by its entry (cell_text writes it for a part with no name); an empty
        // cell, -1, is "."
        let found = number.parse::<usize>().ok().filter(|&number| number < parts.len());
        let Some(number) = found else {
            match parts.len() {
                0 => bail!("cell {text}: the executable's table has no parts"),
                count => bail!("cell {text}: #N is a part, 0 to {} (an empty cell is .)", count - 1),
            }
        };
        number as i32
    } else {
        match parts.iter().position(|part| part.name == name) {
            Some(at) => at as i32,
            None => bail!("cell {text}: no part is named {name} (the parts are way00-51, room00-59, door00-99, part00-31, step00-31)"),
        }
    };
    Ok((part, attr))
}

/// What a mod changes: one field of a layout or of one of its rooms.
#[derive(Clone, Debug, PartialEq)]
pub enum Field {
    GridSize([f32; 2]),
    Id(i32),
    Rate(i32),
    Fixed(i32),
    Cells(Vec<Vec<(i32, i32)>>),
}

impl Field {
    fn name(&self) -> &'static str {
        match self {
            Field::GridSize(_) => "grid_size",
            Field::Id(_) => "id",
            Field::Rate(_) => "rate",
            Field::Fixed(_) => "fixed",
            Field::Cells(_) => "cells",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    /// The layout's index in [`LAYOUTS`].
    pub layout: usize,
    /// The room, None for the layout's own field (its grid size).
    pub room: Option<usize>,
    pub field: Field,
    pub module: String,
    pub what: String,
}

/// The room scripts and the parts their cells name.
#[derive(Clone, Debug)]
pub struct Layouts {
    parts: Vec<Part>,
    layouts: Vec<Layout>,
}

fn number(value: &Value, what: &str) -> Result<i32> {
    value
        .as_f64()
        .filter(|number| number.fract() == 0.0 && *number >= i32::MIN as f64 && *number <= i32::MAX as f64)
        .map(|number| number as i32)
        .ok_or_else(|| Error::invalid(format!("{what} is a whole number")))
}

impl Layouts {
    /// The scripts of [`paths`], in order, with the executable's part names.
    pub fn read(files: &[Vec<u8>], parts: Vec<Part>) -> Result<Layouts> {
        if files.len() != LAYOUTS.len() {
            bail!("{} room scripts given, {} wanted", files.len(), LAYOUTS.len());
        }
        let layouts = files.iter().zip(LAYOUTS).map(|(data, name)| Layout::parse(data, name)).collect::<Result<_>>()?;
        Ok(Layouts { parts, layouts })
    }

    pub fn write(&self) -> Vec<Vec<u8>> {
        self.layouts.iter().map(|layout| layout.script.write()).collect()
    }

    pub fn rooms(&self, layout: usize) -> Vec<Room> {
        let name = LAYOUTS[layout];
        (0..self.layouts[layout].rooms.len()).map(|number| self.layouts[layout].room(number, name).unwrap()).collect()
    }

    pub fn to_json(&self) -> Value {
        let mut out = Vec::new();
        for (index, layout) in self.layouts.iter().enumerate() {
            let mut record = Map::new();
            record.insert("layout".into(), json!(LAYOUTS[index]));
            if let Some(size) = layout.grid_size() {
                record.insert("grid_size".into(), json!(size));
            }
            let rooms: Vec<Value> = self
                .rooms(index)
                .iter()
                .enumerate()
                .map(|(number, room)| {
                    let rows: Vec<String> = room
                        .cells
                        .iter()
                        .map(|row| row.iter().map(|&cell| cell_text(&self.parts, cell)).collect::<Vec<_>>().join(" "))
                        .collect();
                    json!({"room": number, "id": room.id, "rate": room.rate, "fixed": room.fixed, "cells": rows})
                })
                .collect();
            record.insert("rooms".into(), Value::Array(rooms));
            out.push(Value::Object(record));
        }
        json!({ "layouts": out })
    }

    fn cells_from(&self, value: &Value, what: &str) -> Result<Vec<Vec<(i32, i32)>>> {
        let Some(rows) = value.as_array() else { bail!("{what}: cells is a list of rows, each a string of cells") };
        let mut cells = Vec::new();
        for row in rows {
            let Some(text) = row.as_str() else {
                bail!("{what}: each row of cells is a string (\"room08 room00 room09\")")
            };
            let row = text
                .split_whitespace()
                .map(|cell| cell_from(&self.parts, cell))
                .collect::<Result<Vec<_>>>()
                .context(what)?;
            cells.push(row);
        }
        let width = cells.first().map_or(0, Vec::len);
        if width == 0 {
            bail!("{what}: a room has at least one row of one cell");
        }
        if cells.iter().any(|row| row.len() != width) {
            bail!("{what}: every row of a room has the same number of cells ({width} in the first)");
        }
        if width * cells.len() > CELLS_MOST {
            bail!("{what}: {width} × {} cells; the floor's grid holds {CELLS_MOST}", cells.len());
        }
        Ok(cells)
    }

    /// The changes a mod's JSON makes: each field that differs from the files'.
    pub fn changes(&self, given: &Value, module: &str) -> Result<Vec<Change>> {
        let mut out = Vec::new();
        let Some(records) = given.get("layouts") else {
            if given.as_object().is_some_and(|object| object.is_empty()) {
                return Ok(out);
            }
            bail!("a layout table is {{\"layouts\": [...]}}");
        };
        let Some(records) = records.as_array() else { bail!("layouts is a list of layouts") };
        for record in records {
            let record = json::object(record, "each of \"layouts\"")?;
            let Some(name) = record.get("layout").and_then(Value::as_str) else {
                bail!("each layout names its file: {{\"layout\": \"d01f01\", ...}}")
            };
            let Some(index) = LAYOUTS.iter().position(|known| *known == name) else {
                bail!("no layout {name} (the layouts are {})", LAYOUTS.join(", "));
            };
            let layout = &self.layouts[index];
            for key in record.keys() {
                if !["layout", "grid_size", "rooms"].contains(&key.as_str()) {
                    bail!("layout {name}: no field {key} (a layout has grid_size and rooms)");
                }
            }
            if let Some(size) = record.get("grid_size") {
                // every item a number: one that is not is an error, not left out
                let values: Option<Vec<f32>> = size
                    .as_array()
                    .and_then(|items| items.iter().map(|item| item.as_f64().map(|v| v as f32)).collect());
                let Some([w, d]) = values.as_deref().and_then(|values| <[f32; 2]>::try_from(values).ok()) else {
                    bail!("layout {name}: grid_size is [width, depth] of a cell");
                };
                if layout.grid_size() != Some([w, d]) {
                    out.push(Change {
                        layout: index,
                        room: None,
                        field: Field::GridSize([w, d]),
                        module: module.into(),
                        what: format!("{name} grid_size {w}, {d}"),
                    });
                }
            }
            let rooms = self.rooms(index);
            let mut seen = Vec::new();
            for room in json::list(record, "rooms", format!("layout {name}"))? {
                let room = json::object(room, format!("layout {name}: each of \"rooms\""))?;
                let number = number(
                    json::member(room, "room"),
                    &format!("layout {name}: a room's \"room\" (its place in the file, from 0)"),
                )?;
                if number < 0 || number as usize >= ROOMS_MOST {
                    bail!("layout {name} room {number}: the rooms are 0 to {}", ROOMS_MOST - 1);
                }
                let number = number as usize;
                if seen.contains(&number) {
                    bail!("layout {name} gives room {number} twice");
                }
                seen.push(number);
                let what = format!("{name} room {number}");
                let current = rooms.get(number);
                if current.is_none() && room.get("cells").is_none() {
                    bail!("{what}: a new room needs its cells");
                }
                for (key, value) in room {
                    let field = match key.as_str() {
                        "room" => continue,
                        "id" => Field::Id(number_of(value, &what, key)?),
                        "rate" => Field::Rate(number_of(value, &what, key)?),
                        "fixed" => Field::Fixed(number_of(value, &what, key)?),
                        "cells" => Field::Cells(self.cells_from(value, &what)?),
                        _ => bail!("{what}: no field {key} (a room has id, rate, fixed and cells)"),
                    };
                    let same = current.is_some_and(|current| match &field {
                        Field::Id(id) => current.id == *id,
                        Field::Rate(rate) => current.rate == *rate,
                        Field::Fixed(fixed) => current.fixed == *fixed,
                        Field::Cells(cells) => current.cells == *cells,
                        Field::GridSize(_) => false,
                    });
                    if !same {
                        let shown = match &field {
                            Field::Cells(cells) => format!("{} × {} cells", cells[0].len(), cells.len()),
                            Field::Id(value) | Field::Rate(value) | Field::Fixed(value) => value.to_string(),
                            Field::GridSize(_) => String::new(),
                        };
                        out.push(Change {
                            layout: index,
                            room: Some(number),
                            field: field.clone(),
                            module: module.into(),
                            what: format!("{what} {} {shown}", field.name()),
                        });
                    }
                }
            }
        }
        Ok(out)
    }

    /// The files with the changes made, by [`paths`]. Two changes of one field with different
    /// values are refused, naming both mods; a new room must follow the last.
    pub fn apply(&self, changes: &[Change]) -> Result<Vec<Vec<u8>>> {
        let wanted = merged(changes)?;
        let mut out = Vec::new();
        for (index, layout) in self.layouts.iter().enumerate() {
            let mine: Vec<&Change> =
                wanted.iter().filter(|((at, _, _), _)| *at == index).map(|(_, &change)| change).collect();
            if mine.is_empty() {
                out.push(layout.script.write());
                continue;
            }
            out.push(rewritten(layout, LAYOUTS[index], &self.edited(index, &mine)?)?);
        }
        Ok(out)
    }

    /// A layout's rooms and grid size with its changes made, a new room (rate 100, fixed 0, its
    /// id its place) after the last.
    fn edited(&self, index: usize, changes: &[&Change]) -> Result<Edited> {
        let mut rooms: Vec<Option<Room>> = self.rooms(index).into_iter().map(Some).collect();
        let mut changed = vec![false; rooms.len()];
        let mut grid = None;
        for change in changes {
            let Some(number) = change.room else {
                if let Field::GridSize(size) = change.field {
                    grid = Some(size);
                }
                continue;
            };
            if number >= rooms.len() {
                rooms.resize(number + 1, None);
                changed.resize(number + 1, false);
            }
            let room =
                rooms[number].get_or_insert_with(|| Room { id: number as i32, rate: 100, fixed: 0, cells: Vec::new() });
            match &change.field {
                Field::Id(value) => room.id = *value,
                Field::Rate(value) => room.rate = *value,
                Field::Fixed(value) => room.fixed = *value,
                Field::Cells(cells) => room.cells = cells.clone(),
                Field::GridSize(_) => {}
            }
            changed[number] = true;
        }
        if let Some(gap) = rooms.iter().position(Option::is_none) {
            bail!(
                "{}: room {gap} is missing; new rooms follow the last (the next is room {})",
                LAYOUTS[index],
                self.layouts[index].rooms.len()
            );
        }
        let rooms: Vec<Room> = rooms.into_iter().flatten().collect();
        if rooms.len() > ROOMS_MOST {
            bail!("{}: {} rooms; the generator holds {ROOMS_MOST}", LAYOUTS[index], rooms.len());
        }
        Ok(Edited { rooms, changed, grid })
    }
}

/// A layout's rooms and grid size as the changes leave them, and which rooms changed.
struct Edited {
    rooms: Vec<Room>,
    changed: Vec<bool>,
    grid: Option<[f32; 2]>,
}

/// The changes by (layout, room, field), each once; two of one field with different values are
/// refused, naming both mods.
/// A change's layout, room (None for the layout's own field) and field.
type FieldKey = (usize, Option<usize>, &'static str);

fn merged(changes: &[Change]) -> Result<BTreeMap<FieldKey, &Change>> {
    let mut wanted: BTreeMap<FieldKey, &Change> = BTreeMap::new();
    for change in changes {
        let key = (change.layout, change.room, change.field.name());
        match wanted.get(&key) {
            Some(known) if known.field != change.field => {
                let what = match change.room {
                    Some(room) => format!("{} room {room} {}", LAYOUTS[change.layout], change.field.name()),
                    None => format!("{} {}", LAYOUTS[change.layout], change.field.name()),
                };
                return Err(crate::given_two_ways(what, &known.module, &change.module));
            }
            Some(_) => {}
            None => {
                wanted.insert(key, change);
            }
        }
    }
    Ok(wanted)
}

/// A layout's file with its changed rooms and grid size written again in place (a grid size the
/// file lacks after its VER), its new rooms after the last; the rest keep their bytes.
fn rewritten(layout: &Layout, name: &str, edited: &Edited) -> Result<Vec<u8>> {
    let nl = layout.script.newline();
    let nl_bytes = nl.as_bytes().to_vec();
    // what is between rooms: the file's own, or three empty lines
    let separator = layout
        .rooms
        .get(1)
        .map_or_else(|| nl.repeat(4).into_bytes(), |&(start, _)| layout.script.statements[start].before.clone());
    let grid_text = |[w, d]: [f32; 2]| format!("GRID_SIZE {w:.6}, {d:.6};");
    let mut statements = Vec::new();
    let mut taken = 0;
    for (at, statement) in layout.script.statements.iter().enumerate() {
        if at < taken {
            continue;
        }
        if let (true, Some(size)) = (Some(at) == layout.grid, edited.grid) {
            statements.extend(cfg_text::statements_of(&grid_text(size), statement.before.clone())?);
            continue;
        }
        if let Some(number) = layout.rooms.iter().position(|&(start, _)| start == at).filter(|&n| edited.changed[n]) {
            let text = room_text(&edited.rooms[number], nl);
            statements.extend(cfg_text::statements_of(&text, statement.before.clone()).context(name)?);
            taken = layout.rooms[number].1 + 1;
            continue;
        }
        statements.push(statement.clone());
        if let (None, "VER", Some(size)) = (layout.grid, statement.tag.as_str(), edited.grid) {
            statements.extend(cfg_text::statements_of(&grid_text(size), nl_bytes.clone())?);
        }
    }
    for room in edited.rooms.iter().skip(layout.rooms.len()) {
        let before = if statements.is_empty() { Vec::new() } else { separator.clone() };
        statements.extend(cfg_text::statements_of(&room_text(room, nl), before).context(name)?);
    }
    let tail = if layout.script.tail.is_empty() { nl_bytes } else { layout.script.tail.clone() };
    Ok(Script { statements, tail }.write())
}

fn number_of(value: &Value, what: &str, key: &str) -> Result<i32> {
    number(value, &format!("{what}: {key}"))
}

/// A room's statements as the disc's files write them (no indent, one statement a line).
fn room_text(room: &Room, nl: &str) -> String {
    let (width, height) = (room.cells.first().map_or(0, Vec::len), room.cells.len());
    let mut text = format!(
        "ROOM_ID {};{nl}ROOM_SIZE {width},{height};{nl}ROOM_RATE {};{nl}ROOM_FIXED {};",
        room.id, room.rate, room.fixed
    );
    for row in &room.cells {
        let values: Vec<String> = row.iter().flat_map(|(part, attr)| [part.to_string(), attr.to_string()]).collect();
        text.push_str(&format!("{nl}RD {};", values.join(",")));
    }
    text.push_str(&format!("{nl}ROOM_END;"));
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts() -> Vec<Part> {
        ["way00", "way01", "room00", "room01", ""]
            .iter()
            .map(|name| Part { name: name.to_string(), kind: 0, link: 0, entrance: 0 })
            .collect()
    }

    fn sample() -> Vec<Vec<u8>> {
        let one = b"VER 1.01;\r\nGRID_SIZE 320.000000, 320.000000;\r\n\r\nROOM_ID 0;\r\nROOM_SIZE 2,1;\r\nROOM_RATE 100;\r\nROOM_FIXED 0;\r\nRD 2,0,-1,1;\r\nROOM_END;\r\n\r\n\r\n\r\nROOM_ID 1;\r\nROOM_SIZE 1,2;\r\nROOM_RATE 50;\r\nROOM_FIXED 0;\r\nRD 0,0;\r\nRD 4,3;\r\nROOM_END;\r\n\r\n".to_vec();
        let mut files = vec![b"VER 1.00;\r\n\r\nROOM_ID 0;\r\nROOM_SIZE 1,1;\r\nROOM_RATE 0;\r\nROOM_FIXED 0;\r\nRD 1,0;\r\nROOM_END;\r\n\r\n".to_vec(); LAYOUTS.len()];
        files[0] = one;
        files
    }

    #[test]
    fn rooms_read_as_named_cells_and_write_back() {
        let files = sample();
        let layouts = Layouts::read(&files, parts()).unwrap();
        assert_eq!(layouts.write(), files);
        let json = layouts.to_json();
        assert_eq!(json["layouts"][0]["grid_size"], json!([320.0, 320.0]));
        assert_eq!(json["layouts"][0]["rooms"][0]["cells"], json!(["room00 .:1"]));
        assert_eq!(json["layouts"][0]["rooms"][1]["cells"], json!(["way00", "#4:3"]));
        assert!(json["layouts"][1].get("grid_size").is_none());
        assert_eq!(layouts.changes(&json, "m").unwrap(), Vec::new());
    }

    #[test]
    fn a_changed_room_is_written_again_and_a_new_one_appended() {
        let files = sample();
        let layouts = Layouts::read(&files, parts()).unwrap();
        let edit = json!({"layouts": [
            {"layout": "d01f01", "rooms": [{"room": 1, "rate": 75, "cells": ["way01 way00"]}, {"room": 2, "cells": ["room01"]}]},
            {"layout": "d02f01", "grid_size": [400, 400]}
        ]});
        let changes = layouts.changes(&edit, "m").unwrap();
        assert_eq!(changes.len(), 4);
        let written = layouts.apply(&changes).unwrap();
        let text = String::from_utf8(written[0].clone()).unwrap();
        // room 0 keeps its bytes
        let first = String::from_utf8_lossy(&files[0]);
        assert!(text.starts_with(&first[..first.find("ROOM_ID 1").unwrap()]));
        assert!(text.contains("ROOM_ID 1;\r\nROOM_SIZE 2,1;\r\nROOM_RATE 75;\r\nROOM_FIXED 0;\r\nRD 1,0,0,0;\r\nROOM_END;\r\n\r\n\r\n\r\nROOM_ID 2;\r\nROOM_SIZE 1,1;\r\nROOM_RATE 100;\r\nROOM_FIXED 0;\r\nRD 3,0;\r\nROOM_END;\r\n\r\n"));
        assert!(String::from_utf8_lossy(&written[1])
            .starts_with("VER 1.00;\r\nGRID_SIZE 400.000000, 400.000000;\r\n\r\nROOM_ID 0;"));
        assert_eq!(written[2], files[2]);
        let again = Layouts::read(&written, parts()).unwrap();
        assert_eq!(again.rooms(0)[2].cells, vec![vec![(3, 0)]]);
        assert_eq!(again.changes(&edit, "m").unwrap(), Vec::new());
    }

    #[test]
    fn mods_that_disagree_and_bad_rooms_are_refused() {
        let layouts = Layouts::read(&sample(), parts()).unwrap();
        let one = layouts
            .changes(&json!({"layouts": [{"layout": "d01f01", "rooms": [{"room": 0, "rate": 1}]}]}), "a")
            .unwrap();
        let two = layouts
            .changes(&json!({"layouts": [{"layout": "d01f01", "rooms": [{"room": 0, "rate": 2}]}]}), "b")
            .unwrap();
        let error = layouts.apply(&[one.clone(), two].concat()).unwrap_err().to_string();
        assert_eq!(error, "d01f01 room 0 rate is given two ways, by mod a and mod b");
        assert!(layouts.apply(&[one.clone(), one].concat()).is_ok());
        for wrong in [
            json!({"layouts": [{"layout": "d01f01", "rooms": [{"room": 0, "cells": ["way00 way01", "way00"]}]}]}),
            json!({"layouts": [{"layout": "d01f01", "rooms": [{"room": 0, "cells": ["nothing"]}]}]}),
            json!({"layouts": [{"layout": "d01f01", "rooms": [{"room": 5, "rate": 3}]}]}),
            json!({"layouts": [{"layout": "x"}]}),
        ] {
            assert!(layouts.changes(&wrong, "m").is_err(), "{wrong}");
        }
        let gap = layouts
            .changes(&json!({"layouts": [{"layout": "d01f01", "rooms": [{"room": 4, "cells": ["way00"]}]}]}), "m")
            .unwrap();
        assert!(layouts.apply(&gap).unwrap_err().to_string().contains("room 2 is missing"));
    }

    #[test]
    fn part_numbers_are_the_tables_entries_and_an_empty_cell_is_a_dot() {
        assert_eq!(cell_from(&parts(), "#4:3").unwrap(), (4, 3));
        assert_eq!(cell_from(&parts(), ".").unwrap(), (-1, 0));
        for wrong in ["#-1", "#5", "#x"] {
            let error = cell_from(&parts(), wrong).unwrap_err().to_string();
            assert!(error.contains("0 to 4") && error.contains("an empty cell is ."), "{error}");
        }
        assert!(cell_from(&[], "#0").unwrap_err().to_string().contains("no parts"));
    }

    #[test]
    fn a_room_past_a_file_without_rooms_is_refused_not_a_panic() {
        let mut files = sample();
        files[2] = b"VER 1.00;\r\n".to_vec();
        let layouts = Layouts::read(&files, parts()).unwrap();
        let gap = layouts
            .changes(&json!({"layouts": [{"layout": LAYOUTS[2], "rooms": [{"room": 1, "cells": ["way00"]}]}]}), "m")
            .unwrap();
        let error = layouts.apply(&gap).unwrap_err().to_string();
        assert!(error.contains("room 0 is missing") && error.contains("the next is room 0"), "{error}");
    }

    #[test]
    fn a_member_of_the_wrong_shape_is_refused_not_ignored() {
        let layouts = Layouts::read(&sample(), parts()).unwrap();
        let refused = |edit: Value| layouts.changes(&edit, "m").unwrap_err().to_string();
        let error = refused(json!({"layouts": [{"layout": "d01f01", "rooms": {"room": 0}}]}));
        assert_eq!(error, "layout d01f01: \"rooms\" is a list, not an object");
        assert!(refused(json!({"layouts": [{"layout": "d01f01", "rooms": [3]}]})).contains("is an object"));
        assert!(refused(json!({"layouts": [{"layout": "d01f01", "grid_size": [400, "x"]}]})).contains("grid_size"));
        let three = json!({"layouts": [{"layout": "d01f01", "grid_size": [400, null, 400]}]});
        assert!(refused(three).contains("grid_size"));
    }

    /// The room scripts of an extracted disc (C2S_CORPUS) read with the executable's part names,
    /// write back unchanged, and their JSON changes nothing.
    /// PartsInfoData and MiniMapInfoData of the executable (C2S_CORPUS) as data:parts: they read
    /// back as written, and a part renamed in its slot is patched in place.
    #[test]
    #[ignore = "needs C2S_CORPUS"]
    fn corpus_parts_table() {
        let corpus = dc2tk_disc::samples::corpus();
        let elf = std::fs::read(std::path::Path::new(&corpus).join(crate::elf::EXECUTABLE)).unwrap();
        let table = parts_table(&elf).unwrap();
        assert_eq!(table.check(), Vec::<String>::new());
        let json = table.to_json();
        assert_eq!(
            json["parts"][80],
            json!({"part": 80, "name": "door00", "kind": 10, "link": 15, "entrance": 1,
            "unread_tiles": [42, 42, 40, 42, 42, 42, 42, 42]})
        );
        assert_eq!(
            (json["parts"][276]["name"].clone(), json["minimap"][0]["map"].clone()),
            (json!(""), json!("d01f01"))
        );
        assert_eq!(json["minimap"][17]["map"], "d07f01");
        assert_eq!(json["minimap"][0]["tiles"][276], 0);
        let (edits, patches) =
            table.changes(&json!({"parts": [{"part": 276, "name": "newp00", "kind": 4, "link": 15}]}), "m").unwrap();
        assert!(edits.is_empty() && patches.len() == 3);
        let patched = crate::elf::apply_patches(&elf, &patches).unwrap();
        let parts = super::parts(&patched).unwrap();
        assert_eq!((parts[276].name.as_str(), parts[276].kind, parts[276].link), ("newp00", 4, 15));
        assert!(table.changes(&json!({"parts": [{"part": 276, "name": "toolong8"}]}), "m").is_err());
        assert!(table.changes(&json!({"parts": [{"part": 32, "name": "wayxx"}]}), "m").is_err());
    }

    #[test]
    #[ignore = "needs C2S_CORPUS"]
    fn corpus_layouts_round_trip() {
        let corpus = dc2tk_disc::samples::corpus();
        let root = std::path::Path::new(&corpus);
        let files: Vec<Vec<u8>> = paths().iter().map(|path| std::fs::read(root.join(path)).unwrap()).collect();
        let parts = super::parts(&std::fs::read(root.join(crate::elf::EXECUTABLE)).unwrap()).unwrap();
        assert_eq!(parts.len(), 277);
        assert_eq!(
            (parts[0].name.as_str(), parts[52].name.as_str(), parts[275].name.as_str()),
            ("way00", "room00", "door99")
        );
        assert_eq!((parts[80].kind, parts[80].link, parts[80].entrance), (0xA, 15, 1));
        let layouts = Layouts::read(&files, parts).unwrap();
        assert_eq!(layouts.write(), files);
        let json = layouts.to_json();
        assert_eq!(layouts.changes(&json, "m").unwrap(), Vec::new());
        // every room rewritten from its JSON gives its own bytes again
        let mut all = Vec::new();
        for (index, record) in json["layouts"].as_array().unwrap().iter().enumerate() {
            for room in record["rooms"].as_array().unwrap() {
                let cells = layouts.cells_from(&room["cells"], "test").unwrap();
                all.push(Change {
                    layout: index,
                    room: Some(room["room"].as_u64().unwrap() as usize),
                    field: Field::Cells(cells),
                    module: "m".into(),
                    what: String::new(),
                });
            }
        }
        assert_eq!(layouts.apply(&all).unwrap(), files);
        let rooms: usize = (0..LAYOUTS.len()).map(|index| layouts.rooms(index).len()).sum();
        eprintln!("{} room layouts in {} files round-trip", rooms, LAYOUTS.len());
    }
}
