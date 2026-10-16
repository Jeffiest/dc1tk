# Character models: MDS, WGT, BBP, MOT, info.cfg, CLO

This page documents how the game stores and loads character models, skeletons, skinning weights, and animations. A character pack (`.chr`) combines several formats:
- `.mds`: scene hierarchy and mesh container
- `.wgt`: vertex skinning weights
- `.bbp`: bone bind-pose matrices
- `.mot`: keyframe animation lists
- `info.cfg`: text script that binds the model, textures, and motion clips together
- `.clo`: cloth and dangling-part simulation script

For a practical guide to editing character models in Blender via glTF, see [editing character models](../guide/models.md). For details on the `dc2tk` converter implementation, see [the implementation notes](../../src/docs/assets/models.md).

All engine source paths on this page are relative to `ps2/` in the ChronicleTwo decompilation. All corpus counts cover the entire extracted PAL disc (`SCES_511.90`) unless a specific sample is named. These counts come from the verification scripts listed in [the implementation notes](../../src/docs/assets/models.md#8-evidence-log-research-scripts-run-while-the-formats-were-worked-out-not-in-this-repository).

## Open questions

- **Texture coordinates and pixels**: The `.img` texture format is documented in [textures.md](textures.md). A material references a texture by name using a case-sensitive `strcmp` call (see section 1.6). The vertical UV (`v`) orientation still requires verification against decoded `.img` pixel data. No vertical flip is expected because both the PS2 GS hardware and glTF place `(0, 0)` at the top-left corner.
- **Unused material fields**: The meaning of material fields `0x10` (likely ambient colour), `0x20`, and `0x30` (likely specular power) is not fully confirmed. Map files also contain 8,598 materials with unusual diffuse values (`w = -11`, or RGB components greater than `1.0`). At runtime, the engine reads only the diffuse colour, field `0x10`, and the texture name.
- **Secondary motion key types**: The exact downstream behaviour of motion key types `12` (vertex morph), `40`/`41` (material alpha and colour), and `50`/`51` (visibility) beyond `MotionProc`—specifically which nodes `SKIP_CHILDREN` hides—is not fully traced.
- **CPU skinning path**: The CPU skinning path (`SHADOW_MOTION` and the `.wgt` file listed as the 4th argument to `MOTION`) is only partially traced. The `_MOTION` command handler ignores its 4th argument and sets `entry[2].data = 0`. Consequently, the engine always skins the main model on VU1 using `<model>.wgt`.

---

## 1. MDS: scene/mesh container

The engine loads `.mds` files through the following call chain:
- `mgLoadMDSFile` (`src/mg_dataset.cpp:503`)
- `CreateFrameVisual` (`src/mg_dataset.cpp:340-470`)
- `mgCVisualMDT::DataAssignMDT`, `mgCVisualFixMDT::DataAssignMDT`, or `mgCVisualMotionMDT::DataAssignMDT` (`src/mg_visual.cpp:238-285`, `src/visualmotion.cpp:71`)

The binary structures (`MDS_HEADER`, `MDTOBJ_HEADER`, `MDT_HEADER`, `MDT_MATERIAL_`, `MDT_FACES`, and `FACES_ID`) are declared in `include/mg_dataset.hpp`.

The `.mds` file buffer must align to a 16-byte boundary in memory. If the buffer is not 16-byte aligned, the engine prints `address error!!`. All archive pack members on the disc align to 16 bytes (see [the disc](disc.md)). `CopyMDTDataPointer` references the vertex arrays directly inside the file buffer, so the loaded file serves as the live runtime array.

### 1.1 Header (0x10)

| Offset | Type | Value | Evidence |
|---|---|---|---|
| `0x00` | `char[4]` | `"MDS\0"` | Present in all 16,873 non-empty files (3 `.mds` files on the disc are 0 bytes). |
| `0x04` | `u32` | `1` | `1` in all files. The engine never reads this field (`unk_04`). |
| `0x08` | `u32` | `object_num` | Loop bound (`mds->object_num`) for reading object records. |
| `0x0C` | `u32` | `object_ofs = 0x10` | `0x10` in all files. Byte offset to the first object record. |

### 1.2 Object records (`MDTOBJ_HEADER`, 0x70 each, back to back)

| Offset | Type | Field | Evidence |
|---|---|---|---|
| `0x00` | `s32` | Record index | Matches the record position in all 248,938 records. The engine does not read this field. |
| `0x04` | `u32` | `size = 0x70` | The loader advances by this byte count. Always `0x70`. |
| `0x08` | `char[32]` | Node name | NUL-padded (all padding bytes are zero). Passed to `frame->SetName`. Text after `--` encodes attribute flags (see section 1.5). |
| `0x28` | `s32` | `mdt_ofs` | Byte offset from the start of the `.mds` file to the mesh block (`0` = no mesh). Read by `CreateFrameVisual`. |
| `0x2C` | `s32` | Parent index | `-1` for a root node. Every record satisfies `parent < index`. Four files on the disc have 2 root nodes. |
| `0x30` | `f32[16]` | Local matrix | Passed to `frame->SetTransMatrix(matrix)`. |

**Matrix convention**: Each matrix uses the `sceVu0` 4×4 layout (4 columns of 4 floats). Column `m[3]` holds the translation `(tx, ty, tz, 1.0)`, and the bottom row is `(0, 0, 0, 1)` in all 34,000 sampled records. `mgMulMatrix(P, A, B)` (`src/mg_math.cpp:672`) computes `P = A * B` using column vectors, and `GetLWMatrix` computes the local-to-world matrix as `LW = parentLW * local`. This layout matches the standard glTF column-major `matrix`. Most nodes use unit scale (147,911 of 151,000 nodes), though scales such as `0.42`, `0.7`, and `2.0` also occur.

The MDT mesh blocks immediately follow the object record array in record order with no gaps. No trailing data follows the last MDT block in any file on the disc.

**Name lookup**: `SearchFrame` and `StrCmp` (`src/mg_frame.cpp`) compare frame names only up to the `--` flag separator. The `info.cfg` script, `.clo` cloth files, `LoadSkin`, and shadow model links all resolve frames by name using this comparison.

### 1.3 MDT block (one mesh)

| Offset | Field | Notes |
|---|---|---|
| `0x00` | `"MDT\0"` | 4-byte magic string. |
| `0x04` | Header size | Always `0x40`. |
| `0x08` | Block size | Equals the byte distance to the next MDT block in all 92,490 meshes. The engine does not read this field. |
| `0x0C` / `0x10` | `vertex_num` / `vertex_ofs` | Array of `f32[4]` `(x, y, z, 1.0)`. `w = 1.0` in every sampled vertex. |
| `0x14` / `0x18` | `normal_num` / `normal_ofs` | Array of `f32[4]` `(x, y, z, 1.0)`. Unit-length vectors, except for 34 zero normals across the corpus. |
| `0x1C` / `0x20` | `colour_num` / `colour_ofs` | Array of `f32[4]` RGBA in the `0.0..1.0` range (never `> 1.0`). Offset is `0` (or `-1` in style B) when the mesh has no vertex colours. |
| `0x24` / `0x28` | `faces_size` / `faces_ofs` | Face primitive list (see section 1.4). |
| `0x2C` / `0x30` | `uv_num` / `uv_ofs` | Array of `f32[4]` `(u, v, 1.0, 0.0)`. `z = 1.0` and `w = 0.0` hold in all 276,000 sampled UVs. About 7% of UVs fall outside `0.0..1.0` for texture tiling. |
| `0x34` / `0x38` | `material_num` / `material_ofs` | Array of `MDT_MATERIAL_` records (`0x60` bytes each). |
| `0x3C` | Reserved | Always `0`. |

Vertices, normals, UVs, and vertex colours use four separate index spaces. Each face corner independently indexes a vertex, a normal, a UV, and a colour, just like Wavefront OBJ. Array lengths differ within the same mesh (for example, Max's `skin1` mesh has 344 vertices, 318 normals, and 237 UVs).

**Binary section layout**: At runtime, the engine locates sections solely through the header offsets. However, byte-exact file reconstruction requires matching the original exporter section order: header, vertices, faces, padding, normals, UVs, colours, and materials. Two exporter styles appear on the disc:
- **Style A (92,412 blocks)**: Pads the end of the faces section with `16 - (end % 16)` zero bytes (always 1 to 16 bytes). It inserts 16 zero bytes after the normals, 16 zero bytes after the UVs only when a colour array follows, and 16 zero bytes after the materials. It sets `colour_ofs = 0` when no colours are present, and sets `uv_ofs` even when `uv_num = 0`.
- **Style B (78 blocks, `colour_ofs = -1`)**: Pads only the faces section to a 16-byte alignment (0 to 15 bytes). In 38 of these blocks, the padding contains leftover non-zero bytes.
- In 22 blocks, the first 32-bit word of the faces header contains uninitialised garbage (`12255244`).

### 1.4 Faces

```c
struct MDT_FACES {
    u32 unk;         // 0 (or 12255244 in 22 blocks)
    u32 header_size; // 0x10
    u32 prim_num;
    u32 zero;        // 0
};

struct FACES_ID {
    u32 type;
    u32 vertex_count;
    u32 material;
    s32 index[vertex_count * stride];
}; // Repeated prim_num times
```

- **Index stride**: In `mgCVisualMDT::CreateFace` (`asm/pal/nonmatchings/mg_visual/CreateFace__...s`), `index_stride` starts at `3` (`vertex`, `normal`, `uv`) and adjusts by the primitive type flags:
  - `+1` if bit `0x100` is set (has vertex colour)
  - `-1` if bit `0x200` is set (no normal)
  - `-1` if bit `0x10` is set (no UV)
  The corner tuple order is always `v, [n], [uv], [c]`. `AnimeDataInit` reads `index[0]` as the vertex index and `index[1]` as the normal index. Corpus range checks confirm that slot 0 always fits `vertex_num` and the final slot of `0x100` primitives always fits `colour_num`.
- **Primitive topology (`type & 7`)**: `3` = triangle list (`vertex_count % 3 == 0` always), `4` = triangle strip. These values match the PS2 GS `PRIM` register. `CreateFacePacket` handles type `4` specially by resending 2 vertices whenever it splits a strip across VU1 batches. Bit `0x8` (flat shading) is never set. Primitive types in the corpus are `4` (495,000), `0x104` (101,000), `3` (99,000), `0x103`, `0x213`, `0x14`, `0x204`, `0x304`, `0x203`, `0x114`, `0x314`, and `0x13`.
- **Strip structure**: Each `FACES_ID` record holds a single strip with no restart markers. Sampled models contain occasional degenerate triangles (9, 19, and 83 in tested packs).
- **Untextured primitives**: When a material has an empty texture name, all primitives using that material set bit `0x10` (true for 15,286 of 15,303 untextured materials).
- **Triangle winding**: Comparing geometric triangle normals against averaged corner normals across `mainchr` and `dungeon` models shows counter-clockwise (CCW) winding on 98.8% of triangle lists. Triangle strips are also 98.8% CCW when odd-numbered strip triangles swap their first two vertices (even triangles use `(i, i+1, i+2)`, odd triangles use `(i+1, i, i+2)`). This matches the default glTF CCW front-face convention.

### 1.5 Name flags (text after `--`)

`mgSetFrameAttr` (`src/mg_dataset.cpp:163`) parses rendering flags from the node name after the `--` delimiter. Before parsing, `conv_new_text` converts any exporter `__` separator into `--`.

| Flag | Effect |
|---|---|
| `v0` / `v1` | Turn drawing off or on. `vcN` sets attribute `unk_84`. |
| `o1` | Always pass the Z-test. |
| `t` | Enable ambient boost. |
| `bY` / `bA` | Enable Y-axis billboard or full camera-facing billboard. |
| `mN` | Select VU program mode `N` (for example, `atoramiria--m1-`). |
| `s1` | Enable VU program option. |
| `fN` | Set fog mode `N`. |
| `z0` / `z1`, `zp0` / `zp1` | Turn Z-write off or on; enable depth bias. |
| `aPP` / `aNN` / `aOF` / `aXX` | Set additive, subtractive, or opaque blending, or set the hex alpha reference `XX`. |
| `n0` / `n1` | Turn clipping off or on. |
| `c0` / `c1` | Turn lighting off (`c0` = unlit) or on (`c1`). |
| `.` before a flag | Propagate the flag to the entire node subtree. |

Common flag suffixes in the corpus include `ba` (3,014 nodes), `n1s1-`, `c0`, `N1`, `s1-`, `n1z0c0f2app-`, and `a40aof-`.

### 1.6 Materials (`MDT_MATERIAL_`, 0x60)

| Offset | Field | Engine Use |
|---|---|---|
| `0x00` | `f32[4] diffuse` | Copied to `mgMaterial.diffuse` and uploaded to VU1. Animated by `.mot` key types `40` and `41`. Usually `(1, 1, 1, 1)`. |
| `0x10` | `f32[4]` | Copied to `mgMaterial.unk_10` (ambient colour in Dark Cloud 1) and uploaded alongside `diffuse`. Usually `(1, 1, 1, 0)`. |
| `0x20` | `f32[4]` | Not read by the engine. Always `0`. |
| `0x30` | `f32` | Not read by the engine. Equals `50.0` in 18,888 of 18,976 materials (likely specular shininess). |
| `0x34` | `char[32]` | Texture name passed to `mgCTextureManager::GetTexture(name, -1)` (`strcmp` with optional global suffix). |
| `0x54` | `s32, f32, f32` | Not read by the engine. Always `0`. |

`CopyMaterial` (`src/mg_visual.cpp:184`) copies only `diffuse`, `unk_10`, and the resolved texture pointer. Across the corpus, 70 material records contain non-zero leftover bytes after the NUL terminator in the texture name field.

**Texture resolution**: Materials reference textures exclusively by name. The engine loads the textures from the `.img` archives declared via `IMG n, "file.img";` in `info.cfg` (`_IMG` and `_IMG_END` call `EnterIMGFile`). For example, Max's materials reference `c01b01` through `c01b04`, which reside in `c01b.img` inside his costume pack.

### 1.7 Bounds

`.mds` files do not store bounding volumes. `mgCreateBBoxSphere` (`src/mg_dataset.cpp`) computes each mesh's bounding box and bounding sphere from its vertex array at load time.

### 1.8 What the engine builds at load (why there is no VU data in the file)

At load time, `CreateFacePacket` (`asm/pal/nonmatchings/mg_visual/`) compiles each `FACES_ID` primitive into VIF `UNPACK` commands and GS `GIF` tags. Each VU1 batch holds:

```
3 * floor(floor((vu1_offset - 2) / q) / 3)
```

vertices, where `q` is the number of quadwords per vertex (`3` without vertex colour, or `4` with vertex colour). `vu1_offset` is `0xB4` for rigid `MDT` meshes (57 vertices per batch, or 42 with colour) and `0xB4 - 2 * bones` for skinned `mgCVisualMotionMDT` meshes. Triangle strips that cross batch boundaries repeat the last 2 vertices at the start of the next batch. Tools that write `.mds` files do not need to generate these packets.

---

## 2. Skinning: VERTEX_ANIME, .wgt, .bbp

### 2.1 Which meshes are skinned

In `info.cfg`, the `VERTEX_ANIME "skin1", "skin2", ...;` statement names up to 24 meshes for vertex skinning (`_VERTEX_ANIME`). When `_MODEL` (`src/character.cpp:1582`) loads `<model>.mds`, it replaces the extension to load `<model>.wgt`. It creates the named frames as `mgCVisualMotionMDT` instances for VU1 hardware skinning, or as plain `MDT` instances when `SHAPE_ANIME 1` selects CPU vertex animation. All other meshes remain rigid and render using their own node transform matrix. On Max (`c01fuku1`), `skin1`, `skin2`, `R_te`, and `L_te` are skinned meshes, whereas `kami` (hair), `kao` (face), and `kaban` (bag) are rigid meshes attached to bone frames.

### 2.2 .wgt (key-list file, block type 20)

A `.wgt` file uses the same key-list container format as `.mot` (see section 3). In each block:
- `frame` is the node index of the skinned mesh.
- `target` is the node index of the influencing bone.
- `type` is always `20` (all 310,379 blocks in the corpus have type `20`).
- Each key stores the vertex index in its `frame` field and the bone weight percentage (`0.0..100.0`) in `value.x`. `value.y`, `value.z`, `value.w`, and the key header padding are always `0`.

Exporters often write empty blocks (`key_count = 0`) for bones that do not influence a mesh. `CreateVertexWeight` skips blocks with `0` keys and only allocates a bone slot when a block contains at least one key. The CPU skinning path also ignores empty blocks, so a writer can safely omit them.

`mgCVisualMotionMDT::CreateVertexWeight` (`asm/pal/nonmatchings/visualmotion/`) applies the following rules:
- It processes only blocks whose `frame` matches the current mesh's node index.
- It assigns one local bone slot to each distinct bone. **A VU1-skinned mesh supports at most 32 bones.** If a mesh references more than 32 bones, the engine prints an error and disables skinning for that mesh. Every mesh in the corpus with more than 32 bones (up to 46) belongs to a shadow `.wgt` file, which uses the CPU `MotionProc2` / `MotionProc3` path and has no bone slot limit. All main model `.wgt` files on the disc use 30 or fewer bones per mesh.
- **Each vertex supports at most 4 bone influences** (the first 4 non-zero weights encountered in file order). A 5th influence triggers an error message and forces weight normalisation. Exactly 16 meshes in the corpus have 5 influences on a vertex.
- The loader divides weights by `100.0`. If a vertex's weights sum to a value strictly between `0.0` and `1.0`, the loader normalises them. If a vertex has no weights at all, the loader assigns a weight of `1.0` to bone slot `0` (the first bone listed for the mesh). All meshes in the corpus sum to `100%`.

### 2.3 .bbp (bind pose)

A `.bbp` file contains `object_num` 4×4 float matrices (`object_num * 16` floats). Each entry is the bind-pose **local-to-model** matrix of the corresponding MDS node (the root node is always the identity matrix). During `_MOTION` (`src/character.cpp:1820`), the engine copies the `.bbp` data over `root->init_matrix`, replacing the initial world matrices that `mgLoadMDSFile` computed from the MDS hierarchy. Out of 2,189 `.bbp` files on the disc, 2,186 contain exactly `object_num` matrices. Nodes that do not participate in skinning sometimes have all-zero `.bbp` matrices. Because `.bbp` bone matrices frequently differ from the MDS rest-pose matrices (77,000 match vs. 74,000 differ across the corpus), the `.bbp` file is the authoritative bind pose.

### 2.4 Skinning formula

From `CreateExtRenderInfoPacket` (`src/visualmotion.cpp`) and `MotionProc2` / `MotionProc3` (`src/gameutil.cpp:501-660`), with `base = rootLW * bbp`, the skinned model-space vertex position is:

```
v_model = sum( weight_i * (boneLW[i] * inv(bbp[bone_i]) * bbp[mesh] * v_mesh_local) )
```

Vertex normals use the upper-left 3×3 rotation part of the same blended matrix without translation.

### 2.5 Costumes (LoadSkin)

Max's base pack (`chara/c01_base.chr.d/c01b.mds`) holds only his skeleton and 3-vertex placeholder triangles. His actual body meshes live in costume packs such as `mainchr/c01fuku1.chr.d` (`c01f.mds`, `c01f.wgt`, `c01f.bbp`, and `c01b.img`). The engine loads costumes through `CCharacter2::LoadSkin` -> `_SKIN_MOTION` -> `CreateChangeFrame` (`src/character.cpp:2520`):
1. It loads the costume `.mds` and `.wgt` files.
2. It copies the costume `.bbp` matrices **only for the skinned mesh nodes**.
3. It remaps the costume's bone slots to the base skeleton's nodes by matching node names (`ChangeWeight`).
4. It attaches each costume mesh visual onto the matching frame name in the base skeleton.

In `c01f.bbp`, every bone matrix is identical to `c01b.bbp` (maximum difference `0.000`), and only the hand mesh nodes (`R_te` and `L_te`) differ.

### 2.6 Which pack Max and Monica come from

`SetupMainUnit` (`src/charasetup.cpp:100`) loads the player's base skeleton and motion pack using the path returned by `GetMainCharaModelName(chara, name, alternate)` (`src/userdata.cpp:1114`, in assembly):
- **Towns** (`EditInit` passes edit mode `1`): Loads `chara/c01_base.chr` for Max and `chara/c02_base.chr` for Monica (`"%s.chr"` under `chara/`).
- **Dungeons** (`InitDungeonMain` and the event command `_DNG_SETUP_MAIN_UNIT` pass edit mode `0`): Loads `mainchr/c01_baseN.chr` or `mainchr/c02_baseN.chr` (`"%s%d.chr"` under `mainchr/`). For Monica, `N` is the model number (`0..3`) of her equipped sword from `CGameDataUsed::GetModelNo`, giving `c02_base0` through `c02_base3`. For Max, the engine adds `2` to the weapon model number, giving `c01_base2` through `c01_base4`. The status menu character viewer selects between the town and dungeon packs using the same logic (`MenuItemCharaDataLoad`).

Event cutscenes explicitly load Max's town pack via `load_party_member(0, "chara/c01_base.chr", ...)` (449 calls across the disc). No engine code or event script ever references `mainchr/c01_base.chr`, `mainchr/c01_base0`, `mainchr/c01_base1`, `chara/c01_base_f.chr`, `chara/_c01_base.chr`, `chara/__c01_base.chr`, or `dungeon/chara/c01_base.chr`. (The `dungeon/chara/` folder instead holds supplemental motion packs added to the player, such as `c01b_win`, `c01b_miss`, and `c01_gio`.)

Each base pack has its own `.mot` timeline and `KEY` frame ranges. For example, Max's idle animation (`立ち`) spans frames `95..97` in `chara/c01_base`, but spans frames `362..364` in `mainchr/c01_base2`. Therefore, modifying how Max moves requires updating the town pack and all three dungeon base packs. Costumes, weapons, and hats (`mainchr/c01fuku1.chr`, etc.) are shared across both modes via `LoadSkin` and `LoadPack`.

---

## 3. .mot motions (key-list file)

`CreateAnimeDataEX` (`src/gameutil.cpp:807`) parses `.mot` files, and `MotionProc` (`src/gameutil.cpp:102`) evaluates them each frame.

```c
struct MotBlockHeader {
    s32 frame;      // MDS node index
    s32 target;     // Sub-target (material index or vertex index + 1)
    s32 type;       // MotionKeyType
    u32 header_sz;  // Always 0x20
    u32 key_count;  // Number of 32-byte key records following this header
    u32 more;       // (key_count + 1) * 32 = total block byte size; 0 on the final block
    u32 unk[2];     // Uninitialised exporter memory
};

struct MotKey {
    u32 frame;      // Timeline frame number
    u32 pad[3];     // Always 0
    f32 value[4];   // Keyframe value (x, y, z, w)
};
```

- **Block chaining (`more`)**: The engine only checks whether `more` is non-zero to continue reading blocks. When non-zero, `more` always equals `(key_count + 1) * 32`. The last two 32-bit words of the block header contain uninitialised exporter memory (pointer-like values such as `0x0137e6ac` and `0x77fcf0b0`) and are never read.
- **Block types (`MotionKeyType` in `include/gameutil.hpp`)**:
  - `0`: rotation (58,590 blocks)
  - `1`: scale (9,957 blocks)
  - `2`: translation (17,506 blocks)
  - `12`: vertex position (1,028 blocks)
  - `40`: material alpha (11,211 blocks)
  - `41`: material colour (0 blocks in `.mot` files)
  - `50` / `51`: visibility (12 blocks)
  - `30..33`: camera tracks (0 blocks in `.mot` files)
  The `frame` field is the numeric MDS node index. `MotionProc` looks up the target node via `root->GetFrame(list->frame)` regardless of the node's name.
- **Dense keyframe spacing**: Keyframe numbers strictly increase within every list and never repeat. Almost all lists store **one keyframe per timeline frame**: out of 10,314,102 intervals between consecutive keys across the corpus, 10,314,034 have a step of `1`. The first key starts at frame `1` in 92,772 lists and at frame `0` in 3,565 lists. Blocks appear in ascending MDS node order, and a single node's blocks appear in type order `40` (one per material), `0`, `1`, `2`.
- **Single shared timeline**: A single `.mot` file stores all animations for a character on one combined timeline (for example, `chara` `c01b.mot` spans frames `1..379`, and `mainchr` `c01b.mot` spans frames `1..598`). Because `SetMotionTime` evaluates every block list on every game tick, every animated node's list must cover the full frame range of all `KEY` clips.

### 3.1 Key types (`MotionProc`, gameutil.cpp:102)

To evaluate a track at floating-point frame `time`, `MotionProc` binary-searches for the last keyframe `key` at or before `(u32)time` and pairs it with the following keyframe `next` (clamped to the last keyframe at the end of the list). It then computes the interpolation factor `t = (time - key) / (next - key)`.

> **Warning**
> If `time` is less than the first keyframe's `frame` number, the binary search lands at index `-1` and reads out-of-bounds memory before the key array. Therefore, the first key in every list must start at or before the lowest start frame of any `KEY` clip.

- **Type `0` (Rotation)**: `value` stores a unit quaternion `(w, x, y, z)` (unit length in all 175,703 sampled keys). For `0.001 < t < 0.999`, the engine interpolates using shortest-arc `QuatSlerp`; outside that range, it snaps to the nearer key. `QuatToMat` (`src/mg_frame.cpp:123`) writes the transpose of the standard rotation matrix, and `SetTransMatrix` overwrites the upper-left 3×3 of the node's local matrix while preserving its translation (discarding any scale stored in the MDS matrix). Converting to glTF quaternion `(x, y, z, w)` requires negating the vector part: `(-x, -y, -z, w)`.
- **Type `1` (Scale)**: Linearly interpolates `(x, y, z)` (`w = 0` in all keys) and passes the result to `SetScale`. The node applies this scale separately from its local matrix (`use_srt`).
- **Type `2` (Translation)**: Linearly interpolates `(x, y, z)` (`w = 0` in all keys) and replaces the node's local translation column.
- **Type `12` (Vertex morph)**: `target` is `vertex_index + 1`, and `value` is `(x, y, z, 1.0)`. The engine linearly interpolates `(x, y, z)` and writes the result directly into the in-place MDS vertex array of the node's `mgCVisualMDT`. For performance, `MotionProc` processes all consecutive blocks that share the same node `frame` in a single loop using the keyframe indices found in the first block. Consequently, all vertex tracks for a node must be contiguous and share identical keyframe numbers (which holds across all files on the disc, such as `event/e200/e206/e206_cloth`). Note that if another block type for the same node immediately follows a type `12` group, this loop erroneously treats it as a vertex track as well (this occurs once on the disc).
- **Type `40` (Material alpha)**: `target` is the MDT material index, and `value` is `(x, 0, 0, 0)`. The engine sets `diffuse.a = 1.0 - lerp(x)`, so `0.0` is fully opaque and `1.0` is fully transparent. All 11,211 tracks in the corpus animate fades in effect packs (mostly on material `0`). The engine does not clamp `t`.
- **Type `41` (Material colour)**: Linearly interpolates `(x, y, z)` into the material's `diffuse` RGB and sets the frame attribute `unk_28 = 2`. Unused on the disc.
- **Types `50` / `51` (Visibility)**: Uses `value.x` from the key at or before `time` with no interpolation. For type `50`, `value.x < 1.0` hides the node (`draw = 0`), while `>= 1.0` sets `VISIBLE | SKIP_CHILDREN`. For type `51`, `value.x < 1.0` sets `SKIP_CHILDREN` alone, while `>= 1.0` sets `VISIBLE`. All 12 visibility tracks on the disc use type `50` (`e532_guil_kage` and `e616_osmond`).
- **Types `30..33` (Camera)**: Animate camera position, target, roll, and field of view on an `mgCCamera`. Never present in `.mot` files.

**Motion blending**: When transitioning between clips (`ChangeMotion`, active while `blend < 1.0`), the engine evaluates the outgoing clip at `(int)(0.9 + frame)` and the incoming clip at its start frame, then blends them by `blend` (`QuatSlerp` or `lerp` with thresholds `0.0001` and `0.9999`). `blend` starts at `0.1` and increases each game tick by `blend_speed` (`0.2` by default from `SetMotion`, or a custom value from a `SEQ` entry).

### 3.2 Time base

The PAL release runs game logic at 25 Hz: `mgFrameRate` is set to `2` (`src/title.cpp`), so `mgEndFrame` waits for two 50 Hz vertical blanks per tick. `CCharacter2::NormalDrive` (`src/character.cpp:974`) advances an active `KEY` clip by `1.2 * step` motion frames per game tick. The `1.2` multiplier compensates for PAL's 25 Hz tick rate relative to NTSC's 30 Hz tick rate. Therefore, a `KEY` clip always plays at **`30 * step` motion frames per second**:
- `step = 1.0` (run): 30 motion fps
- `step = 0.5` (walk): 15 motion fps
- `step = 0.02` (idle): 0.6 motion fps

A looping clip wraps back to `start` as soon as `frame + 1.2 * step` would exceed `end`, so the exact `end` frame is only reached when `CHARA_MOTION_HOLD` is active. Common `step` values on the disc are `0.30` (796 keys), `0.50` (616), `0.40` (575), down to `0.00` (119 static poses).

### 3.3 Motion names

In `info.cfg`, a `MOTION` block binds a `.mot` timeline to named `KEY` clips:

```
MOTION 0, "c01b.mot", "c01b.bbp", "c01b.wgt";
KEY_START;
    KEY "歩き", 11, 31, 0.50;     // Name (<= 35 bytes cp932), start frame, end frame, step
    ...
KEY_END;
MOTION_END;
```

- `_KEY` populates a `CHRINFO_KEY_SET { char name[0x24]; s32 start, end; f32 step; }` record.
- Event and AI scripts trigger animations either by name via `SetMotion(name)` -> `GetKeyListPtr` (`strcmp` across the key lists of all 8 motion sets) or by sequential index via `GetKeyListIndexPtr`. To preserve existing indices, append new `KEY` entries at the end of the list.
- Multiple `KEY` entries can share the same frame range (1,770 pairs on the disc share identical ranges, such as `ドア開け` and `ドア開かない`; 325 share an end frame; 106 partially overlap).
- `SEQ_START` / `SEQ` / `SEQ_END` chain named `KEY` clips together with per-stage blend speeds, transition types, and loop counts.
- `SE "name", kind, bank, frame;` attaches sound cues to a `KEY` clip by name (`_SE`, `src/character.cpp:2140`). If `frame <= 1.0`, the loader treats it as a normalized fraction of the clip's duration and converts it to a timeline frame using the `KEY`'s `start` and `end` values at the moment the `SE` line is parsed. If `frame > 1.0`, the loader treats it as an absolute timeline frame. Because all 1,915 `SE` cues on the disc use fractions (`<= 1.0`), moving a `KEY` clip to a new frame range automatically preserves its sound timing. `SELP` defines looped sound cues with start and end points using the same rule.
- A character supports up to 8 motion sets (`MOTION 0..7`). An addon pack can load additional motion sets onto an already-loaded character without a model (for example, `chara/c01_fishing.chr` defines `MOTION 7` with no `MODEL`).

---

## 4. info.cfg

`CScriptInterpreter` (`src/scriptinterpreter.cpp`) parses `info.cfg` files. The text uses CP932 (Shift-JIS) encoding and supports `//` and `/* */` comments. Each statement has the form `TAG arg, arg, ...;`. The parser only recognises tags that start with an uppercase ASCII letter (`'A'..'Z'`), so it silently ignores the standard first line `v2;` (even though the tag table defines `V2`). Quoted arguments parse as strings (supporting `\"` escapes), unquoted arguments containing `.` parse as floats (`atof`), and all other arguments parse as integers (`atoi`).

For character packs (`tag[]` in `src/character.cpp:178`, checked across 2,532 `info.cfg` files):

| Tag | Arguments | Meaning |
|---|---|---|
| `NAME` | `str` | Display or debug name (ignored by the engine). |
| `IMG` / `IMG_END` | `idx, "x.img"` | Loads texture archive into slot `idx` (`idx < CHARA_IMAGE_MAX`). |
| `MATERIAL_ANIME` | `int` | Ignored by the engine. |
| `VERTEX_ANIME` | `names...` | Names of MDS frames to skin (up to 24, see section 2.1). |
| `SHAPE_ANIME` | `0` or `1` | `1` selects CPU vertex animation instead of VU1 skinning. |
| `MODEL` | `"x.mds"` | Main model file. Automatically loads weights from `"x.wgt"`. |
| `OUTLINE` | `"frame", width` | Toon outline settings (`""` applies to the whole model). |
| `SHADOW_MODEL` | `"x.mds"` | Shadow mesh, linked to main skeleton frames by name. |
| `SHADOW_VERTEX_ANIME` | `names...` | Pack-level shadow skinning frame names. |
| `BODY_SIZE` | `h, w, d` | Character collision dimensions (parsed as integers). |
| `POLY_NUM` | `a, b` | Polygon budget metadata. |
| `SCALE` | `x, y, z` | Base model scale. |
| `OBJECT_NAME` / `OBJECT_NAME2` | `id, "frame", [float]...` | Attachment points (hip, weapon hands, kick point, and body collision spheres). |
| `MOTION` | `set, "x.mot", "x.bbp", "x.wgt"` | Loads motion set `0..7`. `_MOTION` ignores the `.wgt` argument. |
| `SHADOW_MOTION` | `"", "s.bbp", "s.wgt"` | CPU-skinned shadow bind pose and weights. |
| `KEY_START` / `KEY` / `KEY_END` | See section 3.3 | Named animation clips. |
| `SEQ_START` / `SEQ` / `SEQ_END` | | Chained animation sequences. |
| `SE_START` / `SE` / `SELP` / `SE_END` | | Footstep and sound-effect triggers bound to clip names. |
| `CLOTH_START n` / `CLOTH "x.clo"` / `CLOTH_END` | | Cloth simulation scripts (see section 5). |
| `POSITION` / `ROTATION` | `x, y, z` | Initial offset and rotation. |
| `EFFECT_START` / `EFFECT` / `EFFECT_END` | | Attached particle effects. |
| `LOD_MODEL_START n` / `LOD_MODEL lvl, "m.mds", "m.wgt", "m.bbp", dist` / `LOD_MODEL_END` | | Level-of-detail meshes and switch distances. |

Other pack types use smaller tag tables:
- **Costume packs (`LoadSkin`)**: Recognise `IMG`, `IMG_END`, `MODEL`, and `MOTION`.
- **Map model packs (`.pcp` `info.cfg`)**: Use `MDS "name.mds", "original path"; TYPE n; FAR_CLIP d, fade; MDS_END;` (`src/mdslist.cpp`, documented in `re/docs/mdslist/notes.md`).
- **Sky packs (`.sky`)**: Use `SKY_IMG`, `SKY_MDS`, `SUN_MDS`, `SKYB_MDS`, and `SKY_ANIME`.

---

## 5. .clo (cloth / dangling parts), brief

`CDynamicAnime::Load` (`src/dynamicanime.cpp:625`) parses `.clo` scripts using `CScriptInterpreter` and the `dynmc_tag` table (72 files on the disc). These scripts drive spring-mass physics for hair, coats, and dangling straps:
- `FRAME_START n;` / `FRAME "frame";` / `FRAME_END;`: Registers MDS frame names with local cloth frame IDs.
- `VERTEX_START n;` / `VERTEX id, x, y, z;` (world offset) / `VERTEX_L id, x, y, z;` (frame-local offset) / `VERTEX_END;`: Defines simulation particles.
- `FIX_VERTEX frame, vertex[, weight[, ?]];` (plus `FIX_VERTEX_C` with `velocity_rate = 1` and `FIX_VERTEX_S` with `velocity_rate = -1`): Pins a particle to an MDS frame.
- `FRAME_POSE frame, "bone"|"bone_xy"|"b_cdlr", v0, v1, v2, v3;` (and `FRAME_POSE_L`): Drives an MDS frame's orientation from 4 particles. Plain `FRAME_POSE` also detaches the frame from its parent in the hierarchy.
- `DRAW_FRAME ids...;`: Selects frames to draw.
- `BIND_VERTEX_START n;` / `BIND_VERTEX a, b[, rate];`: Defines distance constraints between particles.
- `BOUNDING_BOX;`: Sets bounding box parameters.
- `COLLISION_START n;` / `COLLISION "pipe", frame, cx, cy, cz, rx, ry, rz, axis;`: Defines cylinder/pipe collision primitives attached to frames.
- `GRAVITY x, y, z;` (defaults to `(0, -0.6, 0)`), `K k;` (spring stiffness), and `WIND scale;`.

Because `.clo` scripts look up MDS frames by name, cloth simulation continues to work automatically on a modified mesh as long as the original node names are preserved.
