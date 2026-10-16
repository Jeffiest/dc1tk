//! Cross-references in a decompiled source tree: what starts an event, what loads a map, and where
//! a monster kind is loaded and which AI script it runs. These are the links a mod has to keep
//! intact (a fight that ends from inside its monsters' AI, a story warp that enters a map at one
//! program).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use dc2tk_disc::DiscSource;
use indexmap::IndexMap;

use crate::definitions::Definitions;
use crate::disc::{self, MonsterRow};
use crate::index::{self, Call, Folded};
use crate::{additions, pathnames, Error, Result};

/// Natives that move to a map or load its script, with the argument positions of the map (or the
/// "map/file.stb" path) and of the program it starts at (None when the native takes none).
pub const MAP_CALLS: [(&str, usize, Option<usize>); 7] = [
    ("load_dungeon_map", 0, None),
    ("load_event_script", 0, Some(1)),
    ("jump_to_map", 1, Some(2)),
    ("go_to_interior", 1, Some(2)),
    ("go_to_outside", 1, Some(2)),
    ("enter_interior", 1, Some(2)),
    ("go_to_edit_mode", 0, Some(1)),
];

/// Natives that ask the current map's script to run a program: from monster AI (run_event_script)
/// and from event scripts (queue_dungeon_event). Either runs the program of whatever map is loaded.
pub const REQUEST_CALLS: [&str; 4] = ["run_event_script", "queue_dungeon_event", "_RUN_EVENT_SCRIPT", "_DNG_RUN_EVENT"];

pub const MONSTER_CALLS: [&str; 5] =
    ["load_monster_data", "place_monster", "create_monster", "_LOAD_MONSTER_FILE", "_AUTO_SET_MONSTER"];

pub type Rows = IndexMap<i64, MonsterRow>;
pub type Users = BTreeMap<String, Vec<String>>;

fn name_set(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|name| name.to_string()).collect()
}

/// A call that moves to a map: the map, the target as written (a map or a "map/file.stb" path)
/// and the program it starts at.
#[derive(Clone, Debug)]
pub struct Entry {
    pub call: Call,
    pub map: Option<Folded>,
    pub target: Option<Folded>,
    pub program: Option<Folded>,
}

impl AsRef<Call> for Entry {
    fn as_ref(&self) -> &Call {
        &self.call
    }
}

impl AsRef<Call> for Call {
    fn as_ref(&self) -> &Call {
        self
    }
}

pub fn map_of(target: &Option<Folded>) -> Option<Folded> {
    match target {
        Some(Folded::Text(text)) => Some(Folded::Text(text.split('/').next().unwrap_or("").to_string())),
        other => other.clone(),
    }
}

/// Whether a tree path is one of map_name's scripts (see additions::in_map), by its path without
/// readable names.
pub fn in_map(tree: &Path, path: &str, map_name: &str) -> Result<bool> {
    Ok(additions::in_map(&pathnames::unnamed(tree, path)?, map_name))
}

/// The scripts a call runs in: its own file, or every script that includes the library it is in.
pub fn scripts_of<'a>(call: &'a Call, users: &'a Users) -> Vec<&'a String> {
    if index::is_library(&call.path) {
        users.get(&call.path).map(|found| found.iter().collect()).unwrap_or_default()
    } else {
        vec![&call.path]
    }
}

/// A program number as a report writes it: the number, or the text a script passes instead, quoted.
pub fn program_text(value: &Folded) -> String {
    match value {
        Folded::Int(number) => number.to_string(),
        Folded::Text(text) => format!("{text:?}"),
    }
}

type Extra<'a, T> = &'a dyn Fn(&T) -> Result<Option<String>>;

