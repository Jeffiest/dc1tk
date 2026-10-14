//! Carried over from the Python tool's test_patches.py: mods' patches merged into the scripts they target, run in the VM.

mod build_common;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use build_common::{disc_for, fails, make_tree, map, read, recorded, sdk_dir, strings, tables, write, TempDir, Tools};
use c2_script::node::Node;
use c2_script::patches::{self, Plan, Scripts, Target};
use c2_script::renames::{self, Followed};
use c2_script::vm::{self, Machine, Value};
use c2_script::{build, definitions, editor, library, mods, numbers, parse, pathnames, sdk, Result};
use indexmap::IndexMap;

// A map in two languages and a monster, sharing a library, as the decompiler lays them out.
const MAP: &str = "#pragma natives event\n\nint visits;\n\nPROGRAM(100, start);\nPROGRAM(500, talk);\nMAIN(start);\n\n\
                   #include \"lib/map/s/s17/greet.c\"\n\nvoid start(void)\n{\n    greet();\n}\n\n\
                   void talk(void)\n{\n    int frames;\n\n    frames = 3;\n    while (frames > 0) {\n        wait;\n\
                   \x20       frames = frames - 1;\n    }\n    print(\"talked\");\n}\n";
const GREET: &str = "void greet(void)\n{\n    print(\"hello\");\n}\n";
const MONSTER: &str =
    "#pragma natives monster\n\nPROGRAM(0, start);\nMAIN(start);\n\nvoid start(void)\n{\n    print(\"roar\");\n}\n";
const DISC_H: &str = "enum monster_kind {\n    MONSTER_GOLEM = 168,  // model e33a, AI e33a\n};\n";
const FILES: [(&str, &str); 7] = [
    ("map/s/s17_valley/s17_1.c", MAP),
    ("map/s/s17_valley/s17_2.c", MAP),
    ("map/s/s17_valley/s17.c", MAP),
    ("map/s/s18/s18_1.c", MAP),
    ("lib/map/s/s17/greet.c", GREET),
    ("dungeon/monster/e33a_golem.c", MONSTER),
    ("include/disc.h", DISC_H),
];

type ModFiles<'a> = [(&'a str, &'a [(&'a str, &'a str)])];

fn setup(folder: &Path, mod_files: &ModFiles) -> (PathBuf, Vec<PathBuf>) {
    let root = make_tree(folder, &FILES);
    pathnames::record(
        &root,
        &map(&[
            ("map/s/s17/s17_1.c", "map/s/s17_valley/s17_1.c"),
            ("map/s/s17/s17_2.c", "map/s/s17_valley/s17_2.c"),
            ("map/s/s17/s17.c", "map/s/s17_valley/s17.c"),
            ("dungeon/monster/e33a.c", "dungeon/monster/e33a_golem.c"),
        ]),
    )
    .unwrap();
    let mut dirs = Vec::new();
    for (name, texts) in mod_files {
        let module = folder.join("mods").join(name);
        fs::create_dir_all(&module).unwrap();
        for (path, text) in *texts {
            write(&module, path, text);
        }
        dirs.push(module);
    }
    (root, dirs)
}

fn plan_of(dirs: &[PathBuf], root: &Path) -> Result<Plan> {
    mods::patch_plan(dirs, root, &IndexMap::new(), true)
}

fn program_of(root: &Path, plan: &Plan, script: &str) -> Result<Node> {
    let path = root.join(script);
    let program = parse::parse(&read(&path))?;
    let includes = library::read_includes(&program, &path, None, &sdk_dir(), &[])?;
    let program = library::expand(program, &includes, &IndexMap::new())?;
    let found = plan.for_script(script).unwrap_or_default();
    patches::applied(program, &found, script, root, &IndexMap::new(), None, &sdk_dir())
}

fn machine_globals(program: &Node) -> Vec<Value> {
    vec![Value::Int(0); program["global_num"].as_i64().unwrap() as usize]
}

fn run(program: &Node, name: &str) -> Vec<String> {
    let table = vm::table_of(program, &tables()).unwrap();
    let mut machine = Machine::new(&table, machine_globals(program));
    machine.run(name, Vec::new()).unwrap();
    machine.printed
}

