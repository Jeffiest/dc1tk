//! What changed in an extracted disc since it was extracted, and a mod made of those changes, so
//! editing the files dc2tk setup wrote is enough to make a mod:
//!
//! - extracted/scripts: the scripts and libraries edited or added since decompiling (the tree's
//!   record, as dc2tk changes reads it). The mod carries them whole, as mod save does.
//! - extracted/assets: textures, models and sounds whose files no longer have the hash the asset
//!   tree's manifest gives. The mod carries the files and a replace line by each asset's id, so the
//!   change reaches every place the disc has the asset.
//! - extracted/disc: files of DATA.DAT and pack members whose bytes differ from the disc's, and
//!   files added there. The mod carries them under raw/ with a file: line each; a new file goes on
//!   the disc, a new member into its pack.
//!
//! The disc's own files beside DATA.DAT (movies, SOUND.DAT, the indexes, the boot program) are
//! reported but cannot go in a mod: build writes DATA.DAT's files.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::Path;

use dc2tk_disc::archive::{Archive, Entry};
use dc2tk_disc::layout::{self, Layout};
use dc2tk_disc::Image;
use rayon::prelude::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::discfiles;
use crate::replace::{self, FILE};
use crate::{mods, tree, Error, Result};

/// The folders under the extracted folder.
pub const DISC: &str = "disc";
pub const SCRIPTS: &str = "scripts";
pub const ASSETS: &str = "assets";

/// What changed, by area. Paths are from each area's folder, with "/".
#[derive(Default, Debug)]
pub struct Changes {
    pub scripts_edited: Vec<String>,
    pub scripts_added: Vec<String>,
    pub scripts_removed: Vec<String>,
    /// Asset ids whose files changed, with the changed files.
    pub assets_changed: BTreeMap<String, Vec<String>>,
    /// Files in the asset tree no asset lists.
    pub assets_added: Vec<String>,
    pub assets_removed: Vec<String>,
    /// DATA.DAT files and pack members whose bytes differ from the disc's.
    pub disc_changed: Vec<String>,
    /// Files in the extracted disc that the disc does not have.
    pub disc_added: Vec<String>,
    pub disc_removed: Vec<String>,
    /// Members changed inside a pack that is itself changed: the pack's own file wins.
    pub disc_shadowed: Vec<String>,
    /// The disc's own files (beside DATA.DAT) that changed: build cannot write them.
    pub unsupported: Vec<String>,
    /// Areas that could not be compared, and why.
    pub skipped: Vec<String>,
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        self.scripts_edited.is_empty()
            && self.scripts_added.is_empty()
            && self.assets_changed.is_empty()
            && self.disc_changed.is_empty()
            && self.disc_added.is_empty()
    }
}

fn hash(path: &Path) -> Option<String> {
    let data = fs::read(path).ok()?;
    Some(Sha256::digest(&data).iter().map(|b| format!("{b:02x}")).collect())
}

/// Files under a folder, by path from it with "/", sorted.
fn files_under(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        let Ok(listing) = fs::read_dir(&folder) else {
            continue;
        };
        for entry in listing.flatten() {
            let path = entry.path();
            if path.is_dir() {
                folders.push(path);
            } else {
                found.push(path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"));
            }
        }
    }
    found.sort();
    found
}

fn scripts(tree_dir: &Path, changes: &mut Changes) -> Result<()> {
    match tree::status(tree_dir)? {
        Some(status) => {
            changes.scripts_edited = status.edited;
            changes.scripts_added = status.added;
            changes.scripts_removed = status.removed;
        }
        None => changes.skipped.push(format!(
            "{}: no record of what the decompiler wrote ({}), so edits cannot be told from its output",
            tree_dir.display(),
            tree::ORIGINALS
        )),
    }
    Ok(())
}

fn assets(assets_dir: &Path, changes: &mut Changes) -> Result<()> {
    let manifest_path = assets_dir.join("manifest.json");
    if !manifest_path.is_file() {
        changes.skipped.push(format!("{}: no asset tree (dc2tk assets writes one)", assets_dir.display()));
        return Ok(());
    }
    let manifest: Value = serde_json::from_str(&fs::read_to_string(&manifest_path)?)
        .map_err(|error| Error::invalid(format!("{}: {error}", manifest_path.display())))?;
    let mut listed: Vec<(String, String, String)> = Vec::new();
    for (id, asset) in manifest["assets"].as_object().into_iter().flatten() {
        for (file, expected) in asset["files"].as_object().into_iter().flatten() {
            listed.push((id.clone(), file.clone(), expected.as_str().unwrap_or("").to_string()));
        }
    }
    let known: HashSet<String> = listed.iter().map(|(_, file, _)| file.to_lowercase()).collect();
    let results: Vec<(String, String, Option<bool>)> = listed
        .par_iter()
        .map(|(id, file, expected)| {
            let found = hash(&assets_dir.join(file));
            (id.clone(), file.clone(), found.map(|found| found != *expected))
        })
        .collect();
    for (id, file, changed) in results {
        match changed {
            Some(true) => changes.assets_changed.entry(id).or_default().push(file),
            Some(false) => {}
            None => changes.assets_removed.push(file),
        }
    }
    changes.assets_added = files_under(assets_dir)
        .into_iter()
        .filter(|file| file != "manifest.json" && !known.contains(&file.to_lowercase()))
        .collect();
    Ok(())
}

