# ChronicleTwo-Script

Tools for reading, writing and modding the compiled scripts of Dark Chronicle
(Dark Cloud 2, PAL `SCES_511.90`). Python 3, standard library only.

Most of the game's behaviour lives in compiled bytecode rather than in the
executable: every cutscene, conversation, door, monster AI, the player's
moves and the scripted effects. They are `.stb` files run by the game's
script VM. Level-5's source language was never released, so this project
recovers one, **c2s**, a C dialect, and proves it the way the
[ChronicleTwo](https://github.com/TheMoonPeople/ChronicleTwo) decompilation
proves its C++: every retail script decompiles to c2s and compiles back to
the same bytes.

## Quick start

You need your own copy of the PAL disc as an ISO image.

```
python3 cli.py setup dc2.iso source
```

This extracts the disc to `extracted/` and decompiles every script into
`source/`, as readable C using the SDK and the shipped definitions: named
functions and variables, documented natives, named constants, and English
comments. Edit any scripts or shared library files, then build a new disc
image:

```
python3 cli.py build source dc2.iso -o modded.iso
```

`build` compiles every script in `source/`, finds the ones that differ from
the disc, and writes `modded.iso`: the original image with only those
scripts replaced. Boot it in PCSX2 like the original.

Neither `extracted/` nor `source/` is ever committed: this repository holds
tools and knowledge, never game code or text.

## Layout

```
cli.py           the command line (python3 cli.py --help)
src/c2s/         the tools: disc, format, assembler, decompiler, compiler
src/tests/       tests (python3 cli.py test)
sdk/             C headers scripts include: every native declared and
                 documented, engine constants, inline helpers
definitions/pal/ what we know about the PAL retail scripts: names, docs,
                 translations, disc layout; see docs/definitions.md
docs/            how everything works
PLAN.md          what comes next
```

## Documentation

- [Usage](docs/usage.md): every command, and the modding workflow.
- [The c2s language](docs/language.md) and [the SDK](docs/sdk.md).
- [Definitions](docs/definitions.md): the format for sharing knowledge about
  compiled scripts without sharing the scripts.
- [Examples](docs/examples/README.md): one script at every stage from
  bytecode to annotated source.
- [The script format](docs/format.md): disc, archive, `.stb` layout, VM
  instruction set, native tables.
- [Assembly](docs/assembly.md) and [how the tools work](docs/internals.md).

## Status

Every one of the 1,398 scripts on the PAL disc decompiles, with no function
left as assembly, and compiles back byte for byte; `python3 cli.py verify
extracted` checks this. All 1,081 natives are documented. With the shipped
definitions every function on the disc has a name, named parameters and
locals and a doc comment; every script and shared library has a doc
comment; story flags print as named constants; and dialogue, item, map and NPC
ids carry their English text from the disc. See [PLAN.md](PLAN.md).

## Prior art

- [OpenCloud](https://github.com/Gundorada-Workshop/OpenCloud): opcode enums,
  header layout and a function and label analyzer (`code/script/`).
- [SB2-File-Parser](https://github.com/ArsonAssassin/SB2-File-Parser): header,
  program table and instructions to JSON.
- [DarkCloud-Two-Reforged](https://github.com/ZeldoKavira/DarkCloud-Two-Reforged):
  a linear disassembler with monster native names and a live script dump
  from PCSX2.
- [DarkCloud-Toolkit](https://github.com/Gundorada-Workshop/DarkCloud-Toolkit):
  a Dark Cloud 1 `.stb` disassembler and VM.
- [ChronicleTwo](https://github.com/TheMoonPeople/ChronicleTwo): the VM, every
  native table and the disc format, as matching C++.
