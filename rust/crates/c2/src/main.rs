//! c2: decompile, compile and mod Dark Chronicle. The Rust port of cli.py; commands print what
//! cli.py's do.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use c2_disc::{extract, rebuild, Image};

#[derive(Parser)]
#[command(name = "c2", about = "Decompile, compile and mod Dark Chronicle (Dark Cloud 2)")]
struct Cli {
    /// Print how long the command took.
    #[arg(long, global = true)]
    time: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Write every file in the disc's DATA.DAT.
    Extract { iso: PathBuf, out: String },
    /// Write a copy of an image with files of DATA.DAT replaced or added, from a folder holding
    /// files.json (a list of disc paths) and the files 0.bin, 1.bin, ... in that order.
    #[command(hide = true)]
    WriteDisc { iso: PathBuf, out: PathBuf, files: PathBuf },
}

/// A path as Python's pathlib prints it on this system.
fn python_path(path: &str) -> String {
    if !cfg!(windows) {
        return path.to_string();
    }
    let rooted = path.starts_with('/') || path.starts_with('\\');
    let parts: Vec<&str> = path.split(['/', '\\']).filter(|part| !part.is_empty() && *part != ".").collect();
    let joined = parts.join("\\");
    if rooted {
        format!("\\{joined}")
    } else {
        joined
    }
}

fn run_extract(iso: &Path, out: &str) -> Result<ExitCode> {
    let image = Image::open(iso).with_context(|| format!("cannot read {}", iso.display()))?;
    let manifest = extract::extract(image.bytes(), Path::new(out))?;
    let packed = manifest.files.iter().filter(|item| item.packed).count();
    println!("{} files written to {} ({} from packs)", manifest.files.len(), out, packed);
    println!("{} scripts listed in {}", manifest.scripts.len(), python_path(&format!("{out}/manifest.json")));
    Ok(ExitCode::SUCCESS)
}

fn run_write_disc(iso: &Path, out: &Path, folder: &Path) -> Result<ExitCode> {
    let listed: Vec<String> = serde_json_list(&folder.join("files.json"))?;
    let mut files = BTreeMap::new();
    for (number, path) in listed.into_iter().enumerate() {
        let data = std::fs::read(folder.join(format!("{number}.bin")))?;
        files.insert(path, data);
    }
    let written = rebuild::write_disc(iso, out, &files)?;
    for item in &written.placed {
        let how = if item.added {
            "new"
        } else if item.moved {
            "moved"
        } else {
            "in place"
        };
        println!("{} {} ({} bytes)", how, item.path, item.size);
    }
    if written.grown > 0 {
        println!("the image grew by {} sectors", written.grown);
    }
    if written.placed.iter().any(|item| item.added) {
        println!(
            "the game's file index has {} bytes free for more new files (each takes 13 plus its path)",
            written.index_free
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn serde_json_list(path: &Path) -> Result<Vec<String>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("{} is not a JSON list of paths", path.display()))
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let start = Instant::now();
    let result = match &cli.command {
        Command::Extract { iso, out } => run_extract(iso, out),
        Command::WriteDisc { iso, out, files } => run_write_disc(iso, out, files),
    };
    if cli.time {
        eprintln!("took {:.2?}", start.elapsed());
    }
    match result {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{error:#}");
            ExitCode::from(1)
        }
    }
}
