"""Reading and writing `obj["c2"]`, the node's `extras.c2` as Blender's glTF importer stores it
(an IDProperty group), and finding the map's nodes by role."""

import ast

import bpy

from . import schema

KEY = "c2"


def to_py(value):
    """A plain Python copy of an IDProperty value (groups to dicts, arrays to lists)."""
    if hasattr(value, "to_dict") and hasattr(value, "keys"):
        return {key: to_py(value[key]) for key in value.keys()}
    if hasattr(value, "to_list"):
        return [to_py(item) for item in value.to_list()]
    if isinstance(value, (list, tuple)):
        return [to_py(item) for item in value]
    return value


def recovered(text):
    """The dict Blender's importer turned into text when it could not store it (it falls back to
    str() of the whole value, for example on a list mixing strings and numbers)."""
    try:
        value = ast.literal_eval(text)
    except (ValueError, SyntaxError):
        return None
    return value if isinstance(value, dict) else None


def get(obj):
    """The node's `extras.c2` as a plain dict, or None."""
    if obj is None:
        return None
    value = obj.get(KEY)
    if value is None:
        return None
    if isinstance(value, str):
        return recovered(value)
    value = to_py(value)
    return value if isinstance(value, dict) else None


def is_stringified(obj):
    return obj is not None and isinstance(obj.get(KEY), str)


def put(obj, data):
    obj[KEY] = data


def role(obj):
    data = obj.get(KEY) if obj is not None else None
    if data is None:
        return None
    if isinstance(data, str):
        data = recovered(data) or {}
    try:
        return data.get("role")
    except AttributeError:
        return None


def lookup(obj, path, default=None):
    """The value at `path` (keys and indices) inside obj["c2"], without copying the tree."""
    current = obj.get(KEY) if obj is not None else None
    if isinstance(current, str):
        current = recovered(current)
    for step in path:
        if current is None:
            return default
        try:
            if isinstance(step, int):
                current = current[step]
            else:
                current = current.get(step) if hasattr(current, "get") else None
        except (IndexError, KeyError, TypeError):
            return default
    return default if current is None else current


def _assign(container, step, value):
    if isinstance(step, int):
        while len(container) <= step:
            container.append(None)
        container[step] = value
    else:
        container[step] = value


def assign(obj, path, value):
    """Set the value at `path` in obj["c2"], creating dicts and lists on the way, and store the
    whole tree back (IDProperty lists of strings are copies, so they cannot be set in place)."""
    data = get(obj) or {}
    current = data
    for index, step in enumerate(path[:-1]):
        following = path[index + 1]
        try:
            child = current[step]
        except (KeyError, IndexError, TypeError):
            child = None
        if not isinstance(child, (dict, list)):
            child = [] if isinstance(following, int) else {}
            _assign(current, step, child)
        current = child
    _assign(current, path[-1], value)
    put(obj, data)


def remove(obj, path):
    data = get(obj)
    if data is None:
        return
    current = data
    for step in path[:-1]:
        current = current[step]
    if isinstance(current, dict):
        current.pop(path[-1], None)
    else:
        del current[path[-1]]
    put(obj, data)


# The map's structure.


def map_root(obj):
    """The `map:` root above an object (or the object itself)."""
    while obj is not None:
        if role(obj) == "map" or (obj.name.startswith(schema.PREFIX["map"]) and obj.parent is None):
            return obj
        obj = obj.parent
    return None


def scene_map_roots(scene=None):
    scene = scene or bpy.context.scene
    return [obj for obj in scene.objects if obj.parent is None and (role(obj) == "map" or obj.name.startswith("map:"))]


def current_map_root(context=None):
    context = context or bpy.context
    found = map_root(getattr(context, "active_object", None))
    if found is not None:
        return found
    roots = scene_map_roots(context.scene)
    return roots[0] if roots else None


def map_name(root):
    return schema.game_name(root.name, "map") if root is not None else ""


def descendants(obj):
    found = []
    stack = [obj]
    while stack:
        current = stack.pop()
        found.append(current)
        stack.extend(current.children)
    return found


def group_node(root, name):
    for child in root.children:
        if schema.strip_suffix(child.name) == name:
            return child
    return None


def placement_of(obj):
    while obj is not None:
        if role(obj) == "placement":
            return obj
        obj = obj.parent
    return None


def piece_of(obj):
    while obj is not None:
        if role(obj) == "piece":
            return obj
        obj = obj.parent
    return None


def placements(root):
    return [obj for obj in descendants(root) if role(obj) == "placement"]


def points(root):
    return [obj for obj in descendants(root) if role(obj) == "point"]


def collision_pieces(root, kinds=schema.COLLISION_KINDS):
    return [
        obj
        for obj in descendants(root)
        if role(obj) == "piece" and lookup(obj, ("kind",)) in kinds
    ]


def collision_meshes(root):
    """Mesh objects under collision and camera-collision pieces."""
    found = []
    for piece in collision_pieces(root):
        for obj in descendants(piece):
            if obj.type == "MESH":
                found.append(obj)
    return found


def is_collision_mesh(obj):
    piece = piece_of(obj)
    return obj.type == "MESH" and piece is not None and lookup(piece, ("kind",)) in schema.COLLISION_KINDS


# Point data, as a dict of named fields or as the raw argument list.


def data_field(obj, field):
    data = lookup(obj, ("data",))
    if data is None:
        return field.default
    if hasattr(data, "keys"):
        value = data.get(field.key)
        if value is None:
            return field.default
        if field.size > 1:
            value = list(to_py(value))
            value += list(field.default)[len(value):]
            return value[: field.size]
        return value
    data = to_py(data)
    if field.size > 1:
        value = list(data[field.pos: field.pos + field.size])
        value += list(field.default)[len(value):]
        return value
    return data[field.pos] if field.pos < len(data) else field.default


def set_data_field(obj, field, value):
    data = lookup(obj, ("data",))
    if data is not None and not hasattr(data, "keys"):
        data = to_py(data)
        if field.size > 1:
            for index in range(field.size):
                while len(data) <= field.pos + index:
                    data.append(0)
                data[field.pos + index] = value[index]
        else:
            while len(data) <= field.pos:
                data.append(0)
            data[field.pos] = value
        assign(obj, ("data",), data)
        return
    assign(obj, ("data", field.key), list(value) if field.size > 1 else value)
