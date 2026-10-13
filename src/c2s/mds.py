"""Dark Chronicle (Dark Cloud 2) MDS model files: parse, edit and write back.

Standard library only.  Run as a script to check that every .mds under a
directory parses and writes back byte for byte:

    python -I mds.py check  <extracted-dir>
    python -I mds.py dump   <file.mds>

Format summary (see models.md for the evidence):

MDS file (all little-endian, 16-byte aligned in memory)
  0x00 char[4]  "MDS\\0"
  0x04 u32      version, always 1
  0x08 u32      object_num
  0x0C u32      object_ofs, always 0x10
  object_num records of 0x70 bytes (MDTOBJ_HEADER):
    0x00 s32    index of this record
    0x04 u32    size of the record (0x70)
    0x08 char[32] name, NUL padded; text after "--" is attribute flags
    0x28 s32    mdt_ofs: offset of the MDT from the MDS start, 0 = no mesh
    0x2C s32    parent index, -1 = root
    0x30 f32[16] local matrix, column-major (m[3] = translation)
  then the MDT blocks, in record order, back to back; nothing after them.

MDT block (one mesh, offsets relative to the "MDT" magic)
  0x00 char[4] "MDT\\0"     0x04 header size (0x40)   0x08 block size
  0x0C vertex_num  0x10 vertex_ofs     0x14 normal_num  0x18 normal_ofs
  0x1C colour_num  0x20 colour_ofs     0x24 faces_size  0x28 faces_ofs
  0x2C uv_num      0x30 uv_ofs         0x34 material_num 0x38 material_ofs
  0x3C 0
  vertices  f32[4] (x, y, z, 1)
  normals   f32[4] (x, y, z, 1)
  uvs       f32[4] (u, v, 1, 0)
  colours   f32[4] (r, g, b, a) in 0..1
  faces:    {u32 unk(0), u32 0x10, u32 prim_num, u32 0} then prim_num x
            {u32 type, u32 vertex_count, u32 material, s32 index[count*stride]}
            per vertex: vertex index, [normal], [uv], [colour]
            type & 7 = GS PRIM (3 triangle list, 4 triangle strip),
            0x10 no uv, 0x100 has colour, 0x200 no normal
  materials 0x60 each: f32[4] diffuse, f32[4] ambient?, f32[4] (0),
            f32 power?, char[32] texture name, s32 0, f32 0, f32 0
"""

import struct
import sys
import os

__all__ = ['MDS', 'Node', 'MDT', 'Prim', 'Material', 'read', 'parse', 'build']

PRIM_TRIANGLES = 3
PRIM_STRIP = 4
F_NO_UV = 0x10
F_COLOUR = 0x100
F_NO_NORMAL = 0x200
ENC = 'cp932'


def _name_in(raw):
    """Decode a NUL padded fixed field; keep the raw bytes when they would not
    come back the same (non-zero bytes after the NUL, or bad Shift-JIS)."""
    nul = raw.find(b'\0')
    if nul < 0:
        nul = len(raw)
    try:
        s = raw[:nul].decode(ENC)
        if s.encode(ENC).ljust(len(raw), b'\0') == raw:
            return s, None
    except UnicodeDecodeError:
        s = raw[:nul].decode(ENC, 'replace')
    return s, raw


def _name_out(s, raw, size):
    if raw is not None:
        return raw
    b = s.encode(ENC)
    if len(b) >= size:
        raise ValueError('name too long (%d bytes, max %d): %r' % (len(b), size - 1, s))
    return b.ljust(size, b'\0')


def _vecs(b, ofs, n):
    return [struct.unpack_from('<4f', b, ofs + 16 * i) for i in range(n)]


