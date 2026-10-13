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
use c2_script::{build, definitions, discfiles, index, mods, patches, sdk, startat, tree};

use crate::files;
use crate::Common;

#[derive(Args)]
pub struct BuildArgs {
    /// The source tree, as setup or decompile wrote it.
    source: PathBuf,
    /// The retail disc image the source came from.
    iso: PathBuf,
    /// Disc image to write.
    #[arg(short, long)]
    output: PathBuf,
    /// Test image only: a new game goes straight to this map and program, as in s51:story_50.
    #[arg(long, value_name = "MAP:PROGRAM")]
    start_at: Option<String>,
    /// Layer this mod over the tree; repeat for several, which may not change the same file.
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
        /// Mods to apply, in order.
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
        /// Functions to override, as the script names them.
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
    /// Copy game files off the disc (models, textures, sounds) into a mod, to replace them there.
    Asset {
        /// The disc image.
        iso: PathBuf,
        /// The mod's name.
        name: String,
        /// Files as extract lays them out: snd2/sp/sp_000.snd, or a pack member as
        /// chara/c01_base.chr.d/c01b.img.
        #[arg(required = true)]
        paths: Vec<String>,
        /// Export in a common format to edit there: png for textures.
        #[arg(long = "as")]
        form: Option<String>,
        #[arg(long, default_value = "mods")]
        mods: PathBuf,
    },
    /// Compile what mods change, without a disc, and show their numbers.
    Check {
        /// The decompiled source tree.
        source: PathBuf,
        /// Mods to check together, in build order.
        #[arg(required = true)]
        names: Vec<String>,
        #[arg(long, default_value = "mods")]
        mods: PathBuf,
        #[command(flatten)]
        common: Common,
    },
}

/// The constants mods' additions see: the shipped definitions' and the default SDK's, with the
/// tree's own headers (as mods.py's tree_constants).
fn default_constants(tree_dir: &Path) -> Result<IndexMap<String, i64>> {
    let defs = definitions::load(&[files::default_defs()])?;
    Ok(index::constants(index::tree_headers(tree_dir, &defs, &files::root().join("sdk"))?.values()))
}

fn mod_folders(mods_dir: &Path, names: &[String]) -> Result<Vec<PathBuf>> {
    Ok(names.iter().map(|name| mods::folder(mods_dir, name)).collect::<c2_script::Result<_>>()?)
}

