//! Where the tools' data is, reading and writing text, and paths as messages show them.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// The repository folder: $DC2TK_ROOT, or the nearest folder above the executable that has
/// definitions/ (dc2tk runs from build/release).
pub fn root() -> PathBuf {
    if let Some(root) = std::env::var_os("DC2TK_ROOT") {
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

/// A path as messages show it. On Windows that is with \ throughout, so a path the tools joined
/// with "/" reads as one path, and without empty or "." parts; elsewhere it is as it is.
pub fn shown_path(path: &str) -> String {
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

/// A text file (dc2tk_disc::textfile::read_text), naming the file when it cannot be read.
pub fn read_text(path: &Path) -> Result<String> {
    dc2tk_disc::textfile::read_text(path).with_context(|| format!("cannot read {}", path.display()))
}

/// Writes text to a file, or to standard output when there is no path.
pub fn write_output(path: Option<&Path>, text: &str) -> Result<()> {
    match path {
        Some(path) => fs::write(path, text.as_bytes()).with_context(|| format!("cannot write {}", path.display())),
        None => {
            let mut out = std::io::stdout().lock();
            out.write_all(text.as_bytes())?;
            Ok(out.flush()?)
        }
    }
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
        shown_path(&tree.to_string_lossy())
    ))
}
