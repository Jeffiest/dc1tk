//! MDS model files: a frame hierarchy of 0x70-byte records (name, parent, local matrix), and an
//! MDT mesh block for the frames that have one (vertices, normals, uvs, colours, triangle lists
//! and strips, materials). Parsed and written back byte for byte.

use dc2tk_disc::text;

use crate::{bail, py, Result};

pub const PRIM_TRIANGLES: u32 = 3;
pub const PRIM_STRIP: u32 = 4;
pub const F_NO_UV: u32 = 0x10;
pub const F_COLOUR: u32 = 0x100;
pub const F_NO_NORMAL: u32 = 0x200;

pub const IDENT: [f64; 16] = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

pub(crate) fn i32_at(data: &[u8], at: usize) -> Result<i32> {
    match data.get(at..at + 4) {
        Some(b) => Ok(i32::from_le_bytes(b.try_into().unwrap())),
        None => bail!("unpack_from requires a buffer of at least {} bytes", at + 4),
    }
}

pub(crate) fn u32_at(data: &[u8], at: usize) -> Result<u32> {
    Ok(i32_at(data, at)? as u32)
}

/// struct.unpack_from('<f'): a float32 as a double, a signalling NaN made quiet as the CPU does.
pub(crate) fn f32_at(data: &[u8], at: usize) -> Result<f64> {
    Ok(py::unpack_f32(u32_at(data, at)?))
}

pub(crate) fn floats_at<const N: usize>(data: &[u8], at: usize) -> Result<[f64; N]> {
    let mut out = [0.0; N];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = f32_at(data, at + 4 * i)?;
    }
    Ok(out)
}

/// struct.pack('<Nf', *values).
pub(crate) fn pack_floats(values: &[f64], out: &mut Vec<u8>) -> Result<()> {
    for &v in values {
        out.extend(py::pack_f32(v)?);
    }
    Ok(())
}

/// A NUL-padded name field: the text, and the raw bytes when they would not come back the same
/// (bytes after the NUL, or bad Shift-JIS).
fn name_in(raw: &[u8]) -> (String, Option<Vec<u8>>) {
    let nul = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
    match text::decode_strict(&raw[..nul]) {
        Some(name) => {
            let same = text::encode(&name).is_some_and(|mut back| {
                back.resize(back.len().max(raw.len()), 0);
                back == raw
            });
            if same {
                (name, None)
            } else {
                (name, Some(raw.to_vec()))
            }
        }
        None => (text::decode_lossy(&raw[..nul]), Some(raw.to_vec())),
    }
}

fn name_out(name: &str, raw: &Option<Vec<u8>>, size: usize) -> Result<Vec<u8>> {
    if let Some(raw) = raw {
        return Ok(raw.clone());
    }
    let Some(mut bytes) = text::encode(name) else {
        bail!("'cp932' codec can't encode character in {}", crate::im3::str_repr(name));
    };
    if bytes.len() >= size {
        bail!("name too long ({} bytes, max {}): {}", bytes.len(), size - 1, crate::im3::str_repr(name));
    }
    bytes.resize(size, 0);
    Ok(bytes)
}

/// MDT_MATERIAL_ (0x60 bytes). The colours are lists, as the Python packs whatever it is given.
#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    pub diffuse: Vec<f64>,
    pub ambient: Vec<f64>,
    pub specular: Vec<f64>,
    pub power: f64,
    pub texture: String,
    pub texture_raw: Option<Vec<u8>>,
    pub unk_54: i64,
    pub unk_58: f64,
    pub unk_5c: f64,
}

impl Material {
    pub fn new(texture: &str) -> Material {
        Material {
            diffuse: vec![1.0, 1.0, 1.0, 1.0],
            ambient: vec![1.0, 1.0, 1.0, 0.0],
            specular: vec![0.0; 4],
            power: 50.0,
            texture: texture.to_string(),
            texture_raw: None,
            unk_54: 0,
            unk_58: 0.0,
            unk_5c: 0.0,
        }
    }

    fn unpack(b: &[u8], o: usize) -> Result<Material> {
        let f: [f64; 13] = floats_at(b, o)?;
        let Some(raw) = b.get(o + 0x34..o + 0x54) else {
            bail!("unpack_from requires a buffer of at least {} bytes", o + 0x60);
        };
        let (texture, texture_raw) = name_in(raw);
        Ok(Material {
            diffuse: f[0..4].to_vec(),
            ambient: f[4..8].to_vec(),
            specular: f[8..12].to_vec(),
            power: f[12],
            texture,
            texture_raw,
            unk_54: i32_at(b, o + 0x54)? as i64,
            unk_58: f32_at(b, o + 0x58)?,
            unk_5c: f32_at(b, o + 0x5C)?,
        })
    }

