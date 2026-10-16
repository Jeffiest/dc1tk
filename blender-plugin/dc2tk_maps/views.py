"""Seeing the map as the game frames it, and how a new mesh is drawn: a fixed camera to look
through (52 degrees, the engine's only projection), and a new model node's draw flags, which the
engine reads from the node's name (`--` then aPP, aNN, aOF, aXX, c0, z0, fN)."""

import math
import re

import bpy
from bpy.props import BoolProperty, EnumProperty, IntProperty
from mathutils import Vector

from . import c2data, look, schema

VIEW = "c2 view"


# --- fixed cameras ---------------------------------------------------------------------------


def camera_entry(root, index):
    cameras = c2data.to_py(c2data.lookup(root, ("map", "cameras")))
    return cameras[index] if isinstance(cameras, list) and 0 <= index < len(cameras) and isinstance(cameras[index], dict) else None


def aim_of(root, obj, entry):
    """What a fixed camera looks at: the middle of its first box (where the player is when it
    takes over), else the middle of the map's points."""
    from . import overlay

    if entry:
        for rect in entry.get("rects") or []:
            if isinstance(rect, dict):
                low = overlay._numbers(rect.get("min"), 3)
                high = overlay._numbers(rect.get("max"), 3)
                centre = [(a + b) / 2.0 for a, b in zip(low, high)]
                if rect.get("pos") is not None:
                    matrix = overlay._game_matrix(
                        overlay._numbers(rect.get("pos"), 3), overlay._numbers(rect.get("rot"), 3),
                        overlay._numbers(rect.get("scale"), 3, 1.0),
                    )
                    return root.matrix_world @ matrix @ Vector(schema.game_to_blender(centre))
                return root.matrix_world @ Vector(schema.game_to_blender(centre))
    spots = [point.matrix_world.translation for point in c2data.points(root)]
    if spots:
        return sum(spots, Vector()) / len(spots)
    return obj.matrix_world.translation + Vector((0.0, 100.0, 0.0))


class DC2TK_OT_view_camera(bpy.types.Operator):
    """Look through the active fixed camera (or the first), as the game would: 52 degrees, aimed at
    the middle of its first box"""

    bl_idname = "dc2tk.view_camera"
    bl_label = "Look Through Fixed Camera"
    bl_options = {"REGISTER", "UNDO"}

    @classmethod
    def poll(cls, context):
        root = c2data.current_map_root(context)
        return root is not None and any(c2data.role(obj) == "camera" for obj in c2data.descendants(root))

    def execute(self, context):
        root = c2data.current_map_root(context)
        active = context.active_object
        cameras = [obj for obj in c2data.descendants(root) if c2data.role(obj) == "camera"]
        obj = active if active in cameras else cameras[0]
        index = c2data.lookup(obj, ("index",), 0)
        entry = camera_entry(root, int(index) if isinstance(index, (int, float)) else 0)
        collection = bpy.data.collections.get(VIEW)
        if collection is None:
            collection = bpy.data.collections.new(VIEW)
            context.scene.collection.children.link(collection)
        view = bpy.data.objects.get(VIEW)
        if view is None or view.type != "CAMERA":
            view = bpy.data.objects.new(VIEW, bpy.data.cameras.new(VIEW))
            collection.objects.link(view)
        view.data.angle = math.radians(52.0)
        view.data.clip_start = 1.0
        view.data.clip_end = 100000.0
        at = obj.matrix_world.translation.copy()
        towards = aim_of(root, obj, entry) - at
        view.location = at
        if towards.length > 1e-6:
            view.rotation_euler = towards.to_track_quat("-Z", "Y").to_euler()
        context.scene.camera = view
        for area in context.screen.areas if context.screen else []:
            if area.type == "VIEW_3D":
                area.spaces.active.region_3d.view_perspective = "CAMERA"
        self.report({"INFO"}, "Looking through fixed camera %s" % index)
        return {"FINISHED"}


# --- a new mesh's draw flags ------------------------------------------------------------------

FLAG_RE = re.compile(r"^(.*?)--(.*)$")


def flags_text(blend, alpha_ref, unlit, no_depth, fog):
    parts = []
    if alpha_ref > 0:
        parts.append("a%02x" % min(255, alpha_ref))
    parts.append({"ADD": "aPP", "SUBTRACT": "aNN", "OPAQUE": "aOF"}.get(blend, ""))
    if unlit:
        parts.append("c0")
    if no_depth:
        parts.append("z0")
    if fog >= 0:
        parts.append("f%d" % fog)
    return "".join(parts)


