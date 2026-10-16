# Memory budgets

The game loads maps, models, textures, scripts and sound banks into fixed memory. This page gives
each budget, where the engine sets it, the largest use of it on the PAL disc, and the headroom
that leaves; `build` checks the loads a mod changes against them (`dc2tk_script::memory`,
`src/crates/dc2tk-script/src/memory/`: the disc, stacks, MDS models, character packs, maps, texture
blocks, sound memory and the check, a file each).

Everything here was worked out statically from the engine's decompilation (`ps2/src`, and its
assembly where a function is not matched yet) and measured over the extracted disc; nothing was
run in an emulator. **Certain** means the size is a constant in the engine and the use is
counted exactly; **estimated** says which of the two is not.

Sizes in the engine are in quadwords (16 bytes): mgCMemory counts its stacks in them, and
`Alloc` refuses an allocation that would reach the end of a stack (`used + n >= size`), so a stack
of n quadwords holds n - 1.

## Summary

| Budget | Size | Largest use on the disc | Headroom | |
|---|---|---|---|---|
| Read buffer (a file read whole) | 3,200,000 | e612_griffon.chr 3,197,312 | 2,688 | certain |
| A town map's files in the read buffer, before the loading screen's packets | 3,120,000 | s48 3,021,376 | 98,624 | certain |
| A dungeon map's files, before the loading screen's packets | 3,040,000 | d05f02 2,744,384 | 295,616 | certain |
| A map script | 312,816 | map/d/d05/d05_5.stb 309,664 | 3,152 | certain |
| Placements of a town map / interior | 320 | m08 230 / m05 245 | 90 / 75 | certain |
| Placements of a dungeon map | 400 | d06b02 102 | 298 | certain |
| Texture blocks of a dungeon map | 15 | d06f02, d06f03 14 | 1 | certain |
| Texture blocks of an interior | 40 | i04h01a 16 | 24 | certain |
| Texture blocks of a town map: alone, beside an interior | 64, 24 | s53 32 (alone) | 32 | certain |
| GS memory of one texture block | 4,768 blocks + Z buffer | s01 (12 left) | | certain |
| Max's model pack (dungeon stack 0) | 1,096,000 | c01_base3 about 1,080,672 | about 15,000 | estimated use |
| Sword, second weapon, hat (stacks 2, 3, 4) | 112,000; 74,400; 48,000 | ws 104,496; wg 71,968; ha 47,792 | | estimated use |
| A skinned mesh | 3,198 vertices, 32 bones | | | certain |
| Map memory (town map / interior / dungeon map) | shared pool | g01 9.47 MB / i04h01a 3.75 MB / d02f01 6.91 MB | unknown | estimated |
| SPU music region | 0x78000 | BG_056 0x77E80 | 384 | certain |
| SPU field region (field, event and monster banks) | 0x10DC10 | s01's set 1600 + EV_100_155 0x10E180 | -1,392 | certain size, use from the scripts |
| SPU system sounds | 0x1D4C0 | SY_000 0x1C6F0 | 3,536 | certain |
| SPU menu bank / player character's bank | 0x30D40 / 0x19020 | SP_009 0x30B30 / CH_047 0x18190 | 528 / 3,728 | estimated end |

## 1. The EE memory map

`MainLoop` (mainloop, asm 0x192298) gives the game one stack, `MainBuffer`, of 0x1A0000 quadwords
(27,262,976 bytes) at 0x40B100. Each loop carves it up when it starts:

**Town and field** (`EditInit`, editloop, asm 0x1AB320): packets (2 x 10,000 and 2 x 35,000
quadwords), the script buffer `ScriptBuffer__2` (20,000), then `TotalDataBuff`, everything but the
last 0x334D0 quadwords, which hold `MenuBuffer__2` and `read_buffer` (the same 200,000 quadwords)
and `WorkBuffer` (10,000). `TotalDataBuff` is about 22,140,928 bytes (estimated: computed from the
sizes, the engine prints it). Into it go the town's own files (cursor, treasure box, effects,
pictures), the texture tables (`SetTableBuffer(0x15E, 0xDD)`), then the scene's stacks, each the
rest of the one before (`CScene::AssignStack`, scene, asm), so they share what is left:

| Stack | Holds |
|---|---|
| 0 `ControlCharaBuff` | the party: `GetCharaMemAllocSize()` quadwords, split by mem_table (section 4) |
| 1 `MainDataBuff` | the main map (`EditMapJump`, editloop.cpp:438) |
| 2 `MainCharaBuff` | its villagers |
| 3 `SubDataBuff` | an interior or a part of Palm Brinks (`LoadSubMap`, mapjump.cpp:207) |
| 4 `SubCharaBuff` | its villagers |
| 5 to 8 `EventBuff` | the event stacks scripts load into (`load_chara(MEMORY_STACK_EVENT, ...)`) |

**Dungeons** (`memoryInit`, dng_main.cpp:240): packets (2 x 10,000, 2 x 20,000, 2 x 60,000), work
10,000, the texture tables (`SetTableBuffer(0x140, 0xAF)`), the party (140,016), scripts
`BuffScriptData` (20,000), the effect heap (40,000) and `BuffReadData` (200,000): 592,757
quadwords, leaving about 17.8 MB. `InitDungeonMain` loads the dungeon's own models and effects
into that, then hands the rest to stack 1, `BuffStageMain` (dng_main.cpp:670), which takes the
floor's map; monsters, villagers and events take the stacks after it.

So the memory a map, its villagers, an interior and an event share is one pool whose free space
depends on the scene. No single number budgets a map: `build` compares a changed map's estimated
memory with the largest of its kind on the disc instead (section 3).

## 2. The read buffer and the script buffer

**Read buffer**, certain. A file the game loads whole goes through `read_buffer`: 0x30D40
quadwords, 3,200,000 bytes, in town (EditInit; `read_buffer_end` = `read_buffer` + 0x30D400) and
200,000 quadwords in dungeons (`BuffReadData`, dng_main.cpp:286, which `EntryEventScript` makes
`read_buffer`). Character packs (`GetLoadBGBuff`, event_func.cpp:2383; the party's packs,
charasetup.cpp), sound banks, and a map's files go through it. The largest pack the game loads,
`event/e600/e612/e612_griffon.chr`, is 3,197,312 bytes: 2,688 to spare. (`e612_gri_kao.chr`, 6.2 MB,
is never loaded: no script names it.)

A map's load (`LoadMapData`, sceneload, asm) reads the `.map`, `.cfg`, `.mpk` and `.sky` of the map
and of its add map (`map/cmn/`, from the map list) one after another into it, each rounded up to
64 bytes; a town or dungeon map without its own sky reads `map/def.sky` (276,912 bytes). The first
map a session loads in town shares the buffer with the loading screen, whose packets lie from
3,120,000 (EditInit: `read_buffer + 0x2F9B80`, 0x1388 quadwords), and in dungeons from 3,040,000
(dng_main.cpp:291). Largest: s48 3,021,376 in town, d05f02 2,744,384 in dungeons.

**Script buffer**, certain. A map's event script is read into a 20,000-quadword stack
(`ScriptBuffer__2` in town, `BuffScriptData` in dungeons), which `SetEventScript` (event.cpp:106)
then gives 0x40 + 0x180 quadwords of interpreter tables, so a script may have 312,816 bytes.
Largest: `map/d/d05/d05_5.stb` 309,664 bytes, 3,152 to spare. A mod that adds much to Moon Flower
Palace's script (or any other script near that size) runs out here first. (The dungeon system
scripts in `dungeon/sys_script/`, up to 361,160 bytes, load elsewhere.)

## 3. Maps

