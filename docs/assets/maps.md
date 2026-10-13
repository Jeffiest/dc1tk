# Dark Chronicle maps: .map, .pcp, .mpk, .ipk, collision

The formats behind a map: what the game loads for a place, how its pieces are
placed and collided with, and what editing one in Blender through glTF has to
respect. Engine source: the ChronicleTwo decompilation's `ps2/` (paths below
are relative to it). Corpus: the extracted PAL disc. Numbers are counts over
the whole corpus. They come from research scripts run over all of it (not in
this repository). Map meshes are the MDS format of
[models.md](models.md); this page covers what is particular to maps.

## Status

**Settled (engine code and whole-corpus checks):**
- All 210 `.map` files parse into a typed tree and are written back byte for
  byte, also through JSON: `c2-assets`' `mapfile.rs` (`mapfile::parse`,
  `mapfile::write`; `cargo test --release corpus` with `C2S_CORPUS` set to the
  extracted disc checks them all). The tree keeps each number as the file
  types it (int, float, or an exponent the engine reads as 0) and every
  optional line and argument count of section 2.3; the layout is computed.
  New numbers are written as `%g` but never with an exponent.
- `c2 map` lists trigger points from that tree: a template's points once for
  every placement of it, under the placement's name and moved by its
  transform; the map's own points as part `""`.
- All 13,372 `.mds` files under `map/` (12,730 in `.pcp` packs, 501 in sky
  packs, 123 in character packs, 18 in `.skyb`) parse and build back byte for
  byte with `c2-assets`' `mds.rs`, and export with `to_gltf.rs` unchanged.
- What `-m`, `-a` and `-c` meshes are, the collision layout and its per-triangle
  attributes, how textures are found, how pieces are placed (section 2-4).

**Not yet written:** the scene exporter and importer (section 7 is the plan);
`FUNC_ANIME_DATA` is only partly decoded; the map memory budget is unmeasured;
how the VU program combines vertex colour with lighting is untraced.

## 1. Files of a map

A map named `s17` lives at `map/<first letter>/<letters 1-3>/<letters 4-6>/`
(`GetMapPath`, `mapselect.cpp:197`), so `map/s/s17/`:

| file | what |
|---|---|
| `s17.map` | text: templates of pieces, their placements, trigger points, lights, fog, cameras |
| `s17.mpk` | pack of model packs: `s17_0.pcp`, holding `info.cfg` and the `.mds` meshes |
| `s17.ipk` | pack of texture files (`.img`) |
| `s17.efp` | pack of effect scripts (`.em`) and effect textures |
| `s17.sky` | sky pack (`info.cfg`, meshes, `.img`), when the map has a sky |
| `s17.cfg` | extra map data: trigger events, water surfaces, river parts, occlusion planes |
| `s17*.stb` | the map's event scripts (one per language), decompiled to C |

File types under `map/`: `.mds` 13,372; `.stb` 962; `.img` 549 (inside `.ipk`,
`.efp`, `.sky`, `.chr`); `.cfg` 434 (222 pcp `info.cfg`, 75 chr/sky `info.cfg`,
about 70 map-level); `.pcp` 223 (one a stray empty `i01h02.pcp`); `.mpk`,
`.ipk`, `.map` 210 each; `.chr` 90 (and 67 inside pcps); `.sky` 56; `.gpi` 38
(Georama); `.em` 26; `.efp` 22. `.pcl`, `.ptl`, `.mpp`, `.mpl`, `.mapb`,
`map.lst`, `map.cfg_`, `make.bat` and `.div` are build leftovers with the
developers' `Y:\` paths.

Six maps sit where the path rule never finds them (`d/d02/f01r/d02f01r.map`,
`d/d06/b01/m07.map`, `i/i04/h02/i01h02.map`, `m/m06/m07.map`,
`s/s38/i03h02.map`, `s/s43/i03h02.map`).

### Loading (`CScene::LoadMapFromMemory`, `GetLoadMapInfo` at `mapjump.cpp:147`)

1. Read the map's `.map`, `.cfg`, `.mpk`, `.ipk`, `.efp`, `.sky`, and an *add
   map* from `map/cmn/<path>` when `map/map.cfg` names one.
2. Run the settings pass of the `.map` (section 2.1).
3. Load the texture files (`.ipk`) and model packs (`.mpk`) of the map and the
   add map into one shared model list.