/// One line per distinct call; the same call in each language variant of a script is listed once.
pub fn grouped<T: AsRef<Call>>(calls: &[T], users: &Users, extra: Option<Extra<T>>) -> Result<Vec<String>> {
    let mut groups: IndexMap<(String, String, Option<String>), Vec<&T>> = IndexMap::new();
    for item in calls {
        let call = item.as_ref();
        groups
            .entry((index::variant_key(&call.path), call.text.clone(), call.function.clone()))
            .or_default()
            .push(item);
    }
    let mut lines = Vec::new();
    for ((_, text, function), members) in &groups {
        let first = members[0].as_ref();
        let mut line = format!("{}:{}", first.path, first.line);
        if members.len() > 1 {
            line +=
                &format!(" (and {} language variant{})", members.len() - 1, if members.len() == 2 { "" } else { "s" });
        }
        line = format!("  {line}  {text}");
        if let Some(function) = function {
            line += &format!("  (in {function})");
        }
        if let Some(extra) = extra {
            if let Some(note) = extra(members[0])? {
                line += &format!("\n      {note}");
            }
        }
        if index::is_library(&first.path) {
            let empty = Vec::new();
            let scripts = users.get(&first.path).unwrap_or(&empty);
            let shown: BTreeSet<String> = scripts.iter().map(|path| index::variant_key(path)).collect();
            let shown: Vec<&String> = shown.iter().collect();
            let listed: Vec<&str> = shown.iter().take(4).map(|path| path.as_str()).collect();
            line += &format!(
                "\n      library included by {} script{}: {}{}",
                scripts.len(),
                if scripts.len() == 1 { "" } else { "s" },
                listed.join(", "),
                if shown.len() > 4 { ", ..." } else { "" }
            );
        }
        lines.push(line);
    }
    Ok(lines)
}

/// Every call that moves to a map, with the map and program it names.
pub fn map_entries(tree: &Path, consts: &IndexMap<String, i64>) -> Result<Vec<Entry>> {
    let names: Vec<&str> = MAP_CALLS.iter().map(|(name, _, _)| *name).collect();
    let mut entries = Vec::new();
    for call in index::calls(tree, &name_set(&names))? {
        let &(_, map_at, program_at) = MAP_CALLS.iter().find(|(name, _, _)| *name == call.name).unwrap();
        if map_at >= call.args.len() {
            continue;
        }
        let target = index::value(&call.args[map_at], consts);
        let program = match program_at {
            Some(at) if at < call.args.len() => index::value(&call.args[at], consts),
            _ => None,
        };
        entries.push(Entry { map: map_of(&target), target, program, call });
    }
    Ok(entries)
}

/// The executable's monster table, read from the disc (empty without one).
pub fn monster_rows(disc: Option<&DiscSource>, defs: &Definitions) -> Rows {
    let layout = defs.executable.get("monster_table").filter(|layout| layout.is_set());
    match (disc.and_then(|disc| disc::executable(disc, defs)), layout) {
        (Some(elf), Some(layout)) => disc::monster_table(&elf, layout),
        _ => Rows::new(),
    }
}

pub fn ai_script(row: &MonsterRow) -> &str {
    if row.script.is_empty() {
        &row.model
    } else {
        &row.script
    }
}

fn row_of<'a>(rows: &'a Rows, value: Option<&Folded>) -> Option<(i64, &'a MonsterRow)> {
    let Some(Folded::Int(number)) = value else {
        return None;
    };
    let kind = i64::try_from(*number).ok()?;
    rows.get(&kind).map(|row| (kind, row))
}

/// Monster kinds the map's scripts load or place, by the AI script each runs.
pub fn map_monster_ais(
    tree: &Path,
    consts: &IndexMap<String, i64>,
    rows: &Rows,
    map_name: &str,
    users: &Users,
) -> Result<IndexMap<String, BTreeSet<i64>>> {
    let mut found: IndexMap<String, BTreeSet<i64>> = IndexMap::new();
    for call in index::calls(tree, &name_set(&MONSTER_CALLS))? {
        if call.args.is_empty() {
            continue;
        }
        let mut here = false;
        for path in scripts_of(&call, users) {
            if in_map(tree, path, map_name)? {
                here = true;
                break;
            }
        }
        if !here {
            continue;
        }
        if let Some((kind, row)) = row_of(rows, index::value(&call.args[0], consts).as_ref()) {
            found.entry(ai_script(row).to_string()).or_default().insert(kind);
        }
    }
    Ok(found)
}

