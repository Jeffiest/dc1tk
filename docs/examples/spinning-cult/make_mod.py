"""Makes the spinning-cult mod's maps and music: the Fish Room of the fish-room example with what
the cult needs, Palm Brinks with the door and a dark light set, and the cult's tune, written into
mods/spinning-cult with this folder's mod.json, scripts and Brother Gill's model:

    c2 map-export m01
    c2 map-export m07
    blender --background --python docs/examples/spinning-cult/make_gill.py
    blender --background --python docs/examples/spinning-cult/make_mod.py
    blender --background --python docs/examples/spinning-cult/make_villagers.py

The Fish Room is built by the fish-room example's make_mod.py (the room, the fish turning, the
door), and then, as one would in Blender with the c2_maps add-on:
- a second fish, awake: the same fish with its pupils grown huge and black, a new model of its own
  (New Model from Selection), hidden (its placement's show 0) until the ending shows it, with a
  Piece animation point in "look at the player" mode, so once shown it turns to follow you;
- an Invent box round the fish: photographing it gives the idea "Endless Turning" (251);
- a second light set, red, for the ending.

Palm Brinks gets the door point door_fish where the closed house door door10 was (as the fish-room
example's) and a ninth light set, dark with a dried-blood fog, that the town goes to once three
have turned.

Nothing of the game's is in this folder: the scenes come from your own disc.
"""

import copy
import importlib.util
import json
import os
import shutil
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(os.path.dirname(HERE)))
MAPS = os.path.join(REPO, "extracted", "maps")
MOD = os.path.join(REPO, "mods", "spinning-cult")
OWN_FILES = ("mod.json", "cult.h", "speech.h", "m01.c", "fishroom.c")
DATA_FILES = ("items.json", "inventions.json")

IDEA_ENDLESS_TURNING = 251
AWAKE_PUPIL = 22.0  # the awake fish's pupils (the turning fish's are 4.5)
TOWN_DARK_LIGHT = 8


