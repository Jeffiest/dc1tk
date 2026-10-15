//! The game files a mod's scripts load by name: character packs, pictures, messages, sound banks,
//! packs and the members inside them, given as string literals. `mod check` looks each up on the
//! disc and warns about the ones it does not find, which would fail only in the game (a load of a
//! missing file returns 0 and the scene goes on without it).
//!
//! The engine opens a name in the current directory (dataread.cpp GetFullPath: the directory
//! change_dir or set_current_dir last set, then the name), and a character pack's model by the
//! member name given (character.cpp LoadPack). The directory is followed through each function
//! in the order of its text: a load after change_dir("event/e00/e50/") in the same function is
//! looked up there; a load in a function that sets none is looked up from the top and, failing
//! that, anywhere on the disc by its file name, since the directory its caller set is not known.
//! After load_pack_file, loads in the same function may come from that pack and are not checked.

use std::collections::HashSet;
use std::sync::LazyLock;

use indexmap::IndexMap;
use regex::Regex;

use crate::index::{self, Folded};

/// The member a load reads from the pack it names: an argument, or the one a helper passes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Member {
    None,
    Arg(usize),
    Fixed(&'static str),
}

/// Calls that load a file by name: (name, the argument holding the file, the member read from it).
/// load_sound and start_background_load take their files among other arguments (any string).
const LOADS: [(&str, usize, Member); 17] = [
    ("load_chara", 1, Member::Arg(2)),
    ("load_chara_motion", 1, Member::Arg(2)),
    ("load_chara_skin", 1, Member::Arg(2)),
    ("load_image", 1, Member::None),
    ("load_message_file", 2, Member::None),
    ("load_pack_file", 0, Member::None),
    ("load_arg_script", 1, Member::None),
    ("load_effect_script_pack", 1, Member::None),
    ("load_sphida_pin_collision", 1, Member::None),
    ("load_reserve_image", 1, Member::None),
    ("seek_disc_file", 0, Member::None),
    ("load_chara_actor", 1, Member::Fixed("info.cfg")),
    ("load_scene_chara", 1, Member::Fixed("info.cfg")),
    ("load_active_chara", 1, Member::Fixed("info.cfg")),
    ("load_inactive_chara", 1, Member::Fixed("info.cfg")),
    ("add_chara_motions", 1, Member::Fixed("info.cfg")),
    ("load_party_member", 1, Member::Fixed("info.cfg")),
];

/// Calls whose every string argument is a file.
const ANY_STRING: [&str; 2] = ["load_sound", "start_background_load"];

const DIRECTORY: [&str; 2] = ["change_dir", "set_current_dir"];

/// A load of a file by name in a script's text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Load {
    pub line: usize,
    pub call: String,
    pub name: String,
    /// The current directory at the call, when the function set it ("" for the top).
    pub dir: Option<String>,
    /// Every directory the function set before the call: a load in a branch may run with any.
    pub dirs: Vec<String>,
    /// The member read from the pack.
    pub member: Option<String>,
    /// Whether load_pack_file came before it in the function.
    pub after_pack: bool,
}

static DEFINITION_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^(?:static\s+inline\s+)?(?:void|int|float|string)\s+\**(\w+)\s*\(").unwrap());

static CALL_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    let names: Vec<&str> = LOADS.iter().map(|(name, _, _)| *name).chain(ANY_STRING).chain(DIRECTORY).collect();
    Regex::new(&format!(r"\b({})\s*\(", names.join("|"))).unwrap()
});

fn text_of(arg: Option<&String>, consts: &IndexMap<String, i64>) -> Option<String> {
    match index::value(arg?, consts) {
        Some(Folded::Text(text)) => Some(text),
        _ => None,
    }
}

/// The directory change_dir or set_current_dir sets: "" for the top (no argument, "" or "/"),
/// otherwise the text without a leading "/"; None when it is not a literal.
fn directory(args: &[String], consts: &IndexMap<String, i64>) -> Option<String> {
    if args.is_empty() {
        return Some(String::new());
    }
    let dir = text_of(args.first(), consts)?;
    let dir = dir.strip_prefix('/').unwrap_or(&dir).to_string();
    Some(dir)
}

