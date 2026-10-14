"""Writes a tiny map scene that follows docs/assets/maps.md section 8, as `c2 map-export` would:
two placements of one template (a model piece, a collision piece and a camera-collision piece,
and a template point), the map's own points (a door, a point light with a KHR light, a sound
emitter, an invent box, and two points whose data is a raw argument list), a fixed camera, and
the map settings on the root.

    python make_test_map.py OUT_DIR            (any Python 3)
    blender --background --python make_test_map.py -- OUT_DIR

writes OUT_DIR/t01.gltf and OUT_DIR/t01.bin. Only the standard library."""

import json
import math
import os
import struct
import sys

NAME = "t01"


def quat_y(degrees):
    half = math.radians(degrees) / 2
    return [0.0, math.sin(half), 0.0, math.cos(half)]


class Builder:
    def __init__(self):
        self.bin = bytearray()
        self.views = []
        self.accessors = []
        self.nodes = []
        self.meshes = []
        self.materials = []

    def _view(self, data, target):
        while len(self.bin) % 4:
            self.bin.append(0)
        offset = len(self.bin)
        self.bin += data
        self.views.append({"buffer": 0, "byteOffset": offset, "byteLength": len(data), "target": target})
        return len(self.views) - 1

    def floats(self, rows, kind):
        size = len(rows[0])
        data = b"".join(struct.pack("<%df" % size, *row) for row in rows)
        accessor = {
            "bufferView": self._view(data, 34962),
            "componentType": 5126,
            "count": len(rows),
            "type": kind,
        }
        if kind == "VEC3":
            accessor["min"] = [min(row[i] for row in rows) for i in range(3)]
            accessor["max"] = [max(row[i] for row in rows) for i in range(3)]
        self.accessors.append(accessor)
        return len(self.accessors) - 1

    def indices(self, values):
        data = struct.pack("<%dH" % len(values), *values)
        self.accessors.append(
            {"bufferView": self._view(data, 34963), "componentType": 5123, "count": len(values), "type": "SCALAR"}
        )
        return len(self.accessors) - 1

    def material(self, name, colour=(0.8, 0.8, 0.8, 1.0)):
        self.materials.append({"name": name, "pbrMetallicRoughness": {"baseColorFactor": list(colour)}})
        return len(self.materials) - 1

    def mesh(self, name, primitives):
        self.meshes.append({"name": name, "primitives": primitives})
        return len(self.meshes) - 1

    def node(self, name, c2, children=(), **extra):
        node = {"name": name}
        if children:
            node["children"] = list(children)
        node.update(extra)
        if c2 is not None:
            node["extras"] = {"c2": c2}
        self.nodes.append(node)
        return len(self.nodes) - 1


def box_mesh(b):
    corners = [(x, y, z) for x in (-10, 10) for y in (0, 30) for z in (-10, 10)]
    faces = [
        (0, 1, 3), (0, 3, 2), (4, 6, 7), (4, 7, 5), (0, 4, 5), (0, 5, 1),
        (2, 3, 7), (2, 7, 6), (0, 2, 6), (0, 6, 4), (1, 5, 7), (1, 7, 3),
    ]
    # Flat corners (one vertex per triangle corner) so normals and colours are per face.
    positions, normals, colours, index = [], [], [], []
    for face in faces:
        a, c, d = (corners[i] for i in face)
        u = [c[i] - a[i] for i in range(3)]
        v = [d[i] - a[i] for i in range(3)]
        n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]]
        length = math.sqrt(sum(x * x for x in n)) or 1.0
        n = [x / length for x in n]
        for corner in (a, c, d):
            index.append(len(positions))
            positions.append([float(x) for x in corner])
            normals.append(n)
            colours.append([0.7, 0.7, 0.7, 1.0] if corner[1] > 0 else [0.47, 0.47, 0.47, 1.0])
    material = b.material("house_wall")
    return b.mesh(
        "house-m.mds",
        [
            {
                "attributes": {
                    "POSITION": b.floats(positions, "VEC3"),
                    "NORMAL": b.floats(normals, "VEC3"),
                    "COLOR_0": b.floats(colours, "VEC4"),
                },
                "indices": b.indices(index),
                "material": material,
            }
        ],
    )


