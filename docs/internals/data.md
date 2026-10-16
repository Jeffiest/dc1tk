# Game data: items, shops, monsters and the rest

This page documents where Dark Chronicle stores its gameplay data tables: item and weapon stats, shop inventories, monster attributes, invention recipes, Georama parts and requests, fishing parameters, dungeon floor medals, character stats, and townspeople.
- For exporting and editing these tables from your own disc, see [editing game data](../guide/data.md).
- For the runtime memory budgets of each data loader, see [memory](memory.md).
- For dungeon floor layouts, room parts, and treasure chests, see [dungeons](dungeons.md).

All engine source paths on this page are relative to `ps2/` in the ChronicleTwo decompilation (with unit-level reverse-engineering notes in `re/docs/<unit>/notes.md`). All corpus counts cover the entire extracted PAL disc (`SCES_511.90`).

Most gameplay tables live in plain-text `.cfg` scripts inside `DATA.DAT` (`TAG arg, arg, ...;`, parsed by `CScriptInterpreter` in `src/scriptinterpreter.cpp`). The remaining tables are compiled directly into the `.data` section of the boot executable `SCES_511.90`. The executable's single `PT_LOAD` segment maps file offset `0x80` to virtual address `0x100000`. Consequently, any `.data` virtual address `A` below `0x37CD80` resides at file offset `A - 0xFFF80` (addresses at `0x37CD80` and above belong to `.bss` and are zero-filled at runtime). All symbol names and addresses below come from `config/pal/main.symbols.txt`.

## How the engine reads a cfg script

Each statement in a `.cfg` script consists of a tag name followed by comma-separated arguments and terminated by a semicolon (`;`). `CScriptInterpreter::GetArg` parses each statement as follows:
1. **Comment stripping**: The parser blanks out `//` line comments and `/* ... */` block comments **before** parsing quotes. A `//` or `/*` sequence inside a quoted string still strips the rest of the line or block.
2. **Argument splitting**: Arguments split on commas outside double quotes. The lexer advances two bytes whenever it encounters a Shift-JIS lead byte, and treats `\"` as an escaped quote character.
3. **Type classification**:
   - A quoted token parses as a string.
   - An unquoted token is read up to its first space: if it contains a decimal point (`.`), C's `atof` parses it as a `float`; if it consists solely of ASCII digits and `-`, `atoi` parses it as a 32-bit wrapping `int`; if it contains any other character or is empty, it is marked invalid (`0` or a null string).
   - An empty trailing argument immediately before `;` is ignored.
4. **Argument limit**: A single statement accepts at most 64 arguments (`SPI_STACK_SIZE`). `PushStack` silently drops any arguments beyond the 64th.

Every `.cfg` script on the PAL disc uses `\r\n` line endings. Character and model pack `info.cfg` files use this exact same interpreter.

## Limits on counts

The table below summarises the hard capacity limits for each data category in the engine (detailed in the corresponding sections below):

| Data Category | Maximum Capacity | Limiting Engine Array or Buffer |
|---|---|---|
| Items | 432 total items (item IDs `1..511`): 116 weapons, 162 usable items, 38 attachments, 68 Ridepod parts, 20 fish, 40 clothes | `local_com_itemdata`, `local_itemdatano_converttable`, and the per-family `local_*` `.bss` arrays |
| Prices and shop inventories | Item IDs `1..511`; at most 63 goods per shop | `CShop::price[512]`; `SHOP_ITEM_MAX` (`64`) and the 64-argument limit per `SHOP` statement |
| Invention recipes | 234 recipes | Menu `InventTeigiStack`: `0x210` units of 16 bytes (`0x24` bytes per recipe) |
| Ideas and scoops | Idea IDs `1..999`; `0x247` 16-byte heap units for the table plus strings per language (room for ~11 more short names in French); at most 388 combined ideas and scoops; display name up to 24 bytes (or the longest existing name in that language) | `pic_name_text_buff_1660` (`0x248` units); `UpdataNetaMemoStr` `sort_keys[0x184]`; `MenuInventNetaMemoDraw` `text[0x20]` |
| Georama parts | 384 parts per town; part IDs `0..127`; part name up to 47 bytes; `g01` `.gpi` file up to `0x77D0` bytes | `GEORAMA_PARTS_LIST_MAX`; stock list ID loop (`0..0x7F`); `name[0x30]`; `MakeDownLoadAnaunce` stack buffer |
| Georama placed parts | 256 placed parts per town | `CEditMap::CreateTable` |
| Fishing spots and fish per spot | 8 fish slots per spot; spot count is dynamically allocated from `FISH_MAP_NUM` | `kFishPlaceMaxFish` (`8`) |
| Georama requests | 16 requests per town; up to 7 conditions per request; `0x300` 16-byte string units across all towns | `EditAnalyzeSrc::data[16]`, `CON_NO`, and `buff_1271` |
| Townspeople (NPCs) | 180 records (3 more than the retail 177); name up to 27 bytes, model name up to 15 bytes; `.cfg` file up to `0x7FD0` bytes | `NpcBaseData[180]`, `NPC_BASE_DATA`, and `LoadNPCCfg` buffer |
| Monsters per dungeon floor | 32 monster entries per floor | `CMonsterLocateInfo` (see section 4) |

## 1. Where each kind of data lives

