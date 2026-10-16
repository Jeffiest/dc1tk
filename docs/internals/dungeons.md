# Dungeon floors: how a floor is built

A dungeon floor is not stored as a pre-built map on the disc. Instead, a floor's `.map` file (such as `d01f01.map`) is a library of part templates, and the engine assembles a grid of parts from a room script in `dungeon/cfg_file/` each time the player enters the floor. This page documents the room script format, how the generator selects and places parts, and how the engine spawns treasure chests, monsters, and stones.

All engine source citations refer to the `ps2/` directory of the ChronicleTwo decompilation (`src/automap.cpp` and `include/automap.hpp`; functions that remain in assembly were analyzed from `asm/pal/nonmatchings` using `m2c`). All counts refer to the extracted PAL disc corpus. For the `.map` file format, see [maps](maps.md). For per-floor monster tables and floor medal requirements, see [data](data.md). For instructions on editing floor layouts, parts, and chests, see [editing dungeons](../guide/dungeons.md).

## 1. Loading a floor

A dungeon floor script calls `load_dungeon_map(map, cfg, flags)` (`_LOAD_DUNGEON_MAP_FILE`, implemented by `LoadDungeonMapFile` in `dng_event` at `0x2931B0`). This function loads the `.map` file like any standard map (looking up its index in the map table and allocating 400 placement slots) and loads the minimap texture archive (`dungeon/minimap/<map>.img`). When `cfg` is non-empty, `LoadDungeonMapFile` reads `dungeon/cfg_file/<cfg>.cfg` into the read buffer, parses the room layouts with `CAutoMapGen::SetupRoomInfo`, bitwise-ORs `flags` into `gen_flag`, and calls `CAutoMapGen::Build`. When `cfg` is empty, the map loads as a fixed scene (such as boss rooms or `s17`).

| flag | name (`AUTOMAP_GEN_FLAG`) | effect |
|---|---|---|
| 0x01 | `AUTOMAP_GEN_DUMMY_TREE` | Places border trees (`o00`, `o01`) on empty cells surrounding the floor |
| 0x02 | `AUTOMAP_GEN_PRESET_FLOOR` | Selects a single whole-floor layout based on the floor number (see below), with no random rooms |
| 0x04 | `AUTOMAP_GEN_DUMMY_MOUNTAIN` | Places the surrounding mountain model `o00` |
| 0x08 | `AUTOMAP_GEN_FIXED_START` | Generates a random `20 × 16` floor whose first room is layout `0` placed at the grid corner `(0, 0)` |
| 0x10 | `AUTOMAP_GEN_NO_IN_OUT` | Does not place an entrance (`way32`–`way35`) |
| 0x20 | `AUTOMAP_GEN_NO_HEALING` | Does not place a healing room |
| 0x40 | `AUTOMAP_GEN_FIXED_FLOOR` | Uses layout `0` as the entire floor |

`CAutoMapGen::Build` (`0x1DA220`) selects the grid size and layout mode from `gen_flag`:

- With `0x40`, the grid adopts layout `0`'s dimensions and copies layout `0` directly.
- With `0x02`, the generator selects a single preset layout `n` based on the current floor number `f` (`floor_id` in the save data): floors `5` and `11` use layout `0`, other floors below `19` pick a random layout from `1`–`19`, and floors `19` and above pick a random layout from `0`–`9`.
- With `0x08`, the generator builds a `20 × 16` random floor.
- Otherwise, the generator builds a `14 × 14` random floor.

The floor grid buffer holds 900 cells (`CAutoMapParts[900]` in `dng_main.cpp`), so a whole-floor preset layout can contain at most 900 cells.

The scripts on the PAL disc call `load_dungeon_map` with the following configurations:

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
| d01f01 ... d07f01 | mstage0-8 | fixed floor (Spheda and similar stages) |

No script on the disc references `d04f01.cfg` or `d07f02.cfg`. The file `_d03f03.cfg` is an unused older copy, and `room_d01` (named twice in scripts) does not exist on the disc.

## 2. The room script (`dungeon/cfg_file/*.cfg`)

