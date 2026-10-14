# new-character: a character of its own, made in Blender

A character that is no one in the game, waving in Barga's Valley (`s17`):
its own skeleton of seven bones, a mesh of boxes skinned to them, a picture
and one motion, `wave`, all made in Blender (`make_model.py`). Nothing of the
game's is in this folder.

- `model/newchr.gltf` (with `newchr.bin` and `newchr_skin.png`) is the
  character as Blender exports it. `mod.json` makes it a new pack,
  `mods/newchr/newchr.chr`, like the villager `p01_01a`'s: build writes its
  model, skin weights, bind pose, motion, picture and an `info.cfg` naming
  them (see [usage](../../usage.md#mods), "Character models as glTF").
- `newchr.c` adds the start program a new game begins at: it loads the pack
  with `load_chara` into the scene's sub-character stack, stands it at the
  map's "talk" point and starts `wave`.

```bash
c2 build extracted/scripts dc2.iso -o new-chara-test.iso --mod new-character --mods docs/examples
```

To make the model again (or change it and build again):

```bash
blender --background --factory-startup --python docs/examples/new-character/make_model.py
```

By hand: model a mesh, add an armature, parent the mesh to it with
weights, give it a material with an image texture (sides powers of two),
set the scene to 30 frames a second, key an action, and export glTF 2.0 as
"glTF Separate" into `model/`.
