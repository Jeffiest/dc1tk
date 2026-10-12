# ChronicleTwo-Script

Tools to decompile, edit and rebuild the scripts of **Dark Chronicle** (Dark
Cloud 2), PAL version.

Most of the game's behaviour lives in compiled scripts rather than in the
executable: every cutscene, conversation, door, monster AI, the player's
moves and the scripted effects. These tools turn every one of them into
readable C, with named functions, documented engine calls and English
comments, and compile your edits back into a disc image you can play in
PCSX2/A future port. Unedited scripts compile back to exactly the bytes on the disc.

> **No game files are included.** This repository holds tools, an SDK and
> knowledge about the scripts (names, documentation), never decompiled code,
> game text or assets. You need your own copy of the PAL disc as an ISO
> image: the tools extract and decompile it on your machine, and the
> results (`extracted/`, `decompiled/`, `mods/`, built images) stay out of
> git.

## How to use

### 1. What you need

- Python 3.8 or later. Nothing else to install.
- Your own Dark Chronicle PAL disc image, saved here as `dc2.iso`.
- [PCSX2](https://pcsx2.net) to play the result.
- Optional: an editor that uses clangd (VS Code with the clangd extension,
  Neovim, Emacs, Zed), which then completes and documents every engine call.

Commands below use `python3`; on Windows type `python`.

### 2. Decompile the game

```
python3 cli.py setup dc2.iso decompiled
```

This takes a few minutes. It extracts the disc into `extracted/` and writes
every script into `decompiled/` as C: one file per script, with code that
several scripts share in `lib/`. Folders and files keep the game's own codes,
with a readable name added: `map/s/s51_raybrandt_castle/` is Raybrandt Castle,
`dungeon/monster/e33a_golem.c` is the Golem's AI. Open `decompiled/` in your
editor.

### 3. Make a change

Keep your changes in a **mod**: a folder in `mods/` that holds only the files
you change. Copy in the files you want to edit:

```
python3 cli.py mod new decompiled my-first-mod map/s/s51_raybrandt_castle/s51_1.c
```

and edit them in `mods/my-first-mod/files/`. The decompiled tree stays as it
was, so several mods can be kept apart and built in any combination.
(You can also edit `decompiled/` directly and save the edits into a mod
later with `python3 cli.py mod save decompiled my-first-mod`.)

To find what to change:

```
python3 cli.py maps decompiled            every map, and which ones you can play in
python3 cli.py map decompiled s51         one map: its programs, triggers, player positions
python3 cli.py xref decompiled monster golem      a monster's AI and who places it
python3 cli.py xref decompiled event 510 --map s51   what starts an event
python3 cli.py flags decompiled           save flags in use, and free ones for a mod
```

[A first test](docs/usage.md#a-first-test) has two small edits to try, and
the [modding guide](docs/modding.md) explains how the game runs its scripts.

### 4. Rebuild the disc

```
python3 cli.py build decompiled dc2.iso -o modded.iso --mod my-first-mod
```

`build` compiles the scripts your mod changes and writes `modded.iso`: your
disc with only those scripts replaced. It takes seconds. Add `--mod` again for
more mods. To test a change without playing up to it, add
`--start-at MAP:PROGRAM` (for example `--start-at s51:story_50`), and New
Game starts at that scene.

### 5. Play

Boot `modded.iso` in PCSX2 like the original, or let the tools start it:

```
python3 cli.py play modded.iso
```

(or add `--play` to `build`).

## Documentation

- [Usage](docs/usage.md): every command, mods, and building.
- [Modding guide](docs/modding.md): how the game runs scripts, as far as a
  mod needs to know; message text codes; patterns; editor setup.
- [The language](docs/language.md): what the decompiled C means, and the C
  you can write in your own code.
- [The SDK](docs/sdk.md): the headers every script includes, and `modkit`,
  the shared library for mods.
- [Definitions](docs/definitions.md): how names and documentation are shared
  without sharing the scripts.
- [Examples](docs/examples/README.md): one made-up script at every stage from
  bytecode to annotated source.
- [The script format](docs/format.md), [assembly](docs/assembly.md) and
  [how the tools work](docs/internals.md).

## What is in this repository

```
cli.py           the command line (python3 cli.py --help)
src/c2s/         the tools: disc, format, assembler, decompiler, compiler, mods
src/tests/       tests (python3 cli.py test)
sdk/             C headers scripts include: every engine call declared and
                 documented, constants, helpers; sdk/modkit/ for mods
definitions/pal/ what is known about the PAL scripts: names, docs,
                 translations; see docs/definitions.md
docs/            how everything works
PLAN.md          what is done and what comes next
```

## Status

Every one of the 1,398 scripts on the PAL disc decompiles, with no function
left as assembly, and compiles back byte for byte (`python3 cli.py verify
extracted` checks this). All 1,081 engine calls are documented. Every
function on the disc has a name, named parameters and locals and a doc
comment; story flags print as named constants; and dialogue, item, map and
character ids carry their English text from your disc. See
[PLAN.md](PLAN.md) for what comes next.

## Prior art

- [ChronicleTwo](https://github.com/TheMoonPeople/ChronicleTwo): the game's
  script VM, every engine call and the disc format, as matching C++. This
  project's knowledge of the engine comes from it.
- [OpenCloud](https://github.com/Gundorada-Workshop/OpenCloud): opcode enums,
  header layout and a function and label analyzer (`code/script/`).
- [SB2-File-Parser](https://github.com/ArsonAssassin/SB2-File-Parser): header,
  program table and instructions to JSON.
- [DarkCloud-Two-Reforged](https://github.com/ZeldoKavira/DarkCloud-Two-Reforged):
  a linear disassembler with monster native names and a live script dump
  from PCSX2.
- [DarkCloud-Toolkit](https://github.com/Gundorada-Workshop/DarkCloud-Toolkit):
  a Dark Cloud 1 `.stb` disassembler and VM.

Dark Chronicle and Dark Cloud are trademarks of their owners. This project is
not affiliated with Level-5 or Sony.
