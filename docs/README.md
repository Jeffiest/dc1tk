# DC2-Toolkit

<p align="center">
  <img src="images/header.png" alt="A Dark Chronicle town map with its trigger points, beside decompiled scripts" width="830">
</p>

<p align="center">
  <a href="https://github.com/fakegenie/DC2-Toolkit/"><b>GitHub Repository</b></a>
  &nbsp;&nbsp;·&nbsp;&nbsp;
  <a href="https://github.com/fakegenie/DC2-Toolkit/releases"><b>Download Releases</b></a>
</p>

**DC2-Toolkit (`dc2tk`)** is a command-line toolkit for modifying the PlayStation 2 game *Dark Chronicle* (*Dark Cloud 2*). You can use `dc2tk` to make small tweaks, create custom items and cutscenes, or build total conversion mods.

`dc2tk` extracts the original game files into standard, editable formats and compiles your changes back into a playable `.iso` disc image. If you do not edit a file, `dc2tk` rebuilds it byte-for-byte identical to the original retail disc.

> **Important:** This repository does not include any copyrighted game files. You must provide your own legally obtained PAL PlayStation 2 copy of *Dark Chronicle* (`SCES_511.90`).

## What You Can Edit

- **Game Data and Text:** Edit item stats, weapon buildup trees, shop inventories, monster stats, inventions, Georama conditions, and menu text using plain text and JSON files.
- **Textures, Models, and Audio:** Convert game textures to `.png`, 3D character models and animations to `.gltf`, sound effects to `.wav`, and background music to `.mid`.
- **Maps and Dungeons:** Edit 3D town maps, interiors, and boss arenas in Blender using the `dc2tk` Blender add-on. Customize random dungeon floor layouts and treasure chests.
- **Scripts:** Decompile all ~2,500 game scripts into readable C code. Edit NPC dialogue, cutscenes, enemy AI, player combat actions, and visual effects, then compile them back to game bytecode.

## Quick Start

