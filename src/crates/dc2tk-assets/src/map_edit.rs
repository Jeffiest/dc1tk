//! A map edited as a scene, back to a .map tree (docs/assets/maps.md section 8). In the glTF scene
//! every placement holds its own copy of its template's pieces and points; here those copies are
//! folded back into templates:
//!
//! - placements whose copy is still the template's keep it;
//! - when the copies changed and agree, the template changes (every placement of it);
//! - placements whose copy differs from the others get a template of their own, a copy named after
//!   the original (scripts name placements, pieces and points, never templates).
//!
//! Placements keep their slots, since scripts can address them by number: an edited one is matched
//! by the slot it came from, a new one is appended, and one deleted from the scene stays with its
//! show flag 0. A template library's previews (one per template, map_scene) fold into their
//! templates the same way and are never placed.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::mapfile::{FixCamera, MapFile, Piece, Placement, Point, Settings, Template};
use crate::{Error, Result};

/// The longest name the engine keeps (31 characters and a terminator).
pub const MAX_NAME: usize = 31;
/// Placements a map can have (400 in dungeons; the largest map on the disc has 245).
pub const MAX_PLACEMENTS: usize = 320;

/// One placement as the scene has it: the placement and its own copy of its template's contents.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlacementEdit {
    /// The `MAP_PARTS` slot it came from; None for a new placement.
    pub slot: Option<usize>,
    pub placement: Placement,
    pub pieces: Vec<Piece>,
    pub points: Option<Vec<Point>>,
    /// A template library's preview of one of its templates (map_scene's `preview`): its contents
    /// are the template's, but it places nothing.
    #[serde(default)]
    pub preview: bool,
}

/// A map as the scene has it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SceneEdit {
    pub settings: Settings,
    pub cameras: Option<Vec<FixCamera>>,
    pub placements: Vec<PlacementEdit>,
    /// The map's own points.
    pub points: Option<Vec<Point>>,
}

/// The template a placement places: the first of its name, as the engine finds it.
fn template_index(map: &MapFile) -> HashMap<&str, usize> {
    let mut found = HashMap::new();
    for (at, template) in map.templates.iter().enumerate() {
        found.entry(template.name.as_str()).or_insert(at);
    }
    found
}

/// The scene of an unedited map: what the exporter writes and what `rebuild` turns back into the
/// same map.
pub fn scene_of(map: &MapFile) -> SceneEdit {
    let templates = template_index(map);
    let placements = map
        .placements
        .iter()
        .enumerate()
        .map(|(slot, placement)| {
            let template = templates.get(placement.template.as_str()).map(|&at| &map.templates[at]);
            PlacementEdit {
                slot: Some(slot),
                placement: placement.clone(),
                pieces: template.map(|template| template.pieces.clone()).unwrap_or_default(),
                points: template.and_then(|template| template.points.clone()),
                preview: false,
            }
        })
        .collect();
    SceneEdit { settings: map.settings.clone(), cameras: map.cameras.clone(), placements, points: map.points.clone() }
}

fn check_name(what: &str, name: &str) -> Result<()> {
    if name.chars().count() > MAX_NAME {
        return Err(Error::invalid(format!("{what} name {name} is longer than {MAX_NAME} characters")));
    }
    Ok(())
}

/// A template name no template has: the original's with _2, _3 and so on, cut to fit.
fn fresh_name(base: &str, taken: &mut HashSet<String>) -> String {
    for number in 2.. {
        let suffix = format!("_{number}");
        let keep: String = base.chars().take(MAX_NAME - suffix.len()).collect();
        let name = format!("{keep}{suffix}");
        if taken.insert(name.to_lowercase()) {
            return name;
        }
    }
    unreachable!()
}

/// Every name the scene gives (placements, pieces, points) is one the engine keeps whole.
fn check_names(scene: &SceneEdit) -> Result<()> {
    for point in scene.points.iter().flatten() {
        check_name("point", &point.name)?;
    }
    for edit in &scene.placements {
        check_name("placement", &edit.placement.name)?;
        for piece in &edit.pieces {
            check_name("piece", &piece.name)?;
        }
        for point in edit.points.iter().flatten() {
            check_name("point", &point.name)?;
        }
    }
    Ok(())
}

/// The placements of each template, by the scene's numbers, in scene order.
fn placements_by_template(scene: &SceneEdit) -> Vec<(String, Vec<usize>)> {
    let mut by_template: Vec<(String, Vec<usize>)> = Vec::new();
    for (at, edit) in scene.placements.iter().enumerate() {
        let name = &edit.placement.template;
        match by_template.iter_mut().find(|(template, _)| template == name) {
            Some((_, list)) => list.push(at),
            None => by_template.push((name.clone(), vec![at])),
        }
    }
    by_template
}

