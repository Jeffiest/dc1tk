//! DATA.HD4, the index of the files in DATA.DAT, and the packs (.chr, .snd, .mpk, ...) that chain
//! files inside one file.

use crate::{iso, text, Error, Result};

pub const INDEX_RECORD: usize = 12;
pub const PACK_NAME: usize = 64;
pub const PACK_SLOT: usize = 0x50;
pub const PACK_ALIGN: usize = 16;

/// The engine reads DATA.HD4 into a buffer of this size (dataread.cpp, header_buff).
pub const INDEX_LIMIT: usize = 0x50000;

/// A file of DATA.DAT: its path (with "/"), size and sector from the start of DATA.DAT.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: String,
    pub size: i32,
    pub sector: i32,
}

/// A member of a pack: its name and where its bytes are in the pack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    pub name: String,
    pub offset: usize,
    pub size: usize,
}

impl Member {
    pub fn data<'a>(&self, pack: &'a [u8]) -> &'a [u8] {
        &pack[self.offset..self.offset + self.size]
    }
}

pub fn padding(size: usize) -> usize {
    (PACK_ALIGN - size % PACK_ALIGN) % PACK_ALIGN
}

pub(crate) fn i32_at(data: &[u8], offset: usize) -> Option<i32> {
    data.get(offset..offset + 4).map(|b| i32::from_le_bytes(b.try_into().unwrap()))
}

fn record(data: &[u8], number: usize) -> Result<(i32, i32, i32)> {
    let at = number * INDEX_RECORD;
    match (i32_at(data, at), i32_at(data, at + 4), i32_at(data, at + 8)) {
        (Some(name), Some(size), Some(sector)) => Ok((name, size, sector)),
        _ => Err(Error::invalid("the file index is cut short")),
    }
}

fn name_bytes(data: &[u8], offset: i32) -> Result<&[u8]> {
    let start = usize::try_from(offset).map_err(|_| Error::invalid("a file index name is out of range"))?;
    let rest = data.get(start..).ok_or_else(|| Error::invalid("a file index name is out of range"))?;
    let end = rest.iter().position(|&b| b == 0).ok_or_else(|| Error::invalid("a file index name is not ended"))?;
    Ok(&rest[..end])
}

fn read_name(data: &[u8], offset: i32) -> Result<String> {
    let name = text::decode_lossy(name_bytes(data, offset)?).replace('\\', "/");
    Ok(name.trim_start_matches('/').to_string())
}

fn record_count(data: &[u8]) -> Result<usize> {
    let first = i32_at(data, 0).ok_or_else(|| Error::invalid("the file index is empty"))?;
    Ok(first as u32 as usize / INDEX_RECORD)
}

pub fn parse_index(data: &[u8]) -> Result<Vec<Entry>> {
    let mut entries = Vec::new();
    for number in 0..record_count(data)? {
        let (name, size, sector) = record(data, number)?;
        let path = read_name(data, name)?;
        if path.is_empty() {
            break;
        }
        entries.push(Entry { path, size, sector });
    }
    Ok(entries)
}

/// The index with records added for new files, (path, size, sector), before the terminator. The
/// names follow the records, so every name moves by the records added; the game's paths keep their
/// order and spelling, and new ones are written with "\" as the game's are.
pub fn with_entries(data: &[u8], added: &[(String, i32, i32)]) -> Result<Vec<u8>> {
    let count = record_count(data)?;
    let mut rows: Vec<(Vec<u8>, i32, i32)> = Vec::with_capacity(count + added.len());
    for number in 0..count {
        let (name, size, sector) = record(data, number)?;
        rows.push((name_bytes(data, name)?.to_vec(), size, sector));
    }
    rows.pop();
    for (path, size, sector) in added {
        let name = text::encode(&path.replace('/', "\\"))
            .ok_or_else(|| Error::invalid(format!("{path}: the path has a character the game's index cannot hold")))?;
        rows.push((name, *size, *sector));
    }
    rows.push((Vec::new(), 0, 0));
    let start = rows.len() * INDEX_RECORD;
    let mut table = Vec::with_capacity(start);
    let mut names = Vec::new();
    for (name, size, sector) in &rows {
        let offset =
            i32::try_from(start + names.len()).map_err(|_| Error::invalid("the file index would be more than 2 GB"))?;
        table.extend(offset.to_le_bytes());
        table.extend(size.to_le_bytes());
        table.extend(sector.to_le_bytes());
        names.extend(name);
        names.push(0);
    }
    table.extend(names);
    let pad = (4 - table.len() % 4) % 4;
    table.extend(std::iter::repeat_n(0, pad));
    Ok(table)
}

/// The disc's file index and where DATA.DAT starts.
pub struct Archive {
    pub entries: Vec<Entry>,
    pub base: u64,
}

