//! Maps: the maps the game lists, and what loading one takes of each budget.

use super::*;

/// How the engine loads a map, which decides its budgets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MapKind {
    /// A town or field map (EditMapJump): stack 1, MainDataBuff, with a sky.
    Town,
    /// An interior or a part of Palm Brinks loaded over the town (LoadSubMap): stack 3.
    Interior,
    /// A dungeon floor or story map (LoadDungeonMapFile): stack 1, BuffStageMain, with a sky.
    Dungeon,
}

/// The map numbers EditMapJump (editloop.cpp:2735) loads as a sub map over Palm Brinks (m01,
/// map 10): `map_no > 0xA && map_no < 0xF`, the town's four parts m02 to m05.
pub const PALM_BRINKS_PARTS: std::ops::RangeInclusive<i64> = 11..=14;

impl MapKind {
    /// How the engine loads a row of its map table: types 7 and 8 in the dungeon loop; the
    /// interiors (MapType in mapselect.hpp: 2 a Georama town's, 4, 6 a future map's) and Palm
    /// Brinks' parts as sub maps (LoadSubMap); every other map as the main map (EditMapJump).
    pub fn of_row(row: &crate::maptable::Row) -> MapKind {
        match row.kind {
            7 | 8 => MapKind::Dungeon,
            2 | 4 | 6 => MapKind::Interior,
            _ if PALM_BRINKS_PARTS.contains(&row.number) => MapKind::Interior,
            _ => MapKind::Town,
        }
    }

    pub(super) fn placements(self) -> usize {
        match self {
            MapKind::Dungeon => DUNGEON_PLACEMENTS,
            _ => TOWN_PLACEMENTS,
        }
    }

    pub(super) fn loading_screen(self) -> u64 {
        match self {
            MapKind::Dungeon => DUNGEON_LOADING_SCREEN,
            _ => TOWN_LOADING_SCREEN,
        }
    }

    pub(super) fn retail(self) -> u64 {
        match self {
            MapKind::Town => RETAIL_TOWN_MAP,
            MapKind::Interior => RETAIL_INTERIOR_MAP,
            MapKind::Dungeon => RETAIL_DUNGEON_MAP,
        }
    }

    fn sky(self) -> bool {
        self != MapKind::Interior
    }

    pub fn describe(self) -> &'static str {
        match self {
            MapKind::Town => "town map",
            MapKind::Interior => "interior",
            MapKind::Dungeon => "dungeon map",
        }
    }
}

/// A map the game lists: a row of a map table (maptable::rows).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapEntry {
    pub name: String,
    pub add: String,
    pub kind: MapKind,
}

/// The maps of the game's map tables (map<language>.cfg, which LoadMapName reads; the languages'
/// list the maps alike), first listing of each name wins. The disc's older map/map.cfg is not
/// read: the game never does, and its types differ from the game's for some maps (s19 a town
/// there, i03h02 an interior).
pub fn map_entries(disc: &dyn Disc) -> Vec<MapEntry> {
    let mut out: Vec<MapEntry> = Vec::new();
    for language in crate::maptable::LANGUAGES {
        let Some(data) = disc.read(&crate::startat::map_table_path(Some(language))) else { continue };
        for row in crate::maptable::rows(&data) {
            if row.code.is_empty() || out.iter().any(|entry| entry.name.eq_ignore_ascii_case(&row.code)) {
                continue;
            }
            let kind = MapKind::of_row(&row);
            out.push(MapEntry { name: row.code, add: row.add, kind });
        }
    }
    out
}

/// Where the engine finds a map's files: map/<first letter>/<letters 1-3>/<letters 4-6>/
/// (GetMapPath, mapselect.cpp:197), and an add map under map/cmn/ (GetLoadMapInfo).
pub(super) fn map_folder(name: &str) -> (String, String) {
    (crate::mapedit::rule_folder(name), name.to_lowercase())
}

pub(super) fn add_folder(add: &str) -> (String, String) {
    match add.rsplit_once('/') {
        Some((folder, name)) => (format!("map/cmn/{folder}"), name.to_string()),
        None => ("map/cmn".to_string(), add.to_string()),
    }
}

/// What loading a map uses of each budget.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MapLoad {
    /// Bytes of the read buffer: .map, .cfg, .mpk and .sky of the map and its add map, each
    /// rounded up to 64 bytes (LoadMapData, sceneload.cpp).
    pub read: u64,
    /// Placements of the map and its add map.
    pub placements: usize,
    /// Estimated bytes of the map's memory stack (LoadMapFromMemory): the .ipk and .efp copied
    /// whole, the models, collision, sky and placements built.
    pub stack: u64,
    /// The .ipk and .efp copied, part of `stack`.
    pub copied: u64,
}