/// Every copy of the bytes the disc has at a path of the extracted disc: a DATA.DAT file, or a
/// member of a pack (a.chr.d/b.img). A pack can hold two members of one name (and the index two
/// files differing in case), which extract writes as one file holding the last.
fn disc_copies<'a>(
    image: &'a [u8],
    disc: &Archive,
    entries: &HashMap<String, Vec<&Entry>>,
    path: &str,
) -> Vec<&'a [u8]> {
    let (top, members) = discfiles::location(path);
    let mut found: Vec<&'a [u8]> =
        entries.get(&top.to_lowercase()).into_iter().flatten().map(|entry| disc.read(image, entry)).collect();
    for name in members {
        let wanted = name.to_lowercase();
        found = found
            .into_iter()
            .filter_map(dc2tk_disc::archive::parse_pack_with_data)
            .flat_map(|members| {
                members.into_iter().filter(|(name, _)| name.to_lowercase() == wanted).map(|(_, data)| data)
            })
            .collect();
    }
    found
}

fn disc(disc_dir: &Path, iso_path: &Path, changes: &mut Changes) -> Result<()> {
    let manifest_path = disc_dir.join("manifest.json");
    if !manifest_path.is_file() {
        changes.skipped.push(format!("{}: not an extracted disc (no manifest.json)", disc_dir.display()));
        return Ok(());
    }
    let manifest: Value = serde_json::from_str(&fs::read_to_string(&manifest_path)?)
        .map_err(|error| Error::invalid(format!("{}: {error}", manifest_path.display())))?;
    let listed: Vec<String> = manifest["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item["path"].as_str().map(str::to_string))
        .collect();
    let image = Image::open(iso_path)?;
    let archive = Archive::open(image.bytes())?;
    let mut entries: HashMap<String, Vec<&Entry>> = HashMap::new();
    for entry in &archive.entries {
        entries.entry(entry.path.to_lowercase()).or_default().push(entry);
    }
    let layout: Option<Layout> = layout::read_layout(disc_dir).ok();
    // files the disc lists twice in other case are one file on a case-insensitive file system
    let mut unique: Vec<&String> = Vec::new();
    let mut seen = HashSet::new();
    for path in &listed {
        if seen.insert(path.to_lowercase()) {
            unique.push(path);
        }
    }
    let compared: Vec<(String, Option<bool>)> = unique
        .par_iter()
        .map(|path| {
            let local = fs::read(disc_dir.join(path)).ok();
            let changed = local.map(|local| {
                !disc_copies(image.bytes(), &archive, &entries, path).iter().any(|copy| *copy == &local[..])
            });
            ((*path).clone(), changed)
        })
        .collect();
    let mut changed = Vec::new();
    for (path, outcome) in compared {
        match outcome {
            Some(true) => changed.push(path),
            Some(false) => {}
            None => changes.disc_removed.push(path),
        }
    }
    // a changed pack carries its members: changed members inside it are left to it
    let changed_set: BTreeSet<String> = changed.iter().map(|path| path.to_lowercase()).collect();
    for path in changed {
        let lower = path.to_lowercase();
        let parts: Vec<&str> = lower.split(discfiles::PACK).collect();
        let inside_changed = (1..parts.len()).any(|end| changed_set.contains(&parts[..end].join(discfiles::PACK)));
        if inside_changed {
            changes.disc_shadowed.push(path);
        } else {
            changes.disc_changed.push(path);
        }
    }
    // the disc's own files
    let own: HashSet<String> = match &layout {
        Some(layout) => {
            for file in &layout.files {
                let bytes = dc2tk_disc::iso::read_at(
                    image.bytes(),
                    file.extent as u64 * dc2tk_disc::iso::SECTOR,
                    file.size as u64,
                );
                if fs::read(disc_dir.join(&file.path)).ok().as_deref() != Some(bytes) {
                    changes.unsupported.push(file.path.clone());
                }
            }
            layout.files.iter().map(|file| file.path.to_lowercase()).collect()
        }
        None => {
            changes.skipped.push(format!(
                "{}: no {} (extracted before dc2tk recorded the disc's layout), so the disc's own files were not compared",
                disc_dir.display(),
                layout::LAYOUT
            ));
            manifest["boot"].as_str().map(|boot| boot.to_lowercase()).into_iter().collect()
        }
    };
    let known: HashSet<String> = listed.iter().map(|path| path.to_lowercase()).collect();
    let bookkeeping = ["manifest.json", layout::LAYOUT, layout::STRUCTURE, "map_index.txt"];
    changes.disc_added = files_under(disc_dir)
        .into_iter()
        .filter(|path| {
            let lower = path.to_lowercase();
            !known.contains(&lower)
                && !own.contains(&lower)
                && !bookkeeping.iter().any(|name| name.eq_ignore_ascii_case(&lower))
        })
        .collect();
    Ok(())
}

/// What changed in the extracted folder (disc/, scripts/, assets/) since the disc was extracted.
pub fn changes(extracted: &Path, iso_path: &Path) -> Result<Changes> {
    let mut found = Changes::default();
    scripts(&extracted.join(SCRIPTS), &mut found)?;
    assets(&extracted.join(ASSETS), &mut found)?;
    disc(&extracted.join(DISC), iso_path, &mut found)?;
    Ok(found)
}

/// The changes as dc2tk diff prints them.
pub fn report(changes: &Changes) -> String {
    let mut lines = Vec::new();
    let mut section = |title: &str, rows: Vec<String>| {
        if !rows.is_empty() {
            lines.push(format!("{title} ({})", rows.len()));
            lines.extend(rows.into_iter().map(|row| format!("  {row}")));
        }
    };
    let scripts: Vec<String> = changes
        .scripts_edited
        .iter()
        .map(|path| format!("edited   {path}"))
        .chain(changes.scripts_added.iter().map(|path| format!("added    {path}")))
        .chain(changes.scripts_removed.iter().map(|path| format!("removed  {path} (a mod cannot remove it)")))
        .collect();
    section("scripts", scripts);
    let assets: Vec<String> = changes
        .assets_changed
        .iter()
        .map(|(id, files)| format!("changed  {id}  ({})", files.join(", ")))
        .chain(changes.assets_added.iter().map(|path| format!("added    {path} (no asset is made of it; left out)")))
        .chain(
            changes.assets_removed.iter().map(|path| format!("removed  {path} (the asset stays as the disc has it)")),
        )
        .collect();
    section("assets", assets);
    let disc: Vec<String> = changes
        .disc_changed
        .iter()
        .map(|path| format!("changed  {path}"))
        .chain(changes.disc_added.iter().map(|path| format!("added    {path}")))
        .chain(
            changes
                .disc_shadowed
                .iter()
                .map(|path| format!("changed  {path} (inside a changed pack, which carries it)")),
        )
        .chain(changes.disc_removed.iter().map(|path| format!("removed  {path} (a mod cannot remove it)")))
        .chain(
            changes
                .unsupported
                .iter()
                .map(|path| format!("changed  {path} (one of the disc's own files; build cannot write it)")),
        )
        .collect();
    section("disc files", disc);
    for note in &changes.skipped {
        lines.push(format!("note: {note}"));
    }
    if changes.is_empty() {
        lines.push("nothing a mod can carry has changed since extracting".into());
    }
    lines.join("\n")
}

/// The name an asset's file takes in the mod: the id without its kind, safe as a file name.
fn safe(id: &str) -> String {
    let name = id.split_once(':').map_or(id, |(_, name)| name);
    name.chars().map(|c| if c.is_alphanumeric() || "_.-@".contains(c) { c } else { '_' }).collect()
}

fn copy(from: &Path, to: &Path) -> Result<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(from, to).map_err(|error| Error::invalid(format!("{}: {error}", from.display())))?;
    Ok(())
}