| Data Table | Location (`DATA.DAT` Path or `SCES_511.90` Symbol) | Format / Struct | Retail Record Count |
|---|---|---|---|
| Item common data (type, stack, limit, icon, model) | `menu/cfg7/comdat.cfg` | cfg `COM` | 423 |
| Weapons (WHP, ABS, attack, durability, elements, limits, synthesis, build-up) | `menu/cfg7/wepdat.cfg` | cfg `WEP` blocks | 112 |
| Usable item effects | `menu/cfg7/itemdat.cfg` | cfg `ITEM` | 160 |
| Attachments (crystals, gems, coins) | `menu/cfg7/atdat.cfg` | cfg `AT_ST` blocks | 36 |
| Ridepod parts | `menu/cfg7/robodat.cfg` | cfg `RB_PARTS` | 63 |
| Fish breeding and racing stats | `menu/cfg7/fishdat.cfg` | cfg `FISH` | 18 |
| Clothing guard table (parsed, never used) | `menu/cfg7/grddat.cfg` | cfg `GRD` | 34 |
| Item names per language | `menu/cfg7/comdatmes0..6.cfg` | cfg `MES_SYS` | 423–428 |
| Spectrumising bonuses per item | ELF `etcitem_spectol_table` (`0x339820`) | `s8 { slot, value } × 425` | 425 |
| Item prices and shop inventories by chapter | `menu/shopusa1..8.cfg`, `menu/shoplst1..8.cfg` | cfg `PRICE`, `SHOP` | 428 prices, 38 shops per file |
| Donny's shop inventory | ELF `dony_shoplist` (`0x358620`) | `{ s16 item, s8 level } × 7` | 7 |
| Monster stats, drops, and resistances | ELF `base_monster_define` (`0x341DA0`) | `BASE_MONSTER_TBL` (`0xB8 × 344`) | 343 |
| Monster names per language | `dungeon/cfg_file/mosdata0..6.cfg` | cfg `NAME` | 343 |
| Monster spawn lists per dungeon floor | `dungeon/cfg_file/mos_place0..6.cfg` | cfg `FLS` / `FL` / `FLE` | 6–31 floors per dungeon |
| Dungeon floor room layouts | `dungeon/cfg_file/d0*.cfg`, `mstage0..8.cfg` ([dungeons](dungeons.md)) | cfg `ROOM_ID`..`ROOM_END` | 291 layouts across 23 files |
| Treasure chest item tables by floor | `dungeon/cfg_file/tbox_d0*.cfg` ([dungeons](dungeons.md)) | cfg `GROUP` / `ITEM` / `FLOOR` | 8 dungeons |
| Dungeon room parts and minimap tiles | ELF `PartsInfoData` (`0x33D440`), `MiniMapInfoData` (`0x33EE40`) ([dungeons](dungeons.md)) | `0x18 × 277`, `0x290 × 18` | 277 parts, 18 maps |
| Invention recipes | `menu/inv6.lst` (plus copies inside menu `.pac` archives) | cfg `DATASET` | 136 |
| Idea and scoop names | `menu/<lang>/neta2.lst` | cfg `PIC_NAME` | 294 |
| Scoop metadata and descriptions | ELF `scoop_table` (`0x352DE0`) and `menu/<lang>/scoop.pac` (`scoop.cfg`) | `SCOOP_DATA` (`0x14 × 53`); cfg `STR` | 53 |
| Georama buildable parts (materials, culture points) | `map/g/gNN/gNN<lang>.gpi` | cfg `EDIT_PARTS` blocks | 13–56 per town |
| Georama fixed/starting parts, rivers, and water | `map/g/gNN/gNN.cfg` | cfg `FIX_EPARTS`, `INIT_EPARTS`, `RIVER_PARTS_NAME`, `WATER_*` | Per town |
| Georama requests and condition labels | `geo0..6.cfg` | cfg `GEO_ANALYZE` | 5 towns, 98 conditions, 49 requests (condition checks are hardcoded) |
| Fish species parameters (sizes, bait preferences, times) | ELF `FishParam` (`0x361A50`) | `FISH_PARAM` (`0x54 × 19`) | 18 |
| Fishing spots and spawn weights | `sg/fish/fishing.cfg` | cfg `FISH_MAP` blocks | 22 maps, 110 fish entries |
| Fish race opponents | `sg/gyo/rfd0..6.cfg` | cfg `NUM` / `DAT` | 4 classes, 216 fish |
| Fish race and Fishing Contest prizes | `gyop.cfg`, `uofp.cfg` | cfg `GRP` / `PRIZE` | 12, 10 |
| Dungeon floor medal goals (time, fishing, Spheda, practice) | `menu/dngmap/dflr0..6.cfg` | cfg `RF_INFO`, `RF_INFO2` | 9–39 floors per dungeon |
| Max and Monica starting HP | ELF `lifetbl_2854` (`0x37C790`) | `f32 × 2` | 2 |
| Monica monster badge form attack and defence | ELF `mos_henge_param` (`0x339D70`) | `0x1C × 57` | 57 |
| Monster badge evolution trees | ELF `monster_progress_tbl` (`0x358C90`) | `s16[19][5]` | 19 |
| Fish race species scaling and terrain affinity | ELF `fish_data` (`0x3630C0`) | `grFISH_DATA` (`0x1C × 18`) | 18 |
| Weapon WHP gain per level-up | ELF `htbl_1662` (`0x33A490`) | `s8[10]` | 10 |
| Starting equipment by language | ELF `weptbl_4503` (`0x33A510`) | `s16[2][10]` | `2 × 2 × 5` |
| Townspeople (abilities, Recruitment/Georama points) | `npc0..6.cfg` | cfg `NPC_INFO` | 177 |
| Gifts that unlock monster badges | ELF `gift_item_tbl` (`0x351660`) | `s16[3] × 14` (terminated by `-1`) | 14 |
| Spheda club parameters | ELF `GolfClubDef` (`0x361340`) | `0xC × 7` (6 reachable) | 6 |
| Aquarium fish breeding combinations | ELF `aquafish_mixTable` (`0x3535A0`) | `s8[171][3]` | 171 |

**Unused legacy files on the disc**: The directories `menu/cfg5` and `menu/cfg6`, plus the files `menu/0/inv*.lst`, `menu/inv4.lst`, `menu/inv5.lst`, `sg/fish/fish.cfg`, `sg/gyo/racefish*.cfg`, `npc_place.cfg` through `npc_place3.cfg`, `npc.cfg`, `edit.cfg`, and the `.gpi` files without a language digit are leftover development files that no code references.

**Hardcoded growth rules**: Character defence and weapon level-up curves are computed in C++ code rather than data tables. `CUserDataManager::Initialize` sets starting defence to `4` or `8`. A weapon's required ABS increases by half of its current requirement (`+50%`) each level, and its maximum WHP increases by one of the 10 values in `htbl_1662` (`0x33A490`) chosen at random.

## 2. Items: `menu/cfg7`

`CGameData::LoadData` (`src/gamedata.cpp`, `0x196980`) parses `comdat.cfg`, `wepdat.cfg`, `itemdat.cfg`, `atdat.cfg`, `robodat.cfg`, `fishdat.cfg`, and `grddat.cfg` from `menu/cfg7/` using the 24-entry tag table `gamedata_tag` (`0x339B80`).

An item ID (`1..511`) maps to its `CDataCommon` record through `local_itemdatano_converttable`. The `CDataCommon::list` field then indexes the item's family-specific table. `ConvertUsedItemType(type)` maps `type` to an item family:
- `1..4`: Weapon
- `5..10`: Clothing
- `11` and `>= 20`: Usable item (`0x1C` = gift box, `0x1E` = fish, `0x23` = roasted fish)
- `12..15`: Ridepod part
- `16..19` and `0x22`: Attachment

The engine stores these records in fixed-size `.bss` arrays (`local_*`): 432 common records, 116 weapons, 162 usable items, 38 attachments, 68 Ridepod parts, 20 fish, and 40 guards.

### 2.1 `COM item, type, list, active_set, stack, max, icon_sheet, icon, message, "model", attribute;`

`_DATACOM` (`0x195BB0`) populates a `CDataCommon` struct (`0x2C` bytes):

| Field | Type | Meaning |
|---|---|---|
| `type` | `u8` | Item category; determines the item family (above) and the 3D model directory. |
| `list` | `s16` | Zero-based index into the family's specific data table. |
| `active_set` | `u8` | Non-zero if the player can equip the item in an active quick-use slot. |
| `stack` | `s16` | Maximum count per inventory slot stack. |
| `max` | `u16` | Maximum total count the player can carry (for weapons, values above `100` are replaced with `144`). |
| `icon_sheet` | `u8` | Menu icon texture sheet index (`GetMenuItemIconTexInfo`: `0` = `wepicon`, `1` = `itemicon`). |
| `icon` | `s16` | Cell index (`0..191`) on the icon sheet. |
| `message` | `s16` | Base offset for the item's name and description message IDs. |
| `model` | `char[16]` | Base filename of the `.chr` model pack (under `item/`, `mainchr/`, or `dungeon/robo/`). |
| `attribute` | `u32` | Bitfield: `0x1` = can be discarded (`IsTrush`), `0x2` = can be spectrumised. |

