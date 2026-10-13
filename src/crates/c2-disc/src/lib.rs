//! Dark Chronicle's disc: the ISO 9660 image, the DATA.HD4 index of DATA.DAT, the packs inside
//! its files, extracting them all, and writing a new image with files replaced or added.

use std::fs::File;
use std::path::Path;

pub mod archive;
mod cp932_fixes;
pub mod extract;
pub mod iso;
pub mod rebuild;
#[doc(hidden)]
pub mod samples;
pub mod text;

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

pub type Result<T> = std::result::Result<T, Error>;

/// A disc image mapped into memory, read without copying.
pub struct Image {
    map: memmap2::Mmap,
}

impl Image {
    pub fn open(path: &Path) -> Result<Image> {
        let file = File::open(path)?;
        // Safety: the image is only read, and the tools do not change an image they are reading.
        let map = unsafe { memmap2::Mmap::map(&file)? };
        Ok(Image { map })
    }

    pub fn bytes(&self) -> &[u8] {
        &self.map
    }
}
