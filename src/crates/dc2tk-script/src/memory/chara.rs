//! Character packs: what building one takes, and the party's packs' stacks.

use super::*;

pub(super) fn text_arg(args: &[Arg], index: usize) -> Option<String> {
    match args.get(index) {
        Some(Arg::Str(text)) => Some(text.clone()),
        _ => None,
    }
}

pub(super) fn int_arg(args: &[Arg], index: usize) -> Option<i64> {
    match args.get(index) {
        Some(Arg::Int(value)) => Some(*value),
        Some(Arg::Float(value)) => Some(*value as i64),
        _ => None,
    }
}

/// The base of a frame name: up to "--" (StrCmp, mg_frame.cpp:948).
fn frame_base(name: &[u8]) -> &[u8] {
    let end = name.windows(2).position(|pair| pair == b"--").unwrap_or(name.len());
    &name[..end]
}

/// What a character pack takes of the stack it is loaded into (CCharacter2::LoadChrFile,
/// ScanInfoFile, character.cpp:1286): its pictures copied (_IMG), its model and shadow model
/// built, its motions converted, its key, outline, sound, cloth and effect tables.
pub fn chara_pack(stack: &mut Stack, pack: &[u8]) -> Option<()> {
    let info = member(pack, "info.cfg")?;
    let statements = info_cfg::parse(info);
    let mut vertex_anime: Vec<Vec<u8>> = Vec::new();
    let mut shape_anime = false;
    let mut keys = 0u64;
    let encode = |name: &str| text::encode(name).unwrap_or_else(|| name.as_bytes().to_vec());
    for (tag, args) in &statements {
        match tag.as_str() {
            "VERTEX_ANIME" => {
                for index in 0..args.len() {
                    if vertex_anime.len() < 24 {
                        if let Some(name) = text_arg(args, index) {
                            vertex_anime.push(frame_base(&encode(&name)).to_vec());
                        }
                    }
                }
            }
            "SHAPE_ANIME" => shape_anime = int_arg(args, 0).unwrap_or(0) != 0,
            "IMG" | "SKIN_IMG" => {
                if let Some(data) = text_arg(args, 1).and_then(|name| member(pack, &name)) {
                    stack.alloc64(data.len() as u64 / 16 + 1);
                }
            }
            "MODEL" | "LOD_MODEL" => {
                let Some(name) = text_arg(args, 0) else { continue };
                let Some(mds) = member(pack, &name) else { continue };
                let stem = name.split('.').next().unwrap_or(&name);
                let weight = member(pack, &format!("{stem}.wgt"));
                let skinned = |frame: &[u8]| {
                    if vertex_anime.iter().any(|wanted| wanted.as_slice() == frame_base(frame)) {
                        if shape_anime {
                            Visual::Copied
                        } else {
                            Visual::Skinned
                        }
                    } else {
                        Visual::Fixed
                    }
                };
                if vertex_anime.is_empty() {
                    model(stack, mds, &|_| Visual::Fixed, None)?;
                } else {
                    model(stack, mds, &skinned, weight)?;
                }
            }
            "SHADOW_MODEL" => {
                if let Some(mds) = text_arg(args, 0).and_then(|name| member(pack, &name)) {
                    model(stack, mds, &|_| Visual::Shadow, None)?;
                }
            }
            "MOTION" | "SHADOW_MOTION" => {
                // MOTION id, mot, bbp[, wgt]; SHADOW_MOTION mot, bbp, wgt
                let skip = if tag == "MOTION" { 1 } else { 0 };
                if let Some(bbp) = text_arg(args, skip + 1).and_then(|name| member(pack, &name)) {
                    stack.alloc(bbp.len() as u64 / 16 + 1);
                }
                if let Some(mot) = text_arg(args, skip).and_then(|name| member(pack, &name)) {
                    key_list_memory(stack, mot);
                }
                if tag == "SHADOW_MOTION" {
                    if let Some(wgt) = text_arg(args, 2).and_then(|name| member(pack, &name)) {
                        key_list_memory(stack, wgt);
                    }
                }
            }
            "KEY_START" => keys = 0,
            "KEY" => keys += 1,
            // the key table, a 0x30-byte CHRINFO_KEY_SET per key and one to end it
            "KEY_END" => stack.alloc64((keys + 1) * 0x30 / 16 + 1),
            "OUTLINE" => stack.alloc(9),
            "SE_START" => stack.alloc(2),
            "SE" | "SELP" => stack.alloc(1),
            "CLOTH_START" => stack.alloc(blocks(int_arg(args, 0).unwrap_or(0).max(0) as u64 * 0x90) + 2),
            _ => {}
        }
    }
    Some(())
}

