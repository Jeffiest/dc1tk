# Rust code review — 2026-10-15

> This is the review as it was written. Line numbers and file names are from the code at c55f1f2, and many of those files have since been split or renamed. [development.md](../development.md) describes how the code is worked on now.
>
> **Status: fixed.** 119 commits on `main` (c55f1f2..e3d5f4e): packages 1-7, then a polish pass. Packages 2b and 7 were added on request: 2b removed all the Python-parity code, including the dc2tk-py crate, and 7 switched the memory checks to the game's own map tables.
>
> Final checks:
> - fmt is clean, and `clippy -D warnings` is clean.
> - 713 debug tests and 747 corpus tests pass.
> - The pack, full-rebuild and all three mod-build images are byte-identical to the retail baseline.
>
> Text output changed on purpose: LF line endings, two-space JSON, Rust float text, and rewritten error messages.
>
> The polish items left over are done too (b060d1d..e3d5f4e): no function is over 100 lines;
> `build` and `setup` take an extracted folder; the SDK's include guards are `DC2TK_*`; the memory
> checks use `dc2tk_assets::bytes`; a scene glTF of the wrong JSON shape is an error; and
> clippy::pedantic's unnecessary_wraps, unused_self, needless_pass_by_value and
> items_after_statements are fixed, but for five deliberate exceptions in `lower` and `composite`.

Scope: the five crates under `src/crates/` (about 88k lines). Each crate was read by its own reviewer. The bug claims marked *(verified)* were checked against the source by hand.

## Verdict

The code reads like the work of someone who knows the engine closely:
- The comments explain *why*, and cite engine symbols and source lines.
- User input mostly goes through `Result`s instead of panicking.
- Byte-exact round trips are tested throughout.

The debt is **structural**:
- **The same small helpers exist many times over:** `list`/`text` over `Node`, byte readers, directory walkers, the `map.cfg` regexes, pack layouts, VLQ readers, matrix code.
- **Several kinds of value are strings instead of enums:** node kinds, opcodes, addressing modes, number kinds, port state, target kinds.
- **A layer still imitates the deleted Python tool** (commit 9511102): frame counting in the parser, Python error texts, a duplicate `py.rs`.
- **A handful of 150–500-line functions** hold the most delicate logic.

## Tooling status

| Check | Result |
|---|---|
| `cargo clippy --workspace --all-targets` | clean |
| `cargo clippy -- -W clippy::pedantic` | about 4,600 warnings: mostly doc backticks, `as` casts and `#[must_use]`. Expected for binary-format code; not worth chasing wholesale. |
| `cargo fmt --check` | 7 diffs (`library.rs`, `verify.rs`, `tests/sdk.rs`, `tests/slots.rs`) |
| `cargo test --workspace` (debug) | **fails**: `parse::tests::nesting_stops_where_python_recursion_does` overflows its 1 MB stack. It passes in `--release`. All other tests pass (about 600). |

## Tier 1 — bugs and user-data safety (small fixes, do first)

1. **A build can overwrite its own input image** *(verified)*: `dc2tk-disc/src/rebuild.rs:250-255`. `write_copy` calls `fs::copy(iso, out)` without checking whether `iso == out`. The SAFETY comment at `dc2tk-disc/src/lib.rs:41` assumes this never happens. Fix: canonicalize both paths and bail if they are the same file.
2. **The debug test run fails** *(verified)*: `parse.rs:4770`. The parser's depth limit is Python's 1000 frames (`PYTHON_FRAMES`, `parse.rs:47-52`), and Rust debug frames are larger. Fix: replace the Python frame counting with a plain depth limit sized for Rust (see 2.1), or give the test a bigger stack.
3. **The test VM has no step or recursion limit**: `vm.rs:228-359`. `dc2tk mod test` runs user scripts in it, so `while(1){}` hangs and self-recursion overflows the stack. Fix: a fuel counter in `execute` and a call-depth cap in `run`.
4. **`map.cfg` is parsed by six regexes that disagree**: `memory.rs:702`, `startat.rs:100`, `maptable.rs:43,48`, `messages.rs:305`, `villagers.rs:116`.
   - Some are anchored and some are not.
   - Some need a 4th field, so a row without one is dropped and every later row number shifts.
   - The Palm Brinks sub-map detection (`memory.rs:722`) depends on those row numbers.

   Fix: one `maptable::rows()` that every caller uses.
