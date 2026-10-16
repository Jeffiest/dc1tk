# Guide: Getting Started

This guide explains how to set up `dc2tk`, extract game files, and create your first mod for Dark Chronicle (Dark Cloud 2).

For details on how the original game works, see [DC2 Internals](../internals/README.md). For details on writing C scripts, see [Scripting](../scripting/README.md).

## What you need

Before you begin, make sure you have these three items:

1. **A PAL PlayStation 2 `.iso` file** of Dark Chronicle (Dark Cloud 2). Obtain this legally from your own game disc and name the file `dc2.iso`.
2. **The `dc2tk` command-line tool**. Download and extract the latest `dc2tk` release into a folder on your computer (or build it from source).
3. **The PCSX2 emulator**. Install PCSX2 so you can run and test your modded game ISO.

## Extracting the game files

Run the `setup` command to unpack `dc2.iso` and convert its contents into editable formats:

1. Copy `dc2.iso` into your working folder.
2. Open a terminal in that folder.
3. Run the setup command:
   ```sh
   dc2tk setup dc2.iso
   ```

`dc2tk setup` creates the `extracted/` folder with five subfolders:

- `extracted/assets/`: Textures (PNG), 3D models (glTF), sound effects (WAV), music (MIDI and SoundFont), and menu text (UTF-8 `.txt`). It also contains `manifest.json`, which maps each asset ID to its locations on the disc.
- `extracted/data/`: Game data tables (such as items, weapons, shops, and monsters) exported as editable `.json` files.
- `extracted/maps/`: Map scenes exported as `.gltf` files that you can open and edit in Blender.
- `extracted/scripts/`: All game scripts decompiled into readable `.c` source files, along with shared code in `lib/` and constants in `include/disc.h`.
- `extracted/disc/`: The raw unpacked files from `DATA.DAT` and the disc root, along with `disc.json` and `disc-structure.bin`. Keep `extracted/disc/` unmodified so `dc2tk` can use it as a clean reference.

## Starter mod tutorial

You can create a mod in two ways:

- **Method 1**: Edit files inside `extracted/` and package your changes into a mod automatically.
- **Method 2**: Keep `extracted/` untouched and create a mod directly inside the `mods/` folder.

### Method 1: Edit extracted files directly (Quickest way)

The fastest way to make a mod is to edit files in `extracted/` and run `dc2tk mod make`.

You can edit:
- Scripts in `extracted/scripts/` (`.c` files)
- Textures, models, sounds, and text in `extracted/assets/` (PNG, glTF, WAV, TXT)
- Raw game files in `extracted/disc/` (you can also add new files there)

Follow these steps to create and play a starter mod:

1. Open an asset in `extracted/assets/` and make a change. For example, open a texture `.png` file in an image editor, change its colors, and save the file.
2. Check which files you modified by running:
   ```sh
   dc2tk diff dc2.iso
   ```
   `dc2tk diff` lists every script, asset, and disc file that changed since extraction.
3. Package your changes into a new mod named `mymod`:
   ```sh
   dc2tk mod make dc2.iso mymod
   ```
   This command creates the `mods/mymod/` folder and writes `mods/mymod/mod.json`:
   - Changed scripts are copied into the mod as whole files.
   - Changed assets in `extracted/assets/` are added as `replace` entries by asset ID. Replacing an asset by ID updates every copy of that asset on the disc.
   - Changed or added files in `extracted/disc/` are added as `file:` entries.
4. Build a new game ISO with your mod and launch it in PCSX2:
   ```sh
   dc2tk build extracted/scripts dc2.iso -o mymod.iso --mod mymod --play
   ```

> **Note**
> Disc files outside `DATA.DAT` (such as video files, `SOUND.DAT`, index files, and the boot program) appear in `dc2tk diff`, but cannot be packaged into a mod.
>
> You can also pass `extracted/disc` instead of `dc2.iso` to `build`, `diff`, and `mod make`. If you do this, keep `extracted/disc` unmodified. `dc2tk build` packs the input folder directly, so any edits inside `extracted/disc` will be baked into the base image underneath your mod.

### Method 2: Create a clean mod in `mods/`

To keep `extracted/` completely unmodified, use the `dc2tk mod` subcommands to copy only the assets or script functions you want to change into `mods/<name>/`.

**To replace an asset or data table:**

1. Copy the asset into a new mod folder (for example, `mymod`) by its asset ID:
   ```sh
   dc2tk mod asset dc2.iso mymod data:item
   ```
   You can also copy textures (`texture:e254a1`), models (`model:c02fuku*`), sounds (`sound:SE_CURSOR_MOVE`), or text (`text:en/shopname`).
2. Open the exported file inside `mods/mymod/` (such as `mods/mymod/data/item.json`), make your edits, and save the file.
3. Build and play your mod:
   ```sh
   dc2tk build extracted/scripts dc2.iso -o mymod.iso --mod mymod --play
   ```

**To patch a single script function:**

1. Create a script patch in `mods/castle/` for a specific function (for example, `start_castle_battle` on map `s51`):
   ```sh
   dc2tk mod override extracted/scripts castle --map s51 start_castle_battle
   ```
2. Open `mods/castle/s51.c` in a text editor, modify the `start_castle_battle` function, and save the file.
3. Build and play your mod:
   ```sh
   dc2tk build extracted/scripts dc2.iso -o modded.iso --mod castle --play
   ```

## Finding your way around

Use these commands to search the game scripts, inspect maps and save flags, and manage your edits:

