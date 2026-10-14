"""Makes the fish-room mod: a door in Palm Brinks (m01) into a new map, The Fish Room, a big black
room with a giant fish spinning in its centre, made in Blender with the c2_maps add-on as you
would by hand, and written into mods/fish-room with this folder's mod.json, fishroom.h, m01.c,
fishroom.c and its tune:

    c2 map-export m01
    c2 map-export m07
    blender --background --python docs/examples/fish-room/make_mod.py

The new map is made from m07 (the opening's Palm Brinks at night, which has no sky): every one of
its placements deleted, the background black and the fog off, and these modelled here, each made a
new model of the map with New Model from Selection:
- the room: a dark floor of tiles and black walls, with floor and wall collision and camera
  collision, so neither the player nor the camera leaves it;
- the fish: a body, a forked tail and fins, with a picture of scales drawn here, and big round eyes
  with tiny pupils staring straight out and a slack open mouth, floating above the floor in the
  middle of the room. A Piece animation point turns it about the vertical, a step a frame,
  for ever;
- a door in the wall behind where the player arrives, with a Door point in front of it, whose
  program (mod.json's "programs") goes back to Palm Brinks.

Palm Brinks gets a door point where the closed house door "door10" was, as in the new-map-door
example (plain JSON, no Blender).

Nothing of the game's is in this folder: the scenes come from your own disc, and the fish, the
room, the door, their pictures and the tune are made here.
"""

import importlib.util
import json
import math
import os
import shutil
import sys

import bmesh
import bpy
import numpy
from mathutils import Vector

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(os.path.dirname(HERE)))
MAPS = os.path.join(REPO, "extracted", "maps")
MOD = os.path.join(REPO, "mods", "fish-room")
OWN_FILES = ("mod.json", "fishroom.h", "m01.c", "fishroom.c")

# The room, in game units (Max is about 26 tall), centred on the origin; keep fishroom.h in step.
ROOM_HALF = 900.0  # the walls stand at x and z = +-ROOM_HALF
WALL_HEIGHT = 500.0
TILE = 100.0  # the floor's tiles, each a quad with the whole tile picture
FISH_HEIGHT = 230.0  # the fish's centre above the floor
SPIN_STEP = 1.5  # degrees a frame
DOOR_WIDTH, DOOR_HEIGHT = 34.0, 52.0

# Palm Brinks' side: the closed door the new one replaces (as new-map-door's).
OLD_DOOR = "door10"
DOOR_AT = (139.869, 6.49539, 142.74)


def game(x, y, z):
    """A game position (Y up) in Blender's scene (Z up): the importer turns Y up into Z up."""
    return Vector((x, -z, y))


UP = Vector((0.0, 0.0, 1.0))


# --- pictures ---------------------------------------------------------------------------------


def picture(name, width, height, colour_at):
    """A picture of the map drawn by colour_at(u, v) -> (r, g, b), u and v from 0 to 1 (v up)."""
    image = bpy.data.images.new(name, width, height, alpha=False)
    pixels = numpy.empty((height, width, 4), dtype=numpy.float32)
    for y in range(height):
        for x in range(width):
            pixels[y, x, :3] = colour_at((x + 0.5) / width, (y + 0.5) / height)
            pixels[y, x, 3] = 1.0
    image.pixels.foreach_set(pixels.ravel())
    image.update()
    return image


def mix(a, b, t):
    return tuple(p + (q - p) * t for p, q in zip(a, b))


def body_colour(u, v):
    """The fish's side, tail (u 0) to head (u 1), belly (v 0) to back (v 1): a blue-green back
    over a pale belly, rows of scales, and a line along the side."""
    back, belly = (0.12, 0.42, 0.55), (0.93, 0.88, 0.70)
    colour = mix(belly, back, min(1.0, max(0.0, (v - 0.3) / 0.5)))
    # scales: overlapping arcs, rows offset by half a scale
    size = 1.0 / 16
    row = int(v / size)
    shift = 0.5 * size if row % 2 else 0.0
    cx = (math.floor((u + shift) / size) + 0.5) * size - shift
    cy = (row + 1.0) * size
    d = math.hypot((u - cx) * 2.0, v - cy) / size
    if 0.62 < d < 0.82:
        colour = tuple(c * 0.72 for c in colour)
    if abs(v - 0.52) < 0.012:
        colour = tuple(c * 0.6 for c in colour)
    return colour