fn shown(path: &Path) -> String {
    path.display().to_string()
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
    let consts = index::constants(index::tree_headers(&args.source, &defs, &sdk_dir)?.values());
    let plan = mods::patch_plan(&mod_dirs, &args.source, &consts, true)?;
    for mod_dir in &mod_dirs {
        let name = mod_dir.file_name().unwrap().to_string_lossy().into_owned();
        let mine: Vec<&patches::Patch> = plan.patches.iter().filter(|patch| patch.module == name).collect();
        println!(
            "mod {}: {} patches, {} files, {} game files",
            name,
            mine.len(),
            mods::contents(mod_dir, &args.source)?.len(),
            discfiles::mod_files(mod_dir)?.len()
        );
        for patch in mine {
            println!("  patch {}", patches::summary(patch));
        }
        let given: BTreeMap<&String, &i64> = plan.numbers.get(&name).map(|found| found.iter().collect()).unwrap_or_default();
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
        let mut target = startat::parse_target(&start_at)?;
        let given: IndexMap<&String, &i64> = plan.numbers.values().flat_map(|found| found.iter()).collect();
        if let Some(number) = given.get(&target.program) {
            // a mod's NEW_PROGRAM name
            target.program = number.to_string();
        }
        for warning in startat::warnings(&args.source, &consts, &target)? {
            println!("warning: {warning}");
        }
        let patched = startat::overrides(&args.source, &target, &overrides)?;
        let mut names: Vec<&String> = patched.keys().collect();
        names.sort();
        let names = names.iter().map(|name| name.as_str()).collect::<Vec<_>>().join(", ");
        overrides.extend(patched);
        println!(
            "start at: a new game goes to map {} at program {} (patched {}; flag {} marks it done)",
            target.map,
            target.program,
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
    let (files, replaced) = discfiles::replaced(&args.iso, &disc_files, &found.files, &tops)?;
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
        if item.data.len() > old {
            // models, textures and sounds load into fixed memory the build cannot see
            println!(
                "warning: {} grew by {} bytes; if the game runs short of memory where it loads, keep it nearer the original's size",
                item.path,
                item.data.len() - old
            );
        }
    }
    let written = rebuild::write_disc(&args.iso, &args.output, &files)?;
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
        println!("the game's file index has {} bytes free for more new files (each takes 13 plus its path)", written.index_free);
    }
    let mut wrong = build::check(&args.output, &found.scripts)?;
    wrong.extend(discfiles::check(&args.output, &replaced)?);
    for path in &wrong {
        println!("reads back wrong: {path}");
    }
    println!(
        "{} scripts changed, {} game files replaced or added, {} disc files rewritten, written to {}",
        found.scripts.len(),
        replaced.len(),
        files.len(),
        shown(&args.output)
    );
    if !wrong.is_empty() {
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
            let (applied, warnings) = mods::apply(&mod_folders(mods_dir, names)?, source, *force, &default_constants(source)?)?;
            for warning in warnings {
                println!("warning: {warning}");
            }
            println!("{} files applied to {}", applied.len(), shown(source));
            Ok(ExitCode::SUCCESS)
        }
        ModCommand::Check { source, names, mods: mods_dir, common } => {
            let defs = common.definitions()?;
            let sdk_dir = common.sdk_dir();
            let headers = sdk::header_texts(&defs, &sdk_dir)?;
            let tables = common.tables()?;
            let tools = Toolchain { headers: &headers, rules: &defs.tables, tables: &tables, sdk_dir: &sdk_dir };
            let result = mods::check(&mod_folders(mods_dir, names)?, source, &tools, &default_constants(source)?)?;
            for (name, found) in &result.numbers {
                let sorted: BTreeMap<&String, &i64> = found.iter().collect();
                for (constant, number) in sorted {
                    println!("{name}: {constant} = {number}");
                }
            }
            for (path, error) in &result.errors {
                println!("error: {path}: {error}");
            }
            println!("{} scripts compiled, {} failed", result.compiled, result.errors.len());
            Ok(if result.errors.is_empty() { ExitCode::SUCCESS } else { ExitCode::from(1) })
        }
        ModCommand::Asset { iso, name, paths, form, mods: mods_dir } => {
            let mod_dir = mods::folder(mods_dir, name)?;
            for path in discfiles::copy_out(iso, &mod_dir, paths, form.as_deref())? {
                let place = path.split('/').fold(mod_dir.join(discfiles::DISC), |folder, part| folder.join(part));
                println!("{}: replace it there, and build puts it on the disc", shown(&place));
            }
            Ok(ExitCode::SUCCESS)
        }
        ModCommand::Override { source, name, functions, map, script, monster, library, mods: mods_dir, common } => {
            let mod_dir = mods::folder(mods_dir, name)?;
            let kinds: Vec<(&str, &String)> = [("map", map), ("script", script), ("monster", monster), ("library", library)]
                .into_iter()
                .filter_map(|(kind, value)| value.as_ref().filter(|value| !value.is_empty()).map(|value| (kind, value)))
                .collect();
            if kinds.len() != 1 {
                bail!("give one of --map, --script, --monster and --library: the scripts to patch");
            }
            let result = mods::override_functions(source, &mod_dir, kinds[0].0, kinds[0].1, functions)?;
            let added = if result.added.is_empty() { "already overrides them".to_string() } else { result.added.join(", ") };
            println!("{}: {}", shown(&result.path), added);
            println!(
                "patches {} script{}; edit the functions there, and call original() to run the script's own",
                result.scripts.len(),
                if result.scripts.len() == 1 { "" } else { "s" }
            );
            mods::write_patch_flags(&mod_dir, source, &common.sdk_dir(), &common.definitions()?.sets)?;
            Ok(ExitCode::SUCCESS)
        }
        ModCommand::New { source, name, files: paths, mods: mods_dir, common } => {
            let mod_dir = mods::folder(mods_dir, name)?;
            let copied = mods::new(source, &mod_dir, paths)?;
            println!("{} files copied into {}; edit them there", copied.len(), shown(&mod_dir.join(mods::FILES)));
            mods::write_editor_flags(&mod_dir, source, &common.sdk_dir(), &common.definitions()?.sets)?;
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
    let from_environment = std::env::var_os("PCSX2").filter(|value| !value.is_empty()).map(PathBuf::from);
    if let Some(candidate) = given.map(Path::to_path_buf).or(from_environment) {
        return candidate.is_file().then_some(candidate);
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    for name in NAMES {
        for folder in std::env::split_paths(&path) {
            for suffix in ["", ".exe"] {
                let candidate = folder.join(format!("{name}{suffix}"));
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    INSTALLS.iter().map(PathBuf::from).find(|candidate| candidate.is_file())
}

/// Boots an image in PCSX2, skipping the PS2 boot screens unless fast_boot is false.
pub fn launch(image: &Path, pcsx2: Option<&Path>, fast_boot: bool) -> Result<PathBuf> {
    let Some(found) = find_pcsx2(pcsx2) else {
        bail!("PCSX2 not found; pass --pcsx2 PATH or set the PCSX2 environment variable");
    };
    if !image.is_file() {
        bail!("no image at {}", image.display());
    }
    let mut command = std::process::Command::new(&found);
    if fast_boot {
        command.arg("-fastboot");
    }
    command.arg("--").arg(std::fs::canonicalize(image)?.to_string_lossy().trim_start_matches(r"\\?\"));
    command.spawn()?;
    Ok(found)
}
