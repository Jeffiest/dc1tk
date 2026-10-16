# How scripts run

What a mod needs to know about how the game runs its scripts: programs, event mode, the state
scripts share, language variants, the events the dungeon starts by itself, and the codes message
text can hold. Each rule names the engine code in the
[ChronicleTwo](https://github.com/TheMoonPeople/ChronicleTwo) decomp it comes from. The script
files and the VM are in [scripts](scripts.md); making a mod that uses these rules is in
[mods](../modding/mods.md) and [editing scripts](../modding/scripts.md).

## Programs

A map script's `PROGRAM` lines are the events the engine can start in it:
`map_start` when a town map loads, `floor_start` when a dungeon floor does,
`monsters_cleared` (1500) once no monster is left on a floor, numbered story
programs that warps name, and so on (`enum program` in `sdk/event.h`). A
monster script's `PROGRAM` lines are its states: `init`, `update`, `damage`,
`death` (`enum program` in `sdk/monster.h`).

Programs run as coroutines: `wait;` gives up the rest of the frame and the
program carries on from there on the next one. The PAL game runs 50 frames a
second, which is why the scripts scale frame counts by `50 / 60`.

## While an event runs, the player waits

In a dungeon, starting a map program puts the dungeon in event mode
(`EventScriptSetup`, `dng_main.cpp`) until the program ends, and the pad only
moves the player outside event mode (`DngMainKey`). So an event script cannot
keep a loop running beside play: the loop would hold the player still.

## An event that never waits costs nothing

If a program reaches its end without a `wait;`, the VM's `run` returns 0
(`runscript.cpp`) and `EventScriptSetup` never enters event mode. The whole
program runs inside one frame, with no camera switch and no lost input. A
program with no `wait;` on any path is in effect a function the dungeon can
call during play.

## Message window 0 stays on screen

The dungeon steps and draws event message window 0 every frame, in event mode
or not (`DngMainDraw`, `dng_main.cpp`), and ending an event does not close it
(`EdEventEnd` only drops the loaded message file). So a window opened by an
event stays up during play until a script closes it with `close_message`.
`show_message(0, "text")` lays out literal text at once;
`set_message_prompt(0, 0)` hides the button prompt for text the player is
not meant to dismiss.

## Monsters can start map events

A monster script's `run_event_script(n)` asks the dungeon to run program `n`
of the map script that is loaded. The dungeon picks the request up on its
next check (`IsEventRun`, `dng_main.cpp`; `CMonsterMan::IsRunEvent`,
`monster.cpp`), while the player is alive and on the ground. Event scripts can
do the same with `queue_dungeon_event`.

This is how some story fights end: the castle's Griffon Soldiers count their
deaths in a shared monster variable and the fourth one runs event 510. Swap
those monsters for others and the fight never ends, unless the map also has a
`monsters_cleared` program.

## Shared state

- **The dungeon frame timer** counts every frame, in play and in events
  (`DngStep`). Monster scripts read and reset it with `get_floor_timer` and
  `reset_floor_timer`, event scripts with `reset_dungeon_timer`. There is one
  per floor, shared by every monster, so several monsters can agree on a
  shared clock. A floor's timed medal challenge reads it too (`dngfloor.cpp`).
- **Monster variables** 0 to 7 belong to each monster; 8 to 135 are shared
  by every monster on the floor (`set_monster_var`, `get_monster_var`).
- **Save flags** are 2,048 bits in the save (`savedata.cpp`). Event scripts
  use `get_flag` and `set_flag`, monster scripts `get_dungeon_flag` and
  `set_dungeon_flag`; both reach the same bits. No script on the disc and
  nothing in the engine uses a flag above 824.
  A flag is saved with the game, so clear one that only matters during a
  scene when the scene ends, and on game over.
- **Save counters** are 128 16-bit numbers in the save (`short_flag` in
  `savedata.cpp`), read and written by event scripts' `get_counter` and
  `set_counter` (values -32768 to 32767). The disc's scripts use 0 to 20 but
  6; the engine reads 5 in the main menu and keeps the fishing tournament's
  count and wins in 21 to 25.
- **Script globals** keep their values between program runs for as long as
  the map's script stays loaded; they are zeroed when it loads
  (`CRunScript::load`).

A mod does not pick a flag, counter, shared monster variable or program number by hand: it asks
for one by name, and the build gives it one nothing else uses (see
[numbers of a mod's own](../modding/mods.md#numbers-of-a-mods-own)).

## Language variants

Map scripts come in one file per language, `s51_1.c` (English) to `s51_5.c`;
the engine loads the one for the console's language (`FileNameConvLanguage`,
`event_func.cpp`; `EntryEventScript`, `dng_main.cpp`). A file without a number (`s51.c`)
is the original Japanese script, which the PAL game never loads. Editing them is in
[editing scripts](../modding/scripts.md#language-variants).

## Message text

Text given to `show_message` (and `modkit_say`, `modkit_text_show`) can hold
these codes, which the engine's message layout reads (`nd_meswin.cpp`,
`font.cpp`):

| Code | Shows |
| --- | --- |
| `\n` | a new line |
| `<page>` | a page break: the window waits for the button before the rest |
| `[Number1]` ... `[Number16]` | a number set with `set_message_value(window, slot, value)` *before* the text is shown: `show_message` lays the numbers out at once, so a value set after it shows 0 (checked in the game); negative numbers and 0 show |
| `[文字列１]` ... `[文字列１６]` | text set with `set_message_string(window, slot, text)`; the slot is in full-width digits |
| `[アイテム１]` ... | an item's name, set with `set_message_item(window, slot, item)` |
| `(O)` `(X)` `(A)` `(#)` | the circle, cross, triangle and square buttons |
| `[l1]` `[r1]` `[l2]` `[r2]` `[start]` `[select]` | the shoulder, Start and Select buttons |
| `(L)` `(R)` `(L3)` `(R3)` | the analog sticks, and pressing them |
| `[+]` `[-]` `[|]` `[!]` | D-pad icons |
| `[heart]` `[hp]` `[mp]` `[lv]` `[fish]` `[weapon]` ... | other icons of the game's font (the table in `font.cpp`) |

`[Number]` is the only tag with an English spelling (`[数値１]` works too);
the text and item tags are written as the Japanese scripts write them, in
every language. The shapes of `(A)` and `(#)` follow the order of the font
table (circle, triangle, cross, square), as the game's text uses them.

The game's own hint windows hold four lines; keep text that updates live as
short, so it never runs past what the window shows.

The menus' own text (item, shop and map names, the system messages) is in
message files with the same codes, already turned from tags (see [text](text.md)); a mod
edits it as text with the same tags (see [editing text](../modding/text.md)).

## Dungeon events

Besides the programs warps and other scripts start, the dungeon starts these
in the loaded map's script on its own (`IsEventRun`, `dng_main.cpp`), while
the player is alive and on the ground:

| Program | When |
| --- | --- |
| `floor_start` (1010) | a floor or dungeon-style map loads without a program of its own; a new game starts here, in dungeon 1 |
| `treasure_chest` (1100) | the player presses the action button at a chest; the chest stays closed unless the program opens it, as the dungeon scripts' own `open_treasure_chest` does, so a chest can be a switch, as the [calculator](../coding/examples/calculator/calculator.c) mod's is |
| `monster_talk` (1150) | the player presses the button by a monster placed as talkable, on maps whose dungeon data allows talking (`dbinfo.monster_talk`) |
| `monsters_cleared` (1500) | no monster is left on the floor; once per floor |
| a monster's `run_event_script(n)` | program `n`, on the monster's request |
| a trigger point's program | the player touches it, presses the action button at it or uses an item there |
