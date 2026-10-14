//! build, mod and play: compiling a tree with mods layered over it into a new disc image, making
//! and checking mods, and booting an image in PCSX2.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{bail, Result};
use clap::{Args, Subcommand};
use indexmap::IndexMap;

use c2_disc::rebuild;
use c2_script::compile::Toolchain;
use c2_script::{
    build, definitions, discfiles, editor, index, loads, memory, migrate, mods, patches, renames, replace, sdk, slim,
    startat, tree,
};

use crate::Common;
use crate::{asset_tree, files};

#[derive(Args)]
pub struct BuildArgs {
    /// The source tree, as setup or decompile wrote it.
    source: PathBuf,
    /// The retail disc image the source came from.
    iso: PathBuf,
    /// Disc image to write.
    #[arg(short, long)]
    output: PathBuf,
    /// Test image only: a new game goes straight to this map and program, as in s51:story_50, or
    /// to a map at map_start, as in m05.
    #[arg(long, value_name = "MAP[:PROGRAM]")]
    start_at: Option<String>,
    /// Layer this mod over the tree; repeat for several, which may not change the same file. The
    /// mods each requires are layered in too, and "requires" and "after" set the order.
    #[arg(long = "mod", value_name = "NAME")]
    mod_names: Vec<String>,
    /// The folder that holds the mods.
    #[arg(long, default_value = "mods")]
    mods: PathBuf,
    /// Ignore a chosen mod's start_at.
    #[arg(long)]
    no_start_at: bool,
    /// Boot the image in PCSX2 when it is built.
    #[arg(long)]
    play: bool,
    /// PCSX2's executable, for --play.
    #[arg(long)]
    pcsx2: Option<PathBuf>,
    /// Compile every script, not only those edited, modded or including an edited library.
    #[arg(long)]
    full: bool,
    #[command(flatten)]
    common: Common,
}

#[derive(Subcommand)]
pub enum ModCommand {
    /// Copy tree files into a mod, to edit them there.
    New {
        /// The decompiled source tree.
        source: PathBuf,
        /// The mod's name, a folder in the mods folder.
        name: String,
        /// Tree files to copy into the mod, relative to the tree.
        #[arg(required = true)]
        files: Vec<String>,
        #[arg(long, default_value = "mods")]
        mods: PathBuf,
        #[command(flatten)]
        common: Common,
    },
    /// Copy every file edited or added in the tree into a mod.
    Save {
        /// The edited source tree.
        source: PathBuf,
        /// The mod's name, a folder in the mods folder.
        name: String,
        #[arg(long, default_value = "mods")]
        mods: PathBuf,
        #[command(flatten)]
        common: Common,
    },
    /// Turn a mod's whole files (copies of the tree's scripts and libraries) into patches of the
    /// functions, globals and programs they change or add, where the mod then builds to the same
    /// bytes, and remove the copies.
    Slim {
        /// The decompiled source tree.
        source: PathBuf,
        /// The mod's name.
        name: String,
        #[arg(long, default_value = "mods")]
        mods: PathBuf,
        #[command(flatten)]
        common: Common,
    },
    /// List the mods, their files, and mods that change the same file.
    List {
        /// The decompiled source tree.
        source: PathBuf,
        #[arg(long, default_value = "mods")]
        mods: PathBuf,
    },
    /// Run the test_ functions of a mod's tests.c without the game.
    Test {
        /// The decompiled source tree.
        source: PathBuf,
        /// The mod's name.
        name: String,
        #[arg(long, default_value = "mods")]
        mods: PathBuf,
        #[command(flatten)]
        common: Common,
    },
    /// Write mods' files into a source tree.
    Apply {
        /// The source tree to apply them to.
        source: PathBuf,
        /// Mods to apply, in order (with the mods they require, ordered by requires and after).
        #[arg(required = true)]
        names: Vec<String>,
        /// Replace files edited in the tree too.
        #[arg(long)]
        force: bool,
        #[arg(long, default_value = "mods")]
        mods: PathBuf,
    },
    /// Start a patch: copy functions of a script into the mod, marked to replace the script's.
    Override {
        /// The decompiled source tree.
        source: PathBuf,
        /// The mod's name.
        name: String,
        /// Functions to override, as the script names them, or natives to wrap (place_monster).
        #[arg(required = true)]
        functions: Vec<String>,
        /// Patch every language of a map's script (s51).
        #[arg(long)]
        map: Option<String>,
        /// Patch scripts by path without readable names (dungeon/monster/e33a.c).
        #[arg(long)]
        script: Option<String>,
        /// Patch a monster kind's AI script (golem).
        #[arg(long)]
        monster: Option<String>,
        /// Patch every script that includes a library (lib/...).
        #[arg(long)]
        library: Option<String>,
        #[arg(long, default_value = "mods")]
        mods: PathBuf,
        #[command(flatten)]
        common: Common,
    },
    /// Write the editor headers of a mod's patches (the declarations of the scripts they target)
    /// and the mod's compile_commands.json, so clangd knows the targets' functions and globals.
    Header {
        /// The decompiled source tree.
        source: PathBuf,
        /// The mod's name.
        name: String,
        #[arg(long, default_value = "mods")]
        mods: PathBuf,
        #[command(flatten)]
        common: Common,
    },
    /// Copy game assets off the disc (models, textures, sounds, text, data tables) into a mod, to
    /// replace them there.
    Asset {
        /// The disc image.
        iso: PathBuf,
        /// The mod's name.
        name: String,
        /// Asset ids (texture:c02a02, model:c02fuku*, sound:SE_CURSOR_MOVE, bank:SP_003,
        /// text:en/shopname, music:BG_000_000), each copied in its common format with its line
        /// added to mod.json; data tables (data:items, data:shops, data:monsters, data:inventions,
        /// data:georama, data:medals, data:floors, data:fishing, data:characters, data:parts,
        /// data:layouts, data:chests, data:requests, data:townspeople, data:gifts, data:spheda,
        /// data:breeding) as JSON; or files as extract lays them out (snd2/sp/sp_000.snd,
        /// chara/c01_base.chr.d/c01b.img), copied into disc/.
        #[arg(required = true)]
        paths: Vec<String>,
        /// Export files in a common format to edit there: png for textures, wav for sound banks,
        /// gltf for character packs, txt for message files, mid for a sound bank's sequences (its
        /// music, with its instruments as a SoundFont).
        #[arg(long = "as")]
        form: Option<String>,
        #[arg(long, default_value = "mods")]
        mods: PathBuf,
        /// Definition set naming the sound entries; the shipped PAL set when omitted.
        #[arg(long)]
        defs: Vec<PathBuf>,
    },
    /// Compile what mods change, without a disc, and show their numbers and the assets they
    /// replace.
    Check {
        /// The decompiled source tree.
        source: PathBuf,
        /// Mods to check together, in build order.
        #[arg(required = true)]
        names: Vec<String>,
        #[arg(long, default_value = "mods")]
        mods: PathBuf,
        /// The disc image (or extracted disc), to check every asset id resolves and no two mods
        /// change one place.
        #[arg(long)]
        disc: Option<PathBuf>,
        /// Write the new names of overridden functions the tree has renamed (found by their
        /// fingerprints) into the mods' files.
        #[arg(long)]
        rename: bool,
        #[command(flatten)]
        common: Common,
    },
    /// Turn a mod's disc/ folder into mod.json lines by asset id, moving its files out of disc/.
    /// Make a mod of everything changed in the extracted folder since extracting: edited and added
    /// scripts, changed assets (by id) and changed or added disc files. c2 diff lists them first.
    Make {
        /// The disc image the extracted folder came from.
        iso: PathBuf,
        /// The mod's name, a new folder in the mods folder.
        name: String,
        /// The extracted folder (disc/, scripts/, assets/).
        #[arg(long, default_value = "extracted")]
        extracted: PathBuf,
        #[arg(long, default_value = "mods")]
        mods: PathBuf,
        #[command(flatten)]
        common: Common,
    },
    Migrate {
        /// The disc image the mod's files replace.
        iso: PathBuf,
        /// The mod's name.
        name: String,
        #[arg(long, default_value = "mods")]
        mods: PathBuf,
        /// Definition set naming the sound entries; the shipped PAL set when omitted.
        #[arg(long)]
        defs: Vec<PathBuf>,
    },
}

