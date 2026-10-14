//! Texture archives (IM3, IM2 and IMG) of TIM2 pictures, to and from RGBA and PNG.
//!
//! An Image keeps its pixels in the engine's terms: paletted pictures keep `indices` (one byte per
//! pixel, raster order) and `palette` (index order, the CSM1 swizzle undone); true-colour ones keep
//! `pixels` as RGBA. Alpha is the PS2's, where 0x80 is opaque; only the PNG side rescales it.

use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

use crate::{bail, py, Error, Result};

pub const IM3_ENTRY: usize = 0x40;
pub const IM1_ENTRY: usize = 0x30;

/// The pixel formats of a TIM2 picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Ct16,
    Ct24,
    Ct32,
    T4,
    T8,
}

impl Format {
    pub fn from_type(image_type: u8) -> Option<Format> {
        Some(match image_type {
            1 => Format::Ct16,
            2 => Format::Ct24,
            3 => Format::Ct32,
            4 => Format::T4,
            5 => Format::T8,
            _ => return None,
        })
    }

    /// The TIM2 image type.
    pub fn image_type(self) -> u8 {
        match self {
            Format::Ct16 => 1,
            Format::Ct24 => 2,
            Format::Ct32 => 3,
            Format::T4 => 4,
            Format::T8 => 5,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Format::Ct16 => "PSMCT16",
            Format::Ct24 => "PSMCT24",
            Format::Ct32 => "PSMCT32",
            Format::T4 => "PSMT4",
            Format::T8 => "PSMT8",
        }
    }

    pub fn bpp(self) -> usize {
        match self {
            Format::Ct16 => 16,
            Format::Ct24 => 24,
            Format::Ct32 => 32,
            Format::T4 => 4,
            Format::T8 => 8,
        }
    }

    pub fn paletted(self) -> bool {
        matches!(self, Format::T4 | Format::T8)
    }

    /// The PSM field of TEX0.
    fn psm(self) -> u64 {
        match self {
            Format::Ct32 => 0,
            Format::Ct24 => 1,
            Format::Ct16 => 2,
            Format::T8 => 0x13,
            Format::T4 => 0x14,
        }
    }
}

/// BlockConv32to8's lut_1246: position, inside a 64-byte group of a 16x16 8-bit block, of each byte
/// of the matching 32-bit column; odd groups use the second half.
const LUT: [u8; 128] = [
    0x00, 0x24, 0x08, 0x2c, 0x01, 0x25, 0x09, 0x2d, 0x02, 0x26, 0x0a, 0x2e, 0x03, 0x27, 0x0b, 0x2f, 0x04, 0x20, 0x0c,
    0x28, 0x05, 0x21, 0x0d, 0x29, 0x06, 0x22, 0x0e, 0x2a, 0x07, 0x23, 0x0f, 0x2b, 0x10, 0x34, 0x18, 0x3c, 0x11, 0x35,
    0x19, 0x3d, 0x12, 0x36, 0x1a, 0x3e, 0x13, 0x37, 0x1b, 0x3f, 0x14, 0x30, 0x1c, 0x38, 0x15, 0x31, 0x1d, 0x39, 0x16,
    0x32, 0x1e, 0x3a, 0x17, 0x33, 0x1f, 0x3b, 0x04, 0x20, 0x0c, 0x28, 0x05, 0x21, 0x0d, 0x29, 0x06, 0x22, 0x0e, 0x2a,
    0x07, 0x23, 0x0f, 0x2b, 0x00, 0x24, 0x08, 0x2c, 0x01, 0x25, 0x09, 0x2d, 0x02, 0x26, 0x0a, 0x2e, 0x03, 0x27, 0x0b,
    0x2f, 0x14, 0x30, 0x1c, 0x38, 0x15, 0x31, 0x1d, 0x39, 0x16, 0x32, 0x1e, 0x3a, 0x17, 0x33, 0x1f, 0x3b, 0x10, 0x34,
    0x18, 0x3c, 0x11, 0x35, 0x19, 0x3d, 0x12, 0x36, 0x1a, 0x3e, 0x13, 0x37, 0x1b, 0x3f,
];

/// Byte of the 8x8 CT32 block (256 bytes, 32 per row) holding each pixel of the 16x16 PSMT8 block.
fn block8() -> &'static [usize; 256] {
    static TABLE: OnceLock<[usize; 256]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut inverse = [0usize; 256];
        for (i, slot) in (0..256).enumerate() {
            let (group, t) = (slot / 64, slot % 64);
            let j = group * 64 + LUT[(group & 1) * 64 + t] as usize;
            inverse[j] = i;
        }
        inverse
    })
}

/// File byte offset of every pixel (raster order) of a w x h PSMT8 level stored swizzled, as the
/// (w/2) x (h/2) PSMCT32 image the engine uploads. Offsets can point past the stored data when h is
/// not a multiple of 16.
pub fn swizzle8_offsets(w: usize, h: usize) -> Result<Vec<usize>> {
    if !w.is_multiple_of(16) {
        bail!("swizzled 8-bit width {w} is not a multiple of 16");
    }
    let table = block8();
    let row_bytes = 2 * w;
    let row_parts: Vec<Vec<(usize, usize)>> = (0..16)
        .map(|iy| {
            (0..w)
                .map(|x| {
                    let (bx, ix) = (x / 16, x % 16);
                    let i = table[iy * 16 + ix];
                    (i >> 5, bx * 32 + (i & 31))
                })
                .collect()
        })
        .collect();
    let mut out = Vec::with_capacity(w * h);
    for y in 0..h {
        let base_row = (y / 16) * 8;
        out.extend(row_parts[y % 16].iter().map(|&(r, c)| (base_row + r) * row_bytes + c));
    }
    Ok(out)
}

fn csm1(i: usize) -> usize {
    (i & 0xE7) | ((i & 8) << 1) | ((i & 16) >> 1)
}

/// PS2 alpha (0..0x80) to PNG alpha (0..255).
fn a2p(a: u8) -> u8 {
    ((a as u32 * 255 + 64) / 128).min(255) as u8
}

/// PNG alpha to PS2 alpha.
fn p2a(p: u8) -> u8 {
    ((p as u32 * 128 + 127) / 255) as u8
}

/// How alpha maps to PNG for a picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlphaMode {
    /// Every alpha <= 0x80: PNG alpha = a * 255 / 128.
    Ps2,
    /// Some alpha > 0x80: PNG alpha = a.
    Raw,
    /// Every alpha 0: written opaque.
    Zero,
    /// PSMCT24, no alpha stored.
    Opaque,
}

impl AlphaMode {
    pub fn name(self) -> &'static str {
        match self {
            AlphaMode::Ps2 => "ps2",
            AlphaMode::Raw => "raw",
            AlphaMode::Zero => "zero",
            AlphaMode::Opaque => "opaque",
        }
    }

    fn to_png(self, a: u8) -> u8 {
        if self == AlphaMode::Raw {
            a
        } else {
            a2p(a)
        }
    }

    fn to_ps2(self, a: u8) -> u8 {
        if self == AlphaMode::Raw {
            a
        } else {
            p2a(a)
        }
    }
}

/// One picture of an archive.
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    /// The name it is registered under (before the first NUL), read as Latin-1.
    pub name: String,
    /// Entry number in the archive; None for a new picture.
    pub index: Option<usize>,
    pub format: Format,
    pub width: usize,
    pub height: usize,
    /// Paletted: one palette index per pixel of the base level.
    pub indices: Option<Vec<u8>>,
    /// Paletted: (r, g, b, a) in index order, PS2 alpha.
    pub palette: Option<Vec<[u8; 4]>>,
    /// True colour: RGBA of the base level, PS2 alpha.
    pub pixels: Option<Vec<u8>>,
    /// Further mip levels, each like indices or pixels.
    pub mips: Vec<Vec<u8>>,
    /// 8-bit pixels are stored in PSMCT32 order.
    pub swizzled: bool,
    /// IM3: header only, no pixels or palette stored.
    pub no_image: bool,
    /// IM3 texture block group.
    pub block: i32,
    /// IM3 GS CLAMP register; None for IM2 and IMG pictures.
    pub clamp: Option<u64>,
    /// The picture's original TIM2 bytes.
    pub tim2: Option<Vec<u8>>,
    /// The original 32-byte name field.
    pub raw_name: Option<Vec<u8>>,
}

impl Image {
    pub fn new(name: &str, format: Format, width: usize, height: usize) -> Image {
        Image {
            name: name.to_string(),
            index: None,
            format,
            width,
            height,
            indices: None,
            palette: None,
            pixels: None,
            mips: Vec::new(),
            swizzled: false,
            no_image: false,
            block: 0,
            clamp: Some(5),
            tim2: None,
            raw_name: None,
        }
    }

