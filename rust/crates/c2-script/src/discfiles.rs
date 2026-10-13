//! Game files a mod replaces: models, textures, sounds and anything else on the disc. A mod keeps
//! them under disc/, laid out as extract lays the disc out: a file at its path in DATA.DAT
//! (snd2/sp/sp_000.snd), or a member of a pack under the pack's name plus ".d"
//! (chara/c01_base.chr.d/c01b.img for the texture inside Max's character pack). build writes each
//! over the disc's own; members of packs inside packs work the same way (a.pac.d/b.chr.d/c.img).
//!
//! A game file can also be given in a common format, named after the file it stands for plus the
//! format's suffix: build converts it, with the disc's own file as the template (palette size,
//! sample rate, layout), and mod asset --as exports to it. Textures are a folder of PNGs
//! (c02a.img.png/c02a01.png), sound banks a folder of WAVs by entry number
//! (sp_000.snd.wav/003.wav) and character packs a folder holding a glTF with its textures
//! (c02a.chr.gltf/c02a.gltf); c2_assets converts them.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use c2_disc::archive::{self, Archive, Entry};
use c2_disc::{rebuild, Image};
use indexmap::IndexMap;

use crate::{Error, Result};

pub const DISC: &str = "disc";
pub const PACK: &str = ".d/";

/// A converter between a game format and a common one: the common format's suffix, the suffix of
/// the game files it converts, and whether the common form is a folder of files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Converter {
    pub suffix: &'static str,
    pub game: &'static str,
    pub folder: bool,
}

pub const CONVERTERS: [Converter; 3] = [
    Converter { suffix: ".png", game: ".img", folder: true },
    Converter { suffix: ".wav", game: ".snd", folder: true },
    Converter { suffix: ".gltf", game: ".chr", folder: true },
];

impl Converter {
    /// The game file's bytes from the common file or folder, with the disc's own file as the
    /// template.
    pub fn build(&self, source: &Path, original: &[u8]) -> Result<Vec<u8>> {
        let built = match self.suffix {
            ".png" => c2_assets::textures::build(source, original),
            ".wav" => c2_assets::sounds::build(source, original),
            _ => c2_assets::models::build(source, original),
        };
        built.map_err(|error| Error::invalid(error.to_string()))
    }

    /// The disc's own file in the common format: {file name: bytes} of the folder.
    pub fn export(&self, data: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
        let exported = match self.suffix {
            ".png" => c2_assets::textures::export(data),
            ".wav" => c2_assets::sounds::export(data),
            _ => c2_assets::models::export(data),
        };
        exported.map_err(|error| Error::invalid(error.to_string()))
    }
}

fn lower(text: &str) -> String {
    text.to_lowercase()
}

/// The game file a path in a common format stands for, and its converter; the path itself and
/// None for a game file.
pub fn converter_for(path: &str, folder: bool) -> (String, Option<Converter>) {
    for converter in CONVERTERS {
        if lower(path).ends_with(converter.suffix) && converter.folder == folder {
            let stem = &path[..path.len() - converter.suffix.len()];
            if lower(stem).ends_with(converter.game) {
                return (stem.to_string(), Some(converter));
            }
        }
    }
    (path.to_string(), None)
}

/// The folder in a common format a mod file is inside (c02a.img.png/ for c02a.img.png/c02a01.png).
pub fn converter_folder(relative: &str) -> Option<String> {
    let parts: Vec<&str> = relative.split('/').collect();
    (1..parts.len()).map(|end| parts[..end].join("/")).find(|folder| converter_for(folder, true).1.is_some())
}

/// Files under a folder, sorted as Python sorts Windows paths (by lowercased parts), as paths
/// from the folder with "/".
pub fn files_in(root: &Path) -> Vec<String> {
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
    found.sort_by_key(|path| -> Vec<String> {
        let path = if cfg!(windows) { path.to_lowercase() } else { path.clone() };
        path.split('/').map(str::to_string).collect()
    });
    found
}

