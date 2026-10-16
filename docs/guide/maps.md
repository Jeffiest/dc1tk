# Editing maps

This guide explains how to export a map as a glTF scene, edit it in Blender, build it into a mod, and create new maps connected to existing maps by doors.

For details on the binary map formats and engine rules, see [the game's maps](../internals/maps.md). For random dungeon floor parts and room layouts, see [editing dungeons](dungeons.md). For the Blender add-on documentation, see [its own README](../../blender-plugin/README.md). For `mod.json` syntax and CLI commands, see [mods](mods.md) and [commands](commands.md).

## Maps in Blender

To edit a map in Blender:

1. Export the map as a glTF scene:
   ```sh
   dc2tk map-export s17
   ```
   This command writes the scene to `extracted/maps/s17/`, including placements, pieces with their meshes, trigger points, lights, cameras, collision meshes, and textures.
2. Import `s17.gltf` into Blender using the [dc2tk_maps add-on](../../blender-plugin/README.md), which displays the scene as the game renders it.
3. Move or add placements, doors, chests, spawn points, and lights; paint collision attributes; edit meshes and textures; or model new objects.
4. Export the scene from Blender into your mod. The add-on writes the scene files and adds `"map:s17": "maps/s17/s17.gltf"` under `"replace"` in `mod.json`.
5. Run `dc2tk build`.

`dc2tk build` updates the `.map` file, rebuilds only the models whose meshes changed, adds any new models to the map's model pack, and adds new or modified textures to its texture pack. Unedited scenes change nothing.

Exporting a dungeon floor map (`dc2tk map-export d01f01`) displays each of its part templates once so you can edit their meshes, collision, and trigger points; the dungeon generator places those parts at runtime (see [editing dungeons](dungeons.md)).

The [spinning-cult](../scripting/examples/spinning-cult/README.md) example edits Palm Brinks (`m01`) and builds a new map from `m07` in Blender, with a door connecting them.

## Exporting a map

Run `dc2tk map-export NAME [-o DIR]` to write `DIR/NAME.gltf` (by default `extracted/maps/NAME/`), one binary buffer per model (`models/<model>.bin`), and all textures in a shared folder (`textures/<picture>.png`):

- Pass `--all` to export every fixed map scene on the disc.
- Pass `--list` to list every exportable map along with its title.

The glTF scene is named after the map. To include an edited map in a mod, add `"map:NAME": "maps/NAME/NAME.gltf"` to `mod.json` (the Blender add-on writes this entry automatically on export). `dc2tk build` compares the glTF scene against the disc's map and rewrites only the files that changed. Note that `dc2tk mod make` does not scan edited map exports because they live outside the standard asset tree.

A map's event scripts (`s17*.stb`, one per language) are decompiled to C in the source tree; see [editing scripts](scripts.md) for how to modify them.

## The glTF scene

The exported glTF scene follows conventions shared by `dc2tk`'s exporter, importer, and the Blender add-on. Whether you edit a scene in Blender or as raw JSON, follow the structure below.

### Axes and units

- **Up axis**: glTF uses Y-up, which matches the game engine. Blender's importer converts Y-up to Z-up on import and back to Y-up on export automatically.
- **Scale**: 1 glTF unit equals 1 game unit (a standard dungeon cell is 320 units across).
- **Rotations**: The game stores rotations as Euler XYZ angles in radians ($T \cdot R_z \cdot R_y \cdot R_x \cdot S$), whereas glTF stores rotations as quaternions. The importer preserves the original Euler numbers of any node whose transform matrix did not change and computes new Euler angles only for nodes that moved.

### Nodes

Every exported node carries an `extras.c2` dictionary defining its `"role"`, and node names use prefixes so the hierarchy is easy to read even without the add-on:

| Node | Name | `extras.c2` |
|---|---|---|
| Scene root | `map:s17` | `{"role": "map", "map": <the .map tree without templates, placements and points: images, packs, light sets, settings, cameras>}` |
| Placement group | `place:NAME` | `{"role": "placement", "template", "show", "group", "far_clip", "slot"}`; its transform is `PARTS_POS`/`ROT`/`SCALE` |
| Piece of a placement's template | `piece:NAME` | `{"role": "piece", "model": "x.mds", "kind": "model" or "collision" or "camera" or "chara", "show", "time", "col_type", "materials"}`; its transform is `PIECE_POS`/`ROT`/`SCALE`, and its children are the model's MDS nodes |
| MDS node | Full node name (`grid54--f2appz0n1t-5_1`) | `{"role": "mds", "mds_index": n}` (meshes are shared across every piece that places the same model) |
| Trigger point | `point:NAME` | `{"role": "point", "kind": "pos" ... "anime", "enable", "flag", "data": {...}}`; transform is `FUNC_POS`; placed under its placement when part of a template, or under `points` when owned by the map |
| Group containers | `placements`, `points`, `cameras` | `{"role": "group"}` |
| Fixed camera | `camera:INDEX` | `{"role": "camera", "index"}`, an empty node at its `FIX_CAMERA_POS`; its other positions and bounding boxes are stored in the root's `map` tree |
| Display-only sky nodes | `sky`, `band:day`, `sky:day`, `background sky:day below` | `{"role": "display", "what": "sky" or "sky band" or "background sky", "band", "model", "spin"}`: displays the sky that the engine draws around the camera (the map's `.sky`, or `map/def.sky` when `SKY_INFO` enables it), including each time band's sky and background sky models, both as drawn and mirrored below the horizon as `CMapSky::DrawSky` renders them. The importer ignores all `"role": "display"` nodes and their textures. |

### Templates and placements

- **Shared meshes and templates**: Because glTF does not instance node sub-trees, each placement node holds its own copy of its template's pieces and points, while sharing the underlying glTF meshes. Editing a mesh in Blender edits every piece that uses that model, just like in the game.
- **Template updates on import**:
  - If you change a piece or point in the exact same way across every placement of a template, `dc2tk` updates the shared template.
  - If you change a piece or point in only some placements, `dc2tk` creates a separate template copy (with a new template name; scripts never reference template names) for those placements.
  - New placements are appended at the end so slot numbers never shift.
  - Deleted placements are kept in the `.map` file with `show = 0` so that later placements keep their original slot numbers.
- **Template libraries**: Maps that place none of their templates directly (the 28 dungeon floor libraries, the Georama `add` maps, and `g00`) display each template once (the first of each name) as a placement node with `"preview": true` and no `slot`. These preview nodes sit on the ground in rows grouped by family (the name prefix before the digits), spaced 480 units apart, with the template's closed walls (`MOVE_FLAG`) stored in `move_flag` (`[-X, -Z, +X, +Z]`). Edits to a preview node's pieces, points, and `move_flag` fold back into the template on import. Moving the preview root node itself changes nothing (and the add-on disables **Duplicate** on preview nodes), because dungeon floor parts are placed at runtime (see [editing dungeons](dungeons.md)).
- **What `.cfg` adds**: The root node's `extras.c2.cfg` holds visual reference data from the map's `.cfg` file: `water` surfaces, `occlusion` planes, `draw_off` areas, `events` (`FUNC_DATA "event"` points with name, event number, trigger mode, and `FUNC_POS`), and `villagers` (see [the game's maps](../internals/maps.md) section 6). `dc2tk build` does not rebuild the `.cfg` file from the glTF scene; a new map copies the `.cfg` file from its `"like"` map.
- **Collision**: Pieces with `"kind": "collision"` (floor and wall collision, `-a`) and `"kind": "camera"` (camera collision, `-c`) hold triangles whose material names encode their collision attributes (`col_ground0_foot2_area6_mask12`). The Blender add-on hides collision pieces by default and colors them by area and footstep sound. For details on collision attributes and why collision meshes should stay split into compact nodes, see [the game's maps](../internals/maps.md) section 4.3.
- **Points**: Point light nodes also carry a `KHR_lights_punctual` light whose color and range the importer reads back. Bounding boxes (`invent` boxes and event trigger sizes) use the node's scale or `data` fields, which the add-on draws in the viewport. All other properties live in `extras.c2`, which Blender displays as Custom Properties.

### Names, order and fields

Because Blender renames duplicate objects, sorts child nodes by name, and restricts custom property types, the glTF scene stores exact metadata in `extras.c2`:

- **Names**: Blender appends `.001` suffixes to repeated names and truncates object names at 63 bytes. Every exported node stores its exact name in `extras.c2.name`. On import, `dc2tk` uses `extras.c2.name`; for new nodes created in Blender (which lack `extras.c2.name`), `dc2tk` uses the Blender node name after stripping its role prefix and any `.NNN` suffix.
- **Order**: Blender exports child nodes sorted alphabetically by name. Exported pieces and points store their original index in `extras.c2.order`. The importer sorts existing nodes by `order` and appends new nodes (without `order`) in name order. Placements are ordered by `slot`.
- **Point data** (`blender-plugin/dc2tk_maps/schema.py`, stored as an object of named fields because Blender cannot store mixed-type lists):
  - `invent`: `id`, `min[3]`, `max[3]`, `unknown_int`, `unknown_float`, `angle`
  - `event`: `kind`, `event_no`, `program`, `args[3]`, `target`, `flags[3]` (action button, use item, `0x100`)
  - `fire`, `flare`: `color[3]`, `draw`, `extra[2]` (when present on the line)
  - `plight`: `power`, `color[3]` (`0`–`511`), `mode`, `args[4]`, `flicker`, `depth`, `period`
  - `effect`: `effect`
  - `sound`: `number`, `near`, `far`, `unknown_int`, `shape`, `start[3]`, `end[3]`
  - `anime`: `placement`, `piece`, `frame`, `drives`, `mode`, `start[3]`, `step[3]`, `end[3]`, plus `uniform` and `relative` (when present; giving one alone writes the other as `0`). Scenes exported by older versions named these fields `kind`, `unknown_int`, `v0`, `v1`, `v2`, and `extra[2]`; both the importer and the add-on still accept those legacy names, and the add-on renames them when edited.
  - `pos`: none
- **Other fields**:
  - Point `flag`: `[a, b, start_hour, end_hour]`
  - Piece `time`: `[start, end]`
  - Placement `far_clip`: `[distance, fade]` or `null`
  - Piece `col_type`: an integer, `[type, param]` when the line has two arguments, or `null` when absent
  - `show` and `enable`: integers
- **Point lights**: `data` holds the engine values, while the `KHR_lights_punctual` light is for viewport display (kept in sync by the add-on). Blender rotates light nodes $90^\circ$ around X on import and back on export.
- **Event boxes**: A door, chest, or ladder whose scale is within `0.01` of `1` gets the engine's default half-extent (`15` for doors, `25` for ladders, `20` for chests). `invent` boxes store `min` and `max` in the point's local space.
- **Cameras**: Fixed cameras live in the root's `map` tree, and each camera also has a `camera:INDEX` empty node at its `FIX_CAMERA_POS` whose position the importer reads back.
- **The root's `map` tree**: Contains the `.map` file's JSON structure excluding templates, placements, and points (`images`, `packs`, `header_order`, `settings`, `cameras`). You can edit these in the add-on's settings editor.

## A scene built into a mod

When you add `"map:NAME": "maps/NAME/NAME.gltf"` under `"replace"` in `mod.json`, `dc2tk build` compares your edited scene against the map's files on the disc (or from an earlier mod) and writes only what changed:

- **The `.map` file**: Reads back the scene tree and rebuilds the `.map` file:
  - Updates a template when every placement of that template agrees.
  - Creates a new template (`NAME_2`) when a placement differs from the others.
  - Keeps deleted placements hidden (`show 0`, because scripts address placements by slot) and appends new placements at the end.
  - Replaces the `.map` file only when its bytes change, and logs placement changes: each new template and the placements that use it, plus how many placements were deleted or added (for example, `note: map:m01 (mod x): template house_2 is new, placed by house02`).
- **Meshes**: Reads each model from the first piece that places it and rebuilds the model only when its meshes differ from the export by value. This ignores Blender's automatic changes to untouched meshes (split and reordered vertices, recomputed normals, float rounding, and vertex colors off by one step or missing alpha) and preserves vertex alpha fades on rebuilt meshes that Blender leaves out. `dc2tk build` refuses two meshes in one model that claim the same frame (a duplicate that kept `extras.mds_index` or a second mesh with the same name), because the frame would keep only the last mesh's triangles. A mesh without `mds_index` and with a unique name becomes a new frame under the model's root frame.
- **New models**: If a piece names a model that no pack has and has child meshes under it, `dc2tk build` compiles a new `.mds` model (a root frame named after the model, one frame per mesh, with vertices where the objects were placed) and adds it to the map's first model pack (`NAME_0.pcp`) with an `info.cfg` entry setting its `TYPE` from the piece's `kind` (`-a` for `collision`, `-c` for `camera`, or drawn otherwise). A `.pcp` pack holds at most `0x1FF` (511) models.
- **Pictures (textures)**: Compares each material's texture image (named by `extras.texture`, the image name, or the file stem) pixel-by-pixel against the disc's textures:
  - A texture counts as changed only when its pixels match **none** of the disc's textures with that name (several maps have different textures under the same name, and which one appears in-game depends on what is loaded—for example, `m05`'s `.map` lists only `e05.img`, yet displays `light01` from `map/cmn` and `e01g01_06` from Palm Brinks).
  - Export and build search texture packs in the same order: first the map's own `.img` packs, then `map/cmn`, then the remaining packs by path.
  - A modified texture goes back into the first `.img` archive that holds it (so other maps using that archive also show the change).
  - A new texture that the disc lacks is appended to the map's own first `.img` archive in the pixel format of its first texture, with power-of-two dimensions up to 1024.
  - Placeholder images generated by the exporter for missing textures or `no_image` render targets are ignored on import, even if another map has a texture of that name with pixels (such as `d01f01`'s `d01b17`).

## A map of a mod's own

You can create a new map as an edited copy of an existing disc map (`"add": {"map:m05x": {"file": ..., "like": "m05"}}`) and give it a C script of its own (`"script": "m05x.c"`).

- When the new map loads, the engine runs its `map_start` program, which places the player and fades in the screen.
- To create an entrance from an existing map, add a `door` event point to the existing map's scene, assign it a symbolic program name in `mod.json` (`"programs": {"door_park": "ENTER_PARK"}`), and write that program in a script patch for the existing map (`jump_to_map(-1, "m05x", map_start)`).
- To create a way back from the new map, add a door in the new map whose program calls the town's arrival program—or a custom program in your town patch that runs the town's arrival setup and places the player at the door.
- The mod's scripts share program numbers by declaring `NEW_PROGRAM` in a shared header (`dc2tk build` assigns numbers from `9000` to `9999` and stores them in `mods/numbers.json`), and scripts reference the new map's ID as `MAP_M05X`.

The [spinning-cult](../scripting/examples/spinning-cult/README.md) example demonstrates all of this: it adds the Fish Room (`fishroom`, `"like": "m07"`) with its own script (`fishroom.c`), entered through a door added to Palm Brinks (`m01`) and exited through the room's own door.

### The map line

A `"map:<name>"` entry accepts `"file"` (the glTF scene path) and `"programs"`. Under `"add"`, it also accepts:

- `"like"`: The existing disc map to copy as a base.
- `"script"`: The C source file for the new map's event script.
- `"title"`: The new map's title in the map table (printable ASCII without quotes; defaults to the `"like"` map's title).
- `"compact"`: When `true`, omits placements deleted from the scene instead of keeping them hidden in their slots (only existing disc scripts rely on original placement slot numbers).

Any other option—including `"only"`—is rejected, because a map entry changes only the single map it names.

## A new map

To create a new map from an existing map on the disc:

1. Export the base map:
   ```sh
   dc2tk map-export m05
   ```
2. Edit the scene in Blender (or click **New Map from This One** in the Blender add-on, which writes the `mod.json` entry, a starter script, and `"compact": true`; see [the add-on](../../blender-plugin/README.md)).
3. Add the new map under `"add"` in `mod.json` with `"like"` naming the base map:
   ```json
   "add": {
     "map:m05x": {"file": "maps/m05x/m05.gltf", "like": "m05", "title": "Palm Brinks Park II"}
   }
   ```

### Naming, files, and map table rules

- **Map name**: Must start with a lowercase letter, contain only lowercase letters, digits, and `_`, be at most 12 characters long (so its filenames fit within the engine's 32-byte limit), and must not match any map on the disc.
- **Files written**: `dc2tk build` imports the scene against the `"like"` map's files just like a `"replace"` entry (with `"compact": true`, deleted placements are omitted rather than kept hidden in their slots; without it, they keep their slots). It writes the new map's files under the new name in the base map's folder (`map/m/m05/m05x.map`, `.mpk`, `.ipk`, plus copies of `.cfg`, `.efp`, and `.sky`): the rebuilt `.map`, the model pack with changed and new models, and the texture pack with changed and new textures (a texture stored in another map's pack is updated there, just like in any map edit).
- **Map table (`map/map<language>.cfg`)**: Appends a copy of the `"like"` map's row (type, list, sound set, area, add map) with the new map name and `"title"` (when given) to every language's map table, numbered after the disc's existing maps (starting at `189` on the PAL disc) so no existing map changes its number.
- **Refused**: Two mods adding the same map name; a map table whose `MAP_NAME_NUM` does not equal its row count (an appended row would not be read); or a map table where the new rows would exceed `MapNameBuff`'s `0x8000` bytes.
- **Testing with `--start-at`**: Passing `--start-at m05x` (or setting `"start_at": "m05x"`) starts a new game directly in the new map as a town (`go_to_edit_mode`).
- **Save flags and memory**: Save files store map flags (such as opened chests) only for maps numbered below `256`; a map numbered `256` or above still loads but saves no flags (`dc2tk build` warns when this happens). A new map is checked against the memory budgets of its `"like"` map's category ([memory](memory.md)).

## A new map's own script

Setting `"script"` on a new map's entry compiles a C file from your mod—a complete map script with its `PROGRAM` table, `MAIN`, and functions—and adds it as the map's script:

```json
"map:m05x": {"file": "maps/m05x/m05.gltf", "like": "m05", "script": "m05x.c"}
```

- **Language variants**: At runtime, the engine looks for `map/m/m05/m05x_1.stb` for English, falling back to `map/m/m05/m05x.stb` ([the game's maps](../internals/maps.md) section 7). `dc2tk build` compiles your script once for each PAL language (`1` to `5`, with `LANGUAGE` defined the same way as for patches of `s51_1.c` to `s51_5.c`). When all five languages compile to identical bytes, `dc2tk build` writes a single `m05x.stb` file that every language falls back to; otherwise, it writes `m05x_1.stb` through `m05x_5.stb` plus `m05x.stb` compiled as Japanese (matching the unnumbered scripts on the disc). Each `.stb` file is stored as a record in `DATA.HD4`.
- **Headers and symbols**: The script sees everything a script patch in your mod sees: the SDK and asset tree headers (`#include <disc.h>` for motion names and constants), `LANGUAGE`, the mod's (and its required mods') `NEW_FLAG`, `NEW_PROGRAM`, and `NEW_MONSTER_VAR` numbers, and the `MAP_<NAME>` constants for new maps. Quoted `#include "..."` directives search upward from the script's folder, then in the mod folder, the `mods/` folder, the source tree, and the SDK (`"modkit/text.c"`). `dc2tk mod check` compiles the script in the same way as `dc2tk build`.
- **Entry program (`map_start`)**: The engine runs `map_start` (`100`) when the map loads, unless the transition call specifies another program (`--start-at m05x:PROGRAM` starts a new game at a specific program, including a `NEW_PROGRAM` name). Your script's `map_start` must place the player (`place_chara`), position the camera (`reset_camera`, or `set_camera_pos` and `set_camera_target`), and fade in the screen (`fade_in(30)`), just like the game's town scripts.

### A door into it, and a way back

A door to another map consists of an `event` point of kind `door` in the map's glTF scene and a script program in that map's script that jumps to the destination map.

As described in [the game's maps](../internals/maps.md) section 5:

- When a door's `data.program` is greater than `0`, activating the door runs that program in the current map's script. To enter a new map, that program calls `jump_to_map(-1, "m05x", map_start); wait;`.
- When `data.program` is `0`, the engine enters `target` directly as an interior sub-map. Do not use `program = 0` for a town-sized map (such as a copy of `m05` with 245 placements); give it a program instead.

To add a door to an existing map and wire its program:

1. Add the door point in Blender (**Add Point** > **Door** at the 3D cursor), or use **Door to Another Map** in the add-on to create the door point, script program, and `"programs"` entry in one step ([the add-on](../../blender-plugin/README.md)).
   In raw glTF JSON, a door is a node under `points` with its translation, rotation, and `extras.c2`:
   ```json
   {"role": "point", "name": "door_park", "kind": "event", "data": {"kind": "door", "event_no": 0, "program": 0, "args": [0, 0, 0], "target": "m05x", "flags": [1, 0, 1]}}
   ```
   (If your new door replaces a dummy closed door `close_door` point, delete that closed door point from the scene.)
2. Assign the program to the door point by name using `"programs"` on the `"map:..."` entry in `mod.json`, so the glTF scene never hardcodes a generated number:
   ```json
   "replace": {"map:m01": {"file": "maps/m01/m01.gltf", "programs": {"door_park": "ENTER_PARK"}}}
   ```
   `"programs"` (supported under both `"replace"` and `"add"`) sets the program for each named event point using a number, a `NEW_PROGRAM` name from the active mods, or a header program name (`map_start`). `dc2tk build` prints the number assigned to each point. Existing points can be assigned programs in the same way.
3. Write the program in a script patch for the map (`#pragma target map m01`, `PROGRAM(ENTER_PARK, enter_park);`).

To create a way back from the new map to an existing town:

- In the spinning-cult example, the Fish Room's exit door point `door_out` is assigned the mod's program `LEAVE_FISH_ROOM`, which calls `jump_to_map(-1, "m01", BACK_FROM_FISH_ROOM)`.
- Naming a custom arrival program in `jump_to_map` replaces the destination town's `map_start`. Therefore, `BACK_FROM_FISH_ROOM` in Palm Brinks' script patch calls `play_map_start_arrival()` (to start the town's music and ambient sounds) and then places the player in front of the door. If you jump to `map_start` instead, a town places the player at its default fallback position for unknown origin maps (in Palm Brinks, near the start of the street).

### Map numbers in scripts

Every active mod's script patches and custom map scripts see each new map's number as `MAP_<NAME>` (`MAP_M05X`, which is `189` for the first new map on the PAL disc) for calls such as `go_to_edit_mode(MAP_M05X, map_start)`. Note that `jump_to_map` and `go_to_interior` also accept the map name string directly. Both `dc2tk build` and `dc2tk mod check` list these constants (`dc2tk mod check` without `--disc` numbers them starting at `189`, after the PAL disc's rows).

## Checks and limits

- **Refused by the importer and `dc2tk build`**:
  - A template name longer than 31 characters
  - More than 320 placements in a regular map (400 in dungeons)
  - Collision meshes that violate the rules in [the game's maps](../internals/maps.md) section 4.3
  - A trigger point of an unknown `kind`
  - A glTF scene with invalid JSON structure (such as a node's `children`, a mesh's `primitives`, or `materials` not being a list, or a child index that does not point to a valid node)
- **Engine constraints**:
  - Placement, piece, node, and point names used by the map's scripts must remain intact ([the game's maps](../internals/maps.md) section 3).
  - Placements are only appended and never reordered because scripts address them by slot index (the largest map on the disc has 245 placements).
  - `TIME_LIGHT_NUM` must be `8` or less.
  - `dc2tk build` checks all map memory budgets (read buffer, loading-screen limit, placements, and VRAM texture blocks; see [memory](memory.md)).
- **Checking in Blender**: Use **Validate Map** in the Blender add-on to check these limits before exporting, and use **Preview Changes** to run `dc2tk build --dry-run` and preview what the build will change ([the add-on](../../blender-plugin/README.md)).
