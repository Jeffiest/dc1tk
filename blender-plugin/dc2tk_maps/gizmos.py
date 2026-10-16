"""Handles in the 3D view for the active point's sizes: an invent box's corners (a box to drag
and scale) and a sound's near and far distances (two arrows)."""

import bpy
from mathutils import Matrix, Vector

from . import c2data, schema


def _active_point(context, kind):
    obj = context.active_object
    if obj is None or c2data.role(obj) != "point" or c2data.lookup(obj, ("kind",)) != kind:
        return None
    return obj


def _box(obj):
    fields = schema.DATA_FIELDS["invent"]
    low = [float(v) for v in c2data.data_field(obj, fields[1])]
    high = [float(v) for v in c2data.data_field(obj, fields[2])]
    corners = [Vector(schema.game_to_blender(corner)) for corner in (low, high)]
    lowest = Vector([min(a, b) for a, b in zip(*corners)])
    highest = Vector([max(a, b) for a, b in zip(*corners)])
    return lowest, highest


def box_matrix(obj):
    """The invent box as a cage's matrix in the point's space: its centre and size."""
    lowest, highest = _box(obj)
    size = [max(1e-3, h - l) for l, h in zip(lowest, highest)]
    return Matrix.Translation((lowest + highest) / 2.0) @ Matrix.Diagonal(size + [1.0])


def set_box(obj, matrix):
    """The cage moved or scaled: the box's min and max (in game axes) from its matrix."""
    centre = matrix.translation
    size = matrix.to_scale()
    lowest = [c - abs(s) / 2.0 for c, s in zip(centre, size)]
    highest = [c + abs(s) / 2.0 for c, s in zip(centre, size)]
    a = schema.blender_to_game(lowest)
    b = schema.blender_to_game(highest)
    low = [round(min(x, y), 3) for x, y in zip(a, b)]
    high = [round(max(x, y), 3) for x, y in zip(a, b)]
    fields = schema.DATA_FIELDS["invent"]
    c2data.set_data_field(obj, fields[1], low)
    c2data.set_data_field(obj, fields[2], high)


class DC2TK_GGT_invent_box(bpy.types.GizmoGroup):
    bl_idname = "DC2TK_GGT_invent_box"
    bl_label = "Invent box"
    bl_space_type = "VIEW_3D"
    bl_region_type = "WINDOW"
    bl_options = {"3D", "PERSISTENT"}

    @classmethod
    def poll(cls, context):
        return _active_point(context, "invent") is not None

    def setup(self, context):
        gizmo = self.gizmos.new("GIZMO_GT_cage_3d")
        gizmo.transform = {"TRANSLATE", "SCALE"}
        gizmo.color = 0.3, 0.9, 0.9
        gizmo.alpha = 0.5
        gizmo.color_highlight = 1.0, 0.7, 0.2
        gizmo.alpha_highlight = 0.8

        def get():
            obj = _active_point(bpy.context, "invent")
            matrix = box_matrix(obj) if obj is not None else Matrix.Identity(4)
            return [value for column in matrix.transposed() for value in column]

        def set_(values):
            obj = _active_point(bpy.context, "invent")
            if obj is not None:
                set_box(obj, Matrix([values[0:4], values[4:8], values[8:12], values[12:16]]).transposed())

        gizmo.target_set_handler("matrix", get=get, set=set_)
        self.box = gizmo

    def refresh(self, context):
        obj = _active_point(context, "invent")
        if obj is not None:
            self.box.matrix_basis = obj.matrix_world.copy()


class DC2TK_GGT_sound_range(bpy.types.GizmoGroup):
    bl_idname = "DC2TK_GGT_sound_range"
    bl_label = "Sound range"
    bl_space_type = "VIEW_3D"
    bl_region_type = "WINDOW"
    bl_options = {"3D", "PERSISTENT"}

    @classmethod
    def poll(cls, context):
        return _active_point(context, "sound") is not None

    def setup(self, context):
        self.arrows = []
        for key, colour in (("near", (0.4, 0.9, 0.4)), ("far", (0.9, 0.5, 0.2))):
            gizmo = self.gizmos.new("GIZMO_GT_arrow_3d")
            gizmo.color = colour
            gizmo.alpha = 0.7
            gizmo.color_highlight = 1.0, 1.0, 1.0
            field = schema.data_field("sound", key)

            def get(field=field):
                obj = _active_point(bpy.context, "sound")
                return float(c2data.data_field(obj, field) or 0.0) if obj is not None else 0.0

            def set_(value, field=field):
                obj = _active_point(bpy.context, "sound")
                if obj is not None:
                    c2data.set_data_field(obj, field, round(max(0.0, value), 2))
                    from . import import_map

                    import_map.point_display(obj)

            gizmo.target_set_handler("offset", get=get, set=set_)
            self.arrows.append(gizmo)

    def refresh(self, context):
        obj = _active_point(context, "sound")
        if obj is None:
            return
        at = Matrix.Translation(obj.matrix_world.translation)
        # along the view's right, so the arrows can be dragged from any angle
        turn = Matrix.Rotation(1.5708, 4, "Y")
        for number, gizmo in enumerate(self.arrows):
            gizmo.matrix_basis = at @ turn @ Matrix.Rotation(number * 0.35, 4, "X")


classes = (DC2TK_GGT_invent_box, DC2TK_GGT_sound_range)


def register():
    for cls in classes:
        try:
            bpy.utils.register_class(cls)
        except (RuntimeError, ValueError) as error:
            print("dc2tk_maps: no %s handles: %s" % (cls.bl_label, error))


def unregister():
    for cls in reversed(classes):
        if getattr(cls, "is_registered", False):
            bpy.utils.unregister_class(cls)
