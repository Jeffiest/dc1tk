//! Character models as glTF for mods: a folder named after the pack plus .gltf (c02a.chr.gltf/)
//! holding what an export writes: c02a.gltf with its .bin, and textures/ with a PNG per texture the
//! model uses (the pack's own pictures where it has them). build rebuilds the mesh, weights and bind
//! pose from the edited glTF with the disc's model as the template, and puts texture PNGs that
//! differ from the export into the pack's texture files; parts left as exported change nothing.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use c2_disc::{archive, rebuild};

use crate::folder::Folder;
use crate::from_gltf::{self, Gltf};
use crate::info_cfg;
use crate::json::Value as Arg;
use crate::{bail, im3, textures, to_gltf, Error, Result};

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

/// The member names of the pack's model, weights and bind pose, from its info.cfg.
pub struct ModelFiles {
    pub model: String,
    pub weights: String,
    pub pose: Arg,
    pub stem: String,
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
        Some(model) if model.truthy() => model,
        _ => bail!("the pack's info.cfg names no model"),
    };
    let Some(model) = model.as_str() else {
        bail!("expected str, bytes or os.PathLike object");
    };
    let stem = path_stem(model).to_string();
    let pose = info.motion0().map_or(Arg::Null, |m| m.bbp.clone());
    Ok(ModelFiles { model: model.to_string(), weights: format!("{stem}.wgt"), pose, stem })
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
                    let png = im3::to_png(&image, true)?;
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

/// The files under a folder by relative path, sorted as the Python lists them.
fn folder_files(folder: &Path) -> Result<Vec<(String, Vec<u8>)>> {
    crate::files_under(folder)?
        .into_iter()
        .map(|path| Ok((crate::relative_posix(&path, folder), std::fs::read(&path)?)))
        .collect()
}

/// {mesh name: what the mesh holds}: its node's transform and skin, and its primitives' vertex data
/// and materials by value, so a tool that writes the same mesh another way still matches.
fn mesh_contents(gltf: &Gltf) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    let empty = Vec::new();
    let nodes = match gltf.json.get("nodes") {
        Some(nodes) => nodes.as_list().ok_or_else(|| Error::invalid("'nodes' is not a list"))?,
        None => &empty,
    };
    for node in nodes {
        let Some(mesh_index) = node.get("mesh") else {
            continue;
        };
        let mesh = from_gltf::index(from_gltf::item(&gltf.json, "meshes")?, mesh_index)?;
        let name_value = node.get("name").filter(|v| v.truthy()).or_else(|| mesh.get("name"));
        let mut name = from_gltf::clean(name_value)?;
        if let Some(stem) = name.strip_suffix("_skin") {
            name = stem.to_string();
        }
        let transform = crate::json::Value::Object(
            ["translation", "rotation", "scale", "matrix", "skin"]
                .iter()
                .map(|key| (key.to_string(), node.get(key).cloned().unwrap_or(crate::json::Value::Null)))
                .collect(),
        );
        let mut parts = vec![transform.sorted_text()];
        let Some(primitives) = from_gltf::item(mesh, "primitives")?.as_list() else {
            bail!("'primitives' is not a list");
        };
        for primitive in primitives {
            let crate::json::Value::Object(attributes) = from_gltf::item(primitive, "attributes")? else {
                bail!("'attributes' is not an object");
            };
            let mut sorted: Vec<&(String, crate::json::Value)> = attributes.iter().collect();
            sorted.sort_by(|a, b| a.0.cmp(&b.0));
            let mut indices: Vec<&crate::json::Value> = sorted.iter().map(|(_, index)| index).collect();
            if let Some(index) = primitive.get("indices") {
                indices.push(index);
            }
            for index in indices {
                if !matches!(index, crate::json::Value::Null) {
                    parts.push(from_gltf::accessor(gltf, index)?.repr());
                }
            }
            let material = match primitive.get("material") {
                Some(number) => from_gltf::index(from_gltf::item(&gltf.json, "materials")?, number)?.clone(),
                None => crate::json::Value::Null,
            };
            parts.push(material.sorted_text());
        }
        out.insert(name, parts.join("\n"));
    }
    Ok(out)
}

/// A glTF read from a folder on disk.
fn load_from_disk(path: &Path) -> Result<Gltf> {
    let folder = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    from_gltf::load_gltf(&std::fs::read(path)?, &|uri| Ok(std::fs::read(folder.join(uri))?))
}

