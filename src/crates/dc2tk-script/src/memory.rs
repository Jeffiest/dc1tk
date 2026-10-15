//! Memory budgets (docs/assets/memory.md): the fixed memory the engine reads files into and builds
//! maps, models, textures and sound banks in, what a load of the disc's files uses of it, and the
//! check `build` runs over the loads a mod's files change.
//!
//! Some budgets are certain and so are their uses: a file read whole into the 3,200,000-byte read
//! buffer, a map's placements against its placement table. Others are certain in size but the use
//! is estimated: what building a model, a map or a character pack takes of a memory stack is
//! counted the way the engine allocates it (mgCMemory::Alloc in quadwords), for the parts that
//! dominate (models, collision, motions, pictures) and not for every small table. Where several
//! loads share one pool (a town's map, its villagers and its events), the budget is not one
//! number, and a load is compared with the largest of its kind on the disc instead.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use dc2tk_assets::im3;
use dc2tk_assets::info_cfg::{self, Arg};
use dc2tk_assets::mds::{self, F_COLOUR, PRIM_STRIP};
use dc2tk_disc::{archive, text};

/// Bytes in a quadword: mgCMemory sizes and allocations count quadwords.
const QW: u64 = 16;

/// The read buffer every whole-file load goes through: `read_buffer`, 0x30D40 quadwords in town
/// (EditInit, editloop.cpp: MenuBuffer__2, read_buffer_end = read_buffer + 0x30D400) and 200000
/// quadwords in dungeons (memoryInit, dng_main.cpp:286 BuffReadData; EntryEventScript sets
/// read_buffer to it). A map's .map, .cfg, .mpk and .sky, a character pack, a sound bank are read
/// into it whole.
pub const READ_BUFFER: u64 = 3_200_000;
/// Where the loading screen's packets start inside the read buffer in town (EditInit:
/// stSetBuffer(read_buffer + 0x2F9B80, 0x1388 quadwords)) while the first map of a town loads.
pub const TOWN_LOADING_SCREEN: u64 = 0x2F_9B80;
/// The same in dungeons (InitDungeonMain, dng_main.cpp:291: BuffReadData + 0x2E630 quadwords).
pub const DUNGEON_LOADING_SCREEN: u64 = 0x2E_6300;
/// Placements a map may have: the placement table SetPlacePartsBuff allocates (map.cpp:224),
/// 0x140 by default (GetLoadMapInfo and LoadSubMap, mapjump.cpp) and 400 in dungeons
/// (LoadDungeonMapFile, dng_event.cpp). The add map's placements count too.
pub const TOWN_PLACEMENTS: usize = 0x140;
pub const DUNGEON_PLACEMENTS: usize = 400;
/// The script buffer a map's event script is read into: 20000 quadwords, ScriptBuffer__2 in town
/// (EditInit, 0x4E20) and BuffScriptData in dungeons (memoryInit, dng_main.cpp:273). The script
/// takes its size in quadwords, SetEventScript then 0x40 + 0x180 more (event.cpp:106), and
/// mgCMemory::Alloc refuses to reach the end, so a script may have this many bytes.
pub const SCRIPT_BUFFER: u64 = 20000;
pub const SCRIPT_FILE: u64 = (SCRIPT_BUFFER - 1 - 0x40 - 0x180) * QW;
/// Vertices of one skinned mesh: _MODEL builds a mesh's vertex weights (32 bytes each, plus
/// 2 quadwords) in a 6400-quadword work buffer (character.cpp:1588, CreateVertexWeight).
pub const SKINNED_VERTICES: u64 = 3198;
/// Bones one skinned mesh may be weighted to (mgCVisualMotionMDT::bone[0x20], CreateVertexWeight
/// drops the weights past it).
pub const SKINNED_BONES: usize = 32;

/// The memory stacks Max's and Monica's packs build into: mem_table (charasetup.cpp:462, data
/// 0x351D50, quadwords), row 0 in dungeons and row 2 in town (GetCharaMemAllocPtr; SetupMainUnit
/// passes edit_mode 1 from EditInit). Stack 0 takes the model pack (mainchr/c0N_baseM.chr in
/// dungeons, chara/c0N_base.chr in town), 2 the sword, 3 the second weapon, 4 the hat or
/// accessory (charasetup.cpp:100-170).
pub const DUNGEON_PARTY_STACKS: [u64; 7] = [68500, 40000, 7000, 4650, 3000, 9900, 4500];
pub const TOWN_PARTY_STACKS: [u64; 7] = [75000, 40000, 7000, 4650, 3000, 9900, 0];

/// Largest estimated use on the retail disc of each estimated budget, measured by the corpus test
/// `corpus_retail_fits_the_budgets` (which fails when one of these no longer matches the disc).
pub const RETAIL_TOWN_MAP: u64 = 9_471_152;
pub const RETAIL_INTERIOR_MAP: u64 = 4_079_968;
pub const RETAIL_DUNGEON_MAP: u64 = 6_911_504;

// ---------------------------------------------------------------------------------------------
// The disc

/// The disc's files by their DATA.DAT path, in any case.
pub trait Disc: Sync {
    fn read(&self, path: &str) -> Option<Cow<'_, [u8]>>;
    /// Every file's path, as the disc spells it.
    fn paths(&self) -> Vec<String>;
}

/// An extracted disc folder (`extracted/disc`): its files outside the packs' `.d` folders.
pub struct Folder {
    root: PathBuf,
    index: HashMap<String, String>,
}

impl Folder {
    pub fn open(root: &Path) -> std::io::Result<Folder> {
        // a folder that is not there is an error, not an empty disc
        fs::read_dir(root)?;
        let files = crate::paths::walk(root, |entry| !(entry.is_dir && entry.relative.ends_with(".d")))?;
        let index = files.into_iter().map(|relative| (relative.to_lowercase(), relative)).collect();
        Ok(Folder { root: root.to_path_buf(), index })
    }
}

impl Disc for Folder {
    fn read(&self, path: &str) -> Option<Cow<'_, [u8]>> {
        let relative = self.index.get(&path.to_lowercase())?;
        fs::read(self.root.join(relative)).ok().map(Cow::Owned)
    }

    fn paths(&self) -> Vec<String> {
        let mut paths: Vec<String> = self.index.values().cloned().collect();
        paths.sort();
        paths
    }
}

/// The files of a disc image's DATA.DAT.
pub struct Image<'a> {
    image: &'a [u8],
    archive: &'a archive::Archive,
    index: HashMap<String, usize>,
}

impl<'a> Image<'a> {
    pub fn new(image: &'a [u8], archive: &'a archive::Archive) -> Image<'a> {
        let index = archive.entries.iter().enumerate().map(|(at, entry)| (entry.path.to_lowercase(), at)).collect();
        Image { image, archive, index }
    }
}

impl Disc for Image<'_> {
    fn read(&self, path: &str) -> Option<Cow<'_, [u8]>> {
        let entry = &self.archive.entries[*self.index.get(&path.to_lowercase())?];
        Some(Cow::Borrowed(self.archive.read(self.image, entry)))
    }

    fn paths(&self) -> Vec<String> {
        self.archive.entries.iter().map(|entry| entry.path.clone()).collect()
    }
}

/// A disc with some files replaced or added (a build's new files over the original image).
pub struct Overlay<'a> {
    base: &'a dyn Disc,
    files: HashMap<String, &'a [u8]>,
    added: Vec<String>,
}

impl<'a> Overlay<'a> {
    pub fn new(base: &'a dyn Disc, files: &'a BTreeMap<String, Vec<u8>>) -> Overlay<'a> {
        Overlay {
            base,
            files: files.iter().map(|(path, data)| (path.to_lowercase(), data.as_slice())).collect(),
            added: files.keys().cloned().collect(),
        }
    }
}

impl Disc for Overlay<'_> {
    fn read(&self, path: &str) -> Option<Cow<'_, [u8]>> {
        match self.files.get(&path.to_lowercase()) {
            Some(data) => Some(Cow::Borrowed(data)),
            None => self.base.read(path),
        }
    }

    fn paths(&self) -> Vec<String> {
        let mut paths = self.base.paths();
        let known: std::collections::HashSet<String> = paths.iter().map(|path| path.to_lowercase()).collect();
        paths.extend(self.added.iter().filter(|path| !known.contains(&path.to_lowercase())).cloned());
        paths
    }
}

/// A pack member by name, ignoring case as GetPackFile does.
fn member<'a>(pack: &'a [u8], name: &str) -> Option<&'a [u8]> {
    let lower = name.to_lowercase();
    archive::parse_pack_with_data(pack)?
        .into_iter()
        .find(|(member, _)| member.to_lowercase() == lower)
        .map(|(_, data)| data)
}

fn members(pack: &[u8]) -> Vec<(String, &[u8])> {
    archive::parse_pack_with_data(pack).unwrap_or_default()
}

fn round_up(value: u64, to: u64) -> u64 {
    value.div_ceil(to) * to
}

// ---------------------------------------------------------------------------------------------
// A memory stack, counted as mgCMemory counts it

/// Quadwords a load takes of an mgCMemory stack (mg_memory.cpp): Alloc adds its size, stAlloc64
/// and Align64 first round the top up to 64 bytes (the stacks start 64-byte aligned).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stack {
    pub used: u64,
}

impl Stack {
    fn alloc(&mut self, quadwords: u64) {
        self.used += quadwords;
    }

    fn align64(&mut self) {
        self.used = round_up(self.used, 4);
    }

    fn alloc64(&mut self, quadwords: u64) {
        self.align64();
        self.alloc(quadwords);
    }

    /// Bytes used.
    pub fn bytes(&self) -> u64 {
        self.used * QW
    }
}

/// align16_blocks / algn16_size: quadwords that hold `bytes`.
fn blocks(bytes: u64) -> u64 {
    bytes.div_ceil(QW)
}

// ---------------------------------------------------------------------------------------------
// MDS models, read for what building them takes

struct Prim {
    kind: u32,
    count: u32,
    material: u32,
}

struct Mesh {
    vertices: u64,
    normals: u64,
    colours: u64,
    uvs: u64,
    materials: u64,
    prims: Vec<Prim>,
}

struct Object {
    name: Vec<u8>,
    parent: i32,
    mesh: Option<Mesh>,
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    data.get(at..at + 4).map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
}

fn i32_at(data: &[u8], at: usize) -> Option<i32> {
    u32_at(data, at).map(|value| value as i32)
}

/// Indices per corner of a primitive of this type: the vertex, a normal, a uv, a colour
/// (CreateFace).
fn stride(kind: u32) -> u64 {
    mds::Prim::stride_of(kind) as u64
}

