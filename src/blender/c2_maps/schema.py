"""The conventions of the map glTF scene (docs/assets/maps.md, section 8) and the fields of
every role and point kind, with labels and defaults. No bpy here, so the tests can use it too."""

import re

# Name prefixes by role (section 8, "Nodes").
PREFIX = {
    "map": "map:",
    "placement": "place:",
    "piece": "piece:",
    "point": "point:",
    "camera": "camera:",
}
GROUP_NAMES = ("placements", "points", "cameras")
ROLES = ("map", "placement", "piece", "mds", "point", "group", "camera")

POINT_KINDS = ("pos", "invent", "event", "flare", "plight", "fire", "effect", "sound", "anime")
PIECE_KINDS = ("model", "collision", "camera", "chara")
COLLISION_KINDS = ("collision", "camera")

NAME_LIMIT = 31  # bytes, cp932: placement, template, piece and point names
TARGET_LIMIT = 15  # FUNC_EVENT_DATA's target
PLACEMENT_LIMIT = 320
PLACEMENT_LIMIT_DUNGEON = 400
TIME_LIGHT_NUM_LIMIT = 8

# The kinds of event point (FUNC_EVENT_DATA's first field) the disc uses.
EVENT_KINDS = (
    ("door", "Door"),
    ("event", "Event"),
    ("t_box", "Treasure box"),
    ("close_door", "Closing door"),
    ("ed_door", "Edit-mode door"),
    ("book", "Book"),
    ("lddr_b", "Ladder (bottom)"),
    ("lddr_t", "Ladder (top)"),
)

# An event point whose FUNC_POS scale is near 1 gets a default size (mapload.cpp:1437): doors
# 15, ladders 25, chests 20. The size is taken as the half-extent of the box.
EVENT_DEFAULT_SIZE = {
    "door": 15.0,
    "close_door": 15.0,
    "ed_door": 15.0,
    "lddr_b": 25.0,
    "lddr_t": 25.0,
    "t_box": 20.0,
}


class Field:
    """A field of a point's `data`: its key when `data` is a dict, or its position (and size)
    when `data` is the raw argument list of the FUNC_<KIND>_DATA line."""

    def __init__(self, key, label, kind, size=1, pos=0, labels=None, default=None, optional=False):
        self.key = key
        self.label = label
        self.kind = kind  # "int", "float" or "str"
        self.size = size
        self.pos = pos
        self.labels = labels  # one per element of a vector
        self.optional = optional
        if default is None:
            zero = {"int": 0, "float": 0.0, "str": ""}[kind]
            default = zero if size == 1 else [zero] * size
        self.default = default


F = Field
_FIRE = [
    F("color", "Colour", "int", 3, 0, ("R", "G", "B"), [128, 90, 38]),
    F("draw", "Draw", "int", 1, 3),
    F("extra", "Extra", "int", 2, 4, optional=True),
]

# The data line of each kind (section 5), with the names the add-on gives its fields.
DATA_FIELDS = {
    "pos": [],
    "invent": [
        F("id", "Invent id", "int", 1, 0),
        F("min", "Box min", "float", 3, 1, ("X", "Y", "Z"), [-10.0, -10.0, -10.0]),
        F("max", "Box max", "float", 3, 4, ("X", "Y", "Z"), [10.0, 10.0, 10.0]),
        F("unknown_int", "Int", "int", 1, 7),
        F("unknown_float", "Float", "float", 1, 8),
        F("angle", "Angle (degrees)", "float", 1, 9),
    ],
    "event": [
        F("kind", "Event kind", "str", 1, 0, default="event"),
        F("event_no", "Event number", "int", 1, 1),
        F("program", "Program", "int", 1, 2),
        F("args", "Arguments", "int", 3, 3, ("Arg 1", "Arg 2", "Call program")),
        F("target", "Target", "str", 1, 6),
        F("flags", "Flags", "int", 3, 7, ("Action button", "Use item", "0x100")),
    ],
    "flare": _FIRE,
    "fire": _FIRE,
    "plight": [
        F("power", "Power", "float", 1, 0, default=100.0),
        F("color", "Colour (0-511)", "int", 3, 1, ("R", "G", "B"), [255, 255, 255]),
        F("mode", "Mode", "int", 1, 4),
        F("args", "Arguments", "int", 4, 5),
        F("flicker", "Flicker type", "int", 1, 9),
        F("depth", "Flicker depth", "float", 1, 10),
        F("period", "Flicker period", "float", 1, 11),
    ],
    "effect": [F("effect", "Effect name", "str", 1, 0)],
    "sound": [
        F("number", "Sound number", "int", 1, 0),
        F("near", "Near", "float", 1, 1, default=50.0),
        F("far", "Far", "float", 1, 2, default=200.0),
        F("unknown_int", "Int", "int", 1, 3),
        F("shape", "Shape", "int", 1, 4),
        F("start", "Start", "float", 3, 5, ("X", "Y", "Z")),
        F("end", "End", "float", 3, 8, ("X", "Y", "Z")),
    ],
    "anime": [
        F("placement", "Placement", "str", 1, 0),
        F("piece", "Piece", "str", 1, 1),
        F("frame", "Frame", "str", 1, 2),
        F("kind", "Kind", "int", 1, 3),
        F("unknown_int", "Int", "int", 1, 4),
        F("v0", "Vector 1", "float", 3, 5),
        F("v1", "Vector 2", "float", 3, 8),
        F("v2", "Vector 3", "float", 3, 11),
        F("extra", "Shorts", "int", 2, 14, optional=True),
    ],
}