def collision_mesh(b):
    primitives = []
    triangles = (
        ("col_ground0_foot2_area6_mask0", [[-40, 0, -40], [40, 0, -40], [40, 0, 40]]),
        ("col_ground0_foot5_area1_mask12", [[-40, 0, -40], [40, 0, 40], [-40, 0, 40]]),
    )
    for name, corners in triangles:
        primitives.append(
            {
                "attributes": {"POSITION": b.floats([[float(v) for v in c] for c in corners], "VEC3")},
                "indices": b.indices([0, 1, 2]),
                "material": b.material(name, (1.0, 1.0, 1.0, 1.0)),
            }
        )
    return b.mesh("house-a.mds", primitives)


def camera_mesh(b):
    corners = [[-40.0, 0.0, -40.0], [40.0, 60.0, -40.0], [40.0, 0.0, -40.0]]
    return b.mesh(
        "house-c.mds",
        [
            {
                "attributes": {"POSITION": b.floats(corners, "VEC3")},
                "indices": b.indices([0, 1, 2]),
                "material": b.material("col_ground0_foot0_area0_mask0", (1.0, 1.0, 1.0, 1.0)),
            }
        ],
    )


def piece(model, kind):
    return {"role": "piece", "model": model, "kind": kind, "show": 1, "time": [0, 0], "col_type": 0, "materials": []}


