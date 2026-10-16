"""What the map's scripts know of it, from `dc2tk map SOURCE NAME --json`: the programs they
define (for event points), where they place the player, and the placements, pieces and points
they name, so renaming or deleting one of those can be warned about. Kept in a text block of the
.blend ("dc2tk scripts NAME"), so it survives saving."""

import json

import bpy

from . import c2data

TEXT = "dc2tk scripts %s"

_cache = {}


def info(root):
    """The scripts' report for the map of root, or None before Read Map Scripts."""
    if root is None:
        return None
    name = c2data.map_name(root)
    text = bpy.data.texts.get(TEXT % name)
    if text is None:
        return None
    body = text.as_string()
    cached = _cache.get(name)
    if cached is not None and cached[0] == body:
        return cached[1]
    try:
        value = json.loads(body)
    except ValueError:
        value = None
    _cache[name] = (body, value)
    return value


def store(name, report):
    text = bpy.data.texts.get(TEXT % name) or bpy.data.texts.new(TEXT % name)
    text.clear()
    text.write(json.dumps(report, indent=1, ensure_ascii=False))
    _cache.pop(name, None)


def references(obj):
    """Where the map's scripts name obj (a placement, piece or point): a list of the calls'
    texts with their file and line."""
    role = c2data.role(obj)
    if role not in ("placement", "piece", "point"):
        return []
    report = info(c2data.map_root(obj))
    if not report:
        return []
    name = c2data.exact_name(obj, role)
    placement = c2data.placement_of(obj)
    place = c2data.exact_name(placement, "placement") if placement is not None else ""
    slot = c2data.lookup(placement, ("slot",)) if placement is not None else None
    found = []
    for use in report.get("names") or []:
        what = use.get("what")
        part = use.get("part")
        named_part = part == place or (isinstance(part, int) and part == slot)
        if role == "placement" and what == "placement" and named_part:
            found.append(use)
        elif role == "piece" and what == "piece" and use.get("name") == name and (part is None or named_part):
            found.append(use)
        elif role == "point" and what == "point" and use.get("name") == name:
            own = placement is None
            if (own and part == "") or (not own and named_part) or part is None:
                found.append(use)
    return found
