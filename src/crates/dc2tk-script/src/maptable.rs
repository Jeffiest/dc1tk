//! The map table (`map/map<language>.cfg`, docs/assets/maps.md section 9) with new maps' rows
//! appended: what `build` writes for a mod's new maps (`"add": {"map:m05x": {"file": ...,
//! "like": "m05"}}`), in every language's table alike.
//!
//! LoadMapName (mapselect.cpp) runs the table at boot: `MAP_NAME_NUM n;` sizes the table (n + 1
//! MAP_NAME_INFO of 0x1C bytes) at the start of MapNameBuff (0x8000 bytes) and puts the strings
//! after it; each `MAP_NAME "name","title","add_path",type,sel_type[,snd_id[,area_no]];` fills the
//! next row, unchecked. GetMapNameInfo finds rows below n only, so a row is appended and n raised
//! with it. A map's number is its row: appending keeps every other map's number, which saves and
//! scripts hold. The save keeps map flags (opened chests and the like) for numbers below 256
//! (SAVE_MAP_FLAG_MAX); a map numbered past that loads but keeps none.

use std::path::Path;
use std::sync::LazyLock;

use dc2tk_assets::info_cfg::{self, Arg, Statement};
use dc2tk_disc::archive::Archive;
use dc2tk_disc::{text, Image};
use indexmap::IndexMap;
use regex::bytes::Regex;

use crate::{replace, startat, Error, Result};

/// The languages whose tables the game reads (map0.cfg Japanese to map6.cfg Korean); `map.cfg`
/// is an older table nothing reads.
pub const LANGUAGES: [i64; 7] = [0, 1, 2, 3, 4, 5, 6];
/// MapNameBuff: the rows and their strings.
pub const BUFFER: usize = 0x8000;
/// MAP_NAME_INFO.
pub const ROW: usize = 0x1C;
/// The map numbers whose flags the save keeps (SAVE_MAP_FLAG_MAX).
pub const FLAGGED_MAPS: i64 = 0x100;

/// A map a mod adds: its name, the map whose row it copies, and its own title if it has one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewMap {
    pub name: String,
    pub like: String,
    pub title: Option<String>,
}

/// A row of the map table, as mlMAP_NAME (mapselect.cpp) stores it:
/// `MAP_NAME "code","title","add_path",type,sel_type[,snd_id[,area_no]];`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// The map's number: the row's place among the table's MAP_NAME rows, from 0, empty rows
    /// counted (mlMAP_NAME fills `map_name[now_no++]`).
    pub number: i64,
    /// The map's code ("m05"); empty in an unused row.
    pub code: String,
    /// The title, in the bytes of the table's language (cp932 for Japanese and English, Latin-1
    /// for the others).
    pub title: Vec<u8>,
    /// The add map's path under map/cmn/ ("trn/train"), empty when the map has none.
    pub add: String,
    /// The map's type (7 and 8 load in the dungeon loop, 4 and 6 are interiors); 0 when the row
    /// gives none.
    pub kind: i64,
    /// The bytes of MapNameBuff the row's strings take: each one that is not empty, with its NUL.
    strings: usize,
    /// The row's line in the table, without its line break; the leading spaces too when the row
    /// starts the line.
    line: std::ops::Range<usize>,
}

/// A statement's argument as the bytes inside its quotes, in the file's own encoding; None when it
/// is not a string.
pub(crate) fn quoted_bytes<'a>(cfg: &'a [u8], (arg, at): &(Arg, std::ops::Range<usize>)) -> Option<&'a [u8]> {
    match arg {
        Arg::Str(_) => cfg.get(at.start + 1..at.end - 1),
        _ => None,
    }
}

/// A statement's argument as an integer; None when it is not one.
pub(crate) fn int_arg(arg: Option<&(Arg, std::ops::Range<usize>)>) -> Option<i64> {
    match arg {
        Some((Arg::Int(value), _)) => Some(*value),
        _ => None,
    }
}

/// The line a statement is on, without its line break: from the line's start when only spaces
/// come before the statement, otherwise from the statement.
fn line_of(cfg: &[u8], statement: &Statement) -> std::ops::Range<usize> {
    let line = cfg[..statement.start].iter().rposition(|&b| b == b'\n').map_or(0, |at| at + 1);
    let start = if cfg[line..statement.start].iter().all(|b| b" \t".contains(b)) { line } else { statement.start };
    let end =
        cfg[statement.end..].iter().position(|&b| b == b'\r' || b == b'\n').map_or(cfg.len(), |at| statement.end + at);
    start..end
}

