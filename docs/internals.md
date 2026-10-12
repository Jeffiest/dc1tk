# How the tools work

## Libraries

The 144,202 functions on the disc have only 10,789 distinct bodies. The
original scripts pulled in shared source files, and each compiled script
carries its own copy of everything it included, so most of a script is
library code. Recovering those libraries is what makes the decompiled source
readable.

A function's identity is its fingerprint: a hash of its bytecode (without
its name or string padding) and the fingerprints of the functions it calls,
plus the native table (see
[how definitions find things](definitions.md#how-definitions-find-things)). No script calls a later function or
itself, so fingerprints are well defined and the same in every file.

`src/c2s/library.py` finds the libraries over the whole disc:

1. A function can join a library when it is the first copy of its body in
   the file and everything it calls can too.
2. Each script is cut greedily from the start into the longest runs of
   functions that appear, in order, in at least one other script.
3. Every distinct run used by two or more scripts becomes a library file,
   placed under `lib/` in the deepest folder all its users share
   (`lib/map/s/s05/s05_12.c`, given the folder's readable name,
   `s05_fish_monster_swamp`, and usually a better name from
   `libraries.json`). Runs only one script uses stay inline.

The libraries evolved while the game was made, so the same helper exists in
several versions and a library file is one exact version. The disc gives
1,097 library files. The median script includes three of them and keeps
about one function of its own; the scripts shrink from 3.9 million lines to
587,000.

The compiler expands each `#include` in place, so the bytes do not change.
Library files carry no padding pragmas; each script lists the padding of
its own and its included functions with `#pragma padding_of NAME HEX` at
the end of the file.

## Names

Retail scripts keep no names, so every name in the source is chosen.
`definitions/pal/names/<table>.json` maps fingerprints to a function name and,
optionally, parameter and local names:

```json
{"functions": {"3f9a...": {"name": "wait_frames", "params": ["frames"]}}}
```

Because the key is the fingerprint, one entry names that function in every
script and library that has it. Two versions of a helper can share a name;
when two such versions would meet in one file, the later one gets a numbered
suffix (`do_nothing_2`). Variable names that would clash with each other or with a
global are ignored.

Every function on the disc now has a name. Names are checked by `verify`
like everything else, since a wrong name can only make the source less
clear, never change the bytes.

Functions without an entry get defaults: library functions are named after
the library that first holds them and their position in it (`map_2_40`),
entry points `on_` plus their program's name (`on_map_start`), or
`program_100` when the program has no name, the header's main
function `start`, and the rest `func_1`, `func_2` in file order. Variables
default to their slot: `a0` for arguments, `v3` for locals, `g1` for
globals.

## Decompiler

`src/c2s/decompile.py` turns a parsed script into a plain dict tree;
`src/c2s/sdk.py` presents it with the SDK's names and helpers, and
`src/c2s/source.py` prints it as C.

1. Declarations come from the addressing modes: each slot is used with one
   mode, so it is an `int` or `float` scalar, a local array (indexed modes, the
   size running to the next used slot) or an array parameter (pointer modes).
   The declared `arg_types` must agree.
2. Expressions are rebuilt by simulating the stack until a statement-ending
   instruction (`POP`, `RET`, `EXT`, `PRINT`, `WAIT`, `END`, `SKIP_END` or a
   jump).
3. Control flow follows the compiler's fixed shapes. Every jump target is a
   `NOP` whose `arg1` is a label number, allocated per function in the order
   the compiler met them: `while` takes its top and exit labels before its
   condition, `if` takes its else label after the condition and its end label
   after the then-block, and `&&`/`||` (`jf_push`/`jt_push`) take theirs after
   the left operand. The decompiler checks the numbers come out in that order,
   so the compiler can reproduce them by counting.
4. A function must end in `push 0; ret`, the compiler's implicit return.
   Anything outside these shapes makes the function an `asm` block.

## Compiler

`src/c2s/parse.py` reads `.c` into the same tree and `src/c2s/compile.py` turns
the tree into text assembly, which `asm.assemble` lays out. Slots, label
numbers, native numbers and string indices are assigned by the rules above;
the assembler computes every offset, record field and header field.

String blocks hold the function's strings in push order, padded to 16 bytes
with whatever the compiler's string buffer held there from earlier blocks
(`stb.block_padding`). That rule gives 41,343 of the 41,637 string blocks;
the other 294, and 1,091 functions with padding but no strings, carry
`#pragma padding_of NAME HEX`. For 403 of them the bytes are the start of
the leftover buffer, zero-filled past its end, but nothing on the disc
says how long the block is, so the bytes are kept as they are. 198 functions in 15 files (mostly
`dungeon/sys_script/d0*.stb`) start their block with strings no instruction
pushes; they carry `#pragma strings "..."`.

# Code style

- Python 3, standard library only.
- Functions and plain data: dicts, lists, tuples. No classes, no wrappers, no
  higher-order functions.
- No comments. If code needs one, it needs to be simpler.
- Intermediate forms (parsed file, instruction lists, syntax trees) are plain
  dicts that can be dumped as JSON, which makes every stage inspectable.