5. **`enter_img` always returns 0 for IMG/IM2 files** *(verified)*: `memory.rs:1086`. It returns `last - current`, and that branch keeps the two equal. The IM3 branch returns `last - block`, which matches the function's doc comment. Either fix it, or note that the engine really does this, and add a test.
6. **Panics on malformed or modded input files:**
   - `map_import.rs:173-398`: indexes glTF arrays with indices from the file, e.g. `meshes[mesh]` *(verified)*.
   - `font.rs:330-382`: trusts header and clut sizes; `colour[3]` panics when the clut size is not a multiple of 4 *(verified)*.
   - `from_gltf.rs:189,252`: allocates `count * n` before any bounds check.
   - `json.rs:303-476`: the hand-written JSON parser has no nesting limit.
   - `dc2tk-disc/src/archive.rs:64-87`: `Vec::with_capacity` on an index count read straight from the file.
   - `cfg_text.rs:375,414`: unchecked indices; `item_data.rs:855` emits an edit that leads to one.
   - `dungeon_layout.rs:563`: `len() - 1` underflows. Line 272 refuses `#-1`, which the bounds check itself allows *(verified)*.
7. **Values from hand-edited JSON are truncated or ignored without an error:**
   - `shop_data.rs:208,268,343`: `item as i32` *(verified)*.
   - `chest_data`/`dungeon_layout` use `.as_array().into_iter().flatten()`, so a wrong JSON shape quietly does nothing.
   - `numbers.rs:140`: `read_lock` drops malformed entries, which can renumber a saved flag.
8. **Possible wrong message copy**: `item_data.rs:903` copies from `MESSAGE_BASE + like` (the item number), not from the copied item's `message` field. Needs confirming.
9. **Errors don't name the file**: `Image::open` and the fs helpers return a transparent `Io` error, so a failure among thousands of files shows only "os error 2". Fix: `Error::Open { path, source }`.
10. **Wrong help text**: `modding.rs:215-217`. The `Make` command carries `Migrate`'s doc line, and `Migrate` has none.

## Tier 2 — cross-cutting cleanups (biggest readability win)

1. **Remove the Python-parity layer.**
   - `PYTHON_FRAMES`/`leaf()`/`framed()`/`Fault` in `parse.rs`.
   - Python error texts: "list index out of range", "pop from empty list", "unpack_from requires…", in `vm.rs` and about 30 sites in dc2tk-assets.
   - `dc2tk-assets/src/py.rs` duplicates `dc2tk_py::float_repr`/`sum_floats`.
   - Stale "parse.py"/"(to come)"/`C2_ROOT` references.

   Keep only the CPython behaviour that output files depend on (dc2tk-py).
2. **Make `Node::list(key)`/`Node::text(key)` methods.** They are currently redefined in about 18 files, and `decompile.rs` re-implements `kind`/`with` on top.
3. **Add `paths.rs` in dc2tk-script** with `join_relative`, `file_name`, `walk(root, filter)` and `inside_mod`. These are currently copied 3–8 times each. Make the walkers report folders they cannot read instead of skipping them.
4. **Add a `bytes::Reader` cursor and one `Pack` type.** LE readers are copied in 6 files, and the 0x50-slot pack format is implemented 3 times (`dc2tk-disc::archive`, `soundbank.rs`, `models.rs:594`).
5. **Use enums for values that are now strings:**
   - VM `Mode`/`Op`: currently 5 mode tables and an 80-line string if-chain (`decompile.rs:235-317`).
   - `NumberKind`: `numbers.rs`, where `_ =>` silently means monster variable.
   - `PortValue`: `soundpass.rs:44-79`, which currently uses `"?"`/`"@entry"`/`"|"`-joined strings.
   - `Target.kind`, `replace::Stage`, `PackType`, and glTF `ComponentType`.
6. **Finish moving tables onto `data_table`.**
   - `monster_data` and `item_data` still carry their own `Kind`/`value_text`/`same`/apply code, and these already differ in behaviour (float printing, range names).
   - Move `file_offset`/`Patch`/`EXECUTABLE` into `elf.rs` and the language helpers into `lang.rs`, so the generic layer stops importing the specific ones.
7. **Fold duplicated tables into one each:**
   - Operator precedence: `parse.rs:73` and `source.rs:21`, which disagree.
   - Narrow-int rules: `compile.rs:393` and `lower.rs:273`.
   - Compare-op mapping: `compile.rs:66` and `decompile.rs:263`.
   - `asm_words`: `parse.rs:401`, a copy of `asm::tokenize`.
   - VLQ readers: 3 copies.
   - Matrix types: two different `Matrix` types with opposite layouts (`to_gltf.rs`, `map_transform.rs`).
