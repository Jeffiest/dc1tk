# Character models: MDS, WGT, BBP, MOT, info.cfg, CLO

How the game stores and loads its character models: the MDS scene and mesh container, skinning
(.wgt weights, .bbp bind poses), .mot motions and their names, the info.cfg script that ties a
pack together, and .clo cloth scripts. These are the formats behind editing character models in
Blender through glTF: how to do that is in [editing character models](../modding/models.md), and
how dc2tk's converters work in [the implementation notes](../../src/docs/assets/models.md).

Engine source: the ChronicleTwo decompilation's `ps2/` (paths below are
relative to it). Corpus: the extracted PAL disc. All numbers are counts over
the whole corpus unless a sample is named; they come from scripts run over
all of it while the formats were worked out (the "evidence" scripts listed in
[the implementation notes](../../src/docs/assets/models.md#8-evidence-log-research-scripts-run-while-the-formats-were-worked-out-not-in-this-repository)).

## Open questions

- Texture pixels: `.img` is in [textures.md](textures.md). A material refers to a texture only by
  name (case-sensitive `strcmp`, see 1.6). UV v orientation still to be confirmed against decoded
  .img data. No flip is expected, because GS and glTF both put (0,0) at the top-left.
- Meaning of material fields 0x10 (ambient?), 0x20, 0x30 (power?) and odd diffuse values (w = -11,
  components > 1: 8,598 of them in map files). The engine only reads diffuse, 0x10 and the name.
- The engine meaning of motion types 12 (vertex), 40/41 (material alpha and colour) and 50/51
  (visibility) beyond what `MotionProc` does is not traced (what `SKIP_CHILDREN` hides).
- The CPU skinning path (`SHADOW_MOTION` and the .wgt given in `MOTION`'s 4th argument) is only
  partly followed. `_MOTION` ignores its 4th argument (sets `entry[2].data = 0`), so the main model
  is skinned only by the VU1 path, from `<model>.wgt`.

---

## 1. MDS: scene/mesh container

Loader: `mgLoadMDSFile` (src/mg_dataset.cpp:503) -> `CreateFrameVisual` (mg_dataset.cpp ~340-470)
-> `mgCVisualMDT/FixMDT/MotionMDT::DataAssignMDT` (src/mg_visual.cpp:238-285, visualmotion.cpp:71).
Struct declarations: include/mg_dataset.hpp (`MDS_HEADER`, `MDTOBJ_HEADER`, `MDT_HEADER`,
`MDT_MATERIAL_`, `MDT_FACES`, `FACES_ID`).

The data must be 16-byte aligned in memory (`printf("address error!!")` otherwise). Pack members
are 16-aligned ([the disc](disc.md)). The vertex arrays are used in place
(`CopyMDTDataPointer`), so the file is also the runtime array.

### 1.1 Header (0x10)
| off | type | value | evidence |
|---|---|---|---|
| 0x00 | char[4] | `"MDS\0"` | all 16,873 non-empty files (3 .mds are 0 bytes) |
| 0x04 | u32 | 1 | all files. Engine never reads it (`unk_04`) |
| 0x08 | u32 | object_num | `mds->object_num` loop bound |
| 0x0C | u32 | object_ofs = 0x10 | all files |

### 1.2 Object records (`MDTOBJ_HEADER`, 0x70 each, back to back)
| off | type | field | evidence |
|---|---|---|---|
| 0x00 | s32 | index of the record | equals its position in all 248,938 records. Not read by the engine |
| 0x04 | u32 | size = 0x70 | loader steps by it. 0x70 in every record |
| 0x08 | char[32] | name, NUL padded (padding always zero) | `frame->SetName`. Text after `--` = attribute flags (1.5) |
| 0x28 | s32 | mdt_ofs from MDS start, 0 = no mesh | `CreateFrameVisual` |
| 0x2C | s32 | parent index, -1 = root | parent < own index in every record. 4 files have 2 roots |
| 0x30 | f32[16] | local matrix | `frame->SetTransMatrix(matrix)` |

Matrix convention: sceVu0 4x4, stored as 4 columns of 4 floats; m[3] = translation, last row
(0,0,0,1) in all 34k records sampled. `mgMulMatrix(P,A,B)` (src/mg_math.cpp:672) computes P = A*B
with column vectors, and `GetLWMatrix` does LW = parentLW * local. That is the same layout as a
glTF `matrix`. Scales are mostly 1 (147,911 of 151k nodes). There are some 0.42/0.7/2.0 scales.

MDT blocks follow the records directly, in record order, with no gaps. Nothing comes after the
last one (checked on every file).

Name lookup: `SearchFrame`/`StrCmp` (src/mg_frame.cpp) compares names up to the `--` flags.
info.cfg, .clo, LoadSkin and the shadow links all find frames by name.

### 1.3 MDT block (one mesh)
| off | field | notes |
|---|---|---|
| 0x00 | `"MDT\0"` | |
| 0x04 | header size 0x40 | |
| 0x08 | block size | equals the distance to the next block in all 92,490 cases. The engine does not read it |
| 0x0C/0x10 | vertex_num / vertex_ofs | f32[4] (x,y,z,1). w = 1.0 in every sampled vertex |
| 0x14/0x18 | normal_num / normal_ofs | f32[4] (x,y,z,1). Unit length except 34 zero normals |
| 0x1C/0x20 | colour_num / colour_ofs | f32[4] RGBA 0..1 (never > 1). ofs 0 (or -1, style B) when none |
| 0x24/0x28 | faces_size / faces_ofs | see 1.4 |
| 0x2C/0x30 | uv_num / uv_ofs | f32[4] (u, v, 1, 0). The z=1, w=0 hold in all 276k sampled. 7% lie outside 0..1 (tiling) |
| 0x34/0x38 | material_num / material_ofs | 0x60 each |
| 0x3C | 0 | always |

The four arrays are separate index spaces. A corner picks a vertex, a normal, a uv and a colour
independently, as in OBJ. Counts differ (Max's skin1: v344 n318 uv237).

**Layout (needed for byte-exact writing; the engine only follows the offsets).** Order is
header, vertices, faces, pad, normals, uvs, colours, materials. Two exporters:
- style A (92,412 blocks): pad after faces = 16 - (end % 16) zero bytes (so 1..16), then 16 zero
  bytes after the normals, 16 after the uvs only when colours follow, and 16 after the materials.
  colour_ofs = 0 when there are no colours. uv_ofs is set even when uv_num = 0.
- style B (78 blocks, `colour_ofs = -1`): only pads faces to 16 (0..15 bytes). In 38 of these the
  pad holds leftover non-zero bytes.
- the faces header's first word is garbage (12255244) in 22 blocks.

### 1.4 Faces
```
MDT_FACES  { u32 unk (0), u32 header_size = 0x10, u32 prim_num, u32 0 }
prim_num x FACES_ID { u32 type, u32 vertex_count, u32 material, s32 index[vertex_count*stride] }
```
- stride = 3 (v, n, uv) +1 if `0x100` (colour) -1 if `0x200` (no normal) -1 if `0x10` (no uv).
  From `mgCVisualMDT::CreateFace` (asm/pal/nonmatchings/mg_visual/CreateFace__...s): index_stride
  starts at 3 and is adjusted by exactly these bits. Corner tuple order is v, [n], [uv], [c]:
  `AnimeDataInit` uses index[0] as vertex and index[1] as normal, and an index-range test over
  the corpus agrees (slot 0 always fits vertex_num, the last slot of 0x100 prims always fits colour_num).
- `type & 7`: 3 = triangle list (count % 3 == 0 always), 4 = triangle strip. These are GS PRIM
  values. `CreateFacePacket` treats 4 specially (re-sends 2 vertices when it splits a strip).
  0x8 (flat) is never set. Types in the corpus: 4 (495k), 0x104 (101k), 3 (99k), 0x103, 0x213,
  0x14, 0x204, 0x304, 0x203, 0x114, 0x314, 0x13.
- one strip per FACES_ID (no restart markers). 9+19+83 degenerate triangles in samples.
- empty texture name -> all its prims carry 0x10 (15,286 of 15,303 materials).

**Winding.** Comparing the triangle normal to the summed corner normals (mainchr + dungeon):
lists are CCW 98.8%, and strips are CCW 98.8% when odd strip triangles are swapped
(even triangles (i, i+1, i+2), odd (i+1, i, i+2)). So glTF's default CCW front face is right.

### 1.5 Name flags (text after `--`)
`mgSetFrameAttr` (src/mg_dataset.cpp:163). `conv_new_text` turns an exporter's `__` into `--`.

| flag | effect |
|---|---|
| `v0`/`v1` | draw off/on. `vcN` sets attr unk_84 |
| `o1` | z test always |
| `t` | ambient boost |
| `bY`/`bA` | billboard Y / full |
| `mN` | VU program mode N (e.g. `atoramiria--m1-`) |
| `s1` | program option |
| `fN` | fog |
| `z0/z1`, `zp0/zp1` | z write off/on, depth bias |
| `aPP`/`aNN`/`aOF`/`aXX` | additive / subtractive / opaque blend, or alpha ref = hex XX |
| `n0/n1` | clipping |
| `c0/c1` | lighting on/off (`c0` = no light) |
| `.` before a flag | apply the flag to the whole subtree |

Common in the corpus: `ba` (3,014), `n1s1-`, `c0`, `N1`, `s1-`, `n1z0c0f2app-`, `a40aof-`, ...

### 1.6 Materials (`MDT_MATERIAL_`, 0x60)
| off | field | engine use |
|---|---|---|
| 0x00 | f32[4] diffuse | `mgMaterial.diffuse`, sent to VU1. Animated by .mot type 40/41. Mostly (1,1,1,1) |
| 0x10 | f32[4] | `mgMaterial.unk_10` (first game: ambient), uploaded with diffuse. Mostly (1,1,1,0) |
| 0x20 | f32[4] | not read. Always 0 |
| 0x30 | f32 | not read. 50.0 in 18,888 of 18,976 (shininess? guess) |
| 0x34 | char[32] | texture name -> `mgCTextureManager::GetTexture(name, -1)` (strcmp, optional global suffix) |
| 0x54 | s32, f32, f32 | not read. Always 0 |

`CopyMaterial` (src/mg_visual.cpp:184) copies only diffuse, 0x10 and the texture lookup. 70 names
have bytes after the NUL.

**Texture reference:** only by name. Textures come from the IMG files that info.cfg's `IMG n,
"file.img"` loads (`_IMG`/`_IMG_END` -> `EnterIMGFile`). For Max: material names `c01b01..c01b04`,
texture file `c01b.img` in the costume pack.

### 1.7 Bounds
Not stored. `mgCreateBBoxSphere` computes box and sphere from the vertices at load (mg_dataset.cpp).

### 1.8 What the engine builds at load (why there is no VU data in the file)
`CreateFacePacket` (asm mg_visual) turns each FACES_ID into VIF UNPACK + GIF data. A batch holds
`3*floor(floor((vu1_offset-2)/q)/3)` vertices, where q is the qwords per vertex (3, or 4 with colour).
vu1_offset is 0xB4 for MDT (so 57 vertices, 42 with colour) and `0xB4 - 2*bones` for a skinned
`mgCVisualMotionMDT`. Strips that span batches overlap by 2 vertices. A writer needs to know none of this.

## 2. Skinning: VERTEX_ANIME, .wgt, .bbp

### 2.1 Which meshes are skinned
info.cfg `VERTEX_ANIME "skin1", "skin2", ...;` (up to 24 names, `_VERTEX_ANIME`). `_MODEL`
(src/character.cpp:1582) loads `<model>.wgt`, the model name with its extension replaced. It loads
the named frames as `mgCVisualMotionMDT` (VU1 skinning), or as plain `MDT` with `SHAPE_ANIME 1`
(CPU vertex animation). Every other mesh is rigid and drawn with its frame's matrix. On Max
(c01fuku1): skin1, skin2, R_te, L_te are skinned. kami, kao, kaban, etc. are rigid meshes on bone frames.

### 2.2 .wgt (key-list file, block type 20)
Same container as .mot (section 3). Each block is `frame` = the skinned mesh's node index,
`target` = bone node index, `type` = 20, and keys = (vertex index, weight in percent in value.x).
Value y/z/w and the key padding are always 0. 310,379 blocks, all type 20. Blocks with 0 keys are
common (empty headers for every bone): `CreateVertexWeight` passes over a block of no keys (it
gives a bone a slot only for a key), and the CPU path's lists of no keys weigh nothing, so they
can go.

`mgCVisualMotionMDT::CreateVertexWeight` (asm visualmotion, decoded):
- only blocks whose frame equals this mesh's node index are used;
- each distinct bone gets a slot. **At most 32 bones per mesh**; more prints an error and drops
  skinning. Every mesh with more than 32 bones (up to 46) is in a shadow .wgt, which goes through
  the CPU `MotionProc2/3` path and has no slot limit. All model .wgt files stay at 30 or fewer;
- per vertex, **at most 4 influences** (the first 4 non-zero in file order). A 5th triggers
  normalising and an error print. 16 meshes in the corpus have 5;
- weights are /100. A vertex whose weights sum to between 0 and 1 is normalised. A vertex with no
  weight gets 1.0 on slot 0, the first bone listed. All corpus meshes sum to 100%.

### 2.3 .bbp (bind pose)
`object_num` x 16 floats, the bind-pose **local-to-model** matrix of every node (identity for the
root). `_MOTION` (character.cpp:1820) memcpy's it over `root->init_matrix`, the base matrix table
that `mgLoadMDSFile` first fills from the MDS's own LW matrices. 2,186 of 2,189 have exactly
object_num entries. Some entries are all-zero (nodes not used for skinning). Bones often differ
from the MDS rest pose (bbp vs MDS world matrix: 77k equal, 74k different), so the bbp is
authoritative.

### 2.4 Skinning formula
From `CreateExtRenderInfoPacket` (visualmotion.cpp) and `MotionProc2/3` (gameutil.cpp:501-660).
With base = rootLW * bbp:
```
v_model = boneLW * inv(bbp[bone]) * bbp[mesh] * v_mesh_local      (summed over weights)
```
Normals use the same matrix without translation.

### 2.5 Costumes (LoadSkin)
`chara/c01_base.chr.d/c01b.mds` holds only the skeleton plus 3-vertex placeholder meshes. The
real meshes are in costume packs (`mainchr/c01fuku1.chr.d`: `c01f.mds`/`.wgt`/`.bbp`,
`c01b.img`), loaded by `CCharacter2::LoadSkin` -> `_SKIN_MOTION` -> `CreateChangeFrame`
(character.cpp:2520). That path loads the costume MDS with its own .wgt, copies the costume
.bbp matrix **only for the skinned mesh nodes**, remaps bone slots by name (`ChangeWeight`) and
then swaps every costume visual onto the base frame with the same name. c01f's skeleton bbp
matches c01b's bbp for every bone (difference 0.000). Only R_te/L_te differ.

### 2.6 Which pack Max and Monica come from

`SetupMainUnit` (charasetup.cpp:100) loads the player's skeleton-and-motion pack from the name
`GetMainCharaModelName(chara, name, alternate)` (userdata.cpp:1114, in asm) gives:
- **towns** (`EditInit` passes edit mode 1): `chara/c01_base.chr` (Max), `chara/c02_base.chr`
  (Monica), `"%s.chr"` under `chara/`;
- **dungeons** (`InitDungeonMain`, the event native `_DNG_SETUP_MAIN_UNIT`: edit mode 0):
  `mainchr/c01_baseN.chr` / `mainchr/c02_baseN.chr`, `"%s%d.chr"` under `mainchr/`, N the model
  number of the equipped weapon (`CGameDataUsed::GetModelNo`, 0 to 3), plus 2 for Max: Max uses
  c01_base2 to c01_base4, Monica c02_base0 to c02_base3. The menus' character view picks between the
  two the same way (`MenuItemCharaDataLoad`).

Event scenes load Max's town pack themselves (`load_party_member(0, "chara/c01_base.chr", ...)`,
449 calls). Nothing in the engine or the scripts names `mainchr/c01_base.chr`,
`mainchr/c01_base0`/`1`, `chara/c01_base_f.chr`, `chara/_c01_base.chr`, `chara/__c01_base.chr` or
`dungeon/chara/c01_base.chr` (`dungeon/chara` holds motion packs added to the player:
`c01b_win`, `c01b_miss`, `c01_gio`). Each pack has its own `.mot` and KEY frames (Max's idle
`立ち` is frames 95-97 in `chara/c01_base`, 362-364 in `mainchr/c01_base2`), so a change to how Max
moves is made in each pack: the town one and the three dungeon ones.
The costumes, weapons and hats are shared (`mainchr/c01fuku1.chr` and the like, loaded with
`LoadSkin`/`LoadPack` in both).

## 3. .mot motions (key-list file)

Parser: `CreateAnimeDataEX` (src/gameutil.cpp:807). Player: `MotionProc` (gameutil.cpp:102).
```
block: s32 frame (node), s32 target, s32 type, u32 0x20, u32 key_count,
       u32 more (= (key_count+1)*32, i.e. block size; 0 on the last), u32[2] garbage
key:   u32 frame, u32[3] 0, f32[4] value
```
- `more` is only tested for zero. Its non-zero value is always the block size. The two trailing
  words are uninitialised exporter memory (pointer-like 0x0137e6ac / 0x77fcf0b0), never read.
- types (`MotionKeyType`, include/gameutil.hpp): 0 rotation (58,590 blocks), 1 scale (9,957),
  2 translation (17,506), 12 vertex position (1,028), 40 material alpha (11,211), 41 material
  colour (none), 50/51 visibility (12), 30-33 camera (none in .mot files). `frame` is the MDS node
  index: `MotionProc` drives `root->GetFrame(list->frame)`, whatever the node's name.
- key frames ascending in every list, never repeated, and **one key every frame**: of 10,314,102
  steps between keys, 10,314,034 are 1. The first key is at frame 1 in 92,772 lists and at 0 in
  3,565. Lists are in node order; a node's lists run 40 (one per material), 0, 1, 2.
- One .mot holds every motion of the character on one timeline (chara c01b.mot: frames 1..379;
  mainchr c01b.mot: 1..598). Every list is applied every game frame (`SetMotionTime` walks the
  whole list), so every list needs keys over every KEY's frames.

### 3.1 Key types (`MotionProc`, gameutil.cpp:102)

The value at time `time` (a float frame): binary search for the last key at or before
`(u32)time`, the key after it (the same key past the last), `t = (time - key) / (next - key)`.
Below the first key the search lands one key before the list (index -1): the engine reads
before the array, so the first key must be at or before every frame a KEY plays. Past the last
key the divisor is 0.
- **0 rotation:** value = (w, x, y, z), unit length (175,703 of 175,703). `QuatSlerp` (shortest
  arc) for 0.001 < t < 0.999, else the nearer key. `QuatToMat` (mg_frame.cpp:123) writes the
  transpose of the usual matrix, and `SetTransMatrix` *replaces* the 3x3 of the node's local
  matrix, keeping its translation (scale from the MDS matrix is lost). glTF rotation =
  (-x, -y, -z, w).
- **1 scale:** lerp of xyz (w = 0 in all) into `SetScale`, which the node applies apart from the
  matrix (`use_srt`).
- **2 translation:** lerp of xyz (w = 0 in all) replaces the local translation.
- **12 vertex:** `target` = vertex index + 1, value = (x, y, z, 1): the vertex of the node's
  `mgCVisualMDT` (the MDS vertex array, used in place) is set to the lerped xyz. `MotionProc` takes
  the following lists in one go while their `frame` stays the same, with the key indices of the
  first: a node's vertex lists must be consecutive and share their key frames (all of the disc's
  do; a few event packs such as `event/e200/e206/e206_cloth`). A list of another type for the same
  node right after the group is taken as a vertex list too (one file has one).
- **40 material alpha:** `target` = the MDT material index, value.x (y, z, w = 0):
  `diffuse.a = 1 - lerp(x)`, so 0 is opaque and 1 invisible. 11,211 lists, all varying (fades in
  effect packs, mostly material 0). No t clamping.
- **41 material colour:** lerp of xyz into the material's diffuse rgb, and the frame's attribute
  `unk_28 = 2`. None on the disc.
- **50 / 51 visibility:** value.x of the key at or before the frame, no interpolation. 50: below
  1 hides the node (`draw = 0`), else `VISIBLE | SKIP_CHILDREN`. 51: below 1 sets
  `SKIP_CHILDREN` alone, else `VISIBLE`. 12 lists, all type 50 (`e532_guil_kage`,
  `e616_osmond`).
- **30-33 camera** (position, target, roll, field of view) run with an `mgCCamera` only; not in
  any .mot.

Blending to the next motion (`ChangeMotion`, used while `blend < 1`) takes the keys at
`(int)(0.9 + frame)` and at the next KEY's start and mixes them by `blend` (slerp or lerp,
thresholds 0.0001/0.9999), which grows by `blend_speed` (0.2 from `SetMotion`, a SEQ's own)
each game frame from 0.1.

### 3.2 Time base

The PAL game runs its logic at 25 Hz: `mgFrameRate` is 2 (title.cpp), and `mgEndFrame` waits that
many 50 Hz vertical blanks per frame. `CCharacter2::NormalDrive` advances a playing KEY by
`1.2 * step` motion frames per game frame (character.cpp:974), the 1.2 making up for PAL's 25
against NTSC's 30. So a KEY plays at **30 × step motion frames a second**: step 1.0 is 30 fps,
walk (0.5) 15, idle (0.02) 0.6. A loop goes back to `start` once `frame + 1.2 * step` would pass
`end`, so `end` itself shows only with `CHARA_MOTION_HOLD`. Steps on the disc: 0.30 (796 KEYs),
0.50 (616), 0.40 (575), ... down to 0.00 (119, held poses).

### 3.3 Motion names
info.cfg:
```
MOTION 0, "c01b.mot", "c01b.bbp", "c01b.wgt";
KEY_START;
    KEY "歩き", 11, 31, 0.50;     // name (<= 35 bytes cp932), start frame, end frame, step
    ...
KEY_END;
MOTION_END;
```
`_KEY` fills `CHRINFO_KEY_SET {char name[0x24]; s32 start, end; f32 step}`. Scripts call
`SetMotion(name)` -> `GetKeyListPtr` (strcmp over the 8 motion sets' key lists), or by number
(`GetKeyListIndexPtr`, counting KEY entries in order), so a new KEY goes after the last. `step`
sets the speed (3.2): walk 0.5, run 1.0, idle 0.02. Several KEYs may
share frames (1,770 pairs have the same range, such as `ドア開け`/`ドア開かない`; 325 share an end
frame; 106 overlap). `SEQ_START/SEQ/SEQ_END` chain named KEYs (blend speed, type, loop count).
`SE "name", kind, bank, frame` ties sounds to KEY names (`_SE`, character.cpp:2140): a frame up
to 1.0 is a fraction of the KEY, turned into a timeline frame with the KEY's range as it stands
when the line is read; a larger one is a timeline frame. All 1,915 on the disc are fractions, so a
KEY moved to other frames keeps its sounds. `SELP` gives a start and an end the same way. Up to 8
motion sets (`MOTION 0..7`); a pack can bring sets alone (`chara/c01_fishing.chr`: `MOTION 7`, no
model) for a character already loaded.

## 4. info.cfg

Interpreter: `CScriptInterpreter` (src/scriptinterpreter.cpp). Text is cp932. `//` and `/* */`
comments. Statement: `TAG arg, arg, ...;`. A tag is taken only if it starts with 'A'-'Z', so
the usual first line `v2;` is ignored (the table entry is `V2`). An argument in quotes is a
string (`\"` escapes), one containing `.` is a float (atof), anything else is an int (atoi).
Character tags (table `tag[]`, character.cpp:178; counts from 2,532 info.cfg):

| tag | args | meaning |
|---|---|---|
| NAME | str | display/debug name (ignored) |
| IMG / IMG_END | idx, "x.img" | texture archives (idx < CHARA_IMAGE_MAX) |
| MATERIAL_ANIME | int | ignored |
| VERTEX_ANIME | names... | frames to skin (2.1) |
| SHAPE_ANIME | 0/1 | 1 = CPU vertex animation instead of VU1 skinning |
| MODEL | "x.mds" | main model. Its weights come from "x.wgt" |
| OUTLINE | "frame", width | toon outline. "" = whole model |
| SHADOW_MODEL | "x.mds" | shadow mesh, linked to main frames by name |
| SHADOW_VERTEX_ANIME | names | (pack-level) |
| BODY_SIZE | h, w, d | collision size (read as ints) |
| POLY_NUM | a, b | polygon budget numbers |
| SCALE | x, y, z | base scale |
| OBJECT_NAME / OBJECT_NAME2 | id, "frame", [float]... | attach points (hip, weapon hands, kick, body collision spheres) |
| MOTION | set, "x.mot", "x.bbp", "x.wgt" | motion set. The .wgt argument is ignored by `_MOTION` |
| SHADOW_MOTION | "", "s.bbp", "s.wgt" | CPU-skinned shadow |
| KEY_START / KEY / KEY_END | 3.1 | named clips |
| SEQ_START / SEQ / SEQ_END | | clip sequences |
| SE_START / SE / SELP / SE_END | | footstep/sound cues by clip name |
| CLOTH_START n / CLOTH "x.clo" / CLOTH_END | | cloth simulation scripts (section 5) |
| POSITION / ROTATION | x, y, z | |
| EFFECT_START / EFFECT / EFFECT_END | | particle effects |
| LOD_MODEL_START n / LOD_MODEL lvl, "m.mds", "m.wgt", "m.bbp", dist / LOD_MODEL_END | | LOD meshes |

Skin packs (LoadSkin) use another table: IMG, IMG_END, MODEL, MOTION.
Map pack (.pcp) info.cfg: `MDS "name.mds", "original path"; TYPE n; FAR_CLIP d, fade; MDS_END;`
(mdslist.cpp, notes in re/docs/mdslist/notes.md). Sky: `SKY_IMG/SKY_MDS/SUN_MDS/SKYB_MDS/SKY_ANIME`.

## 5. .clo (cloth / dangling parts), brief

Text script of the same syntax, run by `CDynamicAnime::Load` (src/dynamicanime.cpp:625) with
the table `dynmc_tag`. 72 files. Tags: `FRAME_START n / FRAME "frame" / FRAME_END` (frame ids),
`VERTEX_START n / VERTEX id, x,y,z (world offset) / VERTEX_L id, x,y,z (frame local) /
VERTEX_END` (particles), `FIX_VERTEX[_C|_S] frame, vertex[, weight[, ?]]` (pin a particle to a
frame; _C velocity_rate 1, _S -1), `FRAME_POSE[_L] frame, "bone"|"bone_xy"|"b_cdlr", v0,v1,v2,v3`
(drive a frame's orientation from 4 particles; plain FRAME_POSE detaches the frame from its parent),
`DRAW_FRAME ids...`, `BIND_VERTEX_START n / BIND_VERTEX a, b[, rate]` (distance constraints),
`BOUNDING_BOX`, `COLLISION_START n / COLLISION "pipe", frame, cx,cy,cz, rx,ry,rz, axis`,
`GRAVITY x,y,z` (default (0,-0.6,0)), `K k`, `WIND scale`.
Cloth frames are found by name, so they keep working on an imported mesh if names are kept.
