"""Importing a map's glTF scene: running `c2 map export`, Blender's glTF importer with the
options the scene needs, then making it workable (collision hidden and coloured, point boxes,
point lights)."""

import glob
import json
import os

import bpy
from bpy.props import StringProperty
from bpy_extras.io_utils import ImportHelper

from . import c2data, prefs, runner, schema


def operator_kwargs(operator, wanted):
    """Only the options this Blender's glTF operator has (they change between versions)."""
    names = {prop.identifier for prop in operator.get_rna_type().properties}
    return {key: value for key, value in wanted.items() if key in names}


def read_gltf(path):
    if path.lower().endswith(".glb"):
        import struct

        with open(path, "rb") as handle:
            head = handle.read(20)
            length = struct.unpack_from("<I", head, 12)[0]
            return json.loads(handle.read(length).decode("utf-8"))
    with open(path, "r", encoding="utf-8") as handle:
        return json.load(handle)


def match_nodes(gltf, roots):
    """{glTF node index: Blender object}, walking both trees: children are paired by name
    (without Blender's .001 suffixes), in order."""
    nodes = gltf.get("nodes", [])
    scene = gltf.get("scenes", [{}])[gltf.get("scene", 0)] if gltf.get("scenes") else {}
    found = {}

    def pair(indices, objects):
        by_name = {}
        for obj in sorted(objects, key=lambda o: (len(o.name), o.name)):
            by_name.setdefault(schema.strip_suffix(obj.name), []).append(obj)
        for index in indices:
            name = nodes[index].get("name", "")
            candidates = by_name.get(name) or by_name.get(schema.strip_suffix(name))
            if not candidates:
                continue
            obj = candidates.pop(0)
            found[index] = obj
            pair(nodes[index].get("children", []), list(obj.children))

    pair(scene.get("nodes", []), roots)
    return found


def ensure_collection(parent, name):
    collection = None
    for child in parent.children:
        if schema.strip_suffix(child.name) == name:
            collection = child
            break
    if collection is None:
        collection = bpy.data.collections.new(name)
        parent.children.link(collection)
    return collection


def find_layer_collection(layer, collection):
    if layer.collection == collection:
        return layer
    for child in layer.children:
        found = find_layer_collection(child, collection)
        if found is not None:
            return found
    return None


def collision_collections(context, root, create=True):
    """{"collision": collection, "camera": collection} beside the map's own collection."""
    home = root.users_collection[0] if root.users_collection else context.scene.collection
    name = c2data.map_name(root)
    found = {}
    for kind, label in (("collision", "collision"), ("camera", "camera collision")):
        title = "%s %s" % (name, label)
        if create:
            found[kind] = ensure_collection(home, title)
        else:
            found[kind] = next((c for c in home.children if schema.strip_suffix(c.name) == title), None)
    return found


def set_collision_visible(context, root, kind, visible):
    collection = collision_collections(context, root, create=False).get(kind)
    if collection is None:
        return False
    layer = find_layer_collection(context.view_layer.layer_collection, collection)
    if layer is not None:
        layer.hide_viewport = not visible
    return True


def collision_visible(context, root, kind):
    collection = collision_collections(context, root, create=False).get(kind)
    if collection is None:
        return None
    layer = find_layer_collection(context.view_layer.layer_collection, collection)
    return layer is not None and not layer.hide_viewport


def move_to(obj, collection):
    if collection in obj.users_collection and len(obj.users_collection) == 1:
        return
    if obj.name not in collection.objects:
        collection.objects.link(obj)
    for other in list(obj.users_collection):
        if other != collection:
            other.objects.unlink(obj)


def colour_collision(root, mode):
    """Viewport colours for the collision materials (their names are never changed)."""
    done = set()
    for obj in c2data.collision_meshes(root):
        camera = c2data.lookup(c2data.piece_of(obj), ("kind",)) == "camera"
        for slot in obj.material_slots:
            material = slot.material
            if material is None or material.name in done:
                continue
            done.add(material.name)
            attrs = schema.parse_col(material.name)
            if attrs is None:
                material.diffuse_color = (1.0, 0.0, 1.0, 1.0)  # not a collision material: stands out
            elif camera:
                material.diffuse_color = (0.55, 0.35, 0.85, 0.6)
            else:
                material.diffuse_color = schema.collision_colour(attrs, mode)
        obj.display_type = "SOLID"
        obj.show_wire = True


