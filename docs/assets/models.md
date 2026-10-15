# Dark Chronicle character models: MDS, WGT, BBP, MOT, info.cfg, CLO

The formats behind editing character models in Blender through glTF.
Engine source: the ChronicleTwo decompilation's `ps2/` (paths below are
relative to it). Corpus: the extracted PAL disc. All numbers are counts over
the whole corpus unless a sample is named; they come from scripts run over
all of it while the formats were worked out (the "evidence" scripts named
below).

The code, in `src/crates/dc2tk-assets/src/`:

| file | what |
|---|---|
| `mds.rs` | MDS parse/edit/write |
| `mot.rs` | .mot/.wgt key-list files and .bbp tables |
| `info_cfg.rs` | info.cfg reader (same tokenising rules as `CScriptInterpreter`) |
| `to_gltf.rs` | character pack or map model -> glTF 2.0 (.gltf + .bin + textures; map options in 6.1) |
| `from_gltf.rs` | glTF -> MDS + .wgt + .bbp, with the original MDS as a template, or a new model (section 7) |
| `motions.rs` | glTF animations -> .mot keys and info.cfg `KEY` lines (section 7.3) |
| `models.rs` | the glTF folders mods use (`mod asset --as gltf`, `build`); map models (`export_map_model`, `build_map_model`); new packs (`new_pack`, 7.4) |

Mods use these through `models.rs`: textures come from the pack's own
`.img` files (not placeholders), only meshes and motions that differ from the
export are rebuilt, and the template's bind pose is kept unless the glTF
changes it.

## Status summary

**Settled (engine code + whole-corpus checks):**
- MDS container, MDTOBJ records, hierarchy, matrices (column-major, LW = parentLW * local).
- MDT mesh block: header, the four vertex arrays (float4 each), face/strip lists with per-corner
  index tuples, material records, the two writer layouts. Byte-exact round trip of all
  16,873 non-empty .mds files (92,490 meshes, 248,938 nodes).
- Faces: `type & 7` is the GS primitive (3 list, 4 strip), flags 0x10/0x100/0x200; index order
  v, n, uv, c. Strip winding: even triangles (i, i+1, i+2), odd (i+1, i, i+2) give CCW fronts
  (checked against stored normals: 99% agree, see "Winding").
- Files hold no VU/VIF/GIF data. The engine builds the VU1 packets at load time.
- .wgt = key-list file with type-20 blocks: (mesh node, bone node, vertex index, weight %).
  .bbp = bind-pose model-space matrix per node. The skinning formula is in the engine source and
  reproduced exactly by the exporter.
- .mot = key-list file; rotation quaternions stored (w,x,y,z) and fed to a QuatToMat that builds
  the transpose (glTF quaternion = conjugate). Checked against the MDS matrices (7,806 of 7,915 first
  keys match) and visually (walk cycle preview).
- Motions are named in info.cfg `KEY "name", start, end, step;`: frame ranges inside one .mot.
- Every .mot key type the disc uses (rotation, scale, translation, vertex, material alpha,
  visibility) and the time base: 30 motion frames a second at step 1.0 (section 3).
