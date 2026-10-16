# How scripts run

This page documents how the engine executes scripts at runtime: script programs, event mode, shared state, language variants, automatic dungeon events, and message text control codes. Each rule cites the corresponding engine source file in the [ChronicleTwo](https://github.com/TheMoonPeople/ChronicleTwo) decompilation.

- For the `.stb` bytecode format and VM instruction set, see [scripts](scripts.md).
- For instructions on creating mods and editing scripts, see [mods](../guide/mods.md).

## Programs

`PROGRAM` declarations define the numbered entry points that the engine starts when events occur:

| Script Category | SDK Header | Example Programs |
|---|---|---|
| Map / event script | `enum program` in `sdk/event.h` | `map_start` (town load), `floor_start` (dungeon floor load), `monsters_cleared` (`1500`, when no monsters remain on a floor), and numbered story programs triggered by warps |
| Monster script | `enum program` in `sdk/monster.h` | `init`, `update`, `damage`, and `death` |

- **Coroutine execution**: Programs execute as coroutines. The `wait;` statement yields execution for the rest of the current frame, and the program resumes from that point on the next frame.
- **PAL frame rate**: The PAL game runs at 50 frames per second, so PAL scripts scale 60 Hz frame counts by `50 / 60`.

## While an event runs, the player waits

- Starting a map program in a dungeon puts the dungeon into event mode (`EventScriptSetup`, `dng_main.cpp`) until the program finishes.
- The engine processes player movement input from the controller only outside event mode (`DngMainKey`).
- Consequently, a map event script cannot keep a background loop running alongside normal gameplay, because the loop holds the player stationary in event mode.

## An event that never waits costs nothing

- If a program reaches its end without executing a `wait;` instruction, the VM's `run` function returns `0` (`runscript.cpp`), and `EventScriptSetup` never enters event mode.
- The entire program completes within a single frame, with no camera transition and no lost controller input.
- A program with no `wait;` on any execution path acts as a synchronous function that the dungeon can call during normal gameplay.

## Message window 0 stays on screen

- The dungeon updates and draws event message window `0` on every frame, both inside and outside event mode (`DngMainDraw`, `dng_main.cpp`).
- Ending an event does not close window `0` (`EdEventEnd` unloads only the loaded message file). Window `0` stays on screen during gameplay until a script closes it with `close_message`.
- Calling `show_message(0, "text")` formats and displays literal text immediately.
- Calling `set_message_prompt(0, 0)` hides the button prompt for persistent text that the player should not dismiss.

## Monsters can start map events

- Calling `run_event_script(n)` in a monster script requests that the dungeon run program `n` of the currently loaded map script.
- The dungeon processes this request on its next event check (`IsEventRun`, `dng_main.cpp`; `CMonsterMan::IsRunEvent`, `monster.cpp`), provided the player is alive and standing on the ground.
- Event scripts can queue a dungeon event in the same way with `queue_dungeon_event`.
- **Story fight progression**: Several story fights end through this mechanism. For example, the castle's Griffon Soldiers count their deaths in a shared monster variable, and the fourth soldier runs event `510`. If a mod replaces those monsters with other monsters, the fight never ends unless the map script also defines a `monsters_cleared` program.

## Shared state

| State | Capacity / Scope | Event Script API | Monster Script API | Engine and Save Behavior |
|---|---|---|---|---|
| Dungeon frame timer | 1 timer per floor | `reset_dungeon_timer` | `get_floor_timer`, `reset_floor_timer` | Counts every frame during both gameplay and events (`DngStep`). Shared by all monsters on the floor and read by the floor's timed medal challenge (`dngfloor.cpp`). |
| Monster variables | `0`–`7` per monster; `8`–`135` shared across the floor | — | `get_monster_var`, `set_monster_var` | Variables `0`–`7` belong to each individual monster. Variables `8`–`135` are shared by all monsters on the floor. |
| Save flags | 2,048 bits (`0`–`2047`) | `get_flag`, `set_flag` | `get_dungeon_flag`, `set_dungeon_flag` | Stored in save data (`savedata.cpp`). No retail script or engine code uses a flag above `824`. Scripts should clear temporary scene flags when the scene ends and on game over. |
| Save counters | 128 signed 16-bit integers (`-32768` to `32767`) | `get_counter`, `set_counter` | — | Stored in `short_flag` (`savedata.cpp`). Retail scripts use `0`–`20` except `6`. The engine reads counter `5` in the main menu and stores fishing tournament entry and win counts in counters `21`–`25`. |
| Script globals | `global_num` slots per loaded script | Direct variable access | Direct variable access | Retain values between program runs while the script remains loaded. Zeroed when the script loads (`CRunScript::load`). |

A mod does not assign a flag, counter, shared monster variable, or program number manually. Instead, the mod requests one by name, and the build tool allocates an unused number (see [numbers of a mod's own](../guide/mods.md#numbers-of-a-mods-own)).

## Language variants

- Map scripts have one file per language, from `s51_1.c` (English) to `s51_5.c`.
- The engine loads the variant that matches the console's language (`FileNameConvLanguage`, `event_func.cpp`; `EntryEventScript`, `dng_main.cpp`).
- A file without a language suffix (`s51.c`) is the original Japanese script, which the PAL game never loads when a numbered variant exists.
- For instructions on editing language variants, see [editing scripts](../guide/mods.md#text-in-each-language).

## Message text

Text passed to `show_message` (as well as `modkit_say` and `modkit_text_show`) can contain inline control codes parsed by the engine's message layout system (`nd_meswin.cpp`, `font.cpp`):

| Code | Output |
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

- **Tag spellings**: `[Number]` is the only tag that supports an English spelling (`[数値１]` also works). The string and item substitution tags must use their Japanese spellings in all languages.
- **Button glyph order**: The `(A)` and `(#)` tags map to the Triangle and Square buttons, matching the glyph order in the font table (Circle, Triangle, Cross, Square).
- **Line limit**: Standard hint windows display four lines. Keep live-updating text to four lines or fewer so that it does not overflow the window.
- **Compiled `.mes` files**: Menu text (item, shop, and map names, and system messages) is stored in `.mes` message files where these tags are pre-compiled into binary control codes (see [text](text.md)). Mods edit `.mes` files using the same text tags (see [editing text](../guide/text.md)).

## Dungeon events

In addition to programs started by warps and scripts, the dungeon loop automatically starts the following programs in the loaded map script (`IsEventRun`, `dng_main.cpp`) while the player is alive and on the ground:

| Program | Trigger Condition |
| --- | --- |
| `floor_start` (1010) | A dungeon floor or dungeon-style map loads without a specific target program. A new game starts here, in dungeon 1 |
| `treasure_chest` (1100) | The player presses the action button at a treasure chest. The chest remains closed unless the program opens it (as `open_treasure_chest` does in standard dungeon scripts), so a mod can use a chest as a reusable switch, as in the [calculator](../scripting/examples/calculator/calculator.c) mod |
| `monster_talk` (1150) | The player presses the action button next to a monster placed as talkable, on maps whose dungeon database entry enables monster talking (`dbinfo.monster_talk`) |
| `monsters_cleared` (1500) | No monsters remain alive on the floor; runs once per floor |
| `run_event_script(n)` | Program `n`, requested by a monster script |
| Trigger point program | The player touches the trigger point, presses the action button at it, or uses an item there |
