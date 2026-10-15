//! The disc's layout, so an extracted disc can be packed back into the image it came from: every
//! file of the ISO 9660 file system is written beside DATA.DAT's files (DATA.DAT itself is not,
//! since its files are already there and DATA.HD4 says where each goes), and disc.json records
//! where each file starts. What no file holds (the volume descriptors, path tables, directories,
//! the UDF descriptors, nonzero bytes in the padding after a file) is kept sector by sector in
//! disc-structure.bin.
//!
//! pack writes the image back from those: the structure sectors, every file at its extent and
//! every DATA.DAT file at the sector its index gives, zeros elsewhere. Unedited, that is the disc's
//! own image, byte for byte.

use std::fs::{self, File};
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::archive;
use crate::iso::{self, Record, SECTOR};
use crate::{Error, Result, WithPath};

pub const LAYOUT: &str = "disc.json";
pub const STRUCTURE: &str = "disc-structure.bin";
const DATA: &str = "DATA.DAT";
const INDEX: &str = "DATA.HD4";

/// A file of the ISO 9660 file system: its path in the extracted folder (the disc's, without the
/// ";1" version and a trailing dot), where it starts and its size.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscFile {
    pub path: String,
    pub extent: u32,
    pub size: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Layout {
    pub note: String,
    /// The image's size in sectors.
    pub sectors: u64,
    /// Every file but DATA.DAT, in extent order.
    pub files: Vec<DiscFile>,
    /// DATA.DAT, written back from the files its index (DATA.HD4) lists.
    pub data: DiscFile,
    /// Runs of sectors (first, count) kept in disc-structure.bin, in that order.
    pub structure: Vec<(u64, u64)>,
}

/// Every file of the ISO 9660 file system with its path, folders first-met order.
pub fn iso_files(image: &[u8]) -> Result<Vec<(String, Record)>> {
    let mut found = Vec::new();
    let mut pending = vec![(String::new(), iso::root_record(image)?)];
    while let Some((path, folder)) = pending.pop() {
        for entry in iso::list_directory(image, &folder)? {
            let child = if path.is_empty() { entry.name.clone() } else { format!("{path}/{}", entry.name) };
            if entry.is_dir {
                pending.push((child, entry));
            } else {
                found.push((child, entry));
            }
        }
    }
    found.sort_by_key(|(_, record)| record.extent);
    Ok(found)
}

/// Byte ranges of the image that files hold: (offset, size).
fn spans(layout: &Layout, entries: &[archive::Entry]) -> Vec<(u64, u64)> {
    let mut found: Vec<(u64, u64)> =
        layout.files.iter().map(|file| (file.extent as u64 * SECTOR, file.size as u64)).collect();
    let base = layout.data.extent as u64 * SECTOR;
    found.extend(entries.iter().map(|entry| (base + entry.sector.max(0) as u64 * SECTOR, entry.size.max(0) as u64)));
    found
}

