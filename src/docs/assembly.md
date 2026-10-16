# Assembly

`src/crates/dc2tk-script/src/asm.rs`. The disassembler prints a parsed script
as text; the assembler parses the text back into the same regions
`stb::read` returns and hands them to `stb::write`, so both directions share
one writer.

```
.natives event
.header SB2 main f_0000 globals 1
.program 100 f_0000
.program 200 f_00d8
.code

.function f_0000 local 1 arg 0
    push "hello"
    print 1
    native _ASSIGN_STACK
    push 5
    call f_00d8
    addr local 0
    ext 3
    load local 0
    jf L_00b0
    wait
L_00b0:
    end
.string "hello"
.padding 7374616c65006974656d

.function f_00d8 local 1 arg 1 types 1
    nop 0
    push 7
    ret
```

- Labels are offsets from the code section: `f_` for function records,
  `L_` for jump targets. The assembler computes every offset, `addr`, `main`,
  `code`, `prog` and `prog_num` from the layout; the header line names a field
  only when the file's value differs from the computed one.
- Addressing modes print as `local`, `local_idx`, `ptr_idx`, `local_f`,
  `local_idx_f`, `ptr_idx_f`, `global`, `global_f`; comparisons as `cmp eq`
  to `cmp ge`; `JMP_FALSE`/`JMP_TRUE` with `arg2` set as `jf_push`/`jt_push`
  (they push the tested value when they jump, for `&&` and `||`).
- `native NAME` is the `PUSH_CONST` int that carries an `EXT`'s native number,
  named from the `.natives` table; an unregistered number prints as
  `native 190`.
- The opcodes, addressing modes and comparisons are `stb::Op`, `stb::Mode`
  and `stb::Compare` (`src/crates/dc2tk-script/src/stb.rs`), one table each
  that the assembler, the compiler, the decompiler and the test VM share.
- Floats print as the shortest decimal that reads back as the same 32 bits
  (`0.5`, `1.0`, `1e20`, `1.5e-7`); where rounding through a double would
  land that text beside a halfway point, the double's exact text prints
  instead. The assembler reads a float (a number with a point or an
  exponent) as a double and rounds it to a float. Other numbers are decimal
  or `0x` hexadecimal, with an optional sign.
- `.function NAME local N arg N` adds `types ...` for the record's
  `arg_types` up to the last nonzero one, and `name N` when the record's
  name word is not 0. The header line adds `code`, `prog`, `prog_num`,
  `unk_14` or `padding HEX` only where the file's differ from the usual.
  `.tail` (which the compiler writes) zero-fills the code section after the
  last function to a multiple of 16.
- Words are separated by white space, and a line holds one item. A string
  is in double quotes, with `\\`, `\"`, `\n`, `\r`, `\t` and `\xHH`
  escapes. The disassembler writes LF line endings; the assembler reads
  CRLF the same.
- `push "text"` takes the function's next `.string`. Where pushes do not use
  the block in order (198 functions, all with unreferenced strings in their
  block), they print as `push string N`, the index into `.string`.
- The final `FUNC_END` is implied. Anything else the text cannot express
  prints as `.op OP ARG1 ARG2` or `.bytes HEX`, so any file round-trips; the
  retail disc needs none of them.
