"""Operators that change the map: adding points, duplicating placements, painting collision
attributes, editing any value of extras.c2."""

import json

import bmesh
import bpy
from bpy.props import BoolProperty, EnumProperty, IntProperty, StringProperty
from mathutils import Matrix

from . import c2data, import_map, schema


def unique_game_name(base, taken):
    """`base`, or `base_2`, `base_3`... not in `taken`, within the 31-byte limit."""
    base = base[: schema.NAME_LIMIT]
    if base not in taken:
        return base
    number = 2
    while True:
        tail = "_%d" % number
        candidate = base[: schema.NAME_LIMIT - len(tail)] + tail
        if candidate not in taken:
            return candidate
        number += 1


def link_like(obj, like, context):
    collections = like.users_collection if like is not None and like.users_collection else [context.collection]
    for collection in collections:
        collection.objects.link(obj)


def ensure_group(root, name, context):
    group = c2data.group_node(root, name)
    if group is not None:
        return group
    group = bpy.data.objects.new(name, None)
    group[c2data.KEY] = {"role": "group"}
    link_like(group, root, context)
    group.parent = root
    group.empty_display_size = 5.0
    return group


def place_at(obj, parent, world):
    obj.parent = parent
    obj.matrix_parent_inverse = Matrix.Identity(4)
    obj.rotation_mode = "XYZ"
    obj.matrix_world = world


class C2_OT_add_point(bpy.types.Operator):
    """Add a trigger point at the 3D cursor, under the selected placement or the map's points"""

    bl_idname = "c2.add_point"
    bl_label = "Add Point"
    bl_options = {"REGISTER", "UNDO"}

    preset: EnumProperty(
        name="Point",
        items=[(preset[0], preset[1], schema.KIND_LABELS[preset[2]]) for preset in schema.POINT_PRESETS],
        default="SPAWN",
    )
    name: StringProperty(name="Name", description="The point's name; empty for one made from its kind")
    in_placement: BoolProperty(
        name="In the selected placement",
        description="Put the point in the active object's placement (copied with its template) "
        "instead of the map's own points",
        default=True,
    )

    @classmethod
    def poll(cls, context):
        return c2data.current_map_root(context) is not None

    def execute(self, context):
        root = c2data.current_map_root(context)
        preset = next(p for p in schema.POINT_PRESETS if p[0] == self.preset)
        _, _, kind, overrides, scale, base = preset
        placement = c2data.placement_of(context.active_object) if self.in_placement else None
        parent = placement if placement is not None else ensure_group(root, "points", context)
        # Scripts find points by placement and name, so a name is unique within its parent.
        taken = {schema.game_name(obj.name, "point") for obj in parent.children if c2data.role(obj) == "point"}
        name = unique_game_name(self.name or base, taken)
        data = schema.new_point(kind, overrides)
        node_name = schema.PREFIX["point"] + name
        if kind == "plight":
            light = bpy.data.lights.new(name, "POINT")
            colour = [c / 255.0 for c in data["data"]["color"]]
            light.color = [min(1.0, c) for c in colour]
            light.energy = float(data["data"]["power"])
            obj = bpy.data.objects.new(node_name, light)
        else:
            obj = bpy.data.objects.new(node_name, None)
        obj[c2data.KEY] = data
        link_like(obj, parent, context)
        world = Matrix.Translation(context.scene.cursor.location)
        if obj.type == "LIGHT":
            world = world @ import_map.light_correction()
        place_at(obj, parent, world)
        if scale != 1.0:
            obj.scale = (scale, scale, scale)
        import_map.point_display(obj)
        for other in context.selected_objects:
            other.select_set(False)
        obj.select_set(True)
        context.view_layer.objects.active = obj
        self.report({"INFO"}, "Added %s under %s" % (obj.name, parent.name))
        return {"FINISHED"}