/// Estimated bytes a character pack takes of the stack it loads into.
pub fn chara_pack_bytes(pack: &[u8]) -> Option<u64> {
    let mut stack = Stack::default();
    chara_pack(&mut stack, pack)?;
    Some(stack.bytes())
}

/// The skinned meshes of a character pack that break the fixed limits of _MODEL: more vertices
/// than its work buffer holds, or more bones than a mesh may use. (frame, vertices, bones).
pub fn skinned_limits(pack: &[u8]) -> Vec<(String, u64, usize)> {
    let mut out = Vec::new();
    let Some(info) = member(pack, "info.cfg") else { return out };
    let statements = info_cfg::parse(info);
    let mut vertex_anime: Vec<Vec<u8>> = Vec::new();
    for (tag, args) in &statements {
        if tag == "VERTEX_ANIME" {
            for index in 0..args.len() {
                if let Some(name) = text_arg(args, index) {
                    vertex_anime.push(frame_base(&text::encode(&name).unwrap_or_default()).to_vec());
                }
            }
        }
        if tag != "MODEL" || vertex_anime.is_empty() {
            continue;
        }
        let Some(name) = text_arg(args, 0) else { continue };
        let (Some(mds), Some(weight)) =
            (member(pack, &name), member(pack, &format!("{}.wgt", name.split('.').next().unwrap_or(&name))))
        else {
            continue;
        };
        let bones = bones_by_frame(weight);
        for (index, object) in objects(mds).unwrap_or_default().iter().enumerate() {
            let Some(mesh) = &object.mesh else { continue };
            if !vertex_anime.iter().any(|wanted| wanted.as_slice() == frame_base(&object.name)) {
                continue;
            }
            let used = bones.get(&(index as u32)).map_or(0, Vec::len);
            if mesh.vertices > SKINNED_VERTICES || used > SKINNED_BONES {
                out.push((text::decode_lossy(&object.name), mesh.vertices, used));
            }
        }
    }
    out
}

// The party's packs

/// The stack a party pack builds into, by its path: (stack, quadwords, where). Max's and Monica's
/// model packs into stack 0, the sword into 2, the second weapon into 3, the hat or accessory
/// into 4 (CCharacter2::LoadPack), the costume into 1 and the shoes into 5 (LoadSkin), in
/// dungeons from mainchr/ and in town the model pack from chara/ (SetupMainUnit).
pub fn party_stack(path: &str) -> Option<(usize, u64, &'static str)> {
    let lower = path.to_lowercase();
    let (folder, name) = lower.rsplit_once('/')?;
    let name = name.strip_suffix(".chr")?;
    let family = name.trim_end_matches(|c: char| c.is_ascii_digit());
    if folder == "chara" {
        return matches!(name, "c01_base" | "c02_base").then_some((0, TOWN_PARTY_STACKS[0], "town"));
    }
    if folder != "mainchr" {
        return None;
    }
    let stack = match family {
        "c01_base" | "c02_base" => 0,
        "c01fuku" | "c02fuku" => 1,
        "ws" | "wm" => 2,
        "wg" | "wr" => 3,
        "ha" | "ac" => 4,
        "sh" | "bo" => 5,
        _ => return None,
    };
    Some((stack, DUNGEON_PARTY_STACKS[stack], "dungeon"))
}
