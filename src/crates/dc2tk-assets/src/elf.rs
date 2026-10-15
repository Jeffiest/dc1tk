//! The boot executable, `SCES_511.90`, as the data tables read and patch it: where a vaddr of its
//! `.data` is in the file, and the bytes a mod changes there, made in place so the executable
//! keeps its size and layout (the monster table, spectrumising, Donny's shop, the character
//! tables, parts of the inventions and fishing tables).

use std::collections::BTreeMap;

use crate::bytes::{u16_at, u32_at};
use crate::{bail, Result};

/// The boot executable, as the disc's root names it.
pub const EXECUTABLE: &str = "SCES_511.90";

/// Where a vaddr is in an ELF file: by the program header that loads it from the file.
pub fn file_offset(elf: &[u8], vaddr: u32) -> Option<usize> {
    if elf.len() < 0x34 || &elf[..4] != b"\x7fELF" {
        return None;
    }
    let (table, size, count) = (u32_at(elf, 0x1C).ok()? as usize, u16_at(elf, 0x2A).ok()?, u16_at(elf, 0x2C).ok()?);
    for number in 0..count as usize {
        let header = table + number * size as usize;
        let word = |at: usize| u32_at(elf, header + at).ok();
        let (kind, offset, address, file_size) = (word(0)?, word(4)?, word(8)?, word(16)?);
        if kind == 1 && vaddr >= address && vaddr - address < file_size {
            return Some((offset + (vaddr - address)) as usize);
        }
    }
    None
}

/// A table of the executable at `address`: its offset in the file, checked to hold `size` bytes.
pub fn table_offset(elf: &[u8], address: u32, size: usize, what: &str) -> Result<usize> {
    match file_offset(elf, address).filter(|start| start + size <= elf.len()) {
        Some(start) => Ok(start),
        None => bail!("{EXECUTABLE}: not the PAL executable (no {what} at 0x{address:X})"),
    }
}

/// Bytes a mod changes in the executable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Patch {
    pub offset: usize,
    pub bytes: Vec<u8>,
    pub what: String,
    pub module: String,
}

/// The executable with the patches made; two that give one value different bytes are refused,
/// naming both mods, as is a patch past the executable's end.
pub fn apply_patches(elf: &[u8], patches: &[Patch]) -> Result<Vec<u8>> {
    let mut elf = elf.to_vec();
    let mut done: BTreeMap<usize, &Patch> = BTreeMap::new();
    for patch in patches {
        if let Some(other) = done.get(&patch.offset) {
            if other.bytes != patch.bytes {
                return Err(crate::given_two_ways(&patch.what, &other.module, &patch.module));
            }
            continue;
        }
        done.insert(patch.offset, patch);
        let Some(place) = elf.get_mut(patch.offset..patch.offset + patch.bytes.len()) else {
            bail!("{}: byte 0x{:X} is past the executable's end", patch.what, patch.offset);
        };
        place.copy_from_slice(&patch.bytes);
    }
    Ok(elf)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// An ELF with one PT_LOAD of 0x2000 bytes at vaddr 0x341000, from file offset 0x100.
    pub(crate) fn sample() -> Vec<u8> {
        let base = 0x341000u32;
        let mut elf = vec![0u8; 0x100 + 0x2000];
        elf[..4].copy_from_slice(b"\x7fELF");
        elf[0x1C..0x20].copy_from_slice(&0x34u32.to_le_bytes());
        elf[0x2A..0x2C].copy_from_slice(&32u16.to_le_bytes());
        elf[0x2C..0x2E].copy_from_slice(&1u16.to_le_bytes());
        let header = [1u32, 0x100, base, base, 0x2000, 0x2000];
        for (at, word) in header.iter().enumerate() {
            elf[0x34 + at * 4..0x38 + at * 4].copy_from_slice(&word.to_le_bytes());
        }
        elf
    }

    #[test]
    fn a_vaddr_is_found_by_the_header_that_loads_it() {
        let elf = sample();
        assert_eq!(file_offset(&elf, 0x341DA0), Some(0x100 + 0xDA0));
        assert_eq!(file_offset(&elf, 0x343000), None);
        assert_eq!(file_offset(&[0; 16], 0x341DA0), None);
        assert!(table_offset(&elf, 0x342FF0, 0x20, "a table").unwrap_err().to_string().contains("no a table"));
    }

    #[test]
    fn patches_that_disagree_or_run_past_the_end_are_refused() {
        let elf = vec![0u8; 8];
        let patch = |offset: usize, byte: u8, module: &str| Patch {
            offset,
            bytes: vec![byte, byte],
            what: format!("value at {offset}"),
            module: module.into(),
        };
        assert_eq!(apply_patches(&elf, &[patch(2, 5, "a"), patch(2, 5, "b")]).unwrap()[2..4], [5, 5]);
        let error = apply_patches(&elf, &[patch(2, 5, "a"), patch(2, 6, "b")]).unwrap_err().to_string();
        assert!(error.contains("mod a and mod b"), "{error}");
        assert!(apply_patches(&elf, &[patch(7, 1, "a")]).unwrap_err().to_string().contains("past"));
    }
}
