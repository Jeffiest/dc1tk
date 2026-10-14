# Plan

What is done, and what comes next. How things work is in [docs/](docs/).

## Done

Each milestone had a gate that had to pass on every script on the PAL disc.

1. **Extract.** Every file in `DATA.DAT`, packs unpacked: 46,353 files,
   1,398 scripts.
2. **Format.** Every `.stb` reads to a plain structure and writes back byte
   for byte, with no byte left unexplained.
3. **Natives.** All 1,407,551 native calls resolve to a number; all but 227
   name a native the engine registers.
4. **Assembly.** Disassemble and assemble round-trip every script.
5. **Decompile.** All 144,202 functions structure into c2s; none needs
   `goto` or falls back to assembly.
6. **Compile.** Every script decompiles, prints, parses and compiles back to
   the same bytes, through shared libraries, the SDK and every definition.

Since then: c2s became C (return types, pointers, casts, `extern` natives);
the SDK documents every native, with 98 inline helpers and about 1,000
named constants; shared code moves into libraries; the definitions layer
holds names, docs and translations; `defs learn` turns edited source into
definitions.

7. **Names for everything.** Every function on the PAL disc is named, with
   named parameters and locals and a doc comment: 144,202 functions in
   8,254 distinct classes. Every script has a doc comment and named
   globals; each of the 1,097 libraries has a descriptive path and a doc.
8. **Text from the disc.** Message calls carry the line they show, read
   from the user's disc; inline strings in the French, German, Italian and
   Spanish scripts carry their English sibling; item, map and NPC ids and
   character model packs carry their names.
9. **Value kinds and story flags.** `message`, `item_id`, `map_id`, `flag`
   and enum-typed parameters, spread into script functions; the 185 story
   flags as `enum flag_story` in `definitions/pal/include/pal.h`.

10. **Valid C.** Every decompiled script passes
    `clang -m32 -fsyntax-only -x c -std=c2x -I sdk -I definitions/pal/include`
    with no errors; headers accept every call retail makes (see
    [the SDK](docs/sdk.md)).

11. **Disc round trip.** `build` compiles an edited source tree and writes a
    new disc image with only the changed scripts replaced, inside their
    packs, in place or in the disc's filler space, with `DATA.HD4`
    updated. An unchanged tree rebuilds the original image byte for byte,
    and an image with edited scripts boots in PCSX2 and plays the edits.

12. **Modding tools.** Decompiled files are `.c` with a `compile_flags.txt`,
    so clangd editors read them with the SDK, and keep the game's codes with
    readable names after them (`map/s/s51_raybrandt_castle/`). Mods are
    folders of changed files that `build --mod` layers over the tree. `xref` and `flags` answer
    what starts an event, how a map is entered, which AI a monster runs and
    which save flags are free; a tree records what the decompiler wrote, so
    `changes`, `mod save`/`apply` and `fork` keep edits apart from its
    output; `build --start-at` makes a test image that starts at a scene.
    `PROGRAM` lines say which trigger point, door or treasure box in the
    map's `.map` file starts them.
    How scripts run, as far as a mod needs, is in
    [docs/modding.md](docs/modding.md).

13. **Definitions format.** A set can give variables a value kind or enum
    (`types`), override return types (`returns`), attach line notes by
    instruction (`notes`), mark scripts built for an older native table
    (`"numbering": "old"` or `"raw"`), and record the disc it was made for
    (`disc.json`). `defs learn` reads all of these from edited source, and
    a whole-disc decompile removes the files an earlier one wrote that it
    no longer writes. Music, sound sets, sound bank entries and fixed
    character slots print as names.

14. **English in place of Japanese.** The game's asset names (motions,
    texture animations, effects, map parts, damage settings) print as
    English `#define`s written into `disc.h` (`MOTION_STAND`); only dialogue
    in the original Japanese scripts stays Japanese. Locals are named for
    what fills them or what they are passed to (`griffon_defeated`,
    `home_x`, `sound_id`; `unset_...` for a slot the function reads but
    never sets).

15. **Mod patches.** A mod no longer copies whole files to change them. A
    patch names its scripts (`#pragma target map s51`, `monster golem`,
    `library ...`) and holds only what it changes: `#pragma override`
    functions, which can call `original()` and chain across mods, added
    functions, globals and programs, and `#pragma every_frame` functions
    that run before every `wait;`. Mods ask for save flags, program numbers
    and shared monster variables by name (`NEW_FLAG`), and `build` gives
    numbers nothing else uses, kept in `mods/numbers.json` so saves stay
    valid. `LANGUAGE` lets one patch carry every language's text, and
    `mod override` copies a function out of a map's five languages with the
    lines that differ under `#if LANGUAGE`. An unedited override compiles
    back to the script's own bytes. `mod check` compiles a mod without a
    disc.

