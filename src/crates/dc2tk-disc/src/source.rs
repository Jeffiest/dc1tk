//! The disc's files as the tools read them, from either form a disc takes: the image (mapped,
//! read without copying) or a folder `extract` wrote (its manifest.json listing the files). A
//! command that takes "the disc image or an extracted disc" opens a DiscSource and reads by
//! DATA.DAT path, the same whichever it was given.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::archive::Archive;
use crate::extract::{Manifest, MANIFEST};
use crate::{iso, Error, Image, Result, WithPath};

/// A disc image or an extracted disc.
pub struct DiscSource {
    form: Form,
    /// Each DATA.DAT file's path, lowercased, to its place in `paths`: the game's paths ignore case.
    index: HashMap<String, usize>,
    /// The DATA.DAT files (not the packs' members), as the disc spells them, in its order.
    paths: Vec<String>,
}

enum Form {
    Image { image: Image, archive: Archive },
    Folder { root: PathBuf, manifest: Manifest },
}

impl DiscSource {
    /// The image at path, or the extracted disc in the folder at path.
    pub fn open(path: &Path) -> Result<DiscSource> {
        let form = if path.is_file() {
            let image = Image::open(path)?;
            let archive = Archive::open(image.bytes())?;
            Form::Image { image, archive }
        } else if path.is_dir() {
            let listing = path.join(MANIFEST);
            if !listing.is_file() {
                return Err(Error::invalid(format!("{} is not an extracted disc (no {MANIFEST})", path.display())));
            }
            let text = fs::read(&listing).with_path("read", &listing)?;
            let manifest = serde_json::from_slice(&text)
                .map_err(|error| Error::invalid(format!("{}: {error}", listing.display())))?;
            Form::Folder { root: path.to_path_buf(), manifest }
        } else {
            return Err(Error::invalid(format!("{} is neither a disc image nor an extracted disc", path.display())));
        };
        let paths: Vec<String> = match &form {
            Form::Image { archive, .. } => archive.entries.iter().map(|entry| entry.path.clone()).collect(),
            Form::Folder { manifest, .. } => {
                manifest.files.iter().filter(|item| !item.packed).map(|item| item.path.clone()).collect()
            }
        };
        let mut index = HashMap::with_capacity(paths.len());
        for (at, path) in paths.iter().enumerate() {
            // the first of two spellings of one path, as the index lookup finds it
            index.entry(path.to_lowercase()).or_insert(at);
        }
        Ok(DiscSource { form, index, paths })
    }

    /// The DATA.DAT files, as the disc spells them, in the disc's order.
    pub fn paths(&self) -> &[String] {
        &self.paths
    }

    /// The disc's spelling of a DATA.DAT path given in any case, when the disc has it.
    pub fn spelling(&self, path: &str) -> Option<&str> {
        self.index.get(&path.to_lowercase()).map(|&at| self.paths[at].as_str())
    }

    /// A DATA.DAT file's bytes, by its path in any case.
    pub fn read(&self, path: &str) -> Result<Cow<'_, [u8]>> {
        let at = *self.index.get(&path.to_lowercase()).ok_or_else(|| Error::Missing(path.to_string()))?;
        match &self.form {
            Form::Image { image, archive } => Ok(Cow::Borrowed(archive.read(image.bytes(), &archive.entries[at]))),
            Form::Folder { root, .. } => {
                let file = folder_path(root, &self.paths[at]);
                Ok(Cow::Owned(fs::read(&file).with_path("read", &file)?))
            }
        }
    }

    /// A file of the disc's own directory tree (the boot executable, SYSTEM.CNF), which extract
    /// writes at the folder's top.
    pub fn disc_file(&self, name: &str) -> Result<Cow<'_, [u8]>> {
        match &self.form {
            Form::Image { image, .. } => {
                let record = iso::find(image.bytes(), name)?.ok_or_else(|| Error::Missing(name.to_string()))?;
                Ok(Cow::Borrowed(iso::read_file(image.bytes(), &record)))
            }
            Form::Folder { root, .. } => {
                let file = root.join(name);
                Ok(Cow::Owned(fs::read(&file).with_path("read", &file)?))
            }
        }
    }

    /// What extract wrote, for an extracted disc: every file with the packs' members after the
    /// file holding them.
    pub fn manifest(&self) -> Option<&Manifest> {
        match &self.form {
            Form::Image { .. } => None,
            Form::Folder { manifest, .. } => Some(manifest),
        }
    }
}

/// A DATA.DAT path inside an extracted disc's folder.
fn folder_path(root: &Path, path: &str) -> PathBuf {
    path.split('/').fold(root.to_path_buf(), |folder, part| folder.join(part))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{extract, samples};

    #[test]
    fn an_image_and_its_extraction_read_alike() {
        let folder = std::env::temp_dir().join(format!("dc2tk-disc-source-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        let iso = folder.join("disc.iso");
        fs::write(&iso, samples::sample_disc()).unwrap();
        extract::extract(&samples::sample_disc(), &folder.join("extracted")).unwrap();
        let image = DiscSource::open(&iso).unwrap();
        let extracted = DiscSource::open(&folder.join("extracted")).unwrap();
        assert_eq!(image.paths(), extracted.paths());
        assert!(image.manifest().is_none() && extracted.manifest().is_some());
        for path in image.paths() {
            assert_eq!(image.read(path).unwrap(), extracted.read(path).unwrap(), "{path}");
        }
        // paths in any case, as the game finds them
        assert_eq!(image.spelling("MAP/T01.STB"), Some("map/t01.stb"));
        assert_eq!(extracted.read("MAP/T01.STB").unwrap(), samples::sample_script());
        assert!(matches!(image.read("map/none.stb"), Err(Error::Missing(_))));
        assert_eq!(image.disc_file("DATA.HD4").unwrap(), extracted.disc_file("DATA.HD4").unwrap());
        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_folder_without_a_manifest_is_not_a_disc() {
        let folder = std::env::temp_dir().join(format!("dc2tk-disc-source-empty-{}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        let error = DiscSource::open(&folder).err().unwrap().to_string();
        assert!(error.contains("not an extracted disc"), "{error}");
        assert!(DiscSource::open(&folder.join("missing")).is_err());
        fs::remove_dir_all(&folder).unwrap();
    }
}
