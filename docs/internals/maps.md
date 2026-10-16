# Maps: .map, .pcp, .mpk, .ipk, collision

This page documents the map file formats and runtime systems: which files the engine loads for a map, how templates and pieces are placed and checked for collision, and what the engine requires for a new map. All engine source citations refer to the `ps2/` directory of the ChronicleTwo decompilation (paths below are relative to `ps2/`). All counts come from analysis scripts run across the entire extracted PAL disc corpus. Map meshes use the MDS format documented in [models](models.md); this page covers map-specific behavior. Random dungeon floors are generated at runtime from template libraries; see [dungeons](dungeons.md). For instructions on editing existing maps and creating new ones, see [modding maps](../guide/maps.md).

**Not yet known:** How the VU microprogram combines vertex color with dynamic lighting is not yet traced. In addition, the `int` and `float` parameters of `invent` points and the `int` parameter of `sound` points are unnamed.

## 1. Files of a map

A map named `s17` resides at `map/<first letter>/<letters 1-3>/<letters 4-6>/`. The engine uses the third directory level only when the map name has six or more characters (`GetMapPath`, `mapselect.cpp:197`). For example, `s17` is at `map/s/s17/`, `m05x` is at `map/m/m05/m05x.map`, and `i03h02` is at `map/i/i03/h02/i03h02.map`:

| file | what |
|---|---|
| `s17.map` | Text script: part templates, placements, trigger points, lights, fog, and cameras |
| `s17.mpk` | Pack of model packs (`s17_0.pcp`), containing `info.cfg` and `.mds` meshes |
| `s17.ipk` | Pack of texture archives (`.img`) |
| `s17.efp` | Pack of effect scripts (`.em`) and effect textures |
| `s17.sky` | Sky pack (`info.cfg`, meshes, and `.img`), present when the map has a sky |
| `s17.cfg` | Extra map data: trigger events, water surfaces, river parts, and occlusion planes |
| `s17*.stb` | Map event scripts (one per language) |

File counts under `map/` on the PAL disc:

- `.mds`: 13,372
- `.stb`: 962
- `.img`: 549 (stored inside `.ipk`, `.efp`, `.sky`, and `.chr` packs)
- `.cfg`: 434 (222 PCP `info.cfg` files, 75 `.chr`/`.sky` `info.cfg` files, and about 70 map-level `.cfg` files)
- `.pcp`: 223 (including one unused empty `i01h02.pcp`)
- `.mpk`, `.ipk`, `.map`: 210 each
- `.chr`: 90 standalone (plus 67 inside `.pcp` packs)
- `.sky`: 56
- `.gpi`: 38 (Georama)
- `.em`: 26
- `.efp`: 22

Files with extensions `.pcl`, `.ptl`, `.mpp`, `.mpl`, `.mapb`, `map.lst`, `map.cfg_`, `make.bat`, and `.div` are unused build artifacts that contain the developers' `Y:\` drive paths.

Six map files sit at paths that `GetMapPath` never reaches (`d/d02/f01r/d02f01r.map`, `d/d06/b01/m07.map`, `i/i04/h02/i01h02.map`, `m/m06/m07.map`, `s/s38/i03h02.map`, and `s/s43/i03h02.map`).

### Loading (`CScene::LoadMapFromMemory`, `GetLoadMapInfo` at `mapjump.cpp:147`)

1. Read the map's `.map`, `.cfg`, `.mpk`, `.ipk`, `.efp`, and `.sky` files. If the map table (`map<language>.cfg`, described below) specifies an *add map*, also read the add map from `map/cmn/<path>`.
2. Run the settings pass of the `.map` file (section 2.1).
3. Load the texture archives (`.ipk`) and model packs (`.mpk`) of both the main map and the add map into one shared model list.
4. Allocate the placement table: 320 slots for standard maps, or 400 slots for dungeons and the title screen.
5. Run the parts pass in order: first the add map (`map.cpp:1352`), then the main `.map` file, then the map's `.cfg` file, and finally `PlacePartsEnd`.

The files `map0.cfg` through `map6.cfg` (one per language, 189 entries each; `map/map.cfg` has 86 entries and is an unused older table) list all registered maps:
`MAP_NAME "name","title","add_path",type,sel_type[,snd_id[,area_no]]`.
A map's ID number is its 0-based row index in this list, so a new map must be appended to the end (section 7). Twenty-two `.map` files on the disc are not listed in `map<language>.cfg` (dungeon variants and debug maps that the engine loads directly by name). The file `snd2/MAP_index.txt` assigns sound sets according to a map's `snd_id`.

## 2. The .map file

A `.map` file is a text tag script (`KEYWORD a,b,...;`) that `CScriptInterpreter` (`scriptinterpreter.cpp`) parses in two passes. The first pass uses `mapinfo_tag` (`mapinfo.cpp:73`) to read textures, model packs, lights, and environment settings. The second pass uses `map_tag` (`mapload.s`) to read part templates, pieces, placements, cameras, and trigger points. Each pass skips the tags belonging to the other pass (any unrecognized tag is skipped up to its terminating `;`).

