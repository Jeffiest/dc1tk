# Usage

Everything runs through one program, `c2`, built with `cargo build --release`
in the repository folder (see the [README](../README.md#1-what-you-need)).
`c2 help COMMAND` lists a command's options. Everything taken from the disc
goes under `extracted/`: the disc's files in `extracted/disc/`, the
decompiled scripts in `extracted/scripts/` and the asset tree in
`extracted/assets/`. `extracted/disc/` holds every file of the disc (DATA.DAT's
files, unpacked, beside the movies, SOUND.DAT and the rest) and the disc's
layout (`disc.json`, `disc-structure.bin`), so `c2 pack` writes the image back
from it byte for byte.

```
c2 setup dc2.iso                                 extract the disc, decompile it all, write the assets
c2 extract dc2.iso extracted/disc                write every file on the disc, and its layout
c2 pack -o copy.iso --check dc2.iso              write the image back from extracted/disc
c2 decompile <script.stb | extracted dir> [-o out]
c2 compile <script.c> [-o script.stb]
c2 build extracted/scripts dc2.iso -o modded.iso   new disc image with your changes
c2 build extracted/scripts dc2.iso -o test.iso --start-at s51:story_50
c2 changes extracted/scripts                     what you edited since decompiling
c2 diff dc2.iso                                  everything changed in extracted/ since extracting
c2 mod make dc2.iso mymod                        a mod of those changes, ready to build
c2 mod override extracted/scripts castle --map s51 start_castle_battle   start a patch
c2 mod check extracted/scripts castle            compile what a mod changes, without a disc
c2 build extracted/scripts dc2.iso -o modded.iso --mod castle --mod other   build with mods
c2 mod list extracted/scripts                    every mod, its patches, numbers and files
c2 mod new extracted/scripts castle map/s/s51_raybrandt_castle/s51_1.c   copy whole files into a mod
c2 mod test extracted/scripts calculator         run a mod's tests.c without the game
c2 assets dc2.iso --force                        write the asset tree again (setup writes it)
c2 mod asset dc2.iso castle texture:e254a1       copy an asset into a mod to replace it
c2 mod migrate dc2.iso castle                    turn a mod's disc/ folder into asset ids
c2 play modded.iso                               boot an image in PCSX2 (or build ... --play)
c2 maps extracted/scripts                        every map: town or dungeon-style, playable or not
c2 map extracted/scripts s17                     one map: programs, free numbers, trigger points, positions
c2 map-export s17                                a map as a glTF scene for Blender (extracted/maps/s17)
c2 xref extracted/scripts event 510 --map s51    what defines, starts and requests an event
c2 xref extracted/scripts map s51                how the game enters a map
c2 xref extracted/scripts monster golem          a monster's model, AI script and users
c2 flags extracted/scripts [--flag N]            save flags in use, and free ones
c2 fork extracted/scripts lib/x.c lib/y.c script.c   a library copy for some scripts
c2 mod save extracted/scripts castle             copy edits made in the tree into a mod
c2 mod apply extracted/scripts castle            write a mod into the tree
c2 verify extracted/disc                         prove every script round-trips
c2 defs check extracted/disc                     how much of the disc the definitions name
c2 defs todo extracted/disc                      unnamed functions, most called first
c2 disassemble <script.stb>                      text assembly, for debugging
c2 assemble <script.asm> -o <script.stb>
c2 defs learn <source> <definition set>
c2 defs disc extracted/disc <definition set>     record which disc a set is for
c2 natives <ChronicleTwo checkout>
```

The tests run with `cargo test --release` in the repository folder.

## Options shared by most commands

- `--defs DIR` applies a definition set. Repeat it to layer sets, later ones
  winning. Without it the shipped `definitions/pal` set is used; `--defs
  none` uses no definitions at all.
- `--sdk DIR` points at the SDK headers and shared libraries (default
  `sdk/`).
- `--natives DIR` points at the native tables (default `src/natives/`).
- `--table NAME` forces a native table (`event`, `monster`, `action`,
  `effect`, `arg`) instead of choosing it from the script's path.
- `--time` prints how long the command took.

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

Given a folder inside the extracted disc (`extracted/disc/map/s/s05`), it
decompiles only the scripts in that folder, at the same tree paths a
whole-disc decompile gives them (`out/map/s/s05_fish_monster_swamp/s05.c`), so the definitions, native table rules and disc
text apply as they do for the whole disc. Code is shared into libraries
only among the scripts decompiled together.

## The asset tree

`assets` does for textures, character models, sounds and the menus' text
what `decompile` does for scripts: it writes each distinct one once, in a
format common tools open (PNG, glTF, WAV, UTF-8 text), with
`manifest.json` giving each its id (`texture:e254a1`, `model:c02fuku*`,
`sound:SE_CURSOR_MOVE`, `text:en/shopname`) and every place the disc has
it. `setup` writes it into `extracted/assets/` (`--no-assets`
leaves it out); to write it again:

```
c2 assets dc2.iso --force      (or c2 assets extracted/disc; -o DIR writes it elsewhere)
```

The ids are what a mod names the assets it replaces by (see
[mods](#mods)). The layout and the rules are in [the design](design.md),
"The asset tree".

## Compiling

`compile` reads a `.c`, follows its `#include`s (library files are looked
up from the script's folder and each folder above it; `<...>` headers from
the SDK, from each definition set's `include/`, and from an `include/`
folder in the script's folder or any folder above it, where the decompiler
puts the generated `disc.h`), and writes the `.stb`.
The output is byte-identical to the original when the source is unchanged.

## Modding workflow

1. `c2 setup dc2.iso` extracts the disc and decompiles every script into
   `extracted/scripts/`.
2. Edit scripts in `extracted/scripts/`, and shared code in
   `extracted/scripts/lib/` (an edit to a library changes every script that
   includes it; `fork` gives some scripts their own copy). The [modding guide](modding.md) covers how scripts run
   and the commands that find what to edit.
3. `c2 build extracted/scripts dc2.iso -o modded.iso` writes a new disc
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
in them.

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

To build one script without a disc,
`c2 compile extracted/scripts/<path>.c -o <path>.stb`.

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
The scene starts with a new game's party. `"start_at"` in `mod.json` takes the
same forms. See [the modding guide](modding.md#trying-a-change-quickly).

### Mods

A mod is a folder in `mods/`. Keep the decompiled tree as the decompiler
wrote it and build any set of mods over it:

```
c2 mod override extracted/scripts castle --map s51 start_castle_battle
c2 build extracted/scripts dc2.iso -o modded.iso --mod castle --mod other
```

A mod holds **patches**: C files that name the scripts they change and hold
only the functions they replace or add. Several mods can patch the same
script, and a patch keeps working when the decompiler's output around it
changes.

```c
// mods/castle/s51.c
#pragma target map s51          // s51_1.c to s51_5.c: every language

#include <event.h>
#include <disc.h>

NEW_FLAG(FIGHT_ON);             // a save flag build picks, free of the game's and other mods'

#pragma override                // replaces the script's start_castle_battle
void start_castle_battle(void)
{
    original();                 // runs the script's own
    set_flag(FIGHT_ON, 1);
}
```

- **`#pragma target`** picks the scripts: `map s51` (the map's language
  variants), `script dungeon/monster/e33a.c` (paths without readable names,
  with `*` and `?`), `monster golem` (a monster kind's AI script, as
  `MONSTER_GOLEM` in `disc.h` names it) or `library lib/...` (every script
  that includes the library). A patch can have several. A `.c` file in the
  mod without one is a library its patches include.
- **`#pragma override`** before a function replaces the function of that
  name in each target, for those scripts only, even when the script gets it
  from a library: no fork needed. `original(...)` inside it calls the
  function it replaced. Two mods can override the same function when the
  later one calls `original()`; when neither does, `build` names both.
  The function's fingerprint (its code's, as the definitions name functions
  by) is kept in the mod's `fingerprints.json`, written by `mod override`,
  `mod check` and `build`. When a later decompile renames the function,
  `build` and `mod check` find it by fingerprint, say the new name and
  build the override as if the patch said it; `mod check --rename` writes
  the new name into the mod's files. If the code is in several functions
  of the script, they name them and you choose.
- **`#pragma override NATIVE`** before a function wraps a native: every
  call the targets make to it (by its SDK name, in the scripts' own code,
  their libraries and the patches) calls the function instead, and
  `original(...)` inside it calls the native. So a mod changes what a
  script asks the engine for without copying the functions that ask:

  ```c
  #pragma target map s51
  #include <event.h>
  #include <disc.h>

  #pragma override place_monster
  void place_rats(monster_kind monster, float x, float y, float z, float rot_x, float rot_y, float rot_z,
                  int talk_param)
  {
      if (monster == MONSTER_GRIFFON_SOLDIER_523) {
          monster = MONSTER_SEWER_RAT;
      }
      original(monster, x, y, z, rot_x, rot_y, rot_z, talk_param);
  }
  ```

  The function takes the native's parameters, `OPTIONAL` ones included,
  and returns what the native returns (or is `void`). A call that leaves
  optional arguments out reaches it with those parameters 0, and its
  `original()` passes on only the arguments the call gave, so the native
  sees the call as the script made it (`place_monster(-1)` places the
  floor's monsters at random still). Give it a name of its own, as above:
  C tools would read a function named like the native as a second
  declaration of it (a function named `place_monster` works too, but
  clangd marks it). Only the calls change: the script's own code compiles
  to the same bytes around them, and the wrapper goes at its end (once for
  each number of arguments the calls give). The function can be called by
  its own name as well. Wrappers from several mods chain as overrides do:
  the later mod's runs first, and its `original()` runs the earlier one's.
  A wrapper's own calls to its native reach the native.
- **`#pragma every_frame`** before a function without parameters calls it
  before every `wait;` in the target, so it runs once a frame whatever the
  script is waiting in: a monster keeping a timer, say. It must not wait.
- **Other functions and globals** are added. A name the target already has
  is refused, since the patch most likely meant to override it; a name
  another mod adds too is renamed for one of them, so mods never need to
  agree on names. A mod's libraries can declare globals of their own.
- **`PROGRAM` lines** add programs. To change a program the script has,
  override its function.
- **`NEW_FLAG(name)`, `NEW_PROGRAM(name)` and `NEW_MONSTER_VAR(name)`**, in
  any file of the mod (a shared `.h` is handy), ask `build` for a save flag
  (from 1024), a program number (from 9000) or a shared monster variable (8
  to 135) that the game, the engine and every mod in `mods/` leave alone,
  hand-written numbers included. The numbers are kept in
  `mods/numbers.json`, so a name keeps its number from build to build and
  saves stay valid when mods are added; a mod left out of a build keeps its
  numbers reserved. Use the name where the number would go:
  `PROGRAM(SHOW_CAPTION, show_caption);` in the map and
  `run_event_script(SHOW_CAPTION);` in the monster. `"start_at"` in
  `mod.json` can name a mod's program too (`"s51:SHOW_CAPTION"`).
- **`LANGUAGE`** is the language of the script a patch is merged into
  (`LANGUAGE_ENGLISH` for `s51_1.c` to `LANGUAGE_SPANISH` for `s51_5.c`;
  -1 for scripts that are not language variants), for `#if` and in code, so
  one patch can carry every language's text.
- **Mods that build on mods.** `"requires": ["base"]` in `mod.json` names
  mods this one needs: choosing it chooses them too (`build` says so), and
  it is applied after them. Its files see their numbers, so it can include
  their headers and libraries by the mod's folder
  (`#include "base/names.h"`, then use `BASE_SEEN`), and its overrides wrap
  theirs (the later mod's runs first; `original()` runs the earlier one).
  A required mod missing from the mods folder is an error. `"after":
  ["other"]` only orders: when `other` is chosen too, this mod is applied
  after it. Where neither says, mods are applied in the order given; a
  cycle is an error naming the mods.

The commands:

- `mod override SOURCE NAME --map s51 FUNCTION...` (or `--script`,
  `--monster`, `--library`) starts a patch, or adds to one: it copies the
  functions from the target (the English variant of a map) into
  `mods/NAME/s51.c`, each after `#pragma override`, with the target line and
  the script's headers, and writes the editor files `mod header` writes. A
  native named instead of a function (`place_monster`) gets a wrapper that
  takes the native's parameters and passes them on, with the native's doc
  comment.
- `mod header SOURCE NAME` writes, for each patch, a header of what its
  target declares (`mods/NAME/.editor/s51.h`: the script's functions with
  their signatures and doc comments, those of its libraries, its globals
  and its `PROGRAM` table), and the mod's `compile_commands.json`, which
  gives each patch the tree's flags and its header, so clangd knows the
  target's names in the patch. Only editors read them: the patch is not
  changed, and builds are the same with them or without. Run it again after
  decompiling again.
- `mod check SOURCE NAME...` compiles every script the mods change, as
  `build` would but without a disc, and prints the mods it added for
  `requires`, the order they apply in, the numbers they were given, and
  overridden functions the tree has renamed (`--rename` writes the new
  names into the mods). With `--disc ISO` it also looks up the files the
  mods' scripts load by name (`load_chara`, `load_sound`,
  `load_message_file`, `load_image`, `start_background_load`, ... with a
  string path, in the directory the function's `change_dir` set) and the
  member a character pack is loaded by (`info.cfg`), and warns about any the
  disc and the mods' own files do not have; such a load fails only in the
  game. Loads the tree's own script makes too are left out (the game has a
  few that fail).
- `build --mod NAME` (repeat it for several) merges each patch into its
  targets after their libraries are included, and lists each mod's patches
  and numbers. The tree is not changed.
- `mod list SOURCE` lists every mod: its patches, what they override and
  add, what it requires and is built after, its numbers, and its whole
  files (below).
- `mod test SOURCE NAME` runs the mod's tests (below).

**Whole files.** A mod can also replace or add whole files, under
`mods/NAME/files/`, laid out like the tree. Patches are better for changing
the game's scripts; whole files suit scripts and libraries of the mod's own.

- `mod new SOURCE NAME FILE...` copies tree files into `mods/NAME/files/`
  to edit there; files the mod already has are kept. The mod gets its own
  `compile_flags.txt`, so an editor finds its libraries first and then the
  tree's.
- `mod save SOURCE NAME` copies every file edited or added in the tree into
  the mod instead, for edits made in the tree.
- `build` compiles each mod file in place of the tree's, libraries included.
  Two chosen mods may not replace the same file; `build` names the file and
  the mods. `mod list` marks the files whose decompiler version changed
  since the mod was made ("stale": the mod replaces the whole file, so
  compare them).
- `mod apply SOURCE NAME...` writes mods' files into the tree instead;
  patches are merged by `build` only.
- `mod slim SOURCE NAME` turns the mod's copies of tree files into patches:
  for each, a patch (`mods/NAME/s51_1.c`, `#pragma target script` or
  `library` and the file's path) that overrides each function the copy
  changes and adds the functions, globals, `PROGRAM` lines, `#define` lines
  and libraries it adds; then the copy goes. It compiles every script the
  copy reaches with the copy and then with the patch, and keeps the copy,
  saying why, unless they are the same bytes: a patch adds at the end of a
  script, so a copy that added a function above the script's own, or
  removes or changes what a patch cannot (a function, global or `PROGRAM`
  line taken away, `MAIN`, an enum), stays a whole file. A copy that
  changes nothing is removed. Files of the mod's own, not in the tree,
  stay as they are.

A mod made on a tree with readable names applies to one without, and the
other way round: files and `#include` lines follow each tree's layout.

**Game assets by id.** A mod names the textures, models, sounds and text
it replaces by their asset ids (see [the asset tree](#the-asset-tree)) in
`mod.json`, with the files anywhere in its folder, and `build` replaces
each at every place the disc has it:

```json
"replace": {
  "texture:e254a1": "art/soldier_green.png",
  "model:c02fuku*": "models/monica/c02b.gltf",
  "sound:SE_EV_000_130_GRIFFON_BREAK_WINDOW": {"file": "sounds/boom.wav", "volume": 90},
  "text:en/shopname": "text/en/shopname.txt",
  "file:snd2/sp/sp_000.snd": "raw/sp_000.snd"
},
"add": {
  "sound:SE_MOD_BEEP": {"file": "sounds/beep.wav", "bank": "SP_004"},
  "file:mods/calculator/ui.img": {"file": "ui", "like": "texture:logo_d00"},
  "file:mods/calculator/calc.chr": {"file": "model/calc.gltf", "like": "model:uki"}
}
```

A `file:` line with `like` makes a new picture file from PNGs, or a new
model pack from a glTF, in the format of the disc's file `like` names (see
[the modding guide](modding.md#pictures-and-models-of-a-mods-own)).
`mod asset ISO NAME ID...` copies assets out by id and adds their lines,
`mod check SOURCE NAME... --disc ISO` checks every line resolves, and
`mod migrate ISO NAME` turns a mod's `disc/` folder (below) into such
lines. Two mods replacing one asset are refused. The rules are in
[the design](design.md), "Mods by name".

A sound reaches every copy of its bank: where a menu pack holds a version
of a bank that differs from `snd2/sp/`'s only in parts the entry does not
use (its volumes), the entry is one sound with both places, and a bank's
folder (`bank:SP_001`) reaches them too. An added sound's name
(`sound:SE_MOD_BEEP`) is a constant the mod's patches use, holding the
entry's number.

**Game files.** A mod can replace models, textures, sounds and any other
file on the disc. It keeps them under `mods/NAME/disc/`, laid out as
`extract` lays the disc out: a file at its path in `DATA.DAT`
(`disc/snd2/sp/sp_000.snd`), or a member of a pack under the pack's name
plus `.d` (`disc/chara/c01_base.chr.d/c01b.img` is the texture in Max's
character pack; packs inside packs work the same way).

- `mod asset ISO NAME PATH...` copies files or pack members off the disc
  image into the mod, to replace there.
- `build` writes each over the disc's own, puts members back into their
  packs, and reads them back from the new image to check. Two chosen mods
  may not replace the same game file.
- Maps, models, textures, scripts and sound banks load into fixed memory,
  so `build` checks each load a changed file is part of against
  [the memory budgets](assets/memory.md) and says what it uses
  (`memory:` lines): a map's files against the 3,200,000-byte read buffer,
  its placements against its table (320, 400 in dungeons), its pictures
  against its texture blocks; a map script against the 312,816 bytes it
  may have; a character pack against the read buffer, its skinned meshes'
  limits (3,198 vertices, 32 bones) and, for Max's and Monica's packs, the
  memory stack it loads into; a sound bank against its region of sound
  memory and every sound set that loads it. Where a budget is certain and
  exceeded it prints `error:`, writes the image anyway and fails; where the
  use is estimated (a model's or a map's built size) or the disc shows no
  room to spare, it prints `warning:`. Nearly every budget is close to full
  on the disc: Max's model packs, the swords, hats and shoes come within a
  few percent of their stacks, the largest map script within 3,152 bytes,
  and the field sound banks a script loads together leave 5,792 bytes at
  most.
- A member a pack does not have is added at the end of the pack, so the
  members it had keep their places (the game finds members by name,
  ignoring case; a name that differs from one the pack has only in case is
  refused).
- A path the disc does not have at all is a new file: `build` puts it in
  free space and gives it a record in `DATA.HD4`, the index the game reads
  at boot. The game reads that index into a fixed buffer with room for
  about 7.8 KB more on the PAL disc (each new file takes 13 bytes plus its
  path, so about 150 files), and `build` refuses when it would overflow and
  says how much is left after each build. Put new models, textures and
  sounds into packs where you can: a new member costs nothing there.
- A game file can also be given in a common format, named after the file it
  stands for plus the format's suffix: `build` converts it, with the disc's
  own file as the template, and `mod asset --as FORMAT` exports to it.
- **Textures as PNG.** A texture file (`.img`) holds many named pictures,
  and models find theirs by name. `mod asset ISO NAME
  chara/c02a.chr.d/c02a.img --as png` writes a folder
  `c02a.img.png/` with one PNG per picture (`c02a01.png`). Edit them in any
  image editor and keep only the ones you change; the rest stay as they
  are. `build` matches colours to the picture's palette when it can and
  otherwise reduces the picture to its palette size (16 or 256 colours),
  makes its mip levels again, and accepts a new size. Unedited PNGs build
  back to the disc's own bytes. See [the texture format](assets/textures.md)
  for the limits (a picture a `#texanime` script animates should keep its
  size).
- **Sound effects as WAV.** A sound bank (`.snd`) holds the entries
  `play_se` plays by number. `mod asset ISO NAME snd2/sp/sp_004.snd --as wav`
  writes a folder `sp_004.snd.wav/` with one WAV per entry that plays a
  single sound (`001.wav`). Replace the ones you want with any WAV (any
  rate up to 48000 Hz, mixed to mono; a `smpl` loop makes it loop), and add
  entries by numbering on from the last (`005_beep.wav`, the name after the
  number goes into the bank's table). A WAV with a name gives the mod's
  patches a constant for its entry, named as `pal.h` names the game's
  sounds: `005_beep.wav` in `sp_004.snd.wav/` is `SE_SP_004_BEEP` (5), so a
  patch can `play_se(bank, SE_SP_004_BEEP)`; `build` and `mod check` list
  them, and refuse a name `pal.h` gives another entry. An entry that plays a sequence (a
  short tune of the bank's instruments) is not exported; a WAV given for it
  replaces the tune with that one sound. A changed entry overwrites its
  samples when no other entry plays them, so the bank stays the same size
  or shrinks, and otherwise gets samples of its own. Unedited WAVs build
  back to the disc's own bytes. Keep a bank's sample data near its original
  size: field, event and monster banks share one region of sound memory with
  almost nothing to spare, and a bank that outgrows it overwrites the system
  sounds (`build` warns; see [the memory budgets](assets/memory.md#6-sound-memory-spu2)
  and [the sound bank format](assets/sounds.md)).
- **Text as UTF-8.** The menus' item, shop, map and NPC names, their lines
  and the system messages are message files (`.mes`): a table of message
  ids, each a run of the font's glyphs and layout codes. `mod asset ISO NAME
  text:en/shopname` writes `text/en/shopname.txt` and its line (ids are
  `text:` and the language, `ja` `en` `fr` `de` `it` `es` `ko`, then the
  file's name: `text:fr/system`, `text:de/allmenu`); `mod asset ISO NAME
  menu/1/shop.pac.d/shopname.mes --as txt` writes `shopname.mes.txt` into
  `disc/` instead. Each message follows a line `@` and its id; line breaks
  are the message's, `<page>` a page break, and the icons and slots are
  written as event text writes them (`(O)`, `[heart]`, `[Number1]`,
  `[Item1]`, `[String1]`), other codes in braces (`{wait 12}`, `{0400}` for
  one with no name). Change the messages you want and drop the rest; a new
  `@id` adds a message. An id reaches every copy of the file in that
  language, in every version (English `allmenu.mes` is in 20 menu packs):
  each gets the messages whose text you changed and keeps its own others.
  Unedited text builds back to the disc's own bytes. A character the
  language's font lacks is refused (English has ASCII, full-width
  punctuation and kana; French, German, Italian and Spanish ASCII and their
  accented letters); `meswin/system_N.mes`, `sysmes_N.mes` and
  `etc/helpN.mes` may not outgrow the buffers the game loads them into, and
  a menu's window or column is only so wide. See [the message
  format](assets/text.md).
- **Maps in Blender.** `c2 map-export s17` writes the map as one glTF scene
  (`extracted/maps/s17/`): placements, pieces with their meshes, trigger
  points, lights, cameras, collision and textures. The
  [c2_maps add-on](../src/blender/README.md) imports it as the game draws it;
  move and add placements, doors, chests, spawn points and lights, paint
  collision, edit meshes and textures, model new objects, and export into a
  mod, which gets the line `"map:s17": "maps/s17/s17.gltf"` under `replace`.
  `build` writes the `.map` again, rebuilds the models whose meshes changed,
  adds new models to the map's model pack and pictures to its textures.
  Unedited scenes change nothing. An example is
  [map-edit](examples/map-edit/); the formats are in
  [maps](assets/maps.md).
- **Game data as JSON.** `mod asset ISO NAME data:items data:shops
  data:monsters` writes `data/items.json` (every item's common data, with
  its weapon, usable effect, attachment, ridepod part or fish stats nested:
  attack, WHP, elements and limits, synthesis points, build-ups, ...),
  `data/shops.json` (each item's buy and sell price and each shop's goods,
  one value or a list of eight for chapters 1 to 8) and
  `data/monsters.json` (each monster kind's HP, attack, defence, ABS,
  money, drops, element and attack-kind rates, model and AI script), with
  their `"data:..."` lines. Items also carry what each one adds when
  spectrumised (`spectrumise`), shops Donny's goods (`donny`). The rest:
  `data:inventions` (recipes: ideas, materials; scoops), `data:georama`
  (each town's parts: materials, culture points, counts; fixed and starting
  parts), `data:medals` (each floor's fastest time, fishing record, seal,
  Spheda prizes, practice medal), `data:floors` (each floor's monsters),
  `data:fishing` (spots, race opponents, prizes, fish species and race
  kinds) and `data:characters` (starting HP and equipment, WHP gained at a
  weapon level-up, Monica's monster forms, badge growth). Keep only the
  records and fields you change, each record with the keys that name it
  (`item`, `shop`, `id`, `recipe`, `dungeon` and `floor`, ...):
  `{"items": [{"item": 1, "weapon": {"attack": 50}}]}`. `build` writes only
  the values that change: items into `menu/cfg7`, shops into all sixteen
  shop lists, a value kept in several files (the 19 copies of `inv6.lst`,
  each language's Georama parts and race fish) into every one, and the
  executable tables into the boot executable `SCES_511.90`, patched in
  place. Lists keep within what the game reads (a recipe's 1 to 4
  materials, a floor's 32 monsters) and lines are never added. Several mods'
  tables merge; two giving one value different numbers are refused, as are
  values the game's field cannot hold. Names are labels (they are text).
  What each field means is in [game data](assets/data.md).
- **Character models as glTF.** A character pack (`.chr`) holds a model,
  its skin weights, bind pose, motions and textures. `mod asset ISO NAME
  chara/c02a.chr --as gltf` writes a folder `c02a.chr.gltf/` with
  `c02a.gltf` and `c02a.bin` (the skeleton, the meshes, the skin and every
  motion as an animation) and `textures/` with the pack's pictures as PNG.
  Open it in Blender (File, Import, glTF 2.0), edit the meshes, and export
  glTF 2.0 over it as "glTF Separate"; or edit the PNGs. `build` rebuilds
  the meshes that differ from the export with the disc's model as the
  template: the skeleton keeps its frames, names and order, since motions
  and cloth find frames by name, and meshes are matched to frames by name
  (Blender's `.001` suffixes are ignored). Keep a mesh to 32 bones and 4
  weights per vertex. Changed PNGs, in `textures/` or beside the glTF as
  Blender writes them, go into the pack's textures by picture name;
  textures the model takes from other packs are changed in those.
  Unedited exports build back to the disc's own bytes. Blender writes every
  mesh its own way, so after a round trip through it every mesh is rebuilt,
  as triangle lists rather than the game's strips: the model grows a few
  percent (Monica's pack by 4%). Max's and Monica's
  clothes are separate costume packs (`mainchr/c01fuku1.chr` and the like):
  their own packs hold the skeleton and motions.

  Motions come back too: every KEY of the pack's motion set 0 is an
  animation (an action in Blender) of its name, one key per motion frame at
  30 frames a second. Set Blender's frame rate to 30 (Output Properties,
  Format) before importing, or it resamples them. `build` writes into the
  pack's `.mot` and `info.cfg` only the animations that differ from the
  export by more than a round trip moves them: an edited motion over its own
  frames, a longer or shorter one (or one whose frames another KEY shares)
  at new frames with its KEY moved, and a new action as a new KEY after the
  last (a copy named `walk.001` takes the copied KEY's speed, others 1.00,
  the speed Blender plays it at). A KEY's `step` scales its speed in the
  game: it plays at 30 × step frames a second. Vertex, material-fade and
  visibility keys are not in the glTF; a rebuilt motion keeps those of the
  motion it came from. Deleting an action changes nothing, and motion sets
  other than 0 are not exported. See [the model formats](assets/models.md)
  (sections 3 and 7.3).

**Additions.** The older way to add to a script without copying it, kept for
mods that use it: its `mod.json` can list additions, applied when it is
built:

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
`modkit_text_open`, `modkit_text_show`, `modkit_text_close`),
`"modkit/input.c"` (`modkit_wait`, `modkit_pressed`, `modkit_wait_for_button`,
`modkit_wait_for_confirm`) and, for monster scripts, `"modkit/tick.c"`
(`modkit_tick(flag, frames, program)`, the timer behind the caption pattern
in the [modding guide](modding.md)). See [the SDK](sdk.md#modkit). A quoted include is looked for from the
script's folder up, then in `sdk/`. Libraries can include libraries; each is
included once.

**Tests.** `mods/NAME/tests.c` holds `int test_...(void)` functions;
`c2 mod test SOURCE NAME` compiles them with the mod over the tree and
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
- **Monica's first fight**, as a mod.
  `c2 mod override extracted/scripts first-fight --map s51 start_castle_battle` writes
  `mods/first-fight/s51.c` with the function in it. There, replace the four
  `MONSTER_GRIFFON_SOLDIER_523` with other kinds from
  `extracted/scripts/include/disc.h`, loading each first:
  `load_monster_data(MONSTER_SEWER_RAT, 1); load_monster_data(MONSTER_FROGGY, 0); ...`
  and `place_monster(MONSTER_SEWER_RAT, ...)` and so on. That soldier's own
  script ends the fight when the fourth one dies, which other monsters do
  not do, so also add
  `PROGRAM(monsters_cleared, start_georama_with_starter_items);` to the
  patch: the engine runs program 1500 once no monster is left. Build with
  `--mod first-fight --start-at s51:500`; the patch reaches all five
  languages of the map's script. Or leave `start_castle_battle` as it is
  and wrap the natives it calls: `c2 mod override extracted/scripts
  first-fight --map s51 load_monster_data place_monster` writes wrappers
  that pass every call on, and changing the kind in each (`if (monster ==
  MONSTER_GRIFFON_SOLDIER_523) { monster = MONSTER_SEWER_RAT; }` before
  `original(...)`) swaps every soldier the map loads and places.

## Improving the definitions

Names, docs and global names you write in decompiled source can be fed back
into a definition set, so everyone's next decompile gets them:

```
c2 defs learn extracted/scripts definitions/pal
```

`learn` compiles each script, matches functions by their bytecode
fingerprint, and records any name, parameter or local name, doc comment, or
global name that differs from the decompiler's defaults; a variable's value
kind or enum (`bgm_file track;`), a changed return type, a `// note: ...`
comment above a statement, and the file name, top comment and global
names of each library. A function that
appears in several files takes the copy you edited; the aliases the
decompiler gives a second copy in one file (`start_event_for_program_506`,
`do_nothing_2`) are not learned as names. See
[definitions](definitions.md).

## Checking a C toolchain can read the output

The tree's `compile_flags.txt` holds the flags, so clangd and `clang-tidy`
find them alone. By hand:

```
clang -m32 -fsyntax-only -x c -std=c2x -I sdk -I definitions/pal/include -I extracted/scripts -I extracted/scripts/include -Wno-unknown-pragmas extracted/scripts/<path>.c
```

`sdk/c2s.h` gives C tools a meaning for c2s's few extensions. What clang
still reports are argument mismatches that exist in the retail scripts.
