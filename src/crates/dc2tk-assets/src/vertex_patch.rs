//! Edited vertices written into a model's own bytes. When a glTF differs from the export of an MDS
//! file only in its corners' values (same meshes, corners, triangles, materials, nodes), each
//! changed position, normal, UV and colour is written over the entry it came from in the MDT's
//! arrays, every other byte left as it is. Positions and normals go back through the map the
//! export used for that mesh (the identity for a map piece, the bind pose for a skinned character
//! mesh), fitted from the export itself. The file keeps its size, which the PC port's file
//! overrides need (a replacement no larger than the retail file).
//!
//! None when anything else changed (a full rebuild is needed) or an entry would get two values.

use std::collections::HashMap;

use serde_json::Value;

use crate::from_gltf::Gltf;
use crate::{mds, to_gltf, Result};

/// The glTF attributes written back, with the MDT array each indexes (by the corner key's slot)
/// and how many numbers it has.
const ATTRIBUTES: [(&str, usize, usize); 4] = [("POSITION", 0, 3), ("NORMAL", 1, 3), ("TEXCOORD_0", 2, 2), ("COLOR_0", 3, 4)];

/// A float accessor's values (VEC2, VEC3 or VEC4).
fn floats(g: &Gltf, accessor: usize, width: usize) -> Option<Vec<Vec<f64>>> {
    let a = g.json["accessors"].get(accessor)?;
    if a["componentType"] != 5126 {
        return None;
    }
    let view = g.json["bufferViews"].get(a["bufferView"].as_u64()? as usize)?;
    let buffer = g.buffers.get(view["buffer"].as_u64().unwrap_or(0) as usize)?;
    let stride = view["byteStride"].as_u64().unwrap_or(4 * width as u64) as usize;
    let start = view["byteOffset"].as_u64().unwrap_or(0) as usize + a["byteOffset"].as_u64().unwrap_or(0) as usize;
    let count = a["count"].as_u64()? as usize;
    (0..count)
        .map(|i| {
            let at = start + i * stride;
            (0..width)
                .map(|k| buffer.get(at + 4 * k..at + 4 * k + 4).map(|b| f32::from_le_bytes(b.try_into().unwrap()) as f64))
                .collect()
        })
        .collect()
}

/// The bytes an accessor reads (its view), for comparing two glTFs.
fn accessor_bytes(g: &Gltf, accessor: usize) -> Option<Vec<u8>> {
    let a = g.json["accessors"].get(accessor)?;
    let view = g.json["bufferViews"].get(a["bufferView"].as_u64()? as usize)?;
    let buffer = g.buffers.get(view["buffer"].as_u64().unwrap_or(0) as usize)?;
    let start = view["byteOffset"].as_u64().unwrap_or(0) as usize + a["byteOffset"].as_u64().unwrap_or(0) as usize;
    let length = view["byteLength"].as_u64()? as usize;
    buffer.get(start..(start + length).min(buffer.len())).map(<[u8]>::to_vec)
}

/// The accessors of the attributes written back.
fn patched_accessors(json: &Value) -> Vec<usize> {
    json["meshes"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|m| m["primitives"].as_array().cloned().unwrap_or_default())
        .flat_map(|p| ATTRIBUTES.iter().filter_map(move |(name, _, _)| p["attributes"][name].as_u64().map(|a| a as usize)).collect::<Vec<_>>())
        .collect()
}

/// The glTF's JSON with what a corner edit changes taken out: those accessors' min and max, the
/// buffers' lengths and URIs (the buffer bytes are compared accessor by accessor).
fn shape(json: &Value) -> Value {
    let mut out = json.clone();
    let patched = patched_accessors(json);
    if let Some(accessors) = out["accessors"].as_array_mut() {
        for &a in &patched {
            if let Some(accessor) = accessors.get_mut(a).and_then(Value::as_object_mut) {
                accessor.remove("min");
                accessor.remove("max");
            }
        }
    }
    if let Some(buffers) = out["buffers"].as_array_mut() {
        for buffer in buffers.iter_mut().filter_map(Value::as_object_mut) {
            buffer.remove("byteLength");
            buffer.remove("uri");
        }
    }
    out
}

/// The 3x4 affine map taking a mesh's MDT vertices to its exported positions (least squares over
/// every corner; exact for the export), or None when the vertices span no volume.
fn fit(pairs: &[([f64; 3], [f64; 3])]) -> Option<[[f64; 4]; 3]> {
    let mut ata = [[0.0f64; 4]; 4];
    let mut atb = [[0.0f64; 3]; 4];
    for (v, p) in pairs {
        let x = [v[0], v[1], v[2], 1.0];
        for r in 0..4 {
            for c in 0..4 {
                ata[r][c] += x[r] * x[c];
            }
            for k in 0..3 {
                atb[r][k] += x[r] * p[k];
            }
        }
    }
    let inv = invert4(&ata)?;
    let mut m = [[0.0; 4]; 3];
    for k in 0..3 {
        for c in 0..4 {
            m[k][c] = (0..4).map(|r| inv[c][r] * atb[r][k]).sum();
        }
    }
    Some(m)
}

fn invert4(m: &[[f64; 4]; 4]) -> Option<[[f64; 4]; 4]> {
    let mut a = *m;
    let mut inv = [[0.0; 4]; 4];
    for (i, row) in inv.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    for col in 0..4 {
        let pivot = (col..4).max_by(|&x, &y| a[x][col].abs().total_cmp(&a[y][col].abs()))?;
        if a[pivot][col].abs() < 1e-12 {
            return None;
        }
        a.swap(col, pivot);
        inv.swap(col, pivot);
        let d = a[col][col];
        for k in 0..4 {
            a[col][k] /= d;
            inv[col][k] /= d;
        }
        for r in 0..4 {
            if r != col {
                let f = a[r][col];
                for k in 0..4 {
                    a[r][k] -= f * a[col][k];
                    inv[r][k] -= f * inv[col][k];
                }
            }
        }
    }
    Some(inv)
}

