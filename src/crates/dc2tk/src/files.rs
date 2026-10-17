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
    root().join("definitions").join("natives")
}

/// The shipped definition set.
pub fn default_defs() -> PathBuf {
    root().join("definitions").join("pal")
}

/// A path as messages show it. On Windows that is with \ throughout, so a path the tools joined
/// with "/" reads as one path, and without empty or "." parts; elsewhere it is as it is.
pub fn shown_path(path: impl AsRef<Path>) -> String {
    let path = path.as_ref().to_string_lossy();
    if !cfg!(windows) {
        return path.into_owned();
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
