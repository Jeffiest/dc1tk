# Character models: MDS, WGT, BBP, MOT, info.cfg, CLO

This page documents the character model, skeleton, skinning weight, and animation formats. A character pack (`.chr`) contains up to six file formats:

| Format | Role |
|---|---|
| `.mds` | Scene hierarchy and mesh container |
| `.wgt` | Vertex skinning weights |
| `.bbp` | Bone bind-pose matrices |
| `.mot` | Keyframe animation lists |
| `info.cfg` | Text script that binds models, textures, and motion clips |
| `.clo` | Cloth and dangling-part physics script |

- For editing character models in Blender via glTF, see [editing character models](../guide/models.md).
- For the `dc2tk` converter implementation, see [the implementation notes](../../src/docs/assets/models.md).
- All engine source paths are relative to `ps2/` in the ChronicleTwo decompilation.
- All corpus counts cover the extracted PAL disc (`SCES_511.90`) unless noted otherwise. See [the implementation notes](../../src/docs/assets/models.md#8-evidence-log-research-scripts-run-while-the-formats-were-worked-out-not-in-this-repository) for verification scripts.

## Open questions

- **Texture coordinates and pixels**: [textures.md](textures.md) documents the `.img` format. Materials reference textures by a case-sensitive `strcmp` call (see section 1.6). Both PS2 GS hardware and glTF place `(0, 0)` at the top-left corner, so no vertical UV (`v`) flip is expected.
- **Unused material fields**: The engine reads only `diffuse`, field `0x10` (likely ambient colour), and the texture name. It ignores `0x20` and `0x30` (likely specular power). Map files also contain 8,598 materials with `w = -11` or RGB values above `1.0`.
- **Secondary motion key types**: The behaviour of motion key types `12` (vertex morph), `40`/`41` (material alpha/colour), and `50`/`51` (visibility) after `MotionProc`—specifically which nodes `SKIP_CHILDREN` hides—is not fully traced.
- **CPU skinning path**: `SHADOW_MOTION` uses CPU skinning. In `MOTION`, `_MOTION` ignores the 4th argument (`.wgt`) and sets `entry[2].data = 0`. The engine always skins the main model on VU1 using `<model>.wgt`.

---

## 1. MDS: scene/mesh container

- **Loader call chain**:
  - `mgLoadMDSFile` (`src/mg_dataset.cpp:503`)
  - `CreateFrameVisual` (`src/mg_dataset.cpp:340-470`)
  - `mgCVisualMDT::DataAssignMDT`, `mgCVisualFixMDT::DataAssignMDT`, or `mgCVisualMotionMDT::DataAssignMDT` (`src/mg_visual.cpp:238-285`, `src/visualmotion.cpp:71`)
- **Struct declarations**: `MDS_HEADER`, `MDTOBJ_HEADER`, `MDT_HEADER`, `MDT_MATERIAL_`, `MDT_FACES`, and `FACES_ID` in `include/mg_dataset.hpp`.
- **Alignment**: The `.mds` buffer must align to 16 bytes in memory, or the engine prints `address error!!`. All archive pack members align to 16 bytes (see [the disc](disc.md)).
- **In-place data**: `CopyMDTDataPointer` points directly into the loaded `.mds` file buffer for vertex arrays.

### 1.1 Header (`MDS_HEADER`, `0x10` bytes)

| Offset | Size | Type | Field | Description |
|---|---|---|---|---|
| `0x00` | `4` | `char[4]` | `magic` | `"MDS\0"` in all 16,873 non-empty files (3 `.mds` files on the disc are 0 bytes). |
| `0x04` | `4` | `u32` | `unk_04` | Always `1`. Never read by the engine. |
| `0x08` | `4` | `u32` | `object_num` | Number of `MDTOBJ_HEADER` records. |
| `0x0C` | `4` | `u32` | `object_ofs` | Byte offset to the first object record. Always `0x10`. |

### 1.2 Object records (`MDTOBJ_HEADER`, `0x70` bytes each)

Object records sit back-to-back starting at `0x10`.

| Offset | Size | Type | Field | Description |
|---|---|---|---|---|
| `0x00` | `4` | `s32` | `index` | Record index (`0..object_num-1` in all 248,938 records). Never read by the engine. |
| `0x04` | `4` | `u32` | `size` | Record byte size. Always `0x70`. |
| `0x08` | `32` | `char[32]` | `name` | NUL-padded node name (`frame->SetName`). Text after `--` encodes flags (section 1.5). |
| `0x28` | `4` | `s32` | `mdt_ofs` | File byte offset to the mesh block (`0` = no mesh). Read by `CreateFrameVisual`. |
| `0x2C` | `4` | `s32` | `parent` | Parent node index (`-1` = root). Always satisfies `parent < index`. Four files have 2 roots. |
| `0x30` | `64` | `f32[16]` | `matrix` | Local transform matrix (`frame->SetTransMatrix(matrix)`). |

- **Matrix layout**: Column-major `sceVu0` 4×4 matrix (4 columns of 4 floats), matching glTF `matrix`.
  - Column `m[3]` holds translation `(tx, ty, tz, 1.0)`.
  - The bottom row is `(0, 0, 0, 1)` in all 34,000 sampled records.
  - `mgMulMatrix(P, A, B)` (`src/mg_math.cpp:672`) computes `P = A * B`. `GetLWMatrix` computes `LW = parentLW * local`.
  - Scale is unit `(1, 1, 1)` in 147,911 of 151,000 sampled nodes (`0.42`, `0.7`, and `2.0` also occur).
- **Mesh block placement**: MDT blocks immediately follow the object record array in record order with no gaps or trailing data.
- **Name comparison**: `SearchFrame` and `StrCmp` (`src/mg_frame.cpp`) compare node names only up to the `--` flag delimiter. `info.cfg`, `.clo`, `LoadSkin`, and shadow models all match frames using this rule.

### 1.3 MDT block (`MDT_HEADER`, `0x40` bytes)

| Offset | Size | Type | Field | Description |
|---|---|---|---|---|
| `0x00` | `4` | `char[4]` | `magic` | `"MDT\0"`. |
| `0x04` | `4` | `u32` | `header_size` | Always `0x40`. |
| `0x08` | `4` | `u32` | `block_size` | Byte distance to the next MDT block in all 92,490 meshes. Never read by the engine. |
| `0x0C` | `4` | `u32` | `vertex_num` | Number of vertices. |
| `0x10` | `4` | `u32` | `vertex_ofs` | Offset to `f32[4]` vertex array `(x, y, z, 1.0)` relative to the MDT block. |
| `0x14` | `4` | `u32` | `normal_num` | Number of normals. |
| `0x18` | `4` | `u32` | `normal_ofs` | Offset to `f32[4]` normal array `(nx, ny, nz, 1.0)` (unit length; 34 zero normals in corpus). |
| `0x1C` | `4` | `u32` | `colour_num` | Number of vertex colours. |
| `0x20` | `4` | `s32` | `colour_ofs` | Offset to `f32[4]` RGBA array (`0.0..1.0`). `0` or `-1` when unused. |
| `0x24` | `4` | `u32` | `faces_size` | Byte size of the faces section. |
| `0x28` | `4` | `u32` | `faces_ofs` | Offset to `MDT_FACES` (section 1.4). |
| `0x2C` | `4` | `u32` | `uv_num` | Number of texture coordinates. |
| `0x30` | `4` | `u32` | `uv_ofs` | Offset to `f32[4]` UV array `(u, v, 1.0, 0.0)` (~7% outside `0.0..1.0` for tiling). |
| `0x34` | `4` | `u32` | `material_num` | Number of materials. |
| `0x38` | `4` | `u32` | `material_ofs` | Offset to `MDT_MATERIAL_` array (`0x60` bytes each, section 1.6). |
| `0x3C` | `4` | `u32` | `reserved` | Always `0`. |

- **Independent index spaces**: Vertices, normals, UVs, and colours use separate arrays and indices (like Wavefront OBJ). For example, Max's `skin1` has 344 vertices, 318 normals, and 237 UVs.
- **Section order on disk**: Header, vertices, faces, padding, normals, UVs, colours, materials.

| Exporter Style | Corpus Count | Section Padding and Offset Rules |
|---|---|---|
| Style A | 92,412 blocks | Pads faces with `16 - (end % 16)` zero bytes (1..16 bytes). Adds 16 zero bytes after normals, after UVs (only if colours follow), and after materials. Sets `colour_ofs = 0` when no colours exist; sets `uv_ofs` even when `uv_num = 0`. |
| Style B | 78 blocks | Sets `colour_ofs = -1`. Pads only the faces section to 16 bytes (0..15 bytes; 38 blocks contain non-zero leftover bytes). |

### 1.4 Faces (`MDT_FACES` and `FACES_ID`)

| Struct | Offset | Size | Type | Field | Description |
|---|---|---|---|---|---|
| `MDT_FACES` | `0x00` | `4` | `u32` | `unk` | `0` (or `12255244` in 22 blocks). |
| `MDT_FACES` | `0x04` | `4` | `u32` | `header_size` | Always `0x10`. |
| `MDT_FACES` | `0x08` | `4` | `u32` | `prim_num` | Number of `FACES_ID` records following this header. |
| `MDT_FACES` | `0x0C` | `4` | `u32` | `zero` | Always `0`. |
| `FACES_ID` | `0x00` | `4` | `u32` | `type` | Topology and attribute bitflags (see table below). |
| `FACES_ID` | `0x04` | `4` | `u32` | `vertex_count` | Number of corners in this primitive. |
| `FACES_ID` | `0x08` | `4` | `u32` | `material` | Index into the mesh's material array. |
| `FACES_ID` | `0x0C` | `var` | `s32[]` | `index` | `vertex_count * index_stride` indices in `v, [n], [uv], [c]` order. |

**Primitive `type` flags** (`mgCVisualMDT::CreateFace` in `asm/pal/nonmatchings/mg_visual/CreateFace__...s`):

| Bits / Mask | Value | Effect |
|---|---|---|
| `type & 7` | `3` | Triangle list (`vertex_count % 3 == 0`). |
| `type & 7` | `4` | Triangle strip (single strip per record, no restart markers). |
| `0x008` | `0` | Flat shading flag; never set on the disc. |
| `0x010` | Set | Omit UV index (`index_stride -= 1`). Set on 15,286 of 15,303 untextured primitives. |
| `0x100` | Set | Include vertex colour index (`index_stride += 1`). |
| `0x200` | Set | Omit normal index (`index_stride -= 1`). |

- **Default stride**: `index_stride` starts at `3` (`v, n, uv`). `AnimeDataInit` reads `index[0]` as vertex and `index[1]` as normal.
- **Corpus `type` distribution**: `0x4` (495,000), `0x104` (101,000), `0x3` (99,000), `0x103`, `0x213`, `0x14`, `0x204`, `0x304`, `0x203`, `0x114`, `0x314`, `0x13`.
- **Winding order**: 98.8% counter-clockwise (CCW), matching glTF. For triangle strips, even triangles use `(i, i+1, i+2)` and odd triangles swap the first two vertices `(i+1, i, i+2)`.

### 1.5 Name flags (text after `--`)

`conv_new_text` converts exporter `__` separators into `--`. `mgSetFrameAttr` (`src/mg_dataset.cpp:163`) parses flags after `--`:

| Flag | Effect |
|---|---|
| `v0` / `v1` | Disable or enable drawing (`vcN` sets `unk_84`). |
| `o1` | Always pass the Z-test. |
| `t` | Enable ambient boost. |
| `bY` / `bA` | Enable Y-axis billboard or full camera-facing billboard (`ba` appears on 3,014 nodes). |
| `mN` | Select VU program mode `N` (e.g. `atoramiria--m1-`). |
| `s1` | Enable VU program option. |
| `fN` | Set fog mode `N`. |
| `z0` / `z1`, `zp0` / `zp1` | Disable or enable Z-write; disable or enable depth bias. |
| `aPP` / `aNN` / `aOF` / `aXX` | Additive, subtractive, or opaque blending, or hex alpha test reference `XX`. |
| `n0` / `n1` | Disable or enable frustum clipping. |
| `c0` / `c1` | Disable (`c0` = unlit) or enable (`c1`) lighting. |
| `.` prefix | Propagate the flag to all child nodes in the subtree. |

### 1.6 Materials (`MDT_MATERIAL_`, `0x60` bytes)

`CopyMaterial` (`src/mg_visual.cpp:184`) reads only `diffuse`, `unk_10`, and `texture_name`:

| Offset | Size | Type | Field | Description |
|---|---|---|---|---|
| `0x00` | `16` | `f32[4]` | `diffuse` | RGBA diffuse colour uploaded to VU1. Animated by `.mot` key types `40` and `41`. Usually `(1, 1, 1, 1)`. |
| `0x10` | `16` | `f32[4]` | `unk_10` | Secondary colour uploaded to VU1 (ambient colour in Dark Cloud 1). Usually `(1, 1, 1, 0)`. |
| `0x20` | `16` | `f32[4]` | `unk_20` | Unused (`0`). |
| `0x30` | `4` | `f32` | `shininess` | Unused (`50.0` in 18,888 of 18,976 materials). |
| `0x34` | `32` | `char[32]` | `texture_name` | Texture name resolved via `mgCTextureManager::GetTexture(name, -1)` (70 records have junk after NUL). |
| `0x54` | `12` | `s32, f32[2]` | `pad` | Unused (`0`). |

- **Texture resolution**: Materials look up textures by name from the `.img` archives loaded by `IMG n, "file.img";` in `info.cfg` (`EnterIMGFile`).

### 1.7 Bounds and VU1 packets

- **Bounds**: Not stored on disk. `mgCreateBBoxSphere` (`src/mg_dataset.cpp`) computes each mesh's bounding box and sphere from the vertex array at load time.
- **VU1 packets**: `CreateFacePacket` (`asm/pal/nonmatchings/mg_visual/`) builds VIF `UNPACK` and GS `GIF` packets in memory at load time. Each VU1 batch holds `3 * floor(floor((vu1_offset - 2) / q) / 3)` vertices:

| Mesh Class | `vu1_offset` | Quadwords/Vertex (`q`) | Max Vertices/Batch |
|---|---|---|---|
| Rigid (`mgCVisualMDT`, uncoloured) | `0xB4` (`180`) | `3` | `57` |
| Rigid (`mgCVisualMDT`, coloured) | `0xB4` (`180`) | `4` | `42` |
| Skinned (`mgCVisualMotionMDT`) | `0xB4 - 2 * bones` | `5` (includes 2 weight qwords) | `3 * floor((178 - 2 * bones) / 15)` |

Triangle strips that cross a batch boundary repeat their last 2 vertices at the start of the next batch.

---

## 2. Skinning: VERTEX_ANIME, .wgt, .bbp

### 2.1 Skinned vs. rigid meshes

- `VERTEX_ANIME "skin1", "skin2", ...;` in `info.cfg` names up to 24 meshes for skinning (`_VERTEX_ANIME`).
- When `_MODEL` (`src/character.cpp:1582`) loads `<model>.mds`, it also loads `<model>.wgt`.
- Named frames become `mgCVisualMotionMDT` instances (VU1 hardware skinning), or plain `MDT` instances when `SHAPE_ANIME 1` selects CPU vertex animation.
- Unnamed frames remain rigid and render with their node transform matrix (e.g. Max's `skin1`, `skin2`, `R_te`, and `L_te` are skinned; `kami`, `kao`, and `kaban` are rigid).

### 2.2 `.wgt` (vertex weights, block type `20`)

`.wgt` files use the binary key-list format (section 3) with block `type = 20` (all 310,379 blocks on the disc):

| Field | Meaning in `.wgt` |
|---|---|
| `header.frame` | MDS node index of the skinned mesh. |
| `header.target` | MDS node index of the influencing bone. |
| `header.type` | Always `20`. |
| `key.frame` | Vertex index within the mesh. |
| `key.value.x` | Weight percentage (`0.0..100.0`). `y`, `z`, `w`, and `pad` are `0`. |

**`mgCVisualMotionMDT::CreateVertexWeight` rules** (`asm/pal/nonmatchings/visualmotion/`):

| Rule | Limit / Behaviour |
|---|---|
| Empty blocks | Blocks with `key_count == 0` are skipped without allocating a bone slot. |
| Bones per VU1 mesh | Maximum **32 bones**. Exceeding 32 prints an error and disables skinning (main `.wgt` files use `<= 30`; shadow `.wgt` files use CPU skinning and reach 46 bones). |
| Weights per vertex | Maximum **4 non-zero bone influences** (in file order). A 5th influence logs an error and forces normalisation (16 meshes in the corpus have 5). |
| Weight normalisation | Divided by `100.0`. Sums in `(0.0, 1.0)` are normalised; unweighted vertices default to weight `1.0` on bone slot `0`. |

### 2.3 `.bbp` (bind pose)

- Contains `object_num` 4×4 column-major float matrices (`object_num * 64` bytes; exact match in 2,186 of 2,189 files).
- Each matrix is the bind-pose **local-to-model** matrix of that MDS node (root is identity; unused nodes are sometimes all-zero).
- `_MOTION` (`src/character.cpp:1820`) copies `.bbp` into `root->init_matrix`, overriding the initial world matrices from `.mds` (74,000 of 151,000 node matrices differ between `.bbp` and `.mds`).

### 2.4 Skinning formula

From `CreateExtRenderInfoPacket` (`src/visualmotion.cpp`) and `MotionProc2` / `MotionProc3` (`src/gameutil.cpp:501-660`):

```
v_model = sum( weight_i * (boneLW[i] * inv(bbp[bone_i]) * bbp[mesh] * v_mesh_local) )
```

Normals transform by the upper-left 3×3 rotation submatrix of the blended matrix.

### 2.5 Costumes (`LoadSkin`)

Max's base pack (`chara/c01_base.chr.d/c01b.mds`) holds only his skeleton and 3-vertex placeholder triangles. Body meshes live in costume packs (`mainchr/c01fuku1.chr.d`: `c01f.mds`, `c01f.wgt`, `c01f.bbp`, `c01b.img`). `CCharacter2::LoadSkin` -> `_SKIN_MOTION` -> `CreateChangeFrame` (`src/character.cpp:2520`):
1. Loads the costume `.mds` and `.wgt` files.
2. Copies the costume `.bbp` matrices **only for skinned mesh nodes** (`R_te` and `L_te` in `c01f.bbp`).
3. Remaps costume bone slots to base skeleton nodes by node name (`ChangeWeight`).
4. Attaches costume mesh visuals to matching frame names on the base skeleton.

### 2.6 Which pack Max and Monica come from

`SetupMainUnit` (`src/charasetup.cpp:100`) resolves the player's base pack via `GetMainCharaModelName(chara, name, alternate)` (`src/userdata.cpp:1114`):

| Mode | Callers | Max Base Pack | Monica Base Pack | Notes |
|---|---|---|---|---|
| Town (`edit = 1`) | `EditInit`, `MenuItemCharaDataLoad`, cutscene `load_party_member` (449 calls) | `chara/c01_base.chr` | `chara/c02_base.chr` | Idle (`立ち`) is frames `95..97` on Max. |
| Dungeon (`edit = 0`) | `InitDungeonMain`, `_DNG_SETUP_MAIN_UNIT`, `MenuItemCharaDataLoad` | `mainchr/c01_base2.chr` .. `c01_base4.chr` (`model_no + 2`) | `mainchr/c02_base0.chr` .. `c02_base3.chr` (`model_no`) | `model_no` (`0..3`) comes from `CGameDataUsed::GetModelNo` on the equipped melee weapon. Idle (`立ち`) is frames `362..364` on Max. |
| Addon motions | Dungeon scripts | `dungeon/chara/c01b_win`, `c01b_miss`, `c01_gio` | `dungeon/chara/c02b_*` | Loaded into extra motion slots without a mesh. |
| Shared equipment | Both modes (`LoadSkin` / `LoadPack`) | `mainchr/c01fuku*.chr`, weapons, hats, shoes | `mainchr/c02fuku*.chr`, weapons, hats, shoes | Shared across town and dungeon base packs. |

- **Unused base packs on the disc**: `mainchr/c01_base.chr`, `mainchr/c01_base0`, `mainchr/c01_base1`, `chara/c01_base_f.chr`, `chara/_c01_base.chr`, `chara/__c01_base.chr`, and `dungeon/chara/c01_base.chr` are never referenced.
- **Modding implication**: Because town and dungeon base packs have separate `.mot` timelines, replacing a core player animation requires updating `chara/c0N_base.chr` and all active `mainchr/c0N_base*.chr` packs.

---

## 3. `.mot` — motions (KEY LIST FILE)

`CreateAnimeDataEX` (`src/gameutil.cpp:807`) parses `.mot` files, and `MotionProc` (`src/gameutil.cpp:102`) evaluates them each tick. A `.mot` file is a sequence of blocks; each block has a `0x20`-byte header followed by `key_count` `0x20`-byte key records:

| Struct | Offset | Size | Type | Field | Description |
|---|---|---|---|---|---|
| `MotBlockHeader` | `0x00` | `4` | `s32` | `frame` | Target MDS node index (`root->GetFrame(list->frame)`). |
| `MotBlockHeader` | `0x04` | `4` | `s32` | `target` | Sub-target: material index (type `40`), `vertex_index + 1` (type `12`), or bone index (`.wgt`). |
| `MotBlockHeader` | `0x08` | `4` | `s32` | `type` | `MotionKeyType` (`include/gameutil.hpp`). |
| `MotBlockHeader` | `0x0C` | `4` | `u32` | `header_sz` | Always `0x20`. |
| `MotBlockHeader` | `0x10` | `4` | `u32` | `key_count` | Number of `MotKey` records in this block. |
| `MotBlockHeader` | `0x14` | `4` | `u32` | `more` | `(key_count + 1) * 32` when another block follows; `0` on the final block. |
| `MotBlockHeader` | `0x18` | `8` | `u32[2]` | `unk` | Uninitialised exporter memory; never read. |
| `MotKey` | `0x00` | `4` | `u32` | `frame` | Timeline frame number (strictly increasing). |
| `MotKey` | `0x04` | `12` | `u32[3]` | `pad` | Always `0`. |
| `MotKey` | `0x10` | `16` | `f32[4]` | `value` | Keyframe value `(x, y, z, w)`. |

- **Timeline structure**:
  - One `.mot` file holds all clips for a character on a single combined timeline (e.g. `1..379` in `chara/c01_base`, `1..598` in `mainchr/c01_base2`).
  - `SetMotionTime` evaluates all blocks on every tick, so every animated node must cover the full frame range of all `KEY` clips.
  - Almost all tracks store 1 key per frame (10,314,034 of 10,314,102 intervals have step `1`), starting at frame `1` (92,772 lists) or `0` (3,565 lists).
  - Blocks are ordered by ascending MDS node index, and within a node by type `40`, `0`, `1`, `2`.

### 3.1 Key types (`MotionProc`, `src/gameutil.cpp:102`)

`MotionProc` binary-searches for the last key `k0 <= (u32)time`, pairs it with `k1` (clamped to the last key), and interpolates with `t = (time - k0) / (k1 - k0)`.

> [!WARNING]
> If `time` is less than the first key's `frame` number, the binary search lands at index `-1` and reads out-of-bounds memory before the key array. Every block's first key must start at or before the lowest `KEY` start frame.

| Type | Name | Corpus Blocks | `target` | `value` Layout | Evaluation and Interpolation |
|---|---|---|---|---|---|
| `0` | Rotation | `58,590` | `0` | Unit quaternion `(w, x, y, z)` | Shortest-arc `QuatSlerp` for `0.001 < t < 0.999`, else snaps. `QuatToMat` (`src/mg_frame.cpp:123`) writes the transposed rotation matrix into the upper-left 3×3 of the local matrix (preserving translation, discarding MDS scale). Negate xyz for glTF: `(-x, -y, -z, w)`. |
| `1` | Scale | `9,957` | `0` | `(sx, sy, sz, 0)` | Linear `lerp` passed to `SetScale` (`use_srt`, applied separately from local matrix). |
| `2` | Translation | `17,506` | `0` | `(tx, ty, tz, 0)` | Linear `lerp` written to the local matrix translation column. |
| `12` | Vertex morph | `1,028` | `vertex + 1` | `(x, y, z, 1.0)` | Linear `lerp` written in place into the MDS vertex array. All type `12` blocks for a node must be contiguous, share identical keyframe numbers, and come last for that node. |
| `40` | Material alpha | `11,211` | Material index | `(alpha, 0, 0, 0)` | Sets `diffuse.a = 1.0 - lerp(alpha)` (`0.0` = opaque, `1.0` = transparent; `t` unclamped). |
| `41` | Material RGB | `0` | Material index | `(r, g, b, 0)` | Linear `lerp` into `diffuse` RGB; sets frame `unk_28 = 2`. |
| `50` | Visibility | `12` | `0` | `(vis, 0, 0, 0)` | Step (no interpolation): `vis < 1.0` sets `draw = 0`; `vis >= 1.0` sets `VISIBLE \| SKIP_CHILDREN`. |
| `51` | Child visibility | `0` | `0` | `(vis, 0, 0, 0)` | Step: `vis < 1.0` sets `SKIP_CHILDREN`; `vis >= 1.0` sets `VISIBLE`. |
| `30..33` | Camera tracks | `0` in `.mot` | `0` | Camera params | Animates `mgCCamera` position, target, roll, and FOV. |

- **Motion blending (`ChangeMotion`)**: While `blend < 1.0`, the engine blends the outgoing clip at `(int)(0.9 + frame)` with the incoming clip at `start` using `blend`. `blend` starts at `0.1` and adds `blend_speed` (`0.2` default, or from `SEQ`) each tick.

### 3.2 Time base

- PAL game logic runs at **25 Hz** (`mgFrameRate = 2` in `src/title.cpp`, waiting two 50 Hz VBlanks per tick).
- `CCharacter2::NormalDrive` (`src/character.cpp:974`) advances `frame` by `1.2 * step` per tick, giving **`30 * step` motion frames per second** (`step = 1.0` -> 30 fps; `step = 0.5` -> 15 fps; `step = 0.02` -> 0.6 fps).
- Looping clips wrap to `start` when `frame + 1.2 * step > end` (the exact `end` frame is reached only with `CHARA_MOTION_HOLD`).

### 3.3 Motion names and sound cues (`info.cfg`)

`_KEY` populates a `CHRINFO_KEY_SET` record (`0x30` bytes: `char name[0x24]; s32 start, END(); f32 step;`). A character supports up to 8 motion sets (`MOTION 0..7`).

| Statement | Arguments | Description |
|---|---|---|
| `MOTION` .. `MOTION_END` | `set, "x.mot", "x.bbp", "x.wgt"` | Binds `.mot` and `.bbp` to slot `set` (`0..7`). `_MOTION` ignores the `.wgt` argument. |
| `KEY_START` / `KEY` / `KEY_END` | `"name", start, end, step` | Registers a named clip (`name <= 35` CP932 bytes). Looked up by name (`GetKeyListPtr`) or index (`GetKeyListIndexPtr`); append new clips at the end to preserve indices. |
| `SEQ_START` / `SEQ` / `SEQ_END` | Clip names, blend speeds, loop counts | Chains multiple `KEY` clips into a sequence. |
| `SE_START` / `SE` / `SELP` / `SE_END` | `"clip", kind, bank, frame[, end_frame]` | Attaches a one-shot (`SE`) or looped (`SELP`) sound cue to `"clip"` (`_SE`, `src/character.cpp:2140`). When `frame <= 1.0` (all 1,915 cues on the disc), it is a fraction `[0.0, 1.0]` of `start..end` evaluated when the `SE` line is parsed; when `> 1.0`, it is an absolute frame. |

---

## 4. `info.cfg`

`CScriptInterpreter` (`src/scriptinterpreter.cpp`) parses `info.cfg` files (`TAG arg, ...;`, CP932, `//` and `/* */` comments). Only tags starting with uppercase `'A'..'Z'` match (`v2;` is ignored). Strings are quoted (`\"` escapes), tokens with `.` use `atof`, and other tokens use `atoi`.

**Character pack tags (`tag[]` in `src/character.cpp:178`, across 2,532 files)**:

| Tag | Arguments | Description |
|---|---|---|
| `NAME` | `"str"` | Debug name (ignored). |
| `IMG` / `IMG_END` | `idx, "x.img"` | Loads texture archive into slot `idx` (`< CHARA_IMAGE_MAX`). |
| `MATERIAL_ANIME` | `int` | Ignored. |
| `VERTEX_ANIME` | `"frame", ...` | Up to 24 MDS frame names to skin on VU1 (section 2.1). |
| `SHAPE_ANIME` | `0` or `1` | `1` uses CPU vertex animation instead of VU1 skinning. |
| `MODEL` | `"x.mds"` | Main mesh file; also loads `"x.wgt"`. |
| `OUTLINE` | `"frame", width` | Toon outline width (`""` = entire model). |
| `SHADOW_MODEL` | `"x.mds"` | Shadow mesh linked to main skeleton frames by name. |
| `SHADOW_VERTEX_ANIME` | `"frame", ...` | Shadow skinning frame names. |
| `BODY_SIZE` | `h, w, d` | Integer collision dimensions. |
| `POLY_NUM` | `a, b` | Polygon count metadata. |
| `SCALE` | `x, y, z` | Base model scale. |
| `OBJECT_NAME` / `OBJECT_NAME2` | `id, "frame", [float]...` | Attachment points (hip, hands, kick point, collision spheres). |
| `MOTION` / `MOTION_END` | `set, "x.mot", "x.bbp", "x.wgt"` | Motion set `0..7` (section 3.3). |
| `SHADOW_MOTION` | `"", "s.bbp", "s.wgt"` | CPU-skinned shadow bind pose and weights. |
| `KEY_*` / `SEQ_*` / `SE_*` | See section 3.3 | Animation clips, sequences, and sound triggers. |
| `CLOTH_START` / `CLOTH` / `CLOTH_END` | `n` / `"x.clo"` | Cloth physics scripts (section 5). |
| `POSITION` / `ROTATION` | `x, y, z` | Initial offset and rotation. |
| `EFFECT_START` / `EFFECT` / `EFFECT_END` | Effect params | Attached particle effects. |
| `LOD_MODEL_START` / `LOD_MODEL` / `LOD_MODEL_END` | `lvl, "m.mds", "m.wgt", "m.bbp", dist` | Level-of-detail meshes and switch distances. |

**Other `info.cfg` dialects**:
- **Costume packs (`LoadSkin`)**: `IMG`, `IMG_END`, `MODEL`, `MOTION`.
- **Map model packs (`.pcp` `info.cfg`, `src/mdslist.cpp`)**: `MDS "name.mds", "src_path"; TYPE n; FAR_CLIP d, fade; MDS_END;`.
- **Sky packs (`.sky`)**: `SKY_IMG`, `SKY_MDS`, `SUN_MDS`, `SKYB_MDS`, `SKY_ANIME`.

---

## 5. `.clo` (cloth / dangling parts), brief

`CDynamicAnime::Load` (`src/dynamicanime.cpp:625`) parses `.clo` scripts using `dynmc_tag` (72 files on the disc) for hair, coat, and strap spring-mass physics. Because `.clo` resolves MDS nodes by name, keeping original node names preserves cloth simulation on modified meshes.

| Statement | Arguments | Description |
|---|---|---|
| `FRAME_START` / `FRAME` / `FRAME_END` | `n` / `"frame"` | Maps MDS frame names to local cloth frame IDs `0..n-1`. |
| `VERTEX_START` / `VERTEX` / `VERTEX_L` / `VERTEX_END` | `id, x, y, z` | Defines simulation particles in world (`VERTEX`) or frame-local (`VERTEX_L`) space. |
| `FIX_VERTEX` / `FIX_VERTEX_C` / `FIX_VERTEX_S` | `frame, vertex[, weight[, ?]]` | Pins a particle to a frame (`_C` sets `velocity_rate = 1`; `_S` sets `-1`). |
| `FRAME_POSE` / `FRAME_POSE_L` | `frame, mode, v0, v1, v2, v3` | Drives a frame's orientation from 4 particles (`"bone"`, `"bone_xy"`, `"b_cdlr"`). `FRAME_POSE` also detaches the frame from its parent. |
| `DRAW_FRAME` | `id, ...` | Marks cloth frames for drawing. |
| `BIND_VERTEX_START` / `BIND_VERTEX` | `a, b[, rate]` | Adds a spring distance constraint between particles `a` and `b`. |
| `BOUNDING_BOX` | Box params | Sets bounding box parameters. |
| `COLLISION_START` / `COLLISION` | `"pipe", frame, cx, cy, cz, rx, ry, rz, axis` | Attaches a cylinder/pipe collider to `frame`. |
| `GRAVITY` / `K` / `WIND` | `x, y, z` / `stiffness` / `scale` | Sets gravity (default `(0, -0.6, 0)`), spring constant, and wind scale. |
