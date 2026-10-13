# c2: the Rust port

`c2` is the Rust port of `cli.py`: the same commands, printing the same
things, writing the same bytes, faster. The Python stays the reference until
every command matches it; see [the design](../docs/design.md) for the plan.

```
cargo build --release          builds rust/target/release/c2
cargo test --release           unit tests
python rust/parity.py ...      checks c2 against cli.py (run from the repository folder)
```

## Ported so far

| command | crate | checked against the Python |
|---|---|---|
| `c2 extract ISO OUT` | `c2-disc` | the whole PAL disc: the same 51,607 files and folders, names, case and bytes |
| disc writing (`build`'s last step) | `c2-disc` | castle-showcase gives the same image as `cli.py build`; a set of files through every branch (in place, into the filler, past the end of the volume, new files, a moved index) gives the same image |
| cp932 names | `c2-disc` | every one- and two-byte code decodes, and every character encodes, as Python's codec does |
| `c2 disassemble`, `c2 assemble` | `c2-script` | all 1,398 scripts disassemble to the same text, and every listing assembles back to its script |

Hidden commands feed `parity.py`: `c2 write-disc ISO OUT FOLDER` writes the
files a folder lists (`files.json`, then `0.bin`, `1.bin`, ...), so it can be
given the files a Python build writes; `c2 disassemble-tree ROOT OUT` and
`c2 assemble-tree ROOT ASM` do every script at once.

## Times on the PAL disc (16 cores)

| | Python | Rust |
|---|---|---|
| extract | 40 s | 21 s (writing 46,000 files; the reading takes under a second) |
| write a 4.7 GB image | about 2 s | about 2 s |
| disassemble all 1,398 scripts | 27.7 s | 1.7 s |

Writing an image is a file copy that Windows does by cloning blocks, as long
as nothing has the source image mapped, so `write_disc` copies before it maps.
A second image written straight after another takes 30-45 s whichever tool
writes it (the disk is still busy with the first), so `parity.py`'s times
for the second writer are not a comparison.

## Layout

| crate | holds | ported from |
|---|---|---|
| `c2-disc` | ISO 9660, `DATA.HD4`, packs, extract, writing an image, cp932 | `iso.py`, `archive.py`, `extract.py`, `rebuild.py` (its disc half) |
| `c2-py` | CPython's float repr, `%g`, `int(x, 0)`, `float()`, `str.split()`, which the output depends on | |
| `c2-script` | `.stb`, assembly, native tables, definition sets (so far: the table rules) | `stb.py`, `asm.py`, `stack.py`, `natives.py`, `definitions.py` (part) |
| `c2` | the command line | `__main__.py` |

Still to come, in order: the decompiler (with the rest of the
definitions and names), the compiler and build, mods, assets,
the remaining commands; then the asset tree and mods by name.
