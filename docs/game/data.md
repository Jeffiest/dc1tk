# Game data: items, shops, monsters and the rest

Where the numbers that make up the game live: item and weapon stats, shops,
monster stats, inventions, Georama, fishing, floor medals and character
stats. Engine source: the ChronicleTwo decompilation's `ps2/` (paths below are
relative to it; `re/docs/<unit>/notes.md` has the reverse engineering of
each unit). Corpus: the extracted PAL disc. Counts are over the whole disc.
Field layouts are given here; the game's values are not (export them from your
own disc: [editing game data](../modding/data.md)). The room the engine gives
each load is in [memory](memory.md); the dungeon floors' layouts, parts and
chests are in [dungeons](dungeons.md).

Most of the data is text cfg scripts in `DATA.DAT` (`TAG arg, arg, ...;`, the
format `CScriptInterpreter` reads, `scriptinterpreter.cpp`); some is compiled
into the boot executable `SCES_511.90`. Its one `PT_LOAD` maps file offset
0x80 to address 0x100000, so a `.data` address `A` is at file offset
`A - 0xFFF80` (for addresses below 0x37CD80; above that is `.bss`, filled at
run time). Addresses below are the symbols of `config/pal/main.symbols.txt`.

## How the engine reads a cfg script

Each statement is a tag and its arguments, ended by `;`. The engine reads each
argument with `GetArg`: comments (`//` to the line's end, `/* */`) blanked
first, even inside quotes; the arguments split at commas outside quotes (a
Shift-JIS lead byte takes the next byte with it, `\"` is a quote); a quoted
argument is a string, otherwise its text up to the first space is a float when
it has a `.` (C's `atof`), an int when it is digits and `-` (`atoi`, wrapping at
32 bits), and invalid when it has anything else or is empty (0, or a null
string); an empty last argument before the `;` is not counted. A statement
takes at most 64 arguments (`SPI_STACK_SIZE`; `PushStack` drops the rest).
Every cfg script of the disc breaks its lines with `\r\n`. A character's or
pack's `info.cfg` is read by the same rules.

## Limits on counts

How many of each kind of record the engine holds (each in its section):

| records | most | limit |
|---|---|---|
| items | 432 items, numbers 1 to 511; 116 weapons, 162 usable items, 38 attachments, 68 ridepod parts, 20 fish, 40 clothes | `local_com_itemdata`, `local_itemdatano_converttable`, the `local_*` family tables |
| prices and shops | items 1 to 511; 63 goods a shop | `CShop::price[512]`; `SHOP_ITEM_MAX` and 64 arguments a statement |
| recipes | 234 | the menu's `InventTeigiStack`: 0x210 units of 16 bytes, 0x24 a recipe |
| ideas | ids 1 to 999; 0x247 16-byte units of table and names a language (French: 11 short names more); 388 ideas and scoops; a name 24 bytes (or the language's longest) | `pic_name_text_buff_1660` (0x248 units); `UpdataNetaMemoStr`'s `sort_keys[0x184]`; `MenuInventNetaMemoDraw`'s `text[0x20]` |
| Georama parts | 384 a town; IDs 0 to 127; names 47 bytes; g01's files 0x77D0 bytes | `GEORAMA_PARTS_LIST_MAX`; the stock list's IDs; `name[0x30]`; `MakeDownLoadAnaunce`'s buffer |
| fixed and starting parts | 256 placed | `CEditMap::CreateTable` |
| fishing spots, a spot's fish | 8 fish a spot | `kFishPlaceMaxFish`; no array bounds the spots (`FISH_MAP_NUM` allocates them) |
| requests | 16 a town; 7 conditions a request; 0x300 string units a file | `EditAnalyzeSrc::data[16]`, `CON_NO`, `buff_1271` |
| townspeople | 180 (3 more); names 27 bytes, models 15; 0x7FD0 bytes a file | `NpcBaseData[180]`, `NPC_BASE_DATA`, `LoadNPCCfg`'s buffer |
| a floor's monsters | 32 | `CMonsterLocateInfo` (section 4) |

## 1. Where each kind of data lives

| data | where | format | records |
|---|---|---|---|
| item common data (type, stack, limit, icon, model) | `menu/cfg7/comdat.cfg` | cfg `COM` | 423 |
| weapons (WHP, ABS, attack, durability, elements, limits, synthesis, build-up) | `menu/cfg7/wepdat.cfg` | cfg `WEP` blocks | 112 |
| usable item effects | `menu/cfg7/itemdat.cfg` | cfg `ITEM` | 160 |
| attachments (crystals, gems, coins) | `menu/cfg7/atdat.cfg` | cfg `AT_ST` blocks | 36 |
| ridepod parts | `menu/cfg7/robodat.cfg` | cfg `RB_PARTS` | 63 |
| fish for breeding and racing | `menu/cfg7/fishdat.cfg` | cfg `FISH` | 18 |
| guard table (read, never used) | `menu/cfg7/grddat.cfg` | cfg `GRD` | 34 |
| item names, per language | `menu/cfg7/comdatmes0..6.cfg` | cfg `MES_SYS` | 423-428 |
| spectrumising: what each item adds to an attachment | ELF `etcitem_spectol_table` 0x339820 | s8 {slot, value} × 425 | 425 |
| prices and shop goods, by chapter | `menu/shopusa1..8.cfg`, `menu/shoplst1..8.cfg` | cfg `PRICE`, `SHOP` | 428 prices, 38 shops per file |
| Donny's shop | ELF `dony_shoplist` 0x358620 | {s16 item, s8 level} × 7 | 7 |
| monster stats, drops, rates | ELF `base_monster_define` 0x341DA0 | `BASE_MONSTER_TBL` 0xB8 × 344 | 343 |
| monster names, per language | `dungeon/cfg_file/mosdata0..6.cfg` | cfg `NAME` | 343 |
| which monsters each floor has | `dungeon/cfg_file/mos_place0..6.cfg` | cfg `FLS`/`FL`/`FLE` | 6-31 floors per dungeon |
| floor layouts (rooms of parts) | `dungeon/cfg_file/d0*.cfg`, `mstage0..8.cfg` ([dungeons](dungeons.md)) | cfg `ROOM_ID`..`ROOM_END` | 291 layouts in 23 files |
| chest items by floor | `dungeon/cfg_file/tbox_d0*.cfg` ([dungeons](dungeons.md)) | cfg `GROUP`/`ITEM`/`FLOOR` | 8 dungeons |
| dungeon parts (name, kind, sides) and mini map tiles | ELF `PartsInfoData` 0x33D440, `MiniMapInfoData` 0x33EE40 ([dungeons](dungeons.md)) | 0x18 × 277, 0x290 × 18 | 277 parts, 18 maps |
| invention recipes | `menu/inv6.lst` and its copies in the menu packs | cfg `DATASET` | 136 |
| idea and scoop names | `menu/<lang>/neta2.lst` | cfg `PIC_NAME` | 294 |
| scoops | ELF `scoop_table` 0x352DE0 and `menu/<lang>/scoop.pac` `scoop.cfg` | `SCOOP_DATA` 0x14 × 53; cfg `STR` | 53 |
| Georama parts (materials, culture points) | `map/g/gNN/gNN<lang>.gpi` | cfg `EDIT_PARTS` blocks | 13-56 per town |
| Georama fixed and starting parts, rivers, water | `map/g/gNN/gNN.cfg` | cfg `FIX_EPARTS`, `INIT_EPARTS`, `RIVER_PARTS_NAME`, `WATER_*` | per town |
| Georama requests and conditions | `geo0..6.cfg` | cfg `GEO_ANALYZE` | 5 towns, 98 conditions, 49 requests (whether a condition is met is code) |
| fish species (sizes, baits, times) | ELF `FishParam` 0x361A50 | `FISH_PARAM` 0x54 × 19 | 18 |
| fishing spots | `sg/fish/fishing.cfg` | cfg `FISH_MAP` blocks | 22 maps, 110 fish |
| fish race opponents | `sg/gyo/rfd0..6.cfg` | cfg `NUM`/`DAT` | 4 classes, 216 fish |
| fish race and tournament prizes | `gyop.cfg`, `uofp.cfg` | cfg `GRP`/`PRIZE` | 12, 10 |
| floor medals (time, fishing, Spheda, practice) | `menu/dngmap/dflr0..6.cfg` | cfg `RF_INFO`, `RF_INFO2` | 9-39 floors per dungeon |
| Max's and Monica's starting HP | ELF `lifetbl_2854` 0x37C790 | f32 × 2 | 2 |
| Monica's monster forms' attack and defence | ELF `mos_henge_param` 0x339D70 | 0x1C × 57 | 57 |
| monster badge evolutions | ELF `monster_progress_tbl` 0x358C90 | s16 [19][5] | 19 |
| fish race kinds (percent of each race figure, affinity) | ELF `fish_data` 0x3630C0 | `grFISH_DATA` 0x1C × 18 | 18 |
| WHP a weapon gains at a level-up | ELF `htbl_1662` 0x33A490 | s8 [10] | 10 |
| starting equipment, by language | ELF `weptbl_4503` 0x33A510 | s16 [2][10] | 2 × 2 × 5 |
| townspeople (abilities, costs) | `npc0..6.cfg` | cfg `NPC_INFO` | 177 |
| gifts that win a badge | ELF `gift_item_tbl` 0x351660 | s16 [3] × 14, then -1 | 14 |
| Spheda clubs | ELF `GolfClubDef` 0x361340 | 0xC × 7 (6 reached) | 6 |
| aquarium breeding | ELF `aquafish_mixTable` 0x3535A0 | s8 [171][3] | 171 |

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

### 2.1 `COM item, type, list, active_set, stack, max, icon_sheet, icon, message, "model", attribute;`

`_DATACOM` (0x195BB0) into `CDataCommon` (0x2C):

| field | type | meaning |
|---|---|---|
| `type` | u8 | item type; gives the family (above) and the model folder |
| `list` | s16 | index in the family's table |
| `active_set` | u8 | non-zero: can be set as an active item |
| `stack` | s16 | how many one stack holds |
| `max` | u16 | most the player can hold (weapons above 100 read as 144) |
| `icon_sheet` | u8 | the menu icon texture (`GetMenuItemIconTexInfo`) |
| `icon` | s16 | the icon on it |
| `message` | s16 | base of the item's message numbers |
| `model` | char[16] | model file base name (`item/`, `mainchr/` or `dungeon/robo/`) |
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

Into `CDataRoboPart` (0x24), by item. `kind` says what
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
`MES_SYS item, "name";` (European languages through `ConvertFontCode`, which
turns each `[UNI00xx]` into the byte xx) and `MES_SYSSPE` (read and
ignored). This is the name `GetItemMessage` gives (an owned weapon's name,
the inventions and Georama menus). Each name is copied into
`gamedata_sysword_buffer_1073`, 0x280 units of 16 bytes (`mgCopyString`;
the allocation reaching the end fails): the PAL languages use 479 to 583,
Korean (not played) 1,038. The menus and item slots show system message
10000 + `message` of `meswin/system_N.mes` (`GetItemMessageNo` kind 1,
`msg_offsettbl_1363`), and the description is the `allmenu.mes` message of
that number (`[String1]` on its first line, then the text) in the menu
packs. `system_N.mes` has 0xD000 bytes.

### 2.8 Limits, icons and models

Limits: 432 items (`local_com_itemdata`, 9 more than the
disc's), numbers 1 to 511, the family tables (116 weapons: 4 more; 38
attachments: 2; 162 usable items: 2; 68 ridepod parts: 5; 20 fish: 2; 40
clothes: 6), each script within the 0x77C0 bytes `LoadGameDataAnalyze`
reads it into, the names within the name buffer in the languages played,
and a name as long as the owned item keeps it (31 bytes for weapons and
ridepod parts, 25 attachments, 20 fish, 21 boiled fish; `CopyDataWeapon`
and the rest copy it unchecked). An item above 425 that is not a weapon or
attachment may not be spectrumised (attribute bit 2): `SetItemSpectolPoint`
reads `etcitem_spectol_table[item - 1]` unchecked, and the table has 425.
The icon is `icon_sheet` (0 `wepicon`, 1 `itemicon` of `img/allitem.img`)
and `icon` (192 a sheet, 8 by 24 of 32 pixels). Selling an item needs a
`PRICE` (section 3).

No item points at cells 52 and 191 of
`itemicon` (191 is the game's 予約, "reserved", placeholder) nor at 52, 56,
110, 150 and 191 of `wepicon` (56 is Lamb's Sword's at night,
`GetMenuItemIconTexGetXY` adding 1 for item 56 in time band 2; 110 and 150
hold numerals and marks). The sheets are in `img/allitem.img` and its copies
in the menu packs (the `@mb2`, `@pack01` and `@trushsel` variants are in packs
the PAL game does not load), each 256 colours for all 192 cells. The model
file is `item/` + `model` + `.chr` for every type but weapons and clothes
(`mainchr/`) and ridepod parts (`dungeon/robo/`) (`GetItemFilePath`). The
menus that show an item's model (`MenuItemChrLoadEndCheck`,
`CMenuInvent::LoadCharaCheck`) load the pack and set no motion, and none of
the disc's 168 item packs has one.

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

A shop lists at most 63 goods: `item_no` has 64 slots, but `SHOP` takes the
shop's number as one of the 64 arguments a statement may have. The shop files
load into the menu's local stack, whose free space is not known.

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
| 0x00 | `id` | s16 | the key |
| 0x02 | `grade` | s16 | grade the script reads |
| 0x04 | `name` | char[32] | `mosdata<language>.cfg` names monsters at boot |
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

The padding at 0x46, 0x55, 0x65, 0xA6, 0xB1 and 0xB6 is never written.

Which monsters a floor has: `dungeon/cfg_file/mos_place<dungeon>.cfg`,
`FLS floor, count;` then `FL id, param, id, param, ...;` (into
`CMonsterLocateInfo`, 32 slots, unchecked) and `FLE;`, read by
`LoadMonsterFile` (0x293BB0, `dng_event.cpp`) for the current floor;
`AutoSetMonster` places up to 24. Event scripts can also load and place one
kind (bosses, fixed floors). `FLS`'s count is read and dropped. The second of
each pair goes into the placed monster's `locate_param`: the talk number, -1
for a monster that cannot be talked to (`CheckMonsterTolk`); talking starts
event 1150 with it (`IsEventRun`, read back by `_GET_MONSTER_TALK_DATA`).
The 32 are `CMonsterLocateInfo`'s slots: `_FL` (0x292DB0) does not check, and
a 33rd would write over the talk numbers. One `FL` line holds them all (64
arguments, the most a statement has).

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
position (0x14). `HowMuchZairyouMakeItem` lists at most four materials. The
menu gives `InventTeigiStack` 0x210 units of 16 bytes (`MenuInventInit`) and
`_INVENT_DATATABLESET` takes 0x24 bytes a recipe, so 234 recipes fit (the
event check's `CheckItemTable` has 0x400 units, 454 recipes). The cards made
(`CInventUserData::created_item[0x100]`) are by item.

### 5.1 Ideas: `menu/<lang>/neta2.lst`

`PIC_INFO n;` then `PIC_NAME id, "name", sort;` per idea and scoop (241
ideas, ids 1 to 250, then 53 scoops). `LoadFilePictureName` reads
`menu/<LanguageCode>/neta2.lst` (`LoadFileMenu`: 0 Japanese, 1 English, 2
French, 3 German, 4 Italian, 5 Spanish; Korean, 6, reads `1/`; `menu/neta2.lst`
is a copy of the Japanese nothing reads) into a 0x5000-byte buffer. The
table and the names share one heap, `pic_name_text_buff_1660`, of 0x248
16-byte units: `_PIC_INFO` takes ceil(n × 8 / 16) + 2 for the 8-byte
`PIC_NAME_INFO` (s16 id, s16 sort, name pointer) and `_PIC_NAME` ceil((length
+ 1) / 16) a name, its length as stored: `ConvertFontCode` turns each
`[UNIxxxx]` code into one byte only when LanguageCode is 5 or more (the asm
at 0x200FCC), so French, German and Italian keep the 9-byte codes.
`mgCMemory::Alloc` refuses the allocation that reaches the end (the name is
then NULL). On the PAL disc the copies take 491 (ja), 508 (en), 566 (fr), 533
(de), 539 (it) and 536 (es) units. `_PIC_NAME` drops an entry of id 30000; a
map invent point of 30001 photographs townsperson 260 (`dng_main.cpp`), so
neither is an idea. Ideas are below 1000 (`[bulb2]` in the notebook), scoops
from 1000.

The notebook (`CMenuInvent::UpdataNetaMemoStr`) lists the known ids
(`CInventUserData::neta_id[0x200]`) in file order, keeping their sort keys in
`sort_keys[0x184]` (388), sorts the first ones, as many as the known ideas,
by the key, and the scoops after them by their own order: the ideas come
first in the file. `MenuInventNetaMemoDraw` prints `[bulb2]` and the name
into `text[0x20]` (24 bytes of name; the game's own French, German and
Spanish names run past it, up to 33 bytes for an idea). The sort key is the
place of the name's first letter × 100 + its place among that letter's
names: A is 0 in English, French, German and Italian (accents, `Œ` and `Æ`
folded), 1 in Spanish (0 for a name opening with `¡`); in Japanese the kana
row of the reading (あ 0, い 1 ... か 5 ... や 35, ゆ 36, よ 37, ら 40 ... わ 45,
Latin letters 50).

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
conditions themselves are not data. The Japanese `.gpi` differs from the
English in three parts' culture points.

Limits: 384 parts a town (`GEORAMA_PARTS_LIST_MAX`: the placed, stock and
make lists, and `MakeDownLoadAnaunce` goes over the first 0x180); IDs 0 to 127
(the stock list, `CMenuGeorama::UpdateGeoramaPartsList`, goes over 0..0x7F;
`CSaveData::build_parts_num[0x100]` and `RemoveInfo.parts_num[256]` count
by ID); a name of 47 bytes (`GEORAMA_PARTS_LIST_ITEM::name[0x30]`, filled
by strcpy); g01's files within 0x77D0 bytes (`MakeDownLoadAnaunce` reads
`g01%d.gpi` into a 0x7800-byte stack buffer; the largest, Korean, has 27,202).
An ID is the stock's for every town: the towns' parts share IDs (house 1 in
four towns), and a part of another town's ID shares its stock. The
`model` (`PARTS_NAME`) must be a map part of the town's map
(`CMap::GetParts`; without one the part has no collision); a ground part
(attribute & 7) makes one of the map's 4 grids. Placed parts: 256 a town
(`CEditMap::CreateTable`). `FIX_EPARTS_START` and `INIT_EPARTS_START`
allocate the fixed and starting parts; a line past the count is dropped.
Code names some IDs: 7, 0x13, 0x1D, 0x23, 0x28-0x2B, 0x2F (requests), the
houses 1, 9, 0x16, 0x1F, 0x38, 0x46-0x48, 0x56 and 0x57.

### 6.1 Requests: `geo<language>.cfg`

`LoadEditAnalyzeData` (`editdata.cpp`, from `MainLoop` and on a language
change) reads `geo%d.cfg` at the disc's root, one file per language with
all five towns, over `tag__6` (0x358B20), into `AnalyzeSrc[5]` (0x340 each:
`condition[64]` texts, `geo_floor[64]`, `data[16]` of 0x1C):

| statement | meaning |
|---|---|
| `GEO_ANALYZE town;` | the town (0 to 4) the next statements fill; clears it |
| `CONDITION number, "text"[, geostone];` | condition 0 to 63's text and the floor whose geostone shows it (dungeon × 100 + floor) |
| `ANALYZE request, unread, "title"[, geostone];` | request 0 to 15 (`GetAnalyzeDataSrc` stops at the first missing one); the second is stored and never read |
| `CON_NO condition, ...;` | the conditions it needs, at most 7 |
| `ON_PARTS "parts";`, `OFF_PARTS "parts";` | map parts (`part` or `part/piece`, `;` between, at most 16, names of 64 bytes) shown and hidden when it is met (`GetOnOffParts`, `PartsOnOff`) |
| `PERCENT points;` | what it adds to the town's percentage |
| `END_ANALYZE;`, `END_GEO_ANALYZE;` | |

Every string is copied into `buff_1271`, 0x3000 bytes (0x300 units of 16)
for all the towns. Whether a condition is met is code: `AnalyzeEditMap`
calls `AnalyzeSharlot`, `AnalyzeStera`, `AnalyzeBenietio`, `AnalyzeHeim`
or `AnalyzeMoonFlower` (`editanalyze.cpp`), which set `condition[64]` from
part IDs, river counts, residents and culture points written as literals,
and `CEditData::Analyze` resolves requests from them. So a request may need
any condition the town's code sets (town 0: 0 to 17), and a condition of
another number is never met. Retail has 9, 10, 9, 11 and 10 requests.

### 6.2 Rivers and water: `gNN.cfg`

`CEditMap::LoadEditInfo` (`emap_tag`, 0x33ABB0) reads, between `EDIT_RIVER
"name";` and `EDIT_RIVER_END;` (both read as nothing), `RIVER_PARTS_NAME
slot, "map part", "model.mds";` (slots 0 to 7: the map part the river piece
replaces and the model drawn), `WATER_PARTS_NAME "model.mds";` and
`MASK_PARTS_NAME 0, "model.mds";` (slot 0 only); `CEditPartsInfo[8]` holds
the rivers. `CMap::LoadCfgFile` (`cfg_tag`, 0x338BF0) reads the water:
`WATER_SURFACE_NUM n;` (it allocates n), blocks `WATER_SURFACE_START;` ..
`WATER_SURFACE_END;` with `WATER_VERTEX nx, nz, x0, y0, z0, x1, y1, z1;`
(the grid and its corners), `WATER_POS x, y, z;`, `WATER_PARAM a, b, c, d;`
(the waves) and `WATER_SHAKE` (read as nothing), and `WATER_DRAW_NUM n;` then
`WATER_DRAW surface, "part or #followN", x, y, z;`. g01 and g05 have rivers.

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

`FISH_MAP_NUM n;` allocates the spots (`FISH_PLACE_MAP`, 0x88 each, from
the scene's stack; a block past n is dropped) and each holds 8 fish
(`kFishPlaceMaxFish`; `fish` indexes `FishParam`, 0 to 18). A spot has one
area: a second `FISH_PLACE` overwrites the first, so a map with two areas
has two spots of that map; only area kind 2 (a circle: x, z, radius) is
tested, the others always match. `GetAppearFish` gathers the fish of every
spot of the map whose area holds the cast (16 at most), an `exclusive`
spot replacing the rest, map -1 when none matches. `FISH` reads its arguments
unchecked. `fishing.cfg` loads into the 1 MB read buffer.

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
| `aquafish_mixTable` | 0x3535A0 | 0x253620 | s8 [171][3] | aquarium breeding |

Each table's size is fixed in `.data`. Spectrumising, by item (1 to 425):
`slot` 0-7 an attribute (flame to scale), 10 attack, 11 durability, any other
nothing; `value` the points, times the item's count or level. Donny's shop: an
`item` of 0 or less ends the list, and Donny's invention level must be above
the row's `level`.

- **`gift_item_tbl`** (0x351660, file offset 0x2516E0): 14
  rows of s16 {gift type, item, unread}, then a row of gift type -1 that
  ends it. `CheckGiftPack` (`monster.cpp`, 0x1DF520) finds the row of the
  monster kind's `gift_type` (section 4) and takes a gift whose three
  slots (`_COLPRIM_GET_GIFT`: item, count, rate) all equal the row's item, so
  the check rarely passes, as in the retail game; `CMonsterMan::CheckDamage`
  then shows the gift mark. The third value is never read.
- **`GolfClubDef`** (0x361340, file offset 0x2613C0): {f32
  power, f32, s32} per club; `GetSphidaClubDef` takes clubs 9 to 14 (row
  club - 9), so the seventh row, of zeros, is never read. `CSphida::DrawStatusSprite`
  shows the power; `_SPHIDA_GET_CULB_DEF` gives event scripts all three.
- **`aquafish_mixTable`** (0x3535A0, file offset
  0x253620): s8 [171][3], {parent, parent, child}, each a fish item less
  0x136 (items 0x136 to 0x150 are the fish). `GetChildFishNo` (menuaqua,
  0x20ED80, from `CAquarium::CombineFish`) goes over all 171 rows, the
  parents in either order, and gives the child + 0x136, or 0x136 when no row
  has the pair.

## 10. Townspeople: `npc<language>.cfg`

`LoadNPCCfg` (`npccfg.cpp`, from `MainLoop` and on a language change) reads
`npc%d.cfg` at the disc's root into a 0x8000-byte buffer (aligned to 64
first), over `npc_spitag`: `NPC_NUM n;` (overwritten by the count read) and
`NPC_INFO number, "name", "model", kind, abilities, points, cost, cost,
cost, cost, debug;` into `NpcBaseData[180]` (`NPC_BASE_DATA`, 0x36):

| argument | field | meaning |
|---|---|---|
| number | s16 `chara_no` | the townsperson (the key) |
| name | char[0x1C] | the name, the language's (strcpy before the "NAME OVER" check: 27 bytes) |
| model | char[0x10] | `chara/%s.chr`, `event/train/t%s.chr`, `menu/npc/t%s.chr` (15 bytes) |
| kind | `unk_31` | the message kind `_GET_PARTY_CHARA_MES_NO` gives for type 4 |
| abilities | `ability_num` | abilities the party menu shows (at most 4) |
| points | `max_npc_point` | the most points |
| cost × 4 | `ability_cost[4]` | each ability's cost |
| debug | `debug_flag` | the debug menu's selection only |

`npc_spi_count_num` is a u8 index with no check: 180 is the most, 3 more
than the disc's 177. Only numbers 1 to 32 can join the party or live in a
Georama house (`CUserDataManager::party_chara[32]`, `JoinPartyChara`); a
townsperson's lines are messages typetbl[type] + number × 100
(`GetPartyCharaMessage`). Where they stand in towns is `npc_place*.cfg`
(`VlgrPlace[0x200]`).