4. Size the placement table: 320 slots, 400 for dungeons and the title.
5. Run the parts pass: the add map first (`map.cpp:1352`), then the map, then
   its `.cfg`, then `PlacePartsEnd`.

`map/map.cfg` and `map0.cfg`-`map6.cfg` (one per language, 189 entries) list
the maps: `MAP_NAME "name","title","add_path",type,sel_type[,snd_id[,area_no]]`.
A map's number is its line, so a new map is appended. 22 map files are not
listed (dungeon variants and debug maps loaded by name). `snd2/MAP_index.txt`
gives sound sets by a map's sound id.

## 2. The .map file

A tag script (`KEYWORD a,b,...;`) read by `CScriptInterpreter`
(`scriptinterpreter.cpp`), twice: `mapinfo_tag` (`mapinfo.cpp:73`) takes the
textures, packs, lights and settings, and `map_tag` (`mapload.s`) the parts,
pieces, placements, cameras and points. Each pass skips the other's tags
(an unknown tag is skipped to its `;`).

**Tokens.** A tag name starts with A-Z. Quoted text is a string (Shift-JIS
bytes allowed inside). Text with a `.` is a float, other text an int; text
with a letter is invalid and reads as 0, so the 3,750 exponents in the files
(`8.4208e-007`, all 1e-5 or smaller) load as 0. Ints and floats convert
freely. At most 64 arguments and 0x2800 bytes of strings per tag. `//` and
`/* */` comments are allowed, though no file has one.

### 2.1 Layout (the same order in all 210 files)

1. `IMG "x.img";` lines (257 in all, up to 16 a map), then `PCP "x.pcp";` lines
   (222, up to 16).
2. `PARTS` templates; 2 blank lines.
3. `ACTIVE_LIGHT_SET`, the `LIGHT_SET` blocks, then `FLOOR`, `CHARA_POS`,
   `DEF_FOOT`, `TIME_FLAG`, `TIME_LIGHT_NUM`, [`TIME_CFADE`], `SKY_INFO`,
   [`LENS_FLARE`], [`ALL_SCISSOR`], [`CHARA_LIGHT_ADJUST`]; 1 blank line.