def point_display(obj):
    """Empty display for a point: a box of its size for events and invent boxes."""
    if obj.type != "EMPTY":
        return
    kind = c2data.lookup(obj, ("kind",))
    if kind == "event":
        event_kind = c2data.lookup(obj, ("data", "kind"))
        if event_kind is None:
            data = c2data.lookup(obj, ("data",))
            if data is not None and not hasattr(data, "keys"):
                event_kind = c2data.to_py(data)[0] if len(data) else None
        obj.empty_display_type = "CUBE"
        scale = obj.scale
        near_one = all(abs(abs(s) - 1.0) < 0.01 for s in scale)
        obj.empty_display_size = schema.EVENT_DEFAULT_SIZE.get(event_kind, 1.0) if near_one else 1.0
    elif kind == "invent":
        field_min, field_max = schema.DATA_FIELDS["invent"][1], schema.DATA_FIELDS["invent"][2]
        low = c2data.data_field(obj, field_min)
        high = c2data.data_field(obj, field_max)
        extent = max([abs(float(v)) for v in list(low) + list(high)] + [1.0])
        obj.empty_display_type = "CUBE"
        obj.empty_display_size = extent
    elif kind == "sound":
        near = c2data.data_field(obj, schema.DATA_FIELDS["sound"][1])
        obj.empty_display_type = "SPHERE"
        obj.empty_display_size = max(float(near or 0.0), 5.0)
    elif kind == "pos":
        obj.empty_display_type = "SINGLE_ARROW"
        obj.empty_display_size = 15.0
    elif kind == "anime":
        obj.empty_display_type = "ARROWS"
        obj.empty_display_size = 10.0
    else:
        obj.empty_display_type = "PLAIN_AXES"
        obj.empty_display_size = 10.0
    obj.show_name = False


def light_values(obj):
    rgb = c2data.data_field(obj, schema.DATA_FIELDS["plight"][1])
    power = c2data.data_field(obj, schema.DATA_FIELDS["plight"][0])
    colour = [max(0.0, min(1.0, float(c) / 255.0)) for c in rgb]
    return colour, float(power or 0.0)


def light_correction():
    """Blender's glTF importer turns a light node by +90 degrees about X (Blender lights shine
    down their -Z, glTF's down -Z of a Y-up node) and its exporter turns it back, so a light
    made here gets the same turn to export the node's own rotation."""
    import math

    from mathutils import Matrix

    return Matrix.Rotation(math.radians(90.0), 4, "X")


def ensure_light(obj):
    """A point light as a Blender point light: an empty is replaced by a light object with the
    same name, transform, parent, children and extras."""
    if obj.type == "LIGHT":
        return obj
    if obj.type != "EMPTY":
        return obj
    name = obj.name
    colour, power = light_values(obj)
    light = bpy.data.lights.new(schema.game_name(name, "point"), "POINT")
    light.color = colour
    light.energy = power
    obj.name = name + "~old"
    new = bpy.data.objects.new(name, light)
    for collection in obj.users_collection:
        collection.objects.link(new)
    new.parent = obj.parent
    new.matrix_parent_inverse = obj.matrix_parent_inverse.copy()
    new.rotation_mode = obj.rotation_mode
    new.matrix_basis = obj.matrix_basis @ light_correction()
    if c2data.KEY in obj:
        new[c2data.KEY] = obj[c2data.KEY]
    for child in list(obj.children):
        world = child.matrix_world.copy()
        child.parent = new
        child.matrix_world = world
    bpy.data.objects.remove(obj)
    return new