A room script is a text `.cfg` file (`TAG arg, ...;`; see [data](data.md)) parsed using the tag table `tag__4`:

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

`SetupRoomInfo` (`0x1D6D80`) allocates space for up to 64 layouts (`AUTOMAP_ROOM_INFO`, `0x18` bytes each) and allocates each layout's cell table when it reads `ROOM_SIZE`. If an `RD` line does not contain exactly `2 * width` arguments, `_RD` returns `0` and drops the line, leaving the layout table incomplete. The generator indexes layouts by their order in the file rather than by `ROOM_ID`. `GRID_SIZE` sets the cell dimensions (`320` in most scripts, or `400` in `d05` and `d06`). A room script without `GRID_SIZE` (`d02f01.cfg`) keeps the previously set size (`InitDungeonMain` sets it to `320` when the dungeon starts). Retail room scripts use `CRLF` line endings, with one statement per line and three blank lines between layouts.

**Cells.** Each cell in an `RD` row specifies a `part` index and an `attribute` integer:

- The `part` index selects an entry in `PartsInfoData` (`0x33D440` in `SCES_511.90`, an array of 277 `AUTOMAP_PARTS_INFO` records). Each record specifies the `PARTS` template name in the floor's `.map` file, the cell kind, the open sides mask (`link`), the entrance side, and eight unused tile numbers (section 2.1). A `part` value of `-1` leaves the cell empty.
- The `attribute` sets the cell's `AUTOMAP_ATTR` flags: bit `0` (`0x1`) hides the cell from the minimap and prevents monsters, chests, and stones from spawning on it (`SearchMapFlatPosition`). Scripts read this value with `GetAttrStatus`. Retail scripts use attribute values `0`, `1`, and `3`.

| parts | kind | what |
|---|---|---|
| `way00`-`way31` (0-31) | 1 corridor | Indexed by open sides (`link`: `1` −z, `2` +z, `4` +x, `8` −x); indices `28`–`31` are dead ends |
| `way32`-`way35` | 0x200 | Floor entrance ("way in"): dead end (`28`–`31`) + `4` |
| `way36`-`way51` | 0x200 | Floor exits ("ways on"): four sets of four (dead end + `8`, `12`, `16`, `20`) |
| `room00`-`room27` (52-79) | 2 room | First room family: edges, corners, and interior cells by `link` |
| `door00`-`door27` (80-107) | 0xA (0x12 closed) | First-family room cells entered by a corridor, indexed by `entrance` and `link`; `door04`–`07` and corresponding `+4` offsets are closed by a key door |
| `room28`-`room55` (108-135) | 4 room | Second room family |
| `room56`-`room59` (136-139) | 0x404 | Healing fountain room (contains trigger point `kaifuku`) |
| `door28`-`door55` (140-167) | 0xC (0x14 closed) | Second-family room entrances |
| `part00`-`part31` (168-199) | 0x104 | Third-family room cells |
| `step00`-`step31` (200-231) | 0x24, 0x64, 0xA4 | Stepped/elevated room cells |
| `door56`-`door99` (232-275) | 0x2C-0x114 | Entrances for `step` and `part` cells |

The template names in `PartsInfoData` are the `PARTS` names that a floor's `.map` file must define. If a `.map` file omits a template name, the engine places nothing in cells that use that part. Room layouts reference parts only by table index, and each dungeon's `.map` file defines its own meshes for those template names (for example, each dungeon provides its own `way00` template).

### 2.1 The parts table

Five engine functions read `PartsInfoData`:

- `CreatRoom` and `CreatFixedMap` read `kind` and `link` for layout cells.
- `SetPartsIndex` reads `kind`, `link`, and `entrance`.
- `IndexToPartsPlace` reads `name` to place the template.
- `CMiniMapSymbol::SetMapInfo` reads `name` to assign each placed part its minimap tile.

