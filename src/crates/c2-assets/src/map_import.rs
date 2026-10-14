//! An edited map scene (docs/assets/maps.md section 8) back to the game's files: the .map rebuilt
//! from the scene's placements, pieces and points (map_scene, map_edit), and the map models whose
//! meshes changed rebuilt from the first piece placing each (models::build_map_model). What did
//! not change keeps its bytes.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use serde_json::{json, Map, Value};

use crate::from_gltf;
use crate::map_export::{find_model, options_for, ModelPack};
use crate::map_scene::{self, SceneNode};
use crate::map_transform::Trs;
use crate::mapfile;
use crate::to_gltf::TextureSearch;
use crate::{map_edit, models, Error, Result};

/// What an import changes.
#[derive(Debug, Default)]
pub struct Imported {
    /// The new .map, when it differs from the original.
    pub map: Option<Vec<u8>>,
    /// Rebuilt models that differ from the original: (model file name, bytes).
    pub models: Vec<(String, Vec<u8>)>,
    pub notes: Vec<String>,
}

fn floats(value: &Value) -> Option<Vec<f64>> {
    value.as_array().map(|items| items.iter().map(|item| item.as_f64().unwrap_or(0.0)).collect())
}

/// A glTF node's transform: translation/rotation/scale, or its matrix decomposed.
fn trs_of(node: &Value) -> Result<Option<Trs>> {
    if let Some(matrix) = floats(&node["matrix"]).filter(|matrix| matrix.len() == 16) {
        let matrix: [f64; 16] = matrix.try_into().unwrap();
        let (translation, rotation, scale) = crate::to_gltf::decompose(&matrix)?;
        let rotation: [f64; 4] = [rotation[0], rotation[1], rotation[2], rotation[3]];
        return Ok(Some(Trs { translation, rotation, scale }));
    }
    let present = ["translation", "rotation", "scale"].iter().any(|key| node.get(key).is_some());
    if !present {
        return Ok(Some(Trs { translation: [0.0; 3], rotation: [0.0, 0.0, 0.0, 1.0], scale: [1.0; 3] }));
    }
    let three = |key: &str, default: [f64; 3]| -> [f64; 3] {
        floats(&node[key]).filter(|values| values.len() == 3).map_or(default, |values| [values[0], values[1], values[2]])
    };
    let rotation = floats(&node["rotation"]).filter(|values| values.len() == 4).map_or([0.0, 0.0, 0.0, 1.0], |q| [q[0], q[1], q[2], q[3]]);
    Ok(Some(Trs { translation: three("translation", [0.0; 3]), rotation, scale: three("scale", [1.0; 3]) }))
}

/// The scene tree under a glTF node; model nodes (role "mds") are left out, their indices kept in
/// `models` for the pieces they hang under.
fn tree_of(doc: &Value, index: usize, model_roots: &mut HashMap<usize, Vec<usize>>, seen: &mut HashSet<usize>) -> Result<SceneNode> {
    if !seen.insert(index) {
        return Err(Error::invalid(format!("glTF node {index} appears twice in the scene tree")));
    }
    let node = &doc["nodes"][index];
    let mut extras = node["extras"].clone();
    // Blender keeps custom properties it could not store as a dict as JSON text
    if let Some(text) = extras["c2"].as_str() {
        if let Ok(parsed) = serde_json::from_str::<Value>(text) {
            extras["c2"] = parsed;
        }
    }
    let role = extras["c2"]["role"].as_str().unwrap_or("").to_string();
    let mut children = Vec::new();
    let mut models = Vec::new();
    for child in node["children"].as_array().into_iter().flatten() {
        let child = child.as_u64().unwrap_or(u64::MAX) as usize;
        let child_role = doc["nodes"][child]["extras"]["c2"]["role"].as_str().unwrap_or("");
        if role == "piece" && (child_role == "mds" || child_role.is_empty()) {
            models.push(child);
        } else {
            children.push(tree_of(doc, child, model_roots, seen)?);
        }
    }
    let model = if role == "piece" { extras["c2"]["model"].as_str().map(str::to_string) } else { None };
    if role == "piece" {
        model_roots.insert(index, models);
    }
    Ok(SceneNode {
        name: node["name"].as_str().unwrap_or("").to_string(),
        trs: trs_of(node)?,
        extras: if extras.is_null() { json!({}) } else { extras },
        children,
        model,
    })
}

