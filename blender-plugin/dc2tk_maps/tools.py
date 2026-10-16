"""Tools for laying a map out: dropping things onto the ground, the game's snapping, game
coordinates for scripts, keeping a template's placements alike, and a player to measure by."""

import math
import os

import bpy
from bpy.props import BoolProperty, EnumProperty, FloatProperty, StringProperty
from mathutils import Matrix, Vector

from . import c2data, edit, prefs, schema


# --- the ground -----------------------------------------------------------------------------


def ground_tree(root, kinds=("collision",), skip=()):
    """A BVH tree of the map's floor and wall collision in world space (or of its drawn meshes,
    kinds ("model",)), leaving out the objects in skip."""
    from mathutils.bvhtree import BVHTree

    vertices, polygons = [], []
    for obj in c2data.descendants(root):
        if obj.type != "MESH" or obj in skip:
            continue
        piece = c2data.piece_of(obj)
        kind = c2data.lookup(piece, ("kind",)) if piece is not None else None
        if kind not in kinds:
            continue
        world = c2data.world_matrix(obj)
        base = len(vertices)
        vertices.extend(world @ vertex.co for vertex in obj.data.vertices)
        polygons.extend([base + index for index in polygon.vertices] for polygon in obj.data.polygons)
    if not polygons:
        return None
    return BVHTree.FromPolygons(vertices, polygons)


def ground_below(tree, point, above):
    """Where the ground is under point: the first hit of a ray cast down from `above` units over
    it, as (location, normal), or None."""
    start = point + Vector((0.0, 0.0, above))
    location, normal, _index, _distance = tree.ray_cast(start, Vector((0.0, 0.0, -1.0)))
    if location is None:
        return None
    return location, normal


def top_selected(context):
    """The selected objects none of whose parents is selected too, so a placement moves once and
    its pieces with it."""
    selected = set(context.selected_objects)
    found = []
    for obj in context.selected_objects:
        parent = obj.parent
        while parent is not None and parent not in selected:
            parent = parent.parent
        if parent is None:
            found.append(obj)
    return found


class DC2TK_OT_drop_to_ground(bpy.types.Operator):
    """Move the selected placements, points and objects down (or up) onto the map's floor
    collision, as the player would stand there"""

    bl_idname = "dc2tk.drop_to_ground"
    bl_label = "Drop to Ground"
    bl_options = {"REGISTER", "UNDO"}

    onto: EnumProperty(
        name="Onto",
        items=(
            ("COLLISION", "Floor collision", "The map's floor and wall collision, where the player walks"),
            ("DRAWN", "Drawn meshes", "What the map draws (for areas without collision yet)"),
        ),
        default="COLLISION",
    )
    above: FloatProperty(
        name="Search above",
        description="How far above each object to look for the ground, in game units",
        default=200.0,
        min=0.0,
    )
    offset: FloatProperty(name="Height above the ground", default=0.0)
    align: BoolProperty(name="Align to the slope", description="Turn each object to the ground's slope", default=False)

    @classmethod
    def poll(cls, context):
        return c2data.current_map_root(context) is not None and bool(context.selected_objects)

    def execute(self, context):
        root = c2data.current_map_root(context)
        moving = top_selected(context)
        skip = {obj for top in moving for obj in c2data.descendants(top)}
        kinds = ("collision",) if self.onto == "COLLISION" else ("model",)
        tree = ground_tree(root, kinds, skip)
        if tree is None:
            self.report({"ERROR"}, "The map has no %s to drop onto" % ("floor collision" if self.onto == "COLLISION" else "meshes"))
            return {"CANCELLED"}
        dropped, missed = 0, []
        for obj in moving:
            world = obj.matrix_world.copy()
            hit = ground_below(tree, world.translation, self.above)
            if hit is None:
                missed.append(obj.name)
                continue
            location, normal = hit
            if self.align:
                turn = Vector((0.0, 0.0, 1.0)).rotation_difference(normal).to_matrix().to_4x4()
                world = Matrix.Translation(world.translation) @ turn @ Matrix.Translation(-world.translation) @ world
            world.translation = location + Vector((0.0, 0.0, self.offset))
            obj.matrix_world = world
            dropped += 1
        if missed:
            self.report({"WARNING"}, "No ground under %s" % ", ".join(missed[:5]))
        self.report({"INFO"}, "Dropped %d onto the ground" % dropped)
        return {"FINISHED"} if dropped else {"CANCELLED"}