/// A map being rebuilt from its scene: the map so far, the template each placement ends up
/// placing, and the template names taken.
struct Rebuild<'a> {
    original: &'a MapFile,
    scene: &'a SceneEdit,
    out: MapFile,
    placed: Vec<String>,
    taken: HashSet<String>,
}

impl Rebuild<'_> {
    /// The placements of template `name` (`list`) folded back into templates: placements with the
    /// same contents share one; the group still the same as the template (else the first) keeps
    /// it, and every other group gets a copy named after it.
    fn fold(&mut self, name: &str, list: &[usize], original_at: Option<usize>) -> Result<()> {
        let scene = self.scene;
        // a placement's copy of its template's contents
        let contents = |at: usize| (&scene.placements[at].pieces, &scene.placements[at].points);
        // placements with the same contents, in first-seen order
        let mut groups: Vec<Vec<usize>> = Vec::new();
        for &at in list {
            match groups.iter_mut().find(|group| contents(group[0]) == contents(at)) {
                Some(group) => group.push(at),
                None => groups.push(vec![at]),
            }
        }
        // a template the map does not define (an add map's, or a name nothing defines) stays
        // where it is unless the scene gives its placements something to place
        if original_at.is_none() && groups.iter().all(|group| contents(group[0]) == (&Vec::new(), &None)) {
            return Ok(());
        }
        let base = match original_at {
            Some(index) => self.out.templates[index].clone(),
            None => {
                // a template another map defines (the add map's, which the engine finds first)
                // cannot be changed from here: its edited copy gets a name of its own
                let external = self.original.placements.iter().any(|placement| placement.template == name);
                let own = if external { fresh_name(name, &mut self.taken) } else { name.to_string() };
                check_name("template", &own)?;
                self.taken.insert(own.to_lowercase());
                for &at in list {
                    self.placed[at] = own.clone();
                }
                Template {
                    name: own,
                    far_clip: None,
                    light_flag: [0.into(), 0.into()],
                    move_flag: Some([0.into(); 4]),
                    pieces: Vec::new(),
                    points: None,
                }
            }
        };
        // the group the template itself keeps: the one still the same, else the first
        let unchanged =
            groups.iter().position(|group| original_at.is_some() && contents(group[0]) == (&base.pieces, &base.points));
        let keeper = unchanged.unwrap_or(0);
        for (number, group) in groups.iter().enumerate() {
            let (pieces, points) = contents(group[0]);
            let template = Template { pieces: pieces.clone(), points: points.clone(), ..base.clone() };
            if number == keeper {
                match original_at {
                    Some(index) => self.out.templates[index] = template,
                    None => self.out.templates.push(template),
                }
                continue;
            }
            let fork = fresh_name(name, &mut self.taken);
            self.out.templates.push(Template { name: fork.clone(), ..template });
            for &at in group {
                self.placed[at] = fork.clone();
            }
        }
        Ok(())
    }

    /// The placements: edited ones in their slots, new ones appended, missing ones hidden.
    fn place(&mut self) -> Result<()> {
        let (original, scene) = (self.original, self.scene);
        let mut seen = vec![false; original.placements.len()];
        let mut appended = Vec::new();
        for (at, edit) in scene.placements.iter().enumerate() {
            if edit.preview {
                continue;
            }
            let placement = Placement { template: self.placed[at].clone(), ..edit.placement.clone() };
            match edit.slot {
                Some(slot) if slot < seen.len() => {
                    if seen[slot] {
                        return Err(Error::invalid(format!(
                            "two placements of the scene came from slot {slot} ({}); give a copy no slot",
                            original.placements[slot].name
                        )));
                    }
                    seen[slot] = true;
                    self.out.placements[slot] = placement;
                }
                _ => appended.push(placement),
            }
        }
        for (slot, kept) in seen.iter().enumerate() {
            if !kept {
                self.out.placements[slot].show = 0.into();
            }
        }
        self.out.placements.extend(appended);
        Ok(())
    }
}

/// The rebuilt map's placements: each new name given once (scripts find placements by name), and
/// no more than a map has room for.
fn check_placements(out: &MapFile, original: &MapFile) -> Result<()> {
    let mut names = HashSet::new();
    for placement in &out.placements {
        if !names.insert(placement.name.to_lowercase())
            && !original.placements.iter().any(|old| old.name == placement.name)
        {
            return Err(Error::invalid(format!("two placements are named {}; names find them", placement.name)));
        }
    }
    if out.placements.len() > MAX_PLACEMENTS {
        return Err(Error::invalid(format!(
            "{} placements; a map has room for {MAX_PLACEMENTS} (400 in dungeons)",
            out.placements.len()
        )));
    }
    Ok(())
}