/// The part of a glTF doc a set of root nodes reaches, renumbered as a doc of its own (nodes,
/// meshes, accessors, materials, textures, images, samplers, views and buffers).
pub fn subtree(doc: &Value, roots: &[usize]) -> Value {
    let list = |key: &str| doc[key].as_array().cloned().unwrap_or_default();
    let (nodes, meshes, accessors, materials, textures) = (list("nodes"), list("meshes"), list("accessors"), list("materials"), list("textures"));
    let (images, samplers, views, buffers) = (list("images"), list("samplers"), list("bufferViews"), list("buffers"));

    // nodes reachable from the roots, in order
    let mut node_order = Vec::new();
    let mut pending: Vec<usize> = roots.iter().rev().copied().collect();
    let mut seen = HashSet::new();
    while let Some(at) = pending.pop() {
        if !seen.insert(at) || at >= nodes.len() {
            continue;
        }
        node_order.push(at);
        for child in nodes[at]["children"].as_array().into_iter().flatten().rev() {
            pending.push(child.as_u64().unwrap_or(0) as usize);
        }
    }
    let renumber = |order: &[usize]| -> HashMap<usize, usize> { order.iter().enumerate().map(|(new, &old)| (old, new)).collect() };
    let node_map = renumber(&node_order);
    let mut mesh_order = Vec::new();
    for &at in &node_order {
        if let Some(mesh) = nodes[at]["mesh"].as_u64().map(|mesh| mesh as usize) {
            if !mesh_order.contains(&mesh) {
                mesh_order.push(mesh);
            }
        }
    }
    let mesh_map = renumber(&mesh_order);
    let (mut accessor_order, mut material_order) = (Vec::new(), Vec::new());
    let add = |order: &mut Vec<usize>, value: &Value| {
        if let Some(at) = value.as_u64().map(|at| at as usize) {
            if !order.contains(&at) {
                order.push(at);
            }
        }
    };
    for &mesh in &mesh_order {
        for primitive in meshes[mesh]["primitives"].as_array().into_iter().flatten() {
            for value in primitive["attributes"].as_object().into_iter().flatten().map(|(_, value)| value) {
                add(&mut accessor_order, value);
            }
            add(&mut accessor_order, &primitive["indices"]);
            add(&mut material_order, &primitive["material"]);
        }
    }
    let texture_keys = [("pbrMetallicRoughness", "baseColorTexture"), ("pbrMetallicRoughness", "metallicRoughnessTexture"), ("", "normalTexture"), ("", "occlusionTexture"), ("", "emissiveTexture")];
    let texture_info = |material: &Value, parent: &str, key: &str| -> Value {
        if parent.is_empty() { material[key]["index"].clone() } else { material[parent][key]["index"].clone() }
    };
    let mut texture_order = Vec::new();
    for &material in &material_order {
        for (parent, key) in texture_keys {
            add(&mut texture_order, &texture_info(&materials[material], parent, key));
        }
    }
    let (mut image_order, mut sampler_order) = (Vec::new(), Vec::new());
    for &texture in &texture_order {
        add(&mut image_order, &textures[texture]["source"]);
        add(&mut sampler_order, &textures[texture]["sampler"]);
    }
    let mut view_order = Vec::new();
    for &accessor in &accessor_order {
        add(&mut view_order, &accessors[accessor]["bufferView"]);
    }
    for &image in &image_order {
        add(&mut view_order, &images[image]["bufferView"]);
    }
    let mut buffer_order = Vec::new();
    for &view in &view_order {
        add(&mut buffer_order, &views[view]["buffer"]);
    }
    let (accessor_map, material_map, texture_map) = (renumber(&accessor_order), renumber(&material_order), renumber(&texture_order));
    let (image_map, sampler_map, view_map, buffer_map) = (renumber(&image_order), renumber(&sampler_order), renumber(&view_order), renumber(&buffer_order));
    let set = |value: &mut Value, map: &HashMap<usize, usize>| {
        if let Some(at) = value.as_u64() {
            *value = json!(map.get(&(at as usize)).copied().unwrap_or(0));
        }
    };

    let out_nodes: Vec<Value> = node_order
        .iter()
        .map(|&at| {
            let mut node = nodes[at].clone();
            if let Some(object) = node.as_object_mut() {
                object.remove("skin");
                object.remove("camera");
                // the scene's own tag, which the model's export does not have
                if let Some(extras) = object.get_mut("extras").and_then(Value::as_object_mut) {
                    extras.remove("c2");
                }
                if let Some(mesh) = object.get_mut("mesh") {
                    set(mesh, &mesh_map);
                }
                if let Some(children) = object.get_mut("children").and_then(Value::as_array_mut) {
                    children.retain(|child| child.as_u64().is_some_and(|child| node_map.contains_key(&(child as usize))));
                    children.iter_mut().for_each(|child| set(child, &node_map));
                }
            }
            node
        })
        .collect();
    let out_meshes: Vec<Value> = mesh_order
        .iter()
        .map(|&at| {
            let mut mesh = meshes[at].clone();
            for primitive in mesh.get_mut("primitives").and_then(Value::as_array_mut).into_iter().flatten() {
                if let Some(attributes) = primitive.get_mut("attributes").and_then(Value::as_object_mut) {
                    attributes.values_mut().for_each(|value| set(value, &accessor_map));
                }
                if let Some(object) = primitive.as_object_mut() {
                    object.remove("targets");
                    if let Some(indices) = object.get_mut("indices") {
                        set(indices, &accessor_map);
                    }
                    if let Some(material) = object.get_mut("material") {
                        set(material, &material_map);
                    }
                }
            }
            mesh
        })
        .collect();
    let out_accessors: Vec<Value> = accessor_order
        .iter()
        .map(|&at| {
            let mut accessor = accessors[at].clone();
            if let Some(view) = accessor.get_mut("bufferView") {
                set(view, &view_map);
            }
            accessor
        })
        .collect();
    let out_materials: Vec<Value> = material_order
        .iter()
        .map(|&at| {
            let mut material = materials[at].clone();
            for (parent, key) in texture_keys {
                let info = if parent.is_empty() { material.get_mut(key) } else { material.get_mut(parent).and_then(|p| p.get_mut(key)) };
                if let Some(index) = info.and_then(|info| info.get_mut("index")) {
                    set(index, &texture_map);
                }
            }
            material
        })
        .collect();
    let out_textures: Vec<Value> = texture_order
        .iter()
        .map(|&at| {
            let mut texture = textures[at].clone();
            if let Some(source) = texture.get_mut("source") {
                set(source, &image_map);
            }
            if let Some(sampler) = texture.get_mut("sampler") {
                set(sampler, &sampler_map);
            }
            texture
        })
        .collect();
    let out_images: Vec<Value> = image_order
        .iter()
        .map(|&at| {
            let mut image = images[at].clone();
            if let Some(view) = image.get_mut("bufferView") {
                set(view, &view_map);
            }
            image
        })
        .collect();
    let out_views: Vec<Value> = view_order
        .iter()
        .map(|&at| {
            let mut view = views[at].clone();
            if let Some(buffer) = view.get_mut("buffer") {
                set(buffer, &buffer_map);
            }
            view
        })
        .collect();
    let mut out = Map::new();
    out.insert("asset".into(), doc["asset"].clone());
    for (key, value) in [
        ("nodes", out_nodes),
        ("meshes", out_meshes),
        ("accessors", out_accessors),
        ("materials", out_materials),
        ("textures", out_textures),
        ("images", out_images),
        ("samplers", sampler_order.iter().map(|&at| samplers[at].clone()).collect()),
        ("bufferViews", out_views),
        ("buffers", buffer_order.iter().map(|&at| buffers[at].clone()).collect()),
    ] {
        out.insert(key.into(), Value::Array(value));
    }
    for key in ["extensionsUsed", "extensionsRequired"] {
        if let Some(list) = doc[key].as_array() {
            let kept: Vec<Value> = list.iter().filter(|name| *name != "KHR_lights_punctual").cloned().collect();
            if !kept.is_empty() {
                out.insert(key.into(), Value::Array(kept));
            }
        }
    }
    let root_list: Vec<usize> = roots.iter().filter_map(|root| node_map.get(root).copied()).collect();
    out.insert("scenes".into(), json!([{"nodes": root_list}]));
    out.insert("scene".into(), json!(0));
    Value::Object(out)
}