    pub fn paletted(&self) -> bool {
        self.format.paletted()
    }

    /// The base level as RGBA (PS2 alpha).
    pub fn rgba(&self) -> Result<Option<Vec<u8>>> {
        if self.no_image {
            return Ok(None);
        }
        if !self.paletted() {
            return Ok(self.pixels.clone());
        }
        let palette = self.palette.as_deref().unwrap_or_default();
        let indices = self.indices.as_deref().unwrap_or_default();
        Ok(Some(lookup(indices, palette, [0, 0, 0, 0])?))
    }
}

/// The colour of every index: the palette, padded to 256 entries with `fill`.
fn lookup(indices: &[u8], palette: &[[u8; 4]], fill: [u8; 4]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(indices.len() * 4);
    for &i in indices {
        let colour = match palette.get(i as usize) {
            Some(colour) => *colour,
            None if palette.len() <= i as usize && (i as usize) < 256 => fill,
            None => bail!("list index out of range"),
        };
        out.extend(colour);
    }
    Ok(out)
}

fn u16_at(data: &[u8], at: usize) -> Result<u16> {
    match data.get(at..at + 2) {
        Some(b) => Ok(u16::from_le_bytes([b[0], b[1]])),
        None => bail!("unpack_from requires a buffer of at least {} bytes", at + 2),
    }
}

fn u32_at(data: &[u8], at: usize) -> Result<u32> {
    match data.get(at..at + 4) {
        Some(b) => Ok(u32::from_le_bytes(b.try_into().unwrap())),
        None => bail!("unpack_from requires a buffer of at least {} bytes", at + 4),
    }
}

fn i32_at(data: &[u8], at: usize) -> Result<i32> {
    Ok(u32_at(data, at)? as i32)
}

fn u64_at(data: &[u8], at: usize) -> Result<u64> {
    match data.get(at..at + 8) {
        Some(b) => Ok(u64::from_le_bytes(b.try_into().unwrap())),
        None => bail!("unpack_from requires a buffer of at least {} bytes", at + 8),
    }
}

/// data[start:end] with Python's clamping of a slice to the data.
fn slice(data: &[u8], start: usize, end: usize) -> &[u8] {
    let start = start.min(data.len());
    &data[start..end.clamp(start, data.len())]
}

/// struct.pack_into: the bytes at `at`, which must fit.
fn put(buffer: &mut [u8], at: usize, bytes: &[u8]) -> Result<()> {
    match buffer.get_mut(at..at + bytes.len()) {
        Some(target) => {
            target.copy_from_slice(bytes);
            Ok(())
        }
        None => bail!("pack_into requires a buffer of at least {} bytes", at + bytes.len()),
    }
}

/// The kinds of archive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Im3,
    Im2,
    Img,
}

#[derive(Clone, Debug)]
struct Entry {
    index: usize,
    pos: usize,
    raw: Vec<u8>,
    name: Vec<u8>,
    offset: i32,
    swizzled: bool,
    block: i32,
    no_image: bool,
    size: Option<i32>,
    clamp: Option<u64>,
    script: bool,
}

fn parse_archive(data: &[u8]) -> Result<(Option<Kind>, Vec<Entry>)> {
    if data.len() < 16 || &data[..2] != b"IM" {
        if data.is_empty() {
            return Ok((None, Vec::new()));
        }
        bail!("not an IMG/IM2/IM3 archive");
    }
    let (kind, count, stride) = match &data[..3] {
        b"IM3" => (Kind::Im3, u32_at(data, 8)?, IM3_ENTRY),
        b"IM2" => (Kind::Im2, u32_at(data, 4)?, IM1_ENTRY),
        b"IMG" => (Kind::Img, u32_at(data, 4)?, IM1_ENTRY),
        _ => bail!("unknown archive tag {}", bytes_repr(&data[..4])),
    };
    let mut entries = Vec::new();
    for i in 0..count as usize {
        let pos = 0x10 + stride * i;
        let raw = slice(data, pos, pos + stride).to_vec();
        let field = slice(&raw, 0, 0x20);
        let name = field.split(|&b| b == 0).next().unwrap_or_default().to_vec();
        let mut entry = Entry {
            index: i,
            pos,
            name,
            offset: 0,
            swizzled: false,
            block: 0,
            no_image: false,
            size: None,
            clamp: None,
            script: false,
            raw: Vec::new(),
        };
        if kind == Kind::Im3 {
            if raw.len() < 0x40 {
                bail!("unpack_from requires a buffer of at least 64 bytes for unpacking 32 bytes at offset 32 (actual buffer size is {})", raw.len());
            }
            entry.script = entry.name.first() == Some(&b'#');
            entry.offset = i32_at(&raw, 0x24)?;
            entry.swizzled = i32_at(&raw, 0x28)? != 0;
            entry.block = i32_at(&raw, 0x2C)?;
            entry.no_image = u16_at(&raw, 0x30)? != 0;
            entry.size = Some(i32_at(&raw, 0x34)?);
            entry.clamp = Some(u64_at(&raw, 0x38)?);
        } else {
            entry.offset = i32_at(&raw, 0x20)?;
            entry.swizzled = kind == Kind::Im2;
        }
        entry.raw = raw;
        entries.push(entry);
    }
    Ok((Some(kind), entries))
}

