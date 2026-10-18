//! Dark Cloud 1's maps, for mods:
//!
//! - **Dungeon floors**: a floor's tag script (`dun/d01main_a.cfg`: lights, fog, and DEF_PATS blocks
//!   naming each part's model `PT_BASE`, floor and wall collision `PT_COLS`, camera collision
//!   `PT_CAM`, torches `PT_FIRE` ...; read by itemdata.cpp's TEIGI tables) and the pack of its parts
//!   (`dun/mpd_pack/d01main_a.mpd`: the parts' .mds and their .img). Exported as `floor.cfg` (the
//!   script as UTF-8), `floor.json` (its statements, parts grouped; for reading, the build takes
//!   floor.cfg) and `parts/<model>.gltf` for every .mds of the pack (`_a` and `_v` models as
//!   collision).
//! - **Georama scenes** (`gedit/e01/in/0101/0101.scn`): a PTS archive (dispctrl.cpp PTS_HEADER and
//!   PTS_ENTRY) of parts, each an EPARTS_ARCHIVE (editpartsinfo.hpp): its EPARTS_INFO_HEADER (size,
//!   grid cells, position, rotation, element ids and names, function points) and up to nine models
//!   (LOD 0-3, collision, shadow, shade, camera, extra). Exported as `scene.json` and
//!   `<part>/<slot>.gltf` (or `.raw` for a block that is not a model).
//! - **Georama layouts** (`gdata0.edt`, editground.cpp CEditGround::Save/Load): GROUND_SAVE_HEADER
//!   and SV_GRD_PART records, as `layout.json`.
//!
//! Every build gives the original bytes when nothing was edited. An edited model is rebuilt with
//! models::build_map_model (only the meshes that changed); in a scene, an edited block is appended
//! to its part and every slot that used the old block points at it, and a part that grows is
//! appended to the scene, so nothing else moves.

use std::path::Path;

use serde_json::{json, Value};

use dc2tk_disc::archive::Pack;
use dc2tk_disc::text;

use crate::from_gltf::{self, Gltf};
use crate::info_cfg::{self, Arg};
use crate::models::{self, Files};
use crate::to_gltf::{Options, TextureSearch};
use crate::{bail, Context, Error, Result};

fn i32_at(data: &[u8], at: usize) -> Result<i32> {
    data.get(at..at + 4)
        .map(|b| i32::from_le_bytes(b.try_into().unwrap()))
        .ok_or_else(|| Error::invalid(format!("the data ends before byte 0x{:x}", at + 4)))
}

fn f32_at(data: &[u8], at: usize) -> Result<f32> {
    Ok(f32::from_bits(i32_at(data, at)? as u32))
}

fn i16_at(data: &[u8], at: usize) -> Result<i16> {
    data.get(at..at + 2)
        .map(|b| i16::from_le_bytes(b.try_into().unwrap()))
        .ok_or_else(|| Error::invalid(format!("the data ends before byte 0x{:x}", at + 2)))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Result<Vec<u8>> {
    if text.len() % 2 != 0 {
        bail!("{text:?}: hex has an odd length");
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).map_err(|_| Error::invalid(format!("{text:?} is not hex"))))
        .collect()
}

fn json_bytes(value: &Value) -> Vec<u8> {
    let mut text = serde_json::to_string_pretty(value).expect("JSON values serialize");
    text.push('\n');
    text.into_bytes()
}

fn read_json(path: &Path) -> Result<Value> {
    let text = std::fs::read_to_string(path).map_err(|e| Error::invalid(format!("cannot read {}: {e}", path.display())))?;
    serde_json::from_str(&text).map_err(|e| Error::invalid(format!("{}: {e}", path.display())))
}

fn load_gltf_file(path: &Path) -> Result<Gltf> {
    let raw = std::fs::read(path).map_err(|e| Error::invalid(format!("cannot read {}: {e}", path.display())))?;
    let folder = path.parent().unwrap_or(Path::new("."));
    from_gltf::load_gltf(&raw, &|uri| {
        std::fs::read(folder.join(uri)).map_err(|e| Error::invalid(format!("cannot read {uri}: {e}")))
    })
}

