//! build --start-at MAP:PROGRAM makes a test image whose New Game goes straight to a scene, so
//! trying a mod does not mean replaying up to it. Nothing in the source tree changes: build
//! compiles a patched copy of the script a new game starts in.
//!
//! A new game enters the dungeon loop on floor 20 of dungeon 1 and runs program 1010 (floor_start)
//! of map/d/d01/d01_<language>.stb (title.cpp, dng_main.cpp EntryEventScript). The patch puts a
//! function in front of that program: the first time it runs it sets START_FLAG and warps the way
//! the game's own story warps out of d01 do (load_dungeon_map, then load_event_script at the
//! program); after that it hands over to the original floor_start, so later visits to the dungeon
//! play normally. That warp works for maps the game loads with load_dungeon_map (story scenes and
//! dungeon-like maps); xref map NAME shows whether the game ever does.
//!
//! Town maps (the georama towns, Palm Brinks's streets, interiors) are not entered that way, and
//! the dungeon loop a new game runs in ignores jump_to_map: the game leaves a dungeon for a town
//! with go_to_edit_mode(map number, program), as d07 does for m05 (go_to_edit_mode(14,
//! map_start)). For a map the game never loads with load_dungeon_map but which has a map number,
//! the patch warps that way instead. A map's number is its row in the disc's map table
//! (map/map<language>.cfg, read by LoadMapName and looked up by GetMapName in mapselect.cpp). The
//! engine loads the town's own script, so the warp shows a town that has none. The town warp
//! clears the skip of the next music load that New Game sets for the opening, so the town's
//! default music plays (town_function_text).

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::OnceLock;

use dc2tk_disc::archive::Archive;
use dc2tk_disc::{rebuild, Image};
use indexmap::IndexMap;
use regex::Regex;

use crate::index::{self, Folded};
use crate::{discfiles, maptable, pathnames, sdk, source, Error, Result};

pub const START_PROGRAM: &str = "floor_start";
pub const START_FLAG: i64 = 2047;
/// The program a target without one starts at: the one a map's arrival runs.
pub const DEFAULT_PROGRAM: &str = "map_start";

fn new_game_script(path: &str) -> bool {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| Regex::new(r"^map/d/d01/d01_[0-9]\.c$").unwrap()).is_match(path)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub map: String,
    pub script: String,
    pub program: String,
}

pub fn parse_target(text: &str) -> Result<Target> {
    static CELL: OnceLock<Regex> = OnceLock::new();
    // a final newline is allowed (a value read from a file)
    let pattern = CELL.get_or_init(|| Regex::new(r"^([A-Za-z0-9_]+)(?:/([A-Za-z0-9_]+))?:(-?\w+)\n?$").unwrap());
    let Some(found) = pattern.captures(text) else {
        return Err(Error::invalid(
            "--start-at takes MAP:PROGRAM or MAP/SCRIPT:PROGRAM, as in s51:story_50 or s06/s06b:500",
        ));
    };
    let map = found[1].to_string();
    Ok(Target {
        script: found.get(2).map_or_else(|| map.clone(), |script| script.as_str().to_string()),
        map,
        program: found[3].to_string(),
    })
}

/// A --start-at or mod.json "start_at" target: what parse_target takes, or MAP (or MAP/SCRIPT)
/// alone, which starts at DEFAULT_PROGRAM.
pub fn parse_start(text: &str) -> Result<Target> {
    static CELL: OnceLock<Regex> = OnceLock::new();
    let bare = CELL.get_or_init(|| Regex::new(r"^([A-Za-z0-9_]+(?:/[A-Za-z0-9_]+)?)\n?$").unwrap());
    let given = match bare.captures(text) {
        Some(found) => format!("{}:{DEFAULT_PROGRAM}", &found[1]),
        None => text.to_string(),
    };
    parse_target(&given).map_err(|_| {
        Error::invalid(
            "--start-at takes MAP, MAP:PROGRAM or MAP/SCRIPT:PROGRAM, as in m05, s51:story_50 or s06/s06b:500",
        )
    })
}

