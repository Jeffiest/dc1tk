# map-edit: a map edited in Blender

Barga's Valley (`s17`) edited with the [c2_maps add-on](../../../src/blender/README.md),
built into a disc:

- the valley's grass and cliff picture (`s17_02`) turned to autumn colours;
- a crate modelled in Blender, with a picture drawn for it, made a new model
  of the map with New Model from Selection, its collision copy included, so
  the player cannot walk through it;
- a start program (`start.c`): a new game begins in the valley facing the
  crate.

Nothing of the game's is in this folder. `make_mod.py` makes the edits on
your own export of the map, as you would by hand in Blender, and writes the
mod into `mods/map-edit`:

```bash
c2 map-export s17
```

```bash
blender --background --python docs/examples/map-edit/make_mod.py
```

```bash
c2 build extracted/scripts dc2.iso -o map-edit.iso --mod map-edit
```

`build` prints what the map line changed: the `.map` (the crate's placement
and template), the model pack (`s17_crate-m.mds`, `s17_crate-a.mds` and
their `info.cfg` entries) and the texture pack (`s17_02` repainted,
`s17_crate` added).

To make the same edits by hand: import `s17` in the add-on's Map panel,
paint `s17_02` (or edit `extracted/maps/s17/textures/s17_02.png` and reload
it), add a cube near the "talk" point, give it a material with an image of
its own, press New Model from Selection, and export into `mods/map-edit`
with this folder's `mod.json` and `start.c` beside it.
