"""The values the panels edit. They live on the window manager (never saved in the scene or
exported) and read and write `obj["c2"]` of the active object (or the map root) through get/set
callbacks, so there is nothing to keep in sync."""

import bpy
from bpy.props import (
    BoolProperty,
    EnumProperty,
    FloatProperty,
    FloatVectorProperty,
    IntProperty,
    IntVectorProperty,
    PointerProperty,
    StringProperty,
)

from . import c2data, schema


def _active():
    return getattr(bpy.context, "active_object", None)


def _int(value, default=0):
    try:
        return int(value)
    except (TypeError, ValueError):
        return default


def _float(value, default=0.0):
    try:
        return float(value)
    except (TypeError, ValueError):
        return default


def _keep_type(old, new):
    """Write a bool where the exporter wrote a bool and an int otherwise."""
    if isinstance(old, bool):
        return bool(new)
    return int(new)


# Plain fields at a path of obj["c2"].


def path_int(path, label, description="", min_value=-(2 ** 31), max_value=2 ** 31 - 1):
    def get(self):
        return _int(c2data.lookup(_active(), path, 0))

    def set(self, value):
        obj = _active()
        if obj is not None:
            c2data.assign(obj, path, _keep_type(c2data.lookup(obj, path), value))

    return IntProperty(name=label, description=description, get=get, set=set, min=min_value, max=max_value)


def path_bool(path, label, description=""):
    def get(self):
        return bool(_int(c2data.lookup(_active(), path, 0)))

    def set(self, value):
        obj = _active()
        if obj is not None:
            c2data.assign(obj, path, _keep_type(c2data.lookup(obj, path), 1 if value else 0))

    return BoolProperty(name=label, description=description, get=get, set=set)


def path_str(path, label, description=""):
    def get(self):
        value = c2data.lookup(_active(), path, "")
        return value if isinstance(value, str) else str(value)

    def set(self, value):
        obj = _active()
        if obj is not None:
            c2data.assign(obj, path, value)

    return StringProperty(name=label, description=description, get=get, set=set)


def path_int_pair(path, start, label, description="", min_value=0, max_value=255):
    """Two ints at path[start], path[start+1] (hours of a FUNC_FLAG, PIECE_TIME)."""

    def get(self):
        values = c2data.to_py(c2data.lookup(_active(), path, [])) or []
        values = list(values) if isinstance(values, list) else []
        return tuple(_int(values[start + i]) if start + i < len(values) else 0 for i in range(2))

    def set(self, value):
        obj = _active()
        if obj is None:
            return
        values = c2data.to_py(c2data.lookup(obj, path, [])) or []
        values = list(values) if isinstance(values, list) else []
        while len(values) < start + 2:
            values.append(0)
        values[start] = int(value[0])
        values[start + 1] = int(value[1])
        c2data.assign(obj, path, values)

    return IntVectorProperty(
        name=label, description=description, size=2, get=get, set=set, min=min_value, max=max_value
    )


def far_clip_value(index, label):
    """FAR_CLIP distance or fade: `far_clip` as [distance, fade] or {"distance", "fade"}."""
    keys = ("distance", "fade")

    def get(self):
        value = c2data.to_py(c2data.lookup(_active(), ("far_clip",)))
        if isinstance(value, dict):
            return _float(value.get(keys[index]))
        if isinstance(value, list) and index < len(value):
            return _float(value[index])
        return 0.0

    def set(self, value):
        obj = _active()
        if obj is None:
            return
        current = c2data.to_py(c2data.lookup(obj, ("far_clip",)))
        if isinstance(current, dict):
            current[keys[index]] = value
        else:
            current = list(current) if isinstance(current, list) else [0.0, 0.0]
            while len(current) <= index:
                current.append(0.0)
            current[index] = value
        c2data.assign(obj, ("far_clip",), current)

    return FloatProperty(name=label, get=get, set=set)


def col_type_value():
    """PIECE_COL_TYPE: an int, or [type, int] when the line had two arguments."""

    def get(self):
        value = c2data.to_py(c2data.lookup(_active(), ("col_type",)))
        if isinstance(value, list):
            return _int(value[0]) if value else 0
        return _int(value)

    def set(self, value):
        obj = _active()
        if obj is None:
            return
        current = c2data.to_py(c2data.lookup(obj, ("col_type",)))
        if isinstance(current, list) and current:
            current[0] = value
            c2data.assign(obj, ("col_type",), current)
        else:
            c2data.assign(obj, ("col_type",), value)

    return IntProperty(
        name="Collision type",
        description="PIECE_COL_TYPE: 0 is normal; non-zero hides the piece from collision queries",
        get=get,
        set=set,
    )


def node_name(role):
    """The game name of the active node: the object name without prefix and Blender suffix."""
    prefix = schema.PREFIX[role]

    def get(self):
        obj = _active()
        return schema.game_name(obj.name, role) if obj is not None else ""

    def set(self, value):
        obj = _active()
        if obj is not None and value:
            obj.name = prefix + value

    return StringProperty(name="Name", description="The game's name (31 bytes at most)", get=get, set=set)


def point_kind():
    items = [(kind, kind, schema.KIND_LABELS[kind]) for kind in schema.POINT_KINDS]

    def get(self):
        kind = c2data.lookup(_active(), ("kind",), "pos")
        return schema.POINT_KINDS.index(kind) if kind in schema.POINT_KINDS else 0

    def set(self, value):
        obj = _active()
        if obj is None:
            return
        kind = schema.POINT_KINDS[value]
        if c2data.lookup(obj, ("kind",)) == kind:
            return
        data = c2data.to_py(c2data.lookup(obj, ("data",)))
        fresh = schema.default_data(kind)
        if isinstance(data, dict):
            fresh.update({key: value for key, value in data.items() if key in fresh})
        c2data.assign(obj, ("kind",), kind)
        c2data.assign(obj, ("data",), fresh)

    return EnumProperty(name="Kind", items=items, get=get, set=set)


