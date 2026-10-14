//! Dark Chronicle's scripts: the .stb format, its text assembly, the native tables, and (to come)
//! decompiling to C and compiling back.

pub mod additions;
pub mod asm;
pub mod assets;
pub mod build;
pub mod capture;
pub mod compile;
pub mod coverage;
pub mod decompile;
pub mod decompiler;
pub mod definitions;
pub mod disc;
pub mod discfiles;
pub mod editor;
pub mod fingerprint;
pub mod flags;
pub mod index;
pub mod learn;
pub mod library;
pub mod loads;
pub mod mapedit;
pub mod maps;
pub mod memory;
pub mod messages;
pub mod migrate;
pub mod mods;
pub mod names;
pub mod natives;
pub mod notes;
pub mod numbers;
pub mod patches;
pub mod pathnames;
pub mod renames;
pub mod replace;
pub mod sdk;
pub mod slim;
pub mod slots;
pub mod soundpass;
pub mod source;
pub mod startat;
#[macro_use]
pub mod node;
pub mod parse;
pub mod stack;
pub mod stb;
pub mod tree;
pub mod vm;
pub mod xref;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Disc(#[from] c2_disc::Error),
}

impl Error {
    pub fn invalid(message: impl Into<String>) -> Error {
        Error::Invalid(message.into())
    }
}

impl From<c2_assets::Error> for Error {
    fn from(error: c2_assets::Error) -> Error {
        match error {
            c2_assets::Error::Invalid(message) => Error::Invalid(message),
            c2_assets::Error::Io(error) => Error::Io(error),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
