//! Where the tools' data is, and reading and writing text as the Python tools do.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// The repository folder: $DC2TK_ROOT (or $C2_ROOT, its name before the rename), or the nearest
/// folder above the executable that has definitions/ (dc2tk runs from build/release).
pub fn root() -> PathBuf {
    if let Some(root) = std::env::var_os("DC2TK_ROOT").or_else(|| std::env::var_os("C2_ROOT")) {
        return PathBuf::from(root);
    }
    if let Ok(exe) = std::env::current_exe() {
        for folder in exe.ancestors().skip(1) {
            if folder.join("definitions").is_dir() {
                return folder.to_path_buf();
            }
        }
    }
    PathBuf::from(".")
}

/// The native tables' folder dc2tk uses by default.
pub fn natives_dir() -> PathBuf {
    root().join("src").join("natives")
}

/// The shipped definition set.
pub fn default_defs() -> PathBuf {
    root().join("definitions").join("pal")
}

/// A path as Python's pathlib prints it on this system.
pub fn python_path(path: &str) -> String {
    if !cfg!(windows) {
        return path.to_string();
    }
    let rooted = path.starts_with('/') || path.starts_with('\\');
    let parts: Vec<&str> = path.split(['/', '\\']).filter(|part| !part.is_empty() && *part != ".").collect();
    let joined = parts.join("\\");
    if rooted {
        format!("\\{joined}")
    } else {
        joined
    }
}

/// Path.read_text() (dc2tk_py::read_text), naming the file when it cannot be read.
pub fn read_text(path: &Path) -> Result<String> {
    dc2tk_py::read_text(path).with_context(|| format!("cannot read {}", path.display()))
}

/// Path.write_text(), or standard output when there is no path.
pub fn write_output(path: Option<&Path>, text: &str) -> Result<()> {
    let text = dc2tk_py::native_lines(text);
    match path {
        Some(path) => fs::write(path, text.as_bytes()).with_context(|| format!("cannot write {}", path.display())),
        None => {
            let mut out = std::io::stdout().lock();
            out.write_all(text.as_bytes())?;
            Ok(out.flush()?)
        }
    }
}

/// Every .stb under a folder (any case), by path from it with "/", sorted.
pub fn find_scripts(root: &Path) -> Result<Vec<String>> {
    let mut found = Vec::new();
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder).with_context(|| format!("cannot read {}", folder.display()))? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                folders.push(path);
            } else if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("stb")) {
                let relative = path.strip_prefix(root).unwrap();
                found.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    found.sort();
    Ok(found)
}

/// When errors name constants a definition set has renamed since the tree was made ("unknown
/// variable FLAG_S17_E308_SEEN"), a note saying their new names and how to bring the tree up to date.
pub fn rename_hint<'a>(sets: &[PathBuf], errors: impl IntoIterator<Item = &'a str>, tree: &Path) -> Option<String> {
    let mut found: Vec<(String, String)> = Vec::new();
    for error in errors {
        for word in error.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
            if word.is_empty() || found.iter().any(|(old, _)| old == word) {
                continue;
            }
            if let Some(new) = dc2tk_script::constants::renamed_to(sets, word) {
                found.push((word.to_string(), new));
            }
        }
    }
    if found.is_empty() {
        return None;
    }
    let names: Vec<String> = found.iter().map(|(old, new)| format!("{old} is now {new}")).collect();
    Some(format!(
        "note: renamed since the tree was made: {}; dc2tk defs migrate {} gives it the new names",
        names.join(", "),
        python_path(&tree.to_string_lossy())
    ))
}
