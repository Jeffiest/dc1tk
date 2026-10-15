//! Villager slots: which villager the engine loads into each character slot of a map, by story
//! chapter and time of day, read from the disc's villager tables as CScene::LoadVillager and
//! CVillagerMngr::GetAppearVlgr use them (scenevillager.cpp, villagermngr.cpp).
//!
//! A map's villagers are the villager ids, in ascending order, whose schedule in npc_place4.cfg
//! puts them at a place (place.cfg) on the map for the current chapter (PROG_INFO) and time (night
//! is 21:00 to 06:00); a villager with places in several alternatives takes several slots, and one
//! without a model in npc.cfg takes none. The engine loads them from slot 8 for the map it jumps
//! to and from slot 24 for an interior (a map of type 4 in map/map1.cfg), as LoadSubVillager does.
//! So a slot holds the villager at its position in that list: the same one in every chapter on a
//! map with few villagers (Cedric in his shop), different ones where the list changes.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use regex::Regex;

use crate::maptable::{int_arg, quoted_bytes};
use crate::messages::{NameKey, NameTable};
use crate::{maptable, startat};

/// Main villager slots start at 8 (16 of them), an interior's at 24 (8).
const MAIN_TOP: i64 = 8;
const MAIN_COUNT: usize = 16;
const SUB_TOP: i64 = 24;
const SUB_COUNT: usize = 8;
/// The demo's progress (体験版), which the retail game never reaches.
const DEMO_PROGRESS: i64 = 100;

struct Point {
    progress: i64,
    after: bool,
    /// (alternative, time 0 noon 1 night) → place
    places: BTreeMap<(i64, i64), i64>,
}

/// The disc's villager tables.
pub struct Tables {
    /// progress id → order
    progress: BTreeMap<i64, i64>,
    /// villager id → schedule points in file order
    schedules: BTreeMap<i64, Vec<Point>>,
    /// place id → map number
    places: BTreeMap<i64, i64>,
    /// villager ids with a model
    models: BTreeSet<i64>,
    /// map number → (code, type)
    pub maps: BTreeMap<i64, (String, i64)>,
}

/// A character of the NPC table (npc.cfg, npc<language>.cfg) as _NPC_INFO (npccfg.cpp) reads
/// it: `NPC_INFO id,"name","pack",...;`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Npc {
    /// The character's number; None when the row gives no integer.
    pub number: Option<i64>,
    /// The name, in the bytes of the table's language.
    pub name: Vec<u8>,
    /// The character pack's name ("p01_03a"), empty for a character without a model.
    pub pack: String,
}

/// The NPC_INFO rows of an NPC table, read with CScriptInterpreter's rules (comments skipped).
pub fn npc_info(cfg: &[u8]) -> Vec<Npc> {
    dc2tk_assets::info_cfg::statements(cfg)
        .iter()
        .filter(|statement| statement.tag == "NPC_INFO")
        .map(|statement| {
            let string = |at: usize| statement.args.get(at).and_then(|arg| quoted_bytes(cfg, arg)).unwrap_or_default();
            Npc {
                number: int_arg(statement.args.first()),
                name: string(1).to_vec(),
                pack: dc2tk_disc::text::decode_lossy(string(2)),
            }
        })
        .collect()
}

fn read_text(root: &Path, name: &str) -> Option<String> {
    std::fs::read(root.join(name)).ok().map(|data| dc2tk_disc::text::decode_lossy(&data))
}

fn without_comment(line: &str) -> &str {
    line.split("//").next().unwrap_or("").trim()
}