impl Archive {
    pub fn open(image: &[u8]) -> Result<Archive> {
        let index = iso::find(image, "DATA.HD4")?.ok_or_else(|| Error::Missing("DATA.HD4".into()))?;
        let dat = iso::find(image, "DATA.DAT")?.ok_or_else(|| Error::Missing("DATA.DAT".into()))?;
        Ok(Archive { entries: parse_index(iso::read_file(image, &index))?, base: dat.extent as u64 * iso::SECTOR })
    }

    pub fn read<'a>(&self, image: &'a [u8], entry: &Entry) -> &'a [u8] {
        let offset = self.base as i64 + entry.sector as i64 * iso::SECTOR as i64;
        iso::read_at(image, offset.max(0) as u64, entry.size.max(0) as u64)
    }

    /// The entry with this path, as the index spells it.
    pub fn entry(&self, path: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.path == path)
    }
}

/// A pack member's name, or None when the bytes cannot be one (so the file is not a pack).
pub fn pack_name(raw: &[u8]) -> Option<String> {
    let end = raw.iter().position(|&b| b == 0)?;
    if end == 0 {
        return None;
    }
    let text = &raw[..end];
    if text.iter().any(|&b| b < 0x20 || b == 0x7F) {
        return None;
    }
    let name = text::decode_strict(text)?;
    if name.contains('/') || name.contains('\\') || name == "." || name == ".." {
        return None;
    }
    Some(name)
}

/// The members of a pack, or None when the data is not one.
/// A pack's members with their bytes, or None when the data is not a pack.
pub fn parse_pack_with_data(data: &[u8]) -> Option<Vec<(String, &[u8])>> {
    Some(
        parse_pack(data)?
            .into_iter()
            .map(|member| {
                let bytes = member.data(data);
                (member.name, bytes)
            })
            .collect(),
    )
}

pub fn parse_pack(data: &[u8]) -> Option<Vec<Member>> {
    let mut members = Vec::new();
    let mut offset = 0usize;
    while offset + PACK_SLOT <= data.len() {
        if data[offset] == 0 {
            if offset + PACK_SLOT == data.len() && !members.is_empty() {
                return Some(members);
            }
            return None;
        }
        let name = pack_name(&data[offset..offset + PACK_NAME])?;
        let data_offset = i32_at(data, offset + PACK_NAME)? as i64;
        let size = i32_at(data, offset + PACK_NAME + 4)? as i64;
        let next_offset = i32_at(data, offset + PACK_NAME + 8)? as i64;
        let start = offset as i64 + data_offset;
        if data_offset != PACK_SLOT as i64 || size < 0 || start + size > data.len() as i64 {
            return None;
        }
        if next_offset != PACK_SLOT as i64 + size + padding(size as usize) as i64 {
            return None;
        }
        members.push(Member { name, offset: start as usize, size: size as usize });
        offset += next_offset as usize;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(members: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        for (name, data) in members {
            let mut slot = vec![0u8; PACK_SLOT];
            slot[..name.len()].copy_from_slice(name.as_bytes());
            slot[64..68].copy_from_slice(&(PACK_SLOT as i32).to_le_bytes());
            slot[68..72].copy_from_slice(&(data.len() as i32).to_le_bytes());
            slot[72..76].copy_from_slice(&((PACK_SLOT + data.len() + padding(data.len())) as i32).to_le_bytes());
            out.extend(slot);
            out.extend(*data);
            out.extend(vec![0u8; padding(data.len())]);
        }
        out.extend(vec![0u8; PACK_SLOT]);
        out
    }

    #[test]
    fn parses_a_pack() {
        let data = pack(&[("a.hd", b"hello"), ("b.bd", &[1u8; 32])]);
        let members = parse_pack(&data).unwrap();
        assert_eq!(members.len(), 2);
        assert_eq!(members[0].name, "a.hd");
        assert_eq!(members[0].data(&data), b"hello");
        assert_eq!(members[1].data(&data), &[1u8; 32]);
    }

    #[test]
    fn not_a_pack() {
        assert!(parse_pack(b"plain text, no pack here, long enough to fill a slot of eighty bytes or more........")
            .is_none());
        assert!(parse_pack(&[0u8; PACK_SLOT]).is_none());
    }

    #[test]
    fn index_with_entries() {
        // an index holding only its terminator
        let original =
            with_entries(&[12, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], &[("a/b.bin".to_string(), 5, 7)]).unwrap();
        let entries = parse_index(&original).unwrap();
        assert_eq!(entries, vec![Entry { path: "a/b.bin".into(), size: 5, sector: 7 }]);
        assert_eq!(&original[original.len() - 12..original.len() - 4], b"a\\b.bin\0");
    }
}