/// The map the scene describes, built on the original: templates folded back from the
/// placements' copies, placements kept in their slots, the map's own points, settings and cameras
/// from the scene.
pub fn rebuild(original: &MapFile, scene: &SceneEdit) -> Result<MapFile> {
    check_names(scene)?;
    let mut out = original.clone();
    out.settings = scene.settings.clone();
    out.cameras = scene.cameras.clone();
    out.points = scene.points.clone();
    let taken = original.templates.iter().map(|template| template.name.to_lowercase()).collect();
    let placed = scene.placements.iter().map(|edit| edit.placement.template.clone()).collect();
    let mut rebuild = Rebuild { original, scene, out, placed, taken };
    let first = template_index(original);
    for (name, list) in placements_by_template(scene) {
        rebuild.fold(&name, &list, first.get(name.as_str()).copied())?;
    }
    rebuild.place()?;
    check_placements(&rebuild.out, original)?;
    Ok(rebuild.out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapfile::{self, Num};

    fn sample() -> MapFile {
        // one template placed twice
        let mut map = MapFile::default();
        map.templates.push(Template {
            name: "door".into(),
            far_clip: None,
            light_flag: [0.into(), 0.into()],
            move_flag: Some([1.into(); 4]),
            pieces: vec![],
            points: None,
        });
        let piece = Piece {
            model: "door-m.mds".into(),
            show: 1.into(),
            name: "door-m".into(),
            pos: [0.into(); 3],
            rot: [0.into(); 3],
            scale: [1.into(); 3],
            materials: None,
            col_type: None,
            time: [0.into(); 2],
        };
        map.templates[0].pieces = vec![piece];
        let place = |name: &str, x: i32| Placement {
            name: name.into(),
            show: 1.into(),
            far_clip: None,
            template: "door".into(),
            group: String::new(),
            pos: [x.into(), 0.into(), 0.into()],
            rot: [0.into(); 3],
            scale: [1.into(); 3],
        };
        map.placements = vec![place("door_a", 0), place("door_b", 100)];
        map
    }

    #[test]
    fn an_unedited_scene_rebuilds_the_same_map() {
        let map = sample();
        assert_eq!(rebuild(&map, &scene_of(&map)).unwrap(), map);
    }

    #[test]
    fn a_piece_moved_in_every_placement_moves_in_the_template() {
        let map = sample();
        let mut scene = scene_of(&map);
        for edit in &mut scene.placements {
            edit.pieces[0].pos[1] = Num::from(5.5f32);
        }
        let out = rebuild(&map, &scene).unwrap();
        assert_eq!(out.templates.len(), 1);
        assert_eq!(out.templates[0].pieces[0].pos[1], Num::from(5.5f32));
    }

    #[test]
    fn a_piece_moved_in_one_placement_gives_it_a_template_of_its_own() {
        let map = sample();
        let mut scene = scene_of(&map);
        scene.placements[1].pieces[0].rot[1] = Num::from(std::f32::consts::FRAC_PI_2);
        let out = rebuild(&map, &scene).unwrap();
        assert_eq!(out.templates.len(), 2);
        assert_eq!(out.templates[0], map.templates[0]);
        assert_eq!(out.templates[1].name, "door_2");
        assert_eq!(out.placements[0].template, "door");
        assert_eq!(out.placements[1].template, "door_2");
        // and it still writes and parses
        let text = mapfile::write(&out).unwrap();
        assert_eq!(mapfile::parse(&text).unwrap(), out);
    }

    #[test]
    fn placements_are_appended_and_deleted_ones_hidden() {
        let map = sample();
        let mut scene = scene_of(&map);
        let mut new = scene.placements[0].clone();
        new.slot = None;
        new.placement.name = "door_c".into();
        scene.placements.remove(0);
        scene.placements.push(new);
        let out = rebuild(&map, &scene).unwrap();
        let shown: Vec<(&str, i32)> = out.placements.iter().map(|p| (p.name.as_str(), p.show.int())).collect();
        assert_eq!(shown, [("door_a", 0), ("door_b", 1), ("door_c", 1)]);
    }

    /// Every map of an extracted disc (DC2TK_CORPUS) rebuilds from its own scene unchanged.
    #[test]
    #[ignore = "needs DC2TK_CORPUS"]
    fn corpus_scenes_rebuild_their_maps() {
        let paths = dc2tk_disc::samples::corpus_files("map", "map");
        assert!(!paths.is_empty());
        for path in &paths {
            let map = mapfile::parse(&std::fs::read(path).unwrap()).unwrap();
            assert_eq!(rebuild(&map, &scene_of(&map)).unwrap(), map, "{}", path.display());
        }
        println!("{} maps rebuild from their scenes", paths.len());
    }

    #[test]
    fn long_names_and_duplicate_slots_are_refused() {
        let map = sample();
        let mut scene = scene_of(&map);
        scene.placements[0].placement.name = "x".repeat(32);
        assert!(rebuild(&map, &scene).is_err());
        let mut scene = scene_of(&map);
        scene.placements[1].slot = Some(0);
        assert!(rebuild(&map, &scene).is_err());
    }
}