### 2.2 Weapons: `WEP`, `WEP_ST`, `WEP_ST_L`, `WEP_ST2`, `WEP_ST2_L`, `WEP_SPE`, `WEP_BUILD`

`WEPNUM n;` sets the weapon count. Each subsequent group of `WEP*` statements populates the next sequential `CDataWeapon` slot (`0x4C` bytes), and `WEP_BUILD` advances the slot counter. Therefore, the `n`-th weapon block in `wepdat.cfg` corresponds to `list = n`.

| Statement | Fields | Type | Meaning |
|---|---|---|---|
| `WEP` | `whp`, `abs` | `s16` | Base WHP gauge (capped at `255`); ABS required for the first level-up. |
| `WEP_ST` | `attack`, `durability` | `s16` | Starting Attack and Durability (Durability reduces WHP loss per hit in `calcWeaponParamWhp`). |
| `WEP_ST_L` | `attack_max`, `durability_max` | `s16` | Maximum Attack and Durability caps for this weapon tier. |
| `WEP_ST2` | `flame`, `chill`, `lightning`, `cyclone`, `smash`, `exorcism`, `beast`, `scale` | `s16[8]` | Starting elemental/attribute stats (in the same order as attachment crystals). |
| `WEP_ST2_L` | `flame_max` .. `scale_max` | `s16[8]` | Maximum caps for each of the 8 attributes. |
| `WEP_SPE` | `synthesis_points`, `palette`, `unknown_47`, `synthesis_per_level`, `special`, `attack_type`, `model_no` | `u8` (`u32` bitfield for `special`) | Starting synthesis points (parsed as a float), model palette index, an unused byte (`unknown_47`), synthesis points gained per level, special status flags (`special`), attack animation/damage type, and player base-pack model number (`0..3`). |
| `WEP_BUILD` | `build_up[3]`, `build_up_kills[3]` | `s16` | Up to 3 target weapon item IDs this weapon can build up into (`0` = none), followed by the monster IDs that must be defeated to unlock each branch (`-1` = none). |

### 2.3 Usable items: `ITEM item, use_flags, status_flags, target_flags, value, value, value;`

`_DATAITEM` populates a `CDataItem` struct (`0x10` bytes) for items in the usable item, gift box, and roasted fish families:
- `use_flags` (`u32`): Effect bitmask checked by the inventory menu (`0x100`, `0x400`, `0x1000`, etc.). If bit `0x800000` is set in the script, the loader expands it to `0x142A8000`.
- `status_flags` (`u32`): Status ailments cured or inflicted by the item.
- `target_flags` (`u16`): Valid target mask (Max, Monica, Ridepod, etc.).
- `values` (`s16[3]`): Numeric effect magnitudes.

### 2.4 Attachments: `AT_ST attack, durability; AT_ST2 flame..scale; AT_ST_SP special;`

`AT_INIT n;` initializes the table, and each subsequent block populates `CDataAttach[list]` (`0x18` bytes) with the stat bonuses (`s16` each) and special ability bitmask (`u32`) that the attachment adds to a weapon.

### 2.5 Ridepod parts: `RB_PARTS item, kind, capacity, file_no, ...;`

Populates `CDataRoboPart` (`0x24` bytes) for `item`. The `kind` field selects the remaining arguments:
- `0` (Body): `defence`, followed by an unread string and `label`.
- `1` (Arm weapon): `whp`, `attack`, `durability`, the 8 elemental attributes, `attack_type`, and `label`.
- `2` (Legs): `unknown_4` and `attack_type`.
- `3` (Energy pack): `energy`.

`capacity` is the core capacity cost required to equip the part, and `file_no` selects its joint and sound files. For legs (`kind = 2`), `unknown_4` is stored in `CDataRoboPart::unk_4` and copied by `CopyDataRoboPart` to offset `+0x36` of the owned part, where no code ever reads it.

### 2.6 Fish: `FISH item, size, unknown_4, power, speed, speed, speed, stamina[, unknown_10];`

Populates `CDataBreedFish` (`0x14` bytes):
- `size` (`f32`): Standard length of the species.
- `unknown_4` (`s16`), `power` (`s16`), `speed[3]` (`s16[3]`), `stamina` (`s16`), and optional `unknown_10` (`s16`).

`CopyDataFish` copies `power`, the three `speed` stats, and `stamina` as a bred fish's racing parameters. The fields `unknown_4` and `unknown_10` are stored in `CDataBreedFish` but never read by any consumer (`CopyDataFish`, `SetFishAdjustScale`, `sgInitGyoRace`, and `CItemSelect::KeyStep` read only `size` and the five racing parameters).

### 2.7 Guards and names

- `grddat.cfg`: `GRD item, value, ...;` populates one `s16` value per clothing item into `local_grddata`, which no code in the engine ever reads.
- `comdatmes<language>.cfg`: `LoadItemSystemMes` (`0x196A70`) parses `MES_SYS item, "name";` (running `ConvertFontCode` on European languages to convert `[UNI00xx]` tags into single `0xXX` bytes) and ignores `MES_SYSSPE`. This table supplies the string returned by `GetItemMessage` (used for default owned weapon names and the invention and Georama menus). Each string is allocated from `gamedata_sysword_buffer_1073`, a fixed heap of `0x280` (640) 16-byte units (`mgCopyString` fails once the heap fills). The retail PAL languages use 479 to 583 units, while the unused Korean file uses 1,038 units.
- Menu display names and descriptions: Inventory menus and message item slots look up message ID `10000 + message` inside `meswin/system_N.mes` (`GetItemMessageNo` kind `1`, `msg_offsettbl_1363`, buffer limit `0xD000` bytes). Item help descriptions use that same message ID inside the menu pack's `allmenu.mes` (formatted with `[String1]` on the first line followed by the description text).

### 2.8 Limits, icons and models

- **Table and buffer limits**:
  - Up to **432 total items** (`local_com_itemdata`, 9 more than the retail 423), with item IDs in `1..511`.
  - Per-family table limits: **116 weapons** (+4 free), **38 attachments** (+2 free), **162 usable items** (+2 free), **68 Ridepod parts** (+5 free), **20 fish** (+2 free), and **40 clothes** (+6 free).
  - Each `.cfg` file in `menu/cfg7/` must fit in the **`0x77C0`-byte** buffer allocated by `LoadGameDataAnalyze`.
  - Item names in `comdatmes<lang>.cfg` must fit in `gamedata_sysword_buffer_1073` (`0x280` 16-byte units) and within the fixed buffers of owned item instances (`CopyDataWeapon` and related functions copy names without bounds checking: max **31 bytes** for weapons and Ridepod parts, **25 bytes** for attachments, **20 bytes** for fish, and **21 bytes** for roasted fish).
  - **Spectrumising limit (`item <= 425`)**: Any non-weapon, non-attachment item with an ID above `425` must **not** set attribute bit `0x2` (spectrumisable), because `SetItemSpectolPoint` indexes `etcitem_spectol_table[item - 1]` without bounds checking and that table has only 425 entries.
  - Every sellable item requires a `PRICE` entry in the shop files (see section 3).