/// The loads by name in a script's text, with the directory each is opened in.
pub fn loads(text: &str, consts: &IndexMap<String, i64>) -> Vec<Load> {
    let text = index::without_comments(text);
    let starts: Vec<usize> = DEFINITION_PATTERN.find_iter(&text).map(|found| found.start()).collect();
    let mut found = Vec::new();
    let mut dir: Option<String> = None;
    let mut dirs: Vec<String> = Vec::new();
    let mut after_pack = false;
    let mut function = usize::MAX;
    for capture in CALL_PATTERN.captures_iter(&text) {
        let whole = capture.get(0).unwrap();
        let line_start = text[..whole.start()].rfind('\n').map_or(0, |at| at + 1);
        if index::in_string(&text[line_start..whole.start()]) {
            continue;
        }
        // a new function: the directory its caller set is not known
        let within = starts.iter().take_while(|&&start| start <= whole.start()).count();
        if within != function {
            function = within;
            dir = None;
            dirs.clear();
            after_pack = false;
        }
        let Some(end) = index::closing_paren(&text, whole.end() - 1) else {
            continue;
        };
        if DEFINITION_PATTERN.find(&text[line_start..whole.end()]).is_some_and(|found| found.start() == 0) {
            continue;
        }
        let name = &capture[1];
        let args = index::split_args(&text[whole.end()..end]);
        let line = text[..whole.start()].matches('\n').count() + 1;
        if DIRECTORY.contains(&name) {
            dir = directory(&args, consts);
            if let Some(dir) = &dir {
                if !dirs.contains(dir) {
                    dirs.push(dir.clone());
                }
            }
            continue;
        }
        let load = |file: String, member: Option<String>| Load {
            line,
            call: name.to_string(),
            name: file,
            dir: dir.clone(),
            dirs: dirs.clone(),
            member,
            after_pack,
        };
        if ANY_STRING.contains(&name) {
            for arg in &args {
                if let Some(file) = text_of(Some(arg), consts) {
                    found.push(load(file, None));
                }
            }
            continue;
        }
        let Some(&(_, at, member)) = LOADS.iter().find(|(known, _, _)| *known == name) else {
            continue;
        };
        let Some(file) = text_of(args.get(at), consts) else {
            continue;
        };
        let member = match member {
            Member::None => None,
            Member::Arg(at) => text_of(args.get(at), consts),
            Member::Fixed(member) => Some(member.to_string()),
        };
        found.push(load(file, member));
        if name == "load_pack_file" {
            after_pack = true;
        }
    }
    found
}

/// The files of the disc (and the ones the chosen mods add or replace), by path, ignoring case.
#[derive(Default)]
pub struct Known {
    paths: HashSet<String>,
    names: HashSet<String>,
    /// Paths whose file a mod gives, so their members are the mod's.
    modded: HashSet<String>,
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

impl Known {
    pub fn new<'a>(disc: impl IntoIterator<Item = &'a str>, modded: impl IntoIterator<Item = &'a str>) -> Known {
        let mut known = Known::default();
        for path in disc {
            known.insert(path);
        }
        for path in modded {
            let lower = known.insert(path);
            known.modded.insert(lower);
        }
        known
    }

    fn insert(&mut self, path: &str) -> String {
        let lower = path.replace('\\', "/").trim_start_matches('/').to_lowercase();
        // a pack member a mod gives (a.pac.d/b.chr) is a file of that name too
        self.names.insert(file_name(&lower).to_string());
        self.paths.insert(lower.clone());
        lower
    }

    pub fn has(&self, path: &str) -> bool {
        self.paths.contains(&path.to_lowercase())
    }
}

