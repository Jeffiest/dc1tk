# Memory budgets

Dark Chronicle loads maps, character models, textures, event scripts, and sound banks into fixed-size memory buffers and stacks. This page documents each runtime memory budget, where the engine configures it, the largest asset allocation on the PAL disc, and the remaining headroom.
- For how `dc2tk build` checks modified files against these budgets, see [staying within memory](../guide/memory.md).
- For record-count limits in gameplay data tables, see [game data](data.md).

All numbers on this page come from static analysis of the ChronicleTwo decompilation (`ps2/src/` and assembly for unmatched functions) and exact measurements across the extracted PAL disc (`SCES_511.90`).
- **Certain** means the buffer size is a hardcoded engine constant and the asset usage is computed exactly.
- **Estimated** indicates whether the buffer bound or the per-asset allocation formula is approximated.

The engine's allocator (`mgCMemory`) measures stack capacities and allocations in 16-byte **quadwords**. Because `mgCMemory::Alloc` rejects any allocation that reaches the final quadword of a stack (`used + n >= size`), a stack configured with `n` quadwords holds at most `n - 1` quadwords (`(n - 1) * 16` bytes).

## Summary

| Memory Budget | Capacity (Bytes / Units) | Largest Use on the PAL Disc | Remaining Headroom | Status |
|---|---|---|---|---|
| Global file read buffer (single file loaded whole) | `3,200,000` bytes | `e612_griffon.chr` (`3,197,312` bytes) | `2,688` bytes | Certain |
| Town map files in read buffer (with loading screen packets) | `3,120,000` bytes | `s48` (`3,021,376` bytes) | `98,624` bytes | Certain |
| Dungeon map files in read buffer (with loading screen packets) | `3,040,000` bytes | `d05f02` (`2,744,384` bytes) | `295,616` bytes | Certain |
| Map event script buffer (`.stb`) | `312,816` bytes | `map/d/d05/d05_5.stb` (`309,664` bytes) | `3,152` bytes | Certain |
| Map part placements (town map / interior) | `320` placements | `m08` (`230`) / `m05` (`245`) | `90` / `75` | Certain |
| Map part placements (dungeon floor) | `400` placements | `d06b02` (`102`) | `298` | Certain |
| Texture blocks for a dungeon map | `15` blocks | `d06f02`, `d06f03` (`14` blocks) | `1` block | Certain |
| Texture blocks for an interior | `40` blocks | `i04h01a` (`16` blocks) | `24` blocks | Certain |
| Texture blocks for a town map (alone / alongside an interior) | `64` / `24` blocks | `s53` (`32` blocks, loaded alone) | `32` blocks | Certain |
| GS VRAM per texture block | `4,768` GS blocks + Z-buffer cache | `s01` (fullest block leaves `12` GS blocks) | `12` GS blocks | Certain |
| Max base model pack (dungeon party stack 0) | `1,096,000` bytes | `c01_base3` (~`1,080,672` bytes) | ~`15,000` bytes | Estimated use |
| Melee weapon, ranged weapon, hat (party stacks 2, 3, 4) | `112,000`; `74,400`; `48,000` bytes | `ws` (`104,496`); `wg` (`71,968`); `ha` (`47,792`) | `7,504`; `2,432`; `208` bytes | Estimated use |
| Single VU1-skinned mesh | `3,198` vertices, `32` bones | | | Certain |
| Scene map heap (town / interior / dungeon) | Shared scene pool | `g01` (`9.47` MB) / `i04h01a` (`3.75` MB) / `d02f01` (`6.91` MB) | Scene-dependent | Estimated |
| SPU2 music region (`BG_*`) | `0x78000` bytes | `BG_056` (`0x77E80` bytes) | `384` bytes | Certain |
| SPU2 field region (field, event, and monster banks combined) | `0x10DC10` bytes | `s01` set `1600` + `EV_100_155` (`0x10E180` bytes) | `-1,392` bytes (overflows) | Certain size, script-traced use |
| SPU2 system sound region (`SY_000`) | `0x1D4C0` bytes | `SY_000` (`0x1C6F0` bytes) | `3,536` bytes | Certain |
| SPU2 menu bank (`SP_*`) / player character bank (`CH_*`) | `0x30D40` / `0x19020` bytes | `SP_009` (`0x30B30`) / `CH_047` (`0x18190`) | `528` / `3,728` bytes | Estimated upper bound |

