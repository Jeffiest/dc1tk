# Editing maps

How to edit a map: export it as a glTF scene, edit it in Blender, build it
into a mod, and make new maps with a door into them and back. What the
game's map files hold, and what the engine needs of a new map, is in [the
game's maps](../game/maps.md); a dungeon floor's parts and layouts are in
[editing dungeons](dungeons.md). The Blender add-on is described in
[its own README](../../blender-plugin/README.md); the `mod.json` lines are
explained in [mods](mods.md) and the commands in [commands](commands.md).

## Maps in Blender

`dc2tk map-export s17` writes the map as one glTF scene
(`extracted/maps/s17/`): placements, pieces with their meshes, trigger
points, lights, cameras, collision and textures. The
[dc2tk_maps add-on](../../blender-plugin/README.md) imports it as the game
draws it; move and add placements, doors, chests, spawn points and lights,
paint collision, edit meshes and textures, model new objects, and export into
a mod, which gets the line `"map:s17": "maps/s17/s17.gltf"` under `replace`.
`build` writes the `.map` again, rebuilds the models whose meshes changed,
adds new models to the map's model pack and pictures to its textures.
Unedited scenes change nothing. A dungeon floor's map (`dc2tk map-export
d01f01`) shows each of its parts once, to edit their models, collision and
points; the floor generator places them ([editing dungeons](dungeons.md)).

The example [spinning-cult](../coding/examples/spinning-cult/README.md) edits
Palm Brinks (`m01`) and builds a new map from `m07` in Blender, with a door
between them.

## Exporting a map

`dc2tk map-export NAME [-o DIR]` writes `DIR/NAME.gltf` (by default
`extracted/maps/NAME/`), a buffer per model (`models/<model>.bin`) and the
pictures, shared by every model (`textures/<picture>.png`); `--all` exports
every fixed scene, and `--list` lists every map there is to export, with its
title. The glTF scene is named after the map. A mod carries an edited scene
with a line `"map:NAME": "maps/NAME/NAME.gltf"` in `mod.json` (the add-on's
export writes it; by hand it is the same line); `build` compares it with the
disc's map and writes only what changed. (`mod make` does not pick up edited
map exports: they are outside the asset tree it compares.)

A map's event scripts (`s17*.stb`, one per language) are decompiled to C in
the source tree; [editing scripts](scripts.md) says how to change them.

## The glTF scene

The scene follows conventions shared by dc2tk's exporter, its importer and
the Blender add-on. What follows is what a person editing a scene, in
Blender or as JSON, has to respect.

**Axes and units.** glTF's Y up is the game's; Blender's importer turns it
into Z up and its exporter back, so nothing else converts. One unit is one
game unit (a dungeon cell is 320). The game's rotations are Euler XYZ in
radians (T·Rz·Ry·Rx·S); glTF stores quaternions, so the importer keeps the
original numbers of any node whose matrix did not change and writes new Euler
angles only for nodes that moved.

**Nodes.** Every node the export writes carries `extras.c2` with its role;
names are prefixed so a tree is readable without the add-on:

| node | name | `extras.c2` |
|---|---|---|
| the scene root | `map:s17` | `{"role": "map", "map": <the .map tree without templates, placements and points: images, packs, light sets, settings, cameras>}` |
| one group per placement | `place:NAME` | `{"role": "placement", "template", "show", "group", "far_clip", "slot"}`; its transform is PARTS_POS/ROT/SCALE |
| one per piece of the placement's template | `piece:NAME` | `{"role": "piece", "model": "x.mds", "kind": "model" or "collision" or "camera" or "chara", "show", "time", "col_type", "materials"}`; its transform is PIECE_POS/ROT/SCALE, its children the model's MDS nodes |
| an MDS node | its full name (`grid54--f2appz0n1t-5_1`) | `{"role": "mds", "mds_index": n}` (meshes shared by every piece placing the same model) |
| a trigger point | `point:NAME` | `{"role": "point", "kind": "pos" ... "anime", "enable", "flag", "data": {...}}`; transform FUNC_POS; under its placement when the template carries it, under `points` when it is the map's own |
| group nodes | `placements`, `points`, `cameras` | `{"role": "group"}` |
| a fixed camera | `camera:INDEX` | `{"role": "camera", "index"}`, an empty at its `FIX_CAMERA_POS`; its other positions and boxes are in the root's `map` tree |
| what the scene only shows | `sky`, `band:day`, `sky:day`, `background sky:day below` | `{"role": "display", "what": "sky" or "sky band" or "background sky", "band", "model", "spin"}`: the sky the engine draws about the camera (the map's `.sky`, else `map/def.sky`, when `SKY_INFO` turns it on), each time band's sky and background sky models, once as drawn and once mirrored below the horizon, as `CMapSky::DrawSky` draws them; the import leaves every node of this role, and its materials' pictures, alone |

**Templates.** glTF has no instancing of node trees, so every placement holds
its own copy of its template's pieces and points; meshes are shared, so editing
a mesh in Blender edits every piece that places it, as in the game. On import,
a piece or point changed the same way in every placement of a template changes
the template; one changed in only some placements gives those placements a
copy of the template of their own (a new template name; scripts never name
templates). New placements are appended (slots are never reordered); a
deleted placement is kept with its show flag 0, so later slots keep their
numbers.

**Template libraries.** A map that places none of its templates (the 28
dungeon floor libraries, the add maps, `g00`) shows each template once (the
first of a name) as a placement node with `"preview": true` and no slot,
laid out on the ground in rows by family (the name before its digits), 480
units apart, with the template's closed sides (`MOVE_FLAG`) as `move_flag`
`[-X, -Z, +X, +Z]`, which the import writes back into the template. Its
pieces and points are edited as any placement's and fold into the template
on import; the preview places nothing, so moving it changes nothing, and the
add-on offers no Duplicate for it. A floor's parts are placed at run time
([editing dungeons](dungeons.md)).

**What the `.cfg` adds.** The root's `extras.c2.cfg` is what the map's `.cfg`
adds, for the eye: `water` surfaces, `occlusion` planes, `draw_off` areas,
`events` (its `FUNC_DATA "event"` points: name, event number, how they go
off, `FUNC_POS`) and `villagers` ([the game's maps](../game/maps.md) section
6). The `.cfg` itself is not rebuilt from the scene; a new map copies its
like map's.

**Collision.** Pieces of kind `collision` (floor and wall, `-a`) and `camera`
(`-c`) hold triangles whose material names give their attributes
(`col_ground0_foot2_area6_mask12`); the add-on hides them by default and
colours them by area and foot sound. The attributes, and why collision
should stay split into several compact nodes, are in [the game's
maps](../game/maps.md) section 4.3.

**Points.** Point lights also get a `KHR_lights_punctual` light whose colour
and range the importer reads back; boxes (`invent` boxes, event sizes) are the
node's scale or `data` fields, which the add-on draws. Every other value is in
`extras.c2`, which Blender shows as custom properties.

### Names, order and fields

Blender renames, reorders and retypes what it imports, so the scene carries
what the importer needs and the importer allows for the rest:

- **Names.** Blender gives repeated names `.001` suffixes (pieces and points
  repeat in every placement, model nodes in every piece) and cuts names at 63
  bytes. Every node's `extras.c2.name` holds its exact name; the importer
  uses it, and for a node without one (added in Blender) the node's name
  without its prefix and `.NNN` suffix.
- **Order.** Blender writes children in name order. Pieces and points carry
  `extras.c2.order` (their index in the template, or in the map's own points);
  the importer sorts by it and appends new ones (no `order`) in name order.
  Placements carry `slot`.
- **Point data** is an object of named fields (Blender cannot store lists
  mixing text and numbers). The names (`blender-plugin/dc2tk_maps/schema.py`):
  - `invent`: `id`, `min[3]`, `max[3]`, `unknown_int`, `unknown_float`, `angle`
  - `event`: `kind`, `event_no`, `program`, `args[3]`, `target`, `flags[3]` (action button, use item, 0x100)
  - `fire`, `flare`: `color[3]`, `draw`, `extra[2]` (when the line has it)
  - `plight`: `power`, `color[3]` (0-511), `mode`, `args[4]`, `flicker`, `depth`, `period`
  - `effect`: `effect`
  - `sound`: `number`, `near`, `far`, `unknown_int`, `shape`, `start[3]`, `end[3]`
  - `anime`: `placement`, `piece`, `frame`, `drives`, `mode`, `start[3]`, `step[3]`, `end[3]`, `uniform` and `relative` (when the line has them; one alone writes the other as 0). Scenes exported before the animated pieces were worked out named them `kind`, `unknown_int`, `v0`, `v1`, `v2` and `extra[2]`; the importer and the add-on read those too, and the add-on renames them all when one is edited
  - `pos`: none
- **Other fields:** a point's `flag` is `[a, b, start_hour, end_hour]`; a
  piece's `time` `[start, end]`; `far_clip` `[distance, fade]` or null;
  `col_type` an int, `[type, param]` when the line has two arguments, or
  null when it has none; `show` and `enable` ints.
- **Point lights:** `data` is what the game gets; the `KHR_lights_punctual`
  light is for display (the add-on keeps it in step), and Blender turns light
  nodes 90° about X on import and back on export.
- **Event boxes:** a door, chest or ladder whose scale is within 0.01 of 1
  gets the engine's default size (doors 15, ladders 25, chests 20) as a
  half-extent; `invent` boxes are `min`/`max` in the point's own space.
- **Cameras:** the fixed cameras are in the root's `map` tree; each also gets
  a `camera:INDEX` empty at its `FIX_CAMERA_POS`, whose position the importer
  reads back.
- **The root's `map` tree** is the `.map` file's own JSON without templates,
  placements and points: `images`, `packs`, `header_order`, `settings`,
  `cameras`. The add-on's settings editor shows any structure.

## A scene built into a mod

A `"map:NAME": "maps/NAME/NAME.gltf"` line under `replace` takes an edited
scene and the map's files as the disc (or an earlier mod) has them, and
`build` writes what changed:

- **The `.map`:** the scene tree is read back and the map rebuilt: a
  template is changed when every placement of it agrees, and a placement
  that differs gets a template of its own (`NAME_2`); deleted placements are
  kept hidden (`show 0`, as scripts address them by slot) and new ones
  appended. The file is replaced only when its bytes differ, and `build`
  says what it made of the placements: each new template and the placements
  that place it, how many were deleted and how many added (`note: map:m01
  (mod x): template house_2 is new, placed by house02`).
- **Meshes:** each model is taken from the first piece placing it and
  rebuilt when its meshes differ from an export by value, so Blender's own
  rewriting of meshes you did not touch (split and reordered vertices,
  recomputed normals, float noise, colours a step off or without alpha) is
  not a change, and a rebuilt mesh keeps the vertex alpha fades Blender
  leaves out. Two meshes of one model standing for one frame (a copy that
  kept its original's `extras.mds_index`, or a second mesh of its name) are
  refused rather than built: the frame would keep the last one's triangles
  alone. A mesh without an index and with a name of its own is a new frame
  of the model, under its root.
- **New models:** a piece naming a model no pack has, with meshes under it,
  is built from them (a root frame named after the model, a frame per mesh,
  vertices where the objects were put) and added to the map's first `.pcp`
  (`NAME_0.pcp`) with an `info.cfg` entry giving its TYPE: from the piece's
  `kind`, else `-a` collision, `-c` camera, else drawn. A pack holds at most
  0x1FF models.
- **Pictures:** each picture a material shows (its `extras.texture`, else the
  image's name, else its file's stem) is compared by pixels with the disc's.
  It counts as changed only when it matches none of the disc's pictures of
  its name: several maps have different pictures under one name, and which
  the game shows depends on what it has loaded (m05's `.map` names only
  `e05.img`, yet it shows `light01` of `map/cmn` and Palm Brinks'
  `e01g01_06`). Export and build search the texture packs in one order,
  whatever the disc's form: the map's own, then `map/cmn`, then the rest by
  path. A changed one goes back into the first `.img` that holds it (other
  maps then show it changed too); one the
  disc lacks goes at the end of the map's own first `.img`, in the format of
  its first picture, its sides powers of two up to 1024. The exporter's
  placeholder for a missing picture is not a picture to add, nor its stand-in
  for a render target (`no_image`) a change, though another map holds a
  picture of that name with pixels (d01f01's `d01b17`).

## A map of a mod's own, and a door into it

A new map is a copy of one of the disc's, edited as a scene (`"add":
{"map:m05x": {"file": ..., "like": "m05"}}`), and it can have a script of
its own: `"script": "m05x.c"` on that line, a whole map script (`PROGRAM`
lines, `MAIN`, its functions) that `build` compiles in every language. Its
`map_start` runs when the map loads, and places the player and fades in. A
way in from the game's maps is a door: a `door` point added to an existing
map's scene, whose program (given by name, `"programs": {"door_park":
"ENTER_PARK"}` on that map's line) a patch of the map's script adds:
`jump_to_map(-1, "m05x", map_start)`. The way back is the same from the new
map: a program naming the town's own arrival program, or one of the mod's
that runs it and places the player at the door. The mod's scripts share
the program numbers by `NEW_PROGRAM` in a header both include (`build` gives
each one from 9000 to 9999, kept in `mods/numbers.json`), and see the
new map's number as `MAP_M05X`.

The example [spinning-cult](../coding/examples/spinning-cult/README.md) does
all of this: the Fish Room (`fishroom`, like `m07`) with a script of its own
(`fishroom.c`), reached through a door added in Palm Brinks (`m01`), and
back through the room's own door.

### The map line

A map line takes `"file"` (the scene) and `"programs"`, and under `add` also
`"like"`, `"script"`, `"title"` (the new map's own title in the map table,
printable ASCII without quotes; the like map's otherwise) and `"compact"`
(`true`: the placements deleted from the scene are left out of the new map
instead of kept hidden in their slots, which only the disc's scripts need).
Any other option, `"only"` among them, is refused: a map line changes the
one map it names.

### A new map

A new map is a copy of a map the disc has, edited as a scene: export the map
(`dc2tk map-export m05`), edit it in Blender, and give it under `add` with
the map it was made from:

```json
"add": {
  "map:m05x": {"file": "maps/m05x/m05.gltf", "like": "m05", "title": "Palm Brinks Park II"}
}
```

The name is lower-case letters, digits and `_`, from a letter, at most 12
long (its files are named after it in the engine's 32-byte file names), and
no map of the disc's. `build` imports the scene against the like map's files
as a `map:` line under `replace` would (with `"compact": true` on the line it
leaves out the placements deleted from the scene rather than hiding them in
their slots, since no script numbers a new map's placements yet; without it
they keep their slots, as on a map of the disc), and writes under the new
name in its own folder (`map/m/m05/m05x.map`, `.mpk`, `.ipk`; the `.cfg`,
`.efp` and `.sky` copied): the rebuilt `.map`, the model pack with the models
that changed and the new ones, the texture pack with the changed and new
pictures (a picture another map's pack holds is changed there, as in any map
edit). It appends to every language's table (`map/map<language>.cfg`) a copy
of the like map's row (type, list, sound set, area, add map) with the new
name, numbered after the disc's maps (189 on the PAL disc), so no map of the
game changes number, and `title` in place of the like map's title when
given. Two mods adding one name are refused, and so is a table whose
`MAP_NAME_NUM` is not its count of rows (a row appended there would not be
read) or that the new rows would take past `MapNameBuff`'s 0x8000 bytes.
`--start-at m05x` (or `"start_at": "m05x"`) counts the new map and enters it
as a town (`go_to_edit_mode`, the way towns are entered).

The save keeps map flags (opened chests and the like) only for maps numbered
below 256; a map numbered past that loads but keeps none, and `build` warns.
A new map counts against the game's memory as its like map's kind
([memory](memory.md)).

The Blender add-on's New Map from This One writes such a line, with a script
and `"compact": true` ([the add-on](../../blender-plugin/README.md)).

### A new map's own script

`"script"` on the new map's line names a C file of the mod, a whole map
script (its `PROGRAM` table, `MAIN`, functions), and `build` compiles it and
adds it as the map's script:

```json
"map:m05x": {"file": "maps/m05x/m05.gltf", "like": "m05", "script": "m05x.c"}
```

The engine looks for `map/m/m05/m05x_1.stb` for English, else
`map/m/m05/m05x.stb` ([the game's maps](../game/maps.md) section 7). The
script is compiled once for each of the PAL languages (1 to 5, `LANGUAGE`
set as for a patch of `s51_1.c` to `s51_5.c`); when they all compile alike
it is one file, `m05x.stb`, which every language falls back to, and otherwise
`m05x_1.stb` to `m05x_5.stb` and `m05x.stb` compiled as Japanese (what the
disc's unnumbered scripts are). Each file is a record of `DATA.HD4`. The
script sees what a patch of its mod sees: the SDK and the tree's headers
(`#include <disc.h>` for the names of motions and the like), `LANGUAGE`,
the mod's `NEW_FLAG`, `NEW_PROGRAM` and `NEW_MONSTER_VAR` numbers (those of
the mods it requires too) and the new maps' numbers (below); its quoted
includes are looked for from its folder up, then in the mod, the mods
folder, the tree and the SDK (`"modkit/text.c"`). `mod check` compiles it as
`build` does.

