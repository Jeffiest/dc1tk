//! A whole map as one glTF scene (docs/assets/maps.md section 8): the scene tree of map_scene with
//! each piece's model under it. A model is exported once (map_models::export_map_model) and its
//! nodes copied under every piece that places it, the copies sharing its meshes, so editing a mesh
//! in Blender edits every piece that places it, as in the game. Each model keeps its own buffer
//! (`models/<stem>.bin`); textures are shared (`textures/<picture>.png`).

use std::collections::{BTreeMap, HashMap};

use serde_json::{json, Map, Value};

use crate::map_scene::{self, SceneNode};
use crate::mapfile::MapFile;
use crate::to_gltf::{Options, TextureSearch};
use crate::{info_cfg, models, Context, Error, Result};

/// What a map's pieces place, as a model pack's info.cfg gives it (`TYPE`, pcpTYPE).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackType {
    /// 0: a drawn model.
    Model,
    /// 1: floor and wall collision.
    Collision,
    /// 2: camera collision.
    Camera,
    /// 3 and 4 (and any other number): an animated character.
    Character,
}

impl PackType {
    /// The type of a TYPE number.
    pub fn from_number(number: i64) -> PackType {
        match number {
            0 => PackType::Model,
            1 => PackType::Collision,
            2 => PackType::Camera,
            _ => PackType::Character,
        }
    }

    /// The TYPE number an info.cfg writes.
    pub fn number(self) -> i64 {
        match self {
            PackType::Model => 0,
            PackType::Collision => 1,
            PackType::Camera => 2,
            PackType::Character => 3,
        }
    }

    /// The name the scene's pieces give the type (`extras.c2.kind`).
    pub fn name(self) -> &'static str {
        match self {
            PackType::Model => "model",
            PackType::Collision => "collision",
            PackType::Camera => "camera",
            PackType::Character => "chara",
        }
    }

    /// Whether the model is collision, exported and rebuilt as such (to_gltf::Options::collision).
    pub fn is_collision(self) -> bool {
        matches!(self, PackType::Collision | PackType::Camera)
    }
}

/// A map's model pack (a .pcp of its .mpk): its info.cfg's entries (model, TYPE) and its files.
pub struct ModelPack {
    pub name: String,
    pub types: Vec<(String, PackType)>,
    pub files: HashMap<String, Vec<u8>>,
}

impl ModelPack {
    /// A pack from its members (as dc2tk-disc's archive gives them).
    pub fn new(name: &str, members: Vec<(String, Vec<u8>)>) -> ModelPack {
        let mut types = Vec::new();
        let mut files = HashMap::new();
        for (member, data) in members {
            if member.eq_ignore_ascii_case("info.cfg") {
                let mut current: Option<String> = None;
                for (tag, args) in info_cfg::parse(&data) {
                    match tag.as_str() {
                        "MDS" => current = args.first().and_then(|arg| arg.as_str()).map(str::to_string),
                        "TYPE" => {
                            if let (Some(model), Some(number)) =
                                (&current, args.first().and_then(|arg| arg.as_f64()).map(|kind| kind as i64))
                            {
                                types.push((model.clone(), PackType::from_number(number)));
                            }
                        }
                        "MDS_END" => current = None,
                        _ => {}
                    }
                }
            }
            files.insert(member.to_lowercase(), data);
        }
        ModelPack { name: name.to_string(), types, files }
    }
}

/// A piece model's pack type and bytes, looked up as the engine does (ignoring case, across the
/// map's packs in order).
pub fn find_model<'a>(packs: &'a [ModelPack], model: &str) -> Option<(PackType, &'a [u8])> {
    let wanted = model.to_lowercase();
    packs.iter().find_map(|pack| {
        let data = pack.files.get(&wanted)?;
        let kind = pack
            .types
            .iter()
            .find(|(name, _)| name.to_lowercase() == wanted)
            .map_or(PackType::Model, |(_, kind)| *kind);
        Some((kind, data.as_slice()))
    })
}