def fin_colour(u, v):
    """Orange fins with darker rays fanning out."""
    ray = (math.atan2(v - 0.5, u + 0.2) * 9.0) % 1.0
    colour = (0.96, 0.52, 0.18)
    return tuple(c * 0.75 for c in colour) if ray < 0.18 else colour


def floor_colour(u, v):
    """A near-black tile with a faint edge, so the floor reads as a floor in the dark."""
    edge = min(u, v, 1.0 - u, 1.0 - v) < 0.035
    return (0.075, 0.075, 0.09) if edge else (0.02, 0.02, 0.025)


def door_colour(u, v):
    """Planks of warm wood, a dark frame round them and a brass knob."""
    plank = int(u * 4)
    colour = (0.50, 0.30, 0.15) if plank % 2 else (0.44, 0.26, 0.12)
    colour = tuple(c - 0.03 * (((plank * 7 + int(v * 40)) % 3) / 2.0) for c in colour)
    if u < 0.06 or u > 0.94 or v < 0.04 or v > 0.96 or abs(u * 4 - round(u * 4)) < 0.03:
        colour = (0.22, 0.12, 0.05)
    if math.hypot((u - 0.8) * 0.5, v - 0.47) < 0.03:
        colour = (0.85, 0.7, 0.25)
    return colour


def flat(rgb):
    return lambda u, v: rgb


def material(image):
    made = bpy.data.materials.new(image.name)
    if bpy.app.version < (5, 0, 0):
        made.use_nodes = True
    nodes = made.node_tree.nodes
    texture = nodes.new("ShaderNodeTexImage")
    texture.image = image
    made.node_tree.links.new(texture.outputs["Color"], nodes["Principled BSDF"].inputs["Base Color"])
    return made


# --- meshes -----------------------------------------------------------------------------------


class Builder:
    """A mesh made of parts: each part's faces get a material and UVs from a function of the
    vertex position (Blender coordinates)."""

    def __init__(self, name):
        self.name = name
        self.shape = bmesh.new()
        self.uv = self.shape.loops.layers.uv.verify()
        self.materials = []

    def slot(self, made):
        if made not in self.materials:
            self.materials.append(made)
        return self.materials.index(made)

    def faces(self, points, faces, made, uv_of):
        """A closed part: its faces are turned to face out."""
        verts = [self.shape.verts.new(point) for point in points]
        index = self.slot(made)
        made_faces = []
        for face in faces:
            new = self.shape.faces.new([verts[i] for i in face])
            new.material_index = index
            for loop in new.loops:
                loop[self.uv].uv = uv_of(loop.vert.co, loop)
            made_faces.append(new)
        bmesh.ops.recalc_face_normals(self.shape, faces=made_faces)

    def quad_uv(self, points, made, facing, corners=((0, 0), (1, 0), (1, 1), (0, 1))):
        """A quad of an open surface, turned to face along facing, with the whole picture."""
        verts = [self.shape.verts.new(point) for point in points]
        new = self.shape.faces.new(verts)
        new.material_index = self.slot(made)
        for loop, corner in zip(new.loops, corners):
            loop[self.uv].uv = corner
        new.normal_update()
        if new.normal.dot(facing) < 0:
            new.normal_flip()

    def object(self, origin):
        """The mesh as an object whose origin is at origin (the vertices are given in world
        coordinates)."""
        mesh = bpy.data.meshes.new(self.name)
        for vert in self.shape.verts:
            vert.co -= origin
        self.shape.to_mesh(mesh)
        self.shape.free()
        for made in self.materials:
            mesh.materials.append(made)
        obj = bpy.data.objects.new(self.name, mesh)
        bpy.context.collection.objects.link(obj)
        obj.location = origin
        return obj


