//! What a map shows besides its placements, for a scene to show and never to build back: its sky
//! (the `.sky` pack, CMapSky::LoadPack, mapsky.cpp) and what its `.cfg` adds (CMap::LoadCfgFile,
//! mapload.cpp:2538): water surfaces, occlusion planes, areas drawn without their parts, the extra
//! trigger points, and where villagers stand. The scene carries them under nodes of role
//! `display` and in the map root's `cfg`, which the import leaves alone.

use serde_json::{json, Value};

use crate::info_cfg::{self, Arg};

/// The time bands a sky has a model of each (MAP_TIME_BAND: day, evening, night, morning).
pub const BANDS: [&str; 4] = ["day", "evening", "night", "morning"];

/// One time band of a sky pack's info.cfg: its picture file, sky model and background sky model
/// (each turned about Y by its speed, degrees a frame) and its sun or moon.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SkyBand {
    pub img: Option<String>,
    pub sky: Option<(String, f64)>,
    pub skyb: Option<(String, f64)>,
    pub sun: Option<String>,
}

fn number(args: &[Arg], at: usize) -> f64 {
    args.get(at).and_then(Arg::as_f64).unwrap_or(0.0)
}

fn text(args: &[Arg], at: usize) -> Option<String> {
    args.get(at).and_then(Arg::as_str).filter(|name| !name.is_empty()).map(str::to_string)
}

/// The sky pack's four bands, as its info.cfg names them (SKY_IMG, SKY_MDS, SKYB_MDS, SUN_MDS),
/// and its shared background model (SKY_BG).
pub fn sky_bands(info: &[u8]) -> ([SkyBand; 4], Option<String>) {
    let mut bands: [SkyBand; 4] = Default::default();
    let mut background = None;
    for (tag, args) in info_cfg::parse(info) {
        if tag == "SKY_BG" {
            background = text(&args, 0);
            continue;
        }
        let band = number(&args, 0);
        // CheckSkyID: bands 0 to 3
        if !(0.0..4.0).contains(&band) {
            continue;
        }
        let entry = &mut bands[band as usize];
        match tag.as_str() {
            "SKY_IMG" => entry.img = text(&args, 1),
            "SKY_MDS" => entry.sky = text(&args, 1).map(|name| (name, number(&args, 2))),
            "SKYB_MDS" => entry.skyb = text(&args, 1).map(|name| (name, number(&args, 2))),
            "SUN_MDS" => entry.sun = text(&args, 1),
            _ => {}
        }
    }
    (bands, background)
}

fn floats(args: &[Arg], from: usize, count: usize) -> Vec<f64> {
    (from..from + count).map(|at| number(args, at)).collect()
}

