//! Dark Chronicle's assets in common formats: texture files (.img) as PNGs, sound banks (.snd) as
//! WAVs and character packs (.chr) as glTF 2.0, exported and built back byte for byte as the
//! Python tools (im3.py, textures.py, soundbank.py, sounds.py, mds.py, mot.py, info_cfg.py,
//! to_gltf.py, from_gltf.py, models.py) do.

pub mod cfg_text;
pub mod data_table;
pub mod dungeon_data;
pub mod folder;
pub mod georama_data;
pub mod from_gltf;
pub mod ids;
pub mod im3;
pub mod info_cfg;
pub mod invention_data;
pub mod item_data;
pub mod json;
pub mod map_edit;
pub mod map_export;
pub mod map_import;
pub mod map_scene;
pub mod map_transform;
pub mod mapfile;
pub mod mds;
pub mod models;
pub mod monster_data;
pub mod mot;
pub mod py;
pub mod shop_data;
pub mod soundbank;
pub mod sounds;
pub mod textures;
pub mod to_gltf;
pub mod tree;
pub mod tree_check;

/// What went wrong, as the Python's exception message says it.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Error {
    pub fn invalid(message: impl Into<String>) -> Error {
        Error::Invalid(message.into())
    }
}

impl From<c2_disc::Error> for Error {
    fn from(error: c2_disc::Error) -> Error {
        match error {
            c2_disc::Error::Invalid(message) => Error::Invalid(message),
            c2_disc::Error::Io(error) => Error::Io(error),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Returns early with an Error::Invalid of the formatted message.
#[macro_export]
macro_rules! bail {
    ($($arg:tt)*) => {
        return Err($crate::Error::invalid(format!($($arg)*)))
    };
}

/// Paths sorted as pathlib sorts them: by their parts, ignoring case on Windows.
pub(crate) fn sorted_paths(mut paths: Vec<std::path::PathBuf>) -> Vec<std::path::PathBuf> {
    if cfg!(windows) {
        paths.sort_by_cached_key(|path| {
            path.to_string_lossy().replace('/', "\\").to_lowercase().split('\\').map(str::to_string).collect::<Vec<_>>()
        });
    } else {
        paths.sort();
    }
    paths
}

/// The files under a folder, at any depth, as sorted(folder.rglob("*")) lists them.
pub(crate) fn files_under(folder: &std::path::Path) -> Result<Vec<std::path::PathBuf>> {
    let mut found = Vec::new();
    let mut pending = vec![folder.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.is_file() {
                found.push(path);
            }
        }
    }
    Ok(sorted_paths(found))
}

/// The path of a file relative to a folder, with "/" (Path.relative_to(...).as_posix()).
pub(crate) fn relative_posix(path: &std::path::Path, folder: &std::path::Path) -> String {
    let relative = path.strip_prefix(folder).unwrap_or(path);
    relative.components().map(|part| part.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/")
}
