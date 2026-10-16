# Character models: the implementation

How dc2tk reads, writes and converts character models: the modules, the glTF export
(`to_gltf.rs`), the import back (`from_gltf.rs`), motions (`motions.rs`) and new packs
(`models::new_pack`), with the status of each, how they were verified and the evidence log. The
formats themselves (MDS, .wgt, .bbp, .mot, info.cfg, .clo) are in
[the game's character models](../../../docs/internals/models.md), sections 1 to 5; section numbers 1
to 5 below refer to that page. How a mod uses all this is in
[editing character models](../../../docs/guide/models.md), which also holds section 7.2, what
any importer has to respect.

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
| `gltf.rs` | glTF's accessor component and element types, shared by the export and the import |
| `math.rs` | 4x4 column-major matrices, points and quaternions (x, y, z, w) of the converters. Their results reach the disc's bytes (an imported bind pose, a rebuilt motion), so each keeps its order of operations: a change here changes what a build writes |
| `numeric.rs` | the arithmetic the bytes depend on: JSON numbers kept int or float, compensated (Neumaier) float sums, float32 narrowing |
| `bytes.rs` | bounds-checked little-endian reads, so a bad offset or count in a modded file is an error naming what was read where, never a panic |

Mods use these through `models.rs`: textures come from the pack's own
`.img` files (not placeholders), only meshes and motions that differ from the
export are rebuilt, and the template's bind pose is kept unless the glTF
changes it.

What the byte-exact writing keeps: the MDS data must be 16-byte aligned in memory (1), and pack
members are 16-aligned by `PACK_ALIGN` in `src/crates/dc2tk-disc/src/archive.rs`. `mds.rs` keeps
the leftover non-zero pad bytes of 38 style-B blocks (1.3), the garbage first word of the faces
header in 22 blocks (1.3), and the bytes after the NUL of 70 material names (1.6). The
converters need nothing of .clo (5): it is not needed for mesh import.

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

**Not settled / open** (the open questions about the formats themselves are on
[the game's page](../../../docs/internals/models.md#open-questions)):
- Texture pixels are decoded by `im3.rs`; the exports carry the pack's own pictures.
- Motion types 12 (vertex), 40/41 (material alpha and colour) and 50/51 (visibility) go to glTF
  as morph targets and carriers Blender keeps (6), not as `KHR_animation_pointer` and
  `KHR_node_visibility`, which Blender 5.1 cannot take back.
- Mesh import (`from_gltf.rs`): our own exports and Blender 5.1 round trips of every character
  pack change nothing (7.1); an edited mesh has been drawn in the game, a new character
  (7.4) not yet.

---

## 6. glTF export (`to_gltf.rs`)

`to_gltf::export` writes a character pack's folder as `<name>.gltf` and `<name>.bin` (30 fps).
`dc2tk mod asset ISO NAME chara/c02a.chr --as gltf` and `dc2tk assets` run it, through `models.rs`,
which puts the pack's own textures in place of the placeholders.
- glTF node i = MDS node i (`extras.mds_index`, `extras.mds_name` keep the full name with flags).
  Local TRS comes from decomposing the MDS matrix. 4 two-root files get a synthetic `_mds_roots`
  parent, appended last.
- rigid meshes sit on their frame node. Skinned meshes go on `<name>_skin` nodes at the scene
  root, with one skin over all frames (joint i = node i) and IBM = inv(bbp) (inverse of the MDS
  world matrix where the bbp entry is missing or singular). The skinning formula (2.4) in glTF
  is: mesh vertices = bbp[mesh] * v_local (model space), joint inverse bind matrix =
  inv(bbp[bone]), mesh node at the scene root. `to_gltf.rs` does exactly this. The `check_gltf`
  evidence script confirmed world(joint) * IBM = I to 3e-5 on Max.
- primitives: one per (material, attribute set). Corners are deduplicated on (v, n, uv, c).
  Degenerate strip triangles are dropped. Attributes are POSITION, NORMAL, TEXCOORD_0, COLOR_0
  (float), JOINTS_0 (u16) and WEIGHTS_0. Unweighted vertices get the engine's default (slot 0, weight 1).
- materials: name = texture name. The base colour texture is `textures/<name>.png`: `to_gltf`
  writes an 8x8 placeholder (`placeholder_png`), which `models.rs` replaces with the pack's own
  picture of that name where its `.img` files have one. Raw MDT fields are kept in `extras`.
  diffuse.a < 1 gives BLEND.
- animations: one per KEY of every motion set the pack holds a .mot for (a name given twice in a
  set gets `_<start>`), sampled every frame from start to end with the engine's interpolation.
  LINEAR, time = (frame - start) / 30 (3.2). Set 0's KEYs (else the first set's) keep their
  names; another set's are named `<set>:<name>` (`3:bow`, with `extras.motion_set`). Rotation,
  translation and scale lists drive the node their `frame` numbers (before, lists went to the
  first node of the same lower-case name, which put 1,866 lists of nodes named alike, `3` and
  `3--ba`, on the wrong node). `extras` keeps start/end/step. The export writes one key per
  motion frame at 1/30 s (step 1.0); the step stays in the animation's `extras` and in info.cfg.
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
  (`grid54--f2appz0n1t-5_1`), since map meshes are told apart by them
  ([maps](../../../docs/internals/maps.md) 4.1); meshes also get `extras.mds_index`. Nodes always
  carry `extras.mds_index` and `extras.mds_name`.
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
- `collision`: for `-a`/`-c` files ([maps](../../../docs/internals/maps.md) 4.3). Each material is
  named for the per-triangle attributes its diffuse colour encodes
  (`col_ground0_foot2_area6_mask12`, `to_gltf::CollisionAttributes`), coloured by them (water
  areas 1 and 7 blue), untextured, with the attributes and the raw MDT fields in `extras`.

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
  template .wgt blocks for meshes that were not replaced are kept. A rebuilt mesh's .wgt has
  blocks with keys alone (2.2): Max's `chara/c01_base` `c01b.wgt`, whose five skinned meshes are
  3-vertex placeholders with 95 blocks each, 464 of them empty, goes from 15,680 to 832 bytes
  when all five are rebuilt, every weight kept;
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

The `.mds` model format has no PS2 VU-specific layout; it stores an indexed mesh that the engine batches at runtime. Any model builder must satisfy six rules:

1. **Keep the skeleton**: Animations, `info.cfg` (`OBJECT_NAME*`, `OUTLINE`, `VERTEX_ANIME`), `.clo` cloth files, `LoadSkin`, and shadow links find frames by name, while `.mot` and `.wgt` files index nodes by number. Always use the original `.mds` as a template: preserve node order, names (including `--` flags), parents, and matrices, replace only `MDT` mesh blocks, and append any new mesh-only leaf nodes at the end. Costume packs must keep the base skeleton's frame names, and bone `bbp` matrices must match the base character because `CreateChangeFrame` copies `bbp` matrices only for mesh nodes.
2. **Geometry**: Split glTF vertices into separate deduplicated position, normal, UV, and color arrays. Write triangle lists (primitive type `3`, or `0x103` with vertex colors, `+0x10` without UVs, `+0x200` without normals) or stripify into triangle strips (type `4`, which uses fewer indices; the engine supports both). Keep one `FACES_ID` per material run. Although counts are stored as `u32` in the file, the engine stores them in 16-bit signed shorts (`mgCFace.vertex_num`, `index_num = count * stride`), so keep each primitive under `32,767 / stride` corners. Use counter-clockwise (CCW) winding, `w = 1` for vertices and normals, and `uv = (u, v, 1, 0)`.
3. **Skinned meshes**: Transform vertex positions back into the mesh node's local space using $\text{inv}(\text{bbp}[\text{mesh}])$. Write the mesh node's `bbp` matrix and write `.wgt` blocks (`frame` = mesh node, `target` = bone node, type `20`, keys = `(vertex index, weight * 100)`). Limits: at most 32 bones per mesh, at most 4 weights per vertex, with weights summing to `100`. List every skinned mesh node in `VERTEX_ANIME` (maximum 24 names). The CPU skinning path (`SHAPE_ANIME` / shadows in `MotionProc3`) also warns above 400 vertices or 800 normals per mesh.
4. **Materials**: Keep texture names to at most 31 bytes in `cp932`, matching textures in the pack's `.img`. Default material values are diffuse `(1, 1, 1, 1)`, field `0x10` `(1, 1, 1, 0)`, and specular power `50`.
5. **Layout**: Build `.mds` files with `mds::build()` (style A layout) and align all `.chr` pack members to 16-byte boundaries.
6. **Memory budget**: Character packs load into fixed character memory stacks (`base_stack` holds the `.mds` visuals and packets), and `_MODEL` uses a 6400-qword work buffer (limiting a skinned mesh to at most 3,198 vertices). See [the memory budgets](../../../docs/internals/memory.md) (character packs)—the main party's packs nearly fill their stacks. `POLY_NUM` in `info.cfg` may also need updating (exact engine usage not traced).

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
with and without a fading and hiding action, on a seven-bone character made in Blender, whose
pack, built into a disc image and copied back out, plays its action in Blender as made, and on
[Brother Gill](../../../docs/scripting/examples/spinning-cult/README.md),
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
