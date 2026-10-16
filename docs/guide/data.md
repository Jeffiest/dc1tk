# Game Data (JSON)

`dc2tk setup` exports the game's data tables (items, weapons, shops, monsters, inventions, fishing, Georama, and dungeon floors) into `extracted/data/*.json`.

To edit a table in a mod, copy it into your mod folder with `dc2tk mod asset`:

```sh
dc2tk mod asset dc2.iso my-mod data:items
```

This creates `mods/my-mod/data/items.json` and adds `"data:items": "data/items.json"` to `mod.json`. Keep only the rows and fields you want to change—`dc2tk build` merges your edits with the original game data and any other mods.

## Available data tables

| Table ID | What it controls | Key fields |
| --- | --- | --- |
| `data:items` | All items, weapons, attachments, Ridepod parts, and gifts | `id`, `name`, `description`, `kind`, `max_stack`, `weapon`, `attachment` |
| `data:shops` | What each shop sells and item prices | `items` (`id`, `price`, `shops` per chapter) |
| `data:monsters` | Monster stats, drops, 3D model, and AI script | `id`, `model`, `script`, `hp`, `attack`, `defense`, `exp`, `abs`, `gilda`, `items` |
| `data:inventions` | Invention recipes and photo ideas | `inventions` (`result`, `idea_1`..`3`), `ideas`, `scoops` |
| `data:floors` | Dungeon floor medal goals and random monster spawn lists | `dungeons` $\rightarrow$ `floors` (`time_limit`, `spheda_holes`, `monsters`) |
| `data:chests` | Treasure chest item pools and mimic chances per floor | `floors` (`items`, `magic_crystal_chance`, `mimic_chance`) |
| `data:layouts` | Dungeon room generation rules | `floors` (`rooms`, `grid_width`, `grid_height`) |
| `data:parts` | Dungeon room piece definitions and minimap tiles | `parts`, `minimap` |
| `data:georama` | Georama building parts, materials, and carpenter points | `parts` (`id`, `cp`, `materials`) |
| `data:requests` | Georama town completion conditions | `requests` |
| `data:town_water` | Georama river and water piece definitions | `rivers`, `water` |
| `data:townspeople` | Recruitable NPC names and Georama house rules | `npcs` |
| `data:fishing` | Fishing spots, fish stats, and Finny Frenzy races | `spots`, `race_fish`, `tournaments` |
| `data:characters` | Hero starting stats, weapon build-up trees, and Ridepod | `characters`, `ridepod` |

For the binary layout of these tables inside the game, see [DC2 Internals: Game Data Tables](../internals/data.md).

## Examples

### Edit an item or weapon (`data:items`)

You only need to include the `id` and the fields you are changing:

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

You can also add new items (using free item IDs up to `435`) or edit `"name"` and `"description"` per language (`"en"`, `"fr"`, `"de"`, `"it"`, `"es"`). `dc2tk build` automatically updates all menu and system message files (`comdatmes`, `itemmes`, `itemhelp`) for you.

### Shops (`data:shops`)

Change an item's price or add it to a shop inventory (`"1"` = sold from the start of the chapter, `"2"` = sold after the mid-chapter update):

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

Change a monster's HP, attack, defense, drops, or even swap its 3D model (`"model"`) and AI script (`"script"`):

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

> **Note:** The engine has a fixed table of 343 monster slots (`id` `0` to `606`), so a mod cannot add brand-new monster IDs, but you can repurpose unused slots or modify any existing monster.
