# Makes Brother Gill, the leader of the Spinning Cult, as one would by hand in Blender: an armature
# of eighteen bones, a skinned robe with a deep pointed hood, long sleeves and thin pale hands, a
# black void inside the hood with two pale eyes (a rigid mesh on the head bone, its frame named
# "eyes--c0" so the game draws it unlit), a 256x256 picture painted here, and six actions at 30
# frames a second: idle, beckon, raise, spin, turn_head and vanish. vanish fades both meshes out
# with the shape keys c2_fade_0 (material 0's fade: 0 opaque, 1 invisible) and hides them on its
# last frame with c2_visible (below 1 hidden). Nothing of the game's is used. Exports glTF Separate
# into model/ beside this script (or to the path given after --):
#
#   blender --background --factory-startup --python docs/coding/examples/spinning-cult/make_gill.py
import bpy, bmesh, sys, math, os, random
import numpy as np
from mathutils import Vector, Matrix

here = os.path.dirname(os.path.abspath(__file__))
given = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
out = (given[0] if given else os.path.join(here, "model", "gill.gltf")).replace("\\", "/")
os.makedirs(os.path.dirname(out), exist_ok=True)
bpy.ops.wm.read_factory_settings(use_empty=True)
scene = bpy.context.scene
scene.render.fps = 30
scene.frame_start = 1
scene.frame_end = 121
rng = random.Random(7)
SIDES = (("L", 1.0), ("R", -1.0))  # the figure's left is +X; it faces -Y (glTF +Z)

# ---------------------------------------------------------------- skeleton (Blender Z up)
arm_data = bpy.data.armatures.new("GillRig")
rig = bpy.data.objects.new("Gill", arm_data)
scene.collection.objects.link(rig)
bpy.context.view_layer.objects.active = rig
bpy.ops.object.mode_set(mode='EDIT')
order = []
def bone(name, head, tail, parent=None):
    b = arm_data.edit_bones.new(name)
    b.head, b.tail = Vector(head), Vector(tail)
    if parent:
        b.parent = arm_data.edit_bones[parent]
        b.use_connect = False
    order.append(name)
bone("root", (0, 0, 0), (0, 0, 3))
bone("hips", (0, 0, 13), (0, 0, 17), "root")
bone("spine", (0, 0, 17), (0, 0.1, 22), "hips")
bone("chest", (0, 0.1, 22), (0, 0.2, 26.4), "spine")
bone("neck", (0, 0.2, 26.4), (0, -0.2, 28.4), "chest")
bone("head", (0, -0.2, 28.4), (0, -0.3, 32.4), "neck")
JOINTS = {  # left side; the right is mirrored in x
    "shoulder": ((1.0, 0.2, 25.2), (3.0, 0.25, 24.8)),
    "upper_arm": ((3.0, 0.25, 24.8), (3.8, 0.1, 19.2)),
    "forearm": ((3.8, 0.1, 19.2), (3.95, -1.4, 14.2)),
    "hand": ((3.95, -1.4, 14.2), (3.95, -1.7, 12.7)),
    "fingers1": ((3.95, -1.7, 12.7), (3.95, -1.8, 10.6)),
    "fingers2": ((3.95, -1.8, 10.6), (3.95, -1.8, 8.8)),
}
CHAIN = ["shoulder", "upper_arm", "forearm", "hand", "fingers1", "fingers2"]
for side, s in SIDES:
    parent = "chest"
    for j in CHAIN:
        h, t = JOINTS[j]
        bone(f"{j}_{side}", (h[0] * s, h[1], h[2]), (t[0] * s, t[1], t[2]), parent)
        parent = f"{j}_{side}"
bpy.ops.object.mode_set(mode='OBJECT')
def joint(name, end=0):
    b = arm_data.bones[name]
    return Vector(b.head_local if end == 0 else b.tail_local)

# ---------------------------------------------------------------- texture regions (u0, v0, u1, v1)
TEX = 256
R_ROBE = (0.0, 0.0, 0.5, 1.0)
R_HOOD = (0.5, 0.5, 0.75, 1.0)
R_SLEEVE = (0.75, 0.0, 1.0, 0.5)
R_EMBLEM = (0.5, 0.0, 0.75, 0.25)
R_BELT = (0.5, 0.25, 0.75, 0.375)
R_BLACK = (0.75, 0.875, 0.875, 1.0)
R_SKIN = (0.875, 0.875, 1.0, 1.0)
R_EYE = (0.75, 0.75, 0.875, 0.875)
def in_region(r, u, v):
    """A point (u, v in 0..1) of a region, kept a texel inside it."""
    pad = 1.5 / TEX
    return (r[0] + pad + (r[2] - r[0] - 2 * pad) * u, r[1] + pad + (r[3] - r[1] - 2 * pad) * v)

