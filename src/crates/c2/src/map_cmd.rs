//! c2 map-export: a map as a glTF scene for Blender (docs/assets/maps.md sections 7 and 8).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use clap::Args;
use rayon::prelude::*;

use c2_assets::map_export::{self, ModelPack};
use c2_assets::mapfile;
use c2_assets::to_gltf::TextureSearch;
use c2_disc::archive::{self, Archive, Entry};
use c2_disc::Image;
use c2_script::mapedit;

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
}

/// The disc's DATA.DAT files, from the image or an extracted disc.
pub enum Disc {
    Image(Image, Archive),
    Folder(PathBuf, Vec<Entry>),
}

impl Disc {
    pub fn open(path: &Path) -> Result<Disc> {
        if path.is_file() {
            let image = Image::open(path).with_context(|| format!("cannot read {}", path.display()))?;
            let archive = Archive::open(image.bytes())?;
            return Ok(Disc::Image(image, archive));
        }
        if !path.join("manifest.json").is_file() {
            bail!("{} is neither a disc image nor an extracted disc (no manifest.json)", path.display());
        }
        let mut entries = Vec::new();
        let mut folders = vec![path.join("map")];
        while let Some(folder) = folders.pop() {
            for entry in fs::read_dir(&folder).with_context(|| format!("cannot read {}", folder.display()))?.flatten() {
                let file = entry.path();
                let relative = file.strip_prefix(path).unwrap().to_string_lossy().replace('\\', "/");
                if file.is_dir() {
                    if !relative.ends_with(".d") {
                        folders.push(file);
                    }
                } else {
                    entries.push(Entry { path: relative, size: 0, sector: 0 });
                }
            }
        }
        Ok(Disc::Folder(path.to_path_buf(), entries))
    }

    pub fn entries(&self) -> &[Entry] {
        match self {
            Disc::Image(_, archive) => &archive.entries,
            Disc::Folder(_, entries) => entries,
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>> {
        match self {
            Disc::Image(image, archive) => {
                let entry = archive.entry(path).with_context(|| format!("the disc has no {path}"))?;
                Ok(archive.read(image.bytes(), entry).to_vec())
            }
            Disc::Folder(root, _) => fs::read(root.join(path)).with_context(|| format!("cannot read {path}")),
        }
    }
}

/// A pack's members with their bytes.
fn members(data: &[u8]) -> Vec<(String, Vec<u8>)> {
    archive::parse_pack(data)
        .map(|members| members.into_iter().map(|member| (member.name.clone(), member.data(data).to_vec())).collect())
        .unwrap_or_default()
}

/// Every .img a map's .ipk holds.
fn images_of(disc: &Disc, ipk: &Option<String>) -> Result<Vec<(String, Vec<u8>)>> {
    match ipk {
        Some(path) => Ok(members(&disc.read(path)?).into_iter().filter(|(name, _)| name.to_lowercase().ends_with(".img")).collect()),
        None => Ok(Vec::new()),
    }
}

/// Texture files to look pictures up in: the map's own first, then every other map's (a few
/// maps use pictures of shared or paired maps).
fn texture_search(disc: &Disc, own: &mapedit::MapFiles) -> Result<TextureSearch> {
    let mut files = images_of(disc, &own.ipk)?;
    let others: Vec<&Entry> = disc
        .entries()
        .iter()
        .filter(|entry| entry.path.to_lowercase().starts_with("map/") && entry.path.to_lowercase().ends_with(".ipk"))
        .filter(|entry| Some(&entry.path) != own.ipk.as_ref())
        .collect();
    let more: Vec<Vec<(String, Vec<u8>)>> =
        others.par_iter().map(|entry| images_of(disc, &Some(entry.path.clone()))).collect::<Result<_>>()?;
    files.extend(more.into_iter().flatten());
    Ok(TextureSearch::new(&files)?)
}

/// Exports one map; returns the report line.
fn export_one(disc: &Disc, name: &str, out: &Path, search: Option<&TextureSearch>) -> Result<String> {
    let files = mapedit::map_files(disc.entries(), name)?;
    let map = mapfile::parse(&disc.read(&files.map)?).with_context(|| files.map.clone())?;
    let mut packs = Vec::new();
    if let Some(mpk) = &files.mpk {
        for (pcp, data) in members(&disc.read(mpk)?) {
            packs.push(ModelPack::new(&pcp, members(&data)));
        }
    }
    let owned;
    let search = match search {
        Some(search) => search,
        None => {
            owned = texture_search(disc, &files)?;
            &owned
        }
    };
    let (written, report) = map_export::export_map(&files.name, &map, &packs, search)?;
    if out.exists() {
        fs::remove_dir_all(out).with_context(|| format!("cannot replace {}", out.display()))?;
    }
    for (path, data) in &written {
        let target = path.split('/').fold(out.to_path_buf(), |folder, part| folder.join(part));
        fs::create_dir_all(target.parent().unwrap())?;
        fs::write(&target, data)?;
    }
    let mut line = format!(
        "{}: {} placements, {} models, {} nodes, written to {}",
        files.name,
        map.placements.len(),
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
            "
  {} pieces name models in none of the map's packs (the game draws nothing there either)",
            report.absent.len()
        ));
    }
    for skipped in &report.skipped {
        line.push_str(&format!("\n  {skipped}"));
    }
    Ok(line)
}

