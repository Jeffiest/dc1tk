//! The asset tree's manifest.json as types: what tree::build writes, and what the index of ids
//! (ids::Index::from_manifest) and the tree's check (tree_check) read back, all through these
//! types so they agree on its shape. The text is serde_json's pretty form
//! (dc2tk_disc::textfile::json_text): the members in the order declared here, an asset's `kind`
//! first.

use std::collections::BTreeMap;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::tree::{Kind, Place};
use crate::{Context, Result};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    /// tree::FORMAT, the layout's version.
    pub format: i64,
    pub counts: Counts,
    /// Every asset by id, in id order.
    pub assets: BTreeMap<String, Asset>,
    /// sound:BANK/N and other ids that name an asset known by another.
    pub aliases: BTreeMap<String, String>,
    /// The game files no asset was made of, and why.
    pub unconverted: Vec<Unconverted>,
}

/// How many assets of each kind the tree holds.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Counts {
    pub texture: usize,
    pub model: usize,
    pub bank: usize,
    pub sound: usize,
    pub text: usize,
    pub music: usize,
}

/// A file of the tree and its sha256, in the order the asset's export wrote them.
pub type Files = IndexMap<String, String>;

/// An asset, by its kind: what each kind's export wrote, and where on the disc it is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Asset {
    Texture {
        name: String,
        files: Files,
        game_hash: String,
        format: String,
        width: usize,
        height: usize,
        /// Each place with the picture's entry there.
        places: Vec<AssetPlace>,
    },
    Model {
        name: String,
        files: Files,
        game_hash: String,
        /// The texture ids its glTF points at, and the pictures it has that no texture is.
        textures: Vec<String>,
        placeholders: Vec<String>,
        places: Vec<AssetPlace>,
    },
    Sound {
        /// Its bank's id and its entry there.
        bank: String,
        entry: usize,
        plays: String,
        label: String,
        category: String,
        /// The WAV of an entry that plays a sample, and the hash of the samples it plays.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        files: Option<Files>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        game_hash: Option<String>,
        /// Each place with the entry there.
        places: Vec<AssetPlace>,
    },
    Bank {
        name: String,
        /// The tree's folder of its WAVs, and its entries' sound ids.
        folder: String,
        game_hash: String,
        sounds: Vec<String>,
        places: Vec<AssetPlace>,
    },
    Text {
        name: String,
        /// text::Language::short.
        language: String,
        files: Files,
        game_hash: String,
        /// How many versions of the file its places hold.
        versions: usize,
        places: Vec<AssetPlace>,
    },
    Music {
        name: String,
        files: Files,
        game_hash: String,
        places: Vec<AssetPlace>,
    },
}

/// A place of an asset: a disc file and the pack members inside it, with the picture (of a
/// texture) or entry (of a sound) there.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AssetPlace {
    pub path: String,
    pub members: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picture: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<usize>,
}

/// A game file no asset was made of: its place and why.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Unconverted {
    pub path: String,
    pub members: Vec<String>,
    pub error: String,
}

impl AssetPlace {
    pub fn new(place: &Place) -> AssetPlace {
        AssetPlace { path: place.path.clone(), members: place.members.clone(), picture: None, entry: None }
    }

    pub fn place(&self) -> Place {
        Place { path: self.path.clone(), members: self.members.clone() }
    }

    /// The picture or entry the place names.
    pub fn at(&self) -> Option<usize> {
        self.picture.or(self.entry)
    }
}

impl Unconverted {
    pub fn place(&self) -> Place {
        Place { path: self.path.clone(), members: self.members.clone() }
    }
}

impl Asset {
    pub fn kind(&self) -> Kind {
        match self {
            Asset::Texture { .. } => Kind::Texture,
            Asset::Model { .. } => Kind::Model,
            Asset::Sound { .. } => Kind::Sound,
            Asset::Bank { .. } => Kind::Bank,
            Asset::Text { .. } => Kind::Text,
            Asset::Music { .. } => Kind::Music,
        }
    }

    /// The game's name for it (a sound has none of its own).
    pub fn name(&self) -> &str {
        match self {
            Asset::Texture { name, .. }
            | Asset::Model { name, .. }
            | Asset::Bank { name, .. }
            | Asset::Text { name, .. }
            | Asset::Music { name, .. } => name,
            Asset::Sound { .. } => "",
        }
    }

    pub fn files(&self) -> Option<&Files> {
        match self {
            Asset::Texture { files, .. }
            | Asset::Model { files, .. }
            | Asset::Text { files, .. }
            | Asset::Music { files, .. } => Some(files),
            Asset::Sound { files, .. } => files.as_ref(),
            Asset::Bank { .. } => None,
        }
    }

    pub fn places(&self) -> &[AssetPlace] {
        match self {
            Asset::Texture { places, .. }
            | Asset::Model { places, .. }
            | Asset::Sound { places, .. }
            | Asset::Bank { places, .. }
            | Asset::Text { places, .. }
            | Asset::Music { places, .. } => places,
        }
    }

    /// The first of its files whose path ends with `suffix` (its PNG, glTF, MIDI file, ...).
    pub fn file(&self, suffix: &str) -> Option<&str> {
        self.files()?.keys().find(|path| path.ends_with(suffix)).map(String::as_str)
    }
}

impl Manifest {
    /// The manifest's JSON text, as the tools write every JSON file.
    pub fn text(&self) -> Vec<u8> {
        dc2tk_disc::textfile::json_text(self).into_bytes()
    }

    pub fn read(data: &[u8]) -> Result<Manifest> {
        serde_json::from_slice(data).context(crate::tree::MANIFEST)
    }
}
