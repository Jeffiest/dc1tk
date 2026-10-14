# c2_maps: a Blender add-on for Dark Chronicle maps

`c2_maps` imports a map that `c2 map-export NAME` wrote as a glTF scene, shows
it as the game draws it, lets you edit its placements, trigger points,
collision, settings, meshes and textures and model new objects, checks it
against the game's limits, and exports it into a mod that `c2 build` turns
into a disc image. The scene's conventions (node names, `extras.c2`,
collision material names) are in [the map formats](../../docs/assets/maps.md),
section 8; the add-on follows them and never renames what carries data.

Blender 4.2 or newer (tested with 5.1); written to run on 3.6 too. Plain
Python, nothing to install besides the add-on.

## Install

Either way, then set the repository folder in the add-on's preferences
(Edit, Preferences, Add-ons, "Dark Chronicle Maps (c2)"). When the add-on is
linked from this repository, the repository is found by itself.

- **Link it (follows the repository):** link `src/blender/c2_maps` into
  Blender's add-ons folder and enable it.
  - Windows (Blender 4.2+: the extensions folder):
    `mklink /J "%APPDATA%\Blender Foundation\Blender\5.1\extensions\user_default\c2_maps" "C:\path\to\ChronicleTwo-Script\src\blender\c2_maps"`
  - Linux, macOS: `ln -s /path/to/ChronicleTwo-Script/src/blender/c2_maps ~/.config/blender/5.1/extensions/user_default/c2_maps`
    (`~/Library/Application Support/Blender/5.1/...` on macOS)
  - Blender 3.6-4.1: the same into `scripts/addons/` instead of
    `extensions/user_default/`.
- **Install a zip:** zip the `c2_maps` folder (the zip holds `c2_maps/...`),
  or `blender --command extension build --source-dir src/blender/c2_maps`,
  then Edit, Preferences, Add-ons, Install from Disk.

Preferences:

| setting | default |
|---|---|
| c2 repository | the repository around the add-on, when linked |
| c2 executable | `build/release/c2.exe` (`c2` elsewhere), else `c2.bat` / `./c2` in the repository |
| map export arguments | `map-export {name} -o {out}` |
| exported maps | `extracted/maps` in the repository |
| mod folder, source tree, retail and output disc images | for Export and Build; the source tree defaults to `extracted/scripts` |

## Use

Everything is in the 3D viewport's sidebar (N), tab **Dark Chronicle**, and
in File, Import.

**Import.** Type the map's name (`s17`) and press Import: the add-on runs
`c2 map-export s17 -o extracted/maps/s17` and imports the result. File,
Import, "Dark Chronicle map (.gltf)" imports a scene exported before. The
import keeps every custom property (`extras.c2`), merges no vertices and keeps
the node tree, then:

- moves collision (pieces of kind `collision`) and camera collision (`camera`)
  into the collections "NAME collision" and "NAME camera collision", hidden;
  the Collision and Camera col. buttons show them;
- colours collision by area (water areas 1 and 7 blue), foot sound, ground or
  ignore mask, with the materials' viewport colour only: the names
  (`col_ground0_foot2_area6_mask12`) are the data and stay as they are;
- shows trigger points as empties: event points as cubes of their size (doors
  15, ladders 25, chests 20 when the scale is 1), invent boxes as cubes and
  their data's min-max box drawn in the viewport, sounds as spheres of their
  near distance;
- makes point lights (`plight`) Blender point lights, with the glTF light's
  range as the light's custom distance;
- draws the materials as the game does (Material Preview and Rendered): the
  picture times the vertex colour, alpha from both (the fades painted into
  terrain), additive glows (waterfalls, light shafts) and subtractive ones,
  unlit meshes, and alpha tests that still blend (soft shadows). The draw
  flags come from the model's frame names (the material's `frame_flags`).
  Blender's own material stays in each node tree, unlinked; the export links
  it back while it writes, so what is written is the same;
