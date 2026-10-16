//! MDS models, read for what building them takes of a memory stack.

use super::*;

pub(super) struct Prim {
    pub(super) kind: u32,
    pub(super) count: u32,
    pub(super) material: u32,
}

pub(super) struct Mesh {
    pub(super) vertices: u64,
    pub(super) normals: u64,
    colours: u64,
    pub(super) uvs: u64,
    pub(super) materials: u64,
    pub(super) prims: Vec<Prim>,
}

pub(super) struct Object {
    pub(super) name: Vec<u8>,
    parent: i32,
    pub(super) mesh: Option<Mesh>,
}

/// Indices per corner of a primitive of this type: the vertex, a normal, a uv, a colour
/// (CreateFace).
fn stride(kind: u32) -> u64 {
    mdsfile::Prim::stride_of(kind) as u64
}

/// The frames of an MDS file and the counts of each mesh (MDS_HEADER, MDTOBJ_HEADER records of
/// 0x70 bytes, MDT_HEADER, the faces section: a count at +8 and FACES_ID records from +0x10).
pub(super) fn objects(mds: &[u8]) -> Option<Vec<Object>> {
    if mds.get(..4)? != b"MDS\0" {
        return None;
    }
    let count = u32_at(mds, 8).ok()? as usize;
    let mut at = u32_at(mds, 12).ok()? as usize;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let size = u32_at(mds, at + 4).ok()? as usize;
        let raw = mds.get(at + 8..at + 40)?;
        let name = raw[..raw.iter().position(|&b| b == 0).unwrap_or(raw.len())].to_vec();
        let mdt = i32_at(mds, at + 40).ok()?;
        let parent = i32_at(mds, at + 44).ok()?;
        let mesh = if mdt > 0 { Some(mesh(mds, mdt as usize)?) } else { None };
        out.push(Object { name, parent, mesh });
        at += size.max(0x70);
    }
    Some(out)
}

pub(super) fn mesh(mds: &[u8], at: usize) -> Option<Mesh> {
    let field = |offset: usize| i32_at(mds, at + offset).ok().map(|value| value.max(0) as u64);
    let faces = at + field(40)? as usize;
    let prim_count = u32_at(mds, faces + 8).ok()?;
    let mut prims = Vec::new();
    let mut cursor = faces + 0x10;
    for _ in 0..prim_count {
        let kind = u32_at(mds, cursor).ok()? & 0xFFFF;
        let count = u32_at(mds, cursor + 4).ok()? & 0xFFFF;
        let material = u32_at(mds, cursor + 8).ok()?;
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
pub(super) enum Visual {
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
pub(super) fn batch(vu1_offset: u64, per_corner: u64) -> u64 {
    (vu1_offset - 2) / per_corner / 3 * 3
}

/// Quadwords of the packet CreateFacePacket (mg_visual) or CreateFaceMotionPacket (visualmotion)
/// builds for one primitive: two header quadwords, an end tag, and per batch two tags, the
/// unpack header, the corners and the program call; a strip repeats two corners per batch.
pub(super) fn packet(prim: &Prim, skinned_bones: Option<u64>) -> u64 {
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
pub(super) fn bones_by_frame(weight: &[u8]) -> HashMap<u32, Vec<u32>> {
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
        (u32_at(data, at).ok(), u32_at(data, at + 4).ok(), u32_at(data, at + 16).ok(), u32_at(data, at + 20).ok())
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
pub(super) fn model(
    stack: &mut Stack,
    mds: &[u8],
    visual_of: &dyn Fn(&[u8]) -> Visual,
    weight: Option<&[u8]>,
) -> Option<()> {
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
pub(super) fn collision(stack: &mut Stack, mds: &[u8]) -> Option<()> {
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
pub(super) fn key_list_memory(stack: &mut Stack, data: &[u8]) {
    for (_, _, keys) in key_lists(data) {
        stack.alloc(0x20 / 16 + 1);
        stack.alloc(keys + 1);
        stack.alloc(keys * 4 / 16 + 1);
    }
}
