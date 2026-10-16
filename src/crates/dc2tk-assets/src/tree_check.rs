//! Checking an asset tree against the disc it came from: every file the manifest lists has the
//! hash the manifest gives it, every asset, unedited, builds back to the disc's own bytes at every
//! place the manifest lists, and every texture file, character pack, bank and message file of the
//! disc is in the manifest.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use rayon::prelude::*;

use dc2tk_disc::files;
use dc2tk_disc::rebuild;

use crate::manifest::{self, Manifest};
use crate::tree::{self, DiscFile, FileKind, Kind, Place};
use crate::{im3, sounds, text, Context, Error, Result};

/// What a check found.
#[derive(Debug, Default)]
pub struct Report {
    pub files: usize,
    pub picture_places: usize,
    pub pack_places: usize,
    pub bank_places: usize,
    pub message_places: usize,
    pub music_places: usize,
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
        FileKind::Messages => "message files",
    }
}

/// The kind of game file an asset of a kind is at, for those the disc's walk finds.
fn file_kind_of(kind: Kind) -> Option<FileKind> {
    match kind {
        Kind::Texture => Some(FileKind::Textures),
        Kind::Model => Some(FileKind::Pack),
        Kind::Bank => Some(FileKind::Bank),
        Kind::Text => Some(FileKind::Messages),
        Kind::Sound | Kind::Music => None,
    }
}

fn read(root: &Path, path: &str) -> Result<Vec<u8>> {
    std::fs::read(root.join(path)).context(path)
}

/// The bytes at a place, as a build reads them: the first disc file of the path, then members by
/// name.
fn at<'a>(tops: &HashMap<&str, &'a [u8]>, place: &Place) -> Result<&'a [u8]> {
    let top =
        tops.get(place.path.as_str()).ok_or_else(|| Error::invalid(format!("{}: no such disc file", place.path)))?;
    rebuild::read_member(top, &place.members)
        .ok_or_else(|| Error::invalid(format!("{}: no such member", place.display())))
}

/// The assets of a kind, in id order.
fn of_kind(manifest: &Manifest, kind: Kind) -> Vec<(&String, &manifest::Asset)> {
    manifest.assets.iter().filter(|(_, asset)| asset.kind() == kind).collect()
}

/// The problems each asset's check finds, in parallel: what `check` gives, or the error that
/// stopped it, named by the asset's id.
fn check_assets<'m>(
    assets: &[(&'m String, &'m manifest::Asset)],
    check: impl Fn(usize, &str, &'m manifest::Asset) -> Result<Vec<String>> + Sync,
) -> Vec<String> {
    assets
        .par_iter()
        .enumerate()
        .flat_map(|(index, (id, asset))| check(index, id, asset).unwrap_or_else(|error| vec![format!("{id}: {error}")]))
        .collect()
}

/// A built asset compared with the disc's bytes at a place.
fn built_back(id: &str, built: &[u8], disc: &[u8], place: &Place) -> Option<String> {
    (built != disc).then(|| format!("{id}: builds back to other bytes at {}", place.display()))
}