/// The constants mods' additions see: the shipped definitions' and the default SDK's, with the
/// tree's own headers (as mods.py's tree_constants).
fn default_constants(tree_dir: &Path) -> Result<IndexMap<String, i64>> {
    let defs = definitions::load(&[files::default_defs()])?;
    Ok(index::constants(index::tree_headers(tree_dir, &defs, &files::root().join("sdk"))?.values()))
}

/// The chosen mods with the mods they require, in the order they are applied ("requires" and
/// "after" in their mod.json); says which were added and the order when it is not the one given.
fn mod_folders(mods_dir: &Path, names: &[String]) -> Result<Vec<PathBuf>> {
    let found = mods::chosen(mods_dir, names)?;
    for (added, by) in &found.added {
        println!("mod {by} requires {added}, so it is built in too");
    }
    let order: Vec<String> =
        found.dirs.iter().map(|dir| dir.file_name().unwrap_or_default().to_string_lossy().into_owned()).collect();
    if order != names {
        println!("mods applied in this order: {}", order.join(", "));
    }
    Ok(found.dirs)
}

fn shown(path: &Path) -> String {
    path.display().to_string()
}

/// The headers of a patch mod's targets and its compile_commands.json (editor), for clangd.
/// Returns the headers.
fn write_editor_files(mod_dir: &Path, tree_dir: &Path, common: &Common) -> Result<Vec<PathBuf>> {
    let defs = common.definitions()?;
    let sdk_dir = common.sdk_dir();
    let headers = sdk::header_texts(&defs, &sdk_dir)?;
    let tables = common.tables()?;
    let tools = Toolchain { headers: &headers, rules: &defs.tables, tables: &tables, sdk_dir: &sdk_dir };
    Ok(editor::write(mod_dir, tree_dir, &tools, &sdk_dir, &defs.sets)?)
}

/// The fingerprints of the functions a mod's patches override, kept in its renames::LOCK so build
/// finds them when a later decompile renames them.
fn record_fingerprints(mod_dir: &Path, tree_dir: &Path, common: &Common) -> Result<()> {
    let defs = common.definitions()?;
    let sdk_dir = common.sdk_dir();
    let headers = sdk::header_texts(&defs, &sdk_dir)?;
    let tables = common.tables()?;
    let tools = Toolchain { headers: &headers, rules: &defs.tables, tables: &tables, sdk_dir: &sdk_dir };
    let dirs = [mod_dir.to_path_buf()];
    let mut plan = mods::patch_plan(&dirs, tree_dir, &default_constants(tree_dir)?, false)?;
    renames::follow(&dirs, &mut plan, tree_dir, &tools, &IndexMap::new())?;
    Ok(())
}

/// The places and changes of the chosen mods' asset ids, and the templates of the game files they
/// make from common formats, read from the disc's index of ids, which is only made when a mod
/// names an id.
fn resolved(mod_dirs: &[PathBuf], disc: &Path, defs: &[PathBuf]) -> Result<(replace::Resolved, discfiles::Templates)> {
    let lines = replace::all_lines(mod_dirs)?;
    if !replace::needs_index(&lines) {
        return Ok((replace::Resolved::default(), replace::templates(&lines, None)?));
    }
    let index = asset_tree::index_of(disc, defs)?;
    Ok((replace::resolve(&lines, &index)?, replace::templates(&lines, Some(&index))?))
}

/// The sound entries the mods add and name (replace::sound_constants), as constants their patches
/// see.
fn with_added(plan: &mut patches::Plan, constants: &BTreeMap<String, IndexMap<String, i64>>) {
    for (module, added) in constants {
        plan.numbers
            .entry(module.clone())
            .or_default()
            .extend(added.iter().map(|(name, number)| (name.clone(), *number)));
        for patch in plan.patches.iter_mut().filter(|patch| patch.module == *module) {
            patch.numbers.extend(added.iter().map(|(name, number)| (name.clone(), *number)));
        }
    }
}

