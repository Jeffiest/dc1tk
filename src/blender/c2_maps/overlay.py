"""Draws the boxes of invent points (their data's min and max) in the 3D viewport. Nothing is
added to the scene, so nothing extra is exported."""

import bpy

from . import c2data, schema

_handle = None

EDGES = ((0, 1), (1, 3), (3, 2), (2, 0), (4, 5), (5, 7), (7, 6), (6, 4), (0, 4), (1, 5), (2, 6), (3, 7))


def _shader():
    import gpu

    for name in ("UNIFORM_COLOR", "3D_UNIFORM_COLOR"):
        try:
            return gpu.shader.from_builtin(name)
        except (ValueError, TypeError):
            continue
    return None


def box_lines(obj):
    fields = schema.DATA_FIELDS["invent"]
    low = [float(v) for v in c2data.data_field(obj, fields[1])]
    high = [float(v) for v in c2data.data_field(obj, fields[2])]
    corners = []
    for x in (low[0], high[0]):
        for y in (low[1], high[1]):
            for z in (low[2], high[2]):
                corners.append(obj.matrix_world @ _vector(schema.game_to_blender((x, y, z))))
    lines = []
    for a, b in EDGES:
        lines.append(corners[a][:])
        lines.append(corners[b][:])
    return lines


def _vector(values):
    from mathutils import Vector

    return Vector(values)


def draw():
    context = bpy.context
    wm = getattr(context, "window_manager", None)
    if wm is None or not hasattr(wm, "c2_maps") or not wm.c2_maps.show_boxes:
        return
    lines = []
    selected = []
    for obj in context.visible_objects:
        if obj.type != "EMPTY" or c2data.KEY not in obj:
            continue
        if c2data.lookup(obj, ("kind",)) != "invent":
            continue
        try:
            found = box_lines(obj)
        except (TypeError, ValueError, IndexError):
            continue
        (selected if obj.select_get() else lines).extend(found)
    if not lines and not selected:
        return
    import gpu
    from gpu_extras.batch import batch_for_shader

    shader = _shader()
    if shader is None:
        return
    gpu.state.line_width_set(1.5)
    for coords, colour in ((lines, (0.3, 0.9, 0.9, 1.0)), (selected, (1.0, 0.7, 0.2, 1.0))):
        if not coords:
            continue
        batch = batch_for_shader(shader, "LINES", {"pos": coords})
        shader.bind()
        shader.uniform_float("color", colour)
        batch.draw(shader)
    gpu.state.line_width_set(1.0)


def register():
    global _handle
    if bpy.app.background:
        return
    _handle = bpy.types.SpaceView3D.draw_handler_add(draw, (), "WINDOW", "POST_VIEW")


def unregister():
    global _handle
    if _handle is not None:
        bpy.types.SpaceView3D.draw_handler_remove(_handle, "WINDOW")
        _handle = None