/// Whether a part model is collision: a floor and wall (`_a`) or camera (`_v`) model.
fn is_collision(stem: &str) -> bool {
    let lower = stem.to_lowercase();
    lower.ends_with("_a") || lower.ends_with("_v")
}

fn stem(name: &str) -> &str {
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}

/// A model block as glTF files under `prefix`, or None when it is not an MDS file.
fn export_model(data: &[u8], prefix: &str, base: &str, collision: bool, textures: &TextureSearch) -> Result<Option<Files>> {
    if !data.starts_with(b"MDS\0") {
        return Ok(None);
    }
    let options = if collision { Options::collision() } else { Options::map(Some(textures)) };
    let (files, _) = models::export_map_model(data, base, &options)?;
    Ok(Some(files.into_iter().map(|(path, bytes)| (format!("{prefix}{path}"), bytes)).collect()))
}

/// A model block rebuilt from `<folder>/<base>.gltf`, or the block itself when there is none.
fn build_model(folder: &Path, base: &str, original: &[u8], collision: bool, textures: &TextureSearch) -> Result<Vec<u8>> {
    let path = folder.join(format!("{base}.gltf"));
    if !original.starts_with(b"MDS\0") || !path.is_file() {
        return Ok(original.to_vec());
    }
    let options = if collision { Options::collision() } else { Options::map(Some(textures)) };
    // the files as the export writes them: the model as it is (the MDS writer does not keep the
    // bytes some of the game's files have past their last block)
    let (exported, _) = models::export_map_model(original, base, &options)?;
    let untouched = exported
        .iter()
        .filter(|(name, _)| !name.starts_with("textures/"))
        .all(|(name, data)| std::fs::read(folder.join(name)).is_ok_and(|on_disk| on_disk == *data));
    if untouched {
        return Ok(original.to_vec());
    }
    let given = load_gltf_file(&path)?;
    // only vertices moved: written into the model's own bytes, so it keeps its size
    let exported_gltf = from_gltf::load_gltf(
        &exported.iter().find(|(name, _)| *name == format!("{base}.gltf")).map(|(_, d)| d.clone()).unwrap_or_default(),
        &|uri| exported.iter().find(|(name, _)| name == uri).map(|(_, d)| d.clone()).ok_or_else(|| Error::invalid(uri.to_string())),
    )?;
    if let Some(patched) = crate::vertex_patch::patch(original, &exported_gltf, &given)? {
        return Ok(patched);
    }
    models::build_map_model(&given, original, &options).context(path.display())
}

fn arg_json(arg: &Arg) -> Value {
    match arg {
        Arg::Null => Value::Null,
        Arg::Int(value) => json!(value),
        Arg::Float(value) => json!(value),
        Arg::Str(value) => json!(value),
    }
}

/// A texture search over the .img files given (a pack's, a folder's).
pub fn textures_of(files: &[(String, Vec<u8>)]) -> Result<TextureSearch> {
    let images: Vec<(String, Vec<u8>)> =
        files.iter().filter(|(name, _)| name.to_lowercase().ends_with(".img")).cloned().collect();
    TextureSearch::new(&images)
}

// ---- dungeon floors ----

