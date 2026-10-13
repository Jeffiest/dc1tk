//! Carried over from the Python tool's test_pathnames.py

mod common;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use c2_script::build;
use c2_script::compile::{self, Toolchain};
use c2_script::disc::MonsterRow;
use c2_script::pathnames::{self, Lookups};
use c2_script::parse;
use common::{archive_files, iso_image, no_texts, pack, tables, write, TempDir, SECTOR};
use indexmap::IndexMap;

fn found() -> Lookups {
    let map = |pairs: &[(&str, &str)]| -> HashMap<String, String> {
        pairs.iter().map(|(key, value)| (key.to_string(), value.to_string())).collect()
    };
    Lookups {
        maps: map(&[("s51", "Raybrandt Castle"), ("i01h01", "Cedric's Shop"), ("s56", "Ixion")]),
        ais: map(&[("e33a", "Golem"), ("b254a", "Griffon Soldier")]),
        titles: map(&[("map/d/d01", "Underground Channel"), ("mainchr/c01.stb", "Max"), ("map/i/i01", "Shops")]),
    }
}

fn renames(pairs: &[(&str, &str)]) -> IndexMap<String, String> {
    pairs.iter().map(|(key, value)| (key.to_string(), value.to_string())).collect()
}

#[test]
fn names_are_plain_words_after_the_code() {
    let found = found();
    assert_eq!(pathnames::slug("Ocean's Roar Cave"), "oceans_roar_cave");
    assert_eq!(pathnames::slug("Dr.Dell"), "dr_dell");
    assert_eq!(pathnames::named_script("map/s/s51/s51_1.c", &found), "map/s/s51_raybrandt_castle/s51_1.c");
    assert_eq!(
        pathnames::named_script("map/i/i01/h01/i01h01_1.c", &found),
        "map/i/i01_shops/h01_cedrics_shop/i01h01_1.c"
    );
    assert_eq!(pathnames::named_script("map/d/d01/d01_1.c", &found), "map/d/d01_underground_channel/d01_1.c");
    assert_eq!(pathnames::named_script("map/s/s56.c", &found), "map/s/s56_ixion.c");
    assert_eq!(pathnames::named_script("dungeon/monster/e33a.c", &found), "dungeon/monster/e33a_golem.c");
    assert_eq!(pathnames::named_script("mainchr/c01.c", &found), "mainchr/c01_max.c");
    assert_eq!(pathnames::named_script("dungeon/eff_script/ac_spark.c", &found), "dungeon/eff_script/ac_spark.c");
}

#[test]
fn libraries_name_their_folders_like_the_scripts_they_serve() {
    let found = found();
    assert_eq!(
        pathnames::named_library("lib/map/s/s51/monica_gear_battle.c", &found),
        "lib/map/s/s51_raybrandt_castle/monica_gear_battle.c"
    );
    assert_eq!(
        pathnames::named_library("lib/dungeon/monster/death_motion.c", &found),
        "lib/dungeon/monster/death_motion.c"
    );
}

fn row(name: &str, model: &str, script: &str) -> MonsterRow {
    MonsterRow { name: name.into(), model: model.into(), script: script.into() }
}

#[test]
fn monster_ai_scripts_take_the_name_of_the_kind_whose_model_they_are() {
    let rows: IndexMap<i64, MonsterRow> = [
        (168, row("Golem", "e33a", "e33a")),
        (169, row("Rave Golem", "e33b", "e33a")),
        (5, row("Mini Golem", "e40a", "e33a")),
        (523, row("Griffon Soldier", "e254a", "b254a")),
    ]
    .into_iter()
    .collect();
    let expected: HashMap<String, String> =
        [("e33a".to_string(), "Golem".to_string()), ("b254a".to_string(), "Griffon Soldier".to_string())].into();
    assert_eq!(pathnames::ai_names(&rows), expected);
}

#[test]
fn a_tree_records_what_each_named_path_was() {
    let folder = TempDir::new();
    let tree = folder.path();
    pathnames::record(
        tree,
        &renames(&[("map/s/s51/s51_1.c", "map/s/s51_raybrandt_castle/s51_1.c"), ("map/t01.c", "map/t01.c")]),
    )
    .unwrap();
    assert_eq!(pathnames::unnamed(tree, "map/s/s51_raybrandt_castle/s51_1.c").unwrap(), "map/s/s51/s51_1.c");
    assert_eq!(pathnames::unnamed(tree, "map/t01.c").unwrap(), "map/t01.c");
    assert_eq!(pathnames::named(tree, "map/s/s51/s51_1.c").unwrap(), "map/s/s51_raybrandt_castle/s51_1.c");
}