/// The scene's map and models against the originals. `read` gives a file of the scene's folder
/// (buffers, images) by relative path.
pub fn import_map(
    gltf: &[u8],
    read: &dyn Fn(&str) -> Result<Vec<u8>>,
    original: &[u8],
    packs: &[ModelPack],
    textures: &TextureSearch,
) -> Result<Imported> {
    let doc: Value = serde_json::from_slice(gltf).map_err(|error| Error::invalid(format!("the scene's glTF: {error}")))?;
    let original_map = mapfile::parse(original)?;
    let roots: Vec<usize> = doc["scenes"][doc["scene"].as_u64().unwrap_or(0) as usize]["nodes"]
        .as_array()
        .map(|nodes| nodes.iter().filter_map(Value::as_u64).map(|node| node as usize).collect())
        .unwrap_or_default();
    // the map's root: the scene root whose extras say so (or one below a wrapper Blender adds)
    let mut root_index = None;
    let mut pending = roots.clone();
    while let Some(at) = pending.pop() {
        let node = &doc["nodes"][at];
        let c2 = &node["extras"]["c2"];
        let is_map = c2["role"] == "map"
            || c2.as_str().is_some_and(|text| text.contains("\"role\": \"map\"") || text.contains("\"role\":\"map\""));
        if is_map {
            root_index = Some(at);
            break;
        }
        pending.extend(node["children"].as_array().into_iter().flatten().filter_map(Value::as_u64).map(|child| child as usize));
    }
    let Some(root_index) = root_index else {
        return Err(Error::invalid("the scene has no map root (a node whose extras.c2.role is \"map\")"));
    };
    let mut model_roots = HashMap::new();
    let tree = tree_of(&doc, root_index, &mut model_roots, &mut HashSet::new())?;

    let mut out = Imported::default();
    let edit = map_scene::read_scene(&tree, &original_map)?;
    let rebuilt = map_edit::rebuild(&original_map, &edit)?;
    let text = mapfile::write(&rebuilt)?;
    if text != original {
        out.map = Some(text);
    }

    // each model's first piece (lowest glTF node index): its node subtree is the model as edited
    let mut piece_nodes: Vec<usize> = model_roots.keys().copied().collect();
    piece_nodes.sort();
    let mut done: HashSet<String> = HashSet::new();
    let buffer_cache: RefCell<HashMap<String, Vec<u8>>> = RefCell::new(HashMap::new());
    let read_cached = |path: &str| -> Result<Vec<u8>> {
        if let Some(data) = buffer_cache.borrow().get(path) {
            return Ok(data.clone());
        }
        let data = read(path)?;
        buffer_cache.borrow_mut().insert(path.to_string(), data.clone());
        Ok(data)
    };
    for piece in piece_nodes {
        let c2 = &doc["nodes"][piece]["extras"]["c2"];
        let model = match c2["model"].as_str() {
            Some(model) => model.to_string(),
            None => match c2.as_str().and_then(|text| serde_json::from_str::<Value>(text).ok()) {
                Some(parsed) => parsed["model"].as_str().unwrap_or("").to_string(),
                None => continue,
            },
        };
        let model_roots_here = &model_roots[&piece];
        if model.is_empty() || model_roots_here.is_empty() || !done.insert(model.to_lowercase()) {
            continue;
        }
        let Some((kind, data)) = find_model(packs, &model) else {
            out.notes.push(format!("{model}: in none of the map's model packs; its meshes are left out"));
            continue;
        };
        if kind >= 3 {
            continue;
        }
        let sub = subtree(&doc, model_roots_here);
        let text = serde_json::to_string(&sub).map_err(|error| Error::invalid(error.to_string()))?;
        let given = from_gltf::load_gltf(text.as_bytes(), &|path| read_cached(path).map_err(|error| Error::invalid(error.to_string())))?;
        let rebuilt = models::build_map_model(&given, data, &options_for(kind, textures))
            .map_err(|error| Error::invalid(format!("{model}: {error}")))?;
        if rebuilt != data {
            out.models.push((model, rebuilt));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_export::export_map;

    /// Every fixed map of an extracted disc (C2S_CORPUS) exported and imported unedited changes
    /// nothing: the same .map, no model rebuilt.
    #[test]
    fn corpus_unedited_scenes_change_nothing() {
        let Some(corpus) = std::env::var_os("C2S_CORPUS") else {
            return;
        };
        let root = std::path::Path::new(&corpus);
        let mut checked = 0;
        for name in ["map/s/s17/s17", "map/m/m05/m05"] {
            let folder = root.join(name).parent().unwrap().to_path_buf();
            let stem = name.rsplit('/').next().unwrap();
            let map_bytes = std::fs::read(root.join(format!("{name}.map"))).unwrap();
            let map = mapfile::parse(&map_bytes).unwrap();
            let mut packs = Vec::new();
            let mpk = folder.join(format!("{stem}.mpk.d"));
            for entry in std::fs::read_dir(&mpk).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let members = std::fs::read_dir(&path)
                        .unwrap()
                        .flatten()
                        .filter(|member| member.path().is_file())
                        .map(|member| (member.file_name().to_string_lossy().into_owned(), std::fs::read(member.path()).unwrap()))
                        .collect();
                    let pack_name = path.file_name().unwrap().to_string_lossy().trim_end_matches(".d").to_string();
                    packs.push(ModelPack::new(&pack_name, members));
                }
            }
            let ipk = folder.join(format!("{stem}.ipk.d"));
            let images: Vec<(String, Vec<u8>)> = std::fs::read_dir(&ipk)
                .unwrap()
                .flatten()
                .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "img"))
                .map(|entry| (entry.file_name().to_string_lossy().into_owned(), std::fs::read(entry.path()).unwrap()))
                .collect();
            let search = TextureSearch::new(&images).unwrap();
            let (files, _) = export_map(stem, &map, &packs, &search).unwrap();
            let gltf = files[&format!("{stem}.gltf")].clone();
            let read = |path: &str| files.get(path).cloned().ok_or_else(|| Error::invalid(format!("no {path}")));
            let imported = import_map(&gltf, &read, &map_bytes, &packs, &search).unwrap();
            assert!(imported.map.is_none(), "{name}: the .map changed");
            let changed: Vec<&String> = imported.models.iter().map(|(model, _)| model).collect();
            assert!(changed.is_empty(), "{name}: models rebuilt: {changed:?}");
            checked += 1;
        }
        println!("{checked} maps export and import unchanged");
    }
}




