//! Carried over from the Python tool's test_rebuild.py: build's changed scripts written into a copy of a tiny disc.

mod build_common;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use build_common::{archive_files, iso_image, pack, strings, write, TempDir, Tools, SECTOR};
use dc2tk_disc::archive::Archive;
use dc2tk_disc::rebuild::{self, Written};
use dc2tk_script::build::{self, Changes};
use dc2tk_script::{compile, parse};
use indexmap::IndexMap;

fn source_text(body: &str) -> String {
    format!("#pragma natives event\n\nPROGRAM(100, f);\nMAIN(f);\n\nvoid f(void)\n{{\n{body}}}\n")
}

fn script(tools: &Tools, body: &str) -> Vec<u8> {
    compile::compile_program(&parse::parse(&source_text(body)).unwrap(), &tools.tables).unwrap()
}

fn disc(tools: &Tools, folder: &Path, filler: usize) -> PathBuf {
    let chara = pack(&[("info.cfg", b"MODEL c01;".to_vec()), ("c01.stb", script(tools, "    wait;\n"))]);
    let (index, dat) = archive_files(&[
        ("\\map\\t01.stb", script(tools, "    wait;\n")),
        ("\\map\\t02.stb", script(tools, "    end;\n")),
        ("\\chara\\c01.chr", chara),
    ]);
    let mut files = vec![("DATA.HD4", index), ("DATA.DAT", dat)];
    if filler > 0 {
        files.push(("DMMY", vec![0; filler * SECTOR]));
    }
    let path = folder.join("disc.iso");
    fs::write(&path, iso_image(&files)).unwrap();
    path
}

fn source(folder: &Path, scripts: &[(&str, &str)]) -> PathBuf {
    let root = folder.join("source");
    for (path, body) in scripts {
        write(&root, path, &source_text(body));
    }
    root
}

fn built(
    tools: &Tools,
    folder: &Path,
    filler: usize,
    scripts: &[(&str, &str)],
) -> (PathBuf, PathBuf, Changes, Written) {
    let iso_path = disc(tools, folder, filler);
    let out = folder.join("out.iso");
    let found =
        build::changes(&source(folder, scripts), &iso_path, &tools.toolchain(), &IndexMap::new(), false, None).unwrap();
    let result = rebuild::write_disc(&iso_path, &out, &found.files).unwrap();
    (iso_path, out, found, result)
}

fn read(path: &Path, top: &str, members: &[&str]) -> Vec<u8> {
    let image = fs::read(path).unwrap();
    let disc = Archive::open(&image).unwrap();
    let entry = disc.entries.iter().find(|entry| entry.path == top).unwrap();
    rebuild::read_member(disc.read(&image, entry), &strings(members)).unwrap().to_vec()
}

fn sources(found: &Changes) -> Vec<String> {
    let mut paths: Vec<String> = found.scripts.iter().map(|change| change.source.clone()).collect();
    paths.sort();
    paths
}

#[test]
fn unchanged_source_rebuilds_the_same_image() {
    let tools = Tools::new();
    let folder = TempDir::new("rebuild-unchanged");
    let (iso_path, out, found, _) =
        built(&tools, folder.path(), 4, &[("map/t01.c", "    wait;\n"), ("map/t02.c", "    end;\n")]);
    assert!(found.scripts.is_empty());
    assert!(fs::read(&out).unwrap() == fs::read(&iso_path).unwrap());
}

#[test]
fn changed_scripts_land_in_place_inside_packs_and_in_the_filler() {
    let tools = Tools::new();
    let folder = TempDir::new("rebuild-changed");
    let long_body = "    wait;\n".repeat(400);
    let (_, out, found, result) = built(
        &tools,
        folder.path(),
        4,
        &[("map/t01.c", &long_body), ("map/t02.c", "    end;\n"), ("chara/c01.chr.d/c01.c", "    wait;\n    wait;\n")],
    );
    assert_eq!(sources(&found), strings(&["chara/c01.chr.d/c01.c", "map/t01.c"]));
    let moved: BTreeMap<&str, bool> = result.placed.iter().map(|item| (item.path.as_str(), item.moved)).collect();
    assert_eq!(moved, BTreeMap::from([("chara/c01.chr", false), ("map/t01.stb", true)]));
    assert_eq!(result.grown, 0);
    assert_eq!(read(&out, "map/t01.stb", &[]), script(&tools, &long_body));
    assert_eq!(read(&out, "map/t02.stb", &[]), script(&tools, "    end;\n"));
    assert_eq!(read(&out, "chara/c01.chr", &["c01.stb"]), script(&tools, "    wait;\n    wait;\n"));
    assert_eq!(read(&out, "chara/c01.chr", &["info.cfg"]), b"MODEL c01;");
    assert!(build::check(&out, &found.scripts).unwrap().is_empty());
}

#[test]
fn image_grows_when_there_is_no_filler() {
    let tools = Tools::new();
    let folder = TempDir::new("rebuild-grows");
    let long_body = "    wait;\n".repeat(400);
    let (iso_path, out, _, result) = built(&tools, folder.path(), 0, &[("map/t01.c", &long_body)]);
    assert!(result.grown > 0);
    assert_eq!(
        fs::metadata(&out).unwrap().len(),
        fs::metadata(&iso_path).unwrap().len() + result.grown * SECTOR as u64
    );
    assert_eq!(read(&out, "map/t01.stb", &[]), script(&tools, &long_body));
}
