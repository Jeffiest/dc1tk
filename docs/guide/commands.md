# CLI Reference

This page lists every `dc2tk` command, explains all command-line options, and describes how decompiling, asset extraction, compiling, and building work.

For a beginner walkthrough, see [Getting Started](README.md). For instructions on creating mods, see [Creating Mods](mods.md). For details on editing `.c` scripts, see [Editing Scripts](scripts.md).

## Overview

All tools run through a single executable named `dc2tk`. You can download a prebuilt release or compile it from source by running `cargo build --release` in the repository folder (see [Getting Started](README.md)).

To see the options for any command in your terminal, run:

```sh
dc2tk help <COMMAND>
```

When you extract a game disc, `dc2tk` writes everything into the `extracted/` directory:

- `extracted/disc/`: Every raw file from the disc (`DATA.DAT` unpacked alongside movie files, `SOUND.DAT`, and system files) plus the disc layout metadata (`disc.json` and `disc-structure.bin`). `dc2tk pack` uses this folder to rebuild the disc image byte-for-byte.
- `extracted/scripts/`: All decompiled `.c` scripts, shared library files (`lib/`), and generated headers (`include/disc.h`).
- `extracted/assets/`: Extracted textures, 3D models, sound effects, music, and text in standard editable formats.
- `extracted/maps/`: Exported map scenes in glTF format for Blender.
- `extracted/data/`: Exported game data tables in JSON format.

## Commands by category

### Setup and extraction

| Command | Description |
| --- | --- |
| `dc2tk setup dc2.iso` | Extract everything from the disc: raw disc files, scripts, assets, maps, and data tables. |
| `dc2tk extract dc2.iso extracted/disc` | Unpack every raw file on the disc and record its sector layout. |
| `dc2tk pack -o copy.iso --check dc2.iso` | Pack `extracted/disc` into an ISO file and verify that it matches `dc2.iso` byte-for-byte. |
| `dc2tk assets dc2.iso --force` | Export the asset tree into `extracted/assets/` (`setup` runs this automatically). |
| `dc2tk map-export s17` | Export map `s17` as a glTF scene for Blender into `extracted/maps/s17`. |

### Building and playing

| Command | Description |
| --- | --- |
| `dc2tk build extracted/scripts dc2.iso -o modded.iso` | Build a new disc image with your script changes in `extracted/scripts`. |
| `dc2tk build extracted/scripts dc2.iso -o modded.iso --mod castle --mod other` | Build a new disc image and apply one or more mods from `mods/`. |
| `dc2tk build extracted/scripts dc2.iso -o test.iso --start-at s51:story_50` | Build a test ISO where selecting **New Game** starts directly at map `s51`, program `story_50`. |
| `dc2tk build extracted/scripts dc2.iso --dry-run --mod castle` | Check what a build changes and verify memory limits without writing an ISO file. |
| `dc2tk play modded.iso` | Launch an ISO image in PCSX2 (you can also add `--play` to `dc2tk build`). |
| `dc2tk decompile <script.stb \| extracted dir> [-o out]` | Decompile a single `.stb` file or an extracted disc folder into `.c` source files. |
| `dc2tk compile <script.c> [-o script.stb]` | Compile a single `.c` script into a `.stb` bytecode file. |

### Creating and managing mods

