//! PNG files, written and read for the textures (im3::to_png, im3::from_png) and the models'
//! pictures: 8-bit RGB, RGBA and paletted PNGs written; every colour type, bit depth and
//! interlacing read, as RGBA or indices and a palette. And the zlib streams PNG and the export's
//! files compress with.

use crate::bytes::clamped;
use crate::{bail, Context, Error, Result};

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

/// The data as a zlib stream at a compression level, as the zlib library (1.3.1) writes it.
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

/// The data of a zlib stream.
pub fn decompress(data: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(data.len() * 4 + 1024);
    let mut stream = flate2::Decompress::new(true);
    loop {
        out.reserve(out.len().max(1024));
        let used = stream.total_in() as usize;
        let before = stream.total_out();
        let status = stream
            .decompress_vec(&data[used..], &mut out, flate2::FlushDecompress::Finish)
            .context("the compressed data does not inflate")?;
        if status == flate2::Status::StreamEnd {
            return Ok(out);
        }
        if stream.total_in() as usize == data.len() && stream.total_out() == before && out.len() < out.capacity() {
            bail!("the compressed data ends before its stream does");
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

/// Text as Latin-1 bytes, as PNG text chunks and the textures' names hold it.
pub(crate) fn latin1(text: &str) -> Result<Vec<u8>> {
    text.chars()
        .map(|c| u8::try_from(c as u32))
        .collect::<std::result::Result<_, _>>()
        .map_err(|_| Error::invalid(format!("{text:?} has a character Latin-1 cannot encode")))
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
        raw.extend_from_slice(clamped(data, y * stride, (y + 1) * stride));
    }
    let (Ok(w), Ok(h)) = (u32::try_from(width), u32::try_from(height)) else {
        bail!("a PNG of {width} by {height} pixels, more than PNG's 32 bits hold");
    };
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend(w.to_be_bytes());
    ihdr.extend(h.to_be_bytes());
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
            bail!("the PNG's image data ends before its last row");
        };
        let mut line = clamped(raw, pos + 1, pos + 1 + stride).to_vec();
        pos += 1 + stride;
        let short = line.len() < stride || prev.len() < stride;
        match filter {
            0 => {}
            1 => {
                if short && stride > bpp {
                    bail!("the PNG's image data ends inside a row");
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
                    bail!("the PNG's image data ends inside a row");
                }
                for i in 0..stride {
                    let left = if i >= bpp { line[i - bpp] as u32 } else { 0 };
                    line[i] = line[i].wrapping_add(((left + prev[i] as u32) >> 1) as u8);
                }
            }
            4 => {
                if short {
                    bail!("the PNG's image data ends inside a row");
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
            _ => bail!("a PNG row of filter {filter}, which PNG does not have"),
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
#[derive(Clone, Debug)]
pub struct Png {
    pub width: usize,
    pub height: usize,
    pub paletted: bool,
    pub data: Vec<u8>,
    pub palette: Vec<[u8; 4]>,
}

impl Png {
    /// The colours (indexed or not) as RGBA, an index past the palette black and opaque.
    pub fn rgba(&self) -> Vec<u8> {
        if !self.paletted {
            return self.data.clone();
        }
        self.data.iter().flat_map(|&i| self.palette.get(i as usize).copied().unwrap_or([0, 0, 0, 0xFF])).collect()
    }
}

/// Every element of a strided slice of `data` (data[start::step]).
fn strided(data: &[u8], start: usize, step: usize) -> Vec<u8> {
    data.iter().skip(start).step_by(step).copied().collect()
}

/// out[start::4] = values, which must be as many as the slots.
fn fill_strided(out: &mut [u8], start: usize, values: &[u8]) -> Result<()> {
    let slots = out.len().saturating_sub(start).div_ceil(4);
    if slots != values.len() {
        bail!("a PNG channel of {} values for {slots} pixels", values.len());
    }
    for (slot, &v) in out.iter_mut().skip(start).step_by(4).zip(values) {
        *slot = v;
    }
    Ok(())
}

/// buffer[start..end] = part, both ends clamped to the buffer (part as long as the slice).
fn assign_fixed(buffer: &mut [u8], start: usize, end: usize, part: &[u8]) {
    let start = start.min(buffer.len());
    let end = end.clamp(start, buffer.len());
    let n = (end - start).min(part.len());
    buffer[start..start + n].copy_from_slice(&part[..n]);
}

/// The chunks of a PNG read reads: its header, palette and transparency, and its image data
/// joined.
struct Chunks<'a> {
    ihdr: &'a [u8],
    plte: Option<&'a [u8]>,
    trns: Option<&'a [u8]>,
    idat: Vec<u8>,
}

fn chunks(png: &[u8]) -> Result<Chunks<'_>> {
    if png.len() < 8 || &png[..8] != b"\x89PNG\r\n\x1a\n" {
        bail!("not a PNG");
    }
    let mut pos = 8;
    let mut idat = Vec::new();
    let (mut plte, mut trns, mut ihdr) = (None, None, None);
    while pos < png.len() {
        if pos + 8 > png.len() {
            bail!("the PNG ends inside the head of a chunk at {pos}");
        }
        let n = u32::from_be_bytes(png[pos..pos + 4].try_into().unwrap()) as usize;
        let tag = &png[pos + 4..pos + 8];
        let body = clamped(png, pos + 8, pos + 8 + n);
        pos += 12 + n;
        match tag {
            b"IHDR" => {
                if body.len() != 13 {
                    bail!("the PNG's IHDR chunk is {} bytes, not 13", body.len());
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
        bail!("the PNG has no IHDR chunk");
    };
    Ok(Chunks { ihdr, plte, trns, idat })
}

/// A PNG's IHDR: its size and how its pixels are stored.
struct Header {
    width: usize,
    height: usize,
    depth: u8,
    colour_type: u8,
    interlaced: bool,
    channels: usize,
}

impl Header {
    fn read(ihdr: &[u8]) -> Result<Header> {
        let width = u32::from_be_bytes(ihdr[0..4].try_into().unwrap()) as usize;
        let height = u32::from_be_bytes(ihdr[4..8].try_into().unwrap()) as usize;
        let (depth, colour_type, interlace) = (ihdr[8], ihdr[9], ihdr[12]);
        let channels = match colour_type {
            0 => 1,
            2 => 3,
            3 => 1,
            4 => 2,
            6 => 4,
            _ => bail!("a PNG of colour type {colour_type}, which PNG does not have"),
        };
        if ![1, 2, 4, 8, 16].contains(&depth) {
            bail!("a PNG of bit depth {depth}, which PNG does not have");
        }
        Ok(Header { width, height, depth, colour_type, interlaced: interlace != 0, channels })
    }

    fn bits_per_pixel(&self) -> usize {
        self.depth as usize * self.channels
    }
}

/// The samples of every pixel in raster order, unfiltered (and the seven Adam7 passes put in
/// place when interlaced).
fn all_samples(raw: &[u8], h: &Header) -> Result<Vec<u8>> {
    let (w, height, bits_pp) = (h.width, h.height, h.bits_per_pixel());
    if !h.interlaced {
        let (rows, _) = unfilter(raw, w, height, bits_pp)?;
        return Ok(rows.iter().flat_map(|row| samples(row, w, h.depth, h.channels)).collect());
    }
    let passes = [(0, 0, 8, 8), (4, 0, 8, 8), (0, 4, 4, 8), (2, 0, 4, 4), (0, 2, 2, 4), (1, 0, 2, 2), (0, 1, 1, 2)];
    let channels = h.channels;
    let mut buffer = vec![0u8; w * height * channels];
    let mut p = 0;
    for (x0, y0, dx, dy) in passes {
        let pw = if w > x0 { (w - x0).div_ceil(dx) } else { 0 };
        let ph = if height > y0 { (height - y0).div_ceil(dy) } else { 0 };
        if pw == 0 || ph == 0 {
            continue;
        }
        let (rows, used) = unfilter(clamped(raw, p, raw.len()), pw, ph, bits_pp)?;
        p += used;
        for (j, row) in rows.iter().enumerate() {
            let s = samples(row, pw, h.depth, channels);
            let y = y0 + j * dy;
            for i in 0..pw {
                let x = x0 + i * dx;
                let at = (y * w + x) * channels;
                let part = clamped(&s, i * channels, (i + 1) * channels);
                assign_fixed(&mut buffer, at, at + channels, part);
            }
        }
    }
    Ok(buffer)
}

/// A PNG's samples of a colour type other than paletted as RGBA: grey widened to the colours,
/// alpha 0xFF where the type has none (0 for the colour its tRNS chunk keys out).
fn rgba_of(samples: Vec<u8>, h: &Header, trns: Option<&[u8]>) -> Result<Vec<u8>> {
    let n = h.width * h.height;
    let mut out = vec![0u8; 4 * n];
    match h.colour_type {
        6 => out = samples,
        2 => {
            for k in 0..3 {
                fill_strided(&mut out, k, &strided(&samples, k, 3))?;
            }
            fill_strided(&mut out, 3, &vec![0xFF; n])?;
            if let Some(trns) = trns.filter(|t| t.len() >= 6) {
                let key: Vec<u8> = if h.depth == 16 {
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
            let mut g = strided(&samples, 0, h.channels);
            if h.depth < 8 {
                let scale = 255 / ((1u32 << h.depth) - 1);
                g = g.iter().map(|&v| (v as u32 * scale) as u8).collect();
            }
            for k in 0..3 {
                fill_strided(&mut out, k, &g)?;
            }
            if h.colour_type == 4 {
                fill_strided(&mut out, 3, &strided(&samples, 1, 2))?;
            } else {
                fill_strided(&mut out, 3, &vec![0xFF; n])?;
                if let Some(trns) = trns.filter(|t| t.len() >= 2) {
                    let key =
                        if h.depth >= 8 { trns[1] as u32 } else { trns[1] as u32 * (255 / ((1u32 << h.depth) - 1)) };
                    for i in 0..n {
                        if g[i] as u32 == key {
                            out[4 * i + 3] = 0;
                        }
                    }
                }
            }
        }
    }
    Ok(out)
}

/// A PNG's pixels: indices and its palette for a paletted PNG, else RGBA.
pub fn read_png(png: &[u8]) -> Result<Png> {
    let chunks = chunks(png)?;
    let h = Header::read(chunks.ihdr)?;
    let raw = decompress(&chunks.idat)?;
    // every pixel takes its bits of the image data: a size the data cannot hold is refused before
    // anything is allocated for it
    if h.width as u128 * h.height as u128 * h.bits_per_pixel() as u128 > 8 * raw.len() as u128 {
        bail!("the PNG says it is {} by {} pixels, more than its image data holds", h.width, h.height);
    }
    let samples = all_samples(&raw, &h)?;
    if h.colour_type == 3 {
        let plte = chunks.plte.unwrap_or_default();
        let mut palette: Vec<[u8; 4]> = plte.as_chunks::<3>().0.iter().map(|c| [c[0], c[1], c[2], 255]).collect();
        if let Some(trns) = chunks.trns.filter(|t| !t.is_empty()) {
            for (colour, &a) in palette.iter_mut().zip(trns) {
                colour[3] = a;
            }
        }
        return Ok(Png { width: h.width, height: h.height, paletted: true, data: samples, palette });
    }
    let data = rgba_of(samples, &h, chunks.trns)?;
    Ok(Png { width: h.width, height: h.height, paletted: false, data, palette: Vec::new() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pngs_read_back_what_was_written() {
        let rgba: Vec<u8> = (0..4 * 6 * 3).map(|i| (i * 7) as u8).collect();
        let png = read_png(&write_png(6, 3, PngMode::Rgba, &rgba, &[], &[("Software", "x")]).unwrap()).unwrap();
        assert_eq!((png.width, png.height, png.paletted, png.data.as_slice()), (6, 3, false, rgba.as_slice()));
        let palette = [[1, 2, 3, 255], [4, 5, 6, 0]];
        let png = read_png(&write_png(2, 1, PngMode::P, &[1, 0], &palette, &[]).unwrap()).unwrap();
        assert_eq!((png.data.as_slice(), png.palette.as_slice()), (&[1, 0][..], &palette[..]));
        assert_eq!(png.rgba(), [4, 5, 6, 0, 1, 2, 3, 255]);
        assert!(read_png(b"\x89PNG\r\n\x1a\n").unwrap_err().to_string().contains("no IHDR"));
        assert_eq!(decompress(&compress(b"hello hello hello", 6)).unwrap(), b"hello hello hello");
    }
}
