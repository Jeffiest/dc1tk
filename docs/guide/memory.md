# Memory Limits

The PlayStation 2 has a fixed amount of memory, and Dark Chronicle loads files into fixed-size memory buffers. Many assets on the retail disc already use nearly 100% of their allocated buffers. If a modded file exceeds its buffer size, the game can crash or overwrite adjacent memory.

To protect your mod from crashes, `dc2tk build` (and `dc2tk build --dry-run`) automatically checks every modified file against the game's memory budgets:

- **File read buffer**: The temporary buffer used when reading a whole file from the disc (`3,200,000` bytes).
- **Map scripts**: Maximum `312,816` bytes per script (some retail map scripts are within `3,152` bytes of this limit).
- **Map object placements**: Maximum `320` placements in towns and interiors, or `400` in dungeons.
- **Texture blocks**: Available Graphics Synthesizer (GS) VRAM blocks for map, character, sky, and effect textures.
- **Sound memory**: SPU2 audio RAM regions and Emotion Engine (EE) sound header stacks.
- **3D skinned meshes**: Maximum `3,198` vertices and `32` bones per skinned mesh.

For the exact byte limits and retail usage of each buffer, see [Memory Budgets](../internals/memory.md). Row count limits for JSON data tables are checked separately when `dc2tk` reads those tables (see [Game Data](data.md)).

## How `dc2tk build` reports memory usage

Whenever `dc2tk build` writes a disc file whose bytes changed, it checks all memory budgets that file participates in and prints one of three prefixes:

- **`memory:`**: Shows how much of each memory budget the modified asset uses.
- **`error:`**: A known, hard memory limit was exceeded. `dc2tk build` still writes the output ISO file (if `-o` was specified), but warns you that the game will fail to load the asset and exits with code `1`.
- **`warning:`**: An estimated memory limit was exceeded, or an asset grew larger than the largest working example on the retail disc.

### How map types are identified

Different map types (`town`, `interior`, and `dungeon`) have different memory rules:

1. `dc2tk build` looks up each map's type from its row in the built disc's map tables (`map/map<language>.cfg`).
2. A new map added by a mod uses the map type of its `"like"` template map.
3. If a modified map file is not listed in any map table, `dc2tk` determines its type from the first letter of its filename (`d` = dungeon, `i` = interior, any other letter = town).

## What build checks

`dc2tk build` performs the following checks on modified files:

- **Maps** (when `.map`, `.cfg`, `.mpk`, `.ipk`, `.efp`, `.sky`, or a custom map's files change):
  - **Read buffer and loading screen**: Checks file size against the read buffer and loading screen memory (exceeding the loading screen buffer is an `error:` in a town map and a `warning:` in a dungeon map).
  - **Placements**: Checks total object placements against the table limit (`320` for towns/interiors, `400` for dungeons).
  - **Texture blocks**: Reports an `error:` if textures exceed the map's total texture blocks, and a `warning:` if a town map exceeds `24` blocks when the original retail map did not.
  - **Estimated map memory**: Because no single fixed number caps a map's total memory, `build` compares a changed map's estimated memory usage against the largest retail map of the same kind. It prints a `warning:` if the map grew and now exceeds that retail maximum.
- **New map table rows**: Rejects adding new maps if the new rows would overflow the map table's fixed memory buffer (see [Adding a new map](maps.md#a-new-map)).
- **Map scripts (`map/**.stb`)**: Checks the compiled `.stb` size against the `312,816`-byte script buffer.
- **Character packs (`.chr`)**:
  - Checks the pack size against the file read buffer.
  - Checks every skinned mesh against the `3,198`-vertex and `32`-bone limits.
  - For Max and Monica's playable character packs, checks the estimated memory footprint against their character memory stacks.
  - Checks the pack's textures against its allocated texture blocks.
- **Skies (`.sky`)**: Checks the file against the read buffer, and checks that each texture file inside the sky fits within a single texture block.
- **Map effects (`.efp`)**: Checks that all textures in the `.efp` pack fit within their single shared texture block.
- **Sound banks (`.snd`)**:
  - Checks the bank size against the file read buffer and the IOP transfer window.
  - Checks the bank against its SPU2 audio memory region (reports an `error:` for music and system banks, or a `warning:` for menu and character banks).
  - Checks every sound set that loads the bank alongside other field banks (reports an `error:` if the combined set exceeds the field sound region).
  - Prints a `warning:` whenever a field, event, or monster sound bank grows larger, because the fullest combination of banks loaded by retail scripts leaves only `5,792` bytes to spare.
  - If the Emotion Engine (EE) tables grew in a music, base, battle, ambient, or object sound bank, checks their estimated size against that layer's EE stack and prints a `warning:` if exceeded (object banks in the same sound set are added together; see [The sound bank format](../internals/sounds.md#8-memory-the-game-gives-a-bank)).
- **Other files**: If any other file on the disc grows larger, `build` prints a note stating that no specific memory budget covers that file.

### What is not checked automatically

The following memory areas are not checked by `dc2tk build`:

- **Monster and event sound bank combinations**: How monster and event banks combine with each field sound set was measured by tracing scripts (see [Memory Budgets](../internals/memory.md)), but is not recalculated during `build`.
- **EE tables for event and monster sound banks**: The exact memory stack where the engine places these tables has not been traced.
- **Ridepod and Monster Transformation stacks**: Memory stacks for the Ridepod and Monica's monster badges are not estimated.
- **Shared memory pool**: Remaining free space in the general shared memory pool is not simulated.
- **Menu and dungeon text files**: Exact memory limits for message files inside menu memory and the dungeon heap are unknown (see [Text limits](../internals/text.md#limits)).

## Tips for staying within memory limits

- **Run `dc2tk build --dry-run` often**: You can check memory usage in a few seconds without waiting to write a 4.7 GB ISO file.
- **Put new assets inside existing packs**: Adding a new member to an existing `.chr` or `.mpk` pack uses zero space in the `DATA.HD4` boot index (which only has room for about 150 new top-level files).
- **Use script patches instead of copying shared functions**: Keep shared code in libraries and override only the functions you need so map scripts stay below the `312,816`-byte limit.
- **Keep textures and sound samples compact**: When replacing textures or WAV sounds in a crowded map or sound bank, match or reduce the resolution, color depth, or sample length of the original asset.