# ---------------------------------------------------------------- mesh building
class Builder:
    def __init__(self, name, groups):
        self.me = bpy.data.meshes.new(name)
        self.obj = bpy.data.objects.new(name, self.me)
        scene.collection.objects.link(self.obj)
        self.bm = bmesh.new()
        self.uv = self.bm.loops.layers.uv.new("UVMap")
        self.deform = self.bm.verts.layers.deform.new()
        self.groups = groups
        for g in groups:
            self.obj.vertex_groups.new(name=g)

    def vert(self, p, weights):
        v = self.bm.verts.new(Vector(p))
        total = sum(weights.values())
        for g, w in weights.items():
            if w > 0:
                v[self.deform][self.groups.index(g)] = w / total
        return v

    def face(self, verts, uvs, hint, smooth=True):
        """A face whose normal is turned to point along `hint`."""
        pts = [v.co for v in verts]
        n = Vector((0, 0, 0))
        for i in range(len(pts)):
            a, b = pts[i], pts[(i + 1) % len(pts)]
            n += Vector(((a.y - b.y) * (a.z + b.z), (a.z - b.z) * (a.x + b.x), (a.x - b.x) * (a.y + b.y)))
        if n.dot(Vector(hint)) < 0:
            verts, uvs = verts[::-1], uvs[::-1]
        try:
            f = self.bm.faces.new(verts)
        except ValueError:
            return None
        for loop, uv in zip(f.loops, uvs):
            loop[self.uv].uv = uv
        f.smooth = smooth
        return f

    def finish(self):
        self.bm.normal_update()
        self.bm.to_mesh(self.me)
        self.bm.free()

def ellipse(c, rx, ry, cy, z, n, jitter=None):
    """n points round a ring at height z, from the back round through the front (-Y)."""
    out = []
    for i in range(n):
        th = -math.pi + (i + 0.5) * 2 * math.pi / n
        dz, dr = jitter(i) if jitter else (0.0, 0.0)
        out.append(Vector((c[0] + (rx + dr) * math.sin(th), c[1] + cy - (ry + dr) * math.cos(th), z + dz)))
    return out

def loft(b, rings, weights, region, v_range=(0.0, 1.0), axis=None, inward=False, skip=None, closed=True):
    """Quads between consecutive rings (lists of points), UVs round the region (u) and up it (v).
    weights[k] are ring k's vertices' weights; `axis(k)` the ring's centre for the normal hint.
    skip(k, i) leaves out the quad between rings k and k+1 from point i to i+1."""
    verts = [[b.vert(p, weights[k]) for p in ring] for k, ring in enumerate(rings)]
    n = len(rings[0])
    for k in range(len(rings) - 1):
        for i in range(n if closed else n - 1):
            if skip and skip(k, i):
                continue
            j = (i + 1) % n
            quad = [verts[k][i], verts[k][j], verts[k + 1][j], verts[k + 1][i]]
            v0 = v_range[0] + (v_range[1] - v_range[0]) * k / (len(rings) - 1)
            v1 = v_range[0] + (v_range[1] - v_range[0]) * (k + 1) / (len(rings) - 1)
            uvs = [in_region(region, i / n, v0), in_region(region, (i + 1) / n, v0),
                   in_region(region, (i + 1) / n, v1), in_region(region, i / n, v1)]
            mid = sum((v.co for v in quad), Vector()) / 4
            centre = axis(k) if axis else Vector((0, 0, mid.z))
            hint = mid - centre
            if inward:
                hint = -hint
            b.face(quad, uvs, hint)
    return verts

def fan(b, ring_verts, apex, weights, region, hint):
    tip = b.vert(apex, weights)
    for i in range(len(ring_verts)):
        j = (i + 1) % len(ring_verts)
        b.face([ring_verts[i], ring_verts[j], tip],
               [in_region(region, 0.2, 0.2), in_region(region, 0.8, 0.2), in_region(region, 0.5, 0.8)], hint)
    return tip

groups = order[:]
body = Builder("robe", groups)
SEG = 16

# the robe: a long tapering tube flaring to a ragged hem at the floor, closed at the neck
def hem(i):
    return ((0.0 if i % 2 else 0.55) + rng.uniform(-0.1, 0.1), rng.uniform(-0.25, 0.2))
ROBE = [  # z, rx, ry, cy, weights
    (0.0, 6.0, 5.2, 0.4, {"hips": 1}),
    (2.6, 5.2, 4.5, 0.35, {"hips": 1}),
    (6.8, 4.35, 3.7, 0.3, {"hips": 1}),
    (11.0, 3.7, 3.05, 0.25, {"hips": 1}),
    (15.0, 3.3, 2.65, 0.2, {"hips": 0.7, "spine": 0.3}),
    (17.0, 3.15, 2.5, 0.15, {"hips": 0.4, "spine": 0.6}),
    (20.0, 3.3, 2.45, 0.15, {"spine": 0.7, "chest": 0.3}),
    (23.4, 3.7, 2.45, 0.2, {"spine": 0.2, "chest": 0.8}),
    (25.6, 3.2, 2.3, 0.25, {"chest": 1}),
    (27.0, 1.3, 1.3, 0.1, {"chest": 0.5, "neck": 0.5}),
]
robe_rings = [ellipse((0, 0), rx, ry, cy, z, SEG, hem if k == 0 else None) for k, (z, rx, ry, cy, _) in enumerate(ROBE)]
robe_v = loft(body, robe_rings, [r[4] for r in ROBE], R_ROBE)
fan(body, robe_v[-1], (0, 0.1, 27.3), {"chest": 0.5, "neck": 0.5}, R_BLACK, (0, 0, 1))
fan(body, robe_v[0], (0, 0.4, 0.3), {"hips": 1}, R_BLACK, (0, 0, -1))