/// Writes the ISO 9660 files (but DATA.DAT) into out_dir, and the layout: disc.json and
/// disc-structure.bin.
pub fn record(image: &[u8], out_dir: &Path) -> Result<Layout> {
    let found = iso_files(image)?;
    let is_data = |path: &str| path.eq_ignore_ascii_case(DATA);
    let Some((data_path, data)) = found.iter().find(|(path, _)| is_data(path)) else {
        return Err(Error::Missing(DATA.into()));
    };
    let files: Vec<DiscFile> = found
        .iter()
        .filter(|(path, _)| !is_data(path))
        .map(|(path, record)| DiscFile { path: path.clone(), extent: record.extent, size: record.size })
        .collect();
    files.par_iter().try_for_each(|file| -> Result<()> {
        let target = out_dir.join(&file.path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).with_path("create", parent)?;
        }
        let bytes = iso::read_at(image, file.extent as u64 * SECTOR, file.size as u64);
        fs::write(&target, bytes).with_path("write", &target)
    })?;
    let mut layout = Layout {
        note: "Where each file of the disc goes, for dc2tk pack; disc-structure.bin holds the sectors no file \
               holds (volume descriptors, path tables, directories)."
            .into(),
        sectors: image.len().div_ceil(SECTOR as usize) as u64,
        files,
        data: DiscFile { path: data_path.clone(), extent: data.extent, size: data.size },
        structure: Vec::new(),
    };
    let index = archive::Archive::open(image)?;
    // per sector: 2 when file bytes fill it, 1 when a file ends in it, 0 when no file is in it
    let mut covered = vec![0u8; layout.sectors as usize];
    for (offset, size) in spans(&layout, &index.entries) {
        let first = (offset / SECTOR) as usize;
        let full = (size / SECTOR) as usize;
        for flag in covered.iter_mut().skip(first).take(full) {
            *flag = 2;
        }
        if size % SECTOR != 0 {
            if let Some(flag) = covered.get_mut(first + full) {
                *flag = (*flag).max(1);
            }
        }
    }
    let ends: std::collections::HashMap<u64, u64> = spans(&layout, &index.entries)
        .into_iter()
        .filter(|(_, size)| size % SECTOR != 0)
        .map(|(offset, size)| (offset / SECTOR + size / SECTOR, size % SECTOR))
        .collect();
    // a sector is kept when packing would not give its bytes: zeros where no file is, zeros after
    // the end of a file
    let kept: Vec<u64> = (0..layout.sectors)
        .into_par_iter()
        .filter(|&sector| {
            let flag = covered[sector as usize];
            if flag == 2 {
                return false;
            }
            let bytes = iso::read_at(image, sector * SECTOR, SECTOR);
            let from = if flag == 1 { ends.get(&sector).copied().unwrap_or(0) as usize } else { 0 };
            bytes.get(from..).is_some_and(|rest| rest.iter().any(|&b| b != 0))
        })
        .collect();
    let mut structure = Vec::with_capacity(kept.len() * SECTOR as usize);
    for &sector in &kept {
        let bytes = iso::read_at(image, sector * SECTOR, SECTOR);
        structure.extend_from_slice(bytes);
        structure.resize(structure.len().div_ceil(SECTOR as usize) * SECTOR as usize, 0);
        match layout.structure.last_mut() {
            Some((first, count)) if *first + *count == sector => *count += 1,
            _ => layout.structure.push((sector, 1)),
        }
    }
    let structure_path = out_dir.join(STRUCTURE);
    fs::write(&structure_path, structure).with_path("write", &structure_path)?;
    let layout_path = out_dir.join(LAYOUT);
    fs::write(&layout_path, crate::textfile::json_text(&layout)).with_path("write", &layout_path)?;
    Ok(layout)
}

pub fn read_layout(dir: &Path) -> Result<Layout> {
    let path = dir.join(LAYOUT);
    let text = fs::read_to_string(&path).map_err(|_| {
        Error::invalid(format!(
            "{} has no {LAYOUT}; extract the disc again (dc2tk extract) to record its layout",
            dir.display()
        ))
    })?;
    serde_json::from_str(&text).map_err(|error| Error::invalid(format!("{}: {error}", path.display())))
}

/// What pack wrote.
pub struct Packed {
    pub files: usize,
    pub data_files: usize,
    pub structure_sectors: u64,
    pub size: u64,
}

#[cfg(windows)]
fn write_at(file: &File, data: &[u8], offset: u64) -> std::io::Result<()> {
    use std::os::windows::fs::FileExt;
    let mut done = 0;
    while done < data.len() {
        done += file.seek_write(&data[done..], offset + done as u64)?;
    }
    Ok(())
}

#[cfg(unix)]
fn write_at(file: &File, data: &[u8], offset: u64) -> std::io::Result<()> {
    use std::os::unix::fs::FileExt;
    file.write_all_at(data, offset)
}