/// Checks the tree at `root` against the disc's files; `temp` is a scratch folder for the models'
/// builds (removed afterwards).
pub fn check(disc: &[DiscFile], root: &Path, temp: &Path) -> Result<Report> {
    let manifest = Manifest::read(&read(root, tree::MANIFEST)?)?;
    let mut tops: HashMap<&str, &[u8]> = HashMap::new();
    for file in disc {
        tops.entry(file.path.as_str()).or_insert(file.data);
    }
    let tops = &tops;
    let mut report = Report::default();
    let places_of =
        |kind: Kind| -> usize { of_kind(&manifest, kind).iter().map(|(_, asset)| asset.places().len()).sum() };
    report.picture_places = places_of(Kind::Texture);
    report.pack_places = places_of(Kind::Model);
    report.bank_places = places_of(Kind::Bank);
    report.message_places = places_of(Kind::Text);
    report.music_places = places_of(Kind::Music);

    // every file has the hash the manifest gives
    let hashes: Vec<(&String, &String)> =
        manifest.assets.values().flat_map(|asset| asset.files().into_iter().flatten()).collect();
    report.files = hashes.len();
    let mut problems: Vec<String> = hashes
        .par_iter()
        .filter_map(|(path, hash)| match read(root, path) {
            Ok(data) if tree::sha256(&[&data]) == **hash => None,
            Ok(_) => Some(format!("{path}: differs from the hash in the manifest")),
            Err(error) => Some(error.to_string()),
        })
        .collect();
    problems.extend(check_textures(&manifest, root, tops));

    // models: the tree's glTF and buffer, as the export names them, built at each place
    let _ = std::fs::remove_dir_all(temp);
    problems.extend(check_assets(&of_kind(&manifest, Kind::Model), |index, id, asset| {
        let (Some(gltf), Some(bin)) = (asset.file(".gltf"), asset.file(".bin")) else {
            return Ok(vec![format!("{id}: no glTF and buffer")]);
        };
        let (gltf, bin) = (read(root, gltf)?, read(root, bin)?);
        let folder: PathBuf = temp.join(index.to_string());
        let mut out = Vec::new();
        for place in asset.places().iter().map(manifest::AssetPlace::place) {
            let pack = at(tops, &place)?;
            out.extend(built_back(
                id,
                &tree::build_model_from(&gltf, &bin, pack, asset.name(), &folder)?,
                pack,
                &place,
            ));
        }
        Ok(out)
    }));

    // banks: the bank's folder of WAVs, built at each place
    problems.extend(check_assets(&of_kind(&manifest, Kind::Bank), |_, id, asset| {
        let manifest::Asset::Bank { folder, .. } = asset else {
            return Ok(Vec::new());
        };
        let mut folder = root.join(folder);
        if !folder.is_dir() {
            // a bank with no WAV of its own (no entry playing a sample, or each one an earlier
            // version's sound) has no folder
            folder = temp.join("no WAVs");
            files::create_dir_all(&folder)?;
        }
        let mut out = Vec::new();
        for place in asset.places().iter().map(manifest::AssetPlace::place) {
            let bank = at(tops, &place)?;
            out.extend(built_back(id, &sounds::build(&folder, bank)?, bank, &place));
        }
        Ok(out)
    }));
    let _ = std::fs::remove_dir_all(temp);

    // message files: the text built into each place, with the first (the one exported) as the
    // reference
    let fonts = text::Fonts::from_files(disc.iter().map(|file| (file.path.as_str(), file.data)));
    problems.extend(check_assets(&of_kind(&manifest, Kind::Text), |_, id, asset| {
        let Some(file) = asset.file(".txt") else {
            return Ok(vec![format!("{id}: no text")]);
        };
        let written =
            String::from_utf8(read(root, file)?).map_err(|_| Error::invalid(format!("{file} is not UTF-8")))?;
        let places: Vec<Place> = asset.places().iter().map(manifest::AssetPlace::place).collect();
        let Some(reference) = places.first() else {
            return Ok(Vec::new());
        };
        let reference = at(tops, reference)?;
        let mut out = Vec::new();
        for place in &places {
            let data = at(tops, place)?;
            out.extend(built_back(id, &text::build(&written, data, reference, &place.display(), &fonts)?, data, place));
        }
        Ok(out)
    }));

    // sequences: the MIDI file built at each place
    problems.extend(check_assets(&of_kind(&manifest, Kind::Music), |_, id, asset| {
        let Some(file) = asset.file(crate::music::SUFFIX) else {
            return Ok(vec![format!("{id}: no MIDI file")]);
        };
        let midi = read(root, file)?;
        let mut out = Vec::new();
        for place in asset.places().iter().map(manifest::AssetPlace::place) {
            let sq = at(tops, &place)?;
            out.extend(built_back(id, &crate::music::build(&midi, sq, None)?, sq, &place));
        }
        Ok(out)
    }));

    listed_files(disc, &manifest, &mut report);
    problems.sort();
    report.problems = problems;
    Ok(report)
}

