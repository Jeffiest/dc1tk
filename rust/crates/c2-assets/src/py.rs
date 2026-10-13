//! Arithmetic as CPython does it, where the converters' output depends on it: sum() of floats
//! (compensated since 3.12), round() (half to even), min()/max() (first wins), // and % (floor),
//! struct.pack('<f') and a number that is an int or a float.

use std::ops::{Add, Mul, Sub};

/// sum() of floats: the start value is the int 0, so the first float is added to 0.0 plainly,
/// then Neumaier's compensated summation, the compensation added at the end when finite.
pub fn sum<I: IntoIterator<Item = f64>>(items: I) -> f64 {
    let mut items = items.into_iter();
    let Some(first) = items.next() else {
        return 0.0;
    };
    let mut total = 0.0 + first;
    let mut compensation = 0.0;
    for x in items {
        let t = total + x;
        if total.abs() >= x.abs() {
            compensation += (total - t) + x;
        } else {
            compensation += (x - t) + total;
        }
        total = t;
    }
    if compensation != 0.0 && compensation.is_finite() {
        total += compensation;
    }
    total
}

/// A Python number of the kinds the converters meet: an int (exact) or a float.
#[derive(Clone, Copy, Debug)]
pub enum Num {
    Int(i64),
    Float(f64),
}

impl Num {
    pub fn f(self) -> f64 {
        match self {
            Num::Int(value) => value as f64,
            Num::Float(value) => value,
        }
    }

    pub fn is_int(self) -> bool {
        matches!(self, Num::Int(_))
    }
}

impl Mul for Num {
    type Output = Num;
    fn mul(self, other: Num) -> Num {
        match (self, other) {
            (Num::Int(a), Num::Int(b)) => Num::Int(a * b),
            (a, b) => Num::Float(a.f() * b.f()),
        }
    }
}

impl Add for Num {
    type Output = Num;
    fn add(self, other: Num) -> Num {
        match (self, other) {
            (Num::Int(a), Num::Int(b)) => Num::Int(a + b),
            (a, b) => Num::Float(a.f() + b.f()),
        }
    }
}

impl Sub for Num {
    type Output = Num;
    fn sub(self, other: Num) -> Num {
        match (self, other) {
            (Num::Int(a), Num::Int(b)) => Num::Int(a - b),
            (a, b) => Num::Float(a.f() - b.f()),
        }
    }
}

/// sum() of ints and floats: ints add exactly until the first float, which is added plainly;
/// floats after it are summed with compensation, ints plainly.
pub fn sum_nums<I: IntoIterator<Item = Num>>(items: I) -> Num {
    let mut items = items.into_iter();
    let mut whole = 0i64;
    let mut total;
    loop {
        match items.next() {
            None => return Num::Int(whole),
            Some(Num::Int(value)) => whole += value,
            Some(Num::Float(value)) => {
                total = whole as f64 + value;
                break;
            }
        }
    }
    let mut compensation = 0.0;
    for item in items {
        match item {
            Num::Float(x) => {
                let t = total + x;
                if total.abs() >= x.abs() {
                    compensation += (total - t) + x;
                } else {
                    compensation += (x - t) + total;
                }
                total = t;
            }
            Num::Int(value) => total += value as f64,
        }
    }
    if compensation != 0.0 && compensation.is_finite() {
        total += compensation;
    }
    Num::Float(total)
}

/// round(x): to the nearest whole number, halves to even.
pub fn round(x: f64) -> f64 {
    x.round_ties_even()
}

/// int(round(x)) for a value that fits.
pub fn round_int(x: f64) -> i64 {
    x.round_ties_even() as i64
}

/// round(x, digits): the decimal correctly rounded (halves to even), read back.
pub fn round_to(x: f64, digits: usize) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let rounded: f64 = format!("{x:.digits$}").parse().expect("a formatted float parses");
    if rounded == 0.0 {
        // round keeps the sign of a value that rounds to zero
        return if x.is_sign_negative() { -0.0 } else { 0.0 };
    }
    rounded
}

/// min(a, b): b only when it is smaller.
pub fn min(a: f64, b: f64) -> f64 {
    if b < a {
        b
    } else {
        a
    }
}

/// max(a, b): b only when it is larger.
pub fn max(a: f64, b: f64) -> f64 {
    if b > a {
        b
    } else {
        a
    }
}

/// min() of a non-empty sequence: the first of the smallest.
pub fn min_of<I: IntoIterator<Item = f64>>(items: I) -> f64 {
    let mut items = items.into_iter();
    let first = items.next().expect("min() of an empty sequence");
    items.fold(first, min)
}

/// max() of a non-empty sequence: the first of the largest.
pub fn max_of<I: IntoIterator<Item = f64>>(items: I) -> f64 {
    let mut items = items.into_iter();
    let first = items.next().expect("max() of an empty sequence");
    items.fold(first, max)
}

/// a // b.
pub fn floor_div(a: i64, b: i64) -> i64 {
    let q = a / b;
    if a % b != 0 && ((a < 0) != (b < 0)) {
        q - 1
    } else {
        q
    }
}

/// a % b, with the sign of b.
pub fn modulo(a: i64, b: i64) -> i64 {
    a - floor_div(a, b) * b
}

/// struct.pack('<f', x): rounded to the nearest float; a finite value too large is an error.
pub fn pack_f32(x: f64) -> crate::Result<[u8; 4]> {
    let y = x as f32;
    if y.is_infinite() && !x.is_infinite() {
        crate::bail!("float too large to pack with f format");
    }
    Ok(y.to_le_bytes())
}

/// int.bit_length().
pub fn bit_length(value: i64) -> u32 {
    64 - value.unsigned_abs().leading_zeros()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sums_like_python() {
        assert_eq!(sum([0.1, 0.2, 0.3]), 0.6);
        assert_eq!(sum([-0.0]).to_bits(), 0.0f64.to_bits());
        assert_eq!(sum_nums([Num::Float(1e100), Num::Int(1), Num::Float(-1e100)]).f(), 0.0);
        assert_eq!(sum_nums([Num::Float(1e100), Num::Float(1.0), Num::Float(-1e100)]).f(), 1.0);
        assert!(sum_nums([Num::Int(1), Num::Int(2)]).is_int());
    }

    #[test]
    fn rounds_like_python() {
        assert_eq!(round_to(0.03125, 4), 0.0312);
        assert_eq!(round_to(2.675, 2), 2.67);
        assert_eq!(round(2.5), 2.0);
        assert_eq!(round(-0.5).to_bits(), (-0.0f64).to_bits());
        assert_eq!(floor_div(-7, 2), -4);
        assert_eq!(modulo(-7, 16), 9);
        assert_eq!(bit_length(-1), 1);
        assert_eq!(bit_length(255), 8);
    }
}