/// The files the mods' scripts load by name (loads.rs) that the disc, with the files the mods
/// give, does not have, or the pack members it lacks: one line each. A load the tree's own script
/// makes too (the game has a few that fail) is left out.
fn missing_loads(
    mod_dirs: &[PathBuf],
    tree_dir: &Path,
    consts: &IndexMap<String, i64>,
    iso: &Path,
    raw: &discfiles::Wanted,
) -> Result<Vec<String>> {
    let image = c2_disc::Image::open(iso)?;
    let archive = c2_disc::archive::Archive::open(image.bytes())?;
    let by_path: std::collections::HashMap<String, &c2_disc::archive::Entry> =
        archive.entries.iter().map(|entry| (entry.path.to_lowercase(), entry)).collect();
    let known =
        loads::Known::new(archive.entries.iter().map(|entry| entry.path.as_str()), raw.keys().map(String::as_str));
    let members = |path: &str| -> Option<Vec<String>> {
        let entry = by_path.get(&path.to_lowercase())?;
        let pack = c2_disc::archive::parse_pack(archive.read(image.bytes(), entry))?;
        Some(pack.into_iter().map(|member| member.name).collect())
    };
    // (mod, file shown, text, the tree's scripts it stands for)
    let mut texts: Vec<(String, String, String, Vec<String>)> = Vec::new();
    let plan = mods::patch_plan(mod_dirs, tree_dir, consts, false)?;
    for patch in &plan.patches {
        texts.push((patch.module.clone(), patch.file_name(), patch.text.clone(), patch.scripts.clone()));
    }
    for mod_dir in mod_dirs {
        let module = mod_dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
        for (path, text) in mods::contents(mod_dir, tree_dir)? {
            texts.push((module.clone(), path.clone(), text, vec![path]));
        }
    }
    let mut out = Vec::new();
    for (module, shown, text, scripts) in texts {
        let originals: Vec<String> =
            scripts.iter().filter_map(|script| c2_py::read_text(&tree_dir.join(script)).ok()).collect();
        for (load, problem) in loads::check(&loads::loads(&text, consts), &known, &members) {
            let literal = format!("\"{}\"", load.name);
            if originals.iter().any(|original| original.contains(&literal)) {
                continue;
            }
            out.push(format!("{module}: {shown}:{} {}(\"{}\"): {problem}", load.line, load.call, load.name));
        }
    }
    Ok(out)
}

/// What a mod's id lines reach, one line each.
fn reached_lines(resolved: &replace::Resolved, module: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (owner, key, ids) in &resolved.reached {
        if owner != module {
            continue;
        }
        let places = |id: &String| {
            let mut places: Vec<String> = Vec::new();
            for edit in resolved.edits.iter().filter(|edit| edit.module == module && edit.id == *id) {
                if !places.contains(&edit.place.display()) {
                    places.push(edit.place.display());
                }
            }
            places.len()
        };
        if let Some(number) =
            resolved.added.get(module).and_then(|added| added.get(key.split_once(':').map_or("", |(_, name)| name)))
        {
            out.push(format!("adds {key} to {} as entry {number}", ids.join(", ")));
            continue;
        }
        for id in ids {
            let count = places(id);
            let via = if id.eq_ignore_ascii_case(key) { String::new() } else { format!(" (by {key})") };
            out.push(format!("replaces {id}{via} at {count} place{}", if count == 1 { "" } else { "s" }));
        }
    }
    out
}

/// The music files a mod adds, one line each, with how its scripts play them.
fn new_music_lines(lines: &[replace::Line], module: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in lines.iter().filter(|line| line.module == module && line.is_new_music()) {
        let Some(number) = line.bgm_number() else { continue };
        out.push(format!(
            "adds {} ({}, like {}): scripts play it with load_bgm({number}); play_bgm(0);",
            line.key,
            c2_assets::music::bgm_path(number),
            line.like.as_deref().unwrap_or_default()
        ));
    }
    out
}

/// Prints what the loads of the files the build rewrites use of the game's fixed memory
/// (c2_script::memory, docs/assets/memory.md); true when a budget that is certain is exceeded.
/// The image is mapped only here, so writing the new image can still clone the original.
fn memory_check(iso: &Path, files: &discfiles::Files) -> Result<bool> {
    let image = c2_disc::Image::open(iso)?;
    let archive = c2_disc::archive::Archive::open(image.bytes())?;
    let original = memory::Image::new(image.bytes(), &archive);
    let modded = memory::Overlay::new(&original, files);
    let changed: Vec<String> = files.keys().cloned().collect();
    let findings = memory::check(&original, &modded, &changed);
    for item in &findings {
        let label = match item.level {
            memory::Level::Error => "error",
            memory::Level::Warning => "warning",
            memory::Level::Note => "memory",
        };
        println!("{label}: {}", item.text);
    }
    Ok(findings.iter().any(|item| item.level == memory::Level::Error))
}