/// A floor's script and parts pack as `floor.cfg`, `floor.json` and `parts/<model>.gltf`.
/// `more_images` are other texture files the floor draws from (the floor's SET_PATH folder).
pub fn floor_export(cfg: &[u8], mpd: &[u8], more_images: &[(String, Vec<u8>)]) -> Result<Files> {
    let Some(script) = text::decode_strict(cfg) else {
        bail!("the floor script is not Shift-JIS text");
    };
    let mut out: Files = vec![("floor.cfg".into(), script.into_bytes())];
    let mut statements = Vec::new();
    let mut parts: Vec<Value> = Vec::new();
    let mut current: Option<Vec<Value>> = None;
    for (tag, args) in info_cfg::parse(cfg) {
        let entry = json!({"tag": tag, "args": args.iter().map(arg_json).collect::<Vec<_>>()});
        match tag.as_str() {
            "DEF_PATS" => current = Some(Vec::new()),
            "DEF_ENDS" => parts.push(Value::Array(current.take().unwrap_or_default())),
            _ => match &mut current {
                Some(part) => part.push(entry),
                None => statements.push(entry),
            },
        }
    }
    out.push((
        "floor.json".into(),
        json_bytes(&json!({
            "note": "a reading of floor.cfg; the build takes floor.cfg (edit that)",
            "settings": statements,
            "parts": parts,
        })),
    ));
    let pack = Pack::read(mpd).ok_or_else(|| Error::invalid("the .mpd is not a pack"))?;
    let members: Vec<(String, Vec<u8>)> = pack.members.iter().map(|m| (m.name.clone(), m.data.clone())).collect();
    let mut images = members.clone();
    images.extend(more_images.iter().cloned());
    let textures = textures_of(&images)?;
    let mut raw = Vec::new();
    for (name, data) in &members {
        if !name.to_lowercase().ends_with(".mds") {
            continue;
        }
        let base = stem(name);
        match export_model(data, "parts/", base, is_collision(base), &textures) {
            Ok(Some(files)) => out.extend(files),
            Ok(None) => {}
            Err(error) => {
                // kept as the file itself (parts/<name>), which the build takes as it is
                out.push((format!("parts/{name}"), data.clone()));
                raw.push(json!({"model": name, "why": error.to_string()}));
            }
        }
    }
    if !raw.is_empty() {
        out.push(("parts/not_converted.json".into(), json_bytes(&Value::Array(raw))));
    }
    Ok(out)
}

/// The floor's script and parts pack with a folder's floor.cfg and edited parts/ glTFs put in.
pub fn floor_build(folder: &Path, cfg: &[u8], mpd: &[u8], more_images: &[(String, Vec<u8>)]) -> Result<(Vec<u8>, Vec<u8>)> {
    let new_cfg = match std::fs::read_to_string(folder.join("floor.cfg")) {
        Ok(script) if text::decode_strict(cfg).as_deref() == Some(script.as_str()) => cfg.to_vec(),
        Ok(script) => text::encode(&script).ok_or_else(|| Error::invalid("floor.cfg has a character Shift-JIS cannot hold"))?,
        Err(_) => cfg.to_vec(),
    };
    let mut pack = Pack::read(mpd).ok_or_else(|| Error::invalid("the .mpd is not a pack"))?;
    let mut images: Vec<(String, Vec<u8>)> = pack.members.iter().map(|m| (m.name.clone(), m.data.clone())).collect();
    images.extend(more_images.iter().cloned());
    let textures = textures_of(&images)?;
    let mut changed = false;
    for member in &mut pack.members {
        if !member.name.to_lowercase().ends_with(".mds") {
            continue;
        }
        let base = stem(&member.name).to_string();
        let raw = folder.join("parts").join(&member.name);
        let built = if raw.is_file() {
            std::fs::read(&raw).map_err(|e| Error::invalid(format!("cannot read {}: {e}", raw.display())))?
        } else {
            build_model(&folder.join("parts"), &base, &member.data, is_collision(&base), &textures)?
        };
        if built != member.data {
            member.data = built;
            changed = true;
        }
    }
    let new_mpd = if changed { pack.write()? } else { mpd.to_vec() };
    Ok((new_cfg, new_mpd))
}

// ---- Georama scenes ----