- lights the scene as the map's active light set does (Rendered and F12):
  a world "c2 NAME" with its background colour behind everything and its
  ambient colour around, and its directional lights as suns ("c2 light NAME
  N", outside the map, never exported). A world of your own is kept;
- turns the 3D view to Material Preview with the scene's world and lights
  (the light set's, so the preview looks like the game rather than a studio)
  and frames where the map is played (its trigger points), leaving nothing
  selected;
- shows the pieces the game draws at the hour in the Map panel (noon to
  start): towns hold day and night versions of pieces (`PIECE_TIME`), and
  showing both at once covers the town. Untick the clock for every piece.
  Hidden collision is left out of renders too.

**Edit.** Select something; the Selected panel shows what it is and its
fields, which write straight into `extras.c2`:

- placement: name, template, group, shown, far clip (or none), its slot;
- piece: name, model, shown, hours, collision type;
- point: name, kind, enabled, hours, flags, and the fields of its kind (an
  event's kind with a menu of door, event, t_box..., its event number,
  program, arguments, target and flags; a light's power, colour, mode and
  flicker; a sound's number, near and far; an invent box's id and corners...).
  A point light's colour also sets the Blender light's (r, g, b / 255);
- "All values" lists every value of `extras.c2`, each with an edit button,
  for anything without a field of its own (cameras, piece materials).

Map Settings shows the map root's settings (`extras.c2.map`: light sets,
fog, background colours, time, sky...) the same way.

Move, turn and scale with Blender's tools as usual. Edit meshes as usual
too: a mesh is shared by every piece that places the same model, as in the
game. Building rebuilds only the models whose meshes you changed (Blender's
own rewriting of untouched meshes is allowed for), and a rebuilt mesh keeps
the vertex alpha fades Blender leaves out.

**Textures.** Paint on a map picture (Texture Paint, or edit the PNG in
`textures/` and reload it), or give a material another of the map's
pictures or a new image: building puts a changed picture back into the
texture file that holds it, and a picture the disc lacks into the map's own,
named by the image (31 Latin-1 characters at most, sides powers of two up to
1024).

**Add.** Points at the 3D cursor: a position (`point`, for spawns and
doors), door, treasure chest, event trigger, point light, sound emitter,
invent box, effect, fire, lens flare. They go into the active object's
placement (copied with its template) or, with nothing in a placement active,
the map's own `points`. Names are made unique within their parent and kept
to 31 bytes. Duplicate Placement copies a placement with its pieces and
points (meshes shared) under a new name (`house01_2`); with no slot it is
appended after the map's placements.

**New models.** Model something (any meshes, with any of the map's pictures
or new images), select it and press New Model from Selection: it becomes a
new placement (`MAP_NAME`, at the meshes' centre) with a piece of a new model
(`MAP_NAME-m.mds`) holding the meshes where you put them, and, unless you
untick it, a collision copy (`-a`, triangulated, area 6) so the player cannot
walk through it, and if you tick it a camera collision copy (`-c`). Building
adds the models to the map's model pack with their `info.cfg` entries.

**Collision.** Show collision, select a collision mesh, go into Edit Mode,
select faces, set ground, foot sound, area and mask in the Add panel's
Collision box, and press "Set selected faces": the faces get the material
`col_groundG_footF_areaA_maskM`, made when missing.

**Check.** Validate Map lists errors and warnings (select an entry's object
with its button; the whole list is in the text "c2 validation"): names over
31 bytes (cp932), event targets over 15, repeated placement names, more than
320 placements (400 for dungeons), unknown point kinds, collision faces that
are not triangles or whose material is not a `col_` name, more than 8
`TIME_LIGHT_NUM`, and nodes without `extras.c2`.

**Export.** Choose the mod folder (`mods/NAME`) and press Export Map to Mod.
After a validation without errors, the map under its `map:` root, and
nothing else in the file, is written as glTF Separate, +Y up, with custom
properties, to `maps/NAME/NAME.gltf` in the mod (textures in `textures/`),
and `mod.json` gets `"map:NAME": "maps/NAME/NAME.gltf"` under `replace`
(made when missing; other keys kept). Blender's `.001` suffixes are taken
off node and collision material names in the written file, since pieces and
points repeat in every placement of a template.

**Build.** Set the retail ISO, the output ISO and the source tree and press
Build ISO with Mod: `c2 build SOURCE ISO -o OUTPUT --mods MODS --mod NAME`
runs in the background, its last lines in the panel and all of it in the
text "c2 output".

## Tests

```
blender --background --factory-startup --python src/blender/tests/test_addon.py
```

enables the add-on, writes a small map with `tests/make_test_map.py` (two
placements of a template with a model, a collision and a camera-collision
piece and a point; a door, a point light with a KHR light, a sound, an
invent box, and two points whose data is a raw argument list; a fixed camera;
map settings), imports it (on Windows through a stand-in for `c2 map
export`), checks the game look, the light set's world and suns and the hour
filter, edits fields, adds a point of every kind, duplicates a placement,
makes a new model from a mesh, paints a collision face, validates, draws
every panel with a checking stand-in layout, exports into a mod and checks
the written glTF: every node and `extras.c2` of the source kept (edited ones
as edited), transforms unchanged, the new nodes with valid extras, the look
back after the export, `mod.json` updated; then runs `c2 build` through the
stand-in. `make_test_map.py OUT_DIR` also runs on its
own with any Python 3.

## What Blender does to the scene

Worth knowing when reading an exported scene:

- Object, mesh and material names are unique in Blender, so repeated names
  get `.001`; the export takes them off again.
- Custom properties hold JSON except integers beyond 32 bits and lists that
  mix text and numbers; a list mixing ints and floats comes back as floats
  (40 as 40.0). When a node's `extras.c2` cannot be stored, Blender's
  importer keeps it as text: the add-on shows it read-only and writes it
  back as JSON on export.
- The exporter writes children in name order and adds `NORMAL` to
  collision meshes; placement order is kept by `slot`.
- It keeps world matrices in 32 bits, so pieces of placements thousands of
  units out come back moved by a few thousandths; the importer takes
  positions within 0.005 as unmoved.
- It writes some meshes' colours without alpha, and gives every primitive of
  a mesh with colours a colour (white where there was none); neither counts
  as an edit.
- Textures it has not changed are copied as they are; changed ones are
  written again as PNG and compared by pixels.
