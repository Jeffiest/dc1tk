# CLI Reference

All tasks run through the `dc2tk` command-line tool. Run `dc2tk help <command>` at any time to see a command's options.

> **Disc input:** Any command that takes `dc2.iso` also accepts `extracted/disc` (the unpacked disc folder created by `setup`).

## Setup and building

| Command | Description |
| --- | --- |
| `dc2tk setup dc2.iso` | Extract the disc, decompile all scripts, and export assets, maps, and data tables into `extracted/`. |
| `dc2tk build extracted/scripts dc2.iso -o modded.iso` | Build a new playable ISO from `extracted/scripts` (and any `--mod` flags). |
| `dc2tk build extracted/scripts dc2.iso -o test.iso --mod mymod --play` | Build with a mod and immediately launch the ISO in PCSX2. |
| `dc2tk build extracted/scripts dc2.iso -o test.iso --start-at s51:story_50` | Build an ISO where choosing **New Game** jumps straight to map `s51` at program `story_50`. |
| `dc2tk build extracted/scripts dc2.iso --dry-run --mod mymod` | Check that a mod builds and fits in memory without writing a 4.7 GB ISO file. |
| `dc2tk play modded.iso` | Boot an ISO in PCSX2 (skips PS2 boot logos; pass `--full-boot` to show them). |

## Creating and managing mods

| Command | Description |
| --- | --- |
| `dc2tk diff dc2.iso` | List all scripts, assets, and disc files changed in `extracted/`. |
| `dc2tk changes extracted/scripts` | List only `.c` scripts changed in `extracted/scripts/`. |
| `dc2tk mod make dc2.iso <name>` | Package everything you edited in `extracted/` into `mods/<name>/`. |
| `dc2tk mod asset dc2.iso <name> <id>` | Copy an asset or data table into `mods/<name>/` and register it in `mod.json` (for example, `data:items` or `texture:e254a1`). |
| `dc2tk mod override extracted/scripts <name> --map s51 <func>` | Create a patch in `mods/<name>/` that overrides `<func>` (also accepts `--monster`, `--script`, or `--library`). |
| `dc2tk mod check extracted/scripts <name>` | Compile and validate a mod in seconds without building an ISO (pass `--disc dc2.iso` to also check asset replacements). |
| `dc2tk mod test extracted/scripts <name>` | Run unit tests in `mods/<name>/tests.c` inside the built-in script VM. |
| `dc2tk mod list extracted/scripts` | List all mods in `mods/`, their patches, and allocated IDs. |
| `dc2tk mod slim extracted/scripts <name>` | Convert full-file script copies in a mod into small function patches. |
| `dc2tk mod header extracted/scripts <name>` | Regenerate `.editor/` headers and `compile_commands.json` for `clangd`. |

## Searching and inspecting the game

| Command | Description |
| --- | --- |
| `dc2tk maps extracted/scripts` | List every map code, its English name, and whether it is a town or dungeon. |
| `dc2tk map extracted/scripts <map>` | Show a map's programs, free program numbers, trigger points, and spawn coordinates. |
| `dc2tk map-export <map>` | Export a map to `extracted/maps/<map>/` as a `.gltf` scene for Blender (`--all` exports all maps). |
| `dc2tk xref extracted/scripts map <map>` | Show every script that warps into `<map>` and which program it starts. |
| `dc2tk xref extracted/scripts monster <name>` | Show a monster's 3D model, AI script, and every script that spawns it. |
| `dc2tk xref extracted/scripts event <num> --map <map>` | Show what defines and triggers event program `<num>` on `<map>`. |
| `dc2tk flags extracted/scripts [--flag <n>]` | Show used and free save flags (pass `--counters` for save counters). |

## How key workflows work

### Decompiling

`dc2tk setup` runs `decompile` over the entire disc and writes `extracted/scripts/`. Code shared across multiple scripts is extracted once into `extracted/scripts/lib/` and included via `#include "lib/..."`. `setup` and `decompile` record file hashes in `original-hashes.json` and refuse to overwrite files you have edited unless you pass `--force`.

You can also decompile a single `.stb` file (`dc2tk decompile file.stb -o file.c`).

### The asset tree

`dc2tk setup` runs `assets` to write `extracted/assets/`, exporting each unique texture (`.png`), character model (`.gltf`), sound effect (`.wav`), music sequence (`.mid` + `.sf2`), and menu text file (`.txt`). `extracted/assets/manifest.json` maps every asset ID (`texture:e254a1`, `model:c02fuku*`, `sound:SE_CURSOR_MOVE`, `music:BG_000_000`, `text:en/shopname`) to every file on the disc that contains a copy of it.

To regenerate `extracted/assets/` on its own, run `dc2tk assets dc2.iso --force`.

### Compiling

`dc2tk compile file.c -o file.stb` compiles a single script file back to `.stb` bytecode. In normal modding, you do not need to run `compile` manually—`dc2tk build` and `dc2tk mod check` compile modified scripts and patches automatically.

### Building

`dc2tk build extracted/scripts dc2.iso -o modded.iso` compiles only the scripts you edited or patched with `--mod`, converts modified assets, checks all changes against PlayStation 2 [Memory Limits](memory.md), and writes `modded.iso`. Unedited files remain byte-for-byte identical to the retail disc.

### Testing without replaying

Pass `--start-at <target>` to `dc2tk build` (or set `"start_at"` in `mod.json`) so that selecting **New Game** at the title screen warps directly to the map or cutscene you are testing:

- `--start-at m05` — Spawn in Palm Brinks (`m05`).
- `--start-at s51:story_50` — Start program `story_50` (`500`) on map `s51`.
- `--start-at d02f05` — Enter dungeon 2, floor 5.

## Advanced / low-level commands

Most modders never need these commands, which are used for low-level debugging or working on `dc2tk` itself:

- `dc2tk extract dc2.iso extracted/disc` / `dc2tk pack extracted/disc -o copy.iso` — Unpack or repack raw disc files without decompiling scripts or converting assets.
- `dc2tk disassemble file.stb` / `dc2tk assemble file.asm -o file.stb` — Convert `.stb` bytecode to or from text assembly.
- `dc2tk fork extracted/scripts lib/old.c lib/new.c script.c` — Split a shared library copy off for specific scripts in the tree.
- `dc2tk verify extracted/disc` — Verify that all 1,398 retail scripts decompile and recompile byte-for-byte.
- `dc2tk defs ...` / `dc2tk natives ...` / `dc2tk api-docs` — Maintain decompiler naming definitions, native tables, and SDK API docs (see [definitions/README.md](../../definitions/README.md)).