/// The EPARTS_ARCHIVE slots: name, offset field, size field (editpartsinfo.hpp).
const SLOTS: [(&str, usize, usize); 9] = [
    ("lod0", 0x48, 0x58),
    ("lod1", 0x4C, 0x5C),
    ("lod2", 0x50, 0x60),
    ("lod3", 0x54, 0x64),
    ("collision", 0x78, 0x7C),
    ("shadow", 0x90, 0x94),
    ("shade", 0xA8, 0xAC),
    ("camera", 0xC0, 0xC4),
    ("extra", 0xD8, 0xDC),
];
const PTS_ARCHIVE: usize = 0xE0;
const SCN_HEADER: usize = 0x10;
const SCN_ENTRY: usize = 0x30;

fn slot_collision(slot: &str) -> bool {
    matches!(slot, "collision" | "camera")
}

struct Entry {
    name: String,
    offset: usize,
}

fn scn_entries(data: &[u8]) -> Result<Vec<Entry>> {
    if !data.starts_with(b"SCN\0") {
        bail!("not a scene (SCN) file");
    }
    let count = i32_at(data, 4)?;
    if !(0..=4096).contains(&count) {
        bail!("{count} parts");
    }
    (0..count as usize)
        .map(|i| {
            let at = SCN_HEADER + SCN_ENTRY * i;
            let raw = data.get(at..at + 16).ok_or_else(|| Error::invalid("the entries run past the file"))?;
            let name = String::from_utf8_lossy(raw.split(|&b| b == 0).next().unwrap_or_default()).to_string();
            let offset = i32_at(data, at + 16)?;
            if offset < 0 || offset as usize + PTS_ARCHIVE > data.len() || &data[offset as usize..offset as usize + 4] != b"PTS\0" {
                bail!("part {name}: no PTS archive at 0x{offset:x}");
            }
            Ok(Entry { name, offset: offset as usize })
        })
        .collect()
}

/// A part's blocks: (slot, offset from the part, size), slots with no data left out.
fn part_blocks(part: &[u8]) -> Result<Vec<(&'static str, usize, usize)>> {
    let mut out = Vec::new();
    for (slot, offset_at, size_at) in SLOTS {
        let (offset, size) = (i32_at(part, offset_at)?, i32_at(part, size_at)?);
        if size > 0 {
            if offset < 0 || offset as usize + size as usize > part.len() {
                bail!("{slot}: 0x{size:x} bytes at 0x{offset:x} run past the part");
            }
            out.push((slot, offset as usize, size as usize));
        }
    }
    Ok(out)
}

/// The end of a part: its last block's end (the info header's when it is last).
fn part_end(part: &[u8]) -> Result<usize> {
    let info = i32_at(part, 4)? as usize;
    let info_end = info + i32_at(part, info + 4)?.max(0) as usize;
    Ok(part_blocks(part)?.iter().map(|(_, offset, size)| offset + size).max().unwrap_or(0).max(info_end))
}

fn info_json(part: &[u8]) -> Result<Value> {
    let at = i32_at(part, 4)? as usize;
    let field = |offset: usize| i32_at(part, at + offset);
    let (width, height) = (field(8)?, field(0xC)?);
    let cells_at = at + field(0x3C)? as usize;
    let cells: Vec<String> = (0..height.max(0) as usize)
        .map(|row| {
            let start = cells_at + row * width.max(0) as usize;
            part.get(start..start + width.max(0) as usize).map(hex).unwrap_or_default()
        })
        .collect();
    let names: Vec<Value> = (0..6)
        .map(|i| -> Result<Value> {
            let offset = field(0x58 + i * 4)?;
            if offset <= 0 {
                return Ok(Value::Null);
            }
            let start = at + offset as usize;
            let end = part[start..].iter().position(|&b| b == 0).map_or(part.len(), |p| start + p);
            Ok(json!(String::from_utf8_lossy(&part[start..end])))
        })
        .collect::<Result<_>>()?;
    Ok(json!({
        "width": width,
        "height": height,
        "kind": field(0x10)?,
        "position": [f32_at(part, at + 0x24)?, f32_at(part, at + 0x28)?, f32_at(part, at + 0x2C)?],
        "rotation": [f32_at(part, at + 0x30)?, f32_at(part, at + 0x34)?, f32_at(part, at + 0x38)?],
        "cells": cells,
        "element_ids": (0..6).map(|i| field(0x40 + i * 4)).collect::<Result<Vec<_>>>()?,
        "element_names": names,
        "function_points": field(0x74)?,
    }))
}

