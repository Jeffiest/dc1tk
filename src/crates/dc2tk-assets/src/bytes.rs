//! Reading the binary formats: little-endian values at an offset (`u32_at` and the like, for
//! files read by random access) and a `Reader` cursor (for records read front to back). Every read
//! is bounds-checked with checked arithmetic, so an offset or a count a modded file gets wrong is an
//! error saying what was read where, never a panic or an overflow.

use crate::numeric;
use crate::{Error, Result};

/// The error of a read past the end: what was read, where, and where the data ends.
fn past_end(data: &[u8], at: usize, n: usize, what: &str) -> Error {
    Error::invalid(format!("{what} ends at byte 0x{:x}, inside the {n}-byte read at 0x{at:x}", data.len()))
}

/// data[at..at + n], or an error saying where the data ends.
pub fn bytes_at(data: &[u8], at: usize, n: usize) -> Result<&[u8]> {
    at.checked_add(n).and_then(|end| data.get(at..end)).ok_or_else(|| past_end(data, at, n, "the data"))
}

/// The N bytes at `at`.
pub fn array_at<const N: usize>(data: &[u8], at: usize) -> Result<[u8; N]> {
    Ok(bytes_at(data, at, N)?.try_into().expect("bytes_at gives N bytes"))
}

pub fn u16_at(data: &[u8], at: usize) -> Result<u16> {
    array_at(data, at).map(u16::from_le_bytes)
}

pub fn u32_at(data: &[u8], at: usize) -> Result<u32> {
    array_at(data, at).map(u32::from_le_bytes)
}

pub fn i32_at(data: &[u8], at: usize) -> Result<i32> {
    array_at(data, at).map(i32::from_le_bytes)
}

pub fn u64_at(data: &[u8], at: usize) -> Result<u64> {
    array_at(data, at).map(u64::from_le_bytes)
}

/// N float32s in a row, as doubles (numeric::f32_value).
pub fn f32s_at<const N: usize>(data: &[u8], at: usize) -> Result<[f64; N]> {
    let raw = bytes_at(data, at, 4 * N)?;
    Ok(std::array::from_fn(|i| numeric::f32_value(u32::from_le_bytes(raw[4 * i..4 * i + 4].try_into().unwrap()))))
}

/// A big-endian u16 (MIDI files).
pub fn be16_at(data: &[u8], at: usize) -> Result<u16> {
    array_at(data, at).map(u16::from_be_bytes)
}

/// A big-endian u32 (MIDI files).
pub fn be32_at(data: &[u8], at: usize) -> Result<u32> {
    array_at(data, at).map(u32::from_be_bytes)
}

/// data[start..end] with both ends clamped to the data: an end past it (or before start) makes the
/// slice shorter or empty. For the parts of a file a reader keeps as they are, however short.
pub fn clamped(data: &[u8], start: usize, end: usize) -> &[u8] {
    let start = start.min(data.len());
    &data[start..end.clamp(start, data.len())]
}

/// A cursor over a file read front to back; `what` names the file (or record) in its errors.
#[derive(Clone, Debug)]
pub(crate) struct Reader<'a> {
    data: &'a [u8],
    at: usize,
    what: &'static str,
}

impl<'a> Reader<'a> {
    /// A reader at the start of `data`.
    pub(crate) fn new(data: &'a [u8], what: &'static str) -> Reader<'a> {
        Reader { data, at: 0, what }
    }

    /// A reader at offset `at` of `data`.
    pub(crate) fn at(data: &'a [u8], at: usize, what: &'static str) -> Reader<'a> {
        Reader { data, at, what }
    }

    /// The bytes left after the position.
    pub(crate) fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.at)
    }

    /// The next n bytes.
    pub(crate) fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let data = self.data;
        let found = self.at.checked_add(n).and_then(|end| data.get(self.at..end));
        let bytes = found.ok_or_else(|| past_end(data, self.at, n, self.what))?;
        self.at += n;
        Ok(bytes)
    }

    pub(crate) fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        Ok(self.take(N)?.try_into().expect("take gives N bytes"))
    }

    pub(crate) fn u8(&mut self) -> Result<u8> {
        Ok(self.array::<1>()?[0])
    }

    pub(crate) fn i8(&mut self) -> Result<i8> {
        Ok(self.u8()? as i8)
    }

    pub(crate) fn u16(&mut self) -> Result<u16> {
        self.array().map(u16::from_le_bytes)
    }

    pub(crate) fn i16(&mut self) -> Result<i16> {
        self.array().map(i16::from_le_bytes)
    }

    pub(crate) fn u32(&mut self) -> Result<u32> {
        self.array().map(u32::from_le_bytes)
    }

    pub(crate) fn i32(&mut self) -> Result<i32> {
        self.array().map(i32::from_le_bytes)
    }

    /// A float32 as a double, a signalling NaN made quiet (numeric::f32_value).
    pub(crate) fn f32(&mut self) -> Result<f64> {
        self.u32().map(numeric::f32_value)
    }

    /// N float32s in a row, as doubles.
    pub(crate) fn f32s<const N: usize>(&mut self) -> Result<[f64; N]> {
        let at = self.at;
        let values = f32s_at(self.data, at).map_err(|_| past_end(self.data, at, 4 * N, self.what))?;
        self.at += 4 * N;
        Ok(values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_past_the_end_are_errors_that_say_where() {
        let data = [1, 0, 0, 0, 0, 0, 0x80, 0x3F];
        assert_eq!(u32_at(&data, 0).unwrap(), 1);
        assert_eq!(f32s_at::<1>(&data, 4).unwrap(), [1.0]);
        assert_eq!(u16_at(&data, 6).unwrap(), 0x3F80);
        let error = u32_at(&data, 6).unwrap_err().to_string();
        assert_eq!(error, "the data ends at byte 0x8, inside the 4-byte read at 0x6");
        // an offset near the top of the address space does not wrap around
        assert!(u32_at(&data, usize::MAX - 1).is_err());
        assert!(bytes_at(&data, usize::MAX, 2).is_err());
        assert_eq!(clamped(&data, 6, 100), &[0x80, 0x3F]);
        assert_eq!(clamped(&data, 9, 4), &[] as &[u8]);
    }

    #[test]
    fn a_reader_moves_on_by_what_it_reads() {
        let data = [7, 0xFE, 0xFF, 2, 0, 0, 0, 0, 0, 0x80, 0x3F, 0, 0, 0, 0x40];
        let mut reader = Reader::new(&data, "the record");
        assert_eq!((reader.u8().unwrap(), reader.i16().unwrap(), reader.u32().unwrap()), (7, -2, 2));
        assert_eq!(reader.f32s::<2>().unwrap(), [1.0, 2.0]);
        assert_eq!(reader.remaining(), 0);
        let error = reader.u8().unwrap_err().to_string();
        assert_eq!(error, "the record ends at byte 0xf, inside the 1-byte read at 0xf");
        assert_eq!(Reader::at(&data, 13, "x").take(2).unwrap(), &[0, 0x40]);
        assert!(Reader::at(&data, usize::MAX, "x").take(2).is_err());
    }
}