4. `MAP_PARTS` placements; 2 blank lines.
5. [`CAMERA_INFO` block]; 2 blank lines.
6. [the map's own `FUNC_POINT`]; 3 blank lines.

Nesting, in a fixed order:

```
PARTS "name";                       a template; nothing is drawn until placed
	[FAR_CLIP distance,fade;]
	LIGHT_FLAG no_light,no_plight;
	[MOVE_FLAG a,b,c,d;]
	PIECE "model.mds",show;         repeated
		PIECE_NAME "name";
		PIECE_POS x,y,z;
		PIECE_ROT x,y,z;            radians
		PIECE_SCALE x,y,z;
		[PIECE_MATERIAL_START n; PIECE_MATERIAL "frame",material,r,g,b,a,int; ... PIECE_MATERIAL_END;]
		[PIECE_COL_TYPE type[,int];]
		PIECE_TIME start_hour,end_hour;
	PIECE_END;
	[FUNC_POINT n; ... FUNC_POINT_END;]   points carried by every placement
PARTS_END;

MAP_PARTS "placement",show;         places a copy of a template
	[MAP_FAR_CLIP distance,fade;]
	PARTS_NAME "template";
	PARTS_GROUP "group";
	PARTS_POS x,y,z;
	PARTS_ROT x,y,z;
	PARTS_SCALE x,y,z;
MAP_PARTS_END;

FUNC_POINT n;                       n = number of FUNC_DATA (the engine ignores it)
	FUNC_DATA "kind",enable;
		FUNC_NAME "name";
		FUNC_FLAG a, b, start_hour,end_hour;
		[FUNC_<KIND>_DATA ...;]     one line, by kind (section 5)
		FUNC_POS x,y,z, rx,ry,rz, sx,sy,sz;
	FUNC_DATA_END;
FUNC_POINT_END;
```

### 2.2 Keywords (counts over the 210 files)

| keyword | count | meaning |
|---|---|---|
| `PARTS` | 10,751 | template |
| `FAR_CLIP` | 7,261 (46 files) | draw distance and fade |
| `LIGHT_FLAG` | 10,751 | draw with scene lights cleared; point lights off. (0,0) 10,744, (1,0) 5, (0,1) 2 |
| `MOVE_FLAG` | 10,615 (missing from 136 parts in 13 files) | bits 1/2/4/8: closed sides -X, -Z, +X, +Z of a dungeon cell (`automap.hpp:58`); read only by the dungeon generator |
| `PIECE` | 65,744 | model file, show flag (0 in 1,046) |
| `PIECE_MATERIAL` | 66 (44 blocks, 7 files) | colour override of one material of a frame |
| `PIECE_COL_TYPE` | 60,710 (missing from 5,034) | 0 = normal; non-zero pieces are not copied into placements (`mapparts.cpp`), so only the town editor sees them (the 118 Georama `-h` footprints) |
| `PIECE_TIME` | 65,744 | shown from start to end hour; equal = always (685 have a window) |
| `MAP_PARTS` | 4,024 (176 files) | placement, show flag (0 in 76) |
| `PARTS_GROUP` | 4,024 (58 non-empty) | group, switched by cameras |
| `CAMERA_INFO` | 22 | number of `FIX_CAMERA` blocks |
| `FIX_CAMERA` | 35 | fixed camera: `FIX_CAMERA_POS`, `FIX_CAMERA_POS2 index,x,y,z`, `FIX_CAMERA_OFF_GROUP index,"group",int`, `FIX_CAMERA_RECT "box",min,max[,pos,rot,scale]` |
| `FUNC_POINT` | 5,933 | points (section 5) |

Settings (`mapinfo.cpp:205-568`): `LIGHT_SET n` blocks (up to 16) hold `FOV`
(ignored: the projection is always 52°, `mapinfo.cpp:277`), `BGCOLOR` and
`BGCOLOR2` (0-255; `BGCOLOR2` 0,0,0 copies `BGCOLOR`), `AMBIENT` (128 is
neutral), `LIGHT index,dx,dy,dz[,r,g,b]` (up to 4), `PLIGHT index,power,x,y,z,r,g,b`,
`FOG_ENABLE`, `FOG near,far[,r,g,b[,offset,far2]]`. `TIME_FLAG follow_clock,blend[,hour[,fixed]]`:
when a map follows the clock the light set is chosen by hour and blended into
the next over the last hour, light 0 following the sun. `TIME_LIGHT_NUM` 4
uses fixed bands, any other N divides the day from 9:00; it must be 8 or less
(`GetLightInfo` arrays). Otherwise `ACTIVE_LIGHT_SET` picks the set. Also
`FLOOR`, `CHARA_POS`, `DEF_FOOT`, `SKY_INFO int,float[,sun_angle_degrees]`,
`LENS_FLARE`, `TIME_CFADE`, `ALL_SCISSOR`, `CHARA_LIGHT_ADJUST`. The engine
also reads `LOD_START`/`LOD_BLEND`/`LOD_PIECE`/`LOD_END` and `_MAP_INFO_TOP`,
which no file uses.

### 2.3 Writing a .map back byte for byte

A writer that keeps only values (no original text) regenerates all 210 files
when it follows these rules:

1. Shift-JIS (cp932): 16 files have Japanese names (`"門"`).
2. LF line ends, a final LF, no trailing spaces, no comments.
3. One tab per nesting level; `LIGHT_SET` bodies use one space; `LIGHT_FLAG`
   has no indent in the 136 parts without `MOVE_FLAG` (an older exporter).
4. Arguments joined by `,`, except these add a space at fixed places:
   `FUNC_FLAG a, b, c,d`; `FUNC_POS p,p,p, r,r,r, s,s,s`;
   `FUNC_INVENT_DATA id, ...`; `FIX_CAMERA_RECT` after the name and between groups.
5. Blank lines fixed per section (2, 1, 2, 2, 3; 10 in every file), even when a
   section is empty.
6. Numbers as C `%g` (6 significant digits) with MSVC's 3-digit exponents;
   whole numbers without a dot; no `-0`. Values the engine should keep must
   not be written as exponents (it reads them as 0).
7. Record what is optional or varies: `MOVE_FLAG`, `PIECE_COL_TYPE` and its
   argument count, `FAR_CLIP`, `MAP_FAR_CLIP`, `TIME_FLAG` (2-4 arguments),
   `FOG` (5 or 7), `SKY_INFO` (2 or 3), `FUNC_FIRE_DATA` (4 or 6),
   `FUNC_ANIME_DATA` (14 or 16), and `LENS_FLARE` (missing in 9 files),
   `TIME_CFADE`, `ALL_SCISSOR`, `CHARA_LIGHT_ADJUST` (missing in 12).
8. `FUNC_POINT n`, `CAMERA_INFO n` and `PIECE_MATERIAL_START n` always equal
   their children, so they can be computed.

Data to keep as it is: 10 files repeat a `PARTS` name (the engine uses the
first); 2 repeat a `MAP_PARTS` name; 2 have gaps in `LIGHT_SET` numbers; 532
pieces name an empty model and about 490 a model in none of the map's packs
(neither is drawn).

## 3. Placement: templates, placements, pieces

`PARTS` builds a template (`mapPARTS`, `mapload.cpp:538`); `MAP_PARTS_END`
calls `PlaceParts(name, pos, rot, scale)` (`mapload.cpp:970`, `map.cpp:313`),
which copies the template into the next free slot with the placement's name,
show flag, far clip and group. An unknown template drops the placement without
taking a slot. 176 maps place their templates; 34 do not (dungeon floor
libraries, add maps, Georama `g00`), and are placed at run time.

**Transforms.** World = placement × piece × the mesh's own node matrices. A
node's local matrix is T·Rz·Ry·Rx·S (`mgCFrame::GetLocalMatrix`,
`mg_frame.cpp:753`), right-handed rotations (`mg_math.cpp:770-790`) in radians:
Blender's Euler `XYZ`. Y is up (gravity is -Y). Scales of -1,1,1 (732 pieces,
mirroring) and 20,20,20 (1,005) occur. A dungeon grid cell is 320 units.

