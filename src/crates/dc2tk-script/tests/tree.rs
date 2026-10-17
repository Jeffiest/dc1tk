//! A decompiled tree's record: telling edits from the decompiler's output.

mod build_common;

use std::fs;
use std::path::{Path, PathBuf};

use build_common::{make_tree, read, strings, write, TempDir};
use dc2tk_script::tree;

const FILES: [(&str, &str); 3] = [
    ("map/a.c", "#include \"lib/shared.c\"  // shared helpers\nvoid a(void)\n{\n}\n"),
    ("map/b.c", "#include \"lib/shared.c\"\nvoid b(void)\n{\n}\n"),
    ("lib/shared.c", "void shared(void)\n{\n}\n"),
];

fn paths() -> Vec<String> {
    FILES.iter().map(|(path, _)| path.to_string()).collect()
}

fn recorded(folder: &Path) -> PathBuf {
    let root = make_tree(folder, &FILES);
    tree::record(&root, &paths(), None, &[]).unwrap();
    root
}

fn status(root: &Path) -> (Vec<String>, Vec<String>, Vec<String>) {
    let found = tree::status(root).unwrap().unwrap();
    (found.edited, found.added, found.removed)
}

#[test]
fn status_tells_edits_from_decompiler_output_ignoring_line_endings() {
    let folder = TempDir::new("tree-status");
    let root = recorded(folder.path());
    fs::write(root.join("map/a.c"), FILES[0].1.replace('\n', "\r\n")).unwrap();
    assert_eq!(status(&root), (vec![], vec![], vec![]));
    write(&root, "lib/shared.c", "void shared(void)\n{\n    WAIT();\n}\n");
    write(&root, "lib/new.c", "");
    write(&root, ".cache/clangd/index.idx", "");
    fs::remove_file(root.join("map/b.c")).unwrap();
    assert_eq!(status(&root), (strings(&["lib/shared.c"]), strings(&["lib/new.c"]), strings(&["map/b.c"])));
}

#[test]
fn overwrites_only_names_edited_files_or_all_of_an_unrecorded_tree() {
    let folder = TempDir::new("tree-overwrites");
    let root = recorded(folder.path());
    assert!(tree::overwrites(&root, &paths()).unwrap().is_empty());
    write(&root, "map/a.c", "edited");
    assert_eq!(tree::overwrites(&root, &paths()).unwrap(), strings(&["map/a.c"]));
    fs::remove_file(root.join(tree::ORIGINALS)).unwrap();
    let mut found = tree::overwrites(&root, &paths()).unwrap();
    found.sort();
    let mut all = paths();
    all.sort();
    assert_eq!(found, all);
}

#[test]
fn record_keeps_files_another_decompile_wrote() {
    let folder = TempDir::new("tree-record");
    let root = recorded(folder.path());
    tree::record(&root, &strings(&["map/a.c"]), None, &[]).unwrap();
    let mut found: Vec<String> = tree::originals(&root).unwrap().unwrap().into_keys().collect();
    found.sort();
    let mut all = paths();
    all.sort();
    assert_eq!(found, all);
}

#[test]
fn library_notes_count_scripts_an_edit_reaches_unasked() {
    let folder = TempDir::new("tree-notes");
    let root = recorded(folder.path());
    write(&root, "lib/shared.c", "void shared(void)\n{\n    WAIT();\n}\n");
    write(&root, "map/a.c", "#include \"lib/shared.c\"\n// edited\n");
    let found = tree::status(&root).unwrap().unwrap();
    assert_eq!(
        tree::library_notes(&root, &found).unwrap(),
        strings(&["edited library lib/shared.c is included by 2 scripts, 1 of which you did not edit"])
    );
}

#[test]
fn fork_gives_chosen_scripts_their_own_copy() {
    let folder = TempDir::new("tree-fork");
    let root = recorded(folder.path());
    let result = tree::fork(&root, "lib/shared.c", "lib/shared_a.c", &strings(&["map/a.c"])).unwrap();
    assert_eq!(result, strings(&["map/b.c"]));
    assert!(read(&root.join("map/a.c")).contains("#include \"lib/shared_a.c\""));
    assert!(read(&root.join("map/b.c")).contains("#include \"lib/shared.c\""));
    let copy = read(&root.join("lib/shared_a.c"));
    assert!(copy.starts_with("// Copy of lib/shared.c for map/a.c") && copy.ends_with(FILES[2].1));
}

#[test]
fn files_a_decompile_no_longer_writes_are_stale_unless_edited() {
    let folder = TempDir::new("tree-stale");
    let root = folder.path();
    for path in ["lib/a/old.c", "lib/a/kept.c", "map/s.c"] {
        write(root, path, &format!("// {path}\n"));
    }
    tree::record(root, &strings(&["lib/a/old.c", "lib/a/kept.c", "map/s.c"]), None, &[]).unwrap();
    write(root, "lib/a/kept.c", "// edited\n");
    let (unedited, edited) = tree::stale(root, &strings(&["map/s.c"])).unwrap();
    assert_eq!((unedited.clone(), edited), (strings(&["lib/a/old.c"]), strings(&["lib/a/kept.c"])));
    tree::remove_files(root, &unedited).unwrap();
    tree::record(root, &strings(&["map/s.c"]), None, &unedited).unwrap();
    assert!(!root.join("lib/a/old.c").exists() && root.join("lib/a/kept.c").exists());
    assert!(!tree::originals(root).unwrap().unwrap().contains_key("lib/a/old.c"));
}