# --- snapping -------------------------------------------------------------------------------


SNAPPING = {
    # grid, rotation step: Georama turns parts in 15 degree steps, dungeon cells are 320 apart
    "TOWN": (10.0, 15.0),
    "CELL": (schema.DUNGEON_CELL, 90.0),
    "FREE": (None, None),
}


class DC2TK_OT_game_snapping(bpy.types.Operator):
    """Set the viewport's grid and Blender's snapping to the game's steps: 10 units and 15 degrees
    in towns, 320-unit cells and quarter turns for dungeon parts, or snapping off"""

    bl_idname = "dc2tk.game_snapping"
    bl_label = "Game Snapping"
    bl_options = {"REGISTER", "UNDO"}

    preset: EnumProperty(
        name="Steps",
        items=(
            ("TOWN", "Town (10 units, 15°)", "A grid of 10 game units, turning in 15 degree steps"),
            ("CELL", "Dungeon cell (320, 90°)", "A grid of dungeon cells, turning in quarter turns"),
            ("FREE", "Off", "No snapping"),
        ),
        default="TOWN",
    )

    def execute(self, context):
        grid, angle = SNAPPING[self.preset]
        tool = context.scene.tool_settings
        if grid is None:
            tool.use_snap = False
            return {"FINISHED"}
        tool.use_snap = True
        elements = {"INCREMENT"}
        tool.snap_elements = elements
        if hasattr(tool, "use_snap_grid_absolute"):
            tool.use_snap_grid_absolute = True
        if hasattr(tool, "snap_angle_increment_3d"):
            tool.snap_angle_increment_3d = math.radians(angle)
        if hasattr(tool, "use_snap_rotate"):
            tool.use_snap_rotate = True
        for area in context.screen.areas if context.screen else []:
            if area.type == "VIEW_3D":
                overlay = area.spaces.active.overlay
                overlay.grid_scale = grid
                overlay.grid_subdivisions = 1 if self.preset == "CELL" else 10
        context.scene.unit_settings.system = "NONE"
        self.report({"INFO"}, "Snapping to %g units and %g°" % (grid, angle))
        return {"FINISHED"}


# --- game coordinates -----------------------------------------------------------------------

AXES = Matrix(((1.0, 0.0, 0.0), (0.0, 0.0, -1.0), (0.0, 1.0, 0.0)))  # game to Blender


def game_position(world):
    return schema.blender_to_game(world.translation)


def game_rotation(world):
    """A world matrix's rotation as the game's Euler XYZ angles (radians, T·Rz·Ry·Rx·S)."""
    turn = AXES.inverted() @ world.to_3x3().normalized() @ AXES
    return tuple(turn.to_euler("XYZ"))


def number(value):
    text = "%.2f" % value
    text = text.rstrip("0").rstrip(".") if "." in text else text
    return "0" if text in ("-0", "") else text


def coordinates_text(world, form, name="SPOT"):
    x, y, z = (number(v) for v in game_position(world))
    yaw = number(game_rotation(world)[1])
    if form == "PLACE":
        return "place_chara(CHARA_PLAYER, %s, %s, %s, %s);" % (x, y, z, yaw)
    if form == "CAMERA":
        return "set_camera_pos(%s, %s, %s);" % (x, y, z)
    if form == "TARGET":
        return "set_camera_target(%s, %s, %s);" % (x, y, z)
    if form == "DEFINE":
        key = "".join(c if c.isalnum() else "_" for c in name.upper()) or "SPOT"
        return "#define %s_X %s\n#define %s_Y %s\n#define %s_Z %s\n#define %s_YAW %s" % (key, x, key, y, key, z, key, yaw)
    return "%s, %s, %s" % (x, y, z)


