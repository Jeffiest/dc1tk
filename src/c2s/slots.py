import re
from pathlib import PurePosixPath

# Character slots a map folder's scripts always fill with the same character, so the slot can be
# named for that folder. The engine fills a few slots itself (enum chara_slot_engine); villagers and
# the rest are loaded by the scripts with _LOAD_CHARA and _LOAD_MOTION (load_chara, and helpers such
# as load_scene_chara and add_chara_motions), whose second argument is the character pack and whose
# fourth is the slot. A slot loaded with different packs in one folder holds different characters
# from scene to scene and keeps its number. See definitions/pal/notes/characters.md.

LOADS = ("_LOAD_CHARA", "_LOAD_MOTION")
ENGINE_SLOTS = {0, 120, 121, 122, 123}
EVENT_PREFIX = re.compile(r"^e\d+[a-z]?_")
# A villager model (p01_20a) several villagers share says nothing about who is in the slot.
VILLAGER_MODEL = re.compile(r"^p\d+_\d+[a-z]$")
PLAYER_PACKS = {"c01": "MAX", "c02": "MONICA"}


def loads(nodes, out):
    for node in nodes:
        if node["kind"] == "native" and node["name"] in LOADS and len(node["args"]) > 3:
            pack, slot = node["args"][1], node["args"][3]
            if pack["kind"] == "string" and slot["kind"] == "int":
                out.append((slot["value"], pack["text"]))
        for key in ("then", "else", "body"):
            if isinstance(node.get(key), list):
                loads(node[key], out)
    return out


def folder(path):
    return PurePosixPath(path).parent.as_posix()


# Adds a program's literal pack loads to {folder: {slot: {pack, ...}}}.
def survey(found, path, program):
    if "asm" in program:
        return
    for function in program["functions"]:
        if function["kind"] == "function":
            for slot, pack in loads(function["body"], []):
                found.setdefault(folder(path), {}).setdefault(slot, set()).add(pack)


def identifier(text):
    return re.sub(r"[^A-Z0-9]+", "_", text.upper()).strip("_")


# A character's name for its pack: the villager the disc's NPC table names for it when there is
# exactly one, Max or Monica for their packs, otherwise the pack's file name without its event
# prefix (e109_starbre is starbre). None for a villager model shared by several villagers.
def character_name(pack, pack_names):
    known = pack_names.get(pack)
    if known and " or " not in known:
        return identifier(known)
    stem = EVENT_PREFIX.sub("", PurePosixPath(pack.replace("\\", "/")).name.split(".", 1)[0])
    if VILLAGER_MODEL.match(stem):
        return None
    head, _, rest = stem.partition("_")
    if head in PLAYER_PACKS:
        return PLAYER_PACKS[head] + ("_" + identifier(rest) if rest and rest != "base" else "")
    return identifier(stem) or None


def folder_code(path):
    parts = PurePosixPath(path).parts
    if parts[:1] == ("map",):
        parts = parts[1:]
    if len(parts) > 1 and len(parts[0]) == 1:
        parts = parts[1:]
    return identifier("_".join(parts))


# {folder: {slot: constant}} for the slots each folder always fills with one pack.
def names(found, pack_names):
    out = {}
    for path, by_slot in sorted(found.items()):
        code = folder_code(path)
        if not code:
            continue
        chosen, used = {}, set()
        for slot, packs in sorted(by_slot.items()):
            # packs of one character from different events (e109_starbre, e110_starbre) agree
            characters = {character_name(pack, pack_names) for pack in packs}
            if len(characters) != 1 or slot in ENGINE_SLOTS:
                continue
            character = characters.pop()
            if character is None:
                continue
            name = "CHARA_%s_%s" % (code, character)
            if name in used:
                name = "%s_%d" % (name, slot)
            used.add(name)
            chosen[slot] = name
        if chosen:
            out[path] = chosen
    return out


# The folders' slot names as enums for disc.h, so the scripts that use them compile.
def header_text(folder_names):
    lines = ["", "// Character slots each map folder's scripts always load with the same character."]
    for path, chosen in sorted(folder_names.items()):
        lines.append("enum folder_slots_%s {" % folder_code(path).lower())
        lines += ["    %s = %d," % (name, slot) for slot, name in sorted(chosen.items())]
        lines.append("};")
    return "\n".join(lines) + "\n"


def with_header(generated, folder_names):
    if generated is None or not folder_names:
        return generated
    position = generated.rindex("#endif")
    return generated[:position].rstrip("\n") + "\n" + header_text(folder_names) + "\n" + generated[position:]


# The context a script in the folder is presented with: its folder's slot names are the kind's.
def in_folder(context, path, folder_names):
    chosen = folder_names.get(folder(path))
    if context is None or not chosen:
        return context
    return dict(context, enums=dict(context["enums"], chara_slot_folder=chosen))
