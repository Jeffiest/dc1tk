//! Character models as glTF for mods: a folder named after the pack plus .gltf (c02a.chr.gltf/)
//! holding what an export writes: c02a.gltf with its .bin, and textures/ with a PNG per texture the
//! model uses (the pack's own pictures where it has them). build rebuilds the mesh, weights and bind
//! pose from the edited glTF with the disc's model as the template, and puts texture PNGs that
//! differ from the export into the pack's texture files; parts left as exported change nothing.
//! new_pack makes a pack of its own from a glTF alone, in a template pack's layout.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use dc2tk_disc::{archive, rebuild};

use crate::folder::Folder;
use crate::from_gltf::{self, Gltf, MeshId};
use crate::info_cfg;
use crate::info_cfg::Arg;
use crate::{bail, im3, motions, textures, to_gltf, Context, Error, Result};

pub const TEXTURES: &str = "textures";

/// The pack's members by name, as a dict of them holds them: the last of a name given twice, in
/// the place of the first.
pub fn members(data: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
    let Some(found) = archive::parse_pack(data) else {
        bail!("not a character pack");
    };
    let mut out: Vec<(String, Vec<u8>)> = Vec::new();
    for member in found {
        let content = member.data(data).to_vec();
        match out.iter_mut().find(|(name, _)| *name == member.name) {
            Some(slot) => slot.1 = content,
            None => out.push((member.name, content)),
        }
    }
    Ok(out)
}

/// What changed inside a character pack, for build's report: its KEYs added, moved or taken out
/// (motions::key_changes), then its members that differ, that it gained or lost, and those left
/// as they were. An unchanged pack gives one line saying so.
pub fn pack_changes(old: &[u8], new: &[u8]) -> Result<Vec<String>> {
    if old == new {
        return Ok(vec!["unchanged".to_string()]);
    }
    let (before, after) = (members(old)?, members(new)?);
    let find = |list: &[(String, Vec<u8>)], name: &str| list.iter().find(|(n, _)| n == name).map(|(_, d)| d.clone());
    let mut lines = match (find(&before, "info.cfg"), find(&after, "info.cfg")) {
        (Some(was), Some(now)) => motions::key_changes(&was, &now),
        _ => Vec::new(),
    };
    let (mut changed, mut kept) = (Vec::new(), Vec::new());
    for (name, data) in &after {
        match find(&before, name) {
            Some(was) if was == *data => kept.push(name.clone()),
            Some(was) => changed.push(format!("{name} ({} bytes, was {})", data.len(), was.len())),
            None => changed.push(format!("{name} ({} bytes, new)", data.len())),
        }
    }
    for (name, _) in before.iter().filter(|(name, _)| find(&after, name).is_none()) {
        changed.push(format!("{name} (taken out)"));
    }
    if !changed.is_empty() {
        lines.push(format!("members changed: {}", changed.join(", ")));
    }
    if !kept.is_empty() {
        lines.push(format!("members unchanged: {}", kept.join(", ")));
    }
    Ok(lines)
}

/// The member names of the pack's model, weights and bind pose, from its info.cfg.
pub struct ModelFiles {
    pub model: String,
    pub weights: String,
    pub pose: Arg,
    pub stem: String,
    /// Motion set 0's .mot, when it names one.
    pub motion: Option<String>,
}

/// Path(name).stem.
fn path_stem(path: &str) -> &str {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    match name.rfind('.') {
        Some(dot) if dot > 0 && dot < name.len() - 1 => &name[..dot],
        _ => name,
    }
}

pub fn model_files(folder: &Folder) -> Result<ModelFiles> {
    let Some(info) = folder.get("info.cfg") else {
        bail!("the pack has no info.cfg");
    };
    let info = info_cfg::read(info);
    let model = match info.model.as_ref() {
        Some(model) if model.is_set() => model,
        _ => bail!("the pack's info.cfg names no model"),
    };
    let Some(model) = model.as_str() else {
        bail!("the pack's info.cfg names its model with {model:?}, which is not a string");
    };
    let stem = path_stem(model).to_string();
    let pose = info.motion0().map_or(Arg::Null, |m| m.bbp.clone());
    let motion = info.motion0().and_then(|m| m.mot.as_str()).filter(|name| !name.is_empty()).map(str::to_string);
    Ok(ModelFiles { model: model.to_string(), weights: format!("{stem}.wgt"), pose, stem, motion })
}

/// The pack's members as the folder they are unpacked into, and its model's files.
pub fn unpacked(data: &[u8]) -> Result<(Folder, ModelFiles)> {
    let mut folder = Folder::new();
    for (name, content) in members(data)? {
        folder.write(&name, content);
    }
    let files = model_files(&folder)?;
    Ok((folder, files))
}

/// (picture name, (member, PNG)) pairs.
pub type Pictures = Vec<(String, (String, Vec<u8>))>;

/// (member, PNG) of every picture of the pack's texture files by picture name, the first of a
/// name.
pub fn pack_pictures(data: &[u8]) -> Result<Pictures> {
    let mut out: Pictures = Vec::new();
    for (name, content) in members(data)? {
        if name.to_lowercase().ends_with(".img") {
            for image in im3::read(&content)? {
                if !image.no_image {
                    let png = im3::to_png(&image)?;
                    if !out.iter().any(|(picture, _)| *picture == image.name) {
                        out.push((image.name.clone(), (name.clone(), png)));
                    }
                }
            }
        }
    }
    Ok(out)
}

fn picture<'a>(pictures: &'a Pictures, name: &str) -> Option<&'a (String, Vec<u8>)> {
    pictures.iter().find(|(picture, _)| picture == name).map(|(_, found)| found)
}

/// The export as a folder: the glTF, its buffer and its textures.
fn export_folder(data: &[u8]) -> Result<Folder> {
    let pictures = pack_pictures(data)?;
    let (source, files) = unpacked(data)?;
    let mut out = Folder::new();
    to_gltf::export(&source, &mut out, &files.stem, 30.0)?;
    for (path, content) in out.files_mut() {
        if let Some(name) = path.strip_prefix("textures/") {
            if let Some(found) = picture(&pictures, path_stem(name)) {
                *content = found.1.clone();
            }
        }
    }
    Ok(out)
}

/// (relative path, bytes): the glTF, its buffer and its textures for a character pack.
pub fn export(data: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
    Ok(export_folder(data)?.sorted())
}

/// The disc's files, for a pack of motions alone to find the model it plays on: every file's path
/// (as extract lays the disc out, `chara/c01_base.chr`) and a reader of a path (a pack's member
/// as `a.chr.d/b.chr`).
pub struct DiscFiles<'a> {
    pub paths: Vec<String>,
    pub read: &'a (dyn Fn(&str) -> Option<Vec<u8>> + Sync),
}

/// Whether a pack holds motion sets and no model (`chara/c01_fishing.chr`: `MOTION 7`, no MODEL),
/// motions for a character another pack loads.
pub fn motions_only(data: &[u8]) -> bool {
    let Ok(found) = members(data) else {
        return false;
    };
    let Some((_, cfg)) = found.iter().find(|(name, _)| name.eq_ignore_ascii_case("info.cfg")) else {
        return false;
    };
    let info = info_cfg::read(cfg);
    info.model.as_ref().is_none_or(|model| !model.is_set()) && !info.motions.is_empty()
}

/// The words of a pack's name, and of three letters or more without their numbers:
/// `vs_e352_guiltoni2` gives vs, e352, guiltoni2, guiltoni.
fn words(stem: &str) -> Vec<String> {
    let mut out: Vec<String> =
        stem.to_lowercase().split(['_', '-', '.']).filter(|w| !w.is_empty()).map(str::to_string).collect();
    for word in out.clone() {
        let letters = word.trim_end_matches(|c: char| c.is_ascii_digit());
        if letters.len() >= 3 && letters.len() < word.len() {
            out.push(letters.to_string());
        }
    }
    out
}

/// The model pack a pack of motions alone plays on (path and bytes): Max's town pack
/// (`chara/c01_base.chr`) for a name of `mints` or `c01`, Monica's (`chara/c02_base.chr`) for
/// `monica` or `c02`, else the model pack beside it sharing the longest word of its name
/// (`e352_guiltoni.chr` for `vs_e352_guiltoni.chr`): the first whose model has every frame its key
/// lists and info.cfg name, else the first with every frame its key lists name (a motion pack's
/// attach points may name another model's frames, as `taxi_ride_c01`'s the taxi's). Max's and Monica's packs (town and dungeon) share their
/// skeletons, so the town one serves the dungeon's motions too.
pub fn skeleton_for(path: &str, data: &[u8], disc: &DiscFiles) -> Result<(String, Vec<u8>)> {
    let found = members(data)?;
    let cfg = found.iter().find(|(name, _)| name.eq_ignore_ascii_case("info.cfg")).map(|(_, data)| data.clone());
    let Some(cfg) = cfg else {
        bail!("{path} has no info.cfg");
    };
    let info = info_cfg::read(&cfg);
    // the frames its lists drive, and the frames its cfg names
    let mut frames = 0usize;
    for (_, motion) in &info.motions {
        let Some(mot) = motion.mot.as_str().and_then(|name| found.iter().find(|(n, _)| n == name)) else {
            continue;
        };
        for list in crate::mot::parse(&mot.1).unwrap_or_default() {
            frames = frames.max(usize::try_from(list.frame).map_or(0, |f| f + 1));
        }
    }
    let named: BTreeSet<String> = info_cfg::parse(&cfg)
        .into_iter()
        .filter(|(tag, _)| matches!(tag.as_str(), "OBJECT_NAME" | "OBJECT_NAME2" | "OUTLINE"))
        .flat_map(|(_, args)| args.into_iter().filter_map(|a| a.as_str().map(str::to_string)))
        .filter(|name| !name.is_empty())
        .collect();
    let name = path.rsplit('/').next().unwrap_or(path);
    let stem = path_stem(name);
    let own = words(stem);
    let mut candidates: Vec<String> = Vec::new();
    if own.iter().any(|w| w.starts_with("mints") || w.starts_with("c01")) {
        candidates.push("chara/c01_base.chr".into());
    }
    if own.iter().any(|w| w.starts_with("monica") || w.starts_with("c02")) {
        candidates.push("chara/c02_base.chr".into());
    }
    let folder = path.rsplit_once('/').map_or("", |(folder, _)| folder);
    let mut beside: Vec<(usize, String)> = disc
        .paths
        .iter()
        .filter(|other| {
            other.to_lowercase().ends_with(".chr")
                && other.rsplit_once('/').map_or("", |(f, _)| f).eq_ignore_ascii_case(folder)
                && !other.eq_ignore_ascii_case(path)
        })
        .map(|other| {
            let theirs = words(path_stem(other.rsplit('/').next().unwrap_or(other)));
            let shared = own.iter().filter(|w| theirs.contains(w)).map(|w| w.len()).max().unwrap_or(0);
            (shared, other.clone())
        })
        .filter(|(shared, _)| *shared > 0)
        .collect();
    beside.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    candidates.extend(beside.into_iter().map(|(_, other)| other));
    let mut fitting: Option<(String, Vec<u8>)> = None;
    for candidate in candidates {
        let Some(bytes) = (disc.read)(&candidate) else {
            continue;
        };
        let stem = path_stem(candidate.rsplit('/').next().unwrap_or(&candidate)).to_string();
        let (pack, _) = crate::tree::model_pack(&bytes, &stem);
        let Ok((folder, files)) = unpacked(&pack) else {
            continue;
        };
        let Some(model) = folder.get(&files.model).and_then(|data| crate::mds::parse(data).ok()) else {
            continue;
        };
        if model.nodes.len() < frames {
            continue;
        }
        let has = |name: &String| model.nodes.iter().any(|node| node.name == *name || node.base_name() == name);
        if named.iter().all(has) {
            return Ok((candidate, bytes));
        }
        fitting.get_or_insert((candidate, bytes));
    }
    if let Some(found) = fitting {
        return Ok(found);
    }
    bail!("{path} holds motions alone, and no model pack on the disc fits them (Max's, Monica's, or one beside it of a name it shares)")
}

/// A pack of motions alone exported on a model pack's skeleton (its stem the first motion set's
/// .mot's): that pack's skeleton, skin, meshes and pictures, with an animation per KEY of the
/// motion pack's sets.
fn motion_export_folder(data: &[u8], skeleton: &[u8], skeleton_name: &str) -> Result<(Folder, String)> {
    let (skeleton, _) = crate::tree::model_pack(skeleton, skeleton_name);
    let pictures = pack_pictures(&skeleton)?;
    let (source, _) = unpacked(&skeleton)?;
    let mut motion = Folder::new();
    for (name, content) in members(data)? {
        motion.write(&name, content);
    }
    let info = info_cfg::read(motion.get("info.cfg").unwrap_or_default());
    let stem = info
        .motions
        .iter()
        .find_map(|(_, m)| m.mot.as_str().filter(|name| !name.is_empty()).map(|name| path_stem(name).to_string()))
        .ok_or_else(|| Error::invalid("the pack names no .mot"))?;
    let mut out = Folder::new();
    to_gltf::export_motions(&motion, &source, &mut out, &stem, 30.0)?;
    for (path, content) in out.files_mut() {
        if let Some(name) = path.strip_prefix("textures/") {
            if let Some(found) = picture(&pictures, path_stem(name)) {
                *content = found.1.clone();
            }
        }
    }
    Ok((out, stem))
}

