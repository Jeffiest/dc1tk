import json
import re
from pathlib import Path

from c2s import flags, index

# Numbers a mod asks for by name instead of choosing them: NEW_FLAG(name) for a save flag,
# NEW_PROGRAM(name) for a map program number and NEW_MONSTER_VAR(name) for a monster variable every
# monster on a floor shares. build gives each name a number that nothing on the disc, in the engine
# or in the chosen mods' own code uses, so mods built together never share one.
#
# The numbers are kept in LOCK in the mods folder, so a name keeps its number from build to build:
# a flag is saved with the game, and a save made with one set of mods must mean the same after
# another mod is added. A number stays reserved while its mod is left out of a build, so it is never
# handed to another mod's name; delete its line from the file to free it.

LOCK = "numbers.json"

# Where each kind's numbers come from. Flags start above every flag the disc and the engine use
# (they stop at 824) with room to spare; 2047 is build --start-at's. Program numbers are the ones
# cli.py map offers mods. Monster variables 8 to 135 are shared by every monster on a floor.
POOLS = {
    "flag": range(1024, flags.FLAG_COUNT - 1),
    "program": range(9000, 10000),
    "monster_var": range(8, 136),
}
WORDS = {"flag": "save flag", "program": "program", "monster_var": "monster variable"}
DECLARATION = re.compile(r"^[ \t]*NEW_(FLAG|PROGRAM|MONSTER_VAR)\(\s*([A-Za-z_]\w*)\s*\)\s*;", re.M)

# Calls whose first argument is a number of the kind: what a mod or the tree may have written by hand.
CALLS = {
    "flag": tuple(flags.FLAG_FUNCTIONS),
    "program": ("run_event_script", "queue_dungeon_event", "load_event_script"),
    "monster_var": ("set_monster_var", "get_monster_var", "set_monster_var2", "get_monster_var2"),
}
LITERAL_PROGRAM = re.compile(r"^PROGRAM\(\s*(\d+)\s*,", re.M)


def literal_calls(text, names):
    pattern = re.compile(r"\b(?:%s)\s*\(\s*(?:\"[^\"]*\"\s*,\s*)?(\d+)\s*[,)]" % "|".join(re.escape(name) for name in names))
    return {int(number) for number in pattern.findall(index.without_comments(text))}


# The names a mod's files ask numbers for: {kind: {name}}.
def requests(texts):
    found = {}
    for text in texts:
        for kind, name in DECLARATION.findall(index.without_comments(text)):
            found.setdefault(kind.lower(), set()).add(name)
    return found


# Numbers written by hand in texts (mods' files): {kind: {number}}.
def written(texts):
    found = {kind: set() for kind in POOLS}
    for text in texts:
        for kind, names in CALLS.items():
            found[kind] |= literal_calls(text, names)
        found["program"] |= {int(number) for number in LITERAL_PROGRAM.findall(text)}
    return found


# Numbers the tree, the engine and these tools use: {kind: {number}}.
def used_by_tree(tree, consts, kinds):
    found = {kind: set() for kind in POOLS}
    if "flag" in kinds:
        found["flag"] = flags.used_numbers(flags.scan(tree, consts))
    if "program" in kinds:
        for entry in index.outlines(tree).values():
            for program in entry["programs"]:
                number = index.value(program["number"], consts)
                if isinstance(number, int):
                    found["program"].add(number)
    if "monster_var" in kinds:
        for call in index.calls(tree, CALLS["monster_var"]):
            number = index.value(call["args"][0], consts) if call["args"] else None
            if isinstance(number, int):
                found["monster_var"].add(number)
    return found


def read_lock(path):
    path = Path(path)
    found = json.loads(path.read_text(encoding="utf-8")) if path.is_file() else {}
    return {kind: dict(found.get(kind, {})) for kind in POOLS}


def write_lock(path, lock):
    note = ("Numbers build gave mods' NEW_FLAG, NEW_PROGRAM and NEW_MONSTER_VAR names, kept so a name keeps "
            "its number and saves stay valid. Delete a line to free its number.")
    body = dict({kind: dict(sorted(lock[kind].items())) for kind in POOLS if lock[kind]}, note=note)
    Path(path).write_text(json.dumps(body, indent=1) + "\n", encoding="utf-8")


# Gives every (mod, kind, name) asked for a number: the one the lock holds, or the lowest free one,
# which is added to the lock. used holds {kind: {number}} nothing may be given. Returns
# ({mod: {name: number}}, whether the lock changed).
def assign(wanted, lock, used):
    result = {}
    changed = False
    for mod, kinds in sorted(wanted.items()):
        for kind, names in sorted(kinds.items()):
            held = lock[kind]
            for name in sorted(names):
                key = "%s/%s" % (mod, name)
                number = held.get(key)
                if number is not None and number in used[kind]:
                    raise SystemExit("%s %s of mod %s has number %d in %s, which something else now uses; delete "
                                     "its line to give it another (a save that set it will then mean nothing)"
                                     % (WORDS[kind], name, mod, number, LOCK))
                if number is None:
                    taken = used[kind] | set(held.values())
                    number = next((candidate for candidate in POOLS[kind] if candidate not in taken), None)
                    if number is None:
                        raise SystemExit("no %s is left for %s of mod %s; %s holds every free one"
                                         % (WORDS[kind], name, mod, LOCK))
                    held[key] = number
                    changed = True
                result.setdefault(mod, {})[name] = number
    return result, changed
