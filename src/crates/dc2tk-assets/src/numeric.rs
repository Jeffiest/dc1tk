//! The arithmetic the converters' bytes depend on, kept exactly as the converters have always done
//! it, since a change in the last bit of a weight or a sample changes the file:
//! - a number read from JSON stays an int or a float (Num), and a sum of them adds the ints
//!   exactly;
//! - floats are summed with Neumaier's compensation, so a long sum (a skin's weights, a frame of
//!   samples, a matrix product's terms) keeps the bits of its exact value where it can;
//! - min and max keep the first of two equal values, and a NaN given first;
//! - narrowing a double to a 32-bit float rounds to nearest, refuses a finite value too large, and
//!   keeps the top of a NaN's payload, quiet, as the CPU's conversion does.
//!
//! The glTF export's matrix square roots, arc cosines and divisions are checked, so a degenerate
//! matrix is an error rather than NaNs in the file.

use std::ops::{Add, Mul, Sub};

/// A number as JSON gives it: an int (exact) or a float.
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

    /// The number a JSON value holds: an int when the JSON text is one, else a float.
    pub fn of(value: &serde_json::Value) -> Option<Num> {
        let number = value.as_number()?;
        match number.as_i64() {
            Some(int) => Some(Num::Int(int)),
            None => number.as_f64().map(Num::Float),
        }
    }

    /// The number as JSON: an int stays one, a float is written with a point.
    pub fn json(self) -> serde_json::Value {
        match self {
            Num::Int(value) => value.into(),
            Num::Float(value) => value.into(),
        }
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

/// One step of Neumaier's summation: the new total, with what it lost added to the compensation.
fn neumaier(total: f64, x: f64, compensation: &mut f64) -> f64 {
    let t = total + x;
    *compensation += if total.abs() >= x.abs() { (total - t) + x } else { (x - t) + total };
    t
}

/// The compensation added to a total at the end, unless it is zero or not finite.
fn compensated(total: f64, compensation: f64) -> f64 {
    if compensation != 0.0 && compensation.is_finite() {
        total + compensation
    } else {
        total
    }
}

/// The sum of floats by Neumaier's compensated summation: the first added to 0.0 plainly, each
/// next one with its rounding error kept apart, and those errors added once at the end.
pub fn compensated_sum(items: impl IntoIterator<Item = f64>) -> f64 {
    let mut items = items.into_iter();
    let Some(first) = items.next() else {
        return 0.0;
    };
    let mut total = 0.0 + first;
    let mut compensation = 0.0;
    for x in items {
        total = neumaier(total, x, &mut compensation);
    }
    compensated(total, compensation)
}

/// The sum of numbers: ints add exactly, and the sum is an int, until the first float, which is
/// added to their total plainly; after it floats are summed with compensation (compensated_sum)
/// and ints added plainly.
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
            Num::Float(x) => total = neumaier(total, x, &mut compensation),
            Num::Int(value) => total += value as f64,
        }
    }
    Num::Float(compensated(total, compensation))
}

/// x to `digits` decimal places: the nearest such decimal to x's exact value (halves to even),
/// read back as a double. A value that rounds to zero keeps its sign.
pub fn round_decimal(x: f64, digits: usize) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let rounded: f64 = format!("{x:.digits$}").parse().expect("a formatted float parses");
    if rounded == 0.0 {
        return if x.is_sign_negative() { -0.0 } else { 0.0 };
    }
    rounded
}

/// The smaller of two values; a, when b is not smaller (a NaN in either place, or two zeros).
pub fn min(a: f64, b: f64) -> f64 {
    if b < a {
        b
    } else {
        a
    }
}

/// The larger of two values; a, when b is not larger (a NaN in either place, or two zeros).
pub fn max(a: f64, b: f64) -> f64 {
    if b > a {
        b
    } else {
        a
    }
}

/// The smallest of a non-empty sequence, the first of equals (min).
pub fn min_of<I: IntoIterator<Item = f64>>(items: I) -> f64 {
    let mut items = items.into_iter();
    let first = items.next().expect("min_of a non-empty sequence");
    items.fold(first, min)
}

/// The largest of a non-empty sequence, the first of equals (max).
pub fn max_of<I: IntoIterator<Item = f64>>(items: I) -> f64 {
    let mut items = items.into_iter();
    let first = items.next().expect("max_of a non-empty sequence");
    items.fold(first, max)
}