/// Python's repr() of bytes, for messages.
fn bytes_repr(data: &[u8]) -> String {
    let mut out = String::from("b'");
    for &b in data {
        match b {
            b'\\' => out.push_str("\\\\"),
            b'\'' => out.push_str("\\'"),
            b'\t' => out.push_str("\\t"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            0x20..=0x7E => out.push(b as char),
            _ => out.push_str(&format!("\\x{b:02x}")),
        }
    }
    out.push('\'');
    out
}

/// Where a TIM2 starts, as a position in the data (Python indexes a negative offset from the end).
fn tim2_start(data: &[u8], offset: i32) -> Result<usize> {
    if offset < 0 || offset as usize > data.len() {
        bail!("no TIM2 at 0x{:x}", offset);
    }
    Ok(offset as usize)
}

/// Length of the TIM2 at off: file header, picture header, pixels and palette (only the header
/// when the IM3 entry says no_image).
fn tim2_extent(data: &[u8], offset: i32, no_image: bool) -> Result<usize> {
    let off = tim2_start(data, offset)?;
    if slice(data, off, off + 4) != b"TIM2" {
        bail!("no TIM2 at 0x{:x}", off);
    }
    let pictures = u16_at(data, off + 6)?;
    let q = off + 16;
    if no_image {
        return Ok(16 + u16_at(data, q + 12)? as usize);
    }
    let mut end = q;
    for _ in 0..pictures.max(1) {
        let clut_size = u32_at(data, end + 4)? as usize;
        let image_size = u32_at(data, end + 8)? as usize;
        let header_size = u16_at(data, end + 12)? as usize;
        end += header_size + image_size + clut_size;
    }
    Ok(end - off)
}

/// The picture header fields of a TIM2 (at 16).
#[derive(Clone, Copy, Debug)]
struct Header {
    total: u32,
    clut_size: u32,
    image_size: u32,
    header_size: u16,
    colours: u16,
    pf: u8,
    mipmaps: u8,
    clut_type: u8,
    image_type: u8,
    width: u16,
    height: u16,
}

fn header(tim: &[u8]) -> Result<Header> {
    if tim.len() < 40 {
        bail!("unpack_from requires a buffer of at least 40 bytes for unpacking 24 bytes at offset 16 (actual buffer size is {})", tim.len());
    }
    Ok(Header {
        total: u32_at(tim, 16)?,
        clut_size: u32_at(tim, 20)?,
        image_size: u32_at(tim, 24)?,
        header_size: u16_at(tim, 28)?,
        colours: u16_at(tim, 30)?,
        pf: tim[32],
        mipmaps: tim[33],
        clut_type: tim[34],
        image_type: tim[35],
        width: u16_at(tim, 36)?,
        height: u16_at(tim, 38)?,
    })
}

fn level_sizes(tim: &[u8], h: &Header) -> Result<Vec<usize>> {
    if h.mipmaps > 1 {
        (0..h.mipmaps as usize).map(|k| Ok(u32_at(tim, 16 + 0x40 + 4 * k)? as usize)).collect()
    } else {
        Ok(vec![h.image_size as usize])
    }
}

fn decode_tim2(tim: &[u8], swizzled: bool, no_image: bool) -> Result<Image> {
    let h = header(tim)?;
    let Some(format) = Format::from_type(h.image_type) else {
        bail!("unsupported TIM2 image type {}", h.image_type);
    };
    let mut image = Image::new("", format, h.width as usize, h.height as usize);
    image.swizzled = swizzled;
    image.no_image = no_image;
    image.tim2 = Some(tim.to_vec());
    if no_image {
        return Ok(image);
    }
    let q = 16;
    let mut pos = q + h.header_size as usize;
    let mut levels = Vec::new();
    for (k, size) in level_sizes(tim, &h)?.into_iter().enumerate() {
        let (lw, lh) = ((h.width as usize >> k).max(1), (h.height as usize >> k).max(1));
        levels.push(decode_level(slice(tim, pos, pos + size), format, lw, lh, swizzled)?);
        pos += size;
    }
    let mut levels = levels.into_iter();
    if format.paletted() {
        let start = q + h.header_size as usize + h.image_size as usize;
        let clut = slice(tim, start, start + h.clut_size as usize);
        image.palette = Some(decode_clut(clut, h.clut_type, format)?);
        image.indices = levels.next();
    } else {
        image.pixels = levels.next();
    }
    image.mips = levels.collect();
    Ok(image)
}

fn decode_level(raw: &[u8], format: Format, w: usize, h: usize, swizzled: bool) -> Result<Vec<u8>> {
    let n = w * h;
    Ok(match format {
        Format::T8 if swizzled => {
            let offsets = swizzle8_offsets(w, h)?;
            offsets.iter().map(|&o| raw.get(o).copied().unwrap_or(0)).collect()
        }
        Format::T8 => {
            let mut out = raw[..n.min(raw.len())].to_vec();
            out.resize(n, 0);
            out
        }
        Format::T4 => {
            let mut out: Vec<u8> = raw.iter().flat_map(|&b| [b & 15, b >> 4]).collect();
            out.resize(n.max(out.len()), 0);
            out.truncate(n);
            out
        }
        Format::Ct32 => raw[..(4 * n).min(raw.len())].to_vec(),
        Format::Ct24 => {
            if n > 0 && raw.len() < 3 * n {
                bail!("attempt to assign bytes of size {} to extended slice of size {}", raw.len().div_ceil(3), n);
            }
            raw[..3 * n].chunks(3).flat_map(|c| [c[0], c[1], c[2], 0x80]).collect()
        }
        Format::Ct16 => {
            let used = &raw[..(2 * n).min(raw.len())];
            if !used.len().is_multiple_of(2) {
                bail!("iterative unpacking requires a buffer of a multiple of 2 bytes");
            }
            let mut out = vec![0u8; 4 * n];
            for (i, pair) in used.chunks(2).enumerate() {
                let v = u16::from_le_bytes([pair[0], pair[1]]);
                let (r, g, b) = ((v & 31) as u8, ((v >> 5) & 31) as u8, ((v >> 10) & 31) as u8);
                let a = if v & 0x8000 != 0 { 0x80 } else { 0 };
                out[4 * i..4 * i + 4].copy_from_slice(&[
                    (r << 3) | (r >> 2),
                    (g << 3) | (g >> 2),
                    (b << 3) | (b >> 2),
                    a,
                ]);
            }
            out
        }
    })
}

/// The inverse of decode_level. base is the level's original stored bytes; bytes no pixel maps to
/// are kept from it.
fn encode_level(
    data: &[u8],
    format: Format,
    w: usize,
    h: usize,
    swizzled: bool,
    base: Option<&[u8]>,
) -> Result<Vec<u8>> {
    let n = w * h;
    let size = (n * format.bpp()).div_ceil(8);
    let zeros;
    let base = match base {
        Some(base) if base.len() == size => base,
        _ => {
            zeros = vec![0u8; size];
            &zeros[..]
        }
    };
    Ok(match format {
        Format::T8 if swizzled => {
            let offsets = swizzle8_offsets(w, h)?;
            let mut select: Vec<usize> = (n..n + size).collect();
            for (p, &o) in offsets.iter().enumerate() {
                if o < size {
                    select[o] = p;
                }
            }
            let source_at = |i: usize| -> Result<u8> {
                if i < data.len() {
                    Ok(data[i])
                } else {
                    match base.get(i - data.len()) {
                        Some(&b) => Ok(b),
                        None => bail!("index out of range"),
                    }
                }
            };
            select.into_iter().map(source_at).collect::<Result<Vec<u8>>>()?
        }
        Format::T8 => data[..n.min(data.len())].to_vec(),
        Format::T4 => {
            let lo: Vec<u8> = data.iter().step_by(2).copied().collect();
            let mut hi: Vec<u8> = data.iter().skip(1).step_by(2).copied().collect();
            if hi.len() < lo.len() {
                let Some(last) = base.last() else {
                    bail!("index out of range");
                };
                hi.push(last >> 4);
            }
            lo.iter().zip(&hi).map(|(a, b)| (a & 15) | ((b & 15) << 4)).collect()
        }
        Format::Ct32 => data[..(4 * n).min(data.len())].to_vec(),
        Format::Ct24 => {
            for k in 0..3 {
                let got = data.len().saturating_sub(k).div_ceil(4);
                if got != n {
                    bail!("attempt to assign bytes of size {got} to extended slice of size {n}");
                }
            }
            (0..n).flat_map(|i| [data[4 * i], data[4 * i + 1], data[4 * i + 2]]).collect()
        }
        Format::Ct16 => {
            let mut out = Vec::with_capacity(2 * n);
            for i in 0..n {
                let Some(p) = data.get(4 * i..4 * i + 4) else {
                    bail!("not enough values to unpack (expected 4, got {})", data.len().saturating_sub(4 * i).min(4));
                };
                let v = (p[0] as u16 >> 3)
                    | ((p[1] as u16 >> 3) << 5)
                    | ((p[2] as u16 >> 3) << 10)
                    | if p[3] >= 0x40 { 0x8000 } else { 0 };
                out.extend(v.to_le_bytes());
            }
            out
        }
    })
}

fn decode_clut(clut: &[u8], clut_type: u8, format: Format) -> Result<Vec<[u8; 4]>> {
    if clut_type & 0x3F != 3 {
        bail!("unsupported CLUT type 0x{:x}", clut_type);
    }
    let colours: Vec<[u8; 4]> = clut.as_chunks::<4>().0.iter().map(|c| [c[0], c[1], c[2], c[3]]).collect();
    Ok(csm1_order(colours, format))
}

/// The CSM1 order (the engine uploads an 8-bit CLUT as a 16x16 PSMCT32 image: colour i sits at
/// file position i with bits 3 and 4 swapped); its own inverse.
fn csm1_order(mut colours: Vec<[u8; 4]>, format: Format) -> Vec<[u8; 4]> {
    if format == Format::T8 && colours.len() >= 256 {
        let first: Vec<[u8; 4]> = (0..256).map(|i| colours[csm1(i)]).collect();
        colours[..256].copy_from_slice(&first);
    }
    colours
}

fn encode_clut(palette: &[[u8; 4]], format: Format, base: Option<&[u8]>) -> Vec<u8> {
    let mut raw: Vec<u8> = csm1_order(palette.to_vec(), format).concat();
    if let Some(base) = base {
        if base.len() > raw.len() {
            raw.extend_from_slice(&base[raw.len()..]);
        }
    }
    raw
}

fn tex0_fields(w: usize, h: usize) -> (u64, u64, u64) {
    let tw = py::bit_length(w as i64 - 1) as u64;
    let th = py::bit_length(h as i64 - 1) as u64;
    let tbw = (w as u64).div_ceil(64).max(1);
    (tbw, tw, th)
}

fn levels_pixels(w: usize, h: usize, levels: usize) -> usize {
    (0..levels).map(|k| (w >> k).max(1) * (h >> k).max(1)).sum()
}

fn checked<T: TryFrom<usize>>(value: usize, what: &str) -> Result<T> {
    T::try_from(value).map_err(|_| Error::invalid(format!("argument out of range for format '{what}'")))
}

/// TIM2 file header and picture header for an image; old is the original TIM2 (fields not derived
/// from the picture are kept from it).
fn build_header(image: &Image, old: Option<&[u8]>) -> Result<(Vec<u8>, Vec<usize>)> {
    let (format, w, h) = (image.format, image.width, image.height);
    let bpp = format.bpp();
    let levels = 1 + image.mips.len();
    let sizes: Vec<usize> = (0..levels).map(|k| ((w >> k).max(1) * (h >> k).max(1) * bpp).div_ceil(8)).collect();
    let image_size: usize = sizes.iter().sum();
    let (colours, clut_size, mut clut_type) = match (&image.palette, image.paletted()) {
        (Some(palette), true) => (palette.len(), 4 * palette.len(), 3u8),
        (None, true) => bail!("object of type 'NoneType' has no len()"),
        _ => (0, 0, 0),
    };
    let (file_header, mut tex0, tex1, regs, tclut, pf, rule, old_total);
    if let Some(old) = old {
        let o = header(old)?;
        file_header = old[..16].to_vec();
        tex0 = u64_at(old, 16 + 0x18)?;
        tex1 = u64_at(old, 16 + 0x20)?;
        regs = u32_at(old, 16 + 0x28)?;
        tclut = u32_at(old, 16 + 0x2C)?;
        pf = o.pf;
        if !old.is_empty() && o.clut_type != 0 && image.paletted() {
            clut_type = o.clut_type;
        }
        let old_pixels = 4 * levels_pixels(o.width as usize, o.height as usize, (o.mipmaps as usize).max(1));
        let header_size = o.header_size as usize;
        rule = if o.total as usize == header_size + o.image_size as usize + o.clut_size as usize {
            "exact"
        } else if o.total as usize == header_size + old_pixels {
            "rgba32"
        } else {
            "keep"
        };
        old_total = o.total as usize;
        if tex0 != 0 {
            // the converter fills PSM, TW and TH (TBW left 0); the engine builds its own TEX0
            let (tbw, tw, th) = tex0_fields(w, h);
            if (tex0 >> 14) & 0x3F != 0 {
                tex0 = (tex0 & !(0x3F << 14)) | (tbw << 14);
            }
            tex0 &= !((0x3F << 20) | (0xF << 26) | (0xF << 30));
            tex0 |= (tw << 26) | (th << 30);
            tex0 |= format.psm() << 20;
        }
    } else {
        file_header = b"TIM2\x03\x00\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00".to_vec();
        (tex0, tex1, regs, tclut) = (0, 0xFFFFFF8F00000000, 0, 0);
        (pf, rule, old_total) = (0, "exact", 0);
    }
    let header_size = if levels == 1 { 0x30 } else { 0x30 + 0x10 + (4 * levels).div_ceil(16) * 16 };
    let total = match rule {
        "exact" => header_size + image_size + clut_size,
        "rgba32" => header_size + 4 * levels_pixels(w, h, levels),
        _ => old_total,
    };
    let mut out = file_header;
    out.extend(checked::<u32>(total, "I")?.to_le_bytes());
    out.extend(checked::<u32>(clut_size, "I")?.to_le_bytes());
    out.extend(checked::<u32>(image_size, "I")?.to_le_bytes());
    out.extend(checked::<u16>(header_size, "H")?.to_le_bytes());
    out.extend(checked::<u16>(colours, "H")?.to_le_bytes());
    out.push(pf);
    out.push(checked::<u8>(levels, "B")?);
    out.push(clut_type);
    out.push(format.image_type());
    out.extend(checked::<u16>(w, "H")?.to_le_bytes());
    out.extend(checked::<u16>(h, "H")?.to_le_bytes());
    out.extend(tex0.to_le_bytes());
    out.extend(tex1.to_le_bytes());
    out.extend(regs.to_le_bytes());
    out.extend(tclut.to_le_bytes());
    if levels > 1 {
        let mut mip = vec![0u8; header_size - 0x30];
        if let Some(old) = old.filter(|old| old.len() >= 16 + 0x40) {
            let old_header = u16_at(old, 16 + 12)? as usize;
            if old_header > 0x30 {
                let keep = slice(old, 16 + 0x30, 16 + old_header);
                let keep = &keep[..keep.len().min(mip.len())];
                mip[..keep.len()].copy_from_slice(keep);
            }
        }
        for (k, size) in sizes.iter().enumerate() {
            mip[0x10 + 4 * k..0x14 + 4 * k].copy_from_slice(&checked::<u32>(*size, "I")?.to_le_bytes());
        }
        out.extend(mip);
    }
    Ok((out, sizes))
}

fn encode_tim2(image: &Image) -> Result<Vec<u8>> {
    let old = image.tim2.as_deref();
    if image.no_image {
        return match old {
            Some(old) => Ok(old.to_vec()),
            None => bail!("no_image picture without a header"),
        };
    }
    let mut reused = None;
    if let Some(old) = old {
        let o = header(old)?;
        let same = o.image_type == image.format.image_type()
            && (o.width as usize, o.height as usize) == (image.width, image.height)
            && o.mipmaps as usize == 1 + image.mips.len()
            && (!image.paletted() || image.palette.as_ref().is_some_and(|p| p.len() == o.colours as usize));
        if same {
            let header_end = 16 + o.header_size as usize;
            let sizes = if o.mipmaps > 1 {
                (0..o.mipmaps as usize).map(|k| Ok(u32_at(old, 16 + 0x40 + 4 * k)? as usize)).collect::<Result<_>>()?
            } else {
                vec![o.image_size as usize]
            };
            let clut_start = header_end + o.image_size as usize;
            let clut_base = slice(old, clut_start, clut_start + o.clut_size as usize);
            reused = Some((slice(old, 0, header_end).to_vec(), sizes, clut_base));
        }
    }
    let (header, sizes, clut_base) = match reused {
        Some((header, sizes, clut_base)) => (header, sizes, Some(clut_base)),
        None => {
            let (header, sizes) = build_header(image, old)?;
            (header, sizes, None)
        }
    };
    let reuse = clut_base.is_some();
    let first = if image.paletted() { &image.indices } else { &image.pixels };
    let Some(first) = first else {
        bail!("the picture has no pixels");
    };
    let mut out = header;
    let mut pos = out.len();
    for (k, (level, &size)) in std::iter::once(first).chain(&image.mips).zip(&sizes).enumerate() {
        let (lw, lh) = ((image.width >> k).max(1), (image.height >> k).max(1));
        let base = if reuse { old.map(|old| slice(old, pos, pos + size)) } else { None };
        let mut encoded = encode_level(level, image.format, lw, lh, image.swizzled, base)?;
        encoded.resize(size, 0);
        out.extend(encoded);
        pos += size;
    }
    if image.paletted() {
        out.extend(encode_clut(image.palette.as_deref().unwrap_or_default(), image.format, clut_base));
    }
    Ok(out)
}

/// Every picture of an IMG/IM2/IM3 archive, in entry order. Texture animation scripts ('#'
/// entries) are not pictures and are skipped (write keeps them from `like`).
pub fn read(data: &[u8]) -> Result<Vec<Image>> {
    let (_, entries) = parse_archive(data)?;
    let mut images = Vec::new();
    for entry in entries {
        if entry.script {
            continue;
        }
        let n = tim2_extent(data, entry.offset, entry.no_image)?;
        let start = entry.offset as usize;
        let mut image = decode_tim2(slice(data, start, start + n), entry.swizzled, entry.no_image)?;
        image.name = entry.name.iter().map(|&b| b as char).collect();
        image.raw_name = Some(slice(&entry.raw, 0, 0x20).to_vec());
        image.index = Some(entry.index);
        image.block = entry.block;
        image.clamp = entry.clamp;
        images.push(image);
    }
    Ok(images)
}

fn latin1(text: &str) -> Result<Vec<u8>> {
    text.chars()
        .map(|c| u8::try_from(c as u32).map_err(|_| Error::invalid("'latin-1' codec can't encode character")))
        .collect()
}

fn name_field(image: &Image) -> Result<Vec<u8>> {
    let name = latin1(&image.name)?;
    if name.len() > 0x1F {
        bail!("name {} longer than 31 bytes", str_repr(&image.name));
    }
    if let Some(raw) = &image.raw_name {
        if raw.split(|&b| b == 0).next().unwrap_or_default() == name.as_slice() {
            return Ok(raw.clone());
        }
    }
    let mut field = name;
    field.resize(0x20, 0);
    Ok(field)
}

/// Python's repr() of a str, for messages (the quotes it picks and the escapes of ASCII).
pub(crate) fn str_repr(text: &str) -> String {
    let quote = if text.contains('\'') && !text.contains('"') { '"' } else { '\'' };
    let mut out = String::new();
    out.push(quote);
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || c as u32 == 0x7F => out.push_str(&format!("\\x{:02x}", c as u32)),
            c if (0x80..0xA0).contains(&(c as u32)) || c == '\u{AD}' => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

/// bytearray slice assignment buffer[start:end] = new (which can change the length).
fn assign(buffer: &mut Vec<u8>, start: usize, end: usize, new: &[u8]) {
    let start = start.min(buffer.len());
    let end = end.clamp(start, buffer.len());
    buffer.splice(start..end, new.iter().copied());
}

/// An archive of the pictures. With `like` (the original archive), pictures are matched to entries
/// by Image.index and everything that is not a picture (scripts, entry fields, padding, stray
/// bytes) is kept; pictures without an index are appended (IM3 only). Without `like` a fresh IM3.
pub fn write(images: &[Image], like: Option<&[u8]>) -> Result<Vec<u8>> {
    let Some(like) = like else {
        return write_fresh(images);
    };
    let (kind, entries) = parse_archive(like)?;
    let Some(kind) = kind else {
        return if images.is_empty() { Ok(like.to_vec()) } else { write_fresh(images) };
    };
    let mut by_index: HashMap<usize, &Image> = HashMap::new();
    let mut any_new = false;
    for image in images {
        match image.index {
            None => any_new = true,
            Some(index) => {
                by_index.insert(index, image);
            }
        }
    }
    if any_new {
        if kind != Kind::Im3 {
            bail!("can only append pictures to IM3 archives");
        }
        return write_appended(images, like, &entries);
    }
    let mut offsets: Vec<usize> = entries
        .iter()
        .filter(|e| e.offset >= 0 && e.offset as usize <= like.len())
        .map(|e| e.offset as usize)
        .collect();
    offsets.sort_unstable();
    offsets.dedup();
    if offsets.is_empty() {
        return Ok(like.to_vec());
    }
    let mut segments: BTreeMap<usize, Vec<u8>> = BTreeMap::new();
    for (i, &o) in offsets.iter().enumerate() {
        let end = offsets.get(i + 1).copied().unwrap_or(like.len());
        segments.insert(o, like[o..end].to_vec());
    }
    let mut new_segments = segments.clone();
    let mut delta_of: HashMap<usize, i64> = HashMap::new();
    for e in &entries {
        let Some(image) = by_index.get(&e.index) else {
            continue;
        };
        if e.script {
            continue;
        }
        let old_len = tim2_extent(like, e.offset, e.no_image)?;
        let tim = encode_tim2(image)?;
        let Some(segment) = segments.get(&(e.offset as usize)) else {
            bail!("{}", e.offset);
        };
        let mut body = tim.clone();
        body.extend_from_slice(slice(segment, old_len, segment.len()));
        if tim.len() != old_len && body.len() % 16 != 0 {
            body.resize(body.len().next_multiple_of(16), 0);
        }
        new_segments.insert(e.offset as usize, body);
        delta_of.insert(e.offset as usize, tim.len() as i64 - old_len as i64);
    }
    let mut prefix = like[..offsets[0]].to_vec();
    let mut out_pos: HashMap<usize, usize> = HashMap::new();
    let mut pos = prefix.len();
    for &o in &offsets {
        out_pos.insert(o, pos);
        pos += new_segments[&o].len();
    }
    for e in &entries {
        if e.offset < 0 {
            continue;
        }
        let Some(&at) = out_pos.get(&(e.offset as usize)) else {
            continue;
        };
        let image = by_index.get(&e.index);
        if let Some(image) = image {
            assign(&mut prefix, e.pos, e.pos + 0x20, &name_field(image)?);
        }
        if kind == Kind::Im3 {
            put(&mut prefix, e.pos + 0x24, &(at as i32).to_le_bytes())?;
            if let Some(image) = image {
                put(&mut prefix, e.pos + 0x28, &(image.swizzled as i32).to_le_bytes())?;
                put(&mut prefix, e.pos + 0x2C, &image.block.to_le_bytes())?;
                let Some(clamp) = image.clamp else {
                    bail!("required argument is not an integer");
                };
                put(&mut prefix, e.pos + 0x38, &clamp.to_le_bytes())?;
                let d = delta_of.get(&(e.offset as usize)).copied().unwrap_or(0);
                if d != 0 {
                    let size = e.size.unwrap_or(0) as i64 + d;
                    put(&mut prefix, e.pos + 0x34, &checked_i32(size)?.to_le_bytes())?;
                }
            }
        } else {
            put(&mut prefix, e.pos + 0x20, &(at as i32).to_le_bytes())?;
        }
    }
    let mut out = prefix;
    for o in &offsets {
        out.extend_from_slice(&new_segments[o]);
    }
    Ok(out)
}

fn checked_i32(value: i64) -> Result<i32> {
    i32::try_from(value).map_err(|_| Error::invalid("'i' format requires -2147483648 <= number <= 2147483647"))
}

fn entry_bytes(image: &Image, offset: usize, size: usize) -> Result<Vec<u8>> {
    let mut out = name_field(image)?;
    out.extend(0x40u32.to_le_bytes());
    out.extend(checked_i32(offset as i64)?.to_le_bytes());
    out.extend((image.swizzled as i32).to_le_bytes());
    out.extend(image.block.to_le_bytes());
    out.extend((image.no_image as i16).to_le_bytes());
    out.extend(0i16.to_le_bytes());
    out.extend(checked_i32(size as i64)?.to_le_bytes());
    let Some(clamp) = image.clamp else {
        bail!("unsupported operand type(s) for &: 'NoneType' and 'int'");
    };
    out.extend(clamp.to_le_bytes());
    Ok(out)
}

fn pad16(data: &mut Vec<u8>) {
    data.resize(data.len().next_multiple_of(16), 0);
}

fn write_fresh(images: &[Image]) -> Result<Vec<u8>> {
    let mut pos = 0x10 + IM3_ENTRY * images.len();
    let (mut table, mut blobs) = (Vec::new(), Vec::new());
    for image in images {
        let mut tim = encode_tim2(image)?;
        table.extend(entry_bytes(image, pos, tim.len())?);
        pad16(&mut tim);
        pos += tim.len();
        blobs.extend(tim);
    }
    let mut out = b"IM3\0".to_vec();
    out.extend(0x10u32.to_le_bytes());
    out.extend((images.len() as u32).to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend(table);
    out.extend(blobs);
    Ok(out)
}

/// An IM3 with extra pictures: the existing archive rewritten, its entry table grown and the new
/// pictures appended.
fn write_appended(images: &[Image], like: &[u8], entries: &[Entry]) -> Result<Vec<u8>> {
    let old: Vec<Image> = images.iter().filter(|i| i.index.is_some()).cloned().collect();
    let new: Vec<&Image> = images.iter().filter(|i| i.index.is_none()).collect();
    let body = write(&old, Some(like))?;
    let grow = IM3_ENTRY * new.len();
    let table_end = 0x10 + IM3_ENTRY * entries.len();
    let mut out = slice(&body, 0, table_end).to_vec();
    let mut rest = slice(&body, table_end, body.len()).to_vec();
    for k in 0..entries.len() {
        let p = 0x10 + IM3_ENTRY * k + 0x24;
        let offset = i32_at(&out, p)? as i64;
        put(&mut out, p, &checked_i32(offset + grow as i64)?.to_le_bytes())?;
    }
    let mut pos = body.len() + grow;
    if !pos.is_multiple_of(16) {
        rest.resize(rest.len() + 16 - pos % 16, 0);
        pos += 16 - pos % 16;
    }
    let mut tail = Vec::new();
    for image in new {
        let mut tim = encode_tim2(image)?;
        out.extend(entry_bytes(image, pos, tim.len())?);
        pad16(&mut tim);
        pos += tim.len();
        tail.extend(tim);
    }
    put(&mut out, 8, &((entries.len() + images.len() - old.len()) as u32).to_le_bytes())?;
    out.extend(rest);
    out.extend(tail);
    Ok(out)
}

/// How alpha maps to PNG for this picture.
pub fn alpha_mode(image: &Image) -> AlphaMode {
    if image.format == Format::Ct24 {
        return AlphaMode::Opaque;
    }
    let top = if image.paletted() {
        image.palette.as_deref().unwrap_or_default().iter().map(|c| c[3]).max()
    } else {
        image.pixels.as_deref().unwrap_or_default().iter().skip(3).step_by(4).copied().max()
    };
    match top {
        None => AlphaMode::Ps2,
        Some(top) if top > 0x80 => AlphaMode::Raw,
        Some(0) => AlphaMode::Zero,
        Some(_) => AlphaMode::Ps2,
    }
}

fn crc32(data: &[u8]) -> u32 {
    let mut hasher = flate2::Crc::new();
    hasher.update(data);
    hasher.sum()
}

fn chunk(tag: &[u8], body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 12);
    out.extend((body.len() as u32).to_be_bytes());
    out.extend(tag);
    out.extend(body);
    let mut tagged = tag.to_vec();
    tagged.extend(body);
    out.extend(crc32(&tagged).to_be_bytes());
    out
}

/// zlib.compress(data, level): the bytes CPython's zlib (1.3.1) writes.
pub fn compress(data: &[u8], level: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() / 2 + 64);
    let mut stream = flate2::Compress::new(flate2::Compression::new(level), true);
    loop {
        out.reserve(data.len() / 2 + 1024);
        let used = stream.total_in() as usize;
        let status = stream
            .compress_vec(&data[used..], &mut out, flate2::FlushCompress::Finish)
            .expect("zlib compresses any input");
        if status == flate2::Status::StreamEnd {
            return out;
        }
    }
}

/// zlib.decompress(data).
pub fn decompress(data: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(data.len() * 4 + 1024);
    let mut stream = flate2::Decompress::new(true);
    loop {
        out.reserve(out.len().max(1024));
        let used = stream.total_in() as usize;
        let before = stream.total_out();
        let status = stream
            .decompress_vec(&data[used..], &mut out, flate2::FlushDecompress::Finish)
            .map_err(|error| Error::invalid(format!("Error -3 while decompressing data: {error}")))?;
        if status == flate2::Status::StreamEnd {
            return Ok(out);
        }
        if stream.total_in() as usize == data.len() && stream.total_out() == before && out.len() < out.capacity() {
            bail!("Error -5 while decompressing data: incomplete or truncated stream");
        }
    }
}

/// The colour type of a PNG write_png makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PngMode {
    /// Indices and a palette of (r, g, b, a).
    P,
    Rgba,
    Rgb,
}

