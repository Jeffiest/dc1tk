//! ISO 9660: the directory records the game's files are found by.

use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

use crate::{Error, Result, WithPath};

pub const SECTOR: u64 = 2048;
pub const PVD_SECTOR: u64 = 16;

/// A directory record: a file or folder, where its data is, and where the record itself is (so a
/// build can point it elsewhere).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub name: String,
    pub extent: u32,
    pub size: u32,
    pub is_dir: bool,
    pub at: Option<u64>,
}

/// Bytes of the image from offset, as many as there are (a read past the end is short).
pub fn read_at(image: &[u8], offset: u64, size: u64) -> &[u8] {
    let start = (offset as usize).min(image.len());
    let end = (offset.saturating_add(size) as usize).min(image.len());
    &image[start..end]
}

fn clean_name(raw: &[u8]) -> String {
    if raw == b"\x00" || raw == b"\x01" {
        return String::new();
    }
    let text: String = raw.iter().map(|&b| if b < 0x80 { b as char } else { '\u{FFFD}' }).collect();
    let name = text.split(';').next().unwrap_or("");
    name.trim_end_matches('.').to_string()
}

fn u32_at(data: &[u8], offset: usize) -> Result<u32> {
    data.get(offset..offset + 4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .ok_or_else(|| Error::invalid("a directory record is cut short"))
}

fn parse_record(raw: &[u8], at: Option<u64>) -> Result<Record> {
    let name_length = *raw.get(32).ok_or_else(|| Error::invalid("a directory record is cut short"))? as usize;
    let name = raw.get(33..33 + name_length).ok_or_else(|| Error::invalid("a directory record is cut short"))?;
    Ok(Record { name: clean_name(name), extent: u32_at(raw, 2)?, size: u32_at(raw, 10)?, is_dir: raw[25] & 2 != 0, at })
}

pub fn root_record(image: &[u8]) -> Result<Record> {
    let pvd = read_at(image, PVD_SECTOR * SECTOR, SECTOR);
    if pvd.len() < 190 || &pvd[1..6] != b"CD001" {
        return Err(Error::NotIso);
    }
    parse_record(&pvd[156..190], None)
}

pub fn list_directory(image: &[u8], record: &Record) -> Result<Vec<Record>> {
    let base = record.extent as u64 * SECTOR;
    let data = read_at(image, base, record.size as u64);
    let mut entries = Vec::new();
    let mut offset = 0usize;
    while offset < data.len() {
        let length = data[offset] as usize;
        if length == 0 {
            offset = (offset / SECTOR as usize + 1) * SECTOR as usize;
            continue;
        }
        let raw = &data[offset..(offset + length).min(data.len())];
        let entry = parse_record(raw, Some(base + offset as u64))?;
        if !entry.name.is_empty() {
            entries.push(entry);
        }
        offset += length;
    }
    Ok(entries)
}

/// A file or folder by path, ignoring case.
pub fn find(image: &[u8], path: &str) -> Result<Option<Record>> {
    let mut record = root_record(image)?;
    let path = path.replace('\\', "/");
    for part in path.trim_matches('/').split('/') {
        let wanted = part.to_uppercase();
        match list_directory(image, &record)?.into_iter().find(|entry| entry.name.to_uppercase() == wanted) {
            Some(entry) => record = entry,
            None => return Ok(None),
        }
    }
    Ok(Some(record))
}

pub fn read_file<'a>(image: &'a [u8], record: &Record) -> &'a [u8] {
    read_at(image, record.extent as u64 * SECTOR, record.size as u64)
}

/// Points a file's directory record at a new extent and size, in the image open as file (at path);
/// ISO 9660 stores both little- and big-endian.
pub fn set_extent(file: &mut File, path: &Path, record: &Record, extent: u32, size: u32) -> Result<()> {
    let at = record.at.ok_or_else(|| Error::invalid("the root has no directory record to change"))?;
    let mut bytes = Vec::with_capacity(16);
    bytes.extend(extent.to_le_bytes());
    bytes.extend(extent.to_be_bytes());
    bytes.extend(size.to_le_bytes());
    bytes.extend(size.to_be_bytes());
    file.seek(SeekFrom::Start(at + 2)).and_then(|_| file.write_all(&bytes)).with_path("write", path)
}
