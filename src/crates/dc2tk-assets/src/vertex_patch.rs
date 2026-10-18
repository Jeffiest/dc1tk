//! Moved vertices written into a model's own bytes. When a glTF differs from the export of an MDS
//! file only in vertex positions (same meshes, corners, normals, UVs, colours, materials, nodes),
//! each moved position is mapped back through the affine map the export used for that mesh (the
//! identity for a map piece, the bind pose for a skinned character mesh) and written over the
//! vertex in the MDT's vertex array, every other byte left as it is. The file keeps its size,
//! which the PC port's file overrides need (a replacement no larger than the retail file).
//!
//! None when anything else changed (a full rebuild is needed) or a vertex would get two positions.

use std::collections::HashMap;

use serde_json::Value;

use crate::from_gltf::Gltf;
use crate::{mds, to_gltf, Result};

/// A float VEC3 accessor's values.
fn vec3s(g: &Gltf, accessor: usize) -> Option<Vec<[f64; 3]>> {
    let a = g.json["accessors"].get(accessor)?;
    if a["type"] != "VEC3" || a["componentType"] != 5126 {
        return None;
    }
    let view = g.json["bufferViews"].get(a["bufferView"].as_u64()? as usize)?;
    let buffer = g.buffers.get(view["buffer"].as_u64().unwrap_or(0) as usize)?;
    let stride = view["byteStride"].as_u64().unwrap_or(12) as usize;
    let start = view["byteOffset"].as_u64().unwrap_or(0) as usize + a["byteOffset"].as_u64().unwrap_or(0) as usize;
    let count = a["count"].as_u64()? as usize;
    (0..count)
        .map(|i| {
            let at = start + i * stride;
            let f = |k: usize| buffer.get(at + 4 * k..at + 4 * k + 4).map(|b| f32::from_le_bytes(b.try_into().unwrap()) as f64);
            Some([f(0)?, f(1)?, f(2)?])
        })
        .collect()
}

/// The bytes an accessor reads (its elements, stride and all), for comparing two glTFs.
fn accessor_bytes(g: &Gltf, accessor: usize) -> Option<Vec<u8>> {
    let a = g.json["accessors"].get(accessor)?;
    let view = g.json["bufferViews"].get(a["bufferView"].as_u64()? as usize)?;
    let buffer = g.buffers.get(view["buffer"].as_u64().unwrap_or(0) as usize)?;
    let start = view["byteOffset"].as_u64().unwrap_or(0) as usize + a["byteOffset"].as_u64().unwrap_or(0) as usize;
    let length = view["byteLength"].as_u64()? as usize;
    buffer.get(start..(start + length).min(buffer.len())).map(<[u8]>::to_vec)
}