/// A PNG of the pixels: mode P (data = indices, palette 0..255), RGBA, or RGB (3 bytes a pixel).
pub fn write_png(
    width: usize,
    height: usize,
    mode: PngMode,
    data: &[u8],
    palette: &[[u8; 4]],
    text: &[(&str, &str)],
) -> Result<Vec<u8>> {
    let (colour_type, bpp) = match mode {
        PngMode::P => (3u8, 1),
        PngMode::Rgba => (6, 4),
        PngMode::Rgb => (2, 3),
    };
    let stride = width * bpp;
    let mut raw = Vec::with_capacity((stride + 1) * height);
    for y in 0..height {
        raw.push(0);
        raw.extend_from_slice(slice(data, y * stride, (y + 1) * stride));
    }
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend(checked::<u32>(width, "I")?.to_be_bytes());
    ihdr.extend(checked::<u32>(height, "I")?.to_be_bytes());
    ihdr.extend([8, colour_type, 0, 0, 0]);
    out.extend(chunk(b"IHDR", &ihdr));
    if mode == PngMode::P {
        let rgb: Vec<u8> = palette.iter().flat_map(|c| [c[0], c[1], c[2]]).collect();
        out.extend(chunk(b"PLTE", &rgb));
        let alphas: Vec<u8> = palette.iter().map(|c| c[3]).collect();
        if alphas.iter().any(|&a| a != 255) {
            let end = alphas.iter().rposition(|&a| a != 0xFF).map_or(0, |i| i + 1);
            let kept = if end == 0 { &alphas[..1] } else { &alphas[..end] };
            out.extend(chunk(b"tRNS", kept));
        }
    }
    for (key, value) in text {
        let mut body = latin1(key)?;
        body.push(0);
        body.extend(latin1(value)?);
        out.extend(chunk(b"tEXt", &body));
    }
    out.extend(chunk(b"IDAT", &compress(&raw, 6)));
    out.extend(chunk(b"IEND", b""));
    Ok(out)
}