# a cord belt at the waist and its end hanging at the front
belt = [ellipse((0, 0), 3.32 + d, 2.66 + d, 0.15, z, SEG) for z, d in ((16.5, 0.0), (17.0, 0.12), (17.5, 0.0))]
loft(body, belt, [{"hips": 0.5, "spine": 0.5}] * 3, R_BELT)
for k, (x, z0, z1) in enumerate(((0.9, 16.8, 10.5), (1.45, 16.8, 12.2))):
    y = -2.62 + 0.2 * k
    seg = [[Vector((x + dx, y - 0.02 * k, z)) for dx in (-0.16, 0.16)] for z in (z0, (z0 + z1) / 2, z1)]
    wts = [{"hips": 0.5, "spine": 0.5}, {"hips": 1}, {"hips": 1}]
    vs = [[body.vert(p, wts[r]) for p in row] for r, row in enumerate(seg)]
    for r in range(2):
        body.face([vs[r][0], vs[r][1], vs[r + 1][1], vs[r + 1][0]],
                  [in_region(R_BELT, 0, r / 2), in_region(R_BELT, 0.3, r / 2), in_region(R_BELT, 0.3, (r + 1) / 2), in_region(R_BELT, 0, (r + 1) / 2)],
                  (0, -1, 0), smooth=False)

# the capelet over the shoulders, on the chest, closed round the neck under the hood
CAPE = [  # z, rx, ry, cy
    (21.0, 4.75, 3.65, 0.3),
    (23.6, 4.0, 3.1, 0.3),
    (25.3, 3.15, 2.75, 0.25),
    (26.4, 2.2, 2.15, 0.1),
]
def ragged(i):
    return ((0.0 if i % 2 else 0.45) + rng.uniform(-0.12, 0.12), rng.uniform(-0.15, 0.1))
cape_rings = [ellipse((0, 0), rx, ry, cy, z, SEG, ragged if k == 0 else None) for k, (z, rx, ry, cy) in enumerate(CAPE)]
cape_v = loft(body, cape_rings, [{"chest": 1}] * len(CAPE), R_HOOD, v_range=(0.0, 0.3))
fan(body, cape_v[-1], (0, 0.1, 26.7), {"chest": 1}, R_BLACK, (0, 0, 1))

# the hood: rings round the head, leaning back to a point, all on the head bone (so it turns whole,
# its round foot over the capelet's collar); a pointed almond of a face opening at the front, each
# ring through it running round the back from one side of the opening to the other; lined in black
HOOD = [  # z, rx, ry, cy, half the opening's angle (0: closed)
    (26.0, 2.95, 2.95, -0.1, 0),
    (27.1, 3.0, 3.1, -0.2, 0),
    (28.2, 2.95, 3.35, -0.25, 40),
    (29.4, 2.85, 3.45, -0.25, 56),
    (30.6, 2.6, 3.3, -0.1, 50),
    (31.6, 2.2, 2.9, 0.2, 26),
    (32.5, 1.6, 2.2, 0.75, 0),
    (33.4, 0.8, 1.1, 1.7, 0),
]
HOOD_TIP = (0, 3.0, 34.2)
N = SEG
def hood_ring(z, rx, ry, cy, alpha, scale=1.0):
    a = math.radians(alpha)
    pts = []
    for i in range(N + 1):
        phi = a + (2 * math.pi - 2 * a) * i / N
        pts.append(Vector((rx * scale * math.sin(phi), cy - ry * scale * math.cos(phi), z)))
    return pts
def open_loft(rings, alphas, region, inward):
    """Quads between rings of N + 1 points running round from one side of the opening to the
    other; a closed ring's ends are one vertex."""
    verts = []
    for pts, alpha in zip(rings, alphas):
        vs = [body.vert(p, {"head": 1}) for p in pts[:N]]
        vs.append(vs[0] if alpha == 0 else body.vert(pts[N], {"head": 1}))
        verts.append(vs)
    for k in range(len(rings) - 1):
        for i in range(N):
            quad = [verts[k][i], verts[k][i + 1], verts[k + 1][i + 1], verts[k + 1][i]]
            v0, v1 = 0.3 + 0.7 * k / (len(rings) - 1), 0.3 + 0.7 * (k + 1) / (len(rings) - 1)
            uvs = [in_region(region, i / N, v0), in_region(region, (i + 1) / N, v0),
                   in_region(region, (i + 1) / N, v1), in_region(region, i / N, v1)]
            mid = sum((v.co for v in quad), Vector()) / 4
            hint = mid - Vector((0, HOOD[k][3], mid.z))
            body.face(quad, uvs, -hint if inward else hint)
    return verts
alphas = [h[4] for h in HOOD]
outer = open_loft([hood_ring(*h) for h in HOOD], alphas, R_HOOD, False)
fan(body, outer[-1][:N], HOOD_TIP, {"head": 1}, R_HOOD, (0, 1, 0.5))
inner = open_loft([hood_ring(*h, scale=0.88) for h in HOOD[1:]], alphas[1:], R_BLACK, True)
fan(body, inner[-1][:N], (0, 2.5, 33.6), {"head": 1}, R_BLACK, (0, -1, -0.5))
# the rim of the opening: its sides joined from the outer shell to the lining, a little darker
for k in range(1, len(HOOD) - 1):
    if alphas[k] == 0 and alphas[k + 1] == 0:
        continue
    for i in (0, N):
        o1, o2, l1, l2 = outer[k][i], outer[k + 1][i], inner[k - 1][i], inner[k][i]
        mid = (o1.co + o2.co) / 2
        body.face([o1, o2, l2, l1], [in_region(R_HOOD, 0.45, 0.0), in_region(R_HOOD, 0.5, 0.0),
                                     in_region(R_HOOD, 0.5, 0.05), in_region(R_HOOD, 0.45, 0.05)],
                  Vector((-mid.x, 0, 0)))
