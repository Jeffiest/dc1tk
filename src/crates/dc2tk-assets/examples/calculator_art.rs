//! The art, model and sounds of the calculator example mod, all made here from code: shapes, a
//! pixel font, seven-segment digits, a low-poly mesh and synthesised tones. Nothing comes from the
//! game's files.
//!
//! ```text
//! cargo run --release -p dc2tk-assets --example calculator_art -- ../docs/scripting/examples/calculator
//! ```
//!
//! writes, under the folder given:
//!
//! - ui/calc_body.png: the calculator's face (256x256);
//! - ui/calc_cursor.png, ui/calc_cursor_wide.png: the highlight frame of a button and of the wide 0;
//! - ui/calc_digits.png: the LCD's seven-segment glyphs, one row of 16x32 cells (0-9, minus, E,
//!   blank);
//! - model/calc.gltf, calc.bin, calc.png: a desk calculator prop (glTF 2.0, Y up, 1 unit = 1 cm);
//! - sounds/*.wav: key, equals, error, clear, open and close (mono, 16-bit, 24000 Hz).
//!
//! The mod's calculator.c places the cursor and digits by the same layout (KEYS, CELL_X and so on
//! below), as constants of its own. Every file is read back with dc2tk-assets' own readers and
//! checked before the program ends.

use std::collections::HashMap;
use std::f64::consts::TAU;
use std::fs;
use std::path::Path;

use dc2tk_assets::from_gltf::{self, Gltf};
use dc2tk_assets::png::{self, PngMode};
use dc2tk_assets::soundbank::{self, Sound};
use serde_json::json;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> Result<()> {
    let Some(out) = std::env::args().nth(1) else {
        return Err("usage: calculator_art <output folder>, e.g. ../docs/scripting/examples/calculator".into());
    };
    let out = Path::new(&out);
    for sub in ["ui", "model", "sounds"] {
        fs::create_dir_all(out.join(sub))?;
    }
    write_ui(&out.join("ui"))?;
    write_model(&out.join("model"))?;
    write_sounds(&out.join("sounds"))?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Drawing: a float RGBA canvas, shapes covered 4x4 per pixel, and a pixel font.

/// Straight (not premultiplied) red, green, blue, alpha in 0..1.
type Rgba = [f32; 4];

fn hex(rgb: u32) -> Rgba {
    let c = |shift: u32| ((rgb >> shift) & 0xFF) as f32 / 255.0;
    [c(16), c(8), c(0), 1.0]
}

fn alpha(c: Rgba, a: f32) -> Rgba {
    [c[0], c[1], c[2], a]
}

fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}

#[derive(Clone, Copy, Debug)]
struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

const fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect { x, y, w, h }
}

impl Rect {
    fn inset(self, d: f32) -> Rect {
        rect(self.x + d, self.y + d, self.w - 2.0 * d, self.h - 2.0 * d)
    }

    fn offset(self, dx: f32, dy: f32) -> Rect {
        rect(self.x + dx, self.y + dy, self.w, self.h)
    }

    /// Whether (px, py) is inside the rectangle with corners rounded to radius r.
    fn has(self, r: f32, px: f32, py: f32) -> bool {
        if px < self.x || py < self.y || px > self.x + self.w || py > self.y + self.h {
            return false;
        }
        let r = r.min(self.w / 2.0).min(self.h / 2.0);
        let cx = px.clamp(self.x + r, self.x + self.w - r);
        let cy = py.clamp(self.y + r, self.y + self.h - r);
        (px - cx).powi(2) + (py - cy).powi(2) <= r * r
    }
}

/// Whether (x, y) is inside a convex polygon (either winding).
fn in_convex(poly: &[(f32, f32)], x: f32, y: f32) -> bool {
    let (mut pos, mut neg) = (false, false);
    for (i, &(ax, ay)) in poly.iter().enumerate() {
        let (bx, by) = poly[(i + 1) % poly.len()];
        let cross = (bx - ax) * (y - ay) - (by - ay) * (x - ax);
        pos |= cross > 0.0;
        neg |= cross < 0.0;
    }
    !(pos && neg)
}

struct Canvas {
    w: usize,
    h: usize,
    px: Vec<Rgba>,
}

impl Canvas {
    fn new(w: usize, h: usize) -> Canvas {
        Canvas { w, h, px: vec![[0.0; 4]; w * h] }
    }

    /// Paints `src` over pixel (x, y), `cover` of it.
    fn blend(&mut self, x: usize, y: usize, src: Rgba, cover: f32) {
        let a = src[3] * cover;
        if a <= 0.0 || x >= self.w || y >= self.h {
            return;
        }
        let d = &mut self.px[y * self.w + x];
        let out_a = a + d[3] * (1.0 - a);
        for i in 0..3 {
            d[i] = (src[i] * a + d[i] * d[3] * (1.0 - a)) / out_a;
        }
        d[3] = out_a;
    }

    /// Paints colour(x, y) over the pixels of `bounds` as much as `inside` covers them (4x4 samples).
    fn fill(&mut self, bounds: Rect, inside: impl Fn(f32, f32) -> bool, colour: impl Fn(f32, f32) -> Rgba) {
        let x0 = bounds.x.floor().max(0.0) as usize;
        let y0 = bounds.y.floor().max(0.0) as usize;
        let x1 = ((bounds.x + bounds.w).ceil().max(0.0) as usize).min(self.w);
        let y1 = ((bounds.y + bounds.h).ceil().max(0.0) as usize).min(self.h);
        for y in y0..y1 {
            for x in x0..x1 {
                let mut n = 0;
                for sy in 0..4 {
                    for sx in 0..4 {
                        if inside(x as f32 + (sx as f32 + 0.5) / 4.0, y as f32 + (sy as f32 + 0.5) / 4.0) {
                            n += 1;
                        }
                    }
                }
                if n > 0 {
                    self.blend(x, y, colour(x as f32 + 0.5, y as f32 + 0.5), n as f32 / 16.0);
                }
            }
        }
    }

    fn round_rect(&mut self, r: Rect, radius: f32, c: Rgba) {
        self.fill(r, |x, y| r.has(radius, x, y), |_, _| c);
    }

    /// A rounded rectangle shaded from `top` to `bottom`.
    fn round_rect_v(&mut self, r: Rect, radius: f32, top: Rgba, bottom: Rgba) {
        self.fill(r, |x, y| r.has(radius, x, y), |_, y| mix(top, bottom, ((y - r.y) / r.h).clamp(0.0, 1.0)));
    }

