//! Carried over from the Python tool's test_flags.py: the flags scripts use, the free ones and who uses one.

mod build_common;

use build_common::{consts, make_tree, TempDir};
use c2_script::flags;
use indexmap::IndexMap;

const FILES: [(&str, &str); 2] = [
    (
        "map/m.c",
        "void start(void)\n{\n    set_flag(FLAG_DOOR_OPEN, 1);\n    open = get_flag(FLAG_DOOR_OPEN);\n    set_flag(base + i, 1);\n}\n",
    ),
    ("dungeon/monster/e1.c", "void tick(void)\n{\n    on = get_dungeon_flag(2040);\n}\n"),
];

fn names() -> IndexMap<String, i64> {
    consts(&[("FLAG_DOOR_OPEN", 900)])
}

fn scanned(name: &str) -> flags::Found {
    let folder = TempDir::new(name);
    flags::scan(&make_tree(folder.path(), &FILES), &names()).unwrap()
}

#[test]
fn scan_counts_literal_and_named_flags_and_lists_computed_ones() {
    let found = scanned("flags-scan");
    assert_eq!(found.uses.keys().copied().collect::<Vec<_>>(), vec![900, 2040]);
    assert_eq!(found.uses[&900].iter().map(|used| used.access).collect::<Vec<_>>(), vec!["set", "get"]);
    assert_eq!(found.computed.iter().map(|used| used.expr.as_str()).collect::<Vec<_>>(), vec!["base + i"]);
}

#[test]
fn free_ranges_leave_out_scripts_engine_and_tools() {
    let found = scanned("flags-free");
    let used = flags::used_numbers(&found);
    assert!([900, 2040, 799, 2047].iter().all(|number| used.contains(number)));
    let ranges = flags::free_ranges(&used);
    assert!(ranges.contains(&(2041, 2046)));
    assert!(ranges.iter().all(|(a, b)| !(*a <= 900 && 900 <= *b)));
    assert!(flags::summary(&found).contains("1 calls pass a flag number worked out at run time"));
}

#[test]
fn who_names_the_flag_and_every_use() {
    let found = scanned("flags-who");
    let text = flags::who(&found, 900, &names());
    assert!(text.starts_with("flag 900 (FLAG_DOOR_OPEN)"), "{text}");
    assert!(text.contains("set  map/m.c:3  set_flag(FLAG_DOOR_OPEN, 1)  (in start)"), "{text}");
    assert!(flags::who(&found, 1500, &names()).contains("free"));
    assert!(flags::who(&found, 799, &names()).contains("engine: charasetup.cpp"));
}
