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
        Some(path) => Ok(members(&disc.read(path)?)
            .into_iter()
            .filter(|(name, _)| name.to_lowercase().ends_with(".img"))
            .collect()),
        None => Ok(Vec::new()),
    }
}

/// The texture packs to look pictures up in, in one order whatever form the disc is in (an
/// image or an extracted folder): the map's own first, then the shared ones (map/cmn), then every
/// other map's by path. A few maps use pictures of shared or paired maps, and several maps have
/// pictures of the same name.
fn texture_packs(disc: &Disc, own: &mapedit::MapFiles) -> Vec<String> {
    let mut others: Vec<String> = disc
        .entries()
        .iter()
        .map(|entry| entry.path.clone())
        .filter(|path| path.to_lowercase().starts_with("map/") && path.to_lowercase().ends_with(".ipk"))
        .filter(|path| Some(path) != own.ipk.as_ref())
        .collect();
    others.sort_by_key(|path| (!path.to_lowercase().starts_with("map/cmn/"), path.to_lowercase()));
    own.ipk.iter().cloned().chain(others).collect()
}

/// Texture files to look pictures up in, from texture_packs.
fn texture_search(disc: &Disc, own: &mapedit::MapFiles) -> Result<TextureSearch> {
    let packs = texture_packs(disc, own);
    let files: Vec<Vec<(String, Vec<u8>)>> =
        packs.par_iter().map(|path| images_of(disc, &Some(path.clone()))).collect::<Result<_>>()?;
    Ok(TextureSearch::new(&files.into_iter().flatten().collect::<Vec<_>>())?)
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
    /// The file's size on the disc; 0 for a new map's file.
    pub old: usize,
    pub new: usize,
}

/// Where a new map's file is: the like map's file renamed and moved to the new map's folder
/// (`map/m/m05/m05.mpk` for m05x is `map/m/m05/m05x.mpk`, as GetMapPath finds it).
fn new_map_path(path: &str, like: &mapedit::MapFiles, name: &str) -> String {
    let file = path.rsplit('/').next().unwrap_or(path);
    let suffix = file.get(like.name.len()..).unwrap_or_default();
    format!("{}/{name}{}", mapedit::rule_folder(name), suffix.to_lowercase())
}

