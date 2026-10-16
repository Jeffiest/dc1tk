//! dc2tk map-export: a map as a glTF scene for Blender (docs/assets/maps.md sections 7 and 8).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use clap::Args;

use dc2tk_assets::map_export::{self, ModelPack};
use dc2tk_assets::mapfile;
use dc2tk_disc::DiscSource;
use dc2tk_script::mapedit::{self, members};
use dc2tk_script::maps;
use dc2tk_script::paths;

use crate::options::Common;

/// Where map-export and setup write the maps, a folder each.
pub const MAPS: &str = "extracted/maps";

#[derive(Args)]
pub struct MapExportArgs {
    /// The map's name (s17), or --all for every map with placements.
    name: Option<String>,
    /// Folder to write: NAME.gltf, models/ and textures/ (default extracted/maps/NAME).
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// The disc: an extracted disc folder or the disc image.
    #[arg(long, default_value = "extracted/disc")]
    disc: PathBuf,
    /// Export every map that places its templates (into extracted/maps/NAME, or -o's folder/NAME).
    #[arg(long)]
    all: bool,
    /// Write nothing: list every map there is to export, a line each, its name, a tab and its
    /// title in the definitions' language (empty for a map the game names nowhere), as the
    /// Blender add-on's map list reads them.
    #[arg(long, conflicts_with_all = ["name", "output", "all"])]
    list: bool,
    #[command(flatten)]
    common: Common,
}

/// The maps of the disc, by name (lower case): every map/ folder's .map file.
fn map_names(disc: &DiscSource) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for path in disc.paths() {
        let lower = path.to_lowercase();
        if lower.starts_with("map/") && lower.ends_with(".map") {
            names.insert(lower.rsplit('/').next().unwrap_or(&lower).trim_end_matches(".map").to_string());
        }
    }
    names
}

/// What the scene shows besides the map: the sky the engine loads for it (the map's own .sky, else
/// map/def.sky, as GetLoadMapInfo names them, when SKY_INFO turns the sky on) and its .cfg.
fn display_of(disc: &DiscSource, files: &mapedit::MapFiles, map: &mapfile::MapFile) -> Result<map_export::Display> {
    let mut display = map_export::Display::default();
    if map.settings.sky_info.sky.int() != 0 {
        let sky = files
            .sky
            .clone()
            .or_else(|| disc.paths().iter().find(|path| path.eq_ignore_ascii_case("map/def.sky")).cloned());
        if let Some(path) = sky {
            display.sky = members(&disc.read(&path)?);
        }
    }
    if let Some(cfg) = &files.cfg {
        display.cfg = Some(disc.read(cfg)?.into_owned());
    }
    Ok(display)
}

/// Exports one map; returns the report line.
fn export_one(disc: &DiscSource, name: &str, out: &Path) -> Result<String> {
    let files = mapedit::map_files(disc.paths(), name)?;
    let map = mapfile::parse(&disc.read(&files.map)?).with_context(|| files.map.clone())?;
    let mut packs = Vec::new();
    if let Some(mpk) = &files.mpk {
        for (pcp, data) in members(&disc.read(mpk)?) {
            packs.push(ModelPack::new(&pcp, members(&data)));
        }
    }
    let search = mapedit::texture_search(disc, &files)?;
    let display = display_of(disc, &files, &map)?;
    let (written, report) = map_export::export_map(&files.name, &map, &packs, &search, &display)?;
    if out.exists() {
        fs::remove_dir_all(out).with_context(|| format!("cannot replace {}", out.display()))?;
    }
    for (path, data) in &written {
        let target = paths::join_relative(out, path);
        if let Some(folder) = target.parent() {
            fs::create_dir_all(folder).with_context(|| format!("cannot create {}", folder.display()))?;
        }
        fs::write(&target, data).with_context(|| format!("cannot write {}", target.display()))?;
    }
    let placed = if map.placements.is_empty() {
        format!("a template library: its {} templates shown once each, placed nowhere", map.templates.len())
    } else {
        format!("{} placements", map.placements.len())
    };
    let mut line = format!(
        "{}: {placed}, {} models, {} nodes, written to {}",
        files.name,
        report.models,
        report.nodes,
        out.join(format!("{}.gltf", files.name)).display()
    );
    if !report.missing_textures.is_empty() {
        line.push_str(&format!(
            "\n  {} textures are nowhere on the disc (placeholders): {}",
            report.missing_textures.len(),
            report.missing_textures.join(", ")
        ));
    }
    if !report.absent.is_empty() {
        line.push_str(&format!(
            "\n  {} pieces name models in none of the map's packs (the game draws nothing there either)",
            report.absent.len()
        ));
    }
    for skipped in &report.skipped {
        line.push_str(&format!("\n  {skipped}"));
    }
    Ok(line)
}

/// Exports every map that places its templates into `root`/NAME, each folder replaced; returns
/// each map's report line.
pub fn export_all(disc: &DiscSource, root: &Path) -> Result<Vec<String>> {
    let mut lines = Vec::new();
    for name in &map_names(disc) {
        let files = mapedit::map_files(disc.paths(), name)?;
        let map = mapfile::parse(&disc.read(&files.map)?).with_context(|| files.map.clone())?;
        if map.placements.is_empty() {
            continue;
        }
        lines.push(export_one(disc, name, &root.join(name))?);
    }
    Ok(lines)
}

pub fn run_map_export(args: &MapExportArgs) -> Result<ExitCode> {
    let disc = DiscSource::open(&args.disc)?;
    if args.list {
        let titles = maps::map_names(&disc, &args.common.definitions()?);
        for name in map_names(&disc) {
            println!("{name}\t{}", titles.get(&name).map_or("", String::as_str));
        }
        return Ok(ExitCode::SUCCESS);
    }
    if args.all {
        let root = args.output.clone().unwrap_or_else(|| PathBuf::from(MAPS));
        let lines = export_all(&disc, &root)?;
        for line in &lines {
            println!("{line}");
        }
        println!("{} maps exported to {}", lines.len(), root.display());
        return Ok(ExitCode::SUCCESS);
    }
    let Some(name) = &args.name else {
        bail!("name a map (dc2tk map-export s17), or --all");
    };
    let out = args.output.clone().unwrap_or_else(|| PathBuf::from(MAPS).join(name));
    println!("{}", export_one(&disc, name, &out)?);
    Ok(ExitCode::SUCCESS)
}
