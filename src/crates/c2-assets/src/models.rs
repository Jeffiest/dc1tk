//! Character models as glTF for mods: a folder named after the pack plus .gltf (c02a.chr.gltf/)
//! holding what an export writes: c02a.gltf with its .bin, and textures/ with a PNG per texture the
//! model uses (the pack's own pictures where it has them). build rebuilds the mesh, weights and bind
//! pose from the edited glTF with the disc's model as the template, and puts texture PNGs that
//! differ from the export into the pack's texture files; parts left as exported change nothing.
//! new_pack makes a pack of its own from a glTF alone, in a template pack's layout.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use c2_disc::{archive, rebuild};

use crate::folder::Folder;
use crate::from_gltf::{self, Gltf, MeshId};
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
fn changed_meshes(given: &Path, exported: &Folder, template: &[u8]) -> Result<BTreeSet<MeshId>> {
    let name = given.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let model = crate::mds::parse(template)?;
    from_gltf::changed_meshes(&load_from_folder(exported, &name)?, &load_from_disk(given)?, &model)
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
        let Some(template) = source.get(&files.model) else {
            bail!("[Errno 2] No such file or directory: {}", im3::str_repr(&files.model));
        };
        let meshes = changed_meshes(&gltf_path, &exported_folder, template).map_err(wrap)?;
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
    let meshes = from_gltf::changed_meshes(&before, given, &model)?;
    let import = from_gltf::ImportOptions { collision: options.collision };
    Ok(from_gltf::import_with(given, original, None, None, Some(&meshes), &import)?.mds)
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
    let mut out = Vec::new();
    for (name, data) in members {
        let Some(raw) = c2_disc::text::encode(name).filter(|raw| !raw.is_empty() && raw.len() < archive::PACK_NAME)
        else {
            bail!("{name} cannot be a member's name (1 to {} bytes of Shift-JIS)", archive::PACK_NAME - 1);
        };
        let pad = archive::padding(data.len());
        let mut slot = vec![0u8; archive::PACK_SLOT];
        slot[..raw.len()].copy_from_slice(&raw);
        let size = i32::try_from(data.len()).map_err(|_| Error::invalid(format!("{name} is too big for a pack")))?;
        slot[64..68].copy_from_slice(&(archive::PACK_SLOT as i32).to_le_bytes());
        slot[68..72].copy_from_slice(&size.to_le_bytes());
        slot[72..76].copy_from_slice(&((archive::PACK_SLOT + data.len() + pad) as i32).to_le_bytes());
        out.extend(slot);
        out.extend(data);
        out.extend(std::iter::repeat_n(0, pad));
    }
    out.extend([0u8; archive::PACK_SLOT]);
    Ok(out)
}

/// The statement tag a line of a cfg starts with (MODEL in `MODEL "a.mds";`).
fn cfg_tag(line: &str) -> &str {
    let line = line.trim_start();
    let end = line.find(|c: char| c.is_whitespace() || c == ';' || c == '"' || c == ',').unwrap_or(line.len());
    &line[..end]
}

