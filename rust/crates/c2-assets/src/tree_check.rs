//! Checking an asset tree against the disc it came from: every file the manifest lists has the
//! hash the manifest gives it, every asset, unedited, builds back to the disc's own bytes at every
//! place the manifest lists, and every texture file, character pack and bank of the disc is in the
//! manifest.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use serde_json::Value;

use c2_disc::rebuild;

use crate::tree::{self, DiscFile, FileKind, Place};
use crate::{im3, sounds, Error, Result};

/// What a check found.
#[derive(Debug, Default)]
pub struct Report {
    pub files: usize,
    pub picture_places: usize,
    pub pack_places: usize,
    pub bank_places: usize,
    /// Places of each kind of game file on the disc, and in the manifest (as assets or failures).
    pub on_disc: BTreeMap<&'static str, usize>,
    pub listed: BTreeMap<&'static str, usize>,
    pub problems: Vec<String>,
}

fn kind_name(kind: FileKind) -> &'static str {
    match kind {
        FileKind::Textures => "texture files",
        FileKind::Pack => "character packs",
        FileKind::Bank => "sound banks",
    }
}

fn read(root: &Path, path: &str) -> Result<Vec<u8>> {
    std::fs::read(root.join(path)).map_err(|error| Error::invalid(format!("{path}: {error}")))
}

/// The bytes at a place, as a build reads them: the first disc file of the path, then members by
/// name.
fn at<'a>(tops: &HashMap<&str, &'a [u8]>, place: &Place) -> Result<&'a [u8]> {
    let top =
        tops.get(place.path.as_str()).ok_or_else(|| Error::invalid(format!("{}: no such disc file", place.path)))?;
    rebuild::read_member(top, &place.members)
        .ok_or_else(|| Error::invalid(format!("{}: no such member", place.display())))
}

fn places(asset: &Value) -> Vec<(Place, Option<usize>)> {
    let found = asset["places"].as_array().map(Vec::as_slice).unwrap_or_default();
    found
        .iter()
        .filter_map(|place| {
            let entry = place.get("picture").or_else(|| place.get("entry")).and_then(Value::as_u64);
            Some((Place::from_json(place)?, entry.map(|e| e as usize)))
        })
        .collect()
}

fn first_file<'a>(asset: &'a Value, suffix: &str) -> Option<&'a str> {
    asset["files"].as_object()?.keys().find(|path| path.ends_with(suffix)).map(String::as_str)
}

