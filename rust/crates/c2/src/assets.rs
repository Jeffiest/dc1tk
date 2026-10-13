//! Hidden commands that run the asset converters over every game file under a folder, for
//! parity.py: textures (.img) as PNGs, sound banks (.snd) as WAVs, character packs (.chr) as glTF.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Mutex;

use anyhow::Result;
use clap::{Subcommand, ValueEnum};
use rayon::prelude::*;

use c2_assets::textures;

/// The common formats and the game files each converts.
#[derive(Clone, Copy, ValueEnum)]
pub enum Kind {
    Png,
    Wav,
    Gltf,
}

impl Kind {
    fn game_suffix(self) -> &'static str {
        match self {
            Kind::Png => ".img",
            Kind::Wav => ".snd",
            Kind::Gltf => ".chr",
        }
    }

    fn suffix(self) -> &'static str {
        match self {
            Kind::Png => ".png",
            Kind::Wav => ".wav",
            Kind::Gltf => ".gltf",
        }
    }

    fn export(self, data: &[u8]) -> c2_assets::Result<Vec<(String, Vec<u8>)>> {
        match self {
            Kind::Png => textures::export(data),
            Kind::Wav => c2_assets::sounds::export(data),
            Kind::Gltf => todo!(),
        }
    }

    fn build(self, folder: &Path, original: &[u8]) -> c2_assets::Result<Vec<u8>> {
        match self {
            Kind::Png => textures::build(folder, original),
            Kind::Wav => c2_assets::sounds::build(folder, original),
            Kind::Gltf => todo!(),
        }
    }
}

#[derive(Subcommand)]
pub enum Command {
    /// Export every game file of a kind under ROOT into OUT/<path><suffix>/ (as mod asset --as
    /// does); the files that cannot be exported are listed in OUT/failed.txt.
    #[command(hide = true)]
    AssetExport { kind: Kind, root: PathBuf, out: PathBuf },
    /// Build every game file of a kind under ROOT that FOLDERS has a <path><suffix>/ folder for,
    /// into OUT/<path>; failures listed in OUT/failed.txt. With --check, compare with the original
    /// instead and print the ones that differ.
    #[command(hide = true)]
    AssetBuild {
        kind: Kind,
        root: PathBuf,
        folders: PathBuf,
        out: PathBuf,
        #[arg(long)]
        check: bool,
    },
}

pub fn run(command: &Command) -> Result<ExitCode> {
    match command {
        Command::AssetExport { kind, root, out } => run_export(*kind, root, out),
        Command::AssetBuild { kind, root, folders, out, check } => run_build(*kind, root, folders, out, *check),
    }
}

/// The game files of a kind under a folder, relative with "/", sorted.
fn game_files(root: &Path, kind: Kind) -> Result<Vec<String>> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.to_string_lossy().to_lowercase().ends_with(kind.game_suffix()) {
                let relative = path.strip_prefix(root)?;
                found
                    .push(relative.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"));
            }
        }
    }
    found.sort();
    Ok(found)
}

fn write_failed(out: &Path, failed: Mutex<Vec<String>>) -> Result<usize> {
    let mut failed = failed.into_inner().unwrap();
    failed.sort();
    std::fs::create_dir_all(out)?;
    let text: String = failed.iter().map(|line| format!("{line}\n")).collect();
    std::fs::write(out.join("failed.txt"), text)?;
    Ok(failed.len())
}

fn run_export(kind: Kind, root: &Path, out: &Path) -> Result<ExitCode> {
    let files = game_files(root, kind)?;
    let failed = Mutex::new(Vec::new());
    files.par_iter().try_for_each(|relative| -> Result<()> {
        let data = std::fs::read(root.join(relative))?;
        match kind.export(&data) {
            Ok(exported) => {
                let folder = out.join(format!("{relative}{}", kind.suffix()));
                for (name, content) in exported {
                    let target = folder.join(&name);
                    std::fs::create_dir_all(target.parent().unwrap())?;
                    std::fs::write(target, content)?;
                }
            }
            Err(_) => failed.lock().unwrap().push(relative.clone()),
        }
        Ok(())
    })?;
    let failed = write_failed(out, failed)?;
    println!("{} files, {} exported, {} failed", files.len(), files.len() - failed, failed);
    Ok(ExitCode::SUCCESS)
}

fn run_build(kind: Kind, root: &Path, folders: &Path, out: &Path, check: bool) -> Result<ExitCode> {
    let files: Vec<String> = game_files(root, kind)?
        .into_iter()
        .filter(|relative| folders.join(format!("{relative}{}", kind.suffix())).is_dir())
        .collect();
    let failed = Mutex::new(Vec::new());
    let differ = Mutex::new(Vec::new());
    files.par_iter().try_for_each(|relative| -> Result<()> {
        let original = std::fs::read(root.join(relative))?;
        match kind.build(&folders.join(format!("{relative}{}", kind.suffix())), &original) {
            Ok(built) if check => {
                if built != original {
                    differ.lock().unwrap().push(relative.clone());
                }
            }
            Ok(built) => {
                let target = out.join(relative);
                std::fs::create_dir_all(target.parent().unwrap())?;
                std::fs::write(target, built)?;
            }
            Err(error) => failed.lock().unwrap().push(format!("{relative}\t{error}")),
        }
        Ok(())
    })?;
    let failed = write_failed(out, failed)?;
    let mut differ = differ.into_inner().unwrap();
    differ.sort();
    for relative in &differ {
        println!("differs: {relative}");
    }
    if check {
        println!("{} files built, {} failed, {} differ from the original", files.len(), failed, differ.len());
    } else {
        println!("{} files built, {} failed", files.len(), failed);
    }
    Ok(ExitCode::SUCCESS)
}