/// The table's rows, read as LoadMapName reads them: CScriptInterpreter's statements (comments
/// skipped), each MAP_NAME one the next row.
pub fn rows(cfg: &[u8]) -> Vec<Row> {
    info_cfg::statements(cfg)
        .iter()
        .filter(|statement| statement.tag == "MAP_NAME")
        .enumerate()
        .map(|(number, statement)| {
            let string = |at: usize| statement.args.get(at).and_then(|arg| quoted_bytes(cfg, arg)).unwrap_or_default();
            let (code, title, add) = (string(0), string(1), string(2));
            Row {
                number: number as i64,
                code: text::decode_lossy(code),
                title: title.to_vec(),
                add: text::decode_lossy(add),
                kind: int_arg(statement.args.get(3)).unwrap_or(0),
                strings: [code, title, add].iter().filter(|text| !text.is_empty()).map(|text| text.len() + 1).sum(),
                line: line_of(cfg, statement),
            }
        })
        .collect()
}

/// MAP_NAME_NUM's count, and where its number is.
fn count(cfg: &[u8]) -> Result<(usize, std::ops::Range<usize>)> {
    let statements = info_cfg::statements(cfg);
    let found = statements
        .iter()
        .find(|statement| statement.tag == "MAP_NAME_NUM")
        .ok_or_else(|| Error::invalid("the map table has no MAP_NAME_NUM"))?;
    match found.args.first() {
        Some((Arg::Int(number), at)) if *number >= 0 => Ok((*number as usize, at.clone())),
        _ => Err(Error::invalid("MAP_NAME_NUM is not a count of maps")),
    }
}

/// The bytes of MapNameBuff a table fills: its rows and its strings, as mlMAP_NAME_NUM and
/// LoadMapName round them.
pub fn buffer_use(cfg: &[u8]) -> Result<usize> {
    let (rows_counted, _) = count(cfg)?;
    let strings: usize = rows(cfg).iter().map(|row| row.strings).sum();
    Ok(((((rows_counted + 1) * ROW) >> 4) + 1) * 16 + (strings / 16 + 1) * 16)
}

/// The table with a row for each new map after its last, each a copy of its like map's row with
/// the new name (and title), MAP_NAME_NUM raised to match; and the new maps' numbers.
pub fn with_new_maps(cfg: &[u8], maps: &[NewMap]) -> Result<(Vec<u8>, Vec<i64>)> {
    let rows: Vec<(String, &[u8])> = rows(cfg).into_iter().map(|row| (row.code, &cfg[row.line])).collect();
    let (counted, digits) = count(cfg)?;
    if counted != rows.len() {
        return Err(Error::invalid(format!(
            "the map table counts {counted} maps (MAP_NAME_NUM) but has {} rows; a row appended there would not be read",
            rows.len()
        )));
    }
    let newline: &[u8] = if cfg.windows(2).any(|pair| pair == b"\r\n") { b"\r\n" } else { b"\n" };
    let mut text = cfg.to_vec();
    if !text.ends_with(b"\n") {
        text.extend_from_slice(newline);
    }
    let mut numbers = Vec::new();
    for (at, map) in maps.iter().enumerate() {
        if rows.iter().any(|(name, _)| name.eq_ignore_ascii_case(&map.name))
            || maps[..at].iter().any(|known| known.name == map.name)
        {
            return Err(Error::invalid(format!("the map table already has a map {}", map.name)));
        }
        let Some((_, like)) = rows.iter().find(|(name, _)| *name == map.like) else {
            return Err(Error::invalid(format!("the map table has no map {} to copy", map.like)));
        };
        let number = (rows.len() + at) as i64;
        text.extend_from_slice(&new_row(like, map, number)?);
        text.extend_from_slice(newline);
        numbers.push(number);
    }
    let raised = (counted + maps.len()).to_string();
    text.splice(digits, raised.into_bytes());
    let used = buffer_use(&text)?;
    if used > BUFFER {
        return Err(Error::invalid(format!(
            "the map table would fill {used} bytes of the {BUFFER} the game reads it into; give the new maps shorter titles"
        )));
    }
    Ok((text, numbers))
}