# the void the face should be: a black egg in the hood, which turns with the head
HEAD_C, HEAD_R = Vector((0, -0.25, 29.7)), (1.9, 1.75, 2.35)
egg = []
for k in range(7):
    a = -math.pi / 2 + math.pi * k / 6
    egg.append([HEAD_C + Vector((HEAD_R[0] * math.cos(a) * math.sin(t), -HEAD_R[1] * math.cos(a) * math.cos(t), HEAD_R[2] * math.sin(a)))
                for t in (-math.pi + (i + 0.5) * 2 * math.pi / 10 for i in range(10))])
egg_v = loft(body, egg[1:-1], [{"head": 1}] * 5, R_BLACK, axis=lambda k: HEAD_C)
fan(body, egg_v[0], HEAD_C - Vector((0, 0, HEAD_R[2])), {"head": 1}, R_BLACK, (0, 0, -1))
fan(body, egg_v[-1], HEAD_C + Vector((0, 0, HEAD_R[2])), {"head": 1}, R_BLACK, (0, 0, 1))

# sleeves: long bells along the arms, black inside, and thin pale hands with long fingers
def basis(d):
    ref = Vector((0, 1, 0)) if abs(d.y) < 0.9 else Vector((1, 0, 0))
    u = (ref - d * ref.dot(d)).normalized()
    return u, d.cross(u)
def ring_round(c, d, rx, ry, n):
    u, v = basis(d)
    return [c + u * (rx * math.cos(2 * math.pi * i / n)) + v * (ry * math.sin(2 * math.pi * i / n)) for i in range(n)]
for side, s in SIDES:
    sh, ua, fa, hd = (f"{j}_{side}" for j in ("shoulder", "upper_arm", "forearm", "hand"))
    p0, p1, p2 = joint(ua), joint(fa), joint(fa, 1)
    d_up, d_fa = (p1 - p0).normalized(), (p2 - p1).normalized()
    d_el = (d_up + d_fa).normalized()
    wrist = p2 + d_fa * 0.55
    SLEEVE = [  # centre, direction, radius, weights
        (p0 + d_up * 0.5, d_up, 0.95, {sh: 0.5, ua: 0.5}),
        (p0.lerp(p1, 0.45), d_up, 1.3, {ua: 1}),
        (p1, d_el, 1.25, {ua: 0.5, fa: 0.5}),
        (p1.lerp(p2, 0.55), d_fa, 1.45, {fa: 1}),
        (wrist, d_fa, 2.05, {fa: 1}),
    ]
    rings = [ring_round(c, d, r, r * 0.9, 10) for c, d, r, _ in SLEEVE]
    sv = loft(body, rings, [x[3] for x in SLEEVE], R_SLEEVE, axis=lambda k: SLEEVE[k][0])
    # inside the bell: a black funnel back into the sleeve, the hand coming out of it
    inside = [ring_round(wrist - d_fa * 0.05, d_fa, 1.85, 1.66, 10), ring_round(p1.lerp(p2, 0.75), d_fa, 0.55, 0.5, 10)]
    iv = [[body.vert(p, {fa: 1}) for p in r] for r in inside]
    for i in range(10):
        j = (i + 1) % 10
        body.face([sv[-1][i], sv[-1][j], iv[0][j], iv[0][i]], [in_region(R_SLEEVE, i / 10, 0.98)] * 2 + [in_region(R_BLACK, 0.5, 0.5)] * 2, d_fa)
        mid = (iv[0][i].co + iv[1][i].co) / 2
        axis_pt = p1.lerp(p2, 0.9)
        body.face([iv[0][i], iv[0][j], iv[1][j], iv[1][i]], [in_region(R_BLACK, 0.5, 0.5)] * 4, -(mid - axis_pt - d_fa * d_fa.dot(mid - axis_pt)))
    fan(body, iv[1], p1.lerp(p2, 0.6), {fa: 1}, R_BLACK, d_fa)

    # the hand: palm facing the body, thumb to the front, four long fingers; a box per part
    f1, f2 = f"fingers1_{side}", f"fingers2_{side}"
    w0, k0 = joint(hd), joint(hd, 1)
    d = (k0 - w0).normalized()
    n_palm = Vector((-s, 0, 0))
    wdir = d.cross(n_palm).normalized()
    if wdir.y > 0:
        wdir = -wdir
    n_palm = wdir.cross(d).normalized() * (1 if wdir.cross(d).dot(Vector((-s, 0, 0))) > 0 else -1)
    def box_chain(points, halfs, weights, region, pointed=False):
        """A four-sided tube through points (half width along wdir, half thickness along n_palm)."""
        rings = []
        for k, (p, (hw, ht)) in enumerate(zip(points, halfs)):
            rings.append([p + wdir * (hw * a) + n_palm * (ht * b) for a, b in ((1, 1), (-1, 1), (-1, -1), (1, -1))])
        vs = loft(body, rings, weights, region, axis=lambda k: points[k])
        mean = sum(points, Vector()) / len(points)
        a_dir = (points[-1] - points[0]).normalized()
        fan(body, vs[0], points[0] - a_dir * 0.05, weights[0], region, -a_dir)
        if pointed:
            fan(body, vs[-1], points[-1] + a_dir * 0.35, weights[-1], region, a_dir)
        else:
            fan(body, vs[-1], points[-1] + a_dir * 0.05, weights[-1], region, a_dir)
    palm_len = (k0 - w0).length
    box_chain([w0 - d * 0.5, w0 + d * (palm_len * 0.5), k0], [(0.36, 0.17), (0.5, 0.17), (0.48, 0.14)],
              [{fa: 0.3, hd: 0.7}, {hd: 1}, {hd: 0.7, f1: 0.3}], R_SKIN)
    j1, j2 = joint(f1, 1), joint(f2, 1)
    for k, (off, scale) in enumerate(((-0.36, 0.82), (-0.12, 1.0), (0.12, 0.96), (0.36, 0.8))):
        base = k0 + wdir * off
        knuckle = base + (j1 - k0) * scale + wdir * (off * 0.7)
        tip = knuckle + (j2 - j1) * scale + wdir * (off * 0.5)
        box_chain([base, base.lerp(knuckle, 0.5), knuckle, knuckle.lerp(tip, 0.55), tip],
                  [(0.11, 0.1), (0.09, 0.085), (0.09, 0.08), (0.07, 0.065), (0.045, 0.04)],
                  [{hd: 0.4, f1: 0.6}, {f1: 1}, {f1: 0.5, f2: 0.5}, {f2: 1}, {f2: 1}], R_SKIN, pointed=True)
    t0 = w0 + d * 0.45 + wdir * 0.42
    t_dir = (d * 0.75 + wdir * 0.5 + n_palm * 0.35).normalized()
    box_chain([t0, t0 + t_dir * 0.9, t0 + t_dir * 1.7], [(0.1, 0.1), (0.085, 0.08), (0.05, 0.05)],
              [{hd: 1}, {hd: 1}, {hd: 1}], R_SKIN, pointed=True)