/// Undoes PNG filtering of one (sub)image; returns its rows and the bytes it used.
fn unfilter(raw: &[u8], width: usize, height: usize, bits_pp: usize) -> Result<(Vec<Vec<u8>>, usize)> {
    let bpp = (bits_pp / 8).max(1);
    let stride = (width * bits_pp).div_ceil(8);
    let mut rows = Vec::with_capacity(height);
    let mut prev = vec![0u8; stride];
    let mut pos = 0;
    for _ in 0..height {
        let Some(&filter) = raw.get(pos) else {
            bail!("index out of range");
        };
        let mut line = slice(raw, pos + 1, pos + 1 + stride).to_vec();
        pos += 1 + stride;
        let short = line.len() < stride || prev.len() < stride;
        match filter {
            0 => {}
            1 => {
                if short && stride > bpp {
                    bail!("bytearray index out of range");
                }
                for i in bpp..stride {
                    line[i] = line[i].wrapping_add(line[i - bpp]);
                }
            }
            2 => {
                line = line.iter().zip(&prev).map(|(a, b)| a.wrapping_add(*b)).collect();
            }
            3 => {
                if short {
                    bail!("bytearray index out of range");
                }
                for i in 0..stride {
                    let left = if i >= bpp { line[i - bpp] as u32 } else { 0 };
                    line[i] = line[i].wrapping_add(((left + prev[i] as u32) >> 1) as u8);
                }
            }
            4 => {
                if short {
                    bail!("bytearray index out of range");
                }
                for i in 0..stride {
                    let a = if i >= bpp { line[i - bpp] as i32 } else { 0 };
                    let b = prev[i] as i32;
                    let c = if i >= bpp { prev[i - bpp] as i32 } else { 0 };
                    let p = a + b - c;
                    let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
                    let predicted = if pa <= pb && pa <= pc {
                        a
                    } else if pb <= pc {
                        b
                    } else {
                        c
                    };
                    line[i] = line[i].wrapping_add(predicted as u8);
                }
            }
            _ => bail!("bad PNG filter {filter}"),
        }
        prev = line.clone();
        rows.push(line);
    }
    Ok((rows, pos))
}

