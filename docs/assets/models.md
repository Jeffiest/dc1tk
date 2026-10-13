# Dark Chronicle character models: MDS, WGT, BBP, MOT, info.cfg, CLO

The formats behind editing character models in Blender through glTF.
Engine source: the ChronicleTwo decompilation's `ps2/` (paths below are
relative to it). Corpus: the extracted PAL disc. All numbers are counts over
the whole corpus unless a sample is named; they come from scripts run over
all of it while the formats were worked out (the "evidence" scripts named
below).

The code, in `src/crates/c2-assets/src/`:

| file | what |
|---|---|
| `mds.rs` | MDS parse/edit/write |
| `mot.rs` | .mot/.wgt key-list files and .bbp tables |
| `info_cfg.rs` | info.cfg reader (same tokenising rules as `CScriptInterpreter`) |
| `to_gltf.rs` | character pack -> glTF 2.0 (.gltf + .bin + textures) |
| `from_gltf.rs` | glTF -> MDS + .wgt + .bbp, with the original MDS as a template (section 7) |
| `models.rs` | the glTF folders mods use (`mod asset --as gltf`, `build`) |

Mods use these through `models.rs`: textures come from the pack's own
`.img` files (not placeholders), only meshes that differ from the export are
rebuilt, and the template's bind pose is kept unless the glTF changes it.

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
- info.cfg syntax and all its tags. .clo is a text script of the same syntax (cloth/spring chains).
- Byte-exact round trip of all 2,159 .mot, 1,825 .wgt (128 empty) and 2,189 .bbp files.

**Not settled / open:**
- Texture pixels: `.img` is researched separately. A material refers to a texture only by
  name (case-sensitive `strcmp`, see below). UV v orientation still to be confirmed against decoded
  .img data. No flip is expected, because GS and glTF both put (0,0) at the top-left.
- Meaning of material fields 0x10 (ambient?), 0x20, 0x30 (power?) and odd diffuse values (w = -11,
  components > 1: 8,598 of them in map files). The engine only reads diffuse, 0x10 and the name.
- Motion time base: `KEY ... step` is frames advanced per game step. The export assumes 30 fps
  (not verified; it only scales time).