    fn pack(&self) -> Result<Vec<u8>> {
        let floats: Vec<f64> =
            self.diffuse.iter().chain(&self.ambient).chain(&self.specular).copied().chain([self.power]).collect();
        if floats.len() != 13 {
            bail!("pack expected 13 items for packing (got {})", floats.len());
        }
        let mut out = Vec::with_capacity(0x60);
        pack_floats(&floats, &mut out)?;
        out.extend(name_out(&self.texture, &self.texture_raw, 32)?);
        let Ok(unk_54) = i32::try_from(self.unk_54) else {
            bail!("'i' format requires -2147483648 <= number <= 2147483647");
        };
        out.extend(unk_54.to_le_bytes());
        pack_floats(&[self.unk_58, self.unk_5c], &mut out)?;
        Ok(out)
    }
}

/// FACES_ID: one triangle list or strip drawn with one material; each vertex a tuple of (vertex,
/// [normal], [uv], [colour]) indices, which of them present depending on the type.
#[derive(Clone, Debug, PartialEq)]
pub struct Prim {
    pub kind: u32,
    pub material: u32,
    pub verts: Vec<Vec<i32>>,
}

/// Which index of a prim vertex.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Vertex,
    Normal,
    Uv,
    Colour,
}

impl Prim {
    pub fn stride_of(kind: u32) -> usize {
        (3 + (kind & F_COLOUR != 0) as i64 - (kind & F_NO_NORMAL != 0) as i64 - (kind & F_NO_UV != 0) as i64) as usize
    }

    pub fn stride(&self) -> usize {
        Prim::stride_of(self.kind)
    }

    /// Position of a slot inside each vertex tuple, or None.
    pub fn slot(&self, what: Slot) -> Option<usize> {
        let mut order = vec![Slot::Vertex];
        if self.kind & F_NO_NORMAL == 0 {
            order.push(Slot::Normal);
        }
        if self.kind & F_NO_UV == 0 {
            order.push(Slot::Uv);
        }
        if self.kind & F_COLOUR != 0 {
            order.push(Slot::Colour);
        }
        order.iter().position(|&s| s == what)
    }

    /// Corner triples (indices into verts) in strip-corrected winding.
    pub fn triangles(&self) -> Vec<[usize; 3]> {
        let n = self.verts.len();
        if self.kind & 7 == PRIM_TRIANGLES {
            return (0..n.saturating_sub(2)).step_by(3).map(|i| [i, i + 1, i + 2]).collect();
        }
        (0..n.saturating_sub(2)).map(|i| if i % 2 == 0 { [i, i + 1, i + 2] } else { [i + 1, i, i + 2] }).collect()
    }
}

/// The section offsets, size and bytes of an MDT block neither writer's layout gives.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub offsets: [i32; 6],
    pub size: i32,
    pub raw: Vec<u8>,
}

/// One mesh (MDT block).
#[derive(Clone, Debug, PartialEq)]
pub struct Mdt {
    pub vertices: Vec<[f64; 4]>,
    pub normals: Vec<[f64; 4]>,
    pub colours: Vec<[f64; 4]>,
    pub uvs: Vec<[f64; 4]>,
    pub prims: Vec<Prim>,
    pub materials: Vec<Material>,
    pub faces_unk00: u32,
    pub unk_3c: i32,
    /// 'A' (16 zero bytes between some sections) or 'B' (packed, colour offset -1).
    pub style: char,
    pub faces_pad: Option<Vec<u8>>,
    pub layout: Option<Layout>,
}

fn vecs(b: &[u8], at: i64, n: i32) -> Result<Vec<[f64; 4]>> {
    if n < 0 {
        return Ok(Vec::new());
    }
    if at < 0 {
        bail!("offset {at} out of range");
    }
    (0..n as usize).map(|i| floats_at(b, at as usize + 16 * i)).collect()
}

impl Default for Mdt {
    fn default() -> Mdt {
        Mdt {
            vertices: Vec::new(),
            normals: Vec::new(),
            colours: Vec::new(),
            uvs: Vec::new(),
            prims: Vec::new(),
            materials: Vec::new(),
            faces_unk00: 0,
            unk_3c: 0,
            style: 'A',
            faces_pad: None,
            layout: None,
        }
    }
}

