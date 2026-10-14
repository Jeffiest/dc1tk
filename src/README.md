# c2: the code

`c2` is the one tool of this repository: it extracts the disc, decompiles
and compiles the scripts, builds mods into a new disc image and writes the
asset tree. Its commands are in [usage](../docs/usage.md); how the
decompiler and compiler work is in [internals](../docs/internals.md).

The workspace's `Cargo.toml` is at the repository root, so run Cargo there:

```
cargo build --release          builds build/release/c2 (c2.exe on Windows; .cargo/config.toml
                               puts Cargo's output in build/), which c2.bat and ./c2 run
cargo test --release           the tests
cargo fmt                      formatting (rustfmt.toml: 120 columns)
```

`c2` finds `sdk/`, `definitions/` and `src/natives/` in the nearest folder
above the executable that has `definitions/`, or in the folder `$C2_ROOT`
names.

The tests use small discs and scripts built in memory
(`c2_disc::samples`), the SDK and the definitions; no game files. With `C2S_CORPUS` set to the absolute path of an extracted disc
(`extracted/disc`), `cargo test --release corpus` also round-trips every
script on it (about 40 s), every `.map` scene file (a few seconds), and checks every load on it
against the memory budgets of [docs/assets/memory.md](../docs/assets/memory.md).

## Crates

| crate | holds |
|---|---|
| `c2-disc` | the disc: ISO 9660, `DATA.HD4` and `DATA.DAT`, the packs inside its files, extracting them all, writing a new image with files replaced or added, cp932 names |
| `c2-script` | the scripts: the `.stb` format, its assembly, the native tables, definition sets, the C parser, the decompiler, names, notes and messages, libraries, the SDK, the compiler, a VM interpreter for `mod test`, the source index (flags, xref, maps), learning definitions, build and mods (patches, additions, numbers, `start_at`, assets by id), and the memory budgets build checks |
| `c2-assets` | textures (IM3/TIM2 to and from PNG), sound banks (SPU2 ADPCM to and from WAV), character packs (MDS, motions, skin weights to and from glTF), map scene files (`.map` as a typed tree), the asset tree and its ids |
| `c2-py` | number and text formatting as CPython does it (float repr, `%g`, `int(x, 0)`, `json.dumps`, `textwrap`, `difflib`, Unicode `str` methods): `c2` was ported from a Python tool and writes the same bytes it did, so trees and mods made with that tool still match |
| `c2` | the command line: one module per group of commands (`setup.rs`, `tree_decompile.rs`, `modding.rs`, `verify.rs`, `defs.rs`, `xref.rs`, `asset_tree.rs`) |

Each module starts with a doc comment saying what it is for. The program
tree the decompiler and compiler pass around is `c2_script::node::Node`:
ordered maps and lists with static keys, built with `node!` (serde_json's
`json!` syntax) and written as JSON, so every stage can be dumped and
compared. serde_json's own `Value` was three times slower, since every node
hashed and allocated its keys.

Some hidden commands run a stage over every script or asset at once
(`disassemble-tree`, `assemble-tree`, `decompile-dump`, `script-sources`,
`compile-tree`, `to-asm-tree`, `resolve-tree`, `index-dump`, `disc-header`,
`asset-export`, `asset-build`, `assets-check`, `write-disc`), for checking a
change against the whole disc. `c2 assets-check DISC TREE` builds every asset
of a tree back and compares it with the disc's bytes at every place the
manifest lists.

## Speed

Against the old Python tool, on the PAL disc (16 cores):

| | Python | c2 |
|---|---|---|
| set up the disc (extract and decompile) | 837 s | 81 s |
| decompile the whole disc to C | 8 min 38 s | 20.6 s |
| disassemble all 1,398 scripts | 27.7 s | 1.7 s |
| `build`, every script compiled, per mod set | 248-410 s | 15-47 s |
| `verify` the whole disc | 15 min 32 s | 40 s |
| export every texture, sound bank and character pack | 38-70 s, 16.5 s, 102-152 s | 8.6 s, 3.0 s, 5.9 s |
| write the asset tree (`c2 assets`) | | 12-16 s |

Writing a 4.7 GB image takes about 2 s either way: it is a file copy that
Windows does by cloning blocks, as long as nothing has the source image
mapped, so `write_disc` copies before it maps.