/// What loading `entry` uses, from the files on `disc`; None when the map is not on it.
pub fn map_load(disc: &dyn Disc, entry: &MapEntry) -> Option<MapLoad> {
    let mut sources = Vec::new();
    if !entry.add.is_empty() {
        sources.push(add_folder(&entry.add));
    }
    sources.push(map_folder(&entry.name));
    let mut load = MapLoad::default();
    let mut stack = Stack::default();
    // the CEditMap (LoadMapFromMemory step 0) and the placement table (SetPlacePartsBuff)
    stack.alloc(0x111);
    let table = entry.kind.placements() as u64;
    stack.alloc(blocks(table * 0x310) + 2);
    stack.alloc(blocks(table * 4) + 2);
    let main = sources.len() - 1;
    let mut found = false;
    for (index, (folder, name)) in sources.iter().enumerate() {
        let file = |extension: &str| disc.read(&format!("{folder}/{name}.{extension}"));
        let size = |data: &Option<Cow<[u8]>>| data.as_ref().map_or(0, |data| round_up(data.len() as u64, 64));
        let map = file("map");
        found |= map.is_some() && index == main;
        let mpk = file("mpk");
        let ipk = file("ipk");
        let efp = file("efp");
        load.read += size(&map) + size(&file("cfg")) + size(&mpk);
        if index == main && entry.kind.sky() {
            let sky = file("sky").or_else(|| disc.read("map/def.sky"));
            load.read += size(&sky);
            if let Some(sky) = &sky {
                map_sky(&mut stack, sky);
            }
        }
        for data in [&ipk, &efp].into_iter().flatten() {
            stack.align64();
            stack.alloc(blocks(data.len() as u64));
            load.copied += round_up(data.len() as u64, 64);
        }
        let parsed = map.as_ref().and_then(|data| dc2tk_assets::mapfile::parse(data).ok());
        let Some(parsed) = parsed else { continue };
        load.placements += parsed.placements.len();
        map_models(&mut stack, &parsed, mpk.as_deref(), ipk.as_deref());
    }
    if !found {
        return None;
    }
    load.stack = stack.bytes();
    Some(load)
}

/// CMapSky::LoadPack (mapsky, asm): the sky's pictures copied, its meshes built as fixed models.
fn map_sky(stack: &mut Stack, sky: &[u8]) {
    stack.alloc(0x13);
    for (name, data) in members(sky) {
        let lower = name.to_lowercase();
        if lower.ends_with(".img") {
            stack.alloc(blocks(data.len() as u64));
        } else if lower.ends_with(".mds") {
            model(stack, data, &|_| Visual::Fixed, None);
        }
    }
}

/// CMap::LoadData (map.cpp:1506) and the parts pass: each picture file's table entry, each model
/// pack's models and collision, each template and placement.
fn map_models(stack: &mut Stack, map: &dc2tk_assets::mapfile::MapFile, mpk: Option<&[u8]>, ipk: Option<&[u8]>) {
    for name in &map.images {
        if ipk.and_then(|pack| member(pack, name)).is_some() {
            stack.alloc(blocks(name.len() as u64 + 1) + 0x12);
        }
    }
    for name in &map.packs {
        if let Some(pcp) = mpk.and_then(|pack| member(pack, name)) {
            model_pack(stack, name, pcp);
        }
    }
    // a template's CMapParts and its pieces (CList<CMapPiece>, 0xD0, and its PieceMaterial table)
    let piece = |piece: &dc2tk_assets::mapfile::Piece| {
        15 + piece.materials.as_ref().map_or(0, |materials| materials.len() as u64 * 2 + 2)
    };
    for template in &map.templates {
        stack.alloc(blocks(0x310 + 0x20) + 2);
        for one in &template.pieces {
            stack.alloc(piece(one));
        }
    }
    // a placement copies its template's pieces without a collision type (CMapParts::Copy)
    for placement in &map.placements {
        let Some(template) = map.templates.iter().find(|template| template.name == placement.template) else {
            continue;
        };
        for one in &template.pieces {
            let collides =
                one.col_type.as_ref().is_some_and(|col| !matches!(col.kind, dc2tk_assets::mapfile::Num::Int(0)));
            if !collides {
                stack.alloc(piece(one));
            }
        }
    }
}

/// CMdsList::LoadPCPFile (mdslist.cpp:484): the model list, and each MDS the info.cfg names built by
/// its TYPE: 0 a fixed model, 1 and 2 collision, 3 and 4 a character (CreateChara).
fn model_pack(stack: &mut Stack, name: &str, pcp: &[u8]) {
    let all = members(pcp);
    let listed = all.iter().filter(|(member, _)| {
        let lower = member.to_lowercase();
        lower.ends_with(".mds") || lower.ends_with(".chr")
    });
    stack.alloc(blocks(name.len() as u64 + 1));
    stack.alloc(blocks(listed.count() as u64 * 0x20) + 2);
    let Some(info) = member(pcp, "info.cfg") else { return };
    let mut current: Option<String> = None;
    let mut kind = 0;
    for (tag, args) in info_cfg::parse(info) {
        match tag.as_str() {
            "MDS" => {
                current = text_arg(&args, 0);
                kind = 0;
                if let Some(name) = &current {
                    stack.alloc(blocks(name.len() as u64 + 1));
                }
            }
            "TYPE" => kind = int_arg(&args, 0).unwrap_or(0),
            "MDS_END" => {
                let Some(data) = current.take().and_then(|name| member(pcp, &name)) else { continue };
                match kind {
                    1 | 2 => {
                        collision(stack, data);
                    }
                    3 | 4 => {
                        stack.alloc(0x68);
                        chara_pack(stack, data);
                    }
                    _ => {
                        model(stack, data, &|_| Visual::Fixed, None);
                    }
                }
            }
            _ => {}
        }
    }
}
