//! DATA.HD4, the index of the files in DATA.DAT, and the packs (.chr, .snd, .mpk, ...) that chain
//! files inside one file.

use crate::{iso, text, Error, Result};

pub const INDEX_RECORD: usize = 12;
/// What a new file takes in the index besides its path's bytes: its record, and the NUL that ends
/// its name.
pub const INDEX_COST_PER_FILE: usize = INDEX_RECORD + 1;

// A pack chains its members, each in a slot of PACK_SLOT bytes: the name (cp932, ended by a NUL)
// in the first PACK_NAME, then three i32s: where the member's data starts from the slot (always
// PACK_SLOT, right after it), its size, and where the next slot starts (PACK_SLOT plus the size
// padded to PACK_ALIGN); then 4 bytes the game does not read. A slot whose name starts with NUL
// ends the pack.
pub const PACK_NAME: usize = 64;
pub const PACK_SLOT: usize = 0x50;
pub const PACK_ALIGN: usize = 16;
const SLOT_OFFSET: usize = PACK_NAME;
const SLOT_SIZE: usize = PACK_NAME + 4;
const SLOT_NEXT: usize = PACK_NAME + 8;
/// Where a slot's last bytes, the ones the game does not read, start.
pub const SLOT_TAIL: usize = PACK_NAME + 12;

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

/// How many records the index has. The names follow the records, so the first record's name
/// starts where the records end; a count of more records than the data holds is refused here,
/// before anything is allocated for them.
fn record_count(data: &[u8]) -> Result<usize> {
    let first = i32_at(data, 0).ok_or_else(|| Error::invalid("the file index is empty"))?;
    let count = first as u32 as usize / INDEX_RECORD;
    if count > data.len() / INDEX_RECORD {
        return Err(Error::invalid(format!(
            "the file index is cut short: it has {count} records, and {} bytes",
            data.len()
        )));
    }
    Ok(count)
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

/// The members of a pack, in order, with where each one's bytes are; or None when the data is not
/// a pack (a slot that does not hold together, or no end slot right at the end).
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
        let data_offset = i32_at(data, offset + SLOT_OFFSET)? as i64;
        let size = i32_at(data, offset + SLOT_SIZE)? as i64;
        let next_offset = i32_at(data, offset + SLOT_NEXT)? as i64;
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

/// Sets a slot's numbers for a member of size bytes: its data right after the slot, its size, and
/// the next slot after the data padded to PACK_ALIGN. The name and the tail stay.
pub fn resize_slot(slot: &mut [u8], size: usize) -> Result<()> {
    let next = i32::try_from(PACK_SLOT + size + padding(size))
        .map_err(|_| Error::invalid(format!("a pack member of {size} bytes is more than a pack can hold")))?;
    slot[SLOT_OFFSET..SLOT_OFFSET + 4].copy_from_slice(&(PACK_SLOT as i32).to_le_bytes());
    slot[SLOT_SIZE..SLOT_SIZE + 4].copy_from_slice(&(size as i32).to_le_bytes());
    slot[SLOT_NEXT..SLOT_NEXT + 4].copy_from_slice(&next.to_le_bytes());
    Ok(())
}

/// A new member's name in cp932, as its slot holds it before the NUL.
fn member_name(name: &str) -> Result<Vec<u8>> {
    text::encode(name).filter(|raw| !raw.is_empty() && raw.len() < PACK_NAME).ok_or_else(|| {
        Error::invalid(format!("{name} cannot be a member's name (1 to {} bytes of Shift-JIS)", PACK_NAME - 1))
    })
}

/// The slot of a new member called name, of size bytes; its tail is zeros.
pub fn write_slot(name: &str, size: usize) -> Result<[u8; PACK_SLOT]> {
    let raw = member_name(name)?;
    let mut slot = [0u8; PACK_SLOT];
    slot[..raw.len()].copy_from_slice(&raw);
    resize_slot(&mut slot, size).map_err(|_| Error::invalid(format!("{name} is too big for a pack")))?;
    Ok(slot)
}

/// A pack of new members, in order, ended by an empty slot.
pub fn write_pack(members: &[(&str, &[u8])]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    for (name, data) in members {
        out.extend(write_slot(name, data.len())?);
        out.extend_from_slice(data);
        out.extend(std::iter::repeat_n(0, padding(data.len())));
    }
    out.extend([0u8; PACK_SLOT]);
    Ok(out)
}

/// A member of a Pack: its bytes, and what its slot holds besides its numbers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackMember {
    pub name: String,
    /// The slot's name field as it was: the name, a NUL, and whatever follows (some of the game's
    /// packs have leftover bytes there, which Pack::write keeps).
    pub raw_name: [u8; PACK_NAME],
    /// The slot's last bytes, which the game does not read (Pack::write keeps them).
    pub slot_tail: [u8; PACK_SLOT - SLOT_TAIL],
    pub data: Vec<u8>,
}

