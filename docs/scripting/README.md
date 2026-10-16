# How Scripts Work & Lifecycle

Dark Chronicle (Dark Cloud 2) uses bytecode scripts for almost all gameplay logic. `dc2tk setup` decompiles all ~2,500 scripts on the disc into readable C files in `extracted/scripts/`.

## The big picture

The game is split into two parts:

- **The engine** (the PS2 executable) renders 3D graphics, runs physics and collision, plays animations and audio, and owns the static data tables.
- **The scripts** (`.stb` files, decompiled to `.c`) control what happens in the world: cutscenes, NPC dialogue, doors, chests, monster AI, player weapon combos, and visual effects.

Scripts call into the engine using ~1,100 built-in functions called **natives** (such as `place_monster` or `show_message`). The engine calls into scripts by starting numbered entry points called **programs** when an event happens.

| Script Type | Folder | SDK Header | What It Controls |
| --- | --- | --- | --- |
| **Map & Event** | `map/` | [`<event.h>`](api/event.md) | Towns, dungeons, cutscenes, NPCs, doors, and chests |
| **Monster AI** | `dungeon/monster/` | [`<monster.h>`](api/monster.md) | Movement, attacks, reactions, and boss phases for one monster |
| **Player Action** | `mainchr/`, `dungeon/act_script/` | [`<action.h>`](api/action.md) | Max, Monica, monster transformations, and the Ridepod |
| **Effect** | `dungeon/eff_script/` | [`<effect.h>`](api/effect.md) | Projectiles, shockwaves, beams, and hit sparks |
| **Argument** | `event/` | [`<arg.h>`](api/arg.md) | Small parameter records loaded by a few scenes |

## Anatomy of a script

Every script file has the same structure:

```c
#include <event.h>          // 1. SDK header for this script type
#include <pal.h>            //    Named save flags, music, and sound IDs
#include <disc.h>           //    Named monsters, items, NPCs, and motions
#include "modkit/text.c"    // 2. Shared helper library

int talk_count;             // 3. Global variable (starts at 0 when the script loads)

PROGRAM(map_start, on_enter);               // 4. Entry points the engine can start
PROGRAM(talk_or_dungeon_start, on_talk);
MAIN(on_enter);

void on_enter(void)
{
    // Runs when the player enters the map
}

void on_talk(void)
{
    talk_count = talk_count + 1;
    modkit_say("Hello!");   // Opens a text box and waits for the player to press (X)
}
```

## What a `PROGRAM` is

A normal C program has one `main()` function. A Dark Chronicle script has a **program table** declared with `PROGRAM(id, function)` lines.

Each `PROGRAM` connects an event number (or name from the SDK header) to a C function in the file. Nothing in a script runs on its own—functions only run when the engine (or another script) starts one of the script's programs:

- **In a town or story map (`<event.h>`)**:
  - `map_start` (`100`): Runs when the player enters the map.
  - `gate` (`300`) / `save_point` (`400`): Runs when the player steps into a time gate or save point.
  - `talk_or_dungeon_start` (`1000`): Runs when the player presses confirm next to an NPC.
  - Numbered programs (`500`, `510`, `11000`, etc.): Cutscenes, doors, or search spots triggered by map markers or warps.
- **In a dungeon (`<event.h>`)**:
  - `floor_start` (`1010`): Runs when a dungeon floor loads.
  - `treasure_chest` (`1100`): Runs when the player opens a chest.
  - `monsters_cleared` (`1500`): Runs once when the last monster on the floor dies.
- **In a monster AI script (`<monster.h>`)**:
  - `init` (`0`): Runs once when the monster spawns.
  - `update` (`4`): Runs in a loop every frame while the monster is alive.
  - `damage` (`2`) / `guard` (`1`) / `death` (`3`): Runs when the monster is hit, blocks, or dies.

Every script also has a `MAIN(function)` line required by the binary header. If a script defines global or `static` variables with initial values (`int count = 3;`), the compiler places that initialization inside `MAIN` and runs it automatically before the first program starts.

## Script lifecycle

### 1. Load
- **Map scripts**: The engine loads a map's `.stb` script when the player enters the map. Only one map script is loaded at a time.
- **Monster scripts**: The engine loads a monster's `.stb` script when that monster spawns on a floor. Each live monster runs its own instance.
- When a script loads, all of its **global variables** are set to `0`.

### 2. Run and yield (`wait;`)
Active programs run as **coroutines**. A program runs until it reaches a `wait;` statement or returns:

