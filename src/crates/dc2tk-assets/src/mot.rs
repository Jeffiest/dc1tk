//! Key-list files (.mot motions, .wgt skin weights) and .bbp bind-pose matrix tables. A key-list
//! file is blocks of a 32-byte header (frame, target, type, 0x20, key count, the block size or 0
//! on the last, two garbage words) and 32-byte keys (frame, 12 bytes, four floats).

use std::collections::BTreeMap;

use crate::bytes::{f32s_at, Reader};
use crate::mds::pack_floats;
use crate::{bail, Result};

/// One block of keys.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyList {
    pub frame: i32,
    pub target: i32,
    pub kind: i32,
    /// (frame, value) pairs.
    pub keys: Vec<(i64, [f64; 4])>,
    pub unk_0c: u32,
    pub garbage: (u32, u32),
    /// The 12 bytes after a key's frame, by key, when not zero.
    pub key_pad: Option<BTreeMap<usize, Vec<u8>>>,
    /// The block's size word when it is neither 0 nor the size.
    pub more_raw: Option<u32>,
}

impl KeyList {
    pub fn new(frame: i32, target: i32, kind: i32, keys: Vec<(i64, [f64; 4])>) -> KeyList {
        KeyList { frame, target, kind, keys, unk_0c: 0x20, garbage: (0, 0), key_pad: None, more_raw: None }
    }
}

/// The size of a block's header, and of each of its keys.
const BLOCK: usize = 32;
/// The bytes between a key's frame and its value.
const KEY_PAD: usize = 12;

pub fn parse(b: &[u8]) -> Result<Vec<KeyList>> {
    let mut out = Vec::new();
    if b.is_empty() {
        return Ok(out);
    }
    let mut reader = Reader::new(b, "the key-list file");
    loop {
        let (frame, target, kind) = (reader.i32()?, reader.i32()?, reader.i32()?);
        let mut list = KeyList::new(frame, target, kind, Vec::new());
        list.unk_0c = reader.u32()?;
        let count = reader.u32()? as usize;
        let more = reader.u32()?;
        list.garbage = (reader.u32()?, reader.u32()?);
        for k in 0..count {
            let frame = reader.u32()? as i64;
            let pad = reader.take(KEY_PAD)?;
            if pad.iter().any(|&x| x != 0) {
                list.key_pad.get_or_insert_with(BTreeMap::new).insert(k, pad.to_vec());
            }
            list.keys.push((frame, reader.f32s()?));
        }
        let expect = (count as u64 + 1) * BLOCK as u64;
        if more != 0 && more as u64 != expect {
            list.more_raw = Some(more);
        }
        out.push(list);
        if more == 0 {
            break;
        }
    }
    if reader.remaining() != 0 {
        bail!("the key-list file has {} bytes after its last block", reader.remaining());
    }
    Ok(out)
}

pub fn build(lists: &[KeyList]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    for (i, list) in lists.iter().enumerate() {
        let last = i == lists.len() - 1;
        let more = match list.more_raw {
            Some(more) => more as u64,
            None if last => 0,
            None => (list.keys.len() as u64 + 1) * BLOCK as u64,
        };
        out.extend(list.frame.to_le_bytes());
        out.extend(list.target.to_le_bytes());
        out.extend(list.kind.to_le_bytes());
        out.extend(list.unk_0c.to_le_bytes());
        out.extend((list.keys.len() as u32).to_le_bytes());
        let Ok(more) = u32::try_from(more) else {
            bail!("key list {i} is {more} bytes, more than its 32-bit size field holds");
        };
        out.extend(more.to_le_bytes());
        out.extend(list.garbage.0.to_le_bytes());
        out.extend(list.garbage.1.to_le_bytes());
        for (k, (frame, value)) in list.keys.iter().enumerate() {
            let Ok(frame) = u32::try_from(*frame) else {
                bail!("key list {i}: key {k} is at frame {frame}, which a key list cannot hold (0 to 2^32 - 1)");
            };
            out.extend(frame.to_le_bytes());
            match list.key_pad.as_ref().and_then(|pads| pads.get(&k)) {
                Some(pad) => out.extend(pad),
                None => out.extend([0u8; KEY_PAD]),
            }
            pack_floats(value, &mut out)?;
        }
    }
    Ok(out)
}

pub fn parse_bbp(b: &[u8]) -> Result<Vec<[f64; 16]>> {
    if !b.len().is_multiple_of(64) {
        bail!("bbp size is not a multiple of 64");
    }
    (0..b.len()).step_by(64).map(|i| f32s_at(b, i)).collect()
}

pub fn build_bbp(matrices: &[[f64; 16]]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(64 * matrices.len());
    for m in matrices {
        pack_floats(m, &mut out)?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_lists_write_back_the_same() {
        let mut first = KeyList::new(3, 1, 0, vec![(0, [1.0, 0.0, 0.0, 0.0]), (10, [0.5, 0.5, 0.5, 0.5])]);
        first.garbage = (7, 9);
        let second = KeyList::new(4, 2, 2, vec![(0, [1.0, 2.0, 3.0, 0.0])]);
        let data = build(&[first, second]).unwrap();
        let again = parse(&data).unwrap();
        assert_eq!(again.len(), 2);
        assert_eq!(again[0].garbage, (7, 9));
        assert_eq!(build(&again).unwrap(), data);
        assert_eq!(parse(&[]).unwrap(), vec![]);
    }
}