## 1. The EE memory map

At boot, `MainLoop` (`src/mainloop.cpp`, assembly `0x192298`) allocates a single master heap, `MainBuffer`, of `0x1A0000` quadwords (`27,262,976` bytes) starting at address `0x40B100`. Each top-level game mode partitions `MainBuffer` on entry:

- **Town and field mode (`EditInit` in `src/editloop.cpp`, assembly `0x1AB320`)**:
  Allocates rendering packet buffers (`2 × 10,000` and `2 × 35,000` quadwords), the event script stack `ScriptBuffer__2` (`20,000` quadwords), and `TotalDataBuff`, which receives everything except the final `0x334D0` quadwords. That final `0x334D0`-quadword tail holds `MenuBuffer__2` and `read_buffer` (which share the same `200,000` quadwords) plus `WorkBuffer` (`10,000` quadwords).
  `TotalDataBuff` spans approximately `22,140,928` bytes. The engine first allocates common town assets (cursor, treasure chests, effects, and UI textures) and the texture manager tables (`SetTableBuffer(0x15E, 0xDD)`) from `TotalDataBuff`. `CScene::AssignStack` (`src/scene.cpp`, assembly) then chains the scene stacks so that each stack receives the unused remainder of the preceding stack:

  | Scene Stack | Contents |
  |---|---|
  | `0` (`ControlCharaBuff`) | Active party characters (`GetCharaMemAllocSize()` quadwords, subdivided by `mem_table`; see section 4). |
  | `1` (`MainDataBuff`) | Main outdoor town or field map (`EditMapJump`, `src/editloop.cpp:438`). |
  | `2` (`MainCharaBuff`) | Townspeople (NPCs) placed on the main map. |
  | `3` (`SubDataBuff`) | Interior sub-map or secondary Palm Brinks district (`LoadSubMap`, `src/mapjump.cpp:207`). |
  | `4` (`SubCharaBuff`) | Townspeople placed inside the sub-map. |
  | `5..8` (`EventBuff`) | Event character and object stacks loaded by scripts (`load_chara(MEMORY_STACK_EVENT, ...)`). |

- **Dungeon mode (`memoryInit`, `src/dng_main.cpp:240`)**:
  Allocates rendering packet buffers (`2 × 10,000`, `2 × 20,000`, and `2 × 60,000` quadwords), `WorkBuffer` (`10,000`), texture manager tables (`SetTableBuffer(0x140, 0xAF)`), party memory (`140,016` quadwords), the script stack `BuffScriptData` (`20,000`), the effect heap (`40,000`), and `BuffReadData` (`200,000`). These fixed allocations consume `592,757` quadwords, leaving approximately `17.8` MB. `InitDungeonMain` loads common dungeon models and effects into that remaining pool and assigns the rest to stack `1` (`BuffStageMain`, `src/dng_main.cpp:670`), which holds the floor map, followed by the stacks for floor monsters, NPCs, and events.

Because stacks `1` through `8` chain dynamically inside the remaining pool, the exact free memory available to a map depends on the other assets loaded in the same scene. See section 3 for the largest map footprints on the disc.

## 2. The read buffer and the script buffer

