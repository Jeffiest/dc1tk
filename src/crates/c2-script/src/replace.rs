//! Mods by name: the game assets a mod replaces, named in its mod.json by their asset ids (see
//! "Mods by name" in docs/design.md) instead of by where they sit on the disc. Where the files sit
//! inside the mod's folder does not matter.
//!
//! ```text
//! "replace": {
//!   "texture:e254a1": "art/soldier_green.png",              a PNG for a picture
//!   "model:c02fuku*": "models/monica/c02b.gltf",            a glTF for every model the pattern matches
//!   "sound:SE_EV_000_130_GRIFFON_BREAK_WINDOW": "sounds/boom.wav",
//!   "sound:EV_000_130/13": {"file": "sounds/chime.wav", "volume": 90},
//!   "bank:SP_003": "sounds/SP_003",                         a folder of WAVs by entry number
//!   "texture:a01ia_01": {"file": "art/floor.png", "only": ["map/a"]},
//!   "text:en/shopname": "text/en/shopname.txt",             a message file's text, by message
//!   "music:BG_000_000": "music/BG_000_000.mid",             a sequence (music, an ambient loop) as MIDI
//!   "music:BG_001_000": {"file": "music/tune.mid", "instruments": {"100": "music/pulse.wav"}},
//!                                                           with WAVs as new programs of its bank
//!   "file:snd2/event/EV_000_130.snd": "raw/bank.snd"        a disc file or pack member by path
//! },
//! "add": {
//!   "sound:SE_MOD_BEEP": {"file": "sounds/beep.wav", "bank": "SP_004"},
//!   "file:mods/calc/ui.img": {"file": "ui", "like": "texture:logo_d00"},     a new texture file of PNGs
//!   "file:mods/calc/calc.chr": {"file": "calc/calc.gltf", "like": "model:uki"} a new pack from a glTF
//!   "map:m05x": {"file": "maps/m05x/m05x.gltf", "like": "m05", "title": "Park"} a new map from a map's scene
//! }
//! ```
//!
//! `"data:items": "data/items.json"` (also data:shops, data:monsters, data:inventions, ...) edits a table of game data
//! as JSON (docs/assets/data.md); c2's data_cmd builds those, merging several mods' tables.
//!
//! A replacement reaches every place the disc has the asset (ids::Index lists them); "only" keeps
//! the places under the given disc paths. A model is a glTF file with its buffer beside it (its
//! textures change through their own ids), or a folder as `mod asset` writes it (the
//! export's glTF and buffer, with textures/ whose edited PNGs go into the pack). A pattern (* and
//! ?) replaces every id it matches with one file; an id given exactly wins over a pattern. "file:"
//! lines are game files by path, as the old disc/ layout gives them (discfiles), and a folder there
//! converts by the game file's suffix. "add" appends a sound entry to every place of a bank; the
//! mod's patches see its name as a constant holding the entry's number. A file: line with "like"
//! makes its game file whole from a common format, with the asset or disc file "like" names as the
//! template (a texture file of PNGs, or a character pack of a glTF): a new file the disc does not
//! have, with pictures or meshes of its own. A text (c2_assets::text) changes, at every place of
//! its id, the messages whose text differs from the one exported (the id's first place), and adds
//! the ids that place lacks; each place keeps the rest of its own version.
//!
//! Each change is built with the disc's own file as the template, as discfiles does. Models are
//! built first, then banks, then pictures, so a texture goes into a pack whose model another line
//! rebuilt. Two chosen mods may not replace one id, one picture or sound entry, or one model, nor
//! change a game file another's file: line or disc/ file covers.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use c2_assets::ids::{self, Index};
use c2_assets::font;
use c2_assets::im3;
use c2_assets::soundbank::Bank;
use c2_assets::sounds::{self, Given};
use c2_assets::text;
use c2_assets::tree::{self, Kind, Place};
use c2_assets::{music, soundfont};
use c2_disc::archive::Archive;
use c2_disc::{rebuild, Image};
use indexmap::IndexMap;
use serde_json::Value;

use crate::discfiles::{self, Files, Template, Templates, Wanted};
use crate::{mods, Error, Result};

pub const REPLACE: &str = "replace";
pub const ADD: &str = "add";
pub const FILE: &str = "file:";
/// A map edited as a glTF scene (docs/assets/maps.md section 8): `"map:s17": "maps/s17/s17.gltf"`;
/// under "add", a new map made from another's scene (`{"file": ..., "like": "m05"}`, section 9).
pub const MAP: &str = "map:";
/// The longest name a new map may have: its files are `<name>_<language>.stb` and the like in the
/// engine's 32-byte file names.
pub const MAP_NAME_MOST: usize = 12;
/// A table of game data edited as JSON (docs/assets/data.md): `"data:items": "data/items.json"`.
pub const DATA: &str = "data:";
/// What a music: line's MIDI file may be called.
const MIDI_SUFFIXES: [&str; 3] = [".mid", ".midi", ".rmi"];
/// The tables a data: line can name, in the order build applies them (parts before layouts, whose
/// cells name the parts).
pub const DATA_TABLES: [&str; 12] = [
    "items", "shops", "monsters", "inventions", "georama", "medals", "floors", "fishing", "characters", "parts",
    "layouts", "chests",
];

/// One line of a mod's replace or add.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub module: String,
    pub key: String,
    /// The file as mod.json gives it, and where it is.
    pub shown: String,
    pub file: PathBuf,
    pub only: Vec<String>,
    pub volume: Option<i64>,
    /// The bank an added sound goes into.
    pub bank: Option<String>,
    pub add: bool,
    /// For a file: line made whole from a common format, the template: an asset id or disc path;
    /// for a new map, the map it is made from.
    pub like: Option<String>,
    /// A new map's title in the map table (every language's), instead of the like map's.
    pub title: Option<String>,
    /// A new map's own event script: a C source of the mod, compiled into the map's script files
    /// (mapscript).
    pub script: Option<PathBuf>,
    /// A map: line's event points given programs: (point, program), the program a number, a
    /// program's name or a NEW_PROGRAM name of the mod (docs/assets/maps.md section 9).
    pub programs: Vec<(String, Value)>,
    /// For a music: line, programs the line adds to the sequence's bank (music::Instrument).
    pub instruments: Vec<music::Instrument>,
}

impl Line {
    pub fn is_file(&self) -> bool {
        self.key.starts_with(FILE)
    }

    pub fn is_map(&self) -> bool {
        self.key.starts_with(MAP)
    }

    pub fn is_data(&self) -> bool {
        self.key.get(..DATA.len()).is_some_and(|prefix| prefix.eq_ignore_ascii_case(DATA))
    }

    /// The table a data: line edits (items).
    pub fn data_table(&self) -> String {
        self.key[DATA.len()..].trim().to_lowercase()
    }

    /// The map a map: line edits (s17).
    pub fn map_name(&self) -> String {
        self.key[MAP.len()..].trim().to_string()
    }

    /// The disc path of a file: line.
    pub fn disc_path(&self) -> String {
        self.key[FILE.len()..].replace('\\', "/").trim_matches('/').to_string()
    }

    fn fail(&self, message: impl std::fmt::Display) -> Error {
        Error::invalid(format!("{} (mod {}): {message}", self.key, self.module))
    }
}

fn name_of(path: &Path) -> String {
    path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
}

fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn suffix_is(path: &Path, suffix: &str) -> bool {
    name_of(path).to_lowercase().ends_with(suffix)
}