def build():
    b = Builder()
    meshes = {"m": box_mesh(b), "a": collision_mesh(b), "c": camera_mesh(b)}

    def placement(name, slot, translation, degrees, far_clip):
        model = b.node(
            "piece:house_m",
            piece("house-m.mds", "model"),
            [b.node("house_body--a40aof-", {"role": "mds", "mds_index": 0}, mesh=meshes["m"])],
            scale=[-1.0, 1.0, 1.0],
        )
        col = b.node(
            "piece:house_a",
            piece("house-a.mds", "collision"),
            [b.node("floor", {"role": "mds", "mds_index": 0}, mesh=meshes["a"])],
        )
        cam = b.node(
            "piece:house_c",
            piece("house-c.mds", "camera"),
            [b.node("wall", {"role": "mds", "mds_index": 0}, mesh=meshes["c"])],
        )
        point = b.node(
            "point:point",
            {"role": "point", "kind": "pos", "enable": 1, "flag": [0, 0, 0, 0], "data": {}},
            translation=[0.0, 0.0, 30.0],
        )
        return b.node(
            "place:" + name,
            {
                "role": "placement",
                "template": "house",
                "show": 1,
                "group": "",
                "far_clip": far_clip,
                "slot": slot,
            },
            [model, col, cam, point],
            translation=translation,
            rotation=quat_y(degrees),
        )

    places = [
        placement("house01", 0, [100.0, 0.0, 50.0], 90, None),
        placement("house02", 1, [-150.0, 5.0, 220.0], 30, [2000.0, 200.0]),
    ]

    door = b.node(
        "point:exit",
        {
            "role": "point",
            "kind": "event",
            "enable": 1,
            "flag": [0, 0, 0, 0],
            "data": {"kind": "door", "event_no": 3, "program": 510, "args": [0, 0, 0], "target": "s17", "flags": [1, 0, 0]},
        },
        translation=[0.0, 0.0, -200.0],
    )
    lamp = b.node(
        "point:lamp0",
        {
            "role": "point",
            "kind": "plight",
            "enable": 1,
            "flag": [0, 0, 18, 6],
            "data": {"power": 120.0, "color": [255, 200, 128], "mode": 0, "args": [0, 0, 0, 0], "flicker": 1, "depth": 0.25, "period": 30.0},
        },
        translation=[20.0, 80.0, 10.0],
        extensions={"KHR_lights_punctual": {"light": 0}},
    )
    sound = b.node(
        "point:river",
        {
            "role": "point",
            "kind": "sound",
            "enable": 1,
            "flag": [0, 0, 0, 0],
            "data": {"number": 12, "near": 40.0, "far": 300.0, "unknown_int": 0, "shape": 0, "start": [0.0, 0.0, 0.0], "end": [0.0, 0.0, 0.0]},
        },
        translation=[-300.0, 0.0, 0.0],
    )
    invent = b.node(
        "point:photo1",
        {
            "role": "point",
            "kind": "invent",
            "enable": 1,
            "flag": [0, 0, 0, 0],
            "data": {"id": 7, "min": [-20.0, -5.0, -20.0], "max": [20.0, 40.0, 20.0], "unknown_int": 0, "unknown_float": 0.0, "angle": 45.0},
        },
        translation=[50.0, 0.0, -50.0],
        rotation=quat_y(45),
    )
    # Two points whose data is the raw argument list of their line instead of named fields: the
    # add-on reads such data by position. A list mixing text and numbers is more than Blender's
    # custom properties can hold, so Blender keeps that whole extras.c2 as text; the add-on
    # writes it back as JSON on export.
    raw_door = b.node(
        "point:rawdoor",
        {
            "role": "point",
            "kind": "event",
            "enable": 1,
            "flag": [0, 0, 0, 0],
            "data": ["door", 1, 520, 0, 0, 0, "s20", 1, 0, 0],
        },
        translation=[0.0, 0.0, 200.0],
    )
    raw_light = b.node(
        "point:rawlight",
        {
            "role": "point",
            "kind": "plight",
            "enable": 1,
            "flag": [0, 0, 0, 0],
            "data": [80.0, 255, 128, 64, 0, 0, 0, 0, 0, 0, 0.0, 0.0],
        },
        translation=[0.0, 50.0, 200.0],
    )
    camera = b.node("camera:0", {"role": "camera", "index": 0, "pos": [0.0, 100.0, -50.0], "groups": ["inside"]})

    placements = b.node("placements", {"role": "group"}, places)
    points = b.node("points", {"role": "group"}, [door, lamp, sound, invent, raw_door, raw_light])
    cameras = b.node("cameras", {"role": "group"}, [camera])
    map_tree = {
        "images": ["t01.img"],
        "packs": ["t01_0.pcp"],
        "active_light_set": 0,
        "light_sets": [
            {
                "index": 0,
                "fov": 52,
                "bgcolor": [10, 20, 30],
                "bgcolor2": [0, 0, 0],
                "ambient": [128, 128, 128],
                "lights": [{"index": 0, "dir": [0.0, -1.0, 0.0], "color": [128, 128, 128]}],
                "fog_enable": 1,
                "fog": [100.0, 3000.0, 40.0, 50.0, 60.0],
            }
        ],
        "floor": 0,
        "def_foot": 2,
        "time_flag": [0, 0],
        "time_light_num": 4,
        "sky_info": [0.0, 1.0],
        "hash": "9f86d081884c7d65",
    }
    root = b.node("map:" + NAME, {"role": "map", "map": map_tree}, [placements, points, cameras])

    gltf = {
        "asset": {"version": "2.0", "generator": "c2 test map"},
        "extensionsUsed": ["KHR_lights_punctual"],
        "extensions": {
            "KHR_lights_punctual": {
                "lights": [{"name": "lamp0", "type": "point", "color": [1.0, 200 / 255, 128 / 255], "intensity": 120.0, "range": 300.0}]
            }
        },
        "scene": 0,
        "scenes": [{"name": NAME, "nodes": [root]}],
        "nodes": b.nodes,
        "meshes": b.meshes,
        "materials": b.materials,
        "accessors": b.accessors,
        "bufferViews": b.views,
        "buffers": [{"uri": NAME + ".bin", "byteLength": len(b.bin)}],
    }
    return gltf, bytes(b.bin)


def write(out_dir):
    os.makedirs(out_dir, exist_ok=True)
    gltf, data = build()
    path = os.path.join(out_dir, NAME + ".gltf")
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(gltf, handle, indent=1)
    with open(os.path.join(out_dir, NAME + ".bin"), "wb") as handle:
        handle.write(data)
    return path


if __name__ == "__main__":
    args = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else sys.argv[1:]
    print(write(args[0] if args else os.path.join(os.getcwd(), "t01")))