class C2_OT_duplicate_placement(bpy.types.Operator):
    """Duplicate the active placement with its pieces and points (meshes shared) under a new
    unique name; it is appended after the map's placements"""

    bl_idname = "c2.duplicate_placement"
    bl_label = "Duplicate Placement"
    bl_options = {"REGISTER", "UNDO"}

    name: StringProperty(name="Name", description="The new placement's name; empty for NAME_2, NAME_3...")
    at_cursor: BoolProperty(name="At the 3D cursor", default=False)

    @classmethod
    def poll(cls, context):
        return c2data.placement_of(context.active_object) is not None

    def execute(self, context):
        source = c2data.placement_of(context.active_object)
        root = c2data.map_root(source)
        taken = {schema.game_name(obj.name, "placement") for obj in c2data.placements(root)} if root else set()
        name = unique_game_name(self.name or schema.game_name(source.name, "placement"), taken)
        copies = {}

        def copy_tree(obj, parent):
            new = obj.copy()  # shares the mesh, copies the custom properties
            copies[obj] = new
            link_like(new, obj, context)
            new.parent = parent
            new.matrix_parent_inverse = obj.matrix_parent_inverse.copy()
            new.matrix_basis = obj.matrix_basis.copy()
            for child in obj.children:
                copy_tree(child, new)
            return new

        top = copy_tree(source, source.parent)
        top.name = schema.PREFIX["placement"] + name
        data = c2data.get(top) or {"role": "placement"}
        data.pop("slot", None)  # a new placement: the importer appends it
        c2data.put(top, data)
        if self.at_cursor:
            world = top.matrix_world.copy()
            world.translation = context.scene.cursor.location
            top.matrix_world = world
        for other in context.selected_objects:
            other.select_set(False)
        top.select_set(True)
        context.view_layer.objects.active = top
        self.report({"INFO"}, "Added placement %s (%d nodes)" % (top.name, len(copies)))
        return {"FINISHED"}


def collision_material(ground, foot, area, mask, mode="AREA"):
    name = schema.col_name(ground, foot, area, mask)
    material = bpy.data.materials.get(name)
    if material is None:
        material = bpy.data.materials.new(name)
    material.diffuse_color = schema.collision_colour((ground, foot, area, mask), mode)
    return material


def material_slot(obj, material):
    for index, slot in enumerate(obj.material_slots):
        if slot.material == material:
            return index
    obj.data.materials.append(material)
    return len(obj.data.materials) - 1


class C2_OT_paint_collision(bpy.types.Operator):
    """Give the selected faces of collision meshes a collision attribute (material col_...);
    in Object Mode, the faces last selected in Edit Mode, or every face when none is"""

    bl_idname = "c2.paint_collision"
    bl_label = "Set Collision Attributes"
    bl_options = {"REGISTER", "UNDO"}

    ground: IntProperty(name="Ground", min=0, max=255, default=0)
    foot: IntProperty(name="Foot sound", min=0, max=255, default=0)
    area: IntProperty(name="Area", min=0, max=255, default=6)
    mask: IntProperty(name="Ignore mask", min=0, max=255, default=0)
    only_collision: BoolProperty(
        name="Only collision meshes",
        description="Skip meshes that are not under a collision or camera-collision piece",
        default=True,
    )

    def invoke(self, context, event):
        settings = context.window_manager.c2_maps
        self.ground, self.foot = settings.paint_ground, settings.paint_foot
        self.area, self.mask = settings.paint_area, settings.paint_mask
        return self.execute(context)

    def execute(self, context):
        mode = context.window_manager.c2_maps.collision_colour
        material = collision_material(self.ground, self.foot, self.area, self.mask, mode)
        objects = [obj for obj in context.selected_objects if obj.type == "MESH"]
        if context.active_object is not None and context.active_object.type == "MESH":
            if context.active_object not in objects:
                objects.append(context.active_object)
        if self.only_collision:
            objects = [obj for obj in objects if c2data.is_collision_mesh(obj)]
        if not objects:
            self.report({"WARNING"}, "Select collision meshes (under a piece of kind collision or camera)")
            return {"CANCELLED"}
        faces = 0
        for obj in objects:
            index = material_slot(obj, material)
            if obj.mode == "EDIT":
                mesh = bmesh.from_edit_mesh(obj.data)
                for face in mesh.faces:
                    if face.select:
                        face.material_index = index
                        faces += 1
                bmesh.update_edit_mesh(obj.data)
            else:
                polygons = [polygon for polygon in obj.data.polygons if polygon.select]
                for polygon in polygons or obj.data.polygons:
                    polygon.material_index = index
                    faces += 1
                obj.data.update()
        self.report({"INFO"}, "%d faces set to %s" % (faces, material.name))
        return {"FINISHED"}