/// How the patch reaches the target map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Warp {
    /// load_dungeon_map, then load_event_script at the program.
    Dungeon,
    /// go_to_edit_mode with the map's number and the program.
    Town(i64),
}

/// The map table's rows by map code: {code: map number}. Each MAP_NAME row is numbered from 0 in
/// file order, empty ones included (maptable::rows); the first row of a code gives its number.
pub fn map_numbers(cfg: &[u8]) -> IndexMap<String, i64> {
    let mut found = IndexMap::new();
    for row in maptable::rows(cfg) {
        if !row.code.is_empty() {
            found.entry(row.code).or_insert(row.number);
        }
    }
    found
}

/// The map table's path on the disc for a language (0 Japanese, 1 English, ...); every language's
/// table numbers the maps alike.
pub fn map_table_path(language: Option<i64>) -> String {
    format!("map/map{}.cfg", language.unwrap_or(1))
}

/// map_numbers of a disc image's map table; empty when the image has none.
pub fn disc_map_numbers(iso: &Path, language: Option<i64>) -> Result<IndexMap<String, i64>> {
    let image = Image::open(iso)?;
    let disc = Archive::open(image.bytes())?;
    let index = discfiles::disc_index(&disc);
    let (top, members) = discfiles::location(&map_table_path(language));
    Ok(index
        .get(&top.to_lowercase())
        .and_then(|entry| rebuild::read_member(disc.read(image.bytes(), entry), &members))
        .map(map_numbers)
        .unwrap_or_default())
}

/// How many calls in the tree load map with load_dungeon_map.
fn dungeon_loads(tree: &Path, consts: &IndexMap<String, i64>, map: &str) -> Result<usize> {
    let names = BTreeSet::from(["load_dungeon_map".to_string()]);
    let wanted = Some(Folded::Text(map.to_string()));
    Ok(index::calls(tree, &names)?
        .into_iter()
        .filter(|call| !call.args.is_empty() && index::value(&call.args[0], consts) == wanted)
        .count())
}

/// The way to warp to target: dungeon-style when the game ever loads the map with
/// load_dungeon_map, as before towns could be targets; otherwise as a town when the map table
/// (numbers, read only then) has the map; otherwise dungeon-style, which warnings warn about.
pub fn warp(
    tree: &Path,
    consts: &IndexMap<String, i64>,
    target: &Target,
    numbers: impl FnOnce() -> Result<IndexMap<String, i64>>,
) -> Result<Warp> {
    if dungeon_loads(tree, consts, &target.map)? > 0 {
        return Ok(Warp::Dungeon);
    }
    let Some(&number) = numbers()?.get(&target.map) else {
        return Ok(Warp::Dungeon);
    };
    if target.script != target.map {
        return Err(Error::invalid(format!(
            "{} is a town map, which runs its own script: start at {}:{} without /{}",
            target.map, target.map, target.program, target.script
        )));
    }
    Ok(Warp::Town(number))
}

pub fn function_text(target: &Target, original: &str) -> String {
    [
        format!(
            "// Added by build --start-at: on a new game, goes straight to map {} at program {}. Flag {}",
            target.map, target.program, START_FLAG
        ),
        format!("// marks that it has; after that the original {original} runs."),
        "void start_at_scene(void)".into(),
        "{".into(),
        "    int started;".into(),
        String::new(),
        format!("    started = get_flag({START_FLAG});"),
        "    if (started == 0) {".into(),
        format!("        set_flag({START_FLAG}, 1);"),
        "        init_event();".into(),
        "        fade_out(0);".into(),
        "        wait;".into(),
        format!("        load_dungeon_map(\"{}\");", target.map),
        format!("        load_event_script(\"{}/{}.stb\", {});", target.map, target.script, target.program),
        "        return;".into(),
        "    }".into(),
        format!("    {original}();"),
        "}".into(),
        String::new(),
    ]
    .join("\n")
}