/// A scene as `scene.json` and each part's models (`<part>/<slot>.gltf`, `.raw` for other blocks).
/// `images` are the texture files the scene's parts draw from (the town's .img files).
pub fn scene_export(data: &[u8], images: &[(String, Vec<u8>)]) -> Result<Files> {
    let textures = textures_of(images)?;
    let mut out = Files::new();
    let mut parts = Vec::new();
    for entry in scn_entries(data)? {
        let part = &data[entry.offset..];
        let mut slots = serde_json::Map::new();
        let mut written: Vec<(usize, String)> = Vec::new();
        for (slot, offset, size) in part_blocks(part).context(&entry.name)? {
            // slots sharing a block (LODs often do) name the first one's file
            if let Some((_, file)) = written.iter().find(|(o, _)| *o == offset) {
                slots.insert(slot.into(), json!(file));
                continue;
            }
            let block = &part[offset..offset + size];
            let prefix = format!("{}/", entry.name);
            // a model the reader cannot convert is kept as its bytes (<slot>.raw)
            let file = match export_model(block, &prefix, slot, slot_collision(slot), &textures) {
                Ok(Some(files)) => {
                    out.extend(files);
                    format!("{prefix}{slot}.gltf")
                }
                Ok(None) | Err(_) => {
                    out.push((format!("{prefix}{slot}.raw"), block.to_vec()));
                    format!("{prefix}{slot}.raw")
                }
            };
            written.push((offset, file.clone()));
            slots.insert(slot.into(), json!(file));
        }
        parts.push(json!({"name": entry.name, "info": info_json(part).context(&entry.name)?, "models": slots}));
    }
    out.push((
        "scene.json".into(),
        json_bytes(&json!({
            "note": "position and rotation may be edited; cells may be edited at the same size; the models are rebuilt from their .gltf / .bin",
            "parts": parts,
        })),
    ));
    Ok(out)
}

fn put_f32(data: &mut [u8], at: usize, value: f64) {
    data[at..at + 4].copy_from_slice(&(value as f32).to_bits().to_le_bytes());
}

/// A part with scene.json's info edits and the folder's models put in: a changed block goes on
/// the end (16-byte aligned) and every slot that pointed at the old one points at it.
fn build_part(folder: &Path, name: &str, part: &[u8], info: Option<&Value>, textures: &TextureSearch) -> Result<Vec<u8>> {
    let end = part_end(part)?;
    let mut out = part[..end].to_vec();
    if let Some(info) = info {
        let at = i32_at(part, 4)? as usize;
        for (key, offset) in [("position", 0x24), ("rotation", 0x30)] {
            if let Some(values) = info[key].as_array() {
                for (i, value) in values.iter().take(3).enumerate() {
                    let value = value.as_f64().ok_or_else(|| Error::invalid(format!("{name}: {key} is not numbers")))?;
                    if f32_at(part, at + offset + i * 4)? as f64 != value {
                        put_f32(&mut out, at + offset + i * 4, value);
                    }
                }
            }
        }
        if let Some(rows) = info["cells"].as_array() {
            let width = i32_at(part, at + 8)?.max(0) as usize;
            let cells_at = at + i32_at(part, at + 0x3C)? as usize;
            for (row, text) in rows.iter().enumerate() {
                let bytes = unhex(text.as_str().unwrap_or_default())?;
                if bytes.len() != width {
                    bail!("{name}: cell row {row} has {} cells, not {width}", bytes.len());
                }
                out[cells_at + row * width..cells_at + (row + 1) * width].copy_from_slice(&bytes);
            }
        }
    }
    let blocks = part_blocks(part)?;
    let mut done: Vec<usize> = Vec::new();
    for &(slot, offset, size) in &blocks {
        if done.contains(&offset) {
            continue;
        }
        done.push(offset);
        let block = &part[offset..offset + size];
        let dir = folder.join(name);
        let built = if block.starts_with(b"MDS\0") && dir.join(format!("{slot}.gltf")).is_file() {
            build_model(&dir, slot, block, slot_collision(slot), textures)?
        } else {
            std::fs::read(dir.join(format!("{slot}.raw"))).unwrap_or_else(|_| block.to_vec())
        };
        if built == block {
            continue;
        }
        while out.len() % 16 != 0 {
            out.push(0);
        }
        let new_offset = out.len() as i32;
        out.extend(&built);
        for &(_, offset_at, size_at) in SLOTS.iter() {
            if i32_at(part, size_at)? > 0 && i32_at(part, offset_at)? as usize == offset {
                out[offset_at..offset_at + 4].copy_from_slice(&new_offset.to_le_bytes());
                out[size_at..size_at + 4].copy_from_slice(&(built.len() as i32).to_le_bytes());
            }
        }
    }
    Ok(out)
}