/// Textures, by the texture file holding them, so each is read once: each picture's PNG built
/// back into the file at every place.
fn check_textures(manifest: &Manifest, root: &Path, tops: &HashMap<&str, &[u8]>) -> Vec<String> {
    // (picture entry, PNG, name) by the texture file's place
    type Wanted<'m> = Vec<(usize, &'m str, &'m str)>;
    let mut pictures: BTreeMap<String, (Place, Wanted)> = BTreeMap::new();
    let mut problems = Vec::new();
    for (id, asset) in of_kind(manifest, Kind::Texture) {
        let Some(file) = asset.file(".png") else {
            problems.push(format!("{id}: no PNG"));
            continue;
        };
        for place in asset.places() {
            let slot = pictures.entry(place.place().display()).or_insert_with(|| (place.place(), Vec::new()));
            slot.1.push((place.picture.unwrap_or(0), file, asset.name()));
        }
    }
    let found: Vec<String> = pictures
        .par_iter()
        .flat_map(|(shown, (place, wanted))| -> Vec<String> {
            let check = || -> Result<Vec<String>> {
                let img = at(tops, place)?;
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
    problems.extend(found);
    problems
}

/// Every game file of the disc, by kind, and those the manifest lists (as assets or failures).
fn listed_files(disc: &[DiscFile], manifest: &Manifest, report: &mut Report) {
    for file in tree::game_files(disc).files {
        *report.on_disc.entry(kind_name(file.kind)).or_default() += 1;
    }
    let mut listed: BTreeMap<FileKind, BTreeSet<String>> = BTreeMap::new();
    for asset in manifest.assets.values() {
        let Some(kind) = file_kind_of(asset.kind()) else {
            continue;
        };
        for place in asset.places() {
            listed.entry(kind).or_default().insert(place.place().display());
        }
    }
    for failed in &manifest.unconverted {
        let place = failed.place();
        let name = place.members.last().cloned().unwrap_or_else(|| place.path.clone());
        if let Some(kind) = tree::file_kind(&name) {
            listed.entry(kind).or_default().insert(place.display());
        }
    }
    for (kind, found) in listed {
        report.listed.insert(kind_name(kind), found.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::tests::{build_disc, disc};

    fn written_tree(name: &str, files: &BTreeMap<String, Vec<u8>>) -> PathBuf {
        let root = std::env::temp_dir().join(format!("dc2tk-assets-tree-{name}-{}", std::process::id()));
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
            let png = crate::png::write_png(8, 8, crate::png::PngMode::Rgb, &[0, 0, 255].repeat(64), &[], &[]).unwrap();
            written.insert("textures/map/a/tex02.png".to_string(), png);
        });
        // its hash, and both places it is at
        assert_eq!(report.problems.len(), 3, "{:?}", report.problems);
        assert!(report.problems[0].contains("builds back to other bytes"));
    }

    /// Real banks of an extracted disc (DC2TK_CORPUS): a music bank at its own path and inside a
    /// pack, and an ambient bank of several sequences. Each sequence is a music asset at every
    /// place, the tree and the index give the same ids, and the tree's MIDI files build back.
    #[test]
    #[ignore = "needs DC2TK_CORPUS"]
    fn corpus_sequences_are_music_in_the_tree_and_build_back() {
        let corpus = dc2tk_disc::samples::corpus();
        let read = |path: &str| std::fs::read(Path::new(&corpus).join(path)).unwrap();
        let bgm = read("snd2/bgm/BG_193.snd");
        let disc = vec![
            ("sg/fish/fish.pak".to_string(), tree::tests::pack(&[("BG_193.snd", &bgm)])),
            ("snd2/bgm/BG_193.snd".to_string(), bgm.clone()),
            ("snd2/env/SR_138.snd".to_string(), read("snd2/env/SR_138.snd")),
        ];
        let (summary, written) = build_disc(&disc);
        let files: Vec<DiscFile> = disc.iter().map(|(path, data)| DiscFile { path: path.clone(), data }).collect();
        let manifest = Manifest::read(&written[tree::MANIFEST]).unwrap();
        let index = crate::ids::Index::from_disc(&files, &tree::sound_names(""));
        assert_eq!(index, crate::ids::Index::from_manifest(&manifest));
        let music = index.get("music:BG_193_000").unwrap();
        let places: Vec<String> = music.places.iter().map(|(place, _)| place.display()).collect();
        assert_eq!(places, ["sg/fish/fish.pak.d/BG_193.snd.d/BG_193_000.sq", "snd2/bgm/BG_193.snd.d/BG_193_000.sq"]);
        assert!(
            written.contains_key("music/snd2/bgm/BG_193_000.mid"),
            "{:?}",
            written.keys().filter(|k| k.starts_with("music")).collect::<Vec<_>>()
        );
        assert!(written.contains_key("music/snd2/bgm/BG_193.sf2"));
        let root = written_tree("music", &written);
        let report = check(&files, &root, &root.join("temp")).unwrap();
        std::fs::remove_dir_all(root).unwrap();
        assert_eq!(report.problems, Vec::<String>::new());
        assert_eq!(report.music_places, summary.music_places);
        assert!(summary.music >= 3, "{}", summary.music);
    }
}