# the fish emblem on the chest: a disc laid on the robe's front, painted in the picture
EMB_C, EMB_R = (0.0, 19.3), 1.35
def robe_front_y(x, z):
    """The robe's surface in front at (x, z), from its rings' ellipses."""
    for k in range(len(ROBE) - 1):
        za, zb = ROBE[k][0], ROBE[k + 1][0]
        if za <= z <= zb:
            t = (z - za) / (zb - za)
            rx = ROBE[k][1] + (ROBE[k + 1][1] - ROBE[k][1]) * t
            ry = ROBE[k][2] + (ROBE[k + 1][2] - ROBE[k][2]) * t
            cy = ROBE[k][3] + (ROBE[k + 1][3] - ROBE[k][3]) * t
            return cy - ry * math.sqrt(max(0.0, 1 - (x / rx) ** 2))
    return -2.4
emb_pts = [(0.0, 0.0)] + [(math.cos(2 * math.pi * i / 12), math.sin(2 * math.pi * i / 12)) for i in range(12)]
emb_v = []
for a, b in emb_pts:
    x, z = EMB_C[0] + a * EMB_R, EMB_C[1] + b * EMB_R
    w = {}
    for k in range(len(ROBE) - 1):
        za, zb = ROBE[k][0], ROBE[k + 1][0]
        if za <= z <= zb:
            t = (z - za) / (zb - za)
            for g, wt in ROBE[k][4].items():
                w[g] = w.get(g, 0) + wt * (1 - t)
            for g, wt in ROBE[k + 1][4].items():
                w[g] = w.get(g, 0) + wt * t
            break
    emb_v.append((body.vert((x, robe_front_y(x, z) - 0.07, z), w), in_region(R_EMBLEM, 0.5 + a * 0.5, 0.5 + b * 0.5)))
for i in range(12):
    j = 1 + (i + 1) % 12
    body.face([emb_v[0][0], emb_v[1 + i][0], emb_v[j][0]], [emb_v[0][1], emb_v[1 + i][1], emb_v[j][1]], (0, -1, 0))
body.finish()

# the eyes: two small round discs on the black egg, a little too far apart, their own rigid mesh
# on the head bone, named with the flag c0 (no lighting)
eyes = Builder("eyes--c0", ["head"])
for sx in (1.0, -1.0):
    x, z, r = 1.2 * sx, 29.85, 0.3
    yy = HEAD_C.y - HEAD_R[1] * math.sqrt(max(0.0, 1 - (x / HEAD_R[0]) ** 2 - ((z - HEAD_C.z) / HEAD_R[2]) ** 2)) - 0.04
    n = Vector((x / HEAD_R[0] ** 2, (yy - HEAD_C.y) / HEAD_R[1] ** 2, 0)).normalized()
    u = Vector((0, 0, 1)).cross(n).normalized()
    v = n.cross(u)
    centre = Vector((x, yy, z))
    c = eyes.vert(centre, {"head": 1})
    rim = [eyes.vert(centre + u * (r * math.cos(2 * math.pi * i / 10)) + v * (r * math.sin(2 * math.pi * i / 10)), {"head": 1}) for i in range(10)]
    for i in range(10):
        j = (i + 1) % 10
        eyes.face([c, rim[i], rim[j]], [in_region(R_EYE, 0.5, 0.5),
                  in_region(R_EYE, 0.5 + 0.5 * math.cos(2 * math.pi * i / 10), 0.5 + 0.5 * math.sin(2 * math.pi * i / 10)),
                  in_region(R_EYE, 0.5 + 0.5 * math.cos(2 * math.pi * j / 10), 0.5 + 0.5 * math.sin(2 * math.pi * j / 10))], n, smooth=False)