- **Icons**: `icon_sheet` selects texture `0` (`wepicon`) or `1` (`itemicon`) inside `img/allitem.img` (and its duplicate copies inside menu `.pac` archives; the `@mb2`, `@pack01`, and `@trushsel` variants belong to unused packs). Each sheet is an 8×24 grid of 32×32-pixel cells (192 icons per sheet) sharing a single 256-colour palette. Unused cells on the retail disc:
  - `itemicon`: cells `52` and `191` (`191` contains the Japanese placeholder `予約`, "reserved").
  - `wepicon`: cells `52`, `56`, `110`, `150`, and `191` (`56` is the night-time icon for item `56`, Lamb's Sword, selected when `GetMenuItemIconTexGetXY` adds `1` during time band `2`; `110` and `150` hold numerals and symbols).
- **3D models**: `GetItemFilePath` resolves the model path as `mainchr/<model>.chr` for weapons and clothing, `dungeon/robo/<model>.chr` for Ridepod parts, and `item/<model>.chr` for all other item types. The menu model viewers (`MenuItemChrLoadEndCheck` and `CMenuInvent::LoadCharaCheck`) load the `.chr` pack without playing any animation, and none of the 168 `item/*.chr` packs on the disc contain `.mot` files.

## 3. Shops: `menu/shopusa<chapter>.cfg`

When an event script opens shop `n` (`_GOTO_MENU` with `MENU_OPEN_SHOP`), `CShopMenu::InitEnd` (`0x295FC0`) loads `menu/shopusa%d.cfg` for languages `1..6` (or `menu/shoplst%d.cfg` for Japanese, `0`), clamping the current chapter (`GetNowChapter`) to `1..8` and parsing with `menu_shop_tag` (`0x358640`):

| Statement | Target Array | Meaning |
|---|---|---|
| `PRICE item, buy, sell;` | `CShop::price[item]` (`s32` pairs, 512 slots, no bounds check) | Base buy price and sell price for `item`. |
| `SHOP shop, item_1, item_2, ...;` | `CShop::item_no[64]` (populated only when `shop` matches the opened shop ID) | List of item IDs sold by `shop`. |

Each of the 16 shop files contains 428 `PRICE` lines followed by 38 `SHOP` lines (each preceded by a `//` comment naming the shopkeeper). Across chapters 1 through 8, several shops expand their inventories and a few item prices change.

- **Shop currencies**: Determined by the shop ID:
  - Shops `23` and `28` (Cedric): Ridepod EXP (`ABS`).
  - Shop `32` (Mayor Need): Medals.
  - Shop `33` (Donny): Gilda, but its inventory is populated from `dony_shoplist` (`0x358620`) based on the player's Photography level rather than from `SHOP 33`.
  - All other shops: Gilda.
- **Dynamic inventory filtering**: After parsing the `SHOP` line, `CheckEventItem` (`0x295370`) removes unique story items that the player already owns or no longer needs, and upgrades Ridepod core items to the next tier.
- **Dynamic sell prices**: Selling a weapon adds `20 * level + 5 * remaining_synth_points` to its base sell price. Selling a gift box adds the sell prices of the items inside it.
- **Capacity limit**: Although `CShop::item_no` has 64 slots (`SHOP_ITEM_MAX`), a `SHOP` statement can list **at most 63 items** because the shop ID itself consumes 1 of the 64 arguments allowed by `CScriptInterpreter`. Shop files load into the menu's local memory stack.

## 4. Monsters: `base_monster_define` in `SCES_511.90`

The executable table `base_monster_define` at virtual address `0x341DA0` (file offset `0x241E20`) is an array of 343 `BASE_MONSTER_TBL` structs (`0xB8` bytes each, declared in `include/monster.hpp`) followed by a 344th terminator record with an empty name. `GetMonsterTable(id)` (`0x1DC230`) looks up records by `id`, and `SetActiveMonster` copies the record into each spawned monster instance. Monster AI scripts can subsequently override several fields at runtime (`_SET_DEF_RATE`, `_SET_GUARD_RATE`, `_SET_ESCAPE_RATE`, `_SET_EXT_PARAM_RATE`, and `_SET_MONSTER_LIFE`).

Used monster IDs span `0..243`, `245..280` (`245..268` are Mimics), `500..520`, `522..523`, `600..622`, `628..630`, `700..704`, `1000..1006`, and `1500..1501`.

| Offset | Field | Type | Meaning |
|---|---|---|---|
| `0x00` | `id` | `s16` | Unique monster ID (lookup key). |
| `0x02` | `grade` | `s16` | Monster rank/tier read by AI scripts. |
| `0x04` | `name` | `char[32]` | Internal name, overwritten at boot by `dungeon/cfg_file/mosdata<language>.cfg`. |
| `0x24` | `model` | `char[16]` | Model pack base name (`dungeon/monster/<model>.chr`). |
| `0x34` | `script` | `char[16]` | AI script base name (`dungeon/monster/<script>.stb`). |
| `0x44` | `gift_type` | `s16` | Row index into `gift_item_tbl` (`0x351660`, see section 9). |
| `0x48` | `sound_bank` | `s32` | Monster sound bank number (`snd2/mon/EN_%03d.snd`; `<= 0` = none). |
| `0x4C` | `unknown_4c` | `s32` | Copied to offset `+0x10E0` of the spawned monster; never read. |
| `0x50` | `hp` | `s32` | Base hit points. |
| `0x54` | `family` | `s8` | Monster family (`0..10`: Beast, Windup, Aquatic, ..., `9` = Undead); also determines which Monster Badge drops. |
| `0x56` | `abs` | `u16` | ABS dropped on defeat (`_SET_DEAD_OFF`). |
| `0x58` | `money` | `u16` | Gilda dropped on defeat (`_SET_DEAD_START`). |
| `0x5A` | `unknown_5a` | `s16` | Copied to offsets `+0x10EE` and `+0x1322` of the spawned monster; never read. |
| `0x5C` | `weapon_wear` | `f32` | WHP deducted from the player's melee weapon per hit. |
| `0x60` | `rage_hits` | `s16` | Number of hits taken before entering Rage mode (`-1` = never, used for bosses). |
| `0x62` | `guard_rate` | `s8` | Percentage chance (`0..100`) to guard against an attack. |
| `0x63` | `dodge_rates` | `s8[2]` | Percentage chances (`0..100`) to dodge melee and ranged attacks. |
| `0x66` | `attack` | `s16` | Base attack power. |
| `0x68` | `defence` | `u8` | Flat damage reduction subtracted from incoming attack power. |
| `0x69` | `stagger` | `s8` | Hit threshold required to stagger (`0` = flinches on every hit). |
| `0x6A` | `boss` | `s8` | Non-zero displays a full-screen boss health bar. |
| `0x6B` | `sword_after_images` | `s8` | Number of weapon trail after-image nodes on the model. |
| `0x6C` | `elements` | `s16[8]` | Damage multiplier (`%`) for each of the 8 weapon attributes (`Flame` through `Scale`; values `>= 50` appear as weaknesses in the Monster Book). |
| `0x7C` | `attack_kind_rates` | `s16[12]` | Damage multiplier (`%`) for each of the 12 attack categories (`vs_attk_index`, `0x351620`). |
| `0x94` | `reaction_flags` | `u32` | Hit reaction bitfield (`CheckDamage`): `0x1` = never staggers, `0x4` = never knocked down. |
| `0x98` | `flags` | `u32` | General flags: bit 0 (`0x1`) = cannot be picked up / thrown. |
| `0x9C` | `companion` | `s32` | Secondary monster ID loaded alongside this monster (`-1` = none). |
| `0xA0` | `drops` | `s16[2]` | Standard item drop IDs (drop 0 rolls at 1-in-6; if that misses, drop 1 rolls at 1-in-5). |
| `0xA4` | `rare_drop` | `s16` | Third item drop ID (rolls at 1-in-4). |
| `0xA8` | `status_immunity` | `u32` | Bitmask of status ailments the monster is immune to (`0x4` = Poison, `0x8` and `0x8000` = Petrify/Grey, `0x10000` = Slow). |
| `0xAC` | `status_rate` | `s16` | Susceptibility rate to status ailments. |
| `0xAE` | `percent_damage_scale` | `s16` | Percentage scaling factor applied to fractional-HP attacks. |
| `0xB0` | `book_area` | `s8` | Area category in the Monster Book. |
| `0xB2` | `book_index` | `s16` | Entry index in the Monster Book (`-1` = hidden from book). |
| `0xB4` | `shared_model` | `s16` | Non-zero causes multiple instances of this monster to share a single loaded model. |

Struct alignment padding bytes at offsets `0x46`, `0x55`, `0x65`, `0xA6`, `0xB1`, and `0xB6` are unused.

**Floor monster spawn tables (`dungeon/cfg_file/mos_place<dungeon>.cfg`)**:
`LoadMonsterFile` (`src/dng_event.cpp`, `0x293BB0`) parses the spawn list for the current dungeon floor:

```
FLS floor, count;
FL id_1, param_1, id_2, param_2, ...;
FLE;
```

- `FLS` selects the floor (its `count` argument is parsed and discarded).
- `FL` writes `(id, param)` pairs into `CMonsterLocateInfo` (**32 slots maximum**). `AutoSetMonster` spawns up to 24 monsters on a random floor, while event scripts can also load and place specific monsters on boss or fixed floors.
- The second integer of each pair (`param`) sets the spawned monster's `locate_param`, which acts as its dialogue index (`-1` = cannot be spoken to, checked by `CheckMonsterTolk`). Talking to a monster with `locate_param >= 0` triggers event `1150` (`IsEventRun`), which retrieves `locate_param` via `_GET_MONSTER_TALK_DATA`.
- **Hard 32-monster limit**: `_FL` (`0x292DB0`) does not bounds-check the 32-slot `id` array in `CMonsterLocateInfo`. A 33rd pair overflows `id[32]` and overwrites the start of the `locate_param[32]` array. In addition, a single `FL` statement can list at most 32 `(id, param)` pairs because `32 * 2 = 64` arguments hits `SPI_STACK_SIZE`.

## 5. Inventions, ideas and scoops

When the player takes a photograph, the subject ID (`USER_PICTURE_INFO.neta_id`) comes either from a map's `FUNC_INVENT_DATA` point (3,039 points across 121 `.map` files; see [maps](maps.md)) or from a monster AI script calling `_SET_SCOOP`. IDs below `1000` are **Ideas** (`[bulb2]`), while IDs `1000` and above are **Scoops**.

- **Invention recipes (`inv6.lst`)**: Stored in `menu/inv6.lst` and duplicated across 19 `.pac` archives (the invention menu reads the copy inside `menu/<lang>/inv2_bg.pac` or `photo_bg1.pac`, whereas event scripts read `menu/inv6.lst`). `CInventDataManage::LoadAnalyzeInventFile` (`0x201AC0`) parses:
  ```
  DATATABLESET n;
  DATASET item, idea_1, idea_2, idea_3, [material_item, material_count] × m, unknown, scale, x, y, z;
  ```
  into `INVENT_DATA_INFO` (`0x24` bytes: `s16 item`, `s16 ideas[3]`, up to 4 `(s16 item, u8 count)` crafting materials, `s16 material_count`, `s16 unk_10`, `f32 pos[3]`, and `f32 scale` for the preview model). The disc defines 136 recipes producing 128 distinct items, each requiring 1 to 3 materials (`_INVENT_DATASET` computes `m = (arg_count - 9) / 2` and ignores lines with `m < 1`; `HowMuchZairyouMakeItem` supports up to 4 materials). `CheckInventEnable` matches a recipe when all 3 chosen ideas match `ideas[3]`.
  **Recipe capacity limit**: `MenuInventInit` allocates `0x210` 16-byte units for `InventTeigiStack`, and `_INVENT_DATATABLESET` allocates `0x24` bytes per recipe, allowing **at most 234 recipes** (the event-side buffer `CheckItemTable` has `0x400` units = 454 recipes). Crafted item flags (`CInventUserData::created_item[0x100]`) are indexed by item ID.
- **Scoops (`scoop_table` and `scoop.cfg`)**: `scoop_table` (`0x352DE0`) holds 53 `SCOOP_DATA` records (`0x14` bytes: `s16 id`, `s16 menu_flag`, `s8 info_slot`, and a text pointer) paired with localized descriptions from `scoop.cfg` (`STR id, "text";` inside `menu/<lang>/scoop.pac`).
- Note that `invent.cfg`, `inv_com.cfg`, `item.cfg`, and `item_com.cfg` are UI layout and command scripts rather than item/invention tables.

### 5.1 Ideas: `menu/<lang>/neta2.lst`

`LoadFilePictureName` reads `menu/<LanguageCode>/neta2.lst` (`LoadFileMenu`: `0` = Japanese, `1` = English, `2` = French, `3` = German, `4` = Italian, `5` = Spanish; language `6`, Korean, falls back to `1/`, and `menu/neta2.lst` is an unused duplicate of `0/neta2.lst`) into a `0x5000`-byte read buffer. The file syntax is:

```
PIC_INFO n;
PIC_NAME id, "name", sort;
```

The retail files list 241 ideas (IDs `1..250`) followed by 53 scoops. Both the index table and the name strings share a single fixed heap, `pic_name_text_buff_1660`, of **`0x248` (584) 16-byte units**:
- `_PIC_INFO` allocates `ceil(n * 8 / 16) + 2` units for the array of 8-byte `PIC_NAME_INFO` records (`s16 id`, `s16 sort`, `char* name`).
- `_PIC_NAME` allocates `ceil((strlen(name) + 1) / 16)` units per name using the raw stored string length: because `ConvertFontCode` collapses `[UNIxxxx]` tags into 1 byte **only when `LanguageCode >= 5`** (Spanish, at assembly address `0x200FCC`), French (`2`), German (`3`), and Italian (`4`) retain the full 9-byte `[UNIxxxx]` sequences in `pic_name_text_buff_1660`.
- `mgCMemory::Alloc` refuses any allocation that reaches the final unit (`used + req >= 0x248`), leaving the name pointer `NULL`. On the PAL disc, `neta2.lst` consumes 491 (ja), 508 (en), 566 (fr), 533 (de), 539 (it), and 536 (es) units out of 583 usable units.

`_PIC_NAME` ignores ID `30000` (and map invent point `30001` photographs NPC `260` in `src/dng_main.cpp`), so neither is stored as an idea.

**Notebook sorting and display limits**:
`CMenuInvent::UpdataNetaMemoStr` collects the player's discovered IDs (`CInventUserData::neta_id[0x200]`) in file order and caches their sort keys in `sort_keys[0x184]`, limiting the combined total of ideas and scoops to **388**. Because it sorts only the first `known_ideas_count` entries by `sort_key` and leaves the remaining entries (scoops) in file order, all ideas (`id < 1000`) must precede all scoops (`id >= 1000`) in `neta2.lst`. `MenuInventNetaMemoDraw` formats `"[bulb2]"` plus the name into a 32-byte buffer `text[0x20]` (**24 bytes** available for the name, though several retail French, German, and Spanish idea names slightly overflow this buffer up to 33 bytes).

The `sort` value is `letter_group * 100 + rank_within_letter`:
- In English, French, German, and Italian, `A` is group `0` (folding accents, `Œ`, and `Æ`).
- In Spanish, `¡` is group `0` and `A` is group `1`.
- In Japanese, the group is the kana column/row index of the reading (`あ` = `0`, `い` = `1`, ..., `か` = `5`, ..., `や` = `35`, `ゆ` = `36`, `よ` = `37`, `ら` = `40`, ..., `わ` = `45`, Latin letters = `50`).

## 6. Georama

For each of the five Georama towns `gNN` (`g01` through `g05`) and language `0..6`:
- **Buildable parts (`map/g/gNN/gNN<lang>.gpi`)**: Parsed by `CEditInfoMngr::LoadEditInfo` using tag table `emap_tag__2` (`0x358A30`). Begins with `EDIT_PARTS_NUM n;`, followed by `EDIT_PARTS "name"; ... EDIT_PARTS_END;` blocks that populate `CEditPartsInfo` (`0x280` bytes each):
  - `ID`: Global part ID (`0..127`).
  - `PARTS_NAME`: Name of the 3D map part inside the town's `.map` file.
  - `PARTS_ATR` and `GROUND_PARTS` / `BLOCK_PARTS` / `RIVER_PARTS` / `FENCE_PARTS`: Placement category flags.
  - Up to four `PARTS_MATERIAL item, count;` lines.
  - `CPOINT category, points;`: Culture points contributed to the town. (The Japanese `.gpi` files differ from the English/PAL files in the culture points of 3 parts.)
  - `WEIGHT`, `GEO_STONE` (the geostone ID that unlocks the recipe), `MAX_NUM`, `PAINT_NUM`, `PAINT_USED`, `PARTS_TYPE`, `PLACE_EPS`, `MAP_NO`, `POLYN`, and `PARTS_COMMENT`.
- **Fixed and starting town layout (`map/g/gNN/gNN.cfg`)**: Parsed by `CEditMap::LoadEditInfo` (`emap_tag`, `0x33ABB0`). Defines pre-existing obstacles and initial parts via `FIX_EPARTS` and `INIT_EPARTS id, x, y, z, angle;`, plus river piece models (see section 6.2).

**Georama limits and shared IDs**:
- **384 parts per town** (`GEORAMA_PARTS_LIST_MAX` bounds the placed, stock, and crafting lists, and `MakeDownLoadAnaunce` iterates over `0..0x17F`).
- **Part IDs `0..127`**: `CMenuGeorama::UpdateGeoramaPartsList` loops over IDs `0..0x7F` (while `CSaveData::build_parts_num[0x100]` and `RemoveInfo.parts_num[256]` index by ID). Because inventory stock counts are stored globally by part ID across all five towns, two towns that define a part with the same `ID` share that part's stock (for example, `ID 1` is shared across four towns).
- **Name and file size limits**: Part names are copied via `strcpy` into `GEORAMA_PARTS_LIST_ITEM::name[0x30]` (**47 bytes max**). For town `g01`, `MakeDownLoadAnaunce` reads `g01%d.gpi` into a `0x7800`-byte stack buffer, limiting `g01<lang>.gpi` to **`0x77D0` bytes** (the largest on the disc is Korean at 27,202 bytes).
- **Map part linkage and placed count**: Every `PARTS_NAME` must match a valid part in the town's `.map` (`CMap::GetParts`; otherwise the placed object has no collision). Ground parts (`attribute & 7`) form one of the town's 4 terrain grids. A town can hold at most **256 placed parts** (`CEditMap::CreateTable`). `FIX_EPARTS_START` and `INIT_EPARTS_START` allocate exact arrays for the declared count and ignore any excess lines.
- **Hardcoded part IDs**: C++ evaluation code checks specific part IDs directly: `7`, `0x13`, `0x1D`, `0x23`, `0x28..0x2B`, and `0x2F` in Georama condition checks, plus house IDs `1`, `9`, `0x16`, `0x1F`, `0x38`, `0x46..0x48`, `0x56`, and `0x57`.

### 6.1 Requests: `geo<language>.cfg`

At boot (`MainLoop`) and whenever the language changes, `LoadEditAnalyzeData` (`src/editdata.cpp`) reads `geo%d.cfg` from the root of `DATA.DAT` using `tag__6` (`0x358B20`). Each file defines all five towns (`0..4`) and populates `AnalyzeSrc[5]` (`0x340` bytes per town: `condition[64]` strings, `geo_floor[64]`, and `data[16]` request structs of `0x1C` bytes each):

| Statement | Meaning |
|---|---|
| `GEO_ANALYZE town;` | Selects and clears town `0..4` for subsequent statements. |
| `CONDITION number, "text"[, geostone];` | Sets the display label for condition `0..63` and the dungeon floor whose geostone unlocks it (`dungeon * 100 + floor`). |
| `ANALYZE request, unread, "title"[, geostone];` | Defines request `0..15` (`GetAnalyzeDataSrc` stops at the first unpopulated slot). The second argument is stored but never read. |
| `CON_NO cond_1, ...;` | Lists up to **7** condition numbers (`0..63`) required to complete the current request. |
| `ON_PARTS "parts";` / `OFF_PARTS "parts";` | Semicolon-separated list of up to 16 map parts (`"part"` or `"part/piece"`, 64-byte buffer) shown or hidden when the request is fulfilled (`GetOnOffParts`, `PartsOnOff`). |
| `PERCENT points;` | Completion percentage added to the town when the request is fulfilled. |
| `END_ANALYZE;` / `END_GEO_ANALYZE;` | Closes the current request or town block. |

All condition and request strings across all five towns are allocated from a single shared buffer, `buff_1271`, of **`0x3000` bytes** (`0x300` 16-byte units).

**Condition evaluation is hardcoded**: When evaluating town progress, `AnalyzeEditMap` calls `AnalyzeSharlot`, `AnalyzeStera`, `AnalyzeBenietio`, `AnalyzeHeim`, or `AnalyzeMoonFlower` (`src/editanalyze.cpp`). Those C++ functions check hardcoded part IDs, placement distances, river counts, resident IDs, and culture point thresholds to set the boolean flags for `condition[0..63]`, after which `CEditData::Analyze` marks a request complete if all of its `CON_NO` conditions are true. Therefore, `.cfg` edits can recombine existing town conditions (`0..17` in town 0) or change `ON_PARTS`, `OFF_PARTS`, and `PERCENT`, but cannot invent new condition logic without modifying code. The five towns have 9, 10, 9, 11, and 10 requests in the retail game.

### 6.2 Rivers and water: `gNN.cfg`

- **River pieces**: Inside `gNN.cfg` (between the no-op markers `EDIT_RIVER "name";` and `EDIT_RIVER_END;`), `CEditMap::LoadEditInfo` (`emap_tag`, `0x33ABB0`) parses:
  - `RIVER_PARTS_NAME slot, "map_part", "model.mds";` (populates `CEditPartsInfo[8]` for slots `0..7` with the map part replaced by the river piece and its `.mds` visual)
  - `WATER_PARTS_NAME "model.mds";`
  - `MASK_PARTS_NAME 0, "model.mds";` (slot `0` only)
  Towns `g01` and `g05` use river pieces.
- **Animated water surfaces**: `CMap::LoadCfgFile` (`cfg_tag`, `0x338BF0`) parses:
  - `WATER_SURFACE_NUM n;` (allocates `n` surfaces)
  - `WATER_SURFACE_START;` .. `WATER_SURFACE_END;` blocks containing `WATER_VERTEX nx, nz, x0, y0, z0, x1, y1, z1;` (grid subdivisions and bounding corners), `WATER_POS x, y, z;`, `WATER_PARAM a, b, c, d;` (wave parameters), and `WATER_SHAKE` (ignored)
  - `WATER_DRAW_NUM n;` followed by `WATER_DRAW surface, "part_or_#followN", x, y, z;`

## 7. Fishing

- **Fish species (`FishParam`)**: Located at `0x361A50` in `SCES_511.90` (`FISH_PARAM` struct, `0x54` bytes × 19 rows; row `0` is empty). Each record stores name and model pointers, the corresponding inventory item ID, base/min/max length (`cm`), weight and Finny Frenzy points per cm, rod pull strength, an `s16[18]` array of bait preferences (`0` = never bites to `3` = favourite, indexed by `EsaInfo` at `0x361A00`), and an `s16[4]` array of time-of-day preferences.
- **Fishing spots (`sg/fish/fishing.cfg`)**: Parsed by `LoadFishPlaceData` into the 1 MB scene read buffer:
  ```
  FISH_MAP_NUM n;
  FISH_MAP map[, exclusive];
      FISH_PLACE kind, "name", f1, f2, f3, f4, f5;
      FISH species_idx, rate, wait_bias;   // Up to 8 per spot
  FISH_MAP_END;
  ```
  `FISH_MAP_NUM n;` allocates `n` `FISH_PLACE_MAP` records (`0x88` bytes each) on the scene stack and ignores any blocks beyond `n`. Each spot holds at most **8 fish entries** (`kFishPlaceMaxFish`, with no bounds check in `FISH`; `species_idx` indexes `FishParam[0..18]`). Each `FISH_MAP` block supports only **one** `FISH_PLACE` area (a second `FISH_PLACE` in the same block overwrites the first, so maps with multiple fishing zones define multiple `FISH_MAP` blocks for the same `map` ID). Only area `kind = 2` (a horizontal circle defined by `x, z, radius`) performs a spatial bounds check; other area kinds match anywhere on the map. When the player casts a line, `GetAppearFish` collects up to 16 fish entries from all matching spots on the current map (if an `exclusive` spot matches, it replaces all non-exclusive spots; if no map matches, it falls back to map `-1`). The retail file defines 22 spot blocks with 110 fish entries.
- **Finny Frenzy (fish racing)**: Base species racing scaling and terrain affinity live in `fish_data` (`0x3630C0`, `grFISH_DATA` of `0x1C` bytes × 18). AI opponent fish live in `sg/gyo/rfd<lang>.cfg` (4 classes, 216 opponents total):
  ```
  NUM class, count;
  DAT item, "name", colour, power, speed_1, speed_2, speed_3, stamina, size;
  ```
  where `colour` sets the fish's `color_no` palette variant.
- **Contest and race prizes (`gyop.cfg` and `uofp.cfg`)**: Parsed by `LoadFishPrize` for the Finny Frenzy (`gyop.cfg`, 12 groups) and Fishing Contest (`uofp.cfg`, 10 groups):
  ```
  GRP count, values...;
  PRIZE value, item_1st, count_1st, item_2nd, count_2nd, item_3rd, count_3rd;
  ```
  `RefreshFishPrize` selects a group index (for Finny Frenzy class `i`, `3 * i + min(2, class_wins)`; for the Fishing Contest, the current contest number) and always reads **only the first `PRIZE` row** of that group (ignoring `GRP`'s extra values and `PRIZE`'s leading `value`).
- **Aquarium breeding (`aquafish_mixTable`)**: Located at `0x3535A0` (`s8[171][3]`, see section 9).

## 8. Floor medals: `menu/dngmap/dflr<dungeon>.cfg`

`CDngFloorManager::LoadDataTable` (`src/dngfloor.cpp`) loads `menu/dngmap/dmap<dungeon>.cfg` (the dungeon map node layout) followed by `menu/dngmap/dflr<dungeon>.cfg` using tag table `tree_map_tag` (`0x3617B0`). Dungeons `0..6` contain 9, 16, 25, 21, 23, 28, and 39 floor entries.

- **`RF_INFO` (`_ROOM_FLOOR_INFO`)**:
  ```
  RF_INFO floor, geostone, fast_time, fishing, fish_record, seal, spheda, item_1, count_1, item_2, count_2, item_3, count_3;
  ```
  Populates `DNGMAP_ROOM_INFO`:
  - `geostone` (`s8`): Whether the floor has a geostone.
  - `fast_time` (`s32`): Target speed-run clear time in 60 Hz frames (`1/60` s).
  - `fishing` (`s8`): Fishing medal type (`< 0` means the caught fish must be `<= fish_record`; `> 0` means `>= fish_record`).
  - `fish_record` (`s16`): Target fish length in `cm * 100`.
  - `seal` (`s8`): Floor character seal (`1` = requires Monica to break, `2` = requires Max).
  - `spheda` (`s8`): Spheda medal availability.
  - `item_1..3` (`s16`) and `count_1..3` (`s8`): Spheda completion prize items and quantities.
- **`RF_INFO2` (`_ROOM_FLOOR_INFO2`)**:
  ```
  RF_INFO2 floor, type, param;
  ```
  Sets the floor's item/practice medal condition (`s8 type`, `s32 param`), checked by `IsClearPractice` against the floor's kill-method bitmask (`type 0` = time limit, `1..4` = defeat all monsters using a specific attack/character category, `5` = clear without healing).

## 9. Characters and other executable tables

All tables below reside in the `.data` section of `SCES_511.90` at fixed sizes:

| Symbol | Virtual Address | File Offset | Layout | Purpose |
|---|---|---|---|---|
| `lifetbl_2854` | `0x37C790` | `0x27C810` | `f32[2]` | Starting and maximum HP for Max and Monica (`CUserDataManager::Initialize`). |
| `mos_henge_param` | `0x339D70` | `0x239DF0` | `0x1C × 57` (`s16 monster_id`, `s16 attack`, `s16 defence`, effect names) | Attack, defence, and effect parameters for Monica's 57 Monster Badge forms (`GetMonsterHengeParam`). |
| `monster_progress_tbl` | `0x358C90` | `0x258D10` | `s16[19][5]` | Monster Badge evolution paths (badge ID followed by the monster ID at each of its 4 evolution levels). |
| `htbl_1662` | `0x33A490` | `0x23A510` | `s8[10]` | Random pool of 10 WHP increments gained when a weapon levels up. |
| `weptbl_4503` | `0x33A510` | `0x23A590` | `s16[2][2][5]` | Starting equipment item IDs indexed by `[language != 0][chara][slot]` (`GetCharaDefaultWeapon`). |
| `etcitem_spectol_table` | `0x339820` | `0x2398A0` | `s8 { slot, value } × 425` | Spectrumising bonus for item IDs `1..425` (`SetItemSpectolPoint`). `slot`: `0..7` = Flame..Scale, `10` = Attack, `11` = Durability, other = none. Bonus equals `value * item_count_or_level`. |
| `dony_shoplist` | `0x358620` | `0x2586A0` | `{ s16 item, s8 level } × 7` (plus terminator with `item <= 0`) | Items unlocked in Donny's shop (`GetDonyShopLineUp`) when Donny's photography level exceeds `level`. |
| `gift_item_tbl` | `0x351660` | `0x2516E0` | `s16[3] × 14` (plus `-1` terminator row) | Preferred gift items for unlocking Monster Badges (see below). |
| `GolfClubDef` | `0x361340` | `0x2613C0` | `0xC × 7` (`{ f32 power, f32, s32 }`) | Spheda club stats (see below). |
| `aquafish_mixTable` | `0x3535A0` | `0x253620` | `s8[171][3]` | Aquarium fish breeding combinations (see below). |

**Additional table details**:
- **`gift_item_tbl` (`0x351660`, file offset `0x2516E0`)**: Contains 14 rows of `s16 { gift_type, item_id, unread }` terminated by a row with `gift_type = -1`. When the player throws a Gift Capsule at a monster, `CheckGiftPack` (`src/monster.cpp`, `0x1DF520`) looks up the monster's `gift_type` (section 4) and checks whether **all three** items inside the capsule (`_COLPRIM_GET_GIFT`) match `item_id`. When they match, `CMonsterMan::CheckDamage` displays the heart/gift indicator. The third `s16` column is never read.
- **`GolfClubDef` (`0x361340`, file offset `0x2613C0`)**: Stores `{ f32 power, f32 unk_4, s32 unk_8 }` for 7 clubs. `GetSphidaClubDef` indexes `club_id - 9` for clubs `9..14`, leaving the 7th row (all zeros) unused. `CSphida::DrawStatusSprite` displays `power`, and `_SPHIDA_GET_CULB_DEF` exposes all three fields to event scripts.
- **`aquafish_mixTable` (`0x3535A0`, file offset `0x253620`)**: Stores 171 rows of `s8 { parent_a, parent_b, child }`, where each value is `fish_item_id - 0x136` (fish item IDs span `0x136..0x150`). When two fish breed in the aquarium (`CAquarium::CombineFish`), `GetChildFishNo` (`src/menuaqua.cpp`, `0x20ED80`) scans all 171 rows matching the two parents in either order and returns `child + 0x136` (or `0x136` if no row matches).

## 10. Townspeople: `npc<language>.cfg`

At boot (`MainLoop`) and on language changes, `LoadNPCCfg` (`src/npccfg.cpp`) reads `npc%d.cfg` from the disc root into a 64-byte-aligned `0x8000`-byte buffer (`0x7FD0` usable bytes) and parses it with `npc_spitag`:

```
NPC_NUM n;
NPC_INFO number, "name", "model", kind, abilities, points, cost_1, cost_2, cost_3, cost_4, debug;
```

Each `NPC_INFO` statement populates the next entry in `NpcBaseData[180]` (`NPC_BASE_DATA`, `0x36` bytes):

| Argument | Struct Field | Meaning |
|---|---|---|
| `number` | `s16 chara_no` | Unique townsperson ID (lookup key). |
| `"name"` | `char name[0x1C]` | Localized display name (**27 bytes max**; `strcpy` runs before the `"NAME OVER"` length check). |
| `"model"` | `char model[0x10]` | Model base name (**15 bytes max**), used to form `chara/%s.chr`, `event/train/t%s.chr`, and `menu/npc/t%s.chr`. |
| `kind` | `u8 unk_31` | Message category offset returned by `_GET_PARTY_CHARA_MES_NO` for query type `4`. |
| `abilities` | `u8 ability_num` | Number of support abilities shown in the party menu (`0..4`). |
| `points` | `u8 max_npc_point` | Maximum ability/stamina points (`Kizuna` / Recruitment points). |
| `cost_1..4` | `u8 ability_cost[4]` | Point cost of each of the 4 abilities. |
| `debug` | `u8 debug_flag` | Selection flag used only in the debug menu. |

The counter `npc_spi_count_num` (`u8`) increments without bounds checking and overwrites the count from `NPC_NUM`. Because `NpcBaseData` holds **180 records**, at most 3 new townspeople can be added beyond the retail 177. However, **only NPCs with `number` in `1..32` can join the party or move into a Georama house** (`CUserDataManager::party_chara[32]`, `JoinPartyChara`). A townsperson's dialogue message IDs in `npcmsg.mes` follow `typetbl[type] + number * 100` (`GetPartyCharaMessage`). Standing positions in towns come from `npc_place*.cfg` (`VlgrPlace[0x200]`).
