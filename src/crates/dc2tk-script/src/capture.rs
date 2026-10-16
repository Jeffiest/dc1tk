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

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::Path;

use dc2tk_assets::manifest::Manifest;
use dc2tk_assets::tree::MANIFEST as ASSET_MANIFEST;
use dc2tk_disc::archive;
use dc2tk_disc::extract::{self, MANIFEST};
use dc2tk_disc::files;
use dc2tk_disc::layout::{self, Layout};
use dc2tk_disc::DiscSource;
use rayon::prelude::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::discfiles;
use crate::replace::{self, FILE};
use crate::{mods, paths, tree, Context, Error, Result};

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
    let data = files::read(path).ok()?;
    Some(Sha256::digest(&data).iter().map(|b| format!("{b:02x}")).collect())
}

/// Files under a folder, by path from it with "/", sorted.
fn files_under(root: &Path) -> Result<Vec<String>> {
    let mut found = paths::walk(root, |_| true)?;
    found.sort();
    Ok(found)
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

/// The asset tree's manifest (dc2tk_assets::manifest), read by its typed form.
fn asset_manifest(assets_dir: &Path) -> Result<Manifest> {
    let path = assets_dir.join(ASSET_MANIFEST);
    let data = fs::read(&path).with_context(|| format!("cannot read {}", path.display()))?;
    Manifest::read(&data).with_context(|| assets_dir.display().to_string())
}

fn assets(assets_dir: &Path, changes: &mut Changes) -> Result<()> {
    if !assets_dir.join(ASSET_MANIFEST).is_file() {
        changes.skipped.push(format!("{}: no asset tree (dc2tk assets writes one)", assets_dir.display()));
        return Ok(());
    }
    let manifest = asset_manifest(assets_dir)?;
    let mut listed: Vec<(String, String, String)> = Vec::new();
    for (id, asset) in &manifest.assets {
        for (file, expected) in asset.files().into_iter().flatten() {
            listed.push((id.clone(), file.clone(), expected.clone()));
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
    changes.assets_added = files_under(assets_dir)?
        .into_iter()
        .filter(|file| file != ASSET_MANIFEST && !known.contains(&file.to_lowercase()))
        .collect();
    Ok(())
}

/// Every copy of the bytes the disc has at a path of the extracted disc: a DATA.DAT file, or a
/// member of a pack (a.chr.d/b.img). A pack can hold two members of one name (and the index two
/// files differing in case), which extract writes as one file holding the last.
fn disc_copies<'a>(disc: &'a DiscSource, path: &str) -> Result<Vec<Cow<'a, [u8]>>> {
    let (top, members) = discfiles::location(path);
    let mut found = disc.copies(&top)?;
    for name in members {
        found = found.into_iter().flat_map(|pack| members_named(pack, &name)).collect();
    }
    Ok(found)
}

/// A pack's members of a name in any case; none when it is not a pack.
fn members_named<'a>(pack: Cow<'a, [u8]>, name: &str) -> Vec<Cow<'a, [u8]>> {
    fn named<'b>(pack: &'b [u8], wanted: &str) -> Vec<&'b [u8]> {
        let members = archive::parse_pack_with_data(pack).unwrap_or_default();
        members.into_iter().filter(|(member, _)| member.to_lowercase() == wanted).map(|(_, data)| data).collect()
    }
    let wanted = name.to_lowercase();
    match pack {
        Cow::Borrowed(pack) => named(pack, &wanted).into_iter().map(Cow::Borrowed).collect(),
        Cow::Owned(pack) => named(&pack, &wanted).into_iter().map(|data| Cow::Owned(data.to_vec())).collect(),
    }
}

