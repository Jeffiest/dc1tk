"""The glTF files dc1tk writes, read into Blender objects and written back.

The add-on reads the files itself (not Blender's glTF importer) so that Blender vertex i of a mesh is glTF vertex i of
its primitives in order. An edit that only moves vertices is then written into the same accessors of the same .bin,
which is what dc1tk's vertex_patch turns into a model of the same size. Everything else in the file (node names,
extras.mds_index, materials, normals, UVs) is kept as it was.

Coordinates stay in the game's space (glTF's, Y up): every imported object hangs under a root empty turned +90 degrees
on X, so the numbers in the files are the numbers Blender shows relative to the root.
"""

import json
import math
import os
import struct

import bpy
from mathutils import Matrix, Quaternion, Vector

COMPONENTS = {5120: "b", 5121: "B", 5122: "h", 5123: "H", 5125: "I", 5126: "f"}
WIDTH = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4}
SIZE = {"b": 1, "B": 1, "h": 2, "H": 2, "I": 4, "f": 4}


class Gltf:
    def __init__(self, path):
        self.path = path
        self.folder = os.path.dirname(path)
        with open(path, "rb") as handle:
            self.raw = handle.read()
        self.json = json.loads(self.raw.decode("utf-8"))
        self.buffers = []
        for buffer in self.json.get("buffers", []):
            uri = buffer.get("uri", "")
            with open(os.path.join(self.folder, uri), "rb") as handle:
                self.buffers.append(bytearray(handle.read()))

    def accessor_place(self, index):
        """(buffer index, start, stride, count, format char, width) of an accessor."""
        accessor = self.json["accessors"][index]
        view = self.json["bufferViews"][accessor["bufferView"]]
        kind = COMPONENTS[accessor["componentType"]]
        width = WIDTH[accessor["type"]]
        stride = view.get("byteStride") or SIZE[kind] * width
        start = view.get("byteOffset", 0) + accessor.get("byteOffset", 0)
        return view.get("buffer", 0), start, stride, accessor["count"], kind, width

    def read(self, index):
        buffer, start, stride, count, kind, width = self.accessor_place(index)
        data = self.buffers[buffer]
        fmt = "<%d%s" % (width, kind)
        out = [struct.unpack_from(fmt, data, start + i * stride) for i in range(count)]
        return [value[0] for value in out] if width == 1 else out

    def write_vec3(self, index, values):
        buffer, start, stride, count, kind, width = self.accessor_place(index)
        assert kind == "f" and width == 3 and count == len(values)
        data = self.buffers[buffer]
        for i, value in enumerate(values):
            struct.pack_into("<3f", data, start + i * stride, *value)
        accessor = self.json["accessors"][index]
        if values and "min" in accessor:
            accessor["min"] = [min(v[k] for v in values) for k in range(3)]
            accessor["max"] = [max(v[k] for v in values) for k in range(3)]

    def append(self, blob, kind, width, count, target=None, bounds=False, values=None):
        """Adds data to the end of buffer 0 as a new accessor; its index."""
        data = self.buffers[0]
        while len(data) % 4:
            data.append(0)
        views = self.json.setdefault("bufferViews", [])
        view = {"buffer": 0, "byteOffset": len(data), "byteLength": len(blob)}
        if target:
            view["target"] = target
        views.append(view)
        data.extend(blob)
        component = {v: k for k, v in COMPONENTS.items()}[kind]
        accessor = {"bufferView": len(views) - 1, "componentType": component, "count": count,
                    "type": {1: "SCALAR", 2: "VEC2", 3: "VEC3", 4: "VEC4"}[width]}
        if bounds and values:
            accessor["min"] = [min(v[k] for v in values) for k in range(width)]
            accessor["max"] = [max(v[k] for v in values) for k in range(width)]
        accessors = self.json.setdefault("accessors", [])
        accessors.append(accessor)
        return len(accessors) - 1

    def save(self, path=None):
        path = path or self.path
        folder = os.path.dirname(path)
        for buffer, data in zip(self.json.get("buffers", []), self.buffers):
            buffer["byteLength"] = len(data)
            with open(os.path.join(folder, buffer["uri"]), "wb") as handle:
                handle.write(bytes(data))
        with open(path, "w", encoding="utf-8", newline="\n") as handle:
            json.dump(self.json, handle, indent=2, ensure_ascii=False)


# ---- import ----