The engine runs `map_start` (100) when the map loads, unless the way in
names another program; `--start-at m05x:PROGRAM` starts a new game at one of
its programs, a `NEW_PROGRAM` name included. A script's `map_start` places
the player (`place_chara`), points the camera (`reset_camera`, or
`set_camera_pos` and `set_camera_target`) and fades in (`fade_in(30)`)
itself, as the town scripts do.

### A door into it, and a way back

A door is an event point of kind `door` in an existing map's scene, and a
program in that map's script that goes to the new map. How the engine's
door works is in [the game's maps](../game/maps.md) section 5: with a
program above 0 (`data.program`) it runs that program of the map's script,
and the door of a new map calls `jump_to_map(-1, "m05x", map_start); wait;`
there; with program 0 it enters `target` as an interior. A town-sized copy
(m05's 245 placements) is not an interior: give it a program.

Add the point in Blender (Add Point, Door, at the 3D cursor) or as JSON in
the scene (a node under `points` with `extras.c2` `{"role": "point",
"name": "door_park", "kind": "event", "data": {"kind": "door", "event_no":
0, "program": 0, "args": [0, 0, 0], "target": "m05x", "flags": [1, 0, 1]}}`
and the door's translation and rotation; a point taking another's place, a
closed door's (`close_door`), deletes that one by leaving it out). The map:
line gives the program by name, so the scene never holds a number build
picks:

```json
"replace": {"map:m01": {"file": "maps/m01/m01.gltf", "programs": {"door_park": "ENTER_PARK"}}}
```

`"programs"` (under `replace` or `add`) sets each named event point's
program: a number, a `NEW_PROGRAM` name of the chosen mods, or a program the
headers name (`map_start`); `build` says which number each got. The program
itself is a patch of the map's script (`#pragma target map m01`,
`PROGRAM(ENTER_PARK, enter_park);`). Existing points take programs the same
way. The way back from spinning-cult's Fish Room is its door point
`door_out`, given the mod's program `LEAVE_FISH_ROOM`, which calls
`jump_to_map(-1, "m01", BACK_FROM_FISH_ROOM)`. A program named on arrival
replaces the town's `map_start`, so `BACK_FROM_FISH_ROOM` in Palm Brinks'
patch runs the town's own arrival (`play_map_start_arrival()`: music,
sounds) and then places the player at the door. Arriving by `map_start`
instead, a town takes its default spot for a map it does not know (Palm
Brinks: by the start of the street).

The add-on's Door to Another Map makes the door point, the program and the
`"programs"` entry in one go ([the add-on](../../blender-plugin/README.md)).

**Map numbers in scripts.** Every chosen mod's patches and new maps'
scripts see each new map's number as `MAP_<NAME>` (`MAP_M05X`, 189 for the
first on the PAL disc), for `go_to_edit_mode(MAP_M05X, map_start)` and the
like; `jump_to_map` and `go_to_interior` take the name too. `build` and `mod
check` list them (`mod check` without `--disc` numbers them after the PAL
disc's 189 rows).

## Checks and limits

**What the importer refuses:** a template name over 31 characters, more than
320 placements (400 in dungeons), meshes that break the collision rules of
[the game's maps](../game/maps.md) section 4.3, a point of an unknown kind.
`build` also refuses a map line's scene of the wrong shape (a node's
`children`, a mesh's `primitives` or the materials not a list, a child that
is not one of the nodes) with an error naming it.

Placement, piece, node and point names that the map's scripts use must
survive an edit ([the game's maps](../game/maps.md) section 3); placements
are only appended, never reordered, since scripts address them by slot. The
largest map of the disc has 245 placements. `TIME_LIGHT_NUM` must be 8 or
less. Memory (read buffer, loading-screen limit, placements, texture blocks)
is in [memory](memory.md); `build` checks it.

In Blender, the add-on's Validate Map checks these before an export, and
Preview Changes runs `dc2tk build --dry-run` to show what the build would
make of the scene ([the add-on](../../blender-plugin/README.md)).