def ellipsoid(builder, centre, radii, made, uv_of, segments=24, rings=14, shape_at=None):
    """A closed ellipsoid along X, from a point at -X to one at +X; shape_at(x) -> (y scale,
    z scale) narrows it along X (the tail)."""
    points = [Vector((centre[0] + radii[0], centre[1], centre[2]))]  # the +X end
    for ring in range(1, rings):
        theta = math.pi * ring / rings
        for segment in range(segments):
            phi = 2.0 * math.pi * segment / segments
            x = radii[0] * math.cos(theta)
            y = radii[1] * math.sin(theta) * math.cos(phi)
            z = radii[2] * math.sin(theta) * math.sin(phi)
            sy, sz = shape_at(x) if shape_at else (1.0, 1.0)
            points.append(Vector((centre[0] + x, centre[1] + y * sy, centre[2] + z * sz)))
    points.append(Vector((centre[0] - radii[0], centre[1], centre[2])))  # the -X end
    last = len(points) - 1

    def at(ring, segment):
        return 1 + (ring - 1) * segments + segment % segments

    faces = [(0, at(1, s), at(1, s + 1)) for s in range(segments)]
    for ring in range(1, rings - 1):
        for s in range(segments):
            faces.append((at(ring, s), at(ring + 1, s), at(ring + 1, s + 1), at(ring, s + 1)))
    faces += [(last, at(rings - 1, s + 1), at(rings - 1, s)) for s in range(segments)]
    builder.faces(points, faces, made, uv_of)


def slab(builder, outline, thickness, made, uv_of, y=0.0):
    """A flat shape of the outline (points in X and Z), thickness thick about Y = y: two faces
    and the edges round it."""
    n = len(outline)
    points = [Vector((x, y - thickness / 2, z)) for x, z in outline]
    points += [Vector((x, y + thickness / 2, z)) for x, z in outline]
    faces = [tuple(range(n)), tuple(range(2 * n - 1, n - 1, -1))]
    faces += [(i, (i + 1) % n, n + (i + 1) % n, n + i) for i in range(n)]
    builder.faces(points, faces, made, uv_of)


