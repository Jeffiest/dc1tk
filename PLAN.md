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
    never sets). The [examples](docs/examples/README.md) are commented
    scripts showing how to write each kind.

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
   Every function has now had one audit. A second look at the 360
   most-called functions found 3 wrong names (`activate_chara_slot` also
   switches characters off, so it is `set_chara_active_by_no`; the red time
   gate scene is Max's, either gate) and 55 wrong docs (return values left
   out, branches misread, a lip sync said to close the stream). The less
   called functions got one pass only.
2. **Fewer numbered names.** About 2,700 function definitions on the disc
   end in a number. Most are copy numbers: a function that appears twice in
   a file (`resume_guard_and_dodge_after_shot_3`), or two functions given
   the same name (`do_nothing_2`); some numbers are part of the name
   (`enter_i01h01_event_105`). The 80 most-called were reviewed: 17
   renamed (`show_system_message_60` is `show_game_over_message`); the
   rest carry an event, analysis or exit number, or are identical stubs
   (`do_nothing_2`, `unlock_interrupts_3`) that cannot be told apart.
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

- Raw-number monster calls. Six monster scripts (`b02a1` to `b02a4`,
  `e236a`, `p00_23a`) call natives by number with no group, from an older
  table, marked `"numbering": "raw"` in the definition set. Same-shaped
  twins pin down 18 of its numbers (`old_direct` in
  `src/c2s/tables/monster.json`); the rest, mostly in `b02a1` and
  `p00_23a`, have no twin.
- Two action scripts (`c01win`, `f204a`) use an older action table too;
  aligning them with their twins pins down 16 of its numbers (`old_direct`
  in `action.json`).
- Calls to numbers the engine does not register (`_EXT_67`, `_EXT_6` and
  others) do nothing: the VM finds no function and goes on. What they did
  in the table they were written for is unknown.
- Padding. Of the bytes after functions, 1,385 blocks follow no known rule:
  294 string blocks the leftover-buffer rule misses and 1,091 functions
  with padding but no strings. 403 of those match the leftover buffer when
  the block length is known, but nothing on the disc gives the length.
  They are kept as `#pragma padding_of` lines.
- Where the leading unpushed strings in 198 functions come from (kept as
  `#pragma strings`).
- Whether the original language had a `for` loop. Loops with an increment
  before the backward jump decompile as `while`, which compiles the same.
- How the eleventh argument of the one 11-argument function is typed, given
  that `arg_types` has ten entries.
