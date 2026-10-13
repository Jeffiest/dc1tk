//! Carried over from the Python tool's test_library.py

mod common;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use c2_script::definitions::Definitions;
use c2_script::library::{self, Model};
use c2_script::node;
use c2_script::node::Node;
use c2_script::stb::{self, Region};
use c2_script::{compile, decompile, names, parse, sdk, source};
use common::{no_texts, sdk_dir, tables, texts, write, TempDir};
use indexmap::IndexMap;

const HELPERS: &str = "
void wait_frames(int count)
{
    while (count > 0) {
        wait;
        count = count - 1;
    }
}

int twice(int value)
{
    wait_frames(value);
    return value * 2;
}
";

fn script_text(helpers: &str, own: i64) -> String {
    format!(
        "#pragma natives event\n\nPROGRAM(100, talk);\nMAIN(talk);\n{helpers}\nvoid talk(void)\n{{\n    int done;\n\n    done = twice({own});\n    _GET_FLAG(12, &done);\n}}\n"
    )
}

fn compiled(text: &str) -> Vec<u8> {
    compile::compile_program(&parse::parse(text).unwrap(), tables()).unwrap()
}

fn script_bytes(own: i64) -> Vec<u8> {
    compiled(&script_text(HELPERS, own))
}

fn corpus(folder: &Path) -> (PathBuf, Vec<String>) {
    let root = folder.to_path_buf();
    for number in 0..3 {
        write(&root.join(format!("map/m{number}.stb")), script_bytes(number));
    }
    write(&root.join("map/solo.stb"), compiled(&script_text("", 0).replace("twice(0)", "1")));
    let paths = ["map/m0.stb", "map/m1.stb", "map/m2.stb", "map/solo.stb"].iter().map(|path| path.to_string()).collect();
    (root, paths)
}

fn model(root: &Path, paths: &[String], defs: &Definitions) -> Model {
    library::model_for(root, paths, tables(), defs, &no_texts()).unwrap()
}

/// The source text of a script of the corpus, as the model shows it.
fn script_source(root: &Path, path: &str, model: &Model) -> (Vec<u8>, String) {
    let data = std::fs::read(root.join(path)).unwrap();
    let program = decompile::decompile(&stb::read(&data), "event", tables());
    (data, source::text(&model.script_program(&program, path).unwrap()).unwrap())
}

fn compiled_with(text: &str, found: &IndexMap<String, String>) -> Vec<u8> {
    compile::compile_program(&library::expand(parse::parse(text).unwrap(), found, &no_texts()).unwrap(), tables()).unwrap()
}

#[test]
fn shared_functions_become_one_library() {
    let folder = TempDir::new();
    let (root, paths) = corpus(folder.path());
    let model = model(&root, &paths, &Definitions::empty());
    let found = model.library_texts().unwrap();
    assert_eq!(found.keys().collect::<Vec<_>>(), ["lib/map/map_1.c"]);
    assert!(found["lib/map/map_1.c"].contains("void map_1_1(int a0)"));
    assert!(found["lib/map/map_1.c"].contains("map_1_1(a0);"));
    for path in &paths {
        let (data, text) = script_source(&root, path, &model);
        if path != "map/solo.stb" {
            assert!(text.contains("#include \"lib/map/map_1.c\""));
            assert!(text.contains("void program_100(void)"));
            assert!(text.contains("v0 = map_1_2("));
        }
        assert_eq!(compiled_with(&text, &found), data, "{path}");
    }
}

#[test]
fn name_table_renames_everywhere() {
    let folder = TempDir::new();
    let (root, paths) = corpus(folder.path());
    let first = model(&root, &paths, &Definitions::empty());
    let fingerprint = first.class_names.iter().find(|(_, name)| *name == "map_1_1").unwrap().0.clone();
    let table = folder.path().join("names");
    std::fs::create_dir(&table).unwrap();
    let entry = serde_json::json!({"name": "wait_frames", "params": ["frames"]});
    let mut functions = serde_json::Map::new();
    functions.insert(fingerprint, entry);
    std::fs::write(table.join("event.json"), serde_json::json!({"functions": functions}).to_string()).unwrap();
    let defs = Definitions { names: names::load(&table).unwrap(), ..Definitions::empty() };
    let model = model(&root, &paths, &defs);
    let found = model.library_texts().unwrap();
    let text = &found["lib/map/map_1.c"];
    assert!(text.contains("void wait_frames(int frames)"));
    assert!(text.contains("while (frames > 0) {"));
    assert!(text.contains("wait_frames(a0);"));
    let (data, text) = script_source(&root, "map/m1.stb", &model);
    assert_eq!(compiled_with(&text, &found), data);
}

