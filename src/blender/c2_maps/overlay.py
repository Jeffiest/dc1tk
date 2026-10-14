"""Draws the boxes of invent points (their data's min and max) in the 3D viewport. Nothing is
added to the scene, so nothing extra is exported.

The boxes are worked out and handed to the GPU only when the scene changes (an edit, a selection,
an undo); every other redraw draws what is already there, so orbiting a big map stays smooth."""

import bpy
from bpy.app.handlers import persistent

from . import c2data, schema

_handle = None
# The batches drawn: [(batch, colour)], or None when the scene changed since they were made.
_batches = None

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


def box_coords(context):
    """(unselected, selected) line coordinates of every visible invent point's box."""
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
    return lines, selected


def invalidate(*_args):
    """Make the next redraw work the boxes out again."""
    global _batches
    _batches = None


@persistent
def _scene_changed(_scene, _depsgraph=None):
    invalidate()


def _make_batches(context, shader):
    from gpu_extras.batch import batch_for_shader

    lines, selected = box_coords(context)
    return [
        (batch_for_shader(shader, "LINES", {"pos": coords}), colour)
        for coords, colour in ((lines, (0.3, 0.9, 0.9, 1.0)), (selected, (1.0, 0.7, 0.2, 1.0)))
        if coords
    ]


def draw():
    global _batches
    context = bpy.context
    wm = getattr(context, "window_manager", None)
    if wm is None or not hasattr(wm, "c2_maps") or not wm.c2_maps.show_boxes:
        return
    shader = _shader()
    if shader is None:
        return
    if _batches is None:
        _batches = _make_batches(context, shader)
    if not _batches:
        return
    import gpu

    gpu.state.line_width_set(1.5)
    shader.bind()
    for batch, colour in _batches:
        shader.uniform_float("color", colour)
        batch.draw(shader)
    gpu.state.line_width_set(1.0)


def register():
    global _handle
    if bpy.app.background:
        return
    _handle = bpy.types.SpaceView3D.draw_handler_add(draw, (), "WINDOW", "POST_VIEW")
    for handlers in (bpy.app.handlers.depsgraph_update_post, bpy.app.handlers.undo_post,
                     bpy.app.handlers.redo_post, bpy.app.handlers.load_post):
        if _scene_changed not in handlers:
            handlers.append(_scene_changed)


def unregister():
    global _handle
    if _handle is not None:
        bpy.types.SpaceView3D.draw_handler_remove(_handle, "WINDOW")
        _handle = None
    for handlers in (bpy.app.handlers.depsgraph_update_post, bpy.app.handlers.undo_post,
                     bpy.app.handlers.redo_post, bpy.app.handlers.load_post):
        if _scene_changed in handlers:
            handlers.remove(_scene_changed)
    invalidate()