def load_fish_room():
    """The fish-room example's make_mod.py, as a module (its main() does not run)."""
    path = os.path.join(REPO, "docs", "examples", "fish-room", "make_mod.py")
    spec = importlib.util.spec_from_file_location("fish_room_make_mod", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def extend_room(fish_room):
    """What the cult adds to the Fish Room, after the fish-room example has built it."""

    def then(root, _fish_placement):
        from c2_maps import c2data

        centre = fish_room.game(0, fish_room.FISH_HEIGHT, 0)
        # the awake fish: hidden until the ending shows it, always turned to the player
        awake = fish_room.new_model(
            [fish_room.make_fish(centre, name="fish_awake", pupil=AWAKE_PUPIL)], "fr_fish_awake", collision=False
        )
        data = c2data.get(awake)
        data["show"] = 0
        c2data.put(awake, data)
        piece = next(child for child in awake.children if c2data.role(child) == "piece")
        fish_room.select_only([piece])
        look = fish_room.add_point("ANIME", "fish_look", centre, in_placement=True)
        data = c2data.get(look)
        data["data"]["mode"] = 6  # look at the player
        data["data"]["start"] = [-25.0, -360.0, 0.0]
        data["data"]["step"] = [0.0, 0.0, 0.0]
        data["data"]["end"] = [25.0, 360.0, 0.0]
        c2data.put(look, data)
        # the photo: a box round the fish, seen from anywhere in the room
        fish_room.select_only([root])
        idea = fish_room.add_point("INVENT", "fish_idea", centre, in_placement=False)
        data = c2data.get(idea)
        data["data"]["id"] = IDEA_ENDLESS_TURNING
        data["data"]["min"] = [-260.0, -140.0, -140.0]
        data["data"]["max"] = [260.0, 140.0, 140.0]
        data["data"]["unknown_float"] = 1600.0
        c2data.put(idea, data)
        # light sets: the dark room fades into black fog; set 1 is the ending's red
        data = c2data.get(root)
        sets = data["map"]["settings"]["light_sets"]
        sets[0]["fog_enable"] = 1
        sets[0]["fog"] = dict(sets[0].get("fog", {}), near=700, far=1800, color=[0, 0, 0])
        red = copy.deepcopy(sets[0])
        red["index"] = 1
        red["bgcolor"] = [24, 0, 0]
        red["bgcolor2"] = [0, 0, 0]
        red["ambient"] = [140, 40, 40]
        red["fog"] = dict(red["fog"], near=150, far=1300, color=[90, 0, 0])
        sets.append(red)
        c2data.put(root, data)

    return then


def darken_town(scene_path):
    """Adds light set 8 to Palm Brinks: night's set, darker, red, with a close fog."""
    with open(scene_path, encoding="utf-8") as file:
        scene = json.load(file)
    root = next(node for node in scene["nodes"] if (node.get("extras", {}).get("c2") or {}).get("role") == "map")
    sets = root["extras"]["c2"]["map"]["settings"]["light_sets"]
    if any(light_set.get("index") == TOWN_DARK_LIGHT for light_set in sets):
        return
    night = next((light_set for light_set in sets if light_set.get("index") == 5), sets[-1])
    dark = copy.deepcopy(night)
    dark["index"] = TOWN_DARK_LIGHT
    dark["bgcolor"] = [10, 0, 0]
    dark["bgcolor2"] = [0, 0, 0]
    dark["ambient"] = [60, 25, 25]
    for light in dark.get("lights", []):
        light["color"] = [min(255, int(c * f)) for c, f in zip(light.get("color", [80, 80, 80]), (0.9, 0.35, 0.35))]
    dark["fog_enable"] = 1
    dark["fog"] = dict(dark.get("fog", {}), near=120, far=900, color=[45, 4, 4])
    sets.append(dark)
    with open(scene_path, "w", encoding="utf-8") as file:
        json.dump(scene, file, ensure_ascii=False)


def compose(folder):
    path = os.path.join(HERE, "compose_turned.py")
    spec = importlib.util.spec_from_file_location("spinning_cult_compose", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    module.write(folder)


def main():
    for name in ("m01", "m07"):
        if not os.path.isfile(os.path.join(MAPS, name, name + ".gltf")):
            raise SystemExit("export the map first: c2 map-export %s" % name)
    gill = os.path.join(HERE, "model", "gill.gltf")
    if not os.path.isfile(gill):
        raise SystemExit("make Brother Gill first: blender --background --python %s" % os.path.join(HERE, "make_gill.py"))
    sys.path.insert(0, os.path.join(REPO, "src", "blender"))
    import addon_utils
    import bpy

    fish_room = load_fish_room()
    for folder in ("maps", "music", "model", "data"):
        if os.path.isdir(os.path.join(MOD, folder)):
            shutil.rmtree(os.path.join(MOD, folder))
    os.makedirs(os.path.join(MOD, "data"), exist_ok=True)
    for name in OWN_FILES:
        target = os.path.join(MOD, name)
        if name == "mod.json" and os.path.isfile(target):
            # keep the lines make_villagers.py added
            with open(target, encoding="utf-8") as file:
                kept = json.load(file)
            with open(os.path.join(HERE, name), encoding="utf-8") as file:
                made = json.load(file)
            for key, value in kept.get("replace", {}).items():
                made.setdefault("replace", {}).setdefault(key, value)
            with open(target, "w", encoding="utf-8") as file:
                json.dump(made, file, ensure_ascii=False, indent=1)
                file.write("\n")
            continue
        shutil.copyfile(os.path.join(HERE, name), target)
    for name in DATA_FILES:
        shutil.copyfile(os.path.join(HERE, "data", name), os.path.join(MOD, "data", name))
    shutil.copytree(os.path.join(HERE, "model"), os.path.join(MOD, "model"))
    shutil.copytree(os.path.join(MAPS, "m01"), os.path.join(MOD, "maps", "m01"))
    fish_room.add_town_door(os.path.join(MOD, "maps", "m01", "m01.gltf"))
    darken_town(os.path.join(MOD, "maps", "m01", "m01.gltf"))
    compose(os.path.join(MOD, "music"))

    bpy.ops.wm.read_factory_settings(use_empty=True)
    addon_utils.enable("c2_maps", default_set=True, persistent=False)
    fish_room.build_room(mod=MOD, then=extend_room(fish_room))
    print("spinning-cult written to %s" % MOD)


if __name__ == "__main__":
    main()