def postprocess(context, gltf_path, new_objects, log=None):
    """Make an imported map workable. Returns the map roots and a list of warnings."""
    warnings = []
    roots = [obj for obj in new_objects if obj.parent is None and c2data.role(obj) == "map"]
    if not roots:
        roots = [obj for obj in new_objects if obj.parent is None and obj.name.startswith("map:")]
    if not roots:
        warnings.append("No map: root node (role \"map\") in the scene")
        return roots, warnings

    try:
        gltf = read_gltf(gltf_path) if gltf_path else None
    except (OSError, ValueError) as error:
        gltf = None
        warnings.append("Could not read %s: %s" % (gltf_path, error))
    matched = match_nodes(gltf, [obj for obj in new_objects if obj.parent is None]) if gltf else {}
    # By name: point lights are replaced by objects of the same name below.
    matched = {index: obj.name for index, obj in matched.items()}

    for obj in new_objects:
        if c2data.is_stringified(obj):
            warnings.append(
                "%s: Blender could not store extras.c2 as properties (kept as text; "
                "the export writes it back as it was)" % obj.name
            )

    mode = context.window_manager.c2_maps.collision_colour if hasattr(context.window_manager, "c2_maps") else "AREA"
    for root in roots:
        collections = collision_collections(context, root)
        for piece in c2data.collision_pieces(root):
            kind = c2data.lookup(piece, ("kind",))
            for obj in c2data.descendants(piece):
                move_to(obj, collections[kind])
        for kind in collections:
            set_collision_visible(context, root, kind, False)
        colour_collision(root, mode)

        for obj in c2data.descendants(root):
            role = c2data.role(obj)
            if role == "placement" and obj.type == "EMPTY":
                obj.empty_display_type = "PLAIN_AXES"
                obj.empty_display_size = 20.0
            elif role == "group" and obj.type == "EMPTY":
                obj.empty_display_size = 5.0

        for obj in c2data.points(root):
            if c2data.lookup(obj, ("kind",)) == "plight":
                ensure_light(obj)
            else:
                point_display(obj)

    # KHR_lights_punctual range: Blender's importer leaves it out, its exporter writes a light's
    # custom distance.
    if gltf:
        lights = gltf.get("extensions", {}).get("KHR_lights_punctual", {}).get("lights", [])
        for index, name in matched.items():
            node = gltf["nodes"][index]
            light_index = node.get("extensions", {}).get("KHR_lights_punctual", {}).get("light")
            if light_index is None or light_index >= len(lights):
                continue
            obj = bpy.data.objects.get(name)
            if obj is not None and obj.type == "LIGHT" and "range" in lights[light_index]:
                obj.data.use_custom_distance = True
                obj.data.cutoff_distance = float(lights[light_index]["range"])

    for screen in getattr(context, "screen", None) and [context.screen] or []:
        for area in screen.areas:
            if area.type == "VIEW_3D":
                for space in area.spaces:
                    if space.type == "VIEW_3D":
                        space.clip_start = 1.0
                        space.clip_end = max(space.clip_end, 100000.0)
    return roots, warnings


def import_gltf(context, path):
    """Blender's glTF import with the options a map needs, then the post-processing."""
    before = set(bpy.data.objects)
    if context.mode != "OBJECT" and context.active_object is not None:
        bpy.ops.object.mode_set(mode="OBJECT")
    wanted = {
        "filepath": path,
        "merge_vertices": False,
        "import_shading": "NORMALS",
        "export_import_convert_lighting_mode": "RAW",
        "guess_original_bind_pose": False,
        "import_select_created_objects": True,
        "import_scene_extras": True,
        "import_scene_as_collection": True,
        "import_merge_material_slots": False,
    }
    try:
        result = bpy.ops.import_scene.gltf(**operator_kwargs(bpy.ops.import_scene.gltf, wanted))
    except RuntimeError as error:
        return None, ["Blender's glTF importer failed on %s: %s" % (path, str(error).strip().splitlines()[-1])]
    if "FINISHED" not in result:
        return None, ["Blender's glTF importer failed on %s" % path]
    new_objects = [obj for obj in bpy.data.objects if obj not in before]
    return postprocess(context, path, new_objects)


def find_export(folder, name):
    candidates = [
        os.path.join(folder, name + ".gltf"),
        os.path.join(folder, name, name + ".gltf"),
        os.path.join(folder, name + ".glb"),
    ]
    for candidate in candidates:
        if os.path.isfile(candidate):
            return candidate
    found = sorted(
        glob.glob(os.path.join(folder, "**", "*.gltf"), recursive=True),
        key=os.path.getmtime,
        reverse=True,
    )
    return found[0] if found else None


