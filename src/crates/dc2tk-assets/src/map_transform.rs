//! The game's transforms (position, Euler rotation in radians, scale; a node's matrix is
//! T·Rz·Ry·Rx·S, mgCFrame::GetLocalMatrix) and glTF's (translation, quaternion, scale). A node whose
//! matrix did not change keeps the numbers its .map gave, so an unedited map writes back byte for
//! byte; a moved one gets new numbers.

use crate::mapfile::Num;

/// A 4x4 matrix as rows (rows are output coordinates: the translation is column 3): the .map's
/// transforms' form, where math::Mat4 is the column-major form of glTF and the MDS.
pub type RowMatrix = [[f64; 4]; 4];

/// glTF's form of a transform: translation, rotation quaternion (x, y, z, w), scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Trs {
    pub translation: [f64; 3],
    pub rotation: [f64; 4],
    pub scale: [f64; 3],
}

fn floats(values: &[Num; 3]) -> [f64; 3] {
    values.map(|value| value.float() as f64)
}

/// The rotation part, Rz·Ry·Rx, of Euler angles in radians.
fn rotation(x: f64, y: f64, z: f64) -> [[f64; 3]; 3] {
    let (sx, cx) = x.sin_cos();
    let (sy, cy) = y.sin_cos();
    let (sz, cz) = z.sin_cos();
    [
        [cy * cz, sx * sy * cz - cx * sz, cx * sy * cz + sx * sz],
        [cy * sz, sx * sy * sz + cx * cz, cx * sy * sz - sx * cz],
        [-sy, sx * cy, cx * cy],
    ]
}

/// T·Rz·Ry·Rx·S of a .map transform (rows are output coordinates).
pub fn matrix(pos: &[Num; 3], rot: &[Num; 3], scale: &[Num; 3]) -> RowMatrix {
    let [x, y, z] = floats(rot);
    let r = rotation(x, y, z);
    let t = floats(pos);
    let s = floats(scale);
    let mut out = [[0.0; 4]; 4];
    for row in 0..3 {
        for col in 0..3 {
            out[row][col] = r[row][col] * s[col];
        }
        out[row][3] = t[row];
    }
    out[3][3] = 1.0;
    out
}

/// The matrix of a glTF transform.
pub fn trs_matrix(trs: &Trs) -> RowMatrix {
    let [x, y, z, w] = trs.rotation;
    let r = [
        [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - z * w), 2.0 * (x * z + y * w)],
        [2.0 * (x * y + z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - x * w)],
        [2.0 * (x * z - y * w), 2.0 * (y * z + x * w), 1.0 - 2.0 * (x * x + y * y)],
    ];
    let mut out = [[0.0; 4]; 4];
    for row in 0..3 {
        for col in 0..3 {
            out[row][col] = r[row][col] * trs.scale[col];
        }
        out[row][3] = trs.translation[row];
    }
    out[3][3] = 1.0;
    out
}

/// A .map transform as glTF writes it.
pub fn to_trs(pos: &[Num; 3], rot: &[Num; 3], scale: &[Num; 3]) -> Trs {
    let [x, y, z] = floats(rot);
    let half = |angle: f64| ((angle / 2.0).sin(), (angle / 2.0).cos());
    let (sx, cx) = half(x);
    let (sy, cy) = half(y);
    let (sz, cz) = half(z);
    // qz * qy * qx
    let rotation = [
        sx * cy * cz - cx * sy * sz,
        cx * sy * cz + sx * cy * sz,
        cx * cy * sz - sx * sy * cz,
        cx * cy * cz + sx * sy * sz,
    ];
    Trs { translation: floats(pos), rotation, scale: floats(scale) }
}

/// Whether two matrices are the same transform, within what float32 files and glTF's floats
/// lose: 1e-4 of the larger of 1 and the values compared; positions within 5e-3 and 2e-6 of them.
pub fn same(a: &RowMatrix, b: &RowMatrix) -> bool {
    (0..4).all(|row| {
        (0..4).all(|column| {
            let (x, y) = (a[row][column], b[row][column]);
            // Blender keeps world matrices in 32 bits, so a piece of a placement thousands of
            // units out comes back moved by up to a few thousandths
            if column == 3 && row < 3 {
                // positions absolutely, so a move far out is still a move
                (x - y).abs() <= 5e-3 + 2e-6 * x.abs().max(y.abs())
            } else {
                (x - y).abs() <= 1e-4 * x.abs().max(y.abs()).max(1.0)
            }
        })
    })
}