/// A mod's replace and add lines, in the order mod.json gives them, each file checked to be in
/// the mod.
pub fn lines(mod_dir: &Path) -> Result<Vec<Line>> {
    let found = mods::entries(mod_dir)?;
    let module = name_of(mod_dir);
    let mut out = Vec::new();
    for (section, add) in [(REPLACE, false), (ADD, true)] {
        let listed = match &found[section] {
            Value::Null => continue,
            Value::Object(listed) => listed,
            _ => {
                return Err(Error::invalid(format!(
                    "{module}: \"{section}\" in {} maps asset ids to files",
                    mods::INDEX
                )))
            }
        };
        for (key, value) in listed {
            out.push(line(mod_dir, &module, key, value, add)?);
        }
    }
    Ok(out)
}

/// A music: line's "instruments": {program: a WAV, or a sample {"file", "keys": [low, high],
/// "root", "loop", "volume", "like"}, or a list of samples by key range}.
fn instruments(mod_dir: &Path, line: &Line, value: &Value) -> Result<Vec<music::Instrument>> {
    let usage = "\"instruments\" maps program numbers to WAVs: {\"100\": \"music/flute.wav\"}, or to samples \
                 {\"file\": ..., \"keys\": [0, 127], \"root\": 60, \"loop\": true, \"volume\": 128, \"like\": 3} or lists of them";
    let Value::Object(programs) = value else {
        return Err(line.fail(usage));
    };
    let mut out = Vec::new();
    for (number, given) in programs {
        let Some(program) = number.trim().parse::<u8>().ok().filter(|&program| program < 128) else {
            return Err(line.fail(format!("instrument {number}: a program is 0 to 127")));
        };
        let samples: Vec<&Value> = match given {
            Value::Array(samples) => samples.iter().collect(),
            other => vec![other],
        };
        let mut instrument = music::Instrument { program, zones: Vec::new(), like: None };
        for sample in samples {
            let fail = |message: &str| line.fail(format!("instrument {number}: {message}"));
            let mut zone = music::InstrumentZone {
                path: PathBuf::new(),
                low: 0,
                high: 127,
                root: 60,
                looped: None,
                volume: 128,
            };
            let file = match sample {
                Value::String(file) => file.clone(),
                Value::Object(fields) => {
                    for (field, value) in fields {
                        let number = |most: u64| value.as_u64().filter(|&n| n <= most).map(|n| n as u8);
                        match field.as_str() {
                            "file" => {}
                            "keys" => {
                                let keys: Option<Vec<u8>> = value
                                    .as_array()
                                    .filter(|keys| keys.len() == 2)
                                    .map(|keys| keys.iter().filter_map(|key| key.as_u64().filter(|&k| k <= 127)).map(|k| k as u8).collect());
                                match keys {
                                    Some(keys) if keys.len() == 2 && keys[0] <= keys[1] => (zone.low, zone.high) = (keys[0], keys[1]),
                                    _ => return Err(fail("\"keys\" is [lowest, highest], 0 to 127")),
                                }
                            }
                            "root" => zone.root = number(127).ok_or_else(|| fail("\"root\" is a key, 0 to 127"))?,
                            "volume" => zone.volume = number(128).ok_or_else(|| fail("\"volume\" is 0 to 128"))?,
                            "loop" => zone.looped = Some(value.as_bool().ok_or_else(|| fail("\"loop\" is true or false"))?),
                            "like" => instrument.like = Some(number(127).ok_or_else(|| fail("\"like\" is a program, 0 to 127"))?),
                            other => return Err(fail(&format!("unknown field \"{other}\""))),
                        }
                    }
                    fields.get("file").and_then(Value::as_str).ok_or_else(|| fail("no \"file\""))?.to_string()
                }
                _ => return Err(line.fail(usage)),
            };
            let inside = Path::new(&file)
                .components()
                .all(|part| matches!(part, std::path::Component::Normal(_) | std::path::Component::CurDir));
            if !inside {
                return Err(fail(&format!("{file} is not a path inside the mod's folder")));
            }
            zone.path = file.split(['/', '\\']).fold(mod_dir.to_path_buf(), |path, part| path.join(part));
            if !(zone.path.is_file() && suffix_is(&zone.path, ".wav")) {
                return Err(fail(&format!("{file} is not a WAV file in the mod's folder")));
            }
            instrument.zones.push(zone);
        }
        if instrument.zones.is_empty() {
            return Err(line.fail(format!("instrument {number}: no samples")));
        }
        out.push(instrument);
    }
    Ok(out)
}

