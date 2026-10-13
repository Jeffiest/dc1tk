"""Export a Dark Chronicle character (.chr.d folder) to glTF 2.0 (.gltf + .bin).

Standard library only.

    python -I -X utf8 to_gltf.py <chr.d> <out.gltf> [--skin <costume chr.d>]
                                 [--fps 30] [--no-anim] [--clips N]

<chr.d> is an unpacked .chr pack with an info.cfg.  Its MODEL is the frame
hierarchy, its MOTION 0 gives the .mot (animation), .bbp (bind pose) and the
KEY list (named clips).  The weights come from "<model>.wgt" (the file the
engine's _MODEL reads, the VU1 skinning path).

--skin <dir> takes the meshes, bind pose and weights from a costume pack
(e.g. mainchr/c01fuku1.chr.d) instead, the way CCharacter2::LoadSkin swaps
visuals onto the base frames by name; the animation still comes from <chr.d>
and is matched to the costume's frames by name.

What is written:
  * one glTF node per MDS frame, same order, local TRS from the MDS matrix;
  * rigid meshes on their own frame's node (vertices in frame space);
  * skinned meshes (VERTEX_ANIME frames with weights) on extra nodes at the
    scene root, vertices moved to model space with the .bbp matrix of their
    frame, one skin whose joints are all frames (joint i = frame i), inverse
    bind matrices = inverse .bbp matrices;
  * one material per MDT material, base colour texture "textures/<name>.png"
    (an 8x8 placeholder: .img textures are not decoded here);
  * one animation per KEY (frames start..end, sampled every frame, --fps).
Engine-only data (frame name flags, raw material fields) goes in "extras".
"""

import json
import math
import os
import struct
import sys
import zlib

from c2s import info_cfg  # noqa: E402
from c2s import mds as mdsmod  # noqa: E402
from c2s import mot as motmod  # noqa: E402

IDENT = (1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0)


# ---------------------------------------------------------------- math
def mat_mul(a, b):
    return mdsmod.mat_mul(a, b)


def mat_inv(m):
    """General 4x4 inverse of a column-major 16-tuple; None if singular."""
    a = [[m[c * 4 + r] for c in range(4)] + [1.0 if r == c else 0.0 for c in range(4)] for r in range(4)]
    for col in range(4):
        piv = max(range(col, 4), key=lambda r: abs(a[r][col]))
        if abs(a[piv][col]) < 1e-12:
            return None
        a[col], a[piv] = a[piv], a[col]
        p = a[col][col]
        a[col] = [x / p for x in a[col]]
        for r in range(4):
            if r != col and a[r][col]:
                f = a[r][col]
                a[r] = [x - f * y for x, y in zip(a[r], a[col])]
    return tuple(a[r][4 + c] for c in range(4) for r in range(4))


def xform_point(m, v):
    return tuple(m[r] * v[0] + m[4 + r] * v[1] + m[8 + r] * v[2] + m[12 + r] for r in range(3))


def xform_dir(m, v):
    return tuple(m[r] * v[0] + m[4 + r] * v[1] + m[8 + r] * v[2] for r in range(3))


def normalize(v, fallback=(0.0, 0.0, 1.0)):
    l = math.sqrt(sum(x * x for x in v))
    return tuple(x / l for x in v) if l > 1e-12 else fallback


def quat_from_rot(c0, c1, c2):
    """Quaternion (x, y, z, w) of the rotation whose columns are c0, c1, c2."""
    m00, m10, m20 = c0
    m01, m11, m21 = c1
    m02, m12, m22 = c2
    tr = m00 + m11 + m22
    if tr > 0:
        s = math.sqrt(tr + 1.0) * 2
        q = ((m21 - m12) / s, (m02 - m20) / s, (m10 - m01) / s, 0.25 * s)
    elif m00 > m11 and m00 > m22:
        s = math.sqrt(1.0 + m00 - m11 - m22) * 2
        q = (0.25 * s, (m01 + m10) / s, (m02 + m20) / s, (m21 - m12) / s)
    elif m11 > m22:
        s = math.sqrt(1.0 + m11 - m00 - m22) * 2
        q = ((m01 + m10) / s, 0.25 * s, (m12 + m21) / s, (m02 - m20) / s)
    else:
        s = math.sqrt(1.0 + m22 - m00 - m11) * 2
        q = ((m02 + m20) / s, (m12 + m21) / s, 0.25 * s, (m10 - m01) / s)
    return normalize(q, (0.0, 0.0, 0.0, 1.0))