**Tokens.** A tag name starts with an uppercase letter (`A`–`Z`). Double-quoted text is a string (Shift-JIS bytes are allowed inside quotes). Unquoted text containing a `.` parses as a `float`, and other unquoted numeric text parses as an `int`. However, unquoted text containing any letter is invalid and evaluates to `0`. Because of this parser rule, all 3,750 scientific-notation exponents in the retail files (such as `8.4208e-007`, all `1e-5` or smaller) load as `0`. Integers and floats convert freely. Each tag allows at most 64 arguments and `0x2800` bytes of string data. The parser accepts `//` and `/* */` comments, though no retail `.map` file contains any.

### 2.1 Layout (the same order in all 210 files)

1. `IMG "x.img";` lines (257 total across the disc, up to 16 per map), followed by `PCP "x.pcp";` lines (222 total, up to 16 per map).
2. `PARTS` templates, followed by 2 blank lines.
3. `ACTIVE_LIGHT_SET`, the `LIGHT_SET` blocks, and then `FLOOR`, `CHARA_POS`, `DEF_FOOT`, `TIME_FLAG`, `TIME_LIGHT_NUM`, [`TIME_CFADE`], `SKY_INFO`, [`LENS_FLARE`], [`ALL_SCISSOR`], and [`CHARA_LIGHT_ADJUST`], followed by 1 blank line.
4. `MAP_PARTS` placements, followed by 2 blank lines.
5. [`CAMERA_INFO` block], followed by 2 blank lines.
6. [The map's own `FUNC_POINT` block], followed by 3 blank lines.

Blocks nest in a fixed order:

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
| `PARTS` | 10,751 | Part template definition |
| `FAR_CLIP` | 7,261 (46 files) | Draw distance and fade range |
| `LIGHT_FLAG` | 10,751 | Disables scene lights or point lights when drawing. `(0,0)` in 10,744 parts, `(1,0)` in 5, `(0,1)` in 2 |
| `MOVE_FLAG` | 10,615 (missing from 136 parts in 13 files) | Bits `1`/`2`/`4`/`8`: closed `-X`, `-Z`, `+X`, `+Z` sides of a dungeon cell (`automap.hpp:58`); read only by the dungeon generator |
| `PIECE` | 65,744 | Model filename and `show` flag (`0` in 1,046 pieces) |
| `PIECE_MATERIAL` | 66 (44 blocks, 7 files) | Color override for one material on a specific frame node |
| `PIECE_COL_TYPE` | 60,710 (missing from 5,034) | `0` = normal; non-zero pieces are not copied into placements (`mapparts.cpp`), so only the town editor uses them (the 118 Georama `-h` footprints) |
| `PIECE_TIME` | 65,744 | Active visibility window from `start_hour` to `end_hour`; equal values mean always visible (685 pieces specify a time window) |
| `MAP_PARTS` | 4,024 (176 files) | Template placement and `show` flag (`0` in 76 placements) |
| `PARTS_GROUP` | 4,024 (58 non-empty) | Visibility group name, toggled by fixed cameras |
| `CAMERA_INFO` | 22 | Number of `FIX_CAMERA` blocks |
| `FIX_CAMERA` | 35 | Fixed camera definition: `FIX_CAMERA_POS`, `FIX_CAMERA_POS2 index,x,y,z`, `FIX_CAMERA_OFF_GROUP index,"group",int`, `FIX_CAMERA_RECT "box",min,max[,pos,rot,scale]` |
| `FUNC_POINT` | 5,933 | Trigger and function points (section 5) |

Environment settings (`mapinfo.cpp:205-568`):

- `LIGHT_SET n` blocks (up to 16) contain:
  - `FOV` (ignored by the engine; projection is always 52°, `mapinfo.cpp:277`)
  - `BGCOLOR` and `BGCOLOR2` (`0`–`255`; if `BGCOLOR2` is `0,0,0`, the engine copies `BGCOLOR`)
  - `AMBIENT` (`128` is neutral)
  - `LIGHT index,dx,dy,dz[,r,g,b]` (up to 4 directional lights)
  - `PLIGHT index,power,x,y,z,r,g,b`
  - `FOG_ENABLE` and `FOG near,far[,r,g,b[,offset,far2]]`
- `TIME_FLAG follow_clock,blend[,hour[,fixed]]`: when a map follows the game clock, the engine selects the active light set by the current hour and blends it into the next set over the final hour of the band, with light `0` tracking the sun.
- `TIME_LIGHT_NUM`: a value of `4` uses fixed time bands, whereas any other value `N` divides the 24-hour day evenly starting from 9:00. `N` must be `8` or less (`GetLightInfo` array bounds). When `TIME_FLAG` does not follow the clock, `ACTIVE_LIGHT_SET` selects the fixed light set.
- Additional setting tags: `FLOOR`, `CHARA_POS`, `DEF_FOOT`, `SKY_INFO int,float[,sun_angle_degrees]`, `LENS_FLARE`, `TIME_CFADE`, `ALL_SCISSOR`, and `CHARA_LIGHT_ADJUST`.
- The engine also recognizes `LOD_START`, `LOD_BLEND`, `LOD_PIECE`, `LOD_END`, and `_MAP_INFO_TOP`, which no file on the disc uses.

### 2.3 Writing a .map back byte for byte

A serializer that stores only parsed values (without retaining the original source text) can regenerate all 210 retail `.map` files byte-for-byte by following these eight rules:

1. Encode text in Shift-JIS (`cp932`); 16 files contain Japanese strings (such as `"門"`).
2. Use `LF` line endings, include a trailing `LF`, emit no trailing spaces, and emit no comments.
3. Indent one tab per nesting level, except inside `LIGHT_SET` bodies (which use one space) and on `LIGHT_FLAG` lines inside the 136 `PARTS` blocks that lack `MOVE_FLAG` (which use no indent, produced by an older exporter).
4. Separate arguments with `,`, except in four tags that insert a space at fixed positions:
   - `FUNC_FLAG a, b, c,d`
   - `FUNC_POS p,p,p, r,r,r, s,s,s`
   - `FUNC_INVENT_DATA id, ...`
   - `FIX_CAMERA_RECT` (after the name and between coordinate groups)
5. Emit the exact number of blank lines after each section (`2`, `1`, `2`, `2`, `3`; 10 blank lines in total per file), even when a section is empty.
6. Format floating-point numbers as C `%g` (6 significant digits) with MSVC's 3-digit exponents (`e-007`), format whole numbers without a decimal point, and format negative zero as `0`. Do not write non-zero values that the engine must preserve using scientific notation, because the parser reads tokens containing `e` as `0`.
7. Track optional tags and variable argument counts per entry: `MOVE_FLAG`, `PIECE_COL_TYPE` (1 or 2 arguments), `FAR_CLIP`, `MAP_FAR_CLIP`, `TIME_FLAG` (2 to 4 arguments), `FOG` (5 or 7 arguments), `SKY_INFO` (2 or 3 arguments), `FUNC_FIRE_DATA` (4 or 6 arguments), `FUNC_ANIME_DATA` (14 or 16 arguments), `LENS_FLARE` (omitted in 9 files), `TIME_CFADE`, `ALL_SCISSOR`, and `CHARA_LIGHT_ADJUST` (omitted in 12 files).
8. Compute the counts in `FUNC_POINT n`, `CAMERA_INFO n`, and `PIECE_MATERIAL_START n` directly from the number of child entries, as they always match.

Retail anomalies to preserve when round-tripping: 10 files repeat a `PARTS` name (the engine uses the first definition), 2 files repeat a `MAP_PARTS` name, 2 files have gaps in `LIGHT_SET` indices, 532 pieces specify an empty model name `""`, and about 490 pieces specify a model name that does not exist in any of the map's loaded packs (the engine skips drawing missing models).

## 3. Placement: templates, placements, pieces

`PARTS` builds a template in memory (`mapPARTS`, `mapload.cpp:538`). `MAP_PARTS_END` calls `PlaceParts(name, pos, rot, scale)` (`mapload.cpp:970`, `map.cpp:313`), which copies the matching template into the next free placement slot and applies the placement's name, `show` flag, far clip, and group. If the template name does not exist, `PlaceParts` ignores the placement without consuming a slot. Across the disc, 176 maps place their templates in the `.map` file; the other 34 maps contain only templates (dungeon floor libraries, add maps, and Georama `g00`) that the engine places at runtime.

**Transforms.** Each vertex's world transform is `placement × piece × node_matrices`. A model node's local matrix is `T · Rz · Ry · Rx · S` (`mgCFrame::GetLocalMatrix`, `mg_frame.cpp:753`), using right-handed rotations (`mg_math.cpp:770-790`) in radians (matching Blender's `XYZ` Euler order). The `+Y` axis points up (gravity acts along `-Y`). Scales of `-1, 1, 1` (732 pieces, used for mirroring) and `20, 20, 20` (1,005 pieces) occur on the disc. A standard dungeon grid cell is 320 world units across.

**Models.** `CMap::SearchMDS` (`map.cpp:245`, `mdslist.cpp:212`) searches for a piece's model by name (case-insensitive) across every loaded model pack in the map and its add map. On the disc, 509 pieces reference a model from another map's pack, and maps frequently place the same mesh multiple times. A model pack's `info.cfg` starts with `PCP_NAME` and lists `MDS "name","source"; TYPE t; [FAR_CLIP]; MDS_END;`. The model list size equals the pack's total `.mds` and `.chr` count, up to a maximum of `0x1FF` (`CMdsList::LoadPCPFile`, `mdslist.cpp:484`).

| TYPE | engine (`pcpTYPE`, `mdslist.cpp`) | members | placed pieces |
|---|---|---|---|
| 0 | drawn model (`mgLoadMDSFile`) | `-m` 7,646, other 240 | 37,318 |
| 1 | floor and wall collision (`LoadCollisionFile`) | `-a` 2,673, Georama `-h` 122, `-s` 5, `-k` 3, other 36 | 13,910 |
| 2 | camera collision | `-c` 2,032, other 7 | 11,605 |
| 3, 4 | animated character (`CreateChara`), e.g. the train engine `kikan-m.chr` | `.chr` 67 | 928 |

The `-a` suffix stands for collision (*atari*, Japanese for a hit), not alpha. `DrawSub` skips odd `TYPE` values, so the engine never renders collision meshes. Georama towns also include `-h` meshes (edit-mode footprints with area kinds `1`, `2`, `3`, and `5`, partitioned by `CEditCollision::Copy`), `-s` meshes, and `-k` meshes.

**How scripts reference map objects:** Scripts look up placements by slot index (`MAP_PARTS` order, with the add map first) or by name (`GetPlaceParts`, `map.cpp:379-395`). Out of 6,304 `bind_handle_map_part` calls on the disc, 5,700 pass a placement name. Scripts look up pieces by `PIECE_NAME`, model frames by node name, and trigger points by `FUNC_NAME` with part name `""` for map-level points (`set_func_point_enabled` has 831 calls, 552 with `""`; see also `bind_handle_func_point`, `get_func_point_pos`, and `set_func_point_pos`). `set_map_part_motion` (202 calls) plays an animation on a `.chr` piece. Consequently, map edits must preserve placement, piece, node, and trigger point names.

## 4. Meshes, textures, collision

### 4.1 Drawn meshes (`-m`)

The PAL disc contains 37,621 drawn meshes with 1,833,433 triangles, encoded as triangle strips (primitive types `0x4` and `0x104`, 218k primitives) and triangle lists (types `0x3` and `0x103`, 49k primitives). Node hierarchies reach up to 18 levels deep and include 35k non-identity node matrices, none of which have negative determinant (mirroring occurs at the piece level instead). Individual meshes contain up to 4,880 vertices and 57 materials.

**Blend modes are node-name flags** (the suffix after `--`, parsed by `mgSetFrameAttr` at `mg_dataset.cpp:163`; see [models](models.md) for the full list of name flags): `aPP` selects additive blending, `aNN` selects subtractive blending, `aOF` forces opaque rendering, `aXX` sets the alpha-test threshold, `c0` disables lighting (unlit), `z0` disables depth buffer writes, and `fN` controls fog. Common flag combinations include `n1z0c0f2app-` (722 nodes, additive and unlit) and `a40aof-` (362 nodes, alpha-tested and opaque). A material with diffuse alpha below `1.0` also enables alpha blending (`0.5` in 441 materials). In 1,488 of the 7,661 `-m` files, multiple nodes share the same base name before `--` (for example, `grid54--f2appz0n1t-5_1` and `grid54--f2appz0n1t-6_1`), so only the full node name including flags uniquely identifies a node.

**Lighting** is computed dynamically from the map's light sets (section 2.2) and point lights. Vertex colors represent painted color tints and alpha fades rather than baked lighting: 13,766 meshes contain vertex colors, covering 43% of `-m` vertex corners, and most meshes use only 1 or 2 distinct vertex colors (typically neutral grays `178` and `119`, with alpha `255` or `0`).

### 4.2 Textures

The engine registers each `IMG` file named in the `.map` with the texture manager (`CMap::LoadData`, `map.cpp:1505`), and materials look up images by name across every loaded `.img` file. Across the disc, 44 maps reference more than one `IMG` file, and 9 maps reference more than one `PCP` file. Out of 3,828 `(map, texture)` references, 3,716 resolve to the map's own `.img` files and 112 (across 35 maps) resolve to shared or paired map `.img` files (such as `cmn/g00/g00.ipk.d/a01.img`). The texture `noIcon` does not exist anywhere on the disc. Consequently, a map exporter requires a search path of `.img` files.

Texture wrap or clamp behavior comes from each image's GS `CLAMP` register value (52,791 UVs lie outside `0`–`1`, confirming active texture tiling). Out of the 5,662 images across the 549 map `.img` files (547 `IM3` and 2 `IM2`), 4,802 clamp (`5`), 839 repeat (`0`), 18 have no `CLAMP` value (`IM2`), and 3 contain garbage values. In addition, 166 `#texanime` scripts across 161 map `.img` files scroll and copy texture regions. Furthermore, 848 `FUNC_ANIME_DATA` points animate pieces and frames (section 5.1), and 44 `PIECE_MATERIAL` blocks recolor materials by name.

### 4.3 Collision (`-a` floor and wall, `-c` camera)

Floor and wall collision (`-a`) comprises 10,370 meshes and 294,135 triangles. Camera collision (`-c`) comprises 3,277 meshes and 59,649 triangles. Both use the standard MDS format and are loaded by `LoadCollisionFile` (`collision.cpp`, `CreateCollisionMDT`, verified in the assembly):

- Each node becomes a `CColFrame` containing its transform matrix and an axis-aligned bounding box computed at load time. The engine does not use a BSP tree, spatial grid, or precomputed bounding volume in the file.
- The loader reads only vertex positions and triangle lists. Any triangle strip or vertex-color-flagged (`0x100`) primitive causes the node to produce no collision triangles. The loader reads triangle corners with a stride of 1 (vertex index only), so a collision primitive must use type `0x213`. Any primitive that includes normal or UV indices is misread. Every collision primitive on the disc uses `0x213`, except for 2 primitives of type `0x13` in `-h` files.
- **Per-triangle attributes are encoded in the material's diffuse color:**
  - `ground = (int)(r * 0.7 + 0.01)`
  - `foot_sound = (int)(g * 0.7 + 0.01)`
  - `area = (int)(b * 0.7 + 0.01)`
  - `ignore_mask = (int)(1 - a)`
  The files store attribute `k` as `k / 0.7` (for example, `g = 2.8571` encodes foot sound `2`, `b = 8.5714` encodes area `6`, and `a = -11` encodes mask `12`). An attribute value of `0` is stored as either `1.0` (the default white channel value) or `0.0`. Every collision primitive on the disc is type `0x213`. Collision materials are untextured except for 103 out of 14,682 materials, and their remaining material fields hold default values (ambient `1, 1, 1, 0` and specular power `50`).
  In `-a` meshes, `ground` is almost always `0`, `foot_sound` ranges from `0` to `27`, `area` is usually `6`, `9`, `5`, `1`, `7`, or `8`, and `ignore_mask` is usually `0`, `12`, `4`, `8`, or `255`. Each collision query bitwise-ANDs its query mask with the triangle's `ignore_mask` (`gameutil.cpp:1010`). Areas `1` and `7` represent water (`GetCPolyAttr`), and `foot_sound` selects the footstep sound effect.
- Collision queries test the triangles of all nodes whose bounding box overlaps the query volume, up to the caller's triangle buffer limit: `128` for monsters and objects, `512` for the camera, and `1024` for Georama edit mode. Therefore, collision meshes should remain split across several compact nodes so that overlapping node counts stay within these buffer limits.
- **The player and NPCs** check movement collisions using `MoveCheck` (`gameutil.cpp:1599`): a raycast along the velocity vector, `GetFootPoly` extending 20 units downward, wall checks via `CheckWidth` (radius `15`; a wall is any triangle with `normal.y < 0.5`), and finally water level checks. `CMapPiece::GetPoly` also requires `PIECE_COL_TYPE` to be `0` (96 pieces set `PIECE_COL_TYPE` to `1`, which hides them from collision queries).
- **The camera** collects camera-collision triangles within the distance to the player plus 10 units and resolves movement using `MoveCamera` (`dng_main.cpp:2165-2187`).

## 5. Trigger points (FUNC_POINT)

Trigger points defined inside a `PARTS` template are copied into each placement of that template (`world = placement × FUNC_POS`). Trigger points defined after the `MAP_PARTS` section belong directly to the map (`mapload.cpp:1147`). `FUNC_POS` stores position, rotation (in radians), and scale. For `event` points, the loader replaces any scale near `1` with a default interaction radius (`15` for doors, `25` for ladders, and `20` for chests; `mapload.cpp:1437`). The last two numbers of `FUNC_FLAG` specify the active hours (`CheckTime`, `funcpoint.cpp:19`: `start < end` defines a daytime window, `start > end` wraps past midnight, and `start == end` is always active).

| kind | count | data line | used for |
|---|---|---|---|
| `pos` | 3,614 | none | Named positions that scripts and doors look up: `point`/`pointN` (3,439, inside door and entrance templates), `kaifuku` (136, recovery spots), `npc_pos`, and `gcamp` (camps) |
| `invent` | 3,018 | `FUNC_INVENT_DATA id, min xyz, max xyz, int, float, angle_degrees` | Photography subject: taking a photo whose line of sight hits the bounding box (`min` and `max` relative to the point's frame) awards invention idea `id` (below `1000` for regular ideas; `1000` and above for scoops). The `float` sets the maximum ray distance (`400` when `0`), and `angle_degrees` restricts the camera angle relative to the point's back axis. The engine checks only points attached to drawn placements (`CMap::InScreenFunc`, `CMapParts::InScreenFunc`); every retail `invent` point is inside a placement, except for one map-level point that can never be photographed |
| `event` | 2,653 | `FUNC_EVENT_DATA` kind, event_no, program, 3 ints, target (15 chars), flags (action button, item, 0x100) | Doors (1,250), events (1,157), treasure chests `t_box` (122), `close_door`, `book`, `ed_door`, and ladders `lddr_b`/`lddr_t`. The `program` field specifies the `PROGRAM` number to run in the map script |
| `flare` | 2,631 | `FUNC_FIRE_DATA r,g,b,draw[,int,int]` | Lens flares (an all-zero color defaults to `128, 90, 38`) |
| `plight` | 2,098 | `FUNC_PLIGHT_DATA power, r,g,b (0-511), mode, 4 ints, flicker type, depth, period` | Point lights |
| `fire` | 1,636 | `FUNC_FIRE_DATA` | Fire effects |
| `effect` | 1,234 | `FUNC_EFFECT_NAME` (an effect from the map's `.efp`) | Particle and visual effects |
| `sound` | 1,129 | `FUNC_SOUND_DATA number, near, far, int, shape, start xyz, end xyz` | Ambient sound emitters |
| `anime` | 848 | `FUNC_ANIME_DATA "placement","piece","frame",drives,mode, start xyz, step xyz, end xyz[,uniform,relative]` (section 5.1) | Per-frame animation of position, rotation, scale, color, or alpha on placements, pieces, and model nodes (including clock hands) |

The strings `"talk"`, `"exit"`, and `"branch"` are point names (`FUNC_NAME`), not point kinds. A map's `.cfg` file can also add trigger points (`FUNC_DATA "event"` with `FUNC_EVENT_DATA n,"check"`).

Many runtime positions do not come from `FUNC_POINT` entries at all; scripts frequently place characters and cameras using literal coordinates (for example, `place_chara(13, 68.04, 41.85, 1180.61, 3.14)`).

**Doors.** In Georama edit/town mode, `CEditEvent` (`editevent.cpp`) handles doors when the player walks up to a door and presses the action button (flags `[1, 0, 1]` on every door on the disc; the third flag, `0x100`, fades the screen out). The engine plays the door-opening animation and sound effect, and then:

- If `program` (the third number of `FUNC_EVENT_DATA`) is greater than `0`, the engine runs that program in the map's script.
- If `program` is `0`, the engine enters `target` as an interior sub-map (using the same mechanism as `go_to_interior`: a sub-map loaded alongside the town, sharing its 320 placement slots and texture blocks), or exits the interior when `target` is `"exit"`.

### 5.1 Animated pieces (`FUNC_ANIME_DATA`)

`mapFUNC_ANIME_DATA` (`mapload.cpp:1257`) reads the `FUNC_ANIME_DATA` line into `CFuncPoint::AnimeData`. A `CObjAnime` instance per point (`include/funcpoint.hpp`, where `OBJ_ANIME_PARAM` and `OBJ_ANIME_MODE` define the enum values below) runs the animation. In ChronicleTwo, the implementation remains in assembly (`funcpoint.cpp:51-55`): `AssignFuncAnime` at `0x2A10B0`, `Step` at `0x2A05D0` (using jump table `at_475__2` at `0x373650`), `SetParam` at `0x2A0C20`, `GetParam` at `0x2A0FD0`, and `CheckOver` at `0x2A0530`. The engine never reads the `anime` point's own `FUNC_POS`.

| argument | field | meaning |
|---|---|---|
| 1 | `placement` | For a map-level point, the target placement's `MAP_PARTS` name (`CMap::AssignFuncPoint`, `map.cpp:1365`; `GetPlaceParts`, `map.cpp:379`, case-sensitive). For a point inside a `PARTS` template, the animation always targets the placement created from that template (`CMapParts::AssignFuncAnime`, `mapparts.cpp:851`, called by `Copy` during `PlaceParts`, `map.cpp:313`) and ignores this string (350 of the 524 template points name the template, and 174 name something else or `""`) |
| 2 | `piece` | The target piece within the placement, matched by `PIECE_NAME` (`SearchPiece`, `mapparts.cpp:107`, case-sensitive). `""` animates the entire placement |
| 3 | `frame` | The target node within the piece's model (`mgCFrame::SearchFrame`, `mg_frame.cpp:980`: depth-first search comparing names up to `--` via `StrCmp`, `mg_frame.cpp:948`). `""` animates the entire piece. When a node is found, the engine sets it to rotate around its own pivot (`SetRotType(2)`) |
| 4 | `drives` | The property modified by `SetParam`: `1` position, `2` rotation in degrees (Euler XYZ, clamped to `±360,000` and converted to radians), `3` scale, `4` vertex/node color (node only: forces unlit rendering via `mgCFrameAttr::no_light` and writes `color`, where `128` per channel is neutral), or `5` alpha (node only: writes `obj_alpha` from the first component, where `1` is opaque). Other values modify nothing |
| 5 | `mode` | The per-frame update mode in `Step` (see table below); other values behave like mode `0` |
| 6-8 | `start` | Initial `(x, y, z)` value |
| 9-11 | `step` | Per-frame `(x, y, z)` delta; in mode `10`, these three values represent `start_hour`, `end_hour`, and `fade_hours` |
| 12-14 | `end` | Target/limit `(x, y, z)` value |
| 15 | `uniform` | If non-zero, copies `x` into `y` and `z` for uniform scaling. Never set on the PAL disc |
| 16 | `relative` | If non-zero with `drives == 1`, transforms the position delta through the piece's own matrix (`CObject::GetMatrix`, or the placement's matrix when `piece` is `""`), so a piece with `frame == ""` advances by that vector every frame (acting as a velocity). Set on 40 lines on the disc, only 8 of which drive position (the battleship's two bobbing hulls in `p01_battleship`) |

Arguments 15 and 16 are optional (5 lines on the disc have only 14 arguments) and default to `0` (`CFuncPoint::Initialize` zeroes the structure).

Animated values are absolute in the coordinate space of the target object's parent and overwrite the object's static transform immediately. `AssignFuncAnime` applies `start` at load time, overriding a node's MDS transform, a piece's `PIECE_POS`/`PIECE_ROT`/`PIECE_SCALE`, or (from the first frame onward) a placement's `MAP_PARTS` transform. For rotations, `start` is an absolute Euler angle, not a relative offset.

| mode | name | each frame |
|---|---|---|
| 0 | hold | Writes the current value again (unused on the disc) |
| 1 | loop | Adds `step` indefinitely |
| 2 | stop | Adds `step`; once any moving axis reaches `end`, clamps and holds `end` |
| 3 | repeat | Adds `step`; once any moving axis reaches `end`, resets to `start` |
| 4 | ping-pong | Adds `step` until an axis reaches `end` (clamping to `end`), then subtracts `step` until an axis reaches `start` (clamping to `start`), repeating indefinitely |
| 5 | there and back once | Same as mode `4` for one cycle, then stops permanently (unused on the disc) |
| 6 | look at the player | Rotation only: computes the yaw (`y`) and pitch (`x`) angles from the target object toward the player (in the placement's or node's coordinate space), clamped between `start` and `end`; `z` remains unchanged |
| 7 | random | Sets each axis independently to `start + (end - start) * rand_0_to_1()` |
| 8 | minute hand | Sets `y = -360 * clock_hour` (one full rotation per game hour); `x` and `z` remain unchanged |
| 9 | hour hand | Sets `y = -30 * clock_hour` (one full rotation per 12 game hours) |
| 10 | by the hour | Evaluates to `end` during `step[0]`–`step[1]` hours (`CheckTime`), `start` outside that window, and blends linearly over `step[2]` hours before and after the window (`SubTime`, taking the shortest arc around the 24-hour clock) |

If `step` overshoots the `[start, end]` range in a single frame, the value alternates between `start` and `end` on every frame (used intentionally on nodes `a_0` to `e_3` in `a05ia`: scale `0.9` and `0.8` with step `-1`). The engine updates each animation every frame while its point is enabled and within its active hours (`CFuncPoint::Check`, `CheckTime`). Both the dungeon loop (`dng_main.cpp:1790`) and the Georama edit loop (`editloop.cpp:279`, plus edit parts at `editmapeffect.cpp:85`) call `CMap::AnimeStep` (`map.cpp:1286`) and `CMapParts::AnimeStep` (`mapparts.cpp:773`) once per frame, regardless of whether the target placement is currently visible. Across the disc, 111 `anime` points start disabled (scripts toggle them with `set_func_point_enabled`), and 4 specify active hours. A disabled point holds its last value.

Across the PAL disc, 848 `anime` points appear in 105 maps: 524 inside templates and 324 at the map level. Of these, 783 animate a model node, 52 animate a whole piece, and 13 animate a whole placement (for example, the ground in `s03`, which scrolls 50 units per frame over 1,600 units and repeats). Twenty-eight points reference a placement or piece name that is absent from the `.map` file (either placed at runtime or left over from development) and therefore animate nothing.

| drives | count | modes |
|---|---|---|
| position | 66 | ping-pong 49, repeat 16, stop 1 |
| rotation | 376 | repeat 122, loop 119, ping-pong 72, stop 27, minute hand 14, hour hand 14, look 8 |
| scale | 199 | ping-pong 162, random 33, stop 4 |
| colour | 202 | ping-pong 173, random 23, repeat 6 |
| alpha | 5 | ping-pong 4, by the hour 1 (`g03`'s `seali`, shown 20:00-6:00 fading over an hour) |

## 6. Map kinds

- **Fixed scenes** (176 files with `MAP_PARTS` placements): towns and outdoor areas (`s`, `m`, `f`, `a`, `z`), interiors, dungeon boss and event rooms (`d..b..`, `d..e..`), special dungeon floors (`d02f09`, `d04f09`, `d05f09`, `d05f10`, `d06f97`–`f99`), and the Georama base maps `g01`–`g05`.
- **Dungeon floor libraries** (28 files named `d0Nf..`): contain `PARTS` templates only. Scripts call `load_dungeon_map(map, room_cfg, flags)`, and `CAutoMapGen::Build` (`automap.cpp:595`) generates a fixed preset floor (flag `0x40`), a floor-indexed preset (`0x2`), or a random 20×16 (`0x8`) or 14×14 grid using `dungeon/cfg_file/<cfg>.cfg`. For each grid cell, the generator places one of the 276 template names hardcoded in the executable (`way00`–`51`, `room00`–`59`, `door00`–`99`, `part00`–`31`, `step00`–`31`) at `(x * 320, 0, y * 320)`, along with surrounding trees or mountains, the geostone stand `p01_gio`, decorative stones `obj01`, and pots `obj02`. See [dungeons](dungeons.md) for the room script format and generator algorithm.
- **Add maps** (5 files under `map/cmn`): shared template libraries that other maps load and place (the train, `trn2`/`s36`, `battleship01`/`02`, and `g00`, which holds the 52 Georama building parts).
- **Georama towns**: the base map is a fixed scene (`g0N.map` loaded together with `g00`'s templates). The file `g0N.cfg` defines fixed parts (`FIX_EPARTS`), initial placements (`INIT_EPARTS id,x,y,z,angle`), and river pieces (`EDIT_RIVER`). The files `g0N[0-6].gpi` define the editable Georama parts. The save file stores the player's placed town layout (`CEditData`: up to 300 parts, with rotation angles in 15° increments).

A map's `.cfg` file (`CMap::LoadCfgFile`) can add several features:

- Water surfaces (`WATER_VERTEX` grid between two corners, placed at `WATER_POS`)
- Occlusion planes (four corner points)
- Draw-culling volumes (`DRAW_OFF_RECT`: parts inside the second bounding box are hidden whenever the camera is inside the first bounding box)
- Extra `FUNC_DATA "event"` trigger points
- Villager spawn positions (`NPC_PLACE` and `NPC_POS x, y, z, yaw`)

When `SKY_INFO` enables the sky, `CMapSky::DrawSky` renders the sky around the camera using the map's `.sky` archive (or `map/def.sky` if the map has no `.sky` file). For each time band, it draws the sky and background sky models once normally above the horizon and once mirrored below the horizon.

## 7. What the engine needs of a new map

- **A row in the map table.** At boot, `LoadMapName` (`mapselect.cpp`) parses `map/map<language>.cfg` into `MapNameBuff` (`0x8000` bytes). `MAP_NAME_NUM n` allocates the table (`n + 1` rows of `0x1C` bytes each), and the string data follows immediately after the table. Each `MAP_NAME` statement populates the next row without bounds checking, and `GetMapNameInfo` looks up only rows with index `< n`. Therefore, adding a new map requires appending a `MAP_NAME` row at the end and incrementing `n`, which preserves the indices of all existing maps stored in save files (`map_no`) and scripts. The retail tables use 9,248 to 13,808 bytes (Korean), leaving room in `MapNameBuff` for hundreds of new rows. Each row contains:
  - `type` (`MapType` in `mapselect.hpp`, read by the edit loop and menus):
    - `1`: Georama town with edit data
    - `2`: Georama town interior, which runs `g00`'s event script (`GotoInterior`, `mapjump.cpp:524`)
    - `3`: Standard town or outdoor area
    - `4`: Standard interior
    - `5`: Future map (`f01` to `f04`: disables the time board and world map, and uses its own area number)
    - `6`: Future map interior
    - `7`, `8`: Dungeon loop maps
  - `sel_type`: Category in the debug map select menu
  - `snd_id`: Sound set and default music ID (`GetMapSndDataID`)
  - `area_no`: Save file area ID for the last visited main map
  - `add_path`: Optional add map loaded alongside the map
  All seven language tables (`map0.cfg`–`map6.cfg`) must assign the same row index to the map.
- **Map files at `GetMapPath(name)`** (section 1): the `.map` file and, when present, the `.cfg`, `.mpk`, `.ipk`, `.efp`, and `.sky` files of the same base name (`GetLoadMapInfo`, `mapjump.cpp:147`). The engine looks up pack members by the names listed in the `.map` file's `IMG` and `PCP` lines, so a duplicated map can reuse another map's internal member names (such as `e05.img` or `m05_0.pcp`). The map's script path is `<path>/<name>_<language>.stb`, falling back to `<name>.stb` (`LoadMapScript`, `LoadScript`); a map without a script loads with no events. Each new top-level file requires a record in `DATA.HD4` (12 bytes plus its null-terminated path; about 7.6 KB is free on the PAL disc, enough for 25 to 35 new maps of 4 to 6 files each). Engine filename buffers are 32 bytes long.
- **An entry transition.** Scripts enter Georama towns with `go_to_edit_mode(number, program)`, standard maps with `jump_to_map(point, name_or_number, program)` (a door is an `event` point whose script program calls `jump_to_map`), interiors with `go_to_interior(point, name)` (a sub-map sharing the town's 320 placement slots), and dungeon maps with `load_dungeon_map(name)`. The specified `program` runs when the map finishes loading.
- **Save flags.** The save file tracks per-map flags (such as opened treasure chests) for map numbers below `256` (`SAVE_MAP_FLAG_MAX`). The retail disc uses 189 map numbers, leaving 67 slots that persist map flags; maps numbered `256` or higher load normally but do not save per-map flags.
- **Memory within engine budgets** (see [memory](memory.md)).

**Map script loading.** `LoadMapScript` (`mapjump.cpp`) loads `GetMapPath(name)` + `.stb` via `LoadScript`, which first tries `<path>_<LanguageCode>.stb` and then falls back to `<path>.stb` (for example, `map/m/m05/m05x_1.stb` for English, or `map/m/m05/m05x.stb` if the language-specific file does not exist). Unnumbered retail scripts are compiled with Japanese strings. When the map loads, the engine runs `map_start` (`100`) unless the transition call specifies a different program (`jump_to_map(point, map, program)` or `go_to_edit_mode(number, program)`). See [running](running.md) for how script programs execute.
