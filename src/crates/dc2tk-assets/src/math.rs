//! The 3D maths of the model and motion converters: 4x4 matrices stored column-major, as glTF and
//! the MDS hold them (a `Mat4`; map_transform's `RowMatrix` is the .map's row-major form), points
//! and directions, and quaternions (x, y, z, w). Their results reach the disc's bytes (an imported
//! bind pose, a rebuilt motion), so each keeps its order of operations and its sums: a change here
//! is a change to what a build writes.

use crate::numeric;
use crate::Result;

/// A 4x4 matrix stored column-major: element (row r, column c) at `c * 4 + r`, the translation
/// at 12..15.
pub(crate) type Mat4 = [f64; 16];

/// A quaternion (x, y, z, w).
pub(crate) type Quat = [f64; 4];

pub(crate) const IDENT: Mat4 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

/// The quaternion of no rotation.
pub(crate) const NO_ROTATION: Quat = [0.0, 0.0, 0.0, 1.0];

/// The direction a normal of no length is given.
pub(crate) const Z: [f64; 3] = [0.0, 0.0, 1.0];

/// a·b, each element's terms summed with compensation (numeric::compensated_sum), which the
/// imported bind poses' bytes depend on.
pub(crate) fn mat_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            out[c * 4 + r] = numeric::compensated_sum((0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]));
        }
    }
    out
}

/// The inverse by Gauss-Jordan elimination, or None when singular.
pub(crate) fn mat_inv(m: &Mat4) -> Option<Mat4> {
    let mut a: Vec<[f64; 8]> = (0..4)
        .map(|r| {
            let mut row = [0.0; 8];
            for c in 0..4 {
                row[c] = m[c * 4 + r];
                row[4 + c] = if r == c { 1.0 } else { 0.0 };
            }
            row
        })
        .collect();
    for col in 0..4 {
        // the pivot: the first of the rows with the largest magnitude in this column
        let mut pivot = col;
        for r in col + 1..4 {
            if a[r][col].abs() > a[pivot][col].abs() {
                pivot = r;
            }
        }
        if a[pivot][col].abs() < 1e-12 {
            return None;
        }
        a.swap(col, pivot);
        let p = a[col][col];
        for x in &mut a[col] {
            *x /= p;
        }
        for r in 0..4 {
            if r != col && a[r][col] != 0.0 {
                let f = a[r][col];
                let pivot_row = a[col];
                for (x, y) in a[r].iter_mut().zip(pivot_row) {
                    *x -= f * y;
                }
            }
        }
    }
    let mut out = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            out[c * 4 + r] = a[r][4 + c];
        }
    }
    Some(out)
}

/// The point v (its first three numbers) moved by m.
pub(crate) fn xform_point(m: &Mat4, v: &[f64]) -> [f64; 3] {
    [0, 1, 2].map(|r| m[r] * v[0] + m[4 + r] * v[1] + m[8 + r] * v[2] + m[12 + r])
}

/// The direction v (its first three numbers) turned by m, without its translation.
pub(crate) fn xform_dir(m: &Mat4, v: &[f64]) -> [f64; 3] {
    [0, 1, 2].map(|r| m[r] * v[0] + m[4 + r] * v[1] + m[8 + r] * v[2])
}

/// The vector divided by its length (its squares summed with compensation), or the fallback when
/// it has none.
pub(crate) fn normalize<const N: usize>(v: &[f64; N], fallback: [f64; N]) -> [f64; N] {
    let length = numeric::compensated_sum(v.iter().map(|x| x * x)).sqrt();
    if length > 1e-12 {
        v.map(|x| x / length)
    } else {
        fallback
    }
}

/// The quaternion of the rotation whose columns are c0, c1, c2.
fn quat_from_rot(c0: &[f64], c1: &[f64], c2: &[f64]) -> Result<Quat> {
    let (m00, m10, m20) = (c0[0], c0[1], c0[2]);
    let (m01, m11, m21) = (c1[0], c1[1], c1[2]);
    let (m02, m12, m22) = (c2[0], c2[1], c2[2]);
    let tr = m00 + m11 + m22;
    let q = if tr > 0.0 {
        let s = numeric::sqrt(tr + 1.0)? * 2.0;
        [numeric::div(m21 - m12, s)?, numeric::div(m02 - m20, s)?, numeric::div(m10 - m01, s)?, 0.25 * s]
    } else if m00 > m11 && m00 > m22 {
        let s = numeric::sqrt(1.0 + m00 - m11 - m22)? * 2.0;
        [0.25 * s, numeric::div(m01 + m10, s)?, numeric::div(m02 + m20, s)?, numeric::div(m21 - m12, s)?]
    } else if m11 > m22 {
        let s = numeric::sqrt(1.0 + m11 - m00 - m22)? * 2.0;
        [numeric::div(m01 + m10, s)?, 0.25 * s, numeric::div(m12 + m21, s)?, numeric::div(m02 - m20, s)?]
    } else {
        let s = numeric::sqrt(1.0 + m22 - m00 - m11)? * 2.0;
        [numeric::div(m02 + m20, s)?, numeric::div(m12 + m21, s)?, 0.25 * s, numeric::div(m10 - m01, s)?]
    };
    Ok(normalize(&q, NO_ROTATION))
}