/// The like map's row with the new map's name, title and number.
fn new_row(like: &[u8], map: &NewMap, number: i64) -> Result<Vec<u8>> {
    static QUOTED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?-u)"[^"]*""#).unwrap());
    // the row's number in the comment the tables end each row with
    static NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"//[ \t]*[0-9]+").unwrap());
    let mut spans = QUOTED.find_iter(like).map(|found| found.range());
    let (Some(name), Some(title)) = (spans.next(), spans.next()) else {
        return Err(Error::invalid(format!("map {}'s row has no title", map.like)));
    };
    let mut row = Vec::new();
    row.extend_from_slice(&like[..name.start]);
    row.extend_from_slice(format!("\"{}\"", map.name).as_bytes());
    row.extend_from_slice(&like[name.end..title.start]);
    match &map.title {
        Some(given) => row.extend_from_slice(format!("\"{given}\"").as_bytes()),
        None => row.extend_from_slice(&like[title.clone()]),
    }
    row.extend_from_slice(&like[title.end..]);
    let row = match NUMBER.find(&row) {
        Some(found) => [&row[..found.start()], format!("// {number}").as_bytes(), &row[found.end()..]].concat(),
        None => row,
    };
    Ok(row)
}

/// The map numbers the built image will have: the table's own and the new maps' after them.
pub fn numbers_with(cfg: &[u8], maps: &[NewMap]) -> Result<IndexMap<String, i64>> {
    let (table, _) = with_new_maps(cfg, maps)?;
    Ok(startat::map_numbers(&table))
}

/// The new maps of mods' lines (`map:` under "add"), in their order; a name given twice is
/// refused, naming both mods.
pub fn new_maps(lines: &[replace::Line]) -> Result<Vec<NewMap>> {
    let mut out: Vec<(NewMap, &str)> = Vec::new();
    for line in lines.iter().filter(|line| line.add) {
        let replace::LineKind::Map { like, title, .. } = &line.kind else { continue };
        let name = line.map_name();
        if let Some((_, other)) = out.iter().find(|(known, _)| known.name == name) {
            return Err(Error::invalid(format!("mods {other} and {} both add a map {name}", line.module)));
        }
        let map = NewMap { name, like: like.clone().unwrap_or_default(), title: title.clone() };
        out.push((map, &line.module));
    }
    Ok(out.into_iter().map(|(map, _)| map).collect())
}

/// The rows of the PAL disc's map tables (MAP_NAME_NUM 189).
pub const DISC_ROWS: i64 = 189;

/// The new maps' numbers on a PAL disc, without reading it: after its 189 rows, in order (what
/// mod check uses without --disc).
pub fn assumed_numbers(maps: &[NewMap]) -> IndexMap<String, i64> {
    maps.iter().enumerate().map(|(at, map)| (map.name.clone(), DISC_ROWS + at as i64)).collect()
}

