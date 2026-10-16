# CLI Reference

Run `dc2tk help <command>` to see all options for any command.

> **Disc input:** Any command that takes `dc2.iso` also accepts `extracted/disc` (created by `dc2tk setup`).

## Setup and building

| Command | Action |
| --- | --- |
| `dc2tk setup dc2.iso` | Extract the disc, decompile all scripts, and export assets, maps, and data tables into `extracted/`. |
| `dc2tk build extracted/scripts dc2.iso -o modded.iso` | Build a playable ISO from `extracted/scripts` and any `--mod` flags. |
| `dc2tk build extracted/scripts dc2.iso -o test.iso --mod mymod --play` | Build with a mod and launch the ISO in PCSX2. |
| `dc2tk build extracted/scripts dc2.iso -o test.iso --start-at s51:story_50` | Build an ISO where **New Game** jumps directly to map `s51` at program `story_50`. |
| `dc2tk build extracted/scripts dc2.iso --dry-run --mod mymod` | Validate that a mod compiles and fits in memory without writing an ISO file. |
| `dc2tk play modded.iso` | Launch an ISO in PCSX2 (skips PS2 boot logos; pass `--full-boot` to keep them). |

## Creating and managing mods

| Command | Action |
| --- | --- |
| `dc2tk diff dc2.iso` | List all scripts, assets, and disc files modified in `extracted/`. |
| `dc2tk changes extracted/scripts` | List only `.c` scripts modified in `extracted/scripts/`. |
| `dc2tk mod make dc2.iso <name>` | Package all modified files in `extracted/` into `mods/<name>/`. |
| `dc2tk mod asset dc2.iso <name> <id>` | Copy an asset or data table into `mods/<name>/` and register it in `mod.json` (for example, `data:items` or `texture:e254a1`). |
| `dc2tk mod override extracted/scripts <name> --map s51 <func>` | Create a patch in `mods/<name>/` that overrides `<func>` (also accepts `--monster`, `--script`, or `--library`). |
| `dc2tk mod check extracted/scripts <name>` | Compile and validate a mod without building an ISO (pass `--disc dc2.iso` to also check assets). |
| `dc2tk mod test extracted/scripts <name>` | Run unit tests in `mods/<name>/tests.c` inside the built-in script VM. |
| `dc2tk mod list extracted/scripts` | List all mods in `mods/`, their patches, and allocated IDs. |
| `dc2tk mod slim extracted/scripts <name>` | Convert full-file script copies in a mod into small function patches. |
| `dc2tk mod header extracted/scripts <name>` | Regenerate `.editor/` headers and `compile_commands.json` for `clangd`. |

## Searching and inspecting the game

| Command | Action |
| --- | --- |
| `dc2tk maps extracted/scripts` | List every map code, English name, and type (town or dungeon). |
| `dc2tk map extracted/scripts <map>` | Show a map's programs, free program numbers, triggers, and spawn coordinates. |
| `dc2tk map-export <map>` | Export a map to `extracted/maps/<map>/` as a `.gltf` scene (`--all` exports all maps). |
| `dc2tk xref extracted/scripts map <map>` | Show every script that warps into `<map>` and which program it starts. |
| `dc2tk xref extracted/scripts monster <name>` | Show a monster's 3D model, AI script, and spawn scripts. |
| `dc2tk xref extracted/scripts event <num> --map <map>` | Show what defines and triggers event program `<num>` on `<map>`. |
| `dc2tk flags extracted/scripts [--flag <n>]` | Show used and free save flags (pass `--counters` for save counters). |

## How key workflows work

### Decompiling

`dc2tk setup` decompiles all scripts on the disc into `extracted/scripts/`. Shared functions are written once to `extracted/scripts/lib/` and included with `#include "lib/..."`. File hashes are saved in `original-hashes.json`; `setup` and `decompile` do not overwrite edited files unless you pass `--force`.

To decompile one `.stb` file manually, run `dc2tk decompile file.stb -o file.c`.

### The asset tree

`dc2tk setup` exports each unique texture (`.png`), character model (`.gltf`), sound effect (`.wav`), music sequence (`.mid` + `.sf2`), and menu text file (`.txt`) into `extracted/assets/`. `extracted/assets/manifest.json` maps every asset ID (`texture:e254a1`, `model:c02fuku*`, `sound:SE_CURSOR_MOVE`, `music:BG_000_000`, `text:en/shopname`) to every disc archive that contains it.

To regenerate `extracted/assets/`, run `dc2tk assets dc2.iso --force`.

### Compiling

`dc2tk compile file.c -o file.stb` compiles one `.c` file to `.stb` bytecode. You do not need to run `compile` manually during normal modding because `dc2tk build` and `dc2tk mod check` compile modified scripts and patches automatically.

### Building

`dc2tk build extracted/scripts dc2.iso -o modded.iso` compiles only modified scripts and `--mod` patches, converts modified assets, checks all changes against PlayStation 2 [Memory Limits](memory.md), and writes `modded.iso`. Unmodified files remain byte-for-byte identical to the retail disc.

### Testing without replaying

Pass `--start-at <target>` to `dc2tk build` (or set `"start_at"` in `mod.json`) to make **New Game** warp directly to a map or event:

| Target | Result on New Game |
| --- | --- |
| `--start-at m05` | Spawn in Palm Brinks (`m05`) |
| `--start-at s51:story_50` | Run program `story_50` (`500`) on map `s51` |
| `--start-at d02f05` | Enter dungeon 2, floor 5 |

## Low-level commands

| Command | Action |
| --- | --- |
| `dc2tk extract dc2.iso extracted/disc` | Unpack raw disc files without decompiling scripts or converting assets. |
| `dc2tk pack extracted/disc -o copy.iso` | Repack raw disc files into an ISO. |
| `dc2tk disassemble file.stb` | Disassemble `.stb` bytecode to text assembly. |
| `dc2tk assemble file.asm -o file.stb` | Assemble text assembly into `.stb` bytecode. |
| `dc2tk fork extracted/scripts lib/old.c lib/new.c script.c` | Split a shared library file for specific scripts. |
| `dc2tk verify extracted/disc` | Verify that all 1,398 retail scripts decompile and recompile byte-for-byte. |
| `dc2tk defs` / `dc2tk natives` / `dc2tk api-docs` | Maintain decompiler naming definitions, native tables, and API docs (see [definitions/README.md](../../definitions/README.md)). |