fn is_number(value: Option<&Folded>, number: i128) -> bool {
    value == Some(&Folded::Int(number))
}

/// Path(path).stem.
fn stem(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    match name.rfind('.') {
        Some(at) if at > 0 => &name[..at],
        _ => name,
    }
}

pub fn event_report(
    tree: &Path,
    consts: &IndexMap<String, i64>,
    number: i128,
    map_name: Option<&str>,
    rows: &Rows,
) -> Result<String> {
    let outline = index::outlines(tree)?;
    let users = index::includers(&outline);
    let named_map = map_name.filter(|name| !name.is_empty());
    let mut lines =
        vec![format!("event {number}{}", named_map.map(|name| format!(" in map {name}")).unwrap_or_default())];
    let mut defined: BTreeMap<(String, String), Vec<&String>> = BTreeMap::new();
    for (path, entry) in &outline {
        if let Some(name) = named_map {
            if !in_map(tree, path, name)? {
                continue;
            }
        }
        for (written, function) in &entry.programs {
            if is_number(index::value(written, consts).as_ref(), number) {
                defined.entry((index::variant_key(path), function.clone())).or_default().push(path);
            }
        }
    }
    lines.extend([String::new(), "defined by:".to_string()]);
    for ((key, function), paths) in &defined {
        let shown = if paths.len() == 1 { paths[0] } else { key };
        lines.push(format!("  {shown}  PROGRAM({number}, {function})"));
    }
    if defined.is_empty() {
        let of = named_map.map(|name| format!(" of {name}")).unwrap_or_default();
        lines.push(format!("  no script{of} has PROGRAM({number}, ...)"));
    }
    let started: Vec<Entry> = map_entries(tree, consts)?
        .into_iter()
        .filter(|entry| {
            is_number(entry.program.as_ref(), number)
                && map_name.is_none_or(|name| entry.map == Some(Folded::Text(name.to_string())))
        })
        .collect();
    lines.extend([String::new(), "started by moving to the map at this program:".to_string()]);
    lines.extend(or_none(grouped(&started, &users, None)?));
    let requested: Vec<Call> = index::calls(tree, &name_set(&REQUEST_CALLS))?
        .into_iter()
        .filter(|call| !call.args.is_empty() && is_number(index::value(&call.args[0], consts).as_ref(), number))
        .collect();
    let ais = match named_map {
        Some(name) if !rows.is_empty() => map_monster_ais(tree, consts, rows, name, &users)?,
        _ => IndexMap::new(),
    };
    let placed_here = |call: &Call| -> Result<Option<String>> {
        let plain = pathnames::unnamed(tree, &call.path)?;
        let kinds = if plain.starts_with("dungeon/monster/") { ais.get(stem(&plain)) } else { None };
        let Some(kinds) = kinds.filter(|kinds| !kinds.is_empty()) else {
            return Ok(None);
        };
        let listed: Vec<String> = kinds.iter().map(|kind| format!("{kind} {}", rows[kind].name)).collect();
        Ok(Some(format!("<- {} places kind {}, which runs this AI", map_name.unwrap_or(""), listed.join(", "))))
    };
    lines.extend([String::new(), format!("requested while a map is loaded (runs the loaded map's program {number}):")]);
    lines.extend(or_none(grouped(&requested, &users, Some(&placed_here))?));
    Ok(lines.join("\n"))
}

fn or_none(lines: Vec<String>) -> Vec<String> {
    or_else(lines, "  none found")
}

fn or_else(lines: Vec<String>, otherwise: &str) -> Vec<String> {
    if lines.is_empty() {
        vec![otherwise.to_string()]
    } else {
        lines
    }
}

/// The note on an entry that starts a program.
pub fn starts_program(entry: &Entry) -> Result<Option<String>> {
    match &entry.program {
        None => Ok(None),
        Some(program) => Ok(Some(format!("starts program {}", program_text(program)))),
    }
}

