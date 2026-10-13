//! xref, maps and map: what starts an event, what enters a map, where a monster is used, and the
//! catalog of the game's maps.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{bail, Result};
use clap::{Args, ValueEnum};

use c2_script::index::{self, Folded};
use c2_script::{maps, xref};

use crate::{files, Common};

#[derive(Clone, Copy, ValueEnum)]
pub enum Kind {
    Event,
    Map,
    Monster,
}

#[derive(Args)]
pub struct XrefArgs {
    /// A decompiled source tree.
    source: PathBuf,
    kind: Kind,
    /// A program number or name, a map name (s51), or a monster number, constant or name.
    #[arg(allow_negative_numbers = true)]
    what: String,
    /// For event: only this map's programs and entries.
    #[arg(long)]
    map: Option<String>,
    /// The extracted disc, for monster models and AI scripts.
    #[arg(long, default_value = "extracted")]
    extracted: PathBuf,
    #[command(flatten)]
    common: Common,
}

#[derive(Args)]
pub struct MapsArgs {
    /// A decompiled source tree.
    source: PathBuf,
    /// The extracted disc, for map names.
    #[arg(long, default_value = "extracted")]
    extracted: PathBuf,
    #[command(flatten)]
    common: Common,
}

#[derive(Args)]
pub struct MapArgs {
    /// A decompiled source tree.
    source: PathBuf,
    /// The map's code, as in s17.
    name: String,
    /// The extracted disc, for names and .map files.
    #[arg(long, default_value = "extracted")]
    extracted: PathBuf,
    #[command(flatten)]
    common: Common,
}

fn print(text: &str) -> Result<ExitCode> {
    files::write_output(None, &format!("{text}\n"))?;
    Ok(ExitCode::SUCCESS)
}

pub fn run_xref(args: &XrefArgs) -> Result<ExitCode> {
    let defs = args.common.definitions()?;
    let consts = index::constants(index::tree_headers(&args.source, &defs, &args.common.sdk_dir())?.values());
    if let Kind::Map = args.kind {
        return print(&xref::map_report(&args.source, &consts, &args.what)?);
    }
    let rows = xref::monster_rows(&args.extracted, &defs);
    if let Kind::Monster = args.kind {
        return print(&xref::monster_report(&args.source, &consts, &rows, &args.what)?);
    }
    let Some(Folded::Int(number)) = index::value(&args.what, &consts) else {
        bail!("{} is not a program number or name", args.what);
    };
    print(&xref::event_report(&args.source, &consts, number, args.map.as_deref(), &rows)?)
}

pub fn run_maps(args: &MapsArgs) -> Result<ExitCode> {
    let defs = args.common.definitions()?;
    let consts = index::constants(index::tree_headers(&args.source, &defs, &args.common.sdk_dir())?.values());
    let found = maps::catalog(&args.source, &args.extracted, &defs, &consts)?;
    print(&maps::summary(&found))
}

pub fn run_map(args: &MapArgs) -> Result<ExitCode> {
    let defs = args.common.definitions()?;
    let headers = index::tree_headers(&args.source, &defs, &args.common.sdk_dir())?;
    let consts = index::constants(headers.values());
    let found = maps::catalog(&args.source, &args.extracted, &defs, &consts)?;
    let programs = index::enum_values(headers.values(), "program");
    print(&maps::detail(&args.name, &found, &args.source, &args.extracted, &programs)?)
}
