//! c2 assets: the disc's textures, character models and sounds as an asset tree (each distinct
//! content once, in a format common tools open, with manifest.json naming each by id and listing
//! every place the disc has it), and the hidden assets-check that builds every asset back at every
//! place.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use rayon::prelude::*;
use serde_json::Value;

use c2_assets::ids::Index;
use c2_assets::tree::{self, DiscFile, SoundNames};
use c2_assets::tree_check;
use c2_disc::archive::Archive;
use c2_disc::Image;

use crate::files;

#[derive(Subcommand)]
pub enum Command {
    /// Write every texture (PNG), character model (glTF) and sound (WAV) of the disc once into an
    /// asset tree, with manifest.json giving each its id and every place the disc has it.
    Assets {
        /// An extracted disc directory, or the disc image.
        source: PathBuf,
        /// Directory to write.
        #[arg(short, long, default_value = "extracted/assets")]
        output: PathBuf,
        /// Replace the asset tree already in the directory.
        #[arg(long)]
        force: bool,
        /// Definition set whose header names sound entries; repeat to layer sets (later ones win);
        /// the shipped PAL set when omitted.
        #[arg(long)]
        defs: Vec<PathBuf>,
    },
    /// Check an asset tree against the disc: every file has the manifest's hash, every asset
    /// builds back to the disc's bytes at every place, and every game file is listed.
    #[command(hide = true)]
    AssetsCheck {
        /// An extracted disc directory, or the disc image.
        source: PathBuf,
        /// The asset tree.
        assets: PathBuf,
    },
}

pub fn run(command: &Command) -> Result<ExitCode> {
    match command {
        Command::Assets { source, output, force, defs } => run_assets(source, output, *force, defs),
        Command::AssetsCheck { source, assets } => run_check(source, assets),
    }
}

/// The disc's files: mapped from the image, or read from an extracted disc.
enum Source {
    Image(Image),
    Files(Vec<(String, Vec<u8>)>),
}

impl Source {
    fn open(path: &Path) -> Result<Source> {
        if path.is_file() {
            return Ok(Source::Image(Image::open(path).with_context(|| format!("cannot read {}", path.display()))?));
        }
        if !path.is_dir() {
            bail!("{} is neither a disc image nor an extracted disc", path.display());
        }
        let manifest_path = path.join("manifest.json");
        let text = std::fs::read(&manifest_path)
            .with_context(|| format!("{} is not an extracted disc (no manifest.json)", path.display()))?;
        let manifest: Value = serde_json::from_slice(&text).with_context(|| manifest_path.display().to_string())?;
        // the disc files whose bytes hold a texture file, character pack, bank or message file
        // (members follow the file holding them in the list), and the English font's table the
        // message files' text needs
        let mut wanted: Vec<String> = Vec::new();
        let mut current: Option<(String, bool)> = None;
        let mut close = |current: &mut Option<(String, bool)>| {
            if let Some((path, true)) = current.take() {
                wanted.push(path);
            }
        };
        for item in manifest["files"].as_array().map(Vec::as_slice).unwrap_or_default() {
            let file = item["path"].as_str().unwrap_or("");
            let relevant = tree::file_kind(file).is_some() || file.eq_ignore_ascii_case(c2_assets::text::ENGLISH_FONT);
            if item["packed"].as_bool() == Some(false) {
                close(&mut current);
                current = Some((file.to_string(), relevant));
            } else if let Some((_, found)) = current.as_mut() {
                *found |= relevant;
            }
        }
        close(&mut current);
        wanted.dedup();
        let files = wanted
            .into_par_iter()
            .map(|file| {
                let data = std::fs::read(path.join(&file)).with_context(|| format!("cannot read {file}"))?;
                Ok((file, data))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Source::Files(files))
    }

    fn files(&self) -> Result<Vec<DiscFile<'_>>> {
        Ok(match self {
            Source::Image(image) => {
                let archive = Archive::open(image.bytes())?;
                archive
                    .entries
                    .iter()
                    .map(|entry| DiscFile { path: entry.path.clone(), data: archive.read(image.bytes(), entry) })
                    .collect()
            }
            Source::Files(files) => files.iter().map(|(path, data)| DiscFile { path: path.clone(), data }).collect(),
        })
    }
}

