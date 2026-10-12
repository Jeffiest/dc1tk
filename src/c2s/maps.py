import functools
import re
from pathlib import Path

from c2s import additions, index, messages, pathnames, xref

# A catalog of the game's maps for mod makers: how the game uses each one (a town map it walks into,
# or a map loaded dungeon-style for a scene or a floor), whether the player plays in it, the
# programs its scripts define and which numbers are free for a mod, the trigger points in its .map
# file and the program each starts, and where its scripts place the player.

TOWN_CALLS = ("jump_to_map", "go_to_interior", "go_to_outside", "enter_interior")
FUNC_FIELD = re.compile(r"^(FUNC_\w+|PARTS_NAME)\s*(.*?);?\s*$")
MOD_PROGRAMS = range(9000, 10000)


def map_names(extracted, defs):
    code = defs["text"].get("languages", {}).get(defs["language"])
    found = messages.disc_names(extracted, code) if code is not None else {}
    return {key: name for key, name in found.get("map_id", {}).items() if isinstance(key, str)}


@functools.lru_cache(maxsize=None)
def map_files(extracted):
    return {path.stem: path for path in Path(extracted).rglob("*.map") if path.is_file()}


def numbers(text):
    return [float(value) for value in re.findall(r"-?\d+(?:\.\d+)?(?:[eE]-?\d+)?", text)]


# The trigger points of a map file: name, the map part they belong to, kind (event, item use,
# sound and so on), the program an event point starts and how, and position and size.
def trigger_points(path):
    text = Path(path).read_bytes().decode("cp932", "replace")
    found, part, point = [], "", None
    for line in re.split(r"[\r\n]+", text):
        line = line.strip()
        match = FUNC_FIELD.match(line)
        if match is None:
            continue
        key, value = match.groups()
        if key == "PARTS_NAME":
            part = value.strip('"')
        elif key == "FUNC_DATA":
            point = {"kind": value.split(",")[0].strip('"'), "part": part, "name": "", "program": None,
                     "check": None, "position": None, "size": None}
        elif point is None:
            continue
        elif key == "FUNC_NAME":
            point["name"] = value.strip('"')
        elif key == "FUNC_EVENT_DATA":
            # kind, event number, program, three arguments, target, then flags for the action button
            # and for using an item (mapFUNC_EVENT_DATA in mapload.cpp); with neither, touching it.
            fields = [field.strip() for field in value.split(",")]
            if fields[0].strip('"') == "event" and len(fields) > 2:
                point["program"] = int(fields[2])
                action = len(fields) > 7 and fields[7] not in ("0", "")
                item = len(fields) > 8 and fields[8] not in ("0", "")
                point["check"] = "on the action button" if action else "on using an item" if item else "on touch"
        elif key == "FUNC_INVENT_DATA":
            point["item"] = int(numbers(value)[0])
        elif key == "FUNC_POS":
            values = numbers(value)
            point["position"], point["size"] = values[:3], values[6:9]
        elif key == "FUNC_DATA_END":
            found.append(point)
            point = None
    return found


def program_numbers(outline, consts, paths):
    found = {}
    for path in paths:
        for program in outline[path]["programs"]:
            number = index.value(program["number"], consts)
            if isinstance(number, int):
                found.setdefault(number, set()).add(program["number"])
    return found


# Everything the catalog says about each map: {code: entry}.
def catalog(tree, extracted, defs, consts):
    names = map_names(extracted, defs)
    outline = index.outlines(tree)
    plain = {path: pathnames.unnamed(tree, path) for path in outline if not index.is_library(path)}
    users = index.includers(outline)
    entries = {}
    for call in xref.map_entries(tree, consts):
        if isinstance(call["map"], str):
            entries.setdefault(call["map"], []).append(call)
    codes = sorted(set(names) | set(entries))
    player_dead = consts.get("player_dead")
    found = {}
    for code in codes:
        scripts = sorted(path for path, unnamed in plain.items() if additions.in_map(unnamed, code))
        programs = program_numbers(outline, consts, scripts)
        calls = entries.get(code, [])
        kinds = []
        if any(call["name"] in TOWN_CALLS for call in calls):
            kinds.append("town")
        if any(call["name"] == "load_dungeon_map" for call in calls):
            kinds.append("dungeon-style")
        found[code] = {"code": code, "name": names.get(code, ""), "scripts": scripts, "programs": programs,
                       "entries": calls, "kinds": kinds, "playable": player_dead in programs, "users": users}
    return found


def summary(found):
    lines = ["%-9s %-30s %-20s %s" % ("map", "name", "the game uses it as", "player plays there")]
    for code, entry in sorted(found.items()):
        if not entry["scripts"]:
            continue
        uses = ", ".join(entry["kinds"]) or "(entered another way)"
        plays = "yes" if entry["playable"] else ""
        lines.append("%-9s %-30s %-20s %s" % (code, entry["name"][:30], uses, plays))
    lines += ["", "\"player plays there\": its scripts handle the player dying (a player_dead program), so the game "
              "lets the player move and fight there.", "Maps used dungeon-style can be a --start-at target."]
    return "\n".join(lines)


def detail(code, found, tree, extracted, consts, programs=()):
    entry = found.get(code)
    if entry is None:
        return "no map %s" % code
    lines = ["map %s%s" % (code, " (%s)" % entry["name"] if entry["name"] else "")]
    lines.append("  the game uses it as: " + (", ".join(entry["kinds"]) or "(entered another way)"))
    lines.append("  player plays there: " + ("yes, its scripts handle the player dying" if entry["playable"]
                                              else "not as far as its scripts show (no player_dead program)"))
    lines += ["", "scripts:"] + ["  " + path for path in entry["scripts"]] or ["  none"]
    names_of = {}
    for name, value in consts.items():
        if isinstance(value, int):
            names_of.setdefault(value, name)
    lines += ["", "programs (number, name):"]
    for number in sorted(entry["programs"]):
        lines.append("  %6d  %s" % (number, ", ".join(sorted(entry["programs"][number]))))
    taken = set(entry["programs"]) | set(programs)
    free = [number for number in MOD_PROGRAMS if number not in taken][:5]
    lines.append("  free for a mod: %s ..." % ", ".join(str(number) for number in free))
    path = map_files(str(extracted)).get(code)
    lines += ["", "trigger points (%s):" % (Path(path).name if path else "no .map file found")]
    for point in trigger_points(path) if path else []:
        where = "at %s" % ", ".join("%g" % value for value in point["position"]) if point["position"] else ""
        if point["program"] is not None:
            what = "starts program %d %s" % (point["program"], point["check"])
        elif "item" in point:
            what = "item use point for item %d" % point["item"]
        else:
            what = point["kind"]
        lines.append("  %-18s %-20s %s %s" % (point["name"] or "(unnamed)", "part " + point["part"] if point["part"] else "",
                                              what, where))
    lines.append("  (switch one off with set_func_point_enabled(part, name, 0); \"\" for the map's own points)")
    lines += ["", "where its scripts place the player (place_chara(0, x, y, z, yaw) and the like):"]
    seen = set()
    for call in index.calls(tree, ["place_chara"]):
        if call["path"] in entry["scripts"] and call["args"][:1] == ["0"]:
            key = tuple(call["args"][1:5])
            if key not in seen:
                seen.add(key)
                lines.append("  %s  (in %s)" % (call["text"], call["function"]))
    lines += ["", "entered by:"] + (xref.grouped(entry["entries"], entry["users"],
                                                lambda call: None if call["program"] is None else "starts program %d" % call["program"])
                                    or ["  none found"])
    return "\n".join(lines)