class DC2TK_OT_copy_coordinates(bpy.types.Operator):
    """Copy where the active object (or the 3D cursor) is in game coordinates, for a script:
    x, y, z with y up, and the yaw in radians as the game turns things"""

    bl_idname = "dc2tk.copy_coordinates"
    bl_label = "Copy Game Coordinates"
    bl_options = {"REGISTER"}

    form: EnumProperty(
        name="As",
        items=(
            ("XYZ", "x, y, z", "The position alone"),
            ("PLACE", "place_chara(...)", "Stand the player there, facing the same way"),
            ("CAMERA", "set_camera_pos(...)", "Put the camera there"),
            ("TARGET", "set_camera_target(...)", "Point the camera there"),
            ("DEFINE", "#define NAME_X ...", "Named constants for a header"),
        ),
        default="PLACE",
    )
    source: EnumProperty(
        name="Of",
        items=(("ACTIVE", "Active object", ""), ("CURSOR", "3D cursor", "")),
        default="ACTIVE",
    )

    def execute(self, context):
        obj = context.active_object
        if self.source == "CURSOR" or obj is None:
            world = context.scene.cursor.matrix.copy()
            name = "CURSOR"
        else:
            world = obj.matrix_world.copy()
            if obj.type == "LIGHT":
                world = world @ Matrix.Rotation(math.radians(-90.0), 4, "X")
            name = c2data.exact_name(obj, c2data.role(obj))
        text = coordinates_text(world, self.form, name)
        context.window_manager.clipboard = text
        self.report({"INFO"}, "Copied: %s" % text.replace("\n", "  "))
        return {"FINISHED"}


# --- a template's placements ----------------------------------------------------------------


def copy_contents(source, target, context):
    """Gives target placement source's pieces and points: each matched by role and name gets
    source's transform and values (and its model nodes' transforms), source's others are copied in,
    and target's that source lacks are removed."""
    theirs = {}
    for child in list(target.children):
        if c2data.role(child) in ("piece", "point"):
            theirs.setdefault((c2data.role(child), c2data.exact_name(child, c2data.role(child))), child)
    kept = set()
    for child in source.children:
        role = c2data.role(child)
        if role not in ("piece", "point"):
            continue
        key = (role, c2data.exact_name(child, role))
        match = theirs.get(key)
        if match is None:
            edit.copy_object_tree(child, target, context)
            continue
        kept.add(match)
        copy_node(child, match)
    for child in theirs.values():
        if child not in kept:
            bpy.data.batch_remove(c2data.descendants(child))


def copy_node(source, target):
    target.matrix_parent_inverse = source.matrix_parent_inverse.copy()
    target.matrix_basis = source.matrix_basis.copy()
    data = c2data.get(source)
    if data is not None:
        c2data.put(target, data)
    if source.type == "LIGHT" and target.type == "LIGHT":
        target.data.color = source.data.color
        target.data.energy = source.data.energy
    by_name = {}
    for child in target.children:
        by_name.setdefault(schema.strip_suffix(child.name), child)
    for child in source.children:
        match = by_name.get(schema.strip_suffix(child.name))
        if match is not None:
            copy_node(child, match)


class DC2TK_OT_apply_to_instances(bpy.types.Operator):
    """Give every placement of the active placement's template its pieces and points as they are
    here, so the build changes the template instead of giving this placement a copy of its own"""

    bl_idname = "dc2tk.apply_to_instances"
    bl_label = "Apply to All Instances"
    bl_options = {"REGISTER", "UNDO"}

    @classmethod
    def poll(cls, context):
        placement = c2data.placement_of(context.active_object)
        return placement is not None and len(c2data.instances(placement)) > 1

    def execute(self, context):
        source = c2data.placement_of(context.active_object)
        others = c2data.instances(source)[1:]
        for target in others:
            copy_contents(source, target, context)
        self.report(
            {"INFO"}, "%d placements of %s now hold what %s does" % (len(others), c2data.template_name(source), source.name)
        )
        return {"FINISHED"}


class DC2TK_OT_make_unique(bpy.types.Operator):
    """Give the active placement a template of its own (a new name), so its edits change it alone
    and no other placement of its template"""

    bl_idname = "dc2tk.make_unique"
    bl_label = "Make Template Unique"
    bl_options = {"REGISTER", "UNDO"}

    name: StringProperty(name="Template", description="The new template's name; empty for NAME_2, NAME_3...")

    @classmethod
    def poll(cls, context):
        placement = c2data.placement_of(context.active_object)
        return placement is not None and not c2data.is_preview(placement)

    def execute(self, context):
        placement = c2data.placement_of(context.active_object)
        root = c2data.map_root(placement)
        taken = c2data.template_names(root) | {c2data.exact_name(obj, "placement") for obj in c2data.placements(root)}
        name = edit.unique_game_name(self.name or c2data.template_name(placement), taken)
        c2data.assign(placement, ("template",), name)
        self.report({"INFO"}, "%s now places a template of its own, %s" % (placement.name, name))
        return {"FINISHED"}