#[test]
fn an_override_replaces_a_library_function_in_the_patched_scripts_only() {
    let patch = "#pragma target map s17\n\n#pragma override\nvoid greet(void)\n{\n    print(\"hi there\");\n}\n";
    let folder = TempDir::new("patches-override");
    let (root, dirs) = setup(folder.path(), &[("hello", &[("s17.c", patch)])]);
    let plan = plan_of(&dirs, &root).unwrap();
    // the two language variants, not the Japanese script the PAL game never loads
    assert_eq!(
        plan.scripts.keys().cloned().collect::<Vec<_>>(),
        strings(&["map/s/s17_valley/s17_1.c", "map/s/s17_valley/s17_2.c"])
    );
    assert_eq!(run(&program_of(&root, &plan, "map/s/s17_valley/s17_2.c").unwrap(), "start"), strings(&["hi there"]));
    assert_eq!(run(&program_of(&root, &plan, "map/s/s18/s18_1.c").unwrap(), "start"), strings(&["hello"]));
    assert_eq!(read(&root.join("lib/map/s/s17/greet.c")), GREET);
}

#[test]
fn original_runs_the_replaced_function_and_mods_chain() {
    let first =
        "#pragma target map s17\n\n#pragma override\nvoid greet(void)\n{\n    print(\"first\");\n    original();\n}\n";
    let second =
        "#pragma target map s17\n\n#pragma override\nvoid greet(void)\n{\n    original();\n    print(\"second\");\n}\n";
    let folder = TempDir::new("patches-original");
    let (root, dirs) = setup(folder.path(), &[("a", &[("p.c", first)]), ("b", &[("p.c", second)])]);
    let plan = plan_of(&dirs, &root).unwrap();
    assert_eq!(
        run(&program_of(&root, &plan, "map/s/s17_valley/s17_1.c").unwrap(), "start"),
        strings(&["first", "hello", "second"])
    );
}

#[test]
fn two_mods_replacing_one_function_without_original_are_refused() {
    let replace = |text: &str| {
        format!("#pragma target map s17\n\n#pragma override\nvoid greet(void)\n{{\n    print(\"{text}\");\n}}\n")
    };
    let (a, b) = (replace("a"), replace("b"));
    let folder = TempDir::new("patches-refused");
    let (root, dirs) = setup(folder.path(), &[("a", &[("p.c", &a)]), ("b", &[("p.c", &b)])]);
    let plan = plan_of(&dirs, &root).unwrap();
    fails(program_of(&root, &plan, "map/s/s17_valley/s17_1.c"), "which mod a overrides too");
}

#[test]
fn overrides_are_checked_against_the_script() {
    let folder = TempDir::new("patches-checked");
    for (text, words) in [
        ("#pragma override\nvoid greeet(void)\n{\n}\n", "close: greet"),
        ("#pragma override\nvoid greet(int loud)\n{\n}\n", "takes 1 parameters"),
        ("void greet(void)\n{\n}\n", "put #pragma override before it"),
        ("void mine(void)\n{\n    original();\n}\n", "only a #pragma override function"),
        ("PROGRAM(500, mine);\nvoid mine(void)\n{\n}\n", "override talk instead"),
        ("int visits;\n", "already has a global visits"),
    ] {
        let patch = format!("#pragma target map s17\n{text}");
        let (root, dirs) = setup(&folder.path().join(words.len().to_string()), &[("m", &[("p.c", &patch)])]);
        let plan = plan_of(&dirs, &root).unwrap();
        fails(program_of(&root, &plan, "map/s/s17_valley/s17_1.c"), words);
    }
}