impl Tables {
    /// The tables of an extracted disc, or None when it lacks them.
    pub fn read(root: &Path) -> Option<Tables> {
        let schedule = read_text(root, "npc_place4.cfg")?;
        let places_text = read_text(root, "place.cfg")?;
        let models_data = std::fs::read(root.join("npc.cfg")).ok()?;
        let maps_data = std::fs::read(root.join(startat::map_table_path(Some(1)))).ok()?;
        let number = |text: &str| text.trim().parse::<i64>().ok();
        let mut progress = BTreeMap::new();
        let mut schedules: BTreeMap<i64, Vec<Point>> = BTreeMap::new();
        let mut villager = None;
        let mut alternative = 0;
        let progress_line = Regex::new(r#"^PROGRESS\s+(\d+)\s*,\s*(\d+)\s*,\s*"(.*?)""#).unwrap();
        let place_line = Regex::new(r"^(NOON|NIGHT)_PLACE\s+(\d+)").unwrap();
        for line in schedule.lines().map(without_comment) {
            let fields: Vec<&str> = line.splitn(2, char::is_whitespace).collect();
            match fields[0] {
                "PROG_INFO" => {
                    let values: Vec<i64> = fields[1].split(',').take(4).filter_map(number).collect();
                    if let [id, _, _, order] = values[..] {
                        progress.insert(id, order);
                    }
                }
                "NPC" => {
                    villager = fields.get(1).and_then(|text| number(text.trim_end_matches(';')));
                    if let Some(villager) = villager {
                        schedules.entry(villager).or_default();
                    }
                }
                _ => {
                    let Some(points) = villager.and_then(|villager| schedules.get_mut(&villager)) else {
                        continue;
                    };
                    if let Some(found) = progress_line.captures(line) {
                        let id: i64 = found[1].parse().unwrap_or(0);
                        alternative = found[2].parse().unwrap_or(0);
                        if points.last().is_none_or(|point| point.progress != id) {
                            points.push(Point { progress: id, after: &found[3] == "以後", places: BTreeMap::new() });
                        }
                    } else if let Some(found) = place_line.captures(line) {
                        let time = if &found[1] == "NOON" { 0 } else { 1 };
                        if let (Some(point), Ok(place)) = (points.last_mut(), found[2].parse()) {
                            point.places.insert((alternative, time), place);
                        }
                    }
                }
            }
        }
        let places = Regex::new(r"(?m)^NPC_PLACE\s+(\d+)\s*,\s*(-?\d+)")
            .unwrap()
            .captures_iter(&places_text)
            .filter_map(|found| Some((found[1].parse().ok()?, found[2].parse().ok()?)))
            .collect();
        let models = npc_info(&models_data)
            .into_iter()
            .filter(|npc| !npc.pack.is_empty())
            .filter_map(|npc| npc.number)
            .collect();
        // a map's number is its row, the empty rows counted
        let maps = maptable::rows(&maps_data)
            .into_iter()
            .filter(|row| !row.code.is_empty())
            .map(|row| (row.number, (row.code, row.kind)))
            .collect();
        Some(Tables { progress, schedules, places, models, maps })
    }

    /// The villagers a map has in a chapter and time, in slot order (GetAppearVlgr).
    fn appear(&self, progress: i64, time: i64, map: i64) -> Vec<i64> {
        let (progress, time) = if progress < 2 { (1, 0) } else { (progress, time) };
        let Some(&order) = self.progress.get(&progress) else {
            return Vec::new();
        };
        let mut found = Vec::new();
        for (&villager, points) in &self.schedules {
            let mut handled = false;
            for point in points.iter().rev() {
                match self.progress.get(&point.progress) {
                    Some(&at) if order >= at => {}
                    _ => continue,
                }
                if point.progress == progress || point.after {
                    for alternative in 0..4 {
                        if let Some(place) = point.places.get(&(alternative, time)) {
                            handled = true;
                            if self.places.get(place) == Some(&map) {
                                found.push(villager);
                            }
                        }
                    }
                    if handled {
                        break;
                    }
                }
            }
            if let (false, Some(first)) = (handled, points.first().filter(|first| first.progress == 1)) {
                for alternative in 0..4 {
                    if let Some(place) = first.places.get(&(alternative, time)) {
                        if self.places.get(place) == Some(&map) {
                            found.push(villager);
                        }
                    }
                }
            }
        }
        // a villager whose model is not in npc.cfg loads into no slot
        found.into_iter().filter(|villager| self.models.contains(villager)).collect()
    }

    /// {map code: {slot: villager ids}}: every villager each slot of a map can hold, over the
    /// chapters and both times of day.
    pub fn slots(&self) -> BTreeMap<String, BTreeMap<i64, BTreeSet<i64>>> {
        let mut out = BTreeMap::new();
        for (&map, (code, kind)) in &self.maps {
            let (top, count) = if *kind == 4 { (SUB_TOP, SUB_COUNT) } else { (MAIN_TOP, MAIN_COUNT) };
            let mut slots: BTreeMap<i64, BTreeSet<i64>> = BTreeMap::new();
            for &progress in self.progress.keys().filter(|&&progress| progress != DEMO_PROGRESS) {
                for time in [0, 1] {
                    for (position, villager) in self.appear(progress, time, map).into_iter().take(count).enumerate() {
                        slots.entry(top + position as i64).or_default().insert(villager);
                    }
                }
            }
            if !slots.is_empty() {
                out.insert(code.clone(), slots);
            }
        }
        out
    }

    /// The map a script runs on, by its file name: i01h01_1.stb is i01h01; a variant that is not a
    /// map of its own runs on the map it extends (i01h01b on i01h01).
    pub fn map_of(&self, path: &str) -> Option<String> {
        let name = path.rsplit('/').next()?.split('.').next()?;
        let stem = Regex::new(r"_\d$").unwrap().replace(name, "").to_string();
        let codes: BTreeSet<&str> = self.maps.values().map(|(code, _)| code.as_str()).collect();
        if codes.contains(stem.as_str()) {
            return Some(stem);
        }
        let shorter = stem.strip_suffix(|c: char| c.is_ascii_lowercase())?;
        codes.contains(shorter).then(|| shorter.to_string())
    }
}

/// A villager's name for a constant or a note: npc<language>.cfg's, or its number.
pub fn villager_name(villager: i64, npc_names: &NameTable) -> String {
    npc_names
        .get(&NameKey::Int(villager))
        .filter(|name| !name.is_empty())
        .cloned()
        .unwrap_or_else(|| format!("villager {villager}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npc_rows_are_read_with_the_engines_rules() {
        let table = b"NPC_NUM 3;\r\nNPC_INFO  1 ,\"Cedric\",\"p01_03a\",2,3,64,12,18,64,0,1;\r\n\
// NPC_INFO  2 ,\"Old\",\"p01_06a\",1,0,0,0,0,0,4,1;\r\nNPC_INFO  51 ,\"\x83jA\",\"\",0;\r\n";
        assert_eq!(
            npc_info(table),
            [
                Npc { number: Some(1), name: b"Cedric".to_vec(), pack: "p01_03a".into() },
                Npc { number: Some(51), name: b"\x83jA".to_vec(), pack: String::new() },
            ]
        );
    }
}