class DC2TK_OT_select_instances(bpy.types.Operator):
    """Select every placement of the active placement's template"""

    bl_idname = "dc2tk.select_instances"
    bl_label = "Select Instances"
    bl_options = {"REGISTER", "UNDO"}

    @classmethod
    def poll(cls, context):
        return c2data.placement_of(context.active_object) is not None

    def execute(self, context):
        placement = c2data.placement_of(context.active_object)
        found = c2data.instances(placement)
        for obj in context.selected_objects:
            obj.select_set(False)
        for obj in found:
            if obj.name in context.view_layer.objects:
                obj.select_set(True)
        context.view_layer.objects.active = placement
        self.report({"INFO"}, "%d placements of %s" % (len(found), c2data.template_name(placement)))
        return {"FINISHED"}


# --- a player to measure by ------------------------------------------------------------------

REFERENCE = "c2 player"


def reference_model(context):
    """Max's model as the asset tree has it (models/chara/c01_base), when the repository has one."""
    root = prefs.repo(prefs.get(context))
    path = os.path.join(root, "extracted", "assets", "models", "chara", "c01_base", "c01_base.gltf") if root else ""
    return path if path and os.path.isfile(path) else None


def reference_collection(context):
    collection = bpy.data.collections.get(REFERENCE)
    if collection is None:
        collection = bpy.data.collections.new(REFERENCE)
        context.scene.collection.children.link(collection)
    return collection


def stand_in(collection):
    """A cylinder of the player's radius (15) and Max's height, with a ring 20 below the feet
    where the engine looks for the floor."""
    import bmesh

    mesh = bpy.data.meshes.new(REFERENCE)
    shape = bmesh.new()
    try:
        bmesh.ops.create_cone(
            shape, cap_ends=True, segments=24, radius1=schema.PLAYER_RADIUS, radius2=schema.PLAYER_RADIUS,
            depth=30.0, matrix=Matrix.Translation((0.0, 0.0, 15.0)),
        )
        shape.to_mesh(mesh)
    finally:
        shape.free()
    obj = bpy.data.objects.new(REFERENCE, mesh)
    obj.display_type = "WIRE"
    collection.objects.link(obj)
    return obj


class DC2TK_OT_player_reference(bpy.types.Operator):
    """Stand Max (or a cylinder of the player's size) at the 3D cursor, to measure doors, steps
    and paths by; it is not part of the map and is never exported"""

    bl_idname = "dc2tk.player_reference"
    bl_label = "Player for Scale"
    bl_options = {"REGISTER", "UNDO"}

    def execute(self, context):
        collection = reference_collection(context)
        for obj in list(collection.objects):
            bpy.data.objects.remove(obj)
        path = reference_model(context)
        made = []
        if path is not None:
            before = set(bpy.data.objects)
            layer = context.view_layer.active_layer_collection
            try:
                from .import_map import find_layer_collection, operator_kwargs

                found = find_layer_collection(context.view_layer.layer_collection, collection)
                if found is not None:
                    context.view_layer.active_layer_collection = found
                bpy.ops.import_scene.gltf(**operator_kwargs(bpy.ops.import_scene.gltf, {"filepath": path}))
            except RuntimeError:
                pass
            finally:
                context.view_layer.active_layer_collection = layer
            made = [obj for obj in bpy.data.objects if obj not in before]
        if not made:
            made = [stand_in(collection)]
        for obj in made:
            for other in list(obj.users_collection):
                if other != collection:
                    other.objects.unlink(obj)
            if collection not in obj.users_collection:
                collection.objects.link(obj)
        tops = [obj for obj in made if obj.parent is None]
        for obj in tops:
            obj.location = context.scene.cursor.location
        self.report({"INFO"}, "Max stands at the cursor" if path else "A stand-in of the player's size stands at the cursor")
        return {"FINISHED"}


classes = (
    DC2TK_OT_drop_to_ground,
    DC2TK_OT_game_snapping,
    DC2TK_OT_copy_coordinates,
    DC2TK_OT_apply_to_instances,
    DC2TK_OT_make_unique,
    DC2TK_OT_select_instances,
    DC2TK_OT_player_reference,
)


def register():
    for cls in classes:
        bpy.utils.register_class(cls)


def unregister():
    for cls in reversed(classes):
        bpy.utils.unregister_class(cls)
