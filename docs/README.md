# DC2-Toolkit

<p align="center">
  <img src="docs/images/header.png" alt="A Dark Chronicle town map with its trigger points, beside decompiled scripts" width="830">
</p>

<p align="center">
  <a href="https://github.com/fakegenie/DC2-Toolkit/"><b>Github</b></a>
  &nbsp;&nbsp;·&nbsp;&nbsp;
  <a href="https://github.com/fakegenie/DC2-Toolkit/releases"><b>Download Releases</b></a>
</p>

**DC2-Toolkit / dc2tk** is a set of tools for modifying the PS2 game Dark Chrinoicle/Dark Cloud 2. With them you can extract or modify almost everything in the game and even create total conversions. We support extracting, converting and injecting:

- **Scripting** DC2 heavily uses it's own scripting language for almost all behaviour in the game. dc2tk can extract all the game's scripts as human readable C code, and compile them back into the game's original bytecode format.
- **Mapping** All game maps can be extracted to modern formats, and we provide a blender plugin to help editing/creating them and a compiler to convert them to the original DC2 Format.
- **Models/Textures/Audio** We can extract and convert all to gltf/png/wav/midi files, and can convert them all back. 
- **Text/Config/Data** Menus, dialogue, shop items, etc in every language, all editable as text and json files

The goal of this project is to support converting from and to the original game formats, not creating anything new. All tools support doing a full byte-identical round-trip. The tools can output an updated .iso image you can play on a ps2 or emulator, and probably most recomp ports.

> **No game files are included.** This repository holds tools and documentation,
> not decompiled code or assets. You need your own legally obtained copy of a
> PAL PS2 version of the game for anything here to work.


## Quick Start

- Download the latest release.
- Extract the zip somewhere and copy your PAL iso to the folder as "dc2.iso"
- Run `dc2tk setup dc2.iso`. This will extract all the assets on the disc to `extracted/`
- Optinonally install the blender plugin from the releases page.
- Modify the files in the extracted folder, see the documentation for some how-tos.
- Run `dc2tk build extracted dc2.iso -o modded.iso` to build a new iso with your changes.

## Notes

- We only support PAL at the moment
- We do not extract/decompile/compile the engine code itself, there exists other projects for this such as [ChronicleTwo](https://github.com/TheMoonPeople/ChronicleTwo)

# Documentation

Three sections, each with its own index:

- **[The game](game/README.md)**: how Dark Chronicle works and how its files are made: the disc,
  the script format and how scripts run, maps, dungeon floors, models, textures, sounds, music,
  message text, game data and memory. Facts about the game, whatever tool you use.
- **[Modding](modding/README.md)**: how to make a mod and edit each kind of asset: getting
  started, mods and `mod.json`, editing scripts, every `dc2tk` command, and a page for each kind
  of asset (textures, sounds, music, text, models, maps, dungeon floors, data tables) and for
  staying within the game's memory.
- **[Coding](coding/README.md)**: writing script code: a tour of the game's scripts, the script
  language and its standard C, the SDK, editor setup, tutorials, the
  [full API](coding/api/README.md) (every native, helper and constant), and the two example mods,
  [the calculator](coding/examples/calculator/calculator.c) and
  [the Spinning Cult](coding/examples/spinning-cult/README.md).

About this project's own code: [src/README.md](../src/README.md) and [src/docs/](../src/docs/)
(how the tools work, why they are built this way, and the rules for changing them);
[definitions/README.md](../definitions/README.md) for the definition sets that name the
decompiled scripts; [blender-plugin/README.md](../blender-plugin/README.md) for the Blender
add-on.

