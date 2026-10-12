import re
from pathlib import Path

from c2s import index, source

# build --start-at MAP:PROGRAM makes a test image whose New Game goes straight to a scene, so trying
# a mod does not mean replaying up to it. Nothing in the source tree changes: build compiles a
# patched copy of the script a new game starts in.
#
# A new game enters the dungeon loop on floor 20 of dungeon 1 and runs program 1010 (floor_start)
# of map/d/d01/d01_<language>.stb (title.cpp, dng_main.cpp EntryEventScript). The patch puts a
# function in front of that program: the first time it runs it sets START_FLAG and warps the way the
# game's own story warps out of d01 do (load_dungeon_map, then load_event_script at the program);
# after that it hands over to the original floor_start, so later visits to the dungeon play normally.
# The warp works for maps the game loads with load_dungeon_map (story scenes and dungeon-like maps);
# xref map NAME shows whether the game ever does.

NEW_GAME_SCRIPT = re.compile(r"^map/d/d01/d01_[0-9]\.c$")
START_PROGRAM = "floor_start"
START_FLAG = 2047
TARGET = re.compile(r"^([A-Za-z0-9_]+)(?:/([A-Za-z0-9_]+))?:(-?\w+)$")


def parse_target(text):
    match = TARGET.match(text)
    if match is None:
        raise SystemExit("--start-at takes MAP:PROGRAM or MAP/SCRIPT:PROGRAM, as in s51:story_50 or s06/s06b:500")
    map_name, script, program = match.groups()
    return {"map": map_name, "script": script or map_name, "program": program}


def function_text(target, original):
    return "\n".join([
        "// Added by build --start-at: on a new game, goes straight to map %s at program %s. Flag %d" % (
            target["map"], target["program"], START_FLAG),
        "// marks that it has; after that the original %s runs." % original,
        "void start_at_scene(void)",
        "{",
        "    int started;",
        "",
        "    started = get_flag(%d);" % START_FLAG,
        "    if (started == 0) {",
        "        set_flag(%d, 1);" % START_FLAG,
        "        init_event();",
        "        fade_out(0);",
        "        wait;",
        '        load_dungeon_map("%s");' % target["map"],
        '        load_event_script("%s/%s.stb", %s);' % (target["map"], target["script"], target["program"]),
        "        return;",
        "    }",
        "    %s();" % original,
        "}",
        "",
    ])


def patched(text, target):
    program = re.compile(r"^PROGRAM\(%s, (\w+)\);$" % START_PROGRAM, re.M)
    match = program.search(text)
    if match is None:
        return None
    original = match.group(1)
    text = text[:match.start()] + "PROGRAM(%s, start_at_scene);" % START_PROGRAM + text[match.end():]
    pragmas = re.search(r"^#pragma padding_of ", text, re.M)
    at = pragmas.start() if pragmas else len(text)
    return text[:at] + function_text(target, original) + "\n" + text[at:]


# The patched new-game scripts, keyed by tree-relative path, for build to compile in their place.
def overrides(tree, target):
    root = Path(tree)
    found = {}
    for path in sorted(root.rglob("*" + source.EXTENSION)):
        relative = path.relative_to(root).as_posix()
        if NEW_GAME_SCRIPT.match(relative):
            text = patched(path.read_text(encoding="utf-8"), target)
            if text is None:
                raise SystemExit("%s has no PROGRAM(%s, ...) line to start from" % (relative, START_PROGRAM))
            found[relative] = text
    if not found:
        raise SystemExit("no map/d/d01/d01_N%s in %s: the tree must hold the scripts a new game starts in"
                         % (source.EXTENSION, tree))
    return found


def warnings(tree, consts, target):
    lines = []
    program = index.value(target["program"], consts)
    if program is None:
        lines.append("program %s is not a number or a program name the headers define" % target["program"])
    loads = [call for call in index.calls(tree, ["load_dungeon_map"])
             if call["args"] and index.value(call["args"][0], consts) == target["map"]]
    if not loads:
        lines.append("the game never loads %s with load_dungeon_map, so the warp may not work; "
                     "xref map %s shows how it is entered" % (target["map"], target["map"]))
    return lines
