# Creating Mods

A mod is a folder inside `mods/` (for example, `mods/castle/`). Keeping your changes in `mods/` leaves `extracted/` clean so you can build any combination of mods together:

```sh
dc2tk build extracted/scripts dc2.iso -o modded.iso --mod castle --mod another-mod
```

Two complete example mods are included in the repository under `docs/scripting/examples/`:

- **[calculator](../scripting/examples/calculator/calculator.c)**: An in-game calculator with a custom 3D model, 2D UI sprites, sound effects, and unit tests.
- **[spinning-cult](../scripting/examples/spinning-cult/README.md)**: A quest mod adding a custom room in Blender, a door in Palm Brinks, a new character, a custom item, and new music.

## Mod folder layout and `mod.json`

A mod folder contains `mod.json` and any `.c` script patches or asset files your mod uses:

```text
mods/my-mod/
├── mod.json            # Declares data tables, replaced/added assets, and settings
├── s51.c               # A script patch (optional)
├── data/
│   └── items.json      # Edited item stats (optional)
└── art/
    └── soldier.png     # Replacement texture (optional)
```

Here is an example `mod.json` showing the main sections:

```json
{
  "requires": ["other-mod"],
  "start_at": "s51:story_50",
  "data": {
    "items": "data/items.json",
    "monsters": "data/monsters.json"
  },
  "replace": {
    "texture:e254a1": "art/soldier.png",
    "sound:SE_CURSOR_MOVE": "sounds/click.wav",
    "music:BG_000_000": "music/title.mid",
    "text:en/shopname": "text/shopname.txt",
    "model:c02fuku*": "models/monica.gltf",
    "map:s17": "maps/s17/s17.gltf"
  },
  "add": {
    "sound:SE_MY_BEEP": {"bank": "SP_042", "file": "sounds/beep.wav"},
    "music:BG_201": {"file": "music/theme.mid", "like": "BG_193"},
    "map:fishroom": {"file": "maps/fishroom.gltf", "like": "m07", "script": "fishroom.c"}
  }
}
```

## Game assets by id

Instead of hunting down every copy of a texture, model, or sound across the disc's pack archives, you reference assets in `mod.json` by their **asset ID** (listed in `extracted/assets/manifest.json`).

Use `dc2tk mod asset` to export an asset or data table into your mod and add it to `mod.json` automatically:

```sh
dc2tk mod asset dc2.iso my-mod data:items
dc2tk mod asset dc2.iso my-mod texture:e254a1
dc2tk mod asset dc2.iso my-mod model:e33a
dc2tk mod asset dc2.iso my-mod sound:SE_CURSOR_MOVE
dc2tk mod asset dc2.iso my-mod music:BG_000_000
dc2tk mod asset dc2.iso my-mod text:en/shopname
```

When you run `dc2tk build`, it updates every copy of that asset across the disc.

## Patches

To change game scripts, add a `.c` patch file to your mod folder. A patch contains only the functions you want to replace or add, so multiple mods can patch the same script without conflicting.

Generate a patch for an existing function with `dc2tk mod override`:

```sh
dc2tk mod override extracted/scripts castle --map s51 start_castle_battle
```

This creates `mods/castle/s51.c`:

```c
#pragma target map s51          // Target all 5 language variants of map s51
#include <event.h>
#include <disc.h>

NEW_FLAG(FIGHT_STARTED);        // Allocate a free save flag for this mod

#pragma override                // Replace start_castle_battle in the target script
void start_castle_battle(void)
{
    original();                 // Run the original start_castle_battle function
    set_flag(FIGHT_STARTED, 1);
}
```

