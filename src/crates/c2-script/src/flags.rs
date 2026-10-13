//! Save bit flags: which numbers a source tree's scripts and the engine use, and which are free for
//! a mod. The save holds FLAG_COUNT one-bit flags (CSaveData::CheckBitFlagNo rejects the rest).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use indexmap::IndexMap;
use std::path::Path;

use crate::index::{self, Call, Folded};
use crate::Result;

pub const FLAG_COUNT: i64 = 0x800;

/// Flags the engine reads or writes by number, from the ChronicleTwo decomp: GetBitFlag and
/// SetBitFlag calls with a literal or a named SAVE_BIT_FLAG, and the named flags it only lists.
pub const ENGINE_FLAGS: [(i64, &str); 40] = [
    (4, "SAVE_FLAG_MENU_TIME_EVENING"),
    (12, "menushop.cpp"),
    (13, "inventmn.cpp"),
    (27, "menushop.cpp"),
    (33, "editloop.cpp, editmenu.cpp"),
    (34, "editloop.cpp"),
    (48, "inventmn.cpp, userdata.cpp"),
    (49, "menusys.cpp"),
    (50, "dng_main.cpp"),
    (52, "dng_main.cpp"),
    (53, "dng_main.cpp"),
    (54, "menumain.cpp"),
    (59, "dng_main.cpp"),
    (60, "fishing.cpp"),
    (61, "editloop.cpp"),
    (63, "menuaqua.cpp"),
    (102, "SAVE_FLAG_MENU_TIME_MIDNIGHT"),
    (103, "SAVE_FLAG_MENU_TIME_1AM"),
    (104, "SAVE_FLAG_MENU_TIME_FORCED_OFF"),
    (220, "dngmenu.cpp"),
    (235, "fishing.cpp"),
    (240, "fishing.cpp"),
    (250, "editloop.cpp"),
    (254, "userdata.cpp"),
    (317, "dng_debug.cpp, dngmenu.cpp"),
    (330, "editanalyze.cpp"),
    (344, "memcard.cpp, menumain.cpp, savedata.cpp"),
    (356, "editanalyze.cpp"),
    (424, "memcard.cpp, menumain.cpp, savedata.cpp"),
    (444, "editanalyze.cpp"),
    (520, "editanalyze.cpp, editexception.cpp, editloop.cpp, editmenu.cpp"),
    (536, "editanalyze.cpp"),
    (600, "SAVE_FLAG_WORLD_MAP_2"),
    (616, "SAVE_FLAG_WORLD_MAP_4"),
    (640, "SAVE_FLAG_WORLD_MAP_3"),
    (700, "SAVE_FLAG_WORLD_MAP_S12_REDIRECT"),
    (702, "SAVE_FLAG_GEKKA_VIEW"),
    (736, "menumain.cpp"),
    (799, "charasetup.cpp, gamedata.cpp, memcard.cpp"),
    (800, "editanalyze.cpp, menumain.cpp"),
];

/// Set by the opening of an image built with build --start-at (see startat).
pub const TOOL_FLAGS: [(i64, &str); 1] = [(2047, "build --start-at")];

/// Natives that take a save flag number as their first argument: event scripts' get_flag and
/// set_flag and monster scripts' get_dungeon_flag and set_dungeon_flag, under SDK or original
/// names.
pub const FLAG_FUNCTIONS: [(&str, &str); 8] = [
    ("get_flag", "get"),
    ("set_flag", "set"),
    ("get_dungeon_flag", "get"),
    ("set_dungeon_flag", "set"),
    ("_GET_FLAG", "get"),
    ("_SET_FLAG", "set"),
    ("_GET_BIT_FLAG", "get"),
    ("_SET_BIT_FLAG", "set"),
];

fn engine(number: i64) -> Option<&'static str> {
    ENGINE_FLAGS.iter().find(|(flag, _)| *flag == number).map(|(_, where_)| *where_)
}

fn tool(number: i64) -> Option<&'static str> {
    TOOL_FLAGS.iter().find(|(flag, _)| *flag == number).map(|(_, what)| *what)
}

/// A call that reads or sets a flag.
#[derive(Clone, Debug)]
pub struct Use {
    pub call: Call,
    pub access: &'static str,
    pub expr: String,
}

/// The flags a tree's scripts use, by number, and the calls whose number is worked out at run
/// time.
pub struct Found {
    pub uses: BTreeMap<i64, Vec<Use>>,
    pub computed: Vec<Use>,
}

pub fn scan(tree: &Path, consts: &IndexMap<String, i64>) -> Result<Found> {
    let names: BTreeSet<String> = FLAG_FUNCTIONS.iter().map(|(name, _)| name.to_string()).collect();
    let mut found = Found { uses: BTreeMap::new(), computed: Vec::new() };
    // index::calls lists files in order and calls in each in order, as the Python does
    for call in index::calls(tree, &names)? {
        // an old-numbered call that lands on a flag native was written for another one
        if call.args.is_empty() || call.meant.is_some() {
            continue;
        }
        let access = FLAG_FUNCTIONS.iter().find(|(name, _)| *name == call.name).map(|(_, access)| *access).unwrap();
        let expr = call.args[0].clone();
        let number = index::value(&expr, consts);
        let found_use = Use { call, access, expr };
        match number {
            Some(Folded::Int(number)) => found.uses.entry(number as i64).or_default().push(found_use),
            _ => found.computed.push(found_use),
        }
    }
    Ok(found)
}

