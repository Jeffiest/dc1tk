# Dark Chronicle game data: items, shops, monsters and the rest

Where the numbers that make up the game live: item and weapon stats, shops,
monster stats, inventions, Georama, fishing, floor medals and character
stats. Engine source: the ChronicleTwo decompilation's `ps2/` (paths below are
relative to it; `re/docs/<unit>/notes.md` has the reverse engineering of
each unit). Corpus: the extracted PAL disc. Counts are over the whole disc.
Field layouts are given here; the game's values are not (export them from your
own disc, below).

Most of the data is text cfg scripts in `DATA.DAT` (`TAG arg, arg, ...;`, the
format `CScriptInterpreter` reads, `scriptinterpreter.cpp`); some is compiled
into the boot executable `SCES_511.90`. Its one `PT_LOAD` maps file offset
0x80 to address 0x100000, so a `.data` address `A` is at file offset
`A - 0xFFF80` (for addresses below 0x37CD80; above that is `.bss`, filled at
run time). Addresses below are the symbols of `config/pal/main.symbols.txt`.

## Status

**Editable through `mod.json`** (`c2 mod asset ISO NAME data:items`, then
`"data:items": "data/items.json"`; see [usage](../usage.md#mods)):

- **Items** (`data:items`, section 2): the seven `menu/cfg7` tables, every
  item number with its weapon, effect, attachment, ridepod and fish data
  nested. `c2-assets`' `item_data.rs`.
- **Shops** (`data:shops`, section 3): prices and every shop's goods, by
  chapter, in the 16 shop lists. `shop_data.rs`.
- **Monsters** (`data:monsters`, section 4): the 343 monster kinds' stats,
  patched in place into `SCES_511.90`. `monster_data.rs`.
- **Spectrumising** (in `data:items`, each item's `spectrumise`: `slot`,
  `value`) and **Donny's shop** (in `data:shops`, `donny`: `row`, `item`,
  `level`), executable tables (section 9). `exe_tables.rs`.
- **Inventions** (`data:inventions`, section 5): `recipes` (all 19 copies of
  `inv6.lst` in step) and `scoops` (executable). `invention_data.rs`.
- **Georama** (`data:georama`, section 6): `parts` of every town, in every
  language's `.gpi`; `fixed_parts` and `starting_parts`. `georama_data.rs`.
- **Floor medals** (`data:medals`, section 8) and **monster placement**
  (`data:floors`, section 4): `floors` by `dungeon` and `floor`.
  `dungeon_data.rs`.
- **Fishing** (`data:fishing`, section 7): `spots`, `race_fish`,
  `prize_groups`, `prizes`, and from the executable `species` and
  `race_kinds`. `fishing_data.rs`.
- **Characters** (`data:characters`, section 9): `characters` (starting HP
  and equipment), `weapon_level_up`, `monster_forms`, `badges`, all executable
  tables. `exe_tables.rs`.

A table holds one record per item, price, shop or monster, named by its
number; a mod keeps only the records and fields it changes (an unedited full
table changes nothing). Several mods' tables merge; two mods giving one value
different numbers are refused, naming both. Values the engine's field cannot
hold, fields a record does not have, item names (text, not data) and the
numbers that tie records together (`list`, a ridepod part's `kind`) are
refused when the mod is read.

The kinds after the first three share `c2-assets`' `data_table.rs`: each
section is a list of records named by their keys (`{"dungeon": 0, "floor":
3}`, `{"town": "g01", "part": 1}`), with labels (names, which are text; given
back unchanged or left out) and fields; `a.b` fields are nested objects. A
field is one argument or several, a list inside a statement (a recipe's
materials, a length the mod may change within the engine's limits), a list
over several statements (a floor's `FL` lines, a part's `PARTS_MATERIAL`
lines: filled in order, emptied when the list is shorter, never added to), or
bytes of the executable. A field kept in several files (the copies of
`inv6.lst`, each language's `.gpi` and `rfd`) is read from the first (the
English file where languages differ) and written to every copy. `build` patches
the executable once with every table's changes.

**Settled (engine code and whole-corpus checks):** every cfg script the tables
come from is read by `c2-assets`' `cfg_text.rs` (statements with the spaces,
line breaks and comments around every argument) and written back byte for
byte; an edit replaces only the arguments it changes. `cargo test --release
corpus` with `C2S_CORPUS` set checks all three versions of the item tables
(`menu/cfg5`, `cfg6`, `cfg7`: 1,260 records), the 16 shop lists, the monster
table and every kind above (136 recipes in 19 copies, 53 scoops, 218 Georama
parts with 5 fixed and 29 starting parts, 161 floors' medals, 129 floors'
monsters, 22 fishing spots, 216 race fish in 7 languages, 22 prize groups, 18
species and race kinds, the character tables): every field's text or bytes
are the ones its value writes, and each exported table applied back changes
nothing. A build with every table edited reads back as edited from the image
(packs holding `inv6.lst` rebuilt around it).

**Not yet:** tried in the game. Adding records (new items, shops, recipes,
parts, spots; lines a list does not have), and the tables still only located:
Georama requests (`geo*.cfg`, mostly code), townspeople (`npc*.cfg`),
`gift_item_tbl`, `GolfClubDef`, `aquafish_mixTable`, idea names (text).

## 1. Where each kind of data lives

| data | where | format | records | editable |
|---|---|---|---|---|
| item common data (type, stack, limit, icon, model) | `menu/cfg7/comdat.cfg` | cfg `COM` | 423 | `data:items` |
| weapons (WHP, ABS, attack, durability, elements, limits, synthesis, build-up) | `menu/cfg7/wepdat.cfg` | cfg `WEP` blocks | 112 | `data:items` |
| usable item effects | `menu/cfg7/itemdat.cfg` | cfg `ITEM` | 160 | `data:items` |
| attachments (crystals, gems, coins) | `menu/cfg7/atdat.cfg` | cfg `AT_ST` blocks | 36 | `data:items` |
| ridepod parts | `menu/cfg7/robodat.cfg` | cfg `RB_PARTS` | 63 | `data:items` |
| fish for breeding and racing | `menu/cfg7/fishdat.cfg` | cfg `FISH` | 18 | `data:items` |
| guard table (read, never used) | `menu/cfg7/grddat.cfg` | cfg `GRD` | 34 | no |
| item names, per language | `menu/cfg7/comdatmes0..6.cfg` | cfg `MES_SYS` | 423-428 | no (text) |
| spectrumising: what each item adds to an attachment | ELF `etcitem_spectol_table` 0x339820 | s8 {slot, value} × 425 | 425 | `data:items` (`spectrumise`) |
| prices and shop goods, by chapter | `menu/shopusa1..8.cfg`, `menu/shoplst1..8.cfg` | cfg `PRICE`, `SHOP` | 428 prices, 38 shops per file | `data:shops` |
| Donny's shop | ELF `dony_shoplist` 0x358620 | {s16 item, s8 level} × 7 | 7 | `data:shops` (`donny`) |
| monster stats, drops, rates | ELF `base_monster_define` 0x341DA0 | `BASE_MONSTER_TBL` 0xB8 × 344 | 343 | `data:monsters` |
| monster names, per language | `dungeon/cfg_file/mosdata0..6.cfg` | cfg `NAME` | 343 | no (text) |
| which monsters each floor has | `dungeon/cfg_file/mos_place0..6.cfg` | cfg `FLS`/`FL`/`FLE` | 6-31 floors per dungeon | `data:floors` |
| invention recipes | `menu/inv6.lst` and its copies in the menu packs | cfg `DATASET` | 136 | `data:inventions` |
| idea and scoop names | `menu/<lang>/neta2.lst` | cfg `PIC_NAME` | 294 | no (text) |
| scoops | ELF `scoop_table` 0x352DE0 and `menu/<lang>/scoop.pac` `scoop.cfg` | `SCOOP_DATA` 0x14 × 53; cfg `STR` | 53 | `data:inventions` (flag, slot; the text is text) |
| Georama parts (materials, culture points) | `map/g/gNN/gNN<lang>.gpi` | cfg `EDIT_PARTS` blocks | 13-56 per town | `data:georama` |
| Georama fixed and starting parts, rivers | `map/g/gNN/gNN.cfg` | cfg `FIX_EPARTS`, `INIT_EPARTS` | per town | `data:georama` (not the rivers) |
| Georama requests and conditions | `geo0..6.cfg` | cfg `GEO_ANALYZE` | 5 towns, 98 conditions, 49 requests | no (checks are code) |
| fish species (sizes, baits, times) | ELF `FishParam` 0x361A50 | `FISH_PARAM` 0x54 × 19 | 18 | `data:fishing` |
| fishing spots | `sg/fish/fishing.cfg` | cfg `FISH_MAP` blocks | 22 maps, 110 fish | `data:fishing` |
| fish race opponents | `sg/gyo/rfd0..6.cfg` | cfg `NUM`/`DAT` | 4 classes, 216 fish | `data:fishing` |
| fish race and tournament prizes | `gyop.cfg`, `uofp.cfg` | cfg `GRP`/`PRIZE` | 12, 10 | `data:fishing` |
| floor medals (time, fishing, Spheda, practice) | `menu/dngmap/dflr0..6.cfg` | cfg `RF_INFO`, `RF_INFO2` | 9-39 floors per dungeon | `data:medals` |
| Max's and Monica's starting HP | ELF `lifetbl_2854` 0x37C790 | f32 × 2 | 2 | `data:characters` |
| Monica's monster forms' attack and defence | ELF `mos_henge_param` 0x339D70 | 0x1C × 57 | 57 | `data:characters` |
| monster badge evolutions | ELF `monster_progress_tbl` 0x358C90 | s16 [19][5] | 19 | `data:characters` |
| fish race kinds (percent of each race figure, affinity) | ELF `fish_data` 0x3630C0 | `grFISH_DATA` 0x1C × 18 | 18 | `data:fishing` |
| WHP a weapon gains at a level-up | ELF `htbl_1662` 0x33A490 | s8 [10] | 10 | `data:characters` |
| starting equipment, by language | ELF `weptbl_4503` 0x33A510 | s16 [2][10] | 2 × 2 × 5 | `data:characters` |
| townspeople (abilities, costs) | `npc0..6.cfg` | cfg `NPC_INFO` | 177 | no |

`menu/cfg5` and `menu/cfg6` are older versions of the item tables, and
`menu/0/inv*.lst`, `menu/inv4.lst`, `inv5.lst`, `sg/fish/fish.cfg`,
`sg/gyo/racefish*.cfg`, `npc_place.cfg` to `npc_place3.cfg`, `npc.cfg`,
`edit.cfg` and the `.gpi` files without a language digit are leftovers no code
names. Character and weapon growth is code, not data: defence starts at 4 or
8 (`CUserDataManager::Initialize`), a weapon's ABS to level grows by half of
itself per level, its WHP by one of the ten values of `htbl_1662` (0x33A490),
picked at random.

## 2. Items: `menu/cfg7`

`CGameData::LoadData` (`gamedata.cpp`, 0x196980) runs `comdat`, `wepdat`,
`itemdat`, `atdat`, `robodat`, `fishdat` and `grddat` (`menu/cfg7/%s`) over
the tag table `gamedata_tag` (0x339B80, 24 tags). An item number (1 to 511)
finds its `CDataCommon` through `local_itemdatano_converttable`; the record's
`list` is the index of its entry in its family's table. The family is
`ConvertUsedItemType(type)`: 1-4 weapon, 5-10 clothing, 11 and 20 up item
(0x1C gift box, 0x1E fish, 0x23 boiled), 12-15 ridepod part, 16-19 and 0x22
attachment. The tables are fixed arrays: 432 common, 116 weapons, 162 items,
38 attachments, 68 ridepod parts, 20 fish, 40 guards (`local_*`, `.bss`).

In the JSON (`data:items`) each `COM` statement is a record, in file order,
named by `item`, with `name` (the English name from `comdatmes1.cfg`, a
label) and the family data under `weapon`, `effect`, `attachment`, `ridepod`
or `fish`.

### 2.1 `COM item, type, list, active_set, stack, max, icon_sheet, icon, message, "model", attribute;`

`_DATACOM` (0x195BB0) into `CDataCommon` (0x2C):

| field | type | meaning |
|---|---|---|
| `type` | u8 | item type; gives the family (above) and the model folder |
| `list` | s16 | index in the family's table (not editable) |
| `active_set` | u8 | non-zero: can be set as an active item |
| `stack` | s16 | how many one stack holds |
| `max` | u16 | most the player can hold (weapons above 100 read as 144) |
| `icon_sheet` | u8 | the menu icon texture (`GetMenuItemIconTexInfo`) |
| `icon` | s16 | the icon on it |
| `message` | s16 | base of the item's message numbers |
| `model` | char[16] | model file base name (`item/`, `main/chr/` or `dungeon/robo/`) |
| `attribute` | u32 | 0x1 can be thrown away (`IsTrush`), 0x2 can be spectrumised |

### 2.2 Weapons: `WEP`, `WEP_ST`, `WEP_ST_L`, `WEP_ST2`, `WEP_ST2_L`, `WEP_SPE`, `WEP_BUILD`

`WEPNUM n;` sets the count; each block fills the next `CDataWeapon` (0x4C) and
`WEP_BUILD` moves on, so block n is the weapon whose `list` is n.

| statement | fields | type | meaning |
|---|---|---|---|
| `WEP` | `whp`, `abs` | s16 | WHP gauge (capped at 255); ABS to the first level-up |
| `WEP_ST` | `attack`, `durability` | s16 | starting attack and durability (durability slows WHP wear, `calcWeaponParamWhp`) |
| `WEP_ST_L` | `attack_max`, `durability_max` | s16 | their limits |
| `WEP_ST2` | `flame` `chill` `lightning` `cyclone` `smash` `exorcism` `beast` `scale` | s16 | starting attributes (the order the attachment crystals add them) |
| `WEP_ST2_L` | `flame_max` .. `scale_max` | s16 | their limits |
| `WEP_SPE` | `synthesis_points`, `palette`, `unknown_47`, `synthesis_per_level`, `special`, `attack_type`, `model_no` | u8, u32 bits for `special` | starting synthesis points (read as a float), model palette, a byte nothing reads here, points per level, special ability bits, attack type, model number |
| `WEP_BUILD` | `build_up` [3], `build_up_kills` [3] | s16 | weapons it builds up into (0 none); monsters to have defeated (-1 none) |

### 2.3 Usable items: `ITEM item, use_flags, status_flags, target_flags, value, value, value;`

`_DATAITEM` into `CDataItem` (0x10), for items of the item, gift box and
boiled families. `use_flags` (u32) is the effect word the menu tests (0x100,
0x400, 0x1000, ...); bit 0x800000 is shorthand the loader turns into
0x142A8000. `status_flags` (u32) are the statuses it acts on,
`target_flags` (u16) what it can be used on, `values` (s16 × 3) the amounts.

### 2.4 Attachments: `AT_ST attack, durability; AT_ST2 flame..scale; AT_ST_SP special;`

`AT_INIT n;`, then blocks by `list` into `CDataAttach` (0x18): what the
attachment adds to a weapon (s16 each) and its special bits (u32).

### 2.5 Ridepod parts: `RB_PARTS item, kind, capacity, file_no, ...;`

Into `CDataRoboPart` (0x24), by item. `kind` (not editable) says what
follows: 0 body (`defence`, then a string nothing reads, `label`), 1 arm
(`whp`, `attack`, `durability`, the eight attributes, `attack_type`,
`label`), 2 legs (`unknown_4`, `attack_type`), 3 energy pack (`energy`).
`capacity` is the energy the part uses, `file_no` its joint and sound files.
The legs' `unknown_4` goes into the part's `unk_4` and from there
(`CopyDataRoboPart`) into the owned part at +0x36, which nothing reads.

### 2.6 Fish: `FISH item, size, unknown_4, power, speed, speed, speed, stamina[, unknown_10];`

Into `CDataBreedFish` (0x14): `size` is a float (the standard size), the rest
s16; power, the three speeds and stamina are a bred fish's race parameters
(`CopyDataFish`). `unknown_4` and `unknown_10` are stored and never read
(the readers, `CopyDataFish`, `SetFishAdjustScale`, `sgInitGyoRace` and
`CItemSelect::KeyStep`, take the size and the five race parameters only).

### 2.7 Guards and names

`GRD item, value, ...;` fills one s16 per clothing item that no code reads.
`comdatmes<language>.cfg` (`LoadItemSystemMes`, 0x196A70) names the items:
`MES_SYS item, "name";` (European languages through `ConvertFontCode`) and
`MES_SYSSPE` (read and ignored).

## 3. Shops: `menu/shopusa<chapter>.cfg`

An event script opens shop n (`_GOTO_MENU` with `MENU_OPEN_SHOP`).
`CShopMenu::InitEnd` (0x295FC0) reads `menu/shopusa%d.cfg` for every language
but Japanese (`menu/shoplst%d.cfg`), the chapter `GetNowChapter` clamped to
1..8, over `menu_shop_tag` (0x358640):

| statement | into | meaning |
|---|---|---|
| `PRICE item, buy, sell;` | `CShop::price[item]` (s32 pairs, 512 slots, not checked) | buy and sell price |
| `SHOP shop, item, ...;` | `CShop::item_no[64]` when `shop` is the one opened | the goods, at most 64 |

Each of the 16 files has 428 `PRICE` lines, then 38 `SHOP` lines, each after
a `//` comment naming the shop. Between chapters a few shops' goods and a
handful of prices change. The currency follows the shop: 23 and 28 take
ridepod energy (ABS), 32 medals, 33 is Donny's shop (its goods from
`dony_shoplist` by invention level), the rest money. After reading,
`CheckEventItem` (0x295370) drops story items the player has or no longer
needs, and swaps ridepod cores for the next. The sell price of a weapon adds
20 per level and 5 per synthesis point left; a gift box adds its contents'.

In the JSON (`data:shops`): `prices` by `item` (`buy`, `sell`), `shops` by
`shop` (`items`; `currency` and `names` are labels). A value the same in all
eight chapters is one value, otherwise a list of eight; a mod may give either.
An edit goes into both sets of files, so every language sees it; an item put
in a shop must have a price in that chapter.

## 4. Monsters: `base_monster_define` in `SCES_511.90`

`BASE_MONSTER_TBL` (0xB8, `monster.hpp`), 343 records and an empty-named end
at 0x341DA0 (file offset 0x241E20), looked up by `id` (`GetMonsterTable`,
0x1DC230). `SetActiveMonster` copies a record into each monster placed;
monster scripts can change some values after that (`_SET_DEF_RATE`,
`_SET_GUARD_RATE`, `_SET_ESCAPE_RATE`, `_SET_EXT_PARAM_RATE`,
`_SET_MONSTER_LIFE`). Ids run 0-243, 245-280, 500-520, 522-523, 600-622,
628-630, 700-704, 1000-1006 and 1500-1501; 245-268 are mimics.

| offset | field | type | meaning |
|---|---|---|---|
| 0x00 | `id` | s16 | the key (not editable) |
| 0x02 | `grade` | s16 | grade the script reads |
| 0x04 | `name` | char[32] | a label: `mosdata<language>.cfg` names monsters at boot |
| 0x24 | `model` | char[16] | `dungeon/monster/<model>.chr` |
| 0x34 | `script` | char[16] | `dungeon/monster/<script>.stb`, the AI |
| 0x44 | `gift_type` | s16 | row of `gift_item_tbl` (0x351660) |
| 0x48 | `sound_bank` | s32 | `snd2/mon/EN_%03d.snd`, 0 or less none |
| 0x4C | `unknown_4c` | s32 | copied into the placed monster (+0x10E0), never read |
| 0x50 | `hp` | s32 | life |
| 0x54 | `family` | s8 | 0-10 (Beast, Windup, Aquatic, ...; 9 Undead); also the badge it can drop |
| 0x56 | `abs` | u16 | ABS given (`_SET_DEAD_OFF`) |
| 0x58 | `money` | u16 | money given (`_SET_DEAD_START`) |
| 0x5A | `unknown_5a` | s16 | copied into the placed monster (+0x10EE and +0x1322), never read |
| 0x5C | `weapon_wear` | f32 | WHP a melee hit takes off the weapon |
| 0x60 | `rage_hits` | s16 | hits before rage (-1 for bosses) |
| 0x62 | `guard_rate` | s8 | chance in 100 to guard |
| 0x63 | `dodge_rates` | s8 [2] | chances in 100 to dodge each kind of attack |
| 0x66 | `attack` | s16 | attack power |
| 0x68 | `defence` | u8 | taken off each hit's attack |
| 0x69 | `stagger` | s8 | stagger hits must build up (0 flinch at every hit) |
| 0x6A | `boss` | s8 | non-zero: a boss (life across the screen) |
| 0x6B | `sword_after_images` | s8 | sword after-image objects of the model |
| 0x6C | `elements` | s16 × 8 | damage % per attribute, flame to scale (the book lists 50 or more as a weakness) |
| 0x7C | `attack_kind_rates` | s16 [12] | damage % per attack kind (`vs_attk_index`, 0x351620) |
| 0x94 | `reaction_flags` | u32 | how hits move it (`CheckDamage`): 0x1 never staggers, 0x4 is never knocked over |
| 0x98 | `flags` | u32 | bit 0: cannot be lifted |
| 0x9C | `companion` | s32 | monster loaded with this one, -1 none |
| 0xA0 | `drops` | s16 [2] | item drops (1 in 6; the second 1 in 5 of those) |
| 0xA4 | `rare_drop` | s16 | third drop, 1 in 4 |
| 0xA8 | `status_immunity` | u32 | hit attribute bits that leave no status (0x4 poison, 0x8 and 0x8000 grey, 0x10000 slow) |
| 0xAC | `status_rate` | s16 | how readily statuses take |
| 0xAE | `percent_damage_scale` | s16 | % scale of percent-of-life hits |
| 0xB0 | `book_area` | s8 | the monster book's area |
| 0xB2 | `book_index` | s16 | the monster book's entry, -1 none |
| 0xB4 | `shared_model` | s16 | non-zero: placed monsters share one model |

The padding at 0x46, 0x55, 0x65, 0xA6, 0xB1 and 0xB6 is never written. In the
JSON (`data:monsters`) `elements` is an object of the eight; build writes only
the bytes of the fields that change, in the built image's `SCES_511.90`,
whose size and layout stay as they were (`rebuild::overwrite_iso_file`).

Which monsters a floor has: `dungeon/cfg_file/mos_place<dungeon>.cfg`,
`FLS floor, count;` then `FL id, param, id, param, ...;` (into
`CMonsterLocateInfo`, 32 slots, unchecked) and `FLE;`, read by
`LoadMonsterFile` (0x293BB0, `dng_event.cpp`) for the current floor;
`AutoSetMonster` places up to 24. Event scripts can also load and place one
kind (bosses, fixed floors). `FLS`'s count is read and dropped. The second of
each pair goes into the placed monster's `locate_param`: the talk number, -1
for a monster that cannot be talked to (`CheckMonsterTolk`); talking starts
event 1150 with it (`IsEventRun`, read back by `_GET_MONSTER_TALK_DATA`).

In the JSON (`data:floors`) each floor's last block is a record named by
`dungeon` and `floor`, with `monsters`, a list of `{"monster", "talk"}` (at
most 32). A changed list fills the floor's `FL` lines in order, the last
taking the rest; lines left over become `FL;`, which adds nothing.

## 5. Inventions, ideas and scoops

A photo's subject (`USER_PICTURE_INFO.neta_id`) comes from the map's
`FUNC_INVENT_DATA` points (3,039 in 121 `.map` files; see
[maps](maps.md)) or the monster script's `_SET_SCOOP`; below 1000 is an idea,
from 1000 a scoop. Recipes: `inv6.lst` (19 copies; the menu reads the one in
`menu/<lang>/inv2_bg.pac` or `photo_bg1.pac`, events `menu/inv6.lst`),
`CInventDataManage::LoadAnalyzeInventFile` (0x201AC0):

`DATATABLESET n;` then `DATASET item, idea, idea, idea, [material, count] × m,
unknown, scale, x, y, z;` into `INVENT_DATA_INFO` (0x24: s16 item, s16 ideas
[3], materials (s16 item, u8 count), s16 count, s16 unknown, float position
[3] and scale of the model shown). 136 recipes, 128 items, 1-3 materials.
A recipe matches when all three ideas are chosen (`CheckInventEnable`).
Idea names: `neta2.lst`, `PIC_NAME id, "name", sort;` (`LoadFilePictureName`).
Scoops: `scoop_table` (0x352DE0, `SCOOP_DATA` 0x14: s16 id, s16 menu flag, s8
info slot, text pointer) with texts from `scoop.cfg` (`STR id, "text";`).
`invent.cfg`, `inv_com.cfg`, `item.cfg` and `item_com.cfg` are menu layouts
and command scripts, not data.

`_INVENT_DATASET` takes m as (arguments - 9) / 2 and fills nothing for a
statement without a material; the arguments after the materials are the
unknown (into `unk_10`, which nothing reads), the scale (0x20) and the
position (0x14). `HowMuchZairyouMakeItem` lists at most four materials. In
the JSON (`data:inventions`): `recipes`, named by `recipe` (the place in the
file, from 0; two recipes may make one item), with `item`, `ideas` [3],
`materials` (1 to 4 `{"item", "count"}`; the count is a byte), `unknown_10`,
`scale` and `position` [3]; an edit goes into all 19 copies, the packs
rebuilt around them. `scoops`, named by `scoop` (the id), with `flag` (the
menu flag that makes the scoop known, `KnowScoop`) and `info` (its slot in
`CScoopDataManager::info`, the saved state).

## 6. Georama

Per town `gNN` (g01 to g05) and language digit: `map/g/gNN/gNN<lang>.gpi`,
read by `CEditInfoMngr::LoadEditInfo` (tag table `emap_tag__2`, 0x358A30):
`EDIT_PARTS_NUM n;`, then blocks `EDIT_PARTS "name";` .. `EDIT_PARTS_END;`
into `CEditPartsInfo` (0x280): `ID`, `PARTS_NAME` (the map part), `PARTS_ATR`
and `GROUND/BLOCK/RIVER/FENCE_PARTS` (bits), up to four `PARTS_MATERIAL item,
count`, `CPOINT category, points` (culture), `WEIGHT`, `GEO_STONE` (the
geostone that unlocks it), `MAX_NUM`, `PAINT_NUM`, `PAINT_USED`, `PARTS_TYPE`,
`PLACE_EPS`, `MAP_NO`, `POLYN`, `PARTS_COMMENT`. `gNN.cfg`
(`CEditMap::LoadEditInfo`, `emap_tag`, 0x33ABB0) holds the fixed and starting
parts (`FIX_EPARTS` and `INIT_EPARTS id, x, y, z, angle`) and the river
pieces. The requests: `geo<lang>.cfg` (`LoadEditAnalyzeData`, `tag__6`
0x358B20): `GEO_ANALYZE town;`, `CONDITION no, "text"[, geostone];`,
`ANALYZE no, unknown, "text"[, geostone];` with `CON_NO`, `ON_PARTS`,
`OFF_PARTS`, `PERCENT`, into `EditAnalyzeSrc[5]` (0x340 each). Whether a
condition is met is code (`AnalyzeSharlot`, `AnalyzeStera`, ... in
`editanalyze.cpp`, with distances and thresholds as literals), so the
conditions themselves are not data.

In the JSON (`data:georama`): `parts`, named by `town` (`"g01"`) and `part`
(its `ID`), labelled with the English `name`, with `model` (`PARTS_NAME`),
`attribute`, `materials` (`{"item", "count"}`, one per `PARTS_MATERIAL`
line: a part keeps at most the lines it has, emptied lines read as none),
`culture_points` [2], `weight`, `geostone`, `max`, `paint_colours`,
`paint_used`, `type`, `place_eps`, `map` and `polyn`; each is read from the
English file and written into all seven languages' (the Japanese file differs
in three parts' culture points; the English values are shown). The comment is
text and left out. `fixed_parts` and `starting_parts`, named by `town` and
`number` (the place in the list), with `part`, `position` [3] and `angle`.
The rivers and water are not in the table.

## 7. Fishing

Species: `FishParam` (0x361A50, `FISH_PARAM` 0x54 × 19, row 0 none): name and
model pointers, item number, base, smallest and largest size, weight and
points per size, pull, s16 [18] bait liking (0 never to 3 best; baits by
`EsaInfo`, 0x361A00) and s16 [4] time-of-day liking. Spots:
`sg/fish/fishing.cfg` (`LoadFishPlaceData`): `FISH_MAP map[, exclusive];`,
`FISH_PLACE kind, "name", 5 floats;` (an area), `FISH fish, rate,
wait_bias;` (up to 8), `FISH_MAP_END;`. Races: base stats in `fish_data`
(0x3630C0, 18 × 0x1C), opponents in `sg/gyo/rfd<lang>.cfg` (`NUM class,
count;`, `DAT item, "name", colour, power, speed × 3, stamina, size;`, the
colour the owned fish's `color_no`). Prizes: `gyop.cfg` (the fish race) and
`uofp.cfg` (the tournament), `GRP count, values;` and `count` × `PRIZE value,
item, count, item, count, item, count;` (`LoadFishPrize`), first to third
place. `RefreshFishPrize` picks a group (race class i: group 3i plus the
class's win count, at most 2; tournament: the tournament count) and takes its
first `PRIZE` only; neither `GRP`'s values nor `PRIZE`'s first are read.
Aquarium breeding: `aquafish_mixTable` (0x3535A0, s8 [171][3]).

In the JSON (`data:fishing`): `spots` (named by `spot`, the place in the
file: `map`, `exclusive`, `area`, `area_values` [5], `fish`: `{"fish", "rate",
"wait_bias"}` per `FISH` line; `FISH` reads its arguments unchecked, so a spot
keeps its number of fish), `race_fish` (by `class` and `number`, the English
`name` a label: `item`, `colour`, `power`, `speed` [3], `stamina`, `size`;
written into all seven languages' `rfd`), `prize_groups` (by `contest`,
`"race"` or `"tournament"`, and `group`: `count`, fixed, and `values`),
`prizes` (by `contest`, `group` and `prize`: `unread` and `places`, three
`{"item", "count"}`), `species` (by `fish`, the `FishParam` row: `item`,
`base_size`, `min_size`, `max_size`, `unknown_18` (nothing reads it; mostly
above `max_size`), `weight_rate`, `point_rate`, `pull_rate`, `bait_liking`
[18], `time_liking` [4]) and `race_kinds` (by `row`: `item`, fixed, `power`,
`stamina`, `speed` [3] in percent, `affinity`).

## 8. Floor medals: `menu/dngmap/dflr<dungeon>.cfg`

`CDngFloorManager::LoadDataTable` (`dngfloor.cpp`) reads `dmap<dungeon>.cfg`
(the floor map grid) and then `dflr`, over `tree_map_tag` (0x3617B0):

`RF_INFO floor, geostone, fast_time, fishing, fish_record, seal, spheda,
item, count, item, count, item, count;` into `DNGMAP_ROOM_INFO`: the fastest
clear time to beat (1/60 s), the fishing record (size × 100), seal (1 needs
Monica, 2 Max), Spheda, and the Spheda prizes. `RF_INFO2 floor, type,
param;`: the practice medal ("defeat with ..."), checked by `IsClearPractice`
against the floor's kill bits (type 0 a time limit, 1-4 a kill kind, 5 no
healing). 9, 16, 25, 21, 23, 28 and 39 floors in dungeons 0 to 6.
`_ROOM_FLOOR_INFO` stores s8 geostone, s32 time, s8 fishing (below zero the
record is a most, above zero a least), s16 record, s8 seal, s8 Spheda, and
each prize as s16 item and s8 count; `_ROOM_FLOOR_INFO2` s8 type and s32
param.

In the JSON (`data:medals`): `floors`, named by `dungeon` and `floor`, with
`geostone`, `fast_time`, `fishing`, `fishing_record`, `seal`, `spheda`,
`spheda_prizes` (three `{"item", "count"}`), `practice_type` and
`practice_param`.

## 9. Characters and other executable tables

| symbol | address | file offset | layout | what |
|---|---|---|---|---|
| `lifetbl_2854` | 0x37C790 | 0x27C810 | f32 [2] | Max's and Monica's starting HP (`CUserDataManager::Initialize` sets both the HP and its most) |
| `mos_henge_param` | 0x339D70 | 0x239DF0 | 0x1C × 57: s16 monster, s16 attack, s16 defence, effect names | Monica's monster forms (`GetMonsterHengeParam`) |
| `monster_progress_tbl` | 0x358C90 | 0x258D10 | s16 [19][5] | a badge and the form at each of its four levels: a row is one path of growth |
| `htbl_1662` | 0x33A490 | 0x23A510 | s8 [10] | WHP gained per level-up, picked at random |
| `weptbl_4503` | 0x33A510 | 0x23A590 | s16 [2][2][5] | starting equipment by language (0 Japanese, 1 the others) and character (`GetCharaDefaultWeapon`) |
| `etcitem_spectol_table` | 0x339820 | 0x2398A0 | s8 {slot, value} × 425 | spectrumising, by item number (`SetItemSpectolPoint`) |
| `dony_shoplist` | 0x358620 | 0x2586A0 | {s16 item, s8 level} × 7, then an end | Donny's shop (`GetDonyShopLineUp`) |
| `gift_item_tbl` | 0x351660 | 0x2516E0 | s16 [3] rows | gifts that win a monster's badge |
| `GolfClubDef` | 0x361340 | 0x2613C0 | 0xC × 7 | Spheda clubs |

The first seven are editable, patched in place like `data:monsters`
(`exe_tables.rs`). Spectrumising is each item's `spectrumise` in `data:items`
(items 1 to 425): `slot` 0-7 an attribute (flame to scale), 10 attack, 11
durability, any other nothing; `value` the points, times the item's count or
level. Donny's shop is `donny` in `data:shops`: `row`, `item` (1 to 511; 0
or less would end the list) and `level` (Donny's invention level must be
above it). `data:characters` has `characters` (by `character`, `"max"` or
`"monica"`: `starting_hp`, `starting_equipment` [5] and
`starting_equipment_japanese` [5]), `weapon_level_up` (one record:
`whp_gain` [10]), `monster_forms` (by `monster`: `attack`, `defence`) and
`badges` (by `row`: `badge`, `forms` [4]).
