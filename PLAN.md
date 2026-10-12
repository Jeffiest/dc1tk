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

## Next: names and docs

1. **Review function names.** They were written from the code and its
   dialogue comments, a few hundred at a time. An audit of 1,800 random
   functions (a fifth of the 8,254 distinct ones) found 85% right; the rest
   were missing docs, about 60 wrong names and a few wrong docs, all fixed.
   A second audit read the monster AI (`dungeon/monster` and its libraries)
   against the SDK and renamed 134 distinct functions and corrected the
   docs of 475: calm and enraged loops swapped, `on_dodge_shot` handlers
   bound to `dodge_melee`, branches no roll can reach, "100 unit" steps cut
   off after a few frames, and docs that read old-numbered calls literally.
   A third read the map libraries' scenes without dialogue (about 7,800
   definitions, 1,800 distinct bodies) and fixed 141 functions: warp docs
   with wrong program numbers, wrong map parts and characters, and names
   like `setup_monster_628_boss_arena` (the Metal Flotsam fight). Still
   unaudited: map scripts' own functions, dialogue scenes, and the later
   files of each scene group, which got a lighter pass.
2. **Review script docs.** Not audited. Library docs were (164 corrected,
   70 libraries renamed: language variants read as floors, docs listing
   content the file lacks), all but about 100 small monster libraries; so
   were the 185 story flags (35 renamed, 91 comments corrected, flag 51
   named; `FLAG_FINAL_BOSS_DEFEATED` was Griffon's defeat, now
   `FLAG_GRIFFON_DEFEATED`, and 732 is the Dark Element's).
3. **Fewer numbered names.** About 2,700 function definitions on the disc
   end in a number. Most are copy numbers: a function that appears twice in
   a file (`resume_guard_and_dodge_after_shot_3`), or two functions given
   the same name (`do_nothing_2`); some numbers are part of the name
   (`enter_i01h01_event_105`). Give copies that callers tell apart, and
   same-named functions that differ, their own names.
4. **`defs learn` for constants.** It learns function names, variables,
   docs, script docs, globals, and library paths and docs. Constants in
   `include/` headers and library globals are still edited by hand.

Every Japanese string outside dialogue now has an English gloss; the
dialogue of the original Japanese scripts stays out of the shared set by
design (see [docs/definitions.md](docs/definitions.md)).

## Next: what numbers mean

1. **Sound effects from other banks.** Music files, sound sets and the
   system, base and battle sounds print as names. `play_se` on a bank a
   script loads by path (`load_sound(port, "snd2/sp/SP_000.snd", &id)`),
   a sound set's event bank, or a monster's or character's own bank still
   prints numbers: every bank's cfg names its entries (8,050 names, in
   Japanese), so English names for the common ones and the same variable
   tracking would name most of the rest (see
   [the sound notes](definitions/pal/notes/sounds.md)).
2. **Script-loaded character slots.** Maps do not fill character slots:
   the engine fills the controlled character, the villagers (in an order
   that changes with the story, so they cannot be named per map; scripts
   find them by id, `find_villager_slot(NPC_...)`) and the time gate and
   save point; scripts load the rest (see
   [the character notes](definitions/pal/notes/characters.md)). Of 643
   (map folder, slot) pairs scripts load, 369 always hold the same pack and
   could be named per folder.

## Next: definitions format

The rest of the proposal in [docs/definitions.md](docs/definitions.md):
line notes anchored to an instruction offset, `types` to give a variable a
value kind or enum, and `returns` to override an inferred return type.

## Later

- **Automated emulator test.** A built image has been booted in PCSX2 by
  hand with the two test edits in [docs/usage.md](docs/usage.md) and both
  showed in game. Script that check, so an image is booted and checked
  without a person watching.
- **Other versions.** The NTSC and Japanese discs as definition sets; Dark
  Cloud 1 as a second engine.
- **More examples.** `docs/examples` shows one made-up script. Add original
  examples of a cutscene, a monster AI and a town talk handler, written
  for the docs rather than taken from the disc.

## Open questions

- Raw-number monster calls. Six monster scripts (`b02a1` to `b02a4`,
  `e236a`, `p00_23a`) call natives by number with no group, from an older
  table, marked `"numbering": "raw"` in the definition set. Same-shaped
  twins pin down 18 of its numbers (`old_direct` in
  `src/c2s/tables/monster.json`); the rest, mostly in `b02a1` and
  `p00_23a`, have no twin.
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