impl Mdt {
    pub fn unpack(b: &[u8], m: usize) -> Result<Mdt> {
        let mut h = [0i32; 15];
        for (i, slot) in h.iter_mut().enumerate() {
            *slot = i32_at(b, m + 4 + 4 * i)?;
        }
        let [hsize, size, vn, vo, nn, no, cn, co, fsz, fo, un, uo, mn, mo, unk_3c] = h;
        if &b[m..m + 4] != b"MDT\0" || hsize != 0x40 {
            bail!("bad MDT at 0x{m:x}");
        }
        let at = |offset: i32| m as i64 + offset as i64;
        let mut mdt = Mdt {
            vertices: vecs(b, at(vo), vn)?,
            normals: vecs(b, at(no), nn)?,
            colours: if cn != 0 { vecs(b, at(co), cn)? } else { Vec::new() },
            uvs: vecs(b, at(uo), un)?,
            unk_3c,
            ..Mdt::default()
        };
        for i in 0..mn.max(0) as i64 {
            let o = at(mo) + 0x60 * i;
            if o < 0 {
                bail!("offset out of range");
            }
            mdt.materials.push(Material::unpack(b, o as usize)?);
        }
        if at(fo) < 0 {
            bail!("offset out of range");
        }
        let f = at(fo) as usize;
        mdt.faces_unk00 = u32_at(b, f)?;
        let (fh, prim_num, f3) = (u32_at(b, f + 4)?, u32_at(b, f + 8)?, u32_at(b, f + 12)?);
        if fh != 0x10 || f3 != 0 {
            bail!("bad faces header");
        }
        let mut q = f + fh as usize;
        for _ in 0..prim_num {
            let (kind, count, material) = (u32_at(b, q)?, u32_at(b, q + 4)? as usize, u32_at(b, q + 8)?);
            let stride = Prim::stride_of(kind);
            let flat: Vec<i32> = (0..count * stride).map(|i| i32_at(b, q + 12 + 4 * i)).collect::<Result<_>>()?;
            q += 12 + 4 * count * stride;
            let verts = if stride == 0 { Vec::new() } else { flat.chunks(stride).map(<[i32]>::to_vec).collect() };
            mdt.prims.push(Prim { kind, material, verts });
        }
        if (q - f) as i64 != fsz as i64 {
            bail!("faces size mismatch");
        }
        mdt.style = if cn == 0 && co == -1 { 'B' } else { 'A' };
        if no > fo {
            let end = at(no).max(0) as usize;
            let pad = &b[q.min(b.len())..end.clamp(q.min(b.len()), b.len())];
            if pad.iter().any(|&x| x != 0) {
                mdt.faces_pad = Some(pad.to_vec());
            }
        }
        let original = &b[m.min(b.len())..(m as i64 + size as i64).clamp(m as i64, b.len() as i64) as usize];
        if mdt.pack()? != original {
            // the exact offsets and gap bytes, so the block is rewritten unchanged
            mdt.layout = Some(Layout { offsets: [vo, no, co, fo, uo, mo], size, raw: original.to_vec() });
        }
        Ok(mdt)
    }

    fn faces_bytes(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        out.extend(self.faces_unk00.to_le_bytes());
        out.extend(0x10u32.to_le_bytes());
        out.extend((self.prims.len() as u32).to_le_bytes());
        out.extend(0u32.to_le_bytes());
        for p in &self.prims {
            let stride = p.stride();
            if p.verts.iter().any(|v| v.len() != stride) {
                bail!("prim vertex tuple does not match its type");
            }
            out.extend(p.kind.to_le_bytes());
            out.extend((p.verts.len() as u32).to_le_bytes());
            out.extend(p.material.to_le_bytes());
            for v in &p.verts {
                for i in v {
                    out.extend(i.to_le_bytes());
                }
            }
        }
        Ok(out)
    }

    fn pad(&self, n: usize) -> Vec<u8> {
        match &self.faces_pad {
            Some(pad) if pad.len() == n => pad.clone(),
            _ => vec![0; n],
        }
    }