/// The translation, rotation and scale of a matrix (a negative determinant flips the x scale).
pub(crate) fn decompose(m: &Mat4) -> Result<([f64; 3], Quat, [f64; 3])> {
    let t = [m[12], m[13], m[14]];
    let cols = [&m[0..3], &m[4..7], &m[8..11]];
    let mut s = cols.map(|c| numeric::compensated_sum(c.iter().map(|x| x * x)).sqrt());
    let det = cols[0][0] * (cols[1][1] * cols[2][2] - cols[1][2] * cols[2][1])
        - cols[1][0] * (cols[0][1] * cols[2][2] - cols[0][2] * cols[2][1])
        + cols[2][0] * (cols[0][1] * cols[1][2] - cols[0][2] * cols[1][1]);
    if det < 0.0 {
        s[0] = -s[0];
    }
    let rc: [[f64; 3]; 3] = std::array::from_fn(|i| {
        let d = if s[i] == 0.0 { 1.0 } else { s[i] };
        [cols[i][0] / d, cols[i][1] / d, cols[i][2] / d]
    });
    Ok((t, quat_from_rot(&rc[0], &rc[1], &rc[2])?, s))
}

/// The matrix of a translation, rotation and scale.
pub(crate) fn compose(t: &[f64; 3], r: &Quat, s: &[f64; 3]) -> Mat4 {
    let [x, y, z, w] = *r;
    let columns = [
        [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y + z * w), 2.0 * (x * z - y * w)],
        [2.0 * (x * y - z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z + x * w)],
        [2.0 * (x * z + y * w), 2.0 * (y * z - x * w), 1.0 - 2.0 * (x * x + y * y)],
    ];
    let mut m = IDENT;
    for (c, column) in columns.iter().enumerate() {
        for row in 0..3 {
            m[c * 4 + row] = column[row] * s[c];
        }
    }
    m[12..15].copy_from_slice(t);
    m
}

/// How two values of a curve mix between their keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mixing {
    /// Rotations (quaternions): along the arc between them (slerp), kept of unit length.
    Spherical,
    /// Anything else: in a straight line.
    Straight,
}

/// The rotation t of the way from a to b, along the shorter arc (b negated when they are more
/// than half a turn apart); a straight mix, normalised, when they are within 0.9995 of each other.
/// A mix of no length (two quaternions of none) is no rotation.
pub(crate) fn slerp(a: &Quat, b: &Quat, t: f64) -> Result<Quat> {
    let mut b = *b;
    let mut d = numeric::compensated_sum(a.iter().zip(&b).map(|(x, y)| x * y));
    if d < 0.0 {
        b = b.map(|x| -x);
        d = -d;
    }
    if d > 0.9995 {
        let mixed: Quat = std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t);
        return Ok(normalize(&mixed, NO_ROTATION));
    }
    let th = numeric::acos(numeric::min(1.0, d))?;
    let s = th.sin();
    let (fa, fb) = (numeric::div(((1.0 - t) * th).sin(), s)?, numeric::div((t * th).sin(), s)?);
    let mixed: Quat = std::array::from_fn(|i| fa * a[i] + fb * b[i]);
    Ok(normalize(&mixed, NO_ROTATION))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_matrix_composed_decomposes_and_inverts_back() {
        let r = normalize(&[0.1, 0.7, -0.2, 0.6], NO_ROTATION);
        let m = compose(&[1.0, -2.0, 3.0], &r, &[2.0, 0.5, 1.5]);
        let (t, q, s) = decompose(&m).unwrap();
        assert_eq!(t, [1.0, -2.0, 3.0]);
        assert!((0..4).all(|i| (q[i] - r[i]).abs() < 1e-12), "{q:?}");
        assert!((0..3).all(|i| (s[i] - [2.0, 0.5, 1.5][i]).abs() < 1e-12), "{s:?}");
        let back = mat_mul(&m, &mat_inv(&m).unwrap());
        assert!((0..16).all(|i| (back[i] - IDENT[i]).abs() < 1e-12));
        assert_eq!(mat_inv(&[0.0; 16]), None);
        assert_eq!(xform_point(&m, &[0.0, 0.0, 0.0]), [1.0, -2.0, 3.0]);
        assert_eq!(xform_dir(&IDENT, &[1.0, 2.0, 3.0]), [1.0, 2.0, 3.0]);
    }

    #[test]
    fn slerp_takes_the_shorter_arc_and_no_rotation_for_nothing() {
        let half_turn_about_z = [0.0, 0.0, 1.0, 0.0];
        let q = slerp(&NO_ROTATION, &half_turn_about_z, 0.5).unwrap();
        let s = std::f64::consts::FRAC_1_SQRT_2;
        assert!((q[2] - s).abs() < 1e-12 && (q[3] - s).abs() < 1e-12, "{q:?}");
        // a quaternion more than half a turn away is taken negated: the same rotation, the short way
        let far = slerp(&NO_ROTATION, &[0.0, 0.0, 0.6, -0.8], 0.5).unwrap();
        let near = slerp(&NO_ROTATION, &[0.0, 0.0, -0.6, 0.8], 0.5).unwrap();
        assert!((0..4).all(|i| (far[i] - near[i]).abs() < 1e-12), "{far:?} {near:?}");
        assert_eq!(slerp(&[0.0; 4], &[0.0; 4], 0.5).unwrap(), NO_ROTATION);
        assert_eq!(normalize(&[0.0; 3], Z), Z);
    }
}
