# Dungeons

Random dungeon floors in *Dark Chronicle* are generated at runtime from 3D room pieces and four JSON data tables.

## Quick steps

1. Copy a dungeon data table into your mod folder:
   ```sh
   dc2tk mod asset dc2.iso my-mod data:floors
   ```
2. Open `mods/my-mod/data/floors.json` and keep only the dungeons and floors you want to change.
3. Verify the entry in `mods/my-mod/mod.json`:
   ```json
   {
     "data": {
       "floors": "data/floors.json"
     }
   }
   ```
4. Build the ISO with `dc2tk build`.

## Dungeon data tables

| Table ID | Controls |
| --- | --- |
| `data:floors` | Floor time limits, Spheda target holes, medal goals, and monster spawn lists |
| `data:chests` | Treasure chest item pools, mimic odds, and magic crystal odds per floor |
| `data:layouts` | Grid dimensions (`grid_width`, `grid_height`) and room counts/types per floor |
| `data:parts` | 3D room/corridor piece files and minimap tiles used by each dungeon |

## Example: Edit floor monsters and time limit (`data:floors`)

Change the monsters and time medal on Underground Channel Floor 1 (`dungeon: 1`, `floor: 0`):

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

Dungeon room and corridor pieces are exported as `.gltf` scenes in `extracted/maps/` (for example, `d01p01` through `d01p15`). Edit any piece in Blender and replace it in `mod.json` with `"map:d01p01": "maps/d01p01/d01p01.gltf"` (see [Maps and Blender](maps.md)).

See [DC2 Internals: Dungeon Floors](../internals/dungeons.md) for binary floor generation rules.
