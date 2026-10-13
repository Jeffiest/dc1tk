//! Writing every file of DATA.DAT, and every member of the packs inside them, as a folder tree:
//! a pack X keeps its bytes as X and its members under X.d/.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use serde::Serialize;

use crate::archive::{self, Archive};
use crate::{iso, Result};

pub const MAX_PACK_DEPTH: usize = 4;
pub const PACK_SUFFIX: &str = ".d";

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct FileItem {
    pub path: String,
    pub size: usize,
    pub packed: bool,
}

/// What extract wrote; written as manifest.json, which marks a finished extraction.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Manifest {
    pub files: Vec<FileItem>,
    pub scripts: Vec<String>,
    pub boot: Option<String>,
}

/// Every file and pack member in the order the Python lists them, with its bytes.
fn walk<'a>(path: String, data: &'a [u8], depth: usize, out: &mut Vec<(String, &'a [u8], bool)>) {
    out.push((path.clone(), data, depth > 0));
    if depth >= MAX_PACK_DEPTH {
        return;
    }
    if let Some(members) = archive::parse_pack(data) {
        for member in members {
            walk(format!("{path}{PACK_SUFFIX}/{}", member.name), member.data(data), depth + 1, out);
        }
    }
}

/// Python's str.splitlines over ASCII text.
fn lines(text: &str) -> Vec<&str> {
    let breaks = ['\n', '\r', '\x0b', '\x0c', '\x1c', '\x1d', '\x1e'];
    let mut out = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        match rest.find(|c| breaks.contains(&c)) {
            Some(at) => {
                out.push(&rest[..at]);
                let skip = if rest[at..].starts_with("\r\n") { 2 } else { 1 };
                rest = &rest[at + skip..];
            }
            None => {
                out.push(rest);
                break;
            }
        }
    }
    out
}

/// The boot executable SYSTEM.CNF names (SCES_511.90).
pub fn boot_name(image: &[u8]) -> Result<Option<String>> {
    let Some(record) = iso::find(image, "SYSTEM.CNF")? else {
        return Ok(None);
    };
    let text: String =
        iso::read_file(image, &record).iter().map(|&b| if b < 0x80 { b as char } else { '\u{FFFD}' }).collect();
    for line in lines(&text) {
        if line.replace(' ', "").starts_with("BOOT2=") {
            let last = line.rsplit('\\').next().unwrap_or("");
            return Ok(Some(last.split(';').next().unwrap_or("").trim().to_string()));
        }
    }
    Ok(None)
}

pub fn extract_boot(image: &[u8], out_dir: &Path) -> Result<Option<String>> {
    let Some(name) = boot_name(image)? else {
        return Ok(None);
    };
    let Some(record) = iso::find(image, &name)? else {
        return Ok(None);
    };
    write_file(&out_dir.join(&name), iso::read_file(image, &record))?;
    Ok(Some(name))
}

fn write_file(path: &Path, data: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, data)?;
    Ok(())
}

/// The key two paths share when the file system takes them for one file.
fn file_key(path: &str) -> String {
    if cfg!(any(windows, target_os = "macos")) {
        path.to_lowercase()
    } else {
        path.to_string()
    }
}

/// Writes every file, in parallel. Where two paths name one file (they differ only in case on a
/// case-insensitive file system), the file keeps the first path's spelling and the last one's
/// bytes, as writing them in order would leave it.
fn write_all(out: &Path, items: &[(String, &[u8], bool)]) -> Result<()> {
    let mut slot: HashMap<String, usize> = HashMap::new();
    let mut files: Vec<(&str, &[u8])> = Vec::new();
    let mut folders: Vec<PathBuf> = Vec::new();
    let mut seen_folders: HashMap<String, ()> = HashMap::new();
    for (path, data, _) in items {
        match slot.get(&file_key(path)) {
            Some(&at) => files[at].1 = data,
            None => {
                slot.insert(file_key(path), files.len());
                files.push((path, data));
            }
        }
        if let Some((folder, _)) = path.rsplit_once('/') {
            if seen_folders.insert(file_key(folder), ()).is_none() {
                folders.push(out.join(folder));
            }
        }
    }
    // folders in order, so each keeps the spelling it is first met with
    fs::create_dir_all(out)?;
    for folder in folders {
        fs::create_dir_all(folder)?;
    }
    files.par_iter().try_for_each(|(path, data)| fs::write(out.join(path), data))?;
    Ok(())
}