def frame_flags(blend, alpha_ref, unlit, no_depth, fog, own):
    """The material's frame_flags as the exporter writes them, for the game look."""
    flags = {"mds_flags": own}
    if blend != "NONE":
        flags["blend"] = blend.lower()
    if alpha_ref > 0:
        flags["alpha_ref"] = alpha_ref
    if unlit:
        flags["lit"] = False
    if no_depth:
        flags["z_write"] = False
    if fog >= 0:
        flags["fog"] = fog
    return flags


def is_new_node(obj):
    return obj is not None and obj.type == "MESH" and c2data.role(obj) == "mds" and "mds_index" not in (c2data.get(obj) or {}) \
        and not c2data.is_collision_mesh(obj)


class DC2TK_OT_draw_flags(bpy.types.Operator):
    """Set how the game draws the active new mesh: blending, alpha test, lighting, depth and fog,
    written into its name after "--" as the engine reads them (the disc's own models keep theirs)"""

    bl_idname = "dc2tk.draw_flags"
    bl_label = "Draw Flags"
    bl_options = {"REGISTER", "UNDO"}

    blend: EnumProperty(
        name="Blend",
        items=(
            ("NONE", "As its alpha", "Blended where the picture or colour is see-through"),
            ("OPAQUE", "Opaque", "aOF: drawn solid"),
            ("ADD", "Additive", "aPP: adds its colour (glows, light shafts)"),
            ("SUBTRACT", "Subtractive", "aNN: takes its colour away (shadows)"),
        ),
        default="NONE",
    )
    alpha_ref: IntProperty(name="Alpha test", description="aXX: pixels below this alpha (0-128) are not drawn; 0 off", min=0, max=255)
    unlit: BoolProperty(name="Unlit", description="c0: drawn without the scene's lights")
    no_depth: BoolProperty(name="No depth write", description="z0: does not hide what is drawn after it")
    fog: IntProperty(name="Fog", description="fN: the fog setting; -1 leaves it out", default=-1, min=-1, max=9)

    @classmethod
    def poll(cls, context):
        return is_new_node(context.active_object)

    def invoke(self, context, event):
        obj = context.active_object
        found = FLAG_RE.match(schema.strip_suffix(obj.name))
        flags = found.group(2) if found else ""
        lowered = flags.lower()
        self.blend = "ADD" if "app" in lowered else "SUBTRACT" if "ann" in lowered else "OPAQUE" if "aof" in lowered else "NONE"
        self.unlit = "c0" in lowered
        self.no_depth = "z0" in lowered
        fog = re.search(r"f(\d)", lowered)
        self.fog = int(fog.group(1)) if fog else -1
        alpha = re.search(r"a([0-9a-f]{2})", lowered.replace("app", "").replace("ann", "").replace("aof", ""))
        self.alpha_ref = int(alpha.group(1), 16) if alpha else 0
        return context.window_manager.invoke_props_dialog(self)

    def execute(self, context):
        obj = context.active_object
        found = FLAG_RE.match(schema.strip_suffix(obj.name))
        base = found.group(1) if found else schema.strip_suffix(obj.name)
        own = flags_text(self.blend, self.alpha_ref, self.unlit, self.no_depth, self.fog)
        obj.name = "%s--%s" % (base, own) if own else base
        for slot in obj.material_slots:
            if slot.material is not None:
                slot.material["frame_flags"] = frame_flags(self.blend, self.alpha_ref, self.unlit, self.no_depth, self.fog, own)
                look.apply(slot.material)
        self.report({"INFO"}, "%s draws as %s" % (obj.name, own or "its material says"))
        return {"FINISHED"}


def draw_flags(layout, obj):
    """The active mesh's draw flags: editable for a new mesh, shown for the disc's."""
    material = obj.active_material
    flags = c2data.to_py(material.get("frame_flags")) if material is not None else None
    box = layout.box()
    if isinstance(flags, dict):
        shown = ", ".join("%s %s" % (key, value) for key, value in flags.items() if key not in ("mds_flags", "inherited_flags"))
        box.label(text="Draws: %s" % (shown or "as its material says"), icon="SHADING_RENDERED")
    if is_new_node(obj):
        box.operator(DC2TK_OT_draw_flags.bl_idname, icon="MODIFIER")
    else:
        box.label(text="The disc's node: its flags stay as its name gives them", icon="INFO")


classes = (DC2TK_OT_view_camera, DC2TK_OT_draw_flags)


def register():
    for cls in classes:
        bpy.utils.register_class(cls)


def unregister():
    for cls in reversed(classes):
        bpy.utils.unregister_class(cls)
