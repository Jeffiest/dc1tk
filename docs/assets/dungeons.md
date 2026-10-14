# Dark Chronicle dungeon floors: how a floor is built

A dungeon floor is not a map on the disc. The floor's map (`d01f01.map` and
the like) is a library of templates, the parts, and the game lays out a grid
of them each time the floor is entered, from a room script of
`dungeon/cfg_file`. This page gives the room script's format, how the parts
are chosen and placed, and how both are edited. Engine source: the
ChronicleTwo decompilation's `ps2/` (`src/automap.cpp`, `include/automap.hpp`;
the functions it leaves as assembly were read from `asm/pal/nonmatchings`,
through m2c). Corpus: the extracted PAL disc. Map files are in
[maps](maps.md), monsters per floor and floor medals in
[data](data.md).

## Status

**Settled (engine code and whole-corpus checks):**
- The room script's format (section 2): `c2-assets`' `dungeon_layout.rs`
  reads all 23 scripts (291 room layouts) with the part names of the
  executable's `PartsInfoData` and writes them back byte for byte; every room
  rewritten from its JSON gives its own bytes (`cargo test --release corpus`
  with `C2S_CORPUS` set).
- How a floor is laid out, from the layouts, and where its parts, chests,
  monsters and stones go (sections 3 and 4).
- `data:layouts` in `mod.json` changes rooms and grid sizes and adds rooms
  (section 5); the parts' models, collision and points are edited in Blender
  through `c2 map-export` of the floor's map (section 6). Both build and read
  back as made.

**Not yet:** tried in the game. The routes `GetDngMapNextRoot` opens (which
of `way36`-`way51` a floor gets) and the dummy tree layout
(`SetDummyTree`) are read only in outline; the parts' `tile` table (unused:
the mini map takes its tiles from `MiniMapInfoData`) is not decoded.

## 1. Loading a floor

A floor script calls `load_dungeon_map(map, cfg, flags)` (`_LOAD_DUNGEON_MAP_FILE`,
`LoadDungeonMapFile` in `dng_event`, 0x2931B0). It loads the map as any
other (its number from the map table, 400 placement slots), the mini map's
pictures (`dungeon/minimap/<map>.img`), then, when `cfg` is given, reads
`dungeon/cfg_file/<cfg>.cfg` into the read buffer, runs it
(`CAutoMapGen::SetupRoomInfo`), ORs `flags` into `gen_flag` and calls
`CAutoMapGen::Build`. With no `cfg` the map is a fixed scene (boss rooms,
`s17`).

| flag | name (`AUTOMAP_GEN_FLAG`) | effect |
|---|---|---|
| 0x01 | `AUTOMAP_GEN_DUMMY_TREE` | trees (`o00`, `o01`) on the empty cells around the floor |
| 0x02 | `AUTOMAP_GEN_PRESET_FLOOR` | a whole layout chosen by the floor's number (below), no random rooms |
| 0x04 | `AUTOMAP_GEN_DUMMY_MOUNTAIN` | places the mountain `o00` |
| 0x08 | `AUTOMAP_GEN_FIXED_START` | a random 20 × 16 floor whose first room is layout 0 at the grid's corner |
| 0x10 | `AUTOMAP_GEN_NO_IN_OUT` | no way in (`way32`-`way35`) |
| 0x20 | `AUTOMAP_GEN_NO_HEALING` | no healing point |
| 0x40 | `AUTOMAP_GEN_FIXED_FLOOR` | layout 0 is the whole floor |

`Build` (0x1DA220): with 0x40, the grid takes layout 0's size and is filled
from it; with 0x02, layout `n` by the floor number `f` (the save's
`floor_id` of the dungeon): `f` 5 and 11 take layout 0, other floors below 19
one of 1-19 at random, floors from 19 one of 0-9; with 0x08 a 20 × 16 random
floor; otherwise a 14 × 14 random floor. The grid holds 900 cells
(`CAutoMapParts[900]`, `dng_main.cpp`), so a layout given whole has at most
900 cells.

What the disc's scripts load (`load_dungeon_map` calls):

| map | room script | flags |
|---|---|---|
| d01f01, d04f01, d04f01s | d01f01 | random 14 × 14 |
| d02f01, d02f01r, d02f01s | d02f01 | random, trees |
| d03f01, d03f01r / d03f02, r, s / d03f03, r | d03f01 / d03f02 / d03f03 | preset by floor, mountain |
| d04f00 | d04f00 | fixed start, no way in, no healing |
| d04f02, d04f02r | d04f02 | fixed start (most without healing) |
| d05f01, d05f01s, d05f02 | d05f01 | random |
| d06f01, d06f02, d06f03 | d06f01, d06f02, d06f03 | random |
| d07f01 | d07f01 | random |
| d01f01 ... d07f01 | mstage0-8 | fixed floor (Spheda and the like) |