/// A template's info.cfg for a new pack named `stem`: NAME, MODEL and the texture file name the new
/// pack's members (<stem>.mds, <stem>.img, which IMG 0 loads; no IMG line without pictures), and
/// what belonged to the template's model alone (motion sets, vertex and shape animation, shadow
/// and LOD models) dropped. Every other line (outline, size, scale, comments) stays as it is.
pub fn new_info_cfg(template: &[u8], stem: &str, pictures: bool) -> Result<Vec<u8>> {
    let text = c2_disc::text::decode_lossy(template);
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let img = format!("IMG 0, \"{stem}.img\";");
    let (mut out, mut in_motion, mut img_written, mut model_written) = (String::new(), false, false, false);
    for line in text.split_inclusive('\n') {
        let tag = cfg_tag(line);
        let ending = if line.ends_with("\r\n") {
            "\r\n"
        } else if line.ends_with('\n') {
            "\n"
        } else {
            ""
        };
        if in_motion {
            in_motion = tag != "MOTION_END";
            continue;
        }
        match tag {
            "MOTION" => in_motion = true,
            "SHADOW_MOTION" | "VERTEX_ANIME" | "SHAPE_ANIME" | "SHADOW_MODEL" | "LOD_MODEL" | "KEY_START" | "KEY"
            | "KEY_END" | "MOTION_END" => {}
            "NAME" => out.push_str(&format!("NAME \"{stem}\";{ending}")),
            "IMG" => {
                if pictures && !img_written {
                    out.push_str(&format!("{img}{ending}"));
                }
                img_written = true;
            }
            "MODEL" => {
                if pictures && !img_written {
                    out.push_str(&format!("{img}{newline}IMG_END;{newline}{newline}"));
                    img_written = true;
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
    c2_disc::text::encode(&out).ok_or_else(|| Error::invalid("the template's cfg is not Shift-JIS text"))
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

/// A new character pack named `stem` (its members <stem>.mds, <stem>.img and info.cfg) built from a
/// glTF, with a template pack (`template_name` is its model's name, for packs whose cfg is named
/// after them) giving its cfg's other lines, the MDS version and the texture format. The model is
/// the glTF's nodes and rigid meshes alone (from_gltf::new_model), and the texture file holds each
/// material's base colour image as a picture of the name the material uses (PNG, sides powers of
/// two, in the format of the template's first picture).
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
        std::fs::read(&file).map_err(|error| Error::invalid(format!("{}: {error}", file.display())))
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
    let mds = from_gltf::new_model(&mut gltf, version, stem).map_err(wrap)?;
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
    let cfg = new_info_cfg(folder.get("info.cfg").unwrap_or_default(), stem, !pictures.is_empty())?;
    members.push(("info.cfg".to_string(), cfg));
    write_pack(&members)
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
            im3::write_png(32, 16, im3::PngMode::Rgba, &face, &[], &[]).unwrap(),
        )
        .unwrap();
        std::fs::write(folder.join("calc.gltf"), gltf).unwrap();
        folder.join("calc.gltf")
    }

    #[test]
    fn a_new_pack_holds_the_gltf_meshes_and_textures_in_the_templates_layout() {
        let folder = std::env::temp_dir().join(format!("c2-assets-new-pack-{}", std::process::id()));
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
        let png = im3::read_png(get(&exported, "textures/calc_face.png")).unwrap();
        let given = im3::read_png(&std::fs::read(folder.join("tex/calc_face.png")).unwrap()).unwrap();
        let rgba = |p: &im3::Png| -> Vec<u8> {
            if p.paletted {
                p.data.iter().flat_map(|&i| p.palette[i as usize]).collect()
            } else {
                p.data.clone()
            }
        };
        assert_eq!(rgba(&png), rgba(&given));
        let json = crate::json::loads(std::str::from_utf8(get(&exported, "calc.gltf")).unwrap()).unwrap();
        let meshes: Vec<&str> = json
            .get("meshes")
            .unwrap()
            .as_list()
            .unwrap()
            .iter()
            .map(|m| m.get("name").unwrap().as_str().unwrap())
            .collect();
        assert_eq!(meshes, ["body", "key"]);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_new_pack_refuses_skins_and_textures_the_game_cannot_wrap() {
        let folder = std::env::temp_dir().join(format!("c2-assets-new-pack-refused-{}", std::process::id()));
        let gltf = calc_gltf(&folder);
        let face: Vec<u8> = [9u8, 9, 9, 255].repeat(48 * 16);
        std::fs::write(
            folder.join("tex/calc_face.png"),
            im3::write_png(48, 16, im3::PngMode::Rgba, &face, &[], &[]).unwrap(),
        )
        .unwrap();
        let error = new_pack(&gltf, &prop_template(), "prop", "calc").unwrap_err().to_string();
        assert!(error.contains("powers of two"), "{error}");
        let text = std::fs::read_to_string(&gltf).unwrap().replace("\"scene\": 0,", "\"skins\": [{\"joints\": [0]}],");
        std::fs::write(&gltf, text).unwrap();
        let error = new_pack(&gltf, &prop_template(), "prop", "calc").unwrap_err().to_string();
        assert!(error.contains("skin"), "{error}");
        std::fs::remove_dir_all(folder).unwrap();
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
        let at = |list: &str, i: &crate::json::Value| from_gltf::index(j.get(list).unwrap(), i).unwrap().clone();
        let mesh = at(
            "meshes",
            from_gltf::index(j.get("nodes").unwrap(), &(node as i64).into()).unwrap().get("mesh").unwrap(),
        );
        let mut bin = folder.get("piece.bin").unwrap().to_vec();
        for primitive in mesh.get("primitives").unwrap().as_list().unwrap() {
            let accessor = at("accessors", primitive.get("attributes").unwrap().get("POSITION").unwrap());
            let view = at("bufferViews", accessor.get("bufferView").unwrap());
            let base = view.get("byteOffset").and_then(|v| v.as_int()).unwrap_or(0) as usize;
            for k in 0..accessor.get("count").unwrap().as_int().unwrap() as usize {
                let x = f32::from_le_bytes(bin[base + 12 * k..base + 12 * k + 4].try_into().unwrap());
                bin[base + 12 * k..base + 12 * k + 4].copy_from_slice(&(x + 1.0).to_le_bytes());
            }
        }
        Gltf { json: gltf.json.clone(), buffers: vec![bin] }
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
        assert_eq!(get(&files, "textures/wall01.png"), im3::to_png(&pictures[0], true).unwrap());
        let json = crate::json::loads(std::str::from_utf8(get(&files, "piece.gltf")).unwrap()).unwrap();
        let list = |key: &str| json.get(key).unwrap().as_list().unwrap().clone();
        let names: Vec<String> =
            list("nodes").iter().map(|n| n.get("name").unwrap().as_str().unwrap().to_string()).collect();
        assert_eq!(names, ["piece", "grid54--f2appz0n1t-5_1", "grid54--a40aof-6_1", "lamp--n1z0c0f2app-"]);
        for (i, node) in list("nodes").iter().enumerate() {
            assert_eq!(node.get("extras").unwrap().get("mds_index").unwrap().as_int(), Some(i as i64));
        }
        let meshes: Vec<String> =
            list("meshes").iter().map(|n| n.get("name").unwrap().as_str().unwrap().to_string()).collect();
        assert_eq!(meshes, names[1..]);
        // wall01 repeats (CLAMP 0), leaf01 clamps (CLAMP 5)
        let wrap = |texture: &str| {
            let t = list("textures").into_iter().find(|t| t.get("name").unwrap().as_str() == Some(texture)).unwrap();
            let s = &list("samplers")[t.get("sampler").unwrap().as_int().unwrap() as usize];
            (s.get("wrapS").unwrap().as_int().unwrap(), s.get("wrapT").unwrap().as_int().unwrap())
        };
        assert_eq!(wrap("wall01"), (10497, 10497));
        assert_eq!(wrap("leaf01"), (33071, 33071));
        // blend flags: additive -> BLEND, alpha test 0x40 with aOF -> MASK 0.5, c0 -> unlit
        let material_of = |node: usize| {
            let mesh = &list("meshes")[list("nodes")[node].get("mesh").unwrap().as_int().unwrap() as usize];
            let number =
                mesh.get("primitives").unwrap().as_list().unwrap()[0].get("material").unwrap().as_int().unwrap();
            list("materials")[number as usize].clone()
        };
        let mode = |m: &crate::json::Value| m.get("alphaMode").and_then(|v| v.as_str()).map(str::to_string);
        assert_eq!(mode(&material_of(1)).as_deref(), Some("BLEND"));
        assert_eq!(mode(&material_of(2)).as_deref(), Some("MASK"));
        assert_eq!(material_of(2).get("alphaCutoff").unwrap().as_f64(), Some(0.5));
        assert!(material_of(3).get("extensions").unwrap().has("KHR_materials_unlit"));
        assert!(!material_of(1).has("extensions"));
        let flags = material_of(3).get("extras").unwrap().get("frame_flags").unwrap().clone();
        assert_eq!(flags.get("mds_flags").unwrap().as_str(), Some("n1z0c0f2app-"));
        assert_eq!(flags.get("z_write").unwrap().as_int(), Some(0));
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
        if let Some(crate::json::Value::List(nodes)) = bare.json.get_mut("nodes") {
            for node in nodes {
                if let crate::json::Value::Object(items) = node {
                    items.retain(|(key, _)| key != "extras");
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
        let json = crate::json::loads(std::str::from_utf8(get(&files, "piece.gltf")).unwrap()).unwrap();
        let names: Vec<&str> = json
            .get("materials")
            .unwrap()
            .as_list()
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
        if let Some(crate::json::Value::List(materials)) = gltf.json.get_mut("materials") {
            materials[0].set("name", "col_ground0_foot5_area9_mask4.001".into());
        }
        if let Some(crate::json::Value::List(meshes)) = gltf.json.get_mut("meshes") {
            if let Some(crate::json::Value::List(primitives)) = meshes[0].get_mut("primitives") {
                for p in primitives {
                    let position = p.get("attributes").unwrap().get("POSITION").unwrap().clone();
                    if let Some(attributes) = p.get_mut("attributes") {
                        attributes.set("NORMAL", position.clone());
                        attributes.set("TEXCOORD_0", position);
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
