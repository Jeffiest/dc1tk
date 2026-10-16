# Editing game data

This guide explains how to edit item and weapon stats, shops, monsters, inventions, Georama, fishing, floor medals, townspeople, and executable tables using JSON files referenced in `mod.json`.

For details on what every field means, where the game stores each file, and the engine's exact array limits, see [game data](../internals/data.md). For dungeon floor room layouts, parts, and chests, see [dungeons](dungeons.md). For `mod.json` syntax, see [mods](mods.md).

## Exporting a table

To export one or more game data tables into your mod:

1. Run `dc2tk mod asset` with the desired `data:` IDs (`ISO` is your disc image or extracted disc folder):
   ```sh
   dc2tk mod asset ISO NAME data:items data:shops data:monsters
   ```
   This command writes the JSON files into `data/` and adds their `"data:..."` entries (such as `"data:items": "data/items.json"`) to `mod.json`. Any table that your mod already lists in `mod.json` is left untouched.
2. Edit the exported JSON files to keep only the records and fields that you want to change or add.
3. Run `dc2tk build`.

### Available `data:` tables

- **`data:items`** (`data/items.json`): Every item's common properties, its nested family stats (`weapon`, usable `effect`, `attachment`, `ridepod` part, or `fish`: attack, WHP, elements, limits, synthesis points, build-ups, and more), and what it adds when spectrumised (`spectrumise`).
- **`data:shops`** (`data/shops.json`): Each item's buy and sell price, each shop's inventory (a single value across all chapters, or a list of 8 values for chapters 1 to 8), and Donny's shop goods (`donny`).
- **`data:monsters`** (`data/monsters.json`): Each monster kind's HP, attack, defence, ABS, money, item drops, elemental and attack-type resistances, model, and AI script.
- **`data:inventions`**: Invention `recipes` (required ideas and crafting materials), `scoops`, and `ideas` (names, notebook sort order, and new ideas).
- **`data:georama`**: Each town's Georama `parts` (crafting materials, culture points, and limits), `fixed_parts`, `starting_parts`, `rivers`, and `water`.
- **`data:medals`**: Each dungeon floor's time trial goal, fishing record, seal condition, Spheda prizes, and practice medal.
- **`data:floors`**: Each dungeon floor's monster spawns.
- **`data:fishing`**: Fishing `spots`, `race_fish` opponents, `prize_groups`, `prizes`, fish `species`, and `race_kinds`.
- **`data:layouts`**: Room layouts used to build dungeon floors (each room's rows of parts as text and its selection rate; new rooms can be added—see [dungeons](dungeons.md)).
- **`data:parts`**: Dungeon floor part definitions and minimap tile arrays (renaming a part in place creates a new kind of part; see [dungeons](dungeons.md)).
- **`data:chests`**: Dungeon floor treasure chest contents (item groups by rank and each floor's assigned groups; see [dungeons](dungeons.md)).
- **`data:characters`**: Starting HP and equipment, WHP gained on weapon level-up, Monica's monster transformation stats, and badge growth tables.
- **`data:requests`**: Georama requests (completion conditions, shown/hidden map parts, and completion percentages).
- **`data:townspeople`**: NPC models, abilities, and ability point costs.
- **`data:gifts`**: Gifts that award a monster transformation badge.
- **`data:spheda`**: Spheda club power values.
- **`data:breeding`**: Aquarium fish breeding pairs.

Passing an unknown `data:` table name prints an error listing all valid tables.

## The tables

The list below summarizes each table, what it holds, and where the game stores it (with links to the corresponding sections of [game data](../internals/data.md)):

- **Items** (`data:items`, [section 2](../internals/data.md#2-items-menucfg7)): The seven `menu/cfg7` tables, storing every item ID with its nested `weapon`, `effect`, `attachment`, `ridepod`, and `fish` stats.
- **Shops** (`data:shops`, [section 3](../internals/data.md#3-shops-menushopusachaptercfg)): Prices and every shop's goods by chapter across the 16 shop list files.
- **Monsters** (`data:monsters`, [section 4](../internals/data.md#4-monsters-base_monster_define-in-sces_51190)): Stats for all 343 monster kinds, patched in place in `SCES_511.90`.
- **Spectrumising** (in `data:items`, each item's `spectrumise`: `slot`, `value`) and **Donny's shop** (in `data:shops`, `donny`: `row`, `item`, `level`): Executable tables ([section 9](../internals/data.md#9-characters-and-other-executable-tables)).
- **Inventions** (`data:inventions`, [section 5](../internals/data.md#5-inventions-ideas-and-scoops)): `recipes` (updated across all 19 copies of `inv6.lst`), `scoops` (executable table), and `ideas` (names and notebook sort order across all 7 copies of `neta2.lst`, by language).
- **Georama** (`data:georama`, [section 6](../internals/data.md#6-georama)): `parts` for every town in every language's `.gpi` file, plus `fixed_parts` and `starting_parts`.
- **Floor medals** (`data:medals`, [section 8](../internals/data.md#8-floor-medals-menudngmapdflrdungeoncfg)) and **monster placement** (`data:floors`, section 4): `floors` keyed by `dungeon` and `floor`.
- **Fishing** (`data:fishing`, [section 7](../internals/data.md#7-fishing)): `spots`, `race_fish`, `prize_groups`, and `prizes`, plus `species` and `race_kinds` from the executable.
- **Dungeon floor layouts** (`data:layouts`): The room layouts from `dungeon/cfg_file/d0*.cfg` and `mstage*.cfg` used to build random and preset floors (rooms are written whole and may be added; see [dungeons](dungeons.md)). Also covered in [dungeons](dungeons.md): `data:parts` (executable tables) and `data:chests` (`tbox_d0*.cfg`).
- **Characters** (`data:characters`, section 9): `characters` (starting HP and equipment), `weapon_level_up`, `monster_forms`, and `badges`—all executable tables.
- **Georama requests** (`data:requests`, [section 6.1](../internals/data.md#61-requests-geolanguagecfg)): Each town's `conditions` and `requests` in every language's `geo<N>.cfg`.
- **Rivers and water** (in `data:georama`, [section 6.2](../internals/data.md#62-rivers-and-water-gnncfg)): `rivers`, `water`, `water_surfaces`, and `water_draws` in each town's `gNN.cfg`.
- **Townspeople** (`data:townspeople`, [section 10](../internals/data.md#10-townspeople-npclanguagecfg)): Every language's `npc<N>.cfg`.
- **Gifts, Spheda clubs, breeding** (`data:gifts`, `data:spheda`, `data:breeding`, section 9): `gift_item_tbl`, `GolfClubDef`, and `aquafish_mixTable`, patched in place in the executable.

### Data stored outside `data:` tables

The following data is not stored in `data:` tables:

- The guard table (`grddat.cfg`, which no game code reads)
- Per-language item and monster names (`comdatmes`, `mosdata`: edited as text, except for an item's `name` and `description` in `data:items`, explained [below](#new-items-names-and-descriptions))
- Scoop names and texts (`neta2.lst`, `scoop.cfg`: edited as text)
- Whether a Georama condition is met (checked by C++ code)
- Where townspeople stand in towns (`npc_place*.cfg`)

## Keeping only what changes

Each table holds one record per item, price, shop, or monster, identified by its key fields (`item`, `shop`, `id`, `recipe`, `dungeon` and `floor`, and so on). Your mod only needs to keep the records and fields that it changes (an unedited full table changes nothing):

```json
{"items": [{"item": 1, "weapon": {"attack": 50}}]}
```

`dc2tk build` writes only the modified values:

- Items into `menu/cfg7`
- Shops into all 16 shop list files
- Values replicated across multiple files (the 19 copies of `inv6.lst`, and each language's Georama parts and race fish) into every copy
- Executable tables into the boot executable `SCES_511.90`, patched in place in a single pass without changing the executable's size

List fields must stay within the engine's limits (such as 1 to 4 materials for a recipe, or up to 32 monsters for a floor).

### Merging and validation across mods

- Tables from multiple mods merge automatically. If two mods set the same field to different values (or patch the same executable byte to different values), `dc2tk build` fails and names both mods.
- `dc2tk` rejects values that do not fit in the engine's field type, unknown field names, and edits to internal linking indices (`list`, or a ridepod part's `kind`).
- Name fields in data tables act as read-only reference labels (because localized text lives in message files), except on items (`name`, `names`, `description`, `descriptions`; see [below](#new-items-names-and-descriptions)).

## The JSON

Each section of a data table is a JSON list of record objects identified by their keys (`{"dungeon": 0, "floor": 3}`, `{"town": "g01", "part": 1}`), along with read-only label fields (which you can leave unchanged or omit) and editable data fields. Dotted field names (`a.b`) are nested JSON objects.

A JSON field maps to one of four underlying storage forms:

1. One or more arguments on a single script statement
2. A variable-length list inside a single statement (such as a recipe's materials, whose length you can change within engine limits)
3. A list spread across multiple statements (such as a floor's `FL` lines or a Georama part's `PARTS_MATERIAL` lines: `dc2tk build` fills existing lines in order, empties unused trailing lines when the list is shorter, and never adds extra lines)
4. Bytes inside the executable

When a field is stored across multiple copies or language files (the copies of `inv6.lst`, and each language's `.gpi` and `rfd`), `dc2tk` exports it from the first copy (the English file when languages differ) and writes changes to all copies.

### Strict schema and float formatting

- **Strict validation**: `dc2tk` validates every JSON member strictly. Any member with the wrong shape (such as `dungeon 0: "groups" is a list, not an object` or `"floors" is a list of records`) or any unknown section, field, or key causes an immediate error naming the offending field.
- **Floating-point numbers**:
  - Script float arguments export in the shortest decimal form that round-trips to the same `f32`, with `.0` appended when whole (`320.0`).
  - Executable floats export in their shortest form without `.0` when whole (`320`).
  - `data:items` keeps the historical format of its `.cfg` files on the disc, where `f32` values were widened to `f64` and printed with all digits (`0.9900000095367432`).
  - `dc2tk build` compares values as the engine stores them (integers exact, floats as `f32`), so writing `0.99` in `data:items` does not count as a change. If a mod writes a script float argument as a JSON integer (`320` instead of `320.0`), `dc2tk build` writes that integer directly (which the engine's float parser accepts); otherwise it writes the shortest `f32` representation with `.0` when whole.

## Adding records

Any record whose key does not exist in the table adds a new record. Include `"like"` (set to the template record's key value, or an object of its keys) to name an existing record to copy, and specify only the fields that differ:

```json
{"item": 429, "like": 1, "name": "Mega Wrench", "weapon": {"attack": 250}}
```

`dc2tk build` copies the `"like"` record's script statements, substitutes the new keys, labels, and fields, appends the record after the last record of its kind in every copy of the file, and increments the file's allocation count header (`WEPNUM`, `DATATABLESET`, `EDIT_PARTS_NUM`, `FISH_MAP_NUM`, and so on). New records from multiple mods are counted together; if two mods add the same record key with different values, `dc2tk build` fails and names both mods. For records keyed by sequential position (`recipe`, `spot`, `request`, or a placed Georama part's `number`), the new key must be the next sequential number.

### Records that can be added

`dc2tk build` refuses any addition that exceeds the engine's capacity limits ([game data](../internals/data.md#limits-on-counts)):

| Record type | Files updated by `dc2tk build` |
|---|---|
| Items (`data:items`) | `comdat.cfg`, the item family's table, all 7 `comdatmes` files, `system_N.mes`, `allmenu.mes` |
| Prices and shops (`data:shops`) | All 16 shop list files |
| Recipes (`data:inventions`) | All 19 copies of `inv6.lst` |
| Ideas (`data:inventions`) | All 7 copies of `neta2.lst` |
| Georama parts (`data:georama`) | All 7 `.gpi` files for that town |
| Fixed and starting parts (`data:georama`) | The town's `gNN.cfg` |
| Fishing spots and spot fish (`data:fishing`) | `fishing.cfg` (up to 256 spots, `dc2tk`'s own limit; no fixed engine array bounds them) |
| Georama requests (`data:requests`) | All 7 `geo<N>.cfg` files |
| Townspeople (`data:townspeople`) | All 7 `npc<N>.cfg` files |

### Records that cannot be added

- **Floor monsters (`data:floors`)**: Already support up to 32 monsters per floor by filling the floor's existing `FL` lines (the last `FL` line holds the remainder, up to the script reader's 64-argument limit per statement).
- **Fixed-size tables**: Monster kinds (the executable table has no spare slots), executable table rows (gifts, Spheda clubs, fish breeding pairs, and Donny's goods—all fixed-size arrays in `.data`), floor medals, race fish, fishing prizes, and Georama conditions (condition checks are hardcoded in C++).
- **Item icons and models**: A new item uses either an existing icon and model on the disc or a custom icon and model built by the mod ([below](#new-items-names-and-descriptions)).

## Items

In `data:items`, each `COM` statement is a record in file order, keyed by `item`, with `name` (the English name from `comdatmes1.cfg`, which acts as a label and renames the item when changed) and family stats nested under `weapon`, `effect`, `attachment`, `ridepod`, or `fish`. Items `1` to `425` also have `spectrumise` with `slot` and `value`.

### New items' names and descriptions

A new item must specify `"like"` naming an existing item of the same family (`type` may change within that family) and must provide `"name"` (used for every language) or `"names"` (`{"en": ..., "fr": ...}` for all 7 languages). `"description"` (all languages) and `"descriptions"` (per language) are optional and default to the `"like"` item's description in each language.

When building a new item, `dc2tk build`:

1. Assigns `"list"` to the next entry in the item's family table and increments that table's count.
2. Defaults `"message"` to the item's number unless explicitly specified.
3. Appends the new `COM` line after the last item in `comdat.cfg`.
4. Appends a copy of the `"like"` item's family block (`WEP`..`WEP_BUILD`, `AT_ST`..`AT_ST_SP`, or `ITEM`, `RB_PARTS`, `FISH`, `GRD`) with your modified fields.
5. Writes `MES_SYS` in all 7 `comdatmes` files.
6. Writes message `10000 + message` into each language's `meswin/system_N.mes` (keeping `system_N.mes` within its `0xD000`-byte buffer) and into every `allmenu.mes` in the menu packs that contains item descriptions.

For an existing item on the disc, changing `"name"` updates the English name, and `"names"` / `"descriptions"` update any language's name or description in those same files.

#### Special characters in item names

Item names and descriptions use standard message text syntax ([the text form](text.md#the-text-form)) in each language's font. In English, they can use accented letters outside the standard English table—any letter from the European font, or any letter that `dc2tk build` composes from an ASCII letter and a mark (such as `gŷros`), up to 32 extra letters shared across all active mods' `text:en/...` files ([letters the English font lacks](text.md#letters-the-english-font-lacks)). Because `comdatmes1.cfg` uses Shift-JIS encoding, `dc2tk build` writes a plain ASCII fallback there (`gyros`, or `ss` for `ß`) so owned weapons, the invention menu, and the Georama menu display the name cleanly.

#### Limits, custom icons, and custom models

`dc2tk build` refuses any item that exceeds the limits in [game data section 2.8](../internals/data.md#28-limits-icons-and-models): total item count and item numbers, family table sizes, script buffers, name buffer size, name length, and spectrumising above item `425`. Selling a new item requires adding a `PRICE` entry in `data:shops` ([shops](#shops)).

To give a new item its own icon and 3D model:

1. **Custom icon**: Item icons live in `texture:itemicon` and `texture:wepicon`. Draw your icon into an unused cell (listed in [game data section 2.8](../internals/data.md#28-limits-icons-and-models)). Because all 192 cells on a sheet share one 256-color palette, keep your PNG paletted with the sheet's original palette so only your new cell changes.
2. **Custom 3D model**: Add a `"file:item/NAME.chr"` entry under `"add"` in `mod.json` with `"like"` naming an existing item model (such as `"model:takaraky"`). `dc2tk build` writes `NAME.img`, `NAME.mds`, and `info.cfg` matching the game's item packs. Do not add animations to an item model because item menus play no motion.

The [spinning-cult example](../scripting/examples/spinning-cult/README.md) creates both a custom icon and a custom model for item `429` (built and verified by re-export; not yet tested in-game).

## Shops

`data:shops` contains three sections:

- **`prices`** (keyed by `item`): `buy` and `sell` prices.
- **`shops`** (keyed by `shop`): `items` sold in the shop (`currency` and `names` are read-only labels).
- **`donny`** (keyed by `row`): `item` (`1` to `511`; `0` or less terminates the list early) and `level` (Donny's invention level must be above this value).

### Chapter values and adding shops

- **Per-chapter values**: A price or shop inventory that is identical across all 8 chapters is written as a single value; otherwise it is a list of 8 values for chapters 1 to 8. Your mod can provide either form.
- **Multi-file sync**: Edits are written into both sets of shop files (`shop` and `shop_usa`), so every language sees the changes. Every item placed in a shop in a given chapter must have a price in that chapter (either on the disc or added by a mod).
- **Adding prices and shops**:
  - Providing both `buy` and `sell` for an item absent from the price lists adds a new `PRICE` line to all 16 shop files.
  - Providing a `shop` number absent from the shop lists (up to `32767`, with its `items`) adds a new `SHOP` line. A script can open the new shop by calling `open_menu(MENU_OPEN_SHOP, number)`, and its display name is message `@<number>` in `text:<language>/shopname`.
  - A shop can list at most `63` items. Shop files load into the menu's local stack (exact free room is unknown, but a few extra lines fit easily).

## Monsters and floors

- **`data:monsters`**: Keyed by `id`, with `name` as a read-only label (`mosdata<language>.cfg` stores monster names) and `elements` as an object of the 8 elemental resistances. `dc2tk build` patches only the changed bytes in `SCES_511.90`, keeping the executable's size and layout unchanged.
- **`data:floors`**: Each floor's spawn record is keyed by `dungeon` and `floor`, with `monsters` as a list of `{"monster", "talk"}` objects (at most `32`). A modified list fills the floor's `FL` lines in order, with the last `FL` line taking any remaining entries; leftover `FL` lines become empty `FL;` statements.

## Inventions and ideas

`data:inventions` contains three sections:

- **`recipes`** (keyed by `recipe`, the 0-based index in the file; multiple recipes can craft the same item): Contains `item`, `ideas` (`[3]`), `materials` (1 to 4 `{"item", "count"}` objects, where `count` is a byte), `unknown_10`, `scale`, and `position` (`[3]`). Edits update all 19 copies of `inv6.lst` and rebuild their enclosing archives. A new recipe (`recipe` set to the next number, with `like` naming a recipe to copy) is appended after the last `DATASET` in every copy and increments `DATATABLESET` (at most `234` recipes).
- **`scoops`** (keyed by `scoop` ID): Contains `flag` (the menu flag checked by `KnowScoop`) and `info` (its save slot in `CScoopDataManager::info`).
- **`ideas`** (keyed by `idea` ID): Lists records such as `{"idea": 1, "name": "Trashcan", "sort": 1911}` (the English name and notebook sort key):
  - Rename an idea with `name` (English) or `names` (`{"fr": ..., "ja": ...}` for `ja`, `en`, `fr`, `de`, `it`, `es`), and set sort keys with `sort` (English) and `sorts` (by language).
  - **Adding a new idea**: Use an `idea` ID not on the disc (`1` to `999`) with `name` (default for all languages) and optional `names` overrides per language (no `like` field is needed):
    ```json
    {"idea": 251, "name": "Endless Turning", "names": {"fr": "Rotation sans fin"}}
    ```
    `dc2tk build` appends `PIC_NAME` after the last idea in each copy of `neta2.lst` and increments `PIC_INFO`.
  - **Automatic alphabetical sort keys**: When you add or rename an idea without specifying a sort key for a language, `dc2tk build` inserts it into alphabetical order automatically: it takes the sort key of the first idea of the same starting letter that sorts after it (using case-folded comparison) and shifts that idea and all subsequent ideas of that letter up by 1 (for example, in English, `"Endless Turning"` takes `"Energy Pipe"`'s key `411`, which becomes `412`). In Japanese, a name starting with a kanji is placed with the Latin names (`50`) unless `sorts` gives an explicit key.
  - **Refused (build errors)**:
    - Idea IDs `0`, `1000` and above, `30000`, and `30001`
    - Exceeding `0x247` string units in any copy (the French copy has 17 units free, enough for 11 new ideas with names under 16 bytes)
    - More than `388` ideas and scoops combined (string units run out first)
    - A name longer than 24 bytes, or longer than the longest existing idea name in that language (`MenuInventNetaMemoDraw`), or empty, or containing a control character, backslash, or character that the language's font lacks
    - A file exceeding the `0x5000`-byte buffer (minus its 64-byte alignment)
    - One value set differently by two mods
  - **Using a new idea**: Photographing a new idea in-game requires an `invent` box point on a map with its ID (`FUNC_INVENT_DATA`, added via the Blender add-on's invent box), and inventing with it requires a recipe in `data:inventions` that references the idea ID.

## Georama

`data:georama` contains:

- **`parts`** (keyed by `town`, such as `"g01"`, and `part` ID, with the English `name` as a label): Contains `model` (`PARTS_NAME`), `attribute`, `materials` (`{"item", "count"}`, one per `PARTS_MATERIAL` line—a part keeps at most the lines it already has, and emptied lines read as none), `culture_points` (`[2]`), `weight`, `geostone`, `max`, `paint_colours`, `paint_used`, `type`, `place_eps`, `map`, and `polyn`. Values are read from the English file (where the Japanese file differs, the English values are shown) and written into all 7 languages' `.gpi` files. The part comment is text and is omitted from the table.
- **`fixed_parts` and `starting_parts`** (keyed by `town` and `number`, the 0-based index in the list): Contains `part`, `position` (`[3]`), and `angle`.
- **Adding new Georama parts**:
  - Add a new part definition with `{"town": "g01", "part": ID, "like": <ID of a part in that town>, "name": ..., "comment": ...}` (`name` and `comment` can each be a single string or a per-language object). `dc2tk build` copies the template part's block into all 7 languages' `.gpi` files after the town's last part and increments `EDIT_PARTS_NUM`, within the limits of [game data section 6](../internals/data.md#6-georama): at most `384` parts per town, IDs `0` to `127`, names up to `47` bytes, and `g01`'s files within `0x77D0` bytes. A new part that shares another town's part `ID` shares its inventory stock; its `model` must exist in the town's map.
  - Add new fixed and starting parts (`number` set to the next index, `like` naming one in the town) to append them after the town's last line and increment `FIX_EPARTS_START` or `INIT_EPARTS_START` (at most `256` placed parts per town).
- **Rivers and water** (no new records can be added):
  - `rivers` keyed by `town` and `slot` (`part`, `model`)
  - `water` keyed by `town` (`model`, `mask_model`)
  - `water_surfaces` keyed by `town` and `surface` (`grid` `[2]`, `corners` `[6]`, `position` `[3]`, `params` `[4]`)
  - `water_draws` keyed by `town` and `number` (`surface`, `follow`, `position` `[3]`)

### Requests

In `data:requests`:

- **`conditions`** (keyed by `town` and `condition`, with the English `text` as a label): Contains `geostone`.
- **`requests`** (keyed by `town` and `request`, with the English `title` as a label): Contains `unread`, `geostone`, `conditions` (`1` to `7` entries), `on_parts`, `off_parts`, and `percent` (for requests that have them).

Edits are written to all 7 languages' `geo<N>.cfg` files. A new request (`request` set to the next number for its town, `like` naming an existing request in the town, and `title` as a single string or per-language object) copies that request's block after the town's last request (at most `16` per town, keeping total strings within `buff_1271`). A request can only use conditions that the town's C++ code sets ([game data section 6.1](../internals/data.md#61-requests-geolanguagecfg)).

## Fishing

`data:fishing` contains:

- **`spots`** (keyed by `spot`, the 0-based index in `fishing.cfg`): Contains `map`, `exclusive`, `area`, `area_values` (`[5]`), and `fish` (`{"fish", "rate", "wait_bias"}` per `FISH` line). Because the engine's `FISH` parser reads arguments without checking the count, a spot's `fish` list can grow up to `8` entries (adding new `FISH` lines after its last) but **cannot shrink** below its original count (an emptied `FISH` line would read uninitialized stack data). A new spot (`spot` set to the next index, `like` naming an existing spot) copies that spot's block after the last spot and increments `FISH_MAP_NUM`.
- **`race_fish`** (keyed by `class` and `number`, with the English `name` as a label): Contains `item`, `colour`, `power`, `speed` (`[3]`), `stamina`, and `size`, written into all 7 languages' `rfd` files.
- **`prize_groups`** (keyed by `contest`, `"race"` or `"tournament"`, and `group`): Contains `count` (fixed) and `values`.
- **`prizes`** (keyed by `contest`, `group`, and `prize`): Contains `unread` and `places` (three `{"item", "count"}` objects).
- **`species`** (keyed by `fish`, the `FishParam` row in the executable): Contains `item`, `base_size`, `min_size`, `max_size`, `unknown_18` (unused by code; mostly above `max_size`), `weight_rate`, `point_rate`, `pull_rate`, `bait_liking` (`[18]`), and `time_liking` (`[4]`).
- **`race_kinds`** (keyed by `row` in the executable): Contains `item` (fixed), `power`, `stamina`, `speed` (`[3]`, in percent), and `affinity`.

## Floor medals

In `data:medals`, `floors` is keyed by `dungeon` and `floor`, with `geostone`, `fast_time`, `fishing`, `fishing_record`, `seal`, `spheda`, `spheda_prizes` (three `{"item", "count"}` objects), `practice_type`, and `practice_param`.

## Characters and other executable tables

All of these tables are patched in place in `SCES_511.90` like `data:monsters`, and new rows cannot be added ([game data section 9](../internals/data.md#9-characters-and-other-executable-tables)):

- **`data:characters`**:
  - `characters` (keyed by `character`, `"max"` or `"monica"`): `starting_hp`, `starting_equipment` (`[5]`), and `starting_equipment_japanese` (`[5]`)
  - `weapon_level_up` (single record): `whp_gain` (`[10]`)
  - `monster_forms` (keyed by `monster`): `attack`, `defence`
  - `badges` (keyed by `row`): `badge`, `forms` (`[4]`)
- **`data:gifts`**: `gifts` keyed by `row`: `gift_type` (`0` or higher; `-1` ends the table there), `item`, `unread`.
- **`data:spheda`**: `clubs` keyed by `club`: `power`, `unknown_4`, `unknown_8`.
- **`data:breeding`**: `breeding` keyed by `row`: `parents` (`[2]`) and `child`, stored as fish indices (`item - 0x136`).

## Townspeople

In `data:townspeople`, `townspeople` is keyed by `npc`, with the English `name` as a label, and `model`, `message_kind`, `abilities`, `points`, `ability_costs` (`[4]`), and `debug`. Edits are written into all 7 languages' `npc<N>.cfg` files. A new townsperson (`npc` set to an unused ID, `like` naming an existing NPC to copy, and `name` as a single string or per-language object) appends a copy of that line after the last entry in every file (at most `180` townspeople).

## An example: the spinning cult

The [spinning-cult example](../scripting/examples/spinning-cult/README.md) uses two data tables, declared in `mod.json` as `"data:items": "data/items.json"` and `"data:inventions": "data/inventions.json"`:

- **`data/items.json`**: Adds item `429` with `"like": 297` (the Treasure Chest Key, a key item), setting `"name": "gŷros"` (using the composed letter `ŷ` that `dc2tk build` draws into the English font), localized `"names"` for the other languages, a `"description"`, a custom icon (`"icon_sheet": 1`, `"icon": 191`, the unused reserved cell drawn via `texture:itemicon`), a custom model (`"model": "gyros"`, built as `item/gyros.chr` with `"like": "model:takaraky"`), `"stack": 1`, `"max": 1`, and `"attribute"`.
- **`data/inventions.json`**: Adds three ideas (`251` to `253`: `"Turning"`, `"More Turning"`, and `"Endless Turning"`, each with localized `"names"` in French, German, Italian, and Spanish) awarded by the Fish Room's `invent` boxes, plus recipe `136` (`"like": 0`), which combines those three ideas to craft item `429` from one of item `224` (a Scrap of Metal) and displays the model on the invention card at `"scale": 1.5` and `"position": [-10.6, 1.75, 0]`.
