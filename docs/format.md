# The script format

Everything here comes from the ChronicleTwo decompilation, which contains the VM itself and so is the authority on the format, checked against all 1,398 scripts on the PAL disc. Paths are relative to a ChronicleTwo checkout.

## Disc layout

- The disc is ISO 9660. Almost every game file lives in `DATA.DAT`, indexed by
  `DATA.HD4` (`ps2/src/dataread.cpp`, `InitCDFile`).
- `DATA.HD4` is an array of 12-byte records `{name_offset, size, sector}`,
  little-endian. The record count is the first word divided by 12, and the
  last record is a terminator whose name is empty. Names are null-terminated,
  use `\` as the separator, and may contain Shift-JIS. The PAL disc has 8,752
  files.
- The engine reads `DATA.HD4` into a buffer of 0x50000 bytes
  (`dataread.cpp`, `header_buff`), so the index can never be larger. A new
  file costs its 12-byte record and its name plus a NUL; a build that would
  pass the limit is refused (`dc2tk_disc::archive::INDEX_LIMIT`), and new
  files belong in packs instead.
- A file's data starts at `sector * 2048` inside `DATA.DAT`.
- Movies (`\MOVIE\`), voice (`\VOICE\`, `\V\`) and IOP modules (`\MODULES\`)
  sit outside the archive and are not needed here.

## Pack files

A pack is a chain of entries (`ps2/include/dataread.hpp`, `PACK_ENTRY`):

```
char name[64]
int  offset   data, relative to this entry
int  size
int  next     next entry, relative to this entry
```

On the PAL disc every entry's data starts at 0x50 (the 0x4C-byte entry
rounded up to 16), `next` is 0x50 plus the size rounded up to 16, and the
chain ends with a 0x50-byte entry whose name is empty. `extract` unpacks a
file only when it matches all of that. A looser check unpacked 587 scripts by
mistake, because a script's header and program table can read as one pack
entry named `SB2`. Characters (`.chr`), map packs (`.mpk`), sound banks
(`.snd`) and others use this format.

## Script files (`.stb`)

From `ps2/include/runscript.hpp` and `ps2/src/runscript.cpp`, confirmed
against all 1,398 scripts on the PAL disc (42 of them inside packs):

```
0x00  header        0x40 bytes
0x40  program table prog_num * 8 bytes
code  function      0x38-byte record, instructions, FUNC_END, string block
      function      ...