- info.cfg syntax and all its tags. .clo is a text script of the same syntax (cloth/spring chains).
- Byte-exact round trip of all 2,159 .mot, 1,825 .wgt (128 empty) and 2,189 .bbp files.
- Motions back from glTF (section 7.3), every motion set and the packs of motions alone
  included: an unedited export changes no motion in any of the 1,628 pack files with motions,
  nor does a Blender round trip of any pack tried (e210a's included); a turned bone comes back as
  given in all of them, and a deleted action takes its KEY out.

**Not settled / open:**
- Texture pixels: `.img` is researched separately. A material refers to a texture only by
  name (case-sensitive `strcmp`, see below). UV v orientation still to be confirmed against decoded
  .img data. No flip is expected, because GS and glTF both put (0,0) at the top-left.
- Meaning of material fields 0x10 (ambient?), 0x20, 0x30 (power?) and odd diffuse values (w = -11,
  components > 1: 8,598 of them in map files). The engine only reads diffuse, 0x10 and the name.
- Motion types 12 (vertex), 40/41 (material alpha and colour) and 50/51 (visibility) go to glTF
  as morph targets and carriers Blender keeps (6), not as `KHR_animation_pointer` and
  `KHR_node_visibility`, which Blender 5.1 cannot take back; their engine meaning beyond what
  `MotionProc` does is not traced (what `SKIP_CHILDREN` hides).
- The CPU skinning path (`SHADOW_MOTION` and the .wgt given in `MOTION`'s 4th argument) is only
  partly followed. `_MOTION` ignores its 4th argument (sets `entry[2].data = 0`), so the main model
  is skinned only by the VU1 path, from `<model>.wgt`.
- Mesh import (`from_gltf.rs`): our own exports and Blender 5.1 round trips of every character
  pack change nothing (7.1); an edited mesh has been drawn in the game, a new character
  (7.4) not yet.

---

## 1. MDS: scene/mesh container

Loader: `mgLoadMDSFile` (src/mg_dataset.cpp:503) -> `CreateFrameVisual` (mg_dataset.cpp ~340-470)
-> `mgCVisualMDT/FixMDT/MotionMDT::DataAssignMDT` (src/mg_visual.cpp:238-285, visualmotion.cpp:71).
Struct declarations: include/mg_dataset.hpp (`MDS_HEADER`, `MDTOBJ_HEADER`, `MDT_HEADER`,
`MDT_MATERIAL_`, `MDT_FACES`, `FACES_ID`).

The data must be 16-byte aligned in memory (`printf("address error!!")` otherwise). Pack members
are 16-aligned (`PACK_ALIGN` in `src/crates/dc2tk-disc/src/archive.rs`). The vertex arrays are used in place
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
  pad holds leftover non-zero bytes, which `mds.rs` keeps.
- the faces header's first word is garbage (12255244) in 22 blocks; kept.

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
(`Prim.triangles()`). So glTF's default CCW front face is right.

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
have bytes after the NUL, which `mds.rs` keeps.

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
can go. A rebuilt mesh's .wgt has blocks with keys alone: Max's `chara/c01_base` `c01b.wgt`, whose
five skinned meshes are 3-vertex placeholders with 95 blocks each, 464 of them empty, goes from
15,680 to 832 bytes when all five are rebuilt, every weight kept.

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
Normals use the same matrix without translation. In glTF this is: mesh vertices = bbp[mesh] *
v_local (model space), joint inverse bind matrix = inv(bbp[bone]), mesh node at the scene root.
`to_gltf.rs` does exactly this. The `check_gltf` evidence script confirmed world(joint) * IBM = I to 3e-5 on Max.

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
moves is made in each pack: the town one and the three dungeon ones, each from its own export.
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
0.50 (616), 0.40 (575), ... down to 0.00 (119, held poses). The export writes one key per motion
frame at 1/30 s (step 1.0); the step stays in the animation's `extras` and in info.cfg.

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
`GRAVITY x,y,z` (default (0,-0.6,0)), `K k`, `WIND scale`. Not needed for mesh import.
Cloth frames are found by name, so they keep working on an imported mesh if names are kept.

## 6. glTF export (`to_gltf.rs`)

`to_gltf::export` writes a character pack's folder as `<name>.gltf` and `<name>.bin` (30 fps).
`dc2tk mod asset ISO NAME chara/c02a.chr --as gltf` and `dc2tk assets` run it, through `models.rs`,
which puts the pack's own textures in place of the placeholders.
- glTF node i = MDS node i (`extras.mds_index`, `extras.mds_name` keep the full name with flags).
  Local TRS comes from decomposing the MDS matrix. 4 two-root files get a synthetic `_mds_roots`
  parent, appended last.
- rigid meshes sit on their frame node. Skinned meshes go on `<name>_skin` nodes at the scene
  root, with one skin over all frames (joint i = node i) and IBM = inv(bbp) (inverse of the MDS
  world matrix where the bbp entry is missing or singular).
- primitives: one per (material, attribute set). Corners are deduplicated on (v, n, uv, c).
  Degenerate strip triangles are dropped. Attributes are POSITION, NORMAL, TEXCOORD_0, COLOR_0
  (float), JOINTS_0 (u16) and WEIGHTS_0. Unweighted vertices get the engine's default (slot 0, weight 1).
- materials: name = texture name. The base colour texture is `textures/<name>.png`, an 8x8
  placeholder until .img decoding exists. Raw MDT fields are kept in `extras`. diffuse.a < 1 gives BLEND.
- animations: one per KEY of every motion set the pack holds a .mot for (a name given twice in a
  set gets `_<start>`), sampled every frame from start to end with the engine's interpolation.
  LINEAR, time = (frame - start) / 30 (3.2). Set 0's KEYs (else the first set's) keep their
  names; another set's are named `<set>:<name>` (`3:bow`, with `extras.motion_set`). Rotation,
  translation and scale lists drive the node their `frame` numbers (before, lists went to the
  first node of the same lower-case name, which put 1,866 lists of nodes named alike, `3` and
  `3--ba`, on the wrong node). `extras` keeps start/end/step.
- the other lists are morph targets of their frame's mesh, named in `extras.targetNames`, and the
  animations drive the mesh's `weights`:
  - vertex lists (12): a target `c2_frame_<f>` (`c2_set<id>_frame_<f>` for another set) for every
    frame f a KEY of the set plays, moving each vertex a list drives to its value at f (through
    the bind matrix for a skinned mesh). An animation gives weight 1 to its frame's target and 0
    to the others, so the linear blend between two frames is the engine's lerp.
  - material fade (40), colour (41) and visibility (50, 51) lists: carriers with no
    displacement, `c2_fade_<k>`, `c2_red_<k>`/`c2_green_<k>`/`c2_blue_<k>` (material k of the
    mesh), `c2_visible` (`c2_visible_51`), whose weight is the list's value (the fade, 1 - alpha;
    the colour; below 1 hidden). The mesh's default weights are the rests (no frame, the
    material's own colour, shown).
  The standard places are not used because Blender 5.1 cannot take them back: its importer stops
  on a `KHR_animation_pointer` channel to a textured material's `baseColorFactor` (a bug in
  `animation_pointer.py`: `base_color_socket.links`), its exporter writes pointer animations only
  in the NLA Tracks or Scene modes and starts them a frame late, and it does not read
  `KHR_node_visibility`. Shape keys and their values come back from Blender as written.
