# Maps and Blender

`dc2tk setup` exports all 3D maps into `extracted/maps/<map-code>/` as `.gltf` scenes for Blender.

## Quick steps

1. Copy a map into your mod folder (or export it with `dc2tk map-export m01`):
   ```sh
   dc2tk mod asset dc2.iso my-mod map:m01
   ```
2. Open `mods/my-mod/maps/m01/m01.gltf` in Blender, edit geometry, collision, or triggers, and export back to `.gltf`.
3. Verify the entry in `mods/my-mod/mod.json`:
   ```json
   {
     "replace": {
       "map:m01": "maps/m01/m01.gltf"
     }
   }
   ```
4. Build the ISO with `dc2tk build`.

## Installing the Blender add-on

1. Download `dc2tk-<version>-blender.zip` from [GitHub Releases](https://github.com/fakegenie/DC2-Toolkit/releases) (or zip `blender-plugin/dc2tk_maps`).
2. In Blender (4.2+), open **Edit -> Preferences -> Get Extensions** (or **Add-ons**), click the top-right menu -> **Install from Disk**, and select the zip file.
3. Press `N` in the 3D Viewport and open the **DC2 Map** tab.

## The glTF scene

Each map `.gltf` scene contains four top-level groups:

| Group | Contents |
| --- | --- |
| **`parts`** | Visual 3D meshes, props, and textures placed in the map. |
| **`collision`** | Collision meshes that block movement and set floor height and surface sound/water types. |
| **`func_points`** | Doors, search spots, save points, time gates, and camera triggers (`program` sets the script `PROGRAM` ID). |
| **`positions`** | Named spawn points where scripts place the player or NPCs. |

## A map of a mod's own

### A new map

Add a new map under `"add"` in `mod.json` using an existing map as a `"like"` template:

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

Each entry in `"doors"` places an entrance trigger in `"in"` (`m01`) that warps the player to `"spawn"`, and an exit trigger in the new map that warps back to `"return"`.

### A new map's own script

Set `"script": "fishroom.c"` to compile `mods/<your-mod>/fishroom.c` as the event script for the new map. See [The Spinning Cult](../scripting/examples/spinning-cult/README.md) (`fishroom.c`) for a working example.

See [DC2 Internals: Maps](../internals/maps.md) for `.map`, `.pcp`, `.mpk`, and `.ipk` binary specifications.