/// The town a warp says the player came from: Palm Brinks (10), or its station (11) when Palm
/// Brinks is where the warp goes. Towns' arrival programs place the player by the map they came
/// from, and a new game's is the dungeon it runs in, which made m01 place the player at another
/// entrance, off the ground of the town as it is at the start. From another town they take their
/// default arrival (m01 does, from the station, unless a train arrival is pending). The previous
/// map is only recorded when the current one changes, so it cannot be the town itself.
pub fn came_from(number: i64) -> i64 {
    if number == 10 {
        11
    } else {
        10
    }
}

/// The warp into a town map, as the game's dungeon scripts leave for one (d01's
/// start_dungeon_floor, d07's floor start, s06's way back to Palm Brinks): Max made the player
/// (a new game starts as Monica; s06 does the same before going to Palm Brinks), the current map
/// set to another town (came_from) so the town's arrival places the player at its default spot,
/// the music load allowed again (allow_bgm_load), then go_to_edit_mode.
///
/// New Game sets the scene's skip_load_bgm before entering the dungeon loop (TitleLoop), which
/// keeps it through its own load (InitDungeonMain) for floor 20's load_dungeon_map("s51"), whose
/// opening starts its own music. The warp runs before that load, so the flag is still set when
/// the town loads, and the edit loop's map load (editLoadSound) would skip the town's default
/// music and clear it: the town's sounds play but its music does not. d01's floor 6 clears it the
/// same way before starting music of its own.
pub fn town_function_text(target: &Target, number: i64, original: &str) -> String {
    [
        format!(
            "// Added by build --start-at: on a new game, goes to town map {} (number {}) at program {}.",
            target.map, number, target.program
        ),
        format!("// Flag {START_FLAG} marks that it has; after that the original {original} runs."),
        "void start_at_scene(void)".into(),
        "{".into(),
        "    int started;".into(),
        String::new(),
        format!("    started = get_flag({START_FLAG});"),
        "    if (started == 0) {".into(),
        format!("        set_flag({START_FLAG}, 1);"),
        "        init_event();".into(),
        "        fade_out(1);".into(),
        "        wait;".into(),
        "        change_player_chara(USER_CHARA_MAX);".into(),
        "        set_active_chara(0);".into(),
        format!(
            "        set_current_map({});  // came from another town: the arrival takes its default spot",
            came_from(number)
        ),
        "        allow_bgm_load();  // New Game skips the next music load, which would be the town's".into(),
        format!("        go_to_edit_mode({number}, {});", target.program),
        "        wait;".into(),
        "        return;".into(),
        "    }".into(),
        format!("    {original}();"),
        "}".into(),
        String::new(),
    ]
    .join("\n")
}

pub fn patched(text: &str, target: &Target) -> Option<String> {
    patched_with(text, target, Warp::Dungeon)
}

/// The new-game script with floor_start running the warp first.
pub fn patched_with(text: &str, target: &Target, warp: Warp) -> Option<String> {
    static PROGRAM: OnceLock<Regex> = OnceLock::new();
    static PRAGMAS: OnceLock<Regex> = OnceLock::new();
    let program = PROGRAM.get_or_init(|| Regex::new(&format!(r"(?m)^PROGRAM\({START_PROGRAM}, (\w+)\);$")).unwrap());
    let found = program.captures(text)?;
    let whole = found.get(0).unwrap();
    let original = found[1].to_string();
    let text = format!("{}PROGRAM({START_PROGRAM}, start_at_scene);{}", &text[..whole.start()], &text[whole.end()..]);
    let pragmas = PRAGMAS.get_or_init(|| Regex::new(r"(?m)^#pragma padding_of ").unwrap());
    let at = pragmas.find(&text).map_or(text.len(), |found| found.start());
    let function = match warp {
        Warp::Dungeon => function_text(target, &original),
        Warp::Town(number) => town_function_text(target, number, &original),
    };
    Some(format!("{}{}\n{}", &text[..at], function, &text[at..]))
}