fn name_of(path: &Path) -> String {
    path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
}

fn joined(root: &Path, relative: &str) -> PathBuf {
    relative.split('/').fold(root.to_path_buf(), |path, part| path.join(part))
}

/// {disc path: file or folder} for the game files a mod holds; a file or folder in a common format
/// is keyed by the game file it stands for.
pub fn mod_files(mod_dir: &Path) -> Result<IndexMap<String, PathBuf>> {
    let root = mod_dir.join(DISC);
    let mut found: IndexMap<String, PathBuf> = IndexMap::new();
    if !root.is_dir() {
        return Ok(found);
    }
    for relative in files_in(&root) {
        let folder = converter_folder(&relative);
        let source = joined(&root, folder.as_deref().unwrap_or(&relative));
        let (target, _) = match &folder {
            Some(folder) => converter_for(folder, true),
            None => converter_for(&relative, false),
        };
        if let Some(existing) = found.get(&target).filter(|existing| **existing != source) {
            return Err(Error::invalid(format!(
                "{}: {} is given twice, as {} and {}",
                name_of(mod_dir),
                target,
                name_of(existing),
                name_of(&source)
            )));
        }
        found.insert(target, source);
    }
    Ok(found)
}

pub fn converter_of(source: &Path) -> Option<Converter> {
    converter_for(&source.to_string_lossy().replace('\\', "/"), source.is_dir()).1
}

/// The game file's bytes for a mod file: the file itself, or converted from its common format.
pub fn game_bytes(source: &Path, original: &[u8]) -> Result<Vec<u8>> {
    match converter_of(source) {
        None => Ok(fs::read(source)?),
        Some(converter) => converter.build(source, original),
    }
}

/// The file of DATA.DAT a disc path is in, and the members down to it: chara/c01_base.chr.d/c01b.img
/// is member c01b.img of chara/c01_base.chr.
pub fn location(path: &str) -> (String, Vec<String>) {
    let mut parts = path.split(PACK).map(str::to_string);
    let top = parts.next().unwrap_or_default();
    (top, parts.collect())
}

/// DATA.DAT files by path.
pub type Files = BTreeMap<String, Vec<u8>>;

/// {disc path: (mod name, file)} for the chosen mods, and the paths two of them replace.
pub type Wanted = IndexMap<String, (String, PathBuf)>;

/// {path: the mods that change it}.
pub type Clashes = IndexMap<String, Vec<String>>;

pub fn collect(mod_dirs: &[PathBuf]) -> Result<(Wanted, Clashes)> {
    let mut found = Wanted::new();
    let mut clashes: IndexMap<String, Vec<String>> = IndexMap::new();
    for mod_dir in mod_dirs {
        for (path, file) in mod_files(mod_dir)? {
            if let Some((owner, _)) = found.get(&path) {
                clashes.entry(path).or_insert_with(|| vec![owner.clone()]).push(name_of(mod_dir));
            } else {
                found.insert(path, (name_of(mod_dir), file));
            }
        }
    }
    Ok((found, clashes))
}

pub fn clash_text(clashes: &IndexMap<String, Vec<String>>) -> String {
    let sorted: BTreeMap<&String, &Vec<String>> = clashes.iter().collect();
    sorted
        .iter()
        .map(|(path, names)| format!("{} is replaced by more than one chosen mod: {}", path, names.join(", ")))
        .collect::<Vec<_>>()
        .join("\n")
}

pub const MAX_MEMBER_NAME: usize = archive::PACK_NAME - 1;

