# Maps: the code

How dc2tk reads, writes, exports and rebuilds maps: the modules, what is
settled and how it was checked, and how the glTF scene's exporter and
importer were planned and built. The formats are in [the game's
maps](../../../docs/game/maps.md) (section numbers below are that page's);
editing a map and the scene's conventions as a person editing one sees them
are in [editing maps](../../../docs/modding/maps.md). Map meshes go through
the MDS code of [models](models.md). Random floors are in
[dungeons](dungeons.md). The Blender add-on's own code is described in
[its README](../../../blender-plugin/README.md).

## Status

**Settled (engine code and whole-corpus checks):**
- All 210 `.map` files parse into a typed tree and are written back byte for
  byte, also through JSON: `dc2tk-assets`' `mapfile.rs` (`mapfile::parse`,
  `mapfile::write`; `cargo test --release -- --include-ignored corpus` with
  `DC2TK_CORPUS` set to the
  extracted disc checks them all). The tree keeps each number as the file
  types it (int, float, or an exponent the engine reads as 0) and every
  optional line and argument count of section 2.3; the layout is computed.
  New numbers are written as `%g` but never with an exponent.
- `dc2tk map` lists trigger points from that tree: a template's points once for
  every placement of it, under the placement's name and moved by its
  transform; the map's own points as part `""`.
- All 13,372 `.mds` files under `map/` (12,730 in `.pcp` packs, 501 in sky
  packs, 123 in character packs, 18 in `.skyb`) parse and build back byte for
  byte with `dc2tk-assets`' `mds.rs`, and export with `to_gltf.rs` unchanged.
- What `-m`, `-a` and `-c` meshes are, the collision layout and its per-triangle
  attributes, how textures are found, how pieces are placed (sections 2-4).
- `FUNC_ANIME_DATA` (animated pieces), every field, from `CObjAnime`'s
  assembly, with each of its 848 lines accounted for (section 5.1).
- One map model to glTF and back (below, "One map model to glTF and back"):
  whole node names, matching by `extras.mds_index`, real textures from a
  search path of `.img` files, blend flags as material settings, collision
  attributes as material names. Every map `.mds` exports, and its unedited
  glTF builds back byte for byte.

- The scene: `dc2tk map-export NAME` writes a map as one glTF scene
  and a mod's `"map:NAME"` line builds an edited scene back into the disc
  (below, "A scene built into a mod"): the `.map`, the meshes that changed,
  new models and pictures. Unedited scenes of the corpus change nothing;
  edits made in Blender 5.1 (points moved and added, meshes moved, collision
  painted, a new model, a texture tinted and a new picture) build and read
  back as made.

- Checked in the game with an example mod (since removed): a model made in
  Blender stands where it was put with a picture of its own, its collision
  copy blocks the player, and a repainted map picture shows.

- Template libraries (the dungeon floors' parts, add maps) export with each
  template shown once and build their edits back into the templates;
  random floors' layouts are `data:layouts` ([dungeons](dungeons.md)).
- New maps: a mod's `"add": {"map:m05x": {"file": ..., "like": "m05"}}`
  builds a map of its own from an edited copy of another's scene, with a row
  in every language's map table, so `--start-at m05x` reaches it (checked in
  the game). Its own script (`"script"`) and a door into it from an existing
  map with a way back build (the example spinning-cult uses both); not yet
  tried in the game.

- What a scene only shows: the sky by time band and what the `.cfg` adds
  (water, occlusion planes, hidden areas, extra trigger points, villagers).
  s17 and m05 exported with them import unchanged, and m01 too through the
  add-on in Blender 5.2. Animated character pieces (`.chr`) are still shown
  without their models.

What is not yet known of the formats is on [the game's
maps](../../../docs/game/maps.md).

## Notes on the formats for the code

- `im3::read` skips a map `.img`'s `#texanime` scripts (section 4.2), so
  they must be kept inside the `.img`.

## Editing maps in Blender (the plan)