def decompose(m):
    t = (m[12], m[13], m[14])
    cols = [m[0:3], m[4:7], m[8:11]]
    s = [math.sqrt(sum(x * x for x in c)) for c in cols]
    det = (cols[0][0] * (cols[1][1] * cols[2][2] - cols[1][2] * cols[2][1])
           - cols[1][0] * (cols[0][1] * cols[2][2] - cols[0][2] * cols[2][1])
           + cols[2][0] * (cols[0][1] * cols[1][2] - cols[0][2] * cols[1][1]))
    if det < 0:
        s[0] = -s[0]
    rc = [tuple(x / (s[i] or 1.0) for x in cols[i]) for i in range(3)]
    return t, quat_from_rot(*rc), tuple(s)


def engine_quat_to_gltf(q):
    """.mot rotation keys are (w, x, y, z) fed to QuatToMat, which builds the
    transpose of the usual matrix; so the glTF rotation is the conjugate."""
    w, x, y, z = q
    return normalize((-x, -y, -z, w), (0.0, 0.0, 0.0, 1.0))


def slerp(a, b, t):
    d = sum(x * y for x, y in zip(a, b))
    if d < 0:
        b, d = tuple(-x for x in b), -d
    if d > 0.9995:
        return normalize(tuple(x + (y - x) * t for x, y in zip(a, b)))
    th = math.acos(min(1.0, d))
    s = math.sin(th)
    fa, fb = math.sin((1 - t) * th) / s, math.sin(t * th) / s
    return normalize(tuple(fa * x + fb * y for x, y in zip(a, b)))


# ---------------------------------------------------------------- glTF builder
class Gltf:
    def __init__(self):
        self.bin = bytearray()
        self.j = {'asset': {'version': '2.0', 'generator': 'ChronicleTwo-Script to_gltf.py'},
                  'scene': 0, 'scenes': [{'nodes': []}], 'nodes': [], 'meshes': [],
                  'materials': [], 'textures': [], 'images': [], 'samplers': [],
                  'accessors': [], 'bufferViews': [], 'buffers': [], 'skins': [],
                  'animations': []}

    def view(self, data, target=None):
        while len(self.bin) % 4:
            self.bin.append(0)
        bv = {'buffer': 0, 'byteOffset': len(self.bin), 'byteLength': len(data)}
        if target:
            bv['target'] = target
        self.bin += data
        self.j['bufferViews'].append(bv)
        return len(self.j['bufferViews']) - 1

    def accessor(self, values, kind, ctype=5126, target=None, minmax=False, normalized=False):
        n = {'SCALAR': 1, 'VEC2': 2, 'VEC3': 3, 'VEC4': 4, 'MAT4': 16}[kind]
        fmt = {5126: 'f', 5123: 'H', 5125: 'I', 5121: 'B'}[ctype]
        flat = [x for v in values for x in (v if n > 1 else (v,))]
        data = struct.pack('<%d%s' % (len(flat), fmt), *flat)
        acc = {'bufferView': self.view(data, target), 'componentType': ctype,
               'count': len(values), 'type': kind}
        if normalized:
            acc['normalized'] = True
        if minmax:
            cols = [[v[i] if n > 1 else v for v in values] for i in range(n)]
            acc['min'] = [min(c) for c in cols]
            acc['max'] = [max(c) for c in cols]
        self.j['accessors'].append(acc)
        return len(self.j['accessors']) - 1

    def finish(self, out_path):
        base = os.path.splitext(os.path.basename(out_path))[0]
        self.j['buffers'] = [{'byteLength': len(self.bin), 'uri': base + '.bin'}]
        for k in [k for k, v in self.j.items() if v == []]:
            del self.j[k]
        with open(os.path.join(os.path.dirname(out_path) or '.', base + '.bin'), 'wb') as f:
            f.write(self.bin)
        with open(out_path, 'w', encoding='utf-8') as f:
            json.dump(self.j, f, ensure_ascii=False, separators=(',', ':'))