/// Builds the chosen mods' map: lines (an edited scene of `c2 map-export`) into the disc files:
/// the map's .map rebuilt from the scene, and the models whose meshes changed rebuilt into their
/// model packs. A map: line under "add" makes a new map of its like map's files (docs/assets/maps.md
/// section 9): each written under the new name, the scene's edits made in the copies, a row
/// appended to every language's map table. `files` holds what the build already rewrote, by
/// DATA.DAT path.
pub fn apply_maps(
    iso: &Path,
    mod_dirs: &[PathBuf],
    files: &c2_script::discfiles::Files,
    program_of: &dyn Fn(&c2_script::replace::Line, &serde_json::Value) -> Result<i64>,
) -> Result<(c2_script::discfiles::Files, Vec<MapChange>)> {
    let lines: Vec<c2_script::replace::Line> =
        c2_script::replace::all_lines(mod_dirs)?.into_iter().filter(|line| line.is_map()).collect();
    let mut out = files.clone();
    let mut changes = Vec::new();
    if lines.is_empty() {
        return Ok((out, changes));
    }
    let disc = Disc::open(iso)?;
    let new_maps = c2_script::maptable::new_maps(&lines)?;
    for line in &lines {
        let name = line.map_name();
        let source = if line.add { line.like.clone().unwrap_or_default() } else { name.clone() };
        if line.add && mapedit::map_files(disc.entries(), &name).is_ok() {
            bail!(
                "{} (mod {}): the disc has a map {name}; edit it with a map: line under \"replace\"",
                line.key,
                line.module
            );
        }
        let found = mapedit::map_files(disc.entries(), &source)
            .with_context(|| format!("{} (mod {})", line.key, line.module))?;
        // where a file of the map goes: its own place, or a new map's
        let place = |path: &str| if line.add { new_map_path(path, &found, &name) } else { path.to_string() };
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
        let mut gltf = fs::read(&line.file).with_context(|| line.file.display().to_string())?;
        if !line.programs.is_empty() {
            let mut given = Vec::new();
            for (point, program) in &line.programs {
                let number = program_of(line, program)
                    .with_context(|| format!("{} (mod {}): \"programs\"", line.key, line.module))?;
                println!("  {} (map {}): point {point} runs program {number}", line.module, name);
                given.push((point.clone(), number));
            }
            gltf = with_programs(&gltf, &given).with_context(|| format!("{} (mod {})", line.key, line.module))?;
        }
        let imported = c2_assets::map_import::import_map(&gltf, &read, &original, &packs, &search)
            .with_context(|| format!("{} (mod {})", line.key, line.module))?;
        for note in &imported.notes {
            println!("note: {} (mod {}): {note}", line.key, line.module);
        }
        if let Some(text) = imported.map {
            changes.push(MapChange {
                module: line.module.clone(),
                map: name.clone(),
                path: place(&found.map),
                old: if line.add { 0 } else { original.len() },
                new: text.len(),
            });
            out.insert(place(&found.map), text);
        }
        if !imported.models.is_empty() || !imported.new_models.is_empty() {
            let Some((mpk_path, mut mpk_data)) = mpk else {
                bail!("{} (mod {}): the map has no model pack to rebuild models in", line.key, line.module);
            };
            let old_size = mpk_data.len();
            for (model, data) in &imported.models {
                let Some(pack) = packs.iter().find(|pack| pack.files.contains_key(&model.to_lowercase())) else {
                    continue;
                };
                // the member's own spelling, as the pack lists it
                let pcp_data = members(&mpk_data)
                    .into_iter()
                    .find(|(name, _)| name.eq_ignore_ascii_case(&pack.name))
                    .map(|(_, data)| data);
                let member = pcp_data
                    .as_deref()
                    .map(members)
                    .and_then(|list| {
                        list.into_iter().find(|(name, _)| name.eq_ignore_ascii_case(model)).map(|(name, _)| name)
                    })
                    .unwrap_or_else(|| model.clone());
                mpk_data = c2_disc::rebuild::replaced_member(&mpk_data, &[pack.name.clone(), member.clone()], data)?;
                println!("  {} (map {}): rebuilt {} in {}", line.module, name, member, pack.name);
            }
            if !imported.new_models.is_empty() {
                mpk_data = with_new_models(mpk_data, &found.name, &imported.new_models)
                    .with_context(|| format!("{} (mod {})", line.key, line.module))?;
                for model in &imported.new_models {
                    println!(
                        "  {} (map {}): added {} ({})",
                        line.module,
                        name,
                        model.file,
                        map_export::kind_name(model.kind)
                    );
                }
            }
            changes.push(MapChange {
                module: line.module.clone(),
                map: name.clone(),
                path: place(&mpk_path),
                old: if line.add { 0 } else { old_size },
                new: mpk_data.len(),
            });
            out.insert(place(&mpk_path), mpk_data);
        }
        if !imported.textures.is_empty() {
            let current = |path: &str| -> Result<Vec<u8>> {
                match out.iter().find(|(known, _)| known.eq_ignore_ascii_case(path)) {
                    Some((_, data)) => Ok(data.clone()),
                    None => disc.read(path),
                }
            };
            let rewritten = with_pictures(&disc, &found, &imported.textures, &current)
                .with_context(|| format!("{} (mod {})", line.key, line.module))?
                .into_iter()
                .map(|Rewritten { path, data, said }| Ok((current(&path)?.len(), path, data, said)))
                .collect::<Result<Vec<_>>>()?;
            for (old, path, data, said) in rewritten {
                // a new map's own pictures go into its copy of the like map's texture pack
                let own = line.add && Some(&path) == found.ipk.as_ref();
                let path = if own { place(&path) } else { path };
                for text in said {
                    let text = if own { text.replace(found.ipk.as_deref().unwrap_or_default(), &path) } else { text };
                    println!("  {} (map {}): {text}", line.module, name);
                }
                let old = if own { 0 } else { old };
                changes.push(MapChange {
                    module: line.module.clone(),
                    map: name.clone(),
                    path: path.clone(),
                    old,
                    new: data.len(),
                });
                out.insert(path, data);
            }
        }
        if line.add {
            // the like map's other files, copied as they are
            let theirs = [
                Some(&found.map),
                found.mpk.as_ref(),
                found.ipk.as_ref(),
                found.cfg.as_ref(),
                found.efp.as_ref(),
                found.sky.as_ref(),
            ];
            let mut copies = Vec::new();
            for path in theirs.into_iter().flatten() {
                let target = place(path);
                if !out.contains_key(&target) {
                    let data = match out.iter().find(|(known, _)| known.eq_ignore_ascii_case(path)) {
                        Some((_, data)) => data.clone(),
                        None => disc.read(path)?,
                    };
                    copies.push((path.clone(), target, data));
                }
            }
            for (path, target, data) in copies {
                changes.push(MapChange {
                    module: line.module.clone(),
                    map: name.clone(),
                    path: target.clone(),
                    old: 0,
                    new: data.len(),
                });
                println!("  {} (map {}): {target}, a copy of {path}", line.module, name);
                out.insert(target, data);
            }
        }
    }
    if !new_maps.is_empty() {
        // a row for each new map in every language's map table
        let module =
            lines.iter().filter(|line| line.add).map(|line| line.module.clone()).collect::<Vec<_>>().join(", ");
        let names = new_maps.iter().map(|map| map.name.as_str()).collect::<Vec<_>>().join(", ");
        for language in c2_script::maptable::LANGUAGES {
            let path = c2_script::startat::map_table_path(Some(language));
            let table = match out.iter().find(|(known, _)| known.eq_ignore_ascii_case(&path)) {
                Some((_, data)) => data.clone(),
                None => match disc.read(&path) {
                    Ok(data) => data,
                    Err(_) => continue,
                },
            };
            let (written, numbers) =
                c2_script::maptable::with_new_maps(&table, &new_maps).with_context(|| path.clone())?;
            if language == 1 {
                for (map, number) in new_maps.iter().zip(&numbers) {
                    println!("  map {} (like {}) is map number {number}", map.name, map.like);
                    if *number >= c2_script::maptable::FLAGGED_MAPS {
                        println!(
                            "warning: map {} is numbered {number}; the save keeps map flags (opened chests and the like) for numbers below {}",
                            map.name,
                            c2_script::maptable::FLAGGED_MAPS
                        );
                    }
                }
            }
            changes.push(MapChange {
                module: module.clone(),
                map: names.clone(),
                path: path.clone(),
                old: table.len(),
                new: written.len(),
            });
            out.insert(path, written);
        }
    }
    Ok((out, changes))
}