**Export a map to one glTF scene:**
- Templates become reusable node trees and placements instances of them, so a
  door placed 30 times is one door; each piece is a node, its mesh shared.
- Meshes through `to_gltf.rs` with the map options (below): node names kept
  whole (flags included) and `extras.mds_index` to match nodes back.
- Real textures from the map's IMGs and a search path of shared ones; GS
  CLAMP to glTF wrap; name flags to glTF materials (done, below; double-sided
  is not: whether the engine culls back faces is untraced).
- Collision (`-a`, `-c`) in their own collections, hidden by default, with
  attributes as named materials (done, below).
- Trigger points as empties (cubes for their boxes) with kind and data in
  custom properties; point lights as Blender lights; fog, light sets, time and
  fixed cameras as scene properties.

**Import into a mod:**
- Changed placements and pieces rewrite the `.map` (section 2.3); placements
  are only appended, never reordered (scripts address them by slot).
- Only meshes that changed are rebuilt (the others stay byte for byte, as for
  characters), matched by `extras.mds_index`, not by base name; edited
  collision is written as type 0x213 triangles with positions only and the
  attribute encoding of section 4.3 (done for one model, below).
- New meshes go into the map's model pack with an `info.cfg` entry (0x1FF at
  most); packs are patched member by member (`dc2tk_disc::rebuild::replaced_member`
  works two packs deep), never repacked: `models::write_pack` reproduces none
  of the map packs.
- New textures go into an `.img` inside the `.ipk`, keeping its `#texanime`.
- Limits: 320 placements (400 dungeons; the largest map has 245), names up to
  31 characters, `TIME_LIGHT_NUM` 8 or less. Memory: see
  [memory](../../../docs/game/memory.md) (read buffer, loading-screen limit,
  placements, texture blocks); `build` checks them.

### One map model to glTF and back (written)

The pieces the scene exporter and importer call, in `dc2tk-assets` (details in
[models](models.md), the glTF export's options for map models and
`from_gltf.rs`):

- `models::export_map_model(data, stem, options)` exports one `.mds`;
  `to_gltf::Options::map(Some(&search))` for drawn meshes,
  `Options::collision()` for `-a`/`-c` (`to_gltf::is_collision_file`).
- `to_gltf::TextureSearch::new(&[(file, bytes)])` is the texture search path:
  the map's `.img` files first, then shared ones; the first file holding a
  picture name wins. Names it lacks get placeholders and are listed in
  `Exported::missing_textures`; `no_image` pictures (filled by `#texanime`)
  in `no_image_textures`.
- `models::build_map_model(given, original, options)` builds the `.mds` back:
  only meshes that differ from an export with the same options are rebuilt,
  each into the frame its `extras.mds_index` (or else its whole name, then its
  base name) gives. Collision is rebuilt as 0x213 position-only triangle lists
  with the attributes its material names give; `from_gltf::collision_problems`
  checks the engine's rules.
- `to_gltf::FrameFlags` (name flags as the engine reads them) and
  `to_gltf::CollisionAttributes` (the diffuse encoding) are public for the
  scene tools.

**Checked on the whole disc** (a research sweep, about 35 s): all 13,372 map
`.mds` export (7,661 `-m`, 2,657 `-a`, 2,028 `-c`, 1,026 others; 71,683 nodes,
55,526 meshes), each with every node named whole and carrying its
`mds_index`, and every unedited glTF builds back to the file's bytes through
the change detection. Every one of the 4,685 collision files, rebuilt whole in
collision mode, passes the rules and gives the same triangles with the same
attributes. With each map's own `.img` files and `map/cmn`'s as the search
path, 17,393 textures are real pictures, 1,753 are `no_image` and 158 are
missing: 129 (map, texture) pairs, of which 121 are in another map's `.img` and
8 nowhere on the disc (`e01h03_06`, `e05g01_01`, `g050104`, `noIcon`). Materials:
6,007 MASK (alpha test), 5,897 BLEND, 8,195 unlit; 6,917 clamped samplers.
For each of the 1,486 `-m` files whose mesh frames repeat a base name, moving
the last such mesh in the glTF changes that frame alone (all others keep their
bytes); for every collision file, renaming one material to a new foot sound
rebuilds just the frames using it, which decode to the new attribute.
`s/s17` (5 files, 158 meshes) and the town `m/m05` (Palm Brinks, 33 files)
export and build back the same way; with every map's `.img` searched, m05
lacks only `e05g01_01` and s17 nothing.

