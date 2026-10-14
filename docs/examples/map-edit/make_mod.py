"""Makes the map-edit mod: Barga's Valley (s17) edited in Blender with the c2_maps add-on, as you
would by hand, and exported into mods/map-edit with this folder's mod.json and start.c.

    c2 map-export s17
    blender --background --python docs/examples/map-edit/make_mod.py

The edits:
- the valley's grass and cliff picture (s17_02) turned to autumn colours;
- a crate modelled here, with a picture of its own drawn here, made a new model of the map
  with a collision copy (New Model from Selection), standing on the ground 30 units in front
  of the map's "talk" point, where the mod's start program faces the player.

Nothing of the game's is in this folder: the scene comes from your own disc.
"""

import os
import shutil
import sys

import bmesh
import bpy
import numpy
from mathutils import Vector

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(os.path.dirname(HERE)))
SCENE = os.path.join(REPO, "extracted", "maps", "s17", "s17.gltf")
MOD = os.path.join(REPO, "mods", "map-edit")

# Where the crate stands (game coordinates, Y up): in front of the "talk" point.
CRATE_X, CRATE_Z = 68.04, 1210.61
CRATE_SIZE = 24.0


def game_to_blender(x, y, z):
    """The add-on's scene is Y up as the game is; Blender's import makes it Z up."""
    return Vector((x, -z, y))


def ground_height(x, z):
    """The top of the floor collision under (x, z), by a ray down through every collision mesh."""
    from c2_maps import c2data

    root = c2data.current_map_root(bpy.context)
    start = game_to_blender(x, 2000.0, z)
    best = None
    for obj in c2data.collision_meshes(root):
        inverse = obj.matrix_world.inverted()
        origin = inverse @ start
        direction = (inverse.to_3x3() @ Vector((0.0, 0.0, -1.0))).normalized()
        hit, location, _, _ = obj.ray_cast(origin, direction)
        if hit:
            height = (obj.matrix_world @ location).z
            best = height if best is None else max(best, height)
    if best is None:
        raise SystemExit("no collision under the crate's spot")
    return best


def autumn(image):
    """Green turned to orange and brown; the rock stays as it is."""
    pixels = numpy.empty(len(image.pixels), dtype=numpy.float32)
    image.pixels.foreach_get(pixels)
    rgba = pixels.reshape(-1, 4)
    r, g, b = rgba[:, 0].copy(), rgba[:, 1].copy(), rgba[:, 2].copy()
    green = (g > r) & (g > b)
    rgba[green, 0] = numpy.minimum(1.0, g[green] * 1.35)
    rgba[green, 1] = g[green] * 0.75
    rgba[green, 2] = b[green] * 0.5
    image.pixels.foreach_set(pixels)
    image.update()


def plank_picture(name, size=64):
    """Planks of a few browns with dark seams and a dark frame: the crate's own picture."""
    image = bpy.data.images.new(name, size, size, alpha=False)
    rows = []
    for y in range(size):
        for x in range(size):
            plank = y // 16
            tone = (0.55, 0.38, 0.2) if plank % 2 else (0.48, 0.32, 0.16)
            grain = 0.04 * ((x * 7 + y * 3 + plank * 11) % 5) / 4.0
            colour = [c - grain for c in tone]
            if y % 16 == 0 or x < 3 or x >= size - 3 or y < 3 or y >= size - 3:
                colour = [0.22, 0.14, 0.07]
            rows.extend(colour + [1.0])
    image.pixels.foreach_set(numpy.array(rows, dtype=numpy.float32))
    image.update()
    return image


def crate_material(image):
    material = bpy.data.materials.new(image.name)
    if bpy.app.version < (5, 0, 0):
        material.use_nodes = True
    nodes = material.node_tree.nodes
    texture = nodes.new("ShaderNodeTexImage")
    texture.image = image
    material.node_tree.links.new(texture.outputs["Color"], nodes["Principled BSDF"].inputs["Base Color"])
    return material


def main():
    if not os.path.isfile(SCENE):
        raise SystemExit("export the map first: c2 map-export s17 (writes %s)" % SCENE)
    sys.path.insert(0, os.path.join(REPO, "src", "blender"))
    import addon_utils

    bpy.ops.wm.read_factory_settings(use_empty=True)
    addon_utils.enable("c2_maps", default_set=True, persistent=False)
    bpy.ops.c2.import_map_gltf(filepath=SCENE)

    grass = next(image for image in bpy.data.images if image.name.startswith("s17_02"))
    autumn(grass)

    # the crate: a cube with the whole picture on every face, standing on the ground
    ground = ground_height(CRATE_X, CRATE_Z)
    bpy.ops.object.select_all(action="DESELECT")
    centre = game_to_blender(CRATE_X, 0.0, CRATE_Z)
    centre.z = ground + CRATE_SIZE / 2
    bpy.ops.mesh.primitive_cube_add(size=CRATE_SIZE, location=centre)
    crate = bpy.context.active_object
    crate.name = "crate"
    shape = bmesh.new()
    shape.from_mesh(crate.data)
    uv = shape.loops.layers.uv.verify()
    for face in shape.faces:
        for loop, corner in zip(face.loops, ((0, 0), (1, 0), (1, 1), (0, 1))):
            loop[uv].uv = corner
    shape.to_mesh(crate.data)
    shape.free()
    crate.data.materials.append(crate_material(plank_picture("s17_crate")))
    result = bpy.ops.c2.new_model(name="s17_crate", collision=True)
    if "FINISHED" not in result:
        raise SystemExit("New Model from Selection failed")

    os.makedirs(MOD, exist_ok=True)
    for name in ("mod.json", "start.c"):
        shutil.copyfile(os.path.join(HERE, name), os.path.join(MOD, name))
    result = bpy.ops.c2.export_mod(mod_dir=MOD, validate=True)
    if "FINISHED" not in result:
        raise SystemExit("the export failed")
    print("map-edit written to %s; build it with:" % MOD)
    print("  c2 build extracted/scripts YOUR.iso -o map-edit.iso --mod map-edit")


main()
