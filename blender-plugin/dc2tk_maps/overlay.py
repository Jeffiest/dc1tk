import bpy
from bpy.app.handlers import persistent

from . import c2data, schema

_handle = None

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


ARC_RADIUS = 20.0
AXIS_HALF = 30.0
LOOP_FRAMES = 50


def _anime_values(obj):
    fields = {field["key"]: field for field in schema.DATA_FIELDS["anime"]}

    def number(key):
        try:
            return int(c2data.data_field(obj, fields[key]))
        except (TypeError, ValueError):
            return 0

    vectors = {key: [float(v) for v in c2data.data_field(obj, fields[key])] for key in ("start", "step", "end")}
    return number("drives"), number("mode"), number("relative"), vectors


def _axis_index(mode, vectors):
    if mode in (8, 9):
        return 1
    if mode in (1, 2, 3, 4, 5):
        change = vectors["step"]
    elif mode in (6, 7, 10):
        change = [b - a for a, b in zip(vectors["start"], vectors["end"])]
    else:
        return None
    index = max(range(3), key=lambda i: abs(change[i]))
    return index if abs(change[index]) > 1e-9 else None


def anime_lines(obj, by_name=None, link=False, nodes_of=None):
    import math

    from mathutils import Euler, Matrix, Vector

    target, _problem = c2data.anime_target(obj, by_name, nodes_of)
    if target is None:
        return []
    drives, mode, relative, vectors = _anime_values(obj)
    parent = target.parent.matrix_world if target.parent is not None else Matrix.Identity(4)
    centre = target.matrix_world.translation.copy()
    lines = []
    if link:
        lines += [obj.matrix_world.translation[:], centre[:]]
    if drives == 1 and not relative:
        start = vectors["start"]
        if mode == 1:
            end = [s + d * LOOP_FRAMES for s, d in zip(start, vectors["step"])]
        elif mode in (2, 3, 4, 5, 7, 10):
            end = vectors["end"]
        else:
            return lines
        lines += [(parent @ Vector(schema.game_to_blender(v)))[:] for v in (start, end)]
    elif drives == 2:
        index = _axis_index(mode, vectors)
        if index is None:
            return lines
        start = [math.radians(v) for v in vectors["start"]]
        unit = [Vector((1, 0, 0)), Vector((0, 1, 0)), Vector((0, 0, 1))]

        kept = [0.0, start[1] if index == 0 else 0.0, start[2] if index < 2 else 0.0]
        axis_game = Euler(kept, "XYZ").to_matrix() @ unit[index]
        turn = parent.to_3x3()
        axis = (turn @ Vector(schema.game_to_blender(axis_game))).normalized()
        if axis.length < 0.5:
            return lines
        lines += [(centre - axis * AXIS_HALF)[:], (centre + axis * AXIS_HALF)[:]]
        facing = Euler(start, "XYZ").to_matrix() @ unit[0 if index == 2 else 2]
        across = turn @ Vector(schema.game_to_blender(facing))
        across -= axis * across.dot(axis)
        if across.length < 1e-6:
            across = axis.orthogonal()
        across.normalize()
        if mode in (1, 8, 9):
            sweep = 360.0
        else:
            sweep = vectors["end"][index] - vectors["start"][index]
            sweep = max(-360.0, min(360.0, sweep))
        if turn.determinant() < 0:
            sweep = -sweep
        steps = max(4, int(abs(sweep) / 10.0))
        arc = [
            centre + (Matrix.Rotation(math.radians(sweep * i / steps), 3, axis) @ across) * ARC_RADIUS
            for i in range(steps + 1)
        ]
        lines += [centre[:], arc[0][:]]
        for a, b in zip(arc, arc[1:]):
            lines += [a[:], b[:]]
    return lines


def anime_coords(context):
    lines = []
    selected = []
    by_root = {}
    nodes_of = {}
    for obj in context.visible_objects:
        if obj.type != "EMPTY" or c2data.KEY not in obj:
            continue
        if c2data.lookup(obj, ("kind",)) != "anime":
            continue
        root = c2data.map_root(obj)
        if root not in by_root:
            by_root[root] = c2data.placements_by_name(root)
        chosen = obj.select_get()
        try:
            found = anime_lines(obj, by_root[root], link=chosen, nodes_of=nodes_of)
        except (TypeError, ValueError, IndexError):
            continue
        (selected if chosen else lines).extend(found)
    return lines, selected


def invalidate(*_args):
    global _batches
    _batches = None


@persistent
def _scene_changed(_scene, _depsgraph=None):
    invalidate()


SELECTED = (1.0, 0.7, 0.2, 1.0)
LAYERS = (("show_boxes", box_coords, (0.3, 0.9, 0.9, 1.0)), ("show_anime", anime_coords, (0.85, 0.45, 0.95, 1.0)))


def _make_batches(context, shader):
    from gpu_extras.batch import batch_for_shader

    batches = []
    for setting, coords_of, colour in LAYERS:
        lines, selected = coords_of(context)
        for coords, tint in ((lines, colour), (selected, SELECTED)):
            if coords:
                batches.append((batch_for_shader(shader, "LINES", {"pos": coords}), tint, setting))
    return batches


def draw():
    global _batches
    context = bpy.context
    wm = getattr(context, "window_manager", None)
    if wm is None or not hasattr(wm, "dc2tk_maps"):
        return
    shown = {setting for setting, _coords_of, _colour in LAYERS if getattr(wm.dc2tk_maps, setting)}
    if not shown:
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
    for batch, colour, setting in _batches:
        if setting in shown:
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