`d04f01.cfg` and `d07f02.cfg` are named by no script; `_d03f03.cfg` is an
older copy and `room_d01` (named twice) is on no disc.

## 2. The room script (`dungeon/cfg_file/*.cfg`)

A cfg script (`TAG arg, ...;`, [data](data.md)) of the tags `tag__4` lists:

```
VER 1.01;                                  read by no tag (skipped)
GRID_SIZE 320.000000, 320.000000;          a cell's width (x) and depth (z) in world units
ROOM_ID 0;                                 a layout: its id (read, never used)
ROOM_SIZE 3,3;                             width, height in cells
ROOM_RATE 100;                             percent: how often random selection keeps it
ROOM_FIXED 0;                              above 0: never chosen at random
RD 60,0,52,0,61,0;                         one row: part, attribute for each cell
RD 55,0,76,0,53,0;
RD 63,0,54,0,62,0;
ROOM_END;                                  next layout
```

`SetupRoomInfo` (0x1D6D80) makes room for 64 layouts (`AUTOMAP_ROOM_INFO`,
0x18 each) and allocates each table at `ROOM_SIZE`; an `RD` line whose
argument count is not twice the width is dropped (`_RD` returns 0), leaving
the table short. Layouts are used by their order in the file. `GRID_SIZE`
sets the cell size (320, but 400 in d05 and d06's scripts); a script
without it (d02f01) keeps the size set before (320 when the dungeon starts,
`InitDungeonMain`). The files are CRLF text, one
statement a line, three blank lines between layouts.

**Cells.** A part is an index into `PartsInfoData` (0x33D440 in
`SCES_511.90`, 277 entries of `AUTOMAP_PARTS_INFO`: the name of a template
of the floor's map, its cell kind, the sides it opens onto, an entrance's
side, and eight tile numbers no code reads); -1 leaves the cell empty. The
attribute is the cell's (`AUTOMAP_ATTR`): bit 0 hides it from the mini map
and keeps monsters, chests and stones off it (`SearchMapFlatPosition`);
scripts read it with `GetAttrStatus`. Values 0, 1 and 3 occur.

| parts | kind | what |
|---|---|---|
| `way00`-`way31` (0-31) | 1 corridor | by the sides they open onto (`link`: 1 −z, 2 +z, 4 +x, 8 −x); 28-31 dead ends |
| `way32`-`way35` | 0x200 | the way in: a dead end (28-31) + 4 |
| `way36`-`way51` | 0x200 | ways on, four sets of four (dead end + 8, 12, 16, 20) |
| `room00`-`room27` (52-79) | 2 room | first room family: edges, corners and middles by `link` |
| `door00`-`door27` (80-107) | 0xA (0x12 closed) | a first-family room cell a corridor enters, by `entrance` and `link`; `door04`-`07` and the like (+4) closed by a door |
| `room28`-`room55` (108-135) | 4 room | second room family |
| `room56`-`room59` (136-139) | 0x404 | the healing point's room (point `kaifuku`) |
| `door28`-`door55` (140-167) | 0xC (0x14 closed) | second-family entrances |
| `part00`-`part31` (168-199) | 0x104 | room cells of a third look |
| `step00`-`step31` (200-231) | 0x24, 0x64, 0xA4 | steps |
| `door56`-`door99` (232-275) | 0x2C-0x114 | entrances of steps and parts |

The table's names are the templates a floor's map must define; a map
lacking one places nothing there. The JSON writes the names (section 5).

## 3. Laying out a random floor (`RandomMapMainProc`, 0x1D9DF0)

1. The grid is cleared. Room 0 is put in: with 0x08 layout 0 at the corner
   (0, 0); otherwise a random layout at a random cell. **Choosing a layout**
   (`CreatRoom`): one at random, kept unless `ROOM_FIXED` > 0 or `ROOM_RATE`
   < a random 0-99, until one is kept (so a script whose layouts are all
   fixed hangs a random floor). **Fitting it:** inside the grid with the
   cells around it free (a margin of one, corners excepted); else another
   cell is tried. Its cells take the part, attribute, the part's kind and
   sides and the room's number.
2. Room 1 likewise, and a corridor between rooms 0 and 1 (`RoomLink`): from
   a cell of room 0 on the side facing room 1 (a room cell, not a healing or
   `part` cell) which becomes an entrance (kind | 8), dug towards room 1
   cell by cell, turning at random, joining any corridor or room it meets
   (`LinkConnectCheck`).
3. 4-6 rooms in all: each new room linked to a random earlier one (128 tries
   to fit each). With 0x08 the corridor out of room 0 leaves it along +x and
   no later room links to room 0.
4. 1-3 more corridors between random pairs of rooms; 1-3 dead-end corridors
   off a random room (`CreatDummyRoot`); three short branches off random
   corridor cells (`CreatTermParts`).
5. Parts by shape (`SetPartsIndex`): a corridor cell takes the first `way`
   part whose `link` is its open sides; an entrance the first part whose
   kind, entrance side and open sides are the cell's. So a layout's edge
   cells must be parts that have entrance versions: a first-family edge with
   one closed side (`room00`, link 14) entered from it becomes `door00`
   (link 15, entrance 1).
6. Unless 0x10, a random dead end becomes the way in (+4, `way32`-`35`).
   With 50%, a room with exactly one entrance (none of `door56`-`63`) gets it
   closed (`CreatDoorRoom`: kind | 0x10, part + 4); the floor's key is put in
   a chest outside that room. Unless 0x20, with 51% one `room28`-`room35`
   cell becomes the healing room `room56`-`59` (`SetHealingPointIndex`).
   Dead ends then become the ways on, one per route the floor manager gives
   (`GetDngMapNextRoot` bits 1, 2, 4, 8: + 8, 12, 16, 20).
7. Placing (`IndexToPartsPlace`, 0x1D9660): the map's placements are cleared
   (a library's own `MAP_PARTS`, if it had any, would go too), every cell's
   part placed at (x × cell width, 0, y × cell depth), unturned, scale 1, its
   placement named after the part; trees and mountain by the flags; then
   `p01_gio` (the geostone's stand), 12 × `obj01` (random stones) and
   `obj02` (the pot) far below (y −99,999) until placed. Each cell keeps its
   placed part and the part's `MOVE_FLAG` (1 −x, 2 −z, 4 +x, 8 +z closed),
   which the mini map and navigation read; the mini map's tile for each part
   is in `MiniMapInfoData` (18 maps, by the first six letters of the map's
   name).

A preset floor (0x02, 0x40) is one layout copied into the grid as it is
(`CreatFixedMap`), its parts placed the same way; nothing is chosen, linked
or closed. A layout given whole names its own corridors and entrances.

## 4. Chests, monsters, stones (`AutoSetTreasureBox`, `AutoSetMonster`)

Everything put on a floor goes to a random flat spot
(`SearchMapFlatPosition`): a random placed part whose cell is not hidden
(attribute bit 0), then up to 16 random points within ±160 of its position,
each cast straight down through the floor collision of that area; the first
surface below the cast that is not area 11 must be **area 5** (the collision
attribute of [maps](maps.md) section 4.3). So a part's floor where things
may stand is painted area 5; other areas (6, the usual floor) carry no spawns.

- **Chests** (`AutoSetTreasureBox__Fv`): 8, at least 320 from the way in;
  their items from `dungeon/cfg_file/tbox_d0<dungeon>.cfg` by floor (groups
  of items by rank), the first two always items 306 and 305 (flag 0x41);
  sizes, mimics and double chests rolled.
- **Mimics:** one chest per mimic monster (ids 245-268) of the floor's list.
- **Random circles:** 0-3. The **geostone** on floors that have one, and the
  12 random stones, each at least 320 from the way in and 40 from anything
  else (`CheckObjectPutArea`). The key of a closed room in a chest outside it.
- **Monsters** (`AutoSetMonster`): up to 24 from the floor's list
  (`mos_place`, `data:floors`), each more than 520 from the way in.

## 5. Layouts as data (`data:layouts`)

`c2 mod asset ISO NAME data:layouts` writes `data/layouts.json`: every room
script of the table in section 1, with its grid size and layouts:

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
in the files' layout, the rest keep their bytes. Several mods' layouts merge;
two giving one field different values are refused. Names not in the table,
uneven rows and more than 900 cells are refused.

Keep in mind: a random floor fits rooms into a 14 × 14 grid (20 × 16 with
0x08) with a free margin, so a large room is chosen and then rarely fits;
its edge cells need entrance versions (section 3, step 5); a preset floor
(d03, mstage) is used by its number, so adding layouts there changes nothing
until a floor picks them.

## 6. The parts in Blender

A floor's map places none of its templates, so `c2 map-export d01f01` shows
each template once (the first of a name), on the ground in rows by family
(`way`, `room`, `door`, `part`, `step`, then the objects), 480 apart: a
placement node marked `"preview": true` with no slot. Its pieces (drawn
model, `-a` floor and wall collision, `-c` camera collision) and points
(`point` positions, `kaifuku`, doors' `key` events) are edited as in any map
([the add-on](../../src/blender/README.md)), and the export into a mod
writes them into the template: moving, adding and deleting pieces and
points, editing the meshes, painting collision (area 5 where things spawn,
section 4), new models. A preview itself places nothing, and moving it
changes nothing. Keep a part within its 320 × 320 cell around its origin
(400 in d05, d06), its open sides where its `link` says: the generator joins
parts edge to edge and never looks at their meshes. An unedited export
changes nothing (d01f01 imported and exported by the add-on in Blender 5.1).

New parts cannot be named: the 276 names are fixed in the executable, and a
layout can only use them. A floor's map may define them differently from
another's (each dungeon's `way00` is its own model).
