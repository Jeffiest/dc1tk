# Usage

Everything runs through `cli.py`, with Python 3 and nothing else installed.

```
python3 cli.py setup dc2.iso source       extract the disc and decompile it all
python3 cli.py extract dc2.iso extracted  write every file on the disc
python3 cli.py decompile <script.stb | extracted dir> [-o out]
python3 cli.py compile <script.c> [-o script.stb]
python3 cli.py build source dc2.iso -o modded.iso   new disc image with your changes
python3 cli.py build source dc2.iso -o test.iso --start-at s51:story_50
python3 cli.py changes source             what you edited since decompiling
python3 cli.py mod new source castle map/s/s51_raybrandt_castle/s51_1.c   start a mod
python3 cli.py build source dc2.iso -o modded.iso --mod castle --mod other  build with mods
python3 cli.py mod list source            every mod, its files, and mods that clash
python3 cli.py mod test source calculator run a mod's tests.c without the game
python3 cli.py play modded.iso            boot an image in PCSX2 (or build ... --play)
python3 cli.py maps source                every map: town or dungeon-style, playable or not
python3 cli.py map source s17             one map: programs, free numbers, trigger points, positions
python3 cli.py xref source event 510 --map s51     what defines, starts and requests an event
python3 cli.py xref source map s51        how the game enters a map
python3 cli.py xref source monster golem  a monster's model, AI script and users
python3 cli.py flags source [--flag N]    save flags in use, and free ones
python3 cli.py fork source lib/x.c lib/y.c script.c   a library copy for some scripts
python3 cli.py verify extracted           prove every script round-trips
python3 cli.py disassemble <script.stb>   text assembly, for debugging
python3 cli.py assemble <script.asm> -o <script.stb>
python3 cli.py defs learn <source> <definition set>
python3 cli.py natives <ChronicleTwo checkout>
python3 cli.py test                       run the tests
```

## Options shared by most commands

- `--defs DIR` applies a definition set. Repeat it to layer sets, later ones
  winning. Without it the shipped `definitions/pal` set is used; `--defs
  none` uses no definitions at all.
- `--sdk DIR` points at the SDK headers (default `sdk/`).
- `--table NAME` forces a native table (`event`, `monster`, `action`,
  `effect`, `arg`) instead of choosing it from the script's path.

## Decompiling

Given a single `.stb`, `decompile` prints one self-contained file: every
function the script contains, including code other scripts share.

Given the extracted disc, it writes a source tree mirroring the disc, one
`.c` per script, and moves the code scripts share into library files
under `lib/`, pulled in with `#include "lib/..."`. A typical script then
contains only its own code. `setup` does this after extracting.

Folders and files keep the game's own code and gain a readable name after
it: `map/s/s51_raybrandt_castle/s51_1.c`, `dungeon/monster/e33a_golem.c`,
`mainchr/c01_max.c`, and libraries follow their scripts' folders
(`lib/map/s/s51_raybrandt_castle/`). Map folders are named from the disc's
map table and monster AI scripts from its monster table, in the definition
set's language; the set's `paths.json` names the rest (dungeons, the main
characters). The game's codes contain underscores too, so the tree records
the disc path of every renamed file in `unnamed-paths.json`; `build`,
`defs learn`, `xref` and `mod` read it, so a renamed file still replaces
the right disc file. Commands that take a map or monster still take the
game's code (`xref source map s51`).