def node_matrix(node):
    if "matrix" in node:
        m = node["matrix"]
        return Matrix([m[0:4], m[4:8], m[8:12], m[12:16]]).transposed()
    t = Vector(node.get("translation", (0, 0, 0)))
    r = node.get("rotation", (0, 0, 0, 1))
    s = node.get("scale", (1, 1, 1))
    q = Quaternion((r[3], r[0], r[1], r[2]))
    return Matrix.Translation(t) @ q.to_matrix().to_4x4() @ Matrix.Diagonal((s[0], s[1], s[2], 1.0))


def flat_matrix(matrix):
    return [matrix[r][c] for r in range(4) for c in range(4)]


def unflat_matrix(values):
    return Matrix([values[0:4], values[4:8], values[8:12], values[12:16]])


def _image(gltf, index, cache):
    if index in cache:
        return cache[index]
    image = None
    entry = gltf.json.get("images", [])[index]
    path = os.path.join(gltf.folder, entry.get("uri", ""))
    if os.path.isfile(path):
        image = bpy.data.images.load(path, check_existing=True)
    cache[index] = image
    return image


def _material(gltf, index, cache, images, collision):
    if index in cache:
        return cache[index]
    entry = gltf.json["materials"][index]
    material = bpy.data.materials.new(entry.get("name", "material"))
    pbr = entry.get("pbrMetallicRoughness", {})
    color = pbr.get("baseColorFactor", [1, 1, 1, 1])
    material.diffuse_color = color
    material.use_nodes = True
    shader = material.node_tree.nodes.get("Principled BSDF")
    if shader:
        shader.inputs["Base Color"].default_value = color
        shader.inputs["Roughness"].default_value = 1.0
        texture = pbr.get("baseColorTexture")
        if texture is not None and not collision:
            source = gltf.json["textures"][texture["index"]].get("source")
            image = _image(gltf, source, images) if source is not None else None
            if image:
                node = material.node_tree.nodes.new("ShaderNodeTexImage")
                node.image = image
                node.interpolation = "Closest"
                node.location = (-300, 300)
                material.node_tree.links.new(node.outputs["Color"], shader.inputs["Base Color"])
                material.node_tree.links.new(node.outputs["Alpha"], shader.inputs["Alpha"])
    material["dc1_gltf_material"] = index
    cache[index] = material
    return material


def _mesh(gltf, index, name, materials, images, collision):
    entry = gltf.json["meshes"][index]
    positions, normals, uvs, colors, faces, face_mats, ranges = [], [], [], [], [], [], []
    slots = []
    for primitive in entry["primitives"]:
        attributes = primitive["attributes"]
        base = len(positions)
        points = gltf.read(attributes["POSITION"])
        positions.extend(points)
        count = len(points)
        normals.extend(gltf.read(attributes["NORMAL"]) if "NORMAL" in attributes else [(0, 0, 1)] * count)
        uvs.extend(gltf.read(attributes["TEXCOORD_0"]) if "TEXCOORD_0" in attributes else [(0, 0)] * count)
        if "COLOR_0" in attributes:
            colors.extend(tuple(c) + (1.0,) * (4 - len(c)) for c in gltf.read(attributes["COLOR_0"]))
        else:
            colors.extend([(1, 1, 1, 1)] * count)
        ranges.append(count)
        material = primitive.get("material")
        if material not in slots:
            slots.append(material)
        slot = slots.index(material)
        indices = gltf.read(primitive["indices"]) if "indices" in primitive else list(range(count))
        for i in range(0, len(indices) - 2, 3):
            tri = (base + indices[i], base + indices[i + 1], base + indices[i + 2])
            # Blender cannot hold a face with a repeated corner; it is left out of the view only
            if len(set(tri)) == 3:
                faces.append(tri)
                face_mats.append(slot)
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(positions, [], faces)
    for slot in slots:
        mesh.materials.append(_material(gltf, slot, materials, images, collision) if slot is not None else None)
    if faces:
        mesh.polygons.foreach_set("material_index", face_mats)
        loops = [v for face in faces for v in face]
        layer = mesh.uv_layers.new(name="UVMap")
        layer.data.foreach_set("uv", [c for v in loops for c in (uvs[v][0], 1.0 - uvs[v][1])])
        if any(c != (1, 1, 1, 1) for c in colors):
            attribute = mesh.color_attributes.new("Col", "FLOAT_COLOR", "POINT")
            attribute.data.foreach_set("color", [c for color in colors for c in color])
        try:
            mesh.normals_split_custom_set([normals[v] for v in loops])
        except (RuntimeError, ValueError):
            pass
    mesh["dc1_vertex_ranges"] = ranges
    mesh["dc1_face_count"] = len(faces)
    return mesh


