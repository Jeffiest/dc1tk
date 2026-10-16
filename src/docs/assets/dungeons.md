# Dungeon floors: the code

What dc2tk implements for the random dungeon floors, how it was checked, and
what is not done. How a floor is built is in [the game's
dungeons](../../../docs/game/dungeons.md) (section numbers below are that
page's); editing the layouts, parts and chests is in [editing
dungeons](../../../docs/modding/dungeons.md). A floor's map goes through the
map code of [maps](maps.md).

## Status

**Settled (engine code and whole-corpus checks):**
- The room script's format (section 2): `dc2tk-assets`' `dungeon_layout.rs`
  reads all 23 scripts (291 room layouts) with the part names of the
  executable's `PartsInfoData` and writes them back byte for byte; every room
  rewritten from its JSON gives its own bytes (`cargo test --release --
  --include-ignored corpus` with `DC2TK_CORPUS` set).
- How a floor is laid out, from the layouts, and where its parts, chests,
  monsters and stones go (sections 3 and 4).
- `data:layouts` in `mod.json` changes rooms and grid sizes and adds rooms;
  the parts' models, collision and points are edited in Blender through
  `dc2tk map-export` of the floor's map. Both build and read back as made.
  An unedited export changes nothing (d01f01 imported and exported by the
  add-on in Blender 5.1).

- Which ways on a floor gets (`GetDngMapNextRoot`, from the dungeon's tree
  map `menu/dngmap/dmap<n>.cfg`, section 3), the dummy trees
  (`SetDummyTree`), and the parts' tile numbers (an older mini map table no
  code reads; the mini map's own is `MiniMapInfoData`), from the engine.
- `data:parts` edits the parts table and the mini maps' tiles in the
  executable, patched in place (section 2.1), and `data:chests` what the
  floors' chests hold (`tbox_d0*.cfg`, section 4.1); both read back as
  written, and every chest script writes back byte for byte.

**Not yet:** tried in the game. The table of parts cannot grow (section
2.1). `dmap<n>.cfg` is not yet a `data:` table. Giving a floor's map a
template of a new name from Blender is not tried yet.