pub fn run_build(args: &BuildArgs) -> Result<ExitCode> {
    let common = &args.common;
    let defs = common.definitions()?;
    let sdk_dir = common.sdk_dir();
    let tables = common.tables()?;
    let mod_dirs = mod_folders(&args.mods, &args.mod_names)?;
    let (mut overrides, clashes) = mods::overlay(&mod_dirs, &args.source, &default_constants(&args.source)?)?;
    if !clashes.is_empty() {
        bail!(mods::clash_text(&clashes));
    }
    let (disc_files, disc_clashes) = discfiles::collect(&mod_dirs)?;
    if !disc_clashes.is_empty() {
        bail!(discfiles::clash_text(&disc_clashes));
    }
    let (resolved, templates) = resolved(&mod_dirs, &args.iso, &common.def_sets())?;
    let id_clashes = replace::clashes(&resolved, &disc_files);
    if !id_clashes.is_empty() {
        bail!("the chosen mods cannot be built together:\n  {}", id_clashes.join("\n  "));
    }
    let consts = index::constants(index::tree_headers(&args.source, &defs, &sdk_dir)?.values());
    let mut plan = mods::patch_plan(&mod_dirs, &args.source, &consts, true)?;
    with_added(&mut plan, &replace::sound_constants(&mod_dirs, &consts, &resolved.added)?);
    let language = c2_script::disc::language_code(&defs);
    // the chosen mods' new maps, numbered after the disc's; every mod's scripts see MAP_<NAME>
    let all_lines = replace::all_lines(&mod_dirs)?;
    let new_maps = c2_script::maptable::new_maps(&all_lines)?;
    if !new_maps.is_empty() {
        let numbers = c2_script::maptable::disc_numbers_with(&args.iso, language, &new_maps)?;
        let constants = c2_script::mapscript::map_constants(&new_maps, &numbers);
        let by_mod =
            mod_dirs.iter().map(|dir| (dir.file_name().unwrap().to_string_lossy().into_owned(), constants.clone()));
        with_added(&mut plan, &by_mod.collect());
    }
    for mod_dir in &mod_dirs {
        let name = mod_dir.file_name().unwrap().to_string_lossy().into_owned();
        let mine: Vec<&patches::Patch> = plan.patches.iter().filter(|patch| patch.module == name).collect();
        println!(
            "mod {}: {} patches, {} files, {} game files",
            name,
            mine.len(),
            mods::contents(mod_dir, &args.source)?.len(),
            discfiles::game_files(mod_dir)?.len()
        );
        for patch in mine {
            println!("  patch {}", patches::summary(patch));
        }
        for line in reached_lines(&resolved, &name) {
            println!("  {line}");
        }
        for line in new_music_lines(&all_lines, &name) {
            println!("  {line}");
        }
        let given: BTreeMap<&String, &i64> =
            plan.numbers.get(&name).map(|found| found.iter().collect()).unwrap_or_default();
        for (constant, number) in given {
            println!("  {constant} = {number}");
        }
        for path in mods::stale(mod_dir, &args.source)? {
            println!("warning: {path} changed in the decompiler's output since {name} was made; compare them");
        }
    }
    let mut start_at = args.start_at.clone();
    if start_at.is_none() && !args.no_start_at {
        let mut wanted: Vec<String> = Vec::new();
        for mod_dir in &mod_dirs {
            if let Some(scene) = mods::entries(mod_dir)?["start_at"].as_str().filter(|scene| !scene.is_empty()) {
                if !wanted.iter().any(|known| known == scene) {
                    wanted.push(scene.to_string());
                }
            }
        }
        wanted.sort();
        if wanted.len() > 1 {
            bail!(
                "the chosen mods start at different scenes ({}); pick one with --start-at, or --no-start-at",
                wanted.join(", ")
            );
        }
        if let Some(scene) = wanted.pop() {
            println!("start at {scene}, from the chosen mod (--no-start-at for a normal New Game)");
            start_at = Some(scene);
        }
    }
    if let Some(start_at) = start_at.filter(|start_at| !start_at.is_empty()) {
        let mut target = startat::parse_start(&start_at)?;
        let given: IndexMap<&String, &i64> = plan.numbers.values().flat_map(|found| found.iter()).collect();
        if let Some(number) = given.get(&target.program) {
            // a mod's NEW_PROGRAM name
            target.program = number.to_string();
        }
        // the chosen mods' new maps are numbered after the disc's
        let warp = startat::warp(&args.source, &consts, &target, || {
            c2_script::maptable::disc_numbers_with(&args.iso, language, &new_maps)
        })?;
        let new_map = new_maps.iter().any(|map| map.name == target.map);
        for warning in startat::warp_warnings(&args.source, &consts, &target, warp)? {
            // a mod's new map is entered by no script of the game's
            if !(new_map && warning.contains("go_to_edit_mode")) {
                println!("warning: {warning}");
            }
        }
        let patched = startat::warp_overrides(&args.source, &target, warp, &overrides)?;
        let mut names: Vec<&String> = patched.keys().collect();
        names.sort();
        let names = names.iter().map(|name| name.as_str()).collect::<Vec<_>>().join(", ");
        overrides.extend(patched);
        let how = match warp {
            startat::Warp::Dungeon => format!(
                "loaded dungeon-style with load_dungeon_map and load_event_script {}/{}.stb",
                target.map, target.script
            ),
            startat::Warp::Town(number) => {
                format!("entered as a town with go_to_edit_mode({number}, {})", target.program)
            }
        };
        println!(
            "start at: a new game goes to map {} at program {}, {} (patched {}; flag {} marks it done)",
            target.map,
            target.program,
            how,
            names,
            startat::START_FLAG
        );
    }
    if let Some(status) = tree::status(&args.source)? {
        if !mod_dirs.is_empty() && (!status.edited.is_empty() || !status.added.is_empty()) {
            println!(
                "note: {} itself has {} edited or added files, which are built in under the mods",
                shown(&args.source),
                status.edited.len() + status.added.len()
            );
        }
        for note in tree::library_notes(&args.source, &status)? {
            println!("note: {note}");
        }
    }
    let headers = sdk::header_texts(&defs, &sdk_dir)?;
    let tools = Toolchain { headers: &headers, rules: &defs.tables, tables: &tables, sdk_dir: &sdk_dir };
    let following = renames::follow(&mod_dirs, &mut plan, &args.source, &tools, &overrides)?;
    for followed in &following.followed {
        println!("note: {}; c2 mod check --rename writes the new name into the mod", renames::describe(followed));
    }
    for note in &following.notes {
        println!("note: {note}");
    }
    let found = build::changes(&args.source, &args.iso, &tools, &overrides, args.full, Some(&plan))?;
    match &found.all_because {
        Some(why) => println!("compiled all {} scripts: {}", found.compiled, why),
        None => println!("compiled {} scripts; the rest are unchanged since decompiling", found.compiled),
    }
    for path in &found.skipped {
        println!("not on the disc, skipped: {path}");
    }
    for change in &found.scripts {
        println!("changed: {}", change.source);
    }
    if found.scripts.is_empty() {
        println!("no script differs from the disc");
    }
    let tops: HashSet<String> = found.scripts.iter().map(|change| change.top.clone()).collect();
    let (files, replaced) = discfiles::replaced(&args.iso, &disc_files, &found.files, &tops, &templates)?;
    for item in &replaced {
        let Some(old) = item.old else {
            println!(
                "added: {} (mod {}, {} bytes{})",
                item.path,
                item.module,
                item.data.len(),
                if item.new_file { ", a new file" } else { "" }
            );
            continue;
        };
        println!("replaced: {} (mod {}, {} bytes, was {})", item.path, item.module, item.data.len(), old);
        for detail in &item.details {
            println!("  {detail}");
        }
    }
    // the letters the English font gains, for every changed English text: the text: lines' and
    // the items' names and descriptions
    let item_letters = crate::data_cmd::english_item_letters(&args.iso, &mod_dirs)?;
    let english = replace::english_letters(&args.iso, &resolved, item_letters)?;
    let (files, changed) = replace::apply_with(&args.iso, &resolved, &files, &english)?;
    for item in &changed {
        println!(
            "replaced: {} ({}; mod {}, {} bytes, was {})",
            item.place.display(),
            item.ids.join(", "),
            item.modules.join(", "),
            item.data.len(),
            item.old
        );
        for detail in &item.details {
            println!("  {detail}");
        }
        for warning in &item.warnings {
            println!("{warning}");
        }
    }
    let (files, new_music) = replace::add_music(&args.iso, &all_lines, &files)?;
    for item in &new_music {
        println!(
            "added: {} ({}; mod {}, {} bytes, a new file)",
            item.place.display(),
            item.ids.join(", "),
            item.modules.join(", "),
            item.data.len()
        );
        for warning in &item.warnings {
            println!("{warning}");
        }
    }
    let program_of = |line: &replace::Line, value: &serde_json::Value| -> Result<i64> {
        Ok(c2_script::mapscript::program_number(value, &line.module, &plan.numbers, &consts)?)
    };
    let (mut files, map_changes) = crate::map_cmd::apply_maps(&args.iso, &mod_dirs, &files, &program_of)?;
    for built in c2_script::mapscript::build_all(&all_lines, &mod_dirs, &args.source, &tools, &plan.numbers)? {
        for (path, data) in built.files {
            println!(
                "added: {path} (map:{}'s script; mod {}, {} bytes, a new file)",
                built.map,
                built.module,
                data.len()
            );
            files.insert(path, data);
        }
    }
    for item in &map_changes {
        if item.old == 0 {
            println!("added: {} (map:{}; mod {}, {} bytes)", item.path, item.map, item.module, item.new);
            continue;
        }
        println!(
            "replaced: {} (map:{}; mod {}, {} bytes, was {})",
            item.path, item.map, item.module, item.new, item.old
        );
    }
    let (files, executable, data_changes) = crate::data_cmd::apply_data(&args.iso, &mod_dirs, &files, &english.slots)?;
    for change in &data_changes {
        for line in crate::data_cmd::described(change) {
            println!("  {line}");
        }
        for (path, new, old) in &change.files {
            println!("replaced: {path} (data:{}; {new} bytes, was {old})", change.table);
        }
    }
    let over_budget = memory_check(&args.iso, &files)?;
    let written = rebuild::write_disc(&args.iso, &args.output, &files)?;
    if let Some(executable) = &executable {
        // the executable tables (monsters, spectrumising, ...): the boot executable patched in
        // place, its size kept
        rebuild::overwrite_iso_file(&args.output, c2_assets::monster_data::EXECUTABLE, executable)?;
    }
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
    let mut wrong = build::check(&args.output, &found.scripts)?;
    wrong.extend(discfiles::check(&args.output, &replaced)?);
    wrong.extend(replace::check(&args.output, &changed)?);
    wrong.extend(replace::check(&args.output, &new_music)?);
    wrong.extend(crate::data_cmd::check(&args.output, &data_changes, &files, executable.as_deref())?);
    for path in &wrong {
        println!("reads back wrong: {path}");
    }
    println!(
        "{} scripts changed, {} game files replaced or added, {} disc files rewritten, written to {}",
        found.scripts.len(),
        replaced.len()
            + changed.len()
            + new_music.len()
            + map_changes.len()
            + data_changes.iter().map(|change| change.files.len()).sum::<usize>(),
        files.len(),
        shown(&args.output)
    );
    if !wrong.is_empty() {
        return Ok(ExitCode::from(1));
    }
    if over_budget {
        println!("the image was written, but the game will not load it as built: see the errors above (docs/assets/memory.md)");
        return Ok(ExitCode::from(1));
    }
    if args.play {
        println!("started PCSX2: {}", launch(&args.output, args.pcsx2.as_deref(), true)?.display());
    }
    Ok(ExitCode::SUCCESS)
}