def import_gltf(path, parent, collection, collision=False, hidden=False):
    """Imports one dc1tk glTF under `parent`; the top objects."""
    gltf = Gltf(path)
    materials, images, made = {}, {}, {}
    rel = os.path.basename(path)

    def make(node_index, owner):
        node = gltf.json["nodes"][node_index]
        name = node.get("name", "node%d" % node_index)
        data = None
        if "mesh" in node:
            data = _mesh(gltf, node["mesh"], name, materials, images, collision)
        obj = bpy.data.objects.new(name, data)
        if data is None:
            obj.empty_display_size = 0.5
        collection.objects.link(obj)
        obj.parent = owner
        obj.matrix_parent_inverse = Matrix.Identity(4)
        obj.matrix_basis = node_matrix(node)
        obj["dc1_file"] = path
        obj["dc1_node"] = node_index
        if "extras" in node and "mds_index" in node["extras"]:
            obj["mds_index"] = node["extras"]["mds_index"]
        if collision and data is not None:
            obj.display_type = "SOLID"
            obj.color = (1, 1, 1, 0.6)
        obj.hide_render = hidden
        made[node_index] = obj
        for child in node.get("children", []):
            make(child, obj)
        return obj

    scene = gltf.json.get("scenes", [{}])[gltf.json.get("scene", 0)]
    tops = [make(n, parent) for n in scene.get("nodes", [])]
    bpy.context.view_layer.update()
    root = top_root(parent)
    for obj in made.values():
        obj["dc1_world0"] = flat_matrix(game_matrix(obj, root))
    if hidden:
        for obj in made.values():
            obj.hide_set(True)
    return tops


def instance(tops, parent, collection):
    """Copies of imported objects sharing their meshes (a model several floor parts use). Only the first import is
    exported; editing either edits both."""
    def copy(obj, owner):
        new = obj.copy()
        for key in ("dc1_file", "dc1_world0"):
            if key in new:
                del new[key]
        new["dc1_shared"] = True
        collection.objects.link(new)
        new.parent = owner
        new.matrix_parent_inverse = Matrix.Identity(4)
        new.matrix_basis = obj.matrix_basis.copy()
        for child in obj.children:
            copy(child, new)
        return new
    return [copy(top, parent) for top in tops]


def top_root(obj):
    """The nearest owner whose space the files' numbers are in (a floor part, a scene part, the root)."""
    while obj is not None and "dc1_space" not in obj:
        obj = obj.parent
    return obj


def game_matrix(obj, root):
    """The object's matrix in game space (relative to the root)."""
    return root.matrix_world.inverted() @ obj.matrix_world if root else obj.matrix_world.copy()


# ---- export ----

def _close(a, b, eps=1e-6):
    return all(abs(a[r][c] - b[r][c]) <= eps for r in range(4) for c in range(4))


def _f32(value):
    return struct.unpack("<f", struct.pack("<f", value))[0]


def mesh_objects_of(path):
    return [o for o in bpy.data.objects if o.get("dc1_file") == path and o.type == "MESH"]


def export_gltf(path, out_path, root, report):
    """Writes the edits of the objects imported from `path` to `out_path`. Returns "same", "moved" or "rebuilt"."""
    gltf = Gltf(path)
    objects = mesh_objects_of(path)
    state = "same"
    for obj in objects:
        node = gltf.json["nodes"][obj["dc1_node"]]
        mesh_index = node["mesh"]
        primitives = gltf.json["meshes"][mesh_index]["primitives"]
        ranges = list(obj.data.get("dc1_vertex_ranges", []))
        world0 = unflat_matrix(obj["dc1_world0"])
        now = game_matrix(obj, top_root(obj))
        delta = None if _close(now, world0) else world0.inverted() @ now
        mesh = obj.data
        points = [tuple(v.co) if delta is None else tuple(delta @ v.co) for v in mesh.vertices]
        same_shape = len(points) == sum(ranges) and len(mesh.polygons) == mesh.get("dc1_face_count", -1)
        if same_shape:
            same_shape = all(len(p.vertices) == 3 for p in mesh.polygons)
        if same_shape:
            at, moved = 0, False
            for primitive, count in zip(primitives, ranges):
                accessor = primitive["attributes"]["POSITION"]
                old = gltf.read(accessor)
                new = [tuple(_f32(c) for c in p) for p in points[at:at + count]]
                at += count
                if any(tuple(_f32(c) for c in o) != n for o, n in zip(old, new)):
                    gltf.write_vec3(accessor, new)
                    moved = True
            if moved and state == "same":
                state = "moved"
            continue
        _rewrite_mesh(gltf, mesh_index, obj, delta)
        state = "rebuilt"
        report({"WARNING"}, "%s / %s: the mesh's faces changed, so dc1tk rebuilds the model; it may come out larger "
               "than retail, which MoonMod refuses" % (os.path.basename(path), obj.name))
    if state != "same" or os.path.normcase(os.path.abspath(path)) != os.path.normcase(os.path.abspath(out_path)):
        if state == "same":
            copy_gltf(path, out_path, gltf)
        else:
            gltf.save(out_path)
    return state


