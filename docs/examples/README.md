# Examples

Each example is a made-up script, written for these docs, shown at every
stage between bytecode and readable source. No retail code appears here.
`src/tests/test_examples.py` rebuilds every stage from the script and fails
if a file below is out of date, so these are exactly what the tools print.

## greeting

| File | What it shows |
|---|---|
| [`greeting.asm`](greeting/greeting.asm) | `cli.py disassemble`: the bytecode as text assembly |
| [`greeting.bare.c`](greeting/greeting.bare.c) | `cli.py decompile --defs none --sdk <empty>`: structure recovered, but natives keep their original Level-5 names and everything else is named by position |
| [`greeting.sdk.c`](greeting/greeting.sdk.c) | `cli.py decompile --defs none`: the SDK names natives, folds helpers (`set_camera_view`, `place_actor`, `set_actor_face`), names constants and entry points, and names locals after the natives that fill them |
| [`greeting.c`](greeting/greeting.c) | `cli.py decompile --defs definitions`: the example's definition set adds function and global names, docs and translations of the Japanese asset names |

All four compile to the same bytes.

The [definition set](greeting/definitions) was not written by hand: it is
what `cli.py defs learn greeting.c definitions` records from the
annotated script. Decompiling the bytecode with it gives the annotated
script back.
