//! The check build runs over the loads a mod's files change.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// A budget that is certain is exceeded: the game will not load it as made.
    Error,
    /// An estimated budget is exceeded, or a load leaves what the disc shows to work.
    Warning,
    /// What a changed load uses.
    Note,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub level: Level,
    pub text: String,
}

pub(super) fn finding(level: Level, text: String) -> Finding {
    Finding { level, text }
}

/// Thousands separated, for reading.
pub(super) fn n(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// The files a map's load reads.
fn map_paths(entry: &MapEntry) -> Vec<String> {
    let mut out = Vec::new();
    let (folder, name) = map_folder(&entry.name);
    for extension in ["map", "cfg", "mpk", "ipk", "efp", "sky"] {
        out.push(format!("{folder}/{name}.{extension}"));
    }
    if !entry.add.is_empty() {
        let (folder, name) = add_folder(&entry.add);
        for extension in ["map", "cfg", "mpk", "ipk"] {
            out.push(format!("{folder}/{name}.{extension}"));
        }
    }
    out
}

/// A map file's map name (map/s/s17/s17.mpk -> s17), when it is a map's own file.
pub(super) fn map_of(path: &str) -> Option<String> {
    let lower = path.to_lowercase();
    let rest = lower.strip_prefix("map/")?;
    if rest.starts_with("cmn/") {
        return None;
    }
    let (_, file) = rest.rsplit_once('/')?;
    let (stem, extension) = file.rsplit_once('.')?;
    ["map", "cfg", "mpk", "ipk", "efp", "sky"].contains(&extension).then(|| stem.to_string())
}

/// What `build` says about the memory the changed files' loads use: an error where a budget that
/// is certain is exceeded, a warning where an estimated one is (and the load grew), a note of what
/// each changed load uses. `changed` are the DATA.DAT paths the build rewrites or adds.
pub fn check(original: &dyn Disc, modded: &dyn Disc, changed: &[String]) -> Vec<Finding> {
    // a file written back with its own bytes changes no load
    let changed: Vec<String> =
        changed.iter().filter(|path| modded.read(path) != original.read(path)).cloned().collect();
    let changed = changed.as_slice();
    let mut out = Vec::new();
    let mut covered: Vec<String> = Vec::new();

    // maps whose files changed, listed or found by name
    let mut entries = map_entries(modded);
    for path in changed {
        if let Some(name) = map_of(path) {
            if !entries.iter().any(|entry| entry.name.eq_ignore_ascii_case(&name)) {
                let kind = match name.chars().next() {
                    Some('d') => MapKind::Dungeon,
                    Some('i') => MapKind::Interior,
                    _ => MapKind::Town,
                };
                entries.push(MapEntry { name, add: String::new(), kind });
            }
        }
    }
    for entry in &entries {
        let paths = map_paths(entry);
        let touched: Vec<&String> = changed.iter().filter(|path| paths.contains(&path.to_lowercase())).collect();
        if touched.is_empty() {
            continue;
        }
        covered.extend(touched.iter().map(|path| path.to_string()));
        out.extend(check_map(original, modded, entry));
    }
    for path in changed {
        let lower = path.to_lowercase();
        if lower.ends_with(".efp") && lower.starts_with("map/") {
            out.extend(check_shared_images(original, modded, path, "map effects", 1));
        } else if lower.ends_with(".sky") {
            covered.push(path.clone());
            out.extend(check_sky(original, modded, path));
        } else if lower.ends_with(".chr") {
            covered.push(path.clone());
            out.extend(check_chara(original, modded, path));
        } else if lower.ends_with(".snd") {
            covered.push(path.clone());
            out.extend(check_sound(original, modded, path));
        } else if lower.ends_with(".stb") && lower.starts_with("map/") {
            covered.push(path.clone());
            out.extend(check_script(original, modded, path));
        }
    }
    for path in changed {
        if covered.contains(path) {
            continue;
        }
        let (Some(new), old) = (modded.read(path), original.read(path)) else { continue };
        let old_size = old.as_ref().map_or(0, |data| data.len());
        if new.len() > old_size {
            out.push(finding(
                Level::Note,
                format!(
                    "{path}: {} bytes ({}); no budget of docs/assets/memory.md covers this file, so keep it near its original size",
                    n(new.len() as u64),
                    if old.is_some() { format!("was {}", n(old_size as u64)) } else { "a new file".to_string() }
                ),
            ));
        }
    }
    out
}

fn check_map(original: &dyn Disc, modded: &dyn Disc, entry: &MapEntry) -> Vec<Finding> {
    let mut out = Vec::new();
    let name = &entry.name;
    let kind = entry.kind;
    let Some(new) = map_load(modded, entry) else { return out };
    let old = map_load(original, entry);
    let blocks = map_blocks(modded, entry).unwrap_or_default();
    let old_blocks = map_blocks(original, entry);
    if new.read > READ_BUFFER {
        out.push(finding(
            Level::Error,
            format!(
                "map {name}: its .map, .cfg, .mpk and sky read into the read buffer take {} bytes, more than its {} (LoadMapData)",
                n(new.read),
                n(READ_BUFFER)
            ),
        ));
    } else if new.read > kind.loading_screen() {
        let level = if kind == MapKind::Dungeon { Level::Warning } else { Level::Error };
        out.push(finding(
            level,
            format!(
                "map {name}: its .map, .cfg, .mpk and sky take {} bytes of the read buffer, past {} where the loading screen's packets lie while the first map loads",
                n(new.read),
                n(kind.loading_screen())
            ),
        ));
    }
    if new.placements > kind.placements() {
        out.push(finding(
            Level::Error,
            format!(
                "map {name}: {} placements, more than the {} its placement table holds (SetPlacePartsBuff); the rest are not placed",
                new.placements,
                kind.placements()
            ),
        ));
    }
    let old_used = old_blocks.map_or(0, |blocks| blocks.blocks);
    let (limit, limit_text) = match kind {
        MapKind::Dungeon => (DUNGEON_EFFECT_BLOCK, "its effects' block 0xF"),
        MapKind::Interior => (TOWN_EFFECT_BLOCK - TOWN_INTERIOR_BLOCK, "the main map's effects at block 0x40"),
        MapKind::Town => (TOWN_EFFECT_BLOCK, "its effects' block 0x40"),
    };
    if blocks.blocks > limit {
        out.push(finding(
            Level::Error,
            format!(
                "map {name}: its pictures take {} texture blocks, reaching {limit_text} (it has {limit}); pictures past it are overwritten",
                blocks.blocks
            ),
        ));
    } else if kind == MapKind::Town && blocks.blocks > TOWN_INTERIOR_BLOCK && old_used <= TOWN_INTERIOR_BLOCK {
        out.push(finding(
            Level::Warning,
            format!(
                "map {name}: its pictures take {} texture blocks, past the 24 a town map has when an interior loads beside it (from block 0x18); fine only if no interior or Palm Brinks part loads with it",
                blocks.blocks
            ),
        ));
    }
    if let Some(old) = &old {
        if new.stack > kind.retail() && new.stack > old.stack {
            out.push(finding(
                Level::Warning,
                format!(
                    "map {name}: estimated {} bytes of map memory, more than the largest {} on the disc ({}); its villagers and events share what is left, so it may run short (estimated)",
                    n(new.stack),
                    kind.describe(),
                    n(kind.retail())
                ),
            ));
        }
    }
    let was = |value: u64, old: Option<u64>| match old {
        Some(old) if old != value => format!(" (was {})", n(old)),
        _ => String::new(),
    };
    out.push(finding(
        Level::Note,
        format!(
            "map {name} ({}): read buffer {}{} of {}; placements {} of {}; texture blocks {}{} of {}; map memory about {}{} (largest {} on the disc {})",
            kind.describe(),
            n(new.read),
            was(new.read, old.as_ref().map(|old| old.read)),
            n(READ_BUFFER),
            new.placements,
            kind.placements(),
            blocks.blocks,
            match old_blocks {
                Some(old) if old.blocks != blocks.blocks => format!(" (was {})", old.blocks),
                _ => String::new(),
            },
            if kind == MapKind::Town { "24 beside an interior, 64 alone".to_string() } else { limit.to_string() },
            n(new.stack),
            was(new.stack, old.as_ref().map(|old| old.stack)),
            kind.describe(),
            n(kind.retail())
        ),
    ));
    out
}

/// Picture files a pack enters at one block: compared with one block and with the original.
fn check_shared_images(original: &dyn Disc, modded: &dyn Disc, path: &str, what: &str, blocks: i64) -> Vec<Finding> {
    let span = |disc: &dyn Disc| {
        disc.read(path).map(|data| {
            let images: Vec<&[u8]> = img_members(&data).into_iter().map(|(_, img)| img).collect();
            shared_block(&images)
        })
    };
    let (Some(new), old) = (span(modded), span(original)) else { return Vec::new() };
    let old_blocks = old.map_or(0, |old| old.blocks);
    if new.blocks > blocks && new.blocks > old_blocks {
        return vec![finding(
            Level::Warning,
            format!(
                "{path}: the {what}' pictures take {} texture blocks where they have {blocks}; the next block's owner overwrites the rest",
                new.blocks
            ),
        )];
    }
    Vec::new()
}

/// A sky's four picture files each go into a block of their own (CMapSky::LoadPack enters sky0 to
/// sky3 at the sky's block + 0 to 3, and deletes each block first), so each must fit in one.
fn check_sky(original: &dyn Disc, modded: &dyn Disc, path: &str) -> Vec<Finding> {
    let mut out = check_read_whole(original, modded, path, "sky");
    let Some(data) = modded.read(path) else { return out };
    for (name, img) in img_members(&data) {
        let used = shared_block(&[img]);
        if used.blocks > 1 {
            out.push(finding(
                Level::Error,
                format!(
                    "{path}: {name} needs {} texture blocks; a sky picture file has one (the next one's load deletes the rest)",
                    used.blocks
                ),
            ));
        }
    }
    out
}

/// A map's event script: read into the script buffer, which then holds the interpreter's stack
/// and call tables (SetEventScript, event.cpp:106: 0x40 and 0x180 quadwords).
fn check_script(original: &dyn Disc, modded: &dyn Disc, path: &str) -> Vec<Finding> {
    let Some(new) = modded.read(path) else { return Vec::new() };
    let old = original.read(path).map(|data| data.len() as u64);
    let size = new.len() as u64;
    if size > SCRIPT_FILE {
        return vec![finding(
            Level::Error,
            format!(
                "{path}: {} bytes; a map's script and the interpreter's tables share a {}-byte script buffer, which holds a script of at most {} bytes (LoadScript, SetEventScript)",
                n(size),
                n(SCRIPT_BUFFER * QW),
                n(SCRIPT_FILE)
            ),
        )];
    }
    if old.is_some_and(|old| size > old) {
        return vec![finding(Level::Note, format!("{path}: script {} bytes of at most {}", n(size), n(SCRIPT_FILE)))];
    }
    Vec::new()
}

pub(super) fn check_read_whole(original: &dyn Disc, modded: &dyn Disc, path: &str, what: &str) -> Vec<Finding> {
    let Some(new) = modded.read(path) else { return Vec::new() };
    let old = original.read(path).map_or(0, |data| data.len());
    if new.len() as u64 > READ_BUFFER {
        return vec![finding(
            Level::Error,
            format!(
                "{path}: {} bytes; a {what} is read whole into the {}-byte read buffer",
                n(new.len() as u64),
                n(READ_BUFFER)
            ),
        )];
    }
    if new.len() > old {
        return vec![finding(
            Level::Note,
            format!("{path}: {} bytes (read buffer {}), was {}", n(new.len() as u64), n(READ_BUFFER), n(old as u64)),
        )];
    }
    Vec::new()
}

fn check_chara(original: &dyn Disc, modded: &dyn Disc, path: &str) -> Vec<Finding> {
    let mut out = check_read_whole(original, modded, path, "character pack");
    let Some(new) = modded.read(path) else { return out };
    let old = original.read(path);
    let old_limits = old.as_ref().map(|old| skinned_limits(old)).unwrap_or_default();
    for (frame, vertices, bones) in skinned_limits(&new) {
        if old_limits.iter().any(|(name, _, _)| *name == frame) {
            continue;
        }
        let what = if vertices > SKINNED_VERTICES {
            format!("{vertices} vertices, more than the {SKINNED_VERTICES} the skinning work buffer holds (_MODEL)")
        } else {
            format!("{bones} bones, more than the {SKINNED_BONES} a skinned mesh may use (CreateVertexWeight drops the rest)")
        };
        out.push(finding(Level::Error, format!("{path}: skinned mesh {frame} has {what}")));
    }
    let estimate = chara_pack_bytes(&new);
    let old_estimate = old.as_ref().and_then(|old| chara_pack_bytes(old));
    if let (Some((stack, quadwords, mode)), Some(estimate)) = (party_stack(path), estimate) {
        let budget = quadwords * QW;
        if estimate > budget && Some(estimate) > old_estimate {
            out.push(finding(
                Level::Warning,
                format!(
                    "{path}: estimated {} bytes, more than the {} of the party's memory stack {stack} it loads into in {mode}s (mem_table); the game prints \"stack over\" and the pack loads incomplete",
                    n(estimate),
                    n(budget)
                ),
            ));
        }
        out.push(finding(
            Level::Note,
            format!(
                "{path}: party stack {stack} ({mode}): about {}{} of {}",
                n(estimate),
                match old_estimate {
                    Some(old) if old != estimate => format!(" (was {})", n(old)),
                    _ => String::new(),
                },
                n(budget)
            ),
        ));
    } else if let (Some(estimate), Some(old_estimate)) = (estimate, old_estimate) {
        if estimate > old_estimate {
            out.push(finding(
                Level::Note,
                format!(
                    "{path}: about {} bytes of the stack it loads into, was {} (event, villager and monster stacks share what a scene leaves; keep near the original)",
                    n(estimate),
                    n(old_estimate)
                ),
            ));
        }
    }
    let blocks = shared_block(&pack_images(&new));
    let old_blocks = old.as_ref().map_or(0, |old| shared_block(&pack_images(old)).blocks);
    let room = if party_stack(path).is_some() { 8 } else { 1 };
    if blocks.blocks > room && blocks.blocks > old_blocks {
        out.push(finding(
            Level::Warning,
            format!(
                "{path}: its pictures take {} texture blocks where it has {room}; the next character's pictures overwrite the rest",
                blocks.blocks
            ),
        ));
    }
    out
}