- **Read buffer (`read_buffer`, certain)**:
  Every file that the engine reads into memory as a single unit passes through `read_buffer`, which measures **`0x30D40` quadwords (`3,200,000` bytes)**:
  - In town mode, `EditInit` sets `read_buffer_end = read_buffer + 0x30D400`.
  - In dungeon mode, `dng_main.cpp:286` allocates `BuffReadData` (`200,000` quadwords), and `EntryEventScript` aliases `read_buffer` to it.
  Character `.chr` packs (`GetLoadBGBuff` in `src/event_func.cpp:2383` and party loaders in `src/charasetup.cpp`), `.snd` sound banks, and map files all stage through `read_buffer`. The largest single file loaded by the game is `event/e600/e612/e612_griffon.chr` at **`3,197,312` bytes**, leaving **`2,688` bytes** of headroom. (The 6.2 MB file `e612_gri_kao.chr` on the disc is an unused leftover never referenced by any script.)

  **Map staging in `read_buffer`**: `LoadMapData` (`src/sceneload.cpp`, assembly) sequentially reads the `.map`, `.cfg`, `.mpk`, and `.sky` files for both the primary map and its common add-map (`map/cmn/`, specified in the map table) into `read_buffer`, rounding each file up to a 64-byte boundary. Any town or dungeon map without its own `.sky` file loads `map/def.sky` (`276,912` bytes). During the initial map load of a session, the loading screen's rendering packets occupy the top of `read_buffer`:
  - In town mode, loading screen packets start at `read_buffer + 0x2F9B80` (`0x1388` quadwords), capping the combined map files at **`3,120,000` bytes** (largest on the disc: `s48` at `3,021,376` bytes, leaving `98,624` bytes).
  - In dungeon mode, loading screen packets start at `3,040,000` bytes (`src/dng_main.cpp:291`), capping the combined dungeon map files at **`3,040,000` bytes** (largest on the disc: `d05f02` at `2,744,384` bytes, leaving `295,616` bytes).

- **Script buffer (`ScriptBuffer__2` / `BuffScriptData`, certain)**:
  A map's `.stb` event script loads into a **`20,000`-quadword (`320,000`-byte)** stack (`ScriptBuffer__2` in town, `BuffScriptData` in dungeons). `SetEventScript` (`src/event.cpp:106`) first allocates `0x40 + 0x180` quadwords (`7,168` bytes) from this stack for interpreter state tables, leaving **`312,816` bytes** for the `.stb` file itself. The largest map script on the disc is `map/d/d05/d05_5.stb` (Moon Flower Palace) at **`309,664` bytes**, leaving only **`3,152` bytes** of headroom. (Note: the large dungeon system scripts in `dungeon/sys_script/`, which reach `361,160` bytes, load into a separate buffer.)

## 3. Maps

- **Part placements (certain)**:
  Before parsing map placements, `SetPlacePartsBuff` (`src/map.cpp:224`) allocates a fixed placement array:
  - **Town maps and interiors**: `0x140` (**`320`**) placements (`GetLoadMapInfo`, `LoadSubMap`). Largest on the disc: `m08` (`230`) and `m05` (`245`).
  - **Dungeon maps**: **`400`** placements (`LoadDungeonMapFile` in `src/dng_event.cpp`). Largest on the disc: `d06b02` (`102`).
  Placements from the common add-map count toward this total. Any placement past the array limit is ignored (`NewPlaceParts` returns `NULL`).

- **Map table (`map/map<language>.cfg`, certain)**:
  Loaded into `MapNameBuff` (**`0x8000` bytes** / 32,768 bytes, `src/mapselect.cpp`). Each row consumes `0x1C` bytes (plus one sentinel row), followed by the row string pool, with each section padded to 16 bytes. Retail files use between `9,248` bytes (Japanese) and `13,808` bytes (Korean).

- **Texture block count (certain)**:
  `CMap::LoadData` (`src/map.cpp:1506`) loads the add-map's `.img` archives followed by the primary map's `.img` archives starting at texture block `0`, advancing the block index after each archive:
  - In **town mode**, interior sub-map textures start at block `0x18` (`24`), and main map effects start at block `0x40` (`64`). Therefore, a town map has **`24` texture blocks** when an interior or Palm Brinks sub-map loads alongside it, or **`64` texture blocks** when no interior loads with it (`s53` uses 32 blocks, `s10` uses 31, and `s09` uses 27, as none of those three maps loads an interior). An interior sub-map has **`40` texture blocks** (`0x18..0x3F`; largest on the disc is `i04h01a` at 16 blocks).
  - In **dungeon mode**, map effects start at block `0x0F` (`15`) and party textures at `0x10` (`16`), giving a dungeon map **`15` texture blocks** (`0x00..0x0E`; largest on the disc are `d06f02` and `d06f03` at 14 blocks).

