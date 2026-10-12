import fnmatch
import re
from pathlib import Path

from c2s import index

# A mod's additions: changes it makes to scripts without carrying copies of them. mod.json lists
# them under "additions", each picking scripts by map or by path and saying what to add:
#
#   {"map": "s17",                                   every script of map s17 (its language variants)
#    "scripts": ["map/s/s17/s17_?.c"],               or paths without readable names, with * and ?
#    "programs": {"9000": "start", "treasure_chest": "open_chest"},   new PROGRAM lines
#    "includes": ["lib/mods/calculator/calculator.c"],               libraries to include
#    "wrap": {"map_start": {"before": ["setup"], "after": ["greet"]}}}   run around a program
#
# Since only the lines a mod adds change, several mods can add to the same script, and a mod still
# fits the scripts after the decompiler improves. A program a script already has can be wrapped but
# not added again. A wrapped program's "after" functions run when its function returns, which a
# program that warps to another map never does.

PROGRAM_LINE = re.compile(r"^PROGRAM\(\s*([^,]+?)\s*,\s*(\w+)\s*\);$", re.M)
MAIN_LINE = re.compile(r"^MAIN\(", re.M)
TRAILING_PRAGMAS = re.compile(r"^#pragma padding_of ", re.M)


# Whether a script, by its path without names, belongs to a map: in a folder named after it, or a
# file named after it (s17.c, s17_1.c, s17b_1.c).
def in_map(path, map_name):
    parts = path.split("/")
    stem = Path(parts[-1]).stem
    return map_name in parts[:-1] or stem == map_name or stem.startswith(map_name + "_") or \
        (stem.startswith(map_name) and stem[len(map_name):len(map_name) + 1].isalpha())


def matches(addition, plain):
    if plain.startswith("lib/"):
        return False
    if "map" in addition and in_map(plain, addition["map"]):
        return True
    return any(fnmatch.fnmatchcase(plain, pattern) for pattern in addition.get("scripts", []))


def number_of(text, consts):
    value = index.value(text, consts)
    return value if isinstance(value, int) else text.strip()


def identifier(text):
    return re.sub(r"\W", "_", text)


def insert_programs(text, lines):
    at = MAIN_LINE.search(text)
    if at is None:
        last = list(PROGRAM_LINE.finditer(text))
        position = last[-1].end() + 1 if last else 0
    else:
        position = at.start()
    return text[:position] + "".join(line + "\n" for line in lines) + text[position:]


def append_code(text, code):
    pragmas = TRAILING_PRAGMAS.search(text)
    position = pragmas.start() if pragmas else len(text)
    return text[:position].rstrip("\n") + "\n\n" + code.rstrip("\n") + "\n" + ("\n" + text[position:] if pragmas else "")


# The script text with one addition applied. mod names the mod, for generated function names and
# messages; place maps a mod file's include path to where this tree keeps it.
def applied(text, addition, mod, consts, place=None):
    place = place or {}
    existing = {number_of(found.group(1), consts): found for found in PROGRAM_LINE.finditer(text)}
    for program, wrap in addition.get("wrap", {}).items():
        key = number_of(program, consts)
        calls = ["    %s();" % name for name in wrap.get("before", [])]
        wrapper = "mod_%s_%s" % (identifier(mod), identifier(program))
        if key in existing:
            original = existing[key].group(2)
            line = existing[key]
            text = text[:line.start()] + "PROGRAM(%s, %s);" % (line.group(1), wrapper) + text[line.end():]
            calls.append("    %s();" % original)
        else:
            text = insert_programs(text, ["PROGRAM(%s, %s);" % (program, wrapper)])
        calls += ["    %s();" % name for name in wrap.get("after", [])]
        text = append_code(text, "// Added by mod %s: runs around program %s.\nvoid %s(void)\n{\n%s\n}\n"
                           % (mod, program, wrapper, "\n".join(calls)))
        existing = {number_of(found.group(1), consts): found for found in PROGRAM_LINE.finditer(text)}
    added = []
    for program, function in addition.get("programs", {}).items():
        key = number_of(program, consts)
        if key in existing:
            raise SystemExit("mod %s adds program %s, which the script already has (as %s); wrap it instead"
                             % (mod, program, existing[key].group(1)))
        added.append("PROGRAM(%s, %s);" % (program, function))
    if added:
        text = insert_programs(text, added)
    for path in addition.get("includes", []):
        line = '#include "%s"' % place.get(path, path)
        if line not in text.split("\n"):
            text = append_code(text, line)
    return text
