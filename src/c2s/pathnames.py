import functools
import json
import re
import unicodedata
from pathlib import Path

from c2s import disc, messages, source

# Readable names in a decompiled tree. A folder or script the game names with a code keeps the code
# and gains a name after it: map/s/s51_raybrandt_castle/s51_1.c, dungeon/monster/e33a_golem.c,
# lib/map/s/s51_raybrandt_castle/... The game's own names contain underscores too (s06b_1,
# d01_sfida_0), so a name cannot be stripped off by rule: the tree records, in UNNAMED, the path
# each renamed file would have without names, which is its disc path for a script. Tools that need
# a script's disc path (build, defs learn, xref, build --start-at) read it from there.
#
# Names come from the disc in the definition set's language: map folders from the map table
# (map/map<language>.cfg, by the folder's map code: s51, or i01h01 for map/i/i01/h01), map scripts
# outside a map folder the same way, and monster AI scripts from the monster table (the kind whose
# model is the script, else the lowest kind that runs it). A definition set's paths.json names the
# rest and wins over the disc.

UNNAMED = "unnamed-paths.json"
MAX_NAME = 40


def slug(name):
    plain = unicodedata.normalize("NFKD", name).encode("ascii", "ignore").decode("ascii")
    words = re.findall(r"[A-Za-z0-9]+", plain.replace("'", ""))
    return "_".join(word.lower() for word in words)[:MAX_NAME].rstrip("_")


# The names a disc and definition set give, for naming a tree decompiled from them.
def lookups(root, defs):
    code = defs["text"].get("languages", {}).get(defs["language"])
    found = messages.disc_names(root, code) if code is not None else {}
    maps = {key: name for key, name in found.get("map_id", {}).items() if isinstance(key, str)}
    elf = disc.executable(Path(root), defs)
    layout = defs.get("executable", {}).get("monster_table")
    rows = disc.monster_table(elf, layout) if elf is not None and layout else {}
    return {"maps": maps, "ais": ai_names(rows), "titles": defs.get("paths", {})}


# Each AI script's name: the kind whose model is the script, else the lowest kind that runs it.
def ai_names(rows):
    ais = {}
    for kind, row in sorted(rows.items()):
        script = row["script"] or row["model"]
        if row["name"] and (script not in ais or row["model"] == script and not ais[script][0]):
            ais[script] = (row["model"] == script, row["name"])
    return {script: name for script, (_, name) in ais.items()}


def map_code(parts, index):
    if parts[0] == "map" and 2 <= index <= 3:
        return "".join(parts[2:index + 1])
    return None


def with_name(code, name):
    text = slug(name) if name else ""
    return code + "_" + text if text and text != code.lower() else code


# The named path of a disc script, given as the tree path it has without names (map/s/s51/s51_1.c).
def named_script(path, found):
    parts = path.split("/")
    out = []
    for index, part in enumerate(parts[:-1]):
        name = found["titles"].get("/".join(parts[:index + 1]))
        code = map_code(parts, index)
        if name is None and code is not None:
            name = found["maps"].get(code)
        out.append(with_name(part, name))
    stem = parts[-1][:-len(source.EXTENSION)]
    name = found["titles"].get("/".join(parts[:-1] + [stem + ".stb"]))
    if name is None and parts[0] == "map" and len(parts) <= 3:
        name = found["maps"].get(stem)
    if name is None and parts[:2] == ["dungeon", "monster"] and len(parts) == 3:
        name = found["ais"].get(stem)
    return "/".join(out + [with_name(stem, name) + source.EXTENSION])


# A library's path names its folders the way the scripts it serves are named (lib/map/s/s51/x.c
# becomes lib/map/s/s51_raybrandt_castle/x.c); its file name is the library's own.
def named_library(path, found):
    parts = path.split("/")
    if parts[0] != "lib" or len(parts) < 3:
        return path
    inner = named_script("/".join(parts[1:-1] + ["placeholder" + source.EXTENSION]), found)
    return "/".join(["lib"] + inner.split("/")[:-1] + [parts[-1]])


def with_named_libraries(model, found):
    libraries = {key: dict(library, path=named_library(library["path"], found)) for key, library in model["libraries"].items()}
    return dict(model, libraries=libraries)


def record(tree, renamed):
    path = Path(tree) / UNNAMED
    known = json.loads(path.read_text(encoding="utf-8"))["paths"] if path.is_file() else {}
    known.update({named: unnamed for unnamed, named in renamed.items() if named != unnamed})
    note = "The path each renamed file would have without readable names; for a script, its disc path. Do not edit."
    path.write_text(json.dumps({"note": note, "paths": dict(sorted(known.items()))}, indent=1) + "\n", encoding="utf-8")


@functools.lru_cache(maxsize=None)
def recorded(tree, modified):
    path = Path(tree) / UNNAMED
    return json.loads(path.read_text(encoding="utf-8"))["paths"] if path.is_file() else {}


def mapping(tree):
    path = Path(tree) / UNNAMED
    return recorded(str(Path(tree).resolve()), path.stat().st_mtime_ns if path.is_file() else 0)


# Folder renames the recorded files imply, as {folder without names: named folder}.
def folders(tree):
    found = {}
    for named_path, plain in mapping(tree).items():
        for plain_folder, named_folder in zip(Path(plain).parents, Path(named_path).parents):
            if plain_folder.as_posix() != ".":
                found[plain_folder.as_posix()] = named_folder.as_posix()
    return found


def moved(path, renames):
    parts = path.split("/")
    for cut in range(len(parts) - 1, 0, -1):
        folder = "/".join(parts[:cut])
        if folder in renames:
            return "/".join([renames[folder]] + parts[cut:])
    return path


# The path a tree file would have without names (its disc path, for a script). A file the record
# does not list, such as one a mod added, is placed by its folders.
def unnamed(tree, path):
    if path in mapping(tree):
        return mapping(tree)[path]
    return moved(path, {named_folder: plain for plain, named_folder in folders(tree).items()})


# The tree path of a file given its path without names.
def named(tree, path):
    for named_path, plain in mapping(tree).items():
        if plain == path:
            return named_path
    return moved(path, folders(tree))
