# DC2-Toolkit

**DC2-Toolkit** (`dc2tk`) extracts every asset of **Dark Chronicle** (Dark
Cloud 2), PAL version, into a form you can edit, and recompiles and rebuilds
them, and mods made from them, into a disc image you can play in PCSX2 (or a
future port):

- **scripts** as readable C, with named functions, documented engine calls
  and English comments, compiled back into the game's bytecode;
- **maps** as glTF scenes for Blender, with an add-on to edit placements,
  trigger points, lights and collision;
- **character models** and their **motions** as glTF;
- **textures** as PNG;
- **sounds** as WAV;
- **music** as MIDI, with each bank's instruments as a SoundFont;
- **text**: menus, items, shops and messages in every language, as text files;
- **game data tables** (items, shops, monsters, dungeons and more) as JSON.

Unedited assets rebuild to exactly the bytes on the disc, so a disc built
from your edits differs from the original only where you changed it.

Most of the game's behaviour lives in compiled scripts rather than in the
executable: every cutscene, conversation, door, monster AI, the player's
moves and the scripted effects. Every one of them decompiles, and every
unedited script compiles back to exactly the bytes on the disc.

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

The tool is one program, `dc2tk`. Build it:

```
cargo build --release
```

in this folder. This writes `build/release/dc2tk` (`dc2tk.exe` on Windows). Run it
from this folder with `dc2tk.bat` (Command Prompt and PowerShell: `dc2tk ...` in
Command Prompt, `.\dc2tk ...` in PowerShell) or `./dc2tk` in a Unix shell such as
Git Bash; each builds the program first if it is missing. The commands below
are written as `dc2tk ...`. To run it from anywhere, put this folder (or
`build/release`) on your PATH instead. The program finds `sdk/`,
`definitions/` and `src/natives/` in the repository folder above it (or in
the folder `$DC2TK_ROOT` names). Run the commands from this folder (it is where
`extracted/` and `mods/` go). `dc2tk help` lists the commands and
`dc2tk help COMMAND` gives one command's options.

### 2. Decompile the game

```
dc2tk setup dc2.iso
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
dc2tk mod override extracted/scripts my-first-mod --map s51 start_castle_battle
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
mod later with `dc2tk mod save extracted/scripts my-first-mod`.)

To find what to change:

```
dc2tk maps extracted/scripts                       every map, and which ones you can play in
dc2tk map extracted/scripts s51                    one map: its programs, triggers, player positions
dc2tk xref extracted/scripts monster golem         a monster's AI and who places it
dc2tk xref extracted/scripts event 510 --map s51   what starts an event
dc2tk flags extracted/scripts                      save flags in use
```

[A first test](docs/usage.md#a-first-test) has two small edits to try, and
the [modding guide](docs/modding.md) explains how the game runs its scripts.

### 4. Rebuild the disc

```
dc2tk build extracted/scripts dc2.iso -o modded.iso --mod my-first-mod
```

`build` compiles the scripts your mod changes and writes `modded.iso`: your
disc with only those scripts replaced (`extracted/disc` in place of `dc2.iso`
works too). It takes seconds. Add `--mod` again for
more mods. To test a change without playing up to it, add
`--start-at MAP:PROGRAM` (for example `--start-at s51:story_50`), and New
Game starts at that scene, with a new game's party; use such images for
testing only. The first build after updating these tools compiles every
script, which takes under a minute; it says so.

### 5. Play

Boot `modded.iso` in PCSX2 like the original, or let the tools start it:

```
dc2tk play modded.iso
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
- [Example mod](docs/examples/calculator/calculator.c): a calculator with its
  own model, screen and sounds (`dc2tk build extracted/scripts dc2.iso -o
  calculator.iso --mod calculator --mods docs/examples`), and
  [a character of its own](docs/examples/new-character/README.md), rigged
  and animated in Blender, and
  [a new map with a door into it](docs/examples/new-map-door/README.md).
- [The script format](docs/format.md), [assembly](docs/assembly.md) and
  [how the tools work](docs/internals.md).
- [Working on dc2tk](docs/development.md): the one rule a change must keep
  (built images never differ from retail where nothing was edited), the
  checks to run and the code's conventions.

## What is in this repository

```
Cargo.toml       the Cargo workspace (cargo build --release, cargo test --release)
src/crates/      the tool, dc2tk: disc, format, assembler, decompiler, compiler,
                 mods, assets; see src/README.md
src/natives/     the engine's native function tables, generated by dc2tk natives
blender-plugin/  dc2tk_maps, the Blender add-on for map scenes; see
                 blender-plugin/README.md
dc2tk, dc2tk.bat run dc2tk from this folder, building it first when missing
sdk/             C headers scripts include: every engine call declared and
                 documented, constants, helpers; sdk/modkit/ for mods
definitions/pal/ what is known about the PAL scripts: names, docs,
                 translations; see docs/definitions.md
docs/            how everything works; docs/development.md for working on
                 the code
tools/           golden.sh, the snapshot that checks a change against the
                 disc byte for byte
CLAUDE.md        the rules of docs/development.md, in brief, for Claude Code
PLAN.md          what is done and what comes next
```

## Status

Every one of the 1,398 scripts on the PAL disc decompiles, with no function
left as assembly, and compiles back byte for byte
(`dc2tk verify extracted/disc` checks this). All 1,081 engine calls are
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