def make_fish(centre):
    """The fish, facing +X, its centre at centre (Blender coordinates). About 420 long."""
    body_m = material(picture("fr_body", 128, 64, body_colour))
    fin_m = material(picture("fr_fin", 32, 32, fin_colour))
    white_m = material(picture("fr_eye", 16, 16, flat((0.97, 0.97, 0.94))))
    pupil_m = material(picture("fr_pupil", 16, 16, flat((0.02, 0.02, 0.02))))
    mouth_m = material(picture("fr_mouth", 16, 16, flat((0.25, 0.06, 0.08))))
    length, thick, tall = 210.0, 58.0, 95.0
    fish = Builder("fish")

    def taper(x):
        t = max(0.0, -x / length)
        return (1.0 - 0.7 * t ** 1.6, 1.0 - 0.62 * t ** 1.6)

    def side_uv(co, _loop):
        return ((co.x - centre.x + length) / (2 * length), (co.z - centre.z + tall) / (2 * tall))

    def box_uv(left, right, bottom, top):
        return lambda co, _loop: ((co.x - left) / (right - left), (co.z - bottom) / (top - bottom))

    ellipsoid(fish, centre, (length, thick, tall), body_m, side_uv, shape_at=taper)
    cx, cy, cz = centre
    # the forked tail
    tail = [(-170, 22), (-300, 112), (-262, 0), (-300, -96), (-170, -22)]
    slab(fish, [(cx + x, cz + z) for x, z in tail], 6.0, fin_m, box_uv(cx - 300, cx - 170, cz - 96, cz + 112), cy)
    # the fin along the back, and a small one under the belly
    back = [(-110, 40), (-60, 160), (40, 150), (70, 75)]
    slab(fish, [(cx + x, cz + z) for x, z in back], 5.0, fin_m, box_uv(cx - 110, cx + 70, cz + 40, cz + 160), cy)
    belly = [(-90, -50), (-70, -112), (-20, -100), (0, -70)]
    slab(fish, [(cx + x, cz + z) for x, z in belly], 4.0, fin_m, box_uv(cx - 90, cx, cz - 112, cz - 50), cy)
    # side fins, sticking out behind the gills
    for side in (-1, 1):
        fin = [(75, -10), (20, -45), (5, -20)]
        points = [Vector((cx + x, cy + side * (thick * 0.78 + abs(z) * 0.5), cz + z)) for x, z in fin]
        points += [Vector((p.x, p.y + side * 3, p.z)) for p in points]
        fish.faces(points, [(0, 1, 2), (5, 4, 3), (0, 3, 4, 1), (1, 4, 5, 2), (2, 5, 3, 0)], fin_m,
                   box_uv(cx + 5, cx + 75, cz - 45, cz - 10))
    # the blank eyes: big round whites with tiny pupils dead in their middle, staring straight out
    eye_x, eye_z = 138.0, 28.0
    surface = thick * math.sqrt(max(0.0, 1.0 - (eye_x / length) ** 2 - (eye_z / tall) ** 2))
    for side in (-1, 1):
        white_at = (cx + eye_x, cy + side * (surface - 4.0), cz + eye_z)
        ellipsoid(fish, white_at, (27.0, 11.0, 27.0), white_m, lambda co, _l: (0.5, 0.5), 16, 8)
        pupil_at = (cx + eye_x, cy + side * (surface + 6.0), cz + eye_z)
        ellipsoid(fish, pupil_at, (4.5, 2.0, 4.5), pupil_m, lambda co, _l: (0.5, 0.5), 10, 6)
    # the slack mouth: a dark oval, a little open, at the front of the head
    ellipsoid(fish, (cx + length - 6.0, cy, cz - 18.0), (8.0, 26.0, 9.0), mouth_m, lambda co, _l: (0.5, 0.5), 14, 7)
    return fish.object(Vector(centre))


def make_room():
    """The floor (tiles) and the four walls, facing in."""
    floor_m = material(picture("fr_floor", 64, 64, floor_colour))
    wall_m = material(picture("fr_wall", 16, 16, flat((0.0, 0.0, 0.0))))
    room = Builder("room")
    count = int(2 * ROOM_HALF / TILE)
    for i in range(count):
        for j in range(count):
            x0, z0 = -ROOM_HALF + i * TILE, -ROOM_HALF + j * TILE
            room.quad_uv([game(x0, 0, z0), game(x0, 0, z0 + TILE), game(x0 + TILE, 0, z0 + TILE), game(x0 + TILE, 0, z0)], floor_m, UP)
    h, top = ROOM_HALF, WALL_HEIGHT
    walls = [
        [(-h, -h), (h, -h)],
        [(h, -h), (h, h)],
        [(h, h), (-h, h)],
        [(-h, h), (-h, -h)],
    ]
    pieces = 6
    for (ax, az), (bx, bz) in walls:
        for k in range(pieces):
            s, t = k / pieces, (k + 1) / pieces
            p0 = (ax + (bx - ax) * s, az + (bz - az) * s)
            p1 = (ax + (bx - ax) * t, az + (bz - az) * t)
            inward = game(0, top / 2, 0) - game((p0[0] + p1[0]) / 2, top / 2, (p0[1] + p1[1]) / 2)
            room.quad_uv([game(p0[0], 0, p0[1]), game(p0[0], top, p0[1]), game(p1[0], top, p1[1]), game(p1[0], 0, p1[1])], wall_m, inward)
    return room.object(Vector((0.0, 0.0, 0.0)))