/// The patched new-game scripts, keyed by tree-relative path, for build to compile in their place.
/// texts holds mods' versions of tree files, which are patched instead of the tree's.
pub fn overrides(tree: &Path, target: &Target, texts: &IndexMap<String, String>) -> Result<IndexMap<String, String>> {
    warp_overrides(tree, target, Warp::Dungeon, texts)
}

/// overrides, warping the given way.
pub fn warp_overrides(
    tree: &Path,
    target: &Target,
    warp: Warp,
    texts: &IndexMap<String, String>,
) -> Result<IndexMap<String, String>> {
    let mut found = IndexMap::new();
    for relative in sdk::files_under(tree, &source::EXTENSION[1..])? {
        if new_game_script(&pathnames::unnamed(tree, &relative)?) {
            let text = match texts.get(&relative).filter(|text| !text.is_empty()) {
                Some(text) => text.clone(),
                None => dc2tk_disc::textfile::read_text(&tree.join(&relative))?,
            };
            let Some(text) = patched_with(&text, target, warp) else {
                return Err(Error::invalid(format!(
                    "{relative} has no PROGRAM({START_PROGRAM}, ...) line to start from"
                )));
            };
            found.insert(relative, text);
        }
    }
    if found.is_empty() {
        return Err(Error::invalid(format!(
            "no map/d/d01/d01_N{} in {}: the tree must hold the scripts a new game starts in",
            source::EXTENSION,
            tree.display()
        )));
    }
    Ok(found)
}

pub fn warnings(tree: &Path, consts: &IndexMap<String, i64>, target: &Target) -> Result<Vec<String>> {
    warp_warnings(tree, consts, target, Warp::Dungeon)
}