class C2_OT_recolour_collision(bpy.types.Operator):
    """Colour the map's collision materials by the chosen attribute (viewport colour only)"""

    bl_idname = "c2.recolour_collision"
    bl_label = "Recolour Collision"
    bl_options = {"REGISTER", "UNDO"}

    @classmethod
    def poll(cls, context):
        return c2data.current_map_root(context) is not None

    def execute(self, context):
        import_map.colour_collision(c2data.current_map_root(context), context.window_manager.c2_maps.collision_colour)
        return {"FINISHED"}


class C2_OT_toggle_collision(bpy.types.Operator):
    """Show or hide the map's collision (or camera collision)"""

    bl_idname = "c2.toggle_collision"
    bl_label = "Toggle Collision"
    bl_options = {"REGISTER", "UNDO"}

    kind: EnumProperty(items=(("collision", "Collision", ""), ("camera", "Camera collision", "")))

    @classmethod
    def poll(cls, context):
        return c2data.current_map_root(context) is not None

    def execute(self, context):
        root = c2data.current_map_root(context)
        visible = import_map.collision_visible(context, root, self.kind)
        if visible is None:
            collections = import_map.collision_collections(context, root)
            for piece in c2data.collision_pieces(root, (self.kind,)):
                for obj in c2data.descendants(piece):
                    import_map.move_to(obj, collections[self.kind])
            visible = False
        import_map.set_collision_visible(context, root, self.kind, not visible)
        return {"FINISHED"}


class C2_OT_set_event_kind(bpy.types.Operator):
    """Set the event point's kind"""

    bl_idname = "c2.set_event_kind"
    bl_label = "Set Event Kind"
    bl_options = {"REGISTER", "UNDO", "INTERNAL"}

    kind: StringProperty()

    def execute(self, context):
        obj = context.active_object
        if obj is None:
            return {"CANCELLED"}
        c2data.set_data_field(obj, schema.DATA_FIELDS["event"][0], self.kind)
        import_map.point_display(obj)
        return {"FINISHED"}


class C2_MT_event_kinds(bpy.types.Menu):
    bl_idname = "C2_MT_event_kinds"
    bl_label = "Event kind"

    def draw(self, context):
        for kind, label in schema.EVENT_KINDS:
            self.layout.operator(C2_OT_set_event_kind.bl_idname, text="%s (%s)" % (label, kind)).kind = kind


def parse_like(text, old):
    """The text typed for a value, as the type the value had."""
    text = text.strip()
    if isinstance(old, bool):
        return text.lower() in ("1", "true", "yes", "on")
    if isinstance(old, int):
        return int(float(text))
    if isinstance(old, float):
        return float(text)
    if isinstance(old, str):
        return text
    if isinstance(old, list) and not text.startswith("["):
        parts = [part.strip() for part in text.split(",")] if text else []
        if old and all(isinstance(item, (int, float)) and not isinstance(item, bool) for item in old):
            return [int(part) if isinstance(old[0], int) and "." not in part else float(part) for part in parts]
        return [json.loads(part) if part[:1] in "-0123456789[{\"tfn" else part for part in parts]
    return json.loads(text)


def show_value(value):
    if isinstance(value, list) and all(isinstance(v, (int, float, str)) for v in value):
        return ", ".join(str(v) for v in value)
    if isinstance(value, (dict, list)) or value is None:
        return json.dumps(value)
    return str(value)


class C2_OT_edit_value(bpy.types.Operator):
    """Edit this value of extras.c2"""

    bl_idname = "c2.edit_value"
    bl_label = "Edit Value"
    bl_options = {"REGISTER", "UNDO", "INTERNAL"}

    object_name: StringProperty()
    path: StringProperty(description="JSON list of keys and indices inside extras.c2")
    value: StringProperty(name="Value", description="Numbers, text, or comma-separated values for a list")

    def target(self):
        return bpy.data.objects.get(self.object_name)

    def invoke(self, context, event):
        obj = self.target()
        if obj is None:
            return {"CANCELLED"}
        self.value = show_value(c2data.to_py(c2data.lookup(obj, json.loads(self.path))))
        return context.window_manager.invoke_props_dialog(self, width=420)

    def draw(self, context):
        self.layout.label(text="%s  %s" % (self.object_name, " / ".join(str(p) for p in json.loads(self.path))))
        self.layout.prop(self, "value", text="")

    def execute(self, context):
        obj = self.target()
        if obj is None:
            return {"CANCELLED"}
        path = json.loads(self.path)
        old = c2data.to_py(c2data.lookup(obj, path))
        try:
            new = parse_like(self.value, old)
        except (ValueError, TypeError) as error:
            self.report({"ERROR"}, "Not a value for %s: %s" % (path[-1], error))
            return {"CANCELLED"}
        c2data.assign(obj, path, new)
        return {"FINISHED"}