#[test]
fn mods_adding_the_same_names_each_keep_their_own() {
    let patch = |number: i64, step: i64| {
        format!(
            "#pragma target map s17\n\nint count;\n\nPROGRAM({number}, bump);\n\nvoid bump(void)\n{{\n    count = count + {step};\n    print(count);\n}}\n"
        )
    };
    let (a, b) = (patch(9000, 1), patch(9001, 10));
    let folder = TempDir::new("patches-names");
    let (root, dirs) = setup(folder.path(), &[("a", &[("p.c", &a)]), ("b", &[("p.c", &b)])]);
    let program = program_of(&root, &plan_of(&dirs, &root).unwrap(), "map/s/s17_valley/s17_1.c").unwrap();
    let by_number: HashMap<i64, String> = program["programs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| (entry["no"].as_i64().unwrap(), entry["function"].as_str().unwrap().to_string()))
        .collect();
    assert!(by_number[&9000] == "bump" && by_number[&9001] == "bump_b");
    let globals: Vec<&str> =
        program["globals"].as_array().unwrap().iter().map(|declared| declared["name"].as_str().unwrap()).collect();
    assert_eq!(globals, ["visits", "count", "count_b"]);
    let table = vm::table_of(&program, &tables()).unwrap();
    let mut machine = Machine::new(&table, machine_globals(&program));
    for name in ["bump", "bump_b", "bump"] {
        machine.run(name, Vec::new()).unwrap();
    }
    assert_eq!(machine.printed, strings(&["1", "10", "2"]));
}

#[test]
fn every_frame_functions_run_before_each_wait() {
    let patch = "#pragma target map s17\n\n#pragma every_frame\nvoid tick(void)\n{\n    print(\"tick\");\n}\n";
    let folder = TempDir::new("patches-every-frame");
    let (root, dirs) = setup(folder.path(), &[("m", &[("p.c", patch)])]);
    let program = program_of(&root, &plan_of(&dirs, &root).unwrap(), "map/s/s17_valley/s17_1.c").unwrap();
    assert_eq!(run(&program, "talk"), strings(&["tick", "tick", "tick", "talked"]));
    let waiting = patch.replace("print(\"tick\");", "wait;");
    let (root, dirs) = setup(&folder.path().join("waits"), &[("m", &[("p.c", &waiting)])]);
    fails(program_of(&root, &plan_of(&dirs, &root).unwrap(), "map/s/s17_valley/s17_1.c"), "tick waits");
}

#[test]
fn language_picks_each_variants_text() {
    let patch = "#pragma target map s17\n\n#pragma override\nvoid greet(void)\n{\n#if LANGUAGE == LANGUAGE_FRENCH\n\
                 \x20   print(\"bonjour\");\n#else\n    print(\"hello\");\n#endif\n    print(LANGUAGE);\n}\n";
    let folder = TempDir::new("patches-language");
    let (root, dirs) = setup(folder.path(), &[("m", &[("p.c", patch)])]);
    let plan = plan_of(&dirs, &root).unwrap();
    assert_eq!(run(&program_of(&root, &plan, "map/s/s17_valley/s17_1.c").unwrap(), "start"), strings(&["hello", "1"]));
    assert_eq!(
        run(&program_of(&root, &plan, "map/s/s17_valley/s17_2.c").unwrap(), "start"),
        strings(&["bonjour", "2"])
    );
}

fn copies(pairs: &[(i64, &str)]) -> BTreeMap<i64, String> {
    pairs.iter().map(|(language, text)| (*language, text.to_string())).collect()
}

#[test]
fn copies_from_several_languages_keep_each_ones_lines() {
    let english = "void f(void)\n{\n    wait;\n    say(\"Hello\");\n}\n";
    let french = english.replace("Hello", "Bonjour");
    assert_eq!(patches::language_text(&copies(&[(1, english), (3, english)])), english);
    assert_eq!(
        patches::language_text(&copies(&[(2, &french), (1, english)])),
        "void f(void)\n{\n    wait;\n#if LANGUAGE == LANGUAGE_FRENCH\n    say(\"Bonjour\");\n#else\n    say(\"Hello\");\n#endif\n}\n"
    );
    let longer = french.replace("    wait;\n", "    wait;\n    wait;\n");
    assert_eq!(
        patches::language_text(&copies(&[(1, english), (2, &longer)])),
        format!("#if LANGUAGE == LANGUAGE_FRENCH\n{longer}#else\n{english}#endif\n")
    );
    // the merged text reads back as each language's own
    let body = |text: &str, defines: &HashMap<String, String>| -> Vec<Node> {
        let program = parse::parse_with(text, defines).unwrap();
        program["functions"][0]["body"]
            .as_array()
            .unwrap()
            .iter()
            .map(|node| {
                let mut node = node.clone();
                node.as_object_mut().unwrap().remove("line");
                node
            })
            .collect()
    };
    for (language, text) in [(1, english), (2, french.as_str())] {
        let merged = patches::language_text(&copies(&[(1, english), (2, &french)]));
        let mut defines: HashMap<String, String> =
            patches::LANGUAGES.iter().map(|(name, value)| (name.to_string(), value.to_string())).collect();
        defines.insert("LANGUAGE".into(), language.to_string());
        assert_eq!(body(&merged, &defines), body(text, &HashMap::new()));
    }
}

fn picked(scripts: &Scripts, kind: &str, value: &str) -> BTreeSet<String> {
    scripts.picked(&Target { kind: kind.into(), values: vec![value.into()] }).unwrap()
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|item| item.to_string()).collect()
}

