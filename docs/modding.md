# Modding guide

What a mod needs to know about how the game runs its scripts, and the tools
that answer questions about a decompiled tree. Each rule names the engine code
in the [ChronicleTwo](https://github.com/TheMoonPeople/ChronicleTwo) decomp it
comes from. The commands are in [usage](usage.md).

## Finding your way around

| Question | Command |
| --- | --- |
| What runs program 510 of map s51, and what asks for it? | `cli.py xref source event 510 --map s51` |
| How does the game get to map s51, and at which program? | `cli.py xref source map s51` |
| Which model and AI script does a monster kind use, and who places it? | `cli.py xref source monster golem` |
| Which save flags are free for a mod? Who uses flag 54? | `cli.py flags source`, `cli.py flags source --flag 54` |
| Which maps can a scene start in, and where does the player play? | `cli.py maps source` |
| A map's programs, free program numbers, trigger points and player positions? | `cli.py map source s17` |
| What have I changed? | `cli.py changes source` |

Files keep the game's codes with a readable name after them
(`map/s/s51_raybrandt_castle/`, `dungeon/monster/e33a_golem.c`); commands
take the codes. `source/include/disc.h` also gives each monster kind's model
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
  by every monster on the floor (`set_monster_var`, `get_monster_var`).
- **Save flags** are 2,048 bits in the save (`savedata.cpp`). Event scripts
  use `get_flag` and `set_flag`, monster scripts `get_dungeon_flag` and
  `set_dungeon_flag`; both reach the same bits. No script on the disc and
  nothing in the engine uses a flag above 824, so a mod can take one from
  825 to 2046 (`cli.py flags` checks); 2047 is taken by `build --start-at`.
  A flag is saved with the game, so clear one that only matters during a
  scene when the scene ends, and on game over.
- **Script globals** keep their values between program runs for as long as
  the map's script stays loaded; they are zeroed when it loads
  (`CRunScript::load`).

### Language variants

Map scripts come in one file per language, `s51_1.c` (English) to `s51_5.c`;
the engine loads the one for the console's language (`FileNameConvLanguage`,
`event_func.cpp`; `EntryEventScript`, `dng_main.cpp`). Edit all five, or put
your code in a library they all include. `xref` lists the variants of one
call once.

## Message text

Text given to `show_message` (and `modkit_say`, `modkit_text_show`) can hold
these codes, which the engine's message layout reads (`nd_meswin.cpp`,
`font.cpp`):

| Code | Shows |
| --- | --- |
| `\n` | a new line |
| `<page>` | a page break: the window waits for the button before the rest |
| `[Number1]` ... `[Number16]` | a number set with `set_message_value(window, slot, value)`; negative numbers and 0 show |
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
| a trigger point's program | the player touches it, presses the action button at it or uses an item there, as `cli.py map` lists |

## Editing shared code

Code that several scripts share lives in `lib/`, and an edit there reaches
every script that includes it. `build` notes each edited library that reaches
scripts you did not edit. To change a library for some scripts only, give
them their own copy first:

```
python3 cli.py fork source lib/dungeon/monster/monster_ai_full_toolkit.c lib/dungeon/monster/golem_toolkit.c dungeon/monster/e33a_golem.c
```

## A pattern: a caption every five seconds during a fight

`sdk/modkit/` ships this pattern ready to use: `modkit_caption` and
`modkit_text_show` (`"modkit/text.c"`) on the map side and `modkit_tick`
(`"modkit/tick.c"`) on the monster side. This made-up example spells it out
with natives, to show how it works, and puts the rules above together. A map program shows the
next caption and never waits, so it runs within a frame while the player keeps
control. The monsters in the fight provide the clock, because event scripts
cannot run beside play and monster scripts do.

In the map script (every language variant, or a library they include):

```c
int caption;

PROGRAM(900, show_next_caption);

// Program 900: shows the next caption in window 0. It never waits, so it runs within one frame.
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

// Called where the fight starts, after its monsters are placed.
void start_captions(void)
{
    set_flag(2040, 1);
    reset_dungeon_timer();
    caption = 0;
    show_next_caption();
}
```

Clear the flag and close the window where the fight ends and in the
`player_dead` program. In the AI of the monsters the fight places, after
forking the library that holds their waits, call this before each `wait;`:

```c
// While the fight's flag is set, asks the map for program 900 every 250 frames (5 seconds at
// 50 Hz). The timer is shared, so with several monsters only the first to see it run out asks.
void tick_captions(void)
{
    int fight;
    int timer;

    fight = get_dungeon_flag(2040);
    if (fight) {
        timer = get_floor_timer();
        if (timer >= 250) {
            reset_floor_timer();
            run_event_script(900);
        }
    }
}
```

The flag keeps the same monsters elsewhere in the game from asking.

## Trying a change quickly

Three things keep the loop short: test a mod's logic without the game
(`cli.py mod test`, see [usage](usage.md#mods)); build only what changed,
which `build` does by itself; and start New Game at the scene you are
working on, then boot it with `--play`. A mod can name its scene in
`mod.json` (`"start_at": "s17:9000"`), so building it is enough.

`build --start-at MAP:PROGRAM` writes a test image whose New Game goes
straight to a scene, without changing the source tree:

```
python3 cli.py build source dc2.iso -o test.iso --start-at s51:story_50
```

A new game starts in dungeon 1's script at `floor_start` (`title.cpp`,
`EntryEventScript` in `dng_main.cpp`). The build compiles that script with a
function in front of `floor_start` that, the first time it runs, sets flag
2047 and warps the way the game's own story warps out of dungeon 1 do:
`load_dungeon_map`, then `load_event_script` at the program. Later visits to
dungeon 1 play normally. This works for maps the game loads with
`load_dungeon_map` (story scenes and dungeon-like maps; `xref map` shows
whether it does), and the scene starts with a new game's party and items, so
a scene that expects more may not play right. Use these images for testing
only.

## Keeping your edits

`setup` and `decompile` write `original-hashes.json` at the root of the tree,
recording what they wrote. From it:

- `cli.py changes source` lists every file you edited, added or removed;
- `setup` and `decompile` refuse to overwrite edited files (`--force`
  overrides);
- `cli.py mod save source castle` copies your edited and added files into
  the mod `mods/castle`.

Better still, keep the tree untouched and do the work in mods: `cli.py mod
new source castle FILE...` copies the files you want to change into
`mods/castle/files/`, and `cli.py build source dc2.iso -o modded.iso --mod
castle --mod another` layers the chosen mods over the tree. A fresh tree,
decompiled after a tools update, takes the same mods; `mod list` marks a mod
file whose decompiler version has changed since, to compare. See
[usage](usage.md#mods).

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
