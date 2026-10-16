# Editing dungeon floors

How to edit the random dungeon floors: their room layouts (`data:layouts`),
the parts table (`data:parts`), what the chests hold (`data:chests`), and
the parts' models, collision and points in Blender. How a floor is built
from these is in [the game's dungeons](../game/dungeons.md) (section numbers
below are that page's); editing a map in general is in [editing
maps](maps.md). Monsters per floor and floor medals are `data:floors` and
`data:medals` ([data](data.md)).

## Layouts (`data:layouts`)

`dc2tk mod asset ISO NAME data:layouts` writes `data/layouts.json`: every room
script of the table in section 1, with its grid size and layouts. The JSON
writes the parts by their names:

```json
{"layouts": [
  {"layout": "d01f01", "grid_size": [320.0, 320.0], "rooms": [
    {"room": 0, "id": 0, "rate": 100, "fixed": 0,
     "cells": ["room08 room00 room09", "room03 room24 room01", "room11 room02 room10"]}
  ]}
]}
```

A row is a string of cells: a part's name, `.` for none, `#N` for a part
with no name, and `:attr` after it when the attribute is not 0 (`way01:1`,
`.:3`). A mod keeps only the layouts, rooms and fields it changes, each room
by `room` (its place in the file, from 0), `cells` whole (its rows give the
size): `{"layouts": [{"layout": "d01f01", "rooms": [{"room": 5, "rate": 0}]}]}`.
A room after the last is added (with `cells`; `rate` 100, `fixed` 0 and `id`
its number unless given), up to 64; rooms are never removed (give one `fixed`
1 to keep it out of random floors). `build` writes each changed room again
in the files' layout (one statement a line, with the file's own line break,
and the file's gap between rooms before a new one), the rest keep their
bytes. A changed `grid_size` is written `GRID_SIZE 400.000000, 400.000000;`
in place, or after `VER` in a file without one (d02f01). Several mods'
layouts merge; two giving one field different values are refused. Names not
in the table, `#N` past its 277 entries, attributes outside -32768 to 32767,
uneven rows, more than 900 cells, rooms past 63, a room given twice, a new
room without `cells` or after a gap (`room 2 is missing`), fields a layout or
room does not have and members of the wrong shape (`"rooms"` an object, not a
list) are refused. Reading, an `RD` line of another count than the room's
width and a tag the room script has no place for are refused too (the disc's
have neither).

Keep in mind: a random floor fits rooms into a 14 × 14 grid (20 × 16 with
0x08) with a free margin, so a large room is chosen and then rarely fits;
its edge cells need entrance versions (section 3, step 5); a preset floor
(d03, mstage) is used by its number, so adding layouts there changes nothing
until a floor picks them.

## The parts table (`data:parts`)

The parts table and the mini maps' tiles are in the executable; what can be
changed there, and why the table cannot grow, is in section 2.1.

`dc2tk mod asset ISO NAME data:parts` writes `data/parts.json`:

```json
{"parts": [{"part": 80, "name": "door00", "kind": 10, "link": 15, "entrance": 1,
            "unread_tiles": [42, 42, 40, 42, 42, 42, 42, 42]}, ...],
 "minimap": [{"map": "d01f01", "tiles": [0, 1, 2, ...]}, ...]}
```

A mod keeps only what it changes (`{"parts": [{"part": 276, "name":
"newp00", "kind": 4, "link": 15}]}`; a map's `tiles` are given whole, 320).
The names of `way32`-`51` and `room56`-`59` are refused (the engine finds
them by name). `build` patches the executable in place, with the other
executable tables. `data:parts` is applied before `data:layouts`, so a
layout of the same build can use a new name.

A new kind of part is an entry repurposed with `data:parts` (entry 276 is
free, and a part only layouts place can be renamed), a template of that name
in the floor's map, its mini map tile set in `minimap`, and layouts that
name it. A renamed part's old template stays in the maps, unplaced.

## What chests hold (`data:chests`)

`dc2tk mod asset ISO NAME data:chests` writes `data/chests.json`, the dungeon
from 0 as the other floor tables number them (0 is `tbox_d01.cfg`; the
chest scripts' format is section 4.1):

```json
{"chests": [{"dungeon": 0,
  "groups": [{"group": 12, "items": [{"item": 1, "rank": 15, "count": 1}, ...]}, ...],
  "floors": [{"floor": 1, "groups": [12, 13, 44]}, ...]}]}
```

A mod keeps the dungeons, groups and floors it changes, a group's `items`
and a floor's `groups` whole; a group or floor the file lacks is added (after
the last of its kind). `build` writes the changed blocks again in the
files' layout (five items a line, the start lines' counts kept true) and
leaves the rest as they were; it refuses a floor naming a group the file
does not have (or one left without items), and two mods giving one group or
floor different contents. Dungeons are 0 to 7, a group gives 1 to 96 items,
a floor (0 to 127) draws from 1 to 64 groups, and a file holds at most 64
groups. A chest script whose `GROUP` counts other than the items it lists is
refused when read (the disc's agree), as is a tag a chest script has no place
for.

## The parts in Blender

A floor's map places none of its templates, so `dc2tk map-export d01f01` shows
each template once (the first of a name), on the ground in rows by family
(`way`, `room`, `door`, `part`, `step`, then the objects), 480 apart: a
placement node marked `"preview": true` with no slot. Its pieces (drawn
model, `-a` floor and wall collision, `-c` camera collision) and points
(`point` positions, `kaifuku`, doors' `key` events) are edited as in any map
([the add-on](../../blender-plugin/README.md)), and the export into a mod
writes them into the template: moving, adding and deleting pieces and
points, editing the meshes, painting collision (area 5 where things spawn,
section 4), new models, and the cell's closed sides (`MOVE_FLAG`, the
preview's `move_flag`, which the add-on draws around each part and toggles
side by side). The add-on also lays a layout's room out from the parts
(Preview Dungeon Layout, from `extracted/data/layouts.json`): copies beside
the map, never exported. A preview itself places nothing, and moving it
changes nothing. Keep a part within its 320 × 320 cell around its origin
(400 in d05, d06), its open sides where its `link` says: the generator joins
parts edge to edge and never looks at their meshes. An unedited export
changes nothing.

## Limits

- The table of parts cannot grow (section 2.1); entries can only be
  repurposed.
- `dmap<n>.cfg`, which gives a floor its ways on (section 3), is not yet a
  `data:` table.
- Giving a floor's map a template of a new name from Blender is not tried
  yet.
- None of these edits has been tried in the game yet.