The tree also gets `compile_flags.txt`, which lets clangd-based editors
read every file as C with the SDK (see [editor setup](modding.md#editor-setup)),
and `original-hashes.json`, a record of what was written. Writing into a tree
again refuses to overwrite files edited since; `--force` overwrites them.

Given a folder inside the extracted disc (`extracted/map/s/s05`), it
decompiles only the scripts in that folder, still under their disc paths
(`out/map/s/s05_fish_monster_swamp/s05.c`), so the definitions, native table rules and disc
text apply as they do for the whole disc. Code is shared into libraries
only among the scripts decompiled together.

## Compiling

`compile` reads a `.c`, follows its `#include`s (library files are looked
up from the script's folder and each folder above it; `<...>` headers from
the SDK, from each definition set's `include/`, and from an `include/`
folder in the script's folder or any folder above it, where the decompiler
puts the generated `disc.h`), and writes the `.stb`.
The output is byte-identical to the original when the source is unchanged.

## Modding workflow

1. `python3 cli.py setup dc2.iso source` extracts the disc and decompiles
   every script into `source/`.
2. Edit scripts in `source/`, and shared code in `source/lib/` (an edit to a
   library changes every script that includes it; `fork` gives some scripts
   their own copy). The [modding guide](modding.md) covers how scripts run
   and the commands that find what to edit.
3. `python3 cli.py build source dc2.iso -o modded.iso` writes a new disc
   image.
4. Boot `modded.iso` in PCSX2.

`build` compiles every script in the tree (about two minutes for the whole
disc) and compares each with the script on the disc, so you never list what
you changed. An unchanged tree gives an image identical to the original. For
each changed script it:

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
in them.

`build` also notes each edited library that reaches scripts you did not edit.
It compiles only the scripts that can differ from the disc: those edited,
replaced or added to by a mod, or including a changed library. Every other
script is as the decompiler wrote it, which compiles back to the disc's own
bytes, so most builds take seconds; `--full` compiles everything.
`--play` boots the new image in PCSX2 (`play IMAGE` does the same for any
image; `--pcsx2 PATH` or `$PCSX2` when it is not installed in the usual
place).

To build one script without a disc, `python3 cli.py compile
source/<path>.c -o <path>.stb`.

### Testing without replaying

`--start-at MAP:PROGRAM` (or `MAP/SCRIPT:PROGRAM`) builds a test image whose
New Game goes straight to that map and program, as the game's own story warps
do; the source tree is not changed. It suits maps the game loads with
`load_dungeon_map`, and the scene starts with a new game's party. See
[the modding guide](modding.md#trying-a-change-quickly).

### Mods

A mod is a folder in `mods/` holding only the files it changes or adds, laid
out like the tree. Keep the decompiled tree as the decompiler wrote it and
build any set of mods over it:

```
python3 cli.py mod new source castle map/s/s51_raybrandt_castle/s51_1.c dungeon/monster/e33a_golem.c
python3 cli.py build source dc2.iso -o modded.iso --mod castle --mod other
```

- `mod new SOURCE NAME FILE...` copies tree files into `mods/NAME/files/`
  to edit there; files the mod already has are kept. The mod gets its own
  `compile_flags.txt`, so an editor finds its libraries first and then the
  tree's.
- `mod save SOURCE NAME` copies every file edited or added in the tree into
  the mod instead, for edits made in the tree.
- `build --mod NAME` (repeat it for several) compiles each mod file in place
  of the tree's, libraries included, without changing the tree. Two chosen
  mods may not change the same file; `build` names the file and the mods.
- `mod list SOURCE` lists every mod and its files, the files whose
  decompiler version changed since the mod was made ("stale": the mod
  replaces the whole file, so compare them), and the mods that cannot be
  built together.
- `mod apply SOURCE NAME...` writes mods into the tree instead.
- `mod test SOURCE NAME` runs the mod's tests (below).

A mod made on a tree with readable names applies to one without, and the
other way round: files and `#include` lines follow each tree's layout.

**Additions.** A mod need not copy a script to add to it. Its `mod.json` can
list additions, applied when it is built:

```json
{
 "additions": [
  {"map": "s17",
   "programs": {"9000": "start_calculator_valley", "treasure_chest": "use_calculator"},
   "includes": ["lib/mods/calculator/calculator.c"]}
 ],
 "start_at": "s17:9000"
}
```

Each addition picks scripts by `"map"` (every script of the map, all
languages) or by `"scripts"` (paths without readable names, with `*` and
`?`), and adds `"programs"`, `"includes"`, or `"wrap"`, which runs functions
`"before"` and `"after"` an existing program:
`{"map_start": {"before": ["my_setup"]}}`. Several mods can add to the same
script; only adding a program the script already has is refused (wrap it
instead). Keep a mod's own libraries under `lib/mods/NAME/`, so their paths
never depend on the tree's names. `"start_at"` is the scene `build` starts a
new game in when the mod is chosen (`--no-start-at` for a normal start).

**Shared libraries.** `sdk/modkit/` holds libraries any script can include:
`#include "modkit/text.c"` (message windows: `modkit_say`, `modkit_caption`,
`modkit_text_show`), `"modkit/input.c"` (`modkit_wait`,
`modkit_wait_for_button`, `modkit_pressed`) and, for monster scripts,
`"modkit/tick.c"` (`modkit_tick`, the timer behind the caption pattern in the
[modding guide](modding.md)). A quoted include is looked for from the
script's folder up, then in `sdk/`. Libraries can include libraries; each is
included once.

**Tests.** `mods/NAME/tests.c` holds `int test_...(void)` functions;
`cli.py mod test SOURCE NAME` compiles them with the mod over the tree and
runs them in an interpreter of the script VM, without the game. A test
passes when it returns nonzero, and `print(...)` output is shown for the
ones that fail. Natives do nothing there (a result they would give is 0),
so test the logic: write it as functions that take what the game would give
(the buttons pressed, say) and check what they return or change.
`--mods DIR` picks another mods folder. Mods hold game scripts, so they stay
out of this repository (`mods/` is ignored).

`changes` lists what you edited, added or removed in the tree itself.

### A first test

Two edits that show within minutes of starting a new game:

- **The opening.** In `lib/map/m/m07_palm_brinks/base_opening_scenes_v1.c`, function
  `play_opening_letter_and_storm_scene`, reverse the three
  `show_narration_line` calls (messages 10010, 10020, 10030) and change
  `set_sprite_color(0, 0, 0, 0, 128)` to `set_sprite_color(0, 128, 0, 0,
  128)`. The letter is narrated last line first, over red instead of black.
- **Monica's first fight.** In `map/s/s51_raybrandt_castle/s51_1.c` (and `_2` to `_5` for
  the other languages), function `start_castle_battle`, replace the four
  `MONSTER_GRIFFON_SOLDIER_523` with other kinds from
  `source/include/disc.h`, loading each first:
  `load_monster_data(MONSTER_SEWER_RAT, 1); load_monster_data(MONSTER_FROGGY, 0); ...`
  and `place_monster(MONSTER_SEWER_RAT, ...)` and so on. That soldier's own
  script ends the fight when the fourth one dies, which other monsters do
  not do, so also add
  `PROGRAM(monsters_cleared, start_georama_with_starter_items);` beside
  `PROGRAM(510, ...)`: the engine runs program 1500 once no monster is
  left.

## Improving the definitions

Names, docs and global names you write in decompiled source can be fed back
into a definition set, so everyone's next decompile gets them:

```
python3 cli.py defs learn source definitions/pal
```

`learn` compiles each script, matches functions by their bytecode
fingerprint, and records any name, parameter or local name, doc comment, or
global name that differs from the decompiler's defaults. See
[definitions](definitions.md).

## Checking a C toolchain can read the output

The tree's `compile_flags.txt` holds the flags, so clangd and `clang-tidy`
find them alone. By hand:

```
clang -m32 -fsyntax-only -x c -std=c2x -I sdk -I definitions/pal/include -I source -I source/include -Wno-unknown-pragmas source/<path>.c
```

`sdk/c2s.h` gives C tools a meaning for c2s's few extensions. What clang
still reports are argument mismatches that exist in the retail scripts.
