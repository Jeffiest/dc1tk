//! Renaming a constant of a definition set: its header, what the set says about it, and its uses in
//! a decompiled tree, which keeps telling unedited files from edited ones.

mod build_common;

use build_common::{fails, make_tree, map, read, write, TempDir};
use c2_script::{constants, tree};

const HEADER: &str = "enum flag_story {\n    // Set when the bridge is fixed.\n    FLAG_E520_SEEN = 520,\n};\n";

const NAMES: &str = "{\n \"functions\": {\n  \"0a\": {\n   \"name\": \"cross_bridge\",\n   \"locals\": [\n    \"e520_seen\"\n   ],\n   \"doc\": \"Reads FLAG_E520_SEEN.\"\n  },\n  \"0b\": {\n   \"name\": \"e520_seen\"\n  }\n }\n}\n";

const SCRIPT: &str = "void cross_bridge(void)\n{\n    int e520_seen;\n\n    e520_seen = get_flag(FLAG_E520_SEEN);\n    e520_seen();\n}\n";

fn set(folder: &TempDir) -> std::path::PathBuf {
    let root = folder.path().join("set");
    write(&root, "include/pal.h", HEADER);
    write(&root, "names/event.json", NAMES);
    write(&root, "notes/flags.md", "FLAG_E520_SEEN is set by the bridge scene.\n");
    root
}

#[test]
fn a_rename_reaches_the_header_the_set_and_a_tree() {
    let folder = TempDir::new("constants-rename");
    let root = set(&folder);
    let edited = make_tree(folder.path(), &[("map/a.c", SCRIPT), ("map/b.c", SCRIPT)]);
    tree::record(&edited, &["map/a.c".to_string(), "map/b.c".to_string()], None, &[]).unwrap();
    write(&edited, "map/b.c", &format!("// my edit\n{SCRIPT}"));
    let renamed =
        constants::rename(&root, "FLAG_E520_SEEN", "FLAG_BRIDGE_FIXED", &map(&[]), std::slice::from_ref(&edited))
            .unwrap();
    assert_eq!(renamed.set_files.len(), 3);
    assert!(read(&root.join("include/pal.h")).contains("    FLAG_BRIDGE_FIXED = 520,"));
    let names = read(&root.join("names/event.json"));
    // the local the decompiler names after the flag follows it; a function of that name does not
    assert!(names.contains("\"bridge_fixed\"") && names.contains("\"name\": \"e520_seen\""), "{names}");
    assert!(names.contains("Reads FLAG_BRIDGE_FIXED."));
    assert_eq!(read(&root.join("notes/flags.md")), "FLAG_BRIDGE_FIXED is set by the bridge scene.\n");
    let script = read(&edited.join("map/a.c"));
    assert!(script.contains("    bridge_fixed = get_flag(FLAG_BRIDGE_FIXED);\n    e520_seen();"), "{script}");
    // the unedited file is still unedited, the edited one still edited
    let status = tree::status(&edited).unwrap().unwrap();
    assert_eq!(status.edited, vec!["map/b.c".to_string()]);
}

#[test]
fn a_rename_needs_a_constant_of_the_set_and_a_free_name() {
    let folder = TempDir::new("constants-refused");
    let root = set(&folder);
    fails(constants::rename(&root, "FLAG_NOT_THERE", "FLAG_X", &map(&[]), &[]), "is not in the headers");
    let sdk = map(&[("event.h", "#define FLAG_TAKEN 3\n")]);
    fails(constants::rename(&root, "FLAG_E520_SEEN", "FLAG_TAKEN", &sdk, &[]), "already used in event.h");
    fails(constants::rename(&root, "FLAG_E520_SEEN", "2BAD", &map(&[]), &[]), "not a valid name");
    assert_eq!(read(&root.join("include/pal.h")), HEADER);
}