/// (relative path, bytes) of a pack of motions alone exported on a model pack (`skeleton`, the
/// pack named `skeleton_name`, as skeleton_for finds it).
pub fn export_motions(data: &[u8], skeleton: &[u8], skeleton_name: &str) -> Result<Vec<(String, Vec<u8>)>> {
    Ok(motion_export_folder(data, skeleton, skeleton_name)?.0.sorted())
}

/// A pack of motions alone with the motions of a folder export_motions wrote (and edited) in: its
/// .mot files and info.cfg rebuilt from the glTF's animations, against the model pack's skeleton.
/// The model, its skin and pictures are the other pack's and change nothing here.
pub fn build_motions(folder: &Path, original: &[u8], skeleton: &[u8], skeleton_name: &str) -> Result<Vec<u8>> {
    let (exported, stem) = motion_export_folder(original, skeleton, skeleton_name)?;
    let gltf = format!("{stem}.gltf");
    let gltf_path = folder.join(&gltf);
    if !gltf_path.is_file() {
        bail!("the folder has no {gltf}, the motions' glTF");
    }
    let unchanged = |name: &str| std::fs::read(folder.join(name)).ok().as_deref() == exported.get(name);
    if unchanged(&gltf) && unchanged(&format!("{stem}.bin")) {
        return Ok(original.to_vec());
    }
    let wrap = |error: Error| match error {
        Error::Invalid(message) => Error::invalid(format!("{gltf}: {message}")),
        other => other,
    };
    let given = load_from_disk(&gltf_path).map_err(wrap)?;
    let before = load_from_folder(&exported, &gltf)?;
    let (skeleton, _) = crate::tree::model_pack(skeleton, skeleton_name);
    let (source, files) = unpacked(&skeleton)?;
    let Some(template) = source.get(&files.model) else {
        bail!("the model pack has no {}", files.model);
    };
    let model = crate::mds::parse(template)?;
    let pack = members(original)?;
    let member = |name: &str| pack.iter().find(|(n, _)| n == name).map(|(_, data)| data.clone());
    let Some((_, cfg)) = pack.iter().find(|(name, _)| name == "info.cfg") else {
        bail!("the pack has no info.cfg");
    };
    match motions::build(&before, &given, &model, &member, cfg, None).map_err(wrap)? {
        Some(rebuilt) => write_motions(original.to_vec(), &rebuilt),
        None => Ok(original.to_vec()),
    }
}

/// models::export for any character pack on the disc: a pack of motions alone on the model pack
/// skeleton_for finds, any other as itself (its cfg named after it, as tree::model_pack reads it).
pub fn export_on_disc(data: &[u8], path: &str, disc: &DiscFiles) -> Result<Vec<(String, Vec<u8>)>> {
    if motions_only(data) {
        let (skeleton_path, skeleton) = skeleton_for(path, data, disc)?;
        let name = path_stem(skeleton_path.rsplit('/').next().unwrap_or(&skeleton_path)).to_string();
        return export_motions(data, &skeleton, &name);
    }
    let stem = path_stem(path.rsplit('/').next().unwrap_or(path));
    crate::tree::exported_model(data, stem)
}

/// models::build for any character pack on the disc, as export_on_disc exports it.
pub fn build_on_disc(folder: &Path, original: &[u8], path: &str, disc: &DiscFiles) -> Result<Vec<u8>> {
    if motions_only(original) {
        let (skeleton_path, skeleton) = skeleton_for(path, original, disc)?;
        let name = path_stem(skeleton_path.rsplit('/').next().unwrap_or(&skeleton_path)).to_string();
        return build_motions(folder, original, &skeleton, &name);
    }
    let stem = path_stem(path.rsplit('/').next().unwrap_or(path));
    crate::tree::build_model(folder, original, stem)
}

/// The files under a folder by relative path, in files_under's order.
fn folder_files(folder: &Path) -> Result<Vec<(String, Vec<u8>)>> {
    crate::files_under(folder)?
        .into_iter()
        .map(|path| Ok((crate::relative_posix(&path, folder), std::fs::read(&path)?)))
        .collect()
}

/// A glTF read from a folder on disk.
fn load_from_disk(path: &Path) -> Result<Gltf> {
    let folder = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    from_gltf::load_gltf(&std::fs::read(path)?, &|uri| Ok(std::fs::read(folder.join(uri))?))
}

/// A glTF read from the export's files.
fn load_from_folder(files: &Folder, name: &str) -> Result<Gltf> {
    let Some(raw) = files.get(name) else {
        bail!("the folder has no {name:?}");
    };
    from_gltf::load_gltf(raw, &|uri| {
        files.get(uri).map(<[u8]>::to_vec).ok_or_else(|| Error::invalid(format!("the folder has no {uri:?}")))
    })
}

/// The meshes of the given glTF that differ from the export's, or that the export does not have:
/// by value (from_gltf::changed_character_meshes), since Blender writes every mesh its own way.
fn changed_meshes(given: &Path, exported: &Folder, template: &[u8]) -> Result<BTreeSet<MeshId>> {
    let name = given.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let model = crate::mds::parse(template)?;
    from_gltf::changed_character_meshes(&load_from_folder(exported, &name)?, &load_from_disk(given)?, &model)
}

/// The pack with the model and textures a folder gives in place of the original's.
pub fn build(folder: &Path, original: &[u8]) -> Result<Vec<u8>> {
    let exported_folder = export_folder(original)?;
    let exported = exported_folder.sorted();
    let exported_get = |path: &str| exported.iter().find(|(p, _)| p == path).map(|(_, c)| c);
    let given = folder_files(folder)?;
    let mut pack = original.to_vec();
    let (source, files) = unpacked(original)?;
    let gltf = format!("{}.gltf", files.stem);
    let model_changed = given
        .iter()
        .filter(|(path, _)| !path.to_lowercase().ends_with(".png"))
        .any(|(path, content)| exported_get(path) != Some(content));
    if model_changed {
        if !given.iter().any(|(path, _)| *path == gltf) {
            bail!("the folder has no {gltf}, the model's glTF");
        }
        let pose_name = match &files.pose {
            p if !p.is_set() => None,
            Arg::Str(name) => Some(name.as_str()),
            other => bail!("the pack's info.cfg names motion set 0's pose with {other:?}, which is not a string"),
        };
        let pose = pose_name.and_then(|name| source.get(name));
        let weights = source.get(&files.weights);
        let gltf_path = folder.join(&gltf);
        let wrap = |error: Error| match error {
            Error::Invalid(message) => Error::invalid(format!("{gltf}: {message}")),
            other => other,
        };
        let Some(template) = source.get(&files.model) else {
            bail!("the folder has no {:?}, the model", files.model);
        };
        let meshes = changed_meshes(&gltf_path, &exported_folder, template).map_err(wrap)?;
        let given_gltf = load_from_disk(&gltf_path).map_err(wrap)?;
        let import = from_gltf::ImportOptions { bbp: pose, wgt: weights, meshes: Some(&meshes), collision: false };
        let built = from_gltf::import_gltf(&given_gltf, template, &import).map_err(wrap)?;
        // a dict of member to content: a member named twice keeps its place and the last content
        let mut written: Vec<(&str, Vec<u8>)> = Vec::new();
        let rebuilt_model = crate::mds::parse(&built.mds)?;
        let mut outputs = vec![(files.model.as_str(), built.mds), (files.weights.as_str(), built.wgt)];
        if let Some(pose_name) = pose_name {
            outputs.push((pose_name, built.bbp));
        }
        for (member, content) in outputs {
            match written.iter_mut().find(|(m, _)| *m == member) {
                Some(slot) => slot.1 = content,
                None => written.push((member, content)),
            }
        }
        for (member, content) in written {
            if !source.is_file(member) {
                if content.is_empty() {
                    continue; // a model with no skinned mesh has no weights to write
                }
                bail!(
                    "the pack has no {member} for the model's new skin weights; a model without skinned meshes cannot gain them"
                );
            }
            pack = rebuild::replaced_member(&pack, &[member.to_string()], &content)?;
        }
        // the animations that differ from the export's go into the motions and their KEY lines
        if let Some(cfg) = source.get("info.cfg") {
            let before = load_from_folder(&exported_folder, &gltf)?;
            let model = crate::mds::parse(template)?;

            let frames: BTreeSet<usize> = meshes
                .iter()
                .filter_map(|id| match id {
                    MeshId::Frame(frame) => Some(*frame),
                    MeshId::New(_) => None,
                })
                .collect();
            let member = |name: &str| source.get(name).map(<[u8]>::to_vec);
            let rebuilt = (!frames.is_empty()).then_some((&rebuilt_model, &frames));
            if let Some(rebuilt) = motions::build(&before, &given_gltf, &model, &member, cfg, rebuilt).map_err(wrap)? {
                pack = write_motions(pack, &rebuilt)?;
            }
        }
    }
    with_textures(pack, &given, &exported)
}

/// The pack with rebuilt motions' .mot files and info.cfg in place of its own.
fn write_motions(mut pack: Vec<u8>, rebuilt: &motions::Rebuilt) -> Result<Vec<u8>> {
    for (member, mot) in &rebuilt.mots {
        pack = rebuild::replaced_member(&pack, std::slice::from_ref(member), mot)?;
    }
    rebuild::replaced_member(&pack, &["info.cfg".to_string()], &rebuilt.info).map_err(Error::from)
}

/// Files by relative path.
pub type Files = Vec<(String, Vec<u8>)>;

/// A map model (one .mds of a map's model pack) as glTF: `<stem>.gltf`, `<stem>.bin` and a PNG
/// per texture in textures/, by relative path, and what the export found. Map options
/// (`to_gltf::Options::map`, `::collision`) name nodes and meshes by the frames' whole names.
pub fn export_map_model(data: &[u8], stem: &str, options: &to_gltf::Options) -> Result<(Files, to_gltf::Exported)> {
    let model = crate::mds::parse(data)?;
    let mut out = Folder::new();
    let found = to_gltf::export_model(&model, &mut out, stem, options)?;
    Ok((out.sorted(), found))
}

/// The map model rebuilt from its edited glTF: only the meshes that differ from an export with
/// the same options (or that it does not have) are rebuilt, each glTF mesh node replacing the mesh
/// of the frame its `extras.mds_index` (or else its name) gives, so the frames left as exported
/// keep their bytes and an unedited glTF gives the original file. With `options.collision` the
/// rebuilt meshes are collision (from_gltf::ImportOptions).
pub fn build_map_model(given: &Gltf, original: &[u8], options: &to_gltf::Options) -> Result<Vec<u8>> {
    let model = crate::mds::parse(original)?;
    let mut exported = Folder::new();
    to_gltf::export_model(&model, &mut exported, "model", options)?;
    let before = load_from_folder(&exported, "model.gltf")?;
    // by geometry: a map's glTF goes through Blender, which rewrites unedited meshes another way
    let meshes = from_gltf::changed_geometry(&before, given, &model)?;
    let import = from_gltf::ImportOptions { meshes: Some(&meshes), collision: options.collision, ..Default::default() };
    Ok(from_gltf::import_gltf(given, original, &import)?.mds)
}

/// A new map model from meshes made in another tool: every mesh of the glTF becomes a frame under
/// a root frame named `stem`, its vertices where the object was put (relative to the glTF's root).
/// With `options.collision` the meshes are collision (from_gltf::ImportOptions).
pub fn new_map_model(given: &Gltf, stem: &str, options: &to_gltf::Options) -> Result<Vec<u8>> {
    let mut model = crate::mds::Mds::new();
    model.nodes.push(crate::mds::Node::new(stem, -1));
    let template = crate::mds::build(&model)?;
    let import = from_gltf::ImportOptions { collision: options.collision, ..Default::default() };
    let built = from_gltf::import_gltf(given, &template, &import)?.mds;
    if crate::mds::parse(&built)?.nodes.iter().all(|node| node.mdt.is_none()) {
        bail!("no mesh to make the model of");
    }
    Ok(built)
}

/// Path(path).name.
fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// The pack with the texture PNGs that differ from the export put into its texture files: a PNG
/// counts wherever it is in the folder, by its picture name.
fn with_textures(mut pack: Vec<u8>, given: &[(String, Vec<u8>)], exported: &[(String, Vec<u8>)]) -> Result<Vec<u8>> {
    let pictures = pack_pictures(&pack)?;
    let mut changed: BTreeMap<String, Vec<(String, Vec<u8>)>> = BTreeMap::new();
    for (path, content) in given {
        let name = file_name(path);
        let export_path = format!("{TEXTURES}/{name}");
        let same = exported.iter().any(|(p, c)| *p == export_path && c == content);
        if !path.to_lowercase().ends_with(".png") || same {
            continue;
        }
        let stem = path_stem(name);
        let Some((member, _)) = picture(&pictures, stem) else {
            bail!("{path}: the pack has no picture {stem}; textures from other packs are changed there");
        };
        if changed.values().flatten().any(|(other, _)| file_name(other) == name) {
            bail!("{path}: picture {stem} is given twice in the folder");
        }
        changed.entry(member.clone()).or_default().push((path.clone(), content.clone()));
    }
    for (member, pngs) in changed {
        let mut temp = Folder::new();
        for (path, content) in pngs {
            temp.write(&textures::file_name(path_stem(file_name(&path))), content);
        }
        let pngs: Vec<(String, Vec<u8>)> =
            temp.sorted().into_iter().filter(|(name, _)| name.to_lowercase().ends_with(textures::SUFFIX)).collect();
        let names = [member.clone()];
        let Some(img) = rebuild::read_member(&pack, &names) else {
            bail!("the pack has no member {member}");
        };
        let built = textures::build_from(&pngs, img)?;
        pack = rebuild::replaced_member(&pack, &names, &built)?;
    }
    Ok(pack)
}