eyes.finish()

body.obj.parent = rig
mod = body.obj.modifiers.new("Armature", 'ARMATURE')
mod.object = rig
eyes.obj.parent = rig
eyes.obj.parent_type = 'BONE'
eyes.obj.parent_bone = "head"
bpy.context.view_layer.update()
eyes.obj.matrix_world = Matrix.Identity(4)

# ---------------------------------------------------------------- the picture
def noise(size, cells, seed):
    g = np.random.default_rng(seed).random((cells + 1, cells + 1))
    t = np.linspace(0, cells, size, endpoint=False)
    i = t.astype(int)
    f = t - i
    f = f * f * (3 - 2 * f)
    a = g[i][:, i] * (1 - f)[None, :] + g[i][:, i + 1] * f[None, :]
    b = g[i + 1][:, i] * (1 - f)[None, :] + g[i + 1][:, i + 1] * f[None, :]
    return a * (1 - f)[:, None] + b * f[:, None]
def fbm(size, seed):
    return 0.5 * noise(size, 4, seed) + 0.3 * noise(size, 12, seed + 1) + 0.2 * noise(size, 40, seed + 2)
px = np.zeros((TEX, TEX, 3))
grime = fbm(TEX, 3)
speck = noise(TEX, 96, 9)
yy, xx = np.mgrid[0:TEX, 0:TEX] / TEX  # yy is v (0 at the bottom), xx is u
cloth = np.array([0.215, 0.245, 0.205])
# robe: folds running down it (darker stripes), grime and dirt rising from the hem
u_r = xx / 0.5
folds = 0.5 + 0.5 * np.sin(u_r * 2 * math.pi * 8 + 1.3 * np.sin(yy * 9)) * (0.6 + 0.4 * yy)
shade = 0.78 + 0.22 * folds + 0.25 * (grime - 0.5) + 0.08 * (speck - 0.5)
dirt = np.clip(1 - yy / 0.32, 0, 1) ** 1.5
robe_col = cloth[None, None, :] * shade[..., None]
robe_col = robe_col * (1 - 0.45 * dirt[..., None]) + np.array([0.13, 0.11, 0.08]) * 0.45 * dirt[..., None]
px[:] = robe_col
def region_mask(r):
    return (xx >= r[0]) & (xx < r[2]) & (yy >= r[1]) & (yy < r[3])
# hood and sleeves: the same cloth, a shade darker, the sleeves' cuffs dirtier
m = region_mask(R_HOOD)
px[m] = (cloth * 0.88)[None, :] * (0.82 + 0.3 * (grime[m] - 0.5) + 0.1 * folds[m])[:, None]
m = region_mask(R_SLEEVE)
cuff = np.clip((yy - 0.38) / 0.12, 0, 1)
px[m] = (cloth * 0.95)[None, :] * (0.8 + 0.3 * (grime[m] - 0.5) + 0.12 * folds[m])[:, None] * (1 - 0.35 * cuff[m])[:, None]
# the belt: twisted cord
m = region_mask(R_BELT)
twist = 0.5 + 0.5 * np.sin((xx + yy * 1.7) * 2 * math.pi * 40)
px[m] = np.array([0.2, 0.17, 0.12])[None, :] * (0.7 + 0.35 * twist[m] + 0.2 * (grime[m] - 0.5))[:, None]
# black: the hood's inside, the void, the sleeves' insides
px[region_mask(R_BLACK)] = 0.0
# the skin: pale, grey-green, a little mottled
m = region_mask(R_SKIN)
px[m] = np.array([0.7, 0.71, 0.63])[None, :] * (0.92 + 0.16 * (speck[m] - 0.5))[:, None]
# the eyes: pale yellow-white, brightest in the middle
m = region_mask(R_EYE)
eu, ev = (xx - 0.8125) / 0.0625, (yy - 0.8125) / 0.0625
er = np.sqrt(eu ** 2 + ev ** 2)
eye = np.array([1.0, 0.98, 0.84])[None, None, :] * (1 - 0.1 * np.clip(er, 0, 1) ** 2)[..., None]
px[m] = eye[m]
# the emblem: a worn pale disc with a fish drawn in the cloth's dark green
m = region_mask(R_EMBLEM)
qx, qy = (xx - 0.625) / 0.125, (yy - 0.125) / 0.125  # -1..1 across the emblem
rr = np.sqrt(qx ** 2 + qy ** 2)
disc = np.clip((0.95 - rr) / 0.04, 0, 1)
ring = np.clip(1 - np.abs(rr - 0.8) / 0.045, 0, 1)
bx, by = (qx + 0.12) / 0.45, qy / 0.25
body_d = (np.sqrt(bx ** 2 + by ** 2) - 1) * 0.25
tail = np.maximum(np.abs(qy) - (qx - 0.28) * 0.85, qx - 0.62)
tail = np.maximum(tail, 0.28 - qx)
fish_line = np.clip(1 - np.abs(body_d) / 0.035, 0, 1)
tail_line = np.clip(1 - np.abs(tail) / 0.035, 0, 1) * (qx > 0.27)
eye_dot = np.clip(1 - (np.sqrt((qx + 0.38) ** 2 + (qy - 0.05) ** 2) - 0.03) / 0.03, 0, 1)
ink = np.clip(fish_line + tail_line + eye_dot + ring, 0, 1)
wear = np.clip(0.55 + 0.9 * (grime - 0.5) + 0.25 * (speck - 0.5), 0.15, 0.9)
pale = np.array([0.66, 0.64, 0.54])
emb = robe_col * (1 - (disc * wear)[..., None]) + pale[None, None, :] * (disc * wear)[..., None]
emb = emb * (1 - 0.75 * (ink * disc)[..., None]) + np.array([0.12, 0.15, 0.12]) * 0.75 * (ink * disc)[..., None]
px[m] = emb[m]
img = bpy.data.images.new("gill_skin", TEX, TEX)
rgba = np.concatenate([np.clip(px, 0, 1), np.ones((TEX, TEX, 1))], axis=2)
img.pixels.foreach_set(rgba.astype(np.float32).ravel())
img.filepath_raw = out.rsplit("/", 1)[0] + "/gill_skin.png"
img.file_format = 'PNG'
img.save()
mat = bpy.data.materials.new("gill_skin")
tex = mat.node_tree.nodes.new("ShaderNodeTexImage")
tex.image = img
bsdf = mat.node_tree.nodes["Principled BSDF"]
mat.node_tree.links.new(tex.outputs["Color"], bsdf.inputs["Base Color"])
bsdf.inputs["Roughness"].default_value = 1.0
body.me.materials.append(mat)
eyes.me.materials.append(mat)