def make_door():
    """A door of planks in a dark frame, set in the wall at +Z, its front facing -Z (the room)."""
    door_m = material(picture("fr_door", 32, 64, door_colour))
    frame_m = material(picture("fr_frame", 16, 16, flat((0.16, 0.09, 0.04))))
    door = Builder("door")
    z = ROOM_HALF - 1.5  # just in front of the wall
    w, top = DOOR_WIDTH / 2, DOOR_HEIGHT
    # seen from the room (looking +Z), +X is on the left: the picture's u runs to -X, v up
    corners = ((0, 0), (0, 1), (1, 1), (1, 0))
    door.quad_uv([game(w, 0, z), game(w, top, z), game(-w, top, z), game(-w, 0, z)], door_m, game(0, 0, -1), corners)
    # the frame: three bars standing out of the wall
    def bar(x0, x1, y0, y1):
        front = z - 4.0
        corners = [game(x0, y0, front), game(x1, y0, front), game(x1, y1, front), game(x0, y1, front)]
        back = [game(x0, y0, z + 1), game(x1, y0, z + 1), game(x1, y1, z + 1), game(x0, y1, z + 1)]
        points = corners + back
        faces = [(0, 1, 2, 3), (1, 5, 6, 2), (5, 4, 7, 6), (4, 0, 3, 7), (3, 2, 6, 7), (4, 5, 1, 0)]
        door.faces(points, faces, frame_m, lambda co, _l: (0.5, 0.5))

    bar(-w - 5, -w, 0, top + 5)
    bar(w, w + 5, 0, top + 5)
    bar(-w - 5, w + 5, top, top + 5)
    return door.object(game(0, top / 2, z))


# --- the scene --------------------------------------------------------------------------------


def clear_map(root):
    """Delete every placement of the map (the importer keeps a deleted one hidden, so scripts'
    slot numbers stay), and make the background black with no fog."""
    from c2_maps import c2data

    gone = set()
    for placement in c2data.placements(root):
        gone.update(obj.name for obj in c2data.descendants(placement))
        gone.add(placement.name)
    for name in gone:
        if name in bpy.data.objects:
            bpy.data.objects.remove(bpy.data.objects[name], do_unlink=True)
    data = c2data.get(root)
    for light_set in data["map"]["settings"]["light_sets"]:
        light_set["bgcolor"] = [0, 0, 0]
        light_set["bgcolor2"] = [0, 0, 0]
        light_set["fog_enable"] = 0
    c2data.put(root, data)


def select_only(objects):
    for obj in bpy.context.selected_objects:
        obj.select_set(False)
    for obj in objects:
        obj.select_set(True)
    bpy.context.view_layer.objects.active = objects[0]


def new_model(objects, name, collision, camera=False):
    select_only(objects)
    result = bpy.ops.c2.new_model(name=name, collision=collision, camera=camera)
    if "FINISHED" not in result:
        raise SystemExit("New Model from Selection failed for %s" % name)
    return bpy.context.active_object


def add_point(preset, name, at, in_placement):
    bpy.context.scene.cursor.location = at
    result = bpy.ops.c2.add_point(preset=preset, name=name, in_placement=in_placement)
    if "FINISHED" not in result:
        raise SystemExit("Add Point failed for %s" % name)
    return bpy.context.active_object