- **Scene stack footprint (estimated)**:
  When `CScene::LoadMapFromMemory` (`src/sceneload.cpp`) constructs a map on its scene stack, it allocates:
  - Full copies of the `.ipk` and `.efp` archives (`LoadMapData` aligns each to 64 bytes and copies `size` bytes).
  - Each `.mds` model in the `.pcp` model pack according to its `TYPE` in `info.cfg` (`CMdsList::LoadPCPFile`, `src/mdslist.cpp:484`):
    - `TYPE 0` (Static visual model, `mgLoadMDSFile` in `src/mg_dataset.cpp:502`): `0x110` bytes per frame, frame name, attribute (`0x90`), bounding volume (`0xB0`), `0x50`-byte `mgCVisualFixMDT`, `0x30` bytes per material, and per primitive an `mgCFace`, face group, and the compiled VU1 packet from `CreateFacePacket` (2 header quadwords, 1 end tag, and for each batch of 57 uncoloured or 42 coloured vertices, 4 quadwords plus 1 quadword per vertex attribute, repeating 2 vertices when splitting a strip).
    - `TYPE 1` and `2` (Collision mesh, `LoadCollisionFile` in `src/collision.cpp:387`): `0x120` bytes per frame and `0x50` bytes per triangle (strips and coloured primitives are ignored).
    - `TYPE 3` and `4` (Animated character model; see section 4).
  - Sky textures and models (`CMapSky::LoadPack` in `src/mapsky.cpp`).
  - The placement array, part templates, and per-placement copies of visual pieces and material tables (`CMapParts::Copy`, `src/mapparts.cpp:803`: `0xD0` bytes per piece plus its material array).

  This estimate omits small allocations (`.map` settings, `.cfg` water definitions, effect scripts, and function points). A map's loading category is determined by its row in `map/map<language>.cfg` (note that the unused legacy file `map/map.cfg` lists different types for some maps): types `7` and `8` load in the dungeon loop; interiors (types `2`, `4`, and `6`) and Palm Brinks districts `m02`–`m05` (maps `11..14`, loaded on top of `m01` by `EditMapJump` in `src/editloop.cpp:2735`) load into `SubDataBuff`; all other maps load into `MainDataBuff`. The largest estimated map footprints on the disc are:
  - Town main map: `g01` at **`9,471,152` bytes** (`9.47` MB)
  - Interior sub-map: `i04h01a` at **`3,752,880` bytes** (`3.75` MB)
  - Dungeon map: `d02f01` at **`6,911,504` bytes** (`6.91` MB)

## 4. Character packs

When `CCharacter2::LoadPack` (`ScanInfoFile`, `src/character.cpp:1286`) loads a `.chr` pack into its target stack, it allocates:
- A 64-byte-aligned copy of each `IMG` texture archive (`stAlloc64(size / 16 + 1)`).
- The main `MODEL` (`.mds`), with `VERTEX_ANIME` frames constructed as `mgCVisualMotionMDT` instances (`0x110` bytes each, plus VU1 packets carrying 2 weight quadwords per vertex in batches of `((180 - 2 * bones - 2) / 5 / 3) * 3` vertices; `src/visualmotion.cpp`).
- The `SHADOW_MODEL` with copied vertex and index arrays.
- Each `MOTION` set's `.bbp` bind-pose matrix array and converted `.mot` keyframe lists (`CreateAnimeDataEX` in `src/gameutil.cpp:807`: `0x20` bytes per track list, `16` bytes per key, and `4` bytes per keyframe number).
- `KEY` clip tables (`0x30` bytes per clip), toon outlines, sound cue tables, and `.clo` cloth simulators.