pub fn extract(image: &[u8], out_dir: &Path) -> Result<Manifest> {
    let archive = Archive::open(image)?;
    let mut items = Vec::new();
    for entry in &archive.entries {
        walk(entry.path.clone(), archive.read(image, entry), 0, &mut items);
    }
    write_all(out_dir, &items)?;
    let files: Vec<FileItem> = items
        .iter()
        .map(|(path, data, packed)| FileItem { path: path.clone(), size: data.len(), packed: *packed })
        .collect();
    let scripts =
        files.iter().filter(|item| item.path.to_lowercase().ends_with(".stb")).map(|item| item.path.clone()).collect();
    let boot = extract_boot(image, out_dir)?;
    let manifest = Manifest { files, scripts, boot };
    // The manifest marks a finished extraction (setup reuses a folder that has one), so it is written
    // whole or not at all.
    let partial = out_dir.join("manifest.json.partial");
    fs::write(&partial, python_json(&manifest)?)?;
    fs::rename(&partial, out_dir.join("manifest.json"))?;
    Ok(manifest)
}

/// JSON as the Python tools write it: json.dumps(indent=1, ensure_ascii=False), with the line
/// endings Path.write_text gives on this system.
pub fn python_json<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b" ");
    let mut serializer = serde_json::Serializer::with_formatter(&mut out, formatter);
    value.serialize(&mut serializer).map_err(|error| crate::Error::invalid(error.to_string()))?;
    if cfg!(windows) {
        let text = String::from_utf8(out).expect("serde_json writes UTF-8");
        return Ok(text.replace('\n', "\r\n").into_bytes());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::samples;

    /// A fresh folder for a test, emptied first.
    fn scratch(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("c2-disc-extract-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        folder
    }

    // tests/test_extract.py

    #[test]
    fn extracts_archive_and_packs() {
        let out = scratch("packs");
        let manifest = extract(&samples::sample_disc(), &out).unwrap();
        assert_eq!(fs::read(out.join("map/t01.stb")).unwrap(), samples::sample_script());
        assert_eq!(fs::read(out.join("chara/c01.chr.d/c01.stb")).unwrap(), samples::sample_script());
        assert_eq!(fs::read(out.join("chara/c01.chr.d/info.cfg")).unwrap(), b"MODEL c01;");
        assert_eq!(fs::read(out.join("readme.txt")).unwrap(), b"hello");
        assert_eq!(manifest.scripts, ["map/t01.stb", "chara/c01.chr.d/c01.stb"]);
        let written: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(out.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(written, serde_json::to_value(&manifest).unwrap());
        assert!(!out.join("manifest.json.partial").exists());
        fs::remove_dir_all(&out).unwrap();
    }

    #[test]
    fn manifest_records_sizes_and_packing() {
        let out = scratch("sizes");
        let manifest = extract(&samples::sample_disc(), &out).unwrap();
        let sizes: HashMap<&str, (usize, bool)> =
            manifest.files.iter().map(|item| (item.path.as_str(), (item.size, item.packed))).collect();
        assert_eq!(sizes["readme.txt"], (5, false));
        assert_eq!(sizes["chara/c01.chr.d/info.cfg"], (10, true));
        fs::remove_dir_all(&out).unwrap();
    }

    #[test]
    fn a_disc_without_system_cnf_has_no_boot_file() {
        let out = scratch("boot");
        assert_eq!(extract_boot(&samples::sample_disc(), &out).unwrap(), None);
        let cnf = b"BOOT2 = cdrom0:\\SCES_511.90;1\r\nVER = 1.00\r\n";
        let image = samples::iso_image(&[("SYSTEM.CNF", cnf), ("SCES_511.90", b"elf")]);
        assert_eq!(extract_boot(&image, &out).unwrap().as_deref(), Some("SCES_511.90"));
        assert_eq!(fs::read(out.join("SCES_511.90")).unwrap(), b"elf");
        fs::remove_dir_all(&out).unwrap();
    }

    #[test]
    fn splits_lines_as_python() {
        assert_eq!(
            lines("BOOT2 = cdrom0:\\SCES_511.90;1\r\nVER = 1.00\r\n"),
            vec!["BOOT2 = cdrom0:\\SCES_511.90;1", "VER = 1.00"]
        );
        assert_eq!(lines("a\n\nb"), vec!["a", "", "b"]);
    }

    #[test]
    fn json_matches_python_indent() {
        let manifest = Manifest {
            files: vec![FileItem { path: "a/モ.cfg".into(), size: 3, packed: false }],
            scripts: vec![],
            boot: None,
        };
        let text = String::from_utf8(python_json(&manifest).unwrap()).unwrap().replace("\r\n", "\n");
        assert_eq!(
            text,
            "{\n \"files\": [\n  {\n   \"path\": \"a/モ.cfg\",\n   \"size\": 3,\n   \"packed\": false\n  }\n ],\n \"scripts\": [],\n \"boot\": null\n}"
        );
    }
}