- a pack of motions alone (`MOTION 7` and no model: `chara/c01_fishing.chr`, the event packs of
  Max's and Monica's motions) is exported on the model pack whose skeleton it plays on
  (`models::skeleton_for`): Max's `chara/c01_base.chr` for a name with `mints` or `c01`, Monica's
  `chara/c02_base.chr` for `monica` or `c02` (their town and dungeon packs share one skeleton),
  else the pack beside it sharing the longest word of its name (`e352_guiltoni.chr` for
  `vs_e352_guiltoni.chr`; numbers dropped, so `mints1` is `mints`): the first whose model has
  every frame its lists and attach points name, else the first with every frame its lists name
  (`taxi_ride_c01`'s attach points are the taxi's). The glTF (named after its first .mot) has
  that pack's skeleton, skin, meshes and pictures, to see the motions on. All 244 such packs on
  the disc export and build back unchanged; through Blender, `c01_fishing`, `vs_e352_guiltoni`,
  `monica_set`, `c01b_win`, `taxi_ride_c01`, `p4_mints1`, `e208_mints` and `e412_monica` come
  back unchanged, and a bone turned in one of `c01_fishing`'s actions changes its set 7 alone.
- Robustness: all 1,950 character packs with a MODEL export, and the self-check (accessor bounds,
  index ranges, joints, weight sums) passes on every one. 670 have skins. For 422 of them the MDS
  rest pose differs from the bbp, so the rest pose shows deformed skin parts, as the engine would
  before a motion runs.

### 6.1 Options for map models (`to_gltf::Options`, `export_model`)

`to_gltf::export_model(model, out, base, options)` exports one MDS on its own (no info.cfg, skin or
motion), the way a map piece is; `models::export_map_model(data, stem, options)` does it for a
file's bytes. `Options::default()` is the character export above (its output is unchanged, byte
for byte, on all 1,950 packs). The options, all off by default:

- `full_names`: glTF nodes and meshes are named by the frames' whole names, `--` flags included
  (`grid54--f2appz0n1t-5_1`), since map meshes are told apart by them ([maps.md](maps.md) 4.1);
  meshes also get `extras.mds_index`. Nodes always carry `extras.mds_index` and `extras.mds_name`.
- `textures`: a `to_gltf::TextureSearch`, the pictures of a list of `.img` files searched by
  picture name (case-sensitive, the first file holding a name wins). Each material's picture is
  written as its PNG (`im3::to_png`) instead of a placeholder, and its sampler's wrap comes from
  the picture's GS CLAMP (`wrap_of`: WMS bits 0-1 and WMT bits 2-3; 0 repeat and 3 region repeat
  give REPEAT, 1 clamp and 2 region clamp CLAMP_TO_EDGE; IM2 pictures have no CLAMP and repeat).
  The texture's `extras` give the `.img` (`img`), the raw `gs_clamp`, and `no_image` for a picture
  registered without pixels (a `#texanime` fills it; it gets a placeholder), or `missing` when no
  file holds the name. `Exported` lists both kinds.
- `blend_flags`: the frame's name flags (1.5, read as `mgSetFrameAttr` reads them, with the
  `.`-marked flags of its ancestors first; `to_gltf::FrameFlags`) become material settings: `aPP`
  and `aNN` give `alphaMode` BLEND (glTF has no additive or subtractive blending), an alpha
  reference `aXX` above 0 gives MASK with `alphaCutoff` XX/0x80, `aOF` OPAQUE, and `c0`
  `KHR_materials_unlit`; otherwise BLEND when diffuse alpha is below 1, as before. The material's
  `extras.frame_flags` keep the flag text (`mds_flags`, `inherited_flags`) and what it decodes to
  (`blend`, `alpha_ref`, `lit`, `z_write`, `fog`). Materials are kept apart per flag set.
- `collision`: for `-a`/`-c` files ([maps.md](maps.md) 4.3). Each material is named for the
  per-triangle attributes its diffuse colour encodes (`col_ground0_foot2_area6_mask12`,
  `to_gltf::CollisionAttributes`), coloured by them (water areas 1 and 7 blue), untextured, with
  the attributes and the raw MDT fields in `extras`.

