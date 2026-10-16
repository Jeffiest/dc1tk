# Dungeons

Random dungeon floors in *Dark Chronicle* are assembled at runtime from 3D room pieces and four JSON data tables:

| Data Table ID | What it controls |
| --- | --- |
| `data:floors` | Floor time limits, Spheda target holes, medal goals, and which monsters spawn on each floor |
| `data:chests` | Treasure chest item lists, mimic odds, and magic crystal odds for each floor |
| `data:layouts` | Grid size (`grid_width`, `grid_height`) and room count/types generated on each floor |
| `data:parts` | Which 3D room/corridor piece files and minimap icons each dungeon uses |

## Edit floor monsters, medals, or chests

1. Copy the table into your mod with `dc2tk mod asset`:
   ```sh
   dc2tk mod asset dc2.iso my-mod data:floors
   dc2tk mod asset dc2.iso my-mod data:chests
   ```
2. Open `mods/my-mod/data/floors.json` or `chests.json` and keep only the dungeons and floors you want to change.
3. Run `dc2tk build`.

For example, to change the monsters and time medal on Underground Channel Floor 1 (`d01`, floor `0`):

```json
{
  "dungeons": [
    {
      "dungeon": 1,
      "floors": [
        {
          "floor": 0,
          "time_limit": 120,
          "monsters": [
            {"id": 0, "count": 5},
            {"id": 168, "count": 2}
          ]
        }
      ]
    }
  ]
}
```

## Edit 3D dungeon room pieces

Each dungeon's 3D room and corridor pieces are exported as `.gltf` scenes in `extracted/maps/` (such as `d01p01` to `d01p15`). You can edit any dungeon piece in Blender and replace it in `mod.json` with `"map:d01p01": "maps/d01p01/d01p01.gltf"` just like a regular map (see [Maps and Blender](maps.md)).

For binary details on how the engine generates dungeon floors, see [DC2 Internals: Dungeon Floors](../internals/dungeons.md).
