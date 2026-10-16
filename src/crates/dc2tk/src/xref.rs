//! xref, maps and map: what starts an event, what enters a map, where a monster is used, and the
//! catalog of the game's maps.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{bail, Result};
use clap::{Args, ValueEnum};

use dc2tk_disc::DiscSource;
use dc2tk_script::index::{self, Folded};
use dc2tk_script::{maps, xref};

use crate::files;
use crate::options::Common;

#[derive(Clone, Copy, ValueEnum)]
pub enum Kind {
    /// What starts or requests a program.
    Event,
    /// Every way into a map.
    Map,
    /// A monster's model, AI script, sharers and placements.
    Monster,
}

#[derive(Args)]
pub struct XrefArgs {
    /// A decompiled source tree.
    source: PathBuf,
    /// What to look up.
    kind: Kind,
    /// A program number or name, a map name (s51), or a monster number, constant or name.
    #[arg(allow_negative_numbers = true)]
    what: String,
    /// For event: only this map's programs and entries.
    #[arg(long)]
    map: Option<String>,
    /// The disc image or extracted disc, for monster models and AI scripts.
    #[arg(long, alias = "extracted", default_value = "extracted/disc")]
    disc: PathBuf,
    #[command(flatten)]
    common: Common,
}

#[derive(Args)]
pub struct MapsArgs {
    /// A decompiled source tree.
    source: PathBuf,
    /// The disc image or extracted disc, for map names.
    #[arg(long, alias = "extracted", default_value = "extracted/disc")]
    disc: PathBuf,
    #[command(flatten)]
    common: Common,
}

#[derive(Args)]
pub struct MapArgs {
    /// A decompiled source tree.
    source: PathBuf,
    /// The map's code, as in s17.
    name: String,
    /// The disc image or extracted disc, for names and .map files.
    #[arg(long, alias = "extracted", default_value = "extracted/disc")]
    disc: PathBuf,
    #[command(flatten)]
    common: Common,
}

/// The disc names and .map files come from; None, with a note, when there is nothing at path.
fn disc_at(path: &Path) -> Result<Option<DiscSource>> {
    if !path.exists() {
        eprintln!("note: no disc at {}, so no names from it", path.display());
        return Ok(None);
    }
    Ok(Some(DiscSource::open(path)?))
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
    let rows = xref::monster_rows(disc_at(&args.disc)?.as_ref(), &defs);
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
    let found = maps::catalog(&args.source, disc_at(&args.disc)?.as_ref(), &defs, &consts)?;
    print(&maps::summary(&found))
}

pub fn run_map(args: &MapArgs) -> Result<ExitCode> {
    let defs = args.common.definitions()?;
    let headers = index::tree_headers(&args.source, &defs, &args.common.sdk_dir())?;
    let consts = index::constants(headers.values());
    let disc = disc_at(&args.disc)?;
    let found = maps::catalog(&args.source, disc.as_ref(), &defs, &consts)?;
    let programs = index::enum_values(headers.values(), "program");
    print(&maps::detail(&args.name, &found, &args.source, disc.as_ref(), &programs)?)
}
