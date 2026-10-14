//! Asset ids and every place the disc has each, without writing the asset tree: what a mod build
//! needs to find the places an id names. Read straight from the disc (Index::from_disc, a few
//! seconds: no picture, model or sound is converted) it gives the ids `c2 assets` gives, since both
//! name the disc's assets with tree::named; read from an asset tree's manifest.json
//! (Index::from_manifest) it gives that tree's.
//!
//! One difference: the tree lists as models only the packs its export converts, while the index
//! takes every pack whose info.cfg names a model (on the PAL disc, the same packs).

use std::collections::{BTreeMap, HashMap, HashSet};

use rayon::prelude::*;
use serde_json::Value;

use crate::soundbank::{Bank, EntryKind};
use crate::tree::{self, DiscFile, Kind, Place, SoundNames};
use crate::{Error, Result};

/// An asset of the index: its id, kind, the game's name for it and its places, each with the
/// picture (of a texture) or entry (of a sound) there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    pub id: String,
    pub kind: Kind,
    pub name: String,
    pub places: Vec<(Place, Option<usize>)>,
    /// A sound's bank id.
    pub bank: Option<String>,
    /// A bank's number of entries.
    pub entries: Option<usize>,
}

/// Every asset of a disc by id, and the aliases (sound:BANK/N for a named entry).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Index {
    pub assets: BTreeMap<String, Asset>,
    pub aliases: BTreeMap<String, String>,
}

fn kind_named(name: &str) -> Option<Kind> {
    [Kind::Texture, Kind::Model, Kind::Bank, Kind::Sound, Kind::Text, Kind::Music]
        .into_iter()
        .find(|kind| kind.name() == name)
}

/// The kind an id or pattern names by its prefix (texture:, model:, bank:, sound:, text:, music:).
pub fn kind_of(id: &str) -> Option<Kind> {
    kind_named(&id.split_once(':')?.0.to_lowercase())
}

/// Whether a pattern with * (any run of characters) and ? (one character) matches the text,
/// ignoring case.
pub fn glob_matches(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.to_lowercase().chars().collect();
    let text: Vec<char> = text.to_lowercase().chars().collect();
    let (mut p, mut t) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some((p, t));
            p += 1;
        } else if let Some((at, from)) = star {
            p = at + 1;
            t = from + 1;
            star = Some((at, from + 1));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&c| c == '*')
}

pub fn is_pattern(id: &str) -> bool {
    id.contains(['*', '?'])
}

impl Index {
    /// The index of the disc's files; names gives the sound entries' names, as for the tree.
    pub fn from_disc(disc: &[DiscFile], names: &SoundNames) -> Index {
        let found = tree::named(disc);
        let files = &found.files;
        let places = |asset: &tree::Asset| -> Vec<(Place, Option<usize>)> {
            asset.places.iter().map(|&(file, entry)| (files[file].place.clone(), entry)).collect()
        };
        let mut index = Index::default();
        for asset in &found.textures {
            index.insert(Kind::Texture, &asset.id, &asset.shown, places(asset));
        }
        let models: Vec<bool> = found
            .packs
            .par_iter()
            .map(|asset| tree::model_stem(files[asset.first()].data, &asset.name).is_ok())
            .collect();
        for (asset, _) in found.packs.iter().zip(models).filter(|(_, model)| *model) {
            index.insert(Kind::Model, &asset.id, &asset.shown, places(asset));
        }
        let entries: Vec<Option<Vec<usize>>> =
            found.banks.par_iter().map(|asset| bank_entries(files[asset.first()].data)).collect();
        let mut taken = HashSet::new();
        for (asset, entries) in found.banks.iter().zip(entries) {
            let Some(entries) = entries else {
                continue;
            };
            let bank_id = Kind::Bank.id(&asset.id);
            let entry_names = tree::entry_names(asset, names);
            for &entry in &entries {
                let (id, is_named) = tree::sound_id(&asset.id, entry, &entry_names, &mut taken);
                if is_named {
                    index.aliases.insert(Kind::Sound.id(&format!("{}/{entry}", asset.id)), id.clone());
                }
                let at = places(asset).into_iter().map(|(place, _)| (place, Some(entry))).collect();
                index.insert(Kind::Sound, &id[Kind::Sound.name().len() + 1..], "", at);
                index.assets.get_mut(&id).unwrap().bank = Some(bank_id.clone());
            }
            index.insert(Kind::Bank, &asset.id, &asset.shown, places(asset));
            index.assets.get_mut(&bank_id).unwrap().entries = Some(entries.len());
        }
        for asset in &found.texts {
            let at = asset.places.iter().map(|&file| (files[file].place.clone(), None)).collect();
            index.insert(Kind::Text, &asset.id, &asset.name, at);
        }
        for asset in &found.sequences {
            let at =
                asset.places.iter().map(|&(file, member)| (tree::sequence_place(files, file, member), None)).collect();
            index.insert(Kind::Music, &asset.id, &asset.shown, at);
        }
        index
    }

