//! mod-pack: a Dark Cloud 1 work folder of edited exports made into a MoonMod package.
//!
//! The work folder holds exports in the layout the export commands write, any subset of:
//!
//! ```text
//! floors/<floor>/                 maps-export (floor.cfg, parts/)
//! scenes/<data path of .scn>/     maps-export (scene.json, part folders)
//! layouts/<data path of .edt>.json
//! models/<data path of .chr>/     model-export
//! scripts/<data path, .c for .stb>  decompile (dun/monstor/*: monster natives, else event)
//! sound/<data path of .snd>/      sound-export
//! lang/<language>.json            lang-export (only the keys whose text changed matter)
//! soundtrack/                     soundtrack.json and recordings, as the port's soundtrack/ folder
//! ```
//!
//! Each is built against the game's own file; a file that comes out as the game's is left out, and
//! one larger than the game's is refused (MoonMod's game.resources.override takes only files no
//! larger). The package gets mod.json, main.lua (one override per file) and resources/<data path>.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{bail, Context, Result};

use dc2tk_assets::{lang_dc1, map_dc1, sound_dc1};

use crate::maps;

fn walk_files(folder: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(folder) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk_files(&path, out);
            } else {
                out.push(path);
            }
        }
    }
}

/// Folders under `root` that end in one of the extensions (a .scn, .chr or .snd export folder),
/// with their path relative to root.
fn export_folders(root: &Path, extensions: &[&str]) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(folder) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = path.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
            if extensions.iter().any(|ext| name.ends_with(ext)) {
                let relative = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
                out.push((relative, path));
            } else {
                stack.push(path);
            }
        }
    }
    out.sort();
    out
}

pub(crate) fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace('\\', "/")
}

/// Every file's data path under a data folder, for models::DiscFiles (skeleton_for looks a
/// motion pack's model up among them).
pub(crate) fn data_paths(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    walk_files(root, &mut files);
    let mut out: Vec<String> = files.iter().map(|f| relative(root, f)).filter(|p| !p.starts_with("mods/")).collect();
    out.sort();
    out
}

/// The data folder a game file is in: the nearest folder above it holding `dun/` and `chara/`.
pub(crate) fn data_root_of(file: &Path) -> Option<PathBuf> {
    file.ancestors().skip(1).find(|dir| dir.join("dun").is_dir() && dir.join("chara").is_dir()).map(Path::to_path_buf)
}

/// A built file for the package: its data path and bytes.
struct Built {
    path: String,
    data: Vec<u8>,
}

fn lua_string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

pub struct Options<'a> {
    pub work: &'a Path,
    pub data: &'a Path,
    pub id: &'a str,
    pub name: Option<&'a str>,
    pub out: &'a Path,
    pub natives: &'a Path,
    pub sdk: &'a Path,
}