- **`#pragma target`**: Picks which scripts to patch (`map s51`, `monster golem`, `script mainchr/c01.c`, or `library lib/...`). A `.c` file in your mod without `#pragma target` is a helper library that your patches can `#include`.
- **`#pragma override`**: Replaces a function in the target script. Call `original(...)` to run the replaced function.
- **`#pragma override <native_name>`**: Wraps an engine native function (such as `place_monster`) across the target script so every call to that native runs your function first:
  ```c
  #pragma override place_monster
  void place_rats(monster_kind monster, float x, float y, float z, float rx, float ry, float rz, int talk)
  {
      if (monster == MONSTER_GRIFFON_SOLDIER_523) {
          monster = MONSTER_SEWER_RAT;
      }
      original(monster, x, y, z, rx, ry, rz, talk);
  }
  ```
- **`#pragma every_frame`**: Runs a `void func(void)` function once per frame before every `wait;` in the target script.
- **`PROGRAM(id, func)`**: Registers a new event program entry point in the target script.

## Numbers of a mod's own

Never hardcode save flag numbers or event program numbers by hand. Declare them with these macros in your patch or mod header:

- `NEW_FLAG(NAME)` — Allocates an unused save flag (`1024`–`2046`).
- `NEW_COUNTER(NAME)` — Allocates an unused 16-bit save counter (`32`–`127`).
- `NEW_PROGRAM(NAME)` — Allocates an unused map event program number (`9000`+).
- `NEW_MONSTER_VAR(NAME)` — Allocates an unused floor-wide monster variable (`8`–`135`).

`dc2tk build` saves the assigned numbers in `mods/numbers.json` so your mod keeps the same IDs across builds and never collides with the game or other mods.

## Text in each language

Every PAL map script has five language variants (`_1.c` English, `_2.c` French, `_3.c` German, `_4.c` Italian, `_5.c` Spanish). A patch with `#pragma target map s51` compiles into all five. Use the built-in `LANGUAGE` constant if you want different text per language:

```c
#if LANGUAGE == LANGUAGE_FRENCH
    modkit_say("Bonjour !");
#else
    modkit_say("Hello!");
#endif
```

## Mods that build on mods

- `"requires": ["base-mod"]` in `mod.json` automatically includes `base-mod` whenever your mod is built, applies `base-mod` first, and lets your patches `#include "base-mod/header.h"`.
- `"after": ["other-mod"]` applies your mod after `other-mod` if both are selected, without requiring it.

## Whole files

If you need to replace an entire script file instead of writing a patch, place it under `mods/<name>/files/` (or run `dc2tk mod new extracted/scripts <name> <script-path>`). You can convert whole-file copies into small function patches at any time with `dc2tk mod slim extracted/scripts <name>`.

To replace or add a raw file on the disc (such as a `.chr` or `.img` built from a template), use `"file:<disc-path>"` in `mod.json`:

```json
"add": {
  "file:mods/calculator/calc_ui.img": {"file": "ui", "like": "texture:logo_d00"}
}
```

## Trying a change quickly

Add `"start_at": "s51:story_50"` to `mod.json` (or pass `--start-at s51:story_50` to `dc2tk build`) to jump straight to your map or event when you pick **New Game** on the title screen:

- `"m01"` — Spawn in Palm Brinks at `map_start`.
- `"s51:story_50"` — Run program `story_50` (`500`) on map `s51`.
- `"s51:MY_PROGRAM"` — Run a `NEW_PROGRAM(MY_PROGRAM)` defined by your mod.
- `"d01f02"` — Enter dungeon 1, floor 2.

## The mod commands

See [CLI Reference: Creating and managing mods](commands.md#creating-and-managing-mods) for the full list of `dc2tk mod` subcommands (`make`, `asset`, `override`, `check`, `test`, `list`, `slim`, `header`).

## Tests

You can test pure C helper functions in your mod without booting PCSX2 by creating `mods/<name>/tests.c` and running:

```sh
dc2tk mod test extracted/scripts <name>
```

### A first test

Each `PROGRAM(id, test_func)` in `tests.c` runs as a separate test case inside `dc2tk`'s built-in script VM. If a function returns a non-zero number, the test passes; if it returns `0`, the test fails:

```c
#include <event.h>
#include "my_math.c"

int test_addition(void)
{
    return add_two(2, 3) == 5;
}

PROGRAM(1, test_addition);
MAIN(test_addition);
```
