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
(`dc2tk_disc::samples`, built only for tests: the other crates enable
dc2tk-disc's `test-support` feature in their dev-dependencies), the SDK and
the definitions; no game files. The tests that read the game's own files are
marked `#[ignore = "needs DC2TK_CORPUS"]`, so a plain `cargo test` lists them
as ignored: with `DC2TK_CORPUS` set to the absolute path of an extracted disc
(`extracted/disc`), `cargo test --release -- --include-ignored` also
round-trips every script on it (about 40 s), every `.map` scene file (and
rebuilds every map from its own scene), every message file and music
sequence (through MIDI too) and every bank's SoundFont, every game data
table (items, shops, monsters, inventions, Georama, dungeon floors and
layouts, chests, fishing, requests, townspeople, the executable's tables),
the character packs'
meshes and motions through glTF (a few seconds each), and checks every load on
it against the memory budgets of [docs/assets/memory.md](../docs/assets/memory.md)
(`corpus_retail_fits_the_budgets`, which also pins the retail maxima the
estimated budgets are measured by). Without the variable, `--include-ignored`
fails, naming it.

## Crates

| crate | holds |
|---|---|
| `dc2tk-disc` | the disc: ISO 9660, `DATA.HD4` and `DATA.DAT`, the packs inside its files, extracting them all, reading an image and an extracted disc alike (`DiscSource`), writing a new image with files replaced or added, cp932 names, the text files the tools write (`textfile`: UTF-8, LF line endings, JSON as serde_json writes it pretty), and file operations whose errors name the file (`files`) |
| `dc2tk-script` | the scripts: the `.stb` format, its assembly, the native tables, definition sets, the C parser and preprocessor (`parse/`), the SDK headers and resolving calls to and from them (`sdk/`), the decompiler, names, notes and messages, libraries, the compiler, a VM interpreter for `mod test`, the source index (flags, xref, maps), learning definitions, decompiling a whole disc into a tree (`decompiler::decompile_tree`), build and mods (patches, additions, numbers, `start_at`, assets by id, maps built from scenes (`mapedit`), data tables (`datatables`), editor headers, overrides followed by fingerprint, wrapped natives, whole files slimmed into patches), and the memory budgets build checks (`memory/`) |
| `dc2tk-assets` | textures (IM3/TIM2 to and from PNG), sound banks (SPU2 ADPCM to and from WAV), character packs (MDS, motions, skin weights to and from glTF), map scene files (`.map` as a typed tree), message files (`.mes` to and from text), music (sequences to and from MIDI, banks as SoundFonts), game data tables (items, shops, monsters, inventions, Georama, dungeons and the rest) as JSON, the boot executable's tables patched in place (`elf`), the asset tree and its ids (`manifest`: manifest.json as types); and what the converters share: the bounds-checked byte readers (`bytes`), PNG (`png`) and palette quantisation (`quantise`), glTF's accessor types (`gltf`), the matrix and quaternion maths (`math`), the arithmetic the bytes depend on (`numeric`), the languages (`lang`) and the data tables' JSON shapes (`json`) |
| `dc2tk` | the command line, which parses the arguments, runs the library's stages and prints what they did: the options commands share (`options.rs`), where the repository's data is (`files.rs`), and one module per group of commands (`setup.rs`, `modding/` for build, mod check, the other mod commands and PCSX2, `verify.rs`, `defs.rs`, `xref.rs`, `asset_tree.rs`, `map_cmd.rs`) |

Each module starts with a doc comment saying what it is for. The program
tree the decompiler and compiler pass around is `dc2tk_script::node::Node`:
ordered maps and lists with static keys, built with `node!` (serde_json's
`json!` syntax) and written as JSON, so every stage can be dumped and
compared. serde_json's own `Value` was three times slower, since every node
hashed and allocated its keys.

Two hidden commands check a change against the whole disc: `dc2tk compile-tree TREE EXTRACTED`
compiles every script of a decompiled tree and compares it with the disc's, and
`dc2tk assets-check DISC TREE` (the image or the extracted disc, and the asset tree) checks every
file of the tree has the manifest's hash, builds every asset back and compares it with the disc's
bytes at every place the manifest lists, and checks every game file is listed. Both fail when
anything differs.

[docs/development.md](../docs/development.md) has the rule every change keeps (an image built
with nothing edited is retail's, byte for byte), the checks to run and the code's conventions.

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