/// One row unpacked into samples (16-bit samples as their high byte).
fn samples(row: &[u8], width: usize, depth: u8, channels: usize) -> Vec<u8> {
    match depth {
        8 => row[..(width * channels).min(row.len())].to_vec(),
        16 => row[..(2 * width * channels).min(row.len())].iter().step_by(2).copied().collect(),
        _ => {
            let per = 8 / depth as usize;
            let mask = (1u8 << depth) - 1;
            let mut out = Vec::with_capacity(row.len() * per);
            for &b in row {
                for k in 0..per {
                    out.push((b >> (8 - depth as usize * (k + 1))) & mask);
                }
            }
            out.truncate(width * channels);
            out
        }
    }
}

/// A decoded PNG: mode P (data = indices, palette of (r, g, b, a)) or RGBA (palette empty), with
/// the PNG's own 0..255 alpha.
pub struct Png {
    pub width: usize,
    pub height: usize,
    pub paletted: bool,
    pub data: Vec<u8>,
    pub palette: Vec<[u8; 4]>,
}

/// Every element of a strided slice of `data` (data[start::step]).
fn strided(data: &[u8], start: usize, step: usize) -> Vec<u8> {
    data.iter().skip(start).step_by(step).copied().collect()
}

/// out[start::4] = values, which must be as many as the slots.
fn fill_strided(out: &mut [u8], start: usize, values: &[u8]) -> Result<()> {
    let slots = out.len().saturating_sub(start).div_ceil(4);
    if slots != values.len() {
        bail!("attempt to assign bytes of size {} to extended slice of size {}", values.len(), slots);
    }
    for (slot, &v) in out.iter_mut().skip(start).step_by(4).zip(values) {
        *slot = v;
    }
    Ok(())
}