/// The scene with a folder's scene.json edits and models put in. A part that changes is written
/// over its old bytes when it still fits before the next part, else appended (64-byte aligned).
pub fn scene_build(folder: &Path, data: &[u8], images: &[(String, Vec<u8>)]) -> Result<Vec<u8>> {
    let textures = textures_of(images)?;
    let scene = read_json(&folder.join("scene.json")).ok();
    let entries = scn_entries(data)?;
    let mut out = data.to_vec();
    for (i, entry) in entries.iter().enumerate() {
        let part = &data[entry.offset..];
        let info = scene.as_ref().and_then(|scene| {
            scene["parts"].as_array()?.iter().find(|p| p["name"] == json!(entry.name)).map(|p| &p["info"])
        });
        let end = part_end(part)?;
        let built = build_part(folder, &entry.name, part, info, &textures).context(&entry.name)?;
        if built == part[..end] {
            continue;
        }
        let room = entries
            .iter()
            .map(|e| e.offset)
            .filter(|&o| o > entry.offset)
            .min()
            .map_or(usize::MAX, |next| next - entry.offset);
        if built.len() <= room.max(end) && entry.offset + built.len() <= out.len() {
            out[entry.offset..entry.offset + built.len()].copy_from_slice(&built);
        } else {
            while out.len() % 64 != 0 {
                out.push(0);
            }
            let at = out.len() as i32;
            out.extend(&built);
            let field = SCN_HEADER + SCN_ENTRY * i + 16;
            out[field..field + 4].copy_from_slice(&at.to_le_bytes());
        }
    }
    Ok(out)
}

// ---- Georama layouts ----

/// A town layout (gdata0.edt) as JSON: every placed part with its rotation and position.
pub fn layout_export(data: &[u8]) -> Result<Vec<u8>> {
    let (count, size, offset, unknown) = (i16_at(data, 0)?, i16_at(data, 2)?, i16_at(data, 4)?, i16_at(data, 6)?);
    let mut parts = Vec::new();
    for i in 0..count.max(0) as usize {
        let at = offset as usize + i * 16;
        parts.push(json!({
            "part": i16_at(data, at)?,
            "rotation": i16_at(data, at + 2)?,
            "position": [f32_at(data, at + 4)?, f32_at(data, at + 8)?, f32_at(data, at + 12)?],
        }));
    }
    let end = offset as usize + count.max(0) as usize * 16;
    Ok(json_bytes(&json!({
        "header": {"size": size, "offset": offset, "unknown": unknown},
        "parts": parts,
        "tail": hex(data.get(end..).unwrap_or_default()),
    })))
}