fn line(mod_dir: &Path, module: &str, key: &str, value: &Value, add: bool) -> Result<Line> {
    let mut line = Line {
        module: module.to_string(),
        key: key.to_string(),
        shown: String::new(),
        file: PathBuf::new(),
        only: Vec::new(),
        volume: None,
        bank: None,
        add,
        like: None,
        title: None,
        script: None,
        programs: Vec::new(),
        instruments: Vec::new(),
    };
    let kind = ids::kind_of(key);
    if kind.is_none() && !line.is_file() && !line.is_map() && !line.is_data() {
        return Err(line.fail(
            "a key is an asset id (texture:, model:, sound:, bank:, text:, music:), file: and a disc path, map: and a map's name, \
             or data: and a table (data:items, data:monsters, ...)",
        ));
    }
    let options = match value {
        Value::String(file) => {
            line.shown = file.clone();
            serde_json::Map::new()
        }
        Value::Object(options) => options.clone(),
        _ => return Err(line.fail("give a file of the mod, or {\"file\": ..., options}")),
    };
    for (option, value) in &options {
        match option.as_str() {
            "file" => line.shown = value.as_str().ok_or_else(|| line.fail("\"file\" is a path"))?.to_string(),
            "only" if !add && !line.is_file() && !line.is_data() => {
                line.only = match value {
                    Value::String(place) => vec![place.clone()],
                    Value::Array(places) => places
                        .iter()
                        .map(|place| place.as_str().map(str::to_string))
                        .collect::<Option<_>>()
                        .ok_or_else(|| line.fail("\"only\" lists disc paths"))?,
                    _ => return Err(line.fail("\"only\" lists disc paths")),
                };
                line.only =
                    line.only.iter().map(|place| place.replace('\\', "/").trim_matches('/').to_string()).collect();
            }
            "volume" if kind == Some(Kind::Sound) => {
                let volume = value.as_i64().filter(|volume| (0..=127).contains(volume));
                line.volume = Some(volume.ok_or_else(|| line.fail("\"volume\" is 0 to 127"))?);
            }
            "instruments" if !add && kind == Some(Kind::Music) => {
                line.instruments = instruments(mod_dir, &line, value)?;
            }
            "bank" if add && kind == Some(Kind::Sound) => {
                let bank = value.as_str().ok_or_else(|| line.fail("\"bank\" is a bank's id"))?;
                line.bank = Some(if bank.starts_with("bank:") { bank.to_string() } else { Kind::Bank.id(bank) });
            }
            "like" if line.is_file() => {
                let like = value.as_str().ok_or_else(|| line.fail("\"like\" is an asset id or a disc path"))?;
                line.like = Some(like.replace('\\', "/").trim_matches('/').to_string());
            }
            "like" if line.is_map() && add => {
                let like = value.as_str().ok_or_else(|| line.fail("\"like\" is the map the new one is made from"))?;
                line.like = Some(like.trim_start_matches(MAP).to_string());
            }
            "title" if line.is_map() && add => {
                let title = value
                    .as_str()
                    .filter(|title| title.chars().all(|c| (' '..='~').contains(&c)) && !title.contains('"'));
                let title = title.ok_or_else(|| line.fail("\"title\" is text of printable ASCII without quotes"))?;
                line.title = Some(title.to_string());
            }
            "script" if line.is_map() && add => {
                let script = value.as_str().filter(|script| script.to_lowercase().ends_with(".c"));
                let script =
                    script.ok_or_else(|| line.fail("\"script\" is a C file of the mod: the new map's event script"))?;
                let inside = Path::new(script)
                    .components()
                    .all(|part| matches!(part, std::path::Component::Normal(_) | std::path::Component::CurDir));
                let path = script.split(['/', '\\']).fold(mod_dir.to_path_buf(), |path, part| path.join(part));
                if !inside || !path.is_file() {
                    return Err(line.fail(format!("{script} is not a file in the mod's folder")));
                }
                line.script = Some(path);
            }
            "programs" if line.is_map() => {
                let listed = value
                    .as_object()
                    .ok_or_else(|| line.fail("\"programs\" gives event points programs: {\"door_park\": \"ENTER_PARK\"}"))?;
                for (point, program) in listed {
                    if program.as_i64().is_none() && !program.as_str().is_some_and(is_identifier) {
                        return Err(line.fail(format!("\"programs\": {point}'s program is a number or a program's name")));
                    }
                    line.programs.push((point.clone(), program.clone()));
                }
            }
            _ => return Err(line.fail(format!("unknown option \"{option}\""))),
        }
    }
    if line.shown.is_empty() {
        return Err(line.fail("no \"file\""));
    }
    let relative = Path::new(&line.shown);
    let inside = relative
        .components()
        .all(|part| matches!(part, std::path::Component::Normal(_) | std::path::Component::CurDir));
    if !inside {
        return Err(line.fail(format!("{} is not a path inside the mod's folder", line.shown)));
    }
    line.file = line.shown.split(['/', '\\']).fold(mod_dir.to_path_buf(), |path, part| path.join(part));
    if !line.file.exists() {
        return Err(line.fail(format!("{} is not in the mod's folder", line.shown)));
    }
    if line.is_data() {
        if add || !DATA_TABLES.contains(&line.data_table().as_str()) {
            return Err(line.fail(format!("a data: line replaces one of the tables {}", DATA_TABLES.map(|table| format!("data:{table}")).join(", "))));
        }
        if !(line.file.is_file() && suffix_is(&line.file, ".json")) {
            return Err(line.fail(format!("{} is not a .json file (mod asset ISO NAME {} writes one)", line.shown, line.key)));
        }
        return Ok(line);
    }
    if line.is_map() {
        let name = line.map_name();
        if !add && (name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')) {
            return Err(line.fail("a map: line replaces a map, named as the game names it (map:s17)"));
        }
        if add {
            let fits = (1..=MAP_NAME_MOST).contains(&name.len())
                && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
                && name.starts_with(|c: char| c.is_ascii_lowercase());
            if !fits {
                return Err(line.fail(format!(
                    "a new map is named in lower-case letters, digits and _, from a letter, at most {MAP_NAME_MOST} long (map:m05x)"
                )));
            }
            let like = line.like.as_deref().unwrap_or_default();
            if like.is_empty() || !like.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err(line.fail("a new map names the map it is made from: {\"file\": ..., \"like\": \"m05\"}"));
            }
        }
        if !(line.file.is_file() && suffix_is(&line.file, ".gltf")) {
            return Err(line.fail(format!("{} is not a .gltf file (c2 map export writes one)", line.shown)));
        }
        return Ok(line);
    }
    if add {
        match kind {
            _ if line.is_file() => {}
            Some(Kind::Sound) => {
                let name = &key[Kind::Sound.name().len() + 1..];
                if !is_identifier(name) {
                    return Err(line.fail("an added sound is named as a C constant (sound:SE_MOD_BEEP)"));
                }
                if line.bank.is_none() {
                    return Err(line.fail("say which bank the sound goes into: {\"file\": ..., \"bank\": \"SP_004\"}"));
                }
            }
            _ => return Err(line.fail("add makes sound entries (sound:NAME with a bank) and files (file:)")),
        }
    }
    let wanted = match kind {
        _ if line.is_file() => None,
        Some(Kind::Texture) if !(line.file.is_file() && suffix_is(&line.file, ".png")) => Some("a PNG file"),
        Some(Kind::Model) if !(line.file.is_dir() || suffix_is(&line.file, ".gltf")) => {
            Some("a .gltf file with its buffer beside it, or a folder as mod asset --as gltf writes it")
        }
        Some(Kind::Sound) if !(line.file.is_file() && suffix_is(&line.file, ".wav")) => Some("a WAV file"),
        Some(Kind::Bank) if !line.file.is_dir() => Some("a folder of WAVs named by entry number (004.wav)"),
        Some(Kind::Text) if !(line.file.is_file() && suffix_is(&line.file, ".txt")) => {
            Some("a .txt file of messages (mod asset text:ID writes one)")
        }
        Some(Kind::Music) if !(line.file.is_file() && MIDI_SUFFIXES.iter().any(|end| suffix_is(&line.file, end))) => {
            Some("a MIDI file (.mid)")
        }
        _ => None,
    };
    if let Some(wanted) = wanted {
        return Err(line.fail(format!("{} is not {wanted}", line.shown)));
    }
    if ids::is_pattern(key) && (add || line.is_file()) {
        return Err(line.fail("patterns are for replacing ids"));
    }
    if let Some(like) = &line.like {
        check_like(&line, like)?;
    }
    Ok(line)
}

/// What a file: line made from a common format makes: a texture file (.img, of PNGs, its template
/// a texture) or a character pack (.chr, of a glTF, its template a model).
fn made_kind(path: &str) -> Option<Kind> {
    let path = path.to_lowercase();
    if path.ends_with(".img") {
        Some(Kind::Texture)
    } else if path.ends_with(".chr") {
        Some(Kind::Model)
    } else {
        None
    }
}

fn check_like(line: &Line, like: &str) -> Result<()> {
    let Some(made) = made_kind(&line.disc_path()) else {
        return Err(line.fail("\"like\" makes texture files (.img) and character packs (.chr)"));
    };
    let template = match ids::kind_of(like) {
        Some(kind) if ids::is_pattern(like) => return Err(line.fail(format!("\"like\" names one {}", kind.name()))),
        Some(kind) => Some(kind),
        None => made_kind(like),
    };
    if template != Some(made) {
        let wanted = match made {
            Kind::Texture => "a texture (texture:NAME) or a texture file's disc path (.img)",
            _ => "a model (model:NAME) or a character pack's disc path (.chr)",
        };
        return Err(line.fail(format!("\"like\" names {wanted}")));
    }
    let fits = match made {
        Kind::Texture => line.file.is_dir() || suffix_is(&line.file, ".png"),
        _ => line.file.is_dir() || suffix_is(&line.file, ".gltf") || suffix_is(&line.file, ".glb"),
    };
    if !fits {
        let wanted = match made {
            Kind::Texture => "a PNG file or a folder of PNGs (one picture each, named by its file name)",
            _ => "a .gltf file (its buffer and images beside it), a .glb, or a folder holding one",
        };
        return Err(line.fail(format!("{} is not {wanted}", line.shown)));
    }
    Ok(())
}