/// The export options a model of a pack type is exported and rebuilt with.
pub fn options_for<'a>(pack_type: PackType, textures: &'a TextureSearch) -> Options<'a> {
    if pack_type.is_collision() {
        Options::collision()
    } else {
        Options::map(Some(textures))
    }
}

/// What the export did: models exported, pieces whose model it could not show, textures missing.
#[derive(Debug, Default)]
pub struct Report {
    pub models: usize,
    pub nodes: usize,
    pub skipped: Vec<String>,
    /// Models pieces name that are in none of the map's packs (the game draws nothing for them).
    pub absent: Vec<String>,
    pub missing_textures: Vec<String>,
}

/// One model's glTF, merged into the scene: where its nodes, meshes and the rest went.
struct Merged {
    nodes: Vec<Value>,
    roots: Vec<usize>,
    mesh_base: usize,
}

/// The scene glTF being written.
struct Scene {
    json: Map<String, Value>,
    files: BTreeMap<String, Vec<u8>>,
    images: HashMap<String, usize>,
}

impl Scene {
    fn list(&mut self, key: &str) -> &mut Vec<Value> {
        let value = self.json.entry(key.to_string()).or_insert_with(|| Value::Array(Vec::new()));
        value.as_array_mut().unwrap()
    }

    fn len(&self, key: &str) -> usize {
        self.json.get(key).and_then(Value::as_array).map_or(0, Vec::len)
    }

    /// Adds a model's glTF (its files as export_map_model gives them): buffers, views, accessors,
    /// images (shared by file name), samplers, textures, materials and meshes; returns its nodes,
    /// renumbered for the scene, for copying under pieces.
    fn merge(&mut self, stem: &str, files: &[(String, Vec<u8>)]) -> Result<Merged> {
        let gltf_name = format!("{stem}.gltf");
        let Some((_, text)) = files.iter().find(|(path, _)| *path == gltf_name) else {
            return Err(Error::invalid(format!("{stem}: the export wrote no {gltf_name}")));
        };
        let doc: Value = serde_json::from_slice(text).context(stem)?;
        let list = |key: &str| doc[key].as_array().cloned().unwrap_or_default();
        let base = |scene: &Scene, key: &str| scene.len(key);
        let (buffer_base, view_base, accessor_base) =
            (base(self, "buffers"), base(self, "bufferViews"), base(self, "accessors"));
        let (sampler_base, texture_base, material_base, mesh_base) =
            (base(self, "samplers"), base(self, "textures"), base(self, "materials"), base(self, "meshes"));
        let shift = |value: &Value, by: usize| json!(value.as_u64().unwrap_or(0) as usize + by);

        for buffer in list("buffers") {
            let uri = buffer["uri"].as_str().unwrap_or_default().to_string();
            let Some((_, data)) = files.iter().find(|(path, _)| *path == uri) else {
                return Err(Error::invalid(format!("{stem}: no buffer {uri}")));
            };
            let path = format!("models/{uri}");
            self.files.insert(path.clone(), data.clone());
            self.list("buffers").push(json!({"uri": path, "byteLength": data.len()}));
        }
        for mut view in list("bufferViews") {
            view["buffer"] = shift(&view["buffer"], buffer_base);
            self.list("bufferViews").push(view);
        }
        for mut accessor in list("accessors") {
            if accessor.get("bufferView").is_some() {
                accessor["bufferView"] = shift(&accessor["bufferView"], view_base);
            }
            self.list("accessors").push(accessor);
        }
        // images: shared by file name across models
        let mut image_map = Vec::new();
        for image in list("images") {
            let uri = image["uri"].as_str().unwrap_or_default().to_string();
            let index = match self.images.get(&uri) {
                Some(&index) => index,
                None => {
                    if let Some((_, data)) = files.iter().find(|(path, _)| *path == uri) {
                        self.files.insert(uri.clone(), data.clone());
                    }
                    let index = self.len("images");
                    self.list("images").push(image.clone());
                    self.images.insert(uri, index);
                    index
                }
            };
            image_map.push(index);
        }
        for sampler in list("samplers") {
            self.list("samplers").push(sampler);
        }
        for mut texture in list("textures") {
            if let Some(source) = texture["source"].as_u64() {
                texture["source"] = json!(image_map.get(source as usize).copied().unwrap_or(0));
            }
            if texture.get("sampler").is_some() {
                texture["sampler"] = shift(&texture["sampler"], sampler_base);
            }
            self.list("textures").push(texture);
        }
        for mut material in list("materials") {
            let texture_infos = [
                ("pbrMetallicRoughness", "baseColorTexture"),
                ("pbrMetallicRoughness", "metallicRoughnessTexture"),
                ("", "normalTexture"),
                ("", "occlusionTexture"),
                ("", "emissiveTexture"),
            ];
            for (parent, key) in texture_infos {
                let info = if parent.is_empty() {
                    material.get_mut(key)
                } else {
                    material.get_mut(parent).and_then(|p| p.get_mut(key))
                };
                if let Some(index) = info.and_then(|info| info.get_mut("index")) {
                    *index = json!(index.as_u64().unwrap_or(0) as usize + texture_base);
                }
            }
            self.list("materials").push(material);
        }
        for mut mesh in list("meshes") {
            for primitive in mesh["primitives"].as_array_mut().into_iter().flatten() {
                if let Some(attributes) = primitive["attributes"].as_object_mut() {
                    for value in attributes.values_mut() {
                        *value = json!(value.as_u64().unwrap_or(0) as usize + accessor_base);
                    }
                }
                for key in ["indices"] {
                    if primitive.get(key).is_some() {
                        primitive[key] = shift(&primitive[key], accessor_base);
                    }
                }
                if primitive.get("material").is_some() {
                    primitive["material"] = shift(&primitive["material"], material_base);
                }
            }
            self.list("meshes").push(mesh);
        }
        for extension in doc["extensionsUsed"].as_array().into_iter().flatten() {
            if !self.list("extensionsUsed").contains(extension) {
                self.list("extensionsUsed").push(extension.clone());
            }
        }
        let roots = doc["scenes"][doc["scene"].as_u64().unwrap_or(0) as usize]["nodes"]
            .as_array()
            .map(|nodes| nodes.iter().filter_map(Value::as_u64).map(|node| node as usize).collect())
            .unwrap_or_default();
        Ok(Merged { nodes: list("nodes"), roots, mesh_base })
    }