/// The frames of an MDS file and the counts of each mesh (MDS_HEADER, MDTOBJ_HEADER records of
/// 0x70 bytes, MDT_HEADER, the faces section: a count at +8 and FACES_ID records from +0x10).
fn objects(mds: &[u8]) -> Option<Vec<Object>> {
    if mds.get(..4)? != b"MDS\0" {
        return None;
    }
    let count = u32_at(mds, 8)? as usize;
    let mut at = u32_at(mds, 12)? as usize;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let size = u32_at(mds, at + 4)? as usize;
        let raw = mds.get(at + 8..at + 40)?;
        let name = raw[..raw.iter().position(|&b| b == 0).unwrap_or(raw.len())].to_vec();
        let mdt = i32_at(mds, at + 40)?;
        let parent = i32_at(mds, at + 44)?;
        let mesh = if mdt > 0 { Some(mesh(mds, mdt as usize)?) } else { None };
        out.push(Object { name, parent, mesh });
        at += size.max(0x70);
    }
    Some(out)
}

fn mesh(mds: &[u8], at: usize) -> Option<Mesh> {
    let field = |offset: usize| i32_at(mds, at + offset).map(|value| value.max(0) as u64);
    let faces = at + field(40)? as usize;
    let prim_count = u32_at(mds, faces + 8)?;
    let mut prims = Vec::new();
    let mut cursor = faces + 0x10;
    for _ in 0..prim_count {
        let kind = u32_at(mds, cursor)? & 0xFFFF;
        let count = u32_at(mds, cursor + 4)? & 0xFFFF;
        let material = u32_at(mds, cursor + 8)?;
        prims.push(Prim { kind, count, material });
        cursor += 12 + 4 * (count as u64 * stride(kind)) as usize;
    }
    Some(Mesh {
        vertices: field(12)?,
        normals: field(20)?,
        colours: field(28)?,
        uvs: field(44)?,
        materials: field(52)?,
        prims,
    })
}

/// How a frame's mesh is built (mgCreateVisualType).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Visual {
    /// mgCVisualFixMDT: VU packets built once, the default (mgLoadMDSFile).
    Fixed,
    /// mgCVisualMotionMDT: skinned packets with per-corner weights (a VERTEX_ANIME frame).
    Skinned,
    /// mgCVisualMDT: the mesh copied, packets made each frame (SHAPE_ANIME).
    Copied,
    /// mgCShadowMDT: vertices and indices copied (SHADOW_MODEL).
    Shadow,
}

/// Corners a VU1 batch holds: CreateFacePacket fills ((vu1_offset - 2) / qwords per corner / 3) * 3
/// corners per batch, a quadword per attribute (the stride), plus two weight quadwords skinned.
fn batch(vu1_offset: u64, per_corner: u64) -> u64 {
    (vu1_offset - 2) / per_corner / 3 * 3
}

/// Quadwords of the packet CreateFacePacket (mg_visual) or CreateFaceMotionPacket (visualmotion)
/// builds for one primitive: two header quadwords, an end tag, and per batch two tags, the
/// unpack header, the corners and the program call; a strip repeats two corners per batch.
fn packet(prim: &Prim, skinned_bones: Option<u64>) -> u64 {
    let colour = prim.kind & F_COLOUR != 0;
    let corner = stride(prim.kind) + if skinned_bones.is_some() { 2 } else { 0 };
    let size = match skinned_bones {
        // vu1_offset = 0xB4 - bones * 2 (DataAssignMotionMDT); 4 or 5 qwords per corner, +1 colour
        Some(bones) => batch(0xB4 - bones * 2, if colour { 6 } else { 5 }),
        // vu1_offset = 180 (mgCVisualMDT::Initialize)
        None => batch(180, if colour { 4 } else { 3 }),
    };
    let mut quadwords = 3;
    let mut left = prim.count as u64;
    while left > 0 {
        let corners = left.min(size);
        quadwords += 4 + corner * corners;
        if prim.kind & 7 == PRIM_STRIP && size < left {
            left += 2;
        }
        left -= size.min(left);
    }
    quadwords
}

/// mgCVisualMDT::CreateFace: an mgCFace (3 quadwords) per primitive, its index list in `memory`
/// when the mesh is copied (in a scratch buffer otherwise), and a face group (4 quadwords) when
/// the walk finds none to join. The walk never joins the last group of the list, only earlier
/// ones of the same material (CreateFace at 0x134xxx, asm).
fn faces(stack: &mut Stack, mesh: &Mesh, visual: Visual, bones: u64) {
    let mut groups: Vec<u32> = Vec::new();
    for prim in &mesh.prims {
        stack.alloc(3);
        if matches!(visual, Visual::Copied | Visual::Shadow) {
            stack.alloc(prim.count as u64 * stride(prim.kind) / 4 + 1);
        }
        let joined = groups.len() > 1 && groups[..groups.len() - 1].contains(&prim.material);
        if !joined {
            groups.push(prim.material);
            stack.alloc(4);
        }
        match visual {
            Visual::Fixed => stack.alloc(packet(prim, None)),
            Visual::Skinned => stack.alloc(packet(prim, Some(bones))),
            Visual::Copied | Visual::Shadow => {}
        }
    }
}

/// Bones each frame of a model is weighted to: the distinct targets of the frame's key lists in
/// the model's .wgt (Mot_File_List records: frame, target, type, key count, more) that have keys
/// (CreateVertexWeight skips the empty ones every bone has).
fn bones_by_frame(weight: &[u8]) -> HashMap<u32, Vec<u32>> {
    let mut out: HashMap<u32, Vec<u32>> = HashMap::new();
    for (frame, target, _) in key_lists(weight).into_iter().filter(|&(_, _, keys)| keys > 0) {
        let bones = out.entry(frame).or_default();
        if !bones.contains(&target) {
            bones.push(target);
        }
    }
    out
}

/// (frame, target, key count) of each key list of a .mot or .wgt file (gameutil.hpp: a 0x20-byte
/// Mot_File_List, then key_count 0x20-byte keys; `more` is nonzero while another list follows).
fn key_lists(data: &[u8]) -> Vec<(u32, u32, u64)> {
    let mut out = Vec::new();
    let mut at = 0;
    while let (Some(frame), Some(target), Some(keys), Some(more)) =
        (u32_at(data, at), u32_at(data, at + 4), u32_at(data, at + 16), u32_at(data, at + 20))
    {
        out.push((frame, target, keys as u64));
        at += 0x20 * (1 + keys as usize);
        if more == 0 {
            break;
        }
    }
    out
}

/// What mgLoadMDSFile (mg_dataset.cpp:502) takes building `mds`: the frames, their names,
/// attributes and bounds, and each mesh's visual, materials, faces and packets.
fn model(stack: &mut Stack, mds: &[u8], visual_of: &dyn Fn(&[u8]) -> Visual, weight: Option<&[u8]>) -> Option<()> {
    let objects = objects(mds)?;
    let n = objects.len() as u64;
    stack.alloc(blocks(n * 0x110) + 2);
    stack.alloc(blocks(n * 4) + 2);
    if weight.is_some() {
        stack.alloc(blocks((n + 2) * 64) + 2);
    }
    let bones = weight.map(bones_by_frame).unwrap_or_default();
    for (index, object) in objects.iter().enumerate() {
        stack.alloc((object.name.len() as u64 + 1) / 16 + 1);
        let dashes = object.name.windows(2).any(|pair| pair == b"--");
        if dashes || object.mesh.is_some() || object.parent < 0 {
            stack.alloc(11);
        }
        let Some(mesh) = &object.mesh else { continue };
        stack.alloc(11);
        let mut visual = visual_of(&object.name);
        if visual == Visual::Skinned && weight.is_none() {
            visual = Visual::Fixed;
        }
        // the visual (mgCVisualMotionMDT 0x110, the others 0x50) and Alloc(1)
        stack.alloc(if visual == Visual::Skinned { 19 } else { 7 });
        stack.alloc(1);
        match visual {
            Visual::Fixed | Visual::Skinned => stack.alloc(mesh.materials * 3),
            Visual::Copied => {
                for count in [mesh.vertices, mesh.normals, mesh.uvs, mesh.colours, mesh.materials * 3] {
                    stack.alloc(count);
                }
            }
            Visual::Shadow => {
                for count in [mesh.vertices, mesh.colours, mesh.materials * 3] {
                    stack.alloc(count);
                }
            }
        }
        let bones = bones.get(&(index as u32)).map_or(0, |list| list.len().min(SKINNED_BONES) as u64);
        faces(stack, mesh, visual, bones);
    }
    Some(())
}

/// What LoadCollisionFile (collision.cpp:387) takes: a CColFrame (0x120) per frame, and per mesh a
/// CCollisionMDT, a 0x50-byte CCPoly per triangle and a bound. A mesh with strips or colours gets
/// no polygons (CreateCollisionMDT refuses it).
fn collision(stack: &mut Stack, mds: &[u8]) -> Option<()> {
    let objects = objects(mds)?;
    stack.alloc(objects.len() as u64 * 0x120 / 16 + 2);
    for mesh in objects.iter().filter_map(|object| object.mesh.as_ref()) {
        stack.alloc(7);
        if !mesh.prims.iter().any(|prim| prim.kind & 7 == PRIM_STRIP || prim.kind & F_COLOUR != 0) {
            let triangles: u64 = mesh.prims.iter().map(|prim| prim.count as u64 / 3).sum();
            stack.alloc(triangles * 0x50 / 16);
        }
        stack.alloc(13);
    }
    Some(())
}

/// What CreateAnimeDataEX (gameutil.cpp:807) takes for a .mot or .wgt: per key list a Mot_List,
/// its values (a quadword per key) and its key frames (four per quadword).
fn key_list_memory(stack: &mut Stack, data: &[u8]) {
    for (_, _, keys) in key_lists(data) {
        stack.alloc(0x20 / 16 + 1);
        stack.alloc(keys + 1);
        stack.alloc(keys * 4 / 16 + 1);
    }
}

// ---------------------------------------------------------------------------------------------
// Character packs

fn text_arg(args: &[Arg], index: usize) -> Option<String> {
    match args.get(index) {
        Some(Arg::Str(text)) => Some(text.clone()),
        _ => None,
    }
}

fn int_arg(args: &[Arg], index: usize) -> Option<i64> {
    match args.get(index) {
        Some(Arg::Int(value)) => Some(*value),
        Some(Arg::Float(value)) => Some(*value as i64),
        _ => None,
    }
}

/// The base of a frame name: up to "--" (StrCmp, mg_frame.cpp:948).
fn frame_base(name: &[u8]) -> &[u8] {
    let end = name.windows(2).position(|pair| pair == b"--").unwrap_or(name.len());
    &name[..end]
}