/// Each file: line's template, {disc path, lowercased: where the template is}, from the disc's
/// index of ids for a "like" that names an id (index is None when no line does).
pub fn templates(lines: &[Line], index: Option<&Index>) -> Result<Templates> {
    let mut out = Templates::new();
    for line in lines {
        let Some(like) = line.like.as_ref().filter(|_| line.is_file()) else {
            continue;
        };
        let template = match ids::kind_of(like) {
            Some(_) => {
                let Some(asset) = index.and_then(|index| index.get(like)) else {
                    return Err(line.fail(format!("the disc has no asset {like}")));
                };
                let (place, picture) = asset.places[0].clone();
                Template { place, picture, name: asset.name.clone() }
            }
            None => {
                let (path, members) = discfiles::location(like);
                let last = members.last().unwrap_or(&path);
                let name = last.rsplit('/').next().unwrap_or(last);
                let name = name.rsplit_once('.').map_or(name, |(stem, _)| stem).to_string();
                Template { place: Place { path, members }, picture: None, name }
            }
        };
        out.insert(line.disc_path().to_lowercase(), template);
    }
    Ok(out)
}

/// The chosen mods' lines, in build order.
pub fn all_lines(mod_dirs: &[PathBuf]) -> Result<Vec<Line>> {
    let mut out = Vec::new();
    for mod_dir in mod_dirs {
        out.extend(lines(mod_dir)?);
    }
    Ok(out)
}

/// The mod's file: lines as discfiles takes them: {disc path: file}.
pub fn raw_files(mod_dir: &Path) -> Result<Vec<(String, PathBuf)>> {
    Ok(lines(mod_dir)?.into_iter().filter(Line::is_file).map(|line| (line.disc_path(), line.file)).collect())
}

/// Whether any of the lines names an id, so that the build needs the disc's index.
pub fn needs_index(lines: &[Line]) -> bool {
    lines.iter().any(|line| (!line.is_file() && !line.is_map() && !line.is_data()) || line.like.as_deref().is_some_and(|like| ids::kind_of(like).is_some()))
}

/// What one line does at one place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// The model built from a glTF file, or a folder as the export writes it.
    Model(PathBuf),
    /// A picture of the texture file, by its entry.
    Picture(usize, PathBuf),
    /// An entry of the bank, replaced or added.
    Entry(Given),
    /// The messages of a text file, with the place its text was exported from (the id's first).
    Text(PathBuf, Place),
    /// A sequence (.sq member of a bank) built from a MIDI file, with instruments it adds to the
    /// bank.
    Sequence(PathBuf, Vec<music::Instrument>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub id: String,
    pub module: String,
    pub key: String,
    /// The asset's name (a model's pack name, which models::build reads its cfg by).
    pub name: String,
    pub place: Place,
    pub change: Change,
}

