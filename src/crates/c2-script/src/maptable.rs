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
use std::sync::OnceLock;

use c2_disc::archive::Archive;
use c2_disc::Image;
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

fn row_pattern() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| Regex::new(r#"(?-u)(?m)^[ \t]*MAP_NAME[ \t]+"([^"\r\n]*)"[^\r\n]*"#).unwrap())
}

fn strings_pattern() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| Regex::new(r#"(?-u)MAP_NAME[ \t]+"([^"]*)"\s*,\s*"([^"]*)"\s*,\s*"([^"]*)""#).unwrap())
}

fn count_pattern() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| Regex::new(r"MAP_NAME_NUM[ \t]+([0-9]+)").unwrap())
}

/// MAP_NAME_NUM's count, and where its number is.
fn count(cfg: &[u8]) -> Result<(usize, std::ops::Range<usize>)> {
    let found = count_pattern().captures(cfg).ok_or_else(|| Error::invalid("the map table has no MAP_NAME_NUM"))?;
    let digits = found.get(1).unwrap();
    let number = std::str::from_utf8(digits.as_bytes())
        .unwrap()
        .parse()
        .map_err(|_| Error::invalid("MAP_NAME_NUM is too large"))?;
    Ok((number, digits.range()))
}

/// The bytes of MapNameBuff a table fills: its rows and its strings, as mlMAP_NAME_NUM and
/// LoadMapName round them.
pub fn buffer_use(cfg: &[u8]) -> Result<usize> {
    let (rows, _) = count(cfg)?;
    let strings: usize = strings_pattern()
        .captures_iter(cfg)
        .map(|found| (1..=3).map(|at| found[at].len()).filter(|&len| len > 0).map(|len| len + 1).sum::<usize>())
        .sum();
    Ok(((((rows + 1) * ROW) >> 4) + 1) * 16 + (strings / 16 + 1) * 16)
}

/// The table with a row for each new map after its last, each a copy of its like map's row with
/// the new name (and title), MAP_NAME_NUM raised to match; and the new maps' numbers.
pub fn with_new_maps(cfg: &[u8], maps: &[NewMap]) -> Result<(Vec<u8>, Vec<i64>)> {
    let rows: Vec<(Vec<u8>, Vec<u8>)> =
        row_pattern().captures_iter(cfg).map(|found| (found[1].to_vec(), found[0].to_vec())).collect();
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
        if rows.iter().any(|(name, _)| name.eq_ignore_ascii_case(map.name.as_bytes()))
            || maps[..at].iter().any(|known| known.name == map.name)
        {
            return Err(Error::invalid(format!("the map table already has a map {}", map.name)));
        }
        let Some((_, like)) = rows.iter().find(|(name, _)| name.as_slice() == map.like.as_bytes()) else {
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
    static QUOTED: OnceLock<Regex> = OnceLock::new();
    static NUMBER: OnceLock<Regex> = OnceLock::new();
    let quoted = QUOTED.get_or_init(|| Regex::new(r#"(?-u)"[^"]*""#).unwrap());
    let mut spans = quoted.find_iter(like).map(|found| found.range());
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
    // the row's number in the comment the tables end each row with
    let comment = NUMBER.get_or_init(|| Regex::new(r"//[ \t]*[0-9]+").unwrap());
    let row = match comment.find(&row) {
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
    for line in lines.iter().filter(|line| line.is_map() && line.add) {
        let name = line.map_name();
        if let Some((_, other)) = out.iter().find(|(known, _)| known.name == name) {
            return Err(Error::invalid(format!("mods {other} and {} both add a map {name}", line.module)));
        }
        let map = NewMap { name, like: line.like.clone().unwrap_or_default(), title: line.title.clone() };
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