/// Euler angles (x, y, z in radians) of a rotation matrix R = Rz·Ry·Rx.
fn euler(r: &[[f64; 3]; 3]) -> [f64; 3] {
    let sy = (-r[2][0]).clamp(-1.0, 1.0);
    let y = sy.asin();
    if sy.abs() < 0.999_999 {
        [r[2][1].atan2(r[2][2]), y, r[1][0].atan2(r[0][0])]
    } else {
        // gimbal lock: x and z turn about the same axis; put it all in z
        [0.0, y, (-r[0][1]).atan2(r[1][1])]
    }
}

/// A glTF transform as a .map gives it: the original numbers when it is the same transform,
/// otherwise new position, Euler angles and scale.
pub fn from_trs(trs: &Trs, original: Option<(&[Num; 3], &[Num; 3], &[Num; 3])>) -> ([Num; 3], [Num; 3], [Num; 3]) {
    if let Some((pos, rot, scale)) = original {
        if same(&matrix(pos, rot, scale), &trs_matrix(trs)) {
            return (*pos, *rot, *scale);
        }
    }
    let unit = trs_matrix(&Trs { scale: [1.0; 3], translation: [0.0; 3], ..*trs });
    let r = [
        [unit[0][0], unit[0][1], unit[0][2]],
        [unit[1][0], unit[1][1], unit[1][2]],
        [unit[2][0], unit[2][1], unit[2][2]],
    ];
    let to_num = |values: [f64; 3]| values.map(|value| Num::from(value as f32));
    (to_num(trs.translation), to_num(euler(&r)), to_num(trs.scale))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nums(values: [f32; 3]) -> [Num; 3] {
        values.map(Num::from)
    }

    #[test]
    fn transforms_go_to_gltf_and_back() {
        let cases = [
            ([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]),
            ([10.0, -5.5, 1180.61], [0.0, -1.25, 0.0], [1.0, 1.0, 1.0]),
            ([1.0, 2.0, 3.0], [0.3, -0.7, 2.9], [20.0, 20.0, 20.0]),
            ([0.0, 0.0, 0.0], [0.1, 0.2, 0.3], [-1.0, 1.0, 1.0]),
            ([5.0, 0.0, 0.0], [1.0, 1.5707964, -0.4], [1.0, 2.0, 3.0]),
        ];
        for (pos, rot, scale) in cases {
            let (pos, rot, scale) = (nums(pos), nums(rot), nums(scale));
            let trs = to_trs(&pos, &rot, &scale);
            assert!(same(&matrix(&pos, &rot, &scale), &trs_matrix(&trs)), "{rot:?}");
            // unchanged: the original numbers
            assert_eq!(from_trs(&trs, Some((&pos, &rot, &scale))), (pos, rot, scale));
            // without the original: numbers that give the same matrix
            let (p, r, s) = from_trs(&trs, None);
            if scale.iter().all(|value| value.float() > 0.0) {
                assert!(same(&matrix(&p, &r, &s), &matrix(&pos, &rot, &scale)), "{rot:?} -> {r:?}");
            }
        }
    }

    #[test]
    fn a_moved_node_gets_new_numbers() {
        let (pos, rot, scale) = (nums([0.0, 0.0, 0.0]), nums([0.0, 0.0, 0.0]), nums([1.0, 1.0, 1.0]));
        let mut trs = to_trs(&pos, &rot, &scale);
        trs.translation[0] = 12.5;
        let (p, r, s) = from_trs(&trs, Some((&pos, &rot, &scale)));
        assert_eq!((p[0], r, s), (Num::from(12.5f32), rot, scale));
    }

    #[test]
    fn a_move_far_out_is_a_move_and_float_noise_is_not() {
        let at = |x: f32| matrix(&nums([x, 0.0, 3000.0]), &nums([0.0; 3]), &nums([1.0; 3]));
        assert!(same(&at(-4557.0), &at(-4557.003)), "32-bit noise far out");
        assert!(!same(&at(-4557.0), &at(-4556.0)), "a move of one unit far out");
        assert!(!same(&at(0.0), &at(0.01)), "a move of a hundredth near the origin");
    }
}
