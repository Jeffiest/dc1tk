# Game Data (JSON)

`dc2tk setup` exports the game's data tables (items, weapons, shops, monsters, inventions, fishing, Georama, and dungeon floors) to `extracted/data/*.json`.

## Quick steps

1. Copy a data table into your mod folder:
   ```sh
   dc2tk mod asset dc2.iso my-mod data:items
   ```
2. Open `mods/my-mod/data/items.json` and keep only the `id` and fields you want to change. `dc2tk build` merges your changes with the base table and other mods.
3. Verify the entry in `mods/my-mod/mod.json`:
   ```json
   {
     "data": {
       "items": "data/items.json"
     }
   }
   ```
4. Build the ISO with `dc2tk build`.

## Available data tables

| Table ID | Controls | Key fields |
| --- | --- | --- |
| `data:items` | Items, weapons, attachments, Ridepod parts, and gifts | `id`, `name`, `description`, `kind`, `max_stack`, `weapon`, `attachment` |
| `data:shops` | Shop inventories and item prices | `items` (`id`, `price`, `shops` per chapter) |
| `data:monsters` | Monster stats, drops, 3D model, and AI script | `id`, `model`, `script`, `hp`, `attack`, `defense`, `exp`, `abs`, `gilda`, `items` |
| `data:inventions` | Invention recipes and photo ideas | `inventions` (`result`, `idea_1`..`3`), `ideas`, `scoops` |
| `data:floors` | Dungeon floor medal goals and monster spawn lists | `dungeons` -> `floors` (`time_limit`, `spheda_holes`, `monsters`) |
| `data:chests` | Treasure chest item pools and mimic odds per floor | `floors` (`items`, `magic_crystal_chance`, `mimic_chance`) |
| `data:layouts` | Dungeon room generation rules | `floors` (`rooms`, `grid_width`, `grid_height`) |
| `data:parts` | Dungeon room piece definitions and minimap tiles | `parts`, `minimap` |
| `data:georama` | Georama building parts, materials, and carpenter points | `parts` (`id`, `cp`, `materials`) |
| `data:requests` | Georama town completion conditions | `requests` |
| `data:town_water` | Georama river and water piece definitions | `rivers`, `water` |
| `data:townspeople` | Recruitable NPC names and Georama house rules | `npcs` |
| `data:fishing` | Fishing spots, fish stats, and Finny Frenzy races | `spots`, `race_fish`, `tournaments` |
| `data:characters` | Hero starting stats, weapon build-up trees, and Ridepod | `characters`, `ridepod` |

See [DC2 Internals: Game Data Tables](../internals/data.md) for binary table formats.

## Examples

### Edit an item or weapon (`data:items`)

Include only the `id` and modified fields:

```json
{
  "items": [
    {
      "id": 2,
      "name": {"en": "Super Wrench"},
      "weapon": {
        "attack": 250,
        "durable": 99,
        "flame": 100
      }
    }
  ]
}
```

### New items' names and descriptions

Add new items using free item IDs up to `435`, or edit `"name"` and `"description"` per language (`"en"`, `"fr"`, `"de"`, `"it"`, `"es"`). `dc2tk build` updates all menu and system message files (`comdatmes`, `itemmes`, `itemhelp`) automatically.

### Shops

Edit `"price"` or assign an item to a `"shop"` in `data:shops` (`"1"` = sold from the start of the chapter, `"2"` = sold after the mid-chapter update):

```json
{
  "items": [
    {
      "id": 251,
      "price": 50,
      "shops": [
        {"shop": 0, "chapters": ["1", "1", "1", "1", "1", "1", "1", "1"]}
      ]
    }
  ]
}
```

### Edit a monster (`data:monsters`)

Edit monster stats, drops, 3D model (`"model"`), or AI script (`"script"`):

```json
{
  "monsters": [
    {
      "id": 168,
      "hp": 5000,
      "attack": 300,
      "gilda": 500
    }
  ]
}
```

> **Note:** The engine has 343 fixed monster slots (`id` `0` to `606`). You cannot add new monster IDs, but you can edit any existing monster or reuse unused slots.