/// Checks the tree at `root` against the disc's files; `temp` is a scratch folder for the models'
/// builds (removed afterwards).
pub fn check(disc: &[DiscFile], root: &Path, temp: &Path) -> Result<Report> {
    let manifest: Value = serde_json::from_slice(&read(root, tree::MANIFEST)?)
        .map_err(|error| Error::invalid(format!("{}: {error}", tree::MANIFEST)))?;
    let mut tops: HashMap<&str, &[u8]> = HashMap::new();
    for file in disc {
        tops.entry(file.path.as_str()).or_insert(file.data);
    }
    let empty = serde_json::Map::new();
    let assets = manifest["assets"].as_object().unwrap_or(&empty);
    let mut report = Report::default();

    // every file has the hash the manifest gives
    let hashes: Vec<(&str, &str)> = assets
        .values()
        .flat_map(|asset| asset["files"].as_object().into_iter().flatten())
        .map(|(path, hash)| (path.as_str(), hash.as_str().unwrap_or("")))
        .collect();
    report.files = hashes.len();
    let mut problems: Vec<String> = hashes
        .par_iter()
        .filter_map(|(path, hash)| match read(root, path) {
            Ok(data) if tree::sha256(&[&data]) == *hash => None,
            Ok(_) => Some(format!("{path}: differs from the hash in the manifest")),
            Err(error) => Some(error.to_string()),
        })
        .collect();

    // textures, by the texture file holding them, so each is read once: (picture entry, PNG, name)
    type Wanted<'m> = Vec<(usize, &'m str, &'m str)>;
    let mut pictures: BTreeMap<String, (Place, Wanted)> = BTreeMap::new();
    let mut packs = Vec::new();
    let mut banks = Vec::new();
    for (id, asset) in assets {
        match asset["kind"].as_str() {
            Some("texture") => {
                let Some(file) = first_file(asset, ".png") else {
                    problems.push(format!("{id}: no PNG"));
                    continue;
                };
                for (place, entry) in places(asset) {
                    report.picture_places += 1;
                    let name = asset["name"].as_str().unwrap_or("");
                    let slot = pictures.entry(place.display()).or_insert_with(|| (place, Vec::new()));
                    slot.1.push((entry.unwrap_or(0), file, name));
                }
            }
            Some("model") => packs.push((id, asset)),
            Some("bank") => banks.push((id, asset)),
            _ => {}
        }
    }
    let texture_problems: Vec<String> = pictures
        .par_iter()
        .flat_map(|(shown, (place, wanted))| -> Vec<String> {
            let check = || -> Result<Vec<String>> {
                let img = at(&tops, place)?;
                let images = im3::read(img)?;
                let mut out = Vec::new();
                for &(entry, file, name) in wanted {
                    let Some(position) = images.iter().position(|image| image.index == Some(entry)) else {
                        out.push(format!("{shown}: no picture {entry}"));
                        continue;
                    };
                    let found = tree::picture_name(&images[position].name);
                    if found != name {
                        out.push(format!("{shown}: picture {entry} is {found}, not {name}"));
                    }
                    let mut built = images.clone();
                    built[position] = im3::from_png(&read(root, file)?, &images[position])?;
                    if im3::write(&built, Some(img))? != img {
                        out.push(format!("{shown}: {file} builds back to other bytes"));
                    }
                }
                Ok(out)
            };
            check().unwrap_or_else(|error| vec![format!("{shown}: {error}")])
        })
        .collect();
    problems.extend(texture_problems);

    // models: the tree's glTF and buffer, as the export names them, built at each place
    let _ = std::fs::remove_dir_all(temp);
    report.pack_places = packs.iter().map(|(_, asset)| places(asset).len()).sum();
    let model_problems: Vec<String> = packs
        .par_iter()
        .enumerate()
        .flat_map(|(index, (id, asset))| -> Vec<String> {
            let check = || -> Result<Vec<String>> {
                let (Some(gltf), Some(bin)) = (first_file(asset, ".gltf"), first_file(asset, ".bin")) else {
                    return Ok(vec![format!("{id}: no glTF and buffer")]);
                };
                let (gltf, bin) = (read(root, gltf)?, read(root, bin)?);
                let folder: PathBuf = temp.join(index.to_string());
                let mut out = Vec::new();
                for (place, _) in places(asset) {
                    let pack = at(&tops, &place)?;
                    let name = asset["name"].as_str().unwrap_or("");
                    let (stem, exported) = tree::exported_gltf(&gltf, pack, name)?;
                    let _ = std::fs::remove_dir_all(&folder);
                    std::fs::create_dir_all(&folder)?;
                    std::fs::write(folder.join(format!("{stem}.gltf")), &exported)?;
                    std::fs::write(folder.join(format!("{stem}.bin")), &bin)?;
                    if tree::build_model(&folder, pack, name)? != pack {
                        out.push(format!("{id}: builds back to other bytes at {}", place.display()));
                    }
                }
                let _ = std::fs::remove_dir_all(&folder);
                Ok(out)
            };
            check().unwrap_or_else(|error| vec![format!("{id}: {error}")])
        })
        .collect();
    problems.extend(model_problems);

    // banks: the bank's folder of WAVs, built at each place
    report.bank_places = banks.iter().map(|(_, asset)| places(asset).len()).sum();
    let bank_problems: Vec<String> = banks
        .par_iter()
        .flat_map(|(id, asset)| -> Vec<String> {
            let check = || -> Result<Vec<String>> {
                let mut folder = root.join(asset["folder"].as_str().unwrap_or(""));
                if !folder.is_dir() {
                    // a bank with no entry playing a sample has no WAVs
                    folder = temp.join("no WAVs");
                    std::fs::create_dir_all(&folder)?;
                }
                let mut out = Vec::new();
                for (place, _) in places(asset) {
                    let bank = at(&tops, &place)?;
                    if sounds::build(&folder, bank)? != bank {
                        out.push(format!("{id}: builds back to other bytes at {}", place.display()));
                    }
                }
                Ok(out)
            };
            check().unwrap_or_else(|error| vec![format!("{id}: {error}")])
        })
        .collect();
    problems.extend(bank_problems);
    let _ = std::fs::remove_dir_all(temp);

    // every game file of the disc is listed
    for file in tree::game_files(disc).files {
        *report.on_disc.entry(kind_name(file.kind)).or_default() += 1;
    }
    let mut listed: BTreeMap<FileKind, std::collections::BTreeSet<String>> = BTreeMap::new();
    for asset in assets.values() {
        let kind = match asset["kind"].as_str() {
            Some("texture") => FileKind::Textures,
            Some("model") => FileKind::Pack,
            Some("bank") => FileKind::Bank,
            _ => continue,
        };
        for (place, _) in places(asset) {
            listed.entry(kind).or_default().insert(place.display());
        }
    }
    for failed in manifest["unconverted"].as_array().map(Vec::as_slice).unwrap_or_default() {
        if let Some(place) = Place::from_json(failed) {
            let name = place.members.last().cloned().unwrap_or_else(|| place.path.clone());
            if let Some(kind) = tree::file_kind(&name) {
                listed.entry(kind).or_default().insert(place.display());
            }
        }
    }
    for (kind, found) in listed {
        report.listed.insert(kind_name(kind), found.len());
    }
    problems.sort();
    report.problems = problems;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::tests::{build_disc, disc};

    fn written_tree(name: &str, files: &BTreeMap<String, Vec<u8>>) -> PathBuf {
        let root = std::env::temp_dir().join(format!("c2-assets-tree-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (path, data) in files {
            let target = root.join(path);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::write(target, data).unwrap();
        }
        root
    }

    fn run(name: &str, edit: impl Fn(&mut BTreeMap<String, Vec<u8>>)) -> Report {
        let disc = disc();
        let (_, mut written) = build_disc(&disc);
        edit(&mut written);
        let root = written_tree(name, &written);
        let files: Vec<DiscFile> = disc.iter().map(|(path, data)| DiscFile { path: path.clone(), data }).collect();
        let report = check(&files, &root, &root.join("temp")).unwrap();
        std::fs::remove_dir_all(root).unwrap();
        report
    }

    #[test]
    fn an_unedited_tree_builds_back_at_every_place() {
        let report = run("unedited", |_| {});
        assert_eq!(report.problems, Vec::<String>::new());
        assert_eq!((report.picture_places, report.pack_places), (9, 4));
        assert_eq!(report.on_disc, report.listed);
    }

    #[test]
    fn an_edited_texture_is_found() {
        let report = run("edited", |written| {
            let png = im3::write_png(8, 8, im3::PngMode::Rgb, &[0, 0, 255].repeat(64), &[], &[]).unwrap();
            written.insert("textures/map/a/tex02.png".to_string(), png);
        });
        // its hash, and both places it is at
        assert_eq!(report.problems.len(), 3, "{:?}", report.problems);
        assert!(report.problems[0].contains("builds back to other bytes"));
    }
}
