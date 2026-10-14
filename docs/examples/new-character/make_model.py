# Makes the example's character as one would by hand in Blender: an armature of seven bones, a mesh
# of boxes weighted to them, a 64x64 picture and one action, "wave", at 30 frames a second; and
# exports it as glTF Separate into model/ beside this script (or to the path given after --):
#
#   blender --background --factory-startup --python docs/examples/new-character/make_model.py
import bpy, bmesh, sys, math, os
from mathutils import Vector

here = os.path.dirname(os.path.abspath(__file__))
given = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
out = (given[0] if given else os.path.join(here, "model", "newchr.gltf")).replace("\\", "/")
os.makedirs(os.path.dirname(out), exist_ok=True)
bpy.ops.wm.read_factory_settings(use_empty=True)
scene = bpy.context.scene
scene.render.fps = 30
scene.frame_start = 1
scene.frame_end = 31

# armature: hips, spine, head, arms, legs (Blender Z up; the export makes it Y up)
arm_data = bpy.data.armatures.new("RigData")
rig = bpy.data.objects.new("Rig", arm_data)
scene.collection.objects.link(rig)
bpy.context.view_layer.objects.active = rig
bpy.ops.object.mode_set(mode='EDIT')
bones = {}
def bone(name, head, tail, parent=None):
    b = arm_data.edit_bones.new(name)
    b.head, b.tail = Vector(head), Vector(tail)
    if parent:
        b.parent = bones[parent]
        b.use_connect = False
    bones[name] = b
bone("hips", (0, 0, 10), (0, 0, 13))
bone("spine", (0, 0, 13), (0, 0, 19), "hips")
bone("head", (0, 0, 19), (0, 0, 25), "spine")
bone("arm_L", (3, 0, 18), (10, 0, 18), "spine")
bone("arm_R", (-3, 0, 18), (-10, 0, 18), "spine")
bone("leg_L", (1.5, 0, 10), (1.5, 0, 0), "hips")
bone("leg_R", (-1.5, 0, 10), (-1.5, 0, 0), "hips")
bpy.ops.object.mode_set(mode='OBJECT')

# mesh: a box per part, each part's vertices weighted to its bone, the torso's lower ring shared
# half and half by hips and spine
me = bpy.data.meshes.new("Body")
body = bpy.data.objects.new("Body", me)
scene.collection.objects.link(body)
bm = bmesh.new()
uv = bm.loops.layers.uv.new("UVMap")
deform = bm.verts.layers.deform.new()
groups = ["hips", "spine", "head", "arm_L", "arm_R", "leg_L", "leg_R"]
for g in groups:
    body.vertex_groups.new(name=g)
parts = [
    # (bone, centre, half size, uv cell)
    ("hips", (0, 0, 11.5), (2.5, 1.5, 1.5), (0, 0)),
    ("spine", (0, 0, 16), (3, 1.8, 3), (1, 0)),
    ("head", (0, 0, 22), (2.2, 2.2, 2.6), (2, 0)),
    ("arm_L", (6.5, 0, 18), (3.5, 0.8, 0.8), (3, 0)),
    ("arm_R", (-6.5, 0, 18), (3.5, 0.8, 0.8), (0, 1)),
    ("leg_L", (1.5, 0, 5), (1, 1, 5), (1, 1)),
    ("leg_R", (-1.5, 0, 5), (1, 1, 5), (2, 1)),
]
for name, c, h, cell in parts:
    vs = []
    for dz in (-1, 1):
        for dy in (-1, 1):
            for dx in (-1, 1):
                v = bm.verts.new((c[0] + dx * h[0], c[1] + dy * h[1], c[2] + dz * h[2]))
                gi = groups.index(name)
                if name == "spine" and dz < 0:
                    v[deform][gi] = 0.5
                    v[deform][groups.index("hips")] = 0.5
                else:
                    v[deform][gi] = 1.0
                vs.append(v)
    faces = [(0, 1, 3, 2), (4, 6, 7, 5), (0, 4, 5, 1), (2, 3, 7, 6), (0, 2, 6, 4), (1, 5, 7, 3)]
    for k, f in enumerate(faces):
        face = bm.faces.new([vs[i] for i in f])
        u0, v0 = cell[0] * 0.25, cell[1] * 0.25 + (0.5 if k >= 3 else 0)
        for loop, (du, dv) in zip(face.loops, [(0, 0), (0.25, 0), (0.25, 0.25), (0, 0.25)]):
            loop[uv].uv = (u0 + du * 0.9, v0 + dv * 0.9)
bm.normal_update()
bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
bm.to_mesh(me)
bm.free()
body.parent = rig
mod = body.modifiers.new("Armature", 'ARMATURE')
mod.object = rig

# a 64x64 texture: a coloured cell per part
img = bpy.data.images.new("newchr_skin", 64, 64)
px = []
colours = [(0.9, 0.3, 0.2), (0.2, 0.6, 0.9), (0.95, 0.8, 0.6), (0.3, 0.8, 0.3), (0.8, 0.8, 0.2), (0.5, 0.3, 0.8), (0.9, 0.5, 0.1), (0.2, 0.2, 0.2)]
for y in range(64):
    for x in range(64):
        k = (x // 16 + 4 * (y // 16)) % len(colours)
        r, g, b = colours[k]
        if x % 16 in (0, 15) or y % 16 in (0, 15):
            r, g, b = r * 0.5, g * 0.5, b * 0.5
        px += [r, g, b, 1.0]
img.pixels = px
img.filepath_raw = out.rsplit("/", 1)[0] + "/newchr_skin.png"
img.file_format = 'PNG'
img.save()
mat = bpy.data.materials.new("newchr_skin")
mat.use_nodes = True
tex = mat.node_tree.nodes.new("ShaderNodeTexImage")
tex.image = img
bsdf = mat.node_tree.nodes["Principled BSDF"]
mat.node_tree.links.new(tex.outputs["Color"], bsdf.inputs["Base Color"])
me.materials.append(mat)

# one motion: wave the right arm, nod the head, bob the hips
rig.animation_data_create()
action = bpy.data.actions.new("wave")
rig.animation_data.action = action
bpy.context.view_layer.objects.active = rig
bpy.ops.object.mode_set(mode='POSE')
pb = rig.pose.bones
for b in pb:
    b.rotation_mode = 'QUATERNION'
def key(frame, arm, head, hip):
    pb["arm_R"].rotation_quaternion = (math.cos(arm / 2), math.sin(arm / 2), 0, 0)
    pb["head"].rotation_quaternion = (math.cos(head / 2), math.sin(head / 2), 0, 0)
    pb["hips"].location = (0, hip, 0)
    for name in ("arm_R", "head"):
        pb[name].keyframe_insert("rotation_quaternion", frame=frame)
    pb["hips"].keyframe_insert("location", frame=frame)
key(1, 0, 0, 0)
key(8, math.radians(80), math.radians(15), 0.5)
key(16, math.radians(45), 0, 0)
key(24, math.radians(80), math.radians(-15), 0.5)
key(31, 0, 0, 0)
bpy.ops.object.mode_set(mode='OBJECT')

bpy.ops.export_scene.gltf(filepath=out, export_format='GLTF_SEPARATE', export_animations=True)
print("DONE", out)