/// What a character pack takes of the stack it is loaded into (CCharacter2::LoadChrFile,
/// ScanInfoFile, character.cpp:1286): its pictures copied (_IMG), its model and shadow model
/// built, its motions converted, its key, outline, sound, cloth and effect tables.
pub fn chara_pack(stack: &mut Stack, pack: &[u8]) -> Option<()> {
    let info = member(pack, "info.cfg")?;
    let statements = info_cfg::parse(info);
    let mut vertex_anime: Vec<Vec<u8>> = Vec::new();
    let mut shape_anime = false;
    let mut keys = 0u64;
    let encode = |name: &str| text::encode(name).unwrap_or_else(|| name.as_bytes().to_vec());
    for (tag, args) in &statements {
        match tag.as_str() {
            "VERTEX_ANIME" => {
                for index in 0..args.len() {
                    if vertex_anime.len() < 24 {
                        if let Some(name) = text_arg(args, index) {
                            vertex_anime.push(frame_base(&encode(&name)).to_vec());
                        }
                    }
                }
            }
            "SHAPE_ANIME" => shape_anime = int_arg(args, 0).unwrap_or(0) != 0,
            "IMG" | "SKIN_IMG" => {
                if let Some(data) = text_arg(args, 1).and_then(|name| member(pack, &name)) {
                    stack.alloc64(data.len() as u64 / 16 + 1);
                }
            }
            "MODEL" | "LOD_MODEL" => {
                let Some(name) = text_arg(args, 0) else { continue };
                let Some(mds) = member(pack, &name) else { continue };
                let stem = name.split('.').next().unwrap_or(&name);
                let weight = member(pack, &format!("{stem}.wgt"));
                let skinned = |frame: &[u8]| {
                    if vertex_anime.iter().any(|wanted| wanted.as_slice() == frame_base(frame)) {
                        if shape_anime {
                            Visual::Copied
                        } else {
                            Visual::Skinned
                        }
                    } else {
                        Visual::Fixed
                    }
                };
                if vertex_anime.is_empty() {
                    model(stack, mds, &|_| Visual::Fixed, None)?;
                } else {
                    model(stack, mds, &skinned, weight)?;
                }
            }
            "SHADOW_MODEL" => {
                if let Some(mds) = text_arg(args, 0).and_then(|name| member(pack, &name)) {
                    model(stack, mds, &|_| Visual::Shadow, None)?;
                }
            }
            "MOTION" | "SHADOW_MOTION" => {
                // MOTION id, mot, bbp[, wgt]; SHADOW_MOTION mot, bbp, wgt
                let skip = if tag == "MOTION" { 1 } else { 0 };
                if let Some(bbp) = text_arg(args, skip + 1).and_then(|name| member(pack, &name)) {
                    stack.alloc(bbp.len() as u64 / 16 + 1);
                }
                if let Some(mot) = text_arg(args, skip).and_then(|name| member(pack, &name)) {
                    key_list_memory(stack, mot);
                }
                if tag == "SHADOW_MOTION" {
                    if let Some(wgt) = text_arg(args, 2).and_then(|name| member(pack, &name)) {
                        key_list_memory(stack, wgt);
                    }
                }
            }
            "KEY_START" => keys = 0,
            "KEY" => keys += 1,
            // the key table, a 0x30-byte CHRINFO_KEY_SET per key and one to end it
            "KEY_END" => stack.alloc64((keys + 1) * 0x30 / 16 + 1),
            "OUTLINE" => stack.alloc(9),
            "SE_START" => stack.alloc(2),
            "SE" | "SELP" => stack.alloc(1),
            "CLOTH_START" => stack.alloc(blocks(int_arg(args, 0).unwrap_or(0).max(0) as u64 * 0x90) + 2),
            _ => {}
        }
    }
    Some(())
}

/// Estimated bytes a character pack takes of the stack it loads into.
pub fn chara_pack_bytes(pack: &[u8]) -> Option<u64> {
    let mut stack = Stack::default();
    chara_pack(&mut stack, pack)?;
    Some(stack.bytes())
}