class Material:
    """MDT_MATERIAL_ (0x60 bytes)."""
    __slots__ = ('diffuse', 'ambient', 'specular', 'power', 'texture',
                 'texture_raw', 'unk_54', 'unk_58', 'unk_5c')

    def __init__(self, texture='', diffuse=(1.0, 1.0, 1.0, 1.0)):
        self.diffuse = tuple(diffuse)       # 0x00 drawn colour (engine: mgMaterial.diffuse)
        self.ambient = (1.0, 1.0, 1.0, 0.0)  # 0x10 copied to mgMaterial.unk_10, sent to VU1
        self.specular = (0.0, 0.0, 0.0, 0.0)  # 0x20 not read by the engine; always 0
        self.power = 50.0                    # 0x30 not read by the engine; mostly 50
        self.texture = texture               # 0x34 looked up with GetTexture(name)
        self.texture_raw = None
        self.unk_54 = 0
        self.unk_58 = 0.0
        self.unk_5c = 0.0

    @classmethod
    def unpack(cls, b, o):
        m = cls.__new__(cls)
        f = struct.unpack_from('<13f', b, o)
        m.diffuse, m.ambient, m.specular, m.power = f[0:4], f[4:8], f[8:12], f[12]
        m.texture, m.texture_raw = _name_in(b[o + 0x34:o + 0x54])
        m.unk_54, m.unk_58, m.unk_5c = struct.unpack_from('<iff', b, o + 0x54)
        return m

    def pack(self):
        return (struct.pack('<13f', *self.diffuse, *self.ambient, *self.specular, self.power)
                + _name_out(self.texture, self.texture_raw, 32)
                + struct.pack('<iff', self.unk_54, self.unk_58, self.unk_5c))


class Prim:
    """FACES_ID: one triangle list or strip drawn with one material.

    ``verts`` holds one tuple per strip/list vertex: (vertex, [normal], [uv],
    [colour]) indices, which of them present depending on ``type``."""
    __slots__ = ('type', 'material', 'verts')

    def __init__(self, type, material, verts):
        self.type, self.material, self.verts = type, material, verts

    @staticmethod
    def stride_of(t):
        return 3 + (1 if t & F_COLOUR else 0) - (1 if t & F_NO_NORMAL else 0) - (1 if t & F_NO_UV else 0)

    @property
    def stride(self):
        return self.stride_of(self.type)

    def slot(self, what):
        """Position of 'v', 'n', 'uv' or 'c' inside each vertex tuple, or None."""
        order = ['v']
        if not self.type & F_NO_NORMAL:
            order.append('n')
        if not self.type & F_NO_UV:
            order.append('uv')
        if self.type & F_COLOUR:
            order.append('c')
        return order.index(what) if what in order else None

    def triangles(self):
        """Corner triples (indices into self.verts) in strip-corrected winding."""
        n = len(self.verts)
        if self.type & 7 == PRIM_TRIANGLES:
            return [(i, i + 1, i + 2) for i in range(0, n - 2, 3)]
        out = []
        for i in range(n - 2):
            out.append((i, i + 1, i + 2) if i % 2 == 0 else (i + 1, i, i + 2))
        return out


