from pathlib import Path

from c2s import disc, index, pathnames

# Cross-references in a decompiled source tree: what starts an event, what loads a map, and where a
# monster kind is loaded and which AI script it runs. These are the links a mod has to keep intact
# (a fight that ends from inside its monsters' AI, a story warp that enters a map at one program).

# Natives that move to a map or load its script, with the argument positions of the map (or the
# "map/file.stb" path) and of the program it starts at (None when the native takes none).
MAP_CALLS = {
    "load_dungeon_map": (0, None), "load_event_script": (0, 1), "jump_to_map": (1, 2),
    "go_to_interior": (1, 2), "go_to_outside": (1, 2), "enter_interior": (1, 2), "go_to_edit_mode": (0, 1),
}

# Natives that ask the current map's script to run a program: from monster AI (run_event_script)
# and from event scripts (queue_dungeon_event). Either runs the program of whatever map is loaded.
REQUEST_CALLS = ("run_event_script", "queue_dungeon_event", "_RUN_EVENT_SCRIPT", "_DNG_RUN_EVENT")

MONSTER_CALLS = ("load_monster_data", "place_monster", "create_monster", "_LOAD_MONSTER_FILE", "_AUTO_SET_MONSTER")


def map_of(target):
    return target.split("/", 1)[0] if isinstance(target, str) else target


# Whether a tree path is one of map_name's scripts: under a folder named after it (map/s/s51/,
# map/d/d01/) or a file named after it (s51.c, s51_1.c, s51b_1.c), going by its path without
# readable names.
def in_map(tree, path, map_name):
    parts = pathnames.unnamed(tree, path).split("/")
    stem = Path(parts[-1]).stem
    return map_name in parts[:-1] or stem == map_name or stem.startswith(map_name + "_") or \
        (stem.startswith(map_name) and stem[len(map_name):len(map_name) + 1].isalpha())


# The scripts a call runs in: its own file, or every script that includes the library it is in.
def scripts_of(call, users):
    return users.get(call["path"], []) if index.is_library(call["path"]) else [call["path"]]


# One line per distinct call; the same call in each language variant of a script is listed once.
def grouped(calls, users, extra=None):
    groups = {}
    for call in calls:
        key = (index.variant_key(call["path"]), call["text"], call["function"])
        groups.setdefault(key, []).append(call)
    lines = []
    for (_, text, function), members in groups.items():
        first = members[0]
        where = "%s:%d" % (first["path"], first["line"])
        if len(members) > 1:
            where += " (and %d language variant%s)" % (len(members) - 1, "" if len(members) == 2 else "s")
        line = "  %s  %s" % (where, text)
        if function:
            line += "  (in %s)" % function
        if extra:
            note = extra(first)
            if note:
                line += "\n      " + note
        if index.is_library(first["path"]):
            scripts = users.get(first["path"], [])
            shown = sorted({index.variant_key(path) for path in scripts})
            line += "\n      library included by %d script%s: %s" % (
                len(scripts), "" if len(scripts) == 1 else "s", ", ".join(shown[:4]) + (", ..." if len(shown) > 4 else ""))
        lines.append(line)
    return lines


def map_entries(tree, consts):
    entries = []
    for call in index.calls(tree, MAP_CALLS):
        map_at, program_at = MAP_CALLS[call["name"]]
        if map_at >= len(call["args"]):
            continue
        target = index.value(call["args"][map_at], consts)
        program = None
        if program_at is not None and program_at < len(call["args"]):
            program = index.value(call["args"][program_at], consts)
        entries.append(dict(call, map=map_of(target), target=target, program=program))
    return entries


def monster_rows(extracted, defs):
    root = Path(extracted)
    elf = disc.executable(root, defs) if root.is_dir() else None
    layout = defs.get("executable", {}).get("monster_table")
    return disc.monster_table(elf, layout) if elf is not None and layout else {}


def ai_script(row):
    return row["script"] or row["model"]


# Monster kinds the map's scripts load or place, by the AI script each runs.
def map_monster_ais(tree, consts, rows, map_name, users):
    found = {}
    for call in index.calls(tree, MONSTER_CALLS):
        if not call["args"] or not any(in_map(tree, path, map_name) for path in scripts_of(call, users)):
            continue
        kind = index.value(call["args"][0], consts)
        if kind in rows:
            found.setdefault(ai_script(rows[kind]), set()).add(kind)
    return found