pub fn read_png(png: &[u8]) -> Result<Png> {
    if png.len() < 8 || &png[..8] != b"\x89PNG\r\n\x1a\n" {
        bail!("not a PNG");
    }
    let mut pos = 8;
    let mut idat = Vec::new();
    let (mut plte, mut trns, mut ihdr) = (None, None, None);
    while pos < png.len() {
        if pos + 8 > png.len() {
            bail!("unpack_from requires a buffer of at least {} bytes", pos + 8);
        }
        let n = u32::from_be_bytes(png[pos..pos + 4].try_into().unwrap()) as usize;
        let tag = &png[pos + 4..pos + 8];
        let body = slice(png, pos + 8, pos + 8 + n);
        pos += 12 + n;
        match tag {
            b"IHDR" => {
                if body.len() != 13 {
                    bail!("unpack requires a buffer of 13 bytes");
                }
                ihdr = Some(body);
            }
            b"PLTE" => plte = Some(body),
            b"tRNS" => trns = Some(body),
            b"IDAT" => idat.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
    }
    let Some(ihdr) = ihdr else {
        bail!("cannot unpack non-iterable NoneType object");
    };
    let w = u32::from_be_bytes(ihdr[0..4].try_into().unwrap()) as usize;
    let h = u32::from_be_bytes(ihdr[4..8].try_into().unwrap()) as usize;
    let (depth, colour_type, interlace) = (ihdr[8], ihdr[9], ihdr[12]);
    let channels = match colour_type {
        0 => 1,
        2 => 3,
        3 => 1,
        4 => 2,
        6 => 4,
        _ => bail!("{colour_type}"),
    };
    let bits_pp = depth as usize * channels;
    let raw = decompress(&idat)?;
    let samples_all = if interlace == 0 {
        let (rows, _) = unfilter(&raw, w, h, bits_pp)?;
        rows.iter().flat_map(|row| samples(row, w, depth, channels)).collect::<Vec<u8>>()
    } else {
        let passes = [(0, 0, 8, 8), (4, 0, 8, 8), (0, 4, 4, 8), (2, 0, 4, 4), (0, 2, 2, 4), (1, 0, 2, 2), (0, 1, 1, 2)];
        let mut buffer = vec![0u8; w * h * channels];
        let mut p = 0;
        for (x0, y0, dx, dy) in passes {
            let pw = if w > x0 { (w - x0).div_ceil(dx) } else { 0 };
            let ph = if h > y0 { (h - y0).div_ceil(dy) } else { 0 };
            if pw == 0 || ph == 0 {
                continue;
            }
            let (rows, used) = unfilter(slice(&raw, p, raw.len()), pw, ph, bits_pp)?;
            p += used;
            for (j, row) in rows.iter().enumerate() {
                let s = samples(row, pw, depth, channels);
                let y = y0 + j * dy;
                for i in 0..pw {
                    let x = x0 + i * dx;
                    let at = (y * w + x) * channels;
                    let part = slice(&s, i * channels, (i + 1) * channels);
                    assign_fixed(&mut buffer, at, at + channels, part);
                }
            }
        }
        buffer
    };
    let n = w * h;
    if colour_type == 3 {
        let plte = plte.unwrap_or_default();
        let mut palette: Vec<[u8; 4]> = plte.as_chunks::<3>().0.iter().map(|c| [c[0], c[1], c[2], 255]).collect();
        if let Some(trns) = trns.filter(|t| !t.is_empty()) {
            for (colour, &a) in palette.iter_mut().zip(trns) {
                colour[3] = a;
            }
        }
        return Ok(Png { width: w, height: h, paletted: true, data: samples_all, palette });
    }
    let mut out = vec![0u8; 4 * n];
    match colour_type {
        6 => out = samples_all,
        2 => {
            for k in 0..3 {
                fill_strided(&mut out, k, &strided(&samples_all, k, 3))?;
            }
            fill_strided(&mut out, 3, &vec![0xFF; n])?;
            if let Some(trns) = trns.filter(|t| t.len() >= 6) {
                let key: Vec<u8> = if depth == 16 {
                    strided(trns, 1, 2).into_iter().take(3).collect()
                } else {
                    vec![trns[1], trns[3], trns[5]]
                };
                for i in 0..n {
                    if out[4 * i..4 * i + 3] == key[..] {
                        out[4 * i + 3] = 0;
                    }
                }
            }
        }
        _ => {
            let mut g = strided(&samples_all, 0, channels);
            if depth < 8 {
                let scale = 255 / ((1u32 << depth) - 1);
                g = g.iter().map(|&v| (v as u32 * scale) as u8).collect();
            }
            for k in 0..3 {
                fill_strided(&mut out, k, &g)?;
            }
            if colour_type == 4 {
                fill_strided(&mut out, 3, &strided(&samples_all, 1, 2))?;
            } else {
                fill_strided(&mut out, 3, &vec![0xFF; n])?;
                if let Some(trns) = trns.filter(|t| t.len() >= 2) {
                    let key = if depth >= 8 { trns[1] as u32 } else { trns[1] as u32 * (255 / ((1u32 << depth) - 1)) };
                    for i in 0..n {
                        if g[i] as u32 == key {
                            out[4 * i + 3] = 0;
                        }
                    }
                }
            }
        }
    }
    Ok(Png { width: w, height: h, paletted: false, data: out, palette: Vec::new() })
}

/// buffer[start:end] = part on a bytearray of fixed size where part has the slice's length.
fn assign_fixed(buffer: &mut [u8], start: usize, end: usize, part: &[u8]) {
    let start = start.min(buffer.len());
    let end = end.clamp(start, buffer.len());
    let n = (end - start).min(part.len());
    buffer[start..start + n].copy_from_slice(&part[..n]);
}

/// The picture's base level as a PNG. Paletted pictures become indexed PNGs (palette in index
/// order) unless indexed is false.
pub fn to_png(image: &Image, indexed: bool) -> Result<Vec<u8>> {
    if image.no_image {
        bail!("{} has no pixels (no_image)", image.name);
    }
    let mode = alpha_mode(image);
    let text = [
        ("Software", "c2"),
        ("im3:name", image.name.as_str()),
        ("im3:format", image.format.name()),
        ("im3:alpha", mode.name()),
    ];
    let flat = matches!(mode, AlphaMode::Zero | AlphaMode::Opaque);
    if image.paletted() && indexed {
        let palette: Vec<[u8; 4]> = image
            .palette
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|&[r, g, b, a]| [r, g, b, if flat { 255 } else { mode.to_png(a) }])
            .collect();
        let indices = image.indices.as_deref().unwrap_or_default();
        return write_png(image.width, image.height, PngMode::P, indices, &palette, &text);
    }
    let Some(mut rgba) = image.rgba()? else {
        bail!("the picture has no pixels");
    };
    if flat {
        let n = 3 * image.width * image.height;
        let rgb: Vec<u8> = rgba.chunks(4).flat_map(|c| c[..c.len().min(3)].to_vec()).collect();
        if rgb.len() != n {
            bail!("attempt to assign bytes of size {} to extended slice of size {}", rgba.len().div_ceil(4), n / 3);
        }
        return write_png(image.width, image.height, PngMode::Rgb, &rgb, &[], &text);
    }
    for a in rgba.iter_mut().skip(3).step_by(4) {
        *a = mode.to_png(*a);
    }
    write_png(image.width, image.height, PngMode::Rgba, &rgba, &[], &text)
}

fn to_ps2_alpha(rgba: &[u8], mode: AlphaMode) -> Vec<u8> {
    let mut out = rgba.to_vec();
    let all_opaque = || out.iter().skip(3).step_by(4).all(|&a| a == 255);
    if mode == AlphaMode::Opaque {
        let count = out.len() / 4;
        for a in out.iter_mut().skip(3).step_by(4).take(count) {
            *a = 0x80;
        }
    } else if mode == AlphaMode::Zero && all_opaque() {
        for a in out.iter_mut().skip(3).step_by(4) {
            *a = 0;
        }
    } else {
        for a in out.iter_mut().skip(3).step_by(4) {
            *a = mode.to_ps2(*a);
        }
    }
    out
}

type Colour = [u8; 4];

/// Up to n representative colours of (colour, count) pairs, by median cut.
fn median_cut(colours: Vec<(Colour, u64)>, n: usize) -> Vec<Colour> {
    let mut boxes = vec![colours];
    while boxes.len() < n {
        let (mut best, mut best_i, mut best_axis) = (-1i32, None, 0);
        for (i, b) in boxes.iter().enumerate() {
            if b.len() < 2 {
                continue;
            }
            for axis in 0..4 {
                let top = b.iter().map(|(c, _)| c[axis]).max().unwrap() as i32;
                let bottom = b.iter().map(|(c, _)| c[axis]).min().unwrap() as i32;
                if top - bottom > best {
                    (best, best_i, best_axis) = (top - bottom, Some(i), axis);
                }
            }
        }
        let Some(best_i) = best_i.filter(|_| best > 0) else {
            break;
        };
        let mut b = boxes.remove(best_i);
        b.sort_by_key(|(c, _)| c[best_axis]);
        let total: u64 = b.iter().map(|(_, w)| w).sum();
        let mut acc = 0u64;
        let mut cut = 1;
        for (k, (_, w)) in b.iter().enumerate() {
            acc += w;
            if acc as f64 >= total as f64 / 2.0 {
                cut = (k + 1).max(1).min(b.len() - 1);
                break;
            }
        }
        let second = b.split_off(cut);
        boxes.push(b);
        boxes.push(second);
    }
    boxes
        .iter()
        .map(|b| {
            let total = b.iter().map(|(_, w)| w).sum::<u64>().max(1);
            let mut colour = [0u8; 4];
            for (a, slot) in colour.iter_mut().enumerate() {
                let weighted: u64 = b.iter().map(|(c, w)| c[a] as u64 * w).sum();
                *slot = py::round_int(weighted as f64 / total as f64) as u8;
            }
            colour
        })
        .collect()
}

/// The index of the nearest palette colour (alpha weighs double), the first of equals; cached.
struct Nearest<'a> {
    palette: &'a [Colour],
    cache: HashMap<Colour, u8>,
}

impl<'a> Nearest<'a> {
    fn new(palette: &'a [Colour]) -> Nearest<'a> {
        Nearest { palette, cache: HashMap::new() }
    }

    fn find(&mut self, c: Colour) -> u8 {
        if let Some(&got) = self.cache.get(&c) {
            return got;
        }
        let (mut best, mut got) = (None, 0usize);
        for (i, p) in self.palette.iter().enumerate() {
            let d: i64 = (0..3).map(|k| (p[k] as i64 - c[k] as i64).pow(2)).sum::<i64>()
                + 2 * (p[3] as i64 - c[3] as i64).pow(2);
            if best.is_none_or(|b| d < b) {
                (best, got) = (Some(d), i);
                if d == 0 {
                    break;
                }
            }
        }
        self.cache.insert(c, got as u8);
        got as u8
    }
}

/// A counted histogram in first-seen order.
struct Histogram {
    order: Vec<(Colour, u64)>,
    at: HashMap<Colour, usize>,
}

impl Histogram {
    fn new() -> Histogram {
        Histogram { order: Vec::new(), at: HashMap::new() }
    }