/// What is wrong with each load: a file the disc does not have where it is opened (or anywhere,
/// when the directory is not known), or a member its pack lacks. members gives a pack's member
/// names by its path on the disc. Each warning is the call and what is missing.
pub fn check(loads: &[Load], known: &Known, members: &dyn Fn(&str) -> Option<Vec<String>>) -> Vec<(Load, String)> {
    let mut out = Vec::new();
    for load in loads {
        let name = load.name.replace('\\', "/");
        // a device path (host:, cdrom0:) is not a file of DATA.DAT
        if name.is_empty() || name.contains(':') {
            continue;
        }
        let path = match &load.dir {
            Some(dir) => format!("{dir}{name}"),
            None => name.trim_start_matches('/').to_string(),
        };
        // where the call may run: the directory last set, or any the function set before it
        let found = std::iter::once(path.clone())
            .chain(load.dirs.iter().map(|dir| format!("{dir}{name}")))
            .find(|path| known.has(path));
        let Some(found) = found else {
            if load.after_pack {
                continue;
            }
            match &load.dir {
                None if !load.dirs.is_empty() => {
                    out.push((load.clone(), format!("the disc has no {name} in {}", load.dirs.join(", "))))
                }
                Some(dir) => out.push((
                    load.clone(),
                    format!(
                        "the disc has no {path} (the directory is {})",
                        if dir.is_empty() { "the top" } else { dir }
                    ),
                )),
                None if known.names.contains(&file_name(&name).to_lowercase()) => {}
                None => out.push((load.clone(), format!("the disc has no file named {}", file_name(&name)))),
            }
            continue;
        };
        let Some(member) = &load.member else {
            continue;
        };
        if known.modded.contains(&found.to_lowercase()) {
            continue;
        }
        if let Some(names) = members(&found) {
            if !names.iter().any(|known| known.eq_ignore_ascii_case(member)) {
                out.push((load.clone(), format!("{found} has no member {member}")));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCRIPT: &str = r#"
void play_scene(void)
{
    load_sound(800, &bank);
    change_dir("event/e00/e50/");
    load_message_file(0, MEMORY_STACK_EVENT, "e50.txt");  // a comment "x.chr"
    load_inactive_chara(11, "e50_soldier.chr");
    load_chara(5, "e50_soldier.chr", "missing.cfg", 12);
    load_inactive_chara(13, "e50_nothing.chr");
    load_sound(4, "snd2/event/EV_000_130.snd", &bank);
}

void other(void)
{
    load_active_chara(11, "e50_soldier.chr");
    load_active_chara(12, "nowhere.chr");
    change_dir();
    start_background_load("snd2/bgm/BG_060.snd", "dungeon/image/gameover.img", 12, 1);
}
"#;

    #[test]
    fn loads_are_found_with_their_directory() {
        let found = loads(SCRIPT, &IndexMap::new());
        let shown: Vec<(String, Option<String>, Option<String>)> =
            found.iter().map(|load| (load.name.clone(), load.dir.clone(), load.member.clone())).collect();
        let dir = Some("event/e00/e50/".to_string());
        assert_eq!(
            shown,
            vec![
                ("e50.txt".into(), dir.clone(), None),
                ("e50_soldier.chr".into(), dir.clone(), Some("info.cfg".into())),
                ("e50_soldier.chr".into(), dir.clone(), Some("missing.cfg".into())),
                ("e50_nothing.chr".into(), dir.clone(), Some("info.cfg".into())),
                ("snd2/event/EV_000_130.snd".into(), dir.clone(), None),
                ("e50_soldier.chr".into(), None, Some("info.cfg".into())),
                ("nowhere.chr".into(), None, Some("info.cfg".into())),
                ("snd2/bgm/BG_060.snd".into(), Some(String::new()), None),
                ("dungeon/image/gameover.img".into(), Some(String::new()), None),
            ]
        );
    }

    #[test]
    fn missing_files_and_members_are_named() {
        let disc = [
            "event/e00/e50/e50.txt",
            "event/e00/e50/e50_soldier.chr",
            "snd2/bgm/BG_060.snd",
            "dungeon/image/gameover.img",
        ];
        let known = Known::new(disc, []);
        let members = |path: &str| -> Option<Vec<String>> {
            (path == "event/e00/e50/e50_soldier.chr").then(|| vec!["info.cfg".to_string(), "a.mds".to_string()])
        };
        let found = loads(SCRIPT, &IndexMap::new());
        let problems: Vec<String> = check(&found, &known, &members).into_iter().map(|(_, text)| text).collect();
        assert_eq!(
            problems,
            vec![
                "event/e00/e50/e50_soldier.chr has no member missing.cfg",
                "the disc has no event/e00/e50/e50_nothing.chr (the directory is event/e00/e50/)",
                "the disc has no event/e00/e50/snd2/event/EV_000_130.snd (the directory is event/e00/e50/)",
                "the disc has no file named nowhere.chr",
            ]
        );
        // a file a mod adds is there
        let known = Known::new(disc, ["event/e00/e50/e50_nothing.chr"]);
        let problems = check(&found, &known, &members);
        assert!(!problems.iter().any(|(_, text)| text.contains("e50_nothing")));
    }

    /// The game's own scripts load files the disc has, but for a few the retail game lacks too (test
    /// maps, eng_ message files and motion packs it never shipped): the check finds the loads and
    /// places their directories. Set C2S_CORPUS to the extracted disc, with the decompiled tree
    /// beside it (extracted/scripts).
    #[test]
    #[ignore = "needs C2S_CORPUS"]
    fn the_games_scripts_load_files_the_disc_has() {
        let disc = dc2tk_disc::samples::corpus();
        let tree = disc.parent().unwrap().join("scripts");
        assert!(tree.is_dir(), "this test reads the decompiled tree beside C2S_CORPUS: {}", tree.display());
        let index = dc2tk_disc::archive::parse_index(&std::fs::read(disc.join("DATA.HD4")).unwrap()).unwrap();
        let known = Known::new(index.iter().map(|entry| entry.path.as_str()), []);
        let members = |path: &str| -> Option<Vec<String>> {
            let data = std::fs::read(disc.join(path)).ok()?;
            Some(dc2tk_disc::archive::parse_pack(&data)?.into_iter().map(|member| member.name).collect())
        };
        let mut problems = Vec::new();
        let mut total = 0;
        for relative in index::source_files(&tree).unwrap() {
            let text = dc2tk_disc::textfile::read_text(&tree.join(&relative)).unwrap();
            let found = loads(&text, &IndexMap::new());
            total += found.len();
            for (load, problem) in check(&found, &known, &members) {
                problems.push(format!("{relative}:{} {}: {problem}", load.line, load.call));
            }
        }
        for problem in &problems {
            println!("{problem}");
        }
        println!("{} of {total} loads not found", problems.len());
        // 52 of 14,818 on the PAL disc
        assert!(total > 10000 && problems.len() * 100 < total, "{} of {total} loads not found", problems.len());
    }
}