    pub fn pack(&self) -> Result<Vec<u8>> {
        let vec = |vs: &[[f64; 4]]| -> Result<Vec<u8>> {
            let mut out = Vec::with_capacity(16 * vs.len());
            for v in vs {
                pack_floats(v, &mut out)?;
            }
            Ok(out)
        };
        let (v, n, c, u) = (vec(&self.vertices)?, vec(&self.normals)?, vec(&self.colours)?, vec(&self.uvs)?);
        let f = self.faces_bytes()?;
        let mut m = Vec::new();
        for material in &self.materials {
            m.extend(material.pack()?);
        }
        let (vo, no, co, fo, uo, mo, size, body);
        if let Some(layout) = &self.layout {
            [vo, no, co, fo, uo, mo] = layout.offsets.map(|o| o as i64);
            let mut buffer = layout.raw.clone();
            for (offset, data) in [(vo, &v), (no, &n), (co, &c), (fo, &f), (uo, &u), (mo, &m)] {
                if !data.is_empty() {
                    assign_at(&mut buffer, offset, data);
                }
            }
            size = layout.size as i64;
            let end = (size.max(0) as usize).clamp(0x40.min(buffer.len()), buffer.len());
            body = buffer[0x40.min(buffer.len())..end].to_vec();
        } else {
            let mut out: Vec<u8> = Vec::new();
            let pos = |out: &Vec<u8>| (0x40 + out.len()) as i64;
            vo = pos(&out);
            out.extend(&v);
            fo = pos(&out);
            out.extend(&f);
            if self.style == 'A' {
                let gap = 16 - (pos(&out) % 16) as usize;
                out.extend(self.pad(gap));
                no = pos(&out);
                out.extend(&n);
                out.extend([0u8; 16]);
                uo = pos(&out);
                out.extend(&u);
                if !c.is_empty() {
                    out.extend([0u8; 16]);
                }
                co = if c.is_empty() { 0 } else { pos(&out) };
                out.extend(&c);
                mo = pos(&out);
                out.extend(&m);
                out.extend([0u8; 16]);
            } else {
                let gap = py::modulo(-pos(&out), 16) as usize;
                out.extend(self.pad(gap));
                no = pos(&out);
                out.extend(&n);
                uo = pos(&out);
                out.extend(&u);
                co = if c.is_empty() { -1 } else { pos(&out) };
                out.extend(&c);
                mo = pos(&out);
                out.extend(&m);
            }
            size = pos(&out);
            body = out;
        }
        let mut out = b"MDT\0".to_vec();
        let counts = [
            self.vertices.len(),
            self.normals.len(),
            self.colours.len(),
            f.len(),
            self.uvs.len(),
            self.materials.len(),
        ];
        let header = [
            0x40,
            size,
            counts[0] as i64,
            vo,
            counts[1] as i64,
            no,
            counts[2] as i64,
            co,
            counts[3] as i64,
            fo,
            counts[4] as i64,
            uo,
            counts[5] as i64,
            mo,
            self.unk_3c as i64,
        ];
        for value in header {
            let Ok(value) = i32::try_from(value) else {
                bail!("'i' format requires -2147483648 <= number <= 2147483647");
            };
            out.extend(value.to_le_bytes());
        }
        out.extend(body);
        Ok(out)
    }
}

/// bytearray slice assignment buffer[offset:offset+len(data)] = data (which can grow the buffer).
fn assign_at(buffer: &mut Vec<u8>, offset: i64, data: &[u8]) {
    let len = buffer.len() as i64;
    let start = if offset < 0 { (len + offset).max(0) } else { offset.min(len) } as usize;
    let end_raw = offset + data.len() as i64;
    let end = if end_raw < 0 { (len + end_raw).max(0) } else { end_raw.min(len) } as usize;
    let end = end.max(start);
    buffer.splice(start..end, data.iter().copied());
}

/// MDTOBJ_HEADER: one frame of the hierarchy (bone, dummy or mesh).
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// The record's index when it is not its position.
    pub index: Option<i32>,
    pub name: String,
    pub name_raw: Option<Vec<u8>>,
    pub parent: i32,
    pub matrix: [f64; 16],
    pub mdt: Option<Mdt>,
}

impl Node {
    pub fn new(name: &str, parent: i32) -> Node {
        Node { index: None, name: name.to_string(), name_raw: None, parent, matrix: IDENT, mdt: None }
    }

    /// The name without the "--" attribute flags.
    pub fn base_name(&self) -> &str {
        self.name.split_once("--").map_or(&self.name, |(base, _)| base)
    }
}

/// An MDS file.
#[derive(Clone, Debug, PartialEq)]
pub struct Mds {
    pub version: u32,
    pub nodes: Vec<Node>,
}

impl Mds {
    pub fn new() -> Mds {
        Mds { version: 1, nodes: Vec::new() }
    }

    /// Local-to-model matrices (column-major) of every node.
    pub fn world_matrices(&self) -> Result<Vec<[f64; 16]>> {
        let mut out: Vec<[f64; 16]> = Vec::with_capacity(self.nodes.len());
        for n in &self.nodes {
            let m = if n.parent >= 0 {
                let Some(parent) = out.get(n.parent as usize) else {
                    bail!("list index out of range");
                };
                mat_mul(parent, &n.matrix)
            } else {
                n.matrix
            };
            out.push(m);
        }
        Ok(out)
    }
}