#[test]
fn targets_pick_monsters_libraries_and_paths() {
    let folder = TempDir::new("patches-targets");
    let (root, _) = setup(folder.path(), &[]);
    let scripts = Scripts::new(&root).unwrap();
    assert_eq!(picked(&scripts, "monster", "golem"), set(&["dungeon/monster/e33a_golem.c"]));
    assert_eq!(picked(&scripts, "monster", "MONSTER_GOLEM"), set(&["dungeon/monster/e33a_golem.c"]));
    assert_eq!(
        picked(&scripts, "library", "lib/map/s/s17/greet.c"),
        set(&["map/s/s17_valley/s17_1.c", "map/s/s17_valley/s17_2.c", "map/s/s17_valley/s17.c", "map/s/s18/s18_1.c"])
    );
    assert_eq!(
        picked(&scripts, "script", "map/s/s17/s17_?.c"),
        set(&["map/s/s17_valley/s17_1.c", "map/s/s17_valley/s17_2.c"])
    );
    let (_, dirs) = setup(&folder.path().join("none"), &[("m", &[("p.c", "#pragma target map s99\n")])]);
    fails(plan_of(&dirs, &root), "picks no script");
}

fn given(pairs: &[(&str, &[(&str, i64)])]) -> BTreeMap<String, IndexMap<String, i64>> {
    pairs
        .iter()
        .map(|(module, names)| {
            (module.to_string(), names.iter().map(|(name, number)| (name.to_string(), *number)).collect())
        })
        .collect()
}

#[test]
fn numbers_are_given_once_kept_and_never_shared() {
    let shared = "NEW_FLAG(FIGHT);\nNEW_PROGRAM(SHOW);\n";
    let patch = "#pragma target map s17\n#include \"names.h\"\n\nPROGRAM(SHOW, show);\n\nvoid show(void)\n{\n    print(FIGHT, SHOW);\n}\n";
    let folder = TempDir::new("patches-numbers");
    let (root, dirs) = setup(
        folder.path(),
        &[
            ("a", &[("names.h", shared), ("p.c", patch)]),
            ("b", &[("names.h", shared), ("p.c", patch), ("old.c", "void f(void)\n{\n    set_flag(1025, 1);\n}\n")]),
        ],
    );
    let lock = folder.path().join("mods").join(numbers::LOCK);
    let plan = plan_of(&dirs, &root).unwrap();
    // b writes flag 1025 by hand, so neither mod is given it
    assert_eq!(
        plan.numbers,
        given(&[("a", &[("FIGHT", 1024), ("SHOW", 9000)]), ("b", &[("FIGHT", 1026), ("SHOW", 9001)])])
    );
    let program = program_of(&root, &plan, "map/s/s17_valley/s17_1.c").unwrap();
    let numbers: BTreeSet<i64> =
        program["programs"].as_array().unwrap().iter().map(|entry| entry["no"].as_i64().unwrap()).collect();
    assert_eq!(numbers, BTreeSet::from([100, 500, 9000, 9001]));
    assert_eq!(run(&program, "show"), strings(&["1024 9000"]));
    assert_eq!(run(&program, "show_b"), strings(&["1026 9001"]));
    let held: serde_json::Value = serde_json::from_str(&read(&lock)).unwrap();
    assert_eq!(held["flag"], serde_json::json!({"a/FIGHT": 1024, "b/FIGHT": 1026}));
    // b alone keeps its numbers, and a's stay reserved
    assert_eq!(plan_of(&dirs[1..], &root).unwrap().numbers, given(&[("b", &[("FIGHT", 1026), ("SHOW", 9001)])]));
    let mine = dirs[0].parent().unwrap().join("c");
    fs::create_dir_all(&mine).unwrap();
    write(&mine, "p.c", "NEW_FLAG(MINE);\n");
    assert_eq!(plan_of(&[mine], &root).unwrap().numbers, given(&[("c", &[("MINE", 1027)])]));
}

