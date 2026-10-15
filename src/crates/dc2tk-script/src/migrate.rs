//! mod migrate: a mod's disc/ folder (game files laid out by where they sit on the disc, see
//! discfiles) turned into mod.json's replace lines by asset id (see replace), so that the mod no
//! longer depends on paths. Each picture PNG becomes a texture id's line, each WAV a sound id's, each
//! model folder a model id's; what has no id (a raw game file, a bank folder adding entries) becomes
//! a file: line. The files move out of disc/ (which build would still read) into textures/,
//! models/, sounds/ and raw/.
//!
//! The migrated mod builds the same image: a line whose id the disc has at more places than the
//! old file covered gets "only" with the places it covered.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use dc2tk_assets::ids::Index;
use dc2tk_assets::tree::{self, Kind};
use dc2tk_assets::{im3, sounds, textures};
use dc2tk_disc::DiscSource;
use indexmap::IndexMap;
use serde_json::Value;

use crate::discfiles::{self, DISC};
use crate::replace::{self, FILE};
use crate::{mods, paths, Error, Result};

/// A line migrate writes: the key, the file in the mod, the old files it comes from, and the
/// places the old files covered.
struct Planned {
    key: String,
    target: String,
    sources: Vec<PathBuf>,
    places: Vec<String>,
    /// Every place of the id, when it has more than the old files covered.
    all: usize,
}

/// mod.json's lines: (key, value).
type Lines = Vec<(String, Value)>;

/// What migrate did: the lines written, and notes.
pub struct Migrated {
    pub lines: Lines,
    pub notes: Vec<String>,
}

