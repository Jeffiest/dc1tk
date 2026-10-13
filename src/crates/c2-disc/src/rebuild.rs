//! Changing files of the disc: a member deep in packs, and a new image with replaced and added
//! files.

use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

use crate::archive::{self, Archive, Member};
use crate::{iso, Error, Result};

pub const SCRIPT_SUFFIXES: [&str; 2] = [".stb", ".STB"];
const VOLUME_SIZE: usize = 80;
const FILLER: &str = "DMMY";

/// The member called name: by its exact name; for the last of a chain also a script by its name
/// without .stb; then ignoring case, as the game finds members (GetPackFile in dataread.cpp).
pub fn member_named<'a>(members: &'a [Member], name: &str, last: bool) -> Option<&'a Member> {
    let script = |member: &Member| {
        last && member.name.rsplit_once('.').map(|(stem, _)| stem) == Some(name)
            && SCRIPT_SUFFIXES.iter().any(|suffix| member.name.ends_with(suffix))
    };
    if let Some(found) = members.iter().find(|member| member.name == name || script(member)) {
        return Some(found);
    }
    let lower = name.to_lowercase();
    members.iter().find(|member| member.name.to_lowercase() == lower)
}

/// The bytes of the member a chain of names leads to inside a pack.
pub fn read_member<'a>(data: &'a [u8], names: &[String]) -> Option<&'a [u8]> {
    let Some((first, rest)) = names.split_first() else {
        return Some(data);
    };
    let members = archive::parse_pack(data)?;
    let member = member_named(&members, first, rest.is_empty())?;
    read_member(member.data(data), rest)
}

/// The pack with the member a chain of names leads to replaced, every pack around it resized.
pub fn replaced_member(data: &[u8], names: &[String], new: &[u8]) -> Result<Vec<u8>> {
    let Some((first, rest)) = names.split_first() else {
        return Ok(new.to_vec());
    };
    let members = archive::parse_pack(data)
        .ok_or_else(|| Error::invalid(format!("{first}: the file holding it is not a pack")))?;
    let member = member_named(&members, first, rest.is_empty())
        .ok_or_else(|| Error::invalid(format!("the pack has no member {first}")))?;
    let inner = replaced_member(member.data(data), rest, new)?;
    let entry = member.offset - archive::PACK_SLOT;
    let old_end = member.offset + member.size + archive::padding(member.size);
    let mut header = data[entry..member.offset].to_vec();
    let size = inner.len();
    header[archive::PACK_NAME + 4..archive::PACK_NAME + 8].copy_from_slice(&(size as i32).to_le_bytes());
    let next = archive::PACK_SLOT + size + archive::padding(size);
    header[archive::PACK_NAME + 8..archive::PACK_NAME + 12].copy_from_slice(&(next as i32).to_le_bytes());
    let mut out = Vec::with_capacity(data.len() - member.size + size);
    out.extend_from_slice(&data[..entry]);
    out.extend(header);
    out.extend(&inner);
    out.extend(std::iter::repeat_n(0, archive::padding(size)));
    out.extend_from_slice(&data[old_end..]);
    Ok(out)
}

pub fn sectors(size: u64) -> u64 {
    size.div_ceil(iso::SECTOR)
}

/// Free sectors for files that move or are new: the DMMY filler first, then past the end of the
/// volume.
struct Space {
    next: u64,
    limit: Option<u64>,
    volume: u64,
}

impl Space {
    fn allocate(&mut self, count: u64) -> u64 {
        if let Some(limit) = self.limit {
            if self.next + count > limit {
                self.next = self.volume;
                self.limit = None;
            }
        }
        let lba = self.next;
        self.next += count;
        self.volume = self.volume.max(self.next);
        lba
    }
}

/// Where build put a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placed {
    pub path: String,
    pub size: usize,
    pub moved: bool,
    pub added: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Written {
    pub placed: Vec<Placed>,
    /// Sectors the image grew by.
    pub grown: u64,
    /// Bytes left in the engine's index buffer.
    pub index_free: i64,
}

fn write_padded(file: &mut File, lba: u64, data: &[u8]) -> Result<()> {
    file.seek(SeekFrom::Start(lba * iso::SECTOR))?;
    file.write_all(data)?;
    let pad = sectors(data.len() as u64) * iso::SECTOR - data.len() as u64;
    file.write_all(&vec![0u8; pad as usize])?;
    Ok(())
}

fn u32_at(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
}