pub fn run(options: &Options) -> Result<ExitCode> {
    let (work, data) = (options.work, options.data);
    let mut built: Vec<Built> = Vec::new();
    let mut problems: Vec<String> = Vec::new();
    let read = |path: &str| std::fs::read(data.join(path)).with_context(|| format!("the game has no {path}"));

    // dungeon floors
    if let Ok(entries) = std::fs::read_dir(work.join("floors")) {
        let mut floors: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        floors.sort();
        for folder in floors {
            let floor = folder.file_name().unwrap_or_default().to_string_lossy().to_string();
            let (cfg_path, mpd_path) = maps::floor_paths(data, &floor);
            let (cfg, mpd) = (std::fs::read(&cfg_path)?, std::fs::read(&mpd_path)?);
            let images = maps::floor_images(data, &cfg);
            match map_dc1::floor_build(&folder, &cfg, &mpd, &images) {
                Ok((new_cfg, new_mpd)) => {
                    built.push(Built { path: format!("dun/{floor}.cfg"), data: new_cfg });
                    built.push(Built { path: format!("dun/mpd_pack/{floor}.mpd"), data: new_mpd });
                }
                Err(error) => problems.push(format!("floors/{floor}: {error}")),
            }
        }
    }
    // Georama scenes
    for (path, folder) in export_folders(&work.join("scenes"), &[".scn"]) {
        let original = read(&path)?;
        let images = maps::scene_images(&data.join(&path));
        match map_dc1::scene_build(&folder, &original, &images) {
            Ok(new) => built.push(Built { path, data: new }),
            Err(error) => problems.push(format!("scenes/{path}: {error}")),
        }
    }
    // Georama layouts
    let mut files = Vec::new();
    walk_files(&work.join("layouts"), &mut files);
    for file in files.iter().filter(|f| f.to_string_lossy().to_lowercase().ends_with(".edt.json")) {
        let path = relative(&work.join("layouts"), file);
        let path = path.strip_suffix(".json").unwrap_or(&path).to_string();
        match map_dc1::layout_build(&std::fs::read(file)?) {
            Ok(new) => built.push(Built { path, data: new }),
            Err(error) => problems.push(format!("layouts/{path}: {error}")),
        }
    }
    // character models, and packs of motions alone (on their character's skeleton)
    let paths = data_paths(data);
    let read_any = |path: &str| std::fs::read(data.join(path)).ok();
    let disc = dc2tk_assets::models::DiscFiles { paths: paths.clone(), read: &read_any };
    for (path, folder) in export_folders(&work.join("models"), &[".chr"]) {
        let original = read(&path)?;
        match dc2tk_assets::models::build_on_disc(&folder, &original, &path, &disc) {
            Ok(new) => built.push(Built { path, data: new }),
            Err(error) => problems.push(format!("models/{path}: {error}")),
        }
    }
    // sound packs
    for (path, folder) in export_folders(&work.join("sound"), &[".snd"]) {
        let original = read(&path)?;
        match sound_dc1::build(&folder, &original) {
            Ok(new) => built.push(Built { path, data: new }),
            Err(error) => problems.push(format!("sound/{path}: {error}")),
        }
    }
    // scripts, compiled by this program's own compile command
    let mut files = Vec::new();
    walk_files(&work.join("scripts"), &mut files);
    let exe = std::env::current_exe()?;
    let temp = std::env::temp_dir().join(format!("dc1tk-mod-pack-{}", std::process::id()));
    std::fs::create_dir_all(&temp)?;
    for file in files.iter().filter(|f| f.extension().is_some_and(|e| e == "c")) {
        let source = relative(&work.join("scripts"), file);
        let path = format!("{}.stb", source.strip_suffix(".c").unwrap_or(&source));
        let table = if path.starts_with("dun/monstor/") { "monster" } else { "event" };
        let output = temp.join(format!("{}.stb", built.len()));
        let status = std::process::Command::new(&exe)
            .args(["compile", "--no-defs", "--table", table, "--natives"])
            .arg(options.natives)
            .arg("--sdk")
            .arg(options.sdk)
            .arg(file)
            .arg("-o")
            .arg(&output)
            .status()?;
        if status.success() {
            built.push(Built { path, data: std::fs::read(&output)? });
        } else {
            problems.push(format!("scripts/{source}: does not compile (see above)"));
        }
    }
    let _ = std::fs::remove_dir_all(&temp);
    // text
    if work.join("lang").is_dir() {
        let (texts, bad) = lang_dc1::import(data, &work.join("lang"))?;
        problems.extend(bad);
        built.extend(texts.into_iter().map(|(path, data)| Built { path, data }));
    }

    // what changed, and whether it fits
    let mut kept = Vec::new();
    let mut too_big = 0;
    for file in built {
        let original = read(&file.path)?;
        if file.data == original {
            continue;
        }
        if maps::size_check(&file.path, file.data.len(), original.len()) {
            kept.push(file);
            continue;
        }
        // too big: the slack the game's files carry taken out (fit_dc1), when that is enough
        match dc2tk_assets::fit_dc1::fit(&file.path, &file.data, original.len())? {
            (Some(fitted), notes) => {
                println!("  made to fit, {} bytes now ({})", fitted.len(), notes.join(", "));
                kept.push(Built { path: file.path, data: fitted });
            }
            (None, notes) => {
                let tried = if notes.is_empty() { "nothing to take out".to_string() } else { notes.join(", ") };
                println!("  still too big after taking out what the file can spare ({tried}); left out");
                too_big += 1;
            }
        }
    }
    for problem in &problems {
        println!("error: {problem}");
    }
    let soundtrack = work.join("soundtrack");
    if kept.is_empty() && !soundtrack.is_dir() {
        bail!("nothing in {} differs from the game's files", work.display());
    }

    // the package
    let package = options.out.join(options.id);
    std::fs::create_dir_all(&package)?;
    for file in &kept {
        let target = package.join("resources").join(&file.path);
        std::fs::create_dir_all(target.parent().expect("in a folder"))?;
        std::fs::write(&target, &file.data)?;
    }
    let paths: Vec<String> = kept.iter().map(|f| format!("  {},", lua_string(&f.path))).collect();
    let main = format!(
        "-- Made by dc1tk mod-pack: each file replaces the game's file of the same path (no larger, as\n\
         -- MoonMod's game.resources.override needs).\n\
         local files = {{\n{}\n}}\n\n\
         for _, path in ipairs(files) do\n\
         \x20 local ok, err = game.resources.override({{path = \"resources/\" .. path, game_path = path}})\n\
         \x20 if ok then\n\
         \x20   moonmod.log(\"info\", \"overriding \" .. path)\n\
         \x20 else\n\
         \x20   moonmod.log(\"error\", path .. \": \" .. tostring(type(err) == \"table\" and err.message or err))\n\
         \x20 end\n\
         end\n",
        paths.join("\n")
    );
    std::fs::write(package.join("main.lua"), main)?;
    let manifest = serde_json::json!({
        "schema_version": 1,
        "id": options.id,
        "name": options.name.unwrap_or(options.id),
        "version": "0.1.0",
        "description": format!("{} game files made with dc1tk", kept.len()),
        "framework_api": 1,
        "games": ["chronicle"],
        "entry": "main.lua",
        "permissions": ["game.read", "game.write", "content.register"],
    });
    std::fs::write(package.join("mod.json"), serde_json::to_string_pretty(&manifest)? + "\n")?;
    if soundtrack.is_dir() {
        let mut files = Vec::new();
        walk_files(&soundtrack, &mut files);
        for file in files {
            let target = package.join("soundtrack").join(relative(&soundtrack, &file));
            std::fs::create_dir_all(target.parent().expect("in a folder"))?;
            std::fs::copy(&file, &target)?;
        }
        println!(
            "soundtrack/ copied into the package: the port reads music replacements from the soundtrack/ folder beside \
             its save folder (docs/SOUNDTRACK.md); copy it there and set Audio > Soundtrack to Custom"
        );
    }
    println!("{} files packed into {}", kept.len(), package.display());
    if too_big > 0 || !problems.is_empty() {
        println!("{too_big} files left out for size, {} problems", problems.len());
        return Ok(ExitCode::FAILURE);
    }
    Ok(ExitCode::SUCCESS)
}