impl PackMember {
    /// A new member: the name in cp932, zeros after it and in the tail.
    pub fn new(name: &str, data: Vec<u8>) -> Result<PackMember> {
        let raw = member_name(name)?;
        let mut raw_name = [0u8; PACK_NAME];
        raw_name[..raw.len()].copy_from_slice(&raw);
        Ok(PackMember { name: name.to_string(), raw_name, slot_tail: [0; PACK_SLOT - SLOT_TAIL], data })
    }
}

/// A pack read whole, to change and write back byte for byte: its members in order, each with its
/// slot's leftover bytes, and the slot that ends it as it was (its first byte is 0; the rest, which
/// some of the game's packs fill, is kept).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pack {
    pub members: Vec<PackMember>,
    pub terminator: [u8; PACK_SLOT],
}

impl Pack {
    /// The pack the data holds, or None when it is not one (as parse_pack decides).
    pub fn read(data: &[u8]) -> Option<Pack> {
        let members = parse_pack(data)?
            .into_iter()
            .map(|member| {
                let slot = &data[member.offset - PACK_SLOT..member.offset];
                PackMember {
                    raw_name: slot[..PACK_NAME].try_into().unwrap(),
                    slot_tail: slot[SLOT_TAIL..].try_into().unwrap(),
                    data: member.data(data).to_vec(),
                    name: member.name,
                }
            })
            .collect();
        Some(Pack { members, terminator: data[data.len() - PACK_SLOT..].try_into().unwrap() })
    }

    /// The pack's bytes: each member's slot, then its data padded to PACK_ALIGN with zeros, then
    /// the end slot.
    pub fn write(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        for member in &self.members {
            let mut slot = [0u8; PACK_SLOT];
            slot[..PACK_NAME].copy_from_slice(&member.raw_name);
            slot[SLOT_TAIL..].copy_from_slice(&member.slot_tail);
            resize_slot(&mut slot, member.data.len())
                .map_err(|_| Error::invalid(format!("{} is too big for a pack", member.name)))?;
            out.extend(slot);
            out.extend(&member.data);
            out.extend(std::iter::repeat_n(0, padding(member.data.len())));
        }
        out.extend(self.terminator);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::samples::pack;

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
    fn a_slot_holds_name_offset_size_and_next() {
        let slot = write_slot("info.cfg", 21).unwrap();
        assert_eq!(&slot[..9], b"info.cfg\0");
        let numbers: Vec<i32> = slot[64..76].chunks(4).map(|b| i32::from_le_bytes(b.try_into().unwrap())).collect();
        assert_eq!(numbers, [0x50, 21, 0x50 + 32]);
        assert_eq!(&slot[76..], [0; 4]);
        assert!(write_slot("", 1).is_err());
        assert!(write_slot(&"x".repeat(64), 1).is_err());
        assert!(write_slot("big", i32::MAX as usize).is_err());
    }

    #[test]
    fn a_pack_read_whole_writes_back_its_leftover_bytes() {
        let mut data = pack(&[("a.hd", b"hello"), ("b.bd", &[1u8; 32])]);
        // bytes after a name's NUL, in a slot's tail and in the end slot
        data[10] = 0x55;
        data[78] = 0x66;
        let end = data.len() - PACK_SLOT;
        data[end + 40] = 0x77;
        let pack = Pack::read(&data).unwrap();
        assert_eq!(pack.members.iter().map(|member| member.name.as_str()).collect::<Vec<_>>(), ["a.hd", "b.bd"]);
        assert_eq!(pack.members[0].raw_name[10], 0x55);
        assert_eq!(pack.members[0].slot_tail, [0, 0, 0x66, 0]);
        assert_eq!(pack.write().unwrap(), data);
        let mut grown = pack.clone();
        grown.members[1].data.push(2);
        grown.members.push(PackMember::new("c.vol", b"v".to_vec()).unwrap());
        let written = grown.write().unwrap();
        let members = parse_pack_with_data(&written).unwrap();
        assert_eq!(members[1], ("b.bd".to_string(), [[1u8; 32].as_slice(), &[2]].concat().as_slice()));
        assert_eq!(members[2], ("c.vol".to_string(), b"v".as_slice()));
        assert_eq!(written[written.len() - PACK_SLOT + 40], 0x77);
        assert_eq!(Pack::read(b"not a pack"), None);
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

    #[test]
    fn an_index_claiming_more_records_than_it_holds_is_refused() {
        // the first name's offset says 0xFFFFFFF0 / 12 records (with_entries made room for them
        // all, 11 GB, before reading one); there are 16 bytes
        let index = [0xF0, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let refused = "the file index is cut short: it has 357913940 records, and 16 bytes";
        assert_eq!(parse_index(&index).unwrap_err().to_string(), refused);
        assert_eq!(with_entries(&index, &[("a.bin".to_string(), 1, 0)]).unwrap_err().to_string(), refused);
        assert!(parse_index(&[0xFF; 4]).is_err());
    }
}
