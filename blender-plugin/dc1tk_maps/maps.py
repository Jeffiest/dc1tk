"""Importing and exporting the three kinds of Dark Cloud 1 map folder `dc2tk maps-export` writes."""

import json
import math
import os
import re
import shutil
import struct

import bpy
from mathutils import Euler, Matrix, Vector

from . import gltf_io

PART_SPACING = 220.0  # floor parts are laid out on a grid for viewing; the game places them itself
CELL = 100.0          # a Georama grid cell, for drawing the cells


def f32(value):
    return struct.unpack("<f", struct.pack("<f", float(value)))[0]


def same_f32(a, b):
    return f32(a) == f32(b)


def collection(name, parent):
    found = bpy.data.collections.get(name)
    if found is None or found.name not in parent.children:
        found = bpy.data.collections.new(name)
        parent.children.link(found)
    return found


def make_root(context, kind, name, source):
    top = bpy.data.collections.new("DC1 %s %s" % (kind, name))
    context.scene.collection.children.link(top)
    root = bpy.data.objects.new("DC1 %s %s" % (kind, name), None)
    root.empty_display_type = "ARROWS"
    root.empty_display_size = 20
    root.rotation_euler = (math.pi / 2, 0, 0)  # the game's Y up is Blender's Z up
    root["dc1_kind"] = kind
    root["dc1_space"] = True
    root["dc1_source"] = source
    top.objects.link(root)
    return root, top


def empty(name, parent, coll, kind="PLAIN_AXES", size=1.0):
    obj = bpy.data.objects.new(name, None)
    obj.empty_display_type = kind
    obj.empty_display_size = size
    coll.objects.link(obj)
    obj.parent = parent
    return obj


def hide_collection(context, coll):
    def find(layer):
        if layer.collection == coll:
            return layer
        for child in layer.children:
            hit = find(child)
            if hit:
                return hit
    layer = find(context.view_layer.layer_collection)
    if layer:
        layer.hide_viewport = True


def stored_position(obj):
    return [f32(v) for v in obj.location]


# ---- dungeon floor ----