**Models.** A piece's model is found by `CMap::SearchMDS` (`map.cpp:245`,
`mdslist.cpp:212`), ignoring case, across every model pack of the map and its
add map; 509 pieces use another map's pack. The same mesh is placed many
times. A model pack's `info.cfg` lists `MDS "name","source"; TYPE t; [FAR_CLIP]; MDS_END;`
(`PCP_NAME` first); its size is the pack's `.mds` and `.chr` count, at most
0x1FF (`CMdsList::LoadPCPFile`, `mdslist.cpp:484`).

| TYPE | engine (`pcpTYPE`, `mdslist.cpp`) | members | placed pieces |
|---|---|---|---|
| 0 | drawn model (`mgLoadMDSFile`) | `-m` 7,646, other 240 | 37,318 |
| 1 | floor and wall collision (`LoadCollisionFile`) | `-a` 2,673, Georama `-h` 122, `-s` 5, `-k` 3, other 36 | 13,910 |
| 2 | camera collision | `-c` 2,032, other 7 | 11,605 |
| 3, 4 | animated character (`CreateChara`), e.g. the train engine `kikan-m.chr` | `.chr` 67 | 928 |

So `-a` is collision (*atari*, a hit), not alpha. `DrawSub` skips odd types,
so collision is never drawn. Georama towns add `-h` (edit-mode footprints,
area kinds 1/2/3/5, split by `CEditCollision::Copy`), `-s` and `-k`.

**How scripts reach a map:** placements by slot (`MAP_PARTS` order, add map
first) or by name (`GetPlaceParts`, `map.cpp:379-395`): of 6,304
`bind_handle_map_part` calls, 5,700 pass a name. Pieces by `PIECE_NAME`,
frames by node name, points by `FUNC_NAME` with part `""` for the map's own
points (`set_func_point_enabled`, 831 calls, 552 with `""`;
`bind_handle_func_point`, `get_func_point_pos`, `set_func_point_pos`).
`set_map_part_motion` (202 calls) plays a motion on a `.chr` piece. So
placement, piece, node and point names must survive an edit.

## 4. Meshes, textures, collision

### 4.1 Drawn meshes (`-m`)

37,621 meshes, 1,833,433 triangles: strips (types 0x4/0x104, 218k primitives)
and lists (0x3/0x103, 49k). Hierarchies up to 18 deep; 35k non-identity node
matrices, none mirrored. Up to 4,880 vertices and 57 materials in a mesh.