KIND_LABELS = {
    "pos": "Position",
    "invent": "Invent box",
    "event": "Event / door / chest",
    "flare": "Lens flare",
    "plight": "Point light",
    "fire": "Fire",
    "effect": "Effect",
    "sound": "Sound emitter",
    "anime": "Piece animation",
}


def default_data(kind, overrides=None):
    data = {}
    for field in DATA_FIELDS.get(kind, []):
        if field.optional:
            continue
        data[field.key] = list(field.default) if isinstance(field.default, list) else field.default
    data.update(overrides or {})
    return data


def new_point(kind, data=None, enable=1, hours=(0, 0)):
    return {
        "role": "point",
        "kind": kind,
        "enable": enable,
        "flag": [0, 0, hours[0], hours[1]],
        "data": default_data(kind, data),
    }


# Presets for the "add point" operator: (id, label, kind, data overrides, node scale, base name).
POINT_PRESETS = (
    ("SPAWN", "Position (spawn)", "pos", {}, 1.0, "point"),
    ("DOOR", "Door", "event", {"kind": "door", "flags": [1, 0, 0]}, 1.0, "door"),
    ("CHEST", "Treasure chest", "event", {"kind": "t_box", "flags": [1, 0, 0]}, 1.0, "t_box"),
    ("EVENT", "Event trigger", "event", {"kind": "event", "flags": [0, 0, 0]}, 30.0, "event"),
    ("LIGHT", "Point light", "plight", {}, 1.0, "plight"),
    ("SOUND", "Sound emitter", "sound", {}, 1.0, "sound"),
    ("INVENT", "Invent box", "invent", {}, 1.0, "invent"),
    ("EFFECT", "Effect", "effect", {}, 1.0, "effect"),
    ("FIRE", "Fire", "fire", {}, 1.0, "fire"),
    ("FLARE", "Lens flare", "flare", {}, 1.0, "flare"),
)

# Collision materials carry their attributes in the name (section 4.3 and 8).
COL_RE = re.compile(r"^col_ground(\d+)_foot(\d+)_area(\d+)_mask(\d+)$")
WATER_AREAS = (1, 7)
SUFFIX_RE = re.compile(r"^(.*)\.(\d{3,})$")


def strip_suffix(name):
    """A name without the `.001` Blender adds to keep object, mesh and material names unique."""
    found = SUFFIX_RE.match(name)
    return found.group(1) if found else name


def parse_col(name):
    """(ground, foot, area, mask) of a collision material name, or None."""
    found = COL_RE.match(strip_suffix(name))
    return tuple(int(part) for part in found.groups()) if found else None


def col_name(ground, foot, area, mask):
    return "col_ground%d_foot%d_area%d_mask%d" % (ground, foot, area, mask)


def game_name(name, role=None):
    """The game's name of a node: no role prefix, no Blender suffix."""
    name = strip_suffix(name)
    prefix = PREFIX.get(role) if role else None
    if prefix is None:
        for candidate in PREFIX.values():
            if name.startswith(candidate):
                prefix = candidate
                break
    if prefix and name.startswith(prefix):
        return name[len(prefix):]
    return name


def name_bytes(text):
    try:
        return len(text.encode("cp932"))
    except UnicodeEncodeError:
        return len(text.encode("utf-8"))


def golden_colour(number, saturation=0.65, value=0.9, alpha=1.0):
    """A distinct colour for a small integer (hue steps by the golden ratio)."""
    import colorsys

    hue = (number * 0.618033988749895) % 1.0
    r, g, b = colorsys.hsv_to_rgb(hue, saturation, value)
    return (r, g, b, alpha)


def collision_colour(attrs, mode):
    ground, foot, area, mask = attrs
    if mode == "AREA":
        if area in WATER_AREAS:
            return (0.15, 0.35, 0.95, 1.0)
        return golden_colour(area + 3)
    if mode == "FOOT":
        return golden_colour(foot + 11)
    if mode == "GROUND":
        return golden_colour(ground + 5)
    if mode == "MASK":
        return (0.4, 0.8, 0.4, 1.0) if mask == 0 else golden_colour(mask + 17)
    return (0.6, 0.6, 0.6, 1.0)


def game_to_blender(v):
    """A game-space (Y up) local vector in Blender's Z-up frame, as the glTF importer converts."""
    return (v[0], -v[2], v[1])


# Friendly labels for keys of the map tree (`extras.c2.map`): the add-on draws whatever the
# exporter writes and labels the keys it knows.
MAP_LABELS = {
    "images": "Texture files (IMG)",
    "img": "Texture files (IMG)",
    "packs": "Model packs (PCP)",
    "pcp": "Model packs (PCP)",
    "active_light_set": "Active light set",
    "light_sets": "Light sets",
    "light_set": "Light sets",
    "fov": "FOV (ignored, always 52)",
    "bgcolor": "Background colour",
    "bgcolor2": "Background colour 2",
    "ambient": "Ambient (128 neutral)",
    "lights": "Lights",
    "light": "Lights",
    "plights": "Point lights",
    "plight": "Point lights",
    "fog_enable": "Fog on",
    "fog": "Fog (near, far, r, g, b, offset, far2)",
    "floor": "Floor",
    "chara_pos": "Character position",
    "def_foot": "Default foot sound",
    "time_flag": "Time (follow clock, blend, hour, fixed)",
    "time_light_num": "Time light sets (8 or less)",
    "time_cfade": "Time colour fade",
    "sky_info": "Sky (int, float, sun angle)",
    "lens_flare": "Lens flare",
    "all_scissor": "All scissor",
    "chara_light_adjust": "Character light adjust",
    "cameras": "Fixed cameras",
    "camera_info": "Fixed cameras",
}


def map_label(key):
    return MAP_LABELS.get(str(key).lower(), str(key))