**Placements**, certain. `SetPlacePartsBuff` (map.cpp:224) allocates the placement table before
the parts pass: 0x140 (320) entries for town maps and interiors (`GetLoadMapInfo`, `LoadSubMap`)
and 400 for dungeons (`LoadDungeonMapFile`, dng_event). The add map's placements count too. A
placement past the end is not placed (`NewPlaceParts` returns NULL).

**The map table**, certain. `map/map<language>.cfg` goes into `MapNameBuff`
(0x8000 bytes, `mapselect.cpp`): 0x1C a row and one more, then the rows'
strings, each part taken to the next multiple of 16 bytes. The disc's
fill 9,248 (Japanese) to 13,808 bytes (Korean) (`dc2tk_script::maptable::buffer_use`); `build` refuses
new maps' rows past the buffer ([maps](maps.md) section 9).

**Texture blocks**, certain (section 5). The add map's picture files then the map's own are
entered from block 0, each at the block the last left (`CMap::LoadData`, map.cpp:1506). In town the
interior's pictures start at block 0x18 and the main map's effects at 0x40; in dungeons the map's
effects are at 0xF and the party at 0x10. So a dungeon map has 15 blocks, an interior 40, and a
town map 24 when an interior or Palm Brinks part loads beside it, 64 when none does. s53 (32), s10
(31) and s09 (27) use more than 24: no interior loads with them.

**Memory**, estimated. What a map takes of its stack (`CScene::LoadMapFromMemory`, sceneload) is
counted the way the engine allocates it, from the files:

- the `.ipk` and `.efp`, copied whole (`LoadMapData`: each `Align64` then its size);
- each model of each model pack by its `TYPE` in the pack's `info.cfg` (`CMdsList::LoadPCPFile`,
  mdslist.cpp:484): 0 a fixed model (`mgLoadMDSFile`, mg_dataset.cpp:502: 0x110 bytes a frame,
  its name, attribute (0x90) and bound (0xB0), a 0x50-byte `mgCVisualFixMDT`, 0x30 a material, and
  per primitive an `mgCFace`, a face group and the VU packet `CreateFacePacket` builds: two header
  quadwords, an end tag, and per batch of 57 corners (42 with colours) four quadwords plus one per
  attribute per corner, a strip repeating two corners a batch); 1 and 2 collision
  (`LoadCollisionFile`, collision.cpp:387: 0x120 a frame, 0x50 a triangle; strips or colours get
  none); 3 and 4 a character (section 4);
- the sky's pictures and models (`CMapSky::LoadPack`, mapsky);
- the placement table, each template and each placement's copy of its template's pieces without
  a collision type (`CMapParts::Copy`, mapparts.cpp:803: 0xD0 a piece and its material table).

Left out: the `.map`'s settings, the `.cfg`, the effect scripts and function points, all small.
How a map loads comes from its row of the game's map table (`map<language>.cfg`; the disc's
older `map/map.cfg`, which nothing reads, gives some maps other types): types 7 and 8 in the
dungeon loop, the interiors (types 2, 4 and 6) and Palm Brinks' parts m02 to m05 (maps 11 to
14, which `EditMapJump`, editloop.cpp:2735, loads over m01) as sub maps, the rest as the main map.
Largest on the disc: town g01 9,471,152 bytes, interior i04h01a 3,752,880, dungeon d02f01
6,911,504 (`memory::RETAIL_TOWN_MAP`, `RETAIL_INTERIOR_MAP`, `RETAIL_DUNGEON_MAP`; the corpus
test `corpus_retail_fits_the_budgets` fails when the disc no longer gives them, so re-measure
them there when the estimate or the map list changes). A map larger than the largest of its
kind may still fit; how much its scene leaves
is not known (section 1).

## 4. Character packs

