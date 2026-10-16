# Mods

Everything about mods: `mod.json`, patches and their pragmas, the numbers a mod asks for, mods
that build on mods, whole files, game assets and files, tests, and starting a new game at the
scene you are working on. Where to start is in [modding](README.md), editing scripts in
[scripts](scripts.md), every command and option in [commands](commands.md), and how the game
runs what a mod changes in [how scripts run](../game/running.md).

A mod is a folder in `mods/`. Keep the decompiled tree as the decompiler
wrote it and build any set of mods over it:

```
dc2tk mod override extracted/scripts castle --map s51 start_castle_battle
dc2tk build extracted/scripts dc2.iso -o modded.iso --mod castle --mod other
```

`--mods DIR` picks another mods folder. Mods hold game scripts, so they stay
out of this repository (`mods/` is ignored).

Two example mods come with the repository, in `docs/coding/examples/`:
[calculator](../coding/examples/calculator/calculator.c), a calculator with its own model,
2D pictures and sounds and a `start_at` in its `mod.json`, built with
`dc2tk build extracted/scripts dc2.iso -o calculator.iso --mod calculator --mods
docs/coding/examples`; and [spinning-cult](../coding/examples/spinning-cult/README.md), a whole
mod with a new map, a door into it, a new character, an item, data tables and music of its own,
whose README says how it is made and built.