impl Default for Mds {
    fn default() -> Mds {
        Mds::new()
    }
}

/// a*b for column-major 4x4 matrices, each element summed as Python's sum() does.
pub fn mat_mul(a: &[f64; 16], b: &[f64; 16]) -> [f64; 16] {
    let mut out = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            out[c * 4 + r] = dc2tk_py::sum_floats((0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]));
        }
    }
    out
}

pub fn parse(b: &[u8]) -> Result<Mds> {
    if b.len() < 16 {
        bail!("unpack_from requires a buffer of at least 16 bytes");
    }
    if &b[..4] != b"MDS\0" {
        bail!("not an MDS file");
    }
    let (version, n, ofs) = (u32_at(b, 4)?, u32_at(b, 8)?, u32_at(b, 12)?);
    let mut mds = Mds { version, nodes: Vec::new() };
    let mut o = ofs as usize;
    for i in 0..n as usize {
        let (index, size) = (i32_at(b, o)?, u32_at(b, o + 4)?);
        if size != 0x70 {
            bail!("unexpected record size {size}");
        }
        let Some(raw) = b.get(o + 8..o + 40) else {
            bail!("unpack_from requires a buffer of at least {} bytes", o + 40);
        };
        let (name, name_raw) = name_in(raw);
        let mdt_ofs = i32_at(b, o + 40)?;
        let parent = i32_at(b, o + 44)?;
        let matrix = floats_at(b, o + 48)?;
        let mdt = if mdt_ofs != 0 {
            if mdt_ofs < 0 {
                bail!("offset out of range");
            }
            Some(Mdt::unpack(b, mdt_ofs as usize)?)
        } else {
            None
        };
        mds.nodes.push(Node {
            index: if index as i64 == i as i64 { None } else { Some(index) },
            name,
            name_raw,
            parent,
            matrix,
            mdt,
        });
        o += size as usize;
    }
    Ok(mds)
}

pub fn build(mds: &Mds) -> Result<Vec<u8>> {
    let n = mds.nodes.len();
    let blocks: Vec<Option<Vec<u8>>> =
        mds.nodes.iter().map(|node| node.mdt.as_ref().map(Mdt::pack).transpose()).collect::<Result<_>>()?;
    let mut pos = 0x10 + 0x70 * n;
    let mut out = b"MDS\0".to_vec();
    out.extend(mds.version.to_le_bytes());
    out.extend((n as u32).to_le_bytes());
    out.extend(0x10u32.to_le_bytes());
    for (i, node) in mds.nodes.iter().enumerate() {
        let mut mo = 0;
        if let Some(block) = &blocks[i] {
            mo = pos;
            pos += block.len();
        }
        out.extend(node.index.unwrap_or(i as i32).to_le_bytes());
        out.extend(0x70u32.to_le_bytes());
        out.extend(name_out(&node.name, &node.name_raw, 32)?);
        out.extend((mo as i32).to_le_bytes());
        out.extend(node.parent.to_le_bytes());
        pack_floats(&node.matrix, &mut out)?;
    }
    for block in blocks.into_iter().flatten() {
        out.extend(block);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_model_writes_back_the_same() {
        let mut model = Mds::new();
        model.nodes.push(Node::new("root", -1));
        let mesh = Mdt {
            vertices: vec![[0.0, 0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [1.0, 1.0, 0.0, 1.0]],
            normals: vec![[0.0, 0.0, 1.0, 1.0]],
            uvs: vec![[0.0, 0.0, 1.0, 0.0]; 3],
            materials: vec![Material::new("tex01")],
            prims: vec![Prim {
                kind: PRIM_TRIANGLES,
                material: 0,
                verts: vec![vec![0, 0, 0], vec![1, 0, 1], vec![2, 0, 2]],
            }],
            ..Mdt::default()
        };
        model.nodes.push(Node { mdt: Some(mesh), ..Node::new("body--x", 0) });
        let data = build(&model).unwrap();
        let again = parse(&data).unwrap();
        assert_eq!(again, model);
        assert_eq!(build(&again).unwrap(), data);
        assert_eq!(again.nodes[1].base_name(), "body");
        assert_eq!(again.nodes[1].mdt.as_ref().unwrap().prims[0].triangles(), vec![[0, 1, 2]]);
    }
}