#[test]
fn clashing_variable_names_are_numbered() {
    let function = parse::parse("MAIN(f);\nvoid f(int a0, int a1)\n{\n    g0 = a0;\n}\n").unwrap()["functions"][0].clone();
    let params = |entry: Node| -> Vec<String> {
        names::with_variables(function.clone(), &entry, &HashSet::new())["params"]
            .as_array()
            .unwrap()
            .iter()
            .map(|param| param["name"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(params(node!({"params": ["x", "x"]})), ["x", "x_2"]);
    assert_eq!(params(node!({"params": ["g0"]}))[0], "g0_2");
    assert_eq!(params(node!({"params": ["x"]}))[0], "x");
}

#[test]
fn padding_of_overrides_an_included_function() {
    let program = parse::parse(
        "#pragma natives event\nPROGRAM(100, talk);\nMAIN(talk);\n#include \"lib.c\"\nvoid talk(void)\n{\n}\n#pragma padding_of helper 01020304\n",
    )
    .unwrap();
    let found = texts(&[("lib.c", "#pragma natives event\nvoid helper(void)\n{\n    print(\"abc\");\n}\n")]);
    let expanded = library::expand(program, &found, &no_texts()).unwrap();
    let functions = expanded["functions"].as_array().unwrap();
    assert_eq!(functions.iter().map(|function| function["name"].as_str().unwrap()).collect::<Vec<_>>(), ["helper", "talk"]);
    assert_eq!(functions[0]["padding"], "01020304");
    let script = stb::read(&compile::compile_program(&expanded, tables()).unwrap());
    let Region::Function(helper) = &script.regions[2] else { panic!("no function") };
    assert_eq!(helper.padding, [1, 2, 3, 4]);
}

#[test]
fn includes_are_found_in_parent_folders() {
    let folder = TempDir::new();
    let root = folder.path();
    write(&root.join("lib/a.c"), "#pragma natives event\n");
    std::fs::create_dir_all(root.join("map/s")).unwrap();
    let program = parse::parse("#include \"lib/a.c\"\nMAIN(f);\n").unwrap();
    let found = library::read_includes(&program, &root.join("map/s/x.c"), None, &sdk_dir(), &[]).unwrap();
    assert_eq!(found, texts(&[("lib/a.c", "#pragma natives event\n")]));
}

#[test]
fn native_calls_are_resolved_after_includes() {
    let program = parse::parse(
        "#pragma natives event\nMAIN(f);\nvoid helper(void)\n{\n}\nvoid f(void)\n{\n    helper();\n    _SET_FLAG(1, 1);\n}\n",
    )
    .unwrap();
    let resolved = sdk::resolve(&program, tables()).unwrap();
    let body = &resolved["functions"][1]["body"];
    assert!(body[0].kind() == "expr" && body[1].kind() == "native");
    for broken in [
        "MAIN(f);\nvoid _f(void)\n{\n}\n",
        "#pragma natives event\nMAIN(f);\nvoid f(void)\n{\n    int x;\n\n    x = _GET_FLAG(1) + 1;\n}\n",
    ] {
        // refused by the parser (a function named like a native) or by resolving
        let refused = parse::parse(broken).and_then(|program| sdk::resolve(&program, tables()));
        assert!(refused.is_err(), "accepted {broken}");
    }
}

#[test]
fn libraries_json_moves_and_documents_a_library() {
    let folder = TempDir::new();
    let (root, paths) = corpus(folder.path());
    let first = model(&root, &paths, &Definitions::empty());
    let identity = first.libraries.values().next().unwrap().fingerprint.clone();
    let libraries: IndexMap<String, Node> =
        [(identity, node!({"path": "lib/town/waiting.c", "doc": "Waiting helpers."}))].into_iter().collect();
    let defs = Definitions { libraries, ..Definitions::empty() };
    let model = model(&root, &paths, &defs);
    let found = model.library_texts().unwrap();
    assert_eq!(found.keys().collect::<Vec<_>>(), ["lib/town/waiting.c"]);
    assert!(found["lib/town/waiting.c"].starts_with("// Waiting helpers.\n\n"));
    let (data, text) = script_source(&root, "map/m1.stb", &model);
    assert!(text.contains("#include \"lib/town/waiting.c\""));
    assert_eq!(compiled_with(&text, &found), data);
}

const GLOBAL_SCRIPT: &str = "#pragma natives event

int counter;

PROGRAM(100, talk);
MAIN(talk);

void bump(void)
{
    counter = counter + 1;
}

void talk(void)
{
    bump();
    _GET_FLAG(%d, &counter);
}
";

#[test]
fn libraries_json_names_the_globals_a_library_uses() {
    let folder = TempDir::new();
    let root = folder.path();
    let mut paths = Vec::new();
    for number in 0..3 {
        let path = format!("map/g{number}.stb");
        write(&root.join(&path), compiled(&GLOBAL_SCRIPT.replace("%d", &(number + 10).to_string())));
        paths.push(path);
    }
    let first = model(root, &paths, &Definitions::empty());
    let identity = first.libraries.values().next().unwrap().fingerprint.clone();
    let libraries: IndexMap<String, Node> =
        [(identity, node!({"globals": {"g0": "visits"}}))].into_iter().collect();
    let defs = Definitions { libraries, ..Definitions::empty() };
    let model = model(root, &paths, &defs);
    let found = model.library_texts().unwrap();
    assert!(found.values().next().unwrap().contains("visits = visits + 1;"));
    for path in &paths {
        let (data, text) = script_source(root, path, &model);
        assert!(text.contains("int visits;"));
        assert_eq!(compiled_with(&text, &found), data);
    }
}