/// The skinned meshes of a character pack that break the fixed limits of _MODEL: more vertices
/// than its work buffer holds, or more bones than a mesh may use. (frame, vertices, bones).
pub fn skinned_limits(pack: &[u8]) -> Vec<(String, u64, usize)> {
    let mut out = Vec::new();
    let Some(info) = member(pack, "info.cfg") else { return out };
    let statements = info_cfg::parse(info);
    let mut vertex_anime: Vec<Vec<u8>> = Vec::new();
    for (tag, args) in &statements {
        if tag == "VERTEX_ANIME" {
            for index in 0..args.len() {
                if let Some(name) = text_arg(args, index) {
                    vertex_anime.push(frame_base(&text::encode(&name).unwrap_or_default()).to_vec());
                }
            }
        }
        if tag != "MODEL" || vertex_anime.is_empty() {
            continue;
        }
        let Some(name) = text_arg(args, 0) else { continue };
        let (Some(mds), Some(weight)) =
            (member(pack, &name), member(pack, &format!("{}.wgt", name.split('.').next().unwrap_or(&name))))
        else {
            continue;
        };
        let bones = bones_by_frame(weight);
        for (index, object) in objects(mds).unwrap_or_default().iter().enumerate() {
            let Some(mesh) = &object.mesh else { continue };
            if !vertex_anime.iter().any(|wanted| wanted.as_slice() == frame_base(&object.name)) {
                continue;
            }
            let used = bones.get(&(index as u32)).map_or(0, Vec::len);
            if mesh.vertices > SKINNED_VERTICES || used > SKINNED_BONES {
                out.push((text::decode_lossy(&object.name), mesh.vertices, used));
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Maps

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

impl MapKind {
    fn placements(self) -> usize {
        match self {
            MapKind::Dungeon => DUNGEON_PLACEMENTS,
            _ => TOWN_PLACEMENTS,
        }
    }

    fn loading_screen(self) -> u64 {
        match self {
            MapKind::Dungeon => DUNGEON_LOADING_SCREEN,
            _ => TOWN_LOADING_SCREEN,
        }
    }

    fn retail(self) -> u64 {
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

/// The older map table, which the game never reads (LoadMapName reads map<language>.cfg).
const OLD_MAP_TABLE: &str = "map/map.cfg";

/// The maps of the disc's map tables, first listing of each name wins: the older map/map.cfg
/// first, then the languages' (which list the maps alike). Types 7 and 8 load in the dungeon
/// loop; 4 and 6 (interiors) and the four parts of Palm Brinks after m01 (rows 11 to 14 of
/// map.cfg, EditMapJump) load as sub maps.
///
/// map.cfg's types and add maps differ from the game's tables for some maps: s19 is a town there
/// and a dungeon in the game's (which loads it with load_dungeon_map), i03h02 an interior there
/// and a town in the game's. The retail budgets above (RETAIL_INTERIOR_MAP) were measured with
/// map.cfg's kinds; which table tells how each map really loads is not settled.
pub fn map_entries(disc: &dyn Disc) -> Vec<MapEntry> {
    let tables = std::iter::once(OLD_MAP_TABLE.to_string())
        .chain(crate::maptable::LANGUAGES.map(|language| crate::startat::map_table_path(Some(language))));
    let mut out: Vec<MapEntry> = Vec::new();
    for table in tables {
        let Some(data) = disc.read(&table) else { continue };
        for row in crate::maptable::rows(&data) {
            if row.code.is_empty() || out.iter().any(|entry| entry.name.eq_ignore_ascii_case(&row.code)) {
                continue;
            }
            let kind = match row.kind {
                7 | 8 => MapKind::Dungeon,
                4 | 6 => MapKind::Interior,
                _ if (11..=14).contains(&row.number) && table == OLD_MAP_TABLE => MapKind::Interior,
                _ => MapKind::Town,
            };
            out.push(MapEntry { name: row.code, add: row.add, kind });
        }
    }
    out
}

/// Where the engine finds a map's files: map/<first letter>/<letters 1-3>/<letters 4-6>/
/// (GetMapPath, mapselect.cpp:197), and an add map under map/cmn/ (GetLoadMapInfo).
fn map_folder(name: &str) -> (String, String) {
    (crate::mapedit::rule_folder(name), name.to_lowercase())
}

fn add_folder(add: &str) -> (String, String) {
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

// ---------------------------------------------------------------------------------------------
// Texture blocks in GS memory

/// GS blocks (256 bytes) a texture block may fill: every block starts at vram_top, 0x2D20
/// (GetVramTopAddress, mainloop.cpp:101: the 512x480 32-bit frame buffers and Z buffer of mgInit(2,3)
/// end at 0x2D00), and ends at vram_fix, 0x3FE0 (mgCTextureManager::Initialize) less the 32 blocks
/// of the photo work texture LoadTakePhoto keeps fixed at the top (photo.cpp:61).
pub const BLOCK_VRAM: i64 = 0x3FC0 - 0x2D20;
/// Unswizzled 8-bit pictures may be put in the Z buffer instead (CheckCopyToZBufVram, GetZBufVram:
/// frame width x width x 4 / 256), at four times their size, while it has room.
pub const BLOCK_ZBUF: i64 = 512 * 512 * 4 / 256;
const CLUT_BLOCKS: i64 = 4;
const PAGE_BLOCKS: i64 = 32;
/// Texture blocks of the town and dungeon texture managers (SetTableBuffer: 0xDD in EditInit,
/// 0xAF in memoryInit).
pub const TOWN_BLOCKS: i64 = 0xDD;
pub const DUNGEON_BLOCKS: i64 = 0xAF;
/// The town's texture blocks (the MapJumpMapInfo records EditInit sets): the main map and its add
/// map from 0, the interior (sub map, loaded beside the main map) from 0x18, the main map's
/// effects at 0x40 and the interior's at 0x41, the party at 0x46, villagers from 0x4E, the sky at
/// 0xA9 to 0xAC. So a main map has 0x18 blocks when an interior loads beside it, 0x40 when none
/// does; an interior 0x28.
pub const TOWN_INTERIOR_BLOCK: i64 = 0x18;
pub const TOWN_EFFECT_BLOCK: i64 = 0x40;
/// A dungeon map's textures (and its add map's) from block 0, its effects at 0xF
/// (LoadDungeonMapFile), the party's textures at 0x10 (SetupMainUnit image_block), the sky at
/// 0x4C to 0x4F.
pub const DUNGEON_EFFECT_BLOCK: i64 = 0xF;

struct Texture {
    vram: i64,
    zbuf: Option<i64>,
    clut: bool,
}

/// mgCTextureManager as far as GetRemainVRAM sees it: the textures of each block, in order.
struct Blocks {
    blocks: HashMap<i64, Vec<Texture>>,
    spills: usize,
}

impl Blocks {
    fn new() -> Blocks {
        Blocks { blocks: HashMap::new(), spills: 0 }
    }

    fn remain(&self, block: i64) -> i64 {
        let mut vram = 0;
        let mut zbuf = BLOCK_ZBUF;
        for texture in self.blocks.get(&block).into_iter().flatten() {
            match texture.zbuf {
                Some(size) if size <= zbuf => zbuf -= size,
                _ => vram += texture.vram,
            }
            if texture.clut {
                vram += CLUT_BLOCKS;
            }
        }
        BLOCK_VRAM - vram
    }

    fn used(&self) -> i64 {
        self.blocks.iter().filter(|(_, list)| !list.is_empty()).map(|(&block, _)| block + 1).max().unwrap_or(0)
    }
}

/// A TIM2 picture's fields EnterTexture reads, from the start of its file: the picture header
/// follows the 0x10-byte file header.
const TIM2_MIPMAPS: usize = 0x21;
const TIM2_IMAGE_TYPE: usize = 0x23;
const TIM2_WIDTH: usize = 0x24;
const TIM2_HEIGHT: usize = 0x26;

/// A picture file's header (mgIMG_FILE_HEADER, 0x10 bytes): the entries of an IMG or IM2 file
/// (num) and of an IM3 (num3); its entries follow it.
const IMG_COUNT: usize = 4;
const IM3_COUNT: usize = 8;
const IMG_ENTRIES: usize = 0x10;
/// An IMG or IM2 entry (mgIMG1_HEADER, 0x30 bytes): a name of 0x20 bytes, then the picture's
/// offset.
const IMG_ENTRY: usize = 0x30;
const IMG_OFFSET: usize = 0x20;
/// An IM3 entry (mgIMG_HEADER, 0x40 bytes): a name of 0x20 bytes, then the picture's offset, its
/// swizzle, its block (the group) and no_image.
const IM3_ENTRY: usize = 0x40;
const IM3_OFFSET: usize = 0x24;
const IM3_SWIZZLED: usize = 0x28;
const IM3_BLOCK: usize = 0x2C;
const IM3_NO_IMAGE: usize = 0x30;
const NAME: usize = 0x20;

/// EnterTexture for one TIM2 picture (mg_texture.cpp:482, 683): its size in GS memory, rounded up
/// to whole pages, and whether it may go in the Z buffer. None when the engine refuses it.
fn texture(img: &[u8], at: usize, swizzled: bool, no_image: bool) -> Option<Texture> {
    let mips = *img.get(at + TIM2_MIPMAPS)? as usize;
    let bpp = im3::Format::from_type(*img.get(at + TIM2_IMAGE_TYPE)?)?.bpp() as i64;
    let u16_at =
        |field: usize| img.get(at + field..at + field + 2).map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]));
    let width = u16_at(TIM2_WIDTH)? as i64;
    let height = u16_at(TIM2_HEIGHT)? as i64;
    let mut images = [false; 4];
    if !no_image {
        for level in images.iter_mut().take(mips.max(1)) {
            *level = true;
        }
    }
    // Conv32To8 unswizzles a one-level 8-bit picture of up to 0x10000 pixels
    let swizzled = swizzled && !(bpp == 8 && mips == 1 && width * height <= 0x10000);
    let mut level = (if bpp == 24 { 32 } else { bpp }) * width * height / 256 / 8;
    let last = images.iter().position(|&present| !present).map_or(3, |first| first as i64 - 1);
    let mut blocks = 0;
    for present in images.iter().take((last + 1).max(0) as usize) {
        if *present {
            blocks += level;
        }
        level /= 4;
    }
    if !images[0] {
        blocks = level;
    }
    let vram = round_up(blocks as u64, PAGE_BLOCKS as u64) as i64;
    Some(Texture { vram, zbuf: (!swizzled && bpp == 8 && images[0]).then_some(vram * 4), clut: bpp <= 8 })
}

/// mgCTextureManager::EnterIMGFile (mg_texture.cpp:753) of a picture file at `block`: an IM3's
/// pictures sorted by group as the engine sorts them, each entered at block + group + the blocks
/// spilled so far, moving on to the next block when a block runs out ("texture over"); IMG and
/// IM2 files go into one running block. Returns what the engine returns,
/// `last_block_used - current_block`, which CMap::LoadData adds to the next file's block:
/// - for an IM3, the blocks past `block` it used (`current_block` stays `block`);
/// - for an IMG or IM2, always 0. A spill moves `current_block` on and sets the picture's block
///   to it, and `last_block_used` follows the picture's block, so the two stay equal. The next
///   file then starts one block on from this one's start, even when this one spilled past it.
fn enter_img(blocks: &mut Blocks, img: &[u8], block: i64, block_max: i64) -> i64 {
    match img.get(..3) {
        Some(b"IM3") => enter_im3(blocks, img, block, block_max),
        Some(magic) if magic.starts_with(b"IM") => {
            enter_img1(blocks, img, block, block_max, magic == b"IM2");
            // last_block_used - current_block, which are equal here (see above)
            0
        }
        _ => 0,
    }
}

/// An entry of an IM3 file.
struct Im3Entry {
    name: Vec<u8>,
    offset: usize,
    swizzled: bool,
    /// The texture block it goes into, from the one the file is entered at.
    group: i64,
    no_image: bool,
}

fn im3_entries(img: &[u8]) -> Vec<Im3Entry> {
    let count = u32_at(img, IM3_COUNT).unwrap_or(0) as usize;
    let mut entries = Vec::new();
    for index in 0..count {
        let at = IMG_ENTRIES + index * IM3_ENTRY;
        let Some(raw) = img.get(at..at + NAME) else { break };
        entries.push(Im3Entry {
            name: raw[..raw.iter().position(|&b| b == 0).unwrap_or(NAME)].to_vec(),
            offset: i32_at(img, at + IM3_OFFSET).unwrap_or(0).max(0) as usize,
            swizzled: i32_at(img, at + IM3_SWIZZLED).unwrap_or(0) != 0,
            group: i32_at(img, at + IM3_BLOCK).unwrap_or(0) as i64,
            no_image: img.get(at + IM3_NO_IMAGE..at + IM3_NO_IMAGE + 2).is_some_and(|bytes| bytes != [0, 0]),
        });
    }
    entries
}

/// An IM3's pictures entered; the blocks past `block` they reach.
fn enter_im3(blocks: &mut Blocks, img: &[u8], block: i64, block_max: i64) -> i64 {
    let mut entries = im3_entries(img);
    // the engine's exchange sort (not stable), which leaves '#' scripts where they are
    for a in 0..entries.len().saturating_sub(1) {
        for b in a + 1..entries.len() {
            entries[a].group = entries[a].group.max(0);
            entries[b].group = entries[b].group.max(0);
            if entries[b].group < entries[a].group && entries[b].name.first() != Some(&b'#') {
                entries.swap(a, b);
            }
        }
    }
    let mut spill = 0;
    let mut last = block;
    for entry in entries {
        if entry.name.first() == Some(&b'#') || entry.name.is_empty() {
            continue;
        }
        let at = spill + block + entry.group;
        let Some(texture) = texture(img, entry.offset, entry.swizzled, entry.no_image) else { continue };
        if at < 0 || at >= block_max {
            continue;
        }
        blocks.blocks.entry(at).or_default().push(texture);
        let mut placed = at;
        if at < block_max - 1 && blocks.remain(at) < 0 {
            spill += 1;
            blocks.spills += 1;
            let moved = blocks.blocks.get_mut(&at).unwrap().pop().unwrap();
            placed = at + 1;
            blocks.blocks.entry(placed).or_default().push(moved);
        }
        last = last.max(placed);
    }
    last - block
}

/// An IMG or IM2 file's pictures entered into one running block.
fn enter_img1(blocks: &mut Blocks, img: &[u8], block: i64, block_max: i64, swizzled: bool) {
    let count = u32_at(img, IMG_COUNT).unwrap_or(0) as usize;
    let mut current = block;
    for index in 0..count {
        let at = IMG_ENTRIES + index * IMG_ENTRY;
        let Some(raw) = img.get(at..at + NAME) else { break };
        if raw[0] == 0 {
            continue;
        }
        let offset = i32_at(img, at + IMG_OFFSET).unwrap_or(0).max(0) as usize;
        let Some(texture) = texture(img, offset, swizzled, false) else { continue };
        if current >= block_max {
            continue;
        }
        blocks.blocks.entry(current).or_default().push(texture);
        // a spilled picture stays in the old block's list, only its block number moves on
        if current < block_max - 1 && blocks.remain(current) < 0 {
            current += 1;
            blocks.spills += 1;
        }
    }
}

/// What a set of picture files takes of the texture blocks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlockUse {
    /// Blocks from the first: the next load's block minus this load's.
    pub blocks: i64,
    /// Pictures moved on to the next block because theirs was full ("texture over").
    pub spills: usize,
}

/// A map's texture blocks (CMap::LoadData, map.cpp:1506, in LoadMapFromMemory step 2): the add
/// map's picture files then the map's own, each found by the names of the .map's IMG lines and
/// then the add map's (up to 16, AddMapInfo), each file entered at the block the last one left.
pub fn map_blocks(disc: &dyn Disc, entry: &MapEntry) -> Option<BlockUse> {
    let (folder, name) = map_folder(&entry.name);
    let map = dc2tk_assets::mapfile::parse(&disc.read(&format!("{folder}/{name}.map"))?).ok()?;
    let mut names: Vec<String> = map.images.iter().take(16).cloned().collect();
    let mut packs = Vec::new();
    if !entry.add.is_empty() {
        let (add_folder, add_name) = add_folder(&entry.add);
        let add = disc
            .read(&format!("{add_folder}/{add_name}.map"))
            .and_then(|data| dc2tk_assets::mapfile::parse(&data).ok());
        for image in add.map(|add| add.images).unwrap_or_default() {
            if names.len() < 16 {
                names.push(image);
            }
        }
        packs.push(disc.read(&format!("{add_folder}/{add_name}.ipk")));
    }
    packs.push(disc.read(&format!("{folder}/{name}.ipk")));
    let block_max = if entry.kind == MapKind::Dungeon { DUNGEON_BLOCKS } else { TOWN_BLOCKS };
    let mut blocks = Blocks::new();
    let mut block = 0;
    for pack in packs.iter().flatten() {
        for image in &names {
            if let Some(img) = member(pack, image) {
                block += enter_img(&mut blocks, img, block, block_max) + 1;
            }
        }
    }
    Some(BlockUse { blocks: block.max(blocks.used()), spills: blocks.spills })
}

/// Picture files entered together at one block (a character's pictures, an effect file's): the
/// blocks they reach.
pub fn shared_block(images: &[&[u8]]) -> BlockUse {
    let mut blocks = Blocks::new();
    for img in images {
        enter_img(&mut blocks, img, 0, TOWN_BLOCKS);
    }
    BlockUse { blocks: blocks.used(), spills: blocks.spills }
}

/// The picture files a character pack enters (its IMG lines; every .img member when it names none).
pub fn pack_images(pack: &[u8]) -> Vec<&[u8]> {
    let mut slots: BTreeMap<i64, String> = BTreeMap::new();
    if let Some(info) = member(pack, "info.cfg") {
        for (tag, args) in info_cfg::parse(info) {
            if tag == "IMG" {
                if let (Some(slot), Some(name)) = (int_arg(&args, 0), text_arg(&args, 1)) {
                    slots.insert(slot, name);
                }
            }
        }
    }
    if slots.is_empty() {
        return members(pack)
            .into_iter()
            .filter(|(name, _)| name.to_lowercase().ends_with(".img"))
            .map(|(_, data)| data)
            .collect();
    }
    slots.values().filter_map(|name| member(pack, name)).collect()
}

/// The .img members of a pack (an effect file's pictures, a sky's).
pub fn img_members(pack: &[u8]) -> Vec<(String, &[u8])> {
    members(pack).into_iter().filter(|(name, _)| name.to_lowercase().ends_with(".img")).collect()
}

// ---------------------------------------------------------------------------------------------
// Sound memory (SPU2)

/// SPU2 regions a bank's samples (.bd) load into, each bank taking bd + 16 bytes (sndLoadSound's
/// LoadHdBd and LoadHdBdAdd place the next bank 16 bytes past the last's samples). The bases are
/// CSound::Init's (sound.cpp:271-283).
/// Music (BG_*), game ports 0 and 11, from 0x5210 up to the field banks at 0x7D210.
pub const SPU_MUSIC: u64 = 0x7_8000;
/// The field banks stacked up from 0x7D210 (base BS_*, battle FG_*, ambient SR_*, objects OB_*,
/// one event bank EV_*) and the monster banks (EN_*) stacked down from 0x18AE20 share this.
pub const SPU_FIELD: u64 = 0x10_DC10;
/// The system sounds SY_000 from 0x18AE20 up to the menu banks at 0x1A82E0.
pub const SPU_SYSTEM: u64 = 0x1_D4C0;
/// A menu bank (SP_*) from 0x1A82E0 up to core 1's reverb work area at 0x1D9020 (its start is
/// estimated from LIBSD's work-area size for mode 4, which every reverb setting on the disc uses).
pub const SPU_MENU: u64 = 0x3_0D40;
/// The player character's bank (CH_*) from 0x1E0000 up to core 0's reverb work area at 0x1F9020
/// (estimated as SPU_MENU's end).
pub const SPU_CHARACTER: u64 = 0x1_9020;
/// The IOP staging buffer every .bd is copied through (sound.cpp:122-159): a .bd up to 0x6DE00
/// goes in one piece and must fit its 0x6DD00 bytes, a larger one in two and may be 0xDBA00.
pub const IOP_STAGING: u64 = 0x6_DD00;
/// The fullest field set the scripts make on the disc: s01's set 1600 with the event bank
/// EV_100_155 (s01.c play_blackstone_railroad_arrival_scene) needs 0x10E180, 1,392 bytes over,
/// and overwrites the start of SY_000's samples; the next fullest (d03b02's 3691 with
/// EV_115_158) leaves 5,792 bytes.
pub const SPU_FIELD_RETAIL_FREE: i64 = 5_792;

/// The size of a bank's samples: its .bd member.
pub fn bank_samples(bank: &[u8]) -> Option<u64> {
    members(bank).into_iter().find(|(name, _)| name.to_lowercase().ends_with(".bd")).map(|(_, data)| data.len() as u64)
}

/// The banks of each sound set of snd2/MAP_index.txt (tab-separated: set, name, kind, text,
/// BG, BS, FG, SR, SR sequence, six OB columns, EV, ...), as CScene::LoadSound loads its field
/// layers: (set, [bank names]). A '*' (keep what is loaded) or '-' names nothing; a '*' among
/// the object columns ends them (LoadSndFileInfo).
fn sound_sets(index: &[u8]) -> Vec<(i64, Vec<String>)> {
    let mut out = Vec::new();
    for line in text::decode_lossy(index).lines() {
        let fields: Vec<String> = line.split('\t').map(|field| field.replace(' ', "")).collect();
        let Ok(set) = fields.first().map_or("", |field| field.as_str()).parse::<i64>() else { continue };
        let mut banks = Vec::new();
        let named = |field: Option<&String>, prefix: &str| {
            field.filter(|field| field.starts_with(prefix) && field.len() >= 6).map(|field| field[..6].to_string())
        };
        banks.extend(named(fields.get(5), "BS"));
        banks.extend(named(fields.get(6), "FG"));
        banks.extend(named(fields.get(7), "SR"));
        for column in 9..15 {
            match fields.get(column) {
                Some(field) if field.starts_with('*') => break,
                field => {
                    if let Some(bank) = named(field, "OB") {
                        if !banks.contains(&bank) {
                            banks.push(bank);
                        }
                    }
                }
            }
        }
        out.push((set, banks));
    }
    out
}

fn check_sound(original: &dyn Disc, modded: &dyn Disc, path: &str) -> Vec<Finding> {
    let mut out = check_read_whole(original, modded, path, "sound bank");
    let Some(new) = modded.read(path).as_deref().and_then(bank_samples) else { return out };
    let old = original.read(path).as_deref().and_then(bank_samples);
    let name = path.rsplit('/').next().unwrap_or(path).to_uppercase();
    let grew = old.is_none_or(|old| new > old);
    if (IOP_STAGING + 1..=IOP_STAGING + 0x100).contains(&new) || new > 2 * IOP_STAGING {
        out.push(finding(
            Level::Error,
            format!(
                "{path}: {} bytes of samples; the IOP copies a bank's samples through a {}-byte buffer, in one piece up to {} bytes and in two above, so it takes at most {} or else {} to {} bytes",
                n(new),
                n(IOP_STAGING),
                n(IOP_STAGING + 0x100),
                n(IOP_STAGING),
                n(IOP_STAGING + 0x101),
                n(2 * IOP_STAGING)
            ),
        ));
    }
    let region = match name.get(..2) {
        Some("BG") => Some((SPU_MUSIC, "music", Level::Error)),
        Some("SY") => Some((SPU_SYSTEM, "system sound", Level::Error)),
        Some("SP") => Some((SPU_MENU, "menu sound", Level::Warning)),
        Some("CH") => Some((SPU_CHARACTER, "player character's sound", Level::Warning)),
        _ => None,
    };
    if let Some((budget, what, level)) = region {
        if new + 16 > budget && grew {
            out.push(finding(
                level,
                format!(
                    "{path}: {} bytes of samples (+16), more than the {} of the {what} region of sound memory it loads into",
                    n(new),
                    n(budget)
                ),
            ));
        }
        if grew {
            out.push(finding(Level::Note, format!("{path}: {what} region {} of {}", n(new + 16), n(budget))));
        }
        return out;
    }
    if !["BS", "FG", "SR", "OB", "EV", "EN"].contains(&name.get(..2).unwrap_or("")) {
        return out;
    }
    // the sets that load this bank with its neighbours, each a stack of banks loaded together
    let stem = name.strip_suffix(".SND").unwrap_or(&name);
    let mut fullest: Option<(i64, u64)> = None;
    if let Some(index) = modded.read("snd2/MAP_index.txt") {
        let paths = modded.paths();
        let bank_size = |bank: &str| {
            let wanted = format!("{}.snd", bank.to_lowercase());
            paths
                .iter()
                .find(|path| {
                    path.to_lowercase().starts_with("snd2/") && path.to_lowercase().ends_with(&format!("/{wanted}"))
                })
                .and_then(|path| modded.read(path).as_deref().and_then(bank_samples))
        };
        let mut sizes: HashMap<String, u64> = HashMap::new();
        for (set, banks) in sound_sets(&index) {
            if !banks.iter().any(|bank| bank.eq_ignore_ascii_case(stem)) {
                continue;
            }
            let mut total = 0;
            for bank in &banks {
                let size = *sizes.entry(bank.clone()).or_insert_with(|| bank_size(bank).unwrap_or(0));
                total += size + 16;
            }
            if fullest.is_none_or(|(_, most)| total > most) {
                fullest = Some((set, total));
            }
        }
    }
    if let Some((set, total)) = fullest {
        if total > SPU_FIELD {
            out.push(finding(
                Level::Error,
                format!(
                    "{path}: sound set {set} loads it with banks that need {} bytes together, more than the {} of the field region of sound memory",
                    n(total),
                    n(SPU_FIELD)
                ),
            ));
        }
    }
    if let (Some(old), true) = (old, grew) {
        out.push(finding(
            Level::Warning,
            format!(
                "{path}: its samples grew by {} bytes; field, event and monster banks share {} bytes of sound memory{}, and the fullest set the disc's scripts make leaves {} (or none: s01's set 1600 with EV_100_155 is 1,392 over already), so growth overwrites the system sounds after it (estimated from the scripts' loads)",
                n(new - old),
                n(SPU_FIELD),
                fullest.map_or(String::new(), |(set, total)| format!(" (its fullest set, {set}, takes {} without event and monster banks)", n(total))),
                n(SPU_FIELD_RETAIL_FREE as u64)
            ),
        ));
    }
    out
}

// ---------------------------------------------------------------------------------------------
// The party's packs

/// The stack a party pack builds into, by its path: (stack, quadwords, where). Max's and Monica's
/// model packs into stack 0, the sword into 2, the second weapon into 3, the hat or accessory
/// into 4 (CCharacter2::LoadPack), the costume into 1 and the shoes into 5 (LoadSkin), in
/// dungeons from mainchr/ and in town the model pack from chara/ (SetupMainUnit).
pub fn party_stack(path: &str) -> Option<(usize, u64, &'static str)> {
    let lower = path.to_lowercase();
    let (folder, name) = lower.rsplit_once('/')?;
    let name = name.strip_suffix(".chr")?;
    let family = name.trim_end_matches(|c: char| c.is_ascii_digit());
    if folder == "chara" {
        return matches!(name, "c01_base" | "c02_base").then_some((0, TOWN_PARTY_STACKS[0], "town"));
    }
    if folder != "mainchr" {
        return None;
    }
    let stack = match family {
        "c01_base" | "c02_base" => 0,
        "c01fuku" | "c02fuku" => 1,
        "ws" | "wm" => 2,
        "wg" | "wr" => 3,
        "ha" | "ac" => 4,
        "sh" | "bo" => 5,
        _ => return None,
    };
    Some((stack, DUNGEON_PARTY_STACKS[stack], "dungeon"))
}

// ---------------------------------------------------------------------------------------------
// The check build runs

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// A budget that is certain is exceeded: the game will not load it as made.
    Error,
    /// An estimated budget is exceeded, or a load leaves what the disc shows to work.
    Warning,
    /// What a changed load uses.
    Note,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub level: Level,
    pub text: String,
}

fn finding(level: Level, text: String) -> Finding {
    Finding { level, text }
}

/// Thousands separated, for reading.
fn n(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// The files a map's load reads.
fn map_paths(entry: &MapEntry) -> Vec<String> {
    let mut out = Vec::new();
    let (folder, name) = map_folder(&entry.name);
    for extension in ["map", "cfg", "mpk", "ipk", "efp", "sky"] {
        out.push(format!("{folder}/{name}.{extension}"));
    }
    if !entry.add.is_empty() {
        let (folder, name) = add_folder(&entry.add);
        for extension in ["map", "cfg", "mpk", "ipk"] {
            out.push(format!("{folder}/{name}.{extension}"));
        }
    }
    out
}

/// A map file's map name (map/s/s17/s17.mpk -> s17), when it is a map's own file.
fn map_of(path: &str) -> Option<String> {
    let lower = path.to_lowercase();
    let rest = lower.strip_prefix("map/")?;
    if rest.starts_with("cmn/") {
        return None;
    }
    let (_, file) = rest.rsplit_once('/')?;
    let (stem, extension) = file.rsplit_once('.')?;
    ["map", "cfg", "mpk", "ipk", "efp", "sky"].contains(&extension).then(|| stem.to_string())
}

/// What `build` says about the memory the changed files' loads use: an error where a budget that
/// is certain is exceeded, a warning where an estimated one is (and the load grew), a note of what
/// each changed load uses. `changed` are the DATA.DAT paths the build rewrites or adds.
pub fn check(original: &dyn Disc, modded: &dyn Disc, changed: &[String]) -> Vec<Finding> {
    // a file written back with its own bytes changes no load
    let changed: Vec<String> =
        changed.iter().filter(|path| modded.read(path) != original.read(path)).cloned().collect();
    let changed = changed.as_slice();
    let mut out = Vec::new();
    let mut covered: Vec<String> = Vec::new();

    // maps whose files changed, listed or found by name
    let mut entries = map_entries(modded);
    for path in changed {
        if let Some(name) = map_of(path) {
            if !entries.iter().any(|entry| entry.name.eq_ignore_ascii_case(&name)) {
                let kind = match name.chars().next() {
                    Some('d') => MapKind::Dungeon,
                    Some('i') => MapKind::Interior,
                    _ => MapKind::Town,
                };
                entries.push(MapEntry { name, add: String::new(), kind });
            }
        }
    }
    for entry in &entries {
        let paths = map_paths(entry);
        let touched: Vec<&String> = changed.iter().filter(|path| paths.contains(&path.to_lowercase())).collect();
        if touched.is_empty() {
            continue;
        }
        covered.extend(touched.iter().map(|path| path.to_string()));
        out.extend(check_map(original, modded, entry));
    }
    for path in changed {
        let lower = path.to_lowercase();
        if lower.ends_with(".efp") && lower.starts_with("map/") {
            out.extend(check_shared_images(original, modded, path, "map effects", 1));
        } else if lower.ends_with(".sky") {
            covered.push(path.clone());
            out.extend(check_sky(original, modded, path));
        } else if lower.ends_with(".chr") {
            covered.push(path.clone());
            out.extend(check_chara(original, modded, path));
        } else if lower.ends_with(".snd") {
            covered.push(path.clone());
            out.extend(check_sound(original, modded, path));
        } else if lower.ends_with(".stb") && lower.starts_with("map/") {
            covered.push(path.clone());
            out.extend(check_script(original, modded, path));
        }
    }
    for path in changed {
        if covered.contains(path) {
            continue;
        }
        let (Some(new), old) = (modded.read(path), original.read(path)) else { continue };
        let old_size = old.as_ref().map_or(0, |data| data.len());
        if new.len() > old_size {
            out.push(finding(
                Level::Note,
                format!(
                    "{path}: {} bytes ({}); no budget of docs/assets/memory.md covers this file, so keep it near its original size",
                    n(new.len() as u64),
                    if old.is_some() { format!("was {}", n(old_size as u64)) } else { "a new file".to_string() }
                ),
            ));
        }
    }
    out
}

fn check_map(original: &dyn Disc, modded: &dyn Disc, entry: &MapEntry) -> Vec<Finding> {
    let mut out = Vec::new();
    let name = &entry.name;
    let kind = entry.kind;
    let Some(new) = map_load(modded, entry) else { return out };
    let old = map_load(original, entry);
    let blocks = map_blocks(modded, entry).unwrap_or_default();
    let old_blocks = map_blocks(original, entry);
    if new.read > READ_BUFFER {
        out.push(finding(
            Level::Error,
            format!(
                "map {name}: its .map, .cfg, .mpk and sky read into the read buffer take {} bytes, more than its {} (LoadMapData)",
                n(new.read),
                n(READ_BUFFER)
            ),
        ));
    } else if new.read > kind.loading_screen() {
        let level = if kind == MapKind::Dungeon { Level::Warning } else { Level::Error };
        out.push(finding(
            level,
            format!(
                "map {name}: its .map, .cfg, .mpk and sky take {} bytes of the read buffer, past {} where the loading screen's packets lie while the first map loads",
                n(new.read),
                n(kind.loading_screen())
            ),
        ));
    }
    if new.placements > kind.placements() {
        out.push(finding(
            Level::Error,
            format!(
                "map {name}: {} placements, more than the {} its placement table holds (SetPlacePartsBuff); the rest are not placed",
                new.placements,
                kind.placements()
            ),
        ));
    }
    let old_used = old_blocks.map_or(0, |blocks| blocks.blocks);
    let (limit, limit_text) = match kind {
        MapKind::Dungeon => (DUNGEON_EFFECT_BLOCK, "its effects' block 0xF"),
        MapKind::Interior => (TOWN_EFFECT_BLOCK - TOWN_INTERIOR_BLOCK, "the main map's effects at block 0x40"),
        MapKind::Town => (TOWN_EFFECT_BLOCK, "its effects' block 0x40"),
    };
    if blocks.blocks > limit {
        out.push(finding(
            Level::Error,
            format!(
                "map {name}: its pictures take {} texture blocks, reaching {limit_text} (it has {limit}); pictures past it are overwritten",
                blocks.blocks
            ),
        ));
    } else if kind == MapKind::Town && blocks.blocks > TOWN_INTERIOR_BLOCK && old_used <= TOWN_INTERIOR_BLOCK {
        out.push(finding(
            Level::Warning,
            format!(
                "map {name}: its pictures take {} texture blocks, past the 24 a town map has when an interior loads beside it (from block 0x18); fine only if no interior or Palm Brinks part loads with it",
                blocks.blocks
            ),
        ));
    }
    if let Some(old) = &old {
        if new.stack > kind.retail() && new.stack > old.stack {
            out.push(finding(
                Level::Warning,
                format!(
                    "map {name}: estimated {} bytes of map memory, more than the largest {} on the disc ({}); its villagers and events share what is left, so it may run short (estimated)",
                    n(new.stack),
                    kind.describe(),
                    n(kind.retail())
                ),
            ));
        }
    }
    let was = |value: u64, old: Option<u64>| match old {
        Some(old) if old != value => format!(" (was {})", n(old)),
        _ => String::new(),
    };
    out.push(finding(
        Level::Note,
        format!(
            "map {name} ({}): read buffer {}{} of {}; placements {} of {}; texture blocks {}{} of {}; map memory about {}{} (largest {} on the disc {})",
            kind.describe(),
            n(new.read),
            was(new.read, old.as_ref().map(|old| old.read)),
            n(READ_BUFFER),
            new.placements,
            kind.placements(),
            blocks.blocks,
            match old_blocks {
                Some(old) if old.blocks != blocks.blocks => format!(" (was {})", old.blocks),
                _ => String::new(),
            },
            if kind == MapKind::Town { "24 beside an interior, 64 alone".to_string() } else { limit.to_string() },
            n(new.stack),
            was(new.stack, old.as_ref().map(|old| old.stack)),
            kind.describe(),
            n(kind.retail())
        ),
    ));
    out
}

/// Picture files a pack enters at one block: compared with one block and with the original.
fn check_shared_images(original: &dyn Disc, modded: &dyn Disc, path: &str, what: &str, blocks: i64) -> Vec<Finding> {
    let span = |disc: &dyn Disc| {
        disc.read(path).map(|data| {
            let images: Vec<&[u8]> = img_members(&data).into_iter().map(|(_, img)| img).collect();
            shared_block(&images)
        })
    };
    let (Some(new), old) = (span(modded), span(original)) else { return Vec::new() };
    let old_blocks = old.map_or(0, |old| old.blocks);
    if new.blocks > blocks && new.blocks > old_blocks {
        return vec![finding(
            Level::Warning,
            format!(
                "{path}: the {what}' pictures take {} texture blocks where they have {blocks}; the next block's owner overwrites the rest",
                new.blocks
            ),
        )];
    }
    Vec::new()
}

/// A sky's four picture files each go into a block of their own (CMapSky::LoadPack enters sky0 to
/// sky3 at the sky's block + 0 to 3, and deletes each block first), so each must fit in one.
fn check_sky(original: &dyn Disc, modded: &dyn Disc, path: &str) -> Vec<Finding> {
    let mut out = check_read_whole(original, modded, path, "sky");
    let Some(data) = modded.read(path) else { return out };
    for (name, img) in img_members(&data) {
        let used = shared_block(&[img]);
        if used.blocks > 1 {
            out.push(finding(
                Level::Error,
                format!(
                    "{path}: {name} needs {} texture blocks; a sky picture file has one (the next one's load deletes the rest)",
                    used.blocks
                ),
            ));
        }
    }
    out
}

/// A map's event script: read into the script buffer, which then holds the interpreter's stack
/// and call tables (SetEventScript, event.cpp:106: 0x40 and 0x180 quadwords).
fn check_script(original: &dyn Disc, modded: &dyn Disc, path: &str) -> Vec<Finding> {
    let Some(new) = modded.read(path) else { return Vec::new() };
    let old = original.read(path).map(|data| data.len() as u64);
    let size = new.len() as u64;
    if size > SCRIPT_FILE {
        return vec![finding(
            Level::Error,
            format!(
                "{path}: {} bytes; a map's script and the interpreter's tables share a {}-byte script buffer, which holds a script of at most {} bytes (LoadScript, SetEventScript)",
                n(size),
                n(SCRIPT_BUFFER * QW),
                n(SCRIPT_FILE)
            ),
        )];
    }
    if old.is_some_and(|old| size > old) {
        return vec![finding(Level::Note, format!("{path}: script {} bytes of at most {}", n(size), n(SCRIPT_FILE)))];
    }
    Vec::new()
}

fn check_read_whole(original: &dyn Disc, modded: &dyn Disc, path: &str, what: &str) -> Vec<Finding> {
    let Some(new) = modded.read(path) else { return Vec::new() };
    let old = original.read(path).map_or(0, |data| data.len());
    if new.len() as u64 > READ_BUFFER {
        return vec![finding(
            Level::Error,
            format!(
                "{path}: {} bytes; a {what} is read whole into the {}-byte read buffer",
                n(new.len() as u64),
                n(READ_BUFFER)
            ),
        )];
    }
    if new.len() > old {
        return vec![finding(
            Level::Note,
            format!("{path}: {} bytes (read buffer {}), was {}", n(new.len() as u64), n(READ_BUFFER), n(old as u64)),
        )];
    }
    Vec::new()
}

fn check_chara(original: &dyn Disc, modded: &dyn Disc, path: &str) -> Vec<Finding> {
    let mut out = check_read_whole(original, modded, path, "character pack");
    let Some(new) = modded.read(path) else { return out };
    let old = original.read(path);
    let old_limits = old.as_ref().map(|old| skinned_limits(old)).unwrap_or_default();
    for (frame, vertices, bones) in skinned_limits(&new) {
        if old_limits.iter().any(|(name, _, _)| *name == frame) {
            continue;
        }
        let what = if vertices > SKINNED_VERTICES {
            format!("{vertices} vertices, more than the {SKINNED_VERTICES} the skinning work buffer holds (_MODEL)")
        } else {
            format!("{bones} bones, more than the {SKINNED_BONES} a skinned mesh may use (CreateVertexWeight drops the rest)")
        };
        out.push(finding(Level::Error, format!("{path}: skinned mesh {frame} has {what}")));
    }
    let estimate = chara_pack_bytes(&new);
    let old_estimate = old.as_ref().and_then(|old| chara_pack_bytes(old));
    if let (Some((stack, quadwords, mode)), Some(estimate)) = (party_stack(path), estimate) {
        let budget = quadwords * QW;
        if estimate > budget && Some(estimate) > old_estimate {
            out.push(finding(
                Level::Warning,
                format!(
                    "{path}: estimated {} bytes, more than the {} of the party's memory stack {stack} it loads into in {mode}s (mem_table); the game prints \"stack over\" and the pack loads incomplete",
                    n(estimate),
                    n(budget)
                ),
            ));
        }
        out.push(finding(
            Level::Note,
            format!(
                "{path}: party stack {stack} ({mode}): about {}{} of {}",
                n(estimate),
                match old_estimate {
                    Some(old) if old != estimate => format!(" (was {})", n(old)),
                    _ => String::new(),
                },
                n(budget)
            ),
        ));
    } else if let (Some(estimate), Some(old_estimate)) = (estimate, old_estimate) {
        if estimate > old_estimate {
            out.push(finding(
                Level::Note,
                format!(
                    "{path}: about {} bytes of the stack it loads into, was {} (event, villager and monster stacks share what a scene leaves; keep near the original)",
                    n(estimate),
                    n(old_estimate)
                ),
            ));
        }
    }
    let blocks = shared_block(&pack_images(&new));
    let old_blocks = old.as_ref().map_or(0, |old| shared_block(&pack_images(old)).blocks);
    let room = if party_stack(path).is_some() { 8 } else { 1 };
    if blocks.blocks > room && blocks.blocks > old_blocks {
        out.push(finding(
            Level::Warning,
            format!(
                "{path}: its pictures take {} texture blocks where it has {room}; the next character's pictures overwrite the rest",
                blocks.blocks
            ),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use dc2tk_assets::mds as mdsfile;

    /// A disc of files given in memory.
    struct Files(BTreeMap<String, Vec<u8>>);

    impl Disc for Files {
        fn read(&self, path: &str) -> Option<Cow<'_, [u8]>> {
            self.0
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(path))
                .map(|(_, data)| Cow::Borrowed(data.as_slice()))
        }

        fn paths(&self) -> Vec<String> {
            self.0.keys().cloned().collect()
        }
    }

    fn prim(kind: u32, count: u32) -> Prim {
        Prim { kind, count, material: 0 }
    }

    #[test]
    fn packets_follow_the_vu_batches() {
        // 57 corners of 3 quadwords fill one batch of a fixed model: 2 header + 1 end + 4 + 171
        assert_eq!(packet(&prim(3, 57), None), 178);
        assert_eq!(packet(&prim(3, 58), None), 185);
        // a strip repeats two corners in the next batch: 57, then 60 + 2 - 57 = 5
        assert_eq!(packet(&prim(4, 60), None), 3 + (4 + 171) + (4 + 15));
        // with colours a corner takes 4 quadwords and a batch 42 corners
        assert_eq!(packet(&prim(3 | F_COLOUR, 42), None), 3 + 4 + 168);
        // skinned: two weight quadwords a corner; 32 bones leave vu1_offset 116, 21 corners a batch
        assert_eq!(packet(&prim(3, 21), Some(32)), 3 + 4 + 21 * 5);
        assert_eq!(packet(&prim(3, 0), None), 3);
    }

    #[test]
    fn stacks_count_as_mgcmemory_does() {
        let mut stack = Stack::default();
        stack.alloc(3);
        stack.alloc64(2);
        assert_eq!(stack.used, 6);
        stack.align64();
        assert_eq!(stack.bytes(), 8 * 16);
    }

    #[test]
    fn a_model_is_counted_from_its_mds() {
        let mut mds = mdsfile::Mds::new();
        mds.nodes.push(mdsfile::Node::new("root", -1));
        let mut mesh = mdsfile::Node::new("box--x", 0);
        mesh.mdt = Some(mdsfile::Mdt {
            vertices: vec![[0.0, 0.0, 0.0, 1.0]; 3],
            normals: vec![[0.0, 1.0, 0.0, 1.0]],
            uvs: vec![[0.0, 0.0, 1.0, 0.0]],
            materials: vec![mdsfile::Material::new("wood")],
            prims: vec![mdsfile::Prim {
                kind: 3,
                material: 0,
                verts: vec![vec![0, 0, 0], vec![1, 0, 0], vec![2, 0, 0]],
            }],
            ..Default::default()
        });
        mds.nodes.push(mesh);
        let data = mdsfile::build(&mds).unwrap();
        let mut stack = Stack::default();
        model(&mut stack, &data, &|_| Visual::Fixed, None).unwrap();
        // frames 2*17+2, table 1+2; root: name 1, attr 11; box: name 1, attr 11, bound 11,
        // visual 7 + 1, material 3, face 3, group 4, packet 3 + 4 + 9
        let expected = 36 + 3 + (1 + 11) + (1 + 11 + 11 + 8 + 3 + 3 + 4 + 16);
        assert_eq!(stack.used, expected);
        let mut stack = Stack::default();
        collision(&mut stack, &data).unwrap();
        // 2 CColFrames 2*18+2, the mesh's CCollisionMDT 7, one triangle 5, bound 13
        assert_eq!(stack.used, 38 + 7 + 5 + 13);
    }

    #[test]
    fn motions_convert_key_lists() {
        // two lists of 4 and 0 keys: each Mot_List 3, values keys+1, key frames keys/4+1
        let mut mot = Vec::new();
        for (keys, more) in [(4u32, 1u32), (0, 0)] {
            let mut header = [0u8; 32];
            header[16..20].copy_from_slice(&keys.to_le_bytes());
            header[20..24].copy_from_slice(&more.to_le_bytes());
            mot.extend(header);
            mot.extend(vec![0u8; 32 * keys as usize]);
        }
        let mut stack = Stack::default();
        key_list_memory(&mut stack, &mot);
        assert_eq!(stack.used, (3 + 5 + 2) + (3 + 1 + 1));
    }

    /// A TIM2 picture header as EnterTexture reads it.
    fn tim2(kind: u8, width: u16, height: u16, mips: u8) -> Vec<u8> {
        let mut data = vec![0u8; 0x30];
        data[0x21] = mips;
        data[0x23] = kind;
        data[0x24..0x26].copy_from_slice(&width.to_le_bytes());
        data[0x26..0x28].copy_from_slice(&height.to_le_bytes());
        data
    }

    /// An IM3 file of pictures (name, group, picture).
    fn im3(pictures: &[(&str, i32, Vec<u8>)]) -> Vec<u8> {
        let mut out = b"IM3\0".to_vec();
        out.extend(0u32.to_le_bytes());
        out.extend((pictures.len() as u32).to_le_bytes());
        out.extend(0u32.to_le_bytes());
        let mut body: Vec<u8> = Vec::new();
        let start = 0x10 + 0x40 * pictures.len();
        for (name, group, picture) in pictures {
            let mut entry = vec![0u8; 0x40];
            entry[..name.len()].copy_from_slice(name.as_bytes());
            entry[0x24..0x28].copy_from_slice(&((start + body.len()) as i32).to_le_bytes());
            entry[0x2C..0x30].copy_from_slice(&group.to_le_bytes());
            out.extend(entry);
            body.extend(picture);
        }
        out.extend(body);
        out
    }

    #[test]
    fn pictures_fill_texture_blocks() {
        // 256x256 at 32 bits is 1024 GS blocks; four fit the 4768 of a block, the fifth spills
        let big = tim2(3, 256, 256, 1);
        let five: Vec<(&str, i32, Vec<u8>)> = (0..5).map(|_| ("p", 0, big.clone())).collect();
        let used = shared_block(&[&im3(&five)]);
        assert_eq!(used, BlockUse { blocks: 2, spills: 1 });
        // a group starts its own block, and spilling moves the later groups on
        let grouped = im3(&[("a", 0, big.clone()), ("b", 1, big.clone())]);
        assert_eq!(shared_block(&[&grouped]).blocks, 2);
        // unswizzled 8-bit pictures go into the Z buffer first (256x256: 256 blocks, 1024 of it)
        let small = tim2(5, 256, 256, 1);
        let mut blocks = Blocks::new();
        enter_img(&mut blocks, &im3(&[("z", 0, small)]), 0, TOWN_BLOCKS);
        assert_eq!(blocks.remain(0), BLOCK_VRAM - CLUT_BLOCKS);
        // a mip chain counts each level a quarter of the last (64 + 16 + 4 blocks), in whole pages
        let chain = texture(&tim2(3, 64, 64, 3), 0, false, false).unwrap();
        assert_eq!(chain.vram, 96);
        assert_eq!(chain.vram % PAGE_BLOCKS, 0);
    }

    /// An IMG (or IM2) file of pictures (name, picture).
    fn img(magic: &[u8; 3], pictures: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut out = magic.to_vec();
        out.push(0);
        out.extend((pictures.len() as u32).to_le_bytes());
        out.extend([0u8; 8]);
        let mut body: Vec<u8> = Vec::new();
        let start = 0x10 + 0x30 * pictures.len();
        for (name, picture) in pictures {
            let mut entry = vec![0u8; 0x30];
            entry[..name.len()].copy_from_slice(name.as_bytes());
            entry[0x20..0x24].copy_from_slice(&((start + body.len()) as i32).to_le_bytes());
            out.extend(entry);
            body.extend(picture);
        }
        out.extend(body);
        out
    }

    #[test]
    fn an_img_file_spills_but_enters_as_one_block() {
        // five 1024-block pictures: the fifth goes on to block 1, yet EnterIMGFile returns 0 for an
        // IMG file (its running block and its last block move together), so the next file is
        // entered at block 1, beside the spilled picture
        let big = tim2(3, 256, 256, 1);
        let five: Vec<(&str, Vec<u8>)> = (0..5).map(|_| ("p", big.clone())).collect();
        for magic in [b"IMG", b"IM2"] {
            let mut blocks = Blocks::new();
            assert_eq!(enter_img(&mut blocks, &img(magic, &five), 0, TOWN_BLOCKS), 0);
            assert_eq!((blocks.used(), blocks.spills), (1, 1));
            // the spilled picture stays in block 0's list, as the engine leaves it
            assert_eq!(blocks.remain(0), BLOCK_VRAM - 5 * 1024);
        }
        // an IM3 file returns the blocks it went past
        let three: Vec<(&str, i32, Vec<u8>)> = (0..5).map(|_| ("p", 0, big.clone())).collect();
        assert_eq!(enter_img(&mut Blocks::new(), &im3(&three), 0, TOWN_BLOCKS), 1);
    }

    #[test]
    fn map_entries_take_the_first_listing_of_a_map() {
        let row =
            |code: &str, kind: i64, number: usize| format!("MAP_NAME \"{code}\",\"\",\"\",{kind},1;// {number}\r\n");
        let mut old = String::from("MAP_NAME_NUM 16;\r\n");
        for number in 0..11 {
            old += &row("", 0, number);
        }
        old += &row("m02", 3, 11);
        for number in 12..15 {
            old += &row("", 0, number);
        }
        old += &row("s19", 3, 15);
        let game = format!(
            "MAP_NAME_NUM 3;\r\n{}{}// {}{}",
            row("s19", 8, 0),
            row("m02", 3, 1),
            row("x01", 7, 2),
            row("s51", 7, 2)
        );
        let disc = Files(BTreeMap::from([
            ("map/map.cfg".to_string(), old.into_bytes()),
            ("map/map1.cfg".to_string(), game.into_bytes()),
        ]));
        let kinds: Vec<(String, MapKind)> =
            map_entries(&disc).into_iter().map(|entry| (entry.name, entry.kind)).collect();
        assert_eq!(
            kinds,
            [
                ("m02".to_string(), MapKind::Interior),
                ("s19".to_string(), MapKind::Town),
                ("s51".to_string(), MapKind::Dungeon)
            ]
        );
    }

    #[test]
    fn party_packs_find_their_stacks() {
        assert_eq!(party_stack("mainchr/c01_base3.chr"), Some((0, 68500, "dungeon")));
        assert_eq!(party_stack("chara/c02_base.chr"), Some((0, 75000, "town")));
        assert_eq!(party_stack("mainchr/ws012.chr"), Some((2, 7000, "dungeon")));
        assert_eq!(party_stack("mainchr/ha001.chr"), Some((4, 3000, "dungeon")));
        assert_eq!(party_stack("chara/c02a.chr"), None);
        assert_eq!(SCRIPT_FILE, 312_816);
    }

    #[test]
    fn whole_files_and_scripts_are_checked() {
        let original = Files(BTreeMap::from([
            ("map/s/s17/s17_1.stb".to_string(), vec![0u8; 1000]),
            ("chara/c02a.chr".to_string(), vec![0u8; 1000]),
            ("menu/a.img".to_string(), vec![0u8; 10]),
        ]));
        let modded = BTreeMap::from([
            ("map/s/s17/s17_1.stb".to_string(), vec![0u8; 400_000]),
            ("chara/c02a.chr".to_string(), vec![0u8; 3_300_000]),
            ("menu/a.img".to_string(), vec![0u8; 20]),
        ]);
        let overlay = Overlay::new(&original, &modded);
        let changed: Vec<String> = modded.keys().cloned().collect();
        let found = check(&original, &overlay, &changed);
        let errors: Vec<&Finding> = found.iter().filter(|item| item.level == Level::Error).collect();
        assert_eq!(errors.len(), 2, "{found:?}");
        assert!(errors.iter().any(|item| item.text.contains("script buffer")));
        assert!(errors.iter().any(|item| item.text.contains("read buffer")));
        assert!(found.iter().any(|item| item.level == Level::Note && item.text.starts_with("menu/a.img")));
        // nothing changed, nothing to say
        assert!(check(&original, &original, &[]).is_empty());
    }

    fn bank(samples: usize) -> Vec<u8> {
        dc2tk_assets::models::write_pack(&[("x.hd".to_string(), vec![0; 32]), ("x.bd".to_string(), vec![0; samples])])
            .unwrap()
    }

    #[test]
    fn sound_banks_are_checked_against_their_regions() {
        let index =
            "800\tx\tEV\ty\t-\tBS_051\tFG_000\tSR_051\tSR_051_000\tOB_001\t-\tOB_001\t*\tOB_009\t-\tEV_050_100\n\
                     801\tx\tEV\ty\t-\t*\t*\t*\t*\t*\t*\t*\t*\t*\t-\t-\n";
        let sets = sound_sets(index.as_bytes());
        assert_eq!(sets[0], (800, vec!["BS_051".to_string(), "FG_000".into(), "SR_051".into(), "OB_001".into()]));
        assert_eq!(sets[1], (801, Vec::<String>::new()));
        let original = Files(BTreeMap::from([
            ("snd2/MAP_index.txt".to_string(), index.as_bytes().to_vec()),
            ("snd2/bs/BS_051.snd".to_string(), bank(0x8_0000)),
            ("snd2/fg/FG_000.snd".to_string(), bank(0x8_0000)),
            ("snd2/SY_000.snd".to_string(), bank(0x1_0000)),
        ]));
        let modded = BTreeMap::from([
            ("snd2/fg/FG_000.snd".to_string(), bank(0x9_0000)),
            ("snd2/SY_000.snd".to_string(), bank(0x6_DD80)),
        ]);
        let overlay = Overlay::new(&original, &modded);
        let found = check(&original, &overlay, &modded.keys().cloned().collect::<Vec<_>>());
        let errors: Vec<&str> =
            found.iter().filter(|item| item.level == Level::Error).map(|item| item.text.as_str()).collect();
        // FG_000 with BS_051 is 0x110020 together; SY_000 outgrows its region and the IOP window
        assert!(errors.iter().any(|text| text.contains("sound set 800")), "{found:?}");
        assert!(errors.iter().any(|text| text.contains("system sound region")), "{found:?}");
        assert!(errors.iter().any(|text| text.contains("IOP")), "{found:?}");
        assert!(
            found.iter().any(|item| item.level == Level::Warning && item.text.starts_with("snd2/fg/FG_000.snd")),
            "{found:#?}"
        );
    }

    #[test]
    fn map_files_name_their_map() {
        assert_eq!(map_of("map/s/s17/s17.mpk").as_deref(), Some("s17"));
        assert_eq!(map_of("map/d/d01/f01/d01f01.ipk").as_deref(), Some("d01f01"));
        assert_eq!(map_of("map/cmn/g00/g00.mpk"), None);
        assert_eq!(map_of("map/s/s17/s17_1.stb"), None);
        assert_eq!(add_folder("g00/g00"), ("map/cmn/g00".to_string(), "g00".to_string()));
        assert_eq!(n(3_200_000), "3,200,000");
    }

    /// The disc's own loads stay within every budget, and the largest estimated uses are the
    /// RETAIL_ constants (when one fails, the budget or the estimate is wrong). Set C2S_CORPUS to
    /// an extracted disc (`extracted/disc`).
    #[test]
    fn corpus_retail_fits_the_budgets() {
        let Some(corpus) = std::env::var_os("C2S_CORPUS") else { return };
        let disc = Folder::open(Path::new(&corpus)).unwrap();
        let mut wrong = Vec::new();
        let mut largest: BTreeMap<MapKind, (u64, String)> = BTreeMap::new();
        let entries = map_entries(&disc);
        assert!(entries.len() > 180, "{} maps listed", entries.len());
        for entry in &entries {
            let Some(load) = map_load(&disc, entry) else { continue };
            let blocks = map_blocks(&disc, entry).unwrap();
            let name = &entry.name;
            if load.read > entry.kind.loading_screen() {
                wrong.push(format!("{name}: read {}", load.read));
            }
            if load.placements > entry.kind.placements() {
                wrong.push(format!("{name}: {} placements", load.placements));
            }
            let limit = match entry.kind {
                MapKind::Dungeon => DUNGEON_EFFECT_BLOCK,
                MapKind::Interior => TOWN_EFFECT_BLOCK - TOWN_INTERIOR_BLOCK,
                MapKind::Town => TOWN_EFFECT_BLOCK,
            };
            if blocks.blocks > limit {
                wrong.push(format!("{name}: {} texture blocks", blocks.blocks));
            }
            let slot = largest.entry(entry.kind).or_default();
            if load.stack > slot.0 {
                *slot = (load.stack, name.clone());
            }
        }
        assert_eq!(largest[&MapKind::Town].0, RETAIL_TOWN_MAP, "{largest:?}");
        assert_eq!(largest[&MapKind::Interior].0, RETAIL_INTERIOR_MAP, "{largest:?}");
        assert_eq!(largest[&MapKind::Dungeon].0, RETAIL_DUNGEON_MAP, "{largest:?}");
        // every sound set's field banks fit together, and every bank its region
        let index = disc.read("snd2/MAP_index.txt").unwrap();
        let paths = disc.paths();
        let mut samples: HashMap<String, u64> = HashMap::new();
        for path in paths
            .iter()
            .filter(|path| path.to_lowercase().starts_with("snd2/") && path.to_lowercase().ends_with(".snd"))
        {
            let size = bank_samples(&disc.read(path).unwrap()).unwrap();
            let name = path.rsplit('/').next().unwrap().to_uppercase().replace(".SND", "");
            let budget = match &name[..2] {
                "BG" => SPU_MUSIC,
                "SY" => SPU_SYSTEM,
                "SP" => SPU_MENU,
                "CH" => SPU_CHARACTER,
                _ => SPU_FIELD,
            };
            if size + 16 > budget || (IOP_STAGING + 1..=IOP_STAGING + 0x100).contains(&size) || size > 2 * IOP_STAGING {
                wrong.push(format!("{path}: {size} bytes of samples"));
            }
            samples.insert(name, size);
        }
        let sets = sound_sets(&index);
        assert!(sets.len() > 350, "{} sound sets", sets.len());
        for (set, banks) in sets {
            let total: u64 = banks.iter().map(|bank| samples.get(bank).map_or(0, |size| size + 16)).sum();
            if total > SPU_FIELD {
                wrong.push(format!("sound set {set}: {total}"));
            }
        }
        for path in disc.paths() {
            let lower = path.to_lowercase();
            if lower.ends_with(".stb") && lower.starts_with("map/") {
                let size = disc.read(&path).unwrap().len() as u64;
                if size > SCRIPT_FILE {
                    wrong.push(format!("{path}: script {size}"));
                }
            }
            if lower.ends_with(".sky") {
                for (name, img) in img_members(&disc.read(&path).unwrap()) {
                    if shared_block(&[img]).blocks > 1 {
                        wrong.push(format!("{path}: {name} spills"));
                    }
                }
            }
            if !lower.ends_with(".chr") {
                continue;
            }
            let data = disc.read(&path).unwrap();
            // e612_gri_kao.chr (6.2 MB) is never loaded: no script names it
            if data.len() as u64 > READ_BUFFER && !lower.ends_with("e612_gri_kao.chr") {
                wrong.push(format!("{path}: {} bytes", data.len()));
            }
            let limits = skinned_limits(&data);
            if !limits.is_empty() {
                wrong.push(format!("{path}: {limits:?}"));
            }
            // the packs a party slot loads with LoadPack (LoadSkin's costumes and shoes are only
            // estimated the same way); Max loads c01_base2 to 4 (his sword's model number + 2,
            // GetMainCharaModelName), so c01_base0 and 1 are left out
            let Some((stack, quadwords, _)) = party_stack(&path) else { continue };
            let skin = [1, 5].contains(&stack);
            let unused = lower.ends_with("c01_base0.chr") || lower.ends_with("c01_base1.chr");
            let estimate = chara_pack_bytes(&data).unwrap();
            if !skin && !unused && estimate > quadwords * QW {
                wrong.push(format!("{path}: estimated {estimate} of {}", quadwords * QW));
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }
}
