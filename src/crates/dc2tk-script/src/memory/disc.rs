//! The disc's files, as the check reads them: an extracted folder or an image (a DiscSource), or
//! an image with a build's files over it.

use super::*;

/// The disc's files by their DATA.DAT path, in any case.
pub trait Disc: Sync {
    fn read(&self, path: &str) -> Option<Cow<'_, [u8]>>;
    /// Every file's path, as the disc spells it.
    fn paths(&self) -> Vec<String>;
}

/// The disc as given: an image, or an extracted disc folder (its files outside the packs' `.d`
/// folders, as its manifest lists them).
impl Disc for DiscSource {
    fn read(&self, path: &str) -> Option<Cow<'_, [u8]>> {
        DiscSource::read(self, path).ok()
    }

    fn paths(&self) -> Vec<String> {
        DiscSource::paths(self).to_vec()
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