    /// The band between two rounded rectangles.
    fn ring(&mut self, outer: Rect, outer_r: f32, inner: Rect, inner_r: f32, c: Rgba) {
        self.fill(outer, |x, y| outer.has(outer_r, x, y) && !inner.has(inner_r, x, y), |_, _| c);
    }

    fn polygon(&mut self, poly: &[(f32, f32)], c: Rgba) {
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for &(x, y) in poly {
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
        }
        self.fill(rect(x0, y0, x1 - x0, y1 - y0), |x, y| in_convex(poly, x, y), |_, _| c);
    }

    /// Text in the pixel font, each font pixel a scale x scale block, its top left at (x, y).
    fn text(&mut self, text: &str, x: i32, y: i32, scale: i32, c: Rgba) {
        let mut pen = x;
        for ch in text.chars() {
            let rows = glyph(ch);
            for (gy, row) in rows.iter().enumerate() {
                for (gx, bit) in row.bytes().enumerate() {
                    if bit != b'#' {
                        continue;
                    }
                    for dy in 0..scale {
                        for dx in 0..scale {
                            let px = pen + gx as i32 * scale + dx;
                            let py = y + gy as i32 * scale + dy;
                            if px >= 0 && py >= 0 {
                                self.blend(px as usize, py as usize, c, 1.0);
                            }
                        }
                    }
                }
            }
            pen += (rows[0].len() as i32 + 1) * scale;
        }
    }

    /// 8-bit RGBA, fully transparent pixels all (0, 0, 0, 0).
    fn rgba8(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.px.len() * 4);
        for p in &self.px {
            let a = (p[3].clamp(0.0, 1.0) * 255.0).round() as u8;
            if a == 0 {
                out.extend([0; 4]);
            } else {
                out.extend([p[0], p[1], p[2]].map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8));
                out.push(a);
            }
        }
        out
    }

    /// The canvas at half size, each pixel the alpha-weighted mean of a 2x2 block.
    fn half(&self) -> Canvas {
        let mut out = Canvas::new(self.w / 2, self.h / 2);
        for y in 0..out.h {
            for x in 0..out.w {
                let mut sum = [0.0f32; 4];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let p = self.px[(2 * y + dy) * self.w + 2 * x + dx];
                    for i in 0..3 {
                        sum[i] += p[i] * p[3];
                    }
                    sum[3] += p[3];
                }
                out.px[y * out.w + x] = if sum[3] > 0.0 {
                    [sum[0] / sum[3], sum[1] / sum[3], sum[2] / sum[3], sum[3] / 4.0]
                } else {
                    [0.0; 4]
                };
            }
        }
        out
    }
}