#[test]
fn a_number_the_tree_takes_later_is_reported() {
    let folder = TempDir::new("patches-taken");
    let (root, dirs) = setup(folder.path(), &[("a", &[("p.c", "NEW_FLAG(FIGHT);\n")])]);
    assert_eq!(plan_of(&dirs, &root).unwrap().numbers, given(&[("a", &[("FIGHT", 1024)])]));
    write(&root, "map/s/s18/s18_1.c", &MAP.replace("print(\"talked\");", "set_flag(1024, 1);"));
    fails(plan_of(&dirs, &root), "which something else now uses");
}

#[test]
fn build_compiles_patched_scripts_and_override_starts_a_patch() {
    let tools = Tools::new();
    let folder = TempDir::new("patches-build");
    let root = recorded(folder.path());
    let iso_path = disc_for(&tools, folder.path(), &root);
    let mods_dir = folder.path().join("mods");
    let module = mods_dir.join("quiet");
    let helper = strings(&["helper"]);
    let result = mods::override_functions(&root, &module, "script", "map/m/t01.c", &helper).unwrap();
    assert!(result.added == helper && result.scripts == strings(&["map/m/t01.c"]));
    let text = read(&result.path);
    assert!(text.contains("#pragma target script map/m/t01.c"), "{text}");
    assert!(text.contains("#pragma override\nvoid helper(void)\n{\n    wait;\n}"), "{text}");
    assert!(mods::override_functions(&root, &module, "script", "map/m/t01.c", &helper).unwrap().added.is_empty());
    c2_py::write_text(&result.path, &text.replace("    wait;\n", "    wait;\n    wait;\n")).unwrap();
    let modules = [module];
    let plan = plan_of(&modules, &root).unwrap();
    let found = build::changes(&root, &iso_path, &tools.toolchain(), &IndexMap::new(), false, Some(&plan)).unwrap();
    assert_eq!(found.scripts.iter().map(|change| change.source.as_str()).collect::<Vec<_>>(), ["map/m/t01.c"]);
    let checking = Tools::with_headers(sdk::header_texts(&definitions::Definitions::empty(), &sdk_dir()).unwrap());
    assert!(mods::check(&modules, &root, &checking.toolchain(), &IndexMap::new()).unwrap().errors.is_empty());
    assert!(mods::listing(&mods_dir, &root, &IndexMap::new())
        .unwrap()
        .contains("patch t01.c: 1 script; overrides helper"));
}

#[test]
fn editors_get_a_header_of_the_targets_declarations_that_build_never_reads() {
    let patch =
        "#pragma target map s17\n\n#pragma override\nvoid greet(void)\n{\n    visits = visits + 1;\n    talk();\n}\n";
    let folder = TempDir::new("patches-editor");
    let (root, dirs) = setup(folder.path(), &[("m", &[("p.c", patch), ("lib.c", "void mine(void)\n{\n}\n")])]);
    write(&root, "map/s/s17_valley/s17_1.c", &MAP.replace("void talk(void)", "// Talks.\nvoid talk(void)"));
    let before = plan_of(&dirs, &root).unwrap();
    let tools = Tools::new();
    let written = editor::write(&dirs[0], &root, &tools.toolchain(), &sdk_dir(), &[]).unwrap();
    assert_eq!(written, [dirs[0].join(editor::FOLDER).join("p.h")]);
    let header = read(&written[0]);
    for line in [
        "//   map/s/s17_valley/s17_1.c (#pragma target map s17)\n",
        "//   PROGRAM(500, talk);\n",
        "extern int visits;\n",
        "void greet(void);\n",
        "// Talks.\nvoid talk(void);\n",
    ] {
        assert!(header.contains(line), "{line}\n{header}");
    }
    let commands: serde_json::Value = serde_json::from_str(&read(&dirs[0].join(editor::COMMANDS))).unwrap();
    let arguments = |file: &str| -> Vec<String> {
        let entry = commands.as_array().unwrap().iter().find(|entry| entry["file"].as_str().unwrap().ends_with(file));
        entry.unwrap()["arguments"].as_array().unwrap().iter().map(|item| item.as_str().unwrap().to_string()).collect()
    };
    let (patched, library) = (arguments("/p.c"), arguments("/lib.c"));
    let at = patched.iter().rposition(|item| item == "-include").unwrap();
    assert!(patched[at + 1].ends_with("/m/.editor/p.h"), "{patched:?}");
    assert!(library.iter().all(|item| !item.ends_with(".h") || item.ends_with("c2s.h")), "{library:?}");
    // the header is not one of the mod's sources, so what build merges is as it was
    assert!(mods::sources(&dirs[0]).unwrap().iter().all(|(path, _)| path.extension().unwrap() == "c"));
    let after = plan_of(&dirs, &root).unwrap();
    assert_eq!(after.patches.iter().map(|patch| &patch.text).collect::<Vec<_>>(), [patch]);
    assert_eq!(
        program_of(&root, &after, "map/s/s17_valley/s17_1.c").unwrap(),
        program_of(&root, &before, "map/s/s17_valley/s17_1.c").unwrap()
    );
}