/// The pack with a new member added at the end of its chain (or of the chain of a pack inside it),
/// so the members it had keep their positions: the engine finds members by name, ignoring case
/// (GetPackFile in dataread.cpp), but some loaders walk a pack by position or take every member
/// with an extension.
pub fn added_member(data: &[u8], names: &[String], new: &[u8]) -> Result<Vec<u8>> {
    let members = archive::parse_pack(data).ok_or_else(|| Error::invalid("not a pack"))?;
    if names.len() > 1 {
        let Some(member) = rebuild::member_named(&members, &names[0], false) else {
            return Err(Error::invalid(format!("no pack {} to add {} to", names[0], names[1..].join("/"))));
        };
        let inner = added_member(member.data(data), &names[1..], new)?;
        return Ok(rebuild::replaced_member(data, &names[..1], &inner)?);
    }
    let name = &names[0];
    if members.iter().any(|member| lower(&member.name) == lower(name)) {
        return Err(Error::invalid(format!(
            "the pack has a member {name} already, in other case; the game would find that one"
        )));
    }
    let raw = c2_disc::text::encode(name)
        .ok_or_else(|| Error::invalid(format!("'cp932' codec can't encode the member name {name}")))?;
    if raw.len() > MAX_MEMBER_NAME {
        return Err(Error::invalid(format!("member names are at most {MAX_MEMBER_NAME} bytes")));
    }
    let last = members.last().ok_or_else(|| Error::invalid("not a pack"))?;
    let end = last.offset + last.size + archive::padding(last.size);
    let mut entry = raw;
    entry.resize(archive::PACK_NAME, 0);
    entry.extend((archive::PACK_SLOT as i32).to_le_bytes());
    entry.extend((new.len() as i32).to_le_bytes());
    entry.extend(((archive::PACK_SLOT + new.len() + archive::padding(new.len())) as i32).to_le_bytes());
    entry.resize(archive::PACK_SLOT, 0);
    let mut out = data[..end].to_vec();
    out.extend(entry);
    out.extend_from_slice(new);
    out.extend(std::iter::repeat_n(0, archive::padding(new.len())));
    out.extend_from_slice(&data[end..]);
    Ok(out)
}

/// The disc's files by path ignoring case, as the game finds them (SearchFile in dataread.cpp).
pub fn disc_index(disc: &Archive) -> HashMap<String, &Entry> {
    disc.entries.iter().map(|entry| (lower(&entry.path), entry)).collect()
}

/// A game file a mod put on the disc: its path, mod, old size (None for a new file or member) and
/// bytes.
#[derive(Clone, Debug)]
pub struct Replaced {
    pub path: String,
    pub module: String,
    pub old: Option<usize>,
    pub data: Vec<u8>,
    pub new_file: bool,
}

/// Puts the mods' files into the disc files build writes. files holds the files the build already
/// rewrote (scripts compiled into their packs), keyed by DATA.DAT path; a replacement goes on top
/// of those. Returns the files, and each replacement.
pub fn replaced(
    iso_path: &Path,
    wanted: &Wanted,
    files: &BTreeMap<String, Vec<u8>>,
    script_tops: &HashSet<String>,
) -> Result<(Files, Vec<Replaced>)> {
    let mut files = files.clone();
    let mut report = Vec::new();
    let image = Image::open(iso_path)?;
    let disc = Archive::open(image.bytes())?;
    let index = disc_index(&disc);
    let sorted: BTreeMap<&String, &(String, PathBuf)> = wanted.iter().collect();
    for (path, (module, file)) in sorted {
        let (top, members) = location(path);
        let fail = |message: String| Error::invalid(format!("{path} (mod {module}): {message}"));
        if !index.contains_key(&lower(&top)) && members.is_empty() {
            // a new file: build gives it a record in the game's file index (rebuild::write_disc)
            if converter_of(file).is_some() {
                return Err(fail("a new file is given as the game file itself".into()));
            }
            let new = fs::read(file)?;
            files.insert(top, new.clone());
            report.push(Replaced { path: path.clone(), module: module.clone(), old: None, data: new, new_file: true });
            continue;
        }
        let Some(entry) = index.get(&lower(&top)) else {
            return Err(fail(format!("{top} is not a file on the disc")));
        };
        let top = entry.path.clone();
        if members.is_empty() && script_tops.contains(&top) {
            return Err(fail(format!(
                "the build also changes scripts inside {top}; replace its members one by one instead"
            )));
        }
        let data = match files.get(&top) {
            Some(data) => data.clone(),
            None => disc.read(image.bytes(), entry).to_vec(),
        };
        let old = rebuild::read_member(&data, &members).map(<[u8]>::to_vec);
        let step = || -> Result<(Vec<u8>, Vec<u8>)> {
            match &old {
                None => {
                    // a member the pack does not have is added (a game file in a common format
                    // needs one to convert from, so it is given as the game file itself)
                    if converter_of(file).is_some() {
                        return Err(Error::invalid(format!(
                            "{} has no member {} to convert to; give the game file itself",
                            top,
                            members.join("/")
                        )));
                    }
                    let new = fs::read(file)?;
                    let packed = added_member(&data, &members, &new)?;
                    Ok((new, packed))
                }
                Some(old) => {
                    let new = game_bytes(file, old)?;
                    let packed = rebuild::replaced_member(&data, &members, &new)?;
                    Ok((new, packed))
                }
            }
        };
        let (new, packed) = step().map_err(|error| fail(error.to_string()))?;
        files.insert(top, packed);
        report.push(Replaced { path: path.clone(), module: module.clone(), old: old.map(|old| old.len()), data: new, new_file: false });
    }
    Ok((files, report))
}

