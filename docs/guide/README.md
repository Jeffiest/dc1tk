# Getting Started

This guide shows you how to extract *Dark Chronicle* (*Dark Cloud 2*) and build your first mod.

## What you need

1. **`dc2.iso`**: Your PAL PlayStation 2 `.iso` copy of *Dark Chronicle* (`SCES_511.90`).
2. **`dc2tk`**: Download and extract the latest release zip from [GitHub Releases](https://github.com/fakegenie/DC2-Toolkit/releases).
3. **[PCSX2](https://pcsx2.net/)**: PlayStation 2 emulator to run and test your modded `.iso`.

## Step 1: Extract the game

Copy `dc2.iso` into the `dc2tk` folder, open a terminal in that folder, and run:

```sh
dc2tk setup dc2.iso
```

This command creates the `extracted/` directory:

| Folder | Contents |
| --- | --- |
| `extracted/data/` | Game data tables as JSON (items, weapons, shops, monsters, inventions) |
| `extracted/assets/` | Textures (`.png`), 3D models (`.gltf`), sounds (`.wav`), music (`.mid`), and menu text (`.txt`) |
| `extracted/maps/` | 3D maps exported as `.gltf` scenes for Blender |
| `extracted/scripts/` | All ~2,500 game scripts decompiled into `.c` files |
| `extracted/disc/` | Unpacked raw disc files used to rebuild the ISO |

## Step 2: Make your first starter mod

Edit files in `extracted/` and package your changes into a reusable mod folder.

### Example: Give Max a 999-Attack Battle Wrench

1. Open `extracted/data/items.json` in a text editor, search for `"Battle Wrench"`, and change `"attack": 11` to `"attack": 999`:
   ```json
         "id": 2,
         "name": { "en": "Battle Wrench" },
         "weapon": {
           "attack": 999,
           "durable": 20
         }
   ```
2. Check your change:
   ```sh
   dc2tk diff dc2.iso
   ```
3. Package the change into a mod folder (`mods/super-wrench/`):
   ```sh
   dc2tk mod make dc2.iso super-wrench
   ```
4. Build `modded.iso` and launch it in PCSX2:
   ```sh
   dc2tk build extracted/scripts dc2.iso -o modded.iso --mod super-wrench --play
   ```

> **Tip: Copy assets directly into a mod**
> Instead of editing `extracted/`, copy individual assets or functions directly into `mods/<name>/`:
>
> ```sh
> dc2tk mod asset dc2.iso super-wrench data:items
> dc2tk mod asset dc2.iso super-wrench texture:e254a1
> dc2tk mod override extracted/scripts super-wrench --map s51 start_castle_battle
> ```
> See [Creating Mods](mods.md) for details on `mod.json` and patches.

## Finding your way around

Use these search commands to find maps, monsters, events, and save flags:

| Task | Command |
| --- | --- |
| List all maps | `dc2tk maps extracted/scripts` |
| Inspect a map's events, triggers, and spawns | `dc2tk map extracted/scripts s51` |
| Find scripts that warp into map `s51` | `dc2tk xref extracted/scripts map s51` |
| Find a monster's model, AI script, and spawns | `dc2tk xref extracted/scripts monster golem` |
| Find what triggers event `510` on map `s51` | `dc2tk xref extracted/scripts event 510 --map s51` |
| Check if save flag `54` is in use | `dc2tk flags extracted/scripts --flag 54` |

## Where to go next

- **[CLI Reference](commands.md)**: Commands and options.
- **[Creating Mods](mods.md)**: `mods/<name>/mod.json`, script patches, and `--start-at`.
- **Editing Assets**:
  - [Game Data (JSON)](data.md) — Items, weapons, shops, monsters, inventions, fishing, and Georama.
  - [Textures (PNG)](textures.md) — Replace or add 2D and 3D textures.
  - [Text and Menus](text.md) — Edit menu strings, item names, and descriptions.
  - [Sound Effects (WAV)](sounds.md) — Replace or add sound effects.
  - [Music (MIDI)](music.md) — Replace or add background music and custom instruments.
  - [3D Models (glTF)](models.md) — Edit character models, outfits, and animations.
  - [Maps and Blender](maps.md) — Edit 3D maps in Blender and add custom rooms and doors.
  - [Dungeons](dungeons.md) — Edit random dungeon floor layouts and chests.
  - [Memory Limits](memory.md) — Keep edits within PlayStation 2 memory budgets.
- **[Scripting](../scripting/README.md)**: Write custom C scripts.
