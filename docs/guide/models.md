# 3D Models (glTF)

`dc2tk setup` exports character, monster, weapon, and item 3D models (including skeletons, textures, and animations) into `extracted/assets/models/` as `.gltf` files.

## Quick steps

1. Copy a model into your mod folder (use a `*` wildcard for hero outfits such as `model:c02fuku*` to update all cutscene and dungeon copies):
   ```sh
   dc2tk mod asset dc2.iso my-mod model:e33a
   ```
2. Import `mods/my-mod/models/e33a/e33a.gltf` into Blender, edit the mesh, textures, or animations, and export as **glTF Separate (`.gltf` + `.bin` + textures)**.
3. Verify the entry in `mods/my-mod/mod.json`:
   ```json
   {
     "replace": {
       "model:e33a": "models/e33a/e33a.gltf"
     }
   }
   ```
4. Build the ISO with `dc2tk build`.

## Animations (`.mot` / Blender Actions)

Character animations import into Blender as **Actions** in the Action Editor / NLA:

| Action Edit | In-Game Result |
| --- | --- |
| Modify an existing Action | Updates that animation on the model. |
| Create a new Action | Adds a new `KEY` entry to `info.cfg` that scripts can play with `set_actor_motion` or `set_motion`. |

## Add a new model pack (`.chr`)

Add a `"file:<disc-path>"` entry under `"add"` in `mod.json` to create a new `.chr` model pack that scripts can load with `load_scene_chara`:

```json
{
  "add": {
    "file:mods/gill/gill.chr": {
      "file": "model/gill.gltf",
      "like": "model:h09"
    },
    "file:mods/calculator/calc.chr": {
      "file": "model/calc.gltf",
      "like": "model:uki"
    }
  }
}
```

| Template (`"like"`) | Use Case |
| --- | --- |
| `"model:h09"` (townsperson) | Animated character with skeleton and `.mot` motions (see [The Spinning Cult](../scripting/examples/spinning-cult/README.md)). |
| `"model:uki"` or `"model:takaraky"` | Static prop or item mesh (see [Calculator](../scripting/examples/calculator/calculator.c)). |

## Rules and limits

| Property | Limit |
| --- | --- |
| **Bone weights** | Maximum **3 bones** per vertex. |
| **Texture dimensions** | Power-of-two width and height (`64x64`, `128x128`, `256x256`). |
| **Party memory** | Hero outfits and weapons must fit inside fixed party memory stacks (see [Memory Limits](memory.md)). |

See [DC2 Internals: 3D Models](../internals/models.md) for `.mds`, `.wgt`, `.bbp`, and `.mot` binary specifications.