## Done: Rust, an asset tree, and mods by name

See [the design](docs/design.md). In order:

1. **Port to Rust** (done: `src/`, one `c2` tool with the old Python
   tool's commands), milestone by milestone, each ending when the Rust gave
   the Python's bytes (disc, scripts, mods, assets) and its tests ran under
   `cargo test`; then the Python was deleted. Work runs across all cores.
   Parity scripts compared the two while both existed;
   [src/README.md](src/README.md) describes the crates.
   - Done: the disc (`c2 extract`, writing an image, cp932), the same bytes
     as the Python on the whole PAL disc.
   - Done: the script reader (`c2 disassemble`, `c2 assemble`), the same
     text for all 1,398 scripts, 16 times faster.
   - Done: the decompiler (`c2 decompile`, a whole disc in 21 s against
     8 min 38 s) and the compiler (`c2 compile`), the same trees and bytes.
   - Done: build and mods (`c2 build`, `c2 mod ...`, `c2 flags`), the same
     images for every mod in `mods/`; and the asset converters
     (`c2-assets`: PNG, WAV, glTF), the same files both ways.
   - Done: `verify`, and the Python's tests as `cargo test`.
   - Done: `xref`, `maps`, `defs`, `natives`, `setup`: every command is ported.
   - Done: the Python removed; `c2` is the only tool.
2. **The asset tree** (done: `c2 assets`): `extracted/assets/` beside `extracted/scripts/`, every distinct
   texture, model and sound once, as PNG, glTF and WAV, with a manifest of
   ids and every place the disc has each.
3. **Mods by name** (done: `replace`/`add` by id in mod.json, `c2 mod migrate`): `mod.json` maps asset ids (`texture:c02a02`,
   `sound:SE_EV_000_130_GRIFFON_BREAK_WINDOW`) to files anywhere in the mod;
   a replacement reaches every copy on the disc. `mod migrate` converts the
   `disc/` layout.

## Now: editing maps in Blender

Goal: open any fixed map in Blender, move, add and delete pieces, placements,
trigger points (doors, chests, spawn points, events), lights and collision,
and build the result into a mod. The formats and the glTF conventions are in
[maps](docs/assets/maps.md) (sections 7 and 8). Phases:

1. `.map` reader and writer in Rust, byte-exact on all 210 files; `c2 map`
   point labels fixed. Mesh side: whole node names, import matching by
   `mds_index`, a collision mode, textures from a search path, blend flags.
2. `c2 map export` (one map or all fixed scenes) to the glTF scene of
   section 8, with real textures.
3. Import: `"map:NAME"` lines in `mod.json`, built by `build` (changed
   `.map`, changed and new meshes, textures), picked up by `mod make`.
4. A Blender add-on: point panels, adding doors, chests and spawn points,
   collision colours, one-click import and export through `c2`.
5. An example mod that edits a map, checked in the game.

Phases 1 to 4 are done, with mesh, texture and new-model edits, the game's
look in Blender (blend flags, vertex alpha, light sets, hours) and unedited
round trips through Blender changing nothing on the maps tried. The example
is [map-edit](docs/examples/map-edit/), checked in the game: the new crate stands
with its own picture and blocks the player, and the repainted grass shows.

## Next: modding

- **Overrides that survive renames.** A patch overrides a function by name,
  and the definitions still rename functions. Record the fingerprint of
  each function a patch overrides, so when a later decompile renames it,
  `mod check` finds it by fingerprint and says the new name (or follows it).
- **Wrapping natives.** `#pragma override place_monster` in a patch would
  route every call the target makes to a native through the mod's function,
  so swapping a fight's monsters would not mean copying the function that
  places them.
- **Editor support for patches.** clangd does not know a target's own
  functions and globals in a patch. `mod override` could write a header of
  their declarations beside the patch.
- **Mods that need mods.** `requires` and `after` in `mod.json`, so a mod
  can build on another's library or numbers and `build` orders them.
- **Whole-file mods into patches.** A `mod slim` that compares each copied
  file with the tree and writes overrides for the functions that changed.
- **Save counters.** `NEW_COUNTER` once the number of 16-bit save counters is
  known (scripts use 0 to 20).

## Next: mod assets

Mods can bring models, textures and sounds made with common tools, not only
scripts (see [usage](docs/usage.md#mods) and [the formats](docs/assets/)):

- **Any game file or pack member** replaced from a mod's `disc/` folder,
  laid out as `extract` lays out the disc, with new members added to packs
  and new files given records in `DATA.HD4` (whose fixed 0x50000-byte
  buffer leaves room for about 150 more on the PAL disc; `build` counts it).
- **Textures as PNG**, a folder per texture file with one PNG per picture
  (`c02a.img.png/c02a01.png`); all 5,217 texture files round-trip.
- **Sound effects as WAV**, a folder per bank with one WAV per entry
  (`sp_004.snd.wav/001.wav`), replacing entries or adding new ones; all 861
  banks round-trip.
- **Character models as glTF** for Blender, a folder per character pack
  with the skeleton, skin, motions and textures (`c02a.chr.gltf/`); only
  meshes that change are rebuilt, with the disc's model as the template.
  Tried through Blender 5.1: the skeleton, motions and textures come back
  unchanged and the game's animations drive the edited mesh.

Unedited exports build back to the disc's own bytes in every case. Still to
do:

1. **Sounds in the game.** A test mod in Monica's first fight (recoloured
   PNG textures, a bigger head from glTF, green soldiers, two event sounds
   as WAV) showed every change in PCSX2 but the sounds: the old ones were
   heard. The bank on the disc is the new one (one copy, the one sound set
   800 names), its rows 4 and 13 are KeyOn rows of new programs, and the
   engine's cfg reader and IOP upload read it as written. Likely the test
   sounds: the crash was a 40-140 Hz boom, near silent on TV speakers, and
   the original's glass layer, shared with entry 6, still plays; the chime
   was at the fade. Rebuild with loud mid-range sounds; if the old ones are
   still heard, read the engine's sound table over PINE once event 50 has
   loaded (`PortInfo` 0x3F6540, port 4) to see what the rows became.
2. **Memory.** Models, textures and sound banks load into fixed memory the
   build cannot see, so it only warns when a file grows. Measure the
   budgets: the character memory stacks, how much of a texture block a
   scene leaves free, and the SPU regions (a sound-set row's banks add up
   to more than the field region as read, so something is not understood).
3. **Names for a mod's new sounds.** New bank entries are used by number;
   generate names for them (`SE_SP_004_BEEP`) as `disc.h` names the game's,
   and have `mod check` check the pack paths and member names scripts load.
4. **Motions from glTF.** Export writes every motion as an animation, but
   build does not read animations back yet: decode the remaining channels
   (per-vertex, material alpha, visibility), confirm the frame rate, and
   write `.mot` keys and `info.cfg` `KEY` ranges from glTF animations.
5. **New characters.** A model is rebuilt on the disc model's skeleton; a
   wholly new character needs a new pack, an `info.cfg` and a skeleton of
   its own from glTF, and cloth (`.clo`) left out or carried over.
6. **Music.** `.sq` sequences (music and ambient loops) are not decoded, and
   a WAV given for a sequence entry replaces it with one sound; sequences
   from MIDI are a separate job.

## Next: names and docs

1. **Review function names.** They were written from the code and its
   dialogue comments, a few hundred at a time. An audit of 1,800 random
   functions (a fifth of the 8,254 distinct ones) found 85% right; the rest
   were missing docs, about 60 wrong names and a few wrong docs, all fixed.
   Audits since then read, against the code and the SDK:
   - the monster AI (`dungeon/monster` and its libraries): 134 distinct
     functions renamed, 475 docs corrected (calm and enraged loops
     swapped, `on_dodge_shot` handlers bound to `dodge_melee`, branches no
     roll can reach, docs that read old-numbered calls literally);
   - the map libraries' scenes without dialogue (1,800 distinct bodies):
     141 functions fixed (wrong program numbers, map parts and characters);
   - the map scripts' own functions (2,700 distinct bodies) and the
     libraries' dialogue scenes (900): 373 functions fixed (Gaspard mixed
     up with Dr. Jaming in d04b01, a Griffon collapse called a stabbing,
     Sun's confession to Alexandra described as another scene, wrong
     flags, characters and text languages).
   Every function has now had two audits. The second read the 360
   most-called functions (46 fixed) and then the other 5,716 distinct
   ones in 62 batches: 1,069 functions corrected, 120 of them renamed,
   and the same fix carried to 889 copies in language variants and
   sibling libraries with identical code. The errors it found were mostly
   docs that called the event started on arrival an entry point, named
   Max where the code uses the active character (or the Ridepod, or
   Monica's monster form), gave scenes to the wrong speaker, quoted raw
   frame counts and distances the code cuts short, or left out return
   values; and names for the wrong monster or event (the Vanguard's name
   on Mammo, a "Griffon" fight against Sirus, Gaspard for Dr. Jaming).
2. **Fewer numbered names.** 2,196 function definitions on the disc end
   in a number (from about 2,700). The rest are copies the decompiler
   numbers because a file holds two functions of one name, identical
   stubs (`do_nothing_2`, `unlock_interrupts_3`) that cannot be told
   apart, or event, exit and analysis numbers that are part of the name.
3. **Constants.** `defs learn` learns function names, variables and their
   kinds, return types, line notes, docs, script docs, globals (a
   library's too), and library paths and docs. The constants (flag names,
   the music, sound and bank enums) live in the set's `include/pal.h`,
   which a decompiled tree includes from the set itself, so an edit there
   is already an edit of the set; renaming one means renaming its uses
   across the tree too (an editor's rename does both).

Script docs (210 corrected), library docs (190 corrected, 85 libraries
renamed) and the 185 story flags (35 renamed, 91 comments corrected) have
been audited too. Every Japanese string outside dialogue has an English
gloss; the dialogue of the original Japanese scripts stays out of the
shared set by design (see [docs/definitions.md](docs/definitions.md)).

## Next: what numbers mean

1. **The last sound effects.** 97% of the entries passed to `play_se`
   and its kin (8,611 of 8,858) print as names: the system, base and
   battle banks, the 210 banks scripts load, followed through copies,
   branches, globals, calls and helpers' out-parameters, and into library
   code when every script that includes it passes the same bank. About
   180 of the rest read the event port the engine filled with the battle
   music in dungeons (`EV_200_000` entry 0, see
   [the sound notes](definitions/pal/notes/sounds.md)); the others are
   helpers whose callers pass different banks.
2. **Villager slots.** Slots each map folder's scripts always load with
   one character are named per folder in `disc.h` (`CHARA_I01_H01_STEVE`),
   as are the engine's fixed slots (`CHARA_PLAYER`). Villager slots change
   with the story and stay numbers; scripts find villagers by id (see
   [the character notes](definitions/pal/notes/characters.md)).

## Later

- **Automated emulator test.** A built image has been booted in PCSX2 by
  hand with the two test edits in [docs/usage.md](docs/usage.md) and both
  showed in game. Script that check, so an image is booted and checked
  without a person watching. PCSX2 can run with its own settings
  (`-datapath`, `-nogui`) and log the game's console output to a file, so a
  test mod could `print` a marker. But no script runs before a new game is
  chosen on the title menu (the title screen only plays the attract movie,
  `title.cpp`), and PCSX2's command line cannot press a button; an input
  recording played from boot could.
- **Other versions.** The NTSC and Japanese discs as definition sets; Dark
  Cloud 1 as a second engine.

## Open questions

- Old native tables. Seven monster scripts (`b02a2` to `b02a7`, `e236a`)
  and two action scripts (`c01win`, `f204a`) were built for older native
  tables. Lining their calls up with current twins pins down 41 monster and
  27 action numbers (`old_direct` in `src/natives/`), which also says
  what most of their unregistered `_EXT_n` calls were meant to be. Still
  unknown: monster old 161 and 162 (only in the b02a2 to b02a7 init), and
  what the current-numbered `_EXT_20(1)` and `_EXT_20(5)` in `b02a1` did
  (the base of the sound group, which the engine leaves unregistered).
  `b02a1` and `p00_23a` use the current table: a call by bare number is
  how any script calls the first native of a group.
- Whether the original language had a `for` loop. All 36,503 loops on the
  disc have the one shape a `while` gives (condition at the top, one jump
  back, two labels), and all 278 `continue`s jump to the condition. Of the
  5,579 loops that end by stepping a variable the condition reads, the 109
  with a `continue` repeat the step by hand before it, which leans against
  a `for` with a continue-to-step label, but does not settle whether the
  keyword existed.

Settled: the bytes after functions and the leading strings (the compiler's
string buffer, see [the format notes](docs/format.md)); the eleventh
argument (three functions take 11, nothing calls them, and the engine never
reads `arg_types`).