`Options::map(textures)` (whole names, textures, blend flags) and `Options::collision()` are the
two sets a map uses; `is_collision_file` tells them apart by file name.

### Sample: Max
An export made while the format was worked out: `c01_base_max.gltf` = `chara/c01_base.chr.d`
(skeleton, 23 clips from c01b.mot) with the meshes of `mainchr/c01fuku1.chr.d` (green default outfit, c01f.mds/.wgt/.bbp). 127 nodes,
14 meshes, 2,676 triangles, 7 materials, 4 placeholder textures (c01b01-04).
`preview_rest.png` and `preview_walk.png` (clip 歩き at 0.25 s) are renders by the `check_gltf` evidence script.
c01_base on its own would only give placeholder triangles.

## 7. Importing back: glTF -> MDS/WGT/BBP

### 7.1 `from_gltf.rs`

`from_gltf::import_gltf` takes a glTF, the template .mds and `ImportOptions` (optionally the template .bbp and .wgt and the meshes to replace),
and gives the new .mds, .wgt and .bbp; `build` runs it, through `models.rs`, for a mod's glTF
folder, with only the meshes that differ from the export. It keeps the template's nodes and
replaces the MDT block of the frame each glTF mesh node stands for (`from_gltf::frames_of`): the
node's `extras.mds_index` when it has one within the template, otherwise its name (Blender `.001`
and a skinned mesh node's `_skin` dropped; a frame may itself end in `_skin`, as e210a's
`neck_skin` does) as a frame's whole name, then as a frame's name without flags. The k-th node of
a repeated name takes the k-th frame of it, counting by Blender's `.NNN` (which numbers names in
the order it made them, the template's) and then in glTF order (Blender writes children first);
a name without flags takes every frame of that name in file order, unless the glTF names its nodes
by whole names, as a map's does, when a frame of the whole name comes first. Blender makes every
frame of a skinned model a bone and a mesh on one an object of the same name parented to it: a
rigid mesh node under a node of its name stands for the parent's frame (in a rigid model, objects
all, for the frame of its name under the parent's). Joints are found the same way.
Steps:
- rigid meshes are brought into frame space through the glTF hierarchy: through the node of their
  frame without a mesh (Blender's bone), or, when the mesh node is its frame's own (an export's),
  from the glTF's world to the frame's in the template, so a node left where it was changes
  nothing (before, such a mesh took its frame's local matrix twice) and one moved carries its mesh;
- skinned meshes are brought from model space into mesh space with inv(bbp[mesh]);
- joints are matched to template nodes by name, and the bbp of every joint is taken from the IBMs;
- positions, normals, uvs and colours are deduplicated into separate arrays;
- faces are written as one triangle list per glTF primitive, split so that each list stays under
  32,767/stride corners;
- the .wgt gets one type-20 block per (mesh, bone), at most 4 weights per vertex, summing to 100;
  template .wgt blocks for meshes that were not replaced are kept;
- materials come from the glTF material `extras` (raw MDT fields) or from the texture image name.
An unknown mesh name becomes a new node, appended under the root. Its name must then be added to
`VERTEX_ANIME` if it is skinned. `new_model` drops `extras.mds_index` from the glTF it builds a
model of, since those indices name another model's frames.

**Which meshes changed** is told by value against a fresh export, mesh by `from_gltf::MeshId`
(the template frame it stands for, or a new name), and only those that differ are passed to the
import, so meshes whose frames share a base name (1,488 map files) are told apart and an unedited
mesh keeps its bytes. Every triangle (degenerate ones dropped) must be matched by one of the other
side's in any corner order, winding kept, within what Blender moves: positions within 0.005 and
2e-6 of the value, UVs within 0.001, colours within 4/255, the zero UVs, white colours and
missing alphas Blender adds allowed (`same_geometry`). Characters
(`changed_character_meshes`, `frame_geometry`): a rigid mesh's corners in its frame's space (as
the import brings them there, so Blender's bone-parented objects match), a skinned mesh's in
model space, each corner's skin as (frame, weight) pairs (the joints as the frames they stand for,
the 4 largest weights normalised and summed by frame) with every weight within 0.01 (Blender
renormalises), materials by the picture they show. Maps (`changed_geometry`, `mesh_geometry`):
corners as the mesh node has them, its matrix within 1e-3 (translations as positions), materials
by name and picture. Before, characters were compared by accessor bytes, so after Blender every
mesh was rebuilt.

What Blender does to a character that is no edit, and how the build tells:
- **Bind matrices.** A joint's bind matrix (the inverse of its inverse bind matrix) counts as
  unchanged within 1e-3 of the export's (its translation, the bone's place, within 0.005 and
  2e-5 of the value), or of the bind matrix Blender makes of it (`exported_binds`): Blender's
  bones rest without scale or shear, so its importer takes each joint's bind relative to its
  parent's, drops the scale (`Matrix.decompose`, whose quaternion is Mike Day's method on Blender's
  axes, `blender_rotation`) and chains the results down the skeleton. For joints whose bind has
  scale or shear (`e612_griffon`'s claws) that is up to 0.05 off, and a short bone (0.07 long,
  200 units out: `b02a`'s `null202`) points only as well as 32-bit head and tail allow, 3e-4
  radians, which its inverse turns into 0.07 in the translation. Before, either rewrote the .bbp.
- **Skinned vertices.** Blender moves a skinned mesh's vertices to fit the bones it rests
  (`skin_into_bind_pose`); the build brings them back, each vertex by its weights' blend of bind
  (the export's, or the one kept) times the glTF's inverse bind matrix (`skin_matrices`), both to
  compare and to import, so they come back where they were.
- **Animated objects.** Blender writes an animated object's transform as its action leaves it at
  the scene's frame, not its rest, so a rigid model's meshes seemed moved (`denkousekka_h`,
  `e244a`, `e516_gun_m`). Every node a translation, rotation or scale channel animates is taken at
  its frame's rest (`animated_nodes_at_rest`), its own transform being its animation's; a node no
  animation moves still carries its mesh where it was put.

**Map models** (`models::build_map_model(given, original, options)`): the same, against an export
with the same `to_gltf::Options`. With `options.collision` the import runs in collision mode
(`from_gltf::ImportOptions::collision`): only positions are read (Blender's
normals, uvs and colours are dropped) and every prim is a type 0x213 triangle list of stride 1;
each material's attributes come from its name (`col_ground0_foot5_area9_mask4`, `.001` allowed;
missing parts are 0), written as k/0.7 and 1-mask as float32 (which decode back exactly for every
k from 0 to 255), unless its `extras` still encode those attributes, in which case its exported
fields are kept. Nodes stay as the glTF has them (one MDS frame per mesh node), so collision stays
in several compact frames. `from_gltf::collision_problems` lists what breaks the engine's rules
(another prim type, partial triangles, bad indices).

Round trip (the `roundtrip_gltf` evidence script: export -> import -> export, then compare every
triangle's corner positions, normals, uvs and weights to 1e-3 and the winding):
- Max (c01_base + c01fuku1): 14/14 meshes identical. Unused vertices are dropped
  (skin1 344 -> 341 vertices).
- e230_utan (24 meshes) and b02a (25 meshes, skinned + rigid): all identical.
- c01_base + c01fuku4: 11/14 identical. The 3 that differ are 1-triangle placeholder meshes whose
  three vertices share one position, so the import merges them into a 0-area triangle. Harmless.
- The rebuilt .mds parses and re-writes byte for byte through `mds.rs`.

Every character pack of the disc (2,663 files) is checked as Blender gives it back (extras
dropped, rigid meshes of skinned models moved onto child objects, names numbered `.001` on, nodes
children first): no mesh changes; its first mesh moved changes that mesh alone, and rebuilt it
leaves every other frame and weight list as it was and its own vertices' weights among the
original's. Through Blender 5.1 (import at 30 fps, export glTF Separate, `--background
--factory-startup`), unedited round trips of Max (`chara/c01_base`, and his costume
`mainchr/c01fuku1`), Monica (`chara/c02_base`), the monsters `e01a` and `e226d` and the villager
`p01_01a` (Need's mayor) rebuild no mesh and change no member; one mesh's vertices moved (a
skinned body, or a rigid head part such as `kami`) rebuild that mesh alone, with its weights (the
same set of per-vertex weights; vertices only degenerate triangles used are dropped) and every
other mesh's, the bind pose and the motions kept. Unedited Blender round trips of 58 packs
(those, `e210a`, `b02a`, `e612_griffon`, `denkousekka_h`, `e244a`, `e516_gun_m`, the vertex,
fade and visibility packs, packs of motions alone, and every 97th character pack of the disc)
change no member.

### 7.2 What any importer has to respect

Nothing VU-specific: the file is an indexed mesh and the engine batches it. Requirements:
1. **Keep the skeleton.** Animations, info.cfg (OBJECT_NAME*, OUTLINE, VERTEX_ANIME), .clo,
   LoadSkin and shadow links all find frames by name, and .mot/.wgt use node indices. Safest is to
   take the original MDS as a template: keep node order, names (with `--` flags), parents and
   matrices, and replace only MDT blocks (and add mesh-only leaf nodes at the end if needed). A
   costume must keep the base skeleton's names. Bone bbp should equal the base character's, since
   `CreateChangeFrame` copies only the mesh nodes' bbp.
2. **Geometry.** Split glTF vertices into separate position / normal / uv / colour arrays
   (deduplicate each). Write triangle lists (type 3, or 0x103 with colour, +0x10 without uv,
   +0x200 without normals) or stripify to type 4 (fewer indices; the engine handles either). Keep
   one FACES_ID per material run. counts are stored as u32 but the engine keeps them as shorts
   (`mgCFace.vertex_num`, `index_num = count*stride`), so keep each prim under 32,767/stride
   corners. Winding CCW. Vertex/normal w = 1, uv = (u, v, 1, 0).
3. **Skinned meshes.** Transform positions back with inv(bbp[mesh]) into the mesh node's space.
   Write the mesh node's bbp, and write .wgt blocks (frame = mesh node, target = bone node,
   type 20, keys (vertex index, weight*100)). Limits: <= 32 bones per mesh, <= 4 weights per
   vertex, weights summing to 100. The node must be listed in `VERTEX_ANIME` (max 24 names). The
   CPU path (SHAPE_ANIME/shadow) also warns above 400 vertices / 800 normals per mesh
   (`MotionProc3`).
4. **Materials.** Texture names <= 31 bytes cp932, matching textures in the pack's .img.
   diffuse (1,1,1,1), 0x10 (1,1,1,0), power 50 as defaults.
5. **Layout.** Use `mds::build()` (style A layout). The pack writer must keep members 16-aligned.
6. **Budget.** Memory comes from fixed character stacks: the MDS visuals and packets are built
   into `base_stack`, and `_MODEL` uses a 6400-qword work buffer (3,198 vertices a skinned mesh).
   See [the memory budgets](memory.md#4-character-packs): the party's packs nearly fill theirs. POLY_NUM in info.cfg may also need updating
   (meaning not traced).

### 7.3 Motions from glTF (`motions.rs`)

`motions::build(exported, given, model, member, info, rebuilt)` gives the pack's rebuilt .mot
files (by member) and info.cfg with the given glTF's animations written in, or nothing when no
animation differs from the export's and none is gone; `models::build` runs it after the meshes,
`models::build_motions` for a pack of motions alone (on its skeleton pack's model, 6). Each
motion set the pack holds a .mot of is rebuilt on its own timeline. Animations are matched to
KEYs by name (the export's names, `_<start>` on a repeated one, `<set>:` before another set's).
A new animation goes to the set its name gives (`3:curtsy`), to its source's set when it is a
copy (`3:bow.001`), else to set 0 (else the pack's first), its KEY named without the set.

**Frames.** A channel's node stands for a frame by its `extras.mds_index`, or else by name: a
name with `--` flags is a frame's whole name, one without the name of every frame of it; among
those, the frames under the frame the node's parent stands for; nodes of one name under one parent
take that name's frames in file order, by Blender's `.NNN` (which numbers names in the order it
made them) and then in glTF order. Blender splits a rigid mesh off its bone as an object of the
same name parented to it: both stand for the frame, and the bone's channels count. A skinned
mesh node stands for none.

**Morph weights.** A mesh node's `weights` channel gives its frame's vertex, material and
visibility channels (6): its targets by `targetNames` (`to_gltf::Target`), each vertex of the
frame's mesh matched to the glTF's vertices by place (as the import brings them into its space,
`from_gltf::morph_meshes`), its value its place plus each frame target's move times its weight;
the carriers' values their weights. An animation that drives a mesh's weights leaves the channels
it has no target for at their rests (the vertex's place, the material's own colour, shown); one
that does not (a tool that dropped the targets) leaves them as they were.

**What changed.** Each animation is sampled at its KEY's frames (frame r at r/30 s, glTF
interpolation: LINEAR with slerp for rotations, STEP, CUBICSPLINE), held after its last key, and
compared with the export's: rotations within 3e-3 radians, translations, scales and vertices
within 1e-3 (plus 2e-5 of the value), fades, colours and visibility within 1e-3. A channel no
list drives counts as unchanged while it holds a rest: the export's node, or the given node's,
since Blender rests a bone at its bind pose (from the inverse bind matrices) and writes that for
every bone an action leaves alone. A Blender round trip moves rotations by up to 7e-4 and
translations by a few millionths, and reduces constant channels to two keys. An animation changed
when a channel differs or it got longer (more than half a motion frame); one that got shorter
changed only if what it holds differs.

A translation of a frame under ancestors shrunk to a scale s (the product of each ancestor's
smallest scale at that time, the larger s of the export's and the given animation) is allowed
1/s times the tolerance: it moves the frame in the model by s times as much. This is what moved
`e210a`'s KEYs: the monster hides its legs by scaling its upper legs to 0.0001 in its guard,
attack, flail and getting-up motions, and Blender's exporter takes each bone's
local matrix from 32-bit armature-space matrices, which leaves the translations of the frames
below them (`chn353`, `eff351`, the lower legs) only to about 1e-6/1e-4, 0.01 to 0.04. A frame
an edit brings back to full size is compared as any other.

**Where it goes.**
- An edited motion of the same length is written over its own frames: each changed channel's
  list gets the given values at the KEY's frames, the frames another KEY also uses keep theirs
  (the motion moves instead when they would change), and the frames just outside get a key of
  their old value if they had none, so no other KEY's motion moves. The KEY line stays.
- A retimed motion, one whose shared frames change, and a new animation go after the last frame
  any list or KEY of its set uses (`last`): every list gets a key per frame of the new range, and
  a key at `last` holding its last value if it ended before, so the frames before keep their
  values. Lists take the animation's values (a node's rest where it does not animate one; the
  rests where it drives a mesh's weights without a target; a channel it leaves as it was, of a
  motion moved without a retime, keeps its own keys), and lists the glTF carries nothing of take
  the source motion's values, stretched to the new length. The KEY's start and end are rewritten
  in info.cfg; a new animation gets a `KEY "name", start, end, step;` line after its set's last
  KEY, its step from `extras.step`, else that of the KEY it is a copy of (`歩き.001` of `歩き`,
  as Blender names a duplicated action, which also gives its source motion), else 1.00 (30 frames
  a second, as Blender plays it). Names are 1 to 35 bytes of Shift-JIS.
- A channel no list drives that an animation moves gets a new list: the rest at frame 0 and
  `last`, the animation's values at its frames, the rest around them. New lists go among their
  node's lists (material, rotation, scale, translation order), never right after a vertex group;
  a new vertex list joins the end of its frame's vertex lists, and every frame's vertex lists
  are keyed at the same frames (MotionProc reads the group with the first list's key numbers).
- A mesh the build rebuilt numbers its vertices anew: its vertex lists are made again, a list for
  each vertex its frame targets move, keyed at every frame a KEY of the set plays with that KEY's
  animation's values.
- Lists nothing writes to keep their bytes; rotations written are turned to the side of the key
  before them.

**Deleting an action.** A KEY whose animation the given glTF no longer has (and which animated
something) is taken out: its KEY line, the `SE`/`SELP` cues and `SEQ` steps naming it (when no
KEY of that name is left; a sequence left without steps goes too; `SE_START`'s count stays, as
some of the disc's are already larger than their lines), and the frames of it no other KEY plays
from every list of its set, with a key of the value it held just before and after each run taken
out, so every other KEY's frames read as before (a list keyed only there keeps its keys). A glTF
with no animation at all deletes nothing (a tool may have left them out). KEYs after it are
counted one lower by scripts that play motions by number (`GetKeyListIndexPtr`), and a script
that plays it by name finds none. info.cfg is edited in place (`info_cfg::statements` gives each
argument's bytes), so comments and layout stay. SE cues are fractions of their KEY on the disc
(3.3) and follow a moved one.

Checks: an unedited export changes nothing in any of the 1,628 pack files with motions, nor
without its node extras and with repeated names numbered as Blender numbers them; a bone of each
pack's first motion turned 30 degrees comes back from a fresh export of the rebuilt pack as given,
and every other motion as it was (1,280 written in place, 348 moved); and that motion's action
deleted takes its KEY out and leaves every other motion as it was (packs of one motion, or whose
KEY's name another repeats, aside). All 244 packs of motions alone export and build back
unchanged. Through Blender 5.1 (import at 30 fps, export glTF Separate, `--background
--factory-startup`): unedited round trips of 58 packs change no motion, among them Max
(`chara/c01_base`, `mainchr/c01_base2`), Monica (`chara/c02_base`), `e210a` (its shrunk legs),
`b02a`, `p00_23a_hibasira` (rigid, names repeated under several parents), `denkousekka_h`
(fades), `e206_cloth`, `e612_gri_kao`, `e372_waku1`, `p5_hata` (vertex lists), `e616_osmond`,
`e532_guil_kage`, `e10a` (visibility), `e206_h_polcurse` (sets 0 and 5) and packs of motions
alone (`c01_fishing`, `vs_e352_guiltoni`, `p4_mints1`); a bone turned in one action changes that
bone's list over that KEY's frames alone (or moves the KEY, when another shares them); a
duplicated action becomes a new KEY, and a stretched one moves to new frames with its KEY. Edited
in Blender: a fade shape key's values changed in `denkousekka_h`'s action rewrites its material
list, a vertex of a frame shape of `e206_cloth` moved rewrites that vertex's list, `c2_visible`
keyed shown in an action of `e616_osmond` rewrites its visibility, and Max's run (`走り`) deleted
takes its KEY line and its two sound cues out.

### 7.4 New characters (`models::new_pack`)

A mod's `file:` line with `like` makes a pack from a glTF alone (`dc2tk-script`'s `discfiles::made`),
with the pack `like` names as the template for the formats: its info.cfg's other lines, its MDS
version and its first picture's texture format. Members, in this order:

- `<stem>.img`: each material's base colour image as a picture of the name the material uses (the
  image's name, else its file's), PNG with sides powers of two, in the template's picture format.
- `<stem>.mds` (`from_gltf::new_model`): a frame for every glTF node, named as the node (names
  made distinct; several roots under one frame named `<stem>`), placed by its translation,
  rotation and scale under its parent; each mesh node's triangles as its frame's mesh. A rigid
  mesh's vertices are in its frame's space; a skinned mesh's frame (Blender's mesh object, under
  the armature) has its vertices in its bind space, inv(bbp[mesh]) times the glTF's model-space
  positions, as `import_gltf` writes any skinned mesh.
- `<stem>.bbp`: every frame's bind matrix, the frame's world matrix unless the skin's inverse
  bind matrix differs (2.3); `_MOTION` copies it over the base matrices, which skinning needs.
- `<stem>.wgt` (with a skin): a type-20 block per (skinned mesh, bone) with keys, at most 4
  weights a vertex summing to 100 (2.2).
- `<stem>.mot` (`motions::new_motion`): every animation that moves a frame, or fades, colours,
  hides or shows a mesh, becomes a KEY, in glTF order, on one timeline from frame 1 (no gaps); a
  channel any animation moves off its rest gets one list of a key every frame from 0 (the rest) to
  the last, the animation's values over its KEY's frames (sampled at r/30 s, as 7.3 reads them)
  and the rest over the others'; channels none moves get no list, so their frames keep the
  model's matrices and materials. The fades, colours and visibility are the carrier shape keys of
  6 (`c2_fade_<k>`, `c2_red_<k>`/`c2_green_<k>`/`c2_blue_<k>`, `c2_visible`, `c2_visible_51`),
  read from the meshes' morph weights as 7.3 reads them, their rests the material's own fade and
  colour and shown (a visibility read within 1e-3 of 1 is written 1, since the engine hides below
  1 and a 32-bit key time lands a hair past its frame's); a frame's lists run material, rotation,
  scale, translation, visibility. A KEY's name is the animation's (1 to 35 bytes of Shift-JIS),
  its step `extras.step` or 1.00 (30 frames a second); Blender's frame rate does not matter,
  since glTF keys are in seconds.
- `info.cfg` (`models::new_info_cfg`): the template's, with NAME, IMG and MODEL naming the new
  members, `VERTEX_ANIME` naming the skinned meshes' frames just before MODEL (`_MODEL` reads the
  list), and `MOTION 0, "<stem>.mot", "<stem>.bbp", "<stem>.wgt";` with the KEYs where the
  template's first motion set was (else at the end). Dropped: the template's motion sets,
  sequences, sound cues (they name its KEYs), cloth, effects, shadow and LOD models, vertex and
  shape animation, and the OUTLINE and OBJECT_NAME lines naming frames the new model lacks
  (`_OBJECT_NAME2` and `_OUTLINE` take a missing frame quietly, but the attach point or outline
  would be lost anyway); BODY_SIZE, SCALE, POLY_NUM and the whole-model `OUTLINE ""` stay.

Refused: a skinned mesh weighted to more than 32 bones or of more than 3,198 vertices, more than
24 skinned meshes (`models::skin_problems`, 2.2 and 7.2), and a skin without an animation (no
motion set, no bind pose), and vertex animation (an animation driving `c2_frame_<f>` shape keys).
No cloth (`.clo`): it names a model's frames and particles. A pack built so reads back the same:
its own export has the glTF's meshes, skin weights and motions within the 7.1 and 7.3
tolerances, and building the glTF given over it changes nothing (checked on a two-bone test glTF,
with and without a fading and hiding action, on [the example](../examples/new-character/README.md),
a seven-bone character made in Blender, whose pack, built into a disc image and copied back out,
plays its action in Blender as made, and on [Brother Gill](../examples/spinning-cult/README.md),
eighteen bones, a skinned and a rigid mesh and six actions, one of them fading both meshes out
and hiding them, which come back through the pack's export and Blender's importer as made).

## 8. Evidence log (research scripts run while the formats were worked out; not in this repository)
- explore1/2: header/record/layout statistics over all 16,876 .mds (section 1).
- explore3: corner-slot index-range test per face type (index order).
- explore4: .mot/.wgt/.bbp block statistics.
- explore5: info.cfg tag/argument counts over 2,532 files.
- verify1: bbp vs MDS world matrices. Quaternion convention vs MDS local matrices (7,806 match
  as engine-transposed, 1,229 would match untransposed).
- verify2: winding vs normals.
- verify3: Max costume vs base skeleton bbp comparison, weight stats.
- verify4 (+ a follow-up query): bones per skinned mesh (model .wgt max 30; shadow .wgt up to 46), influences per vertex (max 5), weight sums (100).
- batch_export: exports + checks all 1,950 character packs.
- explore6: why some blocks need more than the canonical layout (the leftover style-B pad bytes).
- roundtrip_gltf: export -> from_gltf -> export comparison (section 7.1).