8. **Standardise lazy regexes on `LazyLock` statics.** Three idioms are in use now, and several regexes are compiled on every call (`memory.rs:702`, `villagers.rs`, `disc.rs`, `mods.rs:294`).

## Tier 3 — long functions and module structure

| Function | Location | Lines |
|---|---|---|
| `build_set` | `dc2tk-assets/src/motions.rs:964` | 500 |
| `run_mod` | `dc2tk/src/modding.rs:739` | 400 (its `Check` arm alone is about 185) |
| `import_with` | `dc2tk-assets/src/from_gltf.rs:1476` | 323 |
| `run_build` | `dc2tk/src/modding.rs:436` | 300 |
| `tokenize_with` | `dc2tk-script/src/parse.rs:695` | 299 |
| `patches::merged` | `dc2tk-script/src/patches.rs:590` | 279 |
| `tree::build` | `dc2tk-assets/src/tree.rs:759` | 235 |
| `tree_check::check` | `dc2tk-assets/src/tree_check.rs:71` | 230 (5 copy-paste blocks) |
| `replace::line` | `dc2tk-script/src/replace.rs:303` | 221 |
| `replace::apply_with` | `dc2tk-script/src/replace.rs:1071` | 203 (magic stage numbers) |
| `item_data::additions` | `dc2tk-assets/src/item_data.rs:915` | 180 (7 levels deep) |
| `initialised_globals` | `dc2tk-script/src/lower.rs:96` | 173 |
| `parse_state` | `dc2tk-script/src/parse.rs:4504` | 166 |

Module splits worth doing:
- `memory.rs` (2.1k lines) into `memory/{disc,stack,mds,chara,maps,vram,spu,check}`. Its own `// ---` separators already mark the parts.
- `sdk.rs` into `sdk/{headers,resolve,present}`.
- `im3.rs` → split out `png.rs` and `quantise.rs`.
- dc2tk-script's 56 flat `pub mod`s → group them and use `pub(crate)`.
- dc2tk binary: `map_cmd::apply_maps`, `data_cmd`, `item_text` and the planning in `run_build` are library pipeline stages. Move them into dc2tk-script so they can be tested, and merge the three "image or extracted folder" types into one `DiscSource`.

God-structs and tuples:
- `replace::Line` (13 fields, kind re-derived from the key prefix) → a `LineKind` enum.
- `library::Model` (26 fields).
- `parse::Token` (per-kind fields on every token).
- Unnamed tuples: `memory.rs:1020` `(Vec<u8>, usize, bool, i64, bool)`, `ctype::takes -> (i64,i64,i64)`, `parse::parameters -> (Vec<Node>, bool, bool)`.

## Tests to add

- Malformed-input sweeps that assert no panic: every prefix plus byte flips for mds, mot, im3, soundbank, sequence, font, glTF and JSON. cargo-fuzz would be the stronger version.
- Synthetic tests for `data_table::additions`. The corpus tests (`C2S_CORPUS`) skip silently when the corpus is absent; mark them `#[ignore]` or print a skip notice.
- A golden TSV generated by CPython for dc2tk-py: float repr with the shortest-vs-nearest branch, `%g`, `int(text, 0)`. There are currently two `int(text, 0)` implementations, and they disagree on non-ASCII digits.
- Decompiler fallbacks: hand-mutated bytecode should fall back to assembly and reassemble byte-identical.
- Direct unit tests for `soundpass`, `numbers::assign`/`read_lock`, `fingerprint`, `migrate` and `renames::follow`.
- A test that `cp932_fixes::ENCODE_FIXES` is sorted and unique (`text.rs:89` binary-searches it). Also check in the generator script for it.

## Done well

- **Documentation.** Module docs explain engine behaviour and cite engine symbols and source lines (`InventTeigiStack`, `mg_texture.cpp:753`).
- **Error messages.** They say how to fix the input, name both mods in a conflict, and suggest close matches via difflib.
- **Fallbacks instead of failures.** `stb::read` falls back to raw regions, and the decompiler drops a single function to assembly instead of failing.
- **Byte-exactness.** The C89 suite is data-driven, and round-trip tests are strict (canonical VLQ, preserved padding).
- **A thin CLI.** `main.rs` dispatches to `run_*` functions and hides internal commands.
- **The `data_table` builder.** `exe_tables` shows the pattern the other tables should follow.