impl Edit {
    fn slot(&self) -> (String, Option<usize>) {
        let at = match &self.change {
            Change::Model(_) | Change::Text(..) | Change::Sequence(..) => None,
            Change::Picture(entry, _) => Some(*entry),
            Change::Entry(given) => Some(given.number),
        };
        (self.place.display().to_lowercase(), at)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Resolved {
    pub edits: Vec<Edit>,
    /// The entries the mods add, {mod: {name: entry number}}: constants their patches see.
    pub added: BTreeMap<String, IndexMap<String, i64>>,
    /// The ids each line reaches: (mod, key, ids).
    pub reached: Vec<(String, String, Vec<String>)>,
}

/// Whether a disc path given in "only" covers a place: the place itself, or a folder or pack it is
/// in, ignoring case; * and ? match as in ids.
pub fn covers(only: &str, place: &Place) -> bool {
    let shown = place.display().to_lowercase();
    let only = only.to_lowercase();
    if ids::is_pattern(&only) {
        return ids::glob_matches(&only, &shown);
    }
    shown == only || shown.starts_with(&format!("{only}/")) || shown.starts_with(&format!("{only}.d/"))
}

/// The places and changes of the lines, by the disc's index. An id or pattern the disc does not
/// have, an "only" that keeps no place, or one entry given twice in a mod is refused.
pub fn resolve(lines: &[Line], index: &Index) -> Result<Resolved> {
    let mut resolved = Resolved::default();
    let mut modules: Vec<&str> = Vec::new();
    for line in lines {
        if !modules.contains(&line.module.as_str()) {
            modules.push(&line.module);
        }
    }
    let mut added_to: HashMap<String, usize> = HashMap::new();
    let at = index.at();
    for module in modules {
        let mine: Vec<&Line> =
            lines.iter().filter(|line| line.module == module && !line.is_file() && !line.is_map() && !line.is_data()).collect();
        // each id by the line that gives it: an exact id wins over a pattern
        let mut chosen: BTreeMap<String, (&Line, bool)> = BTreeMap::new();
        for line in mine.iter().filter(|line| !line.add) {
            let exact = !ids::is_pattern(&line.key);
            let kind = ids::kind_of(&line.key);
            let found: Vec<&ids::Asset> = if exact {
                index.get(&line.key).into_iter().collect()
            } else {
                index.matching(&line.key).into_iter().filter(|asset| Some(asset.kind) == kind).collect()
            };
            if found.is_empty() {
                return Err(line.fail(if exact {
                    "the disc has no asset of that id"
                } else {
                    "the pattern matches no asset of the disc"
                }));
            }
            for asset in found {
                match chosen.get(&asset.id) {
                    Some((_, true)) if !exact => {}
                    Some((other, other_exact)) if exact == *other_exact => {
                        return Err(line.fail(format!("{} is given by {} too", asset.id, other.key)));
                    }
                    _ => {
                        chosen.insert(asset.id.clone(), (line, exact));
                    }
                }
            }
        }
        let mut reached: IndexMap<&str, Vec<String>> =
            mine.iter().filter(|line| !line.add).map(|line| (line.key.as_str(), Vec::new())).collect();
        for (id, (line, _)) in &chosen {
            reached.get_mut(line.key.as_str()).unwrap().push(id.clone());
            let asset = &index.assets[id];
            let places: Vec<&(Place, Option<usize>)> = asset
                .places
                .iter()
                .filter(|(place, _)| line.only.is_empty() || line.only.iter().any(|only| covers(only, place)))
                .collect();
            if places.is_empty() {
                let shown: Vec<String> = asset.places.iter().take(5).map(|(place, _)| place.display()).collect();
                return Err(line.fail(format!("\"only\" keeps none of the places of {id} ({})", shown.join(", "))));
            }
            let edit = |place: &Place, change: Change| Edit {
                id: id.clone(),
                module: module.to_string(),
                key: line.key.clone(),
                name: asset.name.clone(),
                place: place.clone(),
                change,
            };
            match asset.kind {
                Kind::Texture => {
                    for (place, at) in places {
                        resolved.edits.push(edit(place, Change::Picture(at.unwrap_or(0), line.file.clone())));
                    }
                }
                Kind::Model => {
                    for (place, _) in places {
                        resolved.edits.push(edit(place, Change::Model(line.file.clone())));
                    }
                }
                Kind::Text => {
                    let reference = asset.places[0].0.clone();
                    for (place, _) in places {
                        resolved.edits.push(edit(place, Change::Text(line.file.clone(), reference.clone())));
                    }
                }
                Kind::Music => {
                    for (place, _) in places {
                        resolved.edits.push(edit(place, Change::Sequence(line.file.clone(), line.instruments.clone())));
                    }
                }
                Kind::Sound => {
                    for (place, at) in places {
                        let given =
                            Given { number: at.unwrap_or(0), name: None, path: line.file.clone(), volume: line.volume };
                        resolved.edits.push(edit(place, Change::Entry(given)));
                    }
                }
                Kind::Bank => {
                    let count = asset.entries.unwrap_or(0);
                    let wavs = sounds::numbered(&line.file).map_err(|error| line.fail(error))?;
                    if let Some((number, _, path)) = wavs.iter().find(|(number, _, _)| *number >= count) {
                        return Err(line.fail(format!(
                            "{}: the bank has entries 0 to {}; add new entries with \"add\" (entry {number})",
                            name_of(path),
                            count as i64 - 1
                        )));
                    }
                    // each entry at every place its sound has: copies of the bank in other packs
                    // whose entry plays alike are the same sound (tree::bank_sounds)
                    let mut done: Vec<(String, usize)> = Vec::new();
                    for (place, _) in places {
                        for (number, _, path) in &wavs {
                            let key = (Kind::Sound, place.display().to_lowercase(), Some(*number));
                            let reached: Vec<Place> = match at.get(&key).and_then(|id| index.assets.get(id)) {
                                Some(sound) => sound
                                    .places
                                    .iter()
                                    .filter(|(_, entry)| *entry == Some(*number))
                                    .map(|(place, _)| place.clone())
                                    .collect(),
                                None => vec![place.clone()],
                            };
                            for place in reached {
                                if done.contains(&(place.display().to_lowercase(), *number)) {
                                    continue;
                                }
                                done.push((place.display().to_lowercase(), *number));
                                let given = Given { number: *number, name: None, path: path.clone(), volume: None };
                                resolved.edits.push(edit(&place, Change::Entry(given)));
                            }
                        }
                    }
                }
            }
        }
        for line in mine.iter().filter(|line| line.add) {
            if let Some(found) = index.get(&line.key) {
                return Err(line.fail(format!("the disc has {} already; replace it instead", found.id)));
            }
            let bank_id = line.bank.clone().unwrap_or_default();
            let Some(bank) = index.get(&bank_id).filter(|bank| bank.kind == Kind::Bank) else {
                return Err(line.fail(format!("the disc has no {bank_id}")));
            };
            let taken = added_to.entry(bank.id.clone()).or_default();
            let number = bank.entries.unwrap_or(0) + *taken;
            *taken += 1;
            let name = line.key[Kind::Sound.name().len() + 1..].to_string();
            for (place, _) in &bank.places {
                let given = Given { number, name: Some(name.clone()), path: line.file.clone(), volume: line.volume };
                resolved.edits.push(Edit {
                    id: line.key.clone(),
                    module: module.to_string(),
                    key: line.key.clone(),
                    name: bank.name.clone(),
                    place: place.clone(),
                    change: Change::Entry(given),
                });
            }
            resolved.added.entry(module.to_string()).or_default().insert(name, number as i64);
            reached.insert(&line.key, vec![bank.id.clone()]);
        }
        // one entry of a bank given twice in a mod (by its bank's folder and by its id, say)
        let mut seen: HashMap<(String, Option<usize>), &Edit> = HashMap::new();
        for edit in resolved.edits.iter().filter(|edit| edit.module == module) {
            if let Some(other) = seen.insert(edit.slot(), edit) {
                if other.key != edit.key {
                    return Err(Error::invalid(format!(
                        "{} and {} (mod {module}) both give {} entry {}",
                        other.key,
                        edit.key,
                        edit.place.display(),
                        edit.slot().1.unwrap_or(0)
                    )));
                }
            }
        }
        resolved.reached.extend(reached.into_iter().map(|(key, ids)| (module.to_string(), key.to_string(), ids)));
    }
    Ok(resolved)
}

fn overlaps(a: &Place, b: &Place) -> bool {
    let lower = |place: &Place| -> Vec<String> {
        std::iter::once(place.path.to_lowercase())
            .chain(place.members.iter().map(|member| member.to_lowercase()))
            .collect()
    };
    let (a, b) = (lower(a), lower(b));
    let common = a.len().min(b.len());
    a[..common] == b[..common]
}

/// What the chosen mods cannot do together: one id replaced by two of them, one picture, sound
/// entry or model changed by two, or a change inside a game file another replaces as a whole (by
/// file: or disc/). raw holds the whole game files, from discfiles::collect.
pub fn clashes(resolved: &Resolved, raw: &Wanted) -> Vec<String> {
    let mut out = Vec::new();
    let mut by_id: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    // an added entry's name is its mod's own
    let added = |edit: &&Edit| matches!(&edit.change, Change::Entry(given) if given.name.is_some());
    for edit in resolved.edits.iter().filter(|edit| !added(edit)) {
        let modules = by_id.entry(&edit.id).or_default();
        if !modules.contains(&edit.module.as_str()) {
            modules.push(&edit.module);
        }
    }
    for (id, modules) in &by_id {
        if modules.len() > 1 {
            out.push(format!("{id} is replaced by more than one chosen mod: {}", modules.join(", ")));
        }
    }
    let mut by_slot: BTreeMap<(String, Option<usize>), Vec<&Edit>> = BTreeMap::new();
    for edit in &resolved.edits {
        by_slot.entry(edit.slot()).or_default().push(edit);
    }
    for ((_, at), edits) in &by_slot {
        let first = edits[0];
        if let Some(other) = edits.iter().find(|edit| edit.module != first.module && edit.id != first.id) {
            let what = match (&first.change, at) {
                (Change::Picture(..), Some(at)) => format!(" picture {at}"),
                (Change::Entry(_), Some(at)) => format!(" entry {at}"),
                _ => String::new(),
            };
            out.push(format!(
                "{} (mod {}) and {} (mod {}) both change {}{what}",
                first.id,
                first.module,
                other.id,
                other.module,
                first.place.display()
            ));
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    for (path, (module, _)) in raw {
        let (top, members) = discfiles::location(path);
        let place = Place { path: top, members };
        for edit in resolved.edits.iter().filter(|edit| edit.module != *module && overlaps(&place, &edit.place)) {
            if seen.insert((path.clone(), edit.id.clone())) {
                out.push(format!(
                    "{path} (mod {module}) and {} (mod {}) both change {}",
                    edit.id,
                    edit.module,
                    if place.members.len() >= edit.place.members.len() {
                        place.display()
                    } else {
                        edit.place.display()
                    }
                ));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// A game file or member the id lines changed: the place, the ids and mods that changed it, its
/// old size and its bytes.
#[derive(Clone, Debug)]
pub struct Changed {
    pub place: Place,
    pub ids: Vec<String>,
    pub modules: Vec<String>,
    pub old: usize,
    pub data: Vec<u8>,
    /// What the build says of it, each line "warning: ..." or "note: ...": a text's lines wider than
    /// the game's windows take, the instruments a tune added and the memory they take.
    pub warnings: Vec<String>,
}

/// The model a line gives, built with the disc's pack as the template.
pub fn built_model(source: &Path, pack: &[u8], name: &str, temp: &Path) -> Result<Vec<u8>> {
    if source.is_dir() {
        let stem = tree::model_stem(pack, name)?;
        if source.join(format!("{stem}.gltf")).is_file() {
            return Ok(tree::build_model(source, pack, name)?);
        }
        let gltfs: Vec<PathBuf> = fs::read_dir(source)?
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_file() && suffix_is(path, ".gltf"))
            .collect();
        let [gltf] = gltfs.as_slice() else {
            return Err(Error::invalid(format!(
                "{}: no {stem}.gltf (as the export names it), and not one other .gltf",
                source.display()
            )));
        };
        return built_model(gltf, pack, name, temp);
    }
    let gltf = fs::read(source)?;
    let bin = tree::gltf_buffer(source, &gltf)?;
    Ok(tree::build_model_from(&gltf, &bin, pack, name, temp)?)
}

/// Puts the id lines' changes into the disc files build writes, on top of files (the scripts and
/// whole game files already put in). Returns the files and every place changed.
pub fn apply(iso_path: &Path, resolved: &Resolved, files: &Files) -> Result<(Files, Vec<Changed>)> {
    let image = Image::open(iso_path)?;
    let disc = Archive::open(image.bytes())?;
    let index = discfiles::disc_index(&disc);
    let mut files = files.clone();
    // models, then banks, then pictures, then texts, then the sequences inside banks; outer places
    // first
    let stage = |edit: &Edit| match edit.change {
        Change::Model(_) => 0,
        Change::Entry(_) => 1,
        Change::Picture(..) => 2,
        Change::Text(..) => 3,
        Change::Sequence(..) => 4,
    };
    let fonts = discfiles::disc_fonts(&disc, &image, &index);
    let mut groups: BTreeMap<(usize, usize, String), Vec<&Edit>> = BTreeMap::new();
    for edit in &resolved.edits {
        let key = (stage(edit), edit.place.members.len(), edit.place.display().to_lowercase());
        groups.entry(key).or_default().push(edit);
    }
    static BUILDS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let build = BUILDS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let temp = std::env::temp_dir().join(format!("c2-build-model-{}-{build}", std::process::id()));
    let mut models: Vec<(String, Vec<u8>, Vec<u8>)> = Vec::new();
    let mut touched: IndexMap<String, Changed> = IndexMap::new();
    let mut warnings: Vec<(String, String)> = Vec::new();
    // whether an English text uses the accented letters the build adds to the English font
    let mut english_extra = false;
    for ((stage, _, shown), edits) in groups {
        let first = edits[0];
        let place = &first.place;
        let fail = |error: Error| {
            Error::invalid(format!("{} (mod {}) at {}: {error}", first.key, first.module, place.display()))
        };
        let Some(entry) = index.get(&place.path.to_lowercase()) else {
            return Err(fail(Error::invalid(format!("{} is not a file on the disc", place.path))));
        };
        let mut data = match files.get(&entry.path) {
            Some(data) => data.clone(),
            None => disc.read(image.bytes(), entry).to_vec(),
        };
        let Some(old) = rebuild::read_member(&data, &place.members).map(<[u8]>::to_vec) else {
            return Err(fail(Error::invalid("the pack has no such member".to_string())));
        };
        let new = match stage {
            0 => {
                let Change::Model(source) = &first.change else { unreachable!() };
                match models.iter().find(|(id, from, _)| *id == first.id && *from == old) {
                    Some((_, _, built)) => built.clone(),
                    None => {
                        let built = built_model(source, &old, &first.name, &temp).map_err(fail)?;
                        models.push((first.id.clone(), old.clone(), built.clone()));
                        built
                    }
                }
            }
            3 => {
                let Change::Text(source, reference) = &first.change else { unreachable!() };
                let written = c2_py::read_text(source)?;
                let original = index
                    .get(&reference.path.to_lowercase())
                    .and_then(|entry| rebuild::read_member(disc.read(image.bytes(), entry), &reference.members))
                    .ok_or_else(|| fail(Error::invalid(format!("{} is not on the disc", reference.display()))))?;
                let built =
                    text::build(&written, &old, original, &place.display(), &fonts).map_err(|error| fail(error.into()))?;
                let widths =
                    text::width_warnings(&written, original, &place.display(), &fonts).map_err(|error| fail(error.into()))?;
                warnings.extend(widths.into_iter().map(|warning| (shown.clone(), format!("warning: {warning}"))));
                let english = text::locate(&place.display()).is_some_and(|found| found.language == text::Language::English);
                if english && built != old {
                    // a debug menu's leftover Japanese has glyph numbers there already: only the
                    // messages the mod changed count
                    let before = text::Mes::parse(&old).map_err(|error| fail(error.into()))?;
                    let mes = text::Mes::parse(&built).map_err(|error| fail(error.into()))?;
                    english_extra |= mes
                        .messages()
                        .any(|(id, codes)| before.get(id) != Some(codes) && font::uses_english_extra(codes.iter().copied()));
                }
                built
            }
            4 => {
                let Change::Sequence(source, instruments) = &first.change else { unreachable!() };
                // the bank holding the sequence, for its programs
                let Some((sequence, bank_members)) = place.members.split_last() else {
                    return Err(fail(Error::invalid("a sequence is a member of a bank".to_string())));
                };
                let bank_data = rebuild::read_member(&data, bank_members);
                if instruments.is_empty() || bank_data.is_none() {
                    let bank = bank_data.map(Bank::from_bytes).transpose().map_err(|error| fail(error.into()))?;
                    music::build(&fs::read(source)?, &old, bank.as_ref()).map_err(|error| fail(error.into()))?
                } else {
                    // the bank gains the instruments, then the sequence goes into it
                    let bank_path = bank_members.last().unwrap_or(&place.path);
                    let bank_name = bank_path.rsplit('/').next().unwrap_or(bank_path).to_uppercase();
                    let region = bank_name.starts_with("BG_").then_some(music::MUSIC_REGION);
                    let (bank, notes) = music::build_with_instruments(
                        &fs::read(source)?,
                        bank_data.unwrap_or_default(),
                        sequence,
                        instruments,
                        region,
                    )
                    .map_err(|error| fail(error.into()))?;
                    warnings.extend(notes.into_iter().map(|note| (shown.clone(), format!("note: {note}"))));
                    data = rebuild::replaced_member(&data, bank_members, &bank)?;
                    rebuild::read_member(&data, &place.members).map(<[u8]>::to_vec).unwrap_or_default()
                }
            }
            1 => {
                let mut given: Vec<Given> = edits
                    .iter()
                    .filter_map(|edit| if let Change::Entry(given) = &edit.change { Some(given.clone()) } else { None })
                    .collect();
                given.sort_by_key(|given| given.number);
                sounds::build_entries(&given, &old).map_err(|error| fail(error.into()))?
            }
            _ => {
                let mut images = im3::read(&old).map_err(|error| fail(error.into()))?;
                for edit in &edits {
                    let Change::Picture(at, png) = &edit.change else { unreachable!() };
                    let Some(position) = images.iter().position(|image| image.index == Some(*at)) else {
                        return Err(fail(Error::invalid(format!("no picture {at}"))));
                    };
                    let picture = im3::from_png(&fs::read(png)?, &images[position]).map_err(|error| {
                        Error::invalid(format!("{} (mod {}): {}: {error}", edit.key, edit.module, name_of(png)))
                    })?;
                    images[position] = picture;
                }
                im3::write(&images, Some(&old)).map_err(|error| fail(error.into()))?
            }
        };
        files.insert(entry.path.clone(), rebuild::replaced_member(&data, &place.members, &new)?);
        let changed = touched.entry(shown).or_insert_with(|| Changed {
            place: Place { path: entry.path.clone(), members: place.members.clone() },
            ids: Vec::new(),
            modules: Vec::new(),
            old: old.len(),
            data: Vec::new(),
            warnings: Vec::new(),
        });
        for edit in edits {
            if !changed.ids.contains(&edit.id) {
                changed.ids.push(edit.id.clone());
            }
            if !changed.modules.contains(&edit.module) {
                changed.modules.push(edit.module.clone());
            }
        }
    }
    if english_extra {
        // the English font texture with the accented letters drawn in (c2_assets::font)
        let read = |path: &str| {
            index.get(&path.to_lowercase()).map(|entry| (entry.path.clone(), disc.read(image.bytes(), entry).to_vec()))
        };
        let (Some((path, english)), Some((_, european))) = (read(font::ENGLISH_TEXTURE), read(font::EUROPEAN_TEXTURE))
        else {
            return Err(Error::invalid(format!(
                "an English text uses accented letters, but the disc has no {} or {} to draw them from",
                font::ENGLISH_TEXTURE,
                font::EUROPEAN_TEXTURE
            )));
        };
        let english = files.get(&path).cloned().unwrap_or(english);
        let extended = font::extended_english(&english, &european)?;
        let ids: Vec<String> = touched
            .values()
            .filter(|changed| {
                text::locate(&changed.place.display()).is_some_and(|found| found.language == text::Language::English)
            })
            .flat_map(|changed| changed.ids.clone())
            .collect();
        let modules: Vec<String> = touched.values().flat_map(|changed| changed.modules.clone()).collect();
        touched.insert(
            path.to_lowercase(),
            Changed {
                place: Place { path: path.clone(), members: Vec::new() },
                ids: vec![format!("the English font's accented letters, for {}", ids.join(", "))],
                modules: modules.into_iter().fold(Vec::new(), |mut found, module| {
                    if !found.contains(&module) {
                        found.push(module);
                    }
                    found
                }),
                old: english.len(),
                data: Vec::new(),
                warnings: Vec::new(),
            },
        );
        files.insert(path, extended);
    }
    for (shown, warning) in warnings {
        if let Some(changed) = touched.get_mut(&shown) {
            changed.warnings.push(warning);
        }
    }
    let mut out: Vec<Changed> = touched.into_values().collect();
    for changed in &mut out {
        let top = &files[&changed.place.path];
        changed.data = rebuild::read_member(top, &changed.place.members).unwrap_or_default().to_vec();
    }
    out.sort_by_key(|changed| changed.place.display().to_lowercase());
    Ok((files, out))
}

/// The changed places that read back out of a built image differently.
pub fn check(out_path: &Path, changed: &[Changed]) -> Result<Vec<String>> {
    let image = Image::open(out_path)?;
    let disc = Archive::open(image.bytes())?;
    let index = discfiles::disc_index(&disc);
    Ok(changed
        .iter()
        .filter(|item| {
            let found = index
                .get(&item.place.path.to_lowercase())
                .and_then(|entry| rebuild::read_member(disc.read(image.bytes(), entry), &item.place.members));
            found != Some(&item.data[..])
        })
        .map(|item| item.place.display())
        .collect())
}

/// Adds lines to the mod's replace, keeping the rest of mod.json as it is. A key it has already
/// (ignoring case) is refused.
pub fn add_lines(mod_dir: &Path, new: &[(String, Value)]) -> Result<()> {
    let path = mod_dir.join(mods::INDEX);
    let mut body: serde_json::Map<String, Value> = if path.is_file() {
        match serde_json::from_str(&c2_py::read_text(&path)?) {
            Ok(Value::Object(body)) => body,
            _ => return Err(Error::invalid(format!("{} is not a JSON object", path.display()))),
        }
    } else {
        serde_json::Map::new()
    };
    let mut listed = match body.remove(REPLACE) {
        Some(Value::Object(listed)) => listed,
        None => serde_json::Map::new(),
        Some(_) => return Err(Error::invalid(format!("\"{REPLACE}\" in {} is not an object", path.display()))),
    };
    for (key, value) in new {
        if listed.keys().any(|known| known.to_lowercase() == key.to_lowercase()) {
            return Err(Error::invalid(format!("{} has a line for {key} already", mods::INDEX)));
        }
        listed.insert(key.clone(), value.clone());
    }
    body.insert(REPLACE.to_string(), Value::Object(listed));
    fs::create_dir_all(mod_dir)?;
    let text = c2_py::json::dumps(&Value::Object(body), &c2_py::json::Options::indented(1).unicode()) + "\n";
    c2_py::write_text(&path, &text)?;
    Ok(())
}

/// Copies assets off the disc into the mod by id (or pattern), each in its common format where
/// the mod's line will find it: textures/<name>.png, models/<name>/ (as the export writes it),
/// sounds/<bank>/<NNN_NAME>.wav, sounds/<bank>/, text/<language>/<name>.txt; and adds each one's
/// line to mod.json. An id the mod has a line for already is left as it is. Returns (id, the file
/// in the mod, whether it was written).
pub fn copy_out(
    iso_path: &Path,
    mod_dir: &Path,
    keys: &[String],
    index: &Index,
) -> Result<Vec<(String, String, bool)>> {
    let image = Image::open(iso_path)?;
    let disc = Archive::open(image.bytes())?;
    let disc_index = discfiles::disc_index(&disc);
    let fonts = discfiles::disc_fonts(&disc, &image, &disc_index);
    let known: Vec<String> = if mod_dir.join(mods::INDEX).is_file() {
        lines(mod_dir)?.into_iter().map(|line| line.key.to_lowercase()).collect()
    } else {
        Vec::new()
    };
    let mut new_lines = Vec::new();
    let mut out = Vec::new();
    for key in keys {
        let found: Vec<&ids::Asset> =
            if ids::is_pattern(key) { index.matching(key) } else { index.get(key).into_iter().collect() };
        if found.is_empty() {
            return Err(Error::invalid(format!("{key}: the disc has no asset of that id")));
        }
        for asset in found {
            let stem = tree::file_stem(asset.id.split_once(':').map_or("", |(_, name)| name));
            let (place, at) = &asset.places[0];
            let data = disc_index
                .get(&place.path.to_lowercase())
                .and_then(|entry| rebuild::read_member(disc.read(image.bytes(), entry), &place.members))
                .ok_or_else(|| Error::invalid(format!("{}: {} is not on the disc", asset.id, place.display())))?;
            let bank_stem = |bank: &str| tree::file_stem(bank.split_once(':').map_or(bank, |(_, name)| name));
            let (target, files): (String, Vec<(String, Vec<u8>)>) = match asset.kind {
                Kind::Texture => {
                    let images = im3::read(data)?;
                    let image = images
                        .iter()
                        .find(|image| image.index == *at)
                        .ok_or_else(|| Error::invalid(format!("{}: no picture at {}", asset.id, place.display())))?;
                    let target = format!("textures/{stem}.png");
                    (target.clone(), vec![(target, im3::to_png(image, true)?)])
                }
                Kind::Model => {
                    let target = format!("models/{stem}");
                    let files = tree::exported_model(data, &asset.name)?;
                    (target.clone(), files.into_iter().map(|(name, data)| (format!("{target}/{name}"), data)).collect())
                }
                Kind::Bank => {
                    let target = format!("sounds/{stem}");
                    let files = sounds::export(data)?;
                    (target.clone(), files.into_iter().map(|(name, data)| (format!("{target}/{name}"), data)).collect())
                }
                Kind::Text => {
                    let target = text::text_file(&asset.id[Kind::Text.name().len() + 1..]);
                    let exported = text::export(data, &place.display(), &asset.id, &fonts)?;
                    (target.clone(), vec![(target, exported.into_bytes())])
                }
                Kind::Sound => {
                    let bank = asset.bank.as_deref().unwrap_or_default();
                    let number = at.unwrap_or(0);
                    let wav = sounds::export(data)?
                        .into_iter()
                        .find(|(name, _)| *name == sounds::file_name(number))
                        .ok_or_else(|| {
                            Error::invalid(format!(
                                "{} plays no sample (a sequence, or nothing), so it has no WAV",
                                asset.id
                            ))
                        })?;
                    let name = asset.id[Kind::Sound.name().len() + 1..].to_string();
                    let named = (!name.contains('/')).then_some(name.as_str());
                    let bank_name = &bank[bank.find(':').map_or(0, |at| at + 1)..];
                    let target = format!("sounds/{}/{}", bank_stem(bank), tree::wav_name(number, named, bank_name));
                    (target.clone(), vec![(target, wav.1)])
                }
                Kind::Music => {
                    // the MIDI file, and its bank's programs as a SoundFont beside it to hear it with
                    let target = format!("music/{stem}{}", music::SUFFIX);
                    let mut files = vec![(target.clone(), music::export(data, &asset.name)?)];
                    if let Some((_, members)) = place.members.split_last() {
                        let top = disc_index.get(&place.path.to_lowercase());
                        let bank = top.and_then(|top| rebuild::read_member(disc.read(image.bytes(), top), members));
                        let file = members.last().unwrap_or(&place.path);
                        let file = file.rsplit('/').next().unwrap_or(file);
                        let name = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
                        if let Some(bank) = bank {
                            let font = soundfont::write(&Bank::from_bytes(bank)?, name)?;
                            files.push((format!("music/{}{}", tree::file_stem(name), soundfont::SUFFIX), font));
                        }
                    }
                    (target, files)
                }
            };
            if known.contains(&asset.id.to_lowercase()) || new_lines.iter().any(|(id, _)| id == &asset.id) {
                out.push((asset.id.clone(), target, false));
                continue;
            }
            let fresh = !joined(mod_dir, &target).exists();
            if fresh {
                for (path, content) in files {
                    let file = joined(mod_dir, &path);
                    fs::create_dir_all(file.parent().unwrap())?;
                    fs::write(file, content)?;
                }
            }
            new_lines.push((asset.id.clone(), Value::String(target.clone())));
            out.push((asset.id.clone(), target, fresh));
        }
    }
    add_lines(mod_dir, &new_lines)?;
    Ok(out)
}

fn joined(root: &Path, relative: &str) -> PathBuf {
    relative.split('/').fold(root.to_path_buf(), |path, part| path.join(part))
}

/// What a mod's lines say, for mod list and build: one line each.
pub fn described(mod_dir: &Path) -> Result<Vec<String>> {
    Ok(lines(mod_dir)?
        .iter()
        .map(|line| {
            let mut text = format!("{} {}: {}", if line.add { "adds" } else { "replaces" }, line.key, line.shown);
            if let Some(bank) = &line.bank {
                text.push_str(&format!(" (into {bank})"));
            }
            if let Some(like) = &line.like {
                text.push_str(&format!(" (like {like})"));
            }
            if !line.only.is_empty() {
                text.push_str(&format!(" (only {})", line.only.join(", ")));
            }
            text
        })
        .collect())
}

/// Ids two mods both replace, from their keys alone (without the disc, which patterns and
/// overlapping places need): {id: mods}.
pub fn key_clashes(mod_dirs: &[PathBuf]) -> Result<BTreeMap<String, Vec<String>>> {
    let mut owners: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for line in all_lines(mod_dirs)? {
        // data: tables merge field by field; build refuses two values for one field
        if line.is_file() || line.is_data() || ids::is_pattern(&line.key) {
            continue;
        }
        let modules = owners.entry(line.key.to_lowercase()).or_default();
        if !modules.contains(&line.module) {
            modules.push(line.module.clone());
        }
    }
    Ok(owners.into_iter().filter(|(_, modules)| modules.len() > 1).collect())
}

/// The constant a sound entry a mod's WAV gives is known by: SE_, the bank's name and the name
/// after the WAV's number, in capitals, as the game's sounds are named (`005_beep.wav` in bank
/// sp_004: SE_SP_004_BEEP). None for a WAV without a name (005.wav) or a name of no letters or
/// digits.
pub fn wav_constant(bank: &str, label: &str) -> Option<String> {
    crate::disc::identifier(label, &format!("SE_{}", bank.to_uppercase()))
}

/// The constants a mod's folders of WAVs give its patches: each WAV named after its number and a
/// name, in a bank's folder under disc/ or of a file: line (the folders that can add entries),
/// {name: entry number}. Two WAVs giving one name two numbers are refused.
pub fn wav_constants(mod_dir: &Path) -> Result<IndexMap<String, i64>> {
    let mut found: IndexMap<String, i64> = IndexMap::new();
    for (target, source) in discfiles::game_files(mod_dir)? {
        if !source.is_dir() || !target.to_lowercase().ends_with(".snd") {
            continue;
        }
        let file = target.rsplit('/').next().unwrap_or(&target);
        let bank = &file[..file.len() - 4];
        for (number, label, path) in sounds::numbered(&source)? {
            let Some(name) = label.as_deref().and_then(|label| wav_constant(bank, label)) else {
                continue;
            };
            match found.get(&name) {
                Some(&known) if known != number as i64 => {
                    return Err(Error::invalid(format!(
                        "{}: {} names entry {number} {name}, which another WAV gives entry {known}",
                        name_of(mod_dir),
                        name_of(&path)
                    )));
                }
                _ => {
                    found.insert(name, number as i64);
                }
            }
        }
    }
    Ok(found)
}

/// The sound constants each chosen mod's patches see, {mod: {name: entry number}}: the entries
/// its add lines make (added, from resolve) and the names of its WAVs (wav_constants). A name the
/// game's headers (consts) give another value is refused; one they give the same value is left to
/// them.
pub fn sound_constants(
    mod_dirs: &[PathBuf],
    consts: &IndexMap<String, i64>,
    added: &BTreeMap<String, IndexMap<String, i64>>,
) -> Result<BTreeMap<String, IndexMap<String, i64>>> {
    let mut out = added.clone();
    for mod_dir in mod_dirs {
        let module = name_of(mod_dir);
        for (name, number) in wav_constants(mod_dir)? {
            match consts.get(&name) {
                Some(&known) if known == number => continue,
                Some(&known) => {
                    return Err(Error::invalid(format!(
                        "{module}: a WAV names entry {number} {name}, which the game's headers give {known}; name it otherwise"
                    )));
                }
                None => {}
            }
            let mine = out.entry(module.clone()).or_default();
            match mine.get(&name) {
                Some(&known) if known != number => {
                    return Err(Error::invalid(format!(
                        "{module}: {name} is entry {number} by a WAV's name and {known} by an add line"
                    )));
                }
                _ => {
                    mine.insert(name, number);
                }
            }
        }
    }
    Ok(out)
}