// src/tests/test_rebuild.py's fixtures

const SCRIPT: &str = "#pragma natives event\n\nPROGRAM(100, f);\nMAIN(f);\n\nvoid f(void)\n{\n%s}\n";

fn script(body: &str) -> Vec<u8> {
    compile::compile_program(&parse::parse(&SCRIPT.replace("%s", body)).unwrap(), tables()).unwrap()
}

fn disc(folder: &Path, filler: usize) -> PathBuf {
    let chara = pack(&[("info.cfg", b"MODEL c01;".to_vec()), ("c01.stb", script("    wait;\n"))]);
    let (index, dat) = archive_files(&[
        ("\\map\\t01.stb", script("    wait;\n")),
        ("\\map\\t02.stb", script("    end;\n")),
        ("\\chara\\c01.chr", chara),
    ]);
    let mut files = vec![("DATA.HD4", index), ("DATA.DAT", dat)];
    if filler > 0 {
        files.push(("DMMY", vec![0u8; filler * SECTOR]));
    }
    let path = folder.join("disc.iso");
    std::fs::write(&path, iso_image(&files)).unwrap();
    path
}

fn source(folder: &Path, scripts: &[(&str, &str)]) -> PathBuf {
    let root = folder.join("source");
    for (path, body) in scripts {
        write(&root.join(path), SCRIPT.replace("%s", body));
    }
    root
}

#[test]
fn build_finds_the_disc_script_of_a_named_file() {
    let folder = TempDir::new();
    let iso_path = disc(folder.path(), 4);
    let root = source(folder.path(), &[("map/t01_town.c", "    wait;\n    wait;\n"), ("map/t02.c", "    end;\n")]);
    pathnames::record(&root, &renames(&[("map/t01.c", "map/t01_town.c")])).unwrap();
    let headers = no_texts();
    let sdk_dir = common::sdk_dir();
    let tools = Toolchain { headers: &headers, rules: &[], tables: tables(), sdk_dir: &sdk_dir };
    let found = build::changes(&root, &iso_path, &tools, &no_texts(), false, None).unwrap();
    let changed: Vec<(&str, &str)> =
        found.scripts.iter().map(|change| (change.source.as_str(), change.top.as_str())).collect();
    assert_eq!(changed, [("map/t01_town.c", "map/t01.stb")]);
    assert!(found.skipped.is_empty());
}

#[test]
fn xref_matches_named_folders_by_their_code() {
    let folder = TempDir::new();
    let tree = folder.path();
    for (path, text) in [
        ("map/s/s51_raybrandt_castle/s51_1.c", "PROGRAM(510, end_fight);
"),
        ("dungeon/monster/b254a_griffon_soldier.c", "void on_death(void)
{
    run_event_script(510);
}
"),
        ("map/s/s51_raybrandt_castle/s51_2.c", "void f(void)
{
    place_monster(523);
}
"),
    ] {
        std::fs::create_dir_all(tree.join(path).parent().unwrap()).unwrap();
        std::fs::write(tree.join(path), text).unwrap();
    }
    pathnames::record(
        tree,
        &renames(&[
            ("map/s/s51/s51_1.c", "map/s/s51_raybrandt_castle/s51_1.c"),
            ("map/s/s51/s51_2.c", "map/s/s51_raybrandt_castle/s51_2.c"),
            ("dungeon/monster/b254a.c", "dungeon/monster/b254a_griffon_soldier.c"),
        ]),
    )
    .unwrap();
    let rows: IndexMap<i64, MonsterRow> = [(523, row("Griffon Soldier", "e254a", "b254a"))].into_iter().collect();
    let consts = IndexMap::new();
    let text = c2_script::xref::event_report(tree, &consts, 510, Some("s51"), &rows).unwrap();
    assert!(text.contains("map/s/s51_raybrandt_castle/s51_1.c  PROGRAM(510, end_fight)"), "{text}");
    assert!(text.contains("<- s51 places kind 523 Griffon Soldier"), "{text}");
    let report = c2_script::xref::monster_report(tree, &consts, &rows, "523").unwrap();
    assert!(report.contains("AI script: dungeon/monster/b254a_griffon_soldier.c"), "{report}");
}