def import_floor(context, folder):
    folder = os.path.normpath(folder)
    with open(os.path.join(folder, "floor.json"), encoding="utf-8") as handle:
        floor = json.load(handle)
    name = os.path.basename(folder)
    root, top = make_root(context, "floor", name, folder)
    models = collection("%s models" % name, top)
    collision = collection("%s collision" % name, top)
    camera = collection("%s camera" % name, top)
    fires = collection("%s torches" % name, top)
    parts_dir = os.path.join(folder, "parts")
    used = set()
    imported = {}
    for index, group in enumerate(floor["parts"]):
        tags = {}
        for statement in group:
            tags.setdefault(statement["tag"], []).append(statement["args"])
        base = (tags.get("PT_BASE") or [[None]])[0][0] or "part%d" % index
        holder = empty("%02d %s" % (index, os.path.splitext(base)[0]), root, top, "CUBE", 5)
        holder.location = ((index % 8) * PART_SPACING, 0, (index // 8) * PART_SPACING)
        holder["dc1_space"] = True
        holder["dc1_group"] = index
        holder["dc1_statements"] = json.dumps(group)
        for tag, coll, is_collision, hidden in (("PT_BASE", models, False, False), ("PT_COLS", collision, True, False),
                                                 ("PT_CAM", camera, True, True)):
            for args in tags.get(tag, []):
                if not args or not isinstance(args[0], str):
                    continue
                path = os.path.join(parts_dir, os.path.splitext(args[0])[0] + ".gltf")
                key = os.path.normcase(path)
                if key in imported:
                    gltf_io.instance(imported[key], holder, coll)
                elif os.path.isfile(path):
                    imported[key] = gltf_io.import_gltf(path, holder, coll, collision=is_collision)
                    used.add(key)
        for n, args in enumerate(tags.get("PT_FIRE", [])):
            fire = empty("%02d torch %d" % (index, n), holder, fires, "SINGLE_ARROW", 3)
            fire.location = args[:3]
            fire["dc1_fire"] = n
            fire["dc1_fire0"] = [f32(v) for v in args[:3]]
    # models of the pack no DEF_PATS names, past the last row
    rest = sorted(f for f in os.listdir(parts_dir) if f.endswith(".gltf")
                  and os.path.normcase(os.path.join(parts_dir, f)) not in used) if os.path.isdir(parts_dir) else []
    if rest:
        others = collection("%s other models" % name, top)
        row = (len(floor["parts"]) + 7) // 8 + 1
        for i, file in enumerate(rest):
            holder = empty("other %s" % os.path.splitext(file)[0], root, others, "CUBE", 5)
            holder.location = ((i % 8) * PART_SPACING, 0, (row + i // 8) * PART_SPACING)
            holder["dc1_space"] = True
            stem = os.path.splitext(file)[0]
            gltf_io.import_gltf(os.path.join(parts_dir, file), holder, others, collision=stem.endswith(("_a", "_v")))
        hide_collection(context, others)
    hide_collection(context, camera)
    return root


def _fire_line(line, values):
    indent = line[:len(line) - len(line.lstrip())]
    end = "\r\n" if line.endswith("\r\n") else "\n" if line.endswith("\n") else ""
    return "%sPT_FIRE\t%s%s" % (indent, ",\t".join("%.6f" % v for v in values), end)


def patch_floor_cfg(text, edits):
    """floor.cfg with PT_FIRE n of DEF_PATS block g set to edits[(g, n)]."""
    out, group, fire = [], -1, 0
    for line in text.splitlines(keepends=True):
        code = line.split("//", 1)[0].strip()
        word = code.split(None, 1)[0] if code else ""
        if word == "DEF_PATS":
            group += 1
            fire = 0
        elif word == "PT_FIRE":
            if (group, fire) in edits:
                line = _fire_line(line, edits[(group, fire)])
            fire += 1
        out.append(line)
    return "".join(out)


def copy_folder(source, target):
    if os.path.normcase(os.path.abspath(source)) != os.path.normcase(os.path.abspath(target)):
        shutil.copytree(source, target, dirs_exist_ok=True)


def export_files(root, source, target, report):
    """Writes every glTF imported under `root` to the same place under `target`; counts by result."""
    counts = {"same": 0, "moved": 0, "rebuilt": 0}
    files = sorted({o["dc1_file"] for o in root.children_recursive if "dc1_file" in o})
    for path in files:
        rel = os.path.relpath(path, source)
        counts[gltf_io.export_gltf(path, os.path.join(target, rel), None, report)] += 1
    return counts


def export_floor(root, target, report):
    source = root["dc1_source"]
    copy_folder(source, target)
    counts = export_files(root, source, target, report)
    edits = {}
    for obj in root.children_recursive:
        if "dc1_fire" in obj and obj.parent and "dc1_group" in obj.parent:
            now = stored_position(obj)
            if now != list(obj["dc1_fire0"]):
                edits[(obj.parent["dc1_group"], obj["dc1_fire"])] = now
    if edits:
        path = os.path.join(target, "floor.cfg")
        with open(path, encoding="utf-8", newline="") as handle:
            text = handle.read()
        with open(path, "w", encoding="utf-8", newline="") as handle:
            handle.write(patch_floor_cfg(text, edits))
    counts["torches"] = len(edits)
    return counts


# ---- Georama scene ----

def import_scene(context, folder):
    folder = os.path.normpath(folder)
    with open(os.path.join(folder, "scene.json"), encoding="utf-8") as handle:
        scene = json.load(handle)
    name = os.path.basename(folder)
    root, top = make_root(context, "scene", name, folder)
    models = collection("%s models" % name, top)
    lods = collection("%s lod1-3 shadow shade extra" % name, top)
    collision = collection("%s collision" % name, top)
    camera = collection("%s camera" % name, top)
    grids = collection("%s cells" % name, top)
    for index, part in enumerate(scene["parts"]):
        info = part["info"]
        holder = empty(part["name"], root, top, "ARROWS", 10)
        holder.location = info["position"]
        holder.rotation_mode = "XYZ"
        holder.rotation_euler = info["rotation"]
        holder["dc1_space"] = True
        holder["dc1_part"] = index
        holder["dc1_position0"] = [f32(v) for v in info["position"]]
        holder["dc1_rotation0"] = [f32(v) for v in info["rotation"]]
        done = set()
        for slot, file in part["models"].items():
            if not file.endswith(".gltf") or file in done:
                continue
            done.add(file)
            coll = {"lod0": models, "collision": collision, "camera": camera}.get(slot, lods)
            gltf_io.import_gltf(os.path.join(folder, file), holder, coll, collision=slot in ("collision", "camera"))
        grid = cell_grid(part["name"] + " cells", info)
        grids.objects.link(grid)
        grid.parent = holder
        grid["dc1_cells"] = list(info["cells"])
        grid["dc1_cells0"] = list(info["cells"])
    hide_collection(context, lods)
    hide_collection(context, camera)
    return root


def cell_grid(name, info):
    width, height = max(info["width"], 0), max(info["height"], 0)
    verts, faces, values = [], [], []
    x0, z0 = -width * CELL / 2, -height * CELL / 2
    for row in range(height):
        cells = bytes.fromhex(info["cells"][row]) if row < len(info["cells"]) else b""
        for col in range(width):
            base = len(verts)
            x, z = x0 + col * CELL, z0 + row * CELL
            verts += [(x, 0.5, z), (x + CELL, 0.5, z), (x + CELL, 0.5, z + CELL), (x, 0.5, z + CELL)]
            faces.append((base, base + 3, base + 2, base + 1))
            values.append(cells[col] if col < len(cells) else 0)
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, [], faces)
    if faces:
        attribute = mesh.attributes.new("cell", "INT", "FACE")
        attribute.data.foreach_set("value", values)
    obj = bpy.data.objects.new(name, mesh)
    obj.display_type = "WIRE"
    obj.hide_render = True
    return obj


def export_scene(root, target, report):
    source = root["dc1_source"]
    copy_folder(source, target)
    counts = export_files(root, source, target, report)
    path = os.path.join(source, "scene.json")
    with open(path, encoding="utf-8") as handle:
        scene = json.load(handle)
    changed = 0
    for holder in root.children:
        if "dc1_part" not in holder:
            continue
        info = scene["parts"][holder["dc1_part"]]["info"]
        position = [f32(v) for v in holder.location]
        rotation = [f32(v) for v in holder.rotation_euler]
        for key, now, before in (("position", position, holder["dc1_position0"]),
                                 ("rotation", rotation, holder["dc1_rotation0"])):
            if now != list(before):
                info[key] = now
                changed += 1
        for grid in holder.children:
            if "dc1_cells" in grid and list(grid["dc1_cells"]) != list(grid["dc1_cells0"]):
                rows = [str(r) for r in grid["dc1_cells"]]
                for r, row in enumerate(rows):
                    if len(row) != 2 * info["width"] or not re.fullmatch("[0-9a-fA-F]*", row):
                        raise ValueError("%s: cell row %d must be %d hex bytes" % (holder.name, r, info["width"]))
                info["cells"] = rows
                changed += 1
    target_json = os.path.join(target, "scene.json")
    if changed:
        with open(target_json, "w", encoding="utf-8", newline="\n") as handle:
            json.dump(scene, handle, indent=2, ensure_ascii=False)
    counts["parts"] = changed
    return counts


# ---- Georama layout ----

def import_layout(context, path):
    path = os.path.normpath(path)
    with open(path, encoding="utf-8") as handle:
        layout = json.load(handle)
    name = os.path.basename(os.path.dirname(path)) + "/" + os.path.basename(path)
    root, top = make_root(context, "layout", name, path)
    for index, part in enumerate(layout["parts"]):
        obj = empty("%03d part %d" % (index, part["part"]), root, top, "CUBE", 25)
        obj.location = part["position"]
        obj.rotation_euler = (0, part["rotation"] * math.pi / 2, 0)
        obj.show_name = True
        obj["dc1_layout_index"] = index
        obj["dc1_layout_part"] = part["part"]
        # an arrow along the part's own +Z shows which way it faces
        arrow = empty(obj.name + " facing", obj, top, "SINGLE_ARROW", 40)
        arrow.rotation_euler = (math.pi / 2, 0, 0)
        arrow.hide_select = True
    return root


def export_layout(root, target, report):
    source = root["dc1_source"]
    with open(source, encoding="utf-8") as handle:
        layout = json.load(handle)
    placed = [o for o in root.children if "dc1_layout_part" in o]
    placed.sort(key=lambda o: (o.get("dc1_layout_index", 1 << 30), o.name))
    parts, changed = [], len(placed) != len(layout["parts"])
    seen = set()
    for obj in placed:
        index = obj.get("dc1_layout_index", -1)
        old = layout["parts"][index] if 0 <= index < len(layout["parts"]) and index not in seen else None
        seen.add(index)
        turns = int(round(obj.rotation_euler[1] / (math.pi / 2)))
        position = [f32(v) for v in obj.location]
        part = {"part": int(obj["dc1_layout_part"]), "rotation": turns, "position": position}
        if old is not None:
            if old["part"] == part["part"] and old["rotation"] == turns and \
                    all(same_f32(a, b) for a, b in zip(old["position"], position)):
                part = old
            else:
                changed = True
        else:
            changed = True
        parts.append(part)
    if changed or os.path.normcase(os.path.abspath(source)) != os.path.normcase(os.path.abspath(target)):
        layout["parts"] = parts
        os.makedirs(os.path.dirname(target) or ".", exist_ok=True)
        if changed:
            with open(target, "w", encoding="utf-8", newline="\n") as handle:
                json.dump(layout, handle, indent=2, ensure_ascii=False)
        else:
            shutil.copyfile(source, target)
    return {"parts": len(parts), "changed": int(changed)}


EXPORTERS = {"floor": export_floor, "scene": export_scene, "layout": export_layout}