# ---------------------------------------------------------------- carrier shape keys
for ob in (body.obj, eyes.obj):
    ob.shape_key_add(name="Basis", from_mix=False)
    ob.shape_key_add(name="c2_fade_0", from_mix=False).value = 0.0
    ob.shape_key_add(name="c2_visible", from_mix=False).value = 1.0

# ---------------------------------------------------------------- motions
P = rig.pose.bones
for b in P:
    b.rotation_mode = 'QUATERNION'
def rot(axis, degrees):
    return Matrix.Rotation(math.radians(degrees), 3, axis)
def smooth(a, b, t):
    x = min(1.0, max(0.0, (t - a) / (b - a)))
    return x * x * (3 - 2 * x)
def curl(s, a):
    """Fingers bent towards the palm (which faces the body at rest), in rest axes."""
    return rot('Y', s * a)

def pose_quaternions(spec):
    """Each bone's local rotation for a pose: spec[bone] = ('rel', R), R a rotation about rest
    (world) axes after the parent's, or ('abs', R), the bone's whole rotation from rest."""
    for b in P:
        b.rotation_quaternion = (1, 0, 0, 0)
        b.location = (0, 0, 0)
    bpy.context.view_layer.update()
    for name in order:
        if name not in spec:
            continue
        pb = P[name]
        kind, R = spec[name]
        rest = pb.bone.matrix_local.to_3x3()
        if kind == 'rel' and pb.parent:
            delta = pb.parent.matrix.to_3x3() @ pb.parent.bone.matrix_local.to_3x3().inverted()
            want = delta @ R @ rest
        else:
            want = R @ rest
        m = want.to_4x4()
        m.translation = pb.matrix.translation
        pb.matrix = m
        pb.location = (0, 0, 0)
        bpy.context.view_layer.update()
    return {name: P[name].rotation_quaternion.copy() for name in order}

def idle(t):
    p = 2 * math.pi * t / 4.0
    breath = math.sin(p)
    tilt = 0.5 - 0.5 * math.cos(p)
    spec = {
        "spine": ('rel', rot('X', -0.6 * breath)),
        "chest": ('rel', rot('X', -1.4 * breath)),
        "neck": ('rel', rot('Y', -3 * tilt) @ rot('X', 3)),
        "head": ('rel', rot('Y', -10 * tilt) @ rot('X', 2 * tilt)),
    }
    for side, s in SIDES:
        spec[f"shoulder_{side}"] = ('rel', rot('Y', -s * 1.5 * breath))
        spec[f"upper_arm_{side}"] = ('rel', rot('X', 1.0 * breath))
        spec[f"hand_{side}"] = ('rel', rot('Z', s * 40))
        spec[f"fingers1_{side}"] = ('rel', curl(s, 8 + 5 * math.sin(p + s)))
        spec[f"fingers2_{side}"] = ('rel', curl(s, 10 + 6 * math.sin(p + s)))
    return spec