/// The layout JSON back to its file. With the part list unchanged in length, the header and the
/// tail (the end record and what follows) are kept; otherwise a fresh end record is written.
pub fn layout_build(json_text: &[u8]) -> Result<Vec<u8>> {
    let value: Value = serde_json::from_slice(json_text).map_err(|e| Error::invalid(e.to_string()))?;
    let parts = value["parts"].as_array().ok_or_else(|| Error::invalid("no parts list"))?;
    let header = &value["header"];
    let offset = header["offset"].as_i64().unwrap_or(8) as usize;
    let mut out = vec![0u8; offset];
    out[0..2].copy_from_slice(&(parts.len() as i16).to_le_bytes());
    out[6..8].copy_from_slice(&(header["unknown"].as_i64().unwrap_or(0) as i16).to_le_bytes());
    if offset >= 6 {
        out[4..6].copy_from_slice(&(offset as i16).to_le_bytes());
    }
    for part in parts {
        let number = |key: &str| part[key].as_i64().ok_or_else(|| Error::invalid(format!("a part's {key} is not a number")));
        out.extend((number("part")? as i16).to_le_bytes());
        out.extend((number("rotation")? as i16).to_le_bytes());
        let position = part["position"].as_array().filter(|p| p.len() == 3).ok_or_else(|| Error::invalid("a part's position is not 3 numbers"))?;
        for value in position {
            out.extend((value.as_f64().unwrap_or(0.0) as f32).to_bits().to_le_bytes());
        }
    }
    let tail = unhex(value["tail"].as_str().unwrap_or_default())?;
    let same_count = header["size"].as_i64().is_some_and(|size| size as usize == out.len() + tail.len());
    if same_count {
        out.extend(tail);
    } else {
        out.extend((-1i16).to_le_bytes());
        out.extend((-1i16).to_le_bytes());
        out.extend([0u8; 12]);
    }
    let size = out.len() as i16;
    out[2..4].copy_from_slice(&size.to_le_bytes());
    Ok(out)
}

/// Folder helpers for the CLI: files of a folder by name (no recursion).
pub fn images_in(folder: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(folder) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("img")) {
                if let Ok(data) = std::fs::read(&path) {
                    out.push((path.file_name().unwrap().to_string_lossy().to_string(), data));
                }
            }
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A layout of two parts and its end record (editground.cpp CEditGround::Save).
    fn sample_layout() -> Vec<u8> {
        let mut out = Vec::new();
        for value in [2i16, 0x38, 8, 0] {
            out.extend(value.to_le_bytes());
        }
        for (part, rotation, position) in [(15i16, 1i16, [-551.5f32, 0.0, 149.9]), (3, 0, [12.0, 0.0, -40.0])] {
            out.extend(part.to_le_bytes());
            out.extend(rotation.to_le_bytes());
            for value in position {
                out.extend(value.to_bits().to_le_bytes());
            }
        }
        out.extend((-1i16).to_le_bytes());
        out.extend((-1i16).to_le_bytes());
        out.extend([0u8; 12]);
        out
    }

    #[test]
    fn a_layout_reads_and_writes_back() {
        let data = sample_layout();
        assert_eq!(layout_build(&layout_export(&data).unwrap()).unwrap(), data);
    }

    #[test]
    fn a_layout_takes_a_moved_and_an_added_part() {
        let mut value: Value = serde_json::from_slice(&layout_export(&sample_layout()).unwrap()).unwrap();
        value["parts"][0]["rotation"] = json!(2);
        value["parts"].as_array_mut().unwrap().push(json!({"part": 7, "rotation": 3, "position": [1.0, 2.0, 3.0]}));
        let built = layout_build(serde_json::to_vec(&value).unwrap().as_slice()).unwrap();
        let again: Value = serde_json::from_slice(&layout_export(&built).unwrap()).unwrap();
        assert_eq!(again["parts"][0]["rotation"], json!(2));
        assert_eq!(again["parts"][2]["part"], json!(7));
        assert_eq!(i16_at(&built, 2).unwrap() as usize, built.len());
    }
}
