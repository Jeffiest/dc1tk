"""Dark Chronicle key-list files (.mot motions, .wgt skin weights) and .bbp
bind-pose matrix tables: parse and write back.  Standard library only.

    python -I mot.py check <extracted-dir>     round-trip every .mot/.wgt/.bbp
    python -I mot.py dump  <file.mot|file.wgt>

Key-list file (.mot and .wgt share it; engine: CreateAnimeDataEX, ChangeWeight,
mgCVisualMotionMDT::CreateVertexWeight):
  repeated blocks until one has more == 0:
    0x00 s32 frame      index of the MDS node the keys drive (for .wgt: the skinned mesh node)
    0x04 s32 target     type 12: vertex index + 1; 40/41: material index; 20/21: bone node index
    0x08 s32 type       0 rotation quaternion (w,x,y,z), 1 scale, 2 translation,
                        12 vertex position, 20/21 skin weight, 30-33 camera,
                        40 material alpha, 41 material colour, 50/51 visibility
    0x0C u32 0x20       (always 32: size of the header / of a key)
    0x10 u32 key_count
    0x14 u32 more       0 on the last block, else (key_count+1)*32, the block size
    0x18 u32[2]         exporter garbage (uninitialised memory), never read
    key_count keys of 0x20:
    0x00 u32 frame      motion frame (for skin weights: the vertex index)
    0x04 u32[3]         always 0
    0x10 f32[4] value   (for skin weights: x = weight in percent)

.bbp: node_num matrices of 16 f32 (column-major), the bind-pose
local-to-model matrix of every node of the matching MDS.
"""

import struct
import sys
import os

TYPE_NAMES = {0: 'rotation', 1: 'scale', 2: 'translation', 12: 'vertex',
              20: 'skin_weighted', 21: 'skin_averaged', 30: 'camera_pos',
              31: 'camera_target', 32: 'camera_roll', 33: 'camera_fov',
              40: 'material_alpha', 41: 'material_colour', 50: 'visible',
              51: 'visible2'}


class KeyList:
    __slots__ = ('frame', 'target', 'type', 'keys', 'unk_0c', 'garbage', 'key_pad', 'more_raw')

    def __init__(self, frame, target, type, keys=None):
        self.frame, self.target, self.type = frame, target, type
        self.keys = keys or []   # list of (frame, (x, y, z, w))
        self.unk_0c = 0x20
        self.garbage = (0, 0)
        self.key_pad = None      # {key index: 12 bytes} when non-zero
        self.more_raw = None


def parse(b):
    b = bytes(b)
    out = []
    o = 0
    if not b:
        return out
    while True:
        fr, tg, ty, u0c, kc, more, g0, g1 = struct.unpack_from('<iiiIIIII', b, o)
        kl = KeyList(fr, tg, ty)
        kl.unk_0c, kl.garbage = u0c, (g0, g1)
        q = o + 32
        for k in range(kc):
            kf = struct.unpack_from('<I', b, q)[0]
            pad = b[q + 4:q + 16]
            if any(pad):
                kl.key_pad = kl.key_pad or {}
                kl.key_pad[k] = pad
            kl.keys.append((kf, struct.unpack_from('<4f', b, q + 16)))
            q += 32
        out.append(kl)
        o = q
        expect = (kc + 1) * 32
        if more not in (0, expect):
            kl.more_raw = more
        if not more:
            break
    if o != len(b):
        raise ValueError('trailing data in key-list file')
    return out


def build(lists):
    out = []
    for i, kl in enumerate(lists):
        last = i == len(lists) - 1
        more = kl.more_raw if kl.more_raw is not None else (0 if last else (len(kl.keys) + 1) * 32)
        out.append(struct.pack('<iiiIIIII', kl.frame, kl.target, kl.type, kl.unk_0c,
                               len(kl.keys), more, *kl.garbage))
        for k, (kf, v) in enumerate(kl.keys):
            pad = kl.key_pad.get(k, bytes(12)) if kl.key_pad else bytes(12)
            out.append(struct.pack('<I', kf) + pad + struct.pack('<4f', *v))
    return b''.join(out)


def parse_bbp(b):
    if len(b) % 64:
        raise ValueError('bbp size is not a multiple of 64')
    return [struct.unpack_from('<16f', b, i) for i in range(0, len(b), 64)]


def build_bbp(mats):
    return b''.join(struct.pack('<16f', *m) for m in mats)


def read(path):
    with open(path, 'rb') as f:
        return parse(f.read())


def _check(root):
    import collections
    st = collections.Counter()
    fails = []
    for d, _, fs in os.walk(root):
        for f in fs:
            ext = f.rsplit('.', 1)[-1].lower()
            if ext not in ('mot', 'wgt', 'bbp'):
                continue
            p = os.path.join(d, f)
            b = open(p, 'rb').read()
            st[ext + ' files'] += 1
            if not b:
                st[ext + ' empty'] += 1
            try:
                if ext == 'bbp':
                    ok = build_bbp(parse_bbp(b)) == b
                else:
                    lists = parse(b)
                    ok = build(lists) == b
                    for kl in lists:
                        st[ext + ' type %d %s' % (kl.type, TYPE_NAMES.get(kl.type, '?'))] += 1
                        if kl.key_pad:
                            st[ext + ' blocks with non-zero key padding'] += 1
                        if kl.more_raw is not None:
                            st[ext + ' blocks with odd "more"'] += 1
            except Exception as e:  # noqa
                ok = False
                fails.append((p, repr(e)))
            st[ext + (' round-trip identical' if ok else ' FAILED')] += 1
            if not ok and (not fails or fails[-1][0] != p):
                fails.append((p, 'differs'))
    for k in sorted(st):
        print('%-45s %d' % (k, st[k]))
    for p, e in fails[:20]:
        print('FAIL', p, e)
    return not fails


def _dump(path):
    for kl in read(path):
        ks = kl.keys
        print('frame %3d target %4d type %2d %-15s keys %4d  first %s' % (
            kl.frame, kl.target, kl.type, TYPE_NAMES.get(kl.type, '?'), len(ks),
            (ks[0][0], tuple(round(x, 4) for x in ks[0][1])) if ks else ''))


if __name__ == '__main__':
    if len(sys.argv) >= 3 and sys.argv[1] == 'check':
        sys.exit(0 if _check(sys.argv[2]) else 1)
    elif len(sys.argv) >= 3 and sys.argv[1] == 'dump':
        _dump(sys.argv[2])
    else:
        print(__doc__)
