# The script format

This page documents how the game stores and executes scripts: the `.stb` binary format, the virtual machine (VM) instruction set, and the native function tables that the engine binds to each script category. All specifications come from the [ChronicleTwo](https://github.com/TheMoonPeople/ChronicleTwo) engine decompilation, which contains the VM implementation, and are verified against all 1,398 scripts on the PAL disc. Source paths are relative to a ChronicleTwo checkout. For archive locations, see [the disc](disc.md). For runtime execution and event mode, see [running](running.md). For the decompiled C syntax and SDK, see [Scripting](../scripting/README.md).

## Script files (`.stb`)

The `.stb` layout is defined in `ps2/include/runscript.hpp` and `ps2/src/runscript.cpp`, and verified against all 1,398 scripts on the PAL disc (including 42 scripts stored inside pack files):

```
0x00  header        0x40 bytes
0x40  program table prog_num * 8 bytes
code  function      0x38-byte record, instructions, FUNC_END, string block
      function      ...
```

- **Header** (`RS_PROG_HEADER`, `0x40` bytes): contains `magic[4]`, `main`, `code`, `prog`, `prog_num`, `unk_14`, and `global_num`, followed by `0x24` bytes of padding.
  - On the PAL disc, 1,389 files use `"SB2"` as the magic string and fill the padding with zeros.
  - Nine files use `"STB"` as the magic string and fill the padding with `0xCC` (the MSVC debug stack fill byte), so tools must preserve the padding bytes for byte-exact round-tripping.
  - `unk_14` is always `0`.
  - `prog` is always `0x40`, and `code` always points immediately after the program table.
- **Program table** (`RS_PROGDATA[prog_num]`): an array of 8-byte `{no, func}` entries.
  - `no` is the program number that the engine starts (for example, `100` for init, `200` for main, or `1000` for NPC talk).
  - `func` is the byte offset of the target `funcdata` record **relative to the start of the header**.
- **Functions**: stored consecutively from offset `code` to the end of the file. Each function consists of a `0x38`-byte function record, its bytecode instructions, a `FUNC_END` instruction (12 zero bytes), and a string block holding the string literals that the function pushes. The PAL disc contains 144,202 functions in total, including unreferenced functions that no script calls.
- **Function record** (`0x38` bytes): the engine reads the first 16 bytes as `funcdata` `{addr, name, local, arg}`, and the remaining `0x28` bytes are `arg_types[10]`.
  - `addr` is a byte offset relative to `code`. It always points to the first instruction immediately past the `0x38`-byte record, so a parser can identify a function record at file offset `offset` by checking `word(offset) == offset - code + 0x38`.
  - `name` is `0` in every retail file, so no function names survive in the binary files.
  - `local` is the total number of stack frame slots, including arguments.
  - `arg_types` has one 32-bit entry per argument: `1` for a value (`int` or `float`), or `0` for an array passed by pointer (accessed through addressing modes `0x4` and `0x20`). Entries past `arg` are `0`.
  - Three unreferenced functions on the disc take 11 arguments, which is one more than `arg_types[10]` can hold (`get_jump_velocity` in 968 records, and two arc-movement functions in `map/e01.stb`). Nothing calls any of them.
  - The engine never reads `arg_types`. At runtime, `call_func` uses only `addr`, `local`, and `arg`, and determines each argument's type from its runtime stack tag. Therefore, a decompiler determines the 11th argument's type from how the function body accesses that slot.
- **Instructions** (`vmcode_t`, 12 bytes): `{op, arg1, arg2}`, where each operand is a 32-bit `int` or `float`.
  - Jump targets are byte offsets relative to the `code` section.
  - In `CALL` instructions, `arg2` is a `funcdata` offset **relative to the `code` section**, unlike the program table (which uses header-relative offsets).
- **String blocks**: null-terminated Shift-JIS strings stored in push order, with one copy per `PUSH_CONST` instruction (no deduplication), padded to a multiple of 16 bytes.
  - The original compiler maintained a single zero-filled string buffer per file. As it compiled each function, it wrote the function's strings into the buffer, rounded the write offset up to a multiple of 16 bytes, and wrote the buffer from start to that aligned offset immediately after `FUNC_END`.
  - In most files (1,343 of 1,398), the compiler reset the buffer write offset to `0` for each function. As a result, any padding bytes after a function's strings contain leftover bytes from earlier, longer string blocks in the same file, and a function with no strings emits no string block.
  - In 55 files (the 9 `"STB"` files and 46 `"SB2"` files: the `d01` to `d05` dungeon scripts, `e236a`, and effect scripts), the compiler never reset the write offset between functions. In those 55 files, every string block contains all strings emitted earlier in the file before the function's own strings, and a function with no strings still writes out all strings accumulated so far.
  - After the last function, the compiler zero-pads the `code` section to a multiple of 16 bytes. These rules account for every string block and file tail on the PAL disc.
- **Globals**: an `"SB2"` script reserves `global_num` slots at the bottom of the VM value stack. Dark Cloud 1 scripts (version 1) have no magic header and no globals.

## Instruction set

| Op | Name | Operands |
|---|---|---|
| 0 | FUNC_END | Marks the end of a function; the VM never executes this opcode |
| 1 | LOAD | `arg1` slot, `arg2` addressing mode |
| 2 | LOAD_ADDR | `arg1` slot, `arg2` addressing mode; pushes a pointer |
| 3 | PUSH_CONST | `arg1` type (`1` int, `2` float, `3` string), `arg2` value |
| 4 | POP | Discards the top stack value |
| 5 | STORE | `arg1` slot, `arg2` addressing mode; writes the value, then pushes it back |
| 6-11 | ADD SUB MUL DIV MOD NEG | Arithmetic operations |
| 12, 13 | ITOF, FTOI | Integer-to-float and float-to-integer conversions |
| 14 | CMP | `arg1` 40-45: `==`, `!=`, `<`, `<=`, `>`, `>=` |
| 15 | RET | Pops and returns the return value |
| 16-18 | JMP, JMP_FALSE, JMP_TRUE | `arg1` target offset (relative to `code`) |
| 19 | CALL | `arg2` target function offset (relative to `code`) |
| 20 | PRINT | `arg1` argument count |
| 21 | EXT | `arg1` total stack values popped, including the native number pushed first |
| 22 | NOP | `arg1` label index; has no case in the VM switch statement |
| 23 | WAIT | Yields execution until the next frame |
| 24-26 | AND OR NOT | Logical operations |
| 27 | END | Terminates the current program |
| 28 | SKIP_END | Marks the point where cutscene skipping stops |
| 29, 30 | SIN, COS | Trigonometric operations |

`NOP` appears 472,159 times on the PAL disc and acts as a jump label. Every jump instruction on the disc targets a `NOP`, and `arg1` stores the sequential label number allocated by the compiler within that function.

The instruction set uses eight addressing modes (`RS_ADDR_MODE`):

- `0x1`: local `int`
- `0x2`: local `int` indexed (array)
- `0x4`: `int` indexed through a pointer stored in a local slot
- `0x8`: local `float`
- `0x10`: local `float` indexed (array)
- `0x20`: `float` indexed through a pointer stored in a local slot
- `0x40`: global `int`
- `0x200`: global `float`

## What the instruction set says about the source language

- `STORE` leaves the assigned value on the stack, and expression statements end with `POP`. This indicates that assignment is an expression, as in C.
- `ITOF` and `FTOI` opcodes exist, and addressing modes distinguish `int` and `float` slots. This indicates that the source language had declared `int` and `float` variables. Mixed int/float arithmetic without conversion opcodes is common, and only 3,318 `ITOF`/`FTOI` instructions appear on the disc, so the decompiled C writes them as explicit casts: `(int)x` and `(float)x`.
- The language supports named functions with parameters, local variables, `CALL` instructions, and value-returning `RET` instructions.
- `LOAD_ADDR` and pointer-indexed addressing modes (`0x4` and `0x20`) correspond to address-of expressions (`&x`), arrays, and pointer/array parameters.
- `EXT` pushes no return value onto the stack. At the VM level, every native call is a void statement that returns values through pointer arguments (for example, `_GET_FLAG(12, &done);`). In source code, the SDK wraps single-output natives so they return their result directly (`done = get_flag(12);`; see [the language](../scripting/language.md)).

## Native function tables

Each VM subsystem binds its own table of native functions. The ChronicleTwo decompilation defines all five tables with retail names as `RS_EXTFUNC_INFO {func, no}` arrays, where `no` comes from an enum in the corresponding header:

| Table | Source | Rows | Used by |
|---|---|---|---|
| event | `ps2/src/event_func.cpp` `ext_func_info[697]` | 696 | map and dungeon event scripts |
| monster | `ps2/src/runscript_opcodes.cpp` `ext_func_info[]` | 173 | `dungeon/monster/*.stb` |
| action | `ps2/src/actscript.cpp` `ext_func_info[]` | 82 | `mainchr/c01.stb`, `c02.stb`, `dungeon/act_script/*.stb` |
| effect | `ps2/src/effscript.cpp` `ext_func_info[129]` | 128 | `dungeon/eff_script/*.stb` |
| arg | `ps2/src/event_func.cpp` `esa_ext_func_info[3]` | 2 | argument files loaded by `_LOAD_ARG` |

Every native function's C name matches its enum name, and names are unique within each table. A native function reads its arguments from the VM stack inside its C implementation. Those stack reads indicate argument types, though they do not form a complete signature when the C function indexes the stack inside loops or branches. The script bytecode provides the remaining type information through `LOAD_ADDR` (`&x`) and `PUSH_CONST` type tags. The SDK headers declare and document every native function ([the SDK](../scripting/language.md), [the API](../scripting/api/README.md)).

`EXT` pops `arg1` values from the stack, and the deepest popped value (pushed first) is the native function number. A parser finds this number by walking backward from `EXT` and summing the stack effects of each instruction: `CALL` pops the callee's `arg` count and pushes one return value, and the indexed addressing modes (`0x2`, `0x4`, `0x10`, `0x20`) pop an index before pushing the result. Every one of the 1,407,551 `EXT` calls on the PAL disc traces back to a `PUSH_CONST` integer (or an `ADD` of two `PUSH_CONST` integers).

A script's file path determines which native table it uses (first match wins), verified across the entire disc corpus. (The disc definition file `definitions/pal/definitions.json` stores these path rules; see [definitions](../../definitions/README.md). In C source code, the SDK header that a script includes selects its table.)

| Path | Table |
|---|---|
| `event/tbox/bomb.pack.d/` | effect (`tbox_exp`, loaded by `effscript.cpp`) |
| `event/` | arg (`_LOAD_ARG` files: `*_arg*.stb`, `e200/`) |
| `dungeon/monster/` | monster |
| `dungeon/eff_script/`, `dungeon/articles/`, `dungeon/sphida/` | effect |
| `dungeon/act_script/`, `mainchr/` | action |
| everything else (`map/`, `dungeon/sys_script/`, ...) | event |

Monster scripts usually compute the native number as a group base plus an offset (for example, `push 70; push 15; add` for `_GET_TARGET_ANGLE`), and 12,903 functions on the disc use this pattern. Because different call sites sometimes use different group bases for the same native (`_SET_MOS` uses `180 + 2` 8,630 times and `170 + 12` four times), the decompiled C preserves the base: the SDK declares each native with its most common group base (`NATIVE(70, 15)`), and `AT(n)` marks any call site that uses a different base (`AT(60) get_pos(&target_angle);`).

Across the disc, 227 calls use 22 native numbers that the corresponding table never registers. At runtime, the VM prints `not found ext` and continues. These calls occur in the two old `"STB"` action scripts and a few monster scripts; they decompile as `_EXT_67(...)`, which also recompiles cleanly.

## Older native numberings

Monster scripts on the disc mix three native numbering schemes.

Scripts compiled against an older monster native table call certain natives with group base `60` where the current table uses group base `70`. At runtime, the engine executes whatever native function matches the computed number in the current table. The decompiled C shows the function that the engine actually executes, with a comment naming the native function that the call was originally written for:

```c
AT(60) get_pos(&target_angle);  // old numbering, meant as get_target_angle
```

Shifting group `60` to group `70` (adding `10` to the native number) matches the argument types of all 197 such calls on the disc, and identical functions in newer monster scripts confirm this rule. The `old_groups` map in `src/natives/monster.json` records this mapping.

The scripts that contain those calls (`b07a_atora_yellow`, `e08a`, `e09a`, and the helper libraries used only by them) also call groups `110`, `140`, and `160` where the current table uses `120`, `150`, and `180`. Identical functions elsewhere on the disc confirm every such call. Because `110`, `140`, and `160` are also valid groups in the current table and are used normally by other scripts, the definition set explicitly marks the older scripts with `"numbering": "old"` in their `scripts/` entry. The decompiler emits `old_table_groups` comments (from `monster.json`) only in scripts marked `"old"` and in helper libraries used exclusively by them.

Seven monster scripts (`b02a2` through `b02a7`, which are byte-identical, and `e236a`) push raw native numbers with no group base addition, using an even older and smaller native table. These calls decompile with the `DIRECT` keyword, which no other monster script requires. No single constant offset maps that table to the current one, but comparing these scripts against newer counterparts (`e228a` for `e236a`, and `b02a1`, `b02a`, and `b04a` for the `b02a` bosses) identifies 41 raw numbers (for example, raw numbers `52` to `61` correspond to current numbers `77` to `86`). The `old_direct` table in `monster.json` records these 41 mappings.

Calling a native by a single integer without `ADD` does not by itself indicate the old table, because current scripts also call the first native of a group (offset `0`) by its bare group number (`b02a1` and `p00_23a` do this while using the current table). Therefore, the definition set marks `b02a2` to `b02a7` and `e236a` with `"numbering": "raw"`. In those scripts and the libraries used only by them, any `DIRECT` call to one of the 41 mapped numbers, or any call that lands on a native without a group in the SDK header, receives an explanatory comment:

```c
DIRECT get_monster_var2(&turn_target);  // old numbering, meant as get_target_angle
set_dungeon_flag(turn_target, speed);  // old numbering, meant as turn_to
```

Two action scripts, `c01win` (an old build of Max's script) and `f204a`, were also compiled against an older action table. Comparing their calls with their current counterparts (`c01`, the action libraries, and the other `f` scripts) identifies 27 native numbers, which are recorded as `old_direct` in `action.json`. The definition set marks both scripts as `"raw"`, so their calls—including unregistered `_EXT_` calls—include a comment naming the intended native function (`_EXT_80(...)  // old numbering, meant as add_motion_se`).

