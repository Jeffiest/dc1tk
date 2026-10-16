# How scripts run

This page documents how the engine executes scripts at runtime: script programs, event mode, shared state, language variants, automatic dungeon events, and message text control codes. Each rule cites the corresponding engine source file in the [ChronicleTwo](https://github.com/TheMoonPeople/ChronicleTwo) decompilation. For the `.stb` bytecode format and VM instruction set, see [scripts](scripts.md). For instructions on creating mods and editing scripts, see [mods](../guide/mods.md) and [editing scripts](../guide/mods.md).

## Programs

In a map script, `PROGRAM` declarations define the event handlers that the engine can start: `map_start` when a town map loads, `floor_start` when a dungeon floor loads, `monsters_cleared` (`1500`) when no monsters remain on a floor, numbered story programs triggered by warps, and others (`enum program` in `sdk/event.h`). In a monster script, `PROGRAM` declarations define the monster's state handlers: `init`, `update`, `damage`, and `death` (`enum program` in `sdk/monster.h`).

Programs execute as coroutines. The `wait;` statement yields execution for the rest of the current frame, and the program resumes from that point on the next frame. The PAL game runs at 50 frames per second, which is why PAL scripts scale 60 Hz frame counts by `50 / 60`.

## While an event runs, the player waits

In a dungeon, starting a map program puts the dungeon into event mode (`EventScriptSetup`, `dng_main.cpp`) until the program finishes. The engine processes player movement input from the controller only outside event mode (`DngMainKey`). Consequently, a map event script cannot keep a background loop running alongside normal gameplay, because the loop holds the player stationary in event mode.

## An event that never waits costs nothing

If a program reaches its end without executing a `wait;` instruction, the VM's `run` function returns `0` (`runscript.cpp`), and `EventScriptSetup` never enters event mode. The entire program completes within a single frame, with no camera transition and no lost controller input. A program that contains no `wait;` on any execution path acts as a synchronous function that the dungeon can call during normal gameplay.

## Message window 0 stays on screen

The dungeon updates and draws event message window `0` on every frame, both inside and outside event mode (`DngMainDraw`, `dng_main.cpp`). Ending an event does not close window `0` (`EdEventEnd` unloads only the loaded message file). Therefore, a message window opened by an event stays on screen during gameplay until a script explicitly closes it with `close_message`. Calling `show_message(0, "text")` formats and displays literal text immediately. Calling `set_message_prompt(0, 0)` hides the button prompt for persistent text that the player should not dismiss.

## Monsters can start map events

Calling `run_event_script(n)` in a monster script requests that the dungeon run program `n` of the currently loaded map script. The dungeon processes this request on its next event check (`IsEventRun`, `dng_main.cpp`; `CMonsterMan::IsRunEvent`, `monster.cpp`), provided the player is alive and standing on the ground. Event scripts can queue a dungeon event in the same way with `queue_dungeon_event`.

Several story fights end through this mechanism. For example, the castle's Griffon Soldiers count their deaths in a shared monster variable, and the fourth soldier runs event `510`. If a mod replaces those monsters with other monsters, the fight never ends unless the map script also defines a `monsters_cleared` program.

## Shared state

- **The dungeon frame timer** counts every frame during both gameplay and events (`DngStep`). Monster scripts read and reset the timer with `get_floor_timer` and `reset_floor_timer`, and event scripts reset it with `reset_dungeon_timer`. Each floor has one timer shared by all monsters, so multiple monsters can coordinate actions using a shared clock. A floor's timed medal challenge also reads this timer (`dngfloor.cpp`).
- **Monster variables** `0` to `7` belong to each individual monster. Variables `8` to `135` are shared by all monsters on the floor (`set_monster_var`, `get_monster_var`).
- **Save flags** consist of 2,048 bits in the save data (`savedata.cpp`). Event scripts access them with `get_flag` and `set_flag`, and monster scripts access the same bits with `get_dungeon_flag` and `set_dungeon_flag`. No script on the PAL disc and no code in the engine uses a flag above `824`. Because flags persist in the save file, scripts should clear any temporary scene flag when the scene ends and on game over.
- **Save counters** consist of 128 signed 16-bit integers (`-32768` to `32767`) in the save data (`short_flag` in `savedata.cpp`). Event scripts read and write them with `get_counter` and `set_counter`. The scripts on the disc use counters `0` to `20` except `6`. The engine reads counter `5` in the main menu and stores the fishing tournament's entry count and win counts in counters `21` to `25`.
- **Script globals** retain their values between program runs for as long as the map's script remains loaded. The engine zeroes all script globals when the script loads (`CRunScript::load`).

A mod does not assign a flag, counter, shared monster variable, or program number manually. Instead, the mod requests one by name, and the build tool allocates an unused number (see [numbers of a mod's own](../guide/mods.md#numbers-of-a-mods-own)).

## Language variants

Map scripts have one file per language, from `s51_1.c` (English) to `s51_5.c`. The engine loads the variant that matches the console's language (`FileNameConvLanguage`, `event_func.cpp`; `EntryEventScript`, `dng_main.cpp`). A file without a language suffix (`s51.c`) is the original Japanese script, which the PAL game never loads when a numbered variant exists. For instructions on editing language variants, see [editing scripts](../guide/mods.md#text-in-each-language).

## Message text

Text passed to `show_message` (as well as `modkit_say` and `modkit_text_show`) can contain the following inline control codes, which the engine's message layout system parses (`nd_meswin.cpp`, `font.cpp`):

| Code | Shows |
| --- | --- |
| `\n` | Line break |
| `<page>` | Page break; the window waits for a button press before displaying the remaining text |
| `[Number1]` ... `[Number16]` | Integer value set with `set_message_value(window, slot, value)` *before* `show_message` is called. `show_message` formats numbers immediately, so setting a value afterward displays `0` (verified in-game). Supports `0` and negative numbers |
| `[文字列１]` ... `[文字列１６]` | String set with `set_message_string(window, slot, text)`; the slot number uses full-width digits |
| `[アイテム１]` ... | Item name set with `set_message_item(window, slot, item)` |
| `(O)` `(X)` `(A)` `(#)` | Circle, Cross, Triangle, and Square button icons |
| `[l1]` `[r1]` `[l2]` `[r2]` `[start]` `[select]` | Shoulder, Start, and Select button icons |
| `(L)` `(R)` `(L3)` `(R3)` | Left and right analog stick icons, and L3/R3 press icons |
| `[+]` `[-]` `[|]` `[!]` | Directional pad (D-pad) icons |
| `[heart]` `[hp]` `[mp]` `[lv]` `[fish]` `[weapon]` ... | Additional icons from the game's font table (`font.cpp`) |

`[Number]` is the only tag that supports an English spelling (`[数値１]` also works). The string and item substitution tags must use their Japanese spellings in all languages. The `(A)` and `(#)` tags map to the Triangle and Square buttons, matching the glyph order in the font table (Circle, Triangle, Cross, Square).

The game's standard hint windows display four lines. Keep live-updating text to four lines or fewer so that it does not overflow the window.

Menu text (item, shop, and map names, and system messages) is stored in `.mes` message files where these tags are pre-compiled into binary control codes (see [text](text.md)). Mods edit `.mes` files using the same text tags (see [editing text](../guide/text.md)).

## Dungeon events

In addition to programs started by warps and scripts, the dungeon loop automatically starts the following programs in the loaded map script (`IsEventRun`, `dng_main.cpp`) while the player is alive and on the ground:

| Program | When |
| --- | --- |
| `floor_start` (1010) | A dungeon floor or dungeon-style map loads without a specific target program. A new game starts here, in dungeon 1 |
| `treasure_chest` (1100) | The player presses the action button at a treasure chest. The chest remains closed unless the program opens it (as the standard dungeon scripts' `open_treasure_chest` does), so a mod can use a chest as a reusable switch, as in the [calculator](../scripting/examples/calculator/calculator.c) mod |
| `monster_talk` (1150) | The player presses the action button next to a monster placed as talkable, on maps whose dungeon database entry enables monster talking (`dbinfo.monster_talk`) |
| `monsters_cleared` (1500) | No monsters remain alive on the floor; runs once per floor |
| a monster's `run_event_script(n)` | Program `n`, requested by a monster script |
| a trigger point's program | The player touches the trigger point, presses the action button at it, or uses an item there |