## Patches

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
  script asks the engine for without copying the functions that ask
  (more in [scripts](scripts.md#changing-what-scripts-ask-the-engine-for)):

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
  A patch is C as any script is ([the standard](../coding/c-standard.md)): its
  structs, unions, enums and typedefs (its libraries' too), global arrays
  and structs with values, function pointers, variadic functions, `static`
  functions and locals, and `#pragma std c89` for C's semantics in the
  patch's functions all carry into the target; the SDK headers' macros
  reach its `#if`. A struct the target defines otherwise is refused.
- **`PROGRAM` lines** add programs. To change a program the script has,
  override its function.

## Numbers of a mod's own

`NEW_FLAG(name)`, `NEW_COUNTER(name)`, `NEW_PROGRAM(name)` and
`NEW_MONSTER_VAR(name)`, in any of the mod's own files outside
`files/` (a shared `.h` is handy), ask `build` for a save flag (from 1024), a 16-bit save counter
(from 32, of 128), a program number (from 9000) or a shared monster
variable (8 to 135) that the game, the engine and every mod in `mods/`
leave alone, hand-written numbers included. The numbers are kept in
`mods/numbers.json`, so a name keeps its number from build to build and
saves stay valid when mods are added; a mod left out of a build keeps its
numbers reserved. Use the name where the number would go:
`PROGRAM(SHOW_CAPTION, show_caption);` in the map and
`run_event_script(SHOW_CAPTION);` in the monster. `"start_at"` in
`mod.json` can name a mod's program too (`"s51:SHOW_CAPTION"`). Whole
files under `files/` (below) are compiled without the numbers, so they
can neither ask for one nor use its name; `build` refuses a whole file
that asks. Save flag 2047 is taken by `build --start-at` (see
[trying a change quickly](#trying-a-change-quickly)).

What the game itself keeps in its flags, counters and monster variables, and which numbers it
uses, is in [shared state](../game/running.md#shared-state).

## Text in each language

`LANGUAGE` is the language of the script a patch is merged into
(`LANGUAGE_ENGLISH` for `s51_1.c` to `LANGUAGE_SPANISH` for `s51_5.c`;
-1 for scripts that are not language variants), for `#if` and in code, so
one patch can carry every language's text.

## Mods that build on mods

`"requires": ["base"]` in `mod.json` names
mods this one needs: choosing it chooses them too (`build` says so), and
it is applied after them. Its files see their numbers, so it can include
their headers and libraries by the mod's folder
(`#include "base/names.h"`, then use `BASE_SEEN`), and its overrides wrap
theirs (the later mod's runs first; `original()` runs the earlier one).
A required mod missing from the mods folder is an error. `"after":
["other"]` only orders: when `other` is chosen too, this mod is applied
after it. Where neither says, mods are applied in the order given; a
cycle is an error naming the mods.

A mod can keep code, flags and program numbers for others to use: a
`captions` mod with `captions.h` (`NEW_FLAG(CAPTIONS_ON);`) and a library
`show.c`. Another mod names it in its `mod.json`, `{"requires":
["captions"]}`, and includes its files by its folder:

```c
#pragma target map s17
#include <event.h>
#include "captions/captions.h"
#include "captions/show.c"
```

Building that mod builds `captions` in too, first: the flag has the number
`captions` was given, and where both override one function, this mod's
override runs first and its `original()` runs the one from `captions`.
`"after": ["other"]` orders two mods without either needing the other.

Mods built together merge where they can: patches of one script, overrides
of one function, and data tables field by field. Two mods giving one thing
alike give it once; giving it differently stops `build` (and `mod check
--disc`),
naming it and both mods: `d01f01 room 0 rate is given two ways, by mod a and
mod b`. Two mods may not replace one asset id, one picture or sound entry,
or one game file either.

## The mod commands

- `mod override SOURCE NAME --map s51 FUNCTION...` (or `--script`,
  `--monster`, `--library`) starts a patch, or adds to one: it copies the
  functions from the target (the English variant of a map) into
  `mods/NAME/s51.c`, each after `#pragma override`, with the target line and
  the script's headers, and writes the editor files `mod header` writes.
  Where a map's other languages have the function with different lines
  (text, mostly), those lines go under `#if LANGUAGE == ...` branches, or
  each language gets its whole copy where the copies do not line up. A
  native named instead of a function (`place_monster`) gets a wrapper,
  `place_monster_wrapper`, that takes the native's parameters and passes
  them on, with the native's doc comment.
- `mod header SOURCE NAME` writes, for each patch, a header of what its
  target declares (`mods/NAME/.editor/s51.h`: the script's functions with
  their signatures and doc comments, those of its libraries, its globals
  and its `PROGRAM` table), and the mod's `compile_commands.json`, which
  gives each patch the tree's flags and its header, so clangd knows the
  target's names in the patch. Only editors read them: the patch is not
  changed, and builds are the same with them or without. Run it again after
  decompiling again (see [editor setup](../coding/editor.md)).
- `mod check SOURCE NAME...` compiles every script the mods change, as
  `build` would but without a disc, and prints the mods it added for
  `requires`, the order they apply in, the numbers they were given, and
  overridden functions the tree has renamed (`--rename` writes the new
  names into the mods). With `--disc DISC` (the image or `extracted/disc`)
  it also checks that every asset id the mods name resolves and no two mods
  change one place, and looks up the files the mods' scripts load by name (`load_chara`, `load_sound`,
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

## Whole files

A mod can also replace or add whole files, under
`mods/NAME/files/`, laid out like the tree. Patches are better for changing
the game's scripts; whole files suit scripts and libraries of the mod's own.

- `mod new SOURCE NAME FILE...` copies tree files into `mods/NAME/files/`
  to edit there; files the mod already has are kept. The mod gets its own
  `compile_flags.txt`, so an editor finds its libraries first and then the
  tree's.
- `mod save SOURCE NAME` copies every file edited or added in the tree into
  the mod instead, for edits made in the tree.
- `build` compiles each mod file in place of the tree's, libraries included,
  without the mod's `NEW_FLAG` and kin numbers (above): a whole file writes
  its numbers by hand.
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

## Game assets by id

A mod names the textures, models, sounds and text
it replaces by their asset ids (see [the asset tree](commands.md#the-asset-tree)) in
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
  "file:mods/calculator/calc.chr": {"file": "model/calc.gltf", "like": "model:uki"},
  "music:BG_200": {"file": "music/fish.mid", "like": "BG_193"}
}
```

A `file:` line with `like` makes a new picture file from PNGs, or a new
model pack from a glTF, in the format of the disc's file `like` names (see
[textures](textures.md) and [models](models.md)). A
`music:BG_NNN` line under `add` makes a new music file, `snd2/bgm/BG_200.snd`,
whose tune is the MIDI file, played with `load_bgm(200); play_bgm(0);` (see
[music](music.md)).
`mod asset ISO NAME ID...` copies assets out by id and adds their lines,
`mod check SOURCE NAME... --disc ISO` checks every line resolves, and
`mod migrate ISO NAME` turns a mod's `disc/` folder (below) into such
lines. Two mods replacing one asset are refused. The rules are in
[the design](../../src/docs/design.md#2-mods-by-name), "Mods by name".

A sound reaches every copy of its bank: where a menu pack holds a version
of a bank that differs from `snd2/sp/`'s only in parts the entry does not
use (its volumes), the entry is one sound with both places, and a bank's
folder (`bank:SP_001`) reaches them too. An added sound's name
(`sound:SE_MOD_BEEP`) is a constant the mod's patches use, holding the
entry's number.

## Game files

A mod can replace models, textures, sounds and any other
file on the disc. It keeps them under `mods/NAME/disc/`, laid out as
`extract` lays the disc out: a file at its path in `DATA.DAT`
(`disc/snd2/sp/sp_000.snd`), or a member of a pack under the pack's name
plus `.d` (`disc/chara/c01_base.chr.d/c01b.img` is the texture in Max's
character pack; packs inside packs work the same way).

- `mod asset ISO NAME PATH...` copies files or pack members off the disc
  (the image or `extracted/disc`) into the mod, to replace there.
- `build` writes each over the disc's own, puts members back into their
  packs, and reads them back from the new image to check. Two chosen mods
  may not replace the same game file.
- Maps, models, textures, scripts and sound banks load into fixed memory,
  so `build` checks each load a changed file is part of against
  [the memory budgets](memory.md) and says what it uses
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

Each kind of asset has its own page on exporting it, its editable form and the lines a mod
gives it:

- [Textures](textures.md): PNG.
- [Sound effects](sounds.md): WAV.
- [Music](music.md): MIDI, with a SoundFont of the bank's instruments.
- [Text](text.md): UTF-8 text.
- [Character models](models.md): glTF, opened in Blender.
- [Maps](maps.md): glTF scenes, edited in Blender with the dc2tk_maps add-on.
- [Dungeons](dungeons.md): the floors' parts and room layouts.
- [Game data](data.md): JSON.
- [Memory](memory.md): what the game has room for.

## Additions

The older way to add to a script without copying it, kept for
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

## Shared libraries

`sdk/modkit/` holds libraries any script can include:
`#include "modkit/text.c"` (message windows: `modkit_say`, `modkit_caption`,
`modkit_text_open`, `modkit_text_show`, `modkit_text_close`),
`"modkit/input.c"` (`modkit_wait`, `modkit_pressed`, `modkit_wait_for_button`,
`modkit_wait_for_confirm`) and, for monster scripts, `"modkit/tick.c"`
(`modkit_tick(flag, frames, program)`, the timer behind
[the caption pattern](../coding/tutorials/captions.md)). See
[the SDK](../coding/sdk.md#modkit). A quoted include is looked for from the
script's folder up, then in `sdk/`. Libraries can include libraries; each is
included once.

## Tests

`mods/NAME/tests.c` holds `int test_...(void)` functions;
`dc2tk mod test SOURCE NAME` compiles them with the mod over the tree and
runs them in an interpreter of the script VM, without the game. A test
passes when it returns nonzero, and `print(...)` output is shown for the
ones that fail. Natives do nothing there (a result they would give is 0),
so test the logic: write it as functions that take what the game would give
(the buttons pressed, say) and check what they return or change. A test
that runs past 10,000,000 instructions (a loop that never ends) or nests
calls deeper than 10,000 (a function that calls itself without end) stops
with an error rather than hanging (`vm::STEPS`, `vm::CALLS`).

## Trying a change quickly

Four things keep the loop short: test a mod's logic without the game
(`dc2tk mod test`, see [tests](#tests)); compile what it changes
without a disc (`dc2tk mod check`); build only what changed, which `build`
does by itself; and start New Game at the scene you are working on, then
boot it with `--play`. A mod can name its scene in `mod.json`
(`"start_at": "s17:9000"`, or one of its own programs:
`"s51:SHOW_NEXT_CAPTION"`), so building it is enough; `--no-start-at` builds
a normal New Game.

`mod test` runs the `test_` functions of the mod's `tests.c` (an `int`, no
parameters; nonzero passes), compiled with the mod over the tree, in the
tools' own interpreter of the game's VM. Natives do nothing there, so what
one would return is 0, and `print` records its values, shown when a test
fails. A test that runs past 10,000,000 instructions or nests calls past
10,000 deep stops with an error naming the function, so a loop that never
ends fails rather than hangs.

`build --start-at MAP:PROGRAM` writes a test image whose New Game goes
straight to a scene, without changing the source tree:

```
dc2tk build extracted/scripts dc2.iso -o test.iso --start-at s51:story_50
```

`MAP/SCRIPT:PROGRAM` picks one script of a map that has several
(`--start-at s06/s06b:500`), and a map alone starts at its `map_start`.

A new game starts in dungeon 1's script at `floor_start` (`title.cpp`,
`EntryEventScript` in `dng_main.cpp`). The build compiles that script with a
function in front of `floor_start` that, the first time it runs, sets flag
2047 and warps the way the game's own story warps out of dungeon 1 do:
`load_dungeon_map`, then `load_event_script` at the program. Later visits to
dungeon 1 play normally. That suits maps the game loads with
`load_dungeon_map` (story scenes and dungeon-like maps; `xref map` shows
whether it does). Towns are not loaded that way, and the dungeon loop a new
game runs in ignores `jump_to_map`; for any other map in the disc's map table
(`map/map<language>.cfg`, `map1.cfg` for English, numbered by row; the older
`map/map.cfg` is read by nothing) the warp is the one the game uses to leave a
dungeon for a town, `go_to_edit_mode(map number, program)`, so
`--start-at m05` (at `map_start`) or `--start-at m05:PROGRAM` opens a town.
`build` prints which way it warps. The scene starts with a new game's party
and items, so a scene that expects more may not play right. Use these images
for testing only. The forms `--start-at` takes, and how a town start is set up, are in
[testing without replaying](commands.md#testing-without-replaying).

## A first test

Two edits that show within minutes of starting a new game:

- **The opening.** In `lib/map/m/m07_palm_brinks/base_opening_scenes_v1.c`, function
  `play_opening_letter_and_storm_scene`, reverse the three
  `show_narration_line` calls (messages 10010, 10020, 10030) and change
  `set_sprite_color(0, 0, 0, 0, 128)` to `set_sprite_color(0, 128, 0, 0,
  128)`. The letter is narrated last line first, over red instead of black.
- **Monica's first fight**, as a mod.
  `dc2tk mod override extracted/scripts first-fight --map s51 start_castle_battle` writes
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
  and wrap the natives it calls: `dc2tk mod override extracted/scripts
  first-fight --map s51 load_monster_data place_monster` writes wrappers
  that pass every call on, and changing the kind in each (`if (monster ==
  MONSTER_GRIFFON_SOLDIER_523) { monster = MONSTER_SEWER_RAT; }` before
  `original(...)`) swaps every soldier the map loads and places.
