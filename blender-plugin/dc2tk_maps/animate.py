"""Playing the map's animations (anime points, FUNC_ANIME_DATA) in the 3D view as the engine
steps them (CObjAnime::Step, docs/game/maps.md section 5.1), 25 steps a second. What they move
is put back when playing stops, and before an export, so nothing played is ever written."""

import math
import random

import bpy
from mathutils import Euler, Matrix, Vector

from . import c2data, schema

STEPS_PER_SECOND = 25
AXES = Matrix(((1.0, 0.0, 0.0), (0.0, 0.0, -1.0), (0.0, 1.0, 0.0)))

_state = {"timer": None, "saved": {}, "points": []}


def playing():
    return _state["timer"] is not None


def _values(obj):
    fields = {field["key"]: field for field in schema.DATA_FIELDS["anime"]}

    def number(key):
        try:
            return int(c2data.data_field(obj, fields[key]))
        except (TypeError, ValueError):
            return 0

    vectors = {key: [float(v) for v in c2data.data_field(obj, fields[key])] for key in ("start", "step", "end")}
    return number("drives"), number("mode"), number("uniform"), number("relative"), vectors


def reached(value, step, end):
    """Whether an axis moving by step has reached end (CObjAnime::CheckOver)."""
    return (step > 0 and value >= end) or (step < 0 and value <= end)


def step(point, hour):
    """One frame of point (a dict of its values and state): its new value, as Step computes it."""
    mode, start, delta, end = point["mode"], point["start"], point["step"], point["end"]
    value = point["value"]
    moving = [axis for axis in range(3) if delta[axis] != 0.0]
    if point.get("done"):
        return value
    if mode == 1:
        value = [v + d for v, d in zip(value, delta)]
    elif mode in (2, 3):
        value = [v + d for v, d in zip(value, delta)]
        if any(reached(value[a], delta[a], end[a]) for a in moving):
            if mode == 2:
                value, point["done"] = list(end), True
            else:
                value = list(start)
    elif mode in (4, 5):
        sign = point.get("sign", 1.0)
        value = [v + sign * d for v, d in zip(value, delta)]
        if sign > 0 and any(reached(value[a], delta[a], end[a]) for a in moving):
            value, point["sign"] = list(end), -1.0
        elif sign < 0 and any(reached(value[a], -delta[a], start[a]) for a in moving):
            value, point["sign"] = list(start), 1.0
            if mode == 5:
                point["done"] = True
    elif mode == 7:
        value = [s + (e - s) * random.random() for s, e in zip(start, end)]
    elif mode == 8:
        value = [start[0], -360.0 * hour, start[2]]
    elif mode == 9:
        value = [start[0], -30.0 * hour, start[2]]
    elif mode == 10:
        value = list(end) if _within(hour, delta[0], delta[1]) else list(start)
        fade = delta[2]
        if fade > 0:
            weight = _fade(hour, delta[0], delta[1], fade)
            value = [s + (e - s) * weight for s, e in zip(start, end)]
    else:
        value = list(start)
    if point["uniform"]:
        value = [value[0]] * 3
    point["value"] = value
    return value


def _within(hour, start, end):
    if start == end:
        return True
    if start < end:
        return start <= hour < end
    return hour >= start or hour < end


def _fade(hour, start, end, fade):
    """How far into `end` the hour is: 1 within the hours, blended linearly over fade hours
    before and after (SubTime, the short way round the clock)."""
    if _within(hour, start, end):
        return 1.0
    gap = lambda a, b: min((a - b) % 24.0, (b - a) % 24.0)
    nearest = min(gap(hour, start), gap(hour, end))
    return max(0.0, 1.0 - nearest / fade)


