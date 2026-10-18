//! The Dark Cloud 1 map commands: maps-export (floors, Georama scenes and layouts of a data folder)
//! and the floor build (dc2tk_assets::map_dc1 does the work).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};

use dc2tk_assets::info_cfg;
use dc2tk_assets::map_dc1;

fn files_with(folder: &Path, ext: &str, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(folder)? {
        let path = entry?.path();
        if path.is_dir() {
            if !path.ends_with("mods") {
                files_with(&path, ext, out)?;
            }
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext)) {
            out.push(path);
        }
    }
    Ok(())
}

fn write_files(folder: &Path, files: &[(String, Vec<u8>)]) -> Result<()> {
    for (path, data) in files {
        let target = folder.join(path);
        std::fs::create_dir_all(target.parent().expect("in a folder"))?;
        std::fs::write(&target, data).with_context(|| target.display().to_string())?;
    }
    Ok(())
}

/// The texture files a floor draws from besides its pack's: the .img files of its SET_PATH folder.
fn floor_images(data: &Path, cfg: &[u8]) -> Vec<(String, Vec<u8>)> {
    let set_path = info_cfg::parse(cfg)
        .into_iter()
        .find(|(tag, _)| tag == "SET_PATH")
        .and_then(|(_, args)| args.first().and_then(|a| a.as_str().map(str::to_string)));
    set_path.map_or_else(Vec::new, |path| map_dc1::images_in(&data.join(path)))
}

/// The texture files a scene's parts draw from: the .img files of its folder and the two above.
pub fn scene_images(scn: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut folder = scn.parent();
    for _ in 0..3 {
        let Some(here) = folder else { break };
        out.extend(map_dc1::images_in(here));
        folder = here.parent();
    }
    out
}

fn floor_paths(data: &Path, floor: &str) -> (PathBuf, PathBuf) {
    (data.join("dun").join(format!("{floor}.cfg")), data.join("dun").join("mpd_pack").join(format!("{floor}.mpd")))
}

#[derive(Default)]
struct Tally {
    written: usize,
    failed: usize,
    identical: usize,
    differ: usize,
}

impl Tally {
    fn report(&self, what: &str, check: bool) {
        print!("{what}: {} written", self.written);
        if self.failed > 0 {
            print!(", {} not exported", self.failed);
        }
        if check {
            print!(", {} build back identical, {} do not", self.identical, self.differ);
        }
        println!();
    }

    fn compare(&mut self, name: &str, same: Result<bool>) {
        match same {
            Ok(true) => self.identical += 1,
            Ok(false) => {
                self.differ += 1;
                eprintln!("{name}: builds back to other bytes");
            }
            Err(error) => {
                self.differ += 1;
                eprintln!("{name}: build: {error:#}");
            }
        }
    }
}

pub fn run_export(data: &Path, out: &Path, check: bool) -> Result<ExitCode> {
    // dungeon floors: dun/<floor>.cfg with dun/mpd_pack/<floor>.mpd
    let mut floors = Tally::default();
    let mut names: Vec<String> = std::fs::read_dir(data.join("dun"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|name| name.to_lowercase().ends_with(".cfg"))
        .map(|name| name[..name.len() - 4].to_string())
        .filter(|floor| floor_paths(data, floor).1.is_file())
        .collect();
    names.sort();
    for floor in &names {
        let (cfg_path, mpd_path) = floor_paths(data, floor);
        let (cfg, mpd) = (std::fs::read(&cfg_path)?, std::fs::read(&mpd_path)?);
        let images = floor_images(data, &cfg);
        let folder = out.join("floors").join(floor);
        match map_dc1::floor_export(&cfg, &mpd, &images) {
            Ok(files) => {
                write_files(&folder, &files)?;
                floors.written += 1;
                if check {
                    let built = map_dc1::floor_build(&folder, &cfg, &mpd, &images);
                    floors.compare(floor, built.map(|(c, m)| c == cfg && m == mpd).map_err(Into::into));
                }
            }
            Err(error) => {
                floors.failed += 1;
                eprintln!("{floor}: {error}");
            }
        }
    }
    // Georama scenes and layouts
    let mut scenes = Tally::default();
    let mut paths = Vec::new();
    files_with(data, "scn", &mut paths)?;
    paths.sort();
    for path in &paths {
        let relative = path.strip_prefix(data).unwrap_or(path).to_string_lossy().replace('\\', "/");
        let original = std::fs::read(path)?;
        let images = scene_images(path);
        let folder = out.join("scenes").join(&relative);
        match map_dc1::scene_export(&original, &images) {
            Ok(files) => {
                write_files(&folder, &files)?;
                scenes.written += 1;
                if check {
                    let built = map_dc1::scene_build(&folder, &original, &images);
                    scenes.compare(&relative, built.map(|b| b == original).map_err(Into::into));
                }
            }
            Err(error) => {
                scenes.failed += 1;
                eprintln!("{relative}: {error}");
            }
        }
    }
    let mut layouts = Tally::default();
    let mut paths = Vec::new();
    files_with(data, "edt", &mut paths)?;
    paths.sort();
    for path in &paths {
        let relative = path.strip_prefix(data).unwrap_or(path).to_string_lossy().replace('\\', "/");
        let original = std::fs::read(path)?;
        match map_dc1::layout_export(&original) {
            Ok(json) => {
                let target = out.join("layouts").join(format!("{relative}.json"));
                std::fs::create_dir_all(target.parent().expect("in a folder"))?;
                std::fs::write(&target, &json)?;
                layouts.written += 1;
                if check {
                    layouts.compare(&relative, map_dc1::layout_build(&json).map(|b| b == original).map_err(Into::into));
                }
            }
            Err(error) => {
                layouts.failed += 1;
                eprintln!("{relative}: {error}");
            }
        }
    }
    floors.report("dungeon floors", check);
    scenes.report("Georama scenes", check);
    layouts.report("Georama layouts", check);
    let bad = [&floors, &scenes, &layouts].iter().map(|t| t.failed + t.differ).sum::<usize>();
    Ok(if bad == 0 { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

pub fn run_floor_build(folder: &Path, data: &Path, floor: &str, output: &Path) -> Result<ExitCode> {
    let (cfg_path, mpd_path) = floor_paths(data, floor);
    let (cfg, mpd) = (std::fs::read(&cfg_path)?, std::fs::read(&mpd_path)?);
    let images = floor_images(data, &cfg);
    let (new_cfg, new_mpd) = map_dc1::floor_build(folder, &cfg, &mpd, &images)?;
    std::fs::create_dir_all(output)?;
    std::fs::write(output.join(format!("{floor}.cfg")), new_cfg)?;
    std::fs::write(output.join(format!("{floor}.mpd")), new_mpd)?;
    println!("written to {}", output.display());
    Ok(ExitCode::SUCCESS)
}