def event_report(tree, consts, number, map_name=None, rows=None):
    outline = index.outlines(tree)
    users = index.includers(outline)
    lines = ["event %d%s" % (number, " in map " + map_name if map_name else "")]
    defined = {}
    for path, entry in outline.items():
        if map_name and not in_map(tree, path, map_name):
            continue
        for program in entry["programs"]:
            if index.value(program["number"], consts) == number:
                defined.setdefault((index.variant_key(path), program["function"]), []).append(path)
    lines += ["", "defined by:"]
    for (key, function), paths in sorted(defined.items()):
        lines.append("  %s  PROGRAM(%d, %s)" % (paths[0] if len(paths) == 1 else key, number, function))
    if not defined:
        lines.append("  no script%s has PROGRAM(%d, ...)" % (" of " + map_name if map_name else "", number))
    started = [entry for entry in map_entries(tree, consts) if entry["program"] == number
               and (map_name is None or entry["map"] == map_name)]
    lines += ["", "started by moving to the map at this program:"]
    lines += grouped(started, users) or ["  none found"]
    requested = [call for call in index.calls(tree, REQUEST_CALLS)
                 if call["args"] and index.value(call["args"][0], consts) == number]
    ais = map_monster_ais(tree, consts, rows or {}, map_name, users) if map_name and rows else {}

    def placed_here(call):
        plain = pathnames.unnamed(tree, call["path"])
        kinds = ais.get(Path(plain).stem) if plain.startswith("dungeon/monster/") else None
        if not kinds:
            return None
        return "<- %s places kind %s, which runs this AI" % (map_name, ", ".join(
            "%d %s" % (kind, rows[kind]["name"]) for kind in sorted(kinds)))

    lines += ["", "requested while a map is loaded (runs the loaded map's program %d):" % number]
    lines += grouped(requested, users, placed_here) or ["  none found"]
    return "\n".join(lines)


def map_report(tree, consts, map_name):
    outline = index.outlines(tree)
    users = index.includers(outline)
    entries = [entry for entry in map_entries(tree, consts) if entry["map"] == map_name]
    lines = ["map %s" % map_name, "", "scripts:"]
    lines += ["  " + path for path in sorted(outline) if in_map(tree, path, map_name) and not index.is_library(path)] \
        or ["  none in this tree"]
    lines += ["", "entered by:"]
    lines += grouped(entries, users, lambda entry: None if entry["program"] is None else "starts program %d" % entry["program"]) \
        or ["  no call names it as a string (calls that pass a map number are not matched)"]
    return "\n".join(lines)


def monster_report(tree, consts, rows, wanted):
    text = str(wanted)
    number = int(text) if text.lstrip("-").isdigit() else index.value(text, consts)
    if number is None:
        matches = [kind for kind, row in rows.items() if row["name"].lower() == text.lower()]
        number = matches[0] if matches else None
    if number is None:
        return "no monster kind %s (give its number, its MONSTER_ constant or its name)" % wanted
    row = rows.get(number)
    names = [name for name, value in consts.items() if name.startswith("MONSTER_") and value == number]
    lines = ["monster kind %d%s%s" % (number, " " + row["name"] if row else "", " (%s)" % ", ".join(names) if names else "")]
    if row:
        script = ai_script(row)
        sharing = sorted(kind for kind, other in rows.items() if ai_script(other) == script and kind != number)
        lines += ["  model:     dungeon/monster/%s.chr" % row["model"],
                  "  AI script: %s (on the disc: dungeon/monster/%s.stb)" % (
                      pathnames.named(tree, "dungeon/monster/%s.c" % script), script)]
        if sharing:
            lines.append("  the same AI script also runs kinds: " + ", ".join(
                "%d %s" % (kind, rows[kind]["name"]) for kind in sharing))
    else:
        lines.append("  (the model and AI script are read from the extracted disc; pass --extracted)")
    users = index.includers(index.outlines(tree))
    loads = [call for call in index.calls(tree, MONSTER_CALLS)
             if call["args"] and index.value(call["args"][0], consts) == number]
    lines += ["", "loaded or placed by:"] + (grouped(loads, users) or ["  none found"])
    return "\n".join(lines)