### A scene built into a mod (written)

`dc2tk_assets::map_import::import_map(gltf, read, map, packs, textures)` takes
an edited scene and the map's files as the disc (or an earlier mod) has them,
and gives what to write; `dc2tk build` does so for each `"map:NAME":
"maps/NAME/NAME.gltf"` line of a chosen mod (`dc2tk_script::mapedit::apply_maps`).
What it writes, as a mod's author sees it, is in [editing
maps](../../../docs/modding/maps.md); how:

- **The `.map`:** the scene tree is read back (`map_scene::read_scene`) and
  the map rebuilt (`map_edit::rebuild`).
- **Meshes:** each model is taken from the first piece placing it and
  rebuilt with `models::build_map_model` when its meshes differ from an
  export **by value** (`from_gltf::changed_geometry`): matrices within 1e-3,
  every triangle matched in any corner order, positions within 0.005, UVs
  within 0.001, colours within 4/255, triangles grouped by material and the
  picture it shows. Blender splits and reorders vertices, recomputes normals,
  adds float noise and moves colours a step, so comparing bytes would rebuild
  every mesh. Blender also writes some meshes' colours without alpha, and
  gives every primitive of a mesh what any of them has (UVs of 0, white
  colours); none of that counts as a change, and a rebuilt mesh takes each
  corner's missing alpha from the nearest vertex of the mesh it replaces,
  keeping the fades. Positions (vertices, and mesh, placement and piece
  translations) compare within 0.005 and 2e-6 of their value, absolutely, so
  a move far from the origin is still seen: Blender keeps world matrices in
  32 bits, which moves things thousands of units out by a few thousandths.
  Every one of the 171 scenes `dc2tk map-export --all` writes, imported and
  exported unedited by the add-on in Blender 5.1, changes nothing.
- **New models:** `models::new_map_model` builds a piece's new model from
  the meshes under it.

### The scene's code

- `dc2tk_assets::map_export::export_map` writes the scene (`dc2tk
  map-export`).
- **Transforms** (`map_transform.rs`): the importer keeps the original
  numbers of any node whose matrix did not change (within 1e-4, its
  translation within 0.005 and 2e-6 of the value) and writes new Euler angles
  only for nodes that moved.
- **Template libraries:** the previews are laid out by
  `map_scene::template_previews`. d01f01 (244 templates, 296 models) goes
  through the add-on in Blender 5.1 and back unchanged, and a piece raised in
  one preview changes that template alone.
- **The `.cfg`** is shown from `map_display::cfg_json` (from
  `CMap::LoadCfgFile`).
- **Numbers.** Blender turns ints into floats in lists that mix them, and
  matrices pick up noise near 1e-5. The importer compares every number with
  the original's and keeps the original (its int or float form and text) when
  they agree within 1e-5 of the larger of 1 and the value.
- The root's `map` tree uses `mapfile.rs`'s field names.

### New maps (written)

A new map's line imports the scene as a `replace` line would; `"compact":
true` is `map_edit::Slots::Compact`. Its rows in the map tables are written by
`dc2tk_script::maptable`.

Checked in the game (`new-map-test.iso`): a copy of `m05` with its 63 trees
raised 250 units and two ground pictures repainted builds (4 new files, 7
tables), `dc2tk map-export m05x` from the built image reads it back as made,
while `m05` keeps its pictures, and `--start-at m05x` enters it.