# Fields of a point's data.


def _sync_light(obj, kind, field):
    """Keep a point light's Blender light in step with its data (colour as r,g,b / 255)."""
    if kind != "plight" or obj is None or obj.type != "LIGHT":
        return
    if field.key == "color":
        rgb = c2data.data_field(obj, field)
        obj.data.color = [max(0.0, min(1.0, _float(c) / 255.0)) for c in rgb]


def data_prop(kind, field):
    if field.kind == "str":

        def get(self):
            value = c2data.data_field(_active(), field)
            return value if isinstance(value, str) else str(value)

        def set(self, value):
            obj = _active()
            if obj is not None:
                c2data.set_data_field(obj, field, value)

        return StringProperty(name=field.label, get=get, set=set)

    if field.size == 1:
        if field.kind == "int":

            def get(self):
                return _int(c2data.data_field(_active(), field))

            def set(self, value):
                obj = _active()
                if obj is not None:
                    c2data.set_data_field(obj, field, value)
                    _sync_light(obj, kind, field)

            return IntProperty(name=field.label, get=get, set=set)

        def get(self):
            return _float(c2data.data_field(_active(), field))

        def set(self, value):
            obj = _active()
            if obj is not None:
                c2data.set_data_field(obj, field, value)
                _sync_light(obj, kind, field)

        return FloatProperty(name=field.label, get=get, set=set)

    cast = _int if field.kind == "int" else _float

    def get(self):
        return tuple(cast(v) for v in c2data.data_field(_active(), field))

    def set(self, value):
        obj = _active()
        if obj is not None:
            c2data.set_data_field(obj, field, [cast(v) for v in value])
            _sync_light(obj, kind, field)

    if field.kind == "int":
        return IntVectorProperty(name=field.label, size=field.size, get=get, set=set)
    return FloatVectorProperty(name=field.label, size=field.size, get=get, set=set)


def data_prop_name(kind, field):
    return "d_%s_%s" % (kind, field.key)


def _show_hour(self, context):
    from . import look

    look.show_hour(context, self.hour, self.by_hour)


class C2Settings(bpy.types.PropertyGroup):
    """Values of the panels and of the operators' last use (not saved with the file)."""

    import_name: StringProperty(name="Map", description="The map to export with c2 and import (s17, d01f01, ...)")
    collision_colour: EnumProperty(
        name="Colour collision by",
        items=(
            ("AREA", "Area", "Area kind (1 and 7 are water)"),
            ("FOOT", "Foot sound", "Footstep sound"),
            ("GROUND", "Ground", "Ground value"),
            ("MASK", "Ignore mask", "Query ignore mask"),
        ),
        default="AREA",
    )
    paint_ground: IntProperty(name="Ground", min=0, max=255, default=0)
    paint_foot: IntProperty(name="Foot sound", min=0, max=255, default=0)
    paint_area: IntProperty(name="Area", min=0, max=255, default=6)
    paint_mask: IntProperty(name="Ignore mask", min=0, max=255, default=0)
    show_raw: BoolProperty(name="All values", description="Show every value of extras.c2", default=False)
    show_boxes: BoolProperty(name="Invent boxes", description="Draw invent points' boxes (data min, max)", default=True)
    hour: IntProperty(
        name="Hour",
        description="The time of day to show the map at: pieces with hours (towns' day and night "
        "versions) show only when the game draws them",
        min=0,
        max=23,
        default=12,
        update=_show_hour,
    )
    by_hour: BoolProperty(
        name="By hour",
        description="Show only the pieces the game draws at the hour; off shows every piece",
        default=True,
        update=_show_hour,
    )

    # Placement.
    place_name: node_name("placement")
    place_template: path_str(("template",), "Template", "PARTS_NAME: the template this placement copies")
    place_show: path_bool(("show",), "Shown", "MAP_PARTS show flag")
    place_group: path_str(("group",), "Group", "PARTS_GROUP: switched by fixed cameras")
    place_far_distance: far_clip_value(0, "Far clip distance")
    place_far_fade: far_clip_value(1, "Far clip fade")

    # Piece.
    piece_name: node_name("piece")
    piece_model: path_str(("model",), "Model", "The .mds (or .chr) file")
    piece_show: path_bool(("show",), "Shown", "PIECE show flag")
    piece_time: path_int_pair(("time",), 0, "Hours", "PIECE_TIME start, end (equal: always)")
    piece_col_type: col_type_value()

    # Point.
    point_name: node_name("point")
    point_kind: point_kind()
    point_enable: path_bool(("enable",), "Enabled", "FUNC_DATA enable flag")
    point_flags: path_int_pair(("flag",), 0, "Flags", "FUNC_FLAG's first two numbers", -(2 ** 31), 2 ** 31 - 1)
    point_hours: path_int_pair(("flag",), 2, "Hours", "FUNC_FLAG start, end hour (equal: always)")


for _kind, _fields in schema.DATA_FIELDS.items():
    for _field in _fields:
        C2Settings.__annotations__[data_prop_name(_kind, _field)] = data_prop(_kind, _field)


classes = (C2Settings,)


def register():
    for cls in classes:
        bpy.utils.register_class(cls)
    bpy.types.WindowManager.c2_maps = PointerProperty(type=C2Settings)


def unregister():
    del bpy.types.WindowManager.c2_maps
    for cls in reversed(classes):
        bpy.utils.unregister_class(cls)


def settings(context=None):
    return (context or bpy.context).window_manager.c2_maps