/// What may keep the warp from working: a program the headers do not define, or a map the game
/// never enters the way the warp does.
pub fn warp_warnings(tree: &Path, consts: &IndexMap<String, i64>, target: &Target, warp: Warp) -> Result<Vec<String>> {
    let mut lines = Vec::new();
    if index::value(&target.program, consts).is_none() {
        lines.push(format!("program {} is not a number or a program name the headers define", target.program));
    }
    match warp {
        Warp::Dungeon if dungeon_loads(tree, consts, &target.map)? == 0 => lines.push(format!(
            "the game never loads {} with load_dungeon_map, so the warp may not work; xref map {} shows how it is entered",
            target.map, target.map
        )),
        Warp::Town(number) => {
            let names = BTreeSet::from(["go_to_edit_mode".to_string()]);
            let wanted = Some(Folded::Int(number.into()));
            let entries = index::calls(tree, &names)?
                .into_iter()
                .filter(|call| !call.args.is_empty() && index::value(&call.args[0], consts) == wanted)
                .count();
            if entries == 0 {
                lines.push(format!(
                    "the game never goes to {} (map {}) with go_to_edit_mode, so the warp may not work; xref map {} \
                     shows how it is entered",
                    target.map, number, target.map
                ));
            }
        }
        Warp::Dungeon => {}
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &[u8] = b"MAP_NAME_NUM 4 ;\r\nMAP_NAME \"g01\",\"Sindain\",\"g00/g00\", 1 , 1 , 2000 , 2 ;// 0 \r\n\
MAP_NAME \"\",\"\",\"\",0,0;// 1 \r\n// MAP_NAME \"x99\",\"commented out\",\"\",0,0;\r\n\
MAP_NAME \"m05\",\"\x83p\x81[\x83\x8b\",\"\", 3 , 2 , 1000 ;// 2 \r\n;MAP_NAME \"s01\",\"after a ;\",\"\",3,3;// 3\r\n\
 MAP_NAME \"g01\",\"again\",\"\",0,0;// 4\r\n";

    #[test]
    fn targets() {
        assert_eq!(
            parse_target("s06/s06b:500").unwrap(),
            Target { map: "s06".into(), script: "s06b".into(), program: "500".into() }
        );
        assert_eq!(parse_target("s51:story_50").unwrap().script, "s51");
        assert!(parse_target("s51").is_err());
    }

    #[test]
    fn start_targets_may_leave_out_the_program() {
        assert_eq!(
            parse_start("m05").unwrap(),
            Target { map: "m05".into(), script: "m05".into(), program: "map_start".into() }
        );
        assert_eq!(parse_start("m05\n").unwrap().program, "map_start");
        assert_eq!(parse_start("s06/s06b").unwrap().script, "s06b");
        assert_eq!(parse_start("s51:story_50").unwrap(), parse_target("s51:story_50").unwrap());
        assert_eq!(parse_start("s06/s06b:500").unwrap(), parse_target("s06/s06b:500").unwrap());
        for wrong in ["", "m05:", "m05 map_start", "a/b/c"] {
            assert!(parse_start(wrong).unwrap_err().to_string().contains("MAP, MAP:PROGRAM"), "{wrong:?}");
        }
    }

    #[test]
    fn map_numbers_count_every_row_in_order() {
        // a commented row is no row; a ';' ends an empty statement, so the row after it is read
        let found = map_numbers(TABLE);
        assert_eq!(
            found.into_iter().collect::<Vec<_>>(),
            [("g01".to_string(), 0), ("m05".to_string(), 2), ("s01".to_string(), 3)]
        );
    }

    #[test]
    fn a_town_warp_goes_to_edit_mode_once_then_runs_the_original() {
        let target = parse_start("m05").unwrap();
        let text =
            "PROGRAM(floor_start, setup_floor);\nvoid setup_floor(void)\n{\n}\n#pragma padding_of setup_floor 0\n";
        let town = patched_with(text, &target, Warp::Town(14)).unwrap();
        assert!(town.contains("PROGRAM(floor_start, start_at_scene);"));
        let function = town_function_text(&target, 14, "setup_floor");
        assert!(town.contains(&function));
        let body: Vec<&str> = function.lines().skip(8).map(str::trim).collect();
        assert_eq!(
            body,
            [
                "set_flag(2047, 1);",
                "init_event();",
                "fade_out(1);",
                "wait;",
                "change_player_chara(USER_CHARA_MAX);",
                "set_active_chara(0);",
                "set_current_map(10);  // came from another town: the arrival takes its default spot",
                "allow_bgm_load();  // New Game skips the next music load, which would be the town's",
                "go_to_edit_mode(14, map_start);",
                "wait;",
                "return;",
                "}",
                "setup_floor();",
                "}",
            ]
        );
        // the dungeon warp is what it was
        assert_eq!(patched_with(text, &target, Warp::Dungeon), patched(text, &target));
        assert!(patched(text, &target).unwrap().contains("load_dungeon_map(\"m05\");"));
    }

    #[test]
    fn the_disc_numbers_m05_14() {
        // set C2S_CORPUS to an extracted disc (extracted/disc) to read its map tables
        let Some(corpus) = std::env::var_os("C2S_CORPUS") else {
            return;
        };
        let read = |language: i64| {
            map_numbers(&std::fs::read(Path::new(&corpus).join(map_table_path(Some(language)))).unwrap())
        };
        let english = read(1);
        assert_eq!(english["m05"], 14);
        assert_eq!(english["g01"], 0);
        assert_eq!((english["d01e01"], english["d01f01"], english["s51"], english["s71"]), (22, 55, 130, 188));
        assert_eq!(english.len(), 185);
        for language in [0, 2, 3, 4, 5, 6] {
            assert_eq!(read(language), english, "map{language}.cfg numbers the maps differently");
        }
    }

    #[test]
    fn a_town_warp_never_comes_from_the_town_itself() {
        assert_eq!(came_from(14), 10);
        assert_eq!(came_from(10), 11);
    }
}
