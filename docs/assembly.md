# Assembly

`src/crates/c2-script/src/asm.rs`. The disassembler prints a parsed script
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
- Floats print as the shortest decimal that gives back the same 32 bits.
- `push "text"` takes the function's next `.string`. Where pushes do not use
  the block in order (198 functions, all with unreferenced strings in their
  block), they print as `push string N`, the index into `.string`.
- The final `FUNC_END` is implied. Anything else the text cannot express
  prints as `.op OP ARG1 ARG2` or `.bytes HEX`, so any file round-trips; the
  retail disc needs none of them.
