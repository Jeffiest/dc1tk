//! Dark Chronicle's scripts: the .stb format, its text assembly, the native tables, and (to come)
//! decompiling to C and compiling back.

pub mod asm;
pub mod decompile;
pub mod definitions;
pub mod disc;
pub mod fingerprint;
pub mod messages;
pub mod names;
pub mod natives;
pub mod notes;
pub mod pathnames;
pub mod sdk;
pub mod source;
#[macro_use]
pub mod node;
pub mod stack;
pub mod stb;
pub mod tree;

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

pub type Result<T> = std::result::Result<T, Error>;