    fn add(&mut self, c: Colour, count: u64) {
        match self.at.get(&c) {
            Some(&i) => self.order[i].1 += count,
            None => {
                self.at.insert(c, self.order.len());
                self.order.push((c, count));
            }
        }
    }
}

/// RGBA (PS2 alpha) to indices and a palette of n entries.
fn quantise(rgba: &[u8], n: usize) -> (Vec<u8>, Vec<Colour>) {
    let pixels: Vec<Colour> = rgba.chunks(4).map(|c| [c[0], c[1], c[2], c[3]]).collect();
    let mut hist = Histogram::new();
    for &c in &pixels {
        hist.add(c, 1);
    }
    let mut palette = if hist.order.len() <= n {
        let mut colours: Vec<Colour> = hist.order.iter().map(|(c, _)| *c).collect();
        colours.sort();
        colours
    } else if hist.order.len() > 32768 {
        let mut coarse: Vec<[u64; 5]> = Vec::new();
        let mut coarse_at: HashMap<Colour, usize> = HashMap::new();
        for (c, w) in &hist.order {
            let k = [c[0] >> 3, c[1] >> 3, c[2] >> 3, c[3] >> 3];
            let i = *coarse_at.entry(k).or_insert_with(|| {
                coarse.push([0; 5]);
                coarse.len() - 1
            });
            let s = &mut coarse[i];
            for a in 0..4 {
                s[a] += c[a] as u64 * w;
            }
            s[4] += w;
        }
        let mut merged = Histogram::new();
        for s in &coarse {
            let c = [(s[0] / s[4]) as u8, (s[1] / s[4]) as u8, (s[2] / s[4]) as u8, (s[3] / s[4]) as u8];
            merged.add(c, s[4]);
        }
        median_cut(merged.order, n)
    } else {
        median_cut(hist.order, n)
    };
    let mut nearest = Nearest::new(&palette);
    let indices = pixels.iter().map(|&c| nearest.find(c)).collect();
    palette.resize(n.max(palette.len()), [0, 0, 0, 0]);
    (indices, palette)
}

fn downsample(rgba: &[u8], w: usize, h: usize) -> (Vec<u8>, usize, usize) {
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let mut out = vec![0u8; 4 * nw * nh];
    for y in 0..nh {
        for x in 0..nw {
            for c in 0..4 {
                let mut s = 0u32;
                for dy in 0..2 {
                    for dx in 0..2 {
                        let (sx, sy) = ((2 * x + dx).min(w - 1), (2 * y + dy).min(h - 1));
                        s += rgba[4 * (sy * w + sx) + c] as u32;
                    }
                }
                out[4 * (y * nw + x) + c] = ((s + 2) / 4) as u8;
            }
        }
    }
    (out, nw, nh)
}

/// PNG colours (indexed or not) as RGBA, unused palette entries black and opaque.
pub fn png_rgba(png: &Png) -> Result<Vec<u8>> {
    if png.paletted {
        lookup(&png.data, &png.palette, [0, 0, 0, 0xFF])
    } else {
        Ok(png.data.clone())
    }
}

/// A picture made from PNG bytes, taking name, format, palette size, flags and header from `like`
/// (normally the picture the PNG was exported from). Paletted targets: an indexed PNG with at most
/// as many colours is taken as is; otherwise colours are matched to like's palette, and only if
/// that fails quantised (median cut) to a new palette.
pub fn from_png(png: &[u8], like: &Image) -> Result<Image> {
    let png = read_png(png)?;
    let (w, h) = (png.width, png.height);
    let mut image = like.clone();
    image.width = w;
    image.height = h;
    let mode = alpha_mode(like);
    let same_size = (w, h) == (like.width, like.height);

    let unchanged = if like.paletted() {
        let like_palette = like.palette.as_deref().unwrap_or_default();
        let n = like_palette.len();
        let top = png.data.iter().copied().max().unwrap_or(0) as usize;
        if png.paletted && png.palette.len() <= n && top < png.palette.len() {
            let opaque_png = png.palette.iter().all(|c| c[3] == 255);
            let mut palette: Vec<Colour> = png
                .palette
                .iter()
                .map(|&[r, g, b, a]| {
                    let a2 = match mode {
                        AlphaMode::Opaque => 0x80,
                        AlphaMode::Zero if opaque_png => 0,
                        _ => mode.to_ps2(a),
                    };
                    [r, g, b, a2]
                })
                .collect();
            palette.extend_from_slice(&like_palette[palette.len()..]);
            image.palette = Some(palette);
            image.indices = Some(png.data.clone());
        } else {
            let rgba = to_ps2_alpha(&png_rgba(&png)?, mode);
            let mut candidates: HashMap<Colour, Vec<usize>> = HashMap::new();
            for (i, c) in like_palette.iter().enumerate() {
                candidates.entry(*c).or_default().push(i);
            }
            let old = if same_size { like.indices.as_deref() } else { None };
            let mut indices = vec![0u8; w * h];
            let mut ok = true;
            for (p, slot) in indices.iter_mut().enumerate() {
                let c = rgba.get(4 * p..4 * p + 4).map(|c| [c[0], c[1], c[2], c[3]]);
                let Some(found) = c.and_then(|c| candidates.get(&c)) else {
                    ok = false;
                    break;
                };
                let keep = old.and_then(|old| old.get(p)).filter(|&&o| found.contains(&(o as usize)));
                *slot = match keep {
                    Some(&o) => o,
                    None => found[0] as u8,
                };
            }
            if ok {
                image.palette = Some(like_palette.to_vec());
                image.indices = Some(indices);
            } else {
                let (indices, palette) = quantise(&rgba, n);
                image.indices = Some(indices);
                image.palette = Some(palette);
            }
        }
        image.pixels = None;
        same_size && image.indices == like.indices && image.palette == like.palette
    } else {
        image.pixels = Some(to_ps2_alpha(&png_rgba(&png)?, mode));
        image.indices = None;
        image.palette = None;
        same_size && image.pixels == like.pixels
    };
    if !like.mips.is_empty() && !unchanged {
        image.mips = Vec::new();
        let (mut current, mut cw, mut ch) = (image.rgba()?.unwrap_or_default(), w, h);
        let palette = image.palette.clone().unwrap_or_default();
        let mut nearest = Nearest::new(&palette);
        for _ in &like.mips {
            (current, cw, ch) = downsample(&current, cw, ch);
            if image.paletted() {
                image.mips.push(current.chunks(4).map(|c| nearest.find([c[0], c[1], c[2], c[3]])).collect());
            } else {
                image.mips.push(current.clone());
            }
        }
    } else if unchanged {
        image.mips = like.mips.clone();
    }
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swizzle_table_is_a_permutation() {
        let mut seen = block8().to_vec();
        seen.sort_unstable();
        assert_eq!(seen, (0..256).collect::<Vec<_>>());
    }

    #[test]
    fn alpha_tables_round_trip() {
        for a in 0..=128u8 {
            assert_eq!(p2a(a2p(a)), a);
        }
    }

    #[test]
    fn pictures_round_trip() {
        let mut eight = Image::new("tex01", Format::T8, 32, 16);
        eight.palette = Some((0..256).map(|i| [i as u8, 255 - i as u8, 0, 0x80]).collect());
        eight.indices = Some((0..32 * 16).map(|i| (i % 251) as u8).collect());
        eight.swizzled = true;
        let mut four = Image::new("tex02", Format::T4, 8, 8);
        four.palette = Some((0..16).map(|i| [i * 16, 0, 0, 0x40]).collect());
        four.indices = Some((0..64).map(|i| i % 16).collect());
        let mut rgb = Image::new("tex03", Format::Ct24, 4, 4);
        rgb.pixels = Some([1, 2, 3, 0x80].repeat(16));
        let data = write(&[eight, four, rgb], None).unwrap();
        let images = read(&data).unwrap();
        assert_eq!(images.len(), 3);
        assert_eq!(images[0].indices.as_deref().unwrap()[..4], [0, 1, 2, 3]);
        assert_eq!(write(&images, Some(&data)).unwrap(), data);
        for image in &images {
            let png = to_png(image, true).unwrap();
            let back = from_png(&png, image).unwrap();
            assert_eq!(back.indices, image.indices);
            assert_eq!(back.pixels, image.pixels);
        }
    }
}
