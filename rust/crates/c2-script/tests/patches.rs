//! src/tests/test_patches.py: mods' patches merged into the scripts they target, run in the VM.

mod build_common;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use build_common::{
    disc_for, fails, make_tree, map, read, recorded, sdk_dir, strings, tables, write, TempDir, Tools,
};
use c2_script::node::Node;
use c2_script::patches::{self, Plan, Scripts, Target};
use c2_script::vm::{self, Machine, Value};
use c2_script::{build, definitions, library, mods, numbers, parse, pathnames, sdk, Result};
use indexmap::IndexMap;

// A map in two languages and a monster, sharing a library, as the decompiler lays them out.
const MAP: &str = "#pragma natives event\n\nint visits;\n\nPROGRAM(100, start);\nPROGRAM(500, talk);\nMAIN(start);\n\n\
                   #include \"lib/map/s/s17/greet.c\"\n\nvoid start(void)\n{\n    greet();\n}\n\n\
                   void talk(void)\n{\n    int frames;\n\n    frames = 3;\n    while (frames > 0) {\n        wait;\n\
                   \x20       frames = frames - 1;\n    }\n    print(\"talked\");\n}\n";
const GREET: &str = "void greet(void)\n{\n    print(\"hello\");\n}\n";
const MONSTER: &str = "#pragma natives monster\n\nPROGRAM(0, start);\nMAIN(start);\n\nvoid start(void)\n{\n    print(\"roar\");\n}\n";
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
    assert_eq!(plan.scripts.keys().cloned().collect::<Vec<_>>(), strings(&["map/s/s17_valley/s17_1.c", "map/s/s17_valley/s17_2.c"]));
    assert_eq!(run(&program_of(&root, &plan, "map/s/s17_valley/s17_2.c").unwrap(), "start"), strings(&["hi there"]));
    assert_eq!(run(&program_of(&root, &plan, "map/s/s18/s18_1.c").unwrap(), "start"), strings(&["hello"]));
    assert_eq!(read(&root.join("lib/map/s/s17/greet.c")), GREET);
}

#[test]
fn original_runs_the_replaced_function_and_mods_chain() {
    let first = "#pragma target map s17\n\n#pragma override\nvoid greet(void)\n{\n    print(\"first\");\n    original();\n}\n";
    let second = "#pragma target map s17\n\n#pragma override\nvoid greet(void)\n{\n    original();\n    print(\"second\");\n}\n";
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
    let replace = |text: &str| format!("#pragma target map s17\n\n#pragma override\nvoid greet(void)\n{{\n    print(\"{text}\");\n}}\n");
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
    let globals: Vec<&str> = program["globals"].as_array().unwrap().iter().map(|declared| declared["name"].as_str().unwrap()).collect();
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
    assert_eq!(run(&program_of(&root, &plan, "map/s/s17_valley/s17_2.c").unwrap(), "start"), strings(&["bonjour", "2"]));
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
    assert_eq!(picked(&scripts, "script", "map/s/s17/s17_?.c"), set(&["map/s/s17_valley/s17_1.c", "map/s/s17_valley/s17_2.c"]));
    let (_, dirs) = setup(&folder.path().join("none"), &[("m", &[("p.c", "#pragma target map s99\n")])]);
    fails(plan_of(&dirs, &root), "picks no script");
}

fn given(pairs: &[(&str, &[(&str, i64)])]) -> BTreeMap<String, IndexMap<String, i64>> {
    pairs
        .iter()
        .map(|(module, names)| (module.to_string(), names.iter().map(|(name, number)| (name.to_string(), *number)).collect()))
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
    assert_eq!(plan.numbers, given(&[("a", &[("FIGHT", 1024), ("SHOW", 9000)]), ("b", &[("FIGHT", 1026), ("SHOW", 9001)])]));
    let program = program_of(&root, &plan, "map/s/s17_valley/s17_1.c").unwrap();
    let numbers: BTreeSet<i64> = program["programs"].as_array().unwrap().iter().map(|entry| entry["no"].as_i64().unwrap()).collect();
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
    assert!(mods::listing(&mods_dir, &root, &IndexMap::new()).unwrap().contains("patch t01.c: 1 script; overrides helper"));
}
