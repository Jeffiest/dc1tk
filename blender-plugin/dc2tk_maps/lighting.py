"""The Lighting panel: the map's light sets (background, ambient, fog, directional lights) and how
the game picks them by the hour (TIME_FLAG, TIME_LIGHT_NUM), edited in place, shown as the game
would at the Map panel's hour or one set alone, whose suns can be turned in the 3D view to set the
lights' directions."""

import bpy
from bpy.props import BoolProperty, EnumProperty, FloatProperty, FloatVectorProperty, IntProperty
from mathutils import Vector

from . import c2data, look

_syncing = {"busy": False}


def _root():
    return c2data.current_map_root(bpy.context)


def _settings_path(root):
    tree = c2data.to_py(c2data.lookup(root, ("map",)))
    return ("map", "settings") if isinstance(tree, dict) and isinstance(tree.get("settings"), dict) else ("map",)


def _sets(root):
    settings = c2data.map_settings(root)
    return [s for s in settings.get("light_sets") or [] if isinstance(s, dict)]


# the light set menu's items: Blender reads their strings after the callback returns, so they
# are kept here
_set_items = []


def set_choices(self, context):
    root = c2data.current_map_root(context)
    found = []
    for at, light_set in enumerate(_sets(root) if root else []):
        number = light_set.get("index", at)
        found.append((str(at), "Light set %s" % number, "LIGHT_SET %s" % number))
    _set_items[:] = found or [("0", "No light sets", "")]
    return _set_items


def _chosen(context=None):
    """(the root, the place of the chosen light set in the list, the set) or Nones."""
    context = context or bpy.context
    root = c2data.current_map_root(context)
    if root is None:
        return None, None, None
    sets = _sets(root)
    try:
        at = int(context.window_manager.dc2tk_maps.light_set)
    except (TypeError, ValueError):
        at = 0
    if not 0 <= at < len(sets):
        return root, None, None
    return root, at, sets[at]


def _set_value(key, value):
    root, at, _found = _chosen()
    if at is None:
        return
    c2data.assign(root, _settings_path(root) + ("light_sets", at) + tuple(key), value)
    refresh(bpy.context)


def _get(key, default):
    _root_, _at, found = _chosen()
    current = found
    for step in key:
        if isinstance(current, dict):
            current = current.get(step)
        elif isinstance(current, list) and isinstance(step, int) and step < len(current):
            current = current[step]
        else:
            return default
    return default if current is None else current


def _colour_get(key):
    def get(self):
        value = _get(key, [0, 0, 0])
        value = list(value) + [0] * (3 - len(value)) if isinstance(value, list) else [0, 0, 0]
        return [max(0.0, min(1.0, float(c) / 255.0)) for c in value[:3]]

    return get


def _colour_set(key):
    def set_(self, value):
        _set_value(key, [int(round(c * 255.0)) for c in value])

    return set_


def _fog(self):
    fog = _get(("fog",), None)
    if isinstance(fog, list):
        values = list(fog) + [0.0] * (5 - len(fog))
        return {"near": values[0], "far": values[1], "color": values[2:5], "list": True}
    return dict(fog) if isinstance(fog, dict) else {"near": 0.0, "far": 0.0, "color": [0, 0, 0]}


def _fog_set(field, value):
    _root_, _at, found = _chosen()
    if found is None:
        return
    fog = found.get("fog")
    if isinstance(fog, list):
        values = list(fog) + [0.0] * (5 - len(fog))
        index = {"near": 0, "far": 1}.get(field)
        if index is not None:
            values[index] = value
        else:
            values[2:5] = value
        _set_value(("fog",), values)
        return
    fog = dict(fog) if isinstance(fog, dict) else {}
    fog[field] = value
    _set_value(("fog",), fog)


def _light_list(found):
    return [light for light in (found or {}).get("lights") or [] if isinstance(light, dict)]


def _light_get(slot, key, default):
    _root_, _at, found = _chosen()
    lights = _light_list(found)
    if slot >= len(lights):
        return default
    return lights[slot].get(key, default)


def _light_set(slot, key, value):
    root, at, found = _chosen()
    if at is None or slot >= len(_light_list(found)):
        return
    lights = found.get("lights")
    place = [i for i, light in enumerate(lights) if isinstance(light, dict)][slot]
    c2data.assign(root, _settings_path(root) + ("light_sets", at, "lights", place, key), value)
    refresh(bpy.context)


def _time_flag():
    root = _root()
    flag = c2data.map_settings(root).get("time_flag") if root else None
    return flag


def _time_get(field, index, default):
    def get(self):
        flag = _time_flag()
        if isinstance(flag, dict):
            value = flag.get(field, default)
        elif isinstance(flag, list) and index < len(flag):
            value = flag[index]
        else:
            value = default
        return type(default)(value) if value is not None else default

    return get