/// A pack of the members, in order: each in its 0x50-byte slot, padded to 16 bytes, and an empty
/// slot at the end.
pub fn write_pack(members: &[(String, Vec<u8>)]) -> Result<Vec<u8>> {
    let members: Vec<(&str, &[u8])> = members.iter().map(|(name, data)| (name.as_str(), data.as_slice())).collect();
    Ok(archive::write_pack(&members)?)
}

/// What a new pack's info.cfg says of its model (new_info_cfg).
pub struct NewCfg<'a> {
    /// The pack's name: its members are <stem>.mds, <stem>.img and so on.
    pub stem: &'a str,
    /// Whether the pack has a texture file.
    pub pictures: bool,
    /// The model's frames: OUTLINE and OBJECT_NAME lines naming another are dropped.
    pub frames: &'a [String],
    /// The skinned meshes' frames, which VERTEX_ANIME names.
    pub skinned: &'a [String],
    /// Motion set 0's KEYs (name, start, end, step), when the pack has a motion: MOTION 0 then
    /// loads <stem>.mot, <stem>.bbp and the skin's <stem>.wgt.
    pub keys: Option<&'a [(String, i64, i64, f64)]>,
}

/// Blocks of a template's info.cfg about the template's own model and motions (sequences, sound
/// cues by KEY name, cloth, effects on its frames, LOD models), dropped from a new pack's: by the
/// tag that starts each and the one that ends it.
const MODEL_BLOCKS: [(&str, &str); 6] = [
    ("MOTION", "MOTION_END"),
    ("SEQ_START", "SEQ_END"),
    ("SE_START", "SE_END"),
    ("CLOTH_START", "CLOTH_END"),
    ("EFFECT_START", "EFFECT_END"),
    ("LOD_MODEL_START", "LOD_MODEL_END"),
];

/// A template's info.cfg for a new pack: NAME, MODEL and the texture file name the new pack's
/// members (<stem>.mds, <stem>.img, which IMG 0 loads; no IMG line without pictures), what
/// belonged to the template's model alone dropped (motion sets and their sequences, sound cues,
/// cloth, effects, vertex and shape animation, shadow and LOD models, outlines and attach points
/// on frames the new model lacks), VERTEX_ANIME naming the new skinned meshes before MODEL, and
/// the new motion set where the template's first was (else at the end). Every other line (the
/// whole-model outline, size, scale, comments) stays as it is.
pub fn new_info_cfg(template: &[u8], cfg: &NewCfg) -> Result<Vec<u8>> {
    let stem = cfg.stem;
    let text = dc2tk_disc::text::decode_lossy(template);
    let newline = info_cfg::newline(text.as_bytes());
    let img = format!("IMG 0, \"{stem}.img\";");
    let quoted = |names: &[String]| names.iter().map(|name| format!("\"{name}\"")).collect::<Vec<_>>().join(", ");
    let motion = cfg.keys.map(|keys| {
        let mut block =
            format!("MOTION 0, \"{stem}.mot\", \"{stem}.bbp\", \"{stem}.wgt\";{newline}KEY_START;{newline}");
        for (name, start, end, step) in keys {
            block.push_str(&format!("\t{}{newline}", motions::key_line(name, *start, *end, *step)));
        }
        block.push_str(&format!("KEY_END;{newline}MOTION_END;{newline}"));
        block
    });
    // whether every frame a line names (its text arguments; "" is the whole model) is the model's
    let names_known = |line: &str| {
        let args = dc2tk_disc::text::encode(line).map(|raw| info_cfg::parse(&raw)).unwrap_or_default();
        args.iter()
            .flat_map(|(_, args)| args)
            .filter_map(Arg::as_str)
            .all(|name| name.is_empty() || cfg.frames.iter().any(|frame| frame == name))
    };
    let mut out = String::new();
    let (mut img_written, mut model_written, mut motion_written) = (false, false, false);
    let mut inside: Option<&str> = None;
    for line in text.split_inclusive('\n') {
        let tag = info_cfg::line_tag(line);
        let ending = if line.ends_with("\r\n") {
            "\r\n"
        } else if line.ends_with('\n') {
            "\n"
        } else {
            ""
        };
        if let Some(end) = inside {
            if tag == end {
                inside = None;
            }
            continue;
        }
        if let Some((start, end)) = MODEL_BLOCKS.iter().find(|(start, _)| *start == tag) {
            inside = Some(end);
            if *start == "MOTION" && !motion_written {
                out.push_str(motion.as_deref().unwrap_or_default());
                motion_written = true;
            }
            continue;
        }
        match tag {
            "SHADOW_MOTION"
            | "VERTEX_ANIME"
            | "SHADOW_VERTEX_ANIME"
            | "SHAPE_ANIME"
            | "SHADOW_MODEL"
            | "LOD_MODEL"
            | "KEY_START"
            | "KEY"
            | "KEY_END"
            | "MOTION_END"
            | "SEQ"
            | "SE"
            | "SELP"
            | "CLOTH"
            | "EFFECT" => {}
            "OUTLINE" | "OBJECT_NAME" | "OBJECT_NAME2" if !names_known(line) => {}
            "NAME" => out.push_str(&format!("NAME \"{stem}\";{ending}")),
            "IMG" => {
                if cfg.pictures && !img_written {
                    out.push_str(&format!("{img}{ending}"));
                }
                img_written = true;
            }
            "MODEL" => {
                if cfg.pictures && !img_written {
                    out.push_str(&format!("{img}{newline}IMG_END;{newline}{newline}"));
                    img_written = true;
                }
                if !cfg.skinned.is_empty() {
                    out.push_str(&format!("VERTEX_ANIME {};{newline}", quoted(cfg.skinned)));
                }
                out.push_str(&format!("MODEL \"{stem}.mds\";{ending}"));
                model_written = true;
            }
            _ => out.push_str(line),
        }
    }
    if !model_written {
        bail!("the template's cfg names no model");
    }
    if let Some(motion) = motion.filter(|_| !motion_written) {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push_str(newline);
        }
        out.push_str(newline);
        out.push_str(&motion);
    }
    // the lines dropped leave no runs of blank lines
    let mut blank = false;
    out = out
        .split_inclusive('\n')
        .filter(|line| {
            let was = std::mem::replace(&mut blank, line.trim().is_empty());
            !(was && blank)
        })
        .collect();
    dc2tk_disc::text::encode(&out).ok_or_else(|| Error::invalid("the template's cfg is not Shift-JIS text"))
}

/// The glTF a mod gives for a new model: a .gltf (its buffers and images beside it), a .glb, or a
/// folder holding one of them.
fn gltf_file(source: &Path) -> Result<std::path::PathBuf> {
    if !source.is_dir() {
        return Ok(source.to_path_buf());
    }
    let mut found: Vec<std::path::PathBuf> = std::fs::read_dir(source)?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            let name = path.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
            path.is_file() && (name.ends_with(".gltf") || name.ends_with(".glb"))
        })
        .collect();
    match found.len() {
        1 => Ok(found.remove(0)),
        0 => bail!("{} holds no .gltf or .glb file", source.display()),
        _ => bail!("{} holds more than one .gltf or .glb file", source.display()),
    }
}

/// Most bones a skinned mesh's vertices may be weighted to, and most vertices it may have: one
/// more and CreateVertexWeight drops its skinning, or _MODEL's 6,400-quadword work buffer
/// overflows (32 bytes a vertex, plus 2 quadwords).
pub const SKIN_BONES: usize = 32;
pub const SKIN_VERTICES: usize = 3198;
/// Most VERTEX_ANIME names _VERTEX_ANIME keeps.
pub const SKINNED_MESHES: usize = 24;

/// What breaks the engine's limits on skinned meshes in a model and its .wgt: more than
/// SKIN_BONES bones or SKIN_VERTICES vertices in one, more than SKINNED_MESHES of them.
pub fn skin_problems(model: &crate::mds::Mds, wgt: &[crate::mot::KeyList], skinned: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    if skinned.len() > SKINNED_MESHES {
        out.push(format!("{} skinned meshes; VERTEX_ANIME keeps {SKINNED_MESHES}", skinned.len()));
    }
    for (frame, node) in model.nodes.iter().enumerate() {
        if !skinned.contains(&node.name) {
            continue;
        }
        let bones: BTreeSet<i32> = wgt
            .iter()
            .filter(|list| list.frame == frame as i32 && !list.keys.is_empty())
            .map(|list| list.target)
            .collect();
        if bones.len() > SKIN_BONES {
            out.push(format!(
                "mesh {} is weighted to {} bones; the game skins {SKIN_BONES} at most",
                node.name,
                bones.len()
            ));
        }
        let vertices = node.mdt.as_ref().map_or(0, |mdt| mdt.vertices.len());
        if vertices > SKIN_VERTICES {
            out.push(format!(
                "skinned mesh {} has {vertices} vertices; the game skins {SKIN_VERTICES} at most",
                node.name
            ));
        }
    }
    out
}

