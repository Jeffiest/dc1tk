# Maps: .map, .pcp, .mpk, .ipk, collision

The formats behind a map: what the game loads for a place, how its pieces are
placed and collided with, and what the engine needs of a new map. Engine
source: the ChronicleTwo decompilation's `ps2/` (paths below are relative to
it). Corpus: the extracted PAL disc. Numbers are counts over the whole corpus.
They come from research scripts run over all of it (not in this repository).
Map meshes are the MDS format of [models](models.md); this page covers what is
particular to maps. Random dungeon floors are laid out from template
libraries at run time: [dungeons](dungeons.md). Editing a map, and making a
new one, is in [modding maps](../modding/maps.md).

**Not yet known:** how the VU program combines vertex colour with lighting is
untraced; `invent`'s int and float and `sound`'s int are unnamed.

## 1. Files of a map

A map named `s17` lives at `map/<first letter>/<letters 1-3>/<letters 4-6>/`,
the third folder only for a name of six letters or more (`GetMapPath`,
`mapselect.cpp:197`), so `map/s/s17/`, `map/m/m05/m05x.map` and
`map/i/i03/h02/i03h02.map`:

| file | what |
|---|---|
| `s17.map` | text: templates of pieces, their placements, trigger points, lights, fog, cameras |
| `s17.mpk` | pack of model packs: `s17_0.pcp`, holding `info.cfg` and the `.mds` meshes |
| `s17.ipk` | pack of texture files (`.img`) |
| `s17.efp` | pack of effect scripts (`.em`) and effect textures |
| `s17.sky` | sky pack (`info.cfg`, meshes, `.img`), when the map has a sky |
| `s17.cfg` | extra map data: trigger events, water surfaces, river parts, occlusion planes |
| `s17*.stb` | the map's event scripts (one per language) |

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
   map* from `map/cmn/<path>` when the map table (`map<language>.cfg`, below)
   names one.
2. Run the settings pass of the `.map` (section 2.1).
3. Load the texture files (`.ipk`) and model packs (`.mpk`) of the map and the
   add map into one shared model list.
4. Size the placement table: 320 slots, 400 for dungeons and the title.
5. Run the parts pass: the add map first (`map.cpp:1352`), then the map, then
   its `.cfg`, then `PlacePartsEnd`.