def _time_set(field, index):
    def set_(self, value):
        root = _root()
        if root is None:
            return
        flag = _time_flag()
        value = int(value) if not isinstance(value, float) else value
        if isinstance(flag, list):
            flag = list(flag) + [0] * (index + 1 - len(flag))
            flag[index] = value
        else:
            flag = dict(flag) if isinstance(flag, dict) else {}
            flag[field] = value
        c2data.assign(root, _settings_path(root) + ("time_flag",), flag)
        refresh(bpy.context)

    return set_


def _number_get(key, default):
    def get(self):
        root = _root()
        value = c2data.map_settings(root).get(key, default) if root else default
        if isinstance(value, list):
            value = value[0] if value else default
        try:
            return int(value)
        except (TypeError, ValueError):
            return default

    return get


def _number_set(key):
    def set_(self, value):
        root = _root()
        if root is None:
            return
        old = c2data.map_settings(root).get(key)
        c2data.assign(root, _settings_path(root) + (key,), [int(value)] if isinstance(old, list) else int(value))
        refresh(bpy.context)

    return set_


def _shown(self, context):
    refresh(context)


def refresh(context):
    """Lights the scene again: one light set alone when the panel shows one, else the game at
    the hour."""
    settings = getattr(context.window_manager, "dc2tk_maps", None)
    if settings is None:
        return
    _syncing["busy"] = True
    try:
        for root in c2data.scene_map_roots(context.scene):
            if settings.light_preview == "SET":
                _r, _at, found = _chosen(context)
                lighting = look._plain(found) if found is not None else None
            else:
                lighting = look.lighting_at(root, settings.hour if settings.by_hour else None)
            look.map_world(context, root, lighting)
    finally:
        _syncing["busy"] = False


@bpy.app.handlers.persistent
def sync_suns(scene, depsgraph=None):
    """A sun turned in the 3D view while one light set is shown sets its light's direction (the
    way towards the light, in game axes), keeping the length the file gives it."""
    context = bpy.context
    settings = getattr(getattr(context, "window_manager", None), "dc2tk_maps", None)
    if _syncing["busy"] or settings is None or settings.light_preview != "SET":
        return
    root, at, found = _chosen(context)
    if found is None:
        return
    prefix = look.SUN_PREFIX % c2data.map_name(root)
    lights = _light_list(found)
    for sun in [obj for obj in scene.objects if obj.name.startswith(prefix) and look.SUN_KEY in obj]:
        number = sun[look.SUN_KEY]
        slot = next((i for i, light in enumerate(lights) if light.get("index", i) == number), None)
        if slot is None:
            continue
        towards = sun.matrix_world.to_3x3() @ Vector((0.0, 0.0, 1.0))
        game = Vector((towards.x, towards.z, -towards.y)).normalized()
        old = Vector([float(v) for v in (lights[slot].get("dir") or [0, 1, 0])[:3]])
        if old.length > 0 and (old.normalized() - game).length < 1e-4:
            continue
        length = old.length if old.length > 0 else 1.0
        _syncing["busy"] = True
        try:
            _light_set(slot, "dir", [round(v * length, 4) for v in game])
        finally:
            _syncing["busy"] = False


def lighting_properties():
    """The properties the panel edits, added to the add-on's settings."""
    return {
        "light_preview": EnumProperty(
            name="Show",
            items=(
                ("GAME", "The game at the hour", "The light set the game uses at the Map panel's hour, blended as it does"),
                ("SET", "One light set", "The chosen light set alone; turn its suns in the 3D view to set its lights"),
            ),
            default="GAME",
            update=_shown,
        ),
        "light_set": EnumProperty(name="Light set", items=set_choices, update=_shown),
        "ls_bgcolor": FloatVectorProperty(
            name="Background", subtype="COLOR", size=3, min=0.0, max=1.0,
            get=_colour_get(("bgcolor",)), set=_colour_set(("bgcolor",)),
        ),
        "ls_bgcolor2": FloatVectorProperty(
            name="Background 2", description="0, 0, 0 copies the background", subtype="COLOR", size=3, min=0.0,
            max=1.0, get=_colour_get(("bgcolor2",)), set=_colour_set(("bgcolor2",)),
        ),
        "ls_ambient": FloatVectorProperty(
            name="Ambient", description="128 is neutral (shown as half)", subtype="COLOR", size=3, min=0.0, max=1.0,
            get=_colour_get(("ambient",)), set=_colour_set(("ambient",)),
        ),
        "ls_fog_enable": BoolProperty(
            name="Fog", get=lambda self: bool(int(_get(("fog_enable",), 0) or 0)),
            set=lambda self, value: _set_value(("fog_enable",), int(value)),
        ),
        "ls_fog_near": FloatProperty(
            name="Near", get=lambda self: float(_fog(self).get("near") or 0.0), set=lambda self, v: _fog_set("near", v),
        ),
        "ls_fog_far": FloatProperty(
            name="Far", get=lambda self: float(_fog(self).get("far") or 0.0), set=lambda self, v: _fog_set("far", v),
        ),
        "ls_fog_color": FloatVectorProperty(
            name="Fog colour", subtype="COLOR", size=3, min=0.0, max=1.0,
            get=lambda self: [max(0.0, min(1.0, float(c) / 255.0)) for c in (list(_fog(self).get("color") or [0, 0, 0]) + [0, 0, 0])[:3]],
            set=lambda self, v: _fog_set("color", [int(round(c * 255.0)) for c in v]),
        ),
        "time_follow": BoolProperty(
            name="Follows the clock", description="TIME_FLAG: the light set is chosen by the hour",
            get=_time_get("follow_clock", 0, False), set=_time_set("follow_clock", 0),
        ),
        "time_blend": BoolProperty(
            name="Blends", description="TIME_FLAG: each set blends into the next over the last hour, the first light following the sun",
            get=_time_get("blend", 1, False), set=_time_set("blend", 1),
        ),
        "time_fixed": FloatProperty(
            name="Fixed hour", min=0.0, max=24.0, get=_time_get("fixed_time", 2, 12.0), set=_time_set("fixed_time", 2),
        ),
        "time_fixed_on": BoolProperty(
            name="At a fixed hour", get=_time_get("fixed_time_enable", 3, False), set=_time_set("fixed_time_enable", 3),
        ),
        "time_light_num": IntProperty(
            name="Sets by the hour", description="TIME_LIGHT_NUM: 4 follows day, evening, night and morning; "
            "any other number divides the day from 9:00 (8 at most)", min=0, max=8,
            get=_number_get("time_light_num", 0), set=_number_set("time_light_num"),
        ),
        "active_light_set": IntProperty(
            name="Active set", description="ACTIVE_LIGHT_SET: the set used when the map does not follow the clock",
            min=0, max=15, get=_number_get("active_light_set", 0), set=_number_set("active_light_set"),
        ),
    }


