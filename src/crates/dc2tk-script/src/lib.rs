//! Dark Chronicle's scripts: the .stb format, its text assembly, the native tables, decompiling
//! to C and compiling back, and the tools a mod is built with.

pub mod additions;
pub mod asm;
pub mod assets;
pub mod build;
pub mod capture;
pub mod compile;
pub mod composite;
pub mod constants;
pub mod coverage;
pub mod cpp;
pub mod ctype;
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
pub mod lower;
pub mod mapedit;
pub mod maps;
pub mod mapscript;
pub mod maptable;
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
pub mod slotflow;
pub mod slots;
pub mod soundpass;
pub mod source;
pub mod startat;
#[macro_use]
pub mod node;
pub mod parse;
pub mod paths;
pub mod stack;
pub mod stb;
pub mod tree;
pub mod villagers;
pub mod vm;
pub mod xref;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    /// What is wrong with a statement of a source file, and its line there.
    #[error("line {line}: {error}")]
    At { line: i64, error: Box<Error> },
    /// An error with what was being done when it happened ("texture:tex02 (mod fish) at ...").
    #[error("{what}: {error}")]
    Context { what: String, error: Box<Error> },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Disc(#[from] dc2tk_disc::Error),
}

impl Error {
    pub fn invalid(message: impl Into<String>) -> Error {
        Error::Invalid(message.into())
    }

    /// The error of a statement on a line (a node's "line", when it has one): an error of the
    /// source given the line, unless it has the line of a statement inside this one already.
    pub fn at(self, line: &node::Node) -> Error {
        match (self, line.as_i64()) {
            (error @ Error::Invalid(_), Some(line)) => Error::At { line, error: Box::new(error) },
            (error, _) => error,
        }
    }

    /// Whether the error is one of the input (a source file, a tree) rather than of reading or
    /// writing a file.
    pub fn is_input(&self) -> bool {
        match self {
            Error::Invalid(_) | Error::At { .. } => true,
            Error::Context { error, .. } => error.is_input(),
            Error::Io(_) | Error::Disc(_) => false,
        }
    }
}

impl From<dc2tk_assets::Error> for Error {
    fn from(error: dc2tk_assets::Error) -> Error {
        match error {
            dc2tk_assets::Error::Invalid(message) => Error::Invalid(message),
            dc2tk_assets::Error::Io(error) => Error::Io(error),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_statements_error_has_the_line_of_the_innermost_statement() {
        let inner = Error::invalid("unknown variable x").at(&node::Node::Int(6));
        assert!(inner.is_input());
        assert_eq!(inner.at(&node::Node::Int(5)).to_string(), "line 6: unknown variable x");
        assert_eq!(Error::invalid("no main").at(&node::Node::Null).to_string(), "no main");
    }

    #[test]
    fn an_error_in_context_says_what_was_being_done_and_keeps_its_cause() {
        let wrap = |error: Error| Error::Context { what: "texture:tex02 (mod fish)".into(), error: Box::new(error) };
        let input = wrap(Error::invalid("no picture 3"));
        assert_eq!(input.to_string(), "texture:tex02 (mod fish): no picture 3");
        assert!(input.is_input());
        let io = wrap(Error::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "gone")));
        assert!(!io.is_input());
        assert!(matches!(io, Error::Context { error, .. } if matches!(*error, Error::Io(_))));
    }
}