`map0.cfg`-`map6.cfg` (one per language, 189 entries; `map/map.cfg`, 86,
is an older table nothing reads) list the maps:
`MAP_NAME "name","title","add_path",type,sel_type[,snd_id[,area_no]]`.
A map's number is its line, so a new map is appended (section 7). 22 map
files are not listed (dungeon variants and debug maps loaded by name).
`snd2/MAP_index.txt` gives sound sets by a map's sound id.

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
`mg_dataset.cpp:163`; see [models](models.md), the name flags): `aPP` additive,
`aNN` subtractive, `aOF` opaque, `aXX` alpha-test reference, `c0` unlit, `z0` no
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
0-1, so tiling is real). Of the 5,662 pictures in the 549 map `.img` files (547
IM3, 2 IM2), 4,802 clamp (5), 839 repeat (0), 18 have no CLAMP (IM2) and 3 hold
junk values. 166 `#texanime` scripts in 161 map IMGs scroll and copy texture
areas. 848 `FUNC_ANIME_DATA` points animate pieces and frames (section 5.1),
and 44 `PIECE_MATERIAL` blocks recolour materials, by name.

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
  (g = 2.8571 is foot sound 2, b = 8.5714 is area 6, a = -11 is mask 12). A
  zero is stored as 1.0 (the default white) or 0.0. Every collision prim on the
  disc is 0x213; materials are untextured but for 103 of 14,682, and their other
  fields are the defaults (ambient 1,1,1,0, power 50).
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
| `invent` | 3,018 | `FUNC_INVENT_DATA id, min xyz, max xyz, int, float, angle_degrees` | a photo subject: a photo whose line of sight hits the box (min and max from the point, in its frame) gives idea `id` (below 1000; from 1000 a scoop). The float is how far the line reaches (400 when 0), the angle limits it to within that many degrees of the point's back axis. Only the points of drawn placements are looked at (`CMap::InScreenFunc`, `CMapParts::InScreenFunc`): every disc one is in a placement, and one among the map's own points is never photographed |
| `event` | 2,653 | `FUNC_EVENT_DATA` kind, event_no, program, 3 ints, target (15 chars), flags (action button, item, 0x100) | doors (1,250), events (1,157), treasure boxes `t_box` (122), `close_door`, `book`, `ed_door`, ladders `lddr_b`/`lddr_t`: the program is the map script's `PROGRAM` number it runs |
| `flare` | 2,631 | `FUNC_FIRE_DATA r,g,b,draw[,int,int]` | lens flares (all-zero colour means 128,90,38) |
| `plight` | 2,098 | `FUNC_PLIGHT_DATA power, r,g,b (0-511), mode, 4 ints, flicker type, depth, period` | point lights |
| `fire` | 1,636 | `FUNC_FIRE_DATA` | fires |
| `effect` | 1,234 | `FUNC_EFFECT_NAME` (an effect of the map's `.efp`) | effects |
| `sound` | 1,129 | `FUNC_SOUND_DATA number, near, far, int, shape, start xyz, end xyz` | ambient sound emitters |
| `anime` | 848 | `FUNC_ANIME_DATA "placement","piece","frame",drives,mode, start xyz, step xyz, end xyz[,uniform,relative]` (section 5.1) | moving, turning, scaling, recolouring and fading placements, pieces and model nodes every frame (clock hands among them) |

"talk", "exit" and "branch" are point names, not kinds. The map's `.cfg` can
add points too (`FUNC_DATA "event"` with `FUNC_EVENT_DATA n,"check"`).

Many positions are not in the map at all: scripts place characters and the
camera with literal coordinates (`place_chara(13, 68.04, 41.85, 1180.61, 3.14)`).

**Doors.** The engine's door (`CEditEvent`, `editevent.cpp`, edit mode only):
the player walks to it and presses the action button (flags `[1, 0, 1]`, as
every door of the disc; the third, 0x100, fades the screen out), it plays the
door-opening motion and sound, and then
- with a program (FUNC_EVENT_DATA's third number) above 0, it runs that
  program of the map's script;
- with program 0 it enters `target` as an interior (`go_to_interior`'s way:
  a sub map loaded beside the town, its 320 placements and texture blocks
  shared), or leaves the interior when the target is `exit`.

### 5.1 Animated pieces (`FUNC_ANIME_DATA`)

`mapFUNC_ANIME_DATA` (`mapload.cpp:1257`) reads the line into the point
(`CFuncPoint::AnimeData`); a `CObjAnime` per point (`include/funcpoint.hpp`,
whose `OBJ_ANIME_PARAM` and `OBJ_ANIME_MODE` name the values below) does the
rest. Its code is assembly only (`funcpoint.cpp:51-55`): `AssignFuncAnime`
0x2A10B0, `Step` 0x2A05D0 (its mode switch is the table `at_475__2`
0x373650), `SetParam` 0x2A0C20, `GetParam` 0x2A0FD0, `CheckOver` 0x2A0530.
The point's own `FUNC_POS` is never read.

| argument | field | meaning |
|---|---|---|
| 1 | `placement` | the placement a map's own point moves, by its `MAP_PARTS` name (`CMap::AssignFuncPoint`, `map.cpp:1365`; `GetPlaceParts`, `map.cpp:379`, exact case). A template's points move the placement that copies them (`CMapParts::AssignFuncAnime`, `mapparts.cpp:851`, from `Copy` when `PlaceParts` places it, `map.cpp:313`) and ignore it: 350 of their 524 name the template, 174 something else or `""` |
| 2 | `piece` | the piece moved, by `PIECE_NAME` (`SearchPiece`, `mapparts.cpp:107`, exact case); `""` moves the whole placement |
| 3 | `frame` | the node of the piece's model moved (`mgCFrame::SearchFrame`, `mg_frame.cpp:980`: depth first, names compared up to `--` by `StrCmp`, `mg_frame.cpp:948`); `""` moves the whole piece. A found node turns about its own origin (`SetRotType(2)`) |
| 4 | `drives` | what changes (`SetParam`): 1 position, 2 rotation in degrees (Euler XYZ, kept within ±360,000 and turned into radians), 3 scale, 4 colour (a node only: drawn unlit, `mgCFrameAttr::no_light`, in `color`, whose default is 128 a channel), 5 alpha (a node only: `obj_alpha`, the first number, 1 opaque). Other values change nothing |
| 5 | `mode` | how it changes each frame (`Step`), below; other values act as 0 |
| 6-8 | `start` | the value it starts at |
| 9-11 | `step` | added every frame; mode 10 reads it as start hour, end hour and fade hours |
| 12-14 | `end` | the value it ends at |
| 15 | `uniform` | non-zero: y and z are x's (one number scales evenly). Never set on the disc |
| 16 | `relative` | non-zero with `drives` 1: the position is put through the piece's own matrix (`CObject::GetMatrix`, or the placement's with no piece), so a piece with no `frame` moves by the value every frame (its value becomes a speed). Set on 40 lines, only 8 of them positions (the battleship's two hulls bobbing, `p01_battleship`) |

Arguments 15 and 16 are optional (5 lines have 14 arguments) and default to
0 (`CFuncPoint::Initialize` clears the point).

Values are absolute, in the space of the moved object's parent, and replace
its own from the start: `AssignFuncAnime` sets `start` at once, so a node's
position, rotation or scale from its model and a piece's `PIECE_POS`,
`PIECE_ROT` or `PIECE_SCALE` give way to it, and a placement's `MAP_PARTS`
transform (set after the copy) from the first frame on. A rotation's start
is where it is turned to, not an offset.

| mode | name | each frame |
|---|---|---|
| 0 | hold | sets the value again (never on the disc) |
| 1 | loop | adds `step`, without end |
| 2 | stop | adds `step`; once an axis that moves reaches `end`, holds `end` |
| 3 | repeat | adds `step`; once an axis reaches `end`, back to `start` |
| 4 | ping-pong | adds `step` until an axis reaches `end` (then `end`), then subtracts it until one reaches `start` (then `start`), without end |
| 5 | there and back once | as 4, then stops for good (never on the disc) |
| 6 | look at the player | a rotation only: the yaw (y) and pitch (x) from the moved object to the player (the player's position in the placement's or node's space), each kept between `start` and `end`; z stays |
| 7 | random | `start` + (`end` - `start`) × a new random number from 0 to 1, each axis apart |
| 8 | minute hand | y = -360 × the hour of the game's clock (one turn an hour); x and z stay |
| 9 | hour hand | y = -30 × the hour (one turn in twelve hours) |
| 10 | by the hour | `end` within the hours `step[0]`-`step[1]` (`CheckTime`), `start` outside, blended linearly over `step[2]` hours before and after (`SubTime`, the short way round the clock) |

A step overshooting the range makes a value flicker between `start` and
`end` every frame (`a05ia`'s nodes `a_0` to `e_3`: scale 0.9 and 0.8, step
-1). The animation runs every frame while its point is enabled and within
its hours (`CFuncPoint::Check`, `CheckTime`), from `CMap::AnimeStep`
(`map.cpp:1286`; a template's points from `CMapParts::AnimeStep`,
`mapparts.cpp:773`), which the main loop and the Georama edit loop call once
a frame (`dng_main.cpp:1790`, `editloop.cpp:279`; edit parts too,
`editmapeffect.cpp:85`), whether the placement is shown or not. 111 points
start disabled (scripts switch points with `set_func_point_enabled`), and 4
have hours. A disabled one holds its value.

On the disc: 848 points in 105 maps, 524 in templates and 324 the maps' own;
783 move a model node, 52 a whole piece and 13 a whole placement (`s03`'s
ground, scrolled 50 units a frame over 1,600 and repeated). 28 name a
placement or piece the file lacks (placed at run time, or left over) and move
nothing there.

| drives | count | modes |
|---|---|---|
| position | 66 | ping-pong 49, repeat 16, stop 1 |
| rotation | 376 | repeat 122, loop 119, ping-pong 72, stop 27, minute hand 14, hour hand 14, look 8 |
| scale | 199 | ping-pong 162, random 33, stop 4 |
| colour | 202 | ping-pong 173, random 23, repeat 6 |
| alpha | 5 | ping-pong 4, by the hour 1 (`g03`'s `seali`, shown 20:00-6:00 fading over an hour) |

## 6. Map kinds

- **Fixed scenes** (176 files with placements): towns and areas (`s`, `m`, `f`,
  `a`, `z`), interiors, dungeon boss and event rooms (`d..b..`, `d..e..`),
  special floors (`d02f09`, `d04f09`, `d05f09`, `d05f10`, `d06f97`-`f99`), and the
  Georama bases `g01`-`g05`.
- **Dungeon floor libraries** (28 files `d0Nf..`): templates only. Scripts call
  `load_dungeon_map(map, room_cfg, flags)` and `CAutoMapGen::Build`
  (`automap.cpp:595`) lays out a fixed preset (flag 0x40), a preset by floor
  (0x2), or a random 20×16 (0x8) or 14×14 grid from `dungeon/cfg_file/<cfg>.cfg`.
  Each cell's template, one of the 276 the executable names (`way00`-`51`,
  `room00`-`59`, `door00`-`99`, `part00`-`31`, `step00`-`31`), is placed at
  (x·320, 0, y·320), plus trees or mountains, the geostone `p01_gio`, stones
  `obj01` and pots `obj02`. The format and the generator are in
  [dungeons](dungeons.md).
- **Add maps** (5 under `map/cmn`): template libraries other maps place (train,
  `trn2`/`s36`, battleship01/02, and `g00`, the 52 Georama edit parts).
- **Georama towns**: the base is a fixed scene (`g0N.map` with `g00`'s
  templates); `g0N.cfg` gives fixed parts (`FIX_EPARTS`), starting placements
  (`INIT_EPARTS id,x,y,z,angle`) and the river (`EDIT_RIVER`); `g0N[0-6].gpi`
  the editable parts. The player's own town is in the save (`CEditData`: up to
  300 parts, angle in 15° steps).

What a map's `.cfg` adds (`CMap::LoadCfgFile`): water surfaces
(`WATER_VERTEX`'s grid between two corners, put at `WATER_POS`), occlusion
planes (four corners), areas whose parts are not drawn (`DRAW_OFF_RECT`: the
parts in the second box are not drawn while the camera is in the first), its
`FUNC_DATA "event"` points, and villagers (`NPC_PLACE` and `NPC_POS x, y, z,
yaw`). The sky the engine draws about the camera is the map's `.sky`, else
`map/def.sky`, when `SKY_INFO` turns it on: each time band's sky and
background sky models, once as drawn and once mirrored below the horizon, as
`CMapSky::DrawSky` draws them.

## 7. What the engine needs of a new map

- **A row in the map table.** `LoadMapName` (`mapselect.cpp`) runs
  `map/map<language>.cfg` at boot into `MapNameBuff` (0x8000 bytes):
  `MAP_NAME_NUM n` sizes the table (n + 1 rows of 0x1C) and the strings follow
  it; each `MAP_NAME` fills the next row unchecked, and `GetMapNameInfo` finds
  rows below n only. So a new map is a row appended with n raised: every other
  map keeps its number, which saves (`map_no`) and scripts hold. The tables
  fill 9,248 to 13,808 bytes (Korean), room for hundreds of rows. The row's
  fields: `type` (`MapType`, `mapselect.hpp`: 1 a Georama town with edit
  data, 2 an interior of a Georama town, whose events are `g00`'s script
  (`GotoInterior`, `mapjump.cpp:524`), 4 an interior, 5 a map of the
  future (f01 to f04: no time board or world map, an area number of its own),
  6 an interior of a future map; the rest are plain towns and areas (3) and
  the dungeon loop's maps (7 and 8); the edit loop and the menus read it),
  `sel_type` (the debug map list), `snd_id` (the sound
  set and default music, `GetMapSndDataID`), `area_no` (the save's area of
  the last main map) and `add_path` (an add map loaded with it). Every
  language's table numbers the maps alike and needs the row.
- **Its files** at `GetMapPath(name)` (section 1): the `.map`, and when
  present the `.cfg`, `.mpk`, `.ipk`, `.efp`, `.sky` of the same name
  (`GetLoadMapInfo`, `mapjump.cpp:147`); the members of its packs are found by
  the names the `.map`'s `IMG` and `PCP` lines give, so a copy may keep
  another map's (`e05.img`, `m05_0.pcp`). Its script is
  `<path>/<name>_<language>.stb`, else `<name>.stb` (`LoadMapScript`,
  `LoadScript`); a map without one loads with no events. Each new file takes
  a record of `DATA.HD4` (13 bytes and its path; about 7.6 KB are free, so 25
  to 35 new maps of 4 to 6 files). The engine's file names are 32 bytes.
- **A way in.** Towns are entered with `go_to_edit_mode(number, program)`,
  other maps from a script with `jump_to_map(point, name or number, program)`
  (a door is an event point whose program, in the map's script, calls it),
  `go_to_interior(point, name)` for an interior (a sub map, 320 placements),
  and in the dungeon loop `load_dungeon_map(name)`. The program runs on
  arrival.
- **Save flags.** The save keeps map flags (opened chests and the like) for
  map numbers below 256 (`SAVE_MAP_FLAG_MAX`): the disc uses 189, so 67 new
  maps keep them; a map numbered past that loads but keeps none.
- **Memory** as any map's ([memory](memory.md)).

**Its script.** `LoadMapScript` (`mapjump.cpp`) loads `GetMapPath(name)` +
`.stb` through `LoadScript`, which tries `<path>_<LanguageCode>.stb` and then
the path itself: `map/m/m05/m05x_1.stb` for English, else
`map/m/m05/m05x.stb`. The disc's unnumbered scripts are compiled as Japanese.
The engine runs `map_start` (100) when the map loads, unless the way in names
another program (`jump_to_map(point, map, program)`, `go_to_edit_mode(number,
program)`). How programs run is in [running](running.md).