    /// The index an asset tree's manifest gives.
    pub fn from_manifest(manifest: &Value) -> Result<Index> {
        let mut index = Index::default();
        let empty = serde_json::Map::new();
        for (id, asset) in manifest["assets"].as_object().unwrap_or(&empty) {
            let kind =
                asset["kind"].as_str().and_then(kind_named).ok_or_else(|| Error::invalid(format!("{id}: no kind")))?;
            let places = asset["places"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .map(|place| {
                    let at = place.get("picture").or_else(|| place.get("entry")).and_then(Value::as_u64);
                    Some((Place::from_json(place)?, at.map(|at| at as usize)))
                })
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| Error::invalid(format!("{id}: a place without a path")))?;
            index.assets.insert(
                id.clone(),
                Asset {
                    id: id.clone(),
                    kind,
                    name: asset["name"].as_str().unwrap_or_default().to_string(),
                    places,
                    bank: asset["bank"].as_str().map(str::to_string),
                    entries: asset["sounds"].as_array().map(Vec::len),
                },
            );
        }
        for (alias, id) in manifest["aliases"].as_object().unwrap_or(&empty) {
            index.aliases.insert(alias.clone(), id.as_str().unwrap_or_default().to_string());
        }
        Ok(index)
    }

    fn insert(&mut self, kind: Kind, name: &str, shown: &str, places: Vec<(Place, Option<usize>)>) {
        let id = kind.id(name);
        let asset = Asset { id: id.clone(), kind, name: shown.to_string(), places, bank: None, entries: None };
        self.assets.insert(id, asset);
    }

    /// The asset an id names: by the id, an alias, or either ignoring case (ids differ ignoring
    /// case, as their file names must).
    pub fn get(&self, id: &str) -> Option<&Asset> {
        if let Some(found) = self.assets.get(id).or_else(|| self.aliases.get(id).and_then(|id| self.assets.get(id))) {
            return Some(found);
        }
        let lower = id.to_lowercase();
        let found = self.assets.keys().chain(self.aliases.keys()).find(|known| known.to_lowercase() == lower)?;
        self.get(found)
    }

    /// The assets a pattern matches, by id or alias, in id order.
    pub fn matching(&self, pattern: &str) -> Vec<&Asset> {
        let mut found: BTreeMap<&str, &Asset> = BTreeMap::new();
        for (id, asset) in &self.assets {
            if glob_matches(pattern, id) {
                found.insert(id, asset);
            }
        }
        for (alias, id) in &self.aliases {
            if glob_matches(pattern, alias) {
                if let Some(asset) = self.assets.get(id) {
                    found.insert(id, asset);
                }
            }
        }
        found.into_values().collect()
    }

    /// The id of the asset at a place: a texture by its texture file and picture, a model or bank
    /// by its file, a sound by its bank and entry. Places compare ignoring case.
    pub fn at(&self) -> HashMap<(Kind, String, Option<usize>), String> {
        let mut out = HashMap::new();
        for (id, asset) in &self.assets {
            for (place, at) in &asset.places {
                out.entry((asset.kind, place.display().to_lowercase(), *at)).or_insert_with(|| id.clone());
            }
        }
        out
    }
}

/// The numbers of a bank's entries, as the tree's export reads them; None when it cannot read the
/// bank.
fn bank_entries(data: &[u8]) -> Option<Vec<usize>> {
    let bank = Bank::from_bytes(data).ok()?;
    let entries = bank.entries().ok()?;
    for entry in &entries {
        if entry.kind == EntryKind::KeyOn {
            bank.voices(entry).ok()?;
        }
    }
    Some(entries.iter().map(|entry| entry.index).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::tests::{build_disc, disc};

    #[test]
    fn patterns_match_any_run_and_one_character_ignoring_case() {
        assert!(glob_matches("model:c02fuku*", "model:C02FUKU1t"));
        assert!(glob_matches("texture:e254a?", "texture:e254a1"));
        assert!(!glob_matches("texture:e254a?", "texture:e254a10"));
        assert!(glob_matches("*", ""));
        assert!(glob_matches("a*b*c", "aXbYbZc"));
        assert!(!glob_matches("a*b", "aXbY"));
        assert_eq!(kind_of("sound:SE_X"), Some(Kind::Sound));
        assert_eq!(kind_of("file:a.img"), None);
    }

    #[test]
    fn the_disc_and_the_tree_give_the_same_ids_and_places() {
        let disc = disc();
        let (_, written) = build_disc(&disc);
        let manifest: Value = serde_json::from_slice(&written[tree::MANIFEST]).unwrap();
        let files: Vec<DiscFile> = disc.iter().map(|(path, data)| DiscFile { path: path.clone(), data }).collect();
        let from_disc = Index::from_disc(&files, &tree::sound_names(""));
        assert_eq!(from_disc, Index::from_manifest(&manifest).unwrap());
        assert_eq!(from_disc.get("TEXTURE:Wall@B02").unwrap().id, "texture:wall@b02");
        let ids: Vec<&str> = from_disc.matching("texture:wall*").iter().map(|asset| asset.id.as_str()).collect();
        assert_eq!(ids, ["texture:wall", "texture:wall@b02"]);
        let at = from_disc.at();
        let key = (Kind::Texture, "map/b/b02.ipk.d/x.img".to_string(), Some(1));
        assert_eq!(at[&key], "texture:wall");
    }
}
