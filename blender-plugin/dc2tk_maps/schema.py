import re


PREFIX = {
    "map": "map:",
    "placement": "place:",
    "piece": "piece:",
    "point": "point:",
    "camera": "camera:",
}
GROUP_NAMES = ("placements", "points", "cameras")
ROLES = ("map", "placement", "piece", "mds", "point", "group", "camera", "display")

POINT_KINDS = ("pos", "invent", "event", "flare", "plight", "fire", "effect", "sound", "anime")
PIECE_KINDS = ("model", "collision", "camera", "chara")
COLLISION_KINDS = ("collision", "camera")

NAME_LIMIT = 31
TARGET_LIMIT = 15
PLACEMENT_LIMIT = 320
PLACEMENT_LIMIT_DUNGEON = 400
TIME_LIGHT_NUM_LIMIT = 8


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


EVENT_DEFAULT_SIZE = {
    "door": 15.0,
    "close_door": 15.0,
    "ed_door": 15.0,
    "lddr_b": 25.0,
    "lddr_t": 25.0,
    "t_box": 20.0,
}


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
    {
        "key": "color",
        "label": "Colour",
        "kind": "int",
        "size": 3,
        "labels": ("R", "G", "B"),
        "default": [128, 90, 38],
    },
    {"key": "draw", "label": "Draw", "kind": "int"},
    {"key": "extra", "label": "Extra", "kind": "int", "size": 2, "optional": True},
]


DATA_FIELDS = {
    "pos": [],
    "invent": [
        {"key": "id", "label": "Invent id", "kind": "int"},
        {
            "key": "min",
            "label": "Box min",
            "kind": "float",
            "size": 3,
            "labels": ("X", "Y", "Z"),
            "default": [-10.0, -10.0, -10.0],
        },
        {
            "key": "max",
            "label": "Box max",
            "kind": "float",
            "size": 3,
            "labels": ("X", "Y", "Z"),
            "default": [10.0, 10.0, 10.0],
        },
        {"key": "unknown_int", "label": "Int", "kind": "int"},
        {"key": "unknown_float", "label": "Float", "kind": "float"},
        {"key": "angle", "label": "Angle (degrees)", "kind": "float"},
    ],
    "event": [
        {"key": "kind", "label": "Event kind", "kind": "str", "default": "event"},
        {"key": "event_no", "label": "Event number", "kind": "int"},
        {"key": "program", "label": "Program", "kind": "int"},
        {
            "key": "args",
            "label": "Arguments",
            "kind": "int",
            "size": 3,
            "labels": ("Arg 1", "Arg 2", "Call program"),
        },
        {"key": "target", "label": "Target", "kind": "str"},
        {
            "key": "flags",
            "label": "Flags",
            "kind": "int",
            "size": 3,
            "labels": ("Action button", "Use item", "0x100"),
        },
    ],
    "flare": _FIRE,
    "fire": _FIRE,
    "plight": [
        {"key": "power", "label": "Power", "kind": "float", "default": 100.0},
        {
            "key": "color",
            "label": "Colour (0-511)",
            "kind": "int",
            "size": 3,
            "labels": ("R", "G", "B"),
            "default": [255, 255, 255],
        },
        {"key": "mode", "label": "Mode", "kind": "int"},
        {"key": "args", "label": "Arguments", "kind": "int", "size": 4},
        {"key": "flicker", "label": "Flicker type", "kind": "int"},
        {"key": "depth", "label": "Flicker depth", "kind": "float"},
        {"key": "period", "label": "Flicker period", "kind": "float"},
    ],
    "effect": [{"key": "effect", "label": "Effect name", "kind": "str"}],
    "sound": [
        {"key": "number", "label": "Sound number", "kind": "int"},
        {"key": "near", "label": "Near", "kind": "float", "default": 50.0},
        {"key": "far", "label": "Far", "kind": "float", "default": 200.0},
        {"key": "unknown_int", "label": "Int", "kind": "int"},
        {"key": "shape", "label": "Shape", "kind": "int"},
        {"key": "start", "label": "Start", "kind": "float", "size": 3, "labels": ("X", "Y", "Z")},
        {"key": "end", "label": "End", "kind": "float", "size": 3, "labels": ("X", "Y", "Z")},
    ],
    "anime": [
        {"key": "placement", "label": "Placement", "kind": "str"},
        {"key": "piece", "label": "Piece", "kind": "str"},
        {"key": "frame", "label": "Model node", "kind": "str"},
        {"key": "drives", "label": "Drives", "kind": "int", "default": 2, "choices": ANIME_DRIVES},
        {"key": "mode", "label": "Mode", "kind": "int", "default": 1, "choices": ANIME_MODES},
        {"key": "start", "label": "Start", "kind": "float", "size": 3, "labels": _XYZ},
        {
            "key": "step",
            "label": "Step (per frame)",
            "kind": "float",
            "size": 3,
            "labels": _XYZ,
            "default": [0.0, 1.0, 0.0],
        },
        {"key": "end", "label": "End", "kind": "float", "size": 3, "labels": _XYZ},
        {"key": "uniform", "label": "Uniform (Y and Z follow X)", "kind": "flag", "optional": True},
        {"key": "relative", "label": "Relative (in the piece's own space)", "kind": "flag", "optional": True},
    ],
}