```text
Frame 1                 Frame 2                 Frame 3
[Script runs...]        [Script resumes...]     [Script resumes...]
  set_motion(...);        check_done();           finish_event();
  wait;  ------------->   wait;  ------------->   return;
         [Engine draws]          [Engine draws]
```

- Native functions never block. For example, `set_actor_motion` starts an animation and returns immediately.
- `wait;` pauses the script for the rest of the current frame and hands control back to the engine so it can render graphics, update physics, and advance animations.
- On the next frame (25 frames per second on PAL), the script resumes right after `wait;`.
- To wait for 1 second (25 frames), loop 25 times with `wait;` inside the loop (or call `modkit_wait(25)`).

### 3. Event mode vs. gameplay
- **Programs that `wait;` pause the player**: When a map program executes `wait;`, the engine enters **event mode** until that program returns. While in event mode, the player cannot move or attack.
- **Programs without `wait;` run instantly**: If a map program finishes and returns without ever executing `wait;`, the engine runs the entire function inside a single frame and **never enters event mode**. There is no camera cut and no lost player input.
- **Monsters can start map programs**: A monster AI script can call `run_event_script(program_number)` during combat to start a program in the loaded map script (for example, to trigger a phase transition or end a boss fight when killed).

### 4. Unload
- When the player leaves the map, the map script unloads and all of its global variables are discarded.
- When a monster dies and finishes its `death` program, its script instance ends.

## Where state lives

Choose where to store data based on how long it needs to last:

| Storage | Lifetime | How to Read / Write | Typical Use |
| --- | --- | --- | --- |
| **Local variable** | One function call | Plain C variable inside a function | Loop counters, temporary coordinates |
| **Global variable** | Until the player leaves the map | File-scope C variable (`int visited;`) | "Talked to this NPC twice during this visit" |
| **Monster variable (`0`–`7`)** | Until that monster dies | `get_monster_var` / `set_monster_var` | Per-monster state or phase |
| **Shared monster variable (`8`–`135`)** | Until the player leaves the floor | `get_monster_var` / `set_monster_var` | Floor-wide kill counters shared by all monsters |
| **Save flag (`0`–`2047`)** | Saved to the PS2 memory card | `get_flag` / `set_flag` (map)<br>`get_dungeon_flag` / `set_dungeon_flag` (monster) | Story progress ("cutscene seen", "door unlocked"). Flags `825`–`2046` are unused by the retail game. |
| **Save counter (`0`–`127`)** | Saved to the PS2 memory card | `get_counter` / `set_counter` | Multi-step quest state (`-32768` to `32767`). Counters `26`–`127` are unused by the retail game. |

In a mod, use `NEW_FLAG(NAME)`, `NEW_COUNTER(NAME)`, `NEW_PROGRAM(NAME)`, or `NEW_MONSTER_VAR(NAME)` instead of picking numbers by hand. `dc2tk build` automatically assigns an unused number that will not conflict with the game or other mods.

## Script files and languages

```text
extracted/scripts/
├── map/
│   ├── g/   Georama towns (g01_sindain, ...)
│   ├── m/   Palm Brinks exterior maps (m01_palm_brinks, ...)
│   ├── i/   Palm Brinks interiors (shops, houses, station)
│   ├── f/   Future towns (f01_jurak_mall, ...)
│   ├── s/   Story maps and cutscenes (s51_raybrandt_castle, ...)
│   └── d/   Dungeons (d01 to d05), boss arenas (b01, ...), entrances (e01, ...)
├── dungeon/
│   ├── monster/      Monster and boss AI scripts (e33a_golem.c, ...)
│   ├── eff_script/   Visual effect scripts
│   └── act_script/   Monster transforms and Ridepod action scripts
├── mainchr/          Max (c01_max.c) and Monica (c02_monica.c) action scripts
├── lib/              Shared code included by scripts with #include "lib/..."
└── include/disc.h    Constants extracted from your disc (monsters, items, motions)
```

Every map script comes in **five language files**: `_1.c` (English), `_2.c` (French), `_3.c` (German), `_4.c` (Italian), and `_5.c` (Spanish). A file with no number (`g01.c`) is unused Japanese code. When you write a mod patch with `#pragma target map g01`, `dc2tk build` applies your change to all five language files automatically.

## Next steps

- **[Map, Monster, and Player Scripts](systems.md)**: How towns, cutscenes, dungeons, monster AI, and player combos work.
- **[C Language, SDK, and Patches](language.md)**: Supported C features, `modkit` helpers, patching scripts, and editor setup.
- **[Example Guides](tutorials/README.md)**: Short step-by-step recipes for common scripting tasks.
- **[API Reference](api/README.md)**: Every native function, helper, program number, and constant.