class MDT:
    """One mesh (MDT block)."""

    def __init__(self):
        self.vertices = []   # (x, y, z, w=1)
        self.normals = []    # (x, y, z, w=1)
        self.colours = []    # (r, g, b, a)
        self.uvs = []        # (u, v, 1, 0)
        self.prims = []
        self.materials = []
        self.faces_unk00 = 0  # garbage in a handful of files
        self.unk_3c = 0
        # Two writers made the corpus' MDT blocks; 'A' (the normal one) puts
        # 16 zero bytes between some sections, 'B' (78 blocks, colour_ofs -1)
        # packs them.  See models.md.
        self.style = 'A'
        self.faces_pad = None  # non-zero padding after the faces (38 style B blocks)
        self.layout = None   # explicit offsets/gap bytes when neither style fits

    # -- reading ---------------------------------------------------------
    @classmethod
    def unpack(cls, b, m):
        self = cls()
        (magic, hsize, size, vn, vo, nn, no, cn, co, fsz, fo, un, uo, mn, mo,
         self.unk_3c) = struct.unpack_from('<4s15i', b, m)
        if magic != b'MDT\0' or hsize != 0x40:
            raise ValueError('bad MDT at 0x%x' % m)
        self.vertices = _vecs(b, m + vo, vn)
        self.normals = _vecs(b, m + no, nn)
        self.colours = _vecs(b, m + co, cn) if cn else []
        self.uvs = _vecs(b, m + uo, un)
        self.materials = [Material.unpack(b, m + mo + 0x60 * i) for i in range(mn)]
        f = m + fo
        self.faces_unk00, fh, prim_num, f3 = struct.unpack_from('<4I', b, f)
        if fh != 0x10 or f3 != 0:
            raise ValueError('bad faces header')
        q = f + fh
        for _ in range(prim_num):
            t, cnt, mat = struct.unpack_from('<3I', b, q)
            st = Prim.stride_of(t)
            idx = struct.unpack_from('<%di' % (cnt * st), b, q + 12)
            q += 12 + 4 * cnt * st
            self.prims.append(Prim(t, mat, [idx[i:i + st] for i in range(0, cnt * st, st)]))
        if q - f != fsz:
            raise ValueError('faces size mismatch')
        self.style = 'B' if (cn == 0 and co == -1) else 'A'
        pad = b[q:m + no] if no > fo else b''
        if any(pad):
            self.faces_pad = bytes(pad)
        if self.pack() != b[m:m + size]:
            # Keep the exact offsets and the gap bytes so that the block can
            # be rewritten unchanged.
            self.layout = {'offsets': (vo, no, co, fo, uo, mo), 'size': size,
                           'raw': bytes(b[m:m + size])}
        return self

    # -- writing ---------------------------------------------------------
    def _faces_bytes(self):
        out = [struct.pack('<4I', self.faces_unk00, 0x10, len(self.prims), 0)]
        for p in self.prims:
            st = p.stride
            flat = [i for v in p.verts for i in v]
            if any(len(v) != st for v in p.verts):
                raise ValueError('prim vertex tuple does not match its type')
            out.append(struct.pack('<3I%di' % len(flat), p.type, len(p.verts), p.material, *flat))
        return b''.join(out)

    def _pad(self, n):
        if self.faces_pad is not None and len(self.faces_pad) == n:
            return self.faces_pad
        return bytes(n)

    def pack(self):
        vec = lambda vs: b''.join(struct.pack('<4f', *v) for v in vs)
        V, N, C, U = vec(self.vertices), vec(self.normals), vec(self.colours), vec(self.uvs)
        F = self._faces_bytes()
        M = b''.join(m.pack() for m in self.materials)
        if self.layout is not None:
            vo, no, co, fo, uo, mo = self.layout['offsets']
            buf = bytearray(self.layout['raw'])
            for ofs, data in ((vo, V), (no, N), (co, C), (fo, F), (uo, U), (mo, M)):
                if data:
                    buf[ofs:ofs + len(data)] = data
            size = self.layout['size']
            body = buf[0x40:size]
        else:
            body = bytearray()
            pos = lambda: 0x40 + len(body)
            vo = pos(); body += V
            fo = pos(); body += F
            if self.style == 'A':
                body += self._pad(16 - pos() % 16)   # 1..16 bytes
                no = pos(); body += N
                body += bytes(16)
                uo = pos(); body += U
                if C:
                    body += bytes(16)
                co = pos() if C else 0
                body += C
                mo = pos(); body += M
                body += bytes(16)
            else:
                body += self._pad(-pos() % 16)       # 0..15 bytes
                no = pos(); body += N
                uo = pos(); body += U
                co = pos() if C else -1
                body += C
                mo = pos(); body += M
            size = pos()
        hdr = struct.pack('<4s15i', b'MDT\0', 0x40, size,
                          len(self.vertices), vo, len(self.normals), no,
                          len(self.colours), co, len(F), fo, len(self.uvs), uo,
                          len(self.materials), mo, self.unk_3c)
        return hdr + bytes(body)


class Node:
    """MDTOBJ_HEADER: one frame of the hierarchy (bone, dummy or mesh)."""
    __slots__ = ('index', 'name', 'name_raw', 'parent', 'matrix', 'mdt')

    def __init__(self, name='', parent=-1, matrix=None, mdt=None):
        self.index = None  # None = write the position in the list
        self.name, self.name_raw = name, None
        self.parent = parent
        self.matrix = tuple(matrix) if matrix else (1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0,
                                                     0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0)
        self.mdt = mdt

    @property
    def base_name(self):
        """Name without the "--" attribute flags (what StrCmp compares)."""
        return self.name.split('--', 1)[0]

    @property
    def flags(self):
        return self.name.split('--', 1)[1] if '--' in self.name else ''


class MDS:
    def __init__(self):
        self.version = 1
        self.nodes = []

    def find(self, name):
        for i, n in enumerate(self.nodes):
            if n.base_name.lower() == name.split('--', 1)[0].lower():
                return i
        return -1

    def world_matrices(self):
        """Local-to-model (column-major 16-tuples) for every node."""
        out = []
        for n in self.nodes:
            m = n.matrix
            if n.parent >= 0:
                m = mat_mul(out[n.parent], m)
            out.append(m)
        return out


