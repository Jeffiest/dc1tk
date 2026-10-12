from c2s import index

# Save bit flags: which numbers a source tree's scripts and the engine use, and which are free for
# a mod. The save holds FLAG_COUNT one-bit flags (CSaveData::CheckBitFlagNo rejects the rest).

FLAG_COUNT = 0x800

# Flags the engine reads or writes by number, from the ChronicleTwo decomp: GetBitFlag and
# SetBitFlag calls with a literal or a named SAVE_BIT_FLAG, and the named flags it only lists.
ENGINE_FLAGS = {
    4: "SAVE_FLAG_MENU_TIME_EVENING", 12: "menushop.cpp", 13: "inventmn.cpp", 27: "menushop.cpp",
    33: "editloop.cpp, editmenu.cpp", 34: "editloop.cpp", 48: "inventmn.cpp, userdata.cpp", 49: "menusys.cpp",
    50: "dng_main.cpp", 52: "dng_main.cpp", 53: "dng_main.cpp", 54: "menumain.cpp", 59: "dng_main.cpp",
    60: "fishing.cpp", 61: "editloop.cpp", 63: "menuaqua.cpp", 102: "SAVE_FLAG_MENU_TIME_MIDNIGHT",
    103: "SAVE_FLAG_MENU_TIME_1AM", 104: "SAVE_FLAG_MENU_TIME_FORCED_OFF", 220: "dngmenu.cpp", 235: "fishing.cpp",
    240: "fishing.cpp", 250: "editloop.cpp", 254: "userdata.cpp", 317: "dng_debug.cpp, dngmenu.cpp",
    330: "editanalyze.cpp", 344: "memcard.cpp, menumain.cpp, savedata.cpp", 356: "editanalyze.cpp",
    424: "memcard.cpp, menumain.cpp, savedata.cpp", 444: "editanalyze.cpp",
    520: "editanalyze.cpp, editexception.cpp, editloop.cpp, editmenu.cpp", 536: "editanalyze.cpp",
    600: "SAVE_FLAG_WORLD_MAP_2", 616: "SAVE_FLAG_WORLD_MAP_4", 640: "SAVE_FLAG_WORLD_MAP_3",
    700: "SAVE_FLAG_WORLD_MAP_S12_REDIRECT", 702: "SAVE_FLAG_GEKKA_VIEW", 736: "menumain.cpp",
    799: "charasetup.cpp, gamedata.cpp, memcard.cpp", 800: "editanalyze.cpp, menumain.cpp",
}

# Set by the opening of an image built with build --start-at (see startat.py).
TOOL_FLAGS = {2047: "build --start-at"}

# Natives that take a save flag number as their first argument: event scripts' get_flag and
# set_flag and monster scripts' get_dungeon_flag and set_dungeon_flag, under SDK or original names.
FLAG_FUNCTIONS = {"get_flag": "get", "set_flag": "set", "get_dungeon_flag": "get", "set_dungeon_flag": "set",
                  "_GET_FLAG": "get", "_SET_FLAG": "set", "_GET_BIT_FLAG": "get", "_SET_BIT_FLAG": "set"}


def scan(tree, consts):
    uses, computed = {}, []
    for call in index.calls(tree, FLAG_FUNCTIONS):
        if not call["args"]:
            continue
        number = index.value(call["args"][0], consts)
        use = dict(call, access=FLAG_FUNCTIONS[call["name"]], expr=call["args"][0])
        if isinstance(number, int):
            uses.setdefault(number, []).append(use)
        else:
            computed.append(use)
    return {"uses": uses, "computed": computed}


def used_numbers(found):
    return set(found["uses"]) | set(ENGINE_FLAGS) | set(TOOL_FLAGS)


def free_ranges(used):
    ranges, start = [], None
    for number in range(FLAG_COUNT + 1):
        free = number < FLAG_COUNT and number not in used
        if free and start is None:
            start = number
        elif not free and start is not None:
            ranges.append((start, number - 1))
            start = None
    return ranges


def range_text(ranges):
    return ", ".join(str(a) if a == b else "%d-%d" % (a, b) for a, b in ranges)


def names_by_number(consts):
    names = {}
    for name, number in consts.items():
        if name.startswith(("FLAG_", "SAVE_FLAG_")):
            names.setdefault(number, name)
    return names


def summary(found):
    used = used_numbers(found)
    script_only = set(found["uses"])
    ranges = free_ranges(used)
    largest = max(ranges, key=lambda pair: pair[1] - pair[0], default=None)
    lines = ["%d of %d flags are used: %d by scripts, %d by the engine, %d by these tools"
             % (len(used), FLAG_COUNT, len(script_only), len(ENGINE_FLAGS), len(TOOL_FLAGS)),
             "highest flag a script uses: %d; highest the engine uses: %d" % (max(script_only, default=-1), max(ENGINE_FLAGS))]
    if largest is not None:
        lines.append("largest free block: %s (%d flags); prefer it, since gaps between the game's own flags may be"
                     % (range_text([largest]), largest[1] - largest[0] + 1))
        lines.append("reached through menu data or arithmetic this scan cannot follow")
    lines.append("free: " + range_text(ranges))
    if found["computed"]:
        lines += ["", "%d calls pass a flag number worked out at run time; check these by hand:" % len(found["computed"])]
        lines += ["  %s:%d  %s" % (use["path"], use["line"], use["text"]) for use in found["computed"]]
    return "\n".join(lines)


def used_list(found, consts):
    names = names_by_number(consts)
    lines = []
    for number in sorted(used_numbers(found)):
        uses = found["uses"].get(number, [])
        sets = sum(1 for use in uses if use["access"] == "set")
        parts = []
        if uses:
            parts.append("set %d, read %d" % (sets, len(uses) - sets))
        if number in ENGINE_FLAGS:
            parts.append("engine: " + ENGINE_FLAGS[number])
        if number in TOOL_FLAGS:
            parts.append(TOOL_FLAGS[number])
        lines.append("%5d  %-40s %s" % (number, names.get(number, ""), "; ".join(parts)))
    return "\n".join(lines)


def who(found, number, consts):
    name = names_by_number(consts).get(number)
    lines = ["flag %d%s" % (number, " (%s)" % name if name else "")]
    if number in ENGINE_FLAGS:
        lines.append("  engine: " + ENGINE_FLAGS[number])
    if number in TOOL_FLAGS:
        lines.append("  tools: " + TOOL_FLAGS[number])
    for use in found["uses"].get(number, []):
        lines.append("  %-4s %s:%d  %s  (in %s)" % (use["access"], use["path"], use["line"], use["text"], use["function"]))
    if len(lines) == 1:
        lines.append("  free: no script, the engine or these tools use it")
    return "\n".join(lines)