`CCharacter2::LoadPack` (`ScanInfoFile`, character.cpp:1286) builds a pack into the stacks it is
given. The use is estimated, counted from the pack: each `IMG` picture file copied whole
(`stAlloc64(size / 16 + 1)`), the `MODEL` built as in section 3 with the `VERTEX_ANIME` frames as
skinned visuals (`mgCVisualMotionMDT`, 0x110 bytes, packets with two weight quadwords a corner and
a batch of ((180 - 2 x bones - 2) / 5 / 3) x 3 corners, visualmotion), the `SHADOW_MODEL` with its
vertices and indices copied, each motion's `.bbp` copied and its `.mot` key lists converted
(`CreateAnimeDataEX`, gameutil.cpp:807: 0x20 a list, 16 bytes a key and 4 a key frame), the key
tables (0x30 a key) and the outline, sound and cloth tables.

**The party's stacks**, certain sizes. `GetCharaMemAllocPtr` (charasetup.cpp:248) splits the party's
memory by `mem_table` (data 0x351D50), quadwords:

| Row | Stack 0 | 1 | 2 | 3 | 4 | 5 | 6 |
|---|---|---|---|---|---|---|---|
| 0, Max and Monica in dungeons | 68,500 | 40,000 | 7,000 | 4,650 | 3,000 | 9,900 | 4,500 |
| 1, the Ridepod | 30,000 | 30,000 | 60,000 | 10,000 | 10,000 | | |
| 2, Max and Monica in town | 75,000 | 40,000 | 7,000 | 4,650 | 3,000 | 9,900 | |
| 3, Monica's monster form | 120,000 | | | | | 5,500 | |

`SetupMainUnit` (charasetup.cpp:100) loads the model pack into stack 0 (`mainchr/c01_baseN.chr`,
N the sword's model number + 2, and `mainchr/c02_baseN.chr` in dungeons; `chara/c01_base.chr` and
`chara/c02_base.chr` in town), the costume into 1 and the shoes into 5 (`LoadSkin`), the sword into
2, the second weapon into 3, the hat or accessory into 4, the action script into 6.

The estimate against these, largest per stack on the disc: Max's model packs 1,045,472 to 1,080,672
of 1,096,000 (c01_base0 and 1, which no sword picks, come to 1,105,632 and 1,080,704), Monica's
958,576; in town 664,976 of 1,200,000; swords 104,496 of 112,000, second weapons 71,968 of 74,400,
hats 47,792 of 48,000, shoes 157,616 of 158,400, costumes 641,296 of 640,000 (`LoadSkin` builds
less than `LoadPack`, which the estimate assumes). That the largest packs come within 1 to 3% of
their stacks suggests the estimate is close, and the stacks were sized to the largest pack: there
is little room to grow any of them. Max's model packs carry no real mesh (each mesh has three
vertices; the costume packs hold the body), so their stack 0 is nearly all motions.

**Skinned meshes**, certain. `_MODEL` (character.cpp:1582) builds a skinned mesh's vertex weights
(32 bytes a vertex, plus 2 quadwords) in a 6,400-quadword work buffer, so a mesh named in
`VERTEX_ANIME` may have 3,198 vertices; `CreateVertexWeight` keeps 32 bones a mesh and drops the
weights of any more.

Other characters (villagers, event characters, monsters) load into the shared stacks (section 1).

## 5. Texture memory (GS)

**A texture block**, certain. `mgCTextureManager` keeps numbered blocks; every block uses the same
GS memory, reloaded before its textures are drawn. It starts at `vram_top` = 0x2D20
(`GetVramTopAddress`, mainloop.cpp:101: 0x2D00, the end of the 512x480 32-bit frame buffers and Z
buffer of `mgInit(2, 3)`, plus 0x20) and ends at `vram_fix`, 0x3FE0 (`Initialize`) less the 32 GS
blocks of the photo work texture `LoadTakePhoto` fixes at the top (photo.cpp:61): 4,768 GS blocks
(of 256 bytes). `GetRemainVRAM` (mg_texture.cpp:452) counts each texture's size rounded up to whole
32-block pages and 4 blocks for each palette; an unswizzled 8-bit picture goes in the Z buffer
instead, at four times its size, while its 4,096 units last (`GetZBufVram`, `CheckCopyToZBufVram`).