No code reads the eight integers that follow `entrance` in `AUTOMAP_PARTS_INFO`. Six of those integers match `d01f01`'s minimap tiles for parts `0`–`135`, and the other two differ where `d03` and `d04` differ, indicating that they are an unused legacy per-dungeon tile table (newer parts reuse room tiles in those fields). Instead, the engine reads actual minimap tiles from `MiniMapInfoData` (`0x33EE40`: 18 map entries of `char name[16]; s16 tile[320]`, matched against the first six characters of the floor map's name, so its four `...r` entries never match). Each `tile[part]` value selects a `16 × 16` cell in the `minimap1` texture (`u = (tile % 16) * 16`, `v = (tile / 16) * 16`), or `-1` for no tile. Entries `276`–`319` are `0` in every map.

The random floor generator automatically selects only a subset of `PartsInfoData`:

- Corridors `way00`, `01`, `08`–`11`, `16`, `20`–`23`, and `28`–`31` (the first corridor entry for each `link` mask)
- The first `door` entry for each `(kind, entrance, link)` combination in kinds `0xA`, `0xC`, `0x2C`, `0x6C`, `0xAC`, and `0x10C`, plus their closed versions (`part + 4`)
- Healing rooms `room56`–`59` and entrances/exits `way32`–`51`

All room, `part`, and `step` cells, the secondary corridor variants (`way02`–`07`, `12`–`15`, `17`–`19`, `24`–`27`), and entry `276` (named `""`, kind `0`) appear on a floor only when a room layout explicitly includes them. Several indices are hardcoded: entrances and exits are computed from dead ends (`28`–`31`) plus `4`, `8`, `12`, `16`, or `20` and looked up again by formatted name (`way%d`); the healing room is computed from `room28`–`35` plus `28` or `24` and looked up as `room5%d`; `CreatDoorRoom` skips rooms containing parts `232`–`239`; and the entrance search loop runs to index `280` (past the end of the table).

**The table cannot grow in place.** `MiniMapInfoData` sits immediately after `PartsInfoData` (after 8 bytes of padding whose first word terminates `SetMapInfo`'s loop at entry `277`), and compiled code references the table directly. Growing the table would require relocating `PartsInfoData` and patching all code references. **However, existing entries can be repurposed:** each part name is stored in its own dedicated 8-byte buffer (`0x36C708 + 8 * part`, with no other pointers referencing it), so a part's name (up to 7 characters), `kind`, `link`, and `entrance` can be modified in place. Entry `276` is completely free: its name is `""`, nothing places it, and its minimap tile is `0` in every map. Any layout-only part (a room cell, step cell, or corridor variant) can also be renamed across all dungeons at once; the floor `.map` files then define a template with the new name, and room layouts reference its index. Because the generator looks up `way32`–`51` and `room56`–`59` by formatted string name, those names must remain unchanged.

## 3. Laying out a random floor (`RandomMapMainProc`, 0x1D9DF0)

1. **Clear the grid and place room 0.** With flag `0x08`, the generator places layout `0` at corner `(0, 0)`; otherwise it places a random layout at a random grid position. **Choosing a layout** (`CreatRoom`): the generator picks a layout index at random and rejects it if `ROOM_FIXED > 0` or if `ROOM_RATE` is less than a random integer in `0`–`99`, repeating until it accepts a layout (if every layout in the file has `ROOM_FIXED > 0`, random floor generation hangs in an infinite loop). **Fitting the layout:** the room must lie within the grid with a 1-cell empty margin on all four sides (excluding diagonal corners); if the cells are occupied, the generator tries another grid position. Placed cells record the part index, attribute, part `kind`, open `link` sides, and room number.
2. **Place room 1 and link it to room 0 (`RoomLink`).** The generator places room 1 using the same rules, then digs a corridor from a cell on the side of room 0 facing room 1 (the starting cell must be a standard room cell, not a healing or `part` cell). That boundary cell becomes an entrance (`kind | 8`), and the generator steps toward room 1 cell by cell, turning at random, until the corridor meets any existing corridor or room (`LinkConnectCheck`).
3. **Place rooms 2 through 3–5 (4 to 6 rooms in total).** The generator attempts up to 128 positions per room and links each new room to a randomly chosen earlier room. When flag `0x08` is set, the corridor from room 0 always exits along `+X`, and no subsequent room links directly to room 0.
4. **Add extra corridors and branches.** The generator digs 1 to 3 additional corridors between random pairs of existing rooms, adds 1 to 3 dead-end corridors extending from a random room (`CreatDummyRoot`), and adds three short branches off random corridor cells (`CreatTermParts`).
5. **Assign part indices by connectivity (`SetPartsIndex`).** Each corridor cell receives the first `way` part whose `link` mask matches its open sides. Each room entrance receives the first door part whose `kind`, `entrance` side, and open `link` sides match the cell. Consequently, all outer edge cells of a room layout must use room parts that have corresponding `door` entries: for example, a first-family room edge with one closed side (`room00`, `link = 14`) entered through that closed side becomes `door00` (`link = 15`, `entrance = 1`).
6. **Assign entrances, key doors, healing rooms, and exits.**
   - Unless flag `0x10` is set, a random dead-end corridor becomes the floor entrance (`part + 4`, `way32`–`35`).
   - With 50% probability, a room that has exactly one entrance (and no `door56`–`63` cells) has its entrance locked (`CreatDoorRoom`: sets `kind | 0x10` and `part + 4`), and the generator places the floor's gate key in a treasure chest outside that room.
   - Unless flag `0x20` is set, with 51% probability one `room28`–`room35` cell becomes a healing room `room56`–`59` (`SetHealingPointIndex`).
   - Remaining dead ends become floor exits ("ways on"), one for each active route bit returned by the floor manager (`GetDngMapNextRoot` bits `1`, `2`, `4`, `8`, which add `8`, `12`, `16`, or `20` to the dead-end part index; see below). Each exit claims a random remaining dead end; if no dead end remains, that exit is omitted.
7. **Instantiate map placements (`IndexToPartsPlace`, `0x1D9660`).** The generator clears all existing map placements (removing any `MAP_PARTS` that were in the library `.map`), and places each occupied cell's template at `(x * cell_width, 0, y * cell_depth)` with zero rotation, scale `1`, and the placement name set to the part name. Next, it places border trees and mountains if enabled by `gen_flag`, followed by `p01_gio` (the geostone stand), twelve `obj01` instances (random decorative stones), and `obj02` (the pot), initially positioned far below the floor (`y = -99,999`) until `SearchMapFlatPosition` moves them onto the floor. Each grid cell stores a pointer to its placement and copies the template's `MOVE_FLAG` (`1` −X, `2` −Z, `4` +X, `8` +Z closed), which the minimap and monster navigation read. The minimap looks up each part's tile in `MiniMapInfoData` (18 maps, matched by the first six characters of the map's name).

