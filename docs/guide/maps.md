# Maps and Blender

`dc2tk setup` exports every 3D map in the game into `extracted/maps/<map-code>/` as a `.gltf` scene that you can open and edit in Blender. We also provide a Blender add-on (`dc2tk-*-blender.zip` on the Releases page) to inspect and edit map properties, collision, lighting, and trigger points.

## Installing the Blender add-on

1. Download `dc2tk-<version>-blender.zip` from [GitHub Releases](https://github.com/fakegenie/DC2-Toolkit/releases) (or zip `blender-plugin/dc2tk_maps`).
2. In Blender (4.2+), open **Edit $\rightarrow$ Preferences $\rightarrow$ Get Extensions** (or **Add-ons**), click the top-right dropdown $\rightarrow$ **Install from Disk**, and select the zip file.
3. Open the **DC2 Map** tab in the 3D Viewport sidebar (`N` key).

## The glTF scene

When you open `extracted/maps/m01/m01.gltf` (or import a map via the Blender add-on), the scene is organized into collections/empties:

- **Visual pieces (`parts`)**: The 3D meshes and props placed in the map. You can move, rotate, duplicate, edit meshes, or add new meshes and textures.
- **Collision (`collision`)**: Collision meshes that stop the player and define ground height and surface sound/water types.
- **Trigger points (`func_points`)**: Doors, search spots, save points, time gates, and camera triggers (`program` number sets which script `PROGRAM` runs when triggered).
- **Positions (`positions`)**: Named spawn points where scripts place the player or NPCs when entering the map.

## Edit an existing map

1. Export the map (if not already in `extracted/maps/<map>/`):
   ```sh
   dc2tk map-export m01
   ```
2. Copy or save your edited `.gltf` into your mod folder (for example, `mods/my-mod/maps/m01/m01.gltf`) and register it under `"replace"` in `mod.json`:
   ```json
   {
     "replace": {
       "map:m01": "maps/m01/m01.gltf"
     }
   }
   ```
3. Build your ISO with `dc2tk build`.

## A map of a mod's own

### A new map

You can add a brand-new map (such as a custom interior room or arena) under `"add"` in `mod.json` using an existing map as a template (`"like"`):

```json
{
  "add": {
    "map:fishroom": {
      "file": "maps/fishroom/fishroom.gltf",
      "like": "m07",
      "name": "Strange Room",
      "script": "fishroom.c",
      "doors": [
        {
          "in": "m01",
          "at": [-218.0, 0.0, -781.0],
          "radius": 20.0,
          "spawn": [0.0, 0.0, -68.0, 0.0],
          "return": [-218.0, 0.0, -755.0, 0.0]
        }
      ]
    }
  }
}
```

- `"doors"` automatically places a working door trigger in `"in"` (`m01`) that warps the player into your new map at `"spawn"`, and places an exit door in your new map that warps back to `"return"`!

### A new map's own script

Setting `"script": "fishroom.c"` compiles `mods/<your-mod>/fishroom.c` as the event script for your new map. See [The Spinning Cult](../scripting/examples/spinning-cult/README.md) (`fishroom.c`) for a complete working example.

For binary details on `.map`, `.pcp`, `.mpk`, and `.ipk` files, see [DC2 Internals: Maps](../internals/maps.md).