for fields in DATA_FIELDS.values():
    for field in fields:
        field.setdefault("size", 1)
        field.setdefault("labels", None)
        field.setdefault("optional", False)
        field.setdefault("choices", None)
        if "default" not in field:
            zero = {"int": 0, "flag": 0, "float": 0.0, "str": ""}[field["kind"]]
            field["default"] = zero if field["size"] == 1 else [zero] * field["size"]


def data_field(kind, key):
    for field in DATA_FIELDS[kind]:
        if field["key"] == key:
            return field
    raise KeyError(key)


def anime_vectors(drives, mode):
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
        if field["optional"]:
            continue
        data[field["key"]] = list(field["default"]) if isinstance(field["default"], list) else field["default"]
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


POINT_PRESETS = (
    ("SPAWN", "Position (spawn)", "pos", {}, 1.0, "point"),


    ("DOOR", "Door", "event", {"kind": "door", "flags": [1, 0, 1]}, 1.0, "door"),
    ("CHEST", "Treasure chest", "event", {"kind": "t_box", "flags": [1, 0, 0]}, 1.0, "t_box"),
    ("EVENT", "Event trigger", "event", {"kind": "event", "flags": [0, 0, 0]}, 30.0, "event"),
    ("LIGHT", "Point light", "plight", {}, 1.0, "plight"),
    ("SOUND", "Sound emitter", "sound", {}, 1.0, "sound"),
    ("INVENT", "Invent box", "invent", {}, 1.0, "invent"),
    ("EFFECT", "Effect", "effect", {}, 1.0, "effect"),
    ("FIRE", "Fire", "fire", {}, 1.0, "fire"),
    ("FLARE", "Lens flare", "flare", {}, 1.0, "flare"),

    ("ANIME", "Piece animation", "anime", {"uniform": 0, "relative": 0}, 1.0, "anime"),
)


COL_RE = re.compile(r"^col_ground(\d+)_foot(\d+)_area(\d+)_mask(\d+)$")
WATER_AREAS = (1, 7)
SUFFIX_RE = re.compile(r"^(.*)\.(\d{3,})$")


def strip_suffix(name):
    found = SUFFIX_RE.match(name)
    return found.group(1) if found else name


def parse_col(name):
    found = COL_RE.match(strip_suffix(name))
    return tuple(int(part) for part in found.groups()) if found else None


def col_name(ground, foot, area, mask):
    return "col_ground%d_foot%d_area%d_mask%d" % (ground, foot, area, mask)


def game_name(name, role=None):
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


def truncate_name(name, limit=NAME_LIMIT):
    while name_bytes(name) > limit:
        name = name[:-1]
    return name


def golden_colour(number, saturation=0.65, value=0.9, alpha=1.0):
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
    return (v[0], -v[2], v[1])


def blender_to_game(v):
    return (v[0], v[2], -v[1])


# The player's size where the engine moves it (MoveCheck, gameutil.cpp:1599): walls are kept 15
# units away (CheckWidth), the floor is looked for 20 units below the feet (GetFootPoly), and a
# triangle whose normal's up part is below 0.5 is a wall.
PLAYER_RADIUS = 15.0
FOOT_PROBE = 20.0
WALL_NORMAL_Y = 0.5
# A dungeon grid cell (CAutoMapGen::Build places cell x, y at x*320, 0, y*320).
DUNGEON_CELL = 320.0
# Collision queries take the triangles of every node whose box overlaps, up to the caller's
# buffer: 128 for monsters and objects, 512 for the camera (docs/game/maps.md section 4.3).
QUERY_TRIANGLES = 128
CAMERA_QUERY_TRIANGLES = 512
# Models a map's model pack holds at most (CMdsList::LoadPCPFile).
PACK_MODELS = 0x1FF
# A new map's name: lower-case letters, digits and _, from a letter, 12 at most.
NEW_MAP_RE = re.compile(r"^[a-z][a-z0-9_]{0,11}$")
# A mod's new programs (NEW_PROGRAM) are numbered from 9000 to 9999.
MOD_PROGRAMS = (9000, 10000)


MAP_LABELS = {
    "images": "Texture files (IMG)",
    "packs": "Model packs (PCP)",
    "active_light_set": "Active light set",
    "light_sets": "Light sets",
    "fov": "FOV (ignored, always 52)",
    "bgcolor": "Background colour",
    "bgcolor2": "Background colour 2",
    "ambient": "Ambient (128 neutral)",
    "lights": "Lights",
    "fog_enable": "Fog on",
    "fog": "Fog",
    "floor": "Floor",
    "chara_pos": "Character position",
    "def_foot": "Default foot sound",
    "time_flag": "Time flag",
    "time_light_num": "Time light sets (8 or less)",
    "time_cfade": "Time colour fade",
    "sky_info": "Sky",
    "lens_flare": "Lens flare",
    "all_scissor": "All scissor",
    "chara_light_adjust": "Character light adjust",
    "cameras": "Fixed cameras",
}


def map_label(key):
    return MAP_LABELS.get(key, key)
