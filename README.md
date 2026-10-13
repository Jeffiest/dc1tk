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
> results (`extracted/`, `mods/`, built images) stay out of git.

## How to use

### 1. What you need

- A Rust toolchain ([rustup](https://rustup.rs)), to build the tool once.
- Your own Dark Chronicle disc image, saved here as `dc2.iso`. It must be the
  PAL release (`SCES_511.90`); the NTSC and Japanese discs are not supported
  yet.
- [PCSX2](https://pcsx2.net) to play the result, with a BIOS dumped from your
  own PS2.
- Optional: an editor that uses clangd (VS Code with the clangd extension,
  Neovim, Emacs, Zed), which then completes and documents every engine call.

The tool is one program, `c2`. Build it:

```
cargo build --release
```

in this folder. This writes `build/release/c2` (`c2.exe` on Windows). Run it
from this folder with `c2.bat` (Command Prompt and PowerShell: `c2 ...` in
Command Prompt, `.\c2 ...` in PowerShell) or `./c2` in a Unix shell such as
Git Bash; each builds the program first if it is missing. The commands below
are written as `c2 ...`. To run it from anywhere, put this folder (or
`build/release`) on your PATH instead. The program finds `sdk/`,
`definitions/` and `src/natives/` in the repository folder above it (or in
the folder `$C2_ROOT` names). Run the commands from this folder (it is where
`extracted/` and `mods/` go). `c2 help` lists the commands and
`c2 help COMMAND` gives one command's options.

### 2. Decompile the game

```
c2 setup dc2.iso
```

This takes a minute or two. Everything taken from the disc goes under
`extracted/`: the disc's files in `extracted/disc/`, every script as C in
`extracted/scripts/`, and the textures, models and sounds in
`extracted/assets/`. The scripts are one file per script, with code that
several scripts share in `lib/`. Folders and files keep the game's own
codes, with a readable name added: `map/s/s51_raybrandt_castle/` is
Raybrandt Castle, `dungeon/monster/e33a_golem.c` is the Golem's AI. Open
`extracted/scripts/` in your editor.

### 3. Make a change

Keep your changes in a **mod**: a folder in `mods/` that holds only what you
change. Pick the functions you want to change and copy them into the mod:

```
c2 mod override extracted/scripts my-first-mod --map s51 start_castle_battle
```

and edit them in `mods/my-first-mod/s51.c`, a **patch**. When the mod is
built, each function in it replaces the script's own in every language of
map s51 (`original()` inside it runs the one it replaced), and anything else
it defines is added. The decompiled tree stays as it was, and several mods
can patch the same script, so mods can be kept apart and built in any
combination. A mod that needs a save flag or a program number asks for one
by name (`NEW_FLAG(MY_SCENE_SEEN);`) and the build gives it a free one, so
mods never collide. See [mods](docs/usage.md#mods) for the rest: targeting
a monster's AI, running code every frame, text in each language.
(You can also edit `extracted/scripts/` directly and save the edits into a
mod later with `c2 mod save extracted/scripts my-first-mod`.)

To find what to change:

```
c2 maps extracted/scripts                       every map, and which ones you can play in
c2 map extracted/scripts s51                    one map: its programs, triggers, player positions
c2 xref extracted/scripts monster golem         a monster's AI and who places it
c2 xref extracted/scripts event 510 --map s51   what starts an event
c2 flags extracted/scripts                      save flags in use
```

[A first test](docs/usage.md#a-first-test) has two small edits to try, and
the [modding guide](docs/modding.md) explains how the game runs its scripts.

### 4. Rebuild the disc

```
c2 build extracted/scripts dc2.iso -o modded.iso --mod my-first-mod
```

`build` compiles the scripts your mod changes and writes `modded.iso`: your
disc with only those scripts replaced. It takes seconds. Add `--mod` again for
more mods. To test a change without playing up to it, add
`--start-at MAP:PROGRAM` (for example `--start-at s51:story_50`), and New
Game starts at that scene, with a new game's party; use such images for
testing only. The first build after updating these tools compiles every
script, which takes under a minute; it says so.

### 5. Play

Boot `modded.iso` in PCSX2 like the original, or let the tools start it:

```
c2 play modded.iso
```

(or add `--play` to `build`).

## Documentation

- [Codebase guide](docs/codebase-guide.html): an onboarding tour of the
  game's scripts (towns, dungeons, cutscenes, monsters, the player, effects)
  with walkthroughs of common tasks (open it in a browser).
- [Usage](docs/usage.md): every command, mods, and building.
- [Modding guide](docs/modding.md): how the game runs scripts, as far as a
  mod needs to know; message text codes; patterns; editor setup.
- [The language](docs/language.md): what the decompiled C means, and the C
  you can write in your own code.
- [The SDK](docs/sdk.md): the headers every script includes, and `modkit`,
  the shared library for mods.
- [Definitions](docs/definitions.md): how names and documentation are shared
  without sharing the scripts.
- [Examples](docs/examples/README.md): made-up scripts showing how to write
  a scene, a cutscene, a monster AI and a town talk handler.
- [The script format](docs/format.md), [assembly](docs/assembly.md) and
  [how the tools work](docs/internals.md).

## What is in this repository

```
Cargo.toml       the Cargo workspace (cargo build --release, cargo test --release)
src/crates/      the tool, c2: disc, format, assembler, decompiler, compiler,
                 mods, assets; see src/README.md
src/natives/     the engine's native function tables, generated by c2 natives
sdk/             C headers scripts include: every engine call declared and
                 documented, constants, helpers; sdk/modkit/ for mods
definitions/pal/ what is known about the PAL scripts: names, docs,
                 translations; see docs/definitions.md
docs/            how everything works
PLAN.md          what is done and what comes next
```

## Status

Every one of the 1,398 scripts on the PAL disc decompiles, with no function
left as assembly, and compiles back byte for byte
(`c2 verify extracted/disc` checks this). All 1,081 engine calls are
documented. Every function on the disc has a name, named parameters and
locals and a doc comment; story flags print as named constants; and dialogue, item, map and
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