def build_room():
    from c2_maps import c2data

    scene = os.path.join(MAPS, "m07", "m07.gltf")
    bpy.ops.c2.import_map_gltf(filepath=scene)
    root = c2data.current_map_root(bpy.context)
    clear_map(root)
    new_model([make_room()], "fr_room", collision=True, camera=True)
    fish_placement = new_model([make_fish(game(0, FISH_HEIGHT, 0))], "fr_fish", collision=False)
    new_model([make_door()], "fr_door", collision=False)
    # the fish's piece turns: a Piece animation point on it, a step a frame about Y
    piece = next(child for child in fish_placement.children if c2data.role(child) == "piece")
    select_only([piece])
    spin = add_point("ANIME", "fish_spin", game(0, FISH_HEIGHT, 0), in_placement=True)
    data = c2data.get(spin)
    data["data"]["step"] = [0.0, SPIN_STEP, 0.0]
    c2data.put(spin, data)
    # the way out: a door point in front of the door, its program given by mod.json
    select_only([root])
    door = add_point("DOOR", "door_out", game(0, 0, ROOM_HALF - 15.0), in_placement=False)
    data = c2data.get(door)
    data["data"]["target"] = "m01"
    c2data.put(door, data)
    result = bpy.ops.c2.export_mod(mod_dir=MOD, validate=True)
    if "FINISHED" not in result:
        raise SystemExit("the export failed")


# --- Palm Brinks' door (plain JSON, as new-map-door's make_mod.py) ----------------------------


def c2_of(node):
    c2 = node.get("extras", {}).get("c2")
    if isinstance(c2, str):
        c2 = json.loads(c2)
    return c2 if isinstance(c2, dict) else {}


def add_town_door(scene_path):
    with open(scene_path, encoding="utf-8") as file:
        scene = json.load(file)
    nodes = scene["nodes"]
    points = next(node for node in nodes if c2_of(node).get("role") == "group" and node.get("name") == "points")
    old = next((at for at in points.get("children", []) if c2_of(nodes[at]).get("name") == OLD_DOOR), None)
    if old is None:
        raise SystemExit("%s has no point %s" % (scene_path, OLD_DOOR))
    where = nodes[old].get("translation", [0, 0, 0])
    if any(abs(a - b) > 1.0 for a, b in zip(where, DOOR_AT)):
        print("warning: %s is at %s, not where fishroom.h has it (%s)" % (OLD_DOOR, where, DOOR_AT))
    door = {
        "name": "point:door_fish",
        "extras": {
            "c2": {
                "role": "point", "name": "door_fish", "kind": "event", "enable": 1, "flag": [0, 0, 0, 0],
                "data": {"kind": "door", "event_no": 0, "program": 0, "args": [0, 0, 0], "target": "fishroom", "flags": [1, 0, 1]},
            }
        },
    }
    for key in ("translation", "rotation", "scale"):
        if key in nodes[old]:
            door[key] = list(nodes[old][key])
    points["children"] = [at for at in points["children"] if at != old] + [len(nodes)]
    nodes.append(door)
    with open(scene_path, "w", encoding="utf-8") as file:
        json.dump(scene, file, ensure_ascii=False)


def main():
    for name in ("m01", "m07"):
        if not os.path.isfile(os.path.join(MAPS, name, name + ".gltf")):
            raise SystemExit("export the map first: c2 map-export %s" % name)
    sys.path.insert(0, os.path.join(REPO, "src", "blender"))
    import addon_utils

    if os.path.isdir(os.path.join(MOD, "maps")):
        shutil.rmtree(os.path.join(MOD, "maps"))
    os.makedirs(os.path.join(MOD, "music"), exist_ok=True)
    for name in OWN_FILES:
        shutil.copyfile(os.path.join(HERE, name), os.path.join(MOD, name))
    shutil.copytree(os.path.join(MAPS, "m01"), os.path.join(MOD, "maps", "m01"))
    add_town_door(os.path.join(MOD, "maps", "m01", "m01.gltf"))
    # the tune (compose.py, plain Python)
    spec = importlib.util.spec_from_file_location("fish_room_compose", os.path.join(HERE, "compose.py"))
    compose = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(compose)
    compose.write(os.path.join(MOD, "music"))

    bpy.ops.wm.read_factory_settings(use_empty=True)
    addon_utils.enable("c2_maps", default_set=True, persistent=False)
    build_room()
    print("fish-room written to %s; build it with:" % MOD)
    print("  c2 build extracted/scripts YOUR.iso -o fish-room.iso --mod fish-room")


main()