/// The glTF's JSON with what a position edit changes taken out: POSITION accessors' min and max,
/// the buffers' lengths and URIs (the buffer bytes are compared accessor by accessor).
fn shape(json: &Value) -> Value {
    let mut out = json.clone();
    let positions: Vec<usize> = json["meshes"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|m| m["primitives"].as_array().cloned().unwrap_or_default())
        .filter_map(|p| p["attributes"]["POSITION"].as_u64().map(|a| a as usize))
        .collect();
    if let Some(accessors) = out["accessors"].as_array_mut() {
        for &a in &positions {
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

/// The vertex an exported position comes from: the fitted map undone.
fn unmap(m: &[[f64; 4]; 3], p: [f64; 3]) -> Option<[f64; 3]> {
    let linear = [
        [m[0][0], m[0][1], m[0][2], 0.0],
        [m[1][0], m[1][1], m[1][2], 0.0],
        [m[2][0], m[2][1], m[2][2], 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let inv = invert4(&linear)?;
    let d = [p[0] - m[0][3], p[1] - m[1][3], p[2] - m[2][3]];
    Some([0, 1, 2].map(|r| (0..3).map(|c| inv[r][c] * d[c]).sum()))
}

/// Where each frame's MDT vertex array is in the MDS file.
fn vertex_arrays(data: &[u8]) -> Option<HashMap<usize, usize>> {
    let word = |at: usize| data.get(at..at + 4).map(|b| i32::from_le_bytes(b.try_into().unwrap()));
    let (count, records) = (word(8)? as usize, word(12)? as usize);
    let mut out = HashMap::new();
    for i in 0..count {
        let mdt = word(records + 0x70 * i + 40)?;
        if mdt > 0 {
            out.insert(i, mdt as usize + word(mdt as usize + 16)? as usize);
        }
    }
    Some(out)
}

/// The MDS file with the moved vertices of `given` (an edit of `exported`, the export of `data`)
/// written in, or None (see the module's note).
pub fn patch(data: &[u8], exported: &Gltf, given: &Gltf) -> Result<Option<Vec<u8>>> {
    if shape(&exported.json) != shape(&given.json) {
        return Ok(None);
    }
    let model = mds::parse(data)?;
    let Some(arrays) = vertex_arrays(data) else {
        return Ok(None);
    };
    let mut moved: HashMap<(usize, i64), [f64; 3]> = HashMap::new();
    let accessors = exported.json["accessors"].as_array().map_or(0, Vec::len);
    let mut positions = Vec::new();
    for (mi, mesh) in exported.json["meshes"].as_array().into_iter().flatten().enumerate() {
        let Some(frame) = mesh["extras"]["mds_index"].as_u64().map(|f| f as usize) else {
            return Ok(None);
        };
        let Some(t) = model.nodes.get(frame).and_then(|n| n.mdt.as_ref()) else {
            return Ok(None);
        };
        let corners = to_gltf::corner_vertices(t);
        let primitives = mesh["primitives"].as_array().cloned().unwrap_or_default();
        if primitives.len() != corners.len() {
            return Ok(None);
        }
        let mut pairs = Vec::new();
        let mut edits = Vec::new();
        for (p, vertices) in primitives.iter().zip(&corners) {
            let Some(a) = p["attributes"]["POSITION"].as_u64().map(|a| a as usize) else {
                return Ok(None);
            };
            positions.push(a);
            let (Some(before), Some(after)) = (vec3s(exported, a), vec3s(given, a)) else {
                return Ok(None);
            };
            if before.len() != vertices.len() || after.len() != vertices.len() {
                return Ok(None);
            }
            for ((&v, b), g) in vertices.iter().zip(&before).zip(&after) {
                let Some(original) = t.vertices.get(v as usize) else {
                    return Ok(None);
                };
                pairs.push(([original[0], original[1], original[2]], *b));
                if b != g {
                    edits.push((v, *g));
                }
            }
        }
        if edits.is_empty() {
            continue;
        }
        let Some(map) = fit(&pairs) else {
            return Ok(None);
        };
        for (v, g) in edits {
            let Some(vertex) = unmap(&map, g) else {
                return Ok(None);
            };
            if let Some(was) = moved.insert((frame, v), vertex) {
                if was.iter().zip(&vertex).any(|(a, b)| (a - b).abs() > 1e-4) {
                    return Ok(None);
                }
            }
        }
        let _ = mi;
    }
    // everything but the positions must be the export's, byte for byte
    for a in (0..accessors).filter(|a| !positions.contains(a)) {
        if accessor_bytes(exported, a) != accessor_bytes(given, a) {
            return Ok(None);
        }
    }
    let mut out = data.to_vec();
    for ((frame, v), vertex) in moved {
        let Some(&array) = arrays.get(&frame) else {
            return Ok(None);
        };
        let at = array + 16 * v as usize;
        for (k, value) in vertex.iter().enumerate() {
            let Some(slot) = out.get_mut(at + 4 * k..at + 4 * k + 4) else {
                return Ok(None);
            };
            slot.copy_from_slice(&(*value as f32).to_le_bytes());
        }
    }
    Ok(Some(out))
}