pub fn used_numbers(found: &Found) -> BTreeSet<i64> {
    let mut used: BTreeSet<i64> = found.uses.keys().copied().collect();
    used.extend(ENGINE_FLAGS.iter().map(|(flag, _)| *flag));
    used.extend(TOOL_FLAGS.iter().map(|(flag, _)| *flag));
    used
}

pub fn free_ranges(used: &BTreeSet<i64>) -> Vec<(i64, i64)> {
    let mut ranges = Vec::new();
    let mut start = None;
    for number in 0..=FLAG_COUNT {
        let free = number < FLAG_COUNT && !used.contains(&number);
        match (free, start) {
            (true, None) => start = Some(number),
            (false, Some(first)) => {
                ranges.push((first, number - 1));
                start = None;
            }
            _ => {}
        }
    }
    ranges
}

pub fn range_text(ranges: &[(i64, i64)]) -> String {
    ranges.iter().map(|(a, b)| if a == b { a.to_string() } else { format!("{a}-{b}") }).collect::<Vec<_>>().join(", ")
}

/// The first FLAG_/SAVE_FLAG_ name the headers give each number.
fn names_by_number(consts: &IndexMap<String, i64>) -> HashMap<i64, String> {
    let mut names = HashMap::new();
    for (name, number) in consts {
        if name.starts_with("FLAG_") || name.starts_with("SAVE_FLAG_") {
            names.entry(*number).or_insert_with(|| name.clone());
        }
    }
    names
}

pub fn summary(found: &Found) -> String {
    let used = used_numbers(found);
    let script_only: BTreeSet<i64> = found.uses.keys().copied().collect();
    let ranges = free_ranges(&used);
    let largest = ranges.iter().copied().fold(None, |best: Option<(i64, i64)>, pair| match best {
        Some(best) if best.1 - best.0 >= pair.1 - pair.0 => Some(best),
        _ => Some(pair),
    });
    let mut lines = vec![
        format!(
            "{} of {} flags are used: {} by scripts, {} by the engine, {} by these tools",
            used.len(),
            FLAG_COUNT,
            script_only.len(),
            ENGINE_FLAGS.len(),
            TOOL_FLAGS.len()
        ),
        format!(
            "highest flag a script uses: {}; highest the engine uses: {}",
            script_only.iter().max().copied().unwrap_or(-1),
            ENGINE_FLAGS.iter().map(|(flag, _)| *flag).max().unwrap()
        ),
    ];
    if let Some(largest) = largest {
        lines.push(format!(
            "largest free block: {} ({} flags); prefer it, since gaps between the game's own flags may be",
            range_text(&[largest]),
            largest.1 - largest.0 + 1
        ));
        lines.push("reached through menu data or arithmetic this scan cannot follow".into());
    }
    lines.push(format!("free: {}", range_text(&ranges)));
    if !found.computed.is_empty() {
        lines.push(String::new());
        lines.push(format!(
            "{} calls pass a flag number worked out at run time; check these by hand:",
            found.computed.len()
        ));
        lines.extend(
            found.computed.iter().map(|used| format!("  {}:{}  {}", used.call.path, used.call.line, used.call.text)),
        );
    }
    lines.join("\n")
}

pub fn used_list(found: &Found, consts: &IndexMap<String, i64>) -> String {
    let names = names_by_number(consts);
    let mut lines = Vec::new();
    for number in used_numbers(found) {
        let uses = found.uses.get(&number).map(Vec::as_slice).unwrap_or(&[]);
        let sets = uses.iter().filter(|used| used.access == "set").count();
        let mut parts = Vec::new();
        if !uses.is_empty() {
            parts.push(format!("set {}, read {}", sets, uses.len() - sets));
        }
        if let Some(where_) = engine(number) {
            parts.push(format!("engine: {where_}"));
        }
        if let Some(what) = tool(number) {
            parts.push(what.to_string());
        }
        lines.push(format!(
            "{:>5}  {:<40} {}",
            number,
            names.get(&number).map(String::as_str).unwrap_or(""),
            parts.join("; ")
        ));
    }
    lines.join("\n")
}

pub fn who(found: &Found, number: i64, consts: &IndexMap<String, i64>) -> String {
    let name = names_by_number(consts).get(&number).cloned();
    let mut lines = vec![format!("flag {}{}", number, name.map(|name| format!(" ({name})")).unwrap_or_default())];
    if let Some(where_) = engine(number) {
        lines.push(format!("  engine: {where_}"));
    }
    if let Some(what) = tool(number) {
        lines.push(format!("  tools: {what}"));
    }
    for used in found.uses.get(&number).into_iter().flatten() {
        lines.push(format!(
            "  {:<4} {}:{}  {}  (in {})",
            used.access,
            used.call.path,
            used.call.line,
            used.call.text,
            used.call.function.clone().unwrap_or_else(|| "None".into())
        ));
    }
    if lines.len() == 1 {
        lines.push("  free: no script, the engine or these tools use it".into());
    }
    lines.join("\n")
}