/// The scene with event points given programs (a map: line's "programs"): each named event point's
/// FUNC_EVENT_DATA program (`data.program`: the program a door runs when its door is open, an event
/// point when it is reached), by the point's name.
fn with_programs(gltf: &[u8], programs: &[(String, i64)]) -> Result<Vec<u8>> {
    use serde_json::Value;
    let mut doc: Value = serde_json::from_slice(gltf).context("the scene's glTF")?;
    let nodes = doc["nodes"].as_array_mut().context("the scene's glTF has no nodes")?;
    for (point, number) in programs {
        let mut found = 0;
        for node in nodes.iter_mut() {
            // the add-on keeps extras.c2 as an object; a scene through other tools may hold it as text
            let as_text = node["extras"]["c2"].as_str().map(str::to_string);
            let mut c2 = match &as_text {
                Some(text) => serde_json::from_str::<Value>(text).unwrap_or(Value::Null),
                None => node["extras"]["c2"].clone(),
            };
            let named = match c2["name"].as_str() {
                Some(name) => name.to_string(),
                None => {
                    let shown = node["name"].as_str().unwrap_or_default();
                    let shown = shown.strip_prefix("point:").unwrap_or(shown);
                    // Blender's .001 suffixes
                    match shown.rsplit_once('.') {
                        Some((stem, digits))
                            if digits.len() >= 3 && digits.bytes().all(|byte| byte.is_ascii_digit()) =>
                        {
                            stem.to_string()
                        }
                        _ => shown.to_string(),
                    }
                }
            };
            if c2["role"] != "point" || c2["kind"] != "event" || named != *point {
                continue;
            }
            if !c2["data"].is_object() {
                bail!("event point {point} has no data to give a program");
            }
            c2["data"]["program"] = Value::from(*number);
            node["extras"]["c2"] = match as_text {
                Some(_) => Value::String(c2.to_string()),
                None => c2,
            };
            found += 1;
        }
        if found == 0 {
            bail!("\"programs\" names {point}, but the scene has no event point of that name");
        }
    }
    Ok(serde_json::to_vec(&doc)?)
}

/// Pictures as (file name or picture name, PNG).
type Pngs = Vec<(String, Vec<u8>)>;

/// A texture pack rewritten: its path, bytes, and what was done to it.
struct Rewritten {
    path: String,
    data: Vec<u8>,
    said: Vec<String>,
}