/// A copy of the image with files of DATA.DAT replaced, by path as the index spells it, and new
/// files added. A file that fits its sectors stays where it is; one that grows moves into the DMMY
/// filler, then past the end of the volume. New files get index records, and an index that
/// outgrows its sectors moves too.
///
/// The copy comes first, before the source is mapped: Windows clones a file's blocks for a copy
/// (seconds for 4.7 GB) but not while the file is mapped (half a minute), so callers should not
/// hold the source mapped either.
pub fn write_disc(iso_path: &Path, out_path: &Path, files: &BTreeMap<String, Vec<u8>>) -> Result<Written> {
    let mut out = write_copy(iso_path, out_path)?;
    let source = crate::Image::open(iso_path)?;
    let image = source.bytes();
    let index_record =
        iso::find(image, "DATA.HD4")?.ok_or_else(|| Error::invalid("DATA.HD4 is missing from the disc"))?;
    let data_record =
        iso::find(image, "DATA.DAT")?.ok_or_else(|| Error::invalid("DATA.DAT is missing from the disc"))?;
    let filler = iso::find(image, FILLER)?;
    let mut index_data = iso::read_file(image, &index_record).to_vec();
    let entries = archive::parse_index(&index_data)?;
    let mut position = HashMap::new();
    for (number, entry) in entries.iter().enumerate() {
        position.insert(entry.path.as_str(), number);
    }
    let mut pvd = iso::read_at(image, iso::PVD_SECTOR * iso::SECTOR, iso::SECTOR).to_vec();
    let volume = (u32_at(&pvd, VOLUME_SIZE) as u64).max(sectors(image.len() as u64));
    let mut space = match &filler {
        None => Space { next: volume, limit: None, volume },
        Some(filler) => Space {
            next: filler.extent as u64,
            limit: Some(filler.extent as u64 + sectors(filler.size as u64)),
            volume,
        },
    };

    let mut placed = Vec::new();
    let mut added = Vec::new();
    let data_extent = data_record.extent as i64;
    for (path, data) in files {
        let count = sectors(data.len() as u64);
        let Some(&number) = position.get(path.as_str()) else {
            // a file the disc does not have: into free space, with a new index record
            let lba = space.allocate(count);
            write_padded(&mut out, lba, data)?;
            added.push((path.clone(), data.len() as i32, (lba as i64 - data_extent) as i32));
            placed.push(Placed { path: path.clone(), size: data.len(), moved: true, added: true });
            continue;
        };
        let entry = &entries[number];
        let moved = count > sectors(entry.size.max(0) as u64);
        let lba = if moved { space.allocate(count) } else { (data_extent + entry.sector as i64) as u64 };
        write_padded(&mut out, lba, data)?;
        let at = number * archive::INDEX_RECORD + 4;
        index_data[at..at + 4].copy_from_slice(&(data.len() as i32).to_le_bytes());
        index_data[at + 4..at + 8].copy_from_slice(&((lba as i64 - data_extent) as i32).to_le_bytes());
        placed.push(Placed { path: path.clone(), size: data.len(), moved, added: false });
    }
    if !added.is_empty() {
        index_data = archive::with_entries(&index_data, &added)?;
        if index_data.len() > archive::INDEX_LIMIT {
            return Err(Error::invalid(format!(
                "the game's file index would be {} bytes with {} new files, and the game reads at most {}; \
                 put new files into packs instead",
                index_data.len(),
                added.len(),
                archive::INDEX_LIMIT
            )));
        }
    }
    let mut index_extent = index_record.extent as u64;
    if sectors(index_data.len() as u64) > sectors(index_record.size as u64) {
        index_extent = space.allocate(sectors(index_data.len() as u64));
    }
    write_padded(&mut out, index_extent, &index_data)?;
    if !added.is_empty() {
        // the engine sizes the index by its directory record (sceLseek in InitCDFile)
        iso::set_extent(&mut out, &index_record, index_extent as u32, index_data.len() as u32)?;
    }
    if space.volume > volume {
        pvd[VOLUME_SIZE..VOLUME_SIZE + 4].copy_from_slice(&(space.volume as u32).to_le_bytes());
        pvd[VOLUME_SIZE + 4..VOLUME_SIZE + 8].copy_from_slice(&(space.volume as u32).to_be_bytes());
        out.seek(SeekFrom::Start(iso::PVD_SECTOR * iso::SECTOR))?;
        out.write_all(&pvd)?;
    }
    out.flush()?;
    Ok(Written {
        placed,
        grown: space.volume - volume,
        index_free: archive::INDEX_LIMIT as i64 - index_data.len() as i64,
    })
}

/// The output image, a copy of the input to change in place.
fn write_copy(iso_path: &Path, out_path: &Path) -> Result<File> {
    if let Some(parent) = out_path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::copy(iso_path, out_path)?;
    Ok(OpenOptions::new().read(true).write(true).open(out_path)?)
}

/// The bytes a disc file, or a member inside it, reads back as from an image.
pub fn read_back(image: &[u8], archive: &Archive, top: &str, members: &[String]) -> Option<Vec<u8>> {
    let entry = archive.entry(top)?;
    read_member(archive.read(image, entry), members).map(|data| data.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(members: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        for (name, data) in members {
            let mut slot = vec![0u8; archive::PACK_SLOT];
            slot[..name.len()].copy_from_slice(name.as_bytes());
            slot[64..68].copy_from_slice(&(archive::PACK_SLOT as i32).to_le_bytes());
            slot[68..72].copy_from_slice(&(data.len() as i32).to_le_bytes());
            let next = archive::PACK_SLOT + data.len() + archive::padding(data.len());
            slot[72..76].copy_from_slice(&(next as i32).to_le_bytes());
            out.extend(slot);
            out.extend(*data);
            out.extend(vec![0u8; archive::padding(data.len())]);
        }
        out.extend(vec![0u8; archive::PACK_SLOT]);
        out
    }

    #[test]
    fn replaces_a_member_inside_a_member() {
        let inner = pack(&[("x.cfg", b"old"), ("y.img", b"keep")]);
        let outer = pack(&[("a.chr", &inner), ("s01.stb", b"script")]);
        let names = vec!["a.chr".to_string(), "X.CFG".to_string()];
        let changed = replaced_member(&outer, &names, b"a longer replacement").unwrap();
        assert_eq!(read_member(&changed, &names).unwrap(), b"a longer replacement");
        assert_eq!(read_member(&changed, &["a.chr".into(), "y.img".into()]).unwrap(), b"keep");
        assert_eq!(read_member(&changed, &["s01".into()]).unwrap(), b"script");
        assert_eq!(
            changed,
            pack(&[
                ("a.chr", &pack(&[("x.cfg", b"a longer replacement"), ("y.img", b"keep")])),
                ("s01.stb", b"script")
            ])
        );
    }
}