- **Party character stacks (certain capacities, estimated pack footprints)**:
  `GetCharaMemAllocPtr` (`src/charasetup.cpp:248`) subdivides the active party heap using `mem_table` (`0x351D50`), where all sizes are in **16-byte quadwords**:

  | `mem_table` Row | Stack `0` (Base/Motions) | Stack `1` (Costume) | Stack `2` (Melee Weapon) | Stack `3` (Ranged Weapon) | Stack `4` (Hat/Accessory) | Stack `5` (Shoes) | Stack `6` (Action Script) |
  |---|---|---|---|---|---|---|---|
  | `0` (Max / Monica in dungeons) | `68,500` (`1,096,000` B) | `40,000` (`640,000` B) | `7,000` (`112,000` B) | `4,650` (`74,400` B) | `3,000` (`48,000` B) | `9,900` (`158,400` B) | `4,500` (`72,000` B) |
  | `1` (Ridepod) | `30,000` (`480,000` B) | `30,000` (`480,000` B) | `60,000` (`960,000` B) | `10,000` (`160,000` B) | `10,000` (`160,000` B) | — | — |
  | `2` (Max / Monica in town) | `75,000` (`1,200,000` B) | `40,000` (`640,000` B) | `7,000` (`112,000` B) | `4,650` (`74,400` B) | `3,000` (`48,000` B) | `9,900` (`158,400` B) | — |
  | `3` (Monica Monster Badge form) | `120,000` (`1,920,000` B) | — | — | — | — | `5,500` (`88,000` B) | — |

  `SetupMainUnit` (`src/charasetup.cpp:100`) loads the base skeleton and motion pack into Stack `0` (`mainchr/c01_baseN.chr` with `N = weapon_model_no + 2` for Max and `mainchr/c02_baseN.chr` for Monica in dungeons; `chara/c01_base.chr` and `chara/c02_base.chr` in town), the body costume into Stack `1` and shoes into Stack `5` via `LoadSkin`, the primary weapon into Stack `2`, the secondary weapon into Stack `3`, the hat into Stack `4`, and the combat script into Stack `6`.

  Comparing the estimated pack footprints against the dungeon stack capacities shows how tightly these stacks are sized:
  - **Stack `0` (`1,096,000` bytes)**: Max's active dungeon packs (`c01_base2..4`) use `1,045,472` to `1,080,672` bytes (leaving ~`15,000` bytes; the unused `c01_base0` and `c01_base1` packs estimate at `1,105,632` and `1,080,704` bytes). Monica's dungeon packs peak at `958,576` bytes. In town (`1,200,000` bytes), base packs use up to `664,976` bytes. Because base packs contain only 3-vertex placeholder triangles, Stack `0` is almost entirely `.mot` animation data.
  - **Stack `1` (`640,000` bytes)**: Costumes estimate up to `641,296` bytes under `LoadPack` rules (in practice `LoadSkin` allocates slightly less than `LoadPack`).
  - **Stack `2` (`112,000` bytes)**: Swords/wrenches (`ws*`) peak at `104,496` bytes.
  - **Stack `3` (`74,400` bytes)**: Guns/armbands (`wg*`) peak at `71,968` bytes.
  - **Stack `4` (`48,000` bytes)**: Hats (`ha*`) peak at `47,792` bytes.
  - **Stack `5` (`158,400` bytes)**: Shoes peak at `157,616` bytes.

- **Single skinned mesh limits (certain)**:
  `_MODEL` (`src/character.cpp:1582`) builds the temporary vertex weight table (32 bytes per vertex plus 2 header quadwords) inside a fixed `6,400`-quadword (`102,400`-byte) work buffer. Consequently, any single mesh listed in `VERTEX_ANIME` can have **at most `3,198` vertices**. In addition, `CreateVertexWeight` supports **at most `32` bones per mesh** and drops skinning if a mesh exceeds 32 bones.

Non-party characters (townspeople, cutscene actors, and monsters) load into the shared scene stacks (see section 1).

## 5. Texture memory (GS)

