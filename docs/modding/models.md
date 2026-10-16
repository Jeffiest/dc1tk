# Editing character models

How to edit the game's character models and make models of a mod's own: export a character pack
as glTF, edit it in Blender, and give it back in a mod; add motions; make a pack of a new
character or prop; load it from a script. What the files hold and how the engine reads them is in
[the game's character models](../game/models.md); how dc2tk converts them is in
[the implementation notes](../../src/docs/assets/models.md). Pictures of a mod's own are in
[textures](textures.md), and the `mod asset` command and the rest in [commands](commands.md).

## Exporting a pack to glTF

A character pack (`.chr`) holds a model,
its skin weights, bind pose, motions and textures. `mod asset ISO NAME
chara/c02a.chr --as gltf` writes a folder `c02a.chr.gltf/` with
`c02a.gltf` and `c02a.bin` (the skeleton, the meshes, the skin and every
motion as an animation) and `textures/` with the pack's pictures as PNG.
Open it in Blender (File, Import, glTF 2.0), edit the meshes, and export
glTF 2.0 over it as "glTF Separate"; or edit the PNGs.

## Replacing a model's meshes

`build` rebuilds
the meshes that differ from the export with the disc's model as the
template: the skeleton keeps its frames, names and order, since motions
and cloth find frames by name, and meshes are matched to frames by name
(Blender's `.001` suffixes are ignored). Keep a mesh to 32 bones and 4
weights per vertex. Changed PNGs, in `textures/` or beside the glTF as
Blender writes them, go into the pack's textures by picture name;
textures the model takes from other packs are changed in those.
Unedited exports build back to the disc's own bytes, and so do round trips
through Blender: meshes are compared with the export's by value (each
triangle's corners in its frame's space, their UVs, colours and skin
weights, within what Blender's floats and renormalised weights move), so
only a mesh you edit is rebuilt, as triangle lists rather than the game's
strips (an edited body grows the model a few percent), with its weights as
Blender gives them and every other mesh's weights and the bind pose kept.

### Max and Monica

Max's and Monica's
clothes are separate costume packs (`mainchr/c01fuku1.chr` and the like):
their own packs hold the skeleton and motions, one for towns and one per
weapon model in dungeons. In towns and event scenes Max is
`chara/c01_base.chr` (`model:c01_base`); in dungeons
`mainchr/c01_base2.chr` to `c01_base4.chr` (`model:c01_base2` to
`model:c01_base4`), whichever his weapon picks. Monica is
`chara/c02_base.chr` and `mainchr/c02_base0.chr` to `c02_base3.chr`. Each
has its own motion timeline, so a change to how they move is exported,
edited and given for every one of them (see
[which pack Max and Monica come from](../game/models.md#26-which-pack-max-and-monica-come-from)).

### What any importer has to respect

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
   See [the memory budgets](../game/memory.md) (character packs): the party's packs nearly fill
   theirs. POLY_NUM in info.cfg may also need updating (meaning not traced).

## Editing and adding motions

Motions come back too: every KEY of the pack's motion set 0 is an
animation (an action in Blender) of its name, one key per motion frame at
30 frames a second. Set Blender's frame rate to 30 (Output Properties,
Format) before importing, or it resamples them. `build` writes into the
pack's `.mot` and `info.cfg` only the animations that differ from the
export by more than a round trip moves them: an edited motion over its own
frames, a longer or shorter one (or one whose frames another KEY shares)
at new frames with its KEY moved, and a new action as a new KEY after the
last (a copy named `walk.001` takes the copied KEY's speed, others 1.00,
the speed Blender plays it at). A KEY's `step` scales its speed in the
game: it plays at 30 × step frames a second. Deleting an action takes its
KEY out of `info.cfg`, with its sound cues and the frames no other KEY
plays (scripts that play motions by number then count the later ones one
lower; deleting every action deletes none). Motion sets other than 0 are
actions too, named `3:bow` for KEY `bow` of `MOTION 3`; a new action named
`3:curtsy` joins set 3. A new action's KEY takes the step in its animation's
`extras.step` when it has one (Blender writes none; set it in the glTF after
exporting). Under each character pack it replaces, `build` reports the KEYs
added (with their place among the set's KEYs, from 0), moved or taken out,
and the pack's members that changed and those that did not.

Vertex animation (cloth, flags, faces), material fades and colours and
visibility are shape keys of the mesh, animated with the actions: a
vertex-animated mesh has a shape key `c2_frame_<f>` per motion frame
(keyed 1 on its frame), and `c2_fade_<k>` (0 opaque, 1 invisible),
`c2_red_<k>`/`c2_green_<k>`/`c2_blue_<k>` and `c2_visible` (below 1
hidden) hold the values of material k's fade and colour and of the
frame's visibility as their shape key values; they move nothing. Edit
the shapes or key the values like any shape key.

A pack of motions alone, with no model of its own
(`chara/c01_fishing.chr`, the event packs of Max's and Monica's motions
such as `event/e200/e209/e209_mints.chr`), exports the same way on the
model of the character it is loaded onto (Max's `chara/c01_base.chr`,
Monica's `chara/c02_base.chr`, or the event's model beside it), to see
the motions on; `build` writes back its motions alone (the model, skin
and pictures there are the other pack's). See
[the game's motions](../game/models.md#3-mot-motions-key-list-file) and, for how the export and
the build treat them, [the implementation notes](../../src/docs/assets/models.md) (sections 6
and 7.3).

The [spinning-cult example](../coding/examples/spinning-cult/README.md) adds motions this way:
`make_villagers.py` exports five townspeople from your disc, adds `spin` and `stare` to each in
Blender, and exports them back into the mod, where build adds them as new KEYs.

## Models of a mod's own

Scripts load models by disc path: `load_chara(stack, "mods/calculator/calc.chr", "info.cfg",
slot)` loads a character pack. A mod makes such files from glTF with `file:`
lines that say which of the disc's files to make them like (pictures are made the same way, from
PNGs: see [textures](textures.md)):

```json
"add": {
  "file:mods/calculator/calc.chr": {"file": "model/calc.gltf", "like": "model:uki"}
}
```

- **A model pack** takes a glTF: its nodes and meshes as they are placed,
  each material's base colour PNG as a picture of the pack (sides powers
  of two), its texture named after the image. `model:uki`, the fishing
  float, is a static prop's pack: its `info.cfg` names the new
  `calc.img` and `calc.mds`, with no motion. A rigged character works too:
  its armature becomes the pack's skeleton, its skinned meshes are skinned
  as the game's are and its actions become motions `set_chara_motion`
  plays by name; take a character's model as the template
  (`model:p01_01a`, a villager). A `.glb`, or a folder holding the glTF,
  works as well as a `.gltf`.
- `"like"` makes only these two: a `.img` like a texture (`texture:NAME` or
  a texture file's disc path) and a `.chr` like a model (`model:NAME` or a
  pack's disc path), one id and not a pattern; anything else is refused.

The [calculator example](../coding/examples/calculator/) makes a static prop's pack, `calc.chr`,
like `model:uki`.

### New characters

A `file:` line with `like` naming a character's model
makes a pack of a character of your own from a glTF: its own skeleton
(every node a frame, named as in Blender), its meshes, rigid or skinned
(at most 32 bones and 3,198 vertices a skinned mesh, 24 skinned meshes),
its pictures and every action as a KEY of its name (`extras.step` or 1.00
the speed; Blender's frame rate does not matter here, since actions are
read in seconds). The pack holds `NAME.mds`, `.wgt`, `.bbp`, `.mot`,
`.img` and an `info.cfg` made from the template's: its size, scale and
whole-model outline kept, VERTEX_ANIME naming the skinned meshes, the new
motion set, and what named the template's own frames and motions dropped
(sequences, sound cues, cloth, effects, attach points and outlines on
frames the new model lacks). A skinned model needs an action (its motion
set carries the bind pose). Actions can fade, colour, hide and show
meshes too, with the shape keys an export writes for them: `c2_fade_<k>`
(0 opaque, 1 invisible), `c2_red_<k>`/`c2_green_<k>`/`c2_blue_<k>` and
`c2_visible` (below 1 hidden), shape keys that move nothing, their
values keyed in the action (a mesh's rest values: 0, its material's
colour, 1). Not vertex animation (`c2_frame_<f>`). No cloth: a `.clo`
names a model's frames and particles.

```json
"add": {"file:mods/gill/gill.chr": {"file": "model/gill.gltf", "like": "model:p01_01a"}}
```

The [spinning-cult example](../coding/examples/spinning-cult/README.md) makes
Brother Gill this way (`make_gill.py`): eighteen bones and six motions, among them `vanish`, which
fades him out. It also makes the gyros, the cult's gyroscope, twice: as a character pack like
Brother Gill's (`mods/gyros/gyros.chr`, five bones, each part a rigid mesh on its bone, and one
motion, `spin`), and as item 429's model standing still (`item/gyros.chr` like the Treasure Chest
Key's, `model:takaraky`).

### Loading it from a script

Load it with `load_chara` and play an action with `set_chara_motion`; the spinning-cult example
stands the gyros in a scene with `load_chara(MEMORY_STACK_EVENT, "mods/gyros/gyros.chr",
"info.cfg", slot)`, then places it and calls `set_chara_motion(slot, "spin")`.

## Checking

`build` lists them as new files; `dc2tk mod asset built.iso NAME
mods/calculator/calc.chr --as gltf` copies them
back out of a built image to check. The rules are in [the
design](../../src/docs/design.md#2-mods-by-name), "Mods by name".