A picture's size (`EnterTexture`, mg_texture.cpp:482): bpp x width x height / 2,048 GS blocks (24-bit
counts as 32), each mip level a quarter of the last; a one-level 8-bit picture of up to 65,536
pixels is unswizzled on load (`Conv32To8`).

`EnterIMGFile` (mg_texture.cpp:753) enters an IM3's pictures sorted by group (an exchange sort,
not stable), each at block + group + the blocks spilled so far; when a block runs out the picture
moves to the next ("texture over") and every later group moves with it. IMG and IM2 files go into
one running block. It returns the last block used, which `CMap::LoadData` adds to the next file's
block; other loaders ignore it, so their spill lands in the next owner's block.

**The block map.** Town (`SetTableBuffer(0x15E, 0xDD)`, the `MapJumpMapInfo` records EditInit sets):
the main map and its add map from 0, the interior from 0x18, effects at 0x40 (main) and 0x41
(interior), the party 0x46 (8 blocks), villagers 0x4E to 0x5D, the interior's villagers 0x5E to
0x65, event characters 0x66 to 0x85, menus from 0x86, the font 0x9A, the sky 0xA9 to 0xAC.
Dungeons (`SetTableBuffer(0x140, 0xAF)`): the map from 0, its effects 0xF, the party 0x10 to 0x17,
villagers and event characters 0x18 to 0x27, monsters 0x28 to 0x33 (one block a kind), the sky
0x4C to 0x4F. A sky's four picture files go into four blocks, each deleted before its load, so each
must fit in one; a character's pictures share its one block (the party's eight).

Measured over the disc: every sky picture file and every character pack fits its block (except
`e272_jyuraku.chr`, whose groups reach 7 blocks, and `s17.efp`, 7); f04, s01, s28, s29 (2), s30,
s51, s53, s55 and s67 already spill once inside their maps' blocks, s01's fullest block with 12 GS
blocks to spare. 350 texture slots in town and 320 in dungeons are shared by every load.

## 6. Sound memory (SPU2)

A bank's samples (`.bd`) load into SPU2 memory, each taking bd + 16 bytes (`sndLoadSound`:
`LoadHdBd`/`LoadHdBdAdd` place the next bank 16 bytes on, asm 0x18B9F8). Bases are `CSound::Init`'s
(sound.cpp:271-283); game ports map to driver ports through `GetCSndPortNo`.

| Region | Base, direction | Bound | Size | Largest | |
|---|---|---|---|---|---|
| Music, BG_* (ports 0 and 11) | 0x5210 up | the field banks | 0x78000 | BG_056 0x77E80 | certain |
| Field: base BS, battle FG, ambient SR, objects OB (up to 16), one event EV | 0x7D210 up | meets the monster banks | 0x10DC10 together | see below | certain |
| Monsters EN_* (port 5) | 0x18AE20 down | | | | |
| System SY_000 | 0x18AE20 up | the menu banks | 0x1D4C0 | 0x1C6F0 | certain |
| Menu SP_* (one at a time) | 0x1A82E0 up | core 1's reverb work area, 0x1D9020 | 0x30D40 | SP_009 0x30B30 | estimated end |
| Player character CH_* (one at a time) | 0x1E0000 up | core 0's reverb work area, 0x1F9020 | 0x19020 | CH_047 0x18190 | estimated end |

The reverb work areas end at 0x1DFFFF and 0x1FFFFF (sound.cpp:45, 73); their size (0x6FE0 for mode
4, the only mode the disc uses) is read from LIBSD's table and not traced in IOP code, which is
why the two ends are estimated. That 0x1D9020 - 0x1A82E0 is exactly 200,000 supports it.