def mat_mul(a, b):
    """a*b for column-major 4x4 stored as 16-tuples (a[c*4+r])."""
    return tuple(sum(a[k * 4 + r] * b[c * 4 + k] for k in range(4))
                 for c in range(4) for r in range(4))


def parse(b):
    b = bytes(b)
    magic, ver, n, ofs = struct.unpack_from('<4sIII', b)
    if magic != b'MDS\0':
        raise ValueError('not an MDS file')
    mds = MDS()
    mds.version = ver
    o = ofs
    for i in range(n):
        idx, size = struct.unpack_from('<iI', b, o)
        if size != 0x70:
            raise ValueError('unexpected record size %d' % size)
        node = Node()
        node.index = None if idx == i else idx
        node.name, node.name_raw = _name_in(b[o + 8:o + 40])
        mdt_ofs, node.parent = struct.unpack_from('<ii', b, o + 40)
        node.matrix = struct.unpack_from('<16f', b, o + 48)
        node.mdt = MDT.unpack(b, mdt_ofs) if mdt_ofs else None
        mds.nodes.append(node)
        o += size
    return mds


def build(mds):
    n = len(mds.nodes)
    blocks = [nd.mdt.pack() if nd.mdt else None for nd in mds.nodes]
    pos = 0x10 + 0x70 * n
    out = [struct.pack('<4sIII', b'MDS\0', mds.version, n, 0x10)]
    for i, nd in enumerate(mds.nodes):
        mo = 0
        if blocks[i] is not None:
            mo = pos
            pos += len(blocks[i])
        out.append(struct.pack('<iI', i if nd.index is None else nd.index, 0x70)
                   + _name_out(nd.name, nd.name_raw, 32)
                   + struct.pack('<ii16f', mo, nd.parent, *nd.matrix))
    out.extend(x for x in blocks if x is not None)
    return b''.join(out)


def read(path):
    with open(path, 'rb') as f:
        return parse(f.read())


# ---------------------------------------------------------------------------
def _check(root):
    import collections
    st = collections.Counter()
    fails = []
    for d, _, fs in os.walk(root):
        for f in fs:
            if not f.lower().endswith('.mds'):
                continue
            p = os.path.join(d, f)
            b = open(p, 'rb').read()
            st['files'] += 1
            if not b:
                st['empty (0 bytes, skipped)'] += 1
                continue
            try:
                m = parse(b)
                out = build(m)
            except Exception as e:  # noqa
                st['parse error'] += 1
                fails.append((p, repr(e)))
                continue
            if out == b:
                st['round-trip identical'] += 1
            else:
                st['round-trip DIFFERENT'] += 1
                fails.append((p, 'differs'))
            for nd in m.nodes:
                st['nodes'] += 1
                if nd.mdt:
                    st['meshes'] += 1
                    st['meshes style ' + nd.mdt.style] += 1
                    if nd.mdt.layout:
                        st['meshes kept with explicit layout'] += 1
                    if nd.mdt.faces_pad:
                        st['meshes with non-zero padding after faces'] += 1
                    if nd.mdt.faces_unk00:
                        st['meshes with faces_unk00 != 0'] += 1
                    for mt in nd.mdt.materials:
                        if mt.texture_raw is not None:
                            st['texture names with bytes after NUL'] += 1
                if nd.name_raw is not None:
                    st['node names kept raw'] += 1
                if nd.index is not None:
                    st['node index != position'] += 1
    for k in sorted(st):
        print('%-40s %d' % (k, st[k]))
    for p, e in fails[:20]:
        print('FAIL', p, e)
    return not fails


def _dump(path):
    m = read(path)
    print('version', m.version, 'nodes', len(m.nodes))
    for i, nd in enumerate(m.nodes):
        s = '%3d %-32s parent %3d' % (i, nd.name, nd.parent)
        if nd.mdt:
            t = nd.mdt
            s += '  mesh v%d n%d uv%d c%d prims %d mats [%s] %s' % (
                len(t.vertices), len(t.normals), len(t.uvs), len(t.colours), len(t.prims),
                ', '.join(x.texture for x in t.materials), t.style)
        print(s)


if __name__ == '__main__':
    if len(sys.argv) >= 3 and sys.argv[1] == 'check':
        sys.exit(0 if _check(sys.argv[2]) else 1)
    elif len(sys.argv) >= 3 and sys.argv[1] == 'dump':
        _dump(sys.argv[2])
    else:
        print(__doc__)
