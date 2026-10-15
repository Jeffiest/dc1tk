//! dc2tk assets: the disc's textures, character models and sounds as an asset tree (each distinct
//! content once, in a format common tools open, with manifest.json naming each by id and listing
//! every place the disc has it), and the hidden assets-check that builds every asset back at every
//! place.

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use rayon::prelude::*;

use dc2tk_assets::ids::Index;
use dc2tk_assets::tree::{self, DiscFile, SoundNames};
use dc2tk_assets::tree_check;
use dc2tk_disc::DiscSource;

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
        #[arg(long, value_name = "DIR")]
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

/// The disc's files the asset tree is made from, by DATA.DAT path: an image's every file (mapped,
/// so reading one costs nothing), an extracted disc's only those whose bytes hold a texture file,
/// character pack, bank or message file, and the English font's table the message files' text
/// needs (read in parallel).
fn tree_files(disc: &DiscSource) -> Result<Vec<(String, Cow<'_, [u8]>)>> {
    let Some(manifest) = disc.manifest() else {
        return disc.paths().iter().map(|path| Ok((path.clone(), disc.read(path)?))).collect();
    };
    // members follow the file holding them in the manifest's list: (a file, whether it or one of
    // its members is wanted)
    let mut wanted: Vec<&str> = Vec::new();
    let mut current: Option<(&str, bool)> = None;
    for item in &manifest.files {
        let relevant = tree::file_kind(&item.path).is_some() || dc2tk_assets::text::Fonts::reads(&item.path);
        if item.packed {
            if let Some((_, found)) = current.as_mut() {
                *found |= relevant;
            }
        } else if let Some((path, true)) = current.replace((&item.path, relevant)) {
            wanted.push(path);
        }
    }
    if let Some((path, true)) = current {
        wanted.push(path);
    }
    wanted.dedup();
    wanted.into_par_iter().map(|path| Ok((path.to_string(), disc.read(path)?))).collect()
}

/// The files of tree_files as the asset tree's functions take them.
fn disc_files<'a>(files: &'a [(String, Cow<'_, [u8]>)]) -> Vec<DiscFile<'a>> {
    files.iter().map(|(path, data)| DiscFile { path: path.clone(), data }).collect()
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
/// disc (as `dc2tk assets` names them, without converting anything), the definition sets naming the
/// sound entries.
pub(crate) fn index_of(source: &Path, defs: &[PathBuf]) -> Result<Index> {
    let disc = DiscSource::open(source)?;
    Ok(Index::from_disc(&disc_files(&tree_files(&disc)?), &sound_names(defs)?))
}

/// What a tree's folder holds that a new tree replaces.
const TREE_PARTS: [&str; 5] = [tree::MANIFEST, tree::TEXTURES, tree::MODELS, tree::SOUNDS, tree::MUSIC];

pub fn run_assets(source: &Path, output: &Path, force: bool, defs: &[PathBuf]) -> Result<ExitCode> {
    let names = sound_names(defs)?;
    let has_files = std::fs::read_dir(output).map(|mut entries| entries.next().is_some()).unwrap_or(false);
    if has_files {
        if !force {
            bail!("{} already has files; --force replaces the asset tree in it", output.display());
        }
        for part in TREE_PARTS {
            let path = output.join(part);
            let removed = if path.is_dir() {
                std::fs::remove_dir_all(&path)
            } else if path.is_file() {
                std::fs::remove_file(&path)
            } else {
                continue;
            };
            removed.with_context(|| format!("cannot replace {}", path.display()))?;
        }
    }
    let disc = DiscSource::open(source)?;
    let write = |path: &str, data: &[u8]| -> dc2tk_assets::Result<()> {
        let target = output.join(path);
        std::fs::create_dir_all(target.parent().expect("a tree file is in a folder"))?;
        std::fs::write(&target, data)
            .map_err(|error| dc2tk_assets::Error::invalid(format!("cannot write {}: {error}", target.display())))
    };
    let summary = tree::build(&disc_files(&tree_files(&disc)?), &names, &write)?;
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
    println!("{} sequences (music and ambient loops) from {} places, as MIDI", summary.music, summary.music_places);
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
    let disc = DiscSource::open(source)?;
    let temp = std::env::temp_dir().join(format!("dc2tk-assets-check-{}", std::process::id()));
    let report = tree_check::check(&disc_files(&tree_files(&disc)?), assets, &temp)?;
    println!("{} files have the manifest's hashes", report.files);
    println!(
        "built back: {} picture places, {} character pack places, {} bank places, {} message file places, {} sequence places",
        report.picture_places, report.pack_places, report.bank_places, report.message_places, report.music_places
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