/// The map's texture packs (.ipk) with the scene's pictures put in: a changed picture into the
/// texture file holding it (the map's own first, then other maps', which then show it changed
/// too), a new one at the end of the map's own first texture file.
fn with_pictures(
    disc: &Disc,
    own: &mapedit::MapFiles,
    edits: &[c2_assets::map_import::TextureEdit],
    current: &dyn Fn(&str) -> Result<Vec<u8>>,
) -> Result<Vec<Rewritten>> {
    // texture packs in search order: the map's own, then every other map's
    let packs = texture_packs(disc, own);
    // (pack, texture file) -> (file name, PNG) of its changed pictures
    let mut changed: BTreeMap<(String, String), Pngs> = BTreeMap::new();
    let mut added: Pngs = Vec::new();
    let mut opened: BTreeMap<String, Pngs> = BTreeMap::new();
    for edit in edits {
        if edit.new {
            added.push((edit.name.clone(), edit.png.clone()));
            continue;
        }
        let mut home = None;
        'packs: for pack in &packs {
            if !opened.contains_key(pack) {
                let files = members(&current(pack)?)
                    .into_iter()
                    .filter(|(name, _)| name.to_lowercase().ends_with(".img"))
                    .collect();
                opened.insert(pack.clone(), files);
            }
            for (file, data) in &opened[pack] {
                let pictures = c2_assets::im3::read(data).unwrap_or_default();
                if pictures.iter().any(|picture| picture.name == edit.name) {
                    home = Some((pack.clone(), file.clone()));
                    break 'packs;
                }
            }
        }
        let Some(home) = home else {
            bail!("picture {}: no texture file of the disc holds it", edit.name);
        };
        changed.entry(home).or_default().push((c2_assets::textures::file_name(&edit.name), edit.png.clone()));
    }
    let mut out: BTreeMap<String, (Vec<u8>, Vec<String>)> = BTreeMap::new();
    for ((pack, file), pngs) in changed {
        let (data, said) = match out.remove(&pack) {
            Some(found) => found,
            None => (current(&pack)?, Vec::new()),
        };
        let names = [file.clone()];
        let img = c2_disc::rebuild::read_member(&data, &names).with_context(|| format!("{pack} has no {file}"))?;
        let built = c2_assets::textures::build_from(&pngs, img)?;
        let data = c2_disc::rebuild::replaced_member(&data, &names, &built)?;
        let mut said = said;
        let shared = if Some(&pack) == own.ipk.as_ref() {
            String::new()
        } else {
            " (another map's: it changes there too)".into()
        };
        for (png, _) in &pngs {
            said.push(format!(
                "repainted {} in {pack}/{file}{shared}",
                png.trim_end_matches(c2_assets::textures::SUFFIX)
            ));
        }
        out.insert(pack, (data, said));
    }
    if !added.is_empty() {
        let Some(pack) = &own.ipk else {
            bail!(
                "the map has no texture pack to add pictures {} to",
                added.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>().join(", ")
            );
        };
        let (data, mut said) = match out.remove(pack) {
            Some(found) => found,
            None => (current(pack)?, Vec::new()),
        };
        let Some((file, img)) = members(&data).into_iter().find(|(name, _)| name.to_lowercase().ends_with(".img"))
        else {
            bail!("{pack} holds no texture file to add pictures to");
        };
        let built = c2_assets::textures::with_new_pictures(&added, &img)?;
        let data = c2_disc::rebuild::replaced_member(&data, std::slice::from_ref(&file), &built)?;
        for (name, _) in &added {
            said.push(format!("added picture {name} to {pack}/{file}"));
        }
        out.insert(pack.clone(), (data, said));
    }
    Ok(out.into_iter().map(|(path, (data, said))| Rewritten { path, data, said }).collect())
}

/// The most models and characters a model pack holds (CMdsList::LoadPCPFile).
const PACK_MODELS: usize = 0x1FF;

/// The map's model packs with the new models added to its first (`NAME_0.pcp` when it has one),
/// each with an info.cfg entry giving its TYPE.
fn with_new_models(mpk: Vec<u8>, map: &str, models: &[c2_assets::map_import::NewModel]) -> Result<Vec<u8>> {
    let pcps: Vec<(String, Vec<u8>)> = members(&mpk);
    let first = format!("{map}_0.pcp");
    let Some((pcp, pcp_data)) = pcps.iter().find(|(name, _)| name.eq_ignore_ascii_case(&first)).or(pcps.first()) else {
        bail!("the map's model pack holds no .pcp");
    };
    let inside = members(pcp_data);
    let count =
        inside.iter().filter(|(name, _)| [".mds", ".chr"].iter().any(|end| name.to_lowercase().ends_with(end))).count();
    if count + models.len() > PACK_MODELS {
        bail!("{pcp} holds {count} models; {} more would pass the game's {PACK_MODELS}", models.len());
    }
    let Some((cfg_name, cfg)) = inside.iter().find(|(name, _)| name.eq_ignore_ascii_case("info.cfg")) else {
        bail!("{pcp} has no info.cfg");
    };
    let mut out = mpk;
    let names = vec![pcp.clone()];
    for model in models {
        out = c2_disc::rebuild::added_member(&out, &names, &model.file, &model.data)?;
    }
    let entries: Vec<(&str, i64)> = models.iter().map(|model| (model.file.as_str(), model.kind)).collect();
    let text = info_cfg_with(&c2_disc::text::decode_lossy(cfg), &entries);
    let cfg_bytes = c2_disc::text::encode(&text).with_context(|| format!("{pcp}: info.cfg is not Shift-JIS"))?;
    Ok(c2_disc::rebuild::replaced_member(&out, &[pcp.clone(), cfg_name.clone()], &cfg_bytes)?)
}