/// Copies files and pack members off the disc into a mod's disc/ folder, to edit or replace there,
/// in a common format when form names one (png). Returns the paths written.
pub fn copy_out(iso_path: &Path, mod_dir: &Path, paths: &[String], form: Option<&str>) -> Result<Vec<String>> {
    let mut written = Vec::new();
    let image = Image::open(iso_path)?;
    let disc = Archive::open(image.bytes())?;
    let index = disc_index(&disc);
    for path in paths {
        let path = path.replace('\\', "/").trim_matches('/').to_string();
        let (top, members) = location(&path);
        let Some(entry) = index.get(&lower(&top)) else {
            return Err(Error::invalid(format!("{top} is not a file on the disc")));
        };
        let Some(data) = rebuild::read_member(disc.read(image.bytes(), entry), &members) else {
            return Err(Error::invalid(format!("{} has no member {}", top, members.join("/"))));
        };
        let target = joined(&mod_dir.join(DISC), &path);
        if let Some(form) = form.filter(|form| !form.is_empty()) {
            let suffix = format!(".{}", lower(form).trim_start_matches('.'));
            let converter = CONVERTERS.iter().find(|converter| converter.suffix == suffix);
            let Some(converter) = converter.filter(|converter| lower(&path).ends_with(converter.game)) else {
                return Err(Error::invalid(format!("{path} cannot be exported as {form}")));
            };
            let exported = converter.export(data)?;
            let folder = target.with_file_name(format!("{}{}", name_of(&target), suffix));
            fs::create_dir_all(&folder)?;
            for (name, content) in exported {
                let file = joined(&folder, &name);
                fs::create_dir_all(file.parent().unwrap())?;
                fs::write(&file, content)?;
                written.push(format!("{path}{suffix}/{name}"));
            }
            continue;
        }
        fs::create_dir_all(target.parent().unwrap())?;
        fs::write(&target, data)?;
        written.push(path);
    }
    Ok(written)
}

/// The replaced files that read back out of a built image differently.
pub fn check(out_path: &Path, report: &[Replaced]) -> Result<Vec<String>> {
    let image = Image::open(out_path)?;
    let disc = Archive::open(image.bytes())?;
    let index = disc_index(&disc);
    let mut wrong = Vec::new();
    for item in report {
        let (top, members) = location(&item.path);
        let found = index.get(&lower(&top)).and_then(|entry| rebuild::read_member(disc.read(image.bytes(), entry), &members));
        if found != Some(&item.data[..]) {
            wrong.push(item.path.clone());
        }
    }
    Ok(wrong)
}