/// What mod make wrote.
pub struct Made {
    pub scripts: Vec<String>,
    pub lines: Vec<(String, Value)>,
}

/// Makes a mod of the changes: the scripts whole (files/), changed assets by id (textures/,
/// models/, sounds/) and changed or added disc files (raw/), each with its mod.json line.
pub fn make(extracted: &Path, mod_dir: &Path, changes: &Changes) -> Result<Made> {
    if mod_dir.exists() && fs::read_dir(mod_dir)?.next().is_some() {
        return Err(Error::invalid(format!("{} is there already; choose another name", mod_dir.display())));
    }
    if changes.is_empty() {
        return Err(Error::invalid("nothing a mod can carry has changed since extracting"));
    }
    fs::create_dir_all(mod_dir)?;
    let tree_dir = extracted.join(SCRIPTS);
    let script_paths: Vec<String> = changes.scripts_edited.iter().chain(&changes.scripts_added).cloned().collect();
    let scripts =
        if script_paths.is_empty() { Vec::new() } else { mods::add(&tree_dir, mod_dir, &script_paths, true)? };
    let mut lines: Vec<(String, Value)> = Vec::new();
    let assets_dir = extracted.join(ASSETS);
    for (id, files) in &changes.assets_changed {
        let kind = id.split(':').next().unwrap_or("");
        let target = match kind {
            "texture" => {
                let file = &files[0];
                let target = format!("textures/{}.png", safe(id));
                copy(&assets_dir.join(file), &mod_dir.join(&target))?;
                target
            }
            "model" => {
                // the glTF and its buffer, whichever changed
                let manifest: Value = serde_json::from_str(&fs::read_to_string(assets_dir.join("manifest.json"))?)
                    .map_err(|error| Error::invalid(error.to_string()))?;
                let mut gltf = None;
                for file in manifest["assets"][id]["files"].as_object().into_iter().flatten().map(|(file, _)| file) {
                    let name = file.rsplit('/').next().unwrap_or(file);
                    let target = format!("models/{}/{}", safe(id), name);
                    copy(&assets_dir.join(file), &mod_dir.join(&target))?;
                    if name.to_lowercase().ends_with(".gltf") {
                        gltf = Some(target);
                    }
                }
                gltf.ok_or_else(|| Error::invalid(format!("{id} has no glTF in the asset tree")))?
            }
            "music" => {
                // the MIDI file; the SoundFont beside it is only for listening
                let Some(file) = files.iter().find(|file| file.to_lowercase().ends_with(dc2tk_assets::music::SUFFIX))
                else {
                    continue;
                };
                let name = file.rsplit('/').next().unwrap_or(file);
                let target = format!("music/{name}");
                copy(&assets_dir.join(file), &mod_dir.join(&target))?;
                target
            }
            _ => {
                let file = &files[0];
                let name = file.rsplit('/').next().unwrap_or(file);
                let folder = file.rsplit('/').nth(1).unwrap_or("");
                let target = format!("sounds/{folder}/{name}");
                copy(&assets_dir.join(file), &mod_dir.join(&target))?;
                target
            }
        };
        lines.push((id.clone(), json!(target)));
    }
    let disc_dir = extracted.join(DISC);
    for path in changes.disc_changed.iter().chain(&changes.disc_added) {
        let target = format!("raw/{path}");
        copy(&disc_dir.join(path), &mod_dir.join(&target))?;
        lines.push((format!("{FILE}{path}"), json!(target)));
    }
    if !lines.is_empty() {
        replace::add_lines(mod_dir, &lines)?;
    }
    Ok(Made { scripts, lines })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_and_new_files_in_an_extracted_disc_make_a_mod() {
        let folder = std::env::temp_dir().join(format!("dc2tk-capture-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        let image = dc2tk_disc::samples::sample_disc();
        let iso = folder.join("disc.iso");
        fs::create_dir_all(&folder).unwrap();
        fs::write(&iso, &image).unwrap();
        let extracted = folder.join("extracted");
        dc2tk_disc::extract::extract(&image, &extracted.join(DISC)).unwrap();
        let unchanged = changes(&extracted, &iso).unwrap();
        assert!(unchanged.is_empty(), "{}", report(&unchanged));
        fs::write(extracted.join("disc/readme.txt"), b"HELLO").unwrap();
        fs::write(extracted.join("disc/chara/c01.chr.d/new.cfg"), b"MODEL x;").unwrap();
        fs::create_dir_all(extracted.join("disc/added")).unwrap();
        fs::write(extracted.join("disc/added/file.txt"), b"new").unwrap();
        let found = changes(&extracted, &iso).unwrap();
        assert_eq!(found.disc_changed, ["readme.txt"]);
        assert_eq!(found.disc_added, ["added/file.txt", "chara/c01.chr.d/new.cfg"]);
        assert!(found.skipped.iter().any(|note| note.contains("scripts")));
        let mod_dir = folder.join("mods/captured");
        let made = make(&extracted, &mod_dir, &found).unwrap();
        let keys: Vec<&str> = made.lines.iter().map(|(key, _)| key.as_str()).collect();
        assert_eq!(keys, ["file:readme.txt", "file:added/file.txt", "file:chara/c01.chr.d/new.cfg"]);
        assert_eq!(fs::read(mod_dir.join("raw/readme.txt")).unwrap(), b"HELLO");
        let lines = replace::lines(&mod_dir).unwrap();
        assert_eq!(lines.len(), 3);
        // a second mod of the same name is refused
        assert!(make(&extracted, &mod_dir, &found).is_err());
        fs::remove_dir_all(&folder).unwrap();
    }
}
