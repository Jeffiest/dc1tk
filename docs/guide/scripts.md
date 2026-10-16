# Editing Scripts

This guide explains how the decompiled script tree is organized, how to modify shared library code, how to intercept engine native calls, and how to handle multi-language map scripts.

We recommend keeping your script changes inside a mod patch (see [Creating Mods](mods.md)). For a list of all CLI commands, see [CLI Reference](commands.md). For details on the C syntax and available functions, see [Writing Script Code](../scripting/README.md), [The Language](../scripting/language.md), [The SDK](../scripting/sdk.md), and the [Natives API Reference](../scripting/api/README.md). To learn how the game engine executes scripts, see [How Scripts Run](../internals/running.md).

## The decompiled tree

When you run `dc2tk setup`, it decompiles every script on the disc into `extracted/scripts/`. Each script becomes a `.c` file, and functions shared by multiple scripts are placed in library files under `extracted/scripts/lib/` (see [Decompiling](commands.md#decompiling)).

Folders and files keep their internal game code and add a readable suffix after an underscore:

- **Map scripts**: `map/s/s51_raybrandt_castle/s51_1.c`
- **Monster AI scripts**: `dungeon/monster/e33a_golem.c`
- **Character scripts**: `mainchr/c01_max.c`
- **Shared libraries**: `lib/map/s/s51_raybrandt_castle/...`

`dc2tk` gets these readable names from three sources:
1. Map folder names come from the disc's map table (in the definition set's language).
2. Monster script names come from the disc's monster table.
3. Other folder and file names (such as dungeons and playable characters) come from `paths.json` in the definition set.

Because internal game codes can also contain underscores, `dc2tk` saves the original disc path of every renamed file in `extracted/scripts/unnamed-paths.json`. Commands such as `build`, `defs learn`, `xref`, and `mod` read `unnamed-paths.json` so renamed `.c` files always replace the correct `.stb` file on the disc.

When you run `dc2tk` commands in the terminal:
- Refer to a map by its short code (for example, `dc2tk xref extracted/scripts map s51`).
- Refer to a monster by its ID number, its `MONSTER_` constant, or its short name (for example, `dc2tk xref extracted/scripts monster golem`).

## Editing shared code

Code shared across multiple scripts is stored in `extracted/scripts/lib/`. If you edit a file in `lib/` directly, your change affects every script that includes that library.

### Overriding shared code in a mod patch (Recommended)

In a mod patch, `#pragma override` replaces a library function **only** for the scripts your patch targets:

- **Change one script or monster**: Use `#pragma target monster golem` and put `#pragma override` before a shared function such as `wait_frames`. This modifies `wait_frames` for the Golem AI script only and leaves the 35 other monsters that include the same library untouched (see [Patches](mods.md#patches)).
- **Change every script that uses the library**: Use `#pragma target library lib/...` to apply your override to every script that includes that library.

### Splitting a shared library in the source tree (`dc2tk fork`)

If you are editing `extracted/scripts/` directly instead of using a mod patch, `dc2tk build` prints a notice whenever an edited library affects scripts you did not edit directly.

To give specific scripts their own separate copy of a shared library before editing it, run `dc2tk fork`:

```sh
dc2tk fork extracted/scripts lib/dungeon/monster/monster_ai_full_toolkit.c lib/dungeon/monster/golem_toolkit.c dungeon/monster/e33a_golem.c
```

This command copies `monster_ai_full_toolkit.c` to `golem_toolkit.c` and updates `e33a_golem.c` to `#include` the new copy.

## Changing what scripts ask the engine for

Most in-game actions happen when a script calls an engine native function:
- `place_monster` spawns enemies.
- `play_se` plays a sound effect.
- `add_item` gives an item to the player.

Instead of copying and editing a long scene function, a mod patch can **wrap** a native function for its target scripts. Every call that the target scripts (or their included libraries) make to that native runs your wrapper function instead. Inside your wrapper, call `original(...)` to invoke the actual engine native.

Run `dc2tk mod override` with one or more native names to generate starter wrappers:

```sh
dc2tk mod override extracted/scripts rats --map s51 place_monster load_monster_data
```

This command writes a patch file in `mods/rats/s51.c` containing wrapper functions (complete with parameter lists and documentation comments) that pass every call through to `original(...)` unchanged. You can then edit those wrappers to replace the castle Griffon Soldiers with Sewer Rats across all of map `s51` without copying `start_castle_battle`.

How native wrappers behave:
- **Optional arguments**: If a call in the script leaves out `OPTIONAL` arguments, your wrapper receives `0` for those parameters, and calling `original(...)` forwards only the arguments that the original call provided. For example, `place_monster(-1)` (which spawns random dungeon floor monsters) passes through your wrapper and still sends only one argument to the engine.
- **Minimal changes**: Wrapping a native modifies only the call instructions in the target script and appends the wrapper function at the end. All other code in the script compiles to the exact same bytes.
- **Mod compatibility**: Multiple mods can wrap the same native on the same script. The later mod's wrapper runs first, and its `original(...)` call runs the earlier mod's wrapper (see [Patches](mods.md#patches)).

> **Important**
> If you replace monsters in a story battle, check whether the original monster's AI script triggers the end of the fight. See [Monsters can start map events](../internals/running.md#monsters-can-start-map-events) and [A first test](mods.md#a-first-test) for how to handle battle completion with `PROGRAM(monsters_cleared, ...)`.

## Language variants

On the PAL disc, every map script has five language variants: `s51_1.c` (English), `s51_2.c` (French), `s51_3.c` (German), `s51_4.c` (Italian), and `s51_5.c` (Spanish). At runtime, the game loads the file that matches the PlayStation 2 system language (see [Language variants](../internals/running.md#language-variants)).

- **In a mod patch (Recommended)**: Use `#pragma target map s51`. `dc2tk build` automatically applies your patch to all five language files. If your patch adds or changes text, use the `LANGUAGE` constant to provide strings for each language in one file (see [Text in each language](mods.md#text-in-each-language)).
- **In `extracted/scripts/` directly**: Either apply your edit to all five `.c` files, or move your code into a shared `lib/` file that all five variants include.

When you search scripts with `dc2tk xref`, it groups identical calls across the five language files and lists each match once.
