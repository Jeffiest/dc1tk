# dc2tk: the code

`dc2tk` is the one tool of this repository: it extracts the disc, decompiles
and compiles the scripts, builds mods into a new disc image and writes the
asset tree. Its commands are in [usage](../docs/usage.md); how the
decompiler and compiler work is in [internals](../docs/internals.md).

The workspace's `Cargo.toml` is at the repository root, so run Cargo there:

```
cargo build --release          builds build/release/dc2tk (dc2tk.exe on Windows; .cargo/config.toml
                               puts Cargo's output in build/), which dc2tk.bat and ./dc2tk run
cargo test --release           the tests
cargo fmt                      formatting (rustfmt.toml: 120 columns)
```

`dc2tk` finds `sdk/`, `definitions/` and `src/natives/` in the nearest folder
above the executable that has `definitions/`, or in the folder `$DC2TK_ROOT`
names.

The tests use small discs and scripts built in memory
(`dc2tk_disc::samples`), the SDK and the definitions; no game files. The
tests that read the game's own files are marked ignored: with `DC2TK_CORPUS`
set to the absolute path of an extracted disc (`extracted/disc`),
`cargo test --release -- --include-ignored` also round-trips every
script on it (about 40 s), every `.map` scene file and every message file (a
few seconds each), the item, shop and monster tables, and checks every load on
it against the memory budgets of [docs/assets/memory.md](../docs/assets/memory.md).

## Crates

| crate | holds |
|---|---|
| `dc2tk-disc` | the disc: ISO 9660, `DATA.HD4` and `DATA.DAT`, the packs inside its files, extracting them all, reading an image and an extracted disc alike (`DiscSource`), writing a new image with files replaced or added, cp932 names, and the text files the tools write (`textfile`: UTF-8, LF line endings, JSON as serde_json writes it pretty) |
| `dc2tk-script` | the scripts: the `.stb` format, its assembly, the native tables, definition sets, the C parser, the decompiler, names, notes and messages, libraries, the SDK, the compiler, a VM interpreter for `mod test`, the source index (flags, xref, maps), learning definitions, decompiling a whole disc into a tree (`decompiler::decompile_tree`), build and mods (patches, additions, numbers, `start_at`, assets by id, maps built from scenes (`mapedit`), data tables (`datatables`), editor headers, overrides followed by fingerprint, wrapped natives, whole files slimmed into patches), and the memory budgets build checks |
| `dc2tk-assets` | textures (IM3/TIM2 to and from PNG), sound banks (SPU2 ADPCM to and from WAV), character packs (MDS, motions, skin weights to and from glTF), map scene files (`.map` as a typed tree), message files (`.mes` to and from text), game data tables (items, shops, monsters) as JSON, the asset tree and its ids |
| `dc2tk` | the command line, which parses the arguments, runs the library's stages and prints what they did: the options commands share (`options.rs`), and one module per group of commands (`setup.rs`, `modding/` for build, mod check, the other mod commands and PCSX2, `verify.rs`, `defs.rs`, `xref.rs`, `asset_tree.rs`, `map_cmd.rs`) |

Each module starts with a doc comment saying what it is for. The program
tree the decompiler and compiler pass around is `dc2tk_script::node::Node`:
ordered maps and lists with static keys, built with `node!` (serde_json's
`json!` syntax) and written as JSON, so every stage can be dumped and
compared. serde_json's own `Value` was three times slower, since every node
hashed and allocated its keys.

Two hidden commands check a change against the whole disc: `dc2tk compile-tree TREE EXTRACTED`
compiles every script of a decompiled tree and compares it with the disc's, and
`dc2tk assets-check DISC TREE` builds every asset of a tree back and compares it with the disc's
bytes at every place the manifest lists.

## Speed

On the PAL disc (16 cores):

| | dc2tk |
|---|---|
| set up the disc (extract and decompile) | 81 s |
| decompile the whole disc to C | 20.6 s |
| disassemble all 1,398 scripts | 1.7 s |
| `build`, every script compiled, per mod set | 15-47 s |
| `verify` the whole disc | 40 s |
| export every texture, sound bank and character pack | 8.6 s, 3.0 s, 5.9 s |
| write the asset tree (`dc2tk assets`) | 12-16 s |

Writing a 4.7 GB image takes about 2 s: it is a file copy that
Windows does by cloning blocks, as long as nothing has the source image
mapped, so `write_disc` copies before it maps.