/// A new character pack named `stem` built from a glTF, with a template pack (`template_name` is
/// its model's name, for packs whose cfg is named after them) giving its cfg's other lines, the
/// MDS version and the texture format. Its members: <stem>.mds, the model (from_gltf::new_model:
/// a frame for every node, rigid and skinned meshes); <stem>.img, which holds each material's base
/// colour image as a picture of the name the material uses (PNG, sides powers of two, in the
/// format of the template's first picture); with a skin <stem>.wgt and <stem>.bbp, and with
/// animations <stem>.mot (motions::new_motion); and info.cfg (new_info_cfg). No cloth: a .clo
/// names a model's frames and particles, which a new model does not have.
pub fn new_pack(source: &Path, template: &[u8], template_name: &str, stem: &str) -> Result<Vec<u8>> {
    let (pack, _) = crate::tree::model_pack(template, template_name);
    let (folder, files) = unpacked(&pack)?;
    let Some(model) = folder.get(&files.model) else {
        bail!("the template has no {}, its model", files.model);
    };
    let version = crate::mds::parse(model)?.version;
    let path = gltf_file(source)?;
    let here = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let read = |uri: &str| -> Result<Vec<u8>> {
        let file = here.join(crate::tree::uri_decoded(uri));
        std::fs::read(&file).with_context(|| file.display().to_string())
    };
    let mut gltf = from_gltf::load_gltf(&std::fs::read(&path)?, &read)?;
    let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
    let wrap = |error: Error| match error {
        Error::Invalid(message) => Error::invalid(format!("{name}: {message}")),
        other => other,
    };
    let mut pictures = Vec::new();
    for (texture, image) in from_gltf::material_textures(&gltf).map_err(wrap)? {
        let png = from_gltf::image_bytes(&gltf, &image, &read).map_err(wrap)?;
        if !png.starts_with(b"\x89PNG") {
            return Err(wrap(Error::invalid(format!("the image of texture {texture} is not a PNG"))));
        }
        pictures.push((texture, png));
    }
    let built = from_gltf::new_model(&mut gltf, version, stem).map_err(wrap)?;
    let model = crate::mds::parse(&built.mds)?;
    let skin = crate::mot::parse(&built.wgt)?;
    let problems = skin_problems(&model, &skin, &built.skinned);
    if !problems.is_empty() {
        return Err(wrap(Error::invalid(problems.join("; "))));
    }
    let motion = crate::motions::new_motion(&gltf, &model).map_err(wrap)?;
    if motion.is_none() && !built.skinned.is_empty() {
        // the bind pose comes with a motion set (MOTION 0 loads the .bbp)
        return Err(wrap(Error::invalid("a skinned model needs an animation that moves a bone")));
    }
    let frames: Vec<String> = model.nodes.iter().map(|node| node.name.clone()).collect();
    let mds = built.mds;
    let mut members = Vec::new();
    if !pictures.is_empty() {
        let info = info_cfg::read(folder.get("info.cfg").unwrap_or_default());
        let mut images: Vec<&str> = info.imgs.iter().filter_map(|(_, member)| member.as_str()).collect();
        images.extend(
            folder
                .files()
                .iter()
                .map(|(member, _)| member.as_str())
                .filter(|member| member.to_lowercase().ends_with(".img")),
        );
        let Some(img) = images.iter().find_map(|member| folder.get(member)) else {
            bail!("the template has no texture file to take the pictures' format from");
        };
        let like = textures::template_picture(img, None)?;
        let built = textures::new_file(&pictures, &like, true).map_err(wrap)?;
        members.push((format!("{stem}.img"), built));
    }
    members.push((format!("{stem}.mds"), mds));
    if let Some(motion) = &motion {
        members.push((format!("{stem}.bbp"), built.bbp));
        if !built.skinned.is_empty() {
            members.push((format!("{stem}.wgt"), built.wgt));
        }
        members.push((format!("{stem}.mot"), motion.mot.clone()));
    }
    let cfg = NewCfg {
        stem,
        pictures: !pictures.is_empty(),
        frames: &frames,
        skinned: &built.skinned,
        keys: motion.as_ref().map(|motion| motion.keys.as_slice()),
    };
    members.push(("info.cfg".to_string(), new_info_cfg(folder.get("info.cfg").unwrap_or_default(), &cfg)?));
    write_pack(&members)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mds::{self, Material, Mds, Mdt, Node, Prim, PRIM_TRIANGLES};
    use dc2tk_disc::samples::pack;
    use std::sync::Arc;

    fn sample_pack() -> Vec<u8> {
        let mut model = Mds::new();
        model.nodes.push(Node::new("root", -1));
        let corners = [[0, 0, 0], [1, 0, 1], [2, 0, 2], [0, 0, 0], [2, 0, 2], [3, 0, 3]];
        let mesh = Mdt {
            vertices: vec![[0.0, 0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [1.0, 1.0, 0.0, 1.0], [0.0, 1.0, 0.0, 1.0]],
            normals: vec![[0.0, 0.0, 1.0, 1.0]],
            uvs: vec![[0.0, 0.0, 1.0, 0.0], [1.0, 0.0, 1.0, 0.0], [1.0, 1.0, 1.0, 0.0], [0.0, 1.0, 1.0, 0.0]],
            materials: vec![Material::new("tex01")],
            prims: vec![Prim {
                kind: PRIM_TRIANGLES,
                material: 0,
                verts: corners.iter().map(|c| c.to_vec()).collect(),
            }],
            ..Mdt::default()
        };
        model.nodes.push(Node { mdt: Some(mesh), ..Node::new("body", 0) });
        let mut picture = im3::Image::new("tex01", im3::Format::Ct32, 8, 8);
        picture.pixels = Some([200, 100, 50, 0x80].repeat(64));
        let img = im3::write(&[picture], None).unwrap();
        pack(&[("info.cfg", b"v2;\nMODEL \"c.mds\";\n"), ("c.mds", &mds::build(&model).unwrap()), ("c.img", &img)])
    }

    fn written(files: &[(String, Vec<u8>)], name: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir().join(format!("dc2tk-assets-models-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        for (path, content) in files {
            let target = folder.join(path);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::write(target, content).unwrap();
        }
        folder
    }

    fn get<'a>(files: &'a [(String, Vec<u8>)], name: &str) -> &'a [u8] {
        &files.iter().find(|(path, _)| path == name).unwrap().1
    }

    #[test]
    fn an_unedited_export_builds_back_to_the_same_pack() {
        let pack = sample_pack();
        let exported = export(&pack).unwrap();
        let names: Vec<&str> = exported.iter().map(|(path, _)| path.as_str()).collect();
        assert_eq!(names, ["c.bin", "c.gltf", "textures/tex01.png"]);
        // the texture is the pack's own picture, not a placeholder
        let img = rebuild::read_member(&pack, &["c.img".to_string()]).unwrap();
        assert_eq!(get(&exported, "textures/tex01.png"), im3::to_png(&im3::read(img).unwrap()[0]).unwrap());
        let folder = written(&exported, "unedited");
        let built = build(&folder, &pack).unwrap();
        std::fs::remove_dir_all(folder).unwrap();
        assert_eq!(built, pack);
    }

    #[test]
    fn an_edited_mesh_and_texture_reach_the_pack() {
        let pack = sample_pack();
        let mut exported = export(&pack).unwrap();
        let gltf = crate::from_gltf::load_json(std::str::from_utf8(get(&exported, "c.gltf")).unwrap()).unwrap();
        let mut buffer = get(&exported, "c.bin").to_vec();
        let primitive =
            &gltf.get("meshes").unwrap().as_array().unwrap()[0].get("primitives").unwrap().as_array().unwrap()[0];
        let accessor = from_gltf::index(
            gltf.get("accessors").unwrap(),
            primitive.get("attributes").unwrap().get("POSITION").unwrap(),
        )
        .unwrap();
        let view = from_gltf::index(gltf.get("bufferViews").unwrap(), accessor.get("bufferView").unwrap()).unwrap();
        let base = view.get("byteOffset").and_then(|v| v.as_i64()).unwrap_or(0) as usize;
        for i in 0..accessor.get("count").unwrap().as_i64().unwrap() as usize {
            for axis in 0..2 {
                let at = base + 12 * i + 4 * axis;
                let x = f32::from_le_bytes(buffer[at..at + 4].try_into().unwrap());
                buffer[at..at + 4].copy_from_slice(&(x * 2.0).to_le_bytes());
            }
        }
        for (path, content) in exported.iter_mut() {
            if path == "c.bin" {
                *content = buffer.clone();
            }
            if path == "textures/tex01.png" {
                *content =
                    crate::png::write_png(8, 8, crate::png::PngMode::Rgb, &[0, 0, 255].repeat(64), &[], &[]).unwrap();
            }
        }
        let folder = written(&exported, "edited");
        let built = build(&folder, &pack).unwrap();
        std::fs::remove_dir_all(folder).unwrap();
        let model = mds::parse(rebuild::read_member(&built, &["c.mds".to_string()]).unwrap()).unwrap();
        let corners: BTreeSet<(i64, i64)> =
            model.nodes[1].mdt.as_ref().unwrap().vertices.iter().map(|v| (v[0] as i64, v[1] as i64)).collect();
        assert_eq!(corners, BTreeSet::from([(0, 0), (2, 0), (2, 2), (0, 2)]));
        assert_eq!((model.nodes[0].name.as_str(), model.nodes[1].name.as_str()), ("root", "body"));
        let img = rebuild::read_member(&built, &["c.img".to_string()]).unwrap();
        assert_eq!(im3::read(img).unwrap()[0].pixels.as_deref().unwrap(), [0, 0, 255, 0x80].repeat(64));
        let info = ["info.cfg".to_string()];
        assert_eq!(rebuild::read_member(&built, &info), rebuild::read_member(&pack, &info));
    }

    /// A prop pack as the game's are: a swizzled 8-bit picture, a model, a motion set.
    fn prop_template() -> Vec<u8> {
        let mut picture = im3::Image::new("prop01", im3::Format::T8, 64, 64);
        picture.swizzled = true;
        picture.palette = Some(vec![[90, 90, 90, 0x80]; 256]);
        picture.indices = Some(vec![0; 64 * 64]);
        let img = im3::write(&[picture], None).unwrap();
        let cfg = "v2;\r\n\r\nNAME \"prop\";\r\n\r\nIMG 0, \"prop.img\";\r\nIMG_END;\r\n\r\nMATERIAL_ANIME 0;\r\nMODEL \"prop.mds\";\r\nBODY_SIZE 0.00, 0.00, 0.00;\r\n\r\nMOTION 0, \"prop.mot\", \"\", \"\";\r\nKEY_START;\r\n\tKEY \"loop\", 2, 15, 0.02;\r\nKEY_END;\r\nMOTION_END;\r\n";
        let model = members(&sample_pack()).unwrap().into_iter().find(|(name, _)| name == "c.mds").unwrap().1;
        pack(&[("prop.img", &img), ("prop.mds", &model), ("prop.mot", b"motion"), ("info.cfg", cfg.as_bytes())])
    }

    /// A glTF Separate of two meshes: "body" (textured, under a moved node "calc") and "key"
    /// (untextured), with the texture's PNG in a folder beside it.
    fn calc_gltf(folder: &Path) -> std::path::PathBuf {
        let mut bin = Vec::new();
        for v in [[0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            for x in v {
                bin.extend(x.to_le_bytes());
            }
        }
        for v in [[0f32, 0.0], [1.0, 0.0], [0.0, 1.0]] {
            for x in v {
                bin.extend(x.to_le_bytes());
            }
        }
        for i in [0u16, 1, 2, 0] {
            bin.extend(i.to_le_bytes());
        }
        let gltf = r#"{"asset": {"version": "2.0"}, "scene": 0, "scenes": [{"nodes": [0, 2]}],
 "nodes": [{"name": "calc", "translation": [0, 2, 0], "children": [1]},
           {"name": "body", "mesh": 0, "rotation": [0, 0.7071068, 0, 0.7071068]},
           {"mesh": 1, "scale": [2, 2, 2]}],
 "meshes": [{"name": "body", "primitives": [{"attributes": {"POSITION": 0, "TEXCOORD_0": 1}, "indices": 2, "material": 0}]},
            {"name": "key", "primitives": [{"attributes": {"POSITION": 0}, "indices": 2, "material": 1}]}],
 "materials": [{"name": "Face", "pbrMetallicRoughness": {"baseColorTexture": {"index": 0}}},
               {"name": "Plastic", "pbrMetallicRoughness": {"baseColorFactor": [0.2, 0.2, 0.2, 1]}}],
 "textures": [{"source": 0}], "images": [{"uri": "tex/calc_face.png"}],
 "accessors": [{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"},
               {"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC2"},
               {"bufferView": 2, "componentType": 5123, "count": 3, "type": "SCALAR"}],
 "bufferViews": [{"buffer": 0, "byteOffset": 0, "byteLength": 36}, {"buffer": 0, "byteOffset": 36, "byteLength": 24},
                 {"buffer": 0, "byteOffset": 60, "byteLength": 6}],
 "buffers": [{"uri": "calc.bin", "byteLength": 68}]}"#;
        let _ = std::fs::remove_dir_all(folder);
        std::fs::create_dir_all(folder.join("tex")).unwrap();
        std::fs::write(folder.join("calc.bin"), bin).unwrap();
        let face: Vec<u8> =
            (0..32 * 16).flat_map(|i| if i % 5 == 0 { [255, 255, 255, 255] } else { [20, 40, 200, 255] }).collect();
        std::fs::write(
            folder.join("tex/calc_face.png"),
            crate::png::write_png(32, 16, crate::png::PngMode::Rgba, &face, &[], &[]).unwrap(),
        )
        .unwrap();
        std::fs::write(folder.join("calc.gltf"), gltf).unwrap();
        folder.join("calc.gltf")
    }

    #[test]
    fn a_new_pack_holds_the_gltf_meshes_and_textures_in_the_templates_layout() {
        let folder = std::env::temp_dir().join(format!("dc2tk-assets-new-pack-{}", std::process::id()));
        let gltf = calc_gltf(&folder);
        let built = new_pack(&gltf, &prop_template(), "prop", "calc").unwrap();
        let found = members(&built).unwrap();
        let names: Vec<&str> = found.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["calc.img", "calc.mds", "info.cfg"]);
        let cfg = String::from_utf8(found[2].1.clone()).unwrap();
        assert_eq!(
            cfg,
            "v2;\r\n\r\nNAME \"calc\";\r\n\r\nIMG 0, \"calc.img\";\r\nIMG_END;\r\n\r\nMATERIAL_ANIME 0;\r\nMODEL \"calc.mds\";\r\nBODY_SIZE 0.00, 0.00, 0.00;\r\n\r\n"
        );
        // frames: the two roots under one named after the pack, the node "calc" renamed to keep
        // names distinct, the unnamed mesh node by its mesh's name
        let model = mds::parse(&found[1].1).unwrap();
        let frames: Vec<(&str, i32, bool)> =
            model.nodes.iter().map(|node| (node.name.as_str(), node.parent, node.mdt.is_some())).collect();
        assert_eq!(frames, [("calc", -1, false), ("calc_1", 0, false), ("body", 1, true), ("key", 0, true)]);
        assert_eq!(model.nodes[1].matrix[13], 2.0);
        assert_eq!(model.nodes[3].matrix[0], 2.0);
        let body = model.nodes[2].mdt.as_ref().unwrap();
        assert_eq!(body.vertices.len(), 3);
        assert_eq!(body.materials[0].texture, "calc_face");
        assert_eq!(model.nodes[3].mdt.as_ref().unwrap().materials[0].texture, "");
        let pictures = im3::read(&found[0].1).unwrap();
        assert_eq!((pictures[0].name.as_str(), pictures[0].width, pictures[0].height), ("calc_face", 32, 16));
        assert_eq!((pictures[0].format, pictures[0].swizzled), (im3::Format::T8, true));
        // exported again: both meshes and the picture come back
        let exported = export(&built).unwrap();
        let paths: Vec<&str> = exported.iter().map(|(path, _)| path.as_str()).collect();
        assert_eq!(paths, ["calc.bin", "calc.gltf", "textures/calc_face.png"]);
        let png = crate::png::read_png(get(&exported, "textures/calc_face.png")).unwrap();
        let given = crate::png::read_png(&std::fs::read(folder.join("tex/calc_face.png")).unwrap()).unwrap();
        let rgba = |p: &crate::png::Png| -> Vec<u8> {
            if p.paletted {
                p.data.iter().flat_map(|&i| p.palette[i as usize]).collect()
            } else {
                p.data.clone()
            }
        };
        assert_eq!(rgba(&png), rgba(&given));
        let json = crate::from_gltf::load_json(std::str::from_utf8(get(&exported, "calc.gltf")).unwrap()).unwrap();
        let meshes: Vec<&str> = json
            .get("meshes")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m.get("name").unwrap().as_str().unwrap())
            .collect();
        assert_eq!(meshes, ["body", "key"]);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_new_pack_refuses_textures_the_game_cannot_wrap() {
        let folder = std::env::temp_dir().join(format!("dc2tk-assets-new-pack-refused-{}", std::process::id()));
        let gltf = calc_gltf(&folder);
        let face: Vec<u8> = [9u8, 9, 9, 255].repeat(48 * 16);
        std::fs::write(
            folder.join("tex/calc_face.png"),
            crate::png::write_png(48, 16, crate::png::PngMode::Rgba, &face, &[], &[]).unwrap(),
        )
        .unwrap();
        let error = new_pack(&gltf, &prop_template(), "prop", "calc").unwrap_err().to_string();
        assert!(error.contains("powers of two"), "{error}");
        std::fs::remove_dir_all(folder).unwrap();
    }

    /// Little-endian f32s and u16s for a test glTF's buffer.
    fn f32s(values: &[f32]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    /// A rigged glTF as Blender exports one: bones `hips` (10 up) and `arm` (5 above it) under an
    /// armature node `Rig` beside the skinned quad `Body` (model space; the lower corners on the
    /// hips, the upper 70% on the arm), textured, and an animation `wave` turning the arm 90
    /// degrees about z over a second. With `animated` false, no animation.
    fn rigged_gltf(folder: &Path, animated: bool) -> std::path::PathBuf {
        rigged_gltf_with(folder, animated, false)
    }

    /// `rigged_gltf`, with `vanishing` the quad's carriers `c2_fade_0` and `c2_visible` (shape keys
    /// that move nothing, at rest 0 and 1) and a second animation `vanish` driving them over a
    /// second: the fade 0, 0.5, 1 and the visibility 1, a float's hair under 1 (shown), 0 at 0,
    /// 0.5 and 1 s.
    fn rigged_gltf_with(folder: &Path, animated: bool, vanishing: bool) -> std::path::PathBuf {
        let mut bin = f32s(&[-1.0, 8.0, 0.0, 1.0, 8.0, 0.0, -1.0, 17.0, 0.0, 1.0, 17.0, 0.0]);
        bin.extend(f32s(&[0.0, 1.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0]));
        bin.extend([0u16, 1, 0, 0, 0, 1, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0].iter().flat_map(|v| v.to_le_bytes()));
        bin.extend(f32s(&[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.7, 0.3, 0.0, 0.0, 0.7, 0.3, 0.0, 0.0]));
        bin.extend([0u16, 1, 2, 2, 1, 3].iter().flat_map(|v| v.to_le_bytes()));
        // inverse bind matrices: the bones' world translations undone
        let mut ibm = Vec::new();
        for y in [-10.0f32, -15.0] {
            ibm.extend([1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, y, 0.0, 1.0]);
        }
        bin.extend(f32s(&ibm));
        bin.extend(f32s(&[0.0, 1.0]));
        let half = std::f32::consts::FRAC_PI_4;
        bin.extend(f32s(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, half.sin(), half.cos()]));
        let wave = r#"{"name": "wave", "channels": [{"sampler": 0, "target": {"node": 1, "path": "rotation"}}],
              "samplers": [{"input": 6, "output": 7, "interpolation": "LINEAR"}]}"#;
        let vanish = r#"{"name": "vanish", "channels": [{"sampler": 0, "target": {"node": 2, "path": "weights"}}],
              "samplers": [{"input": 9, "output": 10, "interpolation": "LINEAR"}]}"#;
        let animations = match (animated, vanishing) {
            (true, true) => format!(r#""animations": [{wave}, {vanish}],"#),
            (true, false) => format!(r#""animations": [{wave}],"#),
            (false, true) => format!(r#""animations": [{vanish}],"#),
            (false, false) => String::new(),
        };
        let (morph, targets, accessors, views) = if vanishing {
            bin.extend(f32s(&[0.0; 12]));
            bin.extend(f32s(&[0.0, 0.5, 1.0]));
            bin.extend(f32s(&[0.0, 1.0, 0.5, 0.9999999, 1.0, 0.0]));
            (
                r#""weights": [0, 1], "extras": {"targetNames": ["c2_fade_0", "c2_visible"]}, "#,
                r#", "targets": [{"POSITION": 8}, {"POSITION": 8}]"#,
                r#", {"bufferView": 8, "componentType": 5126, "count": 4, "type": "VEC3"},
               {"bufferView": 9, "componentType": 5126, "count": 3, "type": "SCALAR"},
               {"bufferView": 10, "componentType": 5126, "count": 6, "type": "SCALAR"}"#,
                r#", {"buffer": 0, "byteOffset": 356, "byteLength": 48}, {"buffer": 0, "byteOffset": 404, "byteLength": 12},
                 {"buffer": 0, "byteOffset": 416, "byteLength": 24}"#,
            )
        } else {
            ("", "", "", "")
        };
        let gltf = format!(
            r#"{{"asset": {{"version": "2.0"}}, "scene": 0, "scenes": [{{"nodes": [3]}}],
 "nodes": [{{"name": "hips", "translation": [0, 10, 0], "children": [1]}},
           {{"name": "arm", "translation": [0, 5, 0]}},
           {{"name": "Body", "mesh": 0, "skin": 0}},
           {{"name": "Rig", "children": [2, 0]}}],
 "skins": [{{"inverseBindMatrices": 5, "joints": [0, 1], "name": "Rig"}}],
 {animations}
 "meshes": [{{"name": "Body", {morph}"primitives": [{{"attributes": {{"POSITION": 0, "TEXCOORD_0": 1, "JOINTS_0": 2, "WEIGHTS_0": 3}}, "indices": 4, "material": 0{targets}}}]}}],
 "materials": [{{"name": "skin", "pbrMetallicRoughness": {{"baseColorTexture": {{"index": 0}}}}}}],
 "textures": [{{"source": 0}}], "images": [{{"name": "hero_skin", "uri": "hero_skin.png"}}],
 "accessors": [{{"bufferView": 0, "componentType": 5126, "count": 4, "type": "VEC3"}},
               {{"bufferView": 1, "componentType": 5126, "count": 4, "type": "VEC2"}},
               {{"bufferView": 2, "componentType": 5123, "count": 4, "type": "VEC4"}},
               {{"bufferView": 3, "componentType": 5126, "count": 4, "type": "VEC4"}},
               {{"bufferView": 4, "componentType": 5123, "count": 6, "type": "SCALAR"}},
               {{"bufferView": 5, "componentType": 5126, "count": 2, "type": "MAT4"}},
               {{"bufferView": 6, "componentType": 5126, "count": 2, "type": "SCALAR"}},
               {{"bufferView": 7, "componentType": 5126, "count": 2, "type": "VEC4"}}{accessors}],
 "bufferViews": [{{"buffer": 0, "byteOffset": 0, "byteLength": 48}}, {{"buffer": 0, "byteOffset": 48, "byteLength": 32}},
                 {{"buffer": 0, "byteOffset": 80, "byteLength": 32}}, {{"buffer": 0, "byteOffset": 112, "byteLength": 64}},
                 {{"buffer": 0, "byteOffset": 176, "byteLength": 12}}, {{"buffer": 0, "byteOffset": 188, "byteLength": 128}},
                 {{"buffer": 0, "byteOffset": 316, "byteLength": 8}}, {{"buffer": 0, "byteOffset": 324, "byteLength": 32}}{views}],
 "buffers": [{{"uri": "hero.bin", "byteLength": {}}}]}}"#,
            bin.len()
        );
        let _ = std::fs::remove_dir_all(folder);
        std::fs::create_dir_all(folder).unwrap();
        std::fs::write(folder.join("hero.bin"), bin).unwrap();
        let skin: Vec<u8> =
            (0..256).flat_map(|i| if i % 3 == 0 { [250, 200, 150, 255] } else { [40, 90, 200, 255] }).collect();
        std::fs::write(
            folder.join("hero_skin.png"),
            crate::png::write_png(16, 16, crate::png::PngMode::Rgba, &skin, &[], &[]).unwrap(),
        )
        .unwrap();
        std::fs::write(folder.join("hero.gltf"), gltf).unwrap();
        folder.join("hero.gltf")
    }

    #[test]
    fn a_new_character_has_its_own_skeleton_skin_and_motion_and_reads_back_the_same() {
        let folder = std::env::temp_dir().join(format!("dc2tk-assets-new-character-{}", std::process::id()));
        let path = rigged_gltf(&folder, true);
        let built = new_pack(&path, &prop_template(), "prop", "hero").unwrap();
        let found = members(&built).unwrap();
        let names: Vec<&str> = found.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["hero.img", "hero.mds", "hero.bbp", "hero.wgt", "hero.mot", "info.cfg"]);
        let cfg = String::from_utf8(get(&found, "info.cfg").to_vec()).unwrap();
        assert_eq!(
            cfg,
            "v2;\r\n\r\nNAME \"hero\";\r\n\r\nIMG 0, \"hero.img\";\r\nIMG_END;\r\n\r\nMATERIAL_ANIME 0;\r\nVERTEX_ANIME \"Body\";\r\nMODEL \"hero.mds\";\r\nBODY_SIZE 0.00, 0.00, 0.00;\r\n\r\n\
             MOTION 0, \"hero.mot\", \"hero.bbp\", \"hero.wgt\";\r\nKEY_START;\r\n\tKEY \"wave\", 1, 31, 1.00;\r\nKEY_END;\r\nMOTION_END;\r\n"
        );
        // the frames are the glTF's nodes; the skin weighs the quad's corners to the bones
        let model = crate::mds::parse(get(&found, "hero.mds")).unwrap();
        let frames: Vec<(&str, i32)> = model.nodes.iter().map(|node| (node.name.as_str(), node.parent)).collect();
        assert_eq!(frames, [("Rig", -1), ("Body", 0), ("hips", 0), ("arm", 2)]);
        let weights = crate::mot::parse(get(&found, "hero.wgt")).unwrap();
        let by_bone: Vec<(i32, i32, usize)> = weights.iter().map(|l| (l.frame, l.target, l.keys.len())).collect();
        assert_eq!(by_bone, [(1, 2, 4), (1, 3, 2)]);
        let bind = crate::mot::parse_bbp(get(&found, "hero.bbp")).unwrap();
        assert_eq!((bind.len(), bind[3][13]), (4, 15.0));
        // one rotation list, the arm's: the rest at 0 and a key a frame over the KEY's frames
        let lists = crate::mot::parse(get(&found, "hero.mot")).unwrap();
        assert_eq!(lists.len(), 1);
        assert_eq!((lists[0].frame, lists[0].kind, lists[0].keys.len()), (3, 0, 32));
        let w = lists[0].keys[31].1[0];
        assert!((w - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-6, "{w}");
        // read back: the pack's own export has the same meshes, skin and motion as the glTF given
        let exported = load_from_folder(&folder_of(&export(&built).unwrap()), "hero.gltf").unwrap();
        let given = load_from_disk(&path).unwrap();
        assert!(from_gltf::changed_character_meshes(&exported, &given, &model).unwrap().is_empty());
        let member = |name: &str| found.iter().find(|(n, _)| n == name).map(|(_, data)| data.clone());
        let unchanged = crate::motions::build(&exported, &given, &model, &member, cfg.as_bytes(), None).unwrap();
        assert!(unchanged.is_none());
        // and building the glTF given over the pack changes nothing
        assert_eq!(build(&folder, &built).unwrap(), built);
        // a skin needs an animation, which carries the bind pose
        let path = rigged_gltf(&folder, false);
        let error = new_pack(&path, &prop_template(), "prop", "hero").unwrap_err().to_string();
        assert!(error.contains("animation"), "{error}");
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_new_character_fades_and_hides_its_meshes_with_their_carrier_shape_keys() {
        let folder = std::env::temp_dir().join(format!("dc2tk-assets-new-character-fade-{}", std::process::id()));
        let path = rigged_gltf_with(&folder, true, true);
        let built = new_pack(&path, &prop_template(), "prop", "hero").unwrap();
        let found = members(&built).unwrap();
        let cfg = String::from_utf8(get(&found, "info.cfg").to_vec()).unwrap();
        assert!(
            cfg.contains("KEY_START;\r\n\tKEY \"wave\", 1, 31, 1.00;\r\n\tKEY \"vanish\", 32, 62, 1.00;\r\nKEY_END;"),
            "{cfg}"
        );
        // the body's material fade and visibility before the arm's rotation, keyed every frame
        let lists = crate::mot::parse(get(&found, "hero.mot")).unwrap();
        let kinds: Vec<(i32, i32, i32, usize)> =
            lists.iter().map(|l| (l.frame, l.kind, l.target, l.keys.len())).collect();
        assert_eq!(kinds, [(1, 40, 0, 63), (1, 50, 0, 63), (3, 0, 0, 63)]);
        let at = |list: &crate::mot::KeyList, frame: i64| list.keys.iter().find(|(f, _)| *f == frame).unwrap().1[0];
        let (fade, shown) = (&lists[0], &lists[1]);
        // at rest and over wave: opaque and shown; vanish fades to invisible and hides at its end
        // (shown exactly while the glTF holds it within a float's noise of 1: the engine hides
        // below 1)
        for frame in [0, 1, 20, 31] {
            assert_eq!((at(fade, frame), at(shown, frame)), (0.0, 1.0), "frame {frame}");
        }
        assert!((at(fade, 32 + 15) - 0.5).abs() < 1e-6 && at(shown, 32 + 15) == 1.0);
        assert!((at(fade, 62) - 1.0).abs() < 1e-6 && at(shown, 62).abs() < 1e-6);
        assert!(at(shown, 61) > 0.0 && at(shown, 61) < 1.0);
        // read back: the pack's export carries them as the same shape keys and motions
        let exported = load_from_folder(&folder_of(&export(&built).unwrap()), "hero.gltf").unwrap();
        let names = |gltf: &Gltf| -> Vec<String> {
            let meshes = gltf.json.get("meshes").unwrap().as_array().unwrap();
            let names = meshes[0].get("extras").and_then(|e| e.get("targetNames")).and_then(|n| n.as_array());
            names.into_iter().flatten().filter_map(|n| n.as_str().map(str::to_string)).collect()
        };
        assert_eq!(names(&exported), ["c2_fade_0", "c2_visible"]);
        let given = load_from_disk(&path).unwrap();
        let model = crate::mds::parse(get(&found, "hero.mds")).unwrap();
        let member = |name: &str| found.iter().find(|(n, _)| n == name).map(|(_, data)| data.clone());
        let unchanged = crate::motions::build(&exported, &given, &model, &member, cfg.as_bytes(), None).unwrap();
        assert!(unchanged.is_none());
        assert_eq!(build(&folder, &built).unwrap(), built);
        // a fade alone makes a KEY too
        let path = rigged_gltf_with(&folder, false, true);
        let built = new_pack(&path, &prop_template(), "prop", "hero").unwrap();
        let cfg = String::from_utf8(get(&members(&built).unwrap(), "info.cfg").to_vec()).unwrap();
        assert!(cfg.contains("KEY_START;\r\n\tKEY \"vanish\", 1, 31, 1.00;\r\nKEY_END;"), "{cfg}");
        std::fs::remove_dir_all(folder).unwrap();
    }

    /// The glTF as Blender's importer and exporter give a character back: extras dropped, every
    /// rigid mesh of a skinned model moved off its node onto a child node of its name (an object
    /// parented to its bone), names given twice numbered .001 on in the order the nodes were made
    /// (a skinned model's bones and objects apart), and the nodes in post-order (children first).
    pub(crate) fn blender_like(gltf: &Gltf) -> Gltf {
        use serde_json::Value;
        let mut json = gltf.json.clone();
        let strip = |value: &mut Value| {
            if let Value::Object(items) = value {
                items.retain(|key, _| key != "extras");
            }
        };
        let skinned = json.get("skins").and_then(Value::as_array).is_some_and(|skins| !skins.is_empty());
        let Some(Value::Array(mut nodes)) = json.get("nodes").cloned() else {
            return Gltf { json, buffers: gltf.buffers.clone() };
        };
        nodes.iter_mut().for_each(strip);
        for key in ["meshes", "materials"] {
            if let Some(Value::Array(list)) = json.get_mut(key) {
                list.iter_mut().for_each(strip);
            }
        }
        if skinned {
            for gi in 0..nodes.len() {
                if nodes[gi].get("mesh").is_some() && !nodes[gi].get("skin").is_some() {
                    let mesh = nodes[gi].get("mesh").unwrap().clone();
                    let name = nodes[gi].get("name").cloned().unwrap_or(Value::Null);
                    if let Value::Object(items) = &mut nodes[gi] {
                        items.retain(|key, _| key != "mesh");
                    }
                    nodes.push(serde_json::json!({"name": name, "mesh": mesh}));
                    let child = Value::from(nodes.len() as i64 - 1);
                    match nodes[gi].get_mut("children") {
                        Some(Value::Array(children)) => children.push(child),
                        _ => nodes[gi]["children"] = Value::Array(vec![child]),
                    }
                }
            }
        }
        let mut seen: BTreeMap<(bool, String), usize> = BTreeMap::new();
        for node in nodes.iter_mut() {
            let Some(name) = node.get("name").and_then(Value::as_str).map(str::to_string) else {
                continue;
            };
            // a skinned model's nodes are bones, named apart from objects; a rigid one's all objects
            let count = seen.entry((skinned && node.get("mesh").is_some(), name.clone())).or_insert(0);
            if *count > 0 {
                node["name"] = Value::String(format!("{name}.{:03}", *count));
            }
            *count += 1;
        }
        // post-order from the scene's roots
        let children = |node: &Value| -> Vec<usize> {
            node.get("children")
                .and_then(Value::as_array)
                .map_or(Vec::new(), |list| list.iter().filter_map(|c| c.as_i64()).map(|c| c as usize).collect())
        };
        let roots: Vec<usize> = json.get("scenes").and_then(Value::as_array).unwrap()[0]
            .get("nodes")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_i64().unwrap() as usize)
            .collect();
        let mut order = Vec::new();
        fn visit(at: usize, nodes: &[Value], children: &dyn Fn(&Value) -> Vec<usize>, order: &mut Vec<usize>) {
            for c in children(&nodes[at]) {
                visit(c, nodes, children, order);
            }
            order.push(at);
        }
        for root in roots {
            visit(root, &nodes, &children, &mut order);
        }
        let mut new_of = vec![0i64; nodes.len()];
        for (new, &old) in order.iter().enumerate() {
            new_of[old] = new as i64;
        }
        let renumber = |value: &mut Value| {
            if let Some(old) = value.as_i64() {
                *value = Value::from(new_of[old as usize]);
            }
        };
        let mut reordered: Vec<Value> = order.iter().map(|&old| nodes[old].clone()).collect();
        for node in reordered.iter_mut() {
            if let Some(Value::Array(list)) = node.get_mut("children") {
                list.iter_mut().for_each(renumber);
            }
        }
        json["nodes"] = Value::Array(reordered);
        if let Some(Value::Array(scenes)) = json.get_mut("scenes") {
            for scene in scenes {
                if let Some(Value::Array(list)) = scene.get_mut("nodes") {
                    list.iter_mut().for_each(renumber);
                }
            }
        }
        if let Some(Value::Array(skins)) = json.get_mut("skins") {
            for skin in skins {
                if let Some(Value::Array(list)) = skin.get_mut("joints") {
                    list.iter_mut().for_each(renumber);
                }
                if let Some(skeleton) = skin.get_mut("skeleton") {
                    renumber(skeleton);
                }
            }
        }
        if let Some(Value::Array(animations)) = json.get_mut("animations") {
            for animation in animations {
                if let Some(Value::Array(channels)) = animation.get_mut("channels") {
                    for channel in channels {
                        if let Some(node) = channel.get_mut("target").and_then(|t| t.get_mut("node")) {
                            renumber(node);
                        }
                    }
                }
            }
        }
        Gltf { json, buffers: gltf.buffers.clone() }
    }

    /// Changes each value of an attribute of the meshes of the nodes named `name` in place.
    pub(crate) fn edit_attribute(gltf: &mut Gltf, name: &str, attribute: &str, edit: &dyn Fn(&mut [f32])) {
        use serde_json::Value;
        let j = gltf.json.clone();
        let at = |list: &str, i: &Value| from_gltf::index(j.get(list).unwrap(), i).unwrap().clone();
        let nodes = j.get("nodes").unwrap().as_array().unwrap();
        for node in
            nodes.iter().filter(|n| n.get("name").and_then(Value::as_str) == Some(name) && n.get("mesh").is_some())
        {
            let mesh = at("meshes", node.get("mesh").unwrap());
            for primitive in mesh.get("primitives").unwrap().as_array().unwrap() {
                let accessor = at("accessors", primitive.get("attributes").unwrap().get(attribute).unwrap());
                let view = at("bufferViews", accessor.get("bufferView").unwrap());
                let base = view.get("byteOffset").and_then(|v| v.as_i64()).unwrap_or(0) as usize;
                let n = match accessor.get("type").unwrap().as_str().unwrap() {
                    "VEC2" => 2,
                    "VEC3" => 3,
                    _ => 4,
                };
                for k in 0..accessor.get("count").unwrap().as_i64().unwrap() as usize {
                    let bytes = &mut Arc::make_mut(&mut gltf.buffers[0])[base + 4 * n * k..base + 4 * n * (k + 1)];
                    let mut values: Vec<f32> =
                        bytes.chunks(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).collect();
                    edit(&mut values);
                    bytes.copy_from_slice(&f32s(&values));
                }
            }
        }
    }

    /// A pack of motions alone for Max (`MOTION 7`, no model) whose one KEY turns the hip of the
    /// character_pack skeleton, and the disc it is on (that pack as Max's `chara/c01_base.chr`).
    fn motion_pack() -> Vec<u8> {
        let turn = |f: i64| {
            let half = (5.0 * f as f64).to_radians() / 2.0;
            [half.cos(), half.sin(), 0.0, 0.0]
        };
        let mot =
            crate::mot::build(&[crate::mot::KeyList::new(1, 0, 0, (1..=8).map(|f| (f, turn(f))).collect())]).unwrap();
        let cfg = "v2;\nOBJECT_NAME2 0, \"hip\";\nMOTION 7, \"bow.mot\", \"\", \"\";\nKEY_START;\n\tKEY \"bow\", 1, 8, 0.50;\nKEY_END;\nMOTION_END;\n";
        pack(&[("info.cfg", cfg.as_bytes()), ("bow.mot", &mot)])
    }

    #[test]
    fn a_pack_of_motions_alone_is_exported_and_built_on_its_characters_skeleton() {
        let (motion, skeleton) = (motion_pack(), character_pack());
        assert!(motions_only(&motion) && !motions_only(&skeleton));
        let read = |path: &str| (path == "chara/c01_base.chr").then(|| skeleton.clone());
        let disc = DiscFiles { paths: vec!["chara/c01_base.chr".into(), "event/e1/e1_mints.chr".into()], read: &read };
        let (found, _) = skeleton_for("event/e1/e1_mints.chr", &motion, &disc).unwrap();
        assert_eq!(found, "chara/c01_base.chr");
        assert!(skeleton_for("event/e1/e1_guard.chr", &motion, &disc).is_err());
        let files = export_on_disc(&motion, "event/e1/e1_mints.chr", &disc).unwrap();
        let names: Vec<&str> = files.iter().map(|(path, _)| path.as_str()).collect();
        assert_eq!(names, ["bow.bin", "bow.gltf", "textures/body01.png"]);
        let gltf = load_from_folder(&folder_of(&files), "bow.gltf").unwrap();
        let animations = gltf.json.get("animations").unwrap().as_array().unwrap().clone();
        assert_eq!(animations.len(), 1);
        assert_eq!(animations[0].get("name").unwrap().as_str(), Some("bow"));
        // unedited: the same pack
        let folder = written(&files, "motion-unedited");
        assert_eq!(build_on_disc(&folder, &motion, "event/e1/e1_mints.chr", &disc).unwrap(), motion);
        std::fs::remove_dir_all(folder).unwrap();
        // the hip's turn doubled: bow.mot alone changes, over the KEY's frames
        let mut edited = gltf;
        let channel = &animations[0].get("channels").unwrap().as_array().unwrap()[0];
        let sampler =
            from_gltf::index(animations[0].get("samplers").unwrap(), channel.get("sampler").unwrap()).unwrap().clone();
        let output = from_gltf::accessor(&edited, sampler.get("output").unwrap()).unwrap();
        let view_of = |gltf: &Gltf| {
            let accessor =
                from_gltf::index(gltf.json.get("accessors").unwrap(), sampler.get("output").unwrap()).unwrap();
            let view =
                from_gltf::index(gltf.json.get("bufferViews").unwrap(), accessor.get("bufferView").unwrap()).unwrap();
            view.get("byteOffset").and_then(|v| v.as_i64()).unwrap_or(0) as usize
        };
        let base = view_of(&edited);
        for k in 0..output.len() {
            let q: Vec<f64> = output.at(k as i64).unwrap().iter().map(|v| v.f()).collect();
            // twice the angle: the square of the quaternion (x, 0, 0, w)
            let doubled = [2.0 * q[0] * q[3], 0.0, 0.0, q[3] * q[3] - q[0] * q[0]];
            for (i, x) in doubled.iter().enumerate() {
                let at = base + 16 * k + 4 * i;
                Arc::make_mut(&mut edited.buffers[0])[at..at + 4].copy_from_slice(&(*x as f32).to_le_bytes());
            }
        }
        let mut changed = files.clone();
        for (path, content) in changed.iter_mut() {
            if path == "bow.bin" {
                *content = edited.buffers[0].to_vec();
            }
        }
        let folder = written(&changed, "motion-edited");
        let built = build_on_disc(&folder, &motion, "event/e1/e1_mints.chr", &disc).unwrap();
        std::fs::remove_dir_all(folder).unwrap();
        let info = ["info.cfg".to_string()];
        assert_eq!(rebuild::read_member(&built, &info), rebuild::read_member(&motion, &info));
        let lists = crate::mot::parse(rebuild::read_member(&built, &["bow.mot".to_string()]).unwrap()).unwrap();
        let w = lists[0].keys.iter().find(|k| k.0 == 8).unwrap().1[0];
        assert!((w - 40f64.to_radians().cos()).abs() < 1e-5, "{w}");
    }

    /// A character pack: bones root and hip (10 up), a skinned quad `skin` (its lower corners on the
    /// root, the upper 40% root and 60% hip, and an empty weight block for the hat's bone as the
    /// disc's files have), and a rigid triangle `hat` on frame `hat` 5 above the hip.
    fn character_pack() -> Vec<u8> {
        let mut model = Mds::new();
        model.nodes.push(Node::new("root", -1));
        let mut hip = Node::new("hip", 0);
        hip.matrix[13] = 10.0;
        model.nodes.push(hip);
        let mut skin = quad(0.0, "body01");
        skin.vertices = vec![[0.0, 0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [0.0, 12.0, 0.0, 1.0], [1.0, 12.0, 0.0, 1.0]];
        model.nodes.push(Node { mdt: Some(skin), ..Node::new("skin", 0) });
        let mut hat = Node { mdt: Some(quad(0.0, "body01")), ..Node::new("hat", 1) };
        hat.matrix[13] = 5.0;
        model.nodes.push(hat);
        let wgt = crate::mot::build(&[
            crate::mot::KeyList::new(
                2,
                0,
                20,
                vec![
                    (0, [100.0, 0.0, 0.0, 0.0]),
                    (1, [100.0, 0.0, 0.0, 0.0]),
                    (2, [40.0, 0.0, 0.0, 0.0]),
                    (3, [40.0, 0.0, 0.0, 0.0]),
                ],
            ),
            crate::mot::KeyList::new(2, 1, 20, vec![(2, [60.0, 0.0, 0.0, 0.0]), (3, [60.0, 0.0, 0.0, 0.0])]),
            crate::mot::KeyList::new(2, 3, 20, Vec::new()),
        ])
        .unwrap();
        let mut bind = model.world_matrices().unwrap();
        bind[1][13] = 9.5; // the bind pose differs from the rest, as the disc's often do
        let mut picture = im3::Image::new("body01", im3::Format::Ct32, 8, 8);
        picture.pixels = Some([200, 100, 50, 0x80].repeat(64));
        let cfg = "v2;\nVERTEX_ANIME \"skin\";\nMODEL \"c.mds\";\nMOTION 0, \"\", \"c.bbp\", \"c.wgt\";\nMOTION_END;\n";
        pack(&[
            ("info.cfg", cfg.as_bytes()),
            ("c.mds", &mds::build(&model).unwrap()),
            ("c.wgt", &wgt),
            ("c.bbp", &crate::mot::build_bbp(&bind).unwrap()),
            ("c.img", &im3::write(&[picture], None).unwrap()),
        ])
    }

    /// Every character pack of an extracted disc (DC2TK_CORPUS) whose model exports: its export as
    /// Blender gives it back (blender_like) changes no mesh; one mesh moved changes that mesh
    /// alone, and rebuilt it leaves every other frame and weight as they were and keeps its own
    /// vertices' weights.
    #[test]
    #[ignore = "needs DC2TK_CORPUS"]
    fn corpus_characters_compare_by_value() {
        use rayon::prelude::*;
        let corpus = dc2tk_disc::samples::corpus();
        let packs: Vec<std::path::PathBuf> = crate::files_under(Path::new(&corpus))
            .unwrap()
            .into_iter()
            .filter(|path| path.extension().is_some_and(|e| e.eq_ignore_ascii_case("chr")))
            .collect();
        let check = |path: &std::path::PathBuf| -> std::result::Result<bool, String> {
            let fail = |what: String| format!("{}: {what}", path.display());
            let data = std::fs::read(path).unwrap();
            let stem = path.file_stem().unwrap().to_string_lossy().to_string();
            let (pack, _) = crate::tree::model_pack(&data, &stem);
            let Ok(files) = export(&pack) else {
                return Ok(false);
            };
            let (source, names) = unpacked(&pack).unwrap();
            let template = source.get(&names.model).unwrap();
            let model = mds::parse(template).unwrap();
            let exported = load_from_folder(&folder_of(&files), &format!("{}.gltf", names.stem)).unwrap();
            let blender = blender_like(&exported);
            let changed =
                from_gltf::changed_character_meshes(&exported, &blender, &model).map_err(|e| fail(e.to_string()))?;
            if !changed.is_empty() {
                let named: Vec<&str> = changed
                    .iter()
                    .map(|id| match id {
                        MeshId::Frame(f) => model.nodes[*f].name.as_str(),
                        MeshId::New(name) => name.as_str(),
                    })
                    .collect();
                return Err(fail(format!("unedited, changed {named:?}")));
            }
            // the first mesh with a triangle, moved
            let geometry = from_gltf::frame_geometry(&blender, &model).unwrap();
            let ids = from_gltf::mesh_ids(&blender.json, &model).unwrap();
            let Some((gi, id)) = ids.into_iter().find(|(_, id)| geometry.get(id).is_some_and(|g| !g.groups.is_empty()))
            else {
                return Ok(true);
            };
            let node_name = blender.json.get("nodes").unwrap().as_array().unwrap()[gi]
                .get("name")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string();
            let mut moved = blender_like(&exported);
            edit_attribute(&mut moved, &node_name, "POSITION", &|p| p[0] += 3.0);
            let changed = from_gltf::changed_character_meshes(&exported, &moved, &model).unwrap();
            if changed != BTreeSet::from([id.clone()]) {
                return Err(fail(format!("{node_name} moved, changed {changed:?}")));
            }
            let pose = names.pose.as_str().and_then(|name| source.get(name));
            let weights = source.get(&names.weights).filter(|w| !w.is_empty());
            let import = from_gltf::ImportOptions { bbp: pose, wgt: weights, meshes: Some(&changed), collision: false };
            let built = from_gltf::import_gltf(&moved, template, &import).map_err(|e| fail(e.to_string()))?;
            let MeshId::Frame(frame) = id else {
                return Err(fail(format!("{node_name} stands for no frame")));
            };
            let rebuilt = mds::parse(&built.mds).unwrap();
            for (i, node) in model.nodes.iter().enumerate() {
                if i != frame && rebuilt.nodes[i] != *node {
                    return Err(fail(format!("{node_name} moved, frame {} changed", node.name)));
                }
            }
            if let Some(weights) = weights {
                let (old, new) = (crate::mot::parse(weights).unwrap(), crate::mot::parse(&built.wgt).unwrap());
                let others = |lists: &[crate::mot::KeyList]| -> Vec<crate::mot::KeyList> {
                    lists.iter().filter(|l| l.frame != frame as i32 && !l.keys.is_empty()).cloned().collect()
                };
                if others(&old) != others(&new) {
                    return Err(fail(format!("{node_name} moved, other meshes' weights changed")));
                }
                // its vertices' skins, rounded to a tenth of a percent: those rebuilt are the original's
                let vertices = model.nodes[frame].mdt.as_ref().map_or(0, |t| t.vertices.len());
                let skins = |lists: &[crate::mot::KeyList], count: usize| -> BTreeSet<Vec<(i64, i64)>> {
                    to_gltf::vertex_weights(lists, frame, count)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|mut skin| {
                            skin.sort_by_key(|(bone, _)| *bone);
                            skin.into_iter().map(|(bone, w)| (bone, (w * 1000.0).round() as i64)).collect()
                        })
                        .collect()
                };
                let count = rebuilt.nodes[frame].mdt.as_ref().map_or(0, |t| t.vertices.len());
                let (before, after) = (skins(&old, vertices), skins(&new, count));
                if !after.is_subset(&before) {
                    return Err(fail(format!("{node_name} moved, its weights changed: {after:?} not in {before:?}")));
                }
            }
            Ok(true)
        };
        let results: Vec<std::result::Result<bool, String>> = packs.par_iter().map(check).collect();
        let failures: Vec<&String> = results.iter().filter_map(|r| r.as_ref().err()).collect();
        let checked = results.iter().filter(|r| matches!(r, Ok(true))).count();
        assert!(
            failures.is_empty(),
            "{} of {} packs: {:#?}",
            failures.len(),
            checked + failures.len(),
            &failures[..failures.len().min(20)]
        );
        assert!(checked > 1500, "{checked} packs checked");
    }

    /// The folder of a glTF: the export's files with the glTF and its buffer replaced.
    fn folder_with(exported: &[(String, Vec<u8>)], gltf: &Gltf, name: &str) -> std::path::PathBuf {
        let mut files: Vec<(String, Vec<u8>)> = exported.to_vec();
        for (path, content) in files.iter_mut() {
            if *path == "c.gltf" {
                *content = serde_json::to_string(&gltf.json).expect("a JSON value serializes").into_bytes();
            } else if *path == "c.bin" {
                *content = gltf.buffers[0].to_vec();
            }
        }
        written(&files, name)
    }

    #[test]
    fn a_character_through_blender_rebuilds_only_the_mesh_edited_with_its_weights() {
        let pack = character_pack();
        let exported = export(&pack).unwrap();
        let blender = blender_like(&load_from_folder(&folder_of(&exported), "c.gltf").unwrap());
        // unedited: the same pack, bind pose, empty weight blocks and all
        let folder = folder_with(&exported, &blender, "blender-unedited");
        assert_eq!(build(&folder, &pack).unwrap(), pack);
        std::fs::remove_dir_all(folder).unwrap();
        // weights renormalised by a hair: still the same
        let mut renormalised = blender_like(&load_from_folder(&folder_of(&exported), "c.gltf").unwrap());
        edit_attribute(&mut renormalised, "skin_skin", "WEIGHTS_0", &|w| {
            let total: f32 = w.iter().map(|x| x * 1.003).sum();
            w.iter_mut().for_each(|x| *x = *x * 1.003 / total);
        });
        let folder = folder_with(&exported, &renormalised, "blender-renormalised");
        assert_eq!(build(&folder, &pack).unwrap(), pack);
        std::fs::remove_dir_all(folder).unwrap();
        let original = mds::parse(rebuild::read_member(&pack, &["c.mds".to_string()]).unwrap()).unwrap();
        let weights_of =
            |built: &[u8]| crate::mot::parse(rebuild::read_member(built, &["c.wgt".to_string()]).unwrap()).unwrap();
        // the hat moved: the hat alone is rebuilt, in its frame's space
        let mut moved = blender_like(&load_from_folder(&folder_of(&exported), "c.gltf").unwrap());
        edit_attribute(&mut moved, "hat", "POSITION", &|p| p[0] += 2.0);
        let folder = folder_with(&exported, &moved, "blender-hat");
        let built = build(&folder, &pack).unwrap();
        std::fs::remove_dir_all(folder).unwrap();
        let model = mds::parse(rebuild::read_member(&built, &["c.mds".to_string()]).unwrap()).unwrap();
        assert_eq!(&model.nodes[..3], &original.nodes[..3]);
        let xs: BTreeSet<i64> =
            model.nodes[3].mdt.as_ref().unwrap().vertices.iter().map(|v| v[0].round() as i64).collect();
        assert_eq!(xs, BTreeSet::from([2, 3]));
        assert_eq!(weights_of(&built), weights_of(&pack));
        // a weight painted: the skin alone is rebuilt, its weights as given, the empty block gone
        let mut painted = blender_like(&load_from_folder(&folder_of(&exported), "c.gltf").unwrap());
        edit_attribute(&mut painted, "skin_skin", "WEIGHTS_0", &|w| {
            if (w[0] - 0.4).abs() < 1e-3 {
                w[0] = 0.2;
                w[1] = 0.8;
            }
        });
        let folder = folder_with(&exported, &painted, "blender-painted");
        let built = build(&folder, &pack).unwrap();
        std::fs::remove_dir_all(folder).unwrap();
        let model = mds::parse(rebuild::read_member(&built, &["c.mds".to_string()]).unwrap()).unwrap();
        assert_eq!(model.nodes[3], original.nodes[3]);
        let mut skins: Vec<(i64, Vec<(i32, i64)>)> = Vec::new();
        let mesh = model.nodes[2].mdt.as_ref().unwrap();
        for (v, vertex) in mesh.vertices.iter().enumerate() {
            let mut skin: Vec<(i32, i64)> = weights_of(&built)
                .iter()
                .flat_map(|l| l.keys.iter().filter(|k| k.0 == v as i64).map(move |k| (l.target, k.1[0].round() as i64)))
                .collect();
            skin.sort();
            skins.push((vertex[1].round() as i64, skin));
        }
        skins.sort();
        assert_eq!(
            skins,
            [(0, vec![(0, 100)]), (0, vec![(0, 100)]), (12, vec![(0, 20), (1, 80)]), (12, vec![(0, 20), (1, 80)])]
        );
        assert!(weights_of(&built).iter().all(|l| !l.keys.is_empty()));
        // the bind pose kept
        let bbp = ["c.bbp".to_string()];
        assert_eq!(rebuild::read_member(&built, &bbp), rebuild::read_member(&pack, &bbp));
    }

    /// A quad of two triangles at x offset `x`, as a strip, textured with material 0.
    fn quad(x: f64, texture: &str) -> Mdt {
        Mdt {
            vertices: vec![[x, 0.0, 0.0, 1.0], [x + 1.0, 0.0, 0.0, 1.0], [x, 1.0, 0.0, 1.0], [x + 1.0, 1.0, 0.0, 1.0]],
            normals: vec![[0.0, 0.0, 1.0, 1.0]],
            uvs: vec![[0.0, 0.0, 1.0, 0.0], [2.0, 0.0, 1.0, 0.0], [0.0, 2.0, 1.0, 0.0], [2.0, 2.0, 1.0, 0.0]],
            materials: vec![Material::new(texture)],
            prims: vec![Prim { kind: mds::PRIM_STRIP, material: 0, verts: (0..4).map(|i| vec![i, 0, i]).collect() }],
            ..Mdt::default()
        }
    }

    /// A map piece whose two meshes share the base name `grid54` and differ in their flags, and the
    /// texture file their pictures are in (one repeating, one clamped).
    fn map_piece() -> (Vec<u8>, Vec<u8>) {
        let mut model = Mds::new();
        model.nodes.push(Node::new("piece", -1));
        model.nodes.push(Node { mdt: Some(quad(0.0, "wall01")), ..Node::new("grid54--f2appz0n1t-5_1", 0) });
        model.nodes.push(Node { mdt: Some(quad(2.0, "leaf01")), ..Node::new("grid54--a40aof-6_1", 0) });
        model.nodes.push(Node { mdt: Some(quad(4.0, "wall01")), ..Node::new("lamp--n1z0c0f2app-", 0) });
        let mut wall = im3::Image::new("wall01", im3::Format::Ct32, 8, 8);
        wall.pixels = Some([10, 200, 30, 0x80].repeat(64));
        wall.clamp = Some(0);
        let mut leaf = im3::Image::new("leaf01", im3::Format::Ct32, 8, 8);
        leaf.pixels = Some([0, 90, 0, 0x40].repeat(64));
        (mds::build(&model).unwrap(), im3::write(&[wall, leaf], None).unwrap())
    }

    fn folder_of(files: &[(String, Vec<u8>)]) -> Folder {
        let mut folder = Folder::new();
        for (path, content) in files {
            folder.write(path, content.clone());
        }
        folder
    }

    /// The glTF of an export's files, with its buffer's positions of node `node` moved 1 along x.
    fn moved(files: &[(String, Vec<u8>)], node: usize) -> Gltf {
        let folder = folder_of(files);
        let gltf = load_from_folder(&folder, "piece.gltf").unwrap();
        let j = &gltf.json;
        let at = |list: &str, i: &serde_json::Value| from_gltf::index(j.get(list).unwrap(), i).unwrap().clone();
        let mesh = at(
            "meshes",
            from_gltf::index(j.get("nodes").unwrap(), &(node as i64).into()).unwrap().get("mesh").unwrap(),
        );
        let mut bin = folder.get("piece.bin").unwrap().to_vec();
        for primitive in mesh.get("primitives").unwrap().as_array().unwrap() {
            let accessor = at("accessors", primitive.get("attributes").unwrap().get("POSITION").unwrap());
            let view = at("bufferViews", accessor.get("bufferView").unwrap());
            let base = view.get("byteOffset").and_then(|v| v.as_i64()).unwrap_or(0) as usize;
            for k in 0..accessor.get("count").unwrap().as_i64().unwrap() as usize {
                let x = f32::from_le_bytes(bin[base + 12 * k..base + 12 * k + 4].try_into().unwrap());
                bin[base + 12 * k..base + 12 * k + 4].copy_from_slice(&(x + 1.0).to_le_bytes());
            }
        }
        Gltf { json: gltf.json.clone(), buffers: vec![Arc::new(bin)] }
    }

    #[test]
    fn a_map_piece_exports_whole_names_real_textures_and_blend_flags() {
        let (piece, img) = map_piece();
        let search = to_gltf::TextureSearch::new(&[("piece.img".to_string(), img.clone())]).unwrap();
        let (files, found) = export_map_model(&piece, "piece", &to_gltf::Options::map(Some(&search))).unwrap();
        assert!(found.missing_textures.is_empty());
        let paths: Vec<&str> = files.iter().map(|(path, _)| path.as_str()).collect();
        assert_eq!(paths, ["piece.bin", "piece.gltf", "textures/leaf01.png", "textures/wall01.png"]);
        let pictures = im3::read(&img).unwrap();
        assert_eq!(get(&files, "textures/wall01.png"), im3::to_png(&pictures[0]).unwrap());
        let json = crate::from_gltf::load_json(std::str::from_utf8(get(&files, "piece.gltf")).unwrap()).unwrap();
        let list = |key: &str| json.get(key).unwrap().as_array().unwrap().clone();
        let names: Vec<String> =
            list("nodes").iter().map(|n| n.get("name").unwrap().as_str().unwrap().to_string()).collect();
        assert_eq!(names, ["piece", "grid54--f2appz0n1t-5_1", "grid54--a40aof-6_1", "lamp--n1z0c0f2app-"]);
        for (i, node) in list("nodes").iter().enumerate() {
            assert_eq!(node.get("extras").unwrap().get("mds_index").unwrap().as_i64(), Some(i as i64));
        }
        let meshes: Vec<String> =
            list("meshes").iter().map(|n| n.get("name").unwrap().as_str().unwrap().to_string()).collect();
        assert_eq!(meshes, names[1..]);
        // wall01 repeats (CLAMP 0), leaf01 clamps (CLAMP 5)
        let wrap = |texture: &str| {
            let t = list("textures").into_iter().find(|t| t.get("name").unwrap().as_str() == Some(texture)).unwrap();
            let s = &list("samplers")[t.get("sampler").unwrap().as_i64().unwrap() as usize];
            (s.get("wrapS").unwrap().as_i64().unwrap(), s.get("wrapT").unwrap().as_i64().unwrap())
        };
        assert_eq!(wrap("wall01"), (10497, 10497));
        assert_eq!(wrap("leaf01"), (33071, 33071));
        // blend flags: additive -> BLEND, alpha test 0x40 with aOF -> MASK 0.5, c0 -> unlit
        let material_of = |node: usize| {
            let mesh = &list("meshes")[list("nodes")[node].get("mesh").unwrap().as_i64().unwrap() as usize];
            let number =
                mesh.get("primitives").unwrap().as_array().unwrap()[0].get("material").unwrap().as_i64().unwrap();
            list("materials")[number as usize].clone()
        };
        let mode = |m: &serde_json::Value| m.get("alphaMode").and_then(|v| v.as_str()).map(str::to_string);
        assert_eq!(mode(&material_of(1)).as_deref(), Some("BLEND"));
        assert_eq!(mode(&material_of(2)).as_deref(), Some("MASK"));
        assert_eq!(material_of(2).get("alphaCutoff").unwrap().as_f64(), Some(0.5));
        assert!(material_of(3).get("extensions").unwrap().get("KHR_materials_unlit").is_some());
        assert!(!material_of(1).get("extensions").is_some());
        let flags = material_of(3).get("extras").unwrap().get("frame_flags").unwrap().clone();
        assert_eq!(flags.get("mds_flags").unwrap().as_str(), Some("n1z0c0f2app-"));
        assert_eq!(flags.get("z_write").unwrap().as_bool(), Some(false));
        assert_eq!(list("extensionsUsed")[0].as_str(), Some("KHR_materials_unlit"));
    }

    #[test]
    fn a_map_piece_edit_reaches_only_its_frame_though_base_names_repeat() {
        let (piece, img) = map_piece();
        let search = to_gltf::TextureSearch::new(&[("piece.img".to_string(), img)]).unwrap();
        let options = to_gltf::Options::map(Some(&search));
        let (files, _) = export_map_model(&piece, "piece", &options).unwrap();
        // unedited: the same bytes
        let unedited = load_from_folder(&folder_of(&files), "piece.gltf").unwrap();
        assert_eq!(build_map_model(&unedited, &piece, &options).unwrap(), piece);
        let original = mds::parse(&piece).unwrap();
        let check = |edited: &Gltf| {
            let built = mds::parse(&build_map_model(edited, &piece, &options).unwrap()).unwrap();
            assert_eq!(built.nodes.len(), 4);
            for i in [0, 1, 3] {
                assert_eq!(built.nodes[i], original.nodes[i], "frame {i}");
            }
            let mesh = built.nodes[2].mdt.as_ref().unwrap();
            assert_eq!(built.nodes[2].name, "grid54--a40aof-6_1");
            let xs: BTreeSet<i64> = mesh.vertices.iter().map(|v| v[0] as i64).collect();
            assert_eq!(xs, BTreeSet::from([3, 4]));
            assert_eq!(mesh.materials[0].texture, "leaf01");
        };
        // by extras.mds_index
        let edited = moved(&files, 2);
        check(&edited);
        // and by the whole names when the extras are lost (a tool that drops custom properties)
        let mut bare = edited;
        if let Some(serde_json::Value::Array(nodes)) = bare.json.get_mut("nodes") {
            for node in nodes {
                if let serde_json::Value::Object(items) = node {
                    items.retain(|key, _| key != "extras");
                }
            }
        }
        check(&bare);
    }

    /// A floor collision piece: two frames, materials encoding attributes the way the disc's do
    /// (zero as 1.0 or 0.0; foot sound 2 as 2.857143).
    fn collision_piece() -> Vec<u8> {
        let mut model = Mds::new();
        model.nodes.push(Node::new("col", -1));
        for (k, name) in ["floor01", "floor02"].iter().enumerate() {
            let x = 2.0 * k as f64;
            let mut floor = Material::new("");
            floor.diffuse = vec![1.0, 2.857143, 8.571428, -11.0];
            let mut water = Material::new("");
            water.diffuse = vec![0.0, 0.0, 1.4285715, 1.0];
            let mesh = Mdt {
                vertices: vec![
                    [x, 0.0, 0.0, 1.0],
                    [x + 1.0, 0.0, 0.0, 1.0],
                    [x, 0.0, 1.0, 1.0],
                    [x + 1.0, 0.0, 1.0, 1.0],
                ],
                normals: vec![[0.0, 1.0, 0.0, 1.0]],
                uvs: vec![[0.0, 0.0, 1.0, 0.0]],
                materials: vec![floor, water],
                prims: vec![
                    Prim { kind: from_gltf::COLLISION_PRIM, material: 0, verts: vec![vec![0], vec![2], vec![1]] },
                    Prim { kind: from_gltf::COLLISION_PRIM, material: 1, verts: vec![vec![1], vec![2], vec![3]] },
                ],
                ..Mdt::default()
            };
            model.nodes.push(Node { mdt: Some(mesh), ..Node::new(name, 0) });
        }
        mds::build(&model).unwrap()
    }

    #[test]
    fn collision_exports_its_attributes_and_builds_back_by_the_engines_rules() {
        let piece = collision_piece();
        let options = to_gltf::Options::collision();
        let (files, _) = export_map_model(&piece, "piece", &options).unwrap();
        let json = crate::from_gltf::load_json(std::str::from_utf8(get(&files, "piece.gltf")).unwrap()).unwrap();
        let names: Vec<&str> = json
            .get("materials")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m.get("name").unwrap().as_str().unwrap())
            .collect();
        assert_eq!(names, ["col_ground0_foot2_area6_mask12", "col_ground0_foot0_area1_mask0"]);
        assert!(!files.iter().any(|(path, _)| path.starts_with("textures/")));
        let mut gltf = load_from_folder(&folder_of(&files), "piece.gltf").unwrap();
        assert_eq!(build_map_model(&gltf, &piece, &options).unwrap(), piece);
        // renamed as Blender keeps duplicates apart: the floor becomes foot sound 5, area 9; and the
        // mesh gains normals and uvs, as Blender writes them
        if let Some(serde_json::Value::Array(materials)) = gltf.json.get_mut("materials") {
            materials[0]["name"] = "col_ground0_foot5_area9_mask4.001".into();
        }
        if let Some(serde_json::Value::Array(meshes)) = gltf.json.get_mut("meshes") {
            if let Some(serde_json::Value::Array(primitives)) = meshes[0].get_mut("primitives") {
                for p in primitives {
                    let position = p.get("attributes").unwrap().get("POSITION").unwrap().clone();
                    if let Some(attributes) = p.get_mut("attributes") {
                        attributes["NORMAL"] = position.clone();
                        attributes["TEXCOORD_0"] = position;
                    }
                }
            }
        }
        let built = mds::parse(&build_map_model(&gltf, &piece, &options).unwrap()).unwrap();
        let original = mds::parse(&piece).unwrap();
        assert!(from_gltf::collision_problems(&built).is_empty());
        // the material is shared by both frames' floor triangles: both rebuilt, as 0x213 only
        for frame in [1, 2] {
            let mesh = built.nodes[frame].mdt.as_ref().unwrap();
            assert!(mesh.prims.iter().all(|p| p.kind == 0x213 && p.verts.iter().all(|v| v.len() == 1)));
            let attributes: BTreeSet<to_gltf::CollisionAttributes> =
                mesh.materials.iter().map(|m| to_gltf::CollisionAttributes::decode(&m.diffuse)).collect();
            let wanted = [(0, 5, 9, 4), (0, 0, 1, 0)].map(|(ground, foot, area, mask)| to_gltf::CollisionAttributes {
                ground,
                foot,
                area,
                mask,
            });
            assert_eq!(attributes, BTreeSet::from(wanted));
            assert_eq!(mesh.prims.iter().map(|p| p.verts.len()).sum::<usize>(), 6);
        }
        assert_eq!(built.nodes[0], original.nodes[0]);
    }
}