- **GS VRAM per texture block (`4,768` GS blocks, certain)**:
  `mgCTextureManager` groups textures into numbered logical blocks. Every logical block maps to the same physical GS VRAM region and is DMA-uploaded before drawing geometry that uses that block. The region spans from `vram_top = 0x2D20` (`GetVramTopAddress` in `src/mainloop.cpp:101`: `0x2D00` at the end of the 512×480 32-bit frame buffers and Z-buffer configured by `mgInit(2, 3)`, plus `0x20`) up to `vram_fix = 0x3FE0` (`Initialize`) minus the 32 GS blocks reserved at the top for the photography work texture in `LoadTakePhoto` (`src/photo.cpp:61`). This leaves **`4,768` GS blocks** (of 256 bytes each = `1,220,608` bytes).

  `GetRemainVRAM` (`src/mg_texture.cpp:452`) rounds each texture's allocation up to whole 32-block GS pages (`8,192` bytes) and adds 4 GS blocks (`1,024` bytes) for each CLUT palette. Each mipmap level requires `bpp * width * height / 2048` GS blocks (`24`-bit counts as `32`-bit, and each lower mip level takes one-quarter of the previous level). Single-level 8-bit textures with `width * height <= 65,536` pixels are unswizzled in place at load time (`Conv32To8`) and cached inside unused Z-buffer VRAM at 4× their size while the `4,096`-unit Z-buffer pool lasts (`GetZBufVram`, `CheckCopyToZBufVram`).

  When `EnterIMGFile` (`src/mg_texture.cpp:753`) loads an `IM3` archive, it sorts the picture entries by `entry.block` (using an unstable exchange sort) and places each picture into `base_block + entry.block + spill`. Whenever a logical block fills up (`GetRemainVRAM < 0`), the manager prints `"texture over"`, increments `spill`, and shifts that texture and all subsequent groups in the archive into the next logical block. `CMap::LoadData` advances the next map `.img` file's base block past any spilled blocks, whereas other loaders ignore the return value (allowing a spill to overwrite the next owner's block).

- **Logical texture block assignments**:
  - **Town mode (`SetTableBuffer(0x15E, 0xDD)` — 350 texture slots, 221 blocks)**:
    - `0x00..0x17`: Main map and its common add-map (`0x00..0x3F` when no interior loads)
    - `0x18..0x3F`: Interior sub-map
    - `0x40` / `0x41`: Main map effects / Interior effects
    - `0x46..0x4D`: Active party (`8` blocks)
    - `0x4E..0x5D`: Main map townspeople
    - `0x5E..0x65`: Interior townspeople
    - `0x66..0x85`: Event characters
    - `0x86..`: Menus
    - `0x9A`: Font texture
    - `0xA9..0xAC`: Sky (`4` blocks)
  - **Dungeon mode (`SetTableBuffer(0x140, 0xAF)` — 320 texture slots, 175 blocks)**:
    - `0x00..0x0E`: Dungeon floor map and add-map (`15` blocks)
    - `0x0F`: Dungeon floor effects
    - `0x10..0x17`: Active party (`8` blocks)
    - `0x18..0x27`: Townspeople and event characters
    - `0x28..0x33`: Floor monsters (`1` block per monster species)
    - `0x4C..0x4F`: Sky (`4` blocks)

  Each of a sky's 4 `.img` archives loads into its own dedicated block (which is cleared before loading), so each sky `.img` must fit within a single block. Similarly, a character or monster pack must fit within its single assigned block (or 8 blocks for the party). Across the PAL disc, every sky archive and character pack fits its allocation (except `e272_jyuraku.chr` and `s17.efp`, whose explicit group numbers span 7 blocks). Maps `f04`, `s01`, `s28`, `s29` (twice), `s30`, `s51`, `s53`, `s55`, and `s67` trigger internal block spills within their map block range; the fullest block on the disc (`s01`) leaves just **12 GS blocks** of headroom.

## 6. Sound memory (SPU2)