- Motion types 12 (vertex), 40 (material alpha) and 50 (visibility) are not exported to glTF.
- The CPU skinning path (`SHADOW_MOTION` and the .wgt given in `MOTION`'s 4th argument) is only
  partly followed. `_MOTION` ignores its 4th argument (sets `entry[2].data = 0`), so the main model
  is skinned only by the VU1 path, from `<model>.wgt`.
- The importer (`from_gltf.rs`) is experimental. It round-trips our own exports (section 7) but has
  not been tried on a Blender re-export or in the game.

---

## 1. MDS: scene/mesh container

Loader: `mgLoadMDSFile` (src/mg_dataset.cpp:503) -> `CreateFrameVisual` (mg_dataset.cpp ~340-470)
-> `mgCVisualMDT/FixMDT/MotionMDT::DataAssignMDT` (src/mg_visual.cpp:238-285, visualmotion.cpp:71).
Struct declarations: include/mg_dataset.hpp (`MDS_HEADER`, `MDTOBJ_HEADER`, `MDT_HEADER`,
`MDT_MATERIAL_`, `MDT_FACES`, `FACES_ID`).

The data must be 16-byte aligned in memory (`printf("address error!!")` otherwise). Pack members
are 16-aligned (`PACK_ALIGN` in `src/crates/c2-disc/src/archive.rs`). The vertex arrays are used in place
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
common (empty headers for every bone).

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
  2 translation (17,506), 12 vertex position (1,028; target = vertex index + 1), 40 material
  alpha (11,211; target = material index, alpha = 1 - x), 41 colour, 50/51 visibility (12),
  30-33 camera (none in .mot files).
- key frames ascending in every list. The first key is at frame 1 in 92,772 lists and at 0 in
  3,565. Interpolation: slerp (`QuatSlerp`, shortest arc) for rotation, lerp otherwise, between the
  two keys around the frame (binary search).
- **Rotation:** value = (w, x, y, z), unit length (175,703 of 175,703). `QuatToMat` (mg_frame.cpp:123)
  writes the transpose of the usual matrix, and the result *replaces* the 3x3 of the node's local
  matrix (scale from the MDS matrix is lost). glTF rotation = (-x, -y, -z, w).
- **Translation:** value.xyz replaces local translation (w = 0 in all).
- One .mot holds every motion of the character on one timeline (chara c01b.mot: frames 1..379; mainchr c01b.mot: 1..598).

### 3.1 Motion names
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
(`GetKeyListIndexPtr`, counting KEY entries in order). `step` is frames per game step:
walk 0.5, run 1.0, idle 0.02. `SEQ_START/SEQ/SEQ_END` chain named KEYs (blend speed, type, loop count).
`SE "name", foot, ?, phase` ties sounds to KEY names. Up to 8 motion sets (`MOTION 0..7`).

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
`c2 mod asset ISO NAME chara/c02a.chr --as gltf` and `c2 assets` run it, through `models.rs`,
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
- animations: one per KEY, sampled every frame from start to end with the engine's interpolation.
  LINEAR, time = (frame - start) / fps. Rotation, translation and scale only. `extras` keeps
  start/end/step.
- Robustness: all 1,950 character packs with a MODEL export, and the self-check (accessor bounds,
  index ranges, joints, weight sums) passes on every one. 670 have skins. For 422 of them the MDS
  rest pose differs from the bbp, so the rest pose shows deformed skin parts, as the engine would
  before a motion runs.

### Sample: Max
An export made while the format was worked out: `c01_base_max.gltf` = `chara/c01_base.chr.d`
(skeleton, 23 clips from c01b.mot) with the meshes of `mainchr/c01fuku1.chr.d` (green default outfit, c01f.mds/.wgt/.bbp). 127 nodes,
14 meshes, 2,676 triangles, 7 materials, 4 placeholder textures (c01b01-04).
`preview_rest.png` and `preview_walk.png` (clip 歩き at 0.25 s) are renders by the `check_gltf` evidence script.
c01_base on its own would only give placeholder triangles.

## 7. Importing back: glTF -> MDS/WGT/BBP

### 7.1 `from_gltf.rs` (experimental)

`from_gltf::import_gltf` takes a glTF, the template .mds and optionally the template .bbp and .wgt,
and gives the new .mds, .wgt and .bbp; `build` runs it, through `models.rs`, for a mod's glTF
folder, with only the meshes that differ from the export. It keeps the template's nodes and replaces MDT blocks by mesh name (`X` or `X_skin`; Blender
`.001` suffixes are dropped). Steps:
- rigid meshes are brought into frame space through the glTF hierarchy;
- skinned meshes are brought from model space into mesh space with inv(bbp[mesh]);
- joints are matched to template nodes by name, and the bbp of every joint is taken from the IBMs;
- positions, normals, uvs and colours are deduplicated into separate arrays;
- faces are written as one triangle list per glTF primitive, split so that each list stays under
  32,767/stride corners;
- the .wgt gets one type-20 block per (mesh, bone), at most 4 weights per vertex, summing to 100;
  template .wgt blocks for meshes that were not replaced are kept;
- materials come from the glTF material `extras` (raw MDT fields) or from the texture image name.
An unknown mesh name becomes a new node, appended under the root. Its name must then be added to
`VERTEX_ANIME` if it is skinned.

Round trip (the `roundtrip_gltf` evidence script: export -> import -> export, then compare every
triangle's corner positions, normals, uvs and weights to 1e-3 and the winding):
- Max (c01_base + c01fuku1): 14/14 meshes identical. Unused vertices are dropped
  (skin1 344 -> 341 vertices).
- e230_utan (24 meshes) and b02a (25 meshes, skinned + rigid): all identical.
- c01_base + c01fuku4: 11/14 identical. The 3 that differ are 1-triangle placeholder meshes whose
  three vertices share one position, so the import merges them into a 0-area triangle. Harmless.
- The rebuilt .mds parses and re-writes byte for byte through `mds.rs`.
It has not been run in the game or on a Blender re-export, so the Blender-specific parts are
unexercised: node naming, mesh objects parented to bones, Y-up/Z-up conversion.

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
   into `base_stack`, and `_MODEL` uses a 6400-qword work buffer. Larger meshes than the original
   may overflow. The limits are not measured yet. POLY_NUM in info.cfg may also need updating
   (meaning not traced).

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
