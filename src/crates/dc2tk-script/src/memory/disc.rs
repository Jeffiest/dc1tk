//! The disc's files, as the check reads them: an extracted folder, an image, or an image with
//! a build's files over it.

use super::*;

/// The disc's files by their DATA.DAT path, in any case.
pub trait Disc: Sync {
    fn read(&self, path: &str) -> Option<Cow<'_, [u8]>>;
    /// Every file's path, as the disc spells it.
    fn paths(&self) -> Vec<String>;
}

/// An extracted disc folder (`extracted/disc`): its files outside the packs' `.d` folders.
pub struct Folder {
    root: PathBuf,
    index: HashMap<String, String>,
}

impl Folder {
    pub fn open(root: &Path) -> std::io::Result<Folder> {
        // a folder that is not there is an error, not an empty disc
        fs::read_dir(root)?;
        let files = crate::paths::walk(root, |entry| !(entry.is_dir && entry.relative.ends_with(".d")))?;
        let index = files.into_iter().map(|relative| (relative.to_lowercase(), relative)).collect();
        Ok(Folder { root: root.to_path_buf(), index })
    }
}

impl Disc for Folder {
    fn read(&self, path: &str) -> Option<Cow<'_, [u8]>> {
        let relative = self.index.get(&path.to_lowercase())?;
        fs::read(self.root.join(relative)).ok().map(Cow::Owned)
    }

    fn paths(&self) -> Vec<String> {
        let mut paths: Vec<String> = self.index.values().cloned().collect();
        paths.sort();
        paths
    }
}

/// The files of a disc image's DATA.DAT.
pub struct Image<'a> {
    image: &'a [u8],
    archive: &'a archive::Archive,
    index: HashMap<String, usize>,
}

impl<'a> Image<'a> {
    pub fn new(image: &'a [u8], archive: &'a archive::Archive) -> Image<'a> {
        let index = archive.entries.iter().enumerate().map(|(at, entry)| (entry.path.to_lowercase(), at)).collect();
        Image { image, archive, index }
    }
}

impl Disc for Image<'_> {
    fn read(&self, path: &str) -> Option<Cow<'_, [u8]>> {
        let entry = &self.archive.entries[*self.index.get(&path.to_lowercase())?];
        Some(Cow::Borrowed(self.archive.read(self.image, entry)))
    }

    fn paths(&self) -> Vec<String> {
        self.archive.entries.iter().map(|entry| entry.path.clone()).collect()
    }
}

/// A disc with some files replaced or added (a build's new files over the original image).
pub struct Overlay<'a> {
    base: &'a dyn Disc,
    files: HashMap<String, &'a [u8]>,
    added: Vec<String>,
}

impl<'a> Overlay<'a> {
    pub fn new(base: &'a dyn Disc, files: &'a BTreeMap<String, Vec<u8>>) -> Overlay<'a> {
        Overlay {
            base,
            files: files.iter().map(|(path, data)| (path.to_lowercase(), data.as_slice())).collect(),
            added: files.keys().cloned().collect(),
        }
    }
}

impl Disc for Overlay<'_> {
    fn read(&self, path: &str) -> Option<Cow<'_, [u8]>> {
        match self.files.get(&path.to_lowercase()) {
            Some(data) => Some(Cow::Borrowed(data)),
            None => self.base.read(path),
        }
    }

    fn paths(&self) -> Vec<String> {
        let mut paths = self.base.paths();
        let known: std::collections::HashSet<String> = paths.iter().map(|path| path.to_lowercase()).collect();
        paths.extend(self.added.iter().filter(|path| !known.contains(&path.to_lowercase())).cloned());
        paths
    }
}
