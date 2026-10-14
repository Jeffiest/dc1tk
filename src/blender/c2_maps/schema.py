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
    when `data` is the raw argument list of the FUNC_<KIND>_DATA line.

    `choices` are (value, identifier, label, description) of an int the panel shows as a menu
    (a value not among them shows as the first); `old` is where scenes exported before the field
    had its name kept it: a key, or (key, index) of an element of a list."""

    def __init__(self, key, label, kind, size=1, pos=0, labels=None, default=None, optional=False, choices=None, old=None):
        self.key = key
        self.label = label
        self.kind = kind  # "int", "float", "str" or "flag" (an int 0 or 1)
        self.size = size
        self.pos = pos
        self.labels = labels  # one per element of a vector
        self.optional = optional
        self.choices = choices
        self.old = old
        if default is None:
            zero = {"int": 0, "flag": 0, "float": 0.0, "str": ""}[kind]
            default = zero if size == 1 else [zero] * size
        self.default = default


F = Field

# FUNC_ANIME_DATA (docs/assets/maps.md section 5.1): what a point changes every frame and how.
# Values the engine does not know act as 0 in both.
ANIME_DRIVES = (
    (0, "NOTHING", "Nothing", "Changes nothing"),
    (1, "POSITION", "Position", "Position in the parent's space, game units"),
    (2, "ROTATION", "Rotation", "Rotation in degrees (Euler XYZ)"),
    (3, "SCALE", "Scale", "Scale along each axis"),
    (4, "COLOUR", "Colour", "Draws the model node unlit in this colour, 0-255 (frames only)"),
    (5, "ALPHA", "Alpha", "Alpha factor, 0-1, in the first number (frames only)"),
)
ANIME_MODES = (
    (0, "HOLD", "Hold", "Holds the start value"),
    (1, "LOOP", "Loop", "Adds the step every frame, without end"),
    (2, "STOP", "Stop at the end", "Adds the step until the end value, then holds it"),
    (3, "REPEAT", "Repeat", "Adds the step until the end value, then starts again at the start value"),
    (4, "PINGPONG", "Ping-pong", "Moves by the step from the start value to the end value and back, without end"),
    (5, "ONCE", "There and back once", "Moves by the step to the end value and back to the start value, then stops"),
    (6, "LOOK", "Look at the player", "Turns about Y and X to face the player, kept between start and end (rotation only)"),
    (7, "RANDOM", "Random", "A new random value between start and end every frame"),
    (8, "MINUTE", "Minute hand", "Turns about Y once an hour of the game's clock (-360 degrees an hour)"),
    (9, "HOUR", "Hour hand", "Turns about Y once in twelve hours of the game's clock"),
    (10, "BY_HOUR", "By the hour", "The end value within the step's hours, the start value outside, blended over its fade hours"),
)
_XYZ = ("X", "Y", "Z")
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
        F("frame", "Model node", "str", 1, 2),
        F("drives", "Drives", "int", 1, 3, default=2, choices=ANIME_DRIVES, old="kind"),
        F("mode", "Mode", "int", 1, 4, default=1, choices=ANIME_MODES, old="unknown_int"),
        F("start", "Start", "float", 3, 5, _XYZ, old="v0"),
        F("step", "Step (per frame)", "float", 3, 8, _XYZ, [0.0, 1.0, 0.0], old="v1"),
        F("end", "End", "float", 3, 11, _XYZ, old="v2"),
        F("uniform", "Uniform (Y and Z follow X)", "flag", 1, 14, optional=True, old=("extra", 0)),
        F("relative", "Relative (in the piece's own space)", "flag", 1, 15, optional=True, old=("extra", 1)),
    ],
}


def upgrade_data(kind, data):
    """A point's `data` dict with the names of fields that scenes exported before they had them
    used (anime's kind, unknown_int, v0, v1, v2 and extra) turned into today's."""
    fields = [field for field in DATA_FIELDS.get(kind, []) if field.old is not None]
    if not isinstance(data, dict) or not fields:
        return data
    data = dict(data)
    olds = set()
    for field in fields:
        key, index = (field.old, None) if isinstance(field.old, str) else field.old
        olds.add(key)
        if field.key in data or key not in data:
            continue
        value = data[key]
        if index is not None:
            value = value[index] if isinstance(value, (list, tuple)) and index < len(value) else None
        if value is not None:
            data[field.key] = value
    for key in olds:
        data.pop(key, None)
    return data


def anime_vectors(drives, mode):
    """{key: (label, element labels)} of the vectors an anime point's mode reads (section 5.1);
    the ones it leaves out do nothing."""
    unit = {1: "units", 2: "degrees", 4: "0-255"}.get(drives)
    elements = ("R", "G", "B") if drives == 4 else ("Alpha", "-", "-") if drives == 5 else _XYZ

    def label(text):
        return "%s (%s)" % (text, unit) if unit else text

    if mode in (1, 2, 3, 4, 5):
        found = {"start": (label("Start"), elements), "step": (label("Step per frame"), elements)}
        if mode != 1:
            found["end"] = (label("End"), elements)
        return found
    if mode == 6:
        return {"start": (label("Lowest"), ("Pitch X", "Yaw Y", "Z")), "end": (label("Highest"), ("Pitch X", "Yaw Y", "Z"))}
    if mode == 7:
        return {"start": (label("Lowest"), elements), "end": (label("Highest"), elements)}
    if mode in (8, 9):
        return {"start": (label("Start (Y follows the clock)"), elements)}
    if mode == 10:
        return {
            "start": (label("Outside the hours"), elements),
            "step": ("Hours", ("From", "To", "Fade")),
            "end": (label("Within the hours"), elements),
        }
    return {"start": (label("Value"), elements)}

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
    # as every door of the disc: the action button opens it, and 0x100 fades the screen out before
    # its program runs or the target map loads (editevent.cpp)
    ("DOOR", "Door", "event", {"kind": "door", "flags": [1, 0, 1]}, 1.0, "door"),
    ("CHEST", "Treasure chest", "event", {"kind": "t_box", "flags": [1, 0, 0]}, 1.0, "t_box"),
    ("EVENT", "Event trigger", "event", {"kind": "event", "flags": [0, 0, 0]}, 30.0, "event"),
    ("LIGHT", "Point light", "plight", {}, 1.0, "plight"),
    ("SOUND", "Sound emitter", "sound", {}, 1.0, "sound"),
    ("INVENT", "Invent box", "invent", {}, 1.0, "invent"),
    ("EFFECT", "Effect", "effect", {}, 1.0, "effect"),
    ("FIRE", "Fire", "fire", {}, 1.0, "fire"),
    ("FLARE", "Lens flare", "flare", {}, 1.0, "flare"),
    # turns about Y, a degree a frame; the add operator fills in what the active object is
    ("ANIME", "Piece animation", "anime", {"uniform": 0, "relative": 0}, 1.0, "anime"),
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