**Blend modes are node-name flags** (text after `--`, `mgSetFrameAttr`,
`mg_dataset.cpp:163`; see [models.md](models.md) 1.5): `aPP` additive, `aNN`
subtractive, `aOF` opaque, `aXX` alpha-test reference, `c0` unlit, `z0` no
depth write, `fN` fog. Common: `n1z0c0f2app-` (722 nodes, additive and unlit),
`a40aof-` (362, alpha-tested and opaque). A material's diffuse alpha below 1
also blends (0.5 in 441). 1,488 of 7,661 `-m` files have nodes whose base names
repeat (`grid54--f2appz0n1t-5_1` and `-6_1`), so names with their flags are
what tell nodes apart.

**Lighting** is computed live from the light sets (section 2.2) and point
lights. Vertex colours are painted tints and alpha fades, not baked lighting:
13,766 meshes have colours, 43% of `-m` corners use them, mostly 1-2 colours a
mesh (greys 178 and 119, alpha 255 or 0).

### 4.2 Textures

Each `IMG` the `.map` names is entered into the texture manager
(`CMap::LoadData`, `map.cpp:1505`), and materials find pictures by name across
every loaded file. 44 maps have more than one IMG, 9 more than one PCP. Of
3,828 (map, texture) pairs, 3,716 are in the map's own IMGs and 112 (35 maps)
in shared or paired maps' IMGs (`cmn/g00/g00.ipk.d/a01.img`); `noIcon` is
nowhere on the disc. An exporter needs a search path of IMG files.

Wrap or clamp comes from each picture's GS CLAMP value (52,791 UVs lie outside
0-1, so tiling is real). 166 `#texanime` scripts in 161 map IMGs scroll and
copy texture areas; `im3::read` skips them, so they must be kept inside the
`.img`. 848 `FUNC_ANIME_DATA` points animate pieces and frames, and 44
`PIECE_MATERIAL` blocks recolour materials, by name.

### 4.3 Collision (`-a` floor and wall, `-c` camera)

`-a`: 10,370 meshes, 294,135 triangles; `-c`: 3,277 meshes, 59,649 triangles.
Both are ordinary MDS read by `LoadCollisionFile` (`collision.cpp`,
`CreateCollisionMDT`, confirmed in the assembly):

- Each node becomes a `CColFrame` with its matrix and a bounding box computed
  at load. There is no BSP, grid or stored bound.
- Only positions and triangle lists are read: a strip or a colour-flagged
  (0x100) primitive makes the node have no collision. Corners are read with
  stride 1 (vertex index only), so a collision primitive must be type 0x213;
  anything with normal or UV indices is misread. Every file complies except 2
  primitives of type 0x13 in `-h` files.
- **Per-triangle attributes come from the material's diffuse colour:**
  ground = (int)(r·0.7+0.01), foot sound = (int)(g·0.7+0.01),
  area = (int)(b·0.7+0.01), ignore mask = (int)(1-a). The files store k/0.7
  (g = 2.8571 is foot sound 2, b = 8.5714 is area 6, a = -11 is mask 12).
  In `-a`: ground is almost always 0, foot sound 0-27, area mostly 6, 9, 5, 1,
  7, 8, mask mostly 0, 12, 4, 8, 255. The mask is ANDed with each query's
  (`gameutil.cpp:1010`); areas 1 and 7 are water (`GetCPolyAttr`); foot sound
  picks the footstep effect.
- Queries go through the triangles of nodes whose box overlaps, up to the
  caller's buffer: 128 (monsters, objects), 512 (camera), 1024 (edit mode). So
  collision should stay split into several compact nodes.
- **The player and NPCs** move with `MoveCheck` (`gameutil.cpp:1599`): a cast
  along the velocity, `GetFootPoly` 20 units down, walls by `CheckWidth`
  (radius 15; a wall is a triangle with normal.y below 0.5), then water.
  `CMapPiece::GetPoly` also needs `PIECE_COL_TYPE` 0 (96 pieces have 1, which
  hides them from queries).
- **The camera** gathers camera-collision triangles within the distance to the
  player plus 10 and moves with `MoveCamera` (`dng_main.cpp:2165-2187`).

## 5. Trigger points (FUNC_POINT)