/// A glTF read from the export's files.
fn load_from_folder(files: &Folder, name: &str) -> Result<Gltf> {
    let Some(raw) = files.get(name) else {
        bail!("[Errno 2] No such file or directory: {}", im3::str_repr(name));
    };
    from_gltf::load_gltf(raw, &|uri| {
        files
            .get(uri)
            .map(<[u8]>::to_vec)
            .ok_or_else(|| Error::invalid(format!("[Errno 2] No such file or directory: '{uri}'")))
    })
}

/// The meshes of the given glTF that differ from the export's, or that the export does not have.
fn changed_meshes(given: &Path, exported: &Folder) -> Result<BTreeSet<String>> {
    let name = given.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let before = mesh_contents(&load_from_folder(exported, &name)?)?;
    let after = mesh_contents(&load_from_disk(given)?)?;
    Ok(after.into_iter().filter(|(name, content)| before.get(name) != Some(content)).map(|(name, _)| name).collect())
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
            p if !p.truthy() => None,
            Arg::Str(name) => Some(name.as_str()),
            _ => bail!("unsupported operand type(s) for /: 'WindowsPath' and 'int'"),
        };
        let pose = pose_name.and_then(|name| source.get(name));
        let weights = source.get(&files.weights);
        let gltf_path = folder.join(&gltf);
        let wrap = |error: Error| match error {
            Error::Invalid(message) => Error::invalid(format!("{gltf}: {message}")),
            other => other,
        };
        let meshes = changed_meshes(&gltf_path, &exported_folder).map_err(wrap)?;
        let Some(template) = source.get(&files.model) else {
            bail!("[Errno 2] No such file or directory: {}", im3::str_repr(&files.model));
        };
        let built = load_from_disk(&gltf_path)
            .and_then(|loaded| from_gltf::import_gltf(&loaded, template, pose, weights, Some(&meshes)))
            .map_err(wrap)?;
        // a dict of member to content: a member named twice keeps its place and the last content
        let mut written: Vec<(&str, Vec<u8>)> = Vec::new();
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
    }
    with_textures(pack, &given, &exported)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mds::{self, Material, Mds, Mdt, Node, Prim, PRIM_TRIANGLES};

    fn pack(members: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        for (name, data) in members {
            let mut slot = vec![0u8; archive::PACK_SLOT];
            slot[..name.len()].copy_from_slice(name.as_bytes());
            let pad = archive::padding(data.len());
            slot[64..68].copy_from_slice(&(archive::PACK_SLOT as i32).to_le_bytes());
            slot[68..72].copy_from_slice(&(data.len() as i32).to_le_bytes());
            slot[72..76].copy_from_slice(&((archive::PACK_SLOT + data.len() + pad) as i32).to_le_bytes());
            out.extend(slot);
            out.extend(*data);
            out.extend(vec![0u8; pad]);
        }
        out.extend(vec![0u8; archive::PACK_SLOT]);
        out
    }

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
        let folder = std::env::temp_dir().join(format!("c2-assets-models-{name}-{}", std::process::id()));
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
        assert_eq!(get(&exported, "textures/tex01.png"), im3::to_png(&im3::read(img).unwrap()[0], true).unwrap());
        let folder = written(&exported, "unedited");
        let built = build(&folder, &pack).unwrap();
        std::fs::remove_dir_all(folder).unwrap();
        assert_eq!(built, pack);
    }

    #[test]
    fn an_edited_mesh_and_texture_reach_the_pack() {
        let pack = sample_pack();
        let mut exported = export(&pack).unwrap();
        let gltf = crate::json::loads(std::str::from_utf8(get(&exported, "c.gltf")).unwrap()).unwrap();
        let mut buffer = get(&exported, "c.bin").to_vec();
        let primitive =
            &gltf.get("meshes").unwrap().as_list().unwrap()[0].get("primitives").unwrap().as_list().unwrap()[0];
        let accessor = from_gltf::index(
            gltf.get("accessors").unwrap(),
            primitive.get("attributes").unwrap().get("POSITION").unwrap(),
        )
        .unwrap();
        let view = from_gltf::index(gltf.get("bufferViews").unwrap(), accessor.get("bufferView").unwrap()).unwrap();
        let base = view.get("byteOffset").and_then(|v| v.as_int()).unwrap_or(0) as usize;
        for i in 0..accessor.get("count").unwrap().as_int().unwrap() as usize {
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
                *content = im3::write_png(8, 8, im3::PngMode::Rgb, &[0, 0, 255].repeat(64), &[], &[]).unwrap();
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
}