**Which field banks are resident together.** `CScene::LoadSound` (scenesnd.cpp:365) loads a sound
set's base, battle, ambient and object banks in that order; loading one restarts every later one
right after it, a `*` keeps what is loaded, and a layer reloads only when its bank changes. Town
maps load their map list's set, dungeon floors theirs (`GetMapSndDataID`), scripts others
(`load_sound_set`). **Event banks are never part of a set**: a script's `load_sound(set)` loads
only that set's event column (`GetDefEventSeFile`), as one bank on port 4 after whatever field
banks are loaded. That resolves the old puzzle: set 1837's own field banks (0x114000 with its event
bank) are never loaded together. Its event bank EV_100_155 is loaded in s01
(`play_blackstone_railroad_arrival_scene`) over s01's set 1600: 0x8A350 + 0x83E30 = 0x10E180, 1,392
bytes over, reaching 0x18B380 and overwriting the start of SY_000's samples (probably the menu's
window-switch sound) for the rest of the session. This is predicted, not seen. The next fullest
set a script makes (d03b02's 3691 with EV_115_158) leaves 5,792 bytes; dungeon floors leave at
least 343,824 (set 5600 with its floor's monsters, up to 6 banks and 0x2B2D0 a floor).

The IOP copies a `.bd` through one 0x6DD00-byte buffer (sound.cpp:122-159): in one piece up to
0x6DE00, so 0x6DD01 to 0x6DE00 overrun it, and in two above, up to 0xDBA00. No bank on the disc is
in the bad window; the largest is EV_130_150 at 0xD2B10.

The bank's tables on the EE side are in [the sound bank format](sounds.md#8-memory-the-game-gives-a-bank).

## 7. What build checks

`build` runs the check on every disc file it rewrites whose bytes changed, and prints `error:`
where a certain budget is exceeded (the image is still written, but `build` fails), `warning:`
where an estimated one is (or a load grows past what the disc shows to work), and `memory:` with
what each changed load uses:

- **A map** whose `.map`, `.cfg`, `.mpk`, `.ipk`, `.efp` or `.sky`, or whose add map's files, changed:
  read buffer and loading screen (past the loading screen an error in town, a warning in a
  dungeon), placements, texture blocks (an error past its blocks; a warning when a town map goes
  past 24 that did not), and its estimated memory (a warning when it is more than the largest of
  its kind on the disc and more than before). The maps and their kinds come from the built disc's
  map tables (section 3), so a mod's new map is checked as the kind of the map it is like; a
  changed map file that no table lists is taken by its name's first letter (`d` a dungeon map,
  `i` an interior, else a town map).
- **A map script** (`map/**.stb`): the script buffer.
- **A character pack**: the read buffer; skinned meshes' vertices and bones; for the party's
  packs the estimate against their stack; its pictures against its texture blocks.
- **A sky**: the read buffer, and each picture file in one block.
- **A map's effects** (`.efp`): its pictures against their one block.
- **A sound bank**: the read buffer, the IOP window, its region (music and system an error, menu
  and character a warning), each sound set that loads it with the other field banks (an error
  past the field region), and a warning when a field, event or monster bank grows, since the
  scripts' fullest set leaves 5,792 bytes. For a music, base, battle, ambient or object bank whose
  tables on the EE grew, their estimated size against its layer's EE stack (a warning past it;
  a set's object banks count together; see [sounds](sounds.md#8-memory-the-game-gives-a-bank)).
- Any other file that grows gets a note that no budget covers it.

Not checked: the monster and event banks' combinations with each sound set (measured in this
research by following the scripts, not built into `build`), the EE tables of event and monster
banks (where they go was not traced), the Ridepod's and the monster form's stacks, the shared
pool's free space, and the menu and dungeon message files, whose room in the menu's memory and
the dungeon's heap is not known ([text](text.md#7-limits)).