fn compiled_with(root: &Path, plan: &Plan, script: &str, tools: &Tools) -> Vec<u8> {
    let patched = build::Patched { tree: root, relative: script, patches: plan.for_script(script).unwrap() };
    build::compiled(&root.join(script), &tools.toolchain(), None, None, None, Some(&patched)).unwrap()
}

#[test]
fn an_override_follows_its_function_when_a_new_decompile_renames_it() {
    let patch = "#pragma target map s17\n\nvoid twice(void)\n{\n    greet();\n}\n\n\
                 #pragma override\nvoid greet(void)\n{\n    print(\"hi\");\n    original();\n}\n";
    let folder = TempDir::new("patches-renamed");
    let (root, dirs) = setup(folder.path(), &[("m", &[("p.c", patch)])]);
    let tools = Tools::new();
    let script = "map/s/s17_valley/s17_1.c";
    let mut plan = plan_of(&dirs, &root).unwrap();
    // the first check records the fingerprint of the function the patch overrides
    let found = renames::follow(&dirs, &mut plan, &root, &tools.toolchain(), &IndexMap::new()).unwrap();
    assert!(found.followed.is_empty() && found.notes.is_empty());
    let recorded = renames::read_lock(&dirs[0]).unwrap();
    let print = recorded["p.c"]["greet"].clone();
    assert_eq!(renames::script_prints(&root, script, &tools.toolchain(), &IndexMap::new()).unwrap()["greet"], print);
    let before = compiled_with(&root, &plan, script, &tools);
    // a later decompile names the library's function salute
    for path in ["map/s/s17_valley/s17_1.c", "map/s/s17_valley/s17_2.c", "lib/map/s/s17/greet.c"] {
        write(&root, path, &read(&root.join(path)).replace("greet(", "salute("));
    }
    fails(program_of(&root, &plan_of(&dirs, &root).unwrap(), script), "the script has no function greet");
    let mut plan = plan_of(&dirs, &root).unwrap();
    let found = renames::follow(&dirs, &mut plan, &root, &tools.toolchain(), &IndexMap::new()).unwrap();
    let followed = Followed { module: "m".into(), patch: "p.c".into(), old: "greet".into(), new: "salute".into() };
    assert_eq!(found.followed, [followed]);
    assert_eq!(run(&program_of(&root, &plan, script).unwrap(), "twice"), strings(&["hi", "hello"]));
    assert_eq!(run(&program_of(&root, &plan, script).unwrap(), "start"), strings(&["hi", "hello"]));
    // names are not in the bytes: the patched script compiles as it did before the rename
    assert_eq!(compiled_with(&root, &plan, script, &tools), before);
    // when two functions have the code, the patch must say which
    let library = read(&root.join("lib/map/s/s17/greet.c"));
    write(&root, "lib/map/s/s17/greet.c", &format!("{library}\n{}", library.replace("salute(", "wave(")));
    let mut ambiguous = plan_of(&dirs, &root).unwrap();
    let noted = renames::follow(&dirs, &mut ambiguous, &root, &tools.toolchain(), &IndexMap::new()).unwrap();
    assert!(noted.followed.is_empty());
    assert!(noted.notes[0].contains("has no greet; its code is in salute, wave"), "{:?}", noted.notes);
    write(&root, "lib/map/s/s17/greet.c", &library);
    // rename writes the new name into the mod, with its fingerprint
    assert_eq!(renames::rename(&dirs[0], &found.followed).unwrap(), [dirs[0].join("p.c")]);
    assert_eq!(read(&dirs[0].join("p.c")), patch.replace("greet", "salute"));
    assert_eq!(renames::read_lock(&dirs[0]).unwrap()["p.c"]["salute"], print);
    let mut plan = plan_of(&dirs, &root).unwrap();
    assert!(renames::follow(&dirs, &mut plan, &root, &tools.toolchain(), &IndexMap::new())
        .unwrap()
        .followed
        .is_empty());
}