On a preset floor (flags `0x02` or `0x40`), `CreatFixedMap` copies a single room layout directly into the grid and places its parts without running room selection, corridor linking, or door locking. A whole-floor preset layout specifies its own corridors, entrances, and exits directly (for example, `d03`'s room scripts include `way36`–`51` cells). The generator always places cell templates on the uniform grid and never inspects mesh geometry when connecting cells.

**Selecting floor exits (`CDngFloorManager::GetDngMapNextRoot`, `dngfloor.cpp`).** Floor `100` always receives one exit (bit `1`). On other floors, the engine finds the current floor's node in the dungeon's progression graph (`menu/dngmap/dmap<n>.cfg`, loaded by `LoadDataTable` alongside `dflr<n>.cfg`). For each of the node's four `RI_LINK` directions that leads to a room with a higher `order` (ignoring backward links), the adjacent passage cell (`GI 0`, using the first match) contributes bit `1 << type` from its `RT type,...` statement. There is one bit per route type, named `Nrm`, `Sun`, `Moon`, and `Star` in the debug menu (`dmap0` uses only type `0`; the other dungeons use `0`–`3`). Thus, a floor has at most four exits, one per route type. Each exit template in the floor's `.map` contains a door `event` point that triggers a program in the dungeon script:

- `way36`–`39`: program `300` (`gate`)
- `way40`–`43`: program `310`
- `way44`–`47`: program `320`
- `way48`–`51`: program `330`

The dungeon script calls `use_floor_gate(n)`, which queries `get_next_floor(floor, n)` (`GetDngMapNextFloorID`) for the first forward-linked floor connected by a passage of type `n`. Adding or removing exits on a floor requires editing `dmap<n>.cfg` (adding an `RI_LINK` to a higher-order room with a passage of the desired `RT` type next to it, which also updates the dungeon map screen), ensuring the dungeon script defines programs `310`–`330`, and ensuring the floor's `.map` defines the `way40`–`51` templates.

**Border trees (`SetDummyTree`, flag `0x01`).** In every generation mode, after placing the floor grid and before placing the mountain, `SetDummyTree` places `o00` on every empty cell adjacent (horizontally, vertically, or diagonally) to a placed floor cell, and places `o01` on every empty cell adjacent to an `o00` cell. This forms two concentric rings of trees around the floor, with one tree per cell at the cell's grid coordinates, zero rotation, scale `1`, and no minimap tile. Tree placements do not have a separate limit: each tree consumes one of the 400 placement slots before `p01_gio`, the 12 stones, and the pot are placed, so a large floor with `0x01` enabled can exhaust all 400 slots before those objects are placed.

## 4. Chests, monsters, stones (`AutoSetTreasureBox`, `AutoSetMonster`)

The engine places all floor objects on random flat floor surfaces using `SearchMapFlatPosition`. This function picks a random placed part whose cell does not have attribute bit `0` (hidden) set, and tests up to 16 random `(x, z)` points within `±160` units of the part's center by casting a ray straight down through the floor collision mesh. The highest collision triangle hit by the ray (ignoring collision area `11`) must have **area `5`** (see [maps](maps.md) section 4.3 for collision material encoding). Therefore, floor surfaces where chests, monsters, and stones can spawn must be painted with collision area `5`; standard floor triangles (area `6`) receive no spawns.

- **Treasure chests (`AutoSetTreasureBox__Fv`):** Places 8 chests, each at least `320` units from the floor entrance. Their contents come from `dungeon/cfg_file/tbox_d0<dungeon>.cfg` according to the current floor (which maps floors to ranked item groups). The first two chests always contain items `306` and `305` (when dungeon flag `0x41` is active). The engine then rolls chest sizes, mimics, and double-chest upgrades.
- **Mimics:** One chest becomes a mimic for each mimic monster entry (monster IDs `245`–`268`) in the floor's monster spawn table.
- **Magic circles, geostone, and decorative stones:** Places 0 to 3 random floor circles, the **geostone** (`p01_gio`) on floors that have one, and 12 decorative stones (`obj01`). Each object must be at least `320` units from the floor entrance and at least `40` units from any other placed object (`CheckObjectPutArea`). If the floor has a locked door room, the engine places the gate key inside a chest located outside that locked room.
- **Monsters (`AutoSetMonster`):** Spawns up to 24 monsters from the floor's monster table (`mos_place`; see [data](data.md)), each placed more than `520` units from the floor entrance.

### 4.1 What chests hold

`AutoSetTreasureBox` loads `dungeon/cfg_file/tbox_d0<n>.cfg` (where `n` is the 1-based dungeon number; `tbox_d08.cfg` for "back dungeon 2" is empty), and `CreatTresuarBoxInfo` (`dng_event.cpp`) parses it:

```
GROUP_START 5,29;                 groups, items (read and dropped; the counts)
GROUP 13,6;                       a group: its id, how many items it uses
ITEM 268,60,3,275,70,2,...;       item, rank, count, ... (any number a line, appended)
FLOOR_START 5,19;                 floors, group references (kept, otherwise the counts)
FLOOR 2,4,13,24,63,12;            a floor (the dungeon's floor number), how many groups, their ids
```

The parser supports at most 64 groups of up to 96 items each, and floors `0`–`127` referencing up to 64 groups per floor. Each chest requests a target item rank. `PickupRandomItem` repeatedly picks a random group from the current floor's `FLOOR` entry and a random item from that group until it finds an item whose rank is at least the requested rank (or at most the requested rank, when the requested rank is negative), clamped to the floor's minimum and maximum available ranks. If a `FLOOR` line references a group ID that does not exist in the file, the game hangs in an infinite loop while printing a debug message. If a floor number is omitted from the file, that floor has no item groups (the retail files omit floors that have no random chests, such as boss floors).
