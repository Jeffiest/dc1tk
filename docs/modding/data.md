# Editing game data

Item and weapon stats, shops, monsters, inventions, Georama, fishing, floor medals, townspeople
and the executable's tables are edited as JSON tables a mod gives in its `mod.json`. This page
covers exporting a table, the JSON form, keeping only what changes, adding records and the limits
`build` keeps to. What each field means, where the game keeps it and how many of each the engine
holds is in [game data](../game/data.md); the dungeon floors' layouts, parts and chests, which are
tables too, are in [dungeons](dungeons.md); mod.json itself is in [mods](mods.md).

## Exporting a table

`mod asset ISO NAME data:items data:shops data:monsters` (ISO the disc image or an extracted disc)
writes `data/items.json` (every item's common data, with its weapon, usable effect, attachment,
ridepod part or fish stats nested: attack, WHP, elements and limits, synthesis points, build-ups,
...), `data/shops.json` (each item's buy and sell price and each shop's goods, one value or a list
of eight for chapters 1 to 8) and `data/monsters.json` (each monster kind's HP, attack, defence,
ABS, money, drops, element and attack-kind rates, model and AI script), with their `"data:..."`
lines (`"data:items": "data/items.json"`), leaving a table the mod has a line for already as it
is. Items also carry what each one adds when spectrumised (`spectrumise`), shops Donny's goods
(`donny`). The rest: `data:inventions` (recipes: ideas, materials; scoops; ideas: names and
notebook order, new ideas), `data:georama` (each town's parts: materials, culture points, counts;
fixed and starting parts), `data:medals` (each floor's fastest time, fishing record, seal, Spheda
prizes, practice medal), `data:floors` (each floor's monsters), `data:fishing` (spots, race
opponents, prizes, fish species and race kinds), `data:layouts` (the room layouts dungeon floors
are built from: each room's rows of parts, as text, how often it is chosen; rooms may be added,
see [dungeons](dungeons.md)), `data:parts` (the parts those layouts name, and the mini maps'
tiles: a part renamed in place is a new kind of part), `data:chests` (what each dungeon floor's
chests hold: groups of items by rank, and each floor's groups) and `data:characters` (starting HP
and equipment, WHP gained at a weapon level-up, Monica's monster forms, badge growth),
`data:requests` (Georama requests: their conditions, map parts shown, percentages),
`data:townspeople` (names' models, abilities, costs), `data:gifts` (gifts that win a badge),
`data:spheda` (club power) and `data:breeding` (aquarium breeding pairs); `data:georama` also has
each town's rivers and water. An unknown table name is an error listing the tables.

## The tables

Each table, what it holds and where the game keeps it ([game data](../game/data.md) section):

- **Items** (`data:items`, [section 2](../game/data.md#2-items-menucfg7)): the seven `menu/cfg7`
  tables, every item number with its weapon, effect, attachment, ridepod and fish data nested.
- **Shops** (`data:shops`, [section 3](../game/data.md#3-shops-menushopusachaptercfg)): prices
  and every shop's goods, by chapter, in the 16 shop lists.
- **Monsters** (`data:monsters`,
  [section 4](../game/data.md#4-monsters-base_monster_define-in-sces_51190)): the 343 monster
  kinds' stats, patched in place into `SCES_511.90`.
- **Spectrumising** (in `data:items`, each item's `spectrumise`: `slot`, `value`) and **Donny's
  shop** (in `data:shops`, `donny`: `row`, `item`, `level`), executable tables
  ([section 9](../game/data.md#9-characters-and-other-executable-tables)).
- **Inventions** (`data:inventions`, [section 5](../game/data.md#5-inventions-ideas-and-scoops)):
  `recipes` (all 19 copies of `inv6.lst` in step), `scoops` (executable) and `ideas` (names and
  notebook order in the 7 copies of `neta2.lst`, by language).
- **Georama** (`data:georama`, [section 6](../game/data.md#6-georama)): `parts` of every town, in
  every language's `.gpi`; `fixed_parts` and `starting_parts`.
- **Floor medals** (`data:medals`,
  [section 8](../game/data.md#8-floor-medals-menudngmapdflrdungeoncfg)) and **monster placement**
  (`data:floors`, section 4): `floors` by `dungeon` and `floor`.
- **Fishing** (`data:fishing`, [section 7](../game/data.md#7-fishing)): `spots`, `race_fish`,
  `prize_groups`, `prizes`, and from the executable `species` and `race_kinds`.
- **Dungeon floor layouts** (`data:layouts`): the room layouts random and preset floors are built
  from, `dungeon/cfg_file/d0*.cfg` and `mstage*.cfg`; rooms are written whole and may be added.
  Documented in [dungeons](dungeons.md). The parts they name and the mini map's tiles
  (`data:parts`, executable tables) and what the floors' chests hold (`data:chests`,
  `tbox_d0*.cfg`) are there too.
- **Characters** (`data:characters`, section 9): `characters` (starting HP and equipment),
  `weapon_level_up`, `monster_forms`, `badges`, all executable tables.
- **Georama requests** (`data:requests`, [section 6.1](../game/data.md#61-requests-geolanguagecfg)):
  each town's `conditions` and `requests` in every language's `geo<N>.cfg`.
- **Rivers and water** (in `data:georama`,
  [section 6.2](../game/data.md#62-rivers-and-water-gnncfg)): `rivers`, `water`, `water_surfaces`
  and `water_draws` of each town's `gNN.cfg`.
- **Townspeople** (`data:townspeople`,
  [section 10](../game/data.md#10-townspeople-npclanguagecfg)): every language's `npc<N>.cfg`.
- **Gifts, Spheda clubs, breeding** (`data:gifts`, `data:spheda`, `data:breeding`, section 9):
  `gift_item_tbl`, `GolfClubDef` and `aquafish_mixTable`, patched in place.

Not in any table: the guard table (`grddat.cfg`, which no code reads), the item and monster names
of each language (`comdatmes`, `mosdata`: text, except an item's `name`, below), the scoops'
names and texts (`neta2.lst`, `scoop.cfg`: text), whether a Georama condition is met (code), and
where townspeople stand in towns (`npc_place*.cfg`).

## Keeping only what changes

A table holds one record per item, price, shop or monster, named by its number; a mod keeps only
the records and fields it changes (an unedited full table changes nothing), each record with the
keys that name it (`item`, `shop`, `id`, `recipe`, `dungeon` and `floor`, ...):
`{"items": [{"item": 1, "weapon": {"attack": 50}}]}`. `build` writes only the values that change:
items into `menu/cfg7`, shops into all sixteen shop lists, a value kept in several files (the 19
copies of `inv6.lst`, each language's Georama parts and race fish) into every one, and the
executable tables into the boot executable `SCES_511.90`, patched in place, the file's size
unchanged; it patches the executable once with every table's changes. Lists keep within what the
game reads (a recipe's 1 to 4 materials, a floor's 32 monsters).

Several mods' tables merge; two mods giving one value different numbers are refused, naming both
(in the executable, two mods' patches giving one value different bytes). Values the engine's field
cannot hold, fields a record does not have and the numbers that tie records together (`list`, a
ridepod part's `kind`) are refused when the mod is read. Names are labels (they are text), except
an item's (`name`, `description`, [below](#new-items-names-and-descriptions)).

## The JSON

Each section of a table is a list of records named by their keys (`{"dungeon": 0, "floor": 3}`,
`{"town": "g01", "part": 1}`), with labels (names, which are text; given back unchanged or left
out) and fields; `a.b` fields are nested objects. A field is one argument or several, a list
inside a statement (a recipe's materials, a length the mod may change within the engine's
limits), a list over several statements (a floor's `FL` lines, a part's `PARTS_MATERIAL` lines:
filled in order, emptied when the list is shorter, never added to), or bytes of the executable. A
field kept in several files (the copies of `inv6.lst`, each language's `.gpi` and `rfd`) is read
from the first (the English file where languages differ) and written to every copy.

`mod asset` lays each table out for a person to edit. A table a mod gives back is read member by
member, and a member of the wrong shape is an error naming it (`dungeon 0: "groups" is a list, not
an object`, `"floors" is a list of records`), never read as nothing; so is a section, field or key
the table does not have. A float argument is written in the JSON in its shortest form that reads
back as the same f32, with `.0` when it is whole (`320.0`), and a float of the executable in its
shortest form (`320` when whole); `data:items` alone keeps the form its files have always had,
the f32 widened to an f64 with every digit (`0.9900000095367432`). Values are compared as the
engine holds them: two whole numbers exactly, a float as its f32, so `0.99` there changes nothing.
A float argument a mod gives as a JSON integer (`320`, not `320.0`) is written as that integer
(the engine's float read takes an int); otherwise as the f32's shortest text, `.0` added when it
has no point.

## Adding records

A record whose keys the table lacks is a new record; it names the record it copies with `like`
(that record's key, or an object of its keys) and gives only what differs: `{"item": 429, "like":
1, "name": "Mega Wrench", "weapon": {"attack": 250}}`. Build writes a copy of the like record's
statements, with the new keys, labels and fields, after the last record of its kind (in its town
or file) in each file and each copy, and raises the count the engine allocates by (`WEPNUM`,
`DATATABLESET`, `EDIT_PARTS_NUM`, `FISH_MAP_NUM`, ...). Several mods' new records are counted
together; one record added two ways is refused, naming both mods. A key that is the record's place
(`recipe`, `spot`, `request`, a placed part's `number`) is the next one.

Records that can be added, and the files build writes them into; build refuses past the engine's
limits ([game data](../game/data.md#limits-on-counts)):

| records | where |
|---|---|
| items (`data:items`) | `comdat.cfg`, the family's table, every `comdatmes`, `system_N.mes`, `allmenu.mes` |
| prices and shops (`data:shops`) | all 16 shop lists |
| recipes (`data:inventions`) | all 19 copies of `inv6.lst` |
| ideas (`data:inventions`) | all 7 `neta2.lst` |
| Georama parts (`data:georama`) | the town's 7 `.gpi` |
| fixed and starting parts (`data:georama`) | the town's `gNN.cfg` |
| fishing spots, a spot's fish (`data:fishing`) | `fishing.cfg` (256 spots, the tool's own limit: no array bounds them) |
| requests (`data:requests`) | all 7 `geo<N>.cfg` |
| townspeople (`data:townspeople`) | all 7 `npc<N>.cfg` |

A floor's monsters (`data:floors`) already go up to 32: the `FL` lines are filled, the last taking
the rest. Every statement keeps within the 64 arguments the script reader takes. Not added:
monster kinds (the executable's table has no room), executable table rows (gifts, clubs,
breeding, Donny's goods: fixed sizes in `.data`), floor medals, race fish, prizes and conditions
(whether a condition is met is code); a new item takes an existing icon and model, or one the mod
draws or builds ([below](#new-items-names-and-descriptions)).

## Items

In `data:items` each `COM` statement is a record, in file order, named by `item`, with `name` (the
English name from `comdatmes1.cfg`, a label, which renames the item when changed) and the family
data under `weapon`, `effect`, `attachment`, `ridepod` or `fish`. Each item's `spectrumise` has
`slot` and `value` (items 1 to 425).

### New items, names and descriptions

A new item is `like` an item of the same family (`type` may change within it) and gives a `name`
(every language) or `names` (`{"en": ..., "fr": ...}`, all seven); `description` (every language)
and `descriptions` optional, else the like item's in each language. Build gives it its `list`
(the next entry of its family's table, whose count it raises), `message` its number unless given,
and writes: the `COM` line after the last; a copy of the like item's `WEP`..`WEP_BUILD` block,
`AT_ST`..`AT_ST_SP` block, or `ITEM`, `RB_PARTS`, `FISH`, `GRD` line with the fields given;
`MES_SYS` in all seven `comdatmes`; message 10000 + `message` into each language's
`meswin/system_N.mes` and into every `allmenu.mes` of its menu packs that has the item
descriptions (`system_N.mes` stays within its 0xD000 bytes). For an item of the game's own,
`name` changes the English name and `names`/`descriptions` any language's, the same places.

Names and descriptions are message text ([the text form](text.md#the-text-form)) in each language's font.
In English they may use letters past the font's table, any the European font has or `build`
composes of a letter and a mark (`gŷros`), at most 32 together with the mods' `text:en/...`
lines: `build` draws them into the English font
([letters the English font lacks](text.md#letters-the-english-font-lacks)). The `comdatmes1.cfg` name, which holds Shift-JIS, spells such a letter plainly
(`gyros`, `ß` as `ss`), so an owned weapon's name and the inventions and Georama menus show it so.

Build refuses an item past the limits of
[game data section 2.8](../game/data.md#28-limits-icons-and-models): the item count and numbers,
the family tables, each script's buffer, the name buffer, a name's length, and spectrumising
above item 425. The model is a file the disc has, or one the mod builds; selling the item needs a
`PRICE` ([shops](#shops)).

An icon and a model of the item's own: the sheets are `texture:itemicon` and `texture:wepicon`;
draw into a cell no item points at (game data section 2.8 lists them). The sheet is 256 colours
for all 192 cells, so a mod drawing into a cell keeps the PNG paletted with the sheet's palette
and only that cell changes. A pack of the mod's own is a `file:item/NAME.chr` line `like` an
item's model (`model:takaraky`): build writes `NAME.img`, `NAME.mds` and `info.cfg` as the game's
item packs have them. Give it no animation: the menus that show an item's model set no motion.
The [spinning-cult example](../coding/examples/spinning-cult/README.md) does both for its item
429 (built and read back; not yet seen in the game).

## Shops

In `data:shops`: `prices` by `item` (`buy`, `sell`), `shops` by `shop` (`items`; `currency` and
`names` are labels). A value the same in all eight chapters is one value, otherwise a list of
eight; a mod may give either. An edit goes into both sets of files, so every language sees it; an
item put in a shop must have a price in that chapter (or one any of the mods adds). A price for
an item the lists lack (`buy` and `sell` both given) is a new `PRICE` line in all 16 files, and a
shop they lack (a number up to 32767, with its `items`) a new `SHOP` line; a script opens it with
`open_menu(MENU_OPEN_SHOP, number)`, and its name is message `number` of
`text:<language>/shopname`. A shop lists at most 63 goods. The shop files load into the menu's
local stack, whose free space is not known; a few lines more are well within it.

Donny's shop is `donny` in `data:shops`: `row`, `item` (1 to 511; 0 or less would end the list)
and `level` (Donny's invention level must be above it).

## Monsters and floors

In `data:monsters` `elements` is an object of the eight; a monster's `id` is its key and its
`name` a label (`mosdata<language>.cfg` names monsters). Build writes only the bytes of the fields
that change, in the built image's `SCES_511.90`, whose size and layout stay as they were.

In `data:floors` each floor's last block is a record named by `dungeon` and `floor`, with
`monsters`, a list of `{"monster", "talk"}` (at most 32). A changed list fills the floor's `FL`
lines in order, the last taking the rest; lines left over become `FL;`, which adds nothing.

## Inventions and ideas

In `data:inventions`: `recipes`, named by `recipe` (the place in the file, from 0; two recipes
may make one item), with `item`, `ideas` [3], `materials` (1 to 4 `{"item", "count"}`; the count
is a byte), `unknown_10`, `scale` and `position` [3]; an edit goes into all 19 copies, the packs
rebuilt around them. A new recipe (`recipe` the next number, `like` one to copy) goes after the
last `DATASET` of every copy, `DATATABLESET` raised; at most 234. `scoops`, named by `scoop` (the
id), with `flag` (the menu flag that makes the scoop known, `KnowScoop`) and `info` (its slot in
`CScoopDataManager::info`, the saved state).

`ideas` lists `{"idea": 1, "name": "Trashcan", "sort": 1911}` (the English name and key). A mod
renames an idea with `name` (English) or `names` (`{"fr": ..., "ja": ...}`, the languages ja, en,
fr, de, it, es), and sets keys with `sort` (English) and `sorts` (by language). An id the game
lacks (1 to 999) is a new idea, `name` in every language and `names` over it (no `like`: there is
nothing to copy): `{"idea": 251, "name": "Endless Turning", "names": {"fr": "Rotation sans
fin"}}`. Build writes its `PIC_NAME` after the last idea of each copy and raises `PIC_INFO`. A
name given without its language's key is put in its place: it takes the key of the first name of
its letter that sorts after it (the folded text compared), those from there on move up one (in
English "Endless Turning" takes Energy Pipe's 411, which becomes 412); a rename moves the idea so
too. A Japanese name opening with a kanji goes with the Latin ones (50) unless `sorts` gives its
key. Refused: ids 0, from 1000, 30000 and 30001; a copy past 0x247 units (the French copy has 17
left: 11 more ideas of names under 16 bytes); more than 388 ideas and scoops (the units run out
first); a name longer than 24 bytes or than the longest idea name the game has in that language
(`MenuInventNetaMemoDraw`), empty, with a control character or backslash, or with characters the
language's text lacks; a file past the 0x5000-byte buffer (less its 64-byte alignment); one value
given two ways by two mods. Photographing a new idea needs a map invent point giving its id
(`FUNC_INVENT_DATA`, the Blender add-on's invent box), and inventing with it a recipe naming it.

## Georama

In `data:georama`: `parts`, named by `town` (`"g01"`) and `part` (its `ID`), labelled with the
English `name`, with `model` (`PARTS_NAME`), `attribute`, `materials` (`{"item", "count"}`, one
per `PARTS_MATERIAL` line: a part keeps at most the lines it has, emptied lines read as none),
`culture_points` [2], `weight`, `geostone`, `max`, `paint_colours`, `paint_used`, `type`,
`place_eps`, `map` and `polyn`; each is read from the English file and written into all seven
languages' (where the Japanese file differs, the English values are shown). The comment is text
and left out. `fixed_parts` and `starting_parts`, named by `town` and `number` (the place in the
list), with `part`, `position` [3] and `angle`.

New parts: `{"town": "g01", "part": ID, "like": ID of a part of the town, "name": ...,
"comment": ...}` (each one string or by language) copies the part's block into all seven
languages' files, with the ID, name and comment, after the town's last part, `EDIT_PARTS_NUM`
raised, within the limits of [game data section 6](../game/data.md#6-georama): 384 parts a town,
IDs 0 to 127, names of 47 bytes, g01's files within 0x77D0 bytes. A new part of another town's ID
shares its stock; its `model` must be a map part of the town's map. New fixed and starting parts
(`number` the next, `like` one of the town's) go after the town's last line, `FIX_EPARTS_START`
or `INIT_EPARTS_START` raised; 256 placed a town.

Rivers and water: `rivers` by `town` and `slot` (`part`, `model`), `water` by `town` (`model`,
`mask_model`), `water_surfaces` by `town` and `surface` (`grid` [2], `corners` [6], `position`
[3], `params` [4]) and `water_draws` by `town` and `number` (`surface`, `follow`, `position` [3]);
none is added.

### Requests

In `data:requests`: `conditions` by `town` and `condition`, the English `text` a label, with
`geostone`; `requests` by `town` and `request`, the English `title` a label, with `unread`,
`geostone`, `conditions` (1 to 7), `on_parts`, `off_parts` and `percent` (the ones the request
has). An edit goes into all seven languages' files. A new request (`request` the next of its
town, `like` one of the town's, `title` one string or by language) is a copy of that request's
block, after the town's last; at most 16 a town, the strings within `buff_1271`. A request may
need only conditions the town's code sets
([game data section 6.1](../game/data.md#61-requests-geolanguagecfg)).

## Fishing

In `data:fishing`: `spots` (named by `spot`, the place in the file: `map`, `exclusive`, `area`,
`area_values` [5], `fish`: `{"fish", "rate", "wait_bias"}` per `FISH` line; `FISH` reads its
arguments unchecked, so a spot keeps at least its number of fish, at most 8), `race_fish` (by
`class` and `number`, the English `name` a label: `item`, `colour`, `power`, `speed` [3],
`stamina`, `size`; written into all seven languages' `rfd`), `prize_groups` (by `contest`,
`"race"` or `"tournament"`, and `group`: `count`, fixed, and `values`), `prizes` (by `contest`,
`group` and `prize`: `unread` and `places`, three `{"item", "count"}`), `species` (by `fish`, the
`FishParam` row: `item`, `base_size`, `min_size`, `max_size`, `unknown_18` (nothing reads it;
mostly above `max_size`), `weight_rate`, `point_rate`, `pull_rate`, `bait_liking` [18],
`time_liking` [4]) and `race_kinds` (by `row`: `item`, fixed, `power`, `stamina`, `speed` [3] in
percent, `affinity`).

A spot's `fish` may grow to 8 (new `FISH` lines after its last) but not shrink (an emptied `FISH`
would read whatever the stack held). A new spot (`spot` the next, `like` a spot) copies that
spot's block after the last, `FISH_MAP_NUM` raised.

## Floor medals

In `data:medals`: `floors`, named by `dungeon` and `floor`, with `geostone`, `fast_time`,
`fishing`, `fishing_record`, `seal`, `spheda`, `spheda_prizes` (three `{"item", "count"}`),
`practice_type` and `practice_param`.

## Characters and other executable tables

All are patched in place like `data:monsters`, and rows are not added
([game data section 9](../game/data.md#9-characters-and-other-executable-tables)).
`data:characters` has `characters` (by `character`, `"max"` or `"monica"`: `starting_hp`,
`starting_equipment` [5] and `starting_equipment_japanese` [5]), `weapon_level_up` (one record:
`whp_gain` [10]), `monster_forms` (by `monster`: `attack`, `defence`) and `badges` (by `row`:
`badge`, `forms` [4]).

- **`data:gifts`**: `gifts` by `row`: `gift_type` (0 or more: -1 would end the table there),
  `item`, `unread`.
- **`data:spheda`**: `clubs` by `club`: `power`, `unknown_4`, `unknown_8`.
- **`data:breeding`**: `breeding` by `row`: `parents` [2], `child`, as the table has them (item -
  0x136).

## Townspeople

In `data:townspeople`: `townspeople` by `npc`, the English `name` a label, with `model`,
`message_kind`, `abilities`, `points`, `ability_costs` [4] and `debug`; an edit goes into all
seven languages. A new one (`npc` a number the files lack, `like` one to copy, `name` one string
or by language) is a copy of that line after the last in every file; 180 at most.

## An example: the spinning cult

The [spinning-cult example](../coding/examples/spinning-cult/README.md) has two tables, given in
its mod.json as `"data:items": "data/items.json"` and `"data:inventions": "data/inventions.json"`:

- `data/items.json` adds one item, 429, `like` 297 (the Treasure Chest Key, a key item): its
  `name` `gŷros` (a letter `build` composes and draws into the English font) and `names` for the
  other languages, a `description`, its own icon (`icon_sheet` 1, `icon` 191, the "reserved" cell
  the mod draws into through `texture:itemicon`) and model (`"model": "gyros"`, a pack the mod
  builds as `item/gyros.chr` like `model:takaraky`), `stack` and `max` 1 and its `attribute`.
- `data/inventions.json` adds three ideas, 251 to 253 ("Turning", "More Turning" and "Endless
  Turning", each with `names` in French, German, Italian and Spanish), which the Fish Room's
  invent boxes give, and recipe 136, `like` recipe 0: the three ideas make item 429 from one of
  item 224 (a Scrap of Metal), its card showing the model at `scale` 1.5 and `position` (-10.6, 1.75, 0).
