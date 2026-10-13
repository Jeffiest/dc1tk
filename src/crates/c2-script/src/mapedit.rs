//! Maps a mod edits as glTF scenes (`"map:s17": "maps/s17/s17.gltf"` in mod.json; see
//! docs/assets/maps.md sections 7 and 8): where a map's files are on the disc.

use c2_disc::archive::Entry;

use crate::{Error, Result};

/// A map's files on the disc, by DATA.DAT path: the scene, its model packs and texture packs, and
/// the optional effect pack, sky and extra data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapFiles {
    pub name: String,
    pub folder: String,
    pub map: String,
    pub mpk: Option<String>,
    pub ipk: Option<String>,
    pub efp: Option<String>,
    pub sky: Option<String>,
    pub cfg: Option<String>,
}

/// Where the engine looks for a map: map/<first letter>/<letters 1-3>/<letters 4-6>/<name>.map
/// (GetMapPath, mapselect.cpp:197), so map/s/s17/s17.map and map/i/i03/h02/i03h02.map.
pub fn rule_folder(name: &str) -> String {
    let lower = name.to_lowercase();
    let chars: Vec<char> = lower.chars().collect();
    let part = |from: usize, to: usize| chars.get(from..to.min(chars.len())).map(|slice| slice.iter().collect::<String>()).unwrap_or_default();
    let mut folder = format!("map/{}/{}", part(0, 1), part(0, 3));
    let rest = part(3, 6);
    if !rest.is_empty() {
        folder.push('/');
        folder.push_str(&rest);
    }
    folder
}

/// A map's files by its name: the engine's own path first; a map only found elsewhere (six sit
/// where the rule never reaches) when exactly one .map of that name is on the disc.
pub fn map_files(entries: &[Entry], name: &str) -> Result<MapFiles> {
    let lower = |text: &str| text.to_lowercase();
    let has = |path: &str| entries.iter().find(|entry| lower(&entry.path) == lower(path)).map(|entry| entry.path.clone());
    let file_name = format!("{}.map", lower(name));
    let ruled = format!("{}/{}", rule_folder(name), file_name);
    let map = match has(&ruled) {
        Some(path) => path,
        None => {
            let found: Vec<&Entry> =
                entries.iter().filter(|entry| lower(&entry.path).rsplit('/').next() == Some(file_name.as_str())).collect();
            match found.as_slice() {
                [one] => one.path.clone(),
                [] => return Err(Error::invalid(format!("the disc has no map {name}"))),
                many => {
                    let paths: Vec<&str> = many.iter().map(|entry| entry.path.as_str()).collect();
                    return Err(Error::invalid(format!("map {name} is at more than one place: {}", paths.join(", "))));
                }
            }
        }
    };
    let folder = map.rsplit_once('/').map_or(String::new(), |(folder, _)| folder.to_string());
    let stem = &map[folder.len() + 1..map.len() - ".map".len()];
    let beside = |suffix: &str| has(&format!("{folder}/{stem}{suffix}"));
    Ok(MapFiles {
        name: stem.to_string(),
        mpk: beside(".mpk"),
        ipk: beside(".ipk"),
        efp: beside(".efp"),
        sky: beside(".sky"),
        cfg: beside(".cfg"),
        folder,
        map,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str) -> Entry {
        Entry { path: path.into(), size: 1, sector: 0 }
    }

    #[test]
    fn maps_are_found_by_the_engine_rule_then_by_name() {
        assert_eq!(rule_folder("s17"), "map/s/s17");
        assert_eq!(rule_folder("i03h02"), "map/i/i03/h02");
        let entries = [
            entry("map/s/s17/s17.map"),
            entry("map/s/s17/s17.mpk"),
            entry("map/s/s17/s17.ipk"),
            entry("map/s/s38/i03h02.map"),
            entry("map/s/s43/i03h02.map"),
            entry("map/i/i03/h02/i03h02.map"),
            entry("map/d/d06/b01/m07.map"),
        ];
        let s17 = map_files(&entries, "S17").unwrap();
        assert_eq!((s17.map.as_str(), s17.mpk.as_deref(), s17.efp.as_deref()), ("map/s/s17/s17.map", Some("map/s/s17/s17.mpk"), None));
        assert_eq!(map_files(&entries, "i03h02").unwrap().map, "map/i/i03/h02/i03h02.map");
        assert_eq!(map_files(&entries, "m07").unwrap().folder, "map/d/d06/b01");
        assert!(map_files(&entries, "x99").is_err());
    }
}