fn same_content(a: &Path, b: &Path) -> Result<bool> {
    if a.is_dir() != b.is_dir() {
        return Ok(false);
    }
    if !a.is_dir() {
        return Ok(fs::read(a)? == fs::read(b)?);
    }
    let (left, right) = (discfiles::files_in(a)?, discfiles::files_in(b)?);
    if left != right {
        return Ok(false);
    }
    for path in &left {
        if fs::read(paths::join_relative(a, path))? != fs::read(paths::join_relative(b, path))? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The name part of an id, as a file name.
fn stem(id: &str) -> String {
    tree::file_stem(id.split_once(':').map_or(id, |(_, name)| name))
}

/// The lines planned for one disc/ file or folder: (id, the file's place in the mod, the file).
type ById = Vec<(String, String, PathBuf)>;

/// The disc's ids by (kind, place, entry): Index::at.
type Ids = HashMap<(Kind, String, Option<usize>), String>;

/// Plans the lines for one disc/ file or folder; None when it has no id (it becomes a file: line).
fn by_id(path: &str, source: &Path, data: &[u8], index: &Index, at: &Ids) -> Result<Option<ById>> {
    let place = path.to_lowercase();
    let Some(converter) = discfiles::conversion(path, source) else {
        return Ok(None);
    };
    match converter.suffix {
        ".png" => pictures_by_id(&place, source, data, at),
        ".wav" => sounds_by_id(&place, source, index, at),
        ".mid" => music_by_id(&place, source, at),
        ".txt" => Ok(text_by_id(place, source, at)),
        _ => Ok(model_by_id(place, source, at)),
    }
}

/// A texture file's folder of PNGs: each a texture id's line, found as textures::build finds its
/// picture (by name, or name_index). None when a PNG is no picture of the file's.
fn pictures_by_id(place: &str, source: &Path, data: &[u8], at: &Ids) -> Result<Option<ById>> {
    let Ok(images) = im3::read(data) else {
        return Ok(None);
    };
    let mut by_name: BTreeMap<String, usize> = BTreeMap::new();
    for (position, image) in images.iter().enumerate() {
        by_name.entry(textures::file_name(&image.name)).or_insert(position);
        by_name.entry(textures::file_name(&format!("{}_{}", image.name, image.index.unwrap_or(0)))).or_insert(position);
    }
    let mut out = Vec::new();
    for name in discfiles::files_in(source)? {
        if name.contains('/') || !name.to_lowercase().ends_with(textures::SUFFIX) {
            continue;
        }
        let Some(id) = by_name
            .get(&name)
            .and_then(|&position| at.get(&(Kind::Texture, place.to_string(), images[position].index)))
        else {
            return Ok(None);
        };
        out.push((id.clone(), format!("textures/{}.png", stem(id)), paths::join_relative(source, &name)));
    }
    Ok(Some(out))
}

/// A bank's folder of WAVs: each a sound id's line. None when one adds an entry the bank lacks.
fn sounds_by_id(place: &str, source: &Path, index: &Index, at: &Ids) -> Result<Option<ById>> {
    let Some(bank) = at.get(&(Kind::Bank, place.to_string(), None)) else {
        return Ok(None);
    };
    let count = index.assets[bank].entries.unwrap_or(0);
    let wavs = sounds::numbered(source)?;
    if wavs.iter().any(|(number, _, _)| *number >= count) {
        return Ok(None);
    }
    let bank_name = &bank[Kind::Bank.name().len() + 1..];
    let mut out = Vec::new();
    for (number, _, file) in wavs {
        let Some(id) = at.get(&(Kind::Sound, place.to_string(), Some(number))) else {
            return Ok(None);
        };
        let name = id[Kind::Sound.name().len() + 1..].to_string();
        let named = (!name.contains('/')).then_some(name.as_str());
        let target = format!("sounds/{}/{}", tree::file_stem(bank_name), tree::wav_name(number, named, bank_name));
        out.push((id.clone(), target, file));
    }
    Ok(Some(out))
}

/// A music bank's folder: each MIDI file a sequence's line (BG_000_000.mid for its
/// BG_000_000.sq); the SoundFont beside them is only for listening.
fn music_by_id(place: &str, source: &Path, at: &Ids) -> Result<Option<ById>> {
    let mut out = Vec::new();
    for name in discfiles::files_in(source)? {
        let lower = name.to_lowercase();
        if name.contains('/') || !lower.ends_with(dc2tk_assets::music::SUFFIX) {
            continue;
        }
        let sequence = &lower[..lower.len() - dc2tk_assets::music::SUFFIX.len()];
        let member = format!("{place}{}{sequence}.sq", discfiles::PACK);
        let Some(id) = at.get(&(Kind::Music, member, None)) else {
            return Ok(None);
        };
        let target = format!("music/{}{}", stem(id), dc2tk_assets::music::SUFFIX);
        out.push((id.clone(), target, paths::join_relative(source, &name)));
    }
    Ok(Some(out))
}

/// A message file's text: a text id's line.
fn text_by_id(place: String, source: &Path, at: &Ids) -> Option<ById> {
    let id = at.get(&(Kind::Text, place, None))?;
    let target = dc2tk_assets::text::text_file(&id[Kind::Text.name().len() + 1..]);
    Some(vec![(id.clone(), target, source.to_path_buf())])
}

/// A character pack's model folder: a model id's line.
fn model_by_id(place: String, source: &Path, at: &Ids) -> Option<ById> {
    let id = at.get(&(Kind::Model, place, None))?;
    Some(vec![(id.clone(), format!("models/{}", stem(id)), source.to_path_buf())])
}

/// Migrates the mod's disc/ folder, with the disc image its files replace and the disc's index of
/// ids. Nothing is moved or written when any line cannot be made.
pub fn migrate(mod_dir: &Path, iso_path: &Path, index: &Index) -> Result<Migrated> {
    let found = discfiles::mod_files(mod_dir)?;
    if found.is_empty() {
        return Err(Error::invalid(format!("{} has no {DISC}/ folder to migrate", mod_dir.display())));
    }
    let (planned, used) = planned(mod_dir, &found, iso_path, index)?;
    let (written, mut notes) = lines_of(mod_dir, &planned, index)?;
    moved(mod_dir, &planned, &used, &mut notes)?;
    replace::add_lines(mod_dir, &written)?;
    replace::lines(mod_dir)?;
    Ok(Migrated { lines: written, notes })
}

/// The lines for every disc/ file, an id's places gathered into one line (refused when they give
/// it differently), and every file a line takes.
fn planned(
    mod_dir: &Path,
    found: &IndexMap<String, PathBuf>,
    iso_path: &Path,
    index: &Index,
) -> Result<(Vec<Planned>, Vec<PathBuf>)> {
    let root = mod_dir.join(DISC);
    let disc = DiscSource::open(iso_path)?;
    let at = index.at();
    let mut planned: Vec<Planned> = Vec::new();
    let mut used: Vec<PathBuf> = Vec::new();
    for (path, source) in found {
        let (top, members) = discfiles::location(path);
        let lines = match disc.member(&top, &members)? {
            Some(data) => by_id(path, source, &data, index, &at)?,
            None => None,
        };
        let lines = lines.unwrap_or_else(|| {
            let relative = source.strip_prefix(&root).unwrap_or(source).to_string_lossy().replace('\\', "/");
            vec![(format!("{FILE}{path}"), format!("raw/{relative}"), source.clone())]
        });
        for (key, target, file) in lines {
            used.push(file.clone());
            match planned.iter_mut().find(|known| known.key == key) {
                Some(known) => {
                    if !same_content(&known.sources[0], &file)? {
                        return Err(Error::invalid(format!(
                            "{key} is given differently at {} and {path}; keep one and migrate again",
                            known.places.join(", ")
                        )));
                    }
                    known.sources.push(file);
                    known.places.push(path.clone());
                }
                None => {
                    let all = index.get(&key).map_or(0, |asset| asset.places.len());
                    planned.push(Planned { key, target, sources: vec![file], places: vec![path.clone()], all });
                }
            }
        }
    }
    Ok((planned, used))
}

/// mod.json's line for each planned one, with "only" where the id is at more places than the old
/// files covered, and notes saying so. A line mod.json has already, a file in the way, or two
/// lines moving a file to one place is refused.
fn lines_of(mod_dir: &Path, planned: &[Planned], index: &Index) -> Result<(Lines, Vec<String>)> {
    let known: Vec<String> = replace::lines(mod_dir)?.into_iter().map(|line| line.key.to_lowercase()).collect();
    let mut written = Vec::new();
    let mut notes = Vec::new();
    for plan in planned {
        if known.contains(&plan.key.to_lowercase()) {
            return Err(Error::invalid(format!("{} has a line for {} already", mods::INDEX, plan.key)));
        }
        let target = paths::join_relative(mod_dir, &plan.target);
        if target.exists() {
            return Err(Error::invalid(format!(
                "{} is there already; move it away and migrate again",
                target.display()
            )));
        }
        let mut places: Vec<String> = match index.get(&plan.key) {
            // the disc's own spelling of each place
            Some(asset) => asset
                .places
                .iter()
                .map(|(place, _)| place.display())
                .filter(|shown| plan.places.iter().any(|path| path.to_lowercase() == shown.to_lowercase()))
                .collect(),
            None => plan.places.clone(),
        };
        places.dedup();
        let value = if plan.all > places.len() && !plan.key.starts_with(FILE) {
            notes.push(format!(
                "{} is at {} places; the mod changed {} of them, so its line has \"only\" (remove it to change them all)",
                plan.key,
                plan.all,
                places.len()
            ));
            serde_json::json!({"file": plan.target, "only": places})
        } else {
            Value::String(plan.target.clone())
        };
        written.push((plan.key.clone(), value));
    }
    let targets: Vec<&String> = planned.iter().map(|plan| &plan.target).collect();
    if let Some(twice) =
        targets.iter().enumerate().find_map(|(at, target)| targets[..at].contains(target).then_some(target))
    {
        return Err(Error::invalid(format!("two lines would move a file to {twice}")));
    }
    Ok((written, notes))
}

/// Moves each line's file into place, then whatever disc/ still holds into unused/ (a copy of a
/// file a line has into unused/duplicates/), saying so, and removes disc/.
fn moved(mod_dir: &Path, planned: &[Planned], used: &[PathBuf], notes: &mut Vec<String>) -> Result<()> {
    let root = mod_dir.join(DISC);
    for plan in planned {
        let target = paths::join_relative(mod_dir, &plan.target);
        fs::create_dir_all(target.parent().unwrap())?;
        fs::rename(&plan.sources[0], &target)?;
    }
    for relative in discfiles::files_in(&root)? {
        let from = paths::join_relative(&root, &relative);
        if !from.is_file() {
            continue;
        }
        let duplicate = used.iter().any(|file| from.starts_with(file));
        let target =
            paths::join_relative(&mod_dir.join(if duplicate { "unused/duplicates" } else { "unused" }), &relative);
        fs::create_dir_all(target.parent().unwrap())?;
        fs::rename(&from, &target)?;
        let why = if duplicate { "a copy of a file a line has" } else { "used by no line" };
        notes.push(format!(
            "{DISC}/{relative} is {why}; moved to {}",
            target.strip_prefix(mod_dir).unwrap_or(&target).display()
        ));
    }
    fs::remove_dir_all(&root)?;
    Ok(())
}
