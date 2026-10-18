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
//! monsters/<stem>/                a new enemy: monster.json, model/ (model-export of its base .chr), script.c
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

/// How large a replacement of a game file may grow on the PC port (play-fork from 2026-10-18;
/// E:\Chronicle-project-Claude\DC1_PORT_SIZE_LIMITS.md), or None for a kind held to its retail size.
/// The port also checks at load time that the file fits the room left where the game reads it.
pub fn port_limit(path: &str) -> Option<usize> {
    let lower = path.to_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");
    match ext {
        "mpd" => Some(6_399_999),
        "cfg" => Some(1_048_576),
        "chr" => Some(16_777_216),
        "stb" => Some(4_194_304),
        "mes" if lower.starts_with("opdat/") => Some(68_992),
        "mes" => Some(1_048_576),
        "snd" => Some(16_777_216),
        "scn" => Some(8_388_608),
        "edt" => Some(65_536),
        _ => None,
    }
}

pub struct Options<'a> {
    pub work: &'a Path,
    pub data: &'a Path,
    pub id: &'a str,
    pub name: Option<&'a str>,
    pub out: &'a Path,
    pub natives: &'a Path,
    pub sdk: &'a Path,
    /// For the PC port: files may grow past the retail size up to their kind's limit (port_limit).
    pub port: bool,
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
    // new enemies: dun/monstor/<stem>.chr and .stb, which the game does not have, plus the Lua that
    // registers them (game.models.register, game.characters.register)
    let mut monsters: Vec<NewMonster> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(work.join("monsters")) {
        let mut folders: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        folders.sort();
        for folder in folders {
            match new_monster(&folder, data, &disc, &exe, options) {
                Ok(monster) => monsters.push(monster),
                Err(error) => problems.push(format!(
                    "monsters/{}: {error:#}",
                    folder.file_name().unwrap_or_default().to_string_lossy()
                )),
            }
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
        if file.data.len() <= original.len() {
            maps::size_check(&file.path, file.data.len(), original.len());
            kept.push(file);
            continue;
        }
        if options.port {
            if let Some(limit) = port_limit(&file.path).filter(|&limit| file.data.len() <= limit) {
                println!(
                    "{}: {} bytes, {} more than the game's {} (port mode: this kind may grow to {limit}; the game also checks it fits its buffer when it loads)",
                    file.path,
                    file.data.len(),
                    file.data.len() - original.len(),
                    original.len()
                );
                kept.push(file);
                continue;
            }
        }
        maps::size_check(&file.path, file.data.len(), original.len());
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
    if kept.is_empty() && !soundtrack.is_dir() && monsters.is_empty() {
        bail!("nothing in {} differs from the game's files", work.display());
    }

    // the package; a previous build's resources go first, so files no longer edited do not linger
    let package = options.out.join(options.id);
    let _ = std::fs::remove_dir_all(package.join("resources"));
    std::fs::create_dir_all(&package)?;
    for file in &kept {
        let target = package.join("resources").join(&file.path);
        std::fs::create_dir_all(target.parent().expect("in a folder"))?;
        std::fs::write(&target, &file.data)?;
    }
    for monster in &monsters {
        for (path, bytes) in [(&monster.chr_path, &monster.chr), (&monster.stb_path, &monster.stb)] {
            let target = package.join("resources").join(path);
            std::fs::create_dir_all(target.parent().expect("in a folder"))?;
            std::fs::write(&target, bytes)?;
        }
    }
    let mut paths: Vec<String> = kept.iter().map(|f| format!("  {},", lua_string(&f.path))).collect();
    for monster in &monsters {
        paths.push(format!("  {},", lua_string(&monster.chr_path)));
        paths.push(format!("  {},", lua_string(&monster.stb_path)));
    }
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
    ) + &monsters.iter().map(|m| m.lua.as_str()).collect::<String>();
    std::fs::write(package.join("main.lua"), main)?;
    let manifest = serde_json::json!({
        "schema_version": 1,
        "id": options.id,
        "name": options.name.unwrap_or(options.id),
        "version": "0.1.0",
        "description": format!("{} game files and {} new enemies made with dc1tk", kept.len(), monsters.len()),
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
    println!("{} files and {} new enemies packed into {}", kept.len(), monsters.len(), package.display());
    if too_big > 0 || !problems.is_empty() {
        println!("{too_big} files left out for size, {} problems", problems.len());
        return Ok(ExitCode::FAILURE);
    }
    Ok(ExitCode::SUCCESS)
}

/// A new enemy from monsters/<stem>/: its model and script under a stem the game does not use, and
/// the Lua that registers it.
struct NewMonster {
    chr_path: String,
    chr: Vec<u8>,
    stb_path: String,
    stb: Vec<u8>,
    lua: String,
}

/// monsters/<stem>/monster.json:
/// `{"name": "...", "template_monster_id": 3, "base": "dun/monstor/<base>.chr", "base_script": "dun/monstor/<base>.stb",
///   "stats": {"hp": .., "defense": .., "reward_exp": .., "reward_money": ..}}`.
/// model/ (a model-export of base, edited) and script.c (a decompiled monster script, edited) are
/// optional; without them the base's own files are used.
fn new_monster(
    folder: &Path,
    data: &Path,
    disc: &dc2tk_assets::models::DiscFiles,
    exe: &Path,
    options: &Options,
) -> Result<NewMonster> {
    let stem = folder.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
    if stem.is_empty()
        || stem.len() > 15
        || !stem.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        bail!("the folder name is the new file stem: 1-15 lowercase letters, digits or _");
    }
    let chr_path = format!("dun/monstor/{stem}.chr");
    let stb_path = format!("dun/monstor/{stem}.stb");
    if data.join(&chr_path).exists() || data.join(&stb_path).exists() {
        bail!("the game already has {chr_path} or {stb_path}; a new enemy needs a stem of its own");
    }
    let definition: serde_json::Value =
        serde_json::from_slice(&std::fs::read(folder.join("monster.json")).context("monster.json")?)
            .context("monster.json is not JSON")?;
    let text = |key: &str| definition.get(key).and_then(|v| v.as_str()).map(str::to_string);
    let name = text("name").context("monster.json needs a name")?;
    let template = definition
        .get("template_monster_id")
        .and_then(|v| v.as_i64())
        .context("monster.json needs template_monster_id")?;
    let base = text("base").context("monster.json needs base, the .chr it is made from")?;
    let base_bytes = std::fs::read(data.join(&base)).with_context(|| format!("the game has no {base}"))?;
    let chr = if folder.join("model").is_dir() {
        dc2tk_assets::models::build_on_disc(&folder.join("model"), &base_bytes, &base, disc)?
    } else {
        base_bytes
    };
    let stb = if folder.join("script.c").is_file() {
        let output = std::env::temp_dir().join(format!("dc1tk-monster-{}-{stem}.stb", std::process::id()));
        let status = std::process::Command::new(exe)
            .args(["compile", "--no-defs", "--table", "monster", "--natives"])
            .arg(options.natives)
            .arg("--sdk")
            .arg(options.sdk)
            .arg(folder.join("script.c"))
            .arg("-o")
            .arg(&output)
            .status()?;
        if !status.success() {
            bail!("script.c does not compile (see above)");
        }
        let bytes = std::fs::read(&output)?;
        let _ = std::fs::remove_file(&output);
        bytes
    } else {
        let base_script = text("base_script").context("without script.c, monster.json needs base_script")?;
        std::fs::read(data.join(&base_script)).with_context(|| format!("the game has no {base_script}"))?
    };
    let mut stat_lua = String::new();
    if let Some(map) = definition.get("stats").and_then(|v| v.as_object()) {
        for (key, value) in map {
            if let Some(number) = value.as_i64() {
                stat_lua.push_str(&format!("{key} = {number}, "));
            }
        }
    }
    let lua = format!(
        "\n-- new enemy {stem}: spawn it with game.characters.spawn\n\
         assert(game.models.register({{id = \"{stem}\", template_monster_id = {template}, model = \"{stem}\", script = \"{stem}\"}}))\n\
         assert(game.characters.register({{id = \"{stem}\", kind = \"enemy\", name = {}, model = \"{stem}\", stats = {{{stat_lua}}}}}))\n",
        lua_string(&name)
    );
    println!("monsters/{stem}: {chr_path} {} bytes, {stb_path} {} bytes (new files)", chr.len(), stb.len());
    Ok(NewMonster { chr_path, chr, stb_path, stb, lua })
}

/// Turns the package on in the save's MoonMod profile (moonmod/profile.json, "enabled"), keeping the
/// profile as it was in profile.json.before-<id> the first time. The game reads it when it starts.
pub fn install(save: &Path, id: &str) -> Result<()> {
    let profile_path = save.join("moonmod").join("profile.json");
    let mut profile: serde_json::Value = match std::fs::read(&profile_path) {
        Ok(bytes) => serde_json::from_slice(&bytes).with_context(|| format!("{} is not JSON", profile_path.display()))?,
        Err(_) => serde_json::json!({"format_version": 1, "enabled": {}, "settings": {}, "native_trusted": {}}),
    };
    if profile.get("enabled").and_then(|v| v.get(id)) == Some(&serde_json::Value::Bool(true)) {
        println!("{id} is already on in {}", profile_path.display());
        return Ok(());
    }
    if profile_path.is_file() {
        let backup = profile_path.with_file_name(format!("profile.json.before-{id}"));
        if !backup.exists() {
            std::fs::copy(&profile_path, &backup)?;
        }
    }
    let enabled = profile
        .as_object_mut()
        .context("the profile is not a JSON object")?
        .entry("enabled")
        .or_insert_with(|| serde_json::json!({}));
    enabled.as_object_mut().context("profile \"enabled\" is not an object")?.insert(id.to_string(), true.into());
    std::fs::create_dir_all(profile_path.parent().expect("in a folder"))?;
    std::fs::write(&profile_path, serde_json::to_string_pretty(&profile)? + "\n")?;
    println!("{id} turned on in {} (restart the game to load it)", profile_path.display());
    Ok(())
}