def light_props():
    """The colour and direction of each of the four directional lights of the chosen set."""
    found = {}
    for slot in range(4):
        found["ls_light%d_color" % slot] = FloatVectorProperty(
            name="Colour", subtype="COLOR", size=3, min=0.0, max=1.0,
            get=(lambda s: lambda self: [max(0.0, min(1.0, float(c) / 255.0)) for c in (list(_light_get(s, "color", [0, 0, 0])) + [0, 0, 0])[:3]])(slot),
            set=(lambda s: lambda self, v: _light_set(s, "color", [int(round(c * 255.0)) for c in v]))(slot),
        )
        found["ls_light%d_dir" % slot] = FloatVectorProperty(
            name="Towards", description="The way towards the light, in game axes (y up)", size=3,
            get=(lambda s: lambda self: [float(v) for v in (list(_light_get(s, "dir", [0, 1, 0])) + [0, 0, 0])[:3]])(slot),
            set=(lambda s: lambda self, v: _light_set(s, "dir", [round(c, 4) for c in v]))(slot),
        )
    return found


def draw(layout, context):
    settings = context.window_manager.dc2tk_maps
    root = c2data.current_map_root(context)
    layout.prop(settings, "light_preview", expand=True)
    col = layout.column(align=True)
    col.prop(settings, "time_follow")
    sub = col.column(align=True)
    sub.enabled = settings.time_follow
    sub.prop(settings, "time_blend")
    sub.prop(settings, "time_light_num")
    col.prop(settings, "time_fixed_on")
    sub = col.row(align=True)
    sub.enabled = settings.time_fixed_on
    sub.prop(settings, "time_fixed")
    col.prop(settings, "active_light_set")
    if settings.light_preview == "GAME":
        lit = look.lighting_at(root, settings.hour if settings.by_hour else None)
        layout.label(text="Showing %02d:00 as the game lights it" % settings.hour if settings.by_hour else "Showing the map's start",
                     icon="TIME")
        if lit is None:
            layout.label(text="The light sets the hour needs are missing", icon="ERROR")
    layout.prop(settings, "light_set", text="")
    _root_, _at, found = _chosen(context)
    if found is None:
        layout.label(text="No light set", icon="INFO")
        return
    col = layout.column(align=True)
    col.prop(settings, "ls_bgcolor")
    col.prop(settings, "ls_bgcolor2")
    col.prop(settings, "ls_ambient")
    box = layout.box()
    box.prop(settings, "ls_fog_enable")
    sub = box.column(align=True)
    sub.enabled = settings.ls_fog_enable
    row = sub.row(align=True)
    row.prop(settings, "ls_fog_near")
    row.prop(settings, "ls_fog_far")
    sub.prop(settings, "ls_fog_color")
    for slot, _light in enumerate(_light_list(found)[:4]):
        box = layout.box()
        box.label(text="Light %d" % slot, icon="LIGHT_SUN")
        box.prop(settings, "ls_light%d_color" % slot)
        box.prop(settings, "ls_light%d_dir" % slot)
    if settings.light_preview == "SET":
        layout.label(text="Turn a sun (c2 light ...) to aim its light", icon="INFO")


def register():
    if sync_suns not in bpy.app.handlers.depsgraph_update_post:
        bpy.app.handlers.depsgraph_update_post.append(sync_suns)


def unregister():
    if sync_suns in bpy.app.handlers.depsgraph_update_post:
        bpy.app.handlers.depsgraph_update_post.remove(sync_suns)