pub fn run_map_export(args: &MapExportArgs) -> Result<ExitCode> {
    let disc = Disc::open(&args.disc)?;
    if args.all {
        let root = args.output.clone().unwrap_or_else(|| PathBuf::from("extracted/maps"));
        let mut names: BTreeMap<String, ()> = BTreeMap::new();
        for entry in disc.entries() {
            let lower = entry.path.to_lowercase();
            if lower.starts_with("map/") && lower.ends_with(".map") {
                let name = lower.rsplit('/').next().unwrap().trim_end_matches(".map").to_string();
                names.insert(name, ());
            }
        }
        let mut done = 0;
        for name in names.keys() {
            let files = mapedit::map_files(disc.entries(), name)?;
            let map = mapfile::parse(&disc.read(&files.map)?).with_context(|| files.map.clone())?;
            if map.placements.is_empty() {
                continue;
            }
            println!("{}", export_one(&disc, name, &root.join(name), None)?);
            done += 1;
        }
        println!("{done} maps exported to {}", root.display());
        return Ok(ExitCode::SUCCESS);
    }
    let Some(name) = &args.name else {
        bail!("name a map (c2 map-export s17), or --all");
    };
    let out = args.output.clone().unwrap_or_else(|| PathBuf::from("extracted/maps").join(name));
    println!("{}", export_one(&disc, name, &out, None)?);
    Ok(ExitCode::SUCCESS)
}

/// What a mod's map: lines changed, for build to print.
pub struct MapChange {
    pub module: String,
    pub map: String,
    pub path: String,
    pub old: usize,
    pub new: usize,
}

/// Builds the chosen mods' map: lines (an edited scene of `c2 map-export`) into the disc files:
/// the map's .map rebuilt from the scene, and the models whose meshes changed rebuilt into their
/// model packs. `files` holds what the build already rewrote, by DATA.DAT path.
pub fn apply_maps(
    iso: &Path,
    mod_dirs: &[PathBuf],
    files: &c2_script::discfiles::Files,
) -> Result<(c2_script::discfiles::Files, Vec<MapChange>)> {
    let lines: Vec<c2_script::replace::Line> =
        c2_script::replace::all_lines(mod_dirs)?.into_iter().filter(|line| line.is_map()).collect();
    let mut out = files.clone();
    let mut changes = Vec::new();
    if lines.is_empty() {
        return Ok((out, changes));
    }
    let disc = Disc::open(iso)?;
    for line in &lines {
        let name = line.map_name();
        let found = mapedit::map_files(disc.entries(), &name)?;
        // a file an earlier step rewrote is the one to build on
        let current = |path: &str| -> Result<Vec<u8>> {
            match out.iter().find(|(known, _)| known.eq_ignore_ascii_case(path)) {
                Some((_, data)) => Ok(data.clone()),
                None => disc.read(path),
            }
        };
        let original = current(&found.map)?;
        let mut packs = Vec::new();
        let mpk = match &found.mpk {
            Some(path) => {
                let data = current(path)?;
                for (pcp, pcp_data) in members(&data) {
                    packs.push(ModelPack::new(&pcp, members(&pcp_data)));
                }
                Some((path.clone(), data))
            }
            None => None,
        };
        let search = texture_search(&disc, &found)?;
        let folder = line.file.parent().map(Path::to_path_buf).unwrap_or_default();
        let read = |relative: &str| -> c2_assets::Result<Vec<u8>> {
            let path = relative.split('/').fold(folder.clone(), |path, part| path.join(part));
            fs::read(&path).map_err(|error| c2_assets::Error::invalid(format!("{}: {error}", path.display())))
        };
        let gltf = fs::read(&line.file).with_context(|| line.file.display().to_string())?;
        let imported = c2_assets::map_import::import_map(&gltf, &read, &original, &packs, &search)
            .with_context(|| format!("{} (mod {})", line.key, line.module))?;
        for note in &imported.notes {
            println!("note: {} (mod {}): {note}", line.key, line.module);
        }
        if let Some(text) = imported.map {
            changes.push(MapChange { module: line.module.clone(), map: name.clone(), path: found.map.clone(), old: original.len(), new: text.len() });
            out.insert(found.map.clone(), text);
        }
        if !imported.models.is_empty() {
            let Some((mpk_path, mut mpk_data)) = mpk else {
                bail!("{} (mod {}): the map has no model pack to rebuild models in", line.key, line.module);
            };
            let old_size = mpk_data.len();
            for (model, data) in &imported.models {
                let Some(pack) = packs.iter().find(|pack| pack.files.contains_key(&model.to_lowercase())) else {
                    continue;
                };
                // the member's own spelling, as the pack lists it
                let pcp_data = members(&mpk_data).into_iter().find(|(name, _)| name.eq_ignore_ascii_case(&pack.name)).map(|(_, data)| data);
                let member = pcp_data
                    .as_deref()
                    .map(members)
                    .and_then(|list| list.into_iter().find(|(name, _)| name.eq_ignore_ascii_case(model)).map(|(name, _)| name))
                    .unwrap_or_else(|| model.clone());
                mpk_data = c2_disc::rebuild::replaced_member(&mpk_data, &[pack.name.clone(), member.clone()], data)?;
                println!("  {} (map {}): rebuilt {} in {}", line.module, name, member, pack.name);
            }
            changes.push(MapChange { module: line.module.clone(), map: name.clone(), path: mpk_path.clone(), old: old_size, new: mpk_data.len() });
            out.insert(mpk_path, mpk_data);
        }
    }
    Ok((out, changes))
}
