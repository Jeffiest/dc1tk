"""EXPERIMENTAL: rebuild MDS meshes (+ .wgt skin weights, + .bbp) from a glTF.

Standard library only.  Uses the original MDS as a template: the frame
hierarchy (names with "--" flags, order, parents, matrices) is kept, because
motions, info.cfg, .clo and costume swapping all depend on it.  Mesh blocks
are replaced by name.

    python -I -X utf8 from_gltf.py <in.gltf|in.glb> <template.mds> <out_dir>
                                   [--bbp template.bbp] [--wgt template.wgt]
                                   [--name stem] [--drop-missing]

Matching: a glTF mesh node named "X" or "X_skin" (as to_gltf.py writes it;
Blender's ".001" suffixes are ignored) replaces the MDT of template node X.
A mesh whose name matches no node is added as a new node under the root (and
must then be listed in info.cfg VERTEX_ANIME if skinned).  Joints are matched
to template nodes by name.

Writes <out_dir>/<stem>.mds, <stem>.wgt and <stem>.bbp.  Untested in the game.
"""

import json
import os
import struct
import sys

from c2s import mds as mdsmod  # noqa: E402
from c2s import mot as motmod  # noqa: E402
from c2s.to_gltf import mat_inv, mat_mul, xform_point, xform_dir, normalize, IDENT  # noqa: E402

COMP = {5120: ('b', 1), 5121: ('B', 1), 5122: ('h', 2), 5123: ('H', 2), 5125: ('I', 4), 5126: ('f', 4)}
NCOMP = {'SCALAR': 1, 'VEC2': 2, 'VEC3': 3, 'VEC4': 4, 'MAT4': 16}


def load_gltf(path):
    raw = open(path, 'rb').read()
    if raw[:4] == b'glTF':
        o = 12
        j, b = None, b''
        while o < len(raw):
            ln, typ = struct.unpack_from('<I4s', raw, o)
            chunk = raw[o + 8:o + 8 + ln]
            if typ == b'JSON':
                j = json.loads(chunk.decode('utf-8'))
            elif typ == b'BIN\0':
                b = chunk
            o += 8 + ln
        bufs = [b]
    else:
        j = json.loads(raw.decode('utf-8'))
        bufs = []
        for bf in j['buffers']:
            uri = bf['uri']
            if uri.startswith('data:'):
                import base64
                bufs.append(base64.b64decode(uri.split(',', 1)[1]))
            else:
                bufs.append(open(os.path.join(os.path.dirname(path), uri), 'rb').read())
    return j, bufs


def accessor(j, bufs, i):
    a = j['accessors'][i]
    n = NCOMP[a['type']]
    fmt, size = COMP[a['componentType']]
    if 'bufferView' not in a:
        vals = [0] * (a['count'] * n)
    else:
        bv = j['bufferViews'][a['bufferView']]
        buf = bufs[bv.get('buffer', 0)]
        base = bv.get('byteOffset', 0) + a.get('byteOffset', 0)
        stride = bv.get('byteStride') or n * size
        vals = []
        for k in range(a['count']):
            vals.extend(struct.unpack_from('<%d%s' % (n, fmt), buf, base + k * stride))
    if a.get('normalized'):
        div = {5120: 127.0, 5121: 255.0, 5122: 32767.0, 5123: 65535.0}[a['componentType']]
        vals = [max(-1.0, v / div) for v in vals]
    if 'sparse' in a:
        raise ValueError('sparse accessors are not supported')
    return [tuple(vals[k:k + n]) for k in range(0, len(vals), n)] if n > 1 else vals


