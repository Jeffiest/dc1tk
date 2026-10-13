//! build --start-at MAP:PROGRAM makes a test image whose New Game goes straight to a scene, so
//! trying a mod does not mean replaying up to it. Nothing in the source tree changes: build
//! compiles a patched copy of the script a new game starts in.
//!
//! A new game enters the dungeon loop on floor 20 of dungeon 1 and runs program 1010 (floor_start)
//! of map/d/d01/d01_<language>.stb (title.cpp, dng_main.cpp EntryEventScript). The patch puts a
//! function in front of that program: the first time it runs it sets START_FLAG and warps the way
//! the game's own story warps out of d01 do (load_dungeon_map, then load_event_script at the
//! program); after that it hands over to the original floor_start, so later visits to the dungeon
//! play normally. The warp works for maps the game loads with load_dungeon_map (story scenes and
//! dungeon-like maps); xref map NAME shows whether the game ever does.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::OnceLock;

use indexmap::IndexMap;
use regex::Regex;

use crate::index::{self, Folded};
use crate::{pathnames, sdk, source, Error, Result};

pub const START_PROGRAM: &str = "floor_start";
pub const START_FLAG: i64 = 2047;

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
    // Python's $ also matches before a final newline
    let pattern = CELL.get_or_init(|| Regex::new(r"^([A-Za-z0-9_]+)(?:/([A-Za-z0-9_]+))?:(-?\w+)\n?$").unwrap());
    let Some(found) = pattern.captures(text) else {
        return Err(Error::invalid("--start-at takes MAP:PROGRAM or MAP/SCRIPT:PROGRAM, as in s51:story_50 or s06/s06b:500"));
    };
    let map = found[1].to_string();
    Ok(Target {
        script: found.get(2).map_or_else(|| map.clone(), |script| script.as_str().to_string()),
        map,
        program: found[3].to_string(),
    })
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

pub fn patched(text: &str, target: &Target) -> Option<String> {
    static PROGRAM: OnceLock<Regex> = OnceLock::new();
    static PRAGMAS: OnceLock<Regex> = OnceLock::new();
    let program = PROGRAM.get_or_init(|| Regex::new(&format!(r"(?m)^PROGRAM\({START_PROGRAM}, (\w+)\);$")).unwrap());
    let found = program.captures(text)?;
    let whole = found.get(0).unwrap();
    let original = found[1].to_string();
    let text = format!("{}PROGRAM({START_PROGRAM}, start_at_scene);{}", &text[..whole.start()], &text[whole.end()..]);
    let pragmas = PRAGMAS.get_or_init(|| Regex::new(r"(?m)^#pragma padding_of ").unwrap());
    let at = pragmas.find(&text).map_or(text.len(), |found| found.start());
    Some(format!("{}{}\n{}", &text[..at], function_text(target, &original), &text[at..]))
}

/// The patched new-game scripts, keyed by tree-relative path, for build to compile in their place.
/// texts holds mods' versions of tree files, which are patched instead of the tree's.
pub fn overrides(tree: &Path, target: &Target, texts: &IndexMap<String, String>) -> Result<IndexMap<String, String>> {
    let mut found = IndexMap::new();
    for relative in sdk::files_under(tree, &source::EXTENSION[1..]) {
        if new_game_script(&pathnames::unnamed(tree, &relative)?) {
            let text = match texts.get(&relative).filter(|text| !text.is_empty()) {
                Some(text) => text.clone(),
                None => c2_py::read_text(&tree.join(&relative))?,
            };
            let Some(text) = patched(&text, target) else {
                return Err(Error::invalid(format!("{relative} has no PROGRAM({START_PROGRAM}, ...) line to start from")));
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
    let mut lines = Vec::new();
    if index::value(&target.program, consts).is_none() {
        lines.push(format!("program {} is not a number or a program name the headers define", target.program));
    }
    let names = BTreeSet::from(["load_dungeon_map".to_string()]);
    let wanted = Some(Folded::Text(target.map.clone()));
    let loads = index::calls(tree, &names)?.into_iter().filter(|call| !call.args.is_empty() && index::value(&call.args[0], consts) == wanted).count();
    if loads == 0 {
        lines.push(format!(
            "the game never loads {} with load_dungeon_map, so the warp may not work; xref map {} shows how it is entered",
            target.map, target.map
        ));
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets() {
        assert_eq!(
            parse_target("s06/s06b:500").unwrap(),
            Target { map: "s06".into(), script: "s06b".into(), program: "500".into() }
        );
        assert_eq!(parse_target("s51:story_50").unwrap().script, "s51");
        assert!(parse_target("s51").is_err());
    }
}
