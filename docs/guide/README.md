# Getting Started

This guide walks you through extracting *Dark Chronicle* (*Dark Cloud 2*) and creating your first mod.

## What you need

1. **`dc2.iso`**: Your own PAL PlayStation 2 `.iso` copy of *Dark Chronicle* (`SCES_511.90`).
2. **`dc2tk`**: Download and extract the latest release zip from [GitHub Releases](https://github.com/fakegenie/DC2-Toolkit/releases).
3. **[PCSX2](https://pcsx2.net/)**: PlayStation 2 emulator to play and test your modded `.iso`.

## Step 1: Extract the game

Copy `dc2.iso` into the `dc2tk` folder, open a terminal (Command Prompt or PowerShell) in that folder, and run:

```sh
dc2tk setup dc2.iso
```

This creates the `extracted/` folder:

| Folder | Contents |
| --- | --- |
| `extracted/data/` | Game data tables as JSON (items, weapons, shops, monsters, inventions) |
| `extracted/assets/` | Textures (`.png`), 3D models (`.gltf`), sounds (`.wav`), music (`.mid`), and menu text (`.txt`) |
| `extracted/maps/` | 3D maps exported as `.gltf` scenes for Blender |
| `extracted/scripts/` | All ~2,500 game scripts decompiled into `.c` files |
| `extracted/disc/` | Raw unpacked disc files (used by `dc2tk` to rebuild the ISO) |

## Step 2: Make your first starter mod

The fastest way to make a mod is to edit files directly in `extracted/` and let `dc2tk` package your changes into a reusable mod folder.

### Example: Give Max a 999-Attack Battle Wrench and a custom texture

1. **Edit an asset or script**:
   - Open `extracted/assets/textures/` and recolor any `.png` texture in an image editor, **or**
   - Open a script in `extracted/scripts/` (such as `extracted/scripts/mainchr/c01_max.c`) in any text editor and save a change.
2. **Check what you changed**:
   ```sh
   dc2tk diff dc2.iso
   ```
   `dc2tk` lists every script, texture, model, or sound you modified.
3. **Package your changes into a mod**:
   ```sh
   dc2tk mod make dc2.iso my-first-mod
   ```
   This creates `mods/my-first-mod/` with a `mod.json` file and copies of your edited files.
4. **Build and play your modded ISO**:
   ```sh
   dc2tk build extracted/scripts dc2.iso -o modded.iso --mod my-first-mod --play
   ```
   This builds `modded.iso` and launches it in PCSX2.

> **Tip: Creating a clean mod from scratch**
> Instead of editing `extracted/` directly, you can copy individual assets or functions straight into a mod folder in `mods/`:
>
> ```sh
> dc2tk mod asset dc2.iso super-wrench data:items          # Copy the items table into mods/super-wrench/
> dc2tk mod asset dc2.iso super-wrench texture:e254a1      # Copy a texture PNG into mods/super-wrench/
> dc2tk mod override extracted/scripts super-wrench --map s51 start_castle_battle
> ```
> See [Creating Mods](mods.md) for details on `mod.json` and patches.

## Finding your way around

Not sure which map, monster, or script controls a scene? Use these search commands:

| What you want to find | Command |
| --- | --- |
| List all maps in the game | `dc2tk maps extracted/scripts` |
| Inspect a map's events, triggers, and spawn points | `dc2tk map extracted/scripts s51` |
| See how the game enters map `s51` | `dc2tk xref extracted/scripts map s51` |
| Find which model and AI script a monster uses, and where it spawns | `dc2tk xref extracted/scripts monster golem` |
| See what triggers event `510` on map `s51` | `dc2tk xref extracted/scripts event 510 --map s51` |
| Check if save flag `54` is used by the game | `dc2tk flags extracted/scripts --flag 54` |

## Where to go next

- **[CLI Reference](commands.md)**: Common commands and options.
- **[Creating Mods](mods.md)**: How `mods/<name>/mod.json`, script patches, and `--start-at` work.
- **Editing Assets**:
  - [Game Data (JSON)](data.md) — Items, weapons, shops, monsters, inventions, fishing, and Georama.
  - [Textures (PNG)](textures.md) — Replace or add 2D and 3D textures.
  - [Text and Menus](text.md) — Edit menu strings, item names, and descriptions.
  - [Sound Effects (WAV)](sounds.md) — Replace or add sound effects.
  - [Music (MIDI)](music.md) — Replace or add background music and custom instruments.
  - [3D Models (glTF)](models.md) — Edit character models, outfits, and animations.
  - [Maps and Blender](maps.md) — Edit 3D maps in Blender and add custom rooms and doors.
  - [Dungeons](dungeons.md) — Edit random dungeon floor layouts and chests.
  - [Memory Limits](memory.md) — Keep your edits within PlayStation 2 memory limits.
- **[Scripting](../scripting/README.md)**: Learn how scripts work and write custom C code.