/// What a map's `.cfg` adds, as JSON for the scene's eye: `water` surfaces (a grid between two
/// corners, put at a position; WATER_VERTEX, WATER_POS), `occlusion` planes (four corners),
/// `draw_off` areas (DRAW_OFF_RECT: the parts in the second box are not drawn while the camera
/// is in the first), `events` (FUNC_DATA "event" points: name, event number, how they go off, and
/// FUNC_POS's position, rotation and size) and `villagers` (NPC_PLACE name, NPC_POS x, y, z, yaw).
pub fn cfg_json(data: &[u8]) -> Value {
    let mut water: Vec<Value> = Vec::new();
    let mut occlusion: Vec<Value> = Vec::new();
    let mut draw_off: Vec<Value> = Vec::new();
    let mut events: Vec<Value> = Vec::new();
    let mut villagers: Vec<Value> = Vec::new();
    let mut event: Option<Value> = None;
    let mut villager: Option<String> = None;
    for (tag, args) in info_cfg::parse(data) {
        match tag.as_str() {
            "WATER_VERTEX" => water.push(json!({
                "grid": [number(&args, 0), number(&args, 1)],
                "corner0": floats(&args, 2, 3),
                "corner1": floats(&args, 5, 3),
                "pos": [0.0, 0.0, 0.0],
            })),
            "WATER_POS" => {
                if let Some(last) = water.last_mut() {
                    last["pos"] = json!(floats(&args, 0, 3));
                }
            }
            "OCCLUSION_PLANE" => {
                occlusion.push(json!((0..4).map(|corner| floats(&args, corner * 3, 3)).collect::<Vec<_>>()))
            }
            "DRAW_OFF_RECT" => draw_off.push(json!({
                "camera": [floats(&args, 0, 3), floats(&args, 3, 3)],
                "hidden": [floats(&args, 6, 3), floats(&args, 9, 3)],
            })),
            "FUNC_DATA" => {
                // FUNC_DATA ends one point (the tag table's second FUNC_DATA is FUNC_DATA_END)
                if let Some(done) = event.take() {
                    events.push(done);
                }
                if text(&args, 0).as_deref() == Some("event") {
                    event = Some(json!({"name": "", "event": 0, "mode": "", "pos": vec![0.0; 9]}));
                }
            }
            "FUNC_NAME" => {
                if let Some(found) = event.as_mut() {
                    found["name"] = json!(text(&args, 0).unwrap_or_default());
                }
            }
            "FUNC_EVENT_DATA" => {
                if let Some(found) = event.as_mut() {
                    found["event"] = json!(number(&args, 0));
                    found["mode"] = json!(text(&args, 1).unwrap_or_default());
                }
            }
            "FUNC_POS" => {
                if let Some(found) = event.as_mut() {
                    found["pos"] = json!(floats(&args, 0, 9));
                }
            }
            "FUNC_DATA_END" => {
                if let Some(done) = event.take() {
                    events.push(done);
                }
            }
            "NPC_PLACE" => villager = text(&args, 0),
            "NPC_POS" => villagers.push(json!({
                "name": villager.clone().unwrap_or_default(),
                "pos": floats(&args, 0, 3),
                "yaw": number(&args, 3),
            })),
            "NPC_PLACE_END" => villager = None,
            _ => {}
        }
    }
    if let Some(done) = event.take() {
        events.push(done);
    }
    json!({"water": water, "occlusion": occlusion, "draw_off": draw_off, "events": events, "villagers": villagers})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sky_packs_bands_come_from_its_info_cfg() {
        let info = b"SKY_IMG 0,\"sky0.img\";\nSKY_MDS 0,\"sky0.mds\",0.01;\nSKYB_MDS 0,\"skyb0.mds\",0.02;\n\
                     SUN_MDS 2,\"moon.mds\";\nSKY_IMG 9,\"no.img\";\nSKY_BG \"bg.mds\";\n";
        let (bands, background) = sky_bands(info);
        assert_eq!(bands[0].img.as_deref(), Some("sky0.img"));
        assert_eq!(bands[0].sky, Some(("sky0.mds".to_string(), 0.01)));
        assert_eq!(bands[0].skyb, Some(("skyb0.mds".to_string(), 0.02)));
        assert_eq!(bands[2].sun.as_deref(), Some("moon.mds"));
        assert_eq!(bands[1], SkyBand::default());
        assert_eq!(background.as_deref(), Some("bg.mds"));
    }

    #[test]
    fn a_maps_cfg_gives_its_water_planes_areas_points_and_villagers() {
        let cfg = b"DRAW_OFF_RECT 500,-10000,-2700,3000,10000,-800, -3000,-10000,-3000,1000,10000,4000;\n\
                    OCCLUSION_PLANE 0,1000,-1600, 1500,1000,-600, 1500,-1000,-600, 0,-1000,-1600;\n\
                    NPC_PLACE \"baker\";\n\tNPC_POS 153,4.1,-161, 0;\nNPC_PLACE_END;\n\
                    FUNC_DATA \"event\";\n\tFUNC_NAME \"event_550\";\n\tFUNC_EVENT_DATA 550,\"every\";\n\
                    \tFUNC_POS 1300.0, 0.0, 1200.0, 0.0, 0.0, 0.0, 160,160,160;\nFUNC_DATA_END;\n\
                    WATER_SURFACE_NUM 1;\nWATER_SURFACE_START;\nWATER_VERTEX 4,4,-100,0,-100,100,0,100;\n\
                    WATER_POS 10,5,20;\nWATER_SURFACE_END;\n";
        let found = cfg_json(cfg);
        assert_eq!(found["draw_off"][0]["camera"][0], json!([500.0, -10000.0, -2700.0]));
        assert_eq!(found["occlusion"][0][1], json!([1500.0, 1000.0, -600.0]));
        assert_eq!(found["villagers"][0], json!({"name": "baker", "pos": [153.0, 4.1, -161.0], "yaw": 0.0}));
        assert_eq!(found["events"][0]["name"], "event_550");
        assert_eq!(found["events"][0]["mode"], "every");
        assert_eq!(found["events"][0]["pos"][6], 160.0);
        assert_eq!(found["water"][0]["corner1"], json!([100.0, 0.0, 100.0]));
        assert_eq!(found["water"][0]["pos"], json!([10.0, 5.0, 20.0]));
    }
}
