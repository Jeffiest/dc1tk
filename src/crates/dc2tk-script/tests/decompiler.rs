//! A whole extracted disc decompiled into a source tree (decompiler::decompile_tree).

mod common;

use std::path::Path;

use common::{sdk_dir, tables, TempDir};
use dc2tk_disc::{extract, samples};
use dc2tk_script::decompiler;
use dc2tk_script::definitions::Definitions;

fn decompiled(extracted: &Path, out: &Path, force: bool) -> dc2tk_script::Result<decompiler::TreeWritten> {
    decompiler::decompile_tree(extracted, out, force, Definitions::empty(), tables(), &sdk_dir())
}

#[test]
fn a_disc_decompiles_into_a_tree_that_keeps_its_edits() {
    let folder = TempDir::new();
    let extracted = folder.path().join("extracted");
    extract::extract(&samples::sample_disc(), &extracted).unwrap();
    let out = folder.path().join("source");
    let written = decompiled(&extracted, &out, false).unwrap();
    // definitions that record no disc have nothing to warn about
    assert!(written.warning.is_none());
    assert_eq!(written.scripts, 2);
    assert!(written.removed.is_empty() && written.kept.is_empty());
    assert!(out.join("map/t01.c").is_file());
    dc2tk_disc::textfile::write_text(&out.join("map/t01.c"), "// mine\n").unwrap();
    let error = decompiled(&extracted, &out, false).err().unwrap().to_string();
    assert!(error.contains("1 edited file") && error.contains("map/t01.c"), "{error}");
    decompiled(&extracted, &out, true).unwrap();
    assert_ne!(dc2tk_disc::textfile::read_text(&out.join("map/t01.c")).unwrap(), "// mine\n");
}

#[test]
fn the_disc_a_set_was_made_for_is_said_to_match() {
    let mut defs = Definitions::empty();
    assert_eq!(
        decompiler::disc_match_text(&defs, &Default::default()),
        "the definition sets record no disc to compare with"
    );
    defs.disc.insert("map/t01.stb".into(), "hash".into());
    assert_eq!(
        decompiler::disc_match_text(&defs, &Default::default()),
        "the disc is the one the definition sets were made for (1 scripts match)"
    );
}