```

- **Header** (`RS_PROG_HEADER`), 0x40 bytes: `magic[4]`, `main`, `code`,
  `prog`, `prog_num`, `unk_14`, `global_num`, then 0x24 bytes of padding.
  1,389 files use `"SB2"` and zero padding. Nine use `"STB"` with padding
  filled with `0xCC`, the MSVC debug fill, so the padding is kept as bytes.
  `unk_14` is always 0. `prog` is always 0x40 and `code` always follows the
  program table.
- **Program table** (`RS_PROGDATA[prog_num]`): `{no, func}`. `no` is the number
  the engine runs (100 init, 200 main, 1000 NPC talk, ...). `func` is a
  `funcdata` offset **from the header**.
- **Functions** follow each other from `code` to the end of the file. Each one
  is a 0x38-byte record, its instructions, a `FUNC_END` instruction (twelve
  zero bytes), and then a string block holding the strings that function
  pushes. 144,202 functions in all, including ones nothing calls.
- **Function record**, 0x38 bytes. The game reads the first 16 as `funcdata`
  `{addr, name, local, arg}`; the other 0x28 bytes are `arg_types[10]`.
  - `addr` is code-relative and always points just past the record, so a
    record is found by `word(offset) == offset - code + 0x38`.
  - `name` is 0 in every retail file, so no function names survive.
  - `local` counts frame slots including arguments.
  - `arg_types` has one entry per argument: 1 for a value (int or float), 0
    for an array passed by pointer (read through modes `0x4` and `0x20`).
    Entries past `arg` are 0. Three functions take 11 arguments, one more
    than the table holds (`get_jump_velocity` in 968 records, and two arc
    moves in `map/e01.stb`); nothing calls any of them. The engine never
    reads `arg_types`: `call_func` uses only `addr`, `local` and `arg`, and an
    argument's kind comes from its stack tag at run time, so the eleventh
    slot's type is just how the body addresses it.
- **Instructions** (`vmcode_t`, 12 bytes): `op`, `arg1`, `arg2` (int or float).
  Jump targets are byte offsets from the code section. `CALL`'s `arg2` is a
  `funcdata` offset **from the code section**, unlike the program table.
- **String blocks**: null-terminated Shift-JIS strings in push order, one copy
  per push (no deduplication), padded to a multiple of 16 bytes. The
  original compiler kept one string buffer per file, zero-filled at the
  start: each function wrote its strings into it, rounded the write offset
  up to 16 and wrote the buffer up to there after `FUNC_END`. Most files
  reset the offset for each function, so the padding is whatever earlier
  blocks left at those positions and a function with no strings has no
  block. 55 files (the nine `STB` files and 46 `SB2` ones: the d01 to d05
  dungeon scripts, `e236a` and effect scripts) never reset it, so every
  block holds the file's strings so far before its own, and a function with
  no strings writes all of them. After the last function the code section is
  zero-filled to a multiple of 16. That predicts every block and tail on the
  PAL disc.
- **Globals**: an `SB2` program carves `global_num` slots off the bottom of the
  value stack. Dark Cloud 1 scripts have no magic and no globals (version 1).

## Instruction set

| Op | Name | Operands |
|---|---|---|
| 0 | FUNC_END | ends every function; the VM never reaches it |
| 1 | LOAD | `arg1` slot, `arg2` addressing mode |
| 2 | LOAD_ADDR | same, pushes a pointer |
| 3 | PUSH_CONST | `arg1` type (1 int, 2 float, 3 string), `arg2` value |
| 4 | POP | |
| 5 | STORE | writes, then pushes the value back |
| 6-11 | ADD SUB MUL DIV MOD NEG | |
| 12, 13 | ITOF, FTOI | |
| 14 | CMP | `arg1` 40-45: `==` `!=` `<` `<=` `>` `>=` |
| 15 | RET | pops the return value |
| 16-18 | JMP, JMP_FALSE, JMP_TRUE | `arg1` target |
| 19 | CALL | `arg2` function |
| 20 | PRINT | `arg1` count |
| 21 | EXT | `arg1` count, including the native number pushed first |
| 22 | NOP | `arg1` a small number; no case in the VM's switch |
| 23 | WAIT | yield until next frame |
| 24-26 | AND OR NOT | |
| 27 | END | |
| 28 | SKIP_END | where a skipped cutscene stops |
| 29, 30 | SIN, COS | |

These numbers, the addressing modes and the comparisons below are defined
once in `src/crates/dc2tk-script/src/stb.rs` (`stb::Op`, `stb::Mode`,
`stb::Compare`) for every part of the tools.

`NOP` appears 472,159 times and is a label: every jump on the disc lands on
one, and its `arg1` is the label's number in the order the compiler allocated
it within the function (see [the decompiler](internals.md#decompiler)). Addressing modes (`RS_ADDR_MODE`): `0x1` local,
`0x2` local indexed, `0x4` indexed through a pointer in a local, `0x8`, `0x10`,
`0x20` the same three as float, `0x40` global, `0x200` global float.

## What the instruction set says about the source language

- `STORE` leaves the value on the stack and statements end in `POP`: assignment
  is an expression, as in C.
- `ITOF`/`FTOI` exist and slots are tagged float by the addressing mode: the
  language had declared `int` and `float` variables. Mixed arithmetic without
  any conversion is common and only 3,318 conversions appear on the disc, so
  c2s writes them as explicit casts, `(int)x` and `(float)x`.
- Named functions with arguments, locals, `CALL` and a value-returning `RET`.
- `LOAD_ADDR` and pointer-indexed modes: `&x`, arrays, and array parameters.
- `EXT` pushes no result. At the VM level every native is a statement and
  returns values through pointer arguments: `_GET_FLAG(12, &done);`. In
  source, a native with one result returns it (`done = get_flag(12);`; see
  [the language](language.md)).

## Native function tables

Each VM user binds its own table of natives. All of them are in the decomp,
with retail names, as `RS_EXTFUNC_INFO {func, no}` rows whose numbers come from
enums in the matching header:

| Table | Source | Rows | Used by |
|---|---|---|---|
| event | `ps2/src/event_func.cpp` `ext_func_info[697]` | 696 | map and dungeon event scripts |
| monster | `ps2/src/runscript_opcodes.cpp` `ext_func_info[]` | 173 | `dungeon/monster/*.stb` |
| action | `ps2/src/actscript.cpp` `ext_func_info[]` | 82 | `mainchr/c01.stb`, `c02.stb`, `dungeon/act_script/*.stb` |
| effect | `ps2/src/effscript.cpp` `ext_func_info[129]` | 128 | `dungeon/eff_script/*.stb` |
| arg | `ps2/src/event_func.cpp` `esa_ext_func_info[3]` | 2 | argument files loaded by `_LOAD_ARG` |

`dc2tk natives <chronicletwo>` reads each table and its enum and writes
`src/natives/<table>.json` (checked in) as `{no, name, accessors}` rows.
Every native's function name matches its enum name, and names are unique
within a table. `accessors` lists the stack calls in the native's body in
source order (`int`, `float`, `string`, `vector`, `any`, `out`,
`out_vector`). It is a hint for argument types, not a signature: bodies index
the stack in loops and branches. The decompiler does not need it, because the
bytecode already shows `&x` (`LOAD_ADDR`) and each constant's type.
`monster.json` and `action.json` also hold `old_groups`, `old_table_groups`
and `old_direct` (see [the SDK](sdk.md)), which were worked out from the
scripts rather than the decomp. The command cannot regenerate them, so it
keeps what the table file it replaces holds of them.

`EXT` pops `arg1` values, and the deepest of them is the native number. It is
found by walking back from the `EXT` and adding up stack effects (`src/crates/dc2tk-script/src/stack.rs`):
`CALL` pops the callee's `arg` and pushes one, and the indexed modes (`0x2`,
`0x4`, `0x10`, `0x20`) pop an index before pushing. Every one of the 1,407,551
calls on the disc ends at a `PUSH_CONST` int.

Which table a script uses is decided by its path (first match wins), checked
against the corpus. The rules are part of the disc's definition set
(`definitions/pal/definitions.json`, see [definitions](definitions.md)); in
source, the SDK header a script includes names its table:

| Path | Table |
|---|---|
| `event/tbox/bomb.pack.d/` | effect (`tbox_exp`, loaded by `effscript.cpp`) |
| `event/` | arg (`_LOAD_ARG` files: `*_arg*.stb`, `e200/`) |
| `dungeon/monster/` | monster |
| `dungeon/eff_script/`, `dungeon/articles/`, `dungeon/sphida/` | effect |
| `dungeon/act_script/`, `mainchr/` | action |
| everything else (`map/`, `dungeon/sys_script/`, ...) | event |

Monster scripts often compute the native number as a group base plus an
offset (`push 70; push 15; add` for `_GET_TARGET_ANGLE`), and 12,903 functions
do it. The base differs between call sites for the same native (`_SET_MOS` is
`180 + 2` 8,630 times and `170 + 12` four times), so c2s keeps it: the SDK declares each native with the base its calls
mostly use (`NATIVE(70, 15)`), and `AT(n)` marks a call with another one
(`AT(60) get_pos(&target_angle);`). The natives stage folds the addition before
looking the number up.

227 calls use 22 numbers that the matching table never registers, so at
runtime the VM prints `not found ext` and moves on. They are in the two old
`STB` action scripts and a few monster scripts, and decompile as
`_EXT_67(...)`, which also compiles.