/// The pixel font: 5x7 cells (wider for a few symbols), '#' lit.
fn glyph(ch: char) -> &'static [&'static str] {
    match ch {
        '0' => &[".###.", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."],
        '1' => &["..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###."],
        '2' => &[".###.", "#...#", "....#", "..##.", ".#...", "#....", "#####"],
        '3' => &["####.", "....#", "....#", ".###.", "....#", "....#", "####."],
        '4' => &["...#.", "..##.", ".#.#.", "#..#.", "#####", "...#.", "...#."],
        '5' => &["#####", "#....", "####.", "....#", "....#", "#...#", ".###."],
        '6' => &["..##.", ".#...", "#....", "####.", "#...#", "#...#", ".###."],
        '7' => &["#####", "....#", "...#.", "..#..", ".#...", ".#...", ".#..."],
        '8' => &[".###.", "#...#", "#...#", ".###.", "#...#", "#...#", ".###."],
        '9' => &[".###.", "#...#", "#...#", ".####", "....#", "...#.", ".##.."],
        'A' => &[".###.", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"],
        'C' => &[".###.", "#...#", "#....", "#....", "#....", "#...#", ".###."],
        'L' => &["#....", "#....", "#....", "#....", "#....", "#....", "#####"],
        '+' => &[".....", "..#..", "..#..", "#####", "..#..", "..#..", "....."],
        '\u{2212}' => &[".....", ".....", ".....", "#####", ".....", ".....", "....."],
        '\u{00D7}' => &[".....", "#...#", ".#.#.", "..#..", ".#.#.", "#...#", "....."],
        '\u{00F7}' => &[".....", "..#..", ".....", "#####", ".....", "..#..", "....."],
        '=' => &[".....", ".....", "#####", ".....", "#####", ".....", "....."],
        '\u{00B1}' => &["..#..", "..#..", "#####", "..#..", "..#..", ".....", "#####"],
        // Erase: a tag pointing left with a cross in it.
        '\u{232B}' => &["...######", "..#.....#", ".#..#.#.#", "#....#..#", ".#..#.#.#", "..#.....#", "...######"],
        // Power: a ring broken at the top by a bar.
        '\u{23FB}' => &["...#...", ".#.#.#.", "#..#..#", "#..#..#", "#.....#", ".#...#.", "..###.."],
        _ => &[".....", ".....", ".....", ".....", ".....", ".....", "....."],
    }
}

fn text_width(text: &str, scale: i32) -> i32 {
    let columns: i32 = text.chars().map(|ch| glyph(ch)[0].len() as i32 + 1).sum();
    (columns - 1) * scale
}

// ---------------------------------------------------------------------------------------------
// Colours: to at most 256 by median cut, when a picture has more.

/// The picture with at most `limit` distinct RGBA colours (fully transparent kept exact), and how
/// many it has.
fn quantize(rgba: &[u8], limit: usize) -> (Vec<u8>, usize) {
    let mut counts: HashMap<[u8; 4], u32> = HashMap::new();
    for p in rgba.as_chunks::<4>().0 {
        *counts.entry(*p).or_default() += 1;
    }
    if counts.len() <= limit {
        return (rgba.to_vec(), counts.len());
    }
    let clear = counts.remove(&[0; 4]).is_some();
    let mut colours: Vec<([u8; 4], u32)> = counts.into_iter().collect();
    colours.sort_unstable();
    let mut boxes = vec![colours];
    let target = limit - clear as usize;
    while boxes.len() < target {
        // Split the box with the widest channel (weighted by its pixels) at its weighted median.
        let spread = |b: &Vec<([u8; 4], u32)>| -> (u32, usize) {
            (0..4)
                .map(|ch| {
                    let lo = b.iter().map(|(c, _)| c[ch]).min().unwrap_or(0);
                    let hi = b.iter().map(|(c, _)| c[ch]).max().unwrap_or(0);
                    ((hi - lo) as u32, ch)
                })
                .max()
                .unwrap_or((0, 0))
        };
        let Some((at, (_, ch))) = boxes
            .iter()
            .map(spread)
            .enumerate()
            .filter(|(i, (range, _))| *range > 0 && boxes[*i].len() > 1)
            .max_by_key(|(i, (range, _))| {
                *range as u64 * (boxes[*i].iter().map(|(_, n)| *n as u64).sum::<u64>()).isqrt()
            })
        else {
            break;
        };
        let mut b = boxes.swap_remove(at);
        b.sort_unstable_by_key(|(c, _)| (c[ch], *c));
        let total: u64 = b.iter().map(|(_, n)| *n as u64).sum();
        let (mut seen, mut cut) = (0u64, 1);
        for (i, (_, n)) in b.iter().enumerate() {
            seen += *n as u64;
            if seen * 2 >= total {
                cut = (i + 1).clamp(1, b.len() - 1);
                break;
            }
        }
        let rest = b.split_off(cut);
        boxes.push(b);
        boxes.push(rest);
    }
    let mut map: HashMap<[u8; 4], [u8; 4]> = HashMap::new();
    map.insert([0; 4], [0; 4]);
    for b in &boxes {
        let (mut sum, mut weight, mut alpha_sum, mut n) = ([0f64; 3], 0f64, 0f64, 0f64);
        for (c, k) in b {
            let w = *k as f64 * c[3] as f64;
            for i in 0..3 {
                sum[i] += c[i] as f64 * w;
            }
            weight += w;
            alpha_sum += c[3] as f64 * *k as f64;
            n += *k as f64;
        }
        let a = (alpha_sum / n).round() as u8;
        let mean = if a == 0 {
            [0; 4]
        } else {
            [(sum[0] / weight).round() as u8, (sum[1] / weight).round() as u8, (sum[2] / weight).round() as u8, a]
        };
        for (c, _) in b {
            map.insert(*c, mean);
        }
    }
    let out: Vec<u8> = rgba.as_chunks::<4>().0.iter().flat_map(|p| map[p]).collect();
    let distinct = out.as_chunks::<4>().0.iter().collect::<std::collections::HashSet<_>>().len();
    (out, distinct)
}

/// Writes the canvas as an RGBA PNG of at most 256 colours; returns the colour count.
fn save_png(canvas: &Canvas, path: &Path) -> Result<usize> {
    let (rgba, colours) = quantize(&canvas.rgba8(), 256);
    let png =
        png::write_png(canvas.w, canvas.h, PngMode::Rgba, &rgba, &[], &[("Software", "dc2tk-assets calculator_art")])?;
    fs::write(path, &png)?;
    // Read it back: the size, power-of-two sides and the colour count.
    let back = png::read_png(&fs::read(path)?)?;
    let distinct = back.data.as_chunks::<4>().0.iter().collect::<std::collections::HashSet<_>>().len();
    if back.width != canvas.w || back.height != canvas.h || back.paletted || back.data != rgba || distinct != colours {
        return Err(format!("{} did not read back as written", path.display()).into());
    }
    if !canvas.w.is_power_of_two() || !canvas.h.is_power_of_two() || canvas.w > 256 || canvas.h > 256 || colours > 256 {
        return Err(format!("{}: {}x{}, {colours} colours", path.display(), canvas.w, canvas.h).into());
    }
    println!("{}: {}x{}, {} colours, {} bytes", path.display(), canvas.w, canvas.h, colours, png.len());
    Ok(colours)
}

// ---------------------------------------------------------------------------------------------
// The calculator's face.

const FACE_SIZE: usize = 256;
const BODY: Rect = rect(8.0, 2.0, 240.0, 252.0);
const BODY_RADIUS: f32 = 18.0;
const BEZEL: Rect = rect(20.0, 26.0, 216.0, 54.0);
const LCD: Rect = rect(26.0, 31.0, 204.0, 44.0);
const PLATE: Rect = rect(14.0, 86.0, 228.0, 162.0);
const SOLAR: Rect = rect(178.0, 9.0, 54.0, 12.0);

/// The LCD: cells of the digit atlas, left to right, the number right-aligned in them.
const CELLS: usize = 9;
const CELL_W: usize = 16;
const CELL_H: usize = 32;
const CELL_STEP: usize = 20;
/// The first cell's corner: the row of cells centred in the LCD (40, 37, which calculator.c's
/// place_sprite calls write as numbers).
const CELL_X: usize = LCD.x as usize + (LCD.w as usize - (CELLS * CELL_STEP - (CELL_STEP - CELL_W))) / 2;
const CELL_Y: usize = LCD.y as usize + (LCD.h as usize - CELL_H) / 2;
const _: () = assert!(CELL_X == 40 && CELL_Y == 37, "calculator.c places the digits at (40 + 20 * cell, 37)");

const GRID_X: f32 = 20.0;
const GRID_Y: f32 = 92.0;
const KEY_W: f32 = 48.0;
const KEY_H: f32 = 26.0;
const GAP_X: f32 = 8.0;
const GAP_Y: f32 = 5.0;
/// The cursor frame stands this far outside its button.
const CURSOR_OUT: f32 = 2.0;

#[derive(Clone, Copy)]
enum Style {
    Digit,
    Operator,
    Equals,
    Clear,
    Function,
    Power,
}

/// A key's face, lip (its side), highlight (its top edge) and label colours.
fn style_colours(style: Style) -> [Rgba; 4] {
    let [face, lip, light, label] = match style {
        Style::Digit => [0xE8E0C8, 0xA2997F, 0xFFFAEC, 0x3A332B],
        Style::Operator => [0xE39A3B, 0x9A5C1B, 0xFFCF8A, 0x2E1C0B],
        Style::Equals => [0x3E9E8C, 0x1F5C51, 0x8BDCCB, 0xFFFFFF],
        Style::Clear => [0xD9604A, 0x8C3324, 0xFF9E88, 0xFFFFFF],
        Style::Function => [0x9C978C, 0x5E5A53, 0xCFCAC0, 0x1F1C19],
        Style::Power => [0x4A4643, 0x262321, 0x77716B, 0xFF6A55],
    };
    [hex(face), hex(lip), hex(light), hex(label)]
}

struct Key {
    label: char,
    row: usize,
    col: usize,
    span: usize,
    style: Style,
}

const fn key(label: char, row: usize, col: usize, span: usize, style: Style) -> Key {
    Key { label, row, col, span, style }
}

/// The buttons, row by row (the 0 is two columns wide).
const KEYS: [Key; 19] = [
    key('\u{23FB}', 0, 0, 1, Style::Power),    // close
    key('C', 0, 1, 1, Style::Clear),           // clear
    key('\u{232B}', 0, 2, 1, Style::Clear),    // erase
    key('\u{00F7}', 0, 3, 1, Style::Operator), // div
    key('7', 1, 0, 1, Style::Digit),           // 7
    key('8', 1, 1, 1, Style::Digit),           // 8
    key('9', 1, 2, 1, Style::Digit),           // 9
    key('\u{00D7}', 1, 3, 1, Style::Operator), // mul
    key('4', 2, 0, 1, Style::Digit),           // 4
    key('5', 2, 1, 1, Style::Digit),           // 5
    key('6', 2, 2, 1, Style::Digit),           // 6
    key('\u{2212}', 2, 3, 1, Style::Operator), // sub
    key('1', 3, 0, 1, Style::Digit),           // 1
    key('2', 3, 1, 1, Style::Digit),           // 2
    key('3', 3, 2, 1, Style::Digit),           // 3
    key('+', 3, 3, 1, Style::Operator),        // add
    key('0', 4, 0, 2, Style::Digit),           // 0
    key('\u{00B1}', 4, 2, 1, Style::Function), // neg
    key('=', 4, 3, 1, Style::Equals),          // equals
];

impl Key {
    fn rect(&self) -> Rect {
        let span = self.span as f32;
        rect(
            GRID_X + self.col as f32 * (KEY_W + GAP_X),
            GRID_Y + self.row as f32 * (KEY_H + GAP_Y),
            span * KEY_W + (span - 1.0) * GAP_X,
            KEY_H,
        )
    }

    /// The key's top (its face without the lip below it).
    fn top(&self) -> Rect {
        let r = self.rect();
        rect(r.x, r.y, r.w, r.h - 4.0)
    }
}

/// The LCD's segment colours: lit, and the faint unlit ghost.
fn segment_lit() -> Rgba {
    hex(0xC6F5B8)
}

const SEGMENT_GHOST: f32 = 0.09;

/// Seven-segment masks (bits a..g = 0..6) of the atlas' glyphs, in its order.
const GLYPHS: [(&str, u8); 13] = [
    ("0", 0b0111111),
    ("1", 0b0000110),
    ("2", 0b1011011),
    ("3", 0b1001111),
    ("4", 0b1100110),
    ("5", 0b1101101),
    ("6", 0b1111101),
    ("7", 0b0000111),
    ("8", 0b1111111),
    ("9", 0b1101111),
    ("minus", 0b1000000),
    ("E", 0b1111001),
    ("blank", 0),
];

/// Draws the lit segments of a seven-segment glyph in the CELL_W x CELL_H cell at (ox, oy).
fn seven_segment(canvas: &mut Canvas, ox: f32, oy: f32, mask: u8, c: Rgba) {
    const W: f32 = 12.5;
    const H: f32 = 26.0;
    const HALF: f32 = 1.45; // half a segment's thickness
    const GAP: f32 = 0.55;
    const SLANT: f32 = 2.25;
    let at = |u: f32, v: f32| (ox + 0.75 + u + SLANT * (1.0 - v / H), oy + 3.0 + v);
    let (l, r, t, m, b) = (HALF, W - HALF, HALF, H / 2.0, H - HALF);
    // a, b, c, d, e, f, g as centre lines between the six corners.
    let lines = [
        ((l, t), (r, t)),
        ((r, t), (r, m)),
        ((r, m), (r, b)),
        ((l, b), (r, b)),
        ((l, m), (l, b)),
        ((l, t), (l, m)),
        ((l, m), (r, m)),
    ];
    for (bit, ((u0, v0), (u1, v1))) in lines.into_iter().enumerate() {
        if mask & (1 << bit) == 0 {
            continue;
        }
        let len = ((u1 - u0).powi(2) + (v1 - v0).powi(2)).sqrt();
        let (du, dv) = ((u1 - u0) / len, (v1 - v0) / len);
        let (nu, nv) = (-dv, du);
        let p = |along: f32, side: f32| {
            let (u, v) =
                if along >= 0.0 { (u0 + du * along, v0 + dv * along) } else { (u1 + du * along, v1 + dv * along) };
            at(u + nu * side, v + nv * side)
        };
        let hexagon = [
            p(GAP, 0.0),
            p(GAP + HALF, HALF),
            p(-(GAP + HALF), HALF),
            p(-GAP, 0.0),
            p(-(GAP + HALF), -HALF),
            p(GAP + HALF, -HALF),
        ];
        canvas.polygon(&hexagon, c);
    }
}

/// The face: rounded with transparent corners for the screen, or square and filled to the edges
/// for the model's texture (whose sides use the strips outside the body).
fn draw_face(for_model: bool) -> Canvas {
    let mut c = Canvas::new(FACE_SIZE, FACE_SIZE);
    let radius = if for_model { 0.0 } else { BODY_RADIUS };
    if for_model {
        c.round_rect(rect(0.0, 0.0, 256.0, 256.0), 0.0, side_colour());
        c.round_rect(rect(248.0, 0.0, 8.0, 256.0), 0.0, rubber_colour());
    }
    // The body: a dark rim, a light top edge, a dark bottom edge and a gentle shade.
    c.round_rect(BODY, radius, hex(0x1E1B19));
    let inner = BODY.inset(2.0);
    let inner_r = (radius - 2.0).max(0.0);
    c.round_rect(rect(inner.x, inner.y, inner.w, inner.h - 1.0), inner_r, hex(0x6E685F));
    c.round_rect(rect(inner.x, inner.y + 1.0, inner.w, inner.h - 1.0), inner_r, hex(0x26231F));
    c.round_rect_v(rect(inner.x, inner.y + 1.0, inner.w, inner.h - 2.0), inner_r, hex(0x4A4640), hex(0x37332E));

    // A solar cell strip along the top (the name beside it is drawn with the labels).
    c.round_rect(SOLAR, 2.0, hex(0x1A1716));
    for i in 0..4 {
        let cell = rect(SOLAR.x + 2.0 + i as f32 * 12.5, SOLAR.y + 2.0, 11.5, SOLAR.h - 4.0);
        c.round_rect_v(cell, 0.5, hex(0x5A4038), hex(0x3A2A26));
        c.round_rect(rect(cell.x, cell.y, cell.w, 1.0), 0.0, alpha(hex(0x8A6A5C), 0.8));
    }

    // The display: a dark bezel, the LCD set into it, its top edge in shadow.
    c.round_rect(BEZEL.offset(0.0, 1.0), 7.0, hex(0x7A746A));
    c.round_rect(BEZEL, 7.0, hex(0x161413));
    c.round_rect_v(LCD, 3.0, hex(0x4A5946), hex(0x56664F));
    c.round_rect(rect(LCD.x + 2.0, LCD.y, LCD.w - 4.0, 2.0), 0.0, alpha(hex(0x1E261C), 0.55));
    for i in 0..CELLS {
        seven_segment(
            &mut c,
            (CELL_X + i * CELL_STEP) as f32,
            CELL_Y as f32,
            0x7F,
            alpha(segment_lit(), SEGMENT_GHOST),
        );
    }
    if for_model {
        seven_segment(&mut c, (CELL_X + (CELLS - 1) * CELL_STEP) as f32, CELL_Y as f32, GLYPHS[0].1, segment_lit());
    }

    // The keys' plate, recessed: shadowed at the top, lit at the bottom.
    c.round_rect(PLATE.offset(0.0, 1.0), 9.0, hex(0x58534B));
    c.round_rect(PLATE, 9.0, hex(0x181614));
    c.round_rect_v(rect(PLATE.x, PLATE.y + 1.5, PLATE.w, PLATE.h - 1.5), 8.5, hex(0x2C2926), hex(0x33302C));

    for k in &KEYS {
        draw_key(&mut c, k);
    }
    if !for_model {
        draw_labels(&mut c, 1);
    }
    c
}

/// The name and the keys' labels, crisp in the pixel font on the face drawn at 1/`shrink` size
/// (the model's texture is the face halved, then labelled at 1x).
fn draw_labels(c: &mut Canvas, shrink: i32) {
    let k = shrink as f32;
    let scale = 2 / shrink;
    let (x, y) = (24 / shrink, 11 / shrink);
    c.text("CALC", x, y, 1, hex(0xC9A85A));
    c.text("8", x + text_width("CALC", 1) + 4, y, 1, hex(0x8C8270));
    for key in &KEYS {
        let top = key.top();
        let top = rect(top.x / k, top.y / k, top.w / k, top.h / k);
        let text = key.label.to_string();
        let tw = text_width(&text, scale);
        let x = (top.x + (top.w - tw as f32) / 2.0).round() as i32;
        let y = (top.y + 1.0 / k + (top.h - 1.0 / k - 7.0 * scale as f32) / 2.0).round() as i32;
        c.text(&text, x, y, scale, style_colours(key.style)[3]);
    }
}

fn side_colour() -> Rgba {
    hex(0x403C37)
}

fn rubber_colour() -> Rgba {
    hex(0x1A1817)
}

fn draw_key(c: &mut Canvas, k: &Key) {
    let [face, lip, light, _] = style_colours(k.style);
    let r = k.rect();
    c.round_rect(r.offset(0.0, 2.0), 5.0, alpha(hex(0x000000), 0.5));
    c.round_rect(r, 5.0, lip);
    let top = k.top();
    c.round_rect(top, 5.0, light);
    c.round_rect_v(rect(top.x + 1.0, top.y + 1.0, top.w - 2.0, top.h - 1.0), 4.0, mix(face, light, 0.25), face);
}

/// The highlight frame, standing CURSOR_OUT outside a button of `w` x KEY_H, in a w2 x 32 picture.
fn draw_cursor(button_w: f32, image_w: usize) -> Canvas {
    let mut c = Canvas::new(image_w, 32);
    let frame = rect(0.0, 0.0, button_w + 2.0 * CURSOR_OUT, KEY_H + 2.0 * CURSOR_OUT);
    let bright = hex(0xFFF29A);
    c.round_rect(frame.inset(3.0), 4.0, alpha(bright, 0.18));
    c.ring(frame, 7.0, frame.inset(3.0), 4.0, alpha(bright, 0.88));
    c.ring(frame.inset(1.0), 6.0, frame.inset(2.0), 5.0, alpha(hex(0xFFFFFF), 0.55));
    c
}

fn draw_digits() -> Canvas {
    let mut c = Canvas::new(CELL_W * 16, CELL_H);
    for (i, (_, mask)) in GLYPHS.iter().enumerate() {
        seven_segment(&mut c, (i * CELL_W) as f32, 0.0, *mask, segment_lit());
    }
    c
}

fn write_ui(dir: &Path) -> Result<()> {
    save_png(&draw_face(false), &dir.join("calc_body.png"))?;
    let narrow = KEYS[0].rect().w;
    let wide = KEYS.iter().find(|k| k.span == 2).map_or(narrow, |k| k.rect().w);
    save_png(&draw_cursor(narrow, 64), &dir.join("calc_cursor.png"))?;
    save_png(&draw_cursor(wide, 128), &dir.join("calc_cursor_wide.png"))?;
    save_png(&draw_digits(), &dir.join("calc_digits.png"))?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// The model: a desk calculator, 60 x 20 x 80 units, its front towards +Z, the origin at the
// middle of its underside.

type V3 = [f32; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize(a: V3) -> V3 {
    let len = dot(a, a).sqrt();
    scale(a, 1.0 / len)
}

#[derive(Default)]
struct Mesh {
    positions: Vec<V3>,
    normals: Vec<V3>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u16>,
}

impl Mesh {
    /// A flat convex polygon, fanned into triangles that face `outward` (the order is turned
    /// round if needed).
    fn polygon(&mut self, points: &[V3], uvs: &[[f32; 2]], outward: V3) {
        let mut n = [0.0f32; 3];
        for (i, p) in points.iter().enumerate() {
            let q = points[(i + 1) % points.len()];
            n[0] += (p[1] - q[1]) * (p[2] + q[2]);
            n[1] += (p[2] - q[2]) * (p[0] + q[0]);
            n[2] += (p[0] - q[0]) * (p[1] + q[1]);
        }
        let n = normalize(n);
        let mut order: Vec<usize> = (0..points.len()).collect();
        let normal = if dot(n, outward) < 0.0 {
            order.reverse();
            scale(n, -1.0)
        } else {
            n
        };
        let base = self.positions.len() as u16;
        for &i in &order {
            self.positions.push(points[i]);
            self.normals.push(normal);
            self.uvs.push(uvs[i]);
        }
        for i in 1..order.len() as u16 - 1 {
            self.indices.extend([base, base + i, base + i + 1]);
        }
    }

    fn quad(&mut self, points: [V3; 4], uv: [f32; 2], outward: V3) {
        self.polygon(&points, &[uv; 4], outward);
    }

    /// A box between two corners, without its top (which nobody sees), all one texel.
    fn foot(&mut self, lo: V3, hi: V3, uv: [f32; 2]) {
        let [x0, y0, z0] = lo;
        let [x1, y1, z1] = hi;
        self.quad([[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]], uv, [0.0, -1.0, 0.0]);
        self.quad([[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]], uv, [0.0, 0.0, 1.0]);
        self.quad([[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0]], uv, [0.0, 0.0, -1.0]);
        self.quad([[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]], uv, [-1.0, 0.0, 0.0]);
        self.quad([[x1, y0, z0], [x1, y0, z1], [x1, y1, z1], [x1, y1, z0]], uv, [1.0, 0.0, 0.0]);
    }
}

const HALF_W: f32 = 30.0;
const FRONT_Z: f32 = 40.0;
const BACK_Z: f32 = -40.0;
const FOOT_H: f32 = 1.0;
const FRONT_TOP: f32 = 5.0;
const TOP: f32 = 20.0;
/// Where the slanted face meets the flat strip at the back.
const SLANT_BACK_Z: f32 = -28.0;
const KEY_RAISE: f32 = 1.6;
const KEY_TAPER: f32 = 0.5;
const DISPLAY_RAISE: f32 = 0.5;

/// UV of a texel of the 256x256 face art.
fn uv(px: f32, py: f32) -> [f32; 2] {
    [px / FACE_SIZE as f32, py / FACE_SIZE as f32]
}

/// The point of the slanted face under pixel (px, py) of the face art.
fn on_face(px: f32, py: f32) -> V3 {
    let s = (px - BODY.x) / BODY.w;
    let t = (py - BODY.y) / BODY.h;
    [-HALF_W + 2.0 * HALF_W * s, TOP + (FRONT_TOP - TOP) * t, SLANT_BACK_Z + (FRONT_Z - SLANT_BACK_Z) * t]
}

/// The outward normal of the slanted face.
fn face_normal() -> V3 {
    let along = sub(on_face(BODY.x, BODY.y + BODY.h), on_face(BODY.x, BODY.y));
    normalize([0.0, along[2], -along[1]])
}

/// A raised block on the slanted face over picture rect `r`: its top shows `top_uv` of the art,
/// its sides one texel at `side_uv`.
fn raised(mesh: &mut Mesh, r: Rect, raise: f32, taper: f32, top_uv: Rect, side_uv: [f32; 2]) {
    let n = face_normal();
    let base = [on_face(r.x, r.y), on_face(r.x + r.w, r.y), on_face(r.x + r.w, r.y + r.h), on_face(r.x, r.y + r.h)];
    let centre = scale(base.iter().fold([0.0; 3], |acc, p| add(acc, *p)), 0.25);
    let top: Vec<V3> = base
        .iter()
        .map(|p| {
            let towards = normalize(sub(centre, *p));
            add(add(*p, scale(towards, taper)), scale(n, raise))
        })
        .collect();
    let top_uvs = [
        uv(top_uv.x, top_uv.y),
        uv(top_uv.x + top_uv.w, top_uv.y),
        uv(top_uv.x + top_uv.w, top_uv.y + top_uv.h),
        uv(top_uv.x, top_uv.y + top_uv.h),
    ];
    mesh.polygon(&top, &top_uvs, n);
    for i in 0..4 {
        let j = (i + 1) % 4;
        let mid = scale(add(base[i], base[j]), 0.5);
        mesh.quad([base[i], base[j], top[j], top[i]], side_uv, sub(mid, centre));
    }
}

fn build_mesh() -> Mesh {
    let mut m = Mesh::default();
    let side = uv(4.0, 128.0);
    let rubber = uv(252.0, 128.0);
    let y0 = FOOT_H;
    // The two sides: the profile front bottom, front top, slant top, back top, back bottom.
    for x in [-HALF_W, HALF_W] {
        let profile =
            [[x, y0, FRONT_Z], [x, FRONT_TOP, FRONT_Z], [x, TOP, SLANT_BACK_Z], [x, TOP, BACK_Z], [x, y0, BACK_Z]];
        m.polygon(&profile, &[side; 5], [x, 0.0, 0.0]);
    }
    let (l, r) = (-HALF_W, HALF_W);
    m.quad([[l, y0, BACK_Z], [r, y0, BACK_Z], [r, y0, FRONT_Z], [l, y0, FRONT_Z]], rubber, [0.0, -1.0, 0.0]);
    m.quad(
        [[l, y0, FRONT_Z], [r, y0, FRONT_Z], [r, FRONT_TOP, FRONT_Z], [l, FRONT_TOP, FRONT_Z]],
        side,
        [0.0, 0.0, 1.0],
    );
    m.quad([[l, y0, BACK_Z], [r, y0, BACK_Z], [r, TOP, BACK_Z], [l, TOP, BACK_Z]], side, [0.0, 0.0, -1.0]);
    m.quad([[l, TOP, BACK_Z], [r, TOP, BACK_Z], [r, TOP, SLANT_BACK_Z], [l, TOP, SLANT_BACK_Z]], side, [0.0, 1.0, 0.0]);
    // The slanted face, showing the face art's body.
    let corners =
        [(BODY.x, BODY.y), (BODY.x + BODY.w, BODY.y), (BODY.x + BODY.w, BODY.y + BODY.h), (BODY.x, BODY.y + BODY.h)];
    let points: Vec<V3> = corners.iter().map(|&(x, y)| on_face(x, y)).collect();
    let uvs: Vec<[f32; 2]> = corners.iter().map(|&(x, y)| uv(x, y)).collect();
    m.polygon(&points, &uvs, face_normal());
    // The display stands a little proud of the face, the keys more.
    raised(&mut m, BEZEL, DISPLAY_RAISE, 0.0, BEZEL, uv(BEZEL.x + 3.0, BEZEL.y + 2.0));
    for k in &KEYS {
        let r = k.rect();
        raised(&mut m, r, KEY_RAISE, KEY_TAPER, k.top(), uv(r.x + r.w / 2.0, r.y + r.h - 2.0));
    }
    // Four rubber feet under the corners.
    for (fx, fz) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let (cx, cz) = (fx * (HALF_W - 6.0), fz * (FRONT_Z - 7.0));
        m.foot([cx - 3.0, 0.0, cz - 3.0], [cx + 3.0, FOOT_H, cz + 3.0], rubber);
    }
    m
}

fn write_model(dir: &Path) -> Result<()> {
    let mut texture = draw_face(true).half();
    draw_labels(&mut texture, 2);
    save_png(&texture, &dir.join("calc.png"))?;

    let mesh = build_mesh();
    let mut bin = Vec::new();
    let mut views = Vec::new();
    let mut push = |bin: &mut Vec<u8>, bytes: Vec<u8>, target: u32| {
        while !bin.len().is_multiple_of(4) {
            bin.push(0);
        }
        views.push(json!({"buffer": 0, "byteOffset": bin.len(), "byteLength": bytes.len(), "target": target}));
        bin.extend(bytes);
    };
    let floats = |values: &[f32]| values.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>();
    push(&mut bin, floats(mesh.positions.as_flattened()), 34962);
    push(&mut bin, floats(mesh.normals.as_flattened()), 34962);
    push(&mut bin, floats(mesh.uvs.as_flattened()), 34962);
    push(&mut bin, mesh.indices.iter().flat_map(|i| i.to_le_bytes()).collect(), 34963);
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in &mesh.positions {
        for i in 0..3 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    let count = mesh.positions.len();
    let gltf = json!({
        "asset": {"version": "2.0", "generator": "dc2tk-assets calculator_art example"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"name": "calculator", "mesh": 0}],
        "meshes": [{
            "name": "calculator",
            "primitives": [{
                "attributes": {"POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2},
                "indices": 3,
                "material": 0,
                "mode": 4,
            }],
        }],
        "materials": [{
            "name": "calculator",
            "pbrMetallicRoughness": {"baseColorTexture": {"index": 0}, "metallicFactor": 0.0, "roughnessFactor": 0.7},
        }],
        "textures": [{"sampler": 0, "source": 0}],
        "samplers": [{"magFilter": 9729, "minFilter": 9729, "wrapS": 33071, "wrapT": 33071}],
        "images": [{"uri": "calc.png", "mimeType": "image/png"}],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": count, "type": "VEC3", "min": lo, "max": hi},
            {"bufferView": 1, "componentType": 5126, "count": count, "type": "VEC3"},
            {"bufferView": 2, "componentType": 5126, "count": count, "type": "VEC2"},
            {"bufferView": 3, "componentType": 5123, "count": mesh.indices.len(), "type": "SCALAR"},
        ],
        "bufferViews": views,
        "buffers": [{"uri": "calc.bin", "byteLength": bin.len()}],
    });
    fs::write(dir.join("calc.bin"), &bin)?;
    fs::write(dir.join("calc.gltf"), serde_json::to_string_pretty(&gltf)? + "\n")?;
    check_model(dir)
}

/// Reads the glTF back with dc2tk-assets' reader and checks its mesh.
fn check_model(dir: &Path) -> Result<()> {
    let raw = fs::read(dir.join("calc.gltf"))?;
    let gltf: Gltf = from_gltf::load_gltf(&raw, &|uri| Ok(fs::read(dir.join(uri))?))?;
    let meshes = from_gltf::item(&gltf.json, "meshes")?.as_array().ok_or("meshes")?;
    let primitive = &from_gltf::item(&meshes[0], "primitives")?.as_array().ok_or("primitives")?[0];
    let attributes = from_gltf::item(primitive, "attributes")?;
    let read = |key: &str| -> Result<Vec<f64>> {
        let values = from_gltf::accessor(&gltf, from_gltf::item(attributes, key)?)?;
        Ok(values.values.iter().map(|v| v.f()).collect())
    };
    let positions = read("POSITION")?;
    let normals = read("NORMAL")?;
    let uvs = read("TEXCOORD_0")?;
    let indices: Vec<f64> =
        from_gltf::accessor(&gltf, from_gltf::item(primitive, "indices")?)?.values.iter().map(|v| v.f()).collect();
    let vertices = positions.len() / 3;
    let triangles = indices.len() / 3;
    let ok = normals.len() == vertices * 3
        && uvs.len() == vertices * 2
        && indices.len().is_multiple_of(3)
        && triangles < 500
        && indices.iter().all(|&i| (i as usize) < vertices)
        && normals.chunks(3).all(|n| ((n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt() - 1.0).abs() < 1e-4)
        && uvs.iter().all(|&t| (0.0..=1.0).contains(&t))
        && positions.chunks(3).all(|p| p[0].abs() <= 30.0 && (0.0..=22.0).contains(&p[1]) && p[2].abs() <= 40.0);
    if !ok {
        return Err("model/calc.gltf did not read back as a sound mesh".into());
    }
    // Every triangle's winding agrees with its normals.
    for t in indices.chunks(3) {
        let p = |k: usize| -> [f64; 3] { std::array::from_fn(|i| positions[t[k] as usize * 3 + i]) };
        let (a, b, c) = (p(0), p(1), p(2));
        let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
        let cross = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        let n = &normals[t[0] as usize * 3..t[0] as usize * 3 + 3];
        if cross[0] * n[0] + cross[1] * n[1] + cross[2] * n[2] <= 0.0 {
            return Err("model/calc.gltf has a triangle wound against its normal".into());
        }
    }
    println!("{}: {vertices} vertices, {triangles} triangles", dir.join("calc.gltf").display());
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Sounds: synthesised, kept within about 300 Hz - 4 kHz (low booms were inaudible in game).

/// The rate: a common one among the game's sound banks (VAGs play at 28000, 32000, 44100, 24000,
/// 22050 Hz ...) and an exact half of the SPU2's 48000 Hz mix, so the console resamples simply.
const RATE: f64 = 24000.0;
/// -3 dBFS.
const PEAK: f64 = 32767.0 * 0.707_946;

fn render(ms: f64, f: impl Fn(f64) -> f64) -> Vec<f64> {
    let n = (RATE * ms / 1000.0).round() as usize;
    (0..n).map(|i| f(i as f64 / RATE)).collect()
}

/// Sample-by-sample render of a tone whose frequency changes (the phase is integrated).
fn glide(ms: f64, freq: impl Fn(f64) -> f64, wave: impl Fn(f64, f64) -> f64) -> Vec<f64> {
    let n = (RATE * ms / 1000.0).round() as usize;
    let mut phase = 0.0;
    (0..n)
        .map(|i| {
            let t = i as f64 / RATE;
            let out = wave(phase, t);
            phase += TAU * freq(t) / RATE;
            out
        })
        .collect()
}

/// Smooth fades at both ends (raised cosine), the peak at -3 dBFS, as 16-bit samples.
fn finish(mut s: Vec<f64>, fade_in_ms: f64, fade_out_ms: f64) -> Sound {
    let n = s.len();
    let fade_in = (RATE * fade_in_ms / 1000.0) as usize;
    let fade_out = (RATE * fade_out_ms / 1000.0) as usize;
    let ramp = |i: usize, len: usize| 0.5 - 0.5 * (std::f64::consts::PI * i as f64 / len as f64).cos();
    for (i, v) in s.iter_mut().enumerate().take(fade_in.min(n)) {
        *v *= ramp(i, fade_in);
    }
    for (i, v) in s.iter_mut().rev().enumerate().take(fade_out.min(n)) {
        *v *= ramp(i, fade_out);
    }
    let peak = s.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    let samples = s.iter().map(|v| (v / peak * PEAK).round() as i64).collect();
    Sound { rate: RATE as i64, samples, loop_start: None }
}

/// A band-pass biquad (RBJ, constant 0 dB peak gain).
fn band_pass(input: &[f64], centre: f64, q: f64) -> Vec<f64> {
    let w = TAU * centre / RATE;
    let alpha = w.sin() / (2.0 * q);
    let a0 = 1.0 + alpha;
    let (b0, b2) = (alpha / a0, -alpha / a0);
    let (a1, a2) = (-2.0 * w.cos() / a0, (1.0 - alpha) / a0);
    let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
    input
        .iter()
        .map(|&x| {
            let y = b0 * x + b2 * x2 - a1 * y1 - a2 * y2;
            (x2, x1, y2, y1) = (x1, x, y1, y);
            y
        })
        .collect()
}

/// Repeatable white noise in -1..1 (xorshift).
fn noise(n: usize, mut seed: u64) -> Vec<f64> {
    (0..n)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 11) as f64 / (1u64 << 52) as f64 - 1.0
        })
        .collect()
}

/// A bell-like note starting at `start`: quick attack, exponential decay, a soft octave above.
fn note(t: f64, start: f64, freq: f64, decay: f64) -> f64 {
    let t = t - start;
    if t < 0.0 {
        return 0.0;
    }
    let env = (1.0 - (-t / 0.002).exp()) * (-t / decay).exp();
    env * ((TAU * freq * t).sin() + 0.22 * (TAU * 2.0 * freq * t).sin())
}

/// A band-limited sawtooth (harmonics up to 4 kHz) at phase p.
fn buzz(p: f64, freq: f64) -> f64 {
    let harmonics = (4000.0 / freq) as usize;
    (1..=harmonics).map(|k| (k as f64 * p).sin() / k as f64).sum()
}

/// A gate that opens at `on` and closes at `off`, with 6 ms ramps.
fn gate(t: f64, on: f64, off: f64) -> f64 {
    let ramp = 0.006;
    ((t - on) / ramp).clamp(0.0, 1.0) * ((off - t) / ramp).clamp(0.0, 1.0)
}

fn write_sounds(dir: &Path) -> Result<()> {
    // A soft click: a short knock of two damped partials and a breath of band-passed noise.
    let tick = render(40.0, |t| (-t / 0.007).exp() * (0.7 * (TAU * 1750.0 * t).sin() + 0.3 * (TAU * 2900.0 * t).sin()));
    let hiss = band_pass(&noise(tick.len(), 0x9E37_79B9_7F4A_7C15), 2600.0, 1.2);
    let key: Vec<f64> = tick
        .iter()
        .zip(&hiss)
        .enumerate()
        .map(|(i, (a, b))| a + 0.8 * b * (-(i as f64 / RATE) / 0.004).exp())
        .collect();

    // A rising fifth, C6 then G6.
    let equals = render(250.0, |t| 0.8 * note(t, 0.0, 1046.5, 0.09) + note(t, 0.075, 1568.0, 0.08));

    // Two low, rough buzzes, a semitone apart so they grate.
    let error = render(300.0, |t| {
        let p1 = TAU * 330.0 * t;
        let p2 = TAU * 349.2 * t;
        let rough = 1.0 - 0.25 * (TAU * 28.0 * t).sin().abs();
        (gate(t, 0.0, 0.13) + gate(t, 0.16, 0.295)) * rough * (buzz(p1, 330.0) + 0.6 * buzz(p2, 349.2))
    });

    // A quick swoop from 2.2 kHz down to 500 Hz.
    let clear = glide(
        150.0,
        |t| 2200.0 * (500.0f64 / 2200.0).powf(t / 0.15),
        |p, t| (-t / 0.12).exp() * (p.sin() + 0.3 * (2.0 * p).sin()),
    );

    // Blips: 660 Hz up to 1320 Hz and back down.
    let blip = |from: f64, to: f64| {
        glide(
            110.0,
            move |t| from * (to / from).powf((t / 0.09).min(1.0)),
            |p, t| (-t / 0.07).exp() * (p.sin() + 0.18 * (3.0 * p).sin()),
        )
    };
    let open = blip(660.0, 1320.0);
    let close = blip(1320.0, 660.0);

    for (name, samples, fade_in, fade_out) in [
        ("key", key, 1.5, 6.0),
        ("equals", equals, 2.0, 25.0),
        ("error", error, 3.0, 10.0),
        ("clear", clear, 2.0, 12.0),
        ("open", open, 2.0, 15.0),
        ("close", close, 2.0, 15.0),
    ] {
        let sound = finish(samples, fade_in, fade_out);
        let path = dir.join(format!("{name}.wav"));
        let wav = soundbank::wav_bytes(&sound)?;
        fs::write(&path, &wav)?;
        let back = soundbank::read_wav(&fs::read(&path)?)?;
        if back != sound {
            return Err(format!("{} did not read back as written", path.display()).into());
        }
        let peak = sound.samples.iter().map(|s| s.abs()).max().unwrap_or(0);
        println!(
            "{}: {} ms, peak {:.1} dBFS, ends {} / {}",
            path.display(),
            sound.samples.len() as f64 * 1000.0 / RATE,
            20.0 * (peak as f64 / 32767.0).log10(),
            sound.samples[0],
            sound.samples[sound.samples.len() - 1],
        );
    }
    Ok(())
}
