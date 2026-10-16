//! Texture archives (IM3, IM2 and IMG) of TIM2 pictures, to and from RGBA and PNG.
//!
//! An Image keeps its pixels in the engine's terms: paletted pictures keep `indices` (one byte per
//! pixel, raster order) and `palette` (index order, the CSM1 swizzle undone); true-colour ones keep
//! `pixels` as RGBA. Alpha is the PS2's, where 0x80 is opaque; only the PNG side rescales it.

use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

use crate::bytes::{clamped, i32_at, u16_at, u32_at, u64_at};
use crate::png::{self, PngMode};
use crate::quantise;
use crate::{bail, Error, Result};

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
        let colour = palette.get(i as usize).copied().unwrap_or(fill);
        out.extend(colour);
    }
    Ok(out)
}

/// Writes the bytes at `at`, which must fit in the buffer.
fn put(buffer: &mut [u8], at: usize, bytes: &[u8]) -> Result<()> {
    match buffer.get_mut(at..at + bytes.len()) {
        Some(target) => {
            target.copy_from_slice(bytes);
            Ok(())
        }
        None => bail!("the archive's header ends before the {} bytes written at 0x{at:x}", bytes.len()),
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
        _ => bail!("unknown archive tag {}", data[..4].escape_ascii()),
    };
    let mut entries = Vec::new();
    for i in 0..count as usize {
        let pos = 0x10 + stride * i;
        let raw = clamped(data, pos, pos + stride).to_vec();
        let field = clamped(&raw, 0, 0x20);
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
                bail!("an IM3 entry is {} bytes, short of 64", raw.len());
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

/// Where a TIM2 starts, as a position in the data; an offset outside the data is an error.
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
    if clamped(data, off, off + 4) != b"TIM2" {
        bail!("no TIM2 at 0x{:x}", off);
    }
    let pictures = u16_at(data, off + 6)?;
    let q = off + TIM2_FILE_HEADER;
    if no_image {
        return Ok(TIM2_FILE_HEADER + u16_at(data, q + 12)? as usize);
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

/// The TIM2 file header ("TIM2", version, alignment, picture count, reserved), before the first
/// picture's header.
pub(crate) const TIM2_FILE_HEADER: usize = 16;

/// The picture header fields of a TIM2 (at TIM2_FILE_HEADER).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Header {
    pub(crate) total: u32,
    pub(crate) clut_size: u32,
    pub(crate) image_size: u32,
    pub(crate) header_size: u16,
    pub(crate) colours: u16,
    pub(crate) pf: u8,
    pub(crate) mipmaps: u8,
    pub(crate) clut_type: u8,
    pub(crate) image_type: u8,
    pub(crate) width: u16,
    pub(crate) height: u16,
}

/// The first picture's header of a TIM2.
pub(crate) fn header(tim: &[u8]) -> Result<Header> {
    if tim.len() < 40 {
        bail!("a TIM2 is {} bytes, short of its 40-byte header", tim.len());
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

/// The most levels a TIM2 picture has: the base level and the six mipmaps the GS's MXL allows.
const MAX_LEVELS: u8 = 7;

fn level_sizes(tim: &[u8], h: &Header) -> Result<Vec<usize>> {
    if h.mipmaps > MAX_LEVELS {
        bail!("a TIM2 picture of {} levels; the GS takes at most {MAX_LEVELS}", h.mipmaps);
    }
    if h.mipmaps > 1 {
        (0..h.mipmaps as usize).map(|k| Ok(u32_at(tim, TIM2_FILE_HEADER + 0x40 + 4 * k)? as usize)).collect()
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
    let mut pos = TIM2_FILE_HEADER + h.header_size as usize;
    let mut levels = Vec::new();
    for (k, size) in level_sizes(tim, &h)?.into_iter().enumerate() {
        let (lw, lh) = ((h.width as usize >> k).max(1), (h.height as usize >> k).max(1));
        levels.push(decode_level(clamped(tim, pos, pos + size), format, lw, lh, swizzled)?);
        pos += size;
    }
    let mut levels = levels.into_iter();
    if format.paletted() {
        let start = TIM2_FILE_HEADER + h.header_size as usize + h.image_size as usize;
        let clut = clamped(tim, start, start + h.clut_size as usize);
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
                bail!("a 24-bit picture of {n} pixels has {} bytes of pixels, not {}", raw.len(), 3 * n);
            }
            raw[..3 * n].chunks(3).flat_map(|c| [c[0], c[1], c[2], 0x80]).collect()
        }
        Format::Ct16 => {
            let used = &raw[..(2 * n).min(raw.len())];
            if !used.len().is_multiple_of(2) {
                bail!("a 16-bit picture's pixels end in half a pixel");
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
                        None => bail!("the picture's data runs past the picture it is based on"),
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
                    bail!("the picture's data runs past the picture it is based on");
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
                    bail!("a 24-bit picture of {n} pixels is given {got} pixels");
                }
            }
            (0..n).flat_map(|i| [data[4 * i], data[4 * i + 1], data[4 * i + 2]]).collect()
        }
        Format::Ct16 => {
            let mut out = Vec::with_capacity(2 * n);
            for i in 0..n {
                let Some(p) = data.get(4 * i..4 * i + 4) else {
                    bail!("palette entry {i} has fewer than 4 bytes");
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
    let tw = w.next_power_of_two().trailing_zeros() as u64;
    let th = h.next_power_of_two().trailing_zeros() as u64;
    let tbw = (w as u64).div_ceil(64).max(1);
    (tbw, tw, th)
}

fn levels_pixels(w: usize, h: usize, levels: usize) -> usize {
    (0..levels).map(|k| (w >> k).max(1) * (h >> k).max(1)).sum()
}

fn checked<T: TryFrom<usize>>(value: usize, what: &str) -> Result<T> {
    T::try_from(value).map_err(|_| Error::invalid(format!("{value} does not fit the TIM2 header's {what}")))
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
        (None, true) => bail!("{} is paletted but has no palette", image.name),
        _ => (0, 0, 0),
    };
    let (file_header, mut tex0, tex1, regs, tclut, pf, rule, old_total);
    if let Some(old) = old {
        let o = header(old)?;
        file_header = old[..16].to_vec();
        tex0 = u64_at(old, TIM2_FILE_HEADER + 0x18)?;
        tex1 = u64_at(old, TIM2_FILE_HEADER + 0x20)?;
        regs = u32_at(old, TIM2_FILE_HEADER + 0x28)?;
        tclut = u32_at(old, TIM2_FILE_HEADER + 0x2C)?;
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
    out.extend(checked::<u32>(total, "total size")?.to_le_bytes());
    out.extend(checked::<u32>(clut_size, "palette size")?.to_le_bytes());
    out.extend(checked::<u32>(image_size, "pixels' size")?.to_le_bytes());
    out.extend(checked::<u16>(header_size, "header size")?.to_le_bytes());
    out.extend(checked::<u16>(colours, "colour count")?.to_le_bytes());
    out.push(pf);
    out.push(checked::<u8>(levels, "mipmap count")?);
    out.push(clut_type);
    out.push(format.image_type());
    out.extend(checked::<u16>(w, "width")?.to_le_bytes());
    out.extend(checked::<u16>(h, "height")?.to_le_bytes());
    out.extend(tex0.to_le_bytes());
    out.extend(tex1.to_le_bytes());
    out.extend(regs.to_le_bytes());
    out.extend(tclut.to_le_bytes());
    if levels > 1 {
        let mut mip = vec![0u8; header_size - 0x30];
        if let Some(old) = old.filter(|old| old.len() >= TIM2_FILE_HEADER + 0x40) {
            let old_header = u16_at(old, TIM2_FILE_HEADER + 12)? as usize;
            if old_header > 0x30 {
                let keep = clamped(old, TIM2_FILE_HEADER + 0x30, TIM2_FILE_HEADER + old_header);
                let keep = &keep[..keep.len().min(mip.len())];
                mip[..keep.len()].copy_from_slice(keep);
            }
        }
        for (k, size) in sizes.iter().enumerate() {
            mip[0x10 + 4 * k..0x14 + 4 * k].copy_from_slice(&checked::<u32>(*size, "mipmap size")?.to_le_bytes());
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
            let header_end = TIM2_FILE_HEADER + o.header_size as usize;
            let sizes = if o.mipmaps > 1 {
                (0..o.mipmaps as usize)
                    .map(|k| Ok(u32_at(old, TIM2_FILE_HEADER + 0x40 + 4 * k)? as usize))
                    .collect::<Result<_>>()?
            } else {
                vec![o.image_size as usize]
            };
            let clut_start = header_end + o.image_size as usize;
            let clut_base = clamped(old, clut_start, clut_start + o.clut_size as usize);
            reused = Some((clamped(old, 0, header_end).to_vec(), sizes, clut_base));
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
        let base = if reuse { old.map(|old| clamped(old, pos, pos + size)) } else { None };
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
        let mut image = decode_tim2(clamped(data, start, start + n), entry.swizzled, entry.no_image)?;
        image.name = entry.name.iter().map(|&b| b as char).collect();
        image.raw_name = Some(clamped(&entry.raw, 0, 0x20).to_vec());
        image.index = Some(entry.index);
        image.block = entry.block;
        image.clamp = entry.clamp;
        images.push(image);
    }
    Ok(images)
}

fn name_field(image: &Image) -> Result<Vec<u8>> {
    let name = png::latin1(&image.name)?;
    if name.len() > 0x1F {
        bail!("name {:?} longer than 31 bytes", image.name);
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
        body.extend_from_slice(clamped(segment, old_len, segment.len()));
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
        let delta = delta_of.get(&(e.offset as usize)).copied().unwrap_or(0);
        rewrite_entry(&mut prefix, kind, e, at, by_index.get(&e.index).copied(), delta)?;
    }
    let mut out = prefix;
    for o in &offsets {
        out.extend_from_slice(&new_segments[o]);
    }
    Ok(out)
}

/// Rewrites entry `e`'s fields in the archive's header (`prefix`) for write: its picture's offset
/// (now `at`) and, when it holds a picture of `image`'s, that picture's name and (IM3) settings,
/// and its size grown by `delta` when the picture changed length.
fn rewrite_entry(
    prefix: &mut Vec<u8>,
    kind: Kind,
    e: &Entry,
    at: usize,
    image: Option<&Image>,
    delta: i64,
) -> Result<()> {
    if let Some(image) = image {
        assign(prefix, e.pos, e.pos + 0x20, &name_field(image)?);
    }
    if kind != Kind::Im3 {
        return put(prefix, e.pos + 0x20, &(at as i32).to_le_bytes());
    }
    put(prefix, e.pos + 0x24, &(at as i32).to_le_bytes())?;
    if let Some(image) = image {
        put(prefix, e.pos + 0x28, &(image.swizzled as i32).to_le_bytes())?;
        put(prefix, e.pos + 0x2C, &image.block.to_le_bytes())?;
        let Some(clamp) = image.clamp else {
            bail!("{} has no clamp setting, which an IM3 entry needs", image.name);
        };
        put(prefix, e.pos + 0x38, &clamp.to_le_bytes())?;
        if delta != 0 {
            let size = e.size.unwrap_or(0) as i64 + delta;
            put(prefix, e.pos + 0x34, &checked_i32(size)?.to_le_bytes())?;
        }
    }
    Ok(())
}

fn checked_i32(value: i64) -> Result<i32> {
    i32::try_from(value).map_err(|_| {
        Error::invalid(format!("the archive would need an offset or size of {value}, past the 2 GB its entries reach"))
    })
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
        bail!("{} has no clamp setting", image.name);
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
    let mut out = clamped(&body, 0, table_end).to_vec();
    let mut rest = clamped(&body, table_end, body.len()).to_vec();
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

/// The picture's base level as a PNG: an indexed one (palette in index order) for a paletted
/// picture.
pub fn to_png(image: &Image) -> Result<Vec<u8>> {
    if image.no_image {
        bail!("{} has no pixels (no_image)", image.name);
    }
    let mode = alpha_mode(image);
    let text = [
        ("Software", "dc2tk"),
        ("im3:name", image.name.as_str()),
        ("im3:format", image.format.name()),
        ("im3:alpha", mode.name()),
    ];
    let flat = matches!(mode, AlphaMode::Zero | AlphaMode::Opaque);
    if image.paletted() {
        let palette: Vec<[u8; 4]> = image
            .palette
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|&[r, g, b, a]| [r, g, b, if flat { 255 } else { mode.to_png(a) }])
            .collect();
        let indices = image.indices.as_deref().unwrap_or_default();
        return png::write_png(image.width, image.height, PngMode::P, indices, &palette, &text);
    }
    let Some(mut rgba) = image.rgba()? else {
        bail!("the picture has no pixels");
    };
    if flat {
        let n = 3 * image.width * image.height;
        let rgb: Vec<u8> = rgba.chunks(4).flat_map(|c| c[..c.len().min(3)].to_vec()).collect();
        if rgb.len() != n {
            bail!("{} has {} pixels, not {} by {}", image.name, rgba.len().div_ceil(4), image.width, image.height);
        }
        return png::write_png(image.width, image.height, PngMode::Rgb, &rgb, &[], &text);
    }
    for a in rgba.iter_mut().skip(3).step_by(4) {
        *a = mode.to_png(*a);
    }
    png::write_png(image.width, image.height, PngMode::Rgba, &rgba, &[], &text)
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

/// A picture made from PNG bytes, taking name, format, palette size, flags and header from `like`
/// (normally the picture the PNG was exported from). Paletted targets: an indexed PNG with at most
/// as many colours is taken as is; otherwise colours are matched to like's palette, and only if
/// that fails quantised (median cut) to a new palette.
pub fn from_png(png: &[u8], like: &Image) -> Result<Image> {
    let png = png::read_png(png)?;
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
            let mut palette: Vec<[u8; 4]> = png
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
            let rgba = to_ps2_alpha(&png.rgba(), mode);
            let mut candidates: HashMap<[u8; 4], Vec<usize>> = HashMap::new();
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
                let (indices, palette) = quantise::quantise(&rgba, n);
                image.indices = Some(indices);
                image.palette = Some(palette);
            }
        }
        image.pixels = None;
        same_size && image.indices == like.indices && image.palette == like.palette
    } else {
        image.pixels = Some(to_ps2_alpha(&png.rgba(), mode));
        image.indices = None;
        image.palette = None;
        same_size && image.pixels == like.pixels
    };
    if !like.mips.is_empty() && !unchanged {
        image.mips = Vec::new();
        let (mut current, mut cw, mut ch) = (image.rgba()?.unwrap_or_default(), w, h);
        let palette = image.palette.clone().unwrap_or_default();
        let mut nearest = quantise::Nearest::new(&palette);
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
            let png = to_png(image).unwrap();
            let back = from_png(&png, image).unwrap();
            assert_eq!(back.indices, image.indices);
            assert_eq!(back.pixels, image.pixels);
        }
    }
}
