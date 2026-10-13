//! Carried over from the Python tool's test_index.py

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use c2_script::index::{self, Folded};
use common::{texts, write, TempDir};
use indexmap::IndexMap;

/// Writes the files under folder/tree.
pub fn tree(folder: &Path, files: &[(&str, &str)]) -> PathBuf {
    let root = folder.join("tree");
    for (path, text) in files {
        write(&root.join(path), text);
    }
    root
}

#[test]
fn constants_come_from_enums_and_defines() {
    let found = texts(&[
        ("a.h", "enum program {\n    map_start = 100,\n    story_50 = 10500,  // note\n};\n#define LIMIT 0x10\n"),
        ("b.h", "static inline void f(int x)\n{\n    x = 5;\n}\n"),
    ]);
    let expected: IndexMap<String, i64> =
        [("map_start", 100), ("story_50", 10500), ("LIMIT", 16)].iter().map(|(name, value)| (name.to_string(), *value)).collect();
    assert_eq!(index::constants(found.values()), expected);
}

#[test]
fn values_fold_like_the_compiler() {
    let consts: IndexMap<String, i64> = [("story_50".to_string(), 10500), ("BASE".to_string(), 8)].into_iter().collect();
    assert_eq!(index::value("50 * 10 + 0 + 10000", &consts), Some(Folded::Int(10500)));
    assert_eq!(index::value("story_50", &consts), Some(Folded::Int(10500)));
    assert_eq!(index::value("-(BASE + 1)", &consts), Some(Folded::Int(-9)));
    assert_eq!(index::value("7 / 2", &consts), Some(Folded::Int(3)));
    assert_eq!(index::value("\"s51/s51.stb\"", &consts), Some(Folded::Text("s51/s51.stb".into())));
    assert_eq!(index::value("base + i", &consts), None);
    assert_eq!(index::value("get(1)", &consts), None);
}

#[test]
fn calls_carry_arguments_lines_and_functions_and_skip_comments() {
    let folder = TempDir::new();
    let root = tree(
        folder.path(),
        &[(
            "map/m.c",
            "// calls set_flag(1, 1) in a comment\nvoid start(void)\n{\n    set_flag(FLAG_A + 1, get(2, 3));\n    print(\"set_flag(9)\");\n}\n",
        )],
    );
    let found = index::calls(&root, &BTreeSet::from(["set_flag".to_string()])).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].path, "map/m.c");
    assert_eq!(found[0].line, 4);
    assert_eq!(found[0].function.as_deref(), Some("start"));
    assert_eq!(found[0].args, ["FLAG_A + 1", "get(2, 3)"]);
}

#[test]
fn includers_follow_libraries_through_libraries() {
    let folder = TempDir::new();
    let root = tree(
        folder.path(),
        &[
            ("map/a_1.c", "PROGRAM(500, go);\n#include \"lib/outer.c\"\n"),
            ("map/b.c", "#include \"lib/inner.c\"\n"),
            ("lib/outer.c", "#include \"lib/inner.c\"\n"),
            ("lib/inner.c", "void f(void)\n{\n}\n"),
        ],
    );
    let outline = index::outlines(&root).unwrap();
    assert_eq!(outline["map/a_1.c"].programs, [("500".to_string(), "go".to_string())]);
    let expected: BTreeMap<String, Vec<String>> = [
        ("lib/outer.c".to_string(), vec!["map/a_1.c".to_string()]),
        ("lib/inner.c".to_string(), vec!["map/a_1.c".to_string(), "map/b.c".to_string()]),
    ]
    .into_iter()
    .collect();
    assert_eq!(index::includers(&outline), expected);
}

#[test]
fn language_variants_share_a_key() {
    assert_eq!(index::variant_key("map/s/s51/s51_3.c"), index::variant_key("map/s/s51/s51_1.c"));
    assert_eq!(index::variant_key("map/s/s51/s51_1.c"), "map/s/s51/s51_N.c");
    assert_eq!(index::variant_key("map/s/s51/s51.c"), "map/s/s51/s51.c");
}