/// The sound entry names of the definition sets' headers (include/*.h), later sets winning.
pub(crate) fn sound_names(defs: &[PathBuf]) -> Result<SoundNames> {
    let sets = if defs.is_empty() { vec![files::default_defs()] } else { defs.to_vec() };
    let mut names = SoundNames::new();
    for set in sets {
        let include = set.join("include");
        let Ok(entries) = std::fs::read_dir(&include) else {
            continue;
        };
        let mut headers: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.extension().is_some_and(|ext| ext == "h"))
            .collect();
        headers.sort();
        for header in headers {
            let text = std::fs::read_to_string(&header).with_context(|| header.display().to_string())?;
            names.extend(tree::sound_names(&text));
        }
    }
    Ok(names)
}

/// The asset ids of a disc image or extracted disc and every place of each, read straight from the
/// disc (as `c2 assets` names them, without converting anything), the definition sets naming the
/// sound entries.
pub(crate) fn index_of(source: &Path, defs: &[PathBuf]) -> Result<Index> {
    let disc = Source::open(source)?;
    Ok(Index::from_disc(&disc.files()?, &sound_names(defs)?))
}

/// What a tree's folder holds that a new tree replaces.
const TREE_PARTS: [&str; 4] = [tree::MANIFEST, tree::TEXTURES, tree::MODELS, tree::SOUNDS];

pub fn run_assets(source: &Path, output: &Path, force: bool, defs: &[PathBuf]) -> Result<ExitCode> {
    let names = sound_names(defs)?;
    let has_files = std::fs::read_dir(output).map(|mut entries| entries.next().is_some()).unwrap_or(false);
    if has_files {
        if !force {
            bail!("{} already has files; --force replaces the asset tree in it", output.display());
        }
        for part in TREE_PARTS {
            let path = output.join(part);
            if path.is_dir() {
                std::fs::remove_dir_all(&path)?;
            } else if path.is_file() {
                std::fs::remove_file(&path)?;
            }
        }
    }
    let disc = Source::open(source)?;
    let write = |path: &str, data: &[u8]| -> c2_assets::Result<()> {
        let target = output.join(path);
        std::fs::create_dir_all(target.parent().expect("a tree file is in a folder"))?;
        std::fs::write(target, data)?;
        Ok(())
    };
    let summary = tree::build(&disc.files()?, &names, &write)?;
    println!(
        "{} textures from {} pictures in {} texture files ({} distinct by their bytes alone; {} names with more than one version)",
        summary.textures,
        summary.picture_places,
        summary.texture_files,
        summary.distinct_pictures,
        summary.names_with_versions
    );
    println!(
        "{} models from {} character packs, {} distinct ({} by their bytes alone; {} textures the disc has nowhere left as placeholders)",
        summary.models,
        summary.pack_places,
        summary.distinct_packs,
        summary.distinct_pack_contents,
        summary.placeholders
    );
    println!(
        "{} banks from {} places: {} entries, {} as WAVs, {} named",
        summary.banks, summary.bank_places, summary.sounds, summary.sounds_exported, summary.sounds_named
    );
    println!("{} texts from {} message files", summary.texts, summary.message_places);
    if !summary.failed.is_empty() {
        let mut reasons: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
        for (_, reason) in &summary.failed {
            *reasons.entry(reason.as_str()).or_default() += 1;
        }
        println!("{} game files not converted (listed in {}):", summary.failed.len(), tree::MANIFEST);
        for (reason, count) in reasons {
            println!("  {count}: {reason}");
        }
    }
    if summary.shadowed > 0 {
        println!(
            "{} game files left out: a member of the same name comes first in their pack, so a build cannot reach them",
            summary.shadowed
        );
    }
    println!("written to {}", output.display());
    Ok(ExitCode::SUCCESS)
}

fn run_check(source: &Path, assets: &Path) -> Result<ExitCode> {
    let disc = Source::open(source)?;
    let temp = std::env::temp_dir().join(format!("c2-assets-check-{}", std::process::id()));
    let report = tree_check::check(&disc.files()?, assets, &temp)?;
    println!("{} files have the manifest's hashes", report.files);
    println!(
        "built back: {} picture places, {} character pack places, {} bank places, {} message file places",
        report.picture_places, report.pack_places, report.bank_places, report.message_places
    );
    for (kind, count) in &report.on_disc {
        println!("{kind}: {count} on the disc, {} in the manifest", report.listed.get(kind).copied().unwrap_or(0));
    }
    let unlisted = report.on_disc.iter().any(|(kind, count)| report.listed.get(kind) != Some(count));
    for problem in report.problems.iter().take(50) {
        println!("  {problem}");
    }
    if report.problems.len() > 50 {
        println!("  ... and {} more", report.problems.len() - 50);
    }
    println!("{} problems", report.problems.len());
    Ok(if report.problems.is_empty() && !unlisted { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}