/// A model pack's info.cfg with an entry (`MDS "file","file"; TYPE t; MDS_END;`) added after its
/// last for each model, in its own line endings.
fn info_cfg_with(cfg: &str, models: &[(&str, i64)]) -> String {
    let newline = if cfg.contains("\r\n") { "\r\n" } else { "\n" };
    let mut text = cfg.trim_end().to_string();
    let ending = &cfg[text.len()..];
    for (file, kind) in models {
        if !text.is_empty() {
            text.push_str(newline);
        }
        text.push_str(&format!("MDS \"{file}\",\"{file}\";{newline}\tTYPE {kind};{newline}MDS_END;"));
    }
    // the line ends and blank lines the file ended with
    text.push_str(if ending.is_empty() { newline } else { ending });
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_maps_files_take_its_name_and_folder() {
        let like = mapedit::MapFiles {
            name: "m05".into(),
            folder: "map/m/m05".into(),
            map: "map/m/m05/m05.map".into(),
            mpk: Some("map/m/m05/m05.mpk".into()),
            ipk: None,
            efp: None,
            sky: None,
            cfg: None,
        };
        assert_eq!(new_map_path(&like.map, &like, "m05x"), "map/m/m05/m05x.map");
        assert_eq!(new_map_path("map/m/m05/m05.MPK", &like, "park2b"), "map/p/par/k2b/park2b.mpk");
    }

    #[test]
    fn programs_are_given_to_event_points_by_name() {
        let scene = serde_json::json!({"nodes": [
            {"name": "point:door_park", "extras": {"c2": {"role": "point", "kind": "event", "data": {"kind": "door", "program": 0}}}},
            {"name": "point:dun07.001", "extras": {"c2": "{\"role\": \"point\", \"kind\": \"event\", \"data\": {\"program\": 290}}"}},
            {"name": "point:door_park2", "extras": {"c2": {"role": "point", "kind": "pos", "data": {}}}}
        ]});
        let written =
            with_programs(scene.to_string().as_bytes(), &[("door_park".into(), 9001), ("dun07".into(), 9002)]).unwrap();
        let doc: serde_json::Value = serde_json::from_slice(&written).unwrap();
        assert_eq!(doc["nodes"][0]["extras"]["c2"]["data"]["program"], 9001);
        let text: serde_json::Value = serde_json::from_str(doc["nodes"][1]["extras"]["c2"].as_str().unwrap()).unwrap();
        assert_eq!(text["data"]["program"], 9002);
        let missing = with_programs(scene.to_string().as_bytes(), &[("door_park2".into(), 1)]).unwrap_err();
        assert!(missing.to_string().contains("no event point"));
    }

    #[test]
    fn info_cfg_gets_an_entry_per_new_model() {
        let cfg = "PCP_NAME \"s17\";\nMDS \"s17_01-m.mds\",\"Y:\\s17_01-m.mds\";\n\tTYPE 0;\nMDS_END;\n\n\n";
        let with = info_cfg_with(cfg, &[("s17_box-m.mds", 0), ("s17_box-a.mds", 1)]);
        assert_eq!(
            with,
            "PCP_NAME \"s17\";\nMDS \"s17_01-m.mds\",\"Y:\\s17_01-m.mds\";\n\tTYPE 0;\nMDS_END;\n\
             MDS \"s17_box-m.mds\",\"s17_box-m.mds\";\n\tTYPE 0;\nMDS_END;\n\
             MDS \"s17_box-a.mds\",\"s17_box-a.mds\";\n\tTYPE 1;\nMDS_END;\n\n\n"
        );
        assert_eq!(
            info_cfg_with("A;\r\n", &[("b.mds", 2)]),
            "A;\r\nMDS \"b.mds\",\"b.mds\";\r\n\tTYPE 2;\r\nMDS_END;\r\n"
        );
    }
}