fn names_of(dirs: &[PathBuf]) -> Vec<String> {
    dirs.iter().map(|dir| dir.file_name().unwrap().to_string_lossy().into_owned()).collect()
}

#[test]
fn required_mods_are_added_and_every_mod_comes_after_those_it_names() {
    let folder = TempDir::new("patches-requires");
    let (root, dirs) = setup(
        folder.path(),
        &[
            ("a", &[("mod.json", "{\"requires\": [\"b\"]}")]),
            ("b", &[("mod.json", "{\"after\": [\"c\", \"unknown\"]}")]),
            ("c", &[("p.c", "NEW_FLAG(C);\n")]),
        ],
    );
    let mods_dir = dirs[0].parent().unwrap();
    let found = mods::chosen(mods_dir, &strings(&["a", "c"])).unwrap();
    assert_eq!(names_of(&found.dirs), strings(&["c", "b", "a"]));
    assert_eq!(found.added, [("b".to_string(), "a".to_string())]);
    // after orders only mods that are chosen
    assert_eq!(names_of(&mods::chosen(mods_dir, &strings(&["a"])).unwrap().dirs), strings(&["b", "a"]));
    let listed = mods::listing(mods_dir, &root, &IndexMap::new()).unwrap();
    assert!(listed.contains("a  (0 files)\n    requires b\n"), "{listed}");
    assert!(listed.contains("    built after c, unknown\n"), "{listed}");
    write(&dirs[2], "mod.json", "{\"requires\": [\"a\"]}");
    fails(mods::chosen(mods_dir, &strings(&["c"])), "(c after a, a after b, b after c)");
    assert!(mods::listing(mods_dir, &root, &IndexMap::new()).unwrap().contains("go round in a cycle"));
    write(&dirs[2], "mod.json", "{\"requires\": [\"gone\"]}");
    fails(mods::chosen(mods_dir, &strings(&["c"])), "mod c requires gone, which is not in");
    write(&dirs[2], "mod.json", "{\"requires\": \"a\"}");
    fails(mods::chosen(mods_dir, &strings(&["c"])), "\"requires\" is a list of mod names");
}

#[test]
fn a_mod_uses_the_library_numbers_and_overrides_of_the_mods_it_requires() {
    let base_patch = "#pragma target map s17\n#include \"names.h\"\n\n#pragma override\nvoid greet(void)\n{\n    print(\"base\");\n    original();\n}\n";
    let patch = "#pragma target map s17\n#include \"base/names.h\"\n#include \"base/value.c\"\n\nNEW_FLAG(OWN);\n\n\
                 #pragma override\nvoid greet(void)\n{\n    print(\"top\", SHARED, OWN, base_value());\n    original();\n}\n";
    let folder = TempDir::new("patches-requires-use");
    let (root, dirs) = setup(
        folder.path(),
        &[
            ("top", &[("mod.json", "{\"requires\": [\"base\"]}"), ("p.c", patch)]),
            (
                "base",
                &[
                    ("names.h", "NEW_FLAG(SHARED);\n"),
                    ("value.c", "int base_value(void)\n{\n    return SHARED + 1;\n}\n"),
                    ("p.c", base_patch),
                ],
            ),
        ],
    );
    let chosen = mods::chosen(dirs[0].parent().unwrap(), &strings(&["top"])).unwrap();
    assert_eq!(names_of(&chosen.dirs), strings(&["base", "top"]));
    let plan = plan_of(&chosen.dirs, &root).unwrap();
    assert_eq!(plan.numbers, given(&[("base", &[("SHARED", 1024)]), ("top", &[("OWN", 1025)])]));
    // the later mod's override runs first, and it sees the flag the mod it requires was given
    assert_eq!(
        run(&program_of(&root, &plan, "map/s/s17_valley/s17_1.c").unwrap(), "start"),
        strings(&["top 1024 1025 1025", "base", "hello"])
    );
}
