# Modding guide

What a mod needs to know about how the game runs its scripts, and the tools
that answer questions about a decompiled tree. Each rule names the engine code
in the [ChronicleTwo](https://github.com/TheMoonPeople/ChronicleTwo) decomp it
comes from. The commands are in [usage](usage.md).

## Finding your way around

| Question | Command |
| --- | --- |
| What runs program 510 of map s51, and what asks for it? | `dc2tk xref extracted/scripts event 510 --map s51` |
| How does the game get to map s51, and at which program? | `dc2tk xref extracted/scripts map s51` |
| Which model and AI script does a monster kind use, and who places it? | `dc2tk xref extracted/scripts monster golem` |
| Which save flags are in use? Who uses flag 54? (A mod asks for its own with `NEW_FLAG`.) | `dc2tk flags extracted/scripts`, `dc2tk flags extracted/scripts --flag 54` |
| Which save counters are in use? Who uses counter 5? (`NEW_COUNTER` for a mod's own.) | `dc2tk flags extracted/scripts --counters`, `dc2tk flags extracted/scripts --counter 5` |
| Which maps can a scene start in, and where does the player play? | `dc2tk maps extracted/scripts` |
| A map's programs, free program numbers, trigger points and player positions? | `dc2tk map extracted/scripts s17` |
| What have I changed? | `dc2tk changes extracted/scripts` (scripts), `dc2tk diff dc2.iso` (scripts, assets and disc files) |
| Turn my edits into a mod | `dc2tk mod make dc2.iso mymod` |
| Make its whole scripts patches of what changed | `dc2tk mod slim extracted/scripts mymod` |

The quickest way to a mod is to edit what `dc2tk setup` wrote and let `dc2tk mod make`
collect it. Edit scripts in `extracted/scripts`, textures, models and sounds in
`extracted/assets` (PNG, glTF, WAV), or any game file in `extracted/disc`, and
add new files there. `dc2tk diff dc2.iso` lists what changed. `dc2tk mod make dc2.iso mymod` writes
`mods/mymod`, with the scripts as whole files, each changed asset as a `replace`
line by its id (so it changes every place the disc has it), and each changed or
added disc file as a `file:` line. Then `dc2tk build extracted/scripts dc2.iso -o
mymod.iso --mod mymod`. The disc's own files beside `DATA.DAT` (movies,
`SOUND.DAT`, the indexes, the boot program) are listed but cannot go in a mod.
`build`, `diff` and `mod make` take an extracted disc folder in place of the
image too, but it has to be untouched: `build` packs the folder it is given as
it is, so a folder you edited would put your edits in under the mod's.

Files keep the game's codes with a readable name after them
(`map/s/s51_raybrandt_castle/`, `dungeon/monster/e33a_golem.c`); commands
take the codes. `extracted/scripts/include/disc.h` also gives each monster kind's model
and AI script:
`MONSTER_GOLEM = 168,  // model e33a, AI e33a`. Several kinds can share one
AI script (all four kinds of `e33a`), and a story fight can use an event-only
kind with its own AI (kind 523, the castle's Griffon Soldier, runs `b254a`
while the ordinary Griffon Soldier, kind 37, runs `e254a`).

`xref` and `flags` read the source tree as text and fold constant
expressions (`50 * 10 + 0 + 10000`, `story_50`), so they see your edits as
soon as you save. Calls whose arguments are worked out at run time are listed
for you to check rather than guessed at.

## How scripts run

### Programs

A map script's `PROGRAM` lines are the events the engine can start in it:
`map_start` when a town map loads, `floor_start` when a dungeon floor does,
`monsters_cleared` (1500) once no monster is left on a floor, numbered story
programs that warps name, and so on (`enum program` in `sdk/event.h`). A
monster script's `PROGRAM` lines are its states: `init`, `update`, `damage`,
`death` (`enum program` in `sdk/monster.h`).

Programs run as coroutines: `wait;` gives up the rest of the frame and the
program carries on from there on the next one. The PAL game runs 50 frames a
second, which is why the scripts scale frame counts by `50 / 60`.

### While an event runs, the player waits

In a dungeon, starting a map program puts the dungeon in event mode
(`EventScriptSetup`, `dng_main.cpp`) until the program ends, and the pad only
moves the player outside event mode (`DngMainKey`). So an event script cannot
keep a loop running beside play: the loop would hold the player still.

### An event that never waits costs nothing

If a program reaches its end without a `wait;`, the VM's `run` returns 0
(`runscript.cpp`) and `EventScriptSetup` never enters event mode. The whole
program runs inside one frame, with no camera switch and no lost input. A
program with no `wait;` on any path is in effect a function the dungeon can
call during play.

### Message window 0 stays on screen

The dungeon steps and draws event message window 0 every frame, in event mode
or not (`DngMainDraw`, `dng_main.cpp`), and ending an event does not close it
(`EdEventEnd` only drops the loaded message file). So a window opened by an
event stays up during play until a script closes it with `close_message`.
`show_message(0, "text")` lays out literal text at once;
`set_message_prompt(0, 0)` hides the button prompt for text the player is
not meant to dismiss.

### Monsters can start map events

A monster script's `run_event_script(n)` asks the dungeon to run program `n`
of the map script that is loaded. The dungeon picks the request up on its
next check (`IsEventRun`, `dng_main.cpp`; `CMonsterMan::IsRunEvent`,
`monster.cpp`), while the player is alive and on the ground. Event scripts can
do the same with `queue_dungeon_event`.

This is how some story fights end: the castle's Griffon Soldiers count their
deaths in a shared monster variable and the fourth one runs event 510. Swap
those monsters for others and the fight never ends, unless the map also has a
`monsters_cleared` program. `xref event` lists every script that requests an
event and marks the AI scripts of monsters the map places.

### Shared state

- **The dungeon frame timer** counts every frame, in play and in events
  (`DngStep`). Monster scripts read and reset it with `get_floor_timer` and
  `reset_floor_timer`, event scripts with `reset_dungeon_timer`. There is one
  per floor, shared by every monster, so several monsters can agree on a
  shared clock. A floor's timed medal challenge reads it too (`dngfloor.cpp`).
- **Monster variables** 0 to 7 belong to each monster; 8 to 135 are shared
  by every monster on the floor (`set_monster_var`, `get_monster_var`). A
  mod asks for a shared one with `NEW_MONSTER_VAR(name)`, and `build` picks
  one no script uses.
- **Save flags** are 2,048 bits in the save (`savedata.cpp`). Event scripts
  use `get_flag` and `set_flag`, monster scripts `get_dungeon_flag` and
  `set_dungeon_flag`; both reach the same bits. No script on the disc and
  nothing in the engine uses a flag above 824; 2047 is taken by
  `build --start-at`. A mod asks for a flag with `NEW_FLAG(name)` (in its
  own sources, outside `files/`) rather than picking a number, and `build`
  gives it one from 1024 up that no
  script, the engine or another mod uses, and keeps it in
  `mods/numbers.json` so saves stay valid (see [usage](usage.md#mods)).
  A flag is saved with the game, so clear one that only matters during a
  scene when the scene ends, and on game over.
- **Save counters** are 128 16-bit numbers in the save (`short_flag` in
  `savedata.cpp`), read and written by event scripts' `get_counter` and
  `set_counter` (values -32768 to 32767). The disc's scripts use 0 to 20 but
  6; the engine reads 5 in the main menu and keeps the fishing tournament's
  count and wins in 21 to 25. A mod asks for one with `NEW_COUNTER(name)`,
  and `build` gives it one from 32 up that nothing else uses, kept in
  `mods/numbers.json` as flags are. `dc2tk flags extracted/scripts --counters`
  lists them.
- **Script globals** keep their values between program runs for as long as
  the map's script stays loaded; they are zeroed when it loads
  (`CRunScript::load`).

### Language variants

Map scripts come in one file per language, `s51_1.c` (English) to `s51_5.c`;
the engine loads the one for the console's language (`FileNameConvLanguage`,
`event_func.cpp`; `EntryEventScript`, `dng_main.cpp`). Edit all five, or put
your code in a library they all include. A file without a number (`s51.c`)
is the original Japanese script, which the PAL game never loads. `xref` lists the variants of one
call once.

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
message files with the same codes, already turned from tags; a mod edits it
as text with the same tags (see [the message format](assets/text.md)).

## Dungeon events

Besides the programs warps and other scripts start, the dungeon starts these
in the loaded map's script on its own (`IsEventRun`, `dng_main.cpp`), while
the player is alive and on the ground:

| Program | When |
| --- | --- |
| `floor_start` (1010) | a floor or dungeon-style map loads without a program of its own; a new game starts here, in dungeon 1 |
| `treasure_chest` (1100) | the player presses the action button at a chest; the chest stays closed unless the program opens it, as the dungeon scripts' own `open_treasure_chest` does, so a chest can be a switch, as the calculator mod's is |
| `monster_talk` (1150) | the player presses the button by a monster placed as talkable, on maps whose dungeon data allows talking (`dbinfo.monster_talk`) |
| `monsters_cleared` (1500) | no monster is left on the floor; once per floor |
| a monster's `run_event_script(n)` | program `n`, on the monster's request |
| a trigger point's program | the player touches it, presses the action button at it or uses an item there, as `dc2tk map` lists |

## Pictures and models of a mod's own

Scripts load pictures and models by disc path: `load_image(stack,
"mods/calculator/calc_ui.img", 0, &block)` loads a texture file, whose pictures
`set_image_texture(image, "calc_body")` then asks for by name, and
`load_chara(stack, "mods/calculator/calc.chr", "info.cfg", slot)` loads a
character pack. A mod makes such files from PNGs and glTF with `file:`
lines that say which of the disc's files to make them like (the calculator
example, `docs/examples/calculator`, does both):

```json
"add": {
  "file:mods/calculator/calc_ui.img": {"file": "ui", "like": "texture:logo_d00"},
  "file:mods/calculator/calc.chr": {"file": "model/calc.gltf", "like": "model:uki"}
}
```

- **A picture file** takes a folder of PNGs (or one PNG): one picture each,
  named after its file (`ui/calc_body.png` is `calc_body`). `texture:logo_d00`,
  the title logo, makes them as the game's 2D pictures are: 8-bit with a
  palette of 256 colours with alpha, so a PNG of more colours is reduced
  to 256, and see-through pixels stay see-through. Sides are up to 1024
  texels, the width a multiple of 16. `texture:sphida_bar` and
  `texture:e100_2d01` are the same format.
- **A model pack** takes a glTF: its nodes and meshes as they are placed,
  each material's base colour PNG as a picture of the pack (sides powers
  of two), its texture named after the image. `model:uki`, the fishing
  float, is a static prop's pack: its `info.cfg` names the new
  `calc.img` and `calc.mds`, with no motion. A rigged character works too:
  its armature becomes the pack's skeleton, its skinned meshes are skinned
  as the game's are and its actions become motions `set_chara_motion`
  plays by name; take a character's model as the template
  (`model:p01_01a`, a villager). A `.glb`, or a folder holding the glTF,
  works as well as a `.gltf`. See
  [the new-character example](examples/new-character/README.md) and
  [usage](usage.md#mods), "New characters".
- `"like"` makes only these two: a `.img` like a texture (`texture:NAME` or
  a texture file's disc path) and a `.chr` like a model (`model:NAME` or a
  pack's disc path), one id and not a pattern; anything else is refused.
- `build` lists them as new files; `dc2tk mod asset built.iso NAME
  mods/calculator/calc_ui.img --as png` (or `calc.chr --as gltf`) copies them
  back out of a built image to check. The rules are in [the
  design](design.md), "Mods by name".

Keep them small: a picture file loads into the memory stack the script
names, and an 8-bit picture takes a byte a texel and 1 KB of palette, so a
256x128 and a 32x32 picture make a file of about 36 KB.

## A map of a mod's own, and a door into it

A new map is a copy of one of the disc's, edited as a scene (`"add":
{"map:m05x": {"file": ..., "like": "m05"}}`), and it can have a script of
its own: `"script": "m05x.c"` on that line, a whole map script (`PROGRAM`
lines, `MAIN`, its functions) that `build` compiles in every language. Its
`map_start` runs when the map loads, and places the player and fades in. A
way in from the game's maps is a door: a `door` point added to an existing
map's scene, whose program (given by name, `"programs": {"door_park":
"ENTER_PARK"}` on that map's line) a patch of the map's script adds:
`jump_to_map(-1, "m05x", map_start)`. The way back is the same from the new
map: a program naming the town's own arrival program, or one of the mod's
that runs it and places the player at the door. The mod's scripts share
the program numbers by `NEW_PROGRAM` in a header both include (`build` gives
each one from 9000 to 9999, kept in `mods/numbers.json`), and see the
new map's number as `MAP_M05X`. See [maps](assets/maps.md) section 9 and the
[new-map-door example](examples/new-map-door/README.md).

A map line takes `"file"` (the scene) and `"programs"`, and under `add` also
`"like"`, `"script"` and `"title"` (the new map's own title in the map table,
printable ASCII without quotes; the like map's otherwise). Any other option,
`"only"` among them, is refused: a map line changes the one map it names. A
new map's name is lower-case letters, digits and `_`, starting with a
letter, at most 12 long, since its files are named after it in the engine's
32-byte file names. Its row goes after the disc's in every language's map
table (`map/map<language>.cfg`), so it is numbered after the disc's maps (189
on the PAL disc) and no map of the game changes number; the save keeps map
flags (opened chests and the like) only for maps numbered below 256.

## What the game has room for

The game loads into fixed memory, and `build` checks every load the files
it changes take part in ([memory](assets/memory.md)): a file read whole, a
map's script (312,816 bytes at most; some of the disc's are within a few
thousand of it), a map's placements (320, 400 in a dungeon), texture blocks,
sound memory, a skinned mesh's vertices and bones. A map counts as the kind
its row in the game's own map tables (`map/map<language>.cfg`) gives it,
town, interior or dungeon; a new map as its like map's. A budget that is
certain and exceeded is an error: the image is still written, but `build`
says the game will not load it and exits with 1. An estimated budget
exceeded, where the load grew, is a warning.

## Music of a mod's own

A `music:` line under `add` gives the mod a music file of its own, made from
a MIDI file and one of the disc's music banks, whose instruments it plays:

```json
"add": {
  "music:BG_200": {"file": "music/fish.mid", "like": "BG_193"}
}
```

`build` writes the new file `snd2/bgm/BG_200.snd`: a copy of `BG_193`
whose one entry plays the MIDI file, converted as a `music:` replace line
converts it (`"instruments"` works as there), and the samples the tune
does not play cut. Scripts play it as the game's own music, by the number
in its name:

```c
load_bgm(200);
play_bgm(0);
```

The engine keeps no table of music files: `load_bgm(n)` reads
`snd2/bgm/BG_<n>.snd` (three digits at least) by name, and `play_bgm(0)`
plays its entry 0 at the default volume of the like bank's `.vol` unless a
volume is given (`play_bgm(0, 100)`). The number is written as it is, in
strict mode too; pick one from 0 to 999 the disc lacks (its own are 0 to
195, listed as `BGM_...` in `pal.h`), and only one mod may add it. A map
loads its sound set's tune when it loads (the like map's, for a new map),
so a map that should play the mod's tune calls `load_bgm` in `map_start`.
See [music](assets/music.md), "A music file of a mod's own".

## Editing shared code

Code that several scripts share lives in `lib/`, and an edit there reaches
every script that includes it. A mod's patch changes a library function for
the scripts it targets only: `#pragma target monster golem` with
`#pragma override` before `wait_frames` changes the Golem's copy and leaves
the 35 other monsters that include the same toolkit alone (see
[usage](usage.md#mods)). `#pragma target library lib/...` patches every
script that includes the library instead.

Editing the tree itself still works: `build` notes each edited library that
reaches scripts you did not edit, and `fork` gives some scripts their own
copy of a library first:

```
dc2tk fork extracted/scripts lib/dungeon/monster/monster_ai_full_toolkit.c lib/dungeon/monster/golem_toolkit.c dungeon/monster/e33a_golem.c
```

## Changing what scripts ask the engine for

Much of what a scene does is in its native calls: which monsters
`place_monster` places, which sound `play_se` plays, which item `add_item`
gives. A patch can wrap a native for the scripts it targets, so every call
they make goes through a function of the mod, which calls the native with
`original(...)`, changed or not:

```
dc2tk mod override extracted/scripts rats --map s51 place_monster load_monster_data
```

writes a wrapper of each that passes every call on unchanged, with the
native's doc comment. Edit them to swap the castle fight's soldiers for
rats, wherever s51's scripts and libraries load or place them, without
copying `start_castle_battle`. Calls that leave a native's optional
arguments out still do (the wrapper sees them as 0, and `original()` passes
on only what the call gave), so `place_monster(-1)`, which places a floor's
own monsters at random, goes through the wrapper too and keeps its meaning.
Only the calls change, so the rest of the script compiles to the same bytes,
and another mod can wrap the same native: the later mod's wrapper runs first
(see [usage](usage.md#mods)). Swapped monsters may not end a fight the
original ones end (see [monsters can start map events](#monsters-can-start-map-events)).

## Mods that build on mods

A mod can keep code, flags and program numbers for others to use: a
`captions` mod with `captions.h` (`NEW_FLAG(CAPTIONS_ON);`) and a library
`show.c`. Another mod names it in its `mod.json`, `{"requires":
["captions"]}`, and includes its files by its folder:

```c
#pragma target map s17
#include <event.h>
#include "captions/captions.h"
#include "captions/show.c"
```

Building that mod builds `captions` in too, first: the flag has the number
`captions` was given, and where both override one function, this mod's
override runs first and its `original()` runs the one from `captions`.
`"after": ["other"]` orders two mods without either needing the other (see
[usage](usage.md#mods)).

Mods built together merge where they can: patches of one script, overrides
of one function, and data tables field by field. Two mods giving one thing
alike give it once; giving it differently stops `build` (and `mod check
--disc`),
naming it and both mods: `d01f01 room 0 rate is given two ways, by mod a and
mod b`. Two mods may not replace one asset id, one picture or sound entry,
or one game file either.

## A pattern: a caption every five seconds during a fight

`sdk/modkit/` ships this pattern ready to use: `modkit_caption` and
`modkit_text_show` (`"modkit/text.c"`) on the map side and `modkit_tick`
(`"modkit/tick.c"`) on the monster side. This made-up example spells it out
with natives, to show how it works, and puts the rules above together. A map program shows the
next caption and never waits, so it runs within a frame while the player keeps
control. The monsters in the fight provide the clock, because event scripts
cannot run beside play and monster scripts do.

The mod's two patches share a header, so they agree on the numbers `build`
gives them:

```c
// captions.h
NEW_FLAG(CAPTIONS_ON);          // set while the fight runs
NEW_PROGRAM(SHOW_NEXT_CAPTION); // the map program the monsters ask for
```

The map's patch (`#pragma target map s51` reaches every language variant):

```c
#pragma target map s51
#include <event.h>
#include "captions.h"

int caption;

PROGRAM(SHOW_NEXT_CAPTION, show_next_caption);

// Shows the next caption in window 0. It never waits, so it runs within one frame.
void show_next_caption(void)
{
    caption = caption + 1;
    set_message_window_mode(0, MES_WIN_VERSATILE_1);
    set_message_prompt(0, 0);
    if (caption == 1) {
        show_message(0, "The first caption");
    }
    if (caption == 2) {
        show_message(0, "The second caption");
    }
    if (caption <= 2) {
        show_message(0, -1);
        set_message_slot(0, 8);
        place_message_bubble(0, 0, 0, 0, 0);
    }
    if (caption == 3) {
        close_message(0);
        set_message_prompt(0, 1);
    }
}

// Starts the captions once the fight's monsters are placed.
#pragma override
void start_castle_battle(void)
{
    original();
    set_flag(CAPTIONS_ON, 1);
    reset_dungeon_timer();
    caption = 0;
    show_next_caption();
}
```

Clear the flag and close the window where the fight ends and in the
`player_dead` program's function, overriding them the same way. The
monsters' patch runs a function every frame, whatever wait their AI is in:

```c
#pragma target monster golem
#include <monster.h>
#include "captions.h"

// While the fight's flag is set, asks the map for the next caption every 125 frames (5 seconds
// at 25 frames a second). The timer is shared, so with several monsters only the first to see it run out asks.
#pragma every_frame
void tick_captions(void)
{
    int fight;
    int timer;

    fight = get_dungeon_flag(CAPTIONS_ON);
    if (fight) {
        timer = get_floor_timer();
        if (timer >= 125) {
            reset_floor_timer();
            run_event_script(SHOW_NEXT_CAPTION);
        }
    }
}
```

The flag keeps the same monsters elsewhere in the game from asking.

## Trying a change quickly

Four things keep the loop short: test a mod's logic without the game
(`dc2tk mod test`, see [usage](usage.md#mods)); compile what it changes
without a disc (`dc2tk mod check`); build only what changed, which `build`
does by itself; and start New Game at the scene you are working on, then
boot it with `--play`. A mod can name its scene in `mod.json`
(`"start_at": "s17:9000"`, or one of its own programs:
`"s51:SHOW_NEXT_CAPTION"`), so building it is enough; `--no-start-at` builds
a normal New Game.

`mod test` runs the `test_` functions of the mod's `tests.c` (an `int`, no
parameters; nonzero passes), compiled with the mod over the tree, in the
tools' own interpreter of the game's VM. Natives do nothing there, so what
one would return is 0, and `print` records its values, shown when a test
fails. A test that runs past 10,000,000 instructions or nests calls past
10,000 deep stops with an error naming the function, so a loop that never
ends fails rather than hangs.

`build --start-at MAP:PROGRAM` writes a test image whose New Game goes
straight to a scene, without changing the source tree:

```
dc2tk build extracted/scripts dc2.iso -o test.iso --start-at s51:story_50
```

`MAP/SCRIPT:PROGRAM` picks one script of a map that has several
(`--start-at s06/s06b:500`), and a map alone starts at its `map_start`.

A new game starts in dungeon 1's script at `floor_start` (`title.cpp`,
`EntryEventScript` in `dng_main.cpp`). The build compiles that script with a
function in front of `floor_start` that, the first time it runs, sets flag
2047 and warps the way the game's own story warps out of dungeon 1 do:
`load_dungeon_map`, then `load_event_script` at the program. Later visits to
dungeon 1 play normally. That suits maps the game loads with
`load_dungeon_map` (story scenes and dungeon-like maps; `xref map` shows
whether it does). Towns are not loaded that way, and the dungeon loop a new
game runs in ignores `jump_to_map`; for any other map in the disc's map table
(`map/map<language>.cfg`, `map1.cfg` for English, numbered by row; the older
`map/map.cfg` is read by nothing) the warp is the one the game uses to leave a
dungeon for a town, `go_to_edit_mode(map number, program)`, so
`--start-at m05` (at `map_start`) or `--start-at m05:PROGRAM` opens a town.
`build` prints which way it warps. The scene starts with a new game's party
and items, so a scene that expects more may not play right. Use these images
for testing only.

## Keeping your edits

`setup` and `decompile` write `original-hashes.json` at the root of the tree,
recording what they wrote. From it:

- `dc2tk changes extracted/scripts` lists every file you edited, added or removed;
- `setup` and `decompile` refuse to overwrite edited files (`--force`
  overrides);
- `dc2tk mod save extracted/scripts castle` copies your edited and added files into
  the mod `mods/castle`.

Better still, keep the tree untouched and do the work in mods:
`dc2tk mod override extracted/scripts castle --map s51 start_castle_battle` starts a
patch holding just that function, and
`dc2tk build extracted/scripts dc2.iso -o modded.iso --mod castle --mod another`
merges the chosen mods into the scripts they target. A patch holds only what it changes, so a fresh tree,
decompiled after a tools update, takes the same mods, and several mods can
change one script. Better definitions can rename a function a patch
overrides; the mod keeps the function's fingerprint, so `build` still finds
it and says its new name, and `dc2tk mod check extracted/scripts castle
--rename` puts the new name in the patch. (`mod new` copies whole files into a mod instead; `mod
list` marks a copied file whose decompiler version has changed since, to
compare. `dc2tk mod slim extracted/scripts castle` turns such copies into
patches of the functions they change, where the mod then builds to the same
bytes.) See [usage](usage.md#mods).

## Editor setup

`setup` and `decompile` write `compile_flags.txt` at the root of the tree.
clangd reads it for every file below, so an editor with clangd (VS Code's
clangd extension, Neovim, Emacs, Zed) knows every native, helper and
constant, with completion, hover docs and jump to definition. It points at
this checkout's `sdk/` and definition sets by absolute path, so move the
checkout and decompile again (or edit the file).

Scripts parse without errors. What clang still warns about are argument
mismatches in the retail scripts themselves. Library files show errors for
names their including scripts declare (a library can use the globals and
functions of the scripts that include it), which is expected.

A patch uses the functions and globals of the scripts it targets without
declaring them, since `build` merges it into them. `mod override` (and
`dc2tk mod header extracted/scripts NAME` at any time) writes a header of each
patch's target into the mod's hidden `.editor/` folder, and a
`compile_commands.json` that has clangd read each patch with the SDK, the
tree's headers, the mods folder (for `#include "base/names.h"` from a mod
it requires) and its target's header first. The patch then shows no false
errors, and hover shows the doc comments of the target's functions.
Write them again after decompiling again, as the target's names can change.
A patch whose `#pragma target` lines pick several scripts gets the
declarations of one script per line.