class C2_OT_clear_value(bpy.types.Operator):
    """Remove this value (an optional field of the .map line, such as FAR_CLIP)"""

    bl_idname = "c2.clear_value"
    bl_label = "Clear Value"
    bl_options = {"REGISTER", "UNDO", "INTERNAL"}

    object_name: StringProperty()
    path: StringProperty()
    to_null: BoolProperty(default=True, description="Keep the key with null instead of removing it")

    def execute(self, context):
        obj = bpy.data.objects.get(self.object_name)
        if obj is None:
            return {"CANCELLED"}
        path = json.loads(self.path)
        if self.to_null:
            c2data.assign(obj, path, None)
        else:
            c2data.remove(obj, path)
        return {"FINISHED"}


class C2_OT_set_value(bpy.types.Operator):
    """Set a value of extras.c2 to the given JSON"""

    bl_idname = "c2.set_value"
    bl_label = "Set Value"
    bl_options = {"REGISTER", "UNDO", "INTERNAL"}

    object_name: StringProperty()
    path: StringProperty()
    json_value: StringProperty()

    def execute(self, context):
        obj = bpy.data.objects.get(self.object_name)
        if obj is None:
            return {"CANCELLED"}
        c2data.assign(obj, json.loads(self.path), json.loads(self.json_value))
        return {"FINISHED"}


expanded = set()


class C2_OT_toggle_expand(bpy.types.Operator):
    """Show or hide the values inside"""

    bl_idname = "c2.toggle_expand"
    bl_label = "Expand"
    bl_options = {"INTERNAL"}

    key: StringProperty()

    def execute(self, context):
        if self.key in expanded:
            expanded.discard(self.key)
        else:
            expanded.add(self.key)
        return {"FINISHED"}


class C2_OT_add_c2(bpy.types.Operator):
    """Give the active object extras.c2 of a role (for objects added by hand)"""

    bl_idname = "c2.add_extras"
    bl_label = "Add c2 Extras"
    bl_options = {"REGISTER", "UNDO"}

    role: EnumProperty(
        items=(
            ("point", "Point", ""),
            ("placement", "Placement", ""),
            ("piece", "Piece", ""),
            ("mds", "MDS node", ""),
            ("group", "Group", ""),
        )
    )

    @classmethod
    def poll(cls, context):
        return context.active_object is not None

    def execute(self, context):
        obj = context.active_object
        if self.role == "point":
            data = schema.new_point("pos")
        elif self.role == "placement":
            data = {"role": "placement", "template": "", "show": 1, "group": "", "far_clip": None}
        elif self.role == "piece":
            data = {"role": "piece", "model": "", "kind": "model", "show": 1, "time": [0, 0], "col_type": 0, "materials": []}
        elif self.role == "mds":
            data = {"role": "mds"}
        else:
            data = {"role": "group"}
        c2data.put(obj, data)
        prefix = schema.PREFIX.get(self.role)
        if prefix and not obj.name.startswith(prefix):
            obj.name = prefix + obj.name
        return {"FINISHED"}


classes = (
    C2_OT_add_point,
    C2_OT_duplicate_placement,
    C2_OT_paint_collision,
    C2_OT_recolour_collision,
    C2_OT_toggle_collision,
    C2_OT_set_event_kind,
    C2_MT_event_kinds,
    C2_OT_edit_value,
    C2_OT_clear_value,
    C2_OT_set_value,
    C2_OT_toggle_expand,
    C2_OT_add_c2,
)


def register():
    for cls in classes:
        bpy.utils.register_class(cls)


def unregister():
    for cls in reversed(classes):
        bpy.utils.unregister_class(cls)