1. Download the latest release zip from the [Releases page](https://github.com/fakegenie/DC2-Toolkit/releases) and extract it to a folder on your computer.
2. Copy your PAL *Dark Chronicle* `.iso` file into that folder and rename the file to `dc2.iso`.
3. Open a terminal (Command Prompt or PowerShell) in that folder and run the setup command:

   ```sh
   dc2tk setup dc2.iso
   ```

   This command extracts the disc contents, decompiles the scripts, and exports editable assets into the `extracted/` folder.
4. Edit files in `extracted/` or create a mod folder in `mods/`. Follow the [Getting Started Guide](guide/README.md) for a step-by-step starter mod tutorial.
5. Build a new playable `.iso` file with your changes:

   ```sh
   dc2tk build extracted/scripts dc2.iso -o modded.iso
   ```

6. Open `modded.iso` in the [PCSX2](https://pcsx2.net/) PlayStation 2 emulator to test and play your mod.

## Documentation Sections

Choose the section that matches what you want to do:

### 1. [Guide](guide/README.md)

**For beginners and mod creators.** Learn how to set up the toolkit, create your first mod, and edit each type of game asset.

- **[Getting Started & Starter Mod Tutorial](guide/README.md):** Extract the game, make your first mod step-by-step, and find files in the project.
- **[CLI Reference](guide/commands.md):** Learn how to use every `dc2tk` command and option.
- **[Creating Mods](guide/mods.md):** Organize your mod with `mod.json`, write function patches, allocate free IDs, and test scenes quickly.
- **Asset Guides:**
  - [Game Data (JSON)](guide/data.md) — Items, shops, monsters, inventions, fishing, and Georama tables.
  - [Textures (PNG)](guide/textures.md) — Recolor characters, UI sprites, and environment textures.
  - [Text and Menus](guide/text.md) — Edit item names, descriptions, and system messages.
  - [Sound Effects (WAV)](guide/sounds.md) — Replace sound effects in sound banks.
  - [Music (MIDI)](guide/music.md) — Edit background music sequences and custom instruments.
  - [3D Models (glTF)](guide/models.md) — Edit character meshes, skeletons, and animations.
  - [Maps and Blender](guide/maps.md) — Edit 3D maps in Blender and add custom maps and doors.
  - [Dungeons](guide/dungeons.md) — Customize dungeon floor layouts, parts, and chests.
  - [Editing Scripts](guide/scripts.md) — Patch shared script libraries, wrap native functions, and support all 5 PAL languages.
  - [Memory Limits](guide/memory.md) — Keep your mod within the PlayStation 2 hardware memory budgets.

### 2. [Scripting](scripting/README.md)

**For modders who want to edit game logic or write custom C scripts.**

- **[Scripting Overview & Script Lifetime](scripting/README.md):** Understand how the engine and scripts interact, how the 5 script types work, how coroutines and `wait;` execute each frame, and how script state persists.
- **[Script Language Guide](scripting/language.md):** Learn the C syntax, built-in statements, pragmas, and data types used by `dc2tk`.
- **[C89 Standard Support](scripting/c-standard.md):** Reference of supported ANSI C (C89) features, structs, pointers, and compiler limits.
- **[SDK Headers and Modkit](scripting/sdk.md):** Learn how `<event.h>`, `<monster.h>`, `<action.h>`, `<effect.h>`, `<disc.h>`, and the `modkit` helper libraries work.
- **[Editor Setup](scripting/editor.md):** Set up VS Code or any `clangd` editor for autocomplete and inline documentation.
- **[Example Guides (Tutorials)](scripting/tutorials/README.md):** Step-by-step guides for common scripting tasks:
  - [Change What an NPC Says](scripting/tutorials/npc-line.md)
  - [Change Enemy Behavior](scripting/tutorials/enemy-behaviour.md)
  - [Add a New Enemy to a Fight](scripting/tutorials/new-enemy.md)
  - [Add a Cutscene to a Town](scripting/tutorials/cutscene.md)
  - [Tune Player Combat](scripting/tutorials/tune-player.md)
  - [Show Live Captions During a Fight](scripting/tutorials/captions.md)
- **[Complete Example Mods](scripting/examples/spinning-cult/README.md):** Inspect two full mods included in the repository: the [Calculator mod](scripting/examples/calculator/calculator.c) and [The Spinning Cult](scripting/examples/spinning-cult/README.md).
- **[API Reference](scripting/api/README.md):** Complete documentation for all ~1,100 engine native functions, SDK helpers, program numbers, and constants.

### 3. [DC2 Internals](internals/README.md)

**For reverse engineers and tool developers.** Deep technical documentation of the original *Dark Chronicle* game engine and binary file formats, verified against the PAL disc and the [ChronicleTwo](https://github.com/TheMoonPeople/ChronicleTwo) engine decompilation.

- **[Internals Overview](internals/README.md):** Architecture of the engine and scripting system.
- **[Disc Layout and Archives](internals/disc.md):** ISO 9660 layout, `DATA.DAT`, `DATA.HD4`, and pack containers.
- **[Script Bytecode and VM](internals/scripts.md):** `.stb` binary format, virtual machine instruction set, and native function tables.
- **[Script Execution and Runtime](internals/running.md):** Coroutine scheduling, event mode, save flags, counters, monster variables, and message control codes.
- **[Map Formats](internals/maps.md):** `.map`, `.pcp`, `.mpk`, `.ipk`, collision meshes, and trigger points.
- **[Dungeon Floor Formats](internals/dungeons.md):** Procedural floor generation, room scripts, and chest tables.
- **[3D Model Formats](internals/models.md):** `.mds` meshes, `.wgt` and `.bbp` skinning, `.mot` keyframe animations, and `.chr` packs.
- **[Texture Formats](internals/textures.md):** `.img` containers, TIM2 pixel formats, palettes, and GS VRAM layout.
- **[Sound Bank Formats](internals/sounds.md):** `.snd` packs, `.cfg`, `.vol`, `.hd`, `.bd`, and SPU2 voice allocation.
- **[Music Sequence Formats](internals/music.md):** `.sq` sequences, `.hed` instrument banks, and the IOP MIDI driver.
- **[Message Text and Font Formats](internals/text.md):** `.mes` binary text files, glyph encoding, and font metrics.
- **[Game Data Tables](internals/data.md):** Binary record layouts for `cfg` files and executable tables in `SCES_511.90`.
- **[Memory Layout](internals/memory.md):** Fixed buffers in EE main RAM, GS VRAM, and SPU2 sound RAM.

## Notes and Scope

- **PAL version only:** `dc2tk` currently supports the PAL PlayStation 2 version of *Dark Chronicle* (`SCES_511.90`).
- **Engine code:** `dc2tk` modifies disc assets, scripts, and data tables. It does not decompile or recompile the C++ game engine executable itself. For engine reverse engineering, see [ChronicleTwo](https://github.com/TheMoonPeople/ChronicleTwo).
- **Contributing to `dc2tk`:** To learn how the Rust workspace is structured and tested, read [src/README.md](../src/README.md), [src/docs/](../src/docs/), [definitions/README.md](../definitions/README.md), and [blender-plugin/README.md](../blender-plugin/README.md).
