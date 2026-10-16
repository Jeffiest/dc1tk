# Commands

Every `dc2tk` command, the options they share, and what decompiling, the asset tree, compiling
and building do. How to make a mod is in [mods](mods.md), editing scripts in
[scripts](scripts.md), and where to start in [modding](README.md).

Everything runs through one program, `dc2tk`, built with `cargo build --release`
in the repository folder (see the [README](../../README.md#1-what-you-need)).
`dc2tk help COMMAND` lists a command's options. Everything taken from the disc
goes under `extracted/`: the disc's files in `extracted/disc/`, the
decompiled scripts in `extracted/scripts/` and the asset tree in
`extracted/assets/`. `extracted/disc/` holds every file of the disc (DATA.DAT's
files, unpacked, beside the movies, SOUND.DAT and the rest) and the disc's
layout (`disc.json`, `disc-structure.bin`), so `dc2tk pack` writes the image back
from it byte for byte.

```
dc2tk setup dc2.iso                                 extract everything: disc, scripts, assets, maps, data
dc2tk extract dc2.iso extracted/disc                write every file on the disc, and its layout
dc2tk pack -o copy.iso --check dc2.iso              write the image back from extracted/disc
dc2tk decompile <script.stb | extracted dir> [-o out]
dc2tk compile <script.c> [-o script.stb]
dc2tk build extracted/scripts dc2.iso -o modded.iso   new disc image with your changes
dc2tk build extracted/scripts dc2.iso -o test.iso --start-at s51:story_50
dc2tk build extracted/scripts dc2.iso --dry-run --mod castle   what a build would change, no image
dc2tk changes extracted/scripts                     what you edited since decompiling
dc2tk diff dc2.iso                                  everything changed in extracted/ since extracting
dc2tk mod make dc2.iso mymod                        a mod of those changes, ready to build
dc2tk mod override extracted/scripts castle --map s51 start_castle_battle   start a patch
dc2tk mod check extracted/scripts castle            compile what a mod changes, without a disc
dc2tk build extracted/scripts dc2.iso -o modded.iso --mod castle --mod other   build with mods
dc2tk mod list extracted/scripts                    every mod, its patches, numbers and files
dc2tk mod new extracted/scripts castle map/s/s51_raybrandt_castle/s51_1.c   copy whole files into a mod
dc2tk mod test extracted/scripts calculator         run a mod's tests.c without the game
dc2tk mod header extracted/scripts castle           editor headers for a mod's patches
dc2tk mod slim extracted/scripts castle             turn a mod's whole files into patches
dc2tk assets dc2.iso --force                        write the asset tree again (setup writes it)
dc2tk mod asset dc2.iso castle texture:e254a1       copy an asset into a mod to replace it
dc2tk mod migrate dc2.iso castle                    turn a mod's disc/ folder into asset ids
dc2tk play modded.iso                               boot an image in PCSX2 (or build ... --play)
dc2tk maps extracted/scripts                        every map: town or dungeon-style, playable or not
dc2tk map extracted/scripts s17                     one map: programs, free numbers, trigger points, positions
dc2tk map extracted/scripts s17 --json              the same for tools: programs, player spots, names scripts use
dc2tk map-export s17                                a map as a glTF scene for Blender (extracted/maps/s17)
dc2tk xref extracted/scripts event 510 --map s51    what defines, starts and requests an event
dc2tk xref extracted/scripts map s51                how the game enters a map
dc2tk xref extracted/scripts monster golem          a monster's model, AI script and users
dc2tk flags extracted/scripts [--flag N]            save flags in use, and free ones
dc2tk flags extracted/scripts --counters [--counter N]   the same for save counters
dc2tk fork extracted/scripts lib/x.c lib/y.c script.c   a library copy for some scripts
dc2tk mod save extracted/scripts castle             copy edits made in the tree into a mod
dc2tk mod apply extracted/scripts castle            write a mod into the tree
dc2tk verify extracted/disc                         prove every script round-trips
dc2tk defs check extracted/disc                     how much of the disc the definitions name
dc2tk defs todo extracted/disc                      unnamed functions, most called first
dc2tk disassemble <script.stb>                      text assembly, for debugging
dc2tk assemble <script.asm> -o <script.stb>
dc2tk defs learn <source> <definition set>
dc2tk defs rename <definition set> OLD NEW --in extracted/scripts   rename a flag or other constant
dc2tk defs migrate extracted/scripts mods/my-mod                    give an older tree or mod the set's renames
dc2tk defs disc extracted/disc <definition set>     record which disc a set is for
dc2tk natives <ChronicleTwo checkout>
dc2tk api-docs                                      write docs/coding/api from the SDK's headers
```

The `mod` commands are described in [mods](mods.md#the-mod-commands), and `defs learn`,
`defs rename` and `defs migrate` in [definitions](../../definitions/README.md).

A command that reads the disc (`setup`, `build`, `assets`, `map-export`, `diff`,
`mod make`, `mod asset`, `mod migrate`, `mod check --disc`, and the `--disc` of
`xref`, `maps` and `map`) takes the disc image or `extracted/disc` alike. Given
the folder, `build` first packs it into the new image (as `dc2tk pack` does) and
`setup` decompiles it where it is. `extract` needs the image, as do `play` and
`pack --check`; `verify`, a whole-disc `decompile`, `defs check`, `defs todo`
and `defs disc` need the extracted folder.

The tests run with `cargo test --release` in the repository folder. The tests that read the
game's own files are marked ignored; with `DC2TK_CORPUS` set to the absolute path of an
extracted disc (`extracted/disc`), `cargo test --release -- --include-ignored` runs them too
(without it they fail, naming the variable).

## Options shared by most commands

- `--defs DIR` applies a definition set. Repeat it to layer sets, later ones
  winning. Without it the shipped `definitions/pal` set is used; `--no-defs`
  (once `--defs none`) uses no definitions at all. `assets`, `mod asset` and
  `mod migrate` take `--defs` only for the set whose header names the sound
  entries.
- `--sdk DIR` points at the SDK headers and shared libraries (default
  `sdk/`).
- `--natives DIR` points at the native tables (default `src/natives/`).
- `--table NAME` (`disassemble`, `decompile`, `compile`) forces a native
  table (`event`, `monster`, `action`, `effect`, `arg`) instead of choosing
  it from the script's path.
- `--mods DIR` (`build` and the `mod` commands) is the folder that holds the
  mods (default `mods`).
- `--time` prints how long the command took.

The default `sdk/`, `definitions/pal` and `src/natives/` are the repository's:
the nearest folder above the executable that has `definitions/`, or the folder
`$DC2TK_ROOT` names.

Options of single commands:

- `setup DISC [OUT]` writes the tree into `OUT` (default `extracted/scripts`),
  extracts an image into `--extracted DIR` (default `extracted/disc`; an
  extraction already there is reused) and writes the asset tree into
  `--assets DIR` (default `extracted/assets`, replaced when there;
  `--no-assets` leaves it out), every map as `map-export --all` writes them
  into `--maps DIR` (default `extracted/maps`, each map's folder replaced;
  `--no-maps`), and every data table as the JSON `mod asset data:...` copies
  into a mod into `--data DIR` (default `extracted/data`, `TABLE.json`
  each; `--no-data`). Edits belong in mods, not in these folders. `--force`
  overwrites files edited since the last decompile.
- `pack [EXTRACTED] -o IMAGE` packs `extracted/disc` when no folder is given;
  `--check IMAGE` compares the image written with another byte for byte, and
  fails naming the first byte (and sector) that differs.
- `verify EXTRACTED` prints the first 20 failures (`--limit N` for more) and
  `--json FILE` writes the whole report, a result per script; it fails when
  any script does not round-trip.
- `play IMAGE` skips the PS2 boot screens (PCSX2's `-fastboot`), as
  `build --play` does; `--full-boot` shows them. PCSX2 is the one `--pcsx2`
  names, else `$PCSX2`, else `pcsx2-qt` or `pcsx2` on the PATH, else the
  usual install folder.
- `map-export NAME` writes into `extracted/maps/NAME` (`-o DIR` elsewhere);
  `--all` exports every map that has placements, each into
  `extracted/maps/NAME` (or `DIR/NAME`). `--list` writes nothing and lists
  every map there is to export, a line each: its name, a tab, and its title
  in the definitions' language (empty for a map the game names nowhere), as
  the Blender add-on's map list reads it. `--disc` is the disc read (default
  `extracted/disc`, or the image).
- `diff` and `mod make` read the extracted folder from `--extracted DIR`
  (default `extracted`, the folder holding `disc/`, `scripts/` and
  `assets/`); `xref`, `maps` and `map` read map names, monster models and
  `.map` files from `--disc` (default `extracted/disc`, or the image; the old
  name `--extracted` still works). Without a disc there they say so and go on
  without the names.
- `flags SOURCE` prints a summary; `--used` lists every used flag with its
  name and users, `--flag N` every use of one. `--counters` (and
  `--counter N`) does the same for the save counters.
- `defs todo` lists 50 functions (`--limit N` for more).
- `mod apply --force` replaces files edited in the tree too.
- `natives CHECKOUT --out DIR` writes the tables elsewhere than
  `src/natives/`.

The text files `dc2tk` writes (decompiled C, headers, `mod.json` and the
other JSON files, reports) are UTF-8 with LF line endings on every system,
and its JSON is indented by two spaces. A float is written as the shortest
text that reads back as the same value (`0.1`, `1.0`, `1e20`), in C and
assembly as in JSON. It reads CRLF as well, so files an
editor saved with Windows line endings, or trees and mods written by older
versions (CRLF on Windows, one-space JSON), read the same. Game files keep
their bytes.

## Decompiling

Given a single `.stb`, `decompile` prints one self-contained file: every
function the script contains, including code other scripts share.

Given the extracted disc and `-o DIR` (a folder needs one), it writes a
source tree mirroring the disc, one `.c` per script, and moves the code scripts share into library files
under `lib/`, pulled in with `#include "lib/..."`. A typical script then
contains only its own code. `setup` does this after extracting. How the
tree's folders and files are named is in [scripts](scripts.md#the-decompiled-tree).

The tree also gets `compile_flags.txt`, which lets clangd-based editors
read every file as C with the SDK (see [editor setup](../coding/editor.md)),
and `original-hashes.json`, a record of what was written. Writing into a tree
again refuses to overwrite files edited since; `--force` overwrites them. A
whole-disc decompile also removes the files an earlier one wrote that it no
longer writes, keeping (and naming) any of them that were edited.

Given a folder inside the extracted disc (`extracted/disc/map/s/s05`), it
decompiles only the scripts in that folder, at the same tree paths a
whole-disc decompile gives them (`out/map/s/s05_fish_monster_swamp/s05.c`), so the definitions, native table rules and disc
text apply as they do for the whole disc. Code is shared into libraries
only among the scripts decompiled together.

## The asset tree

`assets` does for textures, character models, sounds, music and the menus'
text what `decompile` does for scripts: it writes each distinct one once, in
a format common tools open (PNG, glTF, WAV, MIDI with a SoundFont of its
instruments, UTF-8 text), with `manifest.json` giving each its id
(`texture:e254a1`, `model:c02fuku*`, `sound:SE_CURSOR_MOVE`,
`music:BG_000_000`, `text:en/shopname`) and every place the disc has
it. `setup` writes it into `extracted/assets/` (`--no-assets`
leaves it out); to write it again:

```
dc2tk assets dc2.iso --force      (or dc2tk assets extracted/disc; -o DIR writes it elsewhere)
```

The ids are what a mod names the assets it replaces by (see
[mods](mods.md#game-assets-by-id)). The layout and the rules are in
[the design](../../src/docs/design.md#1-the-asset-tree), "The asset tree".

## Compiling

`compile` reads a `.c`, follows its `#include`s (library files are looked
up from the script's folder and each folder above it; `<...>` headers from
the SDK, from each definition set's `include/`, and from an `include/`
folder in the script's folder or any folder above it, where the decompiler
puts the generated `disc.h`), and writes the `.stb` (by default the source's
path with `.stb`).
The output is byte-identical to the original when the source is unchanged.
Source nested deeper than the parser's limit (`parse::NESTING`, 200 levels:
about 64 parentheses, 97 nested blocks or statements, or a chain of 194
operators) is refused as "nested too deeply" rather than overflowing the
stack; every retail script is within it.

## Modding workflow

1. `dc2tk setup dc2.iso` extracts the disc, decompiles every script into
   `extracted/scripts/`, and writes the assets, the maps and the data tables
   beside it (`extracted/assets/`, `maps/`, `data/`).
2. Edit scripts in `extracted/scripts/`, and shared code in
   `extracted/scripts/lib/` (an edit to a library changes every script that
   includes it; `fork` gives some scripts their own copy). [How scripts
   run](../game/running.md) covers how the game runs them, and [finding your way
   around](README.md#finding-your-way-around) the commands that find what to edit.
3. `dc2tk build extracted/scripts dc2.iso -o modded.iso` writes a new disc
   image.
4. Boot `modded.iso` in PCSX2.

`build` compiles the scripts that can differ from the disc and compares each
with the script on the disc, so you never list what you changed. An
unchanged tree gives an image identical to the original. For each changed
script it:

- puts the new script back into its pack, when it lives inside one (a
  character's `.chr` or a map's `.mpk`), leaving the pack's other members
  as they were;
- writes the file back over the original when it still fits in the
  sectors it had, and otherwise into the space of `DMMY`, a 6.5 MB filler
  file at the end of the disc that the game never reads; only when that is
  full does the image grow, and then the ISO 9660 volume size is updated;
- updates the file's size and sector in `DATA.HD4`, the index the game
  reads `DATA.DAT` through. The game reads each file at `DATA.DAT`'s first
  sector plus the index's sector (`dataread.cpp`), so a file placed after
  `DATA.DAT`'s own extent loads the same way.

It then reads every changed script back out of the new image and checks it
is the script the source compiles to. Files outside `DATA.DAT`, the ISO
9660 directories and the UDF bridge are not changed, since no file moves
in them. `-o` may not name the image the build reads (copying a file onto
itself would empty it); `build` refuses it.

`build` also notes each edited library that reaches scripts you did not edit.
It compiles only the scripts that can differ from the disc: those edited,
replaced or added to by a mod, or including a changed library. Every other
script is as the decompiler wrote it, which compiles back to the disc's own
bytes, so most builds take seconds; `--full` compiles everything.
It compiles everything, and says why, when the tree has no record of what
was decompiled, when the SDK or definition headers changed since (after
updating these tools, say), or when a header in the tree's `include/` was
edited.
`--play` boots the new image in PCSX2 (`play IMAGE` does the same for any
image; `--pcsx2 PATH` or `$PCSX2` when it is not installed in the usual
place).

`--dry-run` (in place of `-o`) does everything but write the image: it says
what each step would change (the scripts compiled, the files replaced and
added, what a map: line makes of its scene: templates split, placements
added and deleted, models rebuilt and added, pictures repainted and added)
and runs the memory check, in seconds and without the 4.7 GB an image takes.
It exits 1 where a certain budget is exceeded, as `build` does. The Blender
add-on's Preview Changes runs it.

To build one script without a disc,
`dc2tk compile extracted/scripts/<path>.c -o <path>.stb`.

`changes` lists what you edited, added or removed in the tree itself.

### Testing without replaying

`--start-at MAP:PROGRAM` (or `MAP/SCRIPT:PROGRAM`, or `MAP` alone for its
`map_start`) builds a test image whose New Game goes straight to that map and
program, as the game's own warps do; the source tree is not changed. A map the
game loads with `load_dungeon_map` (story scenes, dungeon-like maps) is loaded
that way, with `load_event_script` at the program. Any other map in the disc's
map table, a town, a street or an interior, is entered as the game leaves a
dungeon for a town, with `go_to_edit_mode(map number, program)`: `--start-at
m05` opens m05 (map 14) at `map_start`, though m05 has no scripts of its own, and `--start-at m05:PROGRAM` runs one of a town's programs. `build`
prints which way it warps; a town takes no `/SCRIPT`, as the game loads its own.
The scene starts with a new game's party. A town start puts Max in control
(a new game starts as Monica), standing at the town's default arrival spot (the
warp says the player came from Palm Brinks, or from its station when the
target is Palm Brinks), at a new game's time of day and story progress, with
the town's default music and ambient sounds: New Game skips the next music
load for its opening scene, so the warp calls `allow_bgm_load()` first, or the
town would load its sounds but no music. A mod's replacement of the town's tune
(`music:BG_000_000` for Palm Brinks' streets) is what plays. `"start_at"` in `mod.json` takes the
same forms. See [trying a change quickly](mods.md#trying-a-change-quickly).