Points inside a template are copied with each placement (world = placement ×
`FUNC_POS`); points after the placements are the map's own
(`mapload.cpp:1147`). `FUNC_POS` is position, rotation (radians) and scale; for
event points a scale near 1 is replaced by a default size (doors 15, ladders
25, chests 20, `mapload.cpp:1437`). `FUNC_FLAG`'s last two numbers are the
hours the point works (`CheckTime`, `funcpoint.cpp:19`: start < end a window,
start > end past midnight, equal always).

| kind | count | data line | used for |
|---|---|---|---|
| `pos` | 3,614 | none | positions scripts and doors look up by name: `point`/`pointN` (3,439, inside door and entrance templates), `kaifuku` (136, recovery spots), `npc_pos`, `gcamp` (camps) |
| `invent` | 3,018 | `FUNC_INVENT_DATA id, min xyz, max xyz, int, float, angle_degrees` | a box with an id; by its name and the game, probably a photo subject for inventing ideas (not confirmed) |
| `event` | 2,653 | `FUNC_EVENT_DATA` kind, event_no, program, 3 ints, target (15 chars), flags (action button, item, 0x100) | doors (1,250), events (1,157), treasure boxes `t_box` (122), `close_door`, `book`, `ed_door`, ladders `lddr_b`/`lddr_t`: the program is the map script's `PROGRAM` number it runs |
| `flare` | 2,631 | `FUNC_FIRE_DATA r,g,b,draw[,int,int]` | lens flares (all-zero colour means 128,90,38) |
| `plight` | 2,098 | `FUNC_PLIGHT_DATA power, r,g,b (0-511), mode, 4 ints, flicker type, depth, period` | point lights |
| `fire` | 1,636 | `FUNC_FIRE_DATA` | fires |
| `effect` | 1,234 | `FUNC_EFFECT_NAME` (an effect of the map's `.efp`) | effects |
| `sound` | 1,129 | `FUNC_SOUND_DATA number, near, far, int, shape, start xyz, end xyz` | ambient sound emitters |
| `anime` | 848 | `FUNC_ANIME_DATA` placement, piece, frame, kind, int, 3 vectors[, 2 shorts] (partly decoded) | procedural motion of pieces (`CObjAnime`) |

"talk", "exit" and "branch" are point names, not kinds. The map's `.cfg` can
add points too (`FUNC_DATA "event"` with `FUNC_EVENT_DATA n,"check"`).

Many positions are not in the map at all: scripts place characters and the
camera with literal coordinates (`place_chara(13, 68.04, 41.85, 1180.61, 3.14)`).

## 6. Map kinds

- **Fixed scenes** (176 files with placements): towns and areas (`s`, `m`, `f`,
  `a`, `z`), interiors, dungeon boss and event rooms (`d..b..`, `d..e..`),
  special floors (`d02f09`, `d04f09`, `d05f09`, `d05f10`, `d06f97`-`f99`), and the
  Georama bases `g01`-`g05`.
- **Dungeon floor libraries** (28 files `d0Nf..`): templates only. Scripts call
  `load_dungeon_map(map, room_cfg, flags)` and `CAutoMapGen::Build`
  (`automap.cpp:595`) lays out a fixed preset (flag 0x40), a preset by floor
  (0x2), or a random 20×16 (0x8) or 14×14 grid from `dungeon/cfg_file/<cfg>.cfg`
  (`GRID_SIZE 320,320`, then per room `ROOM_ID`, `ROOM_SIZE`, `ROOM_RATE`,
  `ROOM_FIXED`, `RD index,attr,...` rows, `ROOM_END`; `automap.cpp:141-215`).
  RD indices are a hard-coded table of 277 template names
  (`PartsInfoData__DATA.s`: `way00`-`51`, `room00`-`59`, `door00`-`99`,
  `part00`-`31`, `step00`-`31`) with each cell's kind and open sides. Each cell's
  template is placed at (x·320, 0, y·320), plus trees or mountains, the
  geostone `p01_gio`, stones `obj01` and pots `obj02`.
- **Add maps** (5 under `map/cmn`): template libraries other maps place (train,
  `trn2`/`s36`, battleship01/02, and `g00`, the 52 Georama edit parts).
- **Georama towns**: the base is a fixed scene (`g0N.map` with `g00`'s
  templates); `g0N.cfg` gives fixed parts (`FIX_EPARTS`), starting placements
  (`INIT_EPARTS id,x,y,z,angle`) and the river (`EDIT_RIVER`); `g0N[0-6].gpi`
  the editable parts. The player's own town is in the save (`CEditData`: up to
  300 parts, angle in 15° steps).

