# 3D Models (glTF)

`dc2tk setup` exports character, monster, weapon, and item 3D models (along with their skeletons, textures, and animations) into `extracted/assets/models/` as standard `.gltf` files that open in Blender.

## Edit an existing character or monster model

1. Copy the model into your mod with `dc2tk mod asset`:
   ```sh
   dc2tk mod asset dc2.iso my-mod model:e33a
   ```
   > **Hero outfits (`*` wildcard):** Max (`c01*`) and Monica (`c02*`) store multiple copies of their outfit meshes across cutscene and dungeon packs (`c02a`, `c02b`, `c02d`, `c02fuku`). Use a wildcard ID such as `model:c02fuku*` so `dc2tk build` updates every pack that holds that outfit.
2. Open the `.gltf` file in Blender (File $\rightarrow$ Import $\rightarrow$ glTF 2.0).
3. Edit the mesh, UV map, textures, or animations, and export back to `.gltf` (File $\rightarrow$ Export $\rightarrow$ glTF 2.0, format **glTF Separate (`.gltf` + `.bin` + textures)**).
4. Build your ISO with `dc2tk build`.

In `mod.json`:

```json
{
  "replace": {
    "model:e33a": "models/e33a/e33a.gltf"
  }
}
```

## Animations (`.mot` / Blender Actions)

When you open an exported `.gltf` in Blender, all of the character's animations appear as **Actions** in the Action Editor / NLA:

- Modifying an existing Action updates that animation in-game.
- Creating a new Action in Blender adds a new animation (`KEY`) to the character's `info.cfg` so scripts can play it by name with `set_actor_motion` or `set_motion`.

## Add a brand-new model pack (`.chr`)

To add a custom character, prop, or item model that scripts can load with `load_scene_chara(slot, "mods/gill/gill.chr")`, add a `"file:..."` entry under `"add"` in `mod.json` with a `"like"` template model:

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

- Use a townsperson (such as `"model:h09"`) as `"like"` for an animated character with bones and motions.
- Use `"model:uki"` (the fishing float) or `"model:takaraky"` for a static prop or item model.
- See [The Spinning Cult](../scripting/examples/spinning-cult/README.md) (`make_gill.py`) and [Calculator](../scripting/examples/calculator/calculator.c) for working examples.

## Rules and limits

- **Bone weights**: Each vertex can be weighted to at most **3 bones** (the PS2 engine skinning limit).
- **Texture sizes**: Model textures must have power-of-two width and height (`64x64`, `128x128`, `256x256`).
- **Memory**: Hero outfits and attachments must fit inside their fixed party memory stacks (see [Memory Limits](memory.md)).
- For binary details on `.mds`, `.wgt`, `.bbp`, and `.mot` files, see [DC2 Internals: 3D Models](../internals/models.md).
