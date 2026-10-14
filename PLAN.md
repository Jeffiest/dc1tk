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

## Done: editing maps in Blender

Any fixed map opens in Blender (`c2 map-export`, the `c2_maps` add-on in
[src/blender](src/blender/README.md)) as the game draws it, and a mod's
`"map:NAME"` line builds the edited scene back: placements, pieces and
trigger points moved, added and deleted, collision painted, meshes and
pictures edited, new models with collision, animated pieces (`FUNC_ANIME_DATA`,
decoded: what moves, how and when) shown and edited. All 171 scenes round-trip through
Blender unchanged. The formats and conventions are in
[maps](docs/assets/maps.md); the example [map-edit](docs/examples/map-edit/)
was checked in the game. Left over:

- **New maps.** A wholly new map needs a map number, the engine's map tables
  and a way in; only the maps on the disc can be edited.
- **Dungeon floors.** Random floors are built from parts at run time; their
  layouts and parts are not editable yet.

## Now: the rest of the game

What still cannot be taken apart, edited and put back, in the order to do it:

1. **`--start-at` for towns** (done). A new game runs in the dungeon loop,
   where `jump_to_map` does nothing; towns are entered with
   `go_to_edit_mode(number, program)`, the number from the disc's
   `map/map<language>.cfg`. `--start-at m05` (or `"start_at": "m05"`) starts
   a new game in a town, scripts of its own or not.
2. **Menu and system text** (done). All 302 `.mes` files (and 8 `fconv`
   files laid out the same way) build back byte for byte and export as UTF-8
   text with the game's codes as tags ([text](docs/assets/text.md));
   `text:en/shopname` in `mod.json` changes a file in every copy and version
   of that language. Japanese and Korean beyond ASCII show as `{XXXX}` (no
   table on the disc gives their glyphs); the English font has no accented
   letters. Not yet tried in the game.
3. **Game data** (done). Where every kind lives is in
   [data](docs/assets/data.md); each is a `data:` id in `mod.json`, JSON with
   only the changed fields, merged field by field across mods: `data:items`
   (item and weapon tables, spectrumising), `data:shops` (prices, goods per
   chapter, Donny's shop), `data:monsters` (stats in `SCES_511.90`, patched
   in place), `data:inventions` (recipes, scoops), `data:georama` (parts,
   fixed and starting parts), `data:medals`, `data:floors` (monsters per
   floor), `data:fishing` and `data:characters` (starting HP and equipment,
   weapon level-ups, Monica's forms, badges). Fields are named where the
   engine reads them; the rest are shown to be unread. Left: adding records
   (new items, shops, recipes, parts), Georama requests (mostly code), rivers,
   townspeople (`npc*.cfg`) and a few executable tables (`gift_item_tbl`,
   `GolfClubDef`, `aquafish_mixTable`). Being tried in the game.
4. **Memory budgets** (done). [memory](docs/assets/memory.md) gives each
   budget from the engine (read buffer, map and loading-screen limits,
   script buffer, placements, texture blocks, GS memory, character stacks,
   SPU regions) with the retail maximum and headroom; `build` checks every
   changed load against them (an error over a certain budget, a warning over
   an estimated one). Estimates remain for map and pack memory; retail is
   predicted to overflow the field SPU region by 1,392 bytes in s01 (set
   1600 with EV_100_155), not observed.

## Next: standard C

Make the script compiler a conforming C89/C90 freestanding implementation
(the whole language, and `<float.h>`, `<limits.h>`, `<stdarg.h>` and
`<stddef.h>`; no hosted library), for code people write: decompiled retail
code keeps compiling to its own bytes. What the compiler lacks today
([language](docs/language.md)), in the order to do it:

1. **A conformance suite.** A test of every C89 feature, each passing,
   failing to compile, or compiling with different behaviour, so progress is
   measured.
2. **Cheap features:** `goto` and labels, `sizeof`, the comma operator,
   unary `+`, `const`/`volatile`/`register`/`auto`, `static` locals (kept
   between calls), initialised globals, multi-dimensional local arrays, K&R
   function definitions, macros with parameters with `#` and `##`, `#error`,
   `#line`, `__FILE__`/`__LINE__`/`__STDC__`, and full `#if` arithmetic.
3. **Integer types:** `char`, `short`, `long`, `signed` and `unsigned` with
   C's widths, wrap-around and conversions, emulated on the VM's ints.
4. **C's semantics in a strict mode** (`#pragma std c89` or a mod option):
   assignment converts to the variable's type (the VM stores the value's own
   type), `&&`/`||` give 0 or 1, and natives in conditions run only when C
   would evaluate them (lowered to branches). Retail code keeps its meaning.
5. **Composite types, lowered to what the VM has:** structs and unions as
   their members' slots, global arrays and global pointers (the VM has no
   indexed access to globals: generated accessors), pointer arithmetic and
   pointers to pointers, function pointers (an index and a generated
   dispatch), and user-defined varargs with `<stdarg.h>`.
6. **`double` is left out on purpose.** C89 asks for about 10 significant
   digits and the VM's floats are 32-bit, so code that uses `double` or
   `long double` is refused with a clear error rather than given 32-bit
   doubles; the implementation documents this as its one departure. The
   `string` type stays as an extension.

## Next: modding

Done: mods that need mods (`requires`, `after`), editor headers for patches
(`mod header`), and overrides that follow a renamed function by its
fingerprint (`mods/NAME/fingerprints.json`, `mod check --rename`); see
[modding](docs/modding.md).

- **Wrapping natives.** `#pragma override place_monster` in a patch would
  route every call the target makes to a native through the mod's function,
  so swapping a fight's monsters would not mean copying the function that
  places them.
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

1. **Sounds in the game** (fixed where c2 was wrong, being tried). The game
   reads the copy `build` rewrites (`DATA.HD4`'s first match); the event 50
   test's sounds were most likely too quiet (a 40-140 Hz boom at volume 44)
   or masked by an unchanged shared glass sample. A real gap was fixed: banks
   whose copies differ (the menu packs' `SP_001`-`SP_003`, the two `SY_000`,
   `BG_193`) were separate assets, so a sound reached only one copy; a sound
   now reaches every version of its bank. `sound-test.iso` has loud test
   tones on the title menu and in event 50.
2. **Memory** (done, see "Now: the rest of the game" item 4). The sound-set
   puzzle is settled: a set's event bank never loads with that set's own
   field banks; a script's `load_sound(set)` adds only the event bank to the
   set already loaded.
3. **Names for a mod's new sounds** (done). `005_beep.wav` in
   `sp_004.snd.wav/` gives a mod's patches `SE_SP_004_BEEP = 5`; `mod check
   --disc` checks the files and pack members scripts load.
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

- **Engine code.** Changing the executable itself has been done in another
  project; it will be brought in here later. Until then mods change scripts
  and data, not `SCES_511.90`.
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
- **Movies.** The `.pss` FMV files replaced from a common video format.

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