def apply(point, value):
    """Puts value into what the point moves: its position, rotation (degrees) or scale in its
    parent's space, in place of its own."""
    target = bpy.data.objects.get(point["target"])
    if target is None:
        return
    # what other animations of the same object drive stays as they left it
    current = target.matrix_basis
    location, rotation, scale = current.decompose()
    drives = point["drives"]
    if drives == 1:
        moved = Vector(schema.game_to_blender(value))
        location = current.translation + current.to_3x3() @ moved if point["relative"] else moved
    elif drives == 2:
        turn = AXES @ Euler([math.radians(v) for v in value], "XYZ").to_matrix() @ AXES.inverted()
        rotation = turn.to_quaternion()
    elif drives == 3:
        scale = Vector((value[0], value[2], value[1]))
    else:
        return
    target.matrix_basis = Matrix.LocRotScale(location, rotation, scale)


def collect(context):
    """The scene's enabled animations that move something drawn: position, rotation and scale
    (colour and alpha are not shown)."""
    found = []
    for root in c2data.scene_map_roots(context.scene):
        by_name = c2data.placements_by_name(root)
        nodes_of = {}
        for obj in c2data.points(root):
            if c2data.lookup(obj, ("kind",)) != "anime" or not c2data.lookup(obj, ("enable",), 1):
                continue
            target, _problem = c2data.anime_target(obj, by_name, nodes_of)
            drives, mode, uniform, relative, vectors = _values(obj)
            if target is None or drives not in (1, 2, 3):
                continue
            found.append({
                "point": obj.name, "target": target.name, "drives": drives, "mode": mode, "uniform": uniform,
                "relative": relative, "start": vectors["start"], "step": vectors["step"], "end": vectors["end"],
                "value": list(vectors["start"]),
            })
    return found


def tick():
    if not playing():
        return None
    context = bpy.context
    settings = getattr(context.window_manager, "dc2tk_maps", None)
    hour = float(settings.hour) if settings is not None else 12.0
    for point in _state["points"]:
        apply(point, step(point, hour))
    return 1.0 / STEPS_PER_SECOND


def start(context):
    stop()
    points = collect(context)
    _state["points"] = points
    _state["saved"] = {}
    for point in points:
        target = bpy.data.objects.get(point["target"])
        if target is not None and point["target"] not in _state["saved"]:
            _state["saved"][point["target"]] = target.matrix_basis.copy()
    # AssignFuncAnime sets the start at once
    for point in points:
        apply(point, point["value"])
    _state["timer"] = tick
    bpy.app.timers.register(tick, first_interval=1.0 / STEPS_PER_SECOND)
    return len(points)


def stop():
    """Stops playing and puts back what the animations moved."""
    if _state["timer"] is not None and bpy.app.timers.is_registered(_state["timer"]):
        bpy.app.timers.unregister(_state["timer"])
    _state["timer"] = None
    for name, matrix in _state["saved"].items():
        obj = bpy.data.objects.get(name)
        if obj is not None:
            obj.matrix_basis = matrix
    _state["saved"] = {}
    _state["points"] = []


class DC2TK_OT_play_animations(bpy.types.Operator):
    """Play the map's animations (anime points) as the game does, or stop and put everything
    back where it was; colour and alpha animations are not shown"""

    bl_idname = "dc2tk.play_animations"
    bl_label = "Play Animations"
    bl_options = {"REGISTER"}

    @classmethod
    def poll(cls, context):
        return c2data.current_map_root(context) is not None

    def execute(self, context):
        if playing():
            stop()
            self.report({"INFO"}, "Animations stopped; everything is back")
            return {"FINISHED"}
        count = start(context)
        self.report({"INFO"}, "Playing %d animations (press again to stop)" % count)
        return {"FINISHED"}


@bpy.app.handlers.persistent
def _before_change(*_args):
    stop()


classes = (DC2TK_OT_play_animations,)


def register():
    for cls in classes:
        bpy.utils.register_class(cls)
    for handlers in (bpy.app.handlers.save_pre, bpy.app.handlers.load_pre, bpy.app.handlers.undo_pre):
        if _before_change not in handlers:
            handlers.append(_before_change)


def unregister():
    stop()
    for handlers in (bpy.app.handlers.save_pre, bpy.app.handlers.load_pre, bpy.app.handlers.undo_pre):
        if _before_change in handlers:
            handlers.remove(_before_change)
    for cls in reversed(classes):
        bpy.utils.unregister_class(cls)
