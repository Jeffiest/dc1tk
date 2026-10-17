//! The SDK: the headers (sdk/, and each definition set's include/) that declare the natives,
//! programs, inline helpers, constants and value kinds the source uses. Compiling resolves the
//! readable source to what the VM runs (resolve); decompiling turns the VM's calls back into it
//! (present): natives by their SDK names, helpers folded back from the statements they expand to,
//! constants for numbers, results as assignments, locals named for what they hold, and the sound
//! entries play_se plays named for the bank they come from.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use indexmap::IndexMap;

use crate::definitions::Definitions;
use crate::natives::Table;
use crate::node::{kinds, Map, Node};
use crate::{assets, names, paths, Error, Result};

pub use crate::soundpass::{library_sounds, Passed, Seed, Sounds};

/// The prelude every script includes; it declares no natives.
pub const PRELUDE: &str = "dc2tk.h";
pub const KEYWORDS: [&str; 34] = [
    "if",
    "else",
    "while",
    "return",
    "break",
    "continue",
    "WAIT",
    "END",
    "SKIP_END",
    "PRINT",
    "SIN",
    "COS",
    "int",
    "float",
    "void",
    "string",
    "any",
    "extern",
    "static",
    "inline",
    "enum",
    "program",
    "NATIVE",
    "OPTIONAL",
    "PROGRAM",
    "MAIN",
    "AT",
    "DIRECT",
    "FLOAT_AS_INT",
    "INT_AS_FLOAT",
    "AND_VALUE",
    "OR_VALUE",
    "DOUBLE_STORE",
    "VM_ARGS",
];
const DECLARATION_KEYS: [&str; 5] = ["natives", "programs", "inlines", "constants", "kinds"];

mod headers;
mod present;
pub mod reference;
mod resolve;

pub use self::headers::*;
pub use self::present::*;
pub use self::resolve::*;