def placeholder_png(name):
    h = zlib.crc32(name.encode('utf-8'))
    c1 = (h & 0xFF, (h >> 8) & 0xFF, (h >> 16) & 0xFF)
    c2 = tuple(255 - x for x in c1)
    rows = b''.join(b'\0' + b''.join(bytes(c1 if (x // 2 + y // 2) % 2 else c2) for x in range(8))
                    for y in range(8))
    chunk = lambda t, d: struct.pack('>I', len(d)) + t + d + struct.pack('>I', zlib.crc32(t + d))
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 8, 8, 8, 2, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(rows)) + chunk(b'IEND', b''))


# ---------------------------------------------------------------- weights
def vertex_weights(wgt_lists, frame_id, vertex_num):
    """Replicates mgCVisualMotionMDT::CreateVertexWeight: per vertex up to 4
    (bone frame, weight) pairs, in file order, weights normalised; a vertex no
    block mentions gets weight 1 on the first bone the blocks list."""
    out = [[] for _ in range(vertex_num)]
    first_bone = None
    for kl in wgt_lists:
        if kl.frame != frame_id or not kl.keys:
            continue
        if first_bone is None:
            first_bone = kl.target
        for v, val in kl.keys:
            if 0 <= v < vertex_num and len(out[v]) < 4 and val[0] != 0.0:
                out[v].append([kl.target, val[0] / 100.0])
    if first_bone is None:
        return None
    for l in out:
        s = sum(w for _, w in l)
        if s == 0:
            l[:] = [[first_bone, 1.0]]
        elif s != 1.0:
            for p in l:
                p[1] /= s
    return out


# ---------------------------------------------------------------- export
def load_char(d):
    info = info_cfg.read(os.path.join(d, 'info.cfg'))
    if not info.model:
        raise SystemExit('%s: info.cfg has no MODEL' % d)
    model = mdsmod.read(os.path.join(d, info.model))
    mo = info.motions.get(0, {})
    bbp = None
    if mo.get('bbp') and os.path.exists(os.path.join(d, mo['bbp'])):
        bbp = motmod.parse_bbp(open(os.path.join(d, mo['bbp']), 'rb').read())
    wname = os.path.splitext(info.model)[0] + '.wgt'
    wgt = None
    if os.path.exists(os.path.join(d, wname)):
        b = open(os.path.join(d, wname), 'rb').read()
        wgt = motmod.parse(b) if b else None
    motion = None
    if mo.get('mot') and os.path.exists(os.path.join(d, mo['mot'])):
        motion = motmod.read(os.path.join(d, mo['mot']))
    return info, model, bbp, wgt, motion


def export(base_dir, out_path, skin_dir=None, fps=30.0, anim=True, max_clips=None):
    binfo, bmodel, bbbp, bwgt, bmotion = load_char(base_dir)
    if skin_dir:
        sinfo, model, bbp, wgt, _ = load_char(skin_dir)
        deform = set(binfo.vertex_anime) | set(sinfo.vertex_anime)
        src_desc = {'base': os.path.basename(os.path.normpath(base_dir)),
                    'skin': os.path.basename(os.path.normpath(skin_dir))}
    else:
        model, bbp, wgt = bmodel, bbbp, bwgt
        deform = set(binfo.vertex_anime)
        # the folder's name only, so an export of the same pack is the same bytes
        src_desc = {'base': os.path.basename(os.path.normpath(base_dir))}
    out_dir = os.path.dirname(os.path.abspath(out_path))
    tex_dir = os.path.join(out_dir, 'textures')
    g = Gltf()
    J = g.j
    world = model.world_matrices()
    nodes = model.nodes

    # skeleton nodes -------------------------------------------------------
    for i, nd in enumerate(nodes):
        t, r, s = decompose(nd.matrix)
        gn = {'name': nd.base_name or ('node%d' % i)}
        if any(abs(x) > 0 for x in t):
            gn['translation'] = list(t)
        if r != (0.0, 0.0, 0.0, 1.0):
            gn['rotation'] = list(r)
        if any(abs(x - 1) > 1e-6 for x in s):
            gn['scale'] = list(s)
        gn['extras'] = {'mds_index': i, 'mds_name': nd.name}
        J['nodes'].append(gn)
    for i, nd in enumerate(nodes):
        if nd.parent >= 0:
            J['nodes'][nd.parent].setdefault('children', []).append(i)
        else:
            J['scenes'][0]['nodes'].append(i)
    if len(J['scenes'][0]['nodes']) > 1:
        # 4 of 16,873 MDS files have two roots; glTF skins want one common root.
        J['nodes'].append({'name': '_mds_roots', 'children': J['scenes'][0]['nodes'],
                           'extras': {'synthetic': True}})
        J['scenes'][0]['nodes'] = [len(J['nodes']) - 1]
    skel_root = J['scenes'][0]['nodes'][0]

    # materials -------------------------------------------------------------
    tex_index = {}
    mat_index = {}

    def material(m, textured):
        key = (m.texture if textured else '', m.diffuse, m.ambient)
        if key in mat_index:
            return mat_index[key]
        pbr = {'baseColorFactor': [max(0.0, min(1.0, x)) for x in m.diffuse],
               'metallicFactor': 0.0, 'roughnessFactor': 1.0}
        if textured and m.texture:
            if m.texture not in tex_index:
                os.makedirs(tex_dir, exist_ok=True)
                fn = m.texture + '.png'
                with open(os.path.join(tex_dir, fn), 'wb') as f:
                    f.write(placeholder_png(m.texture))
                if not J['samplers']:
                    J['samplers'].append({'magFilter': 9729, 'minFilter': 9729})
                J['images'].append({'uri': 'textures/' + fn, 'name': m.texture})
                J['textures'].append({'source': len(J['images']) - 1, 'sampler': 0, 'name': m.texture})
                tex_index[m.texture] = len(J['textures']) - 1
            pbr['baseColorTexture'] = {'index': tex_index[m.texture]}
        gm = {'name': (m.texture or 'untextured') + ('' if textured else '_notex'),
              'pbrMetallicRoughness': pbr,
              'extras': {'mdt_diffuse': list(m.diffuse), 'mdt_ambient': list(m.ambient),
                         'mdt_specular': list(m.specular), 'mdt_power': m.power,
                         'texture': m.texture}}
        if m.diffuse[3] < 1.0:
            gm['alphaMode'] = 'BLEND'
        J['materials'].append(gm)
        mat_index[key] = len(J['materials']) - 1
        return mat_index[key]

    # meshes ----------------------------------------------------------------
    any_skinned = False
    for i, nd in enumerate(nodes):
        t = nd.mdt
        if not t:
            continue
        weights = None
        if wgt and nd.base_name in deform:
            weights = vertex_weights(wgt, i, len(t.vertices))
        skinned = weights is not None
        bind = None
        if skinned:
            bind = bbp[i] if (bbp and i < len(bbp) and mat_inv(bbp[i])) else world[i]
        groups = {}
        for p in t.prims:
            sv, sn, su, sc = p.slot('v'), p.slot('n'), p.slot('uv'), p.slot('c')
            key = (p.material, sn is not None, su is not None, sc is not None)
            gr = groups.setdefault(key, {'map': {}, 'verts': [], 'idx': []})
            for tri in p.triangles():
                corners = [p.verts[c] for c in tri]
                if len({c[sv] for c in corners}) < 3:
                    continue  # degenerate (strip joins)
                for c in corners:
                    k = (c[sv], c[sn] if sn is not None else -1,
                         c[su] if su is not None else -1, c[sc] if sc is not None else -1)
                    if k not in gr['map']:
                        gr['map'][k] = len(gr['verts'])
                        gr['verts'].append(k)
                    gr['idx'].append(gr['map'][k])
        prims = []
        for (mat, hn, hu, hc), gr in sorted(groups.items()):
            if not gr['idx']:
                continue
            vs = gr['verts']
            pos = [tuple(t.vertices[k[0]][:3]) for k in vs]
            if skinned:
                pos = [xform_point(bind, v) for v in pos]
            attrs = {'POSITION': g.accessor(pos, 'VEC3', target=34962, minmax=True)}
            if hn:
                nrm = [normalize(tuple(t.normals[k[1]][:3])) for k in vs]
                if skinned:
                    nrm = [normalize(xform_dir(bind, v)) for v in nrm]
                attrs['NORMAL'] = g.accessor(nrm, 'VEC3', target=34962)
            if hu:
                attrs['TEXCOORD_0'] = g.accessor([tuple(t.uvs[k[2]][:2]) for k in vs], 'VEC2', target=34962)
            if hc:
                attrs['COLOR_0'] = g.accessor([tuple(max(0.0, min(1.0, x)) for x in t.colours[k[3]])
                                               for k in vs], 'VEC4', target=34962)
            if skinned:
                js, ws = [], []
                for k in vs:
                    l = weights[k[0]] + [[0, 0.0]] * (4 - len(weights[k[0]]))
                    js.append(tuple(b for b, _ in l))
                    ws.append(tuple(w for _, w in l))
                attrs['JOINTS_0'] = g.accessor(js, 'VEC4', ctype=5123, target=34962)
                attrs['WEIGHTS_0'] = g.accessor(ws, 'VEC4', target=34962)
            big = len(vs) > 65535
            gp = {'attributes': attrs,
                  'indices': g.accessor(gr['idx'], 'SCALAR', ctype=5125 if big else 5123, target=34963),
                  'mode': 4}
            if t.materials and 0 <= mat < len(t.materials):
                gp['material'] = material(t.materials[mat], hu)
            prims.append(gp)
        if not prims:
            continue
        J['meshes'].append({'name': nd.base_name, 'primitives': prims,
                            'extras': {'mdt_style': t.style}})
        mi = len(J['meshes']) - 1
        if skinned:
            any_skinned = True
            J['nodes'].append({'name': nd.base_name + '_skin', 'mesh': mi, 'skin': 0,
                               'extras': {'mds_index': i, 'skinned_mesh_of': nd.name}})
            J['scenes'][0]['nodes'].append(len(J['nodes']) - 1)
        else:
            J['nodes'][i]['mesh'] = mi

    if any_skinned:
        ibm = []
        for i in range(len(nodes)):
            m = bbp[i] if (bbp and i < len(bbp)) else None
            inv = mat_inv(m) if m else None
            if inv is None:
                inv = mat_inv(world[i]) or IDENT
            ibm.append(inv)
        J['skins'].append({'name': 'skeleton', 'joints': list(range(len(nodes))),
                           'skeleton': skel_root,
                           'inverseBindMatrices': g.accessor(ibm, 'MAT4')})

    # animations ------------------------------------------------------------
    clips = binfo.motions.get(0, {}).get('keys', [])
    if anim and bmotion and clips:
        by_name = {}
        for i, nd in enumerate(nodes):
            by_name.setdefault(nd.base_name.lower(), i)
        chans = {}   # (node, path) -> keylist
        for kl in bmotion:
            if kl.type not in (0, 1, 2) or not kl.keys or kl.frame >= len(bmodel.nodes):
                continue
            tgt = by_name.get(bmodel.nodes[kl.frame].base_name.lower())
            if tgt is None:
                continue
            path = {0: 'rotation', 1: 'scale', 2: 'translation'}[kl.type]
            chans[(tgt, path)] = kl
        seen = set()
        for clip in clips[:max_clips] if max_clips else clips:
            name = clip['name']
            if name in seen:
                name = '%s_%d' % (name, clip['start'])
            seen.add(name)
            s, e = clip['start'], max(clip['start'], clip['end'])
            frames = list(range(s, e + 1))
            times = g.accessor([(f - s) / fps for f in frames], 'SCALAR', minmax=True)
            samplers, channels = [], []
            for (node, path), kl in sorted(chans.items()):
                kf = [k for k, _ in kl.keys]
                vals = []
                for f in frames:
                    vals.append(sample(kl, kf, f, path))
                if path == 'rotation':
                    for a in range(1, len(vals)):
                        if sum(x * y for x, y in zip(vals[a - 1], vals[a])) < 0:
                            vals[a] = tuple(-x for x in vals[a])
                acc = g.accessor(vals, 'VEC4' if path == 'rotation' else 'VEC3')
                samplers.append({'input': times, 'output': acc, 'interpolation': 'LINEAR'})
                channels.append({'sampler': len(samplers) - 1, 'target': {'node': node, 'path': path}})
            J['animations'].append({'name': name, 'samplers': samplers, 'channels': channels,
                                    'extras': {'start': clip['start'], 'end': clip['end'],
                                               'step': clip['step']}})
    J['asset']['extras'] = {'source': src_desc, 'fps_assumed': fps,
                            'note': 'textures are placeholders; MDS node i = glTF node i'}
    g.finish(out_path)
    return g


def sample(kl, kf, f, path):
    """Value at integer frame f, as MotionProc interpolates (slerp/lerp)."""
    import bisect
    hi = bisect.bisect_right(kf, f)
    k = max(0, hi - 1)
    n = min(hi, len(kf) - 1)
    a, b = kl.keys[k][1], kl.keys[n][1]
    t = 0.0 if kf[n] == kf[k] else (f - kf[k]) / (kf[n] - kf[k])
    if path == 'rotation':
        qa, qb = engine_quat_to_gltf(a), engine_quat_to_gltf(b)
        return slerp(qa, qb, t) if 0 < t < 1 else (qa if t <= 0 else qb)
    return tuple(a[i] + (b[i] - a[i]) * t for i in range(3))


if __name__ == '__main__':
    args = sys.argv[1:]
    if len(args) < 2:
        print(__doc__)
        sys.exit(1)
    opts = {'--skin': None, '--fps': '30', '--clips': None}
    flags = set()
    pos = []
    i = 0
    while i < len(args):
        if args[i] in opts:
            opts[args[i]] = args[i + 1]
            i += 2
        elif args[i].startswith('--'):
            flags.add(args[i])
            i += 1
        else:
            pos.append(args[i])
            i += 1
    g = export(pos[0], pos[1], skin_dir=opts['--skin'], fps=float(opts['--fps']),
               anim='--no-anim' not in flags,
               max_clips=int(opts['--clips']) if opts['--clips'] else None)
    J = g.j
    print('nodes %d meshes %d materials %d animations %d bin %d bytes' % (
        len(J['nodes']), len(J.get('meshes', [])), len(J.get('materials', [])),
        len(J.get('animations', [])), len(g.bin)))