pub fn map_report(tree: &Path, consts: &IndexMap<String, i64>, map_name: &str) -> Result<String> {
    let outline = index::outlines(tree)?;
    let users = index::includers(&outline);
    let target = Some(Folded::Text(map_name.to_string()));
    let entries: Vec<Entry> = map_entries(tree, consts)?.into_iter().filter(|entry| entry.map == target).collect();
    let mut lines = vec![format!("map {map_name}"), String::new(), "scripts:".into()];
    let mut paths: Vec<&String> = outline.keys().collect();
    paths.sort();
    let mut scripts = Vec::new();
    for path in paths {
        if in_map(tree, path, map_name)? && !index::is_library(path) {
            scripts.push(format!("  {path}"));
        }
    }
    lines.extend(or_else(scripts, "  none in this tree"));
    lines.extend([String::new(), "entered by:".to_string()]);
    lines.extend(or_else(
        grouped(&entries, &users, Some(&starts_program))?,
        "  no call names it as a string (calls that pass a map number are not matched)",
    ));
    Ok(lines.join("\n"))
}

pub fn monster_report(tree: &Path, consts: &IndexMap<String, i64>, rows: &Rows, wanted: &str) -> Result<String> {
    // a number in decimal, or what folds to one (MONSTER_ constants), or else a name
    let mut number = match wanted.parse::<i128>() {
        Ok(value) => Some(value),
        Err(_) => match index::value(wanted, consts) {
            Some(Folded::Int(value)) => Some(value),
            Some(Folded::Text(_)) => {
                return Err(Error::invalid(format!(
                    "{wanted} is text; give a monster kind's number, its MONSTER_ constant or its name"
                )))
            }
            None => None,
        },
    };
    if number.is_none() {
        let lowered = wanted.to_lowercase();
        number = rows.iter().find(|(_, row)| row.name.to_lowercase() == lowered).map(|(kind, _)| *kind as i128);
    }
    let Some(number) = number else {
        return Ok(format!("no monster kind {wanted} (give its number, its MONSTER_ constant or its name)"));
    };
    let row = row_of(rows, Some(&Folded::Int(number)));
    let names: Vec<&str> = consts
        .iter()
        .filter(|(name, value)| name.starts_with("MONSTER_") && **value as i128 == number)
        .map(|(name, _)| name.as_str())
        .collect();
    let mut lines = vec![format!(
        "monster kind {number}{}{}",
        row.map(|(_, row)| format!(" {}", row.name)).unwrap_or_default(),
        if names.is_empty() { String::new() } else { format!(" ({})", names.join(", ")) }
    )];
    if let Some((kind, row)) = row {
        let script = ai_script(row);
        let mut sharing: Vec<i64> = rows
            .iter()
            .filter(|(other_kind, other)| ai_script(other) == script && **other_kind != kind)
            .map(|(other_kind, _)| *other_kind)
            .collect();
        sharing.sort();
        lines.push(format!("  model:     dungeon/monster/{}.chr", row.model));
        lines.push(format!(
            "  AI script: {} (on the disc: dungeon/monster/{script}.stb)",
            pathnames::named(tree, &format!("dungeon/monster/{script}.c"))?
        ));
        if !sharing.is_empty() {
            let listed: Vec<String> = sharing.iter().map(|kind| format!("{kind} {}", rows[kind].name)).collect();
            lines.push(format!("  the same AI script also runs kinds: {}", listed.join(", ")));
        }
    } else {
        lines.push("  (the model and AI script are read from the extracted disc; pass --extracted)".into());
    }
    let users = index::includers(&index::outlines(tree)?);
    let loads: Vec<Call> = index::calls(tree, &name_set(&MONSTER_CALLS))?
        .into_iter()
        .filter(|call| !call.args.is_empty() && is_number(index::value(&call.args[0], consts).as_ref(), number))
        .collect();
    lines.extend([String::new(), "loaded or placed by:".to_string()]);
    lines.extend(or_none(grouped(&loads, &users, None)?));
    Ok(lines.join("\n"))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A source tree of the files under a fresh folder in the temporary directory.
    pub(crate) fn tree(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("dc2tk-script-{name}-{}", std::process::id())).join("tree");
        let _ = std::fs::remove_dir_all(&root);
        for (path, text) in files {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        root
    }

    const FILES: [(&str, &str); 6] = [
        ("map/s/s99/s99_1.c", "PROGRAM(510, end_fight);\nPROGRAM(story_9, start_fight);\n#include \"lib/map/s/s99/fight.c\"\n"),
        ("map/s/s99/s99_2.c", "PROGRAM(510, end_fight);\nPROGRAM(story_9, start_fight);\n#include \"lib/map/s/s99/fight.c\"\n"),
        (
            "lib/map/s/s99/fight.c",
            "void start_fight(void)\n{\n    load_monster_data(MONSTER_GUARD, 1);\n    place_monster(MONSTER_GUARD, 1.0, 0.0, 2.0);\n}\n",
        ),
        (
            "map/d/d01/d01_1.c",
            "void warp(void)\n{\n    load_dungeon_map(\"s99\");\n    load_event_script(\"s99/s99.stb\", 9 * 10 + 10000);\n}\n",
        ),
        ("dungeon/monster/g7.c", "void on_death(void)\n{\n    run_event_script(510);\n}\n"),
        ("dungeon/monster/x1.c", "void on_death(void)\n{\n    run_event_script(510);\n}\n"),
    ];

    fn consts() -> IndexMap<String, i64> {
        IndexMap::from([("story_9".to_string(), 10090), ("MONSTER_GUARD".to_string(), 7)])
    }

    fn rows() -> Rows {
        let row = |name: &str, model: &str| MonsterRow { name: name.into(), model: model.into(), script: "g7".into() };
        IndexMap::from([(7, row("Guard", "e7")), (8, row("Elite Guard", "e8"))])
    }

    #[test]
    fn event_report_links_a_map_program_to_the_ai_that_requests_it() {
        let root = tree("xref-event-ai", &FILES);
        let text = event_report(&root, &consts(), 510, Some("s99"), &rows()).unwrap();
        assert!(text.contains("map/s/s99/s99_N.c  PROGRAM(510, end_fight)"));
        assert!(text.contains(
            "dungeon/monster/g7.c:3  run_event_script(510)  (in on_death)\n      <- s99 places kind 7 Guard"
        ));
        assert!((text.clone() + "\n").contains("dungeon/monster/x1.c:3  run_event_script(510)  (in on_death)\n"));
        assert!(!text.contains("x1.c:3  run_event_script(510)  (in on_death)\n      <-"));
    }

    #[test]
    fn event_report_finds_warps_that_start_the_program() {
        let root = tree("xref-event-warps", &FILES);
        let text = event_report(&root, &consts(), 10090, Some("s99"), &rows()).unwrap();
        assert!(text.contains("PROGRAM(10090, start_fight)"));
        assert!(text.contains("map/d/d01/d01_1.c:4  load_event_script(\"s99/s99.stb\", 9 * 10 + 10000)  (in warp)"));
    }

    #[test]
    fn map_report_lists_scripts_and_entries() {
        let root = tree("xref-map", &FILES);
        let text = map_report(&root, &consts(), "s99").unwrap();
        assert!(text.contains("  map/s/s99/s99_1.c\n  map/s/s99/s99_2.c"));
        assert!(text.contains("load_dungeon_map(\"s99\")"));
        assert!(text.contains("starts program 10090"));
    }

    #[test]
    fn monster_report_gives_files_sharing_and_users() {
        let root = tree("xref-monster", &FILES);
        let text = monster_report(&root, &consts(), &rows(), "guard").unwrap();
        assert!(text.starts_with("monster kind 7 Guard (MONSTER_GUARD)"));
        assert!(text.contains("AI script: dungeon/monster/g7.c"));
        assert!(text.contains("the same AI script also runs kinds: 8 Elite Guard"));
        assert!(text.contains("lib/map/s/s99/fight.c:3  load_monster_data(MONSTER_GUARD, 1)"));
        assert!(text.contains("library included by 2 scripts: map/s/s99/s99_N.c"));
    }
}