Each loaded `.snd` bank copies its `.bd` ADPCM sample body into the 2 MB SPU2 RAM, consuming `bd_size + 16` bytes (`sndLoadSound` -> `LoadHdBd` / `LoadHdBdAdd` at assembly `0x18B9F8` advances the next bank's pointer by an extra 16 bytes). `CSound::Init` (`src/sound.cpp:271-283`) configures the base SPU2 addresses, and `GetCSndPortNo` maps game ports to IOP driver ports:

| SPU2 Region | Base Address and Direction | Upper / Lower Bound | Capacity (Bytes) | Largest Use on the Disc | Status |
|---|---|---|---|---|---|
| Music (`BG_*`, game ports `0` and `11`) | `0x005210` upward | Base of field region (`0x07D210`) | `0x78000` (`491,520`) | `BG_056` (`0x77E80`) | Certain (`384` B headroom) |
| Field (`BS`, `FG`, `SR`, up to 16 `OB`, and 1 `EV`) | `0x07D210` upward | Meets monster banks growing down from `0x18AE20` | `0x10DC10` (`1,104,912`) combined | `s01` set `1600` + `EV_100_155` (`0x10E180`) | Certain size (`-1,392` B overflow) |
| Monsters (`EN_*`, game port `5`) | `0x18AE20` downward | Top of field banks | Shares `0x10DC10` pool | Up to 6 banks (`0x2B2D0` max on a floor) | Certain |
| System (`SY_000`, game port `6`) | `0x18AE20` upward | Base of menu region (`0x1A82E0`) | `0x1D4C0` (`120,000`) | `SY_000` (`0x1C6F0`) | Certain (`3,536` B headroom) |
| Menu (`SP_*`, one bank at a time) | `0x1A82E0` upward | Core 1 reverb work area (`0x1D9020`) | `0x30D40` (`200,000`) | `SP_009` (`0x30B30`) | Estimated bound (`528` B headroom) |
| Player character (`CH_*`, one bank at a time) | `0x1E0000` upward | Core 0 reverb work area (`0x1F9020`) | `0x19020` (`102,432`) | `CH_047` (`0x18190`) | Estimated bound (`3,728` B headroom) |

The SPU2 hardware reverb work areas end at `0x1DFFFF` (Core 1) and `0x1FFFFF` (Core 0) (`src/sound.cpp:45, 73`). For reverb mode `4` (`Hall`, the only reverb mode used on the disc), Sony's `libsd` table reserves `0x6FE0` bytes per core, placing the reverb work area bases at `0x1D9020` and `0x1F9020`. Note that `0x1D9020 - 0x1A82E0 = 0x30D40` (`200,000` bytes exactly), confirming the menu region bound.

**Co-resident field, event, and monster banks**:
`CScene::LoadSound` (`src/scenesnd.cpp:365`) loads a sound set's Base (`BS`), Battle (`FG`), Ambient (`SR`), and Object (`OB`) banks in that exact stacking order. Reloading any layer resets the SPU2 start address of all later layers in the stack (`*` in a sound set keeps the currently loaded bank, and unchanged bank numbers are not reloaded). Town maps load the default sound set from their map table row, dungeon floors load theirs via `GetMapSndDataID`, and event scripts switch sets via `load_sound_set`.

Crucially, **event banks (`EV_*`) are never loaded as part of a sound set**: `CScene::LoadSound` ignores the event column, and only an explicit script call to `load_sound(set)` loads that set's event bank (`GetDefEventSeFile`) onto game port `4` at the end of the currently active field stack. This explains why sound set `1837` (whose own field banks plus `EV_100_155` would total `0x114000` bytes) never overflows on its own: set `1837`'s field banks are never loaded alongside `EV_100_155`. However, in map `s01` (`play_blackstone_railroad_arrival_scene`), the script loads `EV_100_155` (`0x83E30` bytes) on top of `s01`'s active field set `1600` (`0x8A350` bytes), reaching `0x7D210 + 0x10E180 = 0x18B390`—overflowing `0x18AE20` by **1,392 bytes** (`"SYS AREA HAKAI"`) and overwriting the first 1,392 bytes of `SY_000.snd`'s samples for the rest of the play session. The second largest script combination (`d03b02` set `3691` + `EV_115_158`) leaves `5,792` bytes of headroom. Dungeon floors leave at least `343,824` bytes of headroom (sound set `5600` plus up to 6 floor monster banks totaling `0x2B2D0` bytes).

**IOP transfer buffer window**:
The IOP stages `.bd` uploads through a `0x6DD00`-byte buffer (`src/sound.cpp:122-159`): it transfers `.bd` payloads up to `0x6DE00` bytes in a single chunk (meaning a `.bd` between `0x6DD01` and `0x6DE00` bytes would overrun the IOP buffer by up to 256 bytes) and splits payloads above `0x6DE00` into two chunks (supporting up to `0xDBA00` bytes). No bank on the disc falls in the `0x6DD01..0x6DE00` window; the largest `.bd` on the disc is `EV_130_150.snd` at `0xD2B10` bytes.

For the EE-side `sndSeInfo` and sequence table budgets of each sound port, see [the sound bank format](sounds.md#8-memory-the-game-gives-a-bank).