## 7. Editing maps in Blender (the plan)

**Export a map to one glTF scene:**
- Templates become reusable node trees and placements instances of them, so a
  door placed 30 times is one door; each piece is a node, its mesh shared.
- Meshes through `to_gltf.rs` (already handles every map `.mds`), with node
  names kept whole (flags included) and `extras.mds_index` to match nodes back.
- Real textures from the map's IMGs and a search path of shared ones; GS
  CLAMP to glTF wrap; name flags to glTF materials (additive, alpha-test,
  unlit, double-sided).
- Collision (`-a`, `-c`) in their own collections, hidden by default, with
  attributes as named materials (foot sound, area, ignore mask) rather than
  raw colours.
- Trigger points as empties (cubes for their boxes) with kind and data in
  custom properties; point lights as Blender lights; fog, light sets, time and
  fixed cameras as scene properties.

**Import into a mod:**
- Changed placements and pieces rewrite the `.map` (section 2.3); placements
  are only appended, never reordered (scripts address them by slot).
- Only meshes that changed are rebuilt (the others stay byte for byte, as for
  characters); `from_gltf.rs` must match nodes by `extras.mds_index`, not by
  base name. Edited collision is written as type 0x213 triangles with positions
  only and the attribute encoding of section 4.3.
- New meshes go into the map's model pack with an `info.cfg` entry (0x1FF at
  most); packs are patched member by member (`c2_disc::rebuild::replaced_member`
  works two packs deep), never repacked: `models::write_pack` reproduces none
  of the map packs.
- New textures go into an `.img` inside the `.ipk`, keeping its `#texanime`.
- Limits: 320 placements (400 dungeons; the largest map has 245), names up to
  31 characters, `TIME_LIGHT_NUM` 8 or less. The memory budget is unmeasured.

## 8. The glTF scene (conventions shared by the exporter, the importer and the Blender add-on)

`c2 map export NAME` writes `extracted/maps/NAME/NAME.gltf` (with its `.bin`
and `textures/`); `--all` exports every fixed scene. A mod carries an edited
scene with a line `"map:NAME": "maps/NAME/NAME.gltf"` in `mod.json`; `build`
compares it with the disc's map and writes only what changed, and `mod make`
finds edited exports by the hashes the export records.

**Axes and units.** glTF's Y up is the game's; Blender's importer turns it
into Z up and its exporter back, so nothing else converts. One unit is one
game unit (a dungeon cell is 320). The game's rotations are Euler XYZ in
radians (T·Rz·Ry·Rx·S); glTF stores quaternions, so the importer keeps the
original numbers of any node whose matrix did not change (within 1e-4) and
writes new Euler angles only for nodes that moved.

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
| a fixed camera | `camera:INDEX` | `{"role": "camera", ...}` with children for its positions and boxes |

**Templates.** glTF has no instancing of node trees, so every placement holds
its own copy of its template's pieces and points; meshes are shared, so editing
a mesh in Blender edits every piece that places it, as in the game. On import,
a piece or point changed the same way in every placement of a template changes
the template; one changed in only some placements gives those placements a
copy of the template of their own (a new template name; scripts never name
templates). New placements are appended (slots are never reordered); a
deleted placement is kept with its show flag 0, so later slots keep their
numbers.

**Collision.** Pieces of kind `collision` (floor and wall, `-a`) and `camera`
(`-c`) hold triangles whose material names give their attributes
(`col_ground0_foot2_area6_mask12`); the add-on hides them by default and
colours them by area and foot sound.

**Points.** Point lights also get a `KHR_lights_punctual` light whose colour
and range the importer reads back; boxes (`invent` boxes, event sizes) are the
node's scale or `data` fields, which the add-on draws. Every other value is in
`extras.c2`, which Blender shows as custom properties.

**What the importer refuses:** a template name over 31 characters, more than
320 placements (400 in dungeons), meshes that break the collision rules of
section 4.3, a point of an unknown kind.