| Question | Command |
| --- | --- |
| What defines, starts, and requests program 510 on map `s51`? | `dc2tk xref extracted/scripts event 510 --map s51` |
| How does the game enter map `s51`, and at which program? | `dc2tk xref extracted/scripts map s51` |
| Which model and AI script does a monster use, and which scripts place it? | `dc2tk xref extracted/scripts monster golem` |
| Which save flags are in use? What uses flag 54? (Use `NEW_FLAG` for mod flags.) | `dc2tk flags extracted/scripts`, `dc2tk flags extracted/scripts --flag 54` |
| Which save counters are in use? What uses counter 5? (Use `NEW_COUNTER` for mod counters.) | `dc2tk flags extracted/scripts --counters`, `dc2tk flags extracted/scripts --counter 5` |
| Which maps can start a scene, and which maps are playable? | `dc2tk maps extracted/scripts` |
| What programs, free program numbers, trigger points, and positions exist on map `s17`? | `dc2tk map extracted/scripts s17` |
| What files did I change? | `dc2tk changes extracted/scripts` (scripts only), `dc2tk diff dc2.iso` (scripts, assets, and disc files) |
| How do I package my `extracted/` edits into a mod? | `dc2tk mod make dc2.iso mymod` |
| How do I convert whole script files in a mod into function patches? | `dc2tk mod slim extracted/scripts mymod` |

### How files and monsters are named

Script folders and files keep the internal game code and append a readable name (for example, `map/s/s51_raybrandt_castle/` and `dungeon/monster/e33a_golem.c`). When you run `dc2tk` commands, pass the short game code (such as `s51` or `golem`).

The header file `extracted/scripts/include/disc.h` lists the 3D model and AI script for every monster kind:

```c
MONSTER_GOLEM = 168,  // model e33a, AI e33a
```

Multiple monster kinds can share one AI script (for example, all four variants of `e33a`). Story battles can also use an event-only monster kind with its own custom AI script. For example, monster kind 523 (the castle Griffon Soldier) runs `b254a`, while monster kind 37 (the standard Griffon Soldier) runs `e254a`.

### How `xref` and `flags` inspect scripts

`xref` and `flags` read the `.c` files in `extracted/scripts` directly as text. They evaluate constant math expressions (such as `50 * 10 + 0 + 10000` or `story_50`), so they see your edits immediately after you save a file. When a script calculates an argument at runtime, `xref` and `flags` list that call for you to check manually instead of guessing the value.

`xref event` lists every script that requests an event. It also marks the AI scripts of monsters placed on that map (see [Monsters can start map events](../internals/running.md#monsters-can-start-map-events)).

## Keeping your edits

When `setup` and `decompile` write `extracted/scripts`, they save a record of the original files in `original-hashes.json`. `dc2tk` uses this file in three ways:

- `dc2tk changes extracted/scripts` lists every script file you edited, added, or removed.
- `setup` and `decompile` refuse to overwrite files you edited unless you pass `--force`.
- `dc2tk mod save extracted/scripts castle` copies your edited and added script files into `mods/castle`.

For long-term modding, keep `extracted/scripts` unmodified and put your changes in `mods/`:

- Run `dc2tk mod override extracted/scripts castle --map s51 start_castle_battle` to create a patch file containing only that function.
- Run `dc2tk build extracted/scripts dc2.iso -o modded.iso --mod castle --mod another` to merge one or more mods into the target scripts during the build.

Script patches store only the functions you change. Because of this, you can re-extract `extracted/scripts` after updating `dc2tk` and still use the same mods. Multiple mods can also patch the same script file.

If a `dc2tk` update renames a function in the definitions, `build` still finds your overridden function using the code fingerprint stored in the mod. `build` prints the new function name, and `dc2tk mod check extracted/scripts castle --rename` updates the function name inside your patch file.

If you use whole script files instead of patches (for example, with `mod new` or `mod make`), `dc2tk mod list` warns you when a copied file came from an older decompiler version. You can run `dc2tk mod slim extracted/scripts castle` to convert copied whole files into function patches whenever the compiled output is identical. For full details, see [Creating Mods](mods.md).

## Asset and modding guides

- [Creating Mods](mods.md): How to configure `mod.json`, write patches and pragmas, allocate new IDs, combine mods, replace assets by ID, run unit tests, and skip straight to a scene with `start_at`.
- [Editing Scripts](scripts.md): How the decompiled script tree is organized, how to edit shared library code, how to wrap engine natives, and how language variants work.
- [CLI Reference](commands.md): Every `dc2tk` command, shared options, and how decompiling, compiling, and building work.
- [Textures](textures.md): How to replace and add 2D images and textures using PNG files.
- [Sounds](sounds.md): How to replace and add sound effects using WAV files.
- [Music](music.md): How to edit background music and ambient loops using MIDI files, and how to add new music tracks.
- [Text](text.md): How to edit menu text, item names, and system messages as UTF-8 text.
- [Models](models.md): How to edit character models and animations as glTF files, and how to add custom characters.
- [Maps](maps.md): How to edit map scenes in Blender using glTF, add new maps, and connect doors.
- [Dungeons](dungeons.md): How to edit randomly generated dungeon floors, dungeon parts, and room layouts.
- [Game Data](data.md): How to edit items, shops, monsters, and other data tables as JSON.
- [Memory Limits](memory.md): PlayStation 2 memory limits and how `dc2tk build` checks them.
