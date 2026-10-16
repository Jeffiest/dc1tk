# Editing character models

This guide explains how to edit the game's 3D character models and create new models for a mod. You can export a character pack (`.chr`) as a glTF file, edit its meshes and animations in Blender, add new animations, or create a brand-new character or prop pack and load it from a script.

For details on the `.chr` file format and engine behavior, see [the game's character models](../internals/models.md). For details on how `dc2tk` converts models and animations, see [the implementation notes](../../src/docs/assets/models.md). For custom texture archives, see [textures](textures.md), and for CLI command syntax, see [commands](commands.md).

## Exporting a pack to glTF

A character pack (`.chr`) contains a 3D model, its skeleton and bind pose, skin weights, animations (motions), and textures.

To export and edit a character pack:

1. Export the `.chr` archive as glTF:
   ```sh
   dc2tk mod asset ISO NAME chara/c02a.chr --as gltf
   ```
   This command creates a folder named `c02a.chr.gltf/` containing `c02a.gltf`, `c02a.bin` (which holds the skeleton, meshes, skin weights, and every motion as a glTF animation), and a `textures/` folder with the pack's textures as PNG files.
2. Open Blender and import `c02a.gltf` (**File** > **Import** > **glTF 2.0**).
3. Edit the meshes, animations, or PNG textures.
4. Export the scene from Blender over `c02a.gltf` (**File** > **Export** > **glTF 2.0**, with **Format** set to **glTF Separate**).
5. Run `dc2tk build`.

## Replacing a model's meshes

`dc2tk build` rebuilds only the meshes that changed compared to the original export, using the model on the disc as a template:

- **Skeleton and frame names**: The skeleton keeps its original frames, names, and node order, because animations and cloth physics find frames by name. Meshes match back to skeleton frames by node name (Blender's `.001` duplicate suffixes are ignored).
- **Bone and weight limits**: Keep each skinned mesh to at most 32 bones and at most 4 weights per vertex.
- **Textures**: Any modified PNG files—either in `textures/` or next to the `.gltf` file where Blender writes them—update the pack's `.img` archive by texture name. If a model uses textures stored in another `.chr` pack, edit those textures in that other pack.
- **Byte-identical round trips**: Unedited exports—and unchanged meshes exported back out of Blender—build back to the exact bytes from the disc. `dc2tk build` compares each mesh against the initial export by value (triangle vertex positions in frame space, UVs, vertex colors, and skin weights, within a tolerance that absorbs Blender's float rounding and weight renormalization).
- **Partial mesh rebuilds**: Only meshes that you actually edit are rebuilt. `dc2tk build` writes edited meshes as triangle lists rather than the game's triangle strips (which increases an edited body's size by a few percent), uses the skin weights from Blender for the edited mesh, and preserves the original bind pose and weights of all unedited meshes.

### Max and Monica

Max and Monica store their outfits in separate costume packs (`mainchr/c01fuku1.chr` and similar), while their base packs hold their skeletons and animations—one base pack for towns and one per weapon model in dungeons:

- **Max**: `chara/c01_base.chr` (`model:c01_base`) in towns and event scenes; `mainchr/c01_base2.chr` to `c01_base4.chr` (`model:c01_base2` to `model:c01_base4`) in dungeons, depending on his equipped weapon.
- **Monica**: `chara/c02_base.chr` in towns and event scenes; `mainchr/c02_base0.chr` to `c02_base3.chr` in dungeons.

Each base pack has its own motion timeline. To change how Max or Monica moves everywhere, export, edit, and include each of their base packs in your mod (see [which pack Max and Monica come from](../internals/models.md#26-which-pack-max-and-monica-come-from)).

### What any importer has to respect

The `.mds` model format has no PS2 VU-specific layout; it stores an indexed mesh that the engine batches at runtime. Any model builder must satisfy six rules:

1. **Keep the skeleton**: Animations, `info.cfg` (`OBJECT_NAME*`, `OUTLINE`, `VERTEX_ANIME`), `.clo` cloth files, `LoadSkin`, and shadow links find frames by name, while `.mot` and `.wgt` files index nodes by number. Always use the original `.mds` as a template: preserve node order, names (including `--` flags), parents, and matrices, replace only `MDT` mesh blocks, and append any new mesh-only leaf nodes at the end. Costume packs must keep the base skeleton's frame names, and bone `bbp` matrices must match the base character because `CreateChangeFrame` copies `bbp` matrices only for mesh nodes.
2. **Geometry**: Split glTF vertices into separate deduplicated position, normal, UV, and color arrays. Write triangle lists (primitive type `3`, or `0x103` with vertex colors, `+0x10` without UVs, `+0x200` without normals) or stripify into triangle strips (type `4`, which uses fewer indices; the engine supports both). Keep one `FACES_ID` per material run. Although counts are stored as `u32` in the file, the engine stores them in 16-bit signed shorts (`mgCFace.vertex_num`, `index_num = count * stride`), so keep each primitive under `32,767 / stride` corners. Use counter-clockwise (CCW) winding, `w = 1` for vertices and normals, and `uv = (u, v, 1, 0)`.
3. **Skinned meshes**: Transform vertex positions back into the mesh node's local space using $\text{inv}(\text{bbp}[\text{mesh}])$. Write the mesh node's `bbp` matrix and write `.wgt` blocks (`frame` = mesh node, `target` = bone node, type `20`, keys = `(vertex index, weight * 100)`). Limits: at most 32 bones per mesh, at most 4 weights per vertex, with weights summing to `100`. List every skinned mesh node in `VERTEX_ANIME` (maximum 24 names). The CPU skinning path (`SHAPE_ANIME` / shadows in `MotionProc3`) also warns above 400 vertices or 800 normals per mesh.
4. **Materials**: Keep texture names to at most 31 bytes in `cp932`, matching textures in the pack's `.img`. Default material values are diffuse `(1, 1, 1, 1)`, field `0x10` `(1, 1, 1, 0)`, and specular power `50`.
5. **Layout**: Build `.mds` files with `mds::build()` (style A layout) and align all `.chr` pack members to 16-byte boundaries.
6. **Memory budget**: Character packs load into fixed character memory stacks (`base_stack` holds the `.mds` visuals and packets), and `_MODEL` uses a 6400-qword work buffer (limiting a skinned mesh to at most 3,198 vertices). See [the memory budgets](../internals/memory.md) (character packs)—the main party's packs nearly fill their stacks. `POLY_NUM` in `info.cfg` may also need updating (exact engine usage not traced).

## Editing and adding motions

Every `KEY` in the pack's motion set 0 exports as a glTF animation (an Action in Blender) with the same name, sampled at 1 keyframe per motion frame at 30 frames per second.

> **Important**
> Set Blender's frame rate to **30 FPS** (**Output Properties** > **Format** > **Frame Rate**) before importing the glTF file, or Blender will resample keyframe timings.

### How `build` updates animations

`dc2tk build` writes back to `.mot` and `info.cfg` only the animations that differ from the export beyond Blender round-trip rounding:

- **Edited motions of the same length**: Overwrite their original frames in place (unless another `KEY` shares those frames).
- **Resized or shared-frame motions**: Write to newly allocated motion frames and update the `KEY`'s start and end frame numbers.
- **New Actions**: Append as a new `KEY` after the last entry in the motion set. A duplicated Action named `walk.001` copies the original `KEY`'s speed (`step`); other new Actions default to `step = 1.00` (30 FPS, matching Blender). In the game, a `KEY`'s `step` scales its playback rate to $30 \times \text{step}$ frames per second. To set a custom `step` on a new Action, add `"extras": {"step": ...}` to the animation in the `.gltf` file after exporting from Blender.
- **Deleted Actions**: Deleting an Action removes its `KEY` from `info.cfg`, along with its sound cues and any frames not used by another `KEY`. Note that scripts calling motions by index number will shift down by 1 for all later `KEY`s. (Deleting every Action in the file deletes none.)
- **Additional motion sets (`MOTION 1` and higher)**: Motions in sets other than `0` export as Actions named `<set>:<name>` (for example, `3:bow` for `KEY` `bow` in `MOTION 3`). Creating a new Action named `3:curtsy` adds `curtsy` to motion set 3.
- **Build summary**: For each modified `.chr` pack, `dc2tk build` reports which `KEY`s were added (with their 0-based index in the set), moved, or removed, and which archive members changed.

### Vertex animations, material fades, and visibility (shape keys)

Vertex animations (cloth, flags, facial expressions), material fades and colors, and frame visibility export as mesh shape keys animated by the Actions:

- **Vertex morphs (`c2_frame_<f>`)**: A vertex-animated mesh has one shape key named `c2_frame_<f>` per motion frame, keyed to `1.0` on frame `<f>`.
- **Material fades, colors, and visibility**: Stored on dummy shape keys whose vertices do not move, using their animated weight values:
  - `c2_fade_<k>`: Material `k` fade (`0` = opaque, `1` = invisible)
  - `c2_red_<k>`, `c2_green_<k>`, `c2_blue_<k>`: Material `k` RGB color channels
  - `c2_visible`: Node visibility (`< 1` = hidden, `1` = visible)

You can sculpt the `c2_frame_<f>` shape keys or keyframe the fade, color, and visibility shape key weights in Blender just like any standard shape key.

### Motion-only packs

Some `.chr` archives contain only animations and no model of their own (such as `chara/c01_fishing.chr`, or event motion packs like `event/e200/e209/e209_mints.chr`). `dc2tk` exports these packs attached to the target character's model (`chara/c01_base.chr` for Max, `chara/c02_base.chr` for Monica, or the companion event model) so you can preview and edit the animations on the character in Blender. When building, `dc2tk build` writes back only the motions; the meshes, skin weights, and textures belong to the other pack.

For details on motion files, see [the game's motions](../internals/models.md#3-mot-motions-key-list-file) and sections 6 and 7.3 of [the implementation notes](../../src/docs/assets/models.md).

The [spinning-cult example](../scripting/examples/spinning-cult/README.md) adds custom animations this way: `make_villagers.py` exports five townspeople from the disc, adds `spin` and `stare` Actions to each in Blender, and exports them back into the mod, where `dc2tk build` adds them as new `KEY`s.

## Models of a mod's own

Scripts load character and prop packs by their disc path:

```c
load_chara(stack, "mods/calculator/calc.chr", "info.cfg", slot);
```

You can build a new `.chr` pack from a glTF file by adding a `file:` entry under `"add"` in `mod.json` and naming an existing model on the disc with `"like"` (similar to building custom `.img` files from PNGs; see [textures](textures.md)):

```json
"add": {
  "file:mods/calculator/calc.chr": {"file": "model/calc.gltf", "like": "model:uki"}
}
```

- **Input format**: `"file"` can point to a `.gltf` file, a `.glb` file, or a folder containing the glTF file. `dc2tk build` takes the nodes and meshes as placed and converts each material's base color PNG into a texture in the pack's `.img` (texture dimensions must be powers of two, and each texture is named after the image file stem).
- **Static props**: Using `"like": "model:uki"` (the fishing float) creates a static prop pack whose `info.cfg` names the new `calc.img` and `calc.mds` with no motion files. The [calculator example](../scripting/examples/calculator/) builds `calc.chr` this way.
- **Rigged characters**: Using a rigged character as the template (for example, `"like": "model:p01_01a"`, a villager) converts the glTF armature into the pack's skeleton, skins the meshes, and converts Actions into motion `KEY`s that `set_chara_motion` plays by name.
- **Supported `"like"` targets**: `"like"` supports only `.img` archives (like a `texture:NAME` or `.img` disc path) and `.chr` archives (like a `model:NAME` or `.chr` disc path), using a single ID rather than a wildcard pattern. All other target types are rejected.

### New characters

When `"like"` names a character model, `dc2tk build` creates a complete custom character `.chr` pack from your glTF file:

```json
"add": {"file:mods/gill/gill.chr": {"file": "model/gill.gltf", "like": "model:p01_01a"}}
```

- **Skeleton and meshes**: Every node in the glTF becomes a skeleton frame with its Blender name. Meshes can be rigid (parented to a bone node) or skinned (up to 32 bones and 3,198 vertices per skinned mesh, and at most 24 skinned meshes).
- **Animations**: Every Action becomes a motion `KEY` of the same name (using `extras.step` or default `1.00` speed; Blender's frame rate setting does not affect new character packs because glTF stores animation times in seconds). A skinned model **must** have at least one Action because its motion set carries the bind pose.
- **Generated archive members**: Writes `NAME.mds`, `.wgt`, `.bbp`, `.mot`, `.img`, and an `info.cfg` adapted from the `"like"` template:
  - Keeps the template's bounding size, scale, and whole-model outline.
  - Updates `VERTEX_ANIME` to list the new skinned meshes and writes the new motion set.
  - Drops settings that reference the template's own frames or motions (sequences, sound cues, cloth, effects, attach points, and per-frame outlines for frames that the new model lacks).
- **Animated fades, colors, and visibility**: Actions on a new character can animate material fades (`c2_fade_<k>`: `0` = opaque, `1` = invisible), material colors (`c2_red_<k>`, `c2_green_<k>`, `c2_blue_<k>`), and mesh visibility (`c2_visible`: `< 1` = hidden) using dummy shape keys whose rest values are `0`, the material's color, and `1`. Per-frame vertex morphs (`c2_frame_<f>`) and `.clo` cloth physics are not supported on new characters.

The [spinning-cult example](../scripting/examples/spinning-cult/README.md) builds Brother Gill this way (`make_gill.py`) with 18 bones and 6 motions, including `vanish` (which fades him out). It also builds the cult's gyroscope (`gyros`) twice: once as an animated character pack (`mods/gyros/gyros.chr` like Brother Gill, with 5 bones, rigid meshes on each bone, and a `spin` motion), and once as item 429's static item model (`item/gyros.chr` like the Treasure Chest Key, `model:takaraky`).

### Loading it from a script

In a script, call `load_chara` to load the `.chr` pack and `set_chara_motion` to play an Action by name. For example, the spinning-cult mod loads and animates the gyroscope in a scene with:

```c
load_chara(MEMORY_STACK_EVENT, "mods/gyros/gyros.chr", "info.cfg", slot);
// place the character in the scene, then play the motion:
set_chara_motion(slot, "spin");
```

## Checking

- `dc2tk build` lists custom `.chr` packs as new files.
- Run `dc2tk mod asset built.iso NAME mods/calculator/calc.chr --as gltf` to export a built `.chr` pack back out of a built disc image and inspect it in Blender.
- See [the design](../../src/docs/design.md#2-mods-by-name) ("Mods by name") for design rules.
