//! Paths as the tools write them (from a root, with "/"), and walking a folder's files.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

/// root joined with a relative path written with "/" (or "\"), one part at a time.
pub fn join_relative(root: &Path, relative: &str) -> PathBuf {
    relative.split(['/', '\\']).fold(root.to_path_buf(), |path, part| path.join(part))
}

/// The last part of a path, or "" for a path without one.
pub fn file_name(path: &Path) -> String {
    path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
}

/// A path a mod gives (in mod.json) joined to the mod's folder; None when it would leave the
/// folder: an absolute path, a drive, or a "..".
pub fn inside_mod(mod_dir: &Path, relative: &str) -> Option<PathBuf> {
    let inside = Path::new(relative).components().all(|part| matches!(part, Component::Normal(_) | Component::CurDir));
    inside.then(|| join_relative(mod_dir, relative))
}

/// A file or folder walk meets.
pub struct Entry<'a> {
    pub path: &'a Path,
    /// The path from the root, with "/".
    pub relative: &'a str,
    /// The last part of the path.
    pub name: &'a str,
    pub is_dir: bool,
}

/// The files under root that keep takes, as paths from root with "/", in no particular order.
/// keep sees every folder too, and walk goes into the ones it takes. A root that is not a folder
/// has no files; a folder that cannot be read is an error naming it, so a listing is never
/// quietly short.
pub fn walk(root: &Path, keep: impl Fn(&Entry) -> bool) -> io::Result<Vec<String>> {
    let mut found = Vec::new();
    if !root.is_dir() {
        return Ok(found);
    }
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        let unreadable =
            |error: io::Error| io::Error::new(error.kind(), format!("cannot read {}: {error}", folder.display()));
        for entry in fs::read_dir(&folder).map_err(unreadable)? {
            let path = entry.map_err(unreadable)?.path();
            let relative = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
            let name = file_name(&path);
            let entry = Entry { path: &path, relative: &relative, name: &name, is_dir: path.is_dir() };
            if !keep(&entry) {
                continue;
            }
            if entry.is_dir {
                folders.push(path);
            } else {
                found.push(relative);
            }
        }
    }
    Ok(found)
}

/// Every script (.stb, in any case) under a folder, by path from it with "/", sorted. A folder
/// that is not there is an error, not a disc without scripts.
pub fn scripts_under(root: &Path) -> io::Result<Vec<String>> {
    fs::read_dir(root)
        .map_err(|error| io::Error::new(error.kind(), format!("cannot read {}: {error}", root.display())))?;
    let mut found =
        walk(root, |entry| entry.is_dir || entry.path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("stb")))?;
    found.sort();
    Ok(found)
}

/// Sorts paths with "/" by their parts, lowercased on Windows, whose names ignore case.
pub fn sort_by_parts(paths: &mut [String]) {
    paths.sort_by_cached_key(|path| -> Vec<String> {
        let path = if cfg!(windows) { path.to_lowercase() } else { path.clone() };
        path.split('/').map(str::to_string).collect()
    });
}

/// Whether a path's extension is ext; on Windows, whose names ignore case, in any case.
pub fn has_extension(path: &Path, ext: &str) -> bool {
    path.extension().is_some_and(|found| {
        let found = found.to_string_lossy();
        if cfg!(windows) {
            found.eq_ignore_ascii_case(ext)
        } else {
            found == ext
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mod_paths_stay_inside_the_mod() {
        let root = Path::new("mods").join("fish");
        assert_eq!(inside_mod(&root, "disc/map/a.cfg"), Some(root.join("disc").join("map").join("a.cfg")));
        assert_eq!(inside_mod(&root, "./a.wav"), Some(root.join(".").join("a.wav")));
        assert_eq!(inside_mod(&root, "../other/a.wav"), None);
        assert_eq!(inside_mod(&root, "disc/../../a.wav"), None);
        assert_eq!(inside_mod(&root, "/a.wav"), None);
    }

    #[test]
    fn a_walk_lists_what_keep_takes_and_sorts_by_parts() {
        let root = std::env::temp_dir().join(format!("dc2tk-paths-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for file in ["b/x.c", "a/Y.c", "a/z.h", ".cache/w.c", "a.c"] {
            let path = join_relative(&root, file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, "").unwrap();
        }
        let mut found =
            walk(&root, |entry| !entry.name.starts_with('.') && (entry.is_dir || entry.relative.ends_with(".c")))
                .unwrap();
        sort_by_parts(&mut found);
        // a folder's files come before a file named like the folder with more after it
        assert_eq!(found, ["a/Y.c", "a.c", "b/x.c"]);
        assert_eq!(walk(&root.join("missing"), |_| true).unwrap(), Vec::<String>::new());
        for file in ["map/T01.STB", "map/t02.stb", "map/t01.cfg"] {
            let path = join_relative(&root, file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, "").unwrap();
        }
        assert_eq!(scripts_under(&root).unwrap(), ["map/T01.STB", "map/t02.stb"]);
        assert!(scripts_under(&root.join("missing")).unwrap_err().to_string().contains("missing"));
        fs::remove_dir_all(&root).unwrap();
    }
}
