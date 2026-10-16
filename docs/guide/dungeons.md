# Editing dungeon floors

This guide explains how to edit the randomly generated dungeon floors: their room layouts (`data:layouts`), the parts table and minimap tiles (`data:parts`), treasure chest contents (`data:chests`), and the 3D models, collision meshes, and trigger points of individual dungeon parts in Blender.

For details on how the game engine generates a dungeon floor from these files, see [the game's dungeons](../internals/dungeons.md) (section numbers on this page refer to that document). For general map editing, see [editing maps](maps.md). For floor monster tables (`data:floors`) and floor medals (`data:medals`), see [data](data.md).

## Layouts (`data:layouts`)

To export and edit the dungeon room layout tables:

1. Export `data:layouts`:
   ```sh
   dc2tk mod asset ISO NAME data:layouts
   ```
   This command writes `data/layouts.json`, which contains every room script from the table in section 1 along with its `grid_size` and `rooms`.
2. Edit `data/layouts.json` to modify existing rooms or add new rooms.
3. Run `dc2tk build`.

In `data/layouts.json`, parts are written by name:

```json
{"layouts": [
  {"layout": "d01f01", "grid_size": [320.0, 320.0], "rooms": [
    {"room": 0, "id": 0, "rate": 100, "fixed": 0,
     "cells": ["room08 room00 room09", "room03 room24 room01", "room11 room02 room10"]}
  ]}
]}
```

### Cell syntax and partial edits

- **Cell strings**: Each element of `"cells"` is a row of space-separated cell tokens:
  - A part's name (such as `room08` or `way01`)
  - `.` for an empty cell
  - `#N` for an unnamed part at table index `N`
  - An optional `:attr` suffix when the cell's attribute is not `0` (for example, `way01:1` or `.:3`)
- **Keeping only what changes**: Your mod only needs to include the layouts, rooms, and fields that it changes. Identify each room by `"room"` (its 0-based index in the file). When you edit `"cells"`, provide the entire `"cells"` list for that room (its rows define the room's dimensions):
  ```json
  {"layouts": [{"layout": "d01f01", "rooms": [{"room": 5, "rate": 0}]}]}
  ```
- **Adding and disabling rooms**: A room numbered immediately after the last room in the file is appended as a new room (up to 64 rooms total; `"cells"` is required, while `"rate"` defaults to `100`, `"fixed"` defaults to `0`, and `"id"` defaults to the room number). Rooms cannot be deleted; set `"fixed": 1` (or `"rate": 0`) to keep a room out of random floor generation.
- **How `dc2tk build` writes layouts**: `dc2tk build` rewrites each modified room in the original file's layout (one statement per line, keeping the file's line endings, and keeping the file's blank-line gap before a new room) while leaving all other rooms byte-identical. A modified `grid_size` is written as `GRID_SIZE 400.000000, 400.000000;` in place, or inserted after `VER` in a file that lacks one (`d02f01`). Layout changes from multiple mods merge automatically, and `dc2tk build` fails if two mods give the same field different values.
- **Refused (build errors)**:
  - Part names not in the parts table, or `#N` past its 277 entries
  - Cell attributes outside `-32768` to `32767`
  - Rows of uneven width, or more than 900 cells in a room
  - Room numbers past `63`, duplicate room numbers, or a new room without `"cells"` or after a gap (`room 2 is missing`)
  - Unknown fields on a layout or room, or fields of the wrong JSON type (such as `"rooms"` being an object instead of a list)
  - On read: an `RD` line whose count does not match the room's width, or an unknown script tag (none of the disc's files have either)

### Floor generation constraints

- A random floor fits rooms into a `14 × 14` grid (`20 × 16` with flag `0x08`) with an empty border margin, so very large rooms may be chosen by `"rate"` and then fail to fit.
- Edge cells of a room must have entrance variants (see section 3, step 5 of [the game's dungeons](../internals/dungeons.md)).
- Preset floors (`d03`, `mstage`) select layouts by explicit room number, so adding new rooms to those files changes nothing unless a floor selects them.

## The parts table (`data:parts`)

The parts table and the minimap tile arrays live in the main executable (`SCES_511.90`). What can be changed there—and why the table cannot grow past 277 entries—is explained in section 2.1 of [the game's dungeons](../internals/dungeons.md).

Run `dc2tk mod asset ISO NAME data:parts` to write `data/parts.json`:

```json
{"parts": [{"part": 80, "name": "door00", "kind": 10, "link": 15, "entrance": 1,
            "unread_tiles": [42, 42, 40, 42, 42, 42, 42, 42]}, ...],
 "minimap": [{"map": "d01f01", "tiles": [0, 1, 2, ...]}, ...]}
```

- **Partial edits**: Keep only the parts and fields you change:
  ```json
  {"parts": [{"part": 276, "name": "newp00", "kind": 4, "link": 15}]}
  ```
  When editing a map's `"tiles"` in `"minimap"`, provide all 320 tile values.
- **Protected part names**: Renaming `way32`–`51` or `room56`–`59` is refused because the engine looks up those parts by name.
- **Build order**: `dc2tk build` patches the executable in place along with the other executable tables, and applies `data:parts` before `data:layouts` so a layout in the same build can use a newly renamed part.
- **Creating a new kind of part**:
  1. Repurpose an entry in `data:parts` (entry `276` is unused on the disc, or you can rename a part that only room layouts place).
  2. Add a template with that name to the floor's map.
  3. Set its minimap tile in `"minimap"`.
  4. Reference the new name in `data:layouts`. (If you rename an existing part, its old template remains in the map files unplaced.)

## What chests hold (`data:chests`)

Run `dc2tk mod asset ISO NAME data:chests` to write `data/chests.json`. Dungeons are numbered from `0` to `7` to match the other floor tables (`0` is `tbox_d01.cfg`; the chest script format is described in section 4.1 of [the game's dungeons](../internals/dungeons.md)):

```json
{"chests": [{"dungeon": 0,
  "groups": [{"group": 12, "items": [{"item": 1, "rank": 15, "count": 1}, ...]}, ...],
  "floors": [{"floor": 1, "groups": [12, 13, 44]}, ...]}]}
```

- **Partial edits and additions**: Keep only the dungeons, groups, and floors that you change. Provide a changed group's `"items"` list and a changed floor's `"groups"` list in full. Any group or floor number that the file lacks is appended after the last entry of its kind.
- **How `dc2tk build` writes chest files**: Rewrites modified blocks in the original file layout (five items per line, updating the header counts automatically) and leaves unchanged blocks untouched.
- **Limits and validation**:
  - Dungeons are `0` to `7`.
  - A file holds at most `64` groups, and each group holds `1` to `96` items.
  - Floors are `0` to `127`, and each floor draws from `1` to `64` groups.
  - `dc2tk build` refuses a floor that names a missing or empty group, or two mods that give the same group or floor different contents.
  - On read, `dc2tk` also refuses a chest script whose `GROUP` count does not match the items it lists, or an unrecognized tag.

## The parts in Blender

Because a dungeon floor's map (`d01f01`) has no static placements, running `dc2tk map-export d01f01` lays out each template once (the first of each name) on the ground in rows grouped by family (`way`, `room`, `door`, `part`, `step`, then objects), spaced 480 units apart, as a placement node marked `"preview": true` with no `slot`.

You can edit each part's pieces (drawn model, `-a` floor and wall collision, `-c` camera collision) and trigger points (`point` positions, `kaifuku` healing springs, and door `key` events) in Blender using [the add-on](../../blender-plugin/README.md) just like on any regular map:

- Move, add, and delete pieces and points
- Edit meshes and textures, or add new models
- Paint collision attributes (use area `5` where monsters, chests, and objects spawn; see section 4)
- Toggle the cell's closed walls (`MOVE_FLAG`, stored as the preview node's `move_flag`, which the add-on draws around each part and toggles side by side)

Exporting the scene into a mod writes your changes back into the templates.

- **Previewing room layouts**: In the Blender add-on, use **Preview Dungeon Layout** (reading from `extracted/data/layouts.json`) to assemble a room out of part copies beside the map. These preview rooms are never exported. Moving a `"preview": true` node itself changes nothing.
- **Cell alignment**: Keep each part within its `320 × 320` cell around its origin (`400 × 400` in `d05` and `d06`), with its open sides matching its `link` value, because the floor generator joins parts edge-to-edge without checking their meshes. An unedited export changes nothing.

## Limits

- The parts table (`data:parts`) cannot grow past 277 entries (section 2.1); entries can only be repurposed.
- `dmap<n>.cfg`, which defines floor connections and progression (section 3), is not yet exposed as a `data:` table.
- Adding a template with a brand-new name to a floor's map from Blender has not been tested yet.
- None of these dungeon floor edits has been tested in the game yet.
