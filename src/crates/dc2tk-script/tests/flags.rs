//! Flags: the ones scripts use, the free ones, and who uses one.

mod build_common;

use build_common::{consts, make_tree, TempDir};
use dc2tk_script::flags;
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
    let ranges = flags::free_ranges(&used, flags::FLAG_COUNT);
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

const COUNTER_FILES: [(&str, &str); 1] = [(
    "map/m.c",
    "void start(void)\n{\n    set_counter(0, 1);\n    n = get_counter(12);\n    set_counter(base + 1, n);\n    set_flag(40, 1);\n}\n",
)];

#[test]
fn counters_are_scanned_apart_from_flags_with_the_engines_own() {
    let folder = TempDir::new("flags-counters");
    let found = flags::scan_counters(&make_tree(folder.path(), &COUNTER_FILES), &names()).unwrap();
    assert_eq!(found.uses.keys().copied().collect::<Vec<_>>(), vec![0, 12]);
    assert_eq!(found.computed.len(), 1);
    let used = flags::used_numbers(&found);
    // the menu reads 5, the fishing tournament keeps 21 to 25
    assert_eq!(used, [0, 5, 12, 21, 22, 23, 24, 25].into_iter().collect());
    assert_eq!(flags::free_ranges(&used, flags::COUNTER_COUNT).last(), Some(&(26, 127)));
    let summary = flags::summary(&found);
    assert!(summary.starts_with("8 of 128 counters are used: 2 by scripts, 6 by the engine"), "{summary}");
    assert!(summary.contains("1 calls pass a counter number worked out at run time"), "{summary}");
    assert!(flags::who(&found, 22, &names()).contains("engine: menuaqua.cpp"));
    let twelve = flags::who(&found, 12, &names());
    assert!(twelve.starts_with("counter 12\n  get  map/m.c:4"), "{twelve}");
    assert!(flags::who(&found, 200, &names()).contains("out of range: the save holds 128 counters"));
}