/// A double as the little-endian bytes of the nearest 32-bit float; a finite value too large for
/// one is an error. A NaN keeps the top of its payload and is quiet, as the CPU's conversion
/// leaves it.
pub fn f32_bytes(x: f64) -> crate::Result<[u8; 4]> {
    if x.is_nan() {
        let bits = x.to_bits();
        let narrow = ((bits >> 63) as u32) << 31 | 0x7FC0_0000 | ((bits >> 29) as u32 & 0x007F_FFFF);
        return Ok(narrow.to_le_bytes());
    }
    let y = x as f32;
    if y.is_infinite() && !x.is_infinite() {
        crate::bail!("{x} is too large for a 32-bit float");
    }
    Ok(y.to_le_bytes())
}

/// A 32-bit float's bits as a double; a signalling NaN comes back quiet, as the CPU's conversion
/// makes it.
pub fn f32_value(bits: u32) -> f64 {
    let x = f32::from_bits(bits);
    if x.is_nan() {
        let quiet = bits | 0x0040_0000;
        return f64::from_bits(
            ((quiet >> 31) as u64) << 63 | 0x7FF0_0000_0000_0000 | ((quiet & 0x007F_FFFF) as u64) << 29,
        );
    }
    x as f64
}

/// The square root; of a negative value, an error.
pub fn sqrt(x: f64) -> crate::Result<f64> {
    if x < 0.0 {
        crate::bail!("cannot take the square root of {x}");
    }
    Ok(x.sqrt())
}

/// The arc cosine; outside -1 to 1, an error.
pub fn acos(x: f64) -> crate::Result<f64> {
    if !(-1.0..=1.0).contains(&x) && !x.is_nan() {
        crate::bail!("cannot take the arc cosine of {x}");
    }
    Ok(x.acos())
}

/// a / b; by zero, an error.
pub fn div(a: f64, b: f64) -> crate::Result<f64> {
    if b == 0.0 {
        crate::bail!("cannot divide {a} by zero");
    }
    Ok(a / b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sums_keep_ints_exact_and_compensate_floats() {
        assert_eq!(sum_nums([Num::Float(1e100), Num::Int(1), Num::Float(-1e100)]).f(), 0.0);
        assert_eq!(sum_nums([Num::Float(1e100), Num::Float(1.0), Num::Float(-1e100)]).f(), 1.0);
        assert!(sum_nums([Num::Int(1), Num::Int(2)]).is_int());
        assert_eq!(compensated_sum([0.1; 10]), 1.0);
        assert_eq!(compensated_sum([]), 0.0);
    }

    #[test]
    fn decimals_round_half_to_even_on_the_exact_value() {
        assert_eq!(round_decimal(0.03125, 4), 0.0312);
        // 2.675 is a little below 2.675 as a double
        assert_eq!(round_decimal(2.675, 2), 2.67);
        assert_eq!(round_decimal(-0.00001, 2).to_bits(), (-0.0f64).to_bits());
    }

    #[test]
    fn min_and_max_keep_the_first_of_equals() {
        assert_eq!(min(0.0, -0.0).to_bits(), 0.0f64.to_bits());
        assert_eq!(max(-0.0, 0.0).to_bits(), (-0.0f64).to_bits());
        assert!(min(f64::NAN, 1.0).is_nan());
        assert_eq!(min_of([3.0, 1.0, 2.0]), 1.0);
    }

    #[test]
    fn nans_pass_through_floats_as_the_cpu_converts_them() {
        let x = f32_value(0x7f80_0001);
        assert_eq!(x.to_bits(), 0x7ff8_0000_2000_0000);
        assert_eq!(f32_bytes(x).unwrap(), 0x7fc0_0001u32.to_le_bytes());
        assert!(f32_bytes(1e300).is_err());
    }

    #[test]
    fn json_numbers_stay_ints_or_floats() {
        let value: serde_json::Value = serde_json::from_str("[1, 1.0, -2, 2.5e-3]").unwrap();
        let numbers: Vec<Num> = value.as_array().unwrap().iter().map(|item| Num::of(item).unwrap()).collect();
        assert!(numbers[0].is_int() && !numbers[1].is_int() && numbers[2].is_int() && !numbers[3].is_int());
        assert_eq!(Num::Float(1.0).json().to_string(), "1.0");
        assert_eq!(Num::Int(1).json().to_string(), "1");
    }
}