/// The linear part of a fitted map, inverted.
fn linear_inverse(m: &[[f64; 4]; 3]) -> Option<[[f64; 4]; 4]> {
    invert4(&[
        [m[0][0], m[0][1], m[0][2], 0.0],
        [m[1][0], m[1][1], m[1][2], 0.0],
        [m[2][0], m[2][1], m[2][2], 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ])
}

fn apply3(inv: &[[f64; 4]; 4], d: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|r| (0..3).map(|c| inv[r][c] * d[c]).sum())
}

/// Where each frame's MDT arrays are in the MDS file: vertices, normals, UVs, colours (file
/// offsets; None for an array the MDT has not).
fn arrays(data: &[u8]) -> Option<HashMap<usize, [Option<usize>; 4]>> {
    let word = |at: usize| data.get(at..at + 4).map(|b| i32::from_le_bytes(b.try_into().unwrap()));
    let (count, records) = (word(8)? as usize, word(12)? as usize);
    let mut out = HashMap::new();
    for i in 0..count {
        let mdt = word(records + 0x70 * i + 40)?;
        if mdt > 0 {
            let m = mdt as usize;
            // MDT header: +16 vertices, +24 normals, +32 colours, +48 UVs (mds::Mdt::unpack)
            let at = |field: usize| word(m + field).filter(|&o| o > 0).map(|o| m + o as usize);
            out.insert(i, [at(16), at(24), at(48), at(32)]);
        }
    }
    Some(out)
}

/// The MDS file with the changed corners of `given` (an edit of `exported`, the export of
/// `data`) written in, or None (see the module's note).
pub fn patch(data: &[u8], exported: &Gltf, given: &Gltf) -> Result<Option<Vec<u8>>> {
    if shape(&exported.json) != shape(&given.json) {
        return Ok(None);
    }
    let model = mds::parse(data)?;
    let Some(arrays) = arrays(data) else {
        return Ok(None);
    };
    // (frame, array slot, index) -> the numbers to write over the entry's first ones
    let mut writes: HashMap<(usize, usize, i64), Vec<f64>> = HashMap::new();
    let patched = patched_accessors(&exported.json);
    for mesh in exported.json["meshes"].as_array().into_iter().flatten() {
        let Some(frame) = mesh["extras"]["mds_index"].as_u64().map(|f| f as usize) else {
            return Ok(None);
        };
        let Some(t) = model.nodes.get(frame).and_then(|n| n.mdt.as_ref()) else {
            return Ok(None);
        };
        let keys = to_gltf::corner_keys(t);
        let primitives = mesh["primitives"].as_array().cloned().unwrap_or_default();
        if primitives.len() != keys.len() {
            return Ok(None);
        }
        // the export's positions against the MDT's vertices: the map positions and normals took
        let mut pairs = Vec::new();
        let mut edits: Vec<(usize, i64, Vec<f64>)> = Vec::new();
        for (p, corners) in primitives.iter().zip(&keys) {
            for &(name, slot, width) in &ATTRIBUTES {
                let Some(a) = p["attributes"][name].as_u64().map(|a| a as usize) else { continue };
                let (Some(before), Some(after)) = (floats(exported, a, width), floats(given, a, width)) else {
                    return Ok(None);
                };
                if before.len() != corners.len() || after.len() != corners.len() {
                    return Ok(None);
                }
                for ((key, b), g) in corners.iter().zip(&before).zip(&after) {
                    if slot == 0 {
                        let Some(v) = t.vertices.get(key[0] as usize) else { return Ok(None) };
                        pairs.push(([v[0], v[1], v[2]], [b[0], b[1], b[2]]));
                    }
                    if b != g {
                        if key[slot] < 0 {
                            return Ok(None);
                        }
                        edits.push((slot, key[slot], g.clone()));
                    }
                }
            }
        }
        if edits.is_empty() {
            continue;
        }
        let map = fit(&pairs);
        for (slot, index, value) in edits {
            let value = match slot {
                0 => {
                    let Some(m) = map else { return Ok(None) };
                    let Some(inv) = linear_inverse(&m) else { return Ok(None) };
                    apply3(&inv, [value[0] - m[0][3], value[1] - m[1][3], value[2] - m[2][3]]).to_vec()
                }
                1 => {
                    let Some(inv) = map.as_ref().and_then(linear_inverse) else { return Ok(None) };
                    apply3(&inv, [value[0], value[1], value[2]]).to_vec()
                }
                _ => value,
            };
            if let Some(was) = writes.insert((frame, slot, index), value.clone()) {
                if was.iter().zip(&value).any(|(a, b)| (a - b).abs() > 1e-4) {
                    return Ok(None);
                }
            }
        }
    }
    // everything but the attributes written back must be the export's, byte for byte
    let accessors = exported.json["accessors"].as_array().map_or(0, Vec::len);
    for a in (0..accessors).filter(|a| !patched.contains(a)) {
        if accessor_bytes(exported, a) != accessor_bytes(given, a) {
            return Ok(None);
        }
    }
    let mut out = data.to_vec();
    for ((frame, slot, index), value) in writes {
        let Some(array) = arrays.get(&frame).and_then(|a| a[slot]) else {
            return Ok(None);
        };
        let at = array + 16 * index as usize;
        for (k, number) in value.iter().enumerate() {
            let Some(place) = out.get_mut(at + 4 * k..at + 4 * k + 4) else {
                return Ok(None);
            };
            place.copy_from_slice(&(*number as f32).to_le_bytes());
        }
    }
    Ok(Some(out))
}