/// Writes the image an extracted disc came from: the files at their extents, DATA.DAT's files at
/// their sectors, the structure sectors, zeros elsewhere.
pub fn pack(dir: &Path, out_path: &Path) -> Result<Packed> {
    let layout = read_layout(dir)?;
    let read = |name: &str| {
        let path = dir.join(name);
        fs::read(&path).with_path("read", &path)
    };
    let entries = archive::parse_index(&read(INDEX)?)?;
    let structure = read(STRUCTURE)?;
    let base = layout.data.extent as u64 * SECTOR;
    let mut jobs: Vec<(u64, PathBuf, u64)> =
        layout.files.iter().map(|file| (file.extent as u64 * SECTOR, dir.join(&file.path), file.size as u64)).collect();
    jobs.extend(
        entries
            .iter()
            .map(|entry| (base + entry.sector.max(0) as u64 * SECTOR, dir.join(&entry.path), entry.size.max(0) as u64)),
    );
    if let Some(parent) = out_path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent).with_path("create", parent)?;
    }
    let out = File::create(out_path).with_path("write", out_path)?;
    let size = layout.sectors * SECTOR;
    out.set_len(size).with_path("write", out_path)?;
    jobs.par_iter().try_for_each(|(offset, path, expected)| -> Result<()> {
        let bytes = fs::read(path).with_path("read", path)?;
        if bytes.len() as u64 != *expected {
            return Err(Error::invalid(format!(
                "{} is {} bytes, and the disc's layout has room for {}; pack writes the disc as it was laid out",
                path.display(),
                bytes.len(),
                expected
            )));
        }
        write_at(&out, &bytes, *offset).with_path("write", out_path)
    })?;
    let mut at = 0usize;
    let mut sectors = 0;
    for &(first, count) in &layout.structure {
        let length = (count * SECTOR) as usize;
        let bytes =
            structure.get(at..at + length).ok_or_else(|| Error::invalid(format!("{STRUCTURE} is cut short")))?;
        write_at(&out, bytes, first * SECTOR).with_path("write", out_path)?;
        at += length;
        sectors += count;
    }
    Ok(Packed { files: layout.files.len(), data_files: entries.len(), structure_sectors: sectors, size })
}

/// The first offset where two images differ, or None when they are the same.
pub fn first_difference(a: &[u8], b: &[u8]) -> Option<u64> {
    const CHUNK: usize = 1 << 24;
    let shared = a.len().min(b.len());
    let found = (0..shared.div_ceil(CHUNK)).into_par_iter().find_first(|&chunk| {
        let range = chunk * CHUNK..((chunk + 1) * CHUNK).min(shared);
        a[range.clone()] != b[range]
    });
    match found {
        Some(chunk) => {
            let start = chunk * CHUNK;
            let end = ((chunk + 1) * CHUNK).min(shared);
            (start..end).find(|&at| a[at] != b[at]).map(|at| at as u64)
        }
        None if a.len() != b.len() => Some(shared as u64),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{extract, samples};

    #[test]
    fn an_extracted_disc_packs_back_to_its_image() {
        let folder = std::env::temp_dir().join(format!("dc2tk-disc-pack-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        let image = samples::sample_disc();
        extract::extract(&image, &folder.join("disc")).unwrap();
        let packed = pack(&folder.join("disc"), &folder.join("out.iso")).unwrap();
        let written = fs::read(folder.join("out.iso")).unwrap();
        assert_eq!(first_difference(&image, &written), None);
        assert!(packed.data_files > 0 && packed.structure_sectors > 0);
        // an edit to a file of DATA.DAT reaches the image
        fs::write(folder.join("disc/readme.txt"), b"HELLO").unwrap();
        pack(&folder.join("disc"), &folder.join("out.iso")).unwrap();
        let edited = fs::read(folder.join("out.iso")).unwrap();
        assert!(first_difference(&image, &edited).is_some());
        assert!(edited.windows(5).any(|window| window == b"HELLO"));
        fs::remove_dir_all(&folder).unwrap();
    }
}
