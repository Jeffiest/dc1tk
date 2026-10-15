//! Colour quantisation: the palette of a paletted texture a PNG of more colours is made into
//! (median cut over the picture's colours, alpha among them), and each pixel's nearest colour in
//! it.

use std::collections::HashMap;

/// A colour (r, g, b, a).
pub type Colour = [u8; 4];

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
                *slot = (weighted as f64 / total as f64).round_ties_even() as u8;
            }
            colour
        })
        .collect()
}

/// The index of the nearest palette colour (alpha weighs double), the first of equals; cached.
pub(crate) struct Nearest<'a> {
    palette: &'a [Colour],
    cache: HashMap<Colour, u8>,
}

impl<'a> Nearest<'a> {
    pub(crate) fn new(palette: &'a [Colour]) -> Nearest<'a> {
        Nearest { palette, cache: HashMap::new() }
    }

    pub(crate) fn find(&mut self, c: Colour) -> u8 {
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

/// RGBA (PS2 alpha) to indices and a palette of n entries: the picture's own colours when it has
/// no more (sorted), else n by median cut (over colours of 5 bits a channel when it has more than
/// 32768), the palette padded with transparent black.
pub fn quantise(rgba: &[u8], n: usize) -> (Vec<u8>, Vec<Colour>) {
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
