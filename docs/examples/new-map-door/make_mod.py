"""Makes the new-map-door mod from your own exports of Palm Brinks (m01) and its park (m05), and
writes it into mods/new-map-door with this folder's mod.json, park.h, m01.c and m05x.c:

    dc2tk map-export m01
    dc2tk map-export m05
    python docs/examples/new-map-door/make_mod.py [--maps extracted/maps] [--mods mods]

The edits, as you would make them in Blender with the dc2tk_maps add-on:
- m01: a door point, door_park, where the closed house door "door10" was (Add Point > Door, moved
  onto door10, door10 deleted). Its program is given by mod.json's "programs" (ENTER_PARK), its
  flags are a disc door's (action button, fade out).
- m05x: the park m05 as exported, unedited: the new map is a copy of it with a script of its own.

Nothing of the game's is in this folder: the scenes come from your own disc. Plain Python, no
Blender needed.
"""

import argparse
import json
import os
import shutil

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(os.path.dirname(HERE)))
OWN_FILES = ("mod.json", "park.h", "m01.c", "m05x.c")
# The closed door the new one replaces, and where park.h expects it.
OLD_DOOR = "door10"
DOOR_AT = (139.869, 6.49539, 142.74)


def c2_of(node):
    """A node's extras.c2 as a dict (the add-on keeps it as one; text is read too)."""
    c2 = node.get("extras", {}).get("c2")
    if isinstance(c2, str):
        c2 = json.loads(c2)
    return c2 if isinstance(c2, dict) else {}


def add_door(scene_path):
    with open(scene_path, encoding="utf-8") as file:
        scene = json.load(file)
    nodes = scene["nodes"]
    points = next(
        (node for node in nodes if c2_of(node).get("role") == "group" and node.get("name") == "points"), None
    )
    if points is None:
        raise SystemExit("%s has no points group: export it again with dc2tk map-export m01" % scene_path)
    old = next((at for at in points.get("children", []) if c2_of(nodes[at]).get("name") == OLD_DOOR), None)
    if old is None:
        raise SystemExit("%s has no point %s" % (scene_path, OLD_DOOR))
    where = nodes[old].get("translation", [0, 0, 0])
    if any(abs(a - b) > 1.0 for a, b in zip(where, DOOR_AT)):
        print("warning: %s is at %s, not where park.h has it (%s)" % (OLD_DOOR, where, DOOR_AT))
    door = {
        "name": "point:door_park",
        "extras": {
            "c2": {
                "role": "point",
                "name": "door_park",
                "kind": "event",
                "enable": 1,
                "flag": [0, 0, 0, 0],
                # a door: the program (mod.json's "programs") runs once it has opened and faded out;
                # the target is only a note here
                "data": {
                    "kind": "door",
                    "event_no": 0,
                    "program": 0,
                    "args": [0, 0, 0],
                    "target": "m05x",
                    "flags": [1, 0, 1],
                },
            }
        },
    }
    for key in ("translation", "rotation", "scale"):
        if key in nodes[old]:
            door[key] = list(nodes[old][key])
    # the old door's point is left out of the scene (deleted), the new one added
    points["children"] = [at for at in points["children"] if at != old] + [len(nodes)]
    nodes.append(door)
    with open(scene_path, "w", encoding="utf-8") as file:
        json.dump(scene, file, ensure_ascii=False)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--maps", default=os.path.join(REPO, "extracted", "maps"), help="where dc2tk map-export wrote m01 and m05")
    parser.add_argument("--mods", default=os.path.join(REPO, "mods"), help="the mods folder to write new-map-door into")
    args = parser.parse_args()
    exports = {name: os.path.join(args.maps, name) for name in ("m01", "m05")}
    for name, folder in exports.items():
        if not os.path.isfile(os.path.join(folder, name + ".gltf")):
            raise SystemExit("export the map first: dc2tk map-export %s (writes %s)" % (name, folder))
    mod = os.path.join(args.mods, "new-map-door")
    if os.path.isdir(os.path.join(mod, "maps")):
        shutil.rmtree(os.path.join(mod, "maps"))
    os.makedirs(mod, exist_ok=True)
    shutil.copytree(exports["m01"], os.path.join(mod, "maps", "m01"))
    shutil.copytree(exports["m05"], os.path.join(mod, "maps", "m05x"))
    add_door(os.path.join(mod, "maps", "m01", "m01.gltf"))
    for name in OWN_FILES:
        shutil.copyfile(os.path.join(HERE, name), os.path.join(mod, name))
    print("new-map-door written to %s; build it with:" % mod)
    print("  dc2tk build extracted/scripts YOUR.iso -o new-map-door.iso --mod new-map-door")


main()