def beckon(t):
    e = smooth(0.0, 1.15, t)
    c = (smooth(1.1, 1.45, t) - 0.75 * smooth(1.5, 1.75, t) + 0.75 * smooth(1.8, 2.15, t)
         - 0.75 * smooth(2.2, 2.45, t) + 0.75 * smooth(2.5, 2.85, t))
    spec = idle(0.0)
    spec.update({
        "spine": ('rel', rot('X', 3 * e)),
        "head": ('rel', rot('X', 7 * e) @ rot('Y', 6 * e)),
        "upper_arm_R": ('rel', rot('Z', 16 * e) @ rot('X', -50 * e)),
        "forearm_R": ('rel', rot('X', -42 * e)),
        "hand_R": ('rel', rot('X', -12 * e) @ rot('Z', -40 - 48 * e)),
        "fingers1_R": ('rel', curl(-1, 8 + 50 * c)),
        "fingers2_R": ('rel', curl(-1, 10 + 62 * c)),
    })
    return spec

def raise_(t):
    e = smooth(0.0, 2.7, t)
    tremble = 3 * smooth(2.4, 3.0, t) * math.sin(t * 11.0) * math.sin(t * 4.3)
    spec = {
        "spine": ('rel', rot('X', -3 * e)),
        "chest": ('rel', rot('X', -5 * e)),
        "neck": ('rel', rot('X', -6 * e)),
        "head": ('rel', rot('X', -14 * e)),
    }
    for side, s in SIDES:
        spec[f"shoulder_{side}"] = ('rel', rot('Y', -s * 18 * e))
        spec[f"upper_arm_{side}"] = ('rel', rot('Y', s * 22 * e) @ rot('X', -150 * e))
        spec[f"forearm_{side}"] = ('rel', rot('Y', -s * 14 * e))
        spec[f"hand_{side}"] = ('rel', rot('Y', -s * 80 * e) @ rot('Z', s * 40 * (1 - e)))
        spec[f"fingers1_{side}"] = ('rel', curl(s, 8 + 12 * e + tremble * s))
        spec[f"fingers2_{side}"] = ('rel', curl(s, 10 + 14 * e - tremble * s))
    return spec

def spin(t):
    spec = idle(0.0)
    spec["root"] = ('abs', rot('Z', 360.0 * t / 4.0))
    return spec

def turn_head(t):
    h = smooth(0.4, 1.9, t) - smooth(2.5, 3.7, t)
    spec = idle(0.0)
    spec["neck"] = ('rel', rot('Z', 24 * h) @ rot('X', 3))
    spec["head"] = ('rel', rot('Y', -9 * h) @ rot('Z', 66 * h))
    return spec

def vanish(t):
    spec = idle(0.0)
    spec["head"] = ('rel', rot('X', -6 * smooth(0.0, 2.0, t)))
    return spec

MOTIONS = [  # name, pose(t), seconds, carriers(t) -> {mesh: (fade, visible)}
    ("idle", idle, 4.0, None),
    ("beckon", beckon, 3.0, None),
    ("raise", raise_, 4.0, None),
    ("spin", spin, 4.0, None),
    ("turn_head", turn_head, 4.0, None),
    ("vanish", vanish, 2.0, lambda t, last: {
        body.obj: (smooth(0.0, 1.6, t), 0.0 if last else 1.0),
        eyes.obj: (smooth(0.9, 1.95, t), 0.0 if last else 1.0),
    }),
]

def set_keys(action, datablock, path, index, values, interpolation='LINEAR'):
    fc = action.fcurve_ensure_for_datablock(datablock, path, index=index)
    fc.keyframe_points.add(len(values))
    fc.keyframe_points.foreach_set("co", [x for f, v in enumerate(values) for x in (1 + f, v)])
    for k in fc.keyframe_points:
        k.interpolation = interpolation
    fc.update()

# every pose first (no action assigned, so nothing animated overrides the pose set), then the keys
rig.animation_data_create()
posed = {}
for name, fn, seconds, _ in MOTIONS:
    frames = int(round(seconds * 30))
    posed[name] = [pose_quaternions(fn(f / 30.0)) for f in range(frames + 1)]
for ob in (body.obj, eyes.obj):
    ob.data.shape_keys.animation_data_create()
for name, fn, seconds, carriers in MOTIONS:
    action = bpy.data.actions.new(name)
    rig.animation_data.action = action
    frames = posed[name]
    for b in order:
        # quaternions kept on one side, frame to frame
        qs = [frames[0][b]]
        for f in frames[1:]:
            q = f[b].copy()
            if q.dot(qs[-1]) < 0:
                q.negate()
            qs.append(q)
        for i in range(4):
            set_keys(action, rig, f'pose.bones["{b}"].rotation_quaternion', i, [q[i] for q in qs])
    if carriers:
        n = len(frames)
        for ob in (body.obj, eyes.obj):
            key = ob.data.shape_keys
            key.animation_data.action = action
            values = [carriers(f / 30.0, f == n - 1)[ob] for f in range(n)]
            set_keys(action, key, 'key_blocks["c2_fade_0"].value', 0, [v[0] for v in values])
            set_keys(action, key, 'key_blocks["c2_visible"].value', 0, [v[1] for v in values], 'CONSTANT')
    action.use_fake_user = True
rig.animation_data.action = bpy.data.actions["idle"]
for b in P:
    b.rotation_quaternion = (1, 0, 0, 0)
scene.frame_set(1)

bpy.ops.export_scene.gltf(filepath=out, export_format='GLTF_SEPARATE', export_animations=True)
print("DONE", out)
