//! play, and build --play: finding PCSX2 and booting an image in it.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

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
    std::process::Command::new(&parts[0])
        .args(&parts[1..])
        .spawn()
        .with_context(|| format!("cannot start PCSX2 ({})", found.display()))?;
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

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