def node_matrix(n):
    if 'matrix' in n:
        return tuple(n['matrix'])
    t = n.get('translation', (0, 0, 0))
    x, y, z, w = n.get('rotation', (0, 0, 0, 1))
    s = n.get('scale', (1, 1, 1))
    R = [1 - 2 * (y * y + z * z), 2 * (x * y + w * z), 2 * (x * z - w * y),
         2 * (x * y - w * z), 1 - 2 * (x * x + z * z), 2 * (y * z + w * x),
         2 * (x * z + w * y), 2 * (y * z - w * x), 1 - 2 * (x * x + y * y)]
    return (R[0] * s[0], R[1] * s[0], R[2] * s[0], 0.0, R[3] * s[1], R[4] * s[1], R[5] * s[1], 0.0,
            R[6] * s[2], R[7] * s[2], R[8] * s[2], 0.0, t[0], t[1], t[2], 1.0)


def clean(name):
    name = name or ''
    if len(name) > 4 and name[-4] == '.' and name[-3:].isdigit():
        name = name[:-4]          # Blender duplicate suffix
    return name


def build_mdt(prims, materials):
    """prims: list of dicts {mat, corners: [(pos, nrm|None, uv|None, col|None, key)]} where
    corners come in threes (triangles, CCW).  key identifies the MDT vertex (position + skin)."""
    t = mdsmod.MDT()
    vmap, nmap, umap, cmap = {}, {}, {}, {}
    vkeys = []

    def idx(m, arr, val):
        if val not in m:
            m[val] = len(arr)
            arr.append(val)
        return m[val]
    for p in prims:
        hn = p['corners'][0][1] is not None
        hu = p['corners'][0][2] is not None
        hc = p['corners'][0][3] is not None
        typ = 3 | (0 if hn else 0x200) | (0 if hu else 0x10) | (0x100 if hc else 0)
        stride = mdsmod.Prim.stride_of(typ)
        limit = (32767 // stride) // 3 * 3
        corners = p['corners']
        for s in range(0, len(corners), limit):
            verts = []
            for pos, nrm, uv, col, key in corners[s:s + limit]:
                if key not in vmap:
                    vmap[key] = len(t.vertices)
                    t.vertices.append((pos[0], pos[1], pos[2], 1.0))
                    vkeys.append(key)
                tup = [vmap[key]]
                if hn:
                    tup.append(idx(nmap, t.normals, (nrm[0], nrm[1], nrm[2], 1.0)))
                if hu:
                    tup.append(idx(umap, t.uvs, (uv[0], uv[1], 1.0, 0.0)))
                if hc:
                    tup.append(idx(cmap, t.colours, tuple(col)))
                verts.append(tuple(tup))
            t.prims.append(mdsmod.Prim(typ, p['mat'], verts))
    if not t.normals:
        t.normals.append((0.0, 0.0, 1.0, 1.0))   # the corpus always has >= 1 normal
    t.materials = materials
    return t, vkeys


def material_from_gltf(j, mi):
    m = mdsmod.Material()
    if mi is None:
        return m
    gm = j['materials'][mi]
    ex = gm.get('extras', {})
    pbr = gm.get('pbrMetallicRoughness', {})
    m.diffuse = tuple(ex.get('mdt_diffuse', pbr.get('baseColorFactor', (1.0, 1.0, 1.0, 1.0))))
    m.ambient = tuple(ex.get('mdt_ambient', m.ambient))
    m.specular = tuple(ex.get('mdt_specular', m.specular))
    m.power = ex.get('mdt_power', m.power)
    tex = ex.get('texture')
    if tex is None and 'baseColorTexture' in pbr:
        tx = j['textures'][pbr['baseColorTexture']['index']]
        img = j['images'][tx['source']] if 'source' in tx else {}
        tex = img.get('name') or os.path.splitext(os.path.basename(img.get('uri', '')))[0]
    if tex is None:
        tex = gm.get('name', '')
    m.texture = tex
    if len(tex.encode('cp932', 'replace')) > 31:
        raise ValueError('texture name too long: %r' % tex)
    return m


def close(a, b, eps=1e-5):
    return all(abs(x - y) <= eps * max(1.0, abs(y)) for x, y in zip(a, b))


# meshes, when given, names the meshes to rebuild; the template keeps every other one as it was.
def import_gltf(gltf_path, template_mds, out_dir, bbp_path=None, wgt_path=None, stem=None,
                drop_missing=False, meshes=None):
    j, bufs = load_gltf(gltf_path)
    model = mdsmod.read(template_mds)
    bbp = motmod.parse_bbp(open(bbp_path, 'rb').read()) if bbp_path else model.world_matrices()
    bbp = list(bbp) + [IDENT] * (len(model.nodes) - len(bbp))
    old_wgt = motmod.read(wgt_path) if wgt_path and os.path.getsize(wgt_path) else []
    by_name = {n.base_name: i for i, n in enumerate(model.nodes)}

    # glTF world matrices
    parent = {}
    for i, n in enumerate(j['nodes']):
        for c in n.get('children', []):
            parent[c] = i
    world = {}

    def W(i):
        if i not in world:
            m = node_matrix(j['nodes'][i])
            world[i] = mat_mul(W(parent[i]), m) if i in parent else m
        return world[i]

    def mds_index_of(gi):
        n = j['nodes'][gi]
        nm = clean(n.get('name'))
        return by_name.get(nm)

    # bind matrices of joints from the IBMs (keeps edits to the bind pose)
    template_world = model.world_matrices()
    for sk in j.get('skins', []):
        if 'inverseBindMatrices' not in sk:
            continue
        ibm = accessor(j, bufs, sk['inverseBindMatrices'])
        for k, gi in enumerate(sk['joints']):
            mi = mds_index_of(gi)
            inv = mat_inv(tuple(ibm[k]))
            if mi is None or inv is None:
                continue
            # the template's bind matrix stays unless the glTF's differs from what to_gltf writes for
            # it: its inverse, or the frame's rest matrix when it has none (inverting twice drifts)
            exported = mat_inv(bbp[mi]) or (mat_inv(template_world[mi]) if mi < len(template_world) else None)
            if not (exported and close(tuple(ibm[k]), exported)):
                bbp[mi] = inv

    replaced = set()
    new_wgt = []
    report = []
    for gi, n in enumerate(j['nodes']):
        if 'mesh' not in n:
            continue
        nm = clean(n.get('name') or j['meshes'][n['mesh']].get('name'))
        if nm.endswith('_skin'):
            nm = nm[:-5]
        if meshes is not None and nm not in meshes:
            continue
        target = by_name.get(nm)
        if target is None:
            # a new node, appended under the root
            nd = mdsmod.Node(nm, parent=0 if model.nodes else -1)
            pj = parent.get(gi)
            if pj is not None and mds_index_of(pj) is not None and 'skin' not in n:
                nd.parent = mds_index_of(pj)
            model.nodes.append(nd)
            target = len(model.nodes) - 1
            by_name[nm] = target
            bbp.append(IDENT)
            report.append('added node %d %s' % (target, nm))
        skin = j['skins'][n['skin']] if 'skin' in n else None
        if skin:
            # skinned: glTF positions are model space (mesh node transform ignored)
            to_local = mat_inv(bbp[target]) or IDENT
            if mat_inv(bbp[target]) is None:
                bbp[target] = model.world_matrices()[target]
                to_local = mat_inv(bbp[target]) or IDENT
            joint_map = [mds_index_of(g) for g in skin['joints']]
        else:
            # rigid: bring vertices into the frame's own space through the glTF hierarchy
            fw = None
            for g2, n2 in enumerate(j['nodes']):
                if 'mesh' not in n2 and clean(n2.get('name')) == nm:
                    fw = W(g2)
            # the mesh node is the frame itself (to_gltf.py) or a child of a
            # same-named bone node (what Blender writes for a mesh parented to a bone)
            to_local = mat_mul(mat_inv(fw) or IDENT, W(gi)) if fw is not None else IDENT
        mats, mat_of = [], {}
        prims = []
        weights_of_key = {}
        for p in j['meshes'][n['mesh']]['primitives']:
            if p.get('mode', 4) != 4:
                raise ValueError('only triangle primitives are supported')
            A = p['attributes']
            pos = accessor(j, bufs, A['POSITION'])
            nrm = accessor(j, bufs, A['NORMAL']) if 'NORMAL' in A else None
            uv = accessor(j, bufs, A['TEXCOORD_0']) if 'TEXCOORD_0' in A else None
            col = accessor(j, bufs, A['COLOR_0']) if 'COLOR_0' in A else None
            jts = accessor(j, bufs, A['JOINTS_0']) if skin and 'JOINTS_0' in A else None
            wts = accessor(j, bufs, A['WEIGHTS_0']) if skin and 'WEIGHTS_0' in A else None
            idx = accessor(j, bufs, p['indices']) if 'indices' in p else list(range(len(pos)))
            mk = p.get('material')
            if mk not in mat_of:
                mat_of[mk] = len(mats)
                mats.append(material_from_gltf(j, mk))
            corners = []
            for c in idx:
                P = xform_point(to_local, pos[c])
                N = normalize(xform_dir(to_local, nrm[c])) if nrm else None
                U = (uv[c][0], uv[c][1]) if uv else None
                C = (tuple(col[c]) + (1.0,))[:4] if col else None
                sk = None
                if jts:
                    pairs = sorted(((w, joint_map[jj]) for jj, w in zip(jts[c], wts[c])
                                    if w > 0 and joint_map[jj] is not None), reverse=True)[:4]
                    tot = sum(w for w, _ in pairs) or 1.0
                    sk = tuple((b, round(w / tot * 100.0, 4)) for w, b in pairs)
                key = (P, sk)
                weights_of_key[key] = sk
                corners.append((P, N, U, C, key))
            if corners:
                prims.append({'mat': mat_of[mk], 'corners': corners})
        t, vkeys = build_mdt(prims, mats)
        model.nodes[target].mdt = t
        replaced.add(target)
        tris = sum(len(p.verts) for p in t.prims) // 3
        report.append('mesh %-14s -> node %3d: v%d n%d uv%d c%d tris %d%s' % (
            nm, target, len(t.vertices), len(t.normals), len(t.uvs), len(t.colours), tris,
            ' skinned' if skin else ''))
        if skin:
            per_bone = {}
            for vi, key in enumerate(vkeys):
                for b, w in (weights_of_key.get(key) or ()):
                    per_bone.setdefault(b, []).append((vi, (w, 0.0, 0.0, 0.0)))
            if len(per_bone) > 32:
                report.append('  WARNING: %d bones > 32 (VU1 limit)' % len(per_bone))
            for b in sorted(per_bone):
                new_wgt.append(motmod.KeyList(target, b, 20, per_bone[b]))

    if drop_missing:
        for i, nd in enumerate(model.nodes):
            if nd.mdt and i not in replaced:
                nd.mdt = None
                report.append('dropped mesh of node %d %s' % (i, nd.name))
    wgt = [kl for kl in old_wgt if kl.frame not in replaced] + new_wgt
    stem = stem or os.path.splitext(os.path.basename(template_mds))[0]
    os.makedirs(out_dir, exist_ok=True)
    with open(os.path.join(out_dir, stem + '.mds'), 'wb') as f:
        f.write(mdsmod.build(model))
    with open(os.path.join(out_dir, stem + '.wgt'), 'wb') as f:
        f.write(motmod.build(wgt))
    with open(os.path.join(out_dir, stem + '.bbp'), 'wb') as f:
        f.write(motmod.build_bbp(bbp))
    return report


if __name__ == '__main__':
    a = sys.argv[1:]
    if len(a) < 3:
        print(__doc__)
        sys.exit(1)
    opt = lambda k: a[a.index(k) + 1] if k in a else None
    for line in import_gltf(a[0], a[1], a[2], bbp_path=opt('--bbp'), wgt_path=opt('--wgt'),
                            stem=opt('--name'), drop_missing='--drop-missing' in a):
        print(line)
