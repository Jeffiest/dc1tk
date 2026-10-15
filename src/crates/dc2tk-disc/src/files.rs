//! std::fs's file operations with errors that name the file: `files::read(&path)?` fails with
//! "cannot read <path>: <cause>" (Error::File) rather than the bare "os error 2" std gives, which
//! among the thousands of files a build or a decompile reads says nothing of which one it was.

use std::fs;
use std::path::Path;

use crate::{Result, WithPath};

pub fn read(path: impl AsRef<Path>) -> Result<Vec<u8>> {
    let path = path.as_ref();
    fs::read(path).with_path("read", path)
}

pub fn read_to_string(path: impl AsRef<Path>) -> Result<String> {
    let path = path.as_ref();
    fs::read_to_string(path).with_path("read", path)
}

pub fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Result<()> {
    let path = path.as_ref();
    fs::write(path, contents).with_path("write", path)
}

pub fn create_dir_all(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    fs::create_dir_all(path).with_path("create", path)
}

pub fn read_dir(path: impl AsRef<Path>) -> Result<fs::ReadDir> {
    let path = path.as_ref();
    fs::read_dir(path).with_path("list", path)
}

pub fn remove_file(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    fs::remove_file(path).with_path("remove", path)
}

pub fn remove_dir_all(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    fs::remove_dir_all(path).with_path("remove", path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_name_the_file_and_what_was_done() {
        let missing = std::env::temp_dir().join(format!("dc2tk-files-{}", std::process::id())).join("none");
        let said = |error: crate::Error| error.to_string();
        assert!(said(read(&missing).unwrap_err()).starts_with(&format!("cannot read {}: ", missing.display())));
        assert!(said(read_dir(&missing).unwrap_err()).starts_with(&format!("cannot list {}: ", missing.display())));
        assert!(said(remove_file(&missing).unwrap_err()).starts_with(&format!("cannot remove {}: ", missing.display())));
    }
}
