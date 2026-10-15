//! Key-list files (.mot motions, .wgt skin weights) and .bbp bind-pose matrix tables. A key-list
//! file is blocks of a 32-byte header (frame, target, type, 0x20, key count, the block size or 0
//! on the last, two garbage words) and 32-byte keys (frame, 12 bytes, four floats).

use std::collections::BTreeMap;

use crate::mds::{floats_at, i32_at, pack_floats, u32_at};
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

pub fn parse(b: &[u8]) -> Result<Vec<KeyList>> {
    let mut out = Vec::new();
    if b.is_empty() {
        return Ok(out);
    }
    let mut o = 0;
    loop {
        if o + 32 > b.len() {
            bail!("the data is shorter than the {} bytes read here", o + 32);
        }
        let mut list = KeyList::new(i32_at(b, o)?, i32_at(b, o + 4)?, i32_at(b, o + 8)?, Vec::new());
        list.unk_0c = u32_at(b, o + 12)?;
        let count = u32_at(b, o + 16)? as usize;
        let more = u32_at(b, o + 20)?;
        list.garbage = (u32_at(b, o + 24)?, u32_at(b, o + 28)?);
        let mut q = o + 32;
        for k in 0..count {
            if q + 32 > b.len() {
                bail!("the data is shorter than the {} bytes read here", q + 32);
            }
            let frame = u32_at(b, q)? as i64;
            let pad = &b[q + 4..q + 16];
            if pad.iter().any(|&x| x != 0) {
                list.key_pad.get_or_insert_with(BTreeMap::new).insert(k, pad.to_vec());
            }
            list.keys.push((frame, floats_at(b, q + 16)?));
            q += 32;
        }
        let expect = (count as u64 + 1) * 32;
        if more != 0 && more as u64 != expect {
            list.more_raw = Some(more);
        }
        out.push(list);
        o = q;
        if more == 0 {
            break;
        }
    }
    if o != b.len() {
        bail!("trailing data in key-list file");
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
            None => (list.keys.len() as u64 + 1) * 32,
        };
        out.extend(list.frame.to_le_bytes());
        out.extend(list.target.to_le_bytes());
        out.extend(list.kind.to_le_bytes());
        out.extend(list.unk_0c.to_le_bytes());
        out.extend((list.keys.len() as u32).to_le_bytes());
        let Ok(more) = u32::try_from(more) else {
            bail!("a value does not fit in an unsigned 32-bit int");
        };
        out.extend(more.to_le_bytes());
        out.extend(list.garbage.0.to_le_bytes());
        out.extend(list.garbage.1.to_le_bytes());
        for (k, (frame, value)) in list.keys.iter().enumerate() {
            let Ok(frame) = u32::try_from(*frame) else {
                bail!("a value does not fit in an unsigned 32-bit int");
            };
            out.extend(frame.to_le_bytes());
            match list.key_pad.as_ref().and_then(|pads| pads.get(&k)) {
                Some(pad) => out.extend(pad),
                None => out.extend([0u8; 12]),
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
    (0..b.len()).step_by(64).map(|i| floats_at(b, i)).collect()
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