pub fn run_mod(command: &ModCommand) -> Result<ExitCode> {
    match command {
        ModCommand::Test { source, name, mods: mods_dir, common } => {
            let defs = common.definitions()?;
            let sdk_dir = common.sdk_dir();
            let headers = sdk::header_texts(&defs, &sdk_dir)?;
            let tables = common.tables()?;
            let tools = Toolchain { headers: &headers, rules: &defs.tables, tables: &tables, sdk_dir: &sdk_dir };
            let results = mods::run_tests(&mods::folder(mods_dir, name)?, source, &tools, &default_constants(source)?)?;
            for result in &results {
                println!("{:<5} {}", if result.passed { "ok" } else { "FAIL" }, result.name);
                if !result.passed {
                    for line in &result.printed {
                        println!("      printed: {line}");
                    }
                    if let Some(error) = &result.error {
                        println!("      error: {error}");
                    }
                }
            }
            let failed = results.iter().filter(|result| !result.passed).count();
            println!("{} passed, {} failed", results.len() - failed, failed);
            Ok(if failed > 0 { ExitCode::from(1) } else { ExitCode::SUCCESS })
        }
        ModCommand::List { source, mods: mods_dir } => {
            println!("{}", mods::listing(mods_dir, source, &default_constants(source)?)?);
            Ok(ExitCode::SUCCESS)
        }
        ModCommand::Apply { source, names, force, mods: mods_dir } => {
            let (applied, warnings) =
                mods::apply(&mod_folders(mods_dir, names)?, source, *force, &default_constants(source)?)?;
            for warning in warnings {
                println!("warning: {warning}");
            }
            println!("{} files applied to {}", applied.len(), shown(source));
            Ok(ExitCode::SUCCESS)
        }
        ModCommand::Check { source, names, mods: mods_dir, disc, rename, common } => {
            let defs = common.definitions()?;
            let sdk_dir = common.sdk_dir();
            let headers = sdk::header_texts(&defs, &sdk_dir)?;
            let tables = common.tables()?;
            let tools = Toolchain { headers: &headers, rules: &defs.tables, tables: &tables, sdk_dir: &sdk_dir };
            let mod_dirs = mod_folders(mods_dir, names)?;
            for mod_dir in &mod_dirs {
                let name = mod_dir.file_name().unwrap().to_string_lossy().into_owned();
                for (key, word) in [("requires", "requires"), ("after", "is built after")] {
                    let named = mods::named(mod_dir, key)?;
                    if !named.is_empty() {
                        println!("{name}: {word} {}", named.join(", "));
                    }
                }
            }
            let (raw, raw_clashes) = discfiles::collect(&mod_dirs)?;
            if !raw_clashes.is_empty() {
                bail!(discfiles::clash_text(&raw_clashes));
            }
            let lines = replace::all_lines(&mod_dirs)?;
            let added = match disc {
                Some(disc) => {
                    let (resolved, _) = resolved(&mod_dirs, disc, &common.def_sets())?;
                    let clashes = replace::clashes(&resolved, &raw);
                    if !clashes.is_empty() {
                        bail!("the chosen mods cannot be built together:\n  {}", clashes.join("\n  "));
                    }
                    // the music files they add, made as build makes them (the number free, the
                    // like bank's tune replaced)
                    let given = raw.keys().map(|path| (path.clone(), Vec::new())).collect();
                    let (_, new_music) = replace::add_music(disc, &lines, &given)?;
                    for mod_dir in &mod_dirs {
                        let name = mod_dir.file_name().unwrap().to_string_lossy().into_owned();
                        for line in reached_lines(&resolved, &name) {
                            println!("{name}: {line}");
                        }
                        for line in new_music_lines(&lines, &name) {
                            println!("{name}: {line}");
                        }
                        for item in new_music.iter().filter(|item| item.modules.contains(&name)) {
                            for note in &item.warnings {
                                println!("{name}: {}: {note}", item.place.display());
                            }
                        }
                    }
                    let item_letters = crate::data_cmd::english_item_letters(disc, &mod_dirs)?;
                    let english = replace::english_letters(disc, &resolved, item_letters)?;
                    let (_, _, data_changes) =
                        crate::data_cmd::apply_data(disc, &mod_dirs, &Default::default(), &english.slots)?;
                    for change in &data_changes {
                        for line in crate::data_cmd::described(change) {
                            println!("{line}");
                        }
                    }
                    resolved.added
                }
                None => {
                    let clashes = replace::key_clashes(&mod_dirs)?;
                    if !clashes.is_empty() {
                        let shown: Vec<String> =
                            clashes.iter().map(|(id, names)| format!("{id} ({})", names.join(", "))).collect();
                        bail!(
                            "these assets are replaced by more than one of the chosen mods:\n  {}",
                            shown.join("\n  ")
                        );
                    }
                    for line in &lines {
                        println!(
                            "{}: {} {}: {}",
                            line.module,
                            if line.add { "adds" } else { "replaces" },
                            line.key,
                            line.shown
                        );
                    }
                    if replace::needs_index(&lines) {
                        println!("asset ids not looked up: pass --disc with the disc image to check they resolve");
                    }
                    // an added entry's number needs the disc; its name compiles all the same
                    let mut added: BTreeMap<String, IndexMap<String, i64>> = BTreeMap::new();
                    for line in lines
                        .iter()
                        .filter(|line| line.add && !line.is_file() && !line.is_map() && !line.is_new_music())
                    {
                        added.entry(line.module.clone()).or_default().insert(line.key["sound:".len()..].to_string(), 0);
                    }
                    added
                }
            };
            let consts = default_constants(source)?;
            let numbered = disc.is_some();
            let without: BTreeMap<String, IndexMap<String, i64>> = BTreeMap::new();
            let named = replace::sound_constants(&mod_dirs, &consts, if numbered { &added } else { &without })?;
            for (name, found) in &named {
                for (constant, number) in found {
                    println!("{name}: sound {constant} = {number}");
                }
            }
            // without the disc an added entry's number is not known; its name compiles all the same
            let mut added = replace::sound_constants(&mod_dirs, &consts, &added)?;
            // the new maps' numbers: from the disc's map table, or after the PAL disc's 189 rows
            let new_maps = c2_script::maptable::new_maps(&lines)?;
            if !new_maps.is_empty() {
                let numbers = match disc.as_ref().filter(|disc| disc.is_file()) {
                    Some(disc) => {
                        c2_script::maptable::disc_numbers_with(disc, c2_script::disc::language_code(&defs), &new_maps)?
                    }
                    None => c2_script::maptable::assumed_numbers(&new_maps),
                };
                let constants = c2_script::mapscript::map_constants(&new_maps, &numbers);
                for (constant, number) in &constants {
                    println!("new map {constant} = {number}");
                }
                for mod_dir in &mod_dirs {
                    let name = mod_dir.file_name().unwrap().to_string_lossy().into_owned();
                    added
                        .entry(name)
                        .or_default()
                        .extend(constants.iter().map(|(name, number)| (name.clone(), *number)));
                }
            }
            let mut result = mods::check_with(&mod_dirs, source, &tools, &consts, &added)?;
            // the new maps' own scripts, with the numbers their mods' patches see
            let mut numbers = result.numbers.clone();
            for (name, found) in &added {
                numbers
                    .entry(name.clone())
                    .or_default()
                    .extend(found.iter().map(|(name, number)| (name.clone(), *number)));
            }
            for script in c2_script::mapscript::scripts(&lines) {
                match c2_script::mapscript::build_one(&script, &mod_dirs, source, &tools, &numbers) {
                    Ok(files) => {
                        result.compiled += 1;
                        let paths: Vec<&str> = files.iter().map(|(path, _)| path.as_str()).collect();
                        println!("{}: map {}'s script compiles to {}", script.module, script.map, paths.join(", "));
                    }
                    Err(error) => result.errors.push((script.source.display().to_string(), error.to_string())),
                }
            }
            for (name, found) in &result.numbers {
                let sorted: BTreeMap<&String, &i64> = found.iter().collect();
                for (constant, number) in sorted {
                    println!("{name}: {constant} = {number}");
                }
            }
            for followed in &result.following.followed {
                println!("{}", renames::describe(followed));
            }
            for note in &result.following.notes {
                println!("note: {note}");
            }
            if *rename {
                for mod_dir in &mod_dirs {
                    for path in renames::rename(mod_dir, &result.following.followed)? {
                        println!("renamed in {}", shown(&path));
                    }
                }
            } else if !result.following.followed.is_empty() {
                println!("--rename writes the new names into the mods");
            }
            for (path, error) in &result.errors {
                println!("error: {path}: {error}");
            }
            if let Some(hint) = crate::files::rename_hint(
                &common.def_sets(),
                result.errors.iter().map(|(_, error)| error.as_str()),
                source,
            ) {
                println!("{hint}");
            }
            match disc {
                Some(disc) => {
                    for line in missing_loads(&mod_dirs, source, &consts, disc, &raw)? {
                        println!("warning: {line}");
                    }
                }
                None => println!("files the scripts load not looked up: pass --disc with the disc image to check them"),
            }
            println!("{} scripts compiled, {} failed", result.compiled, result.errors.len());
            Ok(if result.errors.is_empty() { ExitCode::SUCCESS } else { ExitCode::from(1) })
        }
        ModCommand::Make { iso, name, extracted, mods: mods_dir, common } => {
            let mod_dir = mods::folder(mods_dir, name)?;
            let found = c2_script::capture::changes(extracted, iso)?;
            println!("{}", c2_script::capture::report(&found));
            if found.is_empty() {
                return Ok(ExitCode::from(1));
            }
            let made = c2_script::capture::make(extracted, &mod_dir, &found)?;
            if !made.scripts.is_empty() {
                let tree_dir = extracted.join(c2_script::capture::SCRIPTS);
                mods::write_editor_flags(&mod_dir, &tree_dir, &common.sdk_dir(), &common.definitions()?.sets)?;
            }
            println!();
            println!(
                "mod {} made: {} script file{}, {} mod.json line{}, in {}",
                name,
                made.scripts.len(),
                if made.scripts.len() == 1 { "" } else { "s" },
                made.lines.len(),
                if made.lines.len() == 1 { "" } else { "s" },
                shown(&mod_dir)
            );
            println!(
                "try it: c2 build {} {} -o {}.iso --mod {}",
                shown(&extracted.join("scripts")),
                iso.display(),
                name,
                name
            );
            Ok(ExitCode::SUCCESS)
        }
        ModCommand::Migrate { iso, name, mods: mods_dir, defs } => {
            let mod_dir = mods::folder(mods_dir, name)?;
            let found = migrate::migrate(&mod_dir, iso, &asset_tree::index_of(iso, defs)?)?;
            for (key, value) in &found.lines {
                println!("{key}: {}", c2_py::json::dumps(value, &c2_py::json::Options::plain().unicode()));
            }
            for note in &found.notes {
                println!("note: {note}");
            }
            println!("{} lines written to {}", found.lines.len(), shown(&mod_dir.join(mods::INDEX)));
            Ok(ExitCode::SUCCESS)
        }
        ModCommand::Asset { iso, name, paths, form, mods: mods_dir, defs } => {
            let mod_dir = mods::folder(mods_dir, name)?;
            let (tables, paths): (Vec<String>, Vec<String>) =
                paths.iter().cloned().partition(|path| path.to_lowercase().starts_with(replace::DATA));
            for (id, file, written) in crate::data_cmd::copy_out(iso, &mod_dir, &tables)? {
                if written {
                    println!(
                        "{}: {id}; edit it there and keep only what you change, and build writes it",
                        shown(&file)
                    );
                } else {
                    println!("{}: {id} is in the mod already; left as it is", shown(&file));
                }
            }
            let (keys, paths): (Vec<String>, Vec<String>) =
                paths.iter().cloned().partition(|path| c2_assets::ids::kind_of(path).is_some());
            if !keys.is_empty() {
                for (id, file, written) in replace::copy_out(iso, &mod_dir, &keys, &asset_tree::index_of(iso, defs)?)? {
                    let place = file.split('/').fold(mod_dir.clone(), |folder, part| folder.join(part));
                    if written {
                        println!(
                            "{}: {id}; edit it there, and build puts it at every place the disc has it",
                            shown(&place)
                        );
                    } else {
                        println!("{}: {id} is in the mod already; left as it is", shown(&place));
                    }
                }
            }
            for path in discfiles::copy_out(iso, &mod_dir, &paths, form.as_deref())? {
                let place = path.split('/').fold(mod_dir.join(discfiles::DISC), |folder, part| folder.join(part));
                println!("{}: replace it there, and build puts it on the disc", shown(&place));
            }
            Ok(ExitCode::SUCCESS)
        }
        ModCommand::Override { source, name, functions, map, script, monster, library, mods: mods_dir, common } => {
            let mod_dir = mods::folder(mods_dir, name)?;
            let kinds: Vec<(&str, &String)> =
                [("map", map), ("script", script), ("monster", monster), ("library", library)]
                    .into_iter()
                    .filter_map(|(kind, value)| {
                        value.as_ref().filter(|value| !value.is_empty()).map(|value| (kind, value))
                    })
                    .collect();
            if kinds.len() != 1 {
                bail!("give one of --map, --script, --monster and --library: the scripts to patch");
            }
            let headers = sdk::header_texts(&common.definitions()?, &common.sdk_dir())?;
            let result = mods::override_functions(source, &mod_dir, kinds[0].0, kinds[0].1, functions, &headers)?;
            let added =
                if result.added.is_empty() { "already overrides them".to_string() } else { result.added.join(", ") };
            println!("{}: {}", shown(&result.path), added);
            println!(
                "patches {} script{}; edit the functions there, and call original() to run the script's own",
                result.scripts.len(),
                if result.scripts.len() == 1 { "" } else { "s" }
            );
            write_editor_files(&mod_dir, source, common)?;
            record_fingerprints(&mod_dir, source, common)?;
            Ok(ExitCode::SUCCESS)
        }
        ModCommand::Header { source, name, mods: mods_dir, common } => {
            let mod_dir = mods::folder(mods_dir, name)?;
            if !mods::exists(&mod_dir) {
                bail!("no mod at {}", shown(&mod_dir));
            }
            let written = write_editor_files(&mod_dir, source, common)?;
            for path in &written {
                println!("{}", shown(path));
            }
            println!(
                "{} header{} and {} written; editors with clangd read the patches with their targets' declarations",
                written.len(),
                if written.len() == 1 { "" } else { "s" },
                shown(&mod_dir.join(editor::COMMANDS))
            );
            Ok(ExitCode::SUCCESS)
        }
        ModCommand::New { source, name, files: paths, mods: mods_dir, common } => {
            let mod_dir = mods::folder(mods_dir, name)?;
            let copied = mods::new(source, &mod_dir, paths)?;
            println!("{} files copied into {}; edit them there", copied.len(), shown(&mod_dir.join(mods::FILES)));
            mods::write_editor_flags(&mod_dir, source, &common.sdk_dir(), &common.definitions()?.sets)?;
            Ok(ExitCode::SUCCESS)
        }
        ModCommand::Slim { source, name, mods: mods_dir, common } => {
            let mod_dir = mods::folder(mods_dir, name)?;
            if !mods::exists(&mod_dir) {
                bail!("no mod at {}", shown(&mod_dir));
            }
            let defs = common.definitions()?;
            let sdk_dir = common.sdk_dir();
            let headers = sdk::header_texts(&defs, &sdk_dir)?;
            let tables = common.tables()?;
            let tools = Toolchain { headers: &headers, rules: &defs.tables, tables: &tables, sdk_dir: &sdk_dir };
            let outcome = slim::slim(mods_dir, name, source, &tools, &default_constants(source)?)?;
            for slimmed in &outcome.slimmed {
                println!("{}", slim::describe(slimmed, &mod_dir));
            }
            for (file, reason) in &outcome.kept {
                println!("{file}: kept, since {reason}");
            }
            if outcome.slimmed.iter().any(|slimmed| slimmed.patch.is_some()) {
                write_editor_files(&mod_dir, source, common)?;
                record_fingerprints(&mod_dir, source, common)?;
            }
            let patched = outcome.slimmed.iter().filter(|slimmed| slimmed.patch.is_some()).count();
            println!(
                "{patched} made patches, {} the same as the tree's, {} kept; every script they reach builds to the \
                 same bytes as before",
                outcome.slimmed.len() - patched,
                outcome.kept.len()
            );
            Ok(ExitCode::SUCCESS)
        }
        ModCommand::Save { source, name, mods: mods_dir, common } => {
            let mod_dir = mods::folder(mods_dir, name)?;
            let (saved, removed) = mods::save(source, &mod_dir)?;
            println!("{} files saved to {}", saved.len(), shown(&mod_dir));
            for path in removed {
                println!("note: {path} was removed from the tree; a mod cannot remove files");
            }
            mods::write_editor_flags(&mod_dir, source, &common.sdk_dir(), &common.definitions()?.sets)?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

const NAMES: [&str; 3] = ["pcsx2-qt", "pcsx2-qtx64", "pcsx2"];
const INSTALLS: [&str; 5] = [
    "C:/Program Files/PCSX2/pcsx2-qt.exe",
    "C:/Program Files (x86)/PCSX2/pcsx2-qt.exe",
    "/Applications/PCSX2.app/Contents/MacOS/PCSX2",
    "/usr/bin/pcsx2-qt",
    "/usr/local/bin/pcsx2-qt",
];

/// PCSX2's executable: the one given, else the PCSX2 environment variable, else one on the PATH,
/// else a usual install folder.
pub fn find_pcsx2(given: Option<&Path>) -> Option<PathBuf> {
    let on_path = |name: &str| {
        let path = std::env::var_os("PATH").unwrap_or_default();
        std::env::split_paths(&path)
            .flat_map(|folder| ["", ".exe"].map(|suffix| folder.join(format!("{name}{suffix}"))))
            .find(|candidate| candidate.is_file())
    };
    find_pcsx2_with(given, std::env::var_os("PCSX2").map(PathBuf::from), on_path, Path::is_file)
}

/// find_pcsx2 with the environment variable, the PATH search and the file test given.
fn find_pcsx2_with(
    given: Option<&Path>,
    environment: Option<PathBuf>,
    which: impl Fn(&str) -> Option<PathBuf>,
    exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    for candidate in [given.map(Path::to_path_buf), environment].into_iter().flatten() {
        if !candidate.as_os_str().is_empty() {
            return exists(&candidate).then_some(candidate);
        }
    }
    NAMES
        .iter()
        .find_map(|name| which(name))
        .or_else(|| INSTALLS.iter().map(PathBuf::from).find(|candidate| exists(candidate)))
}

/// PCSX2's command line for an image, with the image's full path.
fn command(pcsx2: &Path, image: &Path, fast_boot: bool) -> Vec<std::ffi::OsString> {
    let full = std::fs::canonicalize(image).or_else(|_| std::path::absolute(image)).unwrap_or(image.to_path_buf());
    let mut out = vec![pcsx2.as_os_str().to_owned()];
    if fast_boot {
        out.push("-fastboot".into());
    }
    out.push("--".into());
    out.push(full.to_string_lossy().trim_start_matches(r"\\?\").into());
    out
}

/// Boots an image in PCSX2, skipping the PS2 boot screens unless fast_boot is false.
pub fn launch(image: &Path, pcsx2: Option<&Path>, fast_boot: bool) -> Result<PathBuf> {
    let Some(found) = find_pcsx2(pcsx2) else {
        bail!("PCSX2 not found; pass --pcsx2 PATH or set the PCSX2 environment variable");
    };
    if !image.is_file() {
        bail!("no image at {}", image.display());
    }
    let parts = command(&found, image, fast_boot);
    std::process::Command::new(&parts[0]).args(&parts[1..]).spawn()?;
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    // tests/test_play.py

    #[test]
    fn pcsx2_is_found_where_given_then_in_the_environment_path_and_install_folders() {
        let nowhere = |_: &str| None;
        let given = Path::new("C:/emu/pcsx2.exe");
        assert_eq!(find_pcsx2_with(Some(given), None, nowhere, |_| true), Some(given.to_path_buf()));
        let environment = Some(PathBuf::from("/opt/pcsx2"));
        assert_eq!(find_pcsx2_with(None, environment, nowhere, |_| true), Some(PathBuf::from("/opt/pcsx2")));
        let on_path = |name: &str| Some(PathBuf::from(format!("/bin/{name}")));
        assert_eq!(find_pcsx2_with(None, None, on_path, |_| false), Some(PathBuf::from("/bin/pcsx2-qt")));
        let installed = Path::new(INSTALLS[0]);
        assert_eq!(find_pcsx2_with(None, None, nowhere, |path| path == installed), Some(installed.to_path_buf()));
        assert_eq!(find_pcsx2_with(None, None, nowhere, |_| false), None);
    }

    #[test]
    fn the_image_boots_without_the_ps2_screens() {
        let parts = command(Path::new("pcsx2-qt"), Path::new("game.iso"), true);
        assert_eq!(parts[..3], ["pcsx2-qt", "-fastboot", "--"]);
        assert!(parts[3].to_string_lossy().ends_with("game.iso"));
        assert!(Path::new(&parts[3]).is_absolute());
    }
}
