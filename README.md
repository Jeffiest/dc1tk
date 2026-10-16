# DC2-Toolkit

<p align="center">
  <img src="docs/images/header.png" alt="A Dark Chronicle town map with its trigger points, beside decompiled scripts" width="830">
</p>

<p align="center">
  <a href="https://fakegenie.github.io/DC2-Toolkit/"><b>Documentation</b></a>
  &nbsp;&nbsp;·&nbsp;&nbsp;
  <a href="https://github.com/fakegenie/DC2-Toolkit/releases"><b>Download Releases</b></a>
</p>

**DC2-Toolkit / dc2tk** is a set of tools for modifying the PS2 game Dark Chronicle/Dark Cloud 2. With them you can extract or modify almost everything in the game and even create total conversions. We support extracting, converting and injecting:

- **Scripting** DC2 heavily uses its own scripting language for almost all behaviour in the game. dc2tk can extract all the game's scripts as human readable C code, and compile them back into the game's original bytecode format.
- **Mapping** All game maps can be extracted to modern formats, and we provide a Blender plugin to help editing/creating them and a compiler to convert them to the original DC2 Format.
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
- Optionally install the Blender plugin from the releases page.
- Make a mod in `mods/`, or edit the scripts and assets in `extracted/` and package them with `dc2tk mod make dc2.iso my-mod`. See the documentation for how-tos.
- Run `dc2tk build extracted/scripts dc2.iso -o modded.iso --mod my-mod` to build a new iso with your changes.

## Notes

- We only support PAL at the moment
- We do not extract/decompile/compile the engine code itself, there exists other projects for this such as [ChronicleTwo](https://github.com/TheMoonPeople/ChronicleTwo)

## Prior art

[ChronicleTwo](https://github.com/TheMoonPeople/ChronicleTwo), [OpenCloud](https://github.com/Gundorada-Workshop/OpenCloud), [SB2-File-Parser](https://github.com/ArsonAssassin/SB2-File-Parser), [DarkCloud-Two-Reforged](https://github.com/ZeldoKavira/DarkCloud-Two-Reforged), [DarkCloud-Toolkit](https://github.com/Gundorada-Workshop/DarkCloud-Toolkit)
