# dc1tk_maps: a Blender add-on for Dark Cloud 1 maps

Imports what `dc2tk maps-export` writes and exports edits back the way `floor-build`, `scene-build` and
`layout-build` take them. Blender 4.2+ (tested with 4.2.3).

## Install
```bash
blender --command extension build --source-dir blender-plugin/dc1tk_maps --output-dir build
```
then Edit, Preferences, Get Extensions, Install from Disk, pick `build/dc1tk_maps-0.1.0.zip`. In the add-on's
preferences set the dc2tk executable (`build/release/dc2tk.exe`), the game data folder and a build output folder
(only the Build button needs them).

## Use
File, Import (or sidebar N, tab "Dark Cloud 1"):
- **Floor**: pick `floors/<floor>/floor.json`. Each DEF_PATS part is an empty (`00 d01g01_0`, laid out on a grid for
  viewing; the game places parts itself) holding its model (PT_BASE), collision (PT_COLS, `_a`, coloured by the
  `col_ground/foot/area/mask` materials) and camera collision (PT_CAM, `_v`, hidden collection), and its PT_FIRE torches
  as arrow empties. A model several parts use is shared mesh data; the first part's copy is the one exported.
- **Georama scene**: pick `<name>.scn/scene.json`. Each part is an empty at its position/rotation with its lod0,
  collision and camera models (lod1-3, shadow, shade, extra in a hidden collection) and a wire grid of its cells
  (100 units a cell, drawing only). Cells are edited in the panel as hex rows (`808080`), same size.
- **Georama layout**: pick `layouts/.../gdata0.edt.json`. Every placed part is a cube empty; rotation is quarter turns
  about the game's Y. Duplicating or deleting empties adds or removes parts.

Everything is in the game's space under a root empty turned 90 degrees on X (game Y up).

**Export** writes into the imported folder, or another one (copying the rest). Only files with edits are rewritten:
- glTF: a mesh with the same vertices and faces gets its moved positions written into the same accessors of its `.bin`
  (node names, `extras.mds_index`, normals, UVs kept), so dc1tk's vertex_patch keeps the model's size. Moving or
  scaling a mesh object is baked into its vertices. A mesh whose faces changed is written as new primitives and
  dc1tk rebuilds the model; the add-on warns, as the build may come out larger than retail (MoonMod refuses that).
- floor.cfg: moved torches rewrite their PT_FIRE line, nothing else.
- scene.json: part position/rotation and cells. layout JSON: part, quarter turns, position.

**Build** runs the matching dc2tk build into the build output folder and shows its size check (text "dc2tk output").

## Test
```bash
blender -b --python blender-plugin/tests/dc1_roundtrip.py -- E:/dc1tk-work/maps E:/dc1tk-work/bltest/rt
```
d01main_a, gedit/e01/in/0101/0101.scn and gedit/e01/gdata0.edt export unchanged and build byte-identical to retail;
with `edit` at the end, a vertex, a torch, a scene part and cell and a layout part change and all build at retail size.
