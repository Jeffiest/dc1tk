# The script format

This page documents how the game stores and executes scripts: the `.stb` binary format, the virtual machine (VM) instruction set, and the native function tables that the engine binds to each script category. All specifications come from the [ChronicleTwo](https://github.com/TheMoonPeople/ChronicleTwo) engine decompilation and are verified against all 1,398 scripts on the PAL disc. Source paths are relative to a ChronicleTwo checkout.

- For archive locations, see [the disc](disc.md).
- For runtime execution and event mode, see [running](running.md).
- For the decompiled C syntax and SDK, see [Scripting](../scripting/README.md).

## Script files (`.stb`)

The `.stb` layout is defined in `ps2/include/runscript.hpp` and `ps2/src/runscript.cpp`. It is verified against all 1,398 scripts on the PAL disc (including 42 scripts stored inside pack files):

| File Offset | Section | Size | Description |
|---|---|---|---|
| `0x00` | Header (`RS_PROG_HEADER`) | `0x40` bytes | File header and section offsets |
| `0x40` | Program table (`RS_PROGDATA[prog_num]`) | `prog_num * 8` bytes | Maps program numbers to function offsets |
| `code` | Functions | To end of file | Consecutive functions (`0x38`-byte record, instructions, `FUNC_END`, string block) |

### Header (`RS_PROG_HEADER`, `0x40` bytes)

| Offset | Size | Type | Field | Description |
|---|---|---|---|---|
| `0x00` | 4 | `char[4]` | `magic` | `"SB2"` (1,389 files) or `"STB"` (9 files). Dark Cloud 1 scripts (version 1) have no magic header and no globals. |
| `0x04` | 4 | `int` | `main` | Main entry offset. |
| `0x08` | 4 | `int` | `code` | Byte offset to the first function. Always points immediately after the program table. |
| `0x0C` | 4 | `int` | `prog` | Byte offset to the program table. Always `0x40`. |
| `0x10` | 4 | `int` | `prog_num` | Number of 8-byte entries in the program table. |
| `0x14` | 4 | `int` | `unk_14` | Always `0`. |
| `0x18` | 4 | `int` | `global_num` | Number of global variable slots reserved at the bottom of the VM value stack (`"SB2"` scripts). |
| `0x1C` | `0x24` | `u8[36]` | `padding` | `0x00` in the 1,389 `"SB2"` files; `0xCC` (the MSVC debug stack fill byte) in the 9 `"STB"` files. Tools must preserve these bytes for byte-exact round-tripping. |

### Program table (`RS_PROGDATA[prog_num]`, 8 bytes per entry)

| Offset | Size | Type | Field | Description |
|---|---|---|---|---|
| `0x00` | 4 | `int` | `no` | Program number that the engine starts (for example, `100` for init, `200` for main, or `1000` for NPC talk). |
| `0x04` | 4 | `int` | `func` | Byte offset of the target `funcdata` record **relative to the start of the header**. |

### Functions and function records (`0x38` bytes)

Functions are stored consecutively from offset `code` to the end of the file. The PAL disc contains 144,202 functions in total, including unreferenced functions that no script calls. Each function consists of:

1. A `0x38`-byte function record (`funcdata` plus `arg_types[10]`).
2. Bytecode instructions (`vmcode_t`).
3. A `FUNC_END` instruction (12 zero bytes).
4. A string block holding the string literals that the function pushes.

| Offset | Size | Type | Field | Description |
|---|---|---|---|---|
| `0x00` | 4 | `int` | `addr` | Byte offset of the first instruction **relative to `code`**. Always equals `offset - code + 0x38`. |
| `0x04` | 4 | `int` | `name` | Always `0` in retail files (no function names survive in the binary files). |
| `0x08` | 4 | `int` | `local` | Total number of stack frame slots, including arguments. |
| `0x0C` | 4 | `int` | `arg` | Number of function arguments. |
| `0x10` | `0x28` | `int[10]` | `arg_types` | Argument type tags: `1` for a value (`int` or `float`), or `0` for an array passed by pointer (addressing modes `0x4` and `0x20`). Entries past `arg` are `0`. |

- **Identifying function records**: Because `addr` always points immediately past the `0x38`-byte record, a parser can identify a function record at file offset `offset` by checking `word(offset) == offset - code + 0x38`.
- **11-argument functions**: Three unreferenced functions on the disc take 11 arguments, which is one more than `arg_types[10]` can hold (`get_jump_velocity` in 968 records, and two arc-movement functions in `map/e01.stb`). Nothing calls any of them.
- **Runtime argument handling**: The engine never reads `arg_types`. At runtime, `call_func` uses only `addr`, `local`, and `arg`, and determines each argument's type from its runtime stack tag. A decompiler determines the 11th argument's type from how the function body accesses that slot.

### Instructions (`vmcode_t`, 12 bytes)

| Offset | Size | Type | Field | Description |
|---|---|---|---|---|
| `0x00` | 4 | `int` | `op` | Opcode (`0`–`30`). |
| `0x04` | 4 | `int` / `float` | `arg1` | First operand. In jump instructions, `arg1` is a byte offset **relative to `code`**. |
| `0x08` | 4 | `int` / `float` | `arg2` | Second operand. In `CALL` instructions, `arg2` is a `funcdata` offset **relative to `code`** (unlike the program table, which uses header-relative offsets). |

### String blocks

String blocks store null-terminated Shift-JIS strings in `PUSH_CONST` order, with one copy per `PUSH_CONST` instruction (no deduplication), padded to a multiple of 16 bytes.

- **Compiler string buffer**: The original compiler maintained a single zero-filled string buffer per file. For each function, it wrote the function's strings into the buffer, rounded the write offset up to a multiple of 16 bytes, and wrote the buffer from start to that aligned offset immediately after `FUNC_END`.
- **Reset mode (1,343 of 1,398 files)**: The compiler reset the buffer write offset to `0` for each function. Any padding bytes after a function's strings contain leftover bytes from earlier, longer string blocks in the same file. A function with no strings emits no string block.
- **Cumulative mode (55 files)**: In the 9 `"STB"` files and 46 `"SB2"` files (the `d01` to `d05` dungeon scripts, `e236a`, and effect scripts), the compiler never reset the write offset between functions. Every string block contains all strings emitted earlier in the file before the function's own strings, and a function with no strings still writes out all strings accumulated so far.
- **File tail**: After the last function, the compiler zero-pads the `code` section to a multiple of 16 bytes. These rules account for every string block and file tail on the PAL disc.

## Instruction set

| Op | Name | Operands and Behavior |
|---|---|---|
| 0 | `FUNC_END` | Marks the end of a function; the VM never executes this opcode |
| 1 | `LOAD` | `arg1` slot, `arg2` addressing mode |
| 2 | `LOAD_ADDR` | `arg1` slot, `arg2` addressing mode; pushes a pointer |
| 3 | `PUSH_CONST` | `arg1` type (`1` int, `2` float, `3` string), `arg2` value |
| 4 | `POP` | Discards the top stack value |
| 5 | `STORE` | `arg1` slot, `arg2` addressing mode; writes the value, then pushes it back |
| 6–11 | `ADD`, `SUB`, `MUL`, `DIV`, `MOD`, `NEG` | Arithmetic operations |
| 12, 13 | `ITOF`, `FTOI` | Integer-to-float and float-to-integer conversions |
| 14 | `CMP` | `arg1` 40–45: `==`, `!=`, `<`, `<=`, `>`, `>=` |
| 15 | `RET` | Pops and returns the return value |
| 16–18 | `JMP`, `JMP_FALSE`, `JMP_TRUE` | `arg1` target offset (relative to `code`) |
| 19 | `CALL` | `arg2` target function offset (relative to `code`) |
| 20 | `PRINT` | `arg1` argument count |
| 21 | `EXT` | `arg1` total stack values popped, including the native number pushed first |
| 22 | `NOP` | `arg1` label index; has no case in the VM switch statement |
| 23 | `WAIT` | Yields execution until the next frame |
| 24–26 | `AND`, `OR`, `NOT` | Logical operations |
| 27 | `END` | Terminates the current program |
| 28 | `SKIP_END` | Marks the point where cutscene skipping stops |
| 29, 30 | `SIN`, `COS` | Trigonometric operations |

`NOP` appears 472,159 times on the PAL disc and acts as a jump label. Every jump instruction on the disc targets a `NOP`, and `arg1` stores the sequential label number allocated by the compiler within that function.

### Addressing modes (`RS_ADDR_MODE`)

| Mode | Type | Description |
|---|---|---|
| `0x1` | `int` | Local `int` scalar |
| `0x2` | `int[]` | Local `int` indexed (array) |
| `0x4` | `int*` | `int` indexed through a pointer stored in a local slot |
| `0x8` | `float` | Local `float` scalar |
| `0x10` | `float[]` | Local `float` indexed (array) |
| `0x20` | `float*` | `float` indexed through a pointer stored in a local slot |
| `0x40` | `int` | Global `int` scalar |
| `0x200` | `float` | Global `float` scalar |

## What the instruction set says about the source language

| Bytecode Feature | Source Language Behavior |
|---|---|
| `STORE` followed by `POP` | `STORE` leaves the assigned value on the stack, and expression statements end with `POP`. Assignment is an expression, as in C. |
| `ITOF`, `FTOI`, and typed modes | The source language had declared `int` and `float` variables. Mixed int/float arithmetic without conversion opcodes is common; only 3,318 `ITOF`/`FTOI` instructions appear on the disc, which decompile as explicit casts `(int)x` and `(float)x`. |
| `CALL` and `RET` | The language supports named functions with parameters, local variables, and value-returning `RET` instructions. |
| `LOAD_ADDR` (`0x4`, `0x20`) | Corresponds to address-of expressions (`&x`), arrays, and pointer/array parameters. |
| `EXT` (void return) | `EXT` pushes no return value onto the stack. At the VM level, every native call is a void statement that returns values through pointer arguments (for example, `_GET_FLAG(12, &done);`). The SDK wraps single-output natives so they return their result directly (`done = get_flag(12);`; see [the language](../scripting/language.md)). |

## Native function tables

Each VM subsystem binds its own table of native functions. The ChronicleTwo decompilation defines all five tables with retail names as `RS_EXTFUNC_INFO {func, no}` arrays, where `no` comes from an enum in the corresponding header:

| Table | Source | Rows | Used by |
|---|---|---|---|
| `event` | `ps2/src/event_func.cpp` `ext_func_info[697]` | 696 | Map and dungeon event scripts |
| `monster` | `ps2/src/runscript_opcodes.cpp` `ext_func_info[]` | 173 | `dungeon/monster/*.stb` |
| `action` | `ps2/src/actscript.cpp` `ext_func_info[]` | 82 | `mainchr/c01.stb`, `c02.stb`, `dungeon/act_script/*.stb` |
| `effect` | `ps2/src/effscript.cpp` `ext_func_info[129]` | 128 | `dungeon/eff_script/*.stb` |
| `arg` | `ps2/src/event_func.cpp` `esa_ext_func_info[3]` | 2 | Argument files loaded by `_LOAD_ARG` |

### Path-to-table resolution

A script's file path determines which native table it uses (first match wins), verified across the entire disc corpus. The disc definition file `definitions/pal/definitions.json` stores these path rules (see [definitions](../../definitions/README.md)); in C source code, the SDK header that a script includes selects its table.

| Path Prefix | Native Table |
|---|---|
| `event/tbox/bomb.pack.d/` | `effect` (`tbox_exp`, loaded by `effscript.cpp`) |
| `event/` | `arg` (`_LOAD_ARG` files: `*_arg*.stb`, `e200/`) |
| `dungeon/monster/` | `monster` |
| `dungeon/eff_script/`, `dungeon/articles/`, `dungeon/sphida/` | `effect` |
| `dungeon/act_script/`, `mainchr/` | `action` |
| Everything else (`map/`, `dungeon/sys_script/`, ...) | `event` |

### Native call mechanics

- **Signatures and types**: Every native function's C name matches its enum name, and names are unique within each table. A native function reads its arguments from the VM stack inside its C implementation. Those stack reads indicate argument types, and the script bytecode provides the remaining type information through `LOAD_ADDR` (`&x`) and `PUSH_CONST` type tags when the C function indexes the stack inside loops or branches. The SDK headers declare and document every native function ([the SDK](../scripting/language.md), [the API](../scripting/api/README.md)).
- **Resolving the native number**: `EXT` pops `arg1` values from the stack, and the deepest popped value (pushed first) is the native function number. A parser finds this number by walking backward from `EXT` and summing stack effects: `CALL` pops the callee's `arg` count and pushes one return value, and the indexed addressing modes (`0x2`, `0x4`, `0x10`, `0x20`) pop an index before pushing the result. All 1,407,551 `EXT` calls on the PAL disc trace back to a `PUSH_CONST` integer or an `ADD` of two `PUSH_CONST` integers.
- **Monster group base + offset**: Monster scripts usually compute the native number as a group base plus an offset (for example, `push 70; push 15; add` for `_GET_TARGET_ANGLE`), and 12,903 functions on the disc use this pattern. Different call sites sometimes use different group bases for the same native (`_SET_MOS` uses `180 + 2` 8,630 times and `170 + 12` four times). To preserve the exact bytecode, the SDK declares each native with its most common group base (`NATIVE(70, 15)`), and `AT(n)` marks any call site that uses a different base (`AT(60) get_pos(&target_angle);`).
- **Unregistered native numbers**: Across the disc, 227 calls use 22 native numbers that the corresponding table never registers. At runtime, the VM prints `not found ext` and continues. These calls occur in the two old `"STB"` action scripts and a few monster scripts; they decompile as `_EXT_67(...)`, which recompiles cleanly.

## Older native numberings

Monster and action scripts on the PAL disc include several files compiled against older native tables. At runtime, the engine executes whatever native function matches the computed number in the current table.

| Table | Scheme | Definition Tag | Affected Scripts | Mapping Table in JSON | Mapped Calls / IDs |
|---|---|---|---|---|---|
| `monster` | Group `60` shifted to `70` (`+10`) | (automatic) | `b07a_atora_yellow`, `e08a`, `e09a`, and their exclusive helper libraries | `old_groups` in `definitions/natives/monster.json` | 197 calls |
| `monster` | Groups `110`, `140`, `160` shifted to `120`, `150`, `180` | `"numbering": "old"` | `b07a_atora_yellow`, `e08a`, `e09a`, and their exclusive helper libraries | `old_table_groups` in `definitions/natives/monster.json` | Verified against newer monster scripts |
| `monster` | Raw integer native numbers (no `ADD`) | `"numbering": "raw"` | `b02a2`–`b02a7` (byte-identical) and `e236a` | `old_direct` in `definitions/natives/monster.json` | 41 raw numbers (for example, `52`–`61` map to `77`–`86`) |
| `action` | Older action table | `"numbering": "raw"` | `c01win` (an old build of Max's script) and `f204a` | `old_direct` in `definitions/natives/action.json` | 27 native numbers |

### Shifted monster group bases (`"numbering": "old"`)

- **Group `60` vs `70`**: Scripts compiled against an older monster table call certain natives with group base `60` where the current table uses group base `70`. Shifting group `60` to `70` (adding `10` to the native number) matches the argument types of all 197 such calls on the disc, confirmed by identical functions in newer monster scripts. The decompiled C shows the function that the engine actually executes, with a comment naming the originally intended function:

```c
AT(60) get_pos(&target_angle);  // old numbering, meant as get_target_angle
```

- **Groups `110`, `140`, and `160`**: Those same scripts (`b07a_atora_yellow`, `e08a`, `e09a`, and helper libraries used only by them) also call groups `110`, `140`, and `160` where the current table uses `120`, `150`, and `180`. Because `110`, `140`, and `160` are also valid groups in the current table, the definition set explicitly marks these older scripts with `"numbering": "old"` in their `scripts/` entry. The decompiler emits `old_table_groups` comments only in scripts marked `"old"` and in helper libraries used exclusively by them.

### Raw monster and action native numbers (`"numbering": "raw"`)

- **Raw monster table (`b02a2`–`b02a7`, `e236a`)**: Seven monster scripts push raw native numbers with no group base addition. These calls decompile with the `DIRECT` keyword, which no other monster script requires. Comparing these scripts against newer counterparts (`e228a` for `e236a`, and `b02a1`, `b02a`, and `b04a` for the `b02a` bosses) identifies 41 raw numbers.
- **Why `"numbering": "raw"` is required**: Calling a native by a single integer without `ADD` does not by itself indicate the old table, because current scripts (`b02a1` and `p00_23a`) also call the first native of a group (offset `0`) by its bare group number. Therefore, the definition set marks `b02a2` to `b02a7` and `e236a` with `"numbering": "raw"`. In those scripts and their exclusive helper libraries, any `DIRECT` call to one of the 41 mapped numbers, or any call that lands on a native without a group in the SDK header, receives an explanatory comment:

```c
DIRECT get_monster_var2(&turn_target);  // old numbering, meant as get_target_angle
set_dungeon_flag(turn_target, speed);  // old numbering, meant as turn_to
```

- **Raw action table (`c01win`, `f204a`)**: Comparing `c01win` and `f204a` against their current counterparts (`c01`, the action libraries, and the other `f` scripts) identifies 27 native numbers recorded in `old_direct` in `action.json`. Both scripts are marked `"raw"`, so their calls—including unregistered `_EXT_` calls—include a comment naming the intended native function (`_EXT_80(...)  // old numbering, meant as add_motion_se`).