def report_result(operator, roots, warnings):
    for warning in warnings[:20]:
        operator.report({"WARNING"}, warning)
    if len(warnings) > 20:
        operator.report({"WARNING"}, "... and %d more warnings" % (len(warnings) - 20))
    if roots:
        operator.report({"INFO"}, "Imported %s" % ", ".join(root.name for root in roots))
        return {"FINISHED"}
    return {"CANCELLED"}


class C2_OT_import_gltf(bpy.types.Operator, ImportHelper):
    """Import a map scene that c2 map export wrote (keeps every extras.c2, hides collision)"""

    bl_idname = "c2.import_map_gltf"
    bl_label = "Import Dark Chronicle Map (.gltf)"
    bl_options = {"REGISTER", "UNDO"}

    filename_ext = ".gltf"
    filter_glob: StringProperty(default="*.gltf;*.glb", options={"HIDDEN"})

    def execute(self, context):
        roots, warnings = import_gltf(context, self.filepath)
        if roots is None:
            for warning in warnings:
                self.report({"ERROR"}, warning)
            return {"CANCELLED"}
        return report_result(self, roots, warnings)


class C2_OT_import_map(runner.C2_OT_run_job):
    """Export a map with c2 map export, then import it"""

    bl_idname = "c2.import_map"
    bl_label = "Import Dark Chronicle Map (c2)"
    bl_options = {"REGISTER", "UNDO"}

    map_name: StringProperty(name="Map", description="The map to export and import (s17, d01f01, ...)")
    out_dir: StringProperty(
        name="Folder",
        description="Where c2 writes the export; empty for <exported maps>/NAME",
        subtype="DIR_PATH",
    )

    def folder(self, context):
        if self.out_dir:
            return prefs.absolute(self.out_dir)
        base = prefs.maps_dir(prefs.get(context))
        return os.path.join(base, self.map_name) if base else ""

    def command(self, context):
        if not self.map_name:
            self.map_name = context.window_manager.c2_maps.import_name
        name = self.map_name.strip()
        self.map_name = name
        if not name:
            self.report({"ERROR"}, "Give the map's name (s17, d01f01, ...)")
            return None
        settings = prefs.get(context)
        c2 = prefs.c2_command(settings)
        if c2 is None:
            self.report({"ERROR"}, "c2 not found: set the repository (or c2 itself) in the add-on's preferences")
            return None
        folder = self.folder(context)
        if not folder:
            self.report({"ERROR"}, "No folder to export to: set the repository or the exported maps folder")
            return None
        os.makedirs(folder, exist_ok=True)
        # Split the template first, so a folder with spaces stays one argument.
        args = [part.format(name=name, out=folder) for part in settings.export_args.split()]
        return ("c2 map export", c2 + args, prefs.working_dir(settings))

    def finish(self, context, job):
        if job.returncode != 0:
            self.report({"ERROR"}, "c2 map export failed (exit %s); see the text \"%s\"" % (job.returncode, runner.TEXT_NAME))
            return {"CANCELLED"}
        path = find_export(self.folder(context), self.map_name)
        if path is None:
            self.report({"ERROR"}, "c2 wrote no .gltf into %s" % self.folder(context))
            return {"CANCELLED"}
        roots, warnings = import_gltf(context, path)
        if roots is None:
            for warning in warnings:
                self.report({"ERROR"}, warning)
            return {"CANCELLED"}
        return report_result(self, roots, warnings)

    def invoke(self, context, event):
        if not self.map_name and not context.window_manager.c2_maps.import_name:
            return context.window_manager.invoke_props_dialog(self)
        return super().invoke(context, event)


def menu_import(self, context):
    self.layout.operator(C2_OT_import_gltf.bl_idname, text="Dark Chronicle map (.gltf)")
    self.layout.operator(C2_OT_import_map.bl_idname, text="Dark Chronicle map (c2 map export)")


classes = (C2_OT_import_gltf, C2_OT_import_map)


def register():
    for cls in classes:
        bpy.utils.register_class(cls)
    bpy.types.TOPBAR_MT_file_import.append(menu_import)


def unregister():
    bpy.types.TOPBAR_MT_file_import.remove(menu_import)
    for cls in reversed(classes):
        bpy.utils.unregister_class(cls)