def copy_gltf(path, out_path, gltf=None):
    import shutil
    gltf = gltf or Gltf(path)
    folder = os.path.dirname(out_path)
    os.makedirs(folder, exist_ok=True)
    shutil.copyfile(path, out_path)
    for buffer in gltf.json.get("buffers", []):
        shutil.copyfile(os.path.join(gltf.folder, buffer["uri"]), os.path.join(folder, buffer["uri"]))
    for image in gltf.json.get("images", []):
        source = os.path.join(gltf.folder, image.get("uri", ""))
        if os.path.isfile(source):
            target = os.path.join(folder, image["uri"])
            os.makedirs(os.path.dirname(target), exist_ok=True)
            if not os.path.exists(target):
                shutil.copyfile(source, target)


def _rewrite_mesh(gltf, mesh_index, obj, delta):
    """New primitives (one per material) for a mesh whose faces changed, appended to the buffer."""
    mesh = obj.data.copy()
    mesh.calc_loop_triangles()
    try:
        corner_normals = [tuple(n.vector) for n in mesh.corner_normals]
    except AttributeError:
        mesh.calc_normals_split()
        corner_normals = [tuple(loop.normal) for loop in mesh.loops]
    uv = mesh.uv_layers.active.data if mesh.uv_layers.active else None
    color = mesh.color_attributes.get("Col")
    normal_matrix = delta.to_3x3().inverted().transposed() if delta is not None else None
    old = gltf.json["meshes"][mesh_index]["primitives"]
    by_slot = {}
    for tri in mesh.loop_triangles:
        by_slot.setdefault(tri.material_index, []).append(tri)
    primitives = []
    for slot, tris in sorted(by_slot.items()):
        material = mesh.materials[slot] if slot < len(mesh.materials) else None
        index = material.get("dc1_gltf_material") if material else None
        key_of, pos, nor, tex, col, idx = {}, [], [], [], [], []
        for tri in tris:
            for v, loop in zip(tri.vertices, tri.loops):
                p = mesh.vertices[v].co if delta is None else delta @ mesh.vertices[v].co
                n = Vector(corner_normals[loop])
                if normal_matrix is not None:
                    n = (normal_matrix @ n).normalized()
                t = (uv[loop].uv[0], 1.0 - uv[loop].uv[1]) if uv else (0.0, 0.0)
                c = tuple(color.data[v].color) if color and color.domain == "POINT" else (1, 1, 1, 1)
                key = (v, tuple(round(x, 5) for x in n), tuple(round(x, 6) for x in t))
                if key not in key_of:
                    key_of[key] = len(pos)
                    pos.append(tuple(p))
                    nor.append(tuple(n))
                    tex.append(t)
                    col.append(c)
                idx.append(key_of[key])
        if len(pos) > 65535:
            raise ValueError("%s: more than 65535 corners for one material" % obj.name)
        attributes = {
            "POSITION": gltf.append(struct.pack("<%df" % (3 * len(pos)), *[c for p in pos for c in p]), "f", 3,
                                    len(pos), 34962, True, pos),
            "NORMAL": gltf.append(struct.pack("<%df" % (3 * len(nor)), *[c for p in nor for c in p]), "f", 3,
                                  len(nor), 34962),
            "TEXCOORD_0": gltf.append(struct.pack("<%df" % (2 * len(tex)), *[c for p in tex for c in p]), "f", 2,
                                      len(tex), 34962),
        }
        if any("COLOR_0" in p["attributes"] for p in old):
            attributes["COLOR_0"] = gltf.append(struct.pack("<%df" % (4 * len(col)), *[c for p in col for c in p]),
                                                "f", 4, len(col), 34962)
        primitive = {"attributes": attributes,
                     "indices": gltf.append(struct.pack("<%dH" % len(idx), *idx), "H", 1, len(idx), 34963),
                     "mode": 4}
        if index is not None:
            primitive["material"] = index
        primitives.append(primitive)
    gltf.json["meshes"][mesh_index]["primitives"] = primitives
    bpy.data.meshes.remove(mesh)


def degrees(value):
    return math.degrees(value)
