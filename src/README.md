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
| disc writing (`build`'s last step) | `c2-disc` | a set of files through every branch (in place, into the filler, past the end of the volume, new files, a moved index) gives the same image |
| cp932 names | `c2-disc` | every one- and two-byte code decodes, and every character encodes, as Python's codec does |
| `c2 disassemble`, `c2 assemble` | `c2-script` | all 1,398 scripts disassemble to the same text, and every listing assembles back to its script |
| the C parser | `c2-script` | 3,877 inputs as the same JSON or the same error: every source file under `decompiled/`, `mods/`, `sdk/` and the definitions' headers, 300 of them cut short and with a line gone, and probes of #if conditions, error paths and nesting |
| `c2 decompile` (one script, or a whole extracted disc) | `c2-script` | all 1,398 scripts as the same program trees, C text and named C; a whole-disc decompile writes the same tree as `cli.py decompile`, records and `disc.h` included |
| `c2 compile` | `c2-script` | every script's resolved program and assembly the same; all 1,398 decompiled scripts compile back to the disc's bytes |
| `c2 flags` | `c2-script` | the summary, `--used` and `--flag N` the same |
| `c2 build` | `c2-script`, `c2-disc` | the same printout and the same image, byte for byte, for each mod in `mods/` (patches, additions, NEW_FLAG numbers, `start_at`, models and textures under `disc/`) and for no mods |
| `c2 mod new/save/list/test/apply/override/asset/check` | `c2-script` | the same printout and the same files written (into scratch copies of the tree and mods folder) |
| `c2 play`, `c2 changes`, `c2 fork` | `c2` | |
| `c2 setup` | `c2` | the PAL disc set up both ways gives the same 46,314 extracted files and 2,499 tree files (Python 837 s, Rust 81 s) |
| `c2 natives` | `c2-script` | no ChronicleTwo checkout in the layout it reads was at hand; made-up checkouts give the same tables and output (`rust/parity_natives.py` repeats the check on a real one) |
| `c2 defs check/todo/disc/learn` | `c2-script` | check, todo and disc the same on the PAL disc; learn the same for edited single files (whole-tree learn not compared yet; `rust/parity_defs.py`) |
| `c2 xref`, `c2 maps`, `c2 map` | `c2-script` | 2,718 inputs the same: every map, every program number and name, every monster kind (`rust/parity_xref.py`) |
| `c2 verify` | `c2` | the same report and the same `--json` file for the whole PAL disc (every script through all four stages) |
| `c2 assets` (new: the asset tree of design section 1) | `c2-assets` | no Python counterpart; every one of its 15,645 files builds back to the disc's bytes at every place the manifest lists (`c2 assets-check`), and runs give identical trees |
| textures, sounds, models (`mod asset --as`, and building them back) | `c2-assets` | every `.img` (5,217), `.snd` (861) and `.chr` (2,997) exports to the same PNGs, WAVs and glTF; every unedited export builds back to the Python's bytes, and so do 3,000 edited files (recoloured, resized, re-encoded PNGs; resampled, 8/24-bit and float WAVs; moved, scaled and added meshes), failing where the Python fails |

Hidden commands feed `parity.py`: `c2 write-disc ISO OUT FOLDER` writes the
files a folder lists (`files.json`, then `0.bin`, `1.bin`, ...), so it can be
given the files a Python build writes; `c2 disassemble-tree`,
`c2 assemble-tree`, `c2 decompile-dump`, `c2 script-sources`,
`c2 compile-tree`, `c2 to-asm-tree`, `c2 resolve-tree`, `c2 index-dump`,
`c2 disc-header`, `c2 asset-export` and `c2 asset-build` do every script or
asset at once. The parser's check runs the `parse_dump` example of
`c2-script`.

The program tree is `c2_script::node::Node`: the Python's dicts and lists as
small ordered vectors with static keys, built with `node!` (serde_json's
`json!` syntax) and written as the same JSON. serde_json's own `Value` was
three times slower, since every node hashed and allocated its keys.

## Times on the PAL disc (16 cores)

| | Python | Rust |
|---|---|---|
| extract | 40 s | 21 s (writing 46,000 files; the reading takes under a second) |
| write a 4.7 GB image | about 2 s | about 2 s |
| disassemble all 1,398 scripts | 27.7 s | 1.7 s |
| decompile all 1,398 scripts to program trees | 40 s | 2.9 s |
| decompile the whole disc to C (`decompile extracted`) | 8 min 38 s | 20.6 s |
| compile all 1,398 scripts and compare with the disc | | 13.4 s |
| `build` (every script compiled: the tree's record predates the headers), per mod set | 248-410 s | 15-47 s |
| `verify` the whole disc | | 40 s |
| write the asset tree (`c2 assets`) | | 12-16 s |
| parse the 3,877 inputs of the parser's check (60 MB) | 21.4 s | 2.6 s on one core |
| export every texture to PNG | 38-70 s | 8.6 s |
| export every sound bank to WAV | 16.5 s | 3.0 s |
| export every character pack to glTF | 102-152 s | 5.9 s |
| build every unedited export back (textures, sounds, models) | 74 s, 32 s, 139 s | 2.7 s, 1.7 s, 2.6 s |

Writing an image is a file copy that Windows does by cloning blocks, as long
as nothing has the source image mapped, so `write_disc` copies before it maps.
A second image written straight after another takes 30-45 s whichever tool
writes it (the disk is still busy with the first), so `parity.py`'s times
for the second writer are not a comparison.

## Layout

| crate | holds | ported from |
|---|---|---|
| `c2-disc` | ISO 9660, `DATA.HD4`, packs, extract, writing an image, cp932 | `iso.py`, `archive.py`, `extract.py`, `rebuild.py` (its disc half) |
| `c2-py` | CPython's float repr, `%g`, `int(x, 0)`, `float()`, `str.split()`, `textwrap`, `difflib.get_close_matches`, `json.dumps`, text files with Python's newlines, and the Unicode `str.is*()` methods (tables from CPython 3.13), which the output depends on | |
| `c2-script` | scripts: `.stb`, assembly, native tables, definition sets, the parser, the decompiler, names, notes, messages, libraries, the SDK, the compiler, the source index, flags, and build and mods | the rest of `src/c2s` but the asset modules |
| `c2-assets` | textures (IM3/TIM2, PNG), sound banks (ADPCM, WAV), character packs (MDS, motions, glTF) | `im3.py`, `textures.py`, `soundbank.py`, `sounds.py`, `mds.py`, `mot.py`, `info_cfg.py`, `to_gltf.py`, `from_gltf.py`, `models.py` |
| `c2` | the command line | `__main__.py`, `play.py` |

Every command of `cli.py` is ported. So are the asset tree and mods by name (design sections 1 and 2). Still to come:
removing the Python (the parity scripts use it).