    /// Copies a merged model's node tree under the scene, returning the copies of its roots.
    fn instance(&mut self, model: &Merged) -> Vec<usize> {
        let base = self.len("nodes");
        for node in &model.nodes {
            let mut node = node.clone();
            if let Some(mesh) = node["mesh"].as_u64() {
                node["mesh"] = json!(mesh as usize + model.mesh_base);
            }
            if let Some(children) = node.get_mut("children").and_then(Value::as_array_mut) {
                for child in children.iter_mut() {
                    *child = json!(child.as_u64().unwrap_or(0) as usize + base);
                }
            }
            let index = node["extras"]["mds_index"].clone();
            node["extras"]["c2"] = json!({"role": "mds", "mds_index": index});
            self.list("nodes").push(node);
        }
        model.roots.iter().map(|root| root + base).collect()
    }

    /// Writes a scene node and its children; returns its index.
    fn add_node(&mut self, node: &SceneNode, models: &HashMap<String, Merged>, lights: &mut Vec<Value>) -> usize {
        let mut children: Vec<usize> = node.children.iter().map(|child| self.add_node(child, models, lights)).collect();
        if let Some(model) = node.model.as_ref().and_then(|model| models.get(&model.to_lowercase())) {
            children.extend(self.instance(model));
        }
        let mut out = json!({"name": node.name, "extras": node.extras});
        if let Some(trs) = &node.trs {
            out["translation"] = json!(trs.translation);
            out["rotation"] = json!(trs.rotation);
            out["scale"] = json!(trs.scale);
        }
        if !children.is_empty() {
            out["children"] = json!(children);
        }
        if let Some(light) = point_light(node) {
            out["extensions"] = json!({"KHR_lights_punctual": {"light": lights.len()}});
            lights.push(light);
        }
        let index = self.len("nodes");
        self.list("nodes").push(out);
        index
    }
}