fn disc(disc_dir: &Path, iso_path: &Path, changes: &mut Changes) -> Result<()> {
    let manifest_path = disc_dir.join(MANIFEST);
    if !manifest_path.is_file() {
        changes.skipped.push(format!("{}: not an extracted disc (no {MANIFEST})", disc_dir.display()));
        return Ok(());
    }
    let text = fs::read(&manifest_path).with_context(|| format!("cannot read {}", manifest_path.display()))?;
    let manifest: extract::Manifest = serde_json::from_slice(&text)
        .map_err(|error| Error::invalid(format!("{}: {error}", manifest_path.display())))?;
    let listed: Vec<String> = manifest.files.iter().map(|item| item.path.clone()).collect();
    let original = DiscSource::open(iso_path)?;
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
            let changed = match fs::read(disc_dir.join(path)) {
                Ok(local) => Some(!disc_copies(&original, path)?.iter().any(|copy| copy[..] == local[..])),
                Err(_) => None,
            };
            Ok(((*path).clone(), changed))
        })
        .collect::<Result<_>>()?;
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
                let bytes = original.disc_file(&file.path).ok();
                if fs::read(disc_dir.join(&file.path)).ok().as_deref() != bytes.as_deref() {
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
            manifest.boot.iter().map(|boot| boot.to_lowercase()).collect()
        }
    };
    let known: HashSet<String> = listed.iter().map(|path| path.to_lowercase()).collect();
    let bookkeeping = [MANIFEST, layout::LAYOUT, layout::STRUCTURE, "map_index.txt"];
    changes.disc_added = files_under(disc_dir)?
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
        files::create_dir_all(parent)?;
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
    if mod_dir.exists() && files::read_dir(mod_dir)?.next().is_some() {
        return Err(Error::invalid(format!("{} is there already; choose another name", mod_dir.display())));
    }
    if changes.is_empty() {
        return Err(Error::invalid("nothing a mod can carry has changed since extracting"));
    }
    files::create_dir_all(mod_dir)?;
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
                let manifest = asset_manifest(&assets_dir)?;
                let mut gltf = None;
                for file in
                    manifest.assets.get(id).and_then(|asset| asset.files()).into_iter().flatten().map(|(file, _)| file)
                {
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

    #[test]
    fn an_unedited_asset_tree_has_no_changes() {
        use dc2tk_assets::soundbank::test_bank;
        use dc2tk_assets::tree::{self as asset_tree, DiscFile};
        let pack = |name: &str, data: &[u8]| archive::write_pack(&[(name, data)]).unwrap();
        // a bank, a copy whose volumes differ (every entry the first's sound), and a version
        // whose entry 2 plays at another rate (that entry a sound of its own)
        let disc = [
            ("snd2/sp/SP_009.snd".to_string(), test_bank(&[22050, 11025], 100)),
            ("menu/m.pac".to_string(), pack("SP_009.snd", &test_bank(&[22050, 11025], 90))),
            ("menu/n.pac".to_string(), pack("SP_009.snd", &test_bank(&[22050, 16000], 100))),
        ];
        let files: Vec<DiscFile> = disc.iter().map(|(path, data)| DiscFile { path: path.clone(), data }).collect();
        let tree_dir = std::env::temp_dir().join(format!("dc2tk-capture-assets-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tree_dir);
        let write = |path: &str, data: &[u8]| -> dc2tk_assets::Result<()> {
            let path = tree_dir.join(path);
            fs::create_dir_all(path.parent().unwrap())?;
            Ok(fs::write(path, data)?)
        };
        asset_tree::build(&files, &asset_tree::sound_names(""), &write).unwrap();
        let manifest = asset_manifest(&tree_dir).unwrap();
        assert_eq!(manifest.aliases["sound:SP_009@m/1"], "sound:SP_009/1");
        let mut found = Changes::default();
        assets(&tree_dir, &mut found).unwrap();
        fs::remove_dir_all(&tree_dir).unwrap();
        assert!(found.assets_changed.is_empty(), "{found:?}");
        assert!(found.assets_added.is_empty(), "{found:?}");
        assert!(found.assets_removed.is_empty(), "{found:?}");
        assert!(found.skipped.is_empty(), "{found:?}");
    }
}