For full details on these commands, see [Creating Mods](mods.md#the-mod-commands).

| Command | Description |
| --- | --- |
| `dc2tk mod make dc2.iso mymod` | Package all changes made inside `extracted/` into a ready-to-build mod in `mods/mymod`. |
| `dc2tk mod override extracted/scripts castle --map s51 start_castle_battle` | Create or update a script patch in `mods/castle` that overrides specific functions or natives. |
| `dc2tk mod asset dc2.iso castle texture:e254a1` | Copy an asset (by ID or disc path) into `mods/castle` and add it to `mod.json`. |
| `dc2tk mod check extracted/scripts castle` | Compile all scripts modified by a mod to check for errors without writing an ISO. |
| `dc2tk mod list extracted/scripts` | List every mod in `mods/`, including its patches, allocated IDs, dependencies, and whole files. |
| `dc2tk mod test extracted/scripts calculator` | Run the `tests.c` unit tests in a mod using the built-in VM interpreter without starting the game. |
| `dc2tk mod header extracted/scripts castle` | Generate editor headers (`.editor/`) and `compile_commands.json` for a mod's patches. |
| `dc2tk mod new extracted/scripts castle map/s/s51_raybrandt_castle/s51_1.c` | Copy whole script files from the source tree into `mods/castle/files/`. |
| `dc2tk mod save extracted/scripts castle` | Copy all modified or added scripts from `extracted/scripts` into `mods/castle/files/`. |
| `dc2tk mod slim extracted/scripts castle` | Convert whole script files in a mod into smaller function patches. |
| `dc2tk mod apply extracted/scripts castle` | Copy a mod's whole files directly into `extracted/scripts`. |
| `dc2tk mod migrate dc2.iso castle` | Convert raw files in a mod's `disc/` folder into asset ID entries in `mod.json`. |
| `dc2tk fork extracted/scripts lib/x.c lib/y.c script.c` | Split a shared library file so specific scripts get their own separate copy. |

### Inspecting the game

| Command | Description |
| --- | --- |
| `dc2tk changes extracted/scripts` | List scripts edited, added, or deleted in `extracted/scripts` since decompilation. |
| `dc2tk diff dc2.iso` | List all scripts, assets, and disc files changed in `extracted/` since extraction. |
| `dc2tk maps extracted/scripts` | List every map, its style (town or dungeon), and whether the player can play in it. |
| `dc2tk map extracted/scripts s17` | List a map's event programs, unused program numbers, trigger points, and player positions. |
| `dc2tk map extracted/scripts s17 --json` | Output map metadata as JSON for external tools (programs, player spots, and script names). |
| `dc2tk xref extracted/scripts event 510 --map s51` | Show what defines, starts, and requests event program 510 on map `s51`. |
| `dc2tk xref extracted/scripts map s51` | Show how and where the game enters map `s51`. |
| `dc2tk xref extracted/scripts monster golem` | Show a monster's model, AI script, and every script that loads or places it. |
| `dc2tk flags extracted/scripts [--flag N]` | List used and free save flags, or show every script that uses flag `N`. |
| `dc2tk flags extracted/scripts --counters [--counter N]` | List used and free save counters, or show every script that uses counter `N`. |

### Developer and definition commands

For details on managing symbol definitions, see [Definitions](../../definitions/README.md).

| Command | Description |
| --- | --- |
| `dc2tk verify extracted/disc` | Decompile and recompile every script on the disc to verify byte-for-byte accuracy. |
| `dc2tk disassemble <script.stb>` | Disassemble a compiled `.stb` script into human-readable `.asm` assembly for debugging. |
| `dc2tk assemble <script.asm> -o <script.stb>` | Assemble a `.asm` text file into a `.stb` script file. |
| `dc2tk defs check extracted/disc` | Report how many functions and variables the current definition set names across the disc. |
| `dc2tk defs todo extracted/disc` | List unnamed functions across the disc, ordered from most called to least called. |
| `dc2tk defs learn <source> <definition set>` | Harvest renamed functions and variables from an edited script tree into a definition set. |
| `dc2tk defs rename <definition set> OLD NEW --in extracted/scripts` | Rename a save flag or constant in a definition set and update the script tree. |
| `dc2tk defs migrate extracted/scripts mods/my-mod` | Update an older script tree or mod with symbol renames from the current definition set. |
| `dc2tk defs disc extracted/disc <definition set>` | Record the disc fingerprint that a definition set targets. |
| `dc2tk natives <ChronicleTwo checkout>` | Generate native function tables from a ChronicleTwo engine decompilation checkout. |
| `dc2tk api-docs` | Generate the API documentation in `docs/scripting/api/` from the SDK headers. |

### Input formats (`dc2.iso` vs. `extracted/disc`)

- **Commands that accept either `dc2.iso` or `extracted/disc`**: `setup`, `build`, `assets`, `map-export`, `diff`, `mod make`, `mod asset`, `mod migrate`, `mod check --disc`, and the `--disc` flag on `xref`, `maps`, and `map`. When you pass `extracted/disc` to `build`, `dc2tk` packs that folder into the new ISO first (just like `dc2tk pack`). When you pass `extracted/disc` to `setup`, `dc2tk` decompiles the files already in that folder.
- **Commands that require a `.iso` file**: `extract`, `play`, and `pack --check`.
- **Commands that require the `extracted/disc` folder**: `verify`, whole-disc `decompile`, `defs check`, `defs todo`, and `defs disc`.

### Running the `dc2tk` test suite

If you are developing `dc2tk` itself, run `cargo test --release` in the repository root. Tests that read retail game files are ignored by default. To run those tests as well, set the `DC2TK_CORPUS` environment variable to the absolute path of `extracted/disc` and run:

```sh
cargo test --release -- --include-ignored
```

Without `DC2TK_CORPUS` set, those ignored tests fail and name the missing environment variable.

## Command options

### Options shared by most commands

- `--defs DIR`: Apply a symbol definition set. Repeat this flag to layer multiple sets (later sets override earlier ones). By default, `dc2tk` uses the built-in `definitions/pal` set. Pass `--no-defs` (formerly `--defs none`) to disable definitions completely. Note: `assets`, `mod asset`, and `mod migrate` only use `--defs` to look up sound entry names from the definition set's header.
- `--sdk DIR`: Specify the path to the SDK headers and shared libraries (default: `sdk/`).
- `--natives DIR`: Specify the path to the native function tables (default: `src/natives/`).
- `--table NAME`: (Used with `disassemble`, `decompile`, and `compile`) Force a specific native table (`event`, `monster`, `action`, `effect`, or `arg`) instead of inferring the table from the script's file path.
- `--mods DIR`: (Used with `build` and `mod` subcommands) Specify the directory that contains your mod folders (default: `mods`).
- `--time`: Print the total execution time when the command finishes.

By default, `dc2tk` looks for `sdk/`, `definitions/pal`, and `src/natives/` in the nearest parent directory above the `dc2tk` executable that contains `definitions/`, or in the directory specified by the `DC2TK_ROOT` environment variable.

### Options for individual commands

- **`setup DISC [OUT]`**:
  - `OUT`: Destination folder for decompiled scripts (default: `extracted/scripts`).
  - `--extracted DIR`: Destination folder for raw unpacked disc files (default: `extracted/disc`). If an extraction already exists there, `setup` reuses it.
  - `--assets DIR`: Destination folder for the asset tree (default: `extracted/assets`). Existing files in this folder are replaced. Pass `--no-assets` to skip asset extraction.
  - `--maps DIR`: Destination folder for exported glTF maps (default: `extracted/maps`, equivalent to `map-export --all`). Each map's folder is replaced. Pass `--no-maps` to skip map export.
  - `--data DIR`: Destination folder for exported JSON data tables (default: `extracted/data`, written as `TABLE.json`). Pass `--no-data` to skip data export.
  - `--force`: Overwrite script files in `extracted/scripts` even if you edited them since the last decompile.
- **`pack [EXTRACTED] -o IMAGE`**:
  - Packs `extracted/disc` if you omit `EXTRACTED`.
  - `--check IMAGE`: Compare the newly packed ISO byte-for-byte against an existing ISO file. If any byte differs, `pack` fails and reports the first differing byte and sector.
- **`verify EXTRACTED`**:
  - Prints the first 20 round-trip failures by default. Pass `--limit N` to print up to `N` failures.
  - `--json FILE`: Write the complete verification report (one result per script) to a JSON file. The command exits with an error if any script fails to round-trip.
- **`play IMAGE`**:
  - Skips the PlayStation 2 BIOS boot animation by default (using PCSX2's `-fastboot` flag), matching `build --play`.
  - `--full-boot`: Show the full PS2 startup screens.
  - `--pcsx2 PATH`: Specify the path to the PCSX2 executable. If omitted, `dc2tk` checks the `PCSX2` environment variable, then searches your system `PATH` for `pcsx2-qt` or `pcsx2`, and finally checks standard installation directories.
- **`map-export NAME`**:
  - Exports a map to `extracted/maps/NAME` by default. Pass `-o DIR` to write to another directory.
  - `--all`: Export every map that has object placements into `extracted/maps/NAME` (or `DIR/NAME`).
  - `--list`: Do not export files; instead, print one line per exportable map containing the map code, a tab character, and the map's title in the definition language (or empty if unnamed). The Blender add-on uses this output to populate its map list.
  - `--disc PATH`: Specify the disc image or unpacked disc folder to read from (default: `extracted/disc`).
- **`diff` and `mod make`**:
  - `--extracted DIR`: Specify the root extraction directory containing `disc/`, `scripts/`, and `assets/` (default: `extracted`).
- **`xref`, `maps`, and `map`**:
  - `--disc PATH`: Specify the unpacked disc folder or ISO used to read map titles, monster models, and `.map` files (default: `extracted/disc`; the legacy flag `--extracted` is also accepted). If no disc is found at that path, the command prints a notice and continues without disc metadata.
- **`flags SOURCE`**:
  - Prints a summary of save flags by default.
  - `--used`: List every save flag in use, along with its name and the scripts that reference it.
  - `--flag N`: Show every use of save flag `N`.
  - `--counters`: Inspect save counters instead of save flags.
  - `--counter N` (with `--counters`): Show every use of save counter `N`.
- **`defs todo`**:
  - Lists the top 50 unnamed functions by default. Pass `--limit N` to show `N` functions.
- **`mod apply`**:
  - `--force`: Overwrite files in `extracted/scripts` even if they have local edits.
- **`natives CHECKOUT`**:
  - `--out DIR`: Write the generated native tables to `DIR` instead of `src/natives/`.

### Text formatting and line endings

All text files written by `dc2tk` (decompiled `.c` files, headers, `mod.json`, data JSON files, and reports) use UTF-8 encoding with LF (`\n`) line endings on all operating systems. JSON files use 2-space indentation. Floating-point numbers are formatted as the shortest string that parses back to the exact same value (such as `0.1`, `1.0`, or `1e20`) across C, assembly, and JSON.

When reading files, `dc2tk` accepts both LF and CRLF (`\r\n`) line endings, as well as 1-space JSON from older `dc2tk` versions. Raw binary game files keep their exact original bytes.

## Decompiling

When you pass a single `.stb` file to `dc2tk decompile`, it outputs one self-contained `.c` file containing every function in that script, including shared library functions.

When you pass the `extracted/disc` directory and `-o DIR`, `decompile` creates a complete source tree that mirrors the disc structure:

- Each `.stb` script becomes a `.c` file.
- Functions shared across multiple scripts are moved into `lib/` and included via `#include "lib/..."`. As a result, each individual script file contains only the code unique to that script.
- `compile_flags.txt` is written at the root so C language servers (such as `clangd`) can parse every file with the SDK headers (see [Editor Setup](../scripting/editor.md)).
- `original-hashes.json` records the hash of every generated file. If you run `decompile` or `setup` again, `dc2tk` refuses to overwrite files you have edited unless you pass `--force`. A full-disc decompile also removes obsolete files left over from an earlier decompiler version, except for files you edited (which it keeps and reports).

`dc2tk setup` runs a full-disc decompile automatically after unpacking the ISO. For details on how script folders and files are named, see [Editing Scripts](scripts.md#the-decompiled-tree).

You can also pass a subfolder of `extracted/disc` (for example, `extracted/disc/map/s/s05`). `decompile` then processes only the scripts in that subfolder and writes them to their matching paths in the output tree (such as `out/map/s/s05_fish_monster_swamp/s05.c`), applying the same symbol definitions, native tables, and dialogue text as a full-disc decompile. In a partial decompile, shared code is extracted into `lib/` only among the scripts decompiled in that run.

## The asset tree

`dc2tk assets` extracts textures, character models, sound effects, music, and menu text into standard, editable formats:

- **Textures**: `.png`
- **3D Models**: `.gltf`
- **Sound effects**: `.wav`
- **Music**: `.mid` (MIDI) paired with a `.sf2` SoundFont of the track's instruments
- **Menu and system text**: UTF-8 `.txt`

Many assets appear in multiple packs on the disc. `dc2tk assets` writes each unique asset once and records its asset ID (such as `texture:e254a1`, `model:c02fuku*`, `sound:SE_CURSOR_MOVE`, `music:BG_000_000`, or `text:en/shopname`) and all of its disc locations in `extracted/assets/manifest.json`.

`dc2tk setup` creates `extracted/assets/` automatically (unless you pass `--no-assets`). To regenerate the asset tree later, run:

```sh
dc2tk assets dc2.iso --force
```

You can also pass `extracted/disc` instead of `dc2.iso`, or pass `-o DIR` to write the assets to a different folder.

Mods use these asset IDs in `mod.json` to replace assets across the entire disc at once (see [Creating Mods](mods.md#game-assets-by-id)). For technical details on the asset tree design, see [Design: The asset tree](../../src/docs/design.md#1-the-asset-tree).

## Compiling

`dc2tk compile` reads a `.c` source file, resolves its `#include` directives, and writes a compiled `.stb` bytecode file (defaulting to the input path with a `.stb` extension).

Include paths are resolved in this order:
- **Quoted includes (`#include "..."`)**: Searched from the script's own folder upward through parent folders, and then in `sdk/`.
- **Angle-bracket includes (`#include <...>`)**: Searched in `sdk/`, then in each definition set's `include/` folder, and finally in any `include/` folder in or above the script's directory (where `disc.h` is located).

If you do not modify a decompiled `.c` script, `dc2tk compile` produces a `.stb` file that is byte-for-byte identical to the original retail script.

To prevent stack overflows, the compiler rejects source code nested deeper than 200 parser levels (`parse::NESTING`, which corresponds to roughly 64 nested parentheses, 97 nested blocks, or a chain of 194 operators) with a `"nested too deeply"` error. All retail scripts stay well within this limit.

## Building

A typical modding workflow has four steps:

1. Extract and decompile the game:
   ```sh
   dc2tk setup dc2.iso
   ```
2. Edit scripts in `extracted/scripts/` (or create a mod in `mods/`). Note that editing a file in `extracted/scripts/lib/` affects every script that includes it; use `dc2tk fork` if you want to give specific scripts their own copy of a library. See [How Scripts Run](../internals/running.md) and [Finding Your Way Around](README.md#finding-your-way-around) for help locating scripts.
3. Build a new ISO image:
   ```sh
   dc2tk build extracted/scripts dc2.iso -o modded.iso
   ```
4. Launch `modded.iso` in PCSX2:
   ```sh
   dc2tk play modded.iso
   ```

### How `dc2tk build` works

`dc2tk build` automatically detects which scripts can differ from the original disc: scripts you edited, scripts patched or replaced by a selected `--mod`, and scripts that `#include` a modified library. It skips compiling untouched scripts because they compile to the exact same bytes as the retail disc. As a result, most builds finish in a few seconds.

`dc2tk build` automatically switches to a full recompile (and prints the reason) if:
- You pass the `--full` flag.
- `original-hashes.json` is missing from the script tree.
- The SDK headers or definition headers have changed since the tree was decompiled (for example, after updating `dc2tk`).
- A header in `extracted/scripts/include/` was modified.

When `build` writes a modified file into the new ISO image:

1. **Pack archives**: If the modified file belongs inside a pack archive (such as a `.chr` character pack or `.mpk` map pack), `build` updates that member inside the pack and leaves all other members untouched.
2. **Sector allocation**: If the modified file fits within its original disc sectors, `build` overwrites it in place. If the file grew larger than its original sectors, `build` places it in the space occupied by `DMMY` (a 6.5 MB dummy filler file at the end of the disc that the game never reads). Only if `DMMY` fills up does the ISO file grow larger, in which case `build` updates the ISO 9660 volume size.
3. **Index update**: `build` updates the file's byte size and starting sector in `DATA.HD4`, which is the file table the game uses to read `DATA.DAT`. Because the game calculates each file's position as `DATA.DAT start sector + DATA.HD4 sector offset` (`dataread.cpp`), files placed in `DMMY` after `DATA.DAT` load normally.
4. **Verification**: After writing the new ISO, `build` reads every modified script back out of the new image and verifies that its bytes match the compiled output.

Note that `build` refuses to write the output `-o` image to the same file path as the input `.iso` image, because writing to the input file directly would corrupt it.

### Checking a build without writing an ISO (`--dry-run`)

Pass `--dry-run` instead of `-o modded.iso` to test your build in seconds without writing a 4.7 GB ISO file:

```sh
dc2tk build extracted/scripts dc2.iso --dry-run --mod castle
```

`--dry-run` compiles all modified scripts, processes all asset and map changes (reporting split templates, added or removed object placements, rebuilt models, and repainted textures), and runs all PlayStation 2 [memory checks](memory.md). If any hard memory limit is exceeded, `--dry-run` exits with code `1`, just like a full `build`. The Blender add-on's **Preview Changes** button runs `build --dry-run`.

### Testing without replaying

You can configure a test build so that selecting **New Game** on the title screen warps immediately to the map and event program you are testing, without modifying your source tree:

```sh
dc2tk build extracted/scripts dc2.iso -o test.iso --start-at s51:story_50
```

You can also set `"start_at"` in a mod's `mod.json` file (see [Trying a change quickly](mods.md#trying-a-change-quickly)).

`--start-at` accepts three formats:
- `MAP:PROGRAM`: Start on `MAP` and run event program `PROGRAM` (for example, `s51:story_50` or `m05:500`).
- `MAP/SCRIPT:PROGRAM`: Select a specific script on a map that has multiple scripts (for example, `s06/s06b:500`).
- `MAP`: Start on `MAP` at its default `map_start` program (for example, `m05`).

How the warp works depends on the map type:

- **Story scenes and dungeon-style maps**: Maps that the game normally loads with `load_dungeon_map` are entered using `load_dungeon_map`, followed by `load_event_script` for your chosen program.
- **Towns, streets, and interiors**: Any other map listed in the disc's map table is entered the same way the game transitions from a dungeon to a town, by calling `go_to_edit_mode(map_number, program)`. Because the engine loads town scripts automatically, town maps do not take a `/SCRIPT` part.

When warping into a town:
- You start with a New Game party and inventory, with Max in control (even though a standard New Game starts as Monica).
- Max stands at the town's default arrival point (as if arriving from Palm Brinks, or from the train station when warping into Palm Brinks itself).
- The time of day and story progress match the start of a New Game.
- Because the New Game sequence normally suppresses the first background music load for the opening cutscene, the warp calls `allow_bgm_load()` first so the town's background music and ambient sounds play normally (including any replacement music track in your mod, such as `music:BG_000_000` for Palm Brinks).