/// The display light of a point light (`data` is what the game gets): colour 0-511 shown as 0-1
/// at 255, power as the range.
fn point_light(node: &SceneNode) -> Option<Value> {
    let c2 = node.c2();
    if c2["role"] != "point" || c2["kind"] != "plight" {
        return None;
    }
    let data = &c2["data"];
    let color: Vec<f64> =
        (0..3).map(|at| (data["color"][at].as_f64().unwrap_or(255.0) / 255.0).clamp(0.0, 1.0)).collect();
    let power = data["power"].as_f64().unwrap_or(100.0).max(1.0);
    Some(json!({"type": "point", "name": node.name, "color": color, "intensity": 1.0, "range": power}))
}

/// The map as a glTF scene: `<name>.gltf`, `models/*.bin`, `textures/*.png`, by relative path.
/// `packs` are the map's model packs (and its add map's), `textures` the texture files to look
/// pictures up in (the map's own first).
pub fn export_map(
    name: &str,
    map: &MapFile,
    packs: &[ModelPack],
    textures: &TextureSearch,
) -> Result<(BTreeMap<String, Vec<u8>>, Report)> {
    let kind_of =
        |model: &str| find_model(packs, model).map_or("model".to_string(), |(kind, _)| kind.name().to_string());
    let tree = map_scene::build_scene(map, name, &kind_of)?;
    let mut report = Report::default();
    let mut scene = Scene { json: Map::new(), files: BTreeMap::new(), images: HashMap::new() };
    scene.json.insert("asset".into(), json!({"version": "2.0", "generator": "DC2-Toolkit dc2tk map-export"}));

    // every model a piece places, once
    let mut used: Vec<String> = Vec::new();
    fn collect(node: &SceneNode, used: &mut Vec<String>) {
        if let Some(model) = &node.model {
            if !used.iter().any(|known| known.eq_ignore_ascii_case(model)) {
                used.push(model.clone());
            }
        }
        node.children.iter().for_each(|child| collect(child, used));
    }
    collect(&tree, &mut used);
    let mut merged: HashMap<String, Merged> = HashMap::new();
    for model in &used {
        let Some((kind, data)) = find_model(packs, model) else {
            if !model.is_empty() {
                report.absent.push(model.clone());
            }
            continue;
        };
        if kind == PackType::Character {
            report.skipped.push(format!("{model}: an animated character piece (shown without its model)"));
            continue;
        }
        let stem = model.rsplit_once('.').map_or(model.as_str(), |(stem, _)| stem);
        let (files, found) = models::export_map_model(data, stem, &options_for(kind, textures)).context(model)?;
        for texture in found.missing_textures {
            if !report.missing_textures.contains(&texture) {
                report.missing_textures.push(texture);
            }
        }
        merged.insert(model.to_lowercase(), scene.merge(stem, &files)?);
        report.models += 1;
    }
    let mut lights = Vec::new();
    let root = scene.add_node(&tree, &merged, &mut lights);
    report.nodes = scene.len("nodes");
    if !lights.is_empty() {
        scene.json.insert("extensions".into(), json!({"KHR_lights_punctual": {"lights": lights}}));
        if !scene.list("extensionsUsed").contains(&json!("KHR_lights_punctual")) {
            scene.list("extensionsUsed").push(json!("KHR_lights_punctual"));
        }
    }
    scene.json.insert("scenes".into(), json!([{"name": name, "nodes": [root]}]));
    scene.json.insert("scene".into(), json!(0));
    let text = serde_json::to_vec(&Value::Object(scene.json)).map_err(|error| Error::invalid(error.to_string()))?;
    let mut files = scene.files;
    files.insert(format!("{name}.gltf"), text);
    Ok((files, report))
}