/// startat::disc_map_numbers with the new maps after the disc's own.
pub fn disc_numbers_with(iso: &Path, language: Option<i64>, maps: &[NewMap]) -> Result<IndexMap<String, i64>> {
    if maps.is_empty() {
        return startat::disc_map_numbers(iso, language);
    }
    let image = Image::open(iso)?;
    let disc = Archive::open(image.bytes())?;
    let path = startat::map_table_path(language);
    let entry = disc.entry(&path).ok_or_else(|| Error::invalid(format!("the disc has no {path}")))?;
    numbers_with(disc.read(image.bytes(), entry), maps)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &[u8] = b"MAP_NAME_NUM 3 ;\r\nMAP_NAME \"g01\",\"Sindain\",\"g00/g00\", 1 , 1 , 2000 , 2 ;// 0 \r\n\
MAP_NAME \"\",\"\",\"\",0,0;// 1 \r\nMAP_NAME \"m05\",\"\x83p\x81[\x83\x8b\",\"\", 3 , 2 , 1000 , 1 ;// 2 \r\n";

    #[test]
    fn new_maps_are_appended_as_copies_of_their_like_rows() {
        let maps = [
            NewMap { name: "m05x".into(), like: "m05".into(), title: None },
            NewMap { name: "g01b".into(), like: "g01".into(), title: Some("New Town".into()) },
        ];
        let (table, numbers) = with_new_maps(TABLE, &maps).unwrap();
        assert_eq!(numbers, [3, 4]);
        let expected = [
            b"MAP_NAME_NUM 5 ;\r\n".as_slice(),
            &TABLE[18..],
            b"MAP_NAME \"m05x\",\"\x83p\x81[\x83\x8b\",\"\", 3 , 2 , 1000 , 1 ;// 3 \r\n",
            b"MAP_NAME \"g01b\",\"New Town\",\"g00/g00\", 1 , 1 , 2000 , 2 ;// 4 \r\n",
        ]
        .concat();
        assert_eq!(String::from_utf8_lossy(&table), String::from_utf8_lossy(&expected));
        let numbers = numbers_with(TABLE, &maps).unwrap();
        assert_eq!((numbers["m05"], numbers["m05x"], numbers["g01b"]), (2, 3, 4));
    }

    #[test]
    fn rows_are_numbered_as_the_engine_reads_them() {
        let table = b"MAP_NAME_NUM 4 ;\r\n// MAP_NAME \"old\",\"Old\",\"\",1,1;\r\n  MAP_NAME \"g01\",\"Sindain\",\"g00/g00\", 1 , 1 ;// 0\r\n\
/* MAP_NAME \"x\",\"\",\"\",3,3; */MAP_NAME \"\",\"\",\"\",0,0;\r\nMAP_NAME \"m05\",\"Park\",\"\";\r\nMAP_NAME \"s01\",\"Reservoir\",\"trn/train\",3,3;\r\n";
        let rows = rows(table);
        let read: Vec<(i64, &str, &[u8], &str, i64)> = rows
            .iter()
            .map(|row| (row.number, row.code.as_str(), row.title.as_slice(), row.add.as_str(), row.kind))
            .collect();
        assert_eq!(
            read,
            [
                (0, "g01", b"Sindain".as_slice(), "g00/g00", 1),
                (1, "", b"".as_slice(), "", 0),
                // a row without its type is still a row: the next keeps its number
                (2, "m05", b"Park".as_slice(), "", 0),
                (3, "s01", b"Reservoir".as_slice(), "trn/train", 3),
            ]
        );
        // a row's line keeps its leading spaces and comment, and a row after a comment starts at its tag
        assert_eq!(&table[rows[0].line.clone()], b"  MAP_NAME \"g01\",\"Sindain\",\"g00/g00\", 1 , 1 ;// 0");
        assert_eq!(&table[rows[1].line.clone()], b"MAP_NAME \"\",\"\",\"\",0,0;");
        let numbers = startat::map_numbers(table);
        assert_eq!(numbers.into_iter().collect::<Vec<_>>(), [("g01".into(), 0), ("m05".into(), 2), ("s01".into(), 3)]);
        // the commented rows take no space in MapNameBuff
        assert_eq!(
            buffer_use(table).unwrap(),
            (((5 * ROW) >> 4) + 1) * 16 + ((4 + 8 + 8 + 4 + 5 + 4 + 10 + 10) / 16 + 1) * 16
        );
    }

    #[test]
    fn names_the_table_has_and_unknown_like_maps_are_refused() {
        let again = [NewMap { name: "M05".into(), like: "g01".into(), title: None }];
        assert!(with_new_maps(TABLE, &again).unwrap_err().to_string().contains("already has"));
        let unknown = [NewMap { name: "z99".into(), like: "q01".into(), title: None }];
        assert!(with_new_maps(TABLE, &unknown).unwrap_err().to_string().contains("no map q01"));
        let short = b"MAP_NAME_NUM 2 ;\r\nMAP_NAME \"m05\",\"a\",\"\",3,2;\r\n";
        assert!(with_new_maps(short, &[]).unwrap_err().to_string().contains("counts 2"));
    }

    /// Every language's table of an extracted disc (C2S_CORPUS) takes a new map as number 189,
    /// within its buffer.
    #[test]
    fn corpus_tables_take_a_new_map() {
        let Some(corpus) = std::env::var_os("C2S_CORPUS") else {
            return;
        };
        for language in LANGUAGES {
            let cfg =
                std::fs::read(std::path::Path::new(&corpus).join(startat::map_table_path(Some(language)))).unwrap();
            assert!(buffer_use(&cfg).unwrap() < BUFFER / 2, "map{language}.cfg");
            let maps = [NewMap { name: "m05x".into(), like: "m05".into(), title: None }];
            let (table, numbers) = with_new_maps(&cfg, &maps).unwrap();
            assert_eq!(numbers, [189]);
            let read = startat::map_numbers(&table);
            assert_eq!((read["m05"], read["m05x"], read.len()), (14, 189, 186));
        }
    }
}
