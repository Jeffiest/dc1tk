# Working on dc2tk

How to change the code without breaking what it promises: the one hard rule, the checks to run,
the conventions the code follows, and the traps of developing on Windows. [internals.md](internals.md)
describes how the code is organised and [design.md](design.md) why it is built the way it is.

## The rule

**A disc image the tools build must never differ from retail where nothing was edited.** An
unedited extracted disc packs to the retail bytes, an unedited source tree compiles to the disc's
scripts (`build --full` says no script differs), unedited assets write back to their own bytes,
and a mod builds to the same image today as yesterday. Everything a player sees in PCSX2 follows
from those bytes.

Everything else may change when the change makes the tool better: the text formats of decompiled
C, JSON, headers and reports, warnings and their thresholds, messages, commands and options. A
change to them is deliberate and said in its commit message, never a side effect.

The tool started as a Python program and was ported to Rust. Nothing of the Python version is
kept for its own sake: no CPython float formatting, Python JSON layout, Python whitespace or
`int()` rules, or Python-style messages. Code follows C where the scripts are C, the engine where
a file is the engine's, and plain Rust everywhere else.

## Building

```
cargo build --release
```

The workspace's target folder is `build/` (set in `.cargo/config.toml`), so the binary is
`build/release/dc2tk` (`dc2tk.exe` on Windows); `dc2tk.bat` and `./dc2tk` at the root run it. The
release profile keeps limited debug info (`debug = 1`), so backtraces name functions and lines.

Debug builds of this workspace grow large, over 20 GB of incremental files in a day of work.
Where disk is short, build with

```
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=line-tables-only cargo test --workspace
```

(set on every command: an `export` in one Git Bash call does not carry to the next) and
`cargo clean --profile dev` when space runs out.

## Checks before a commit

1. `cargo fmt --all` (the style is in `rustfmt.toml`: 120 columns).
2. `cargo clippy --workspace --all-targets -- -D warnings`: no warnings, and no `#[allow]` without a
   reason next to it.
3. `cargo test --workspace`: the unit tests, the integration tests in each crate's `tests/`, and
   the C89 suite.
4. The corpus tests, over an extracted retail disc:
   `DC2TK_CORPUS="$PWD/extracted/disc" cargo test --release --workspace -- --include-ignored`.
   They are `#[ignore = "needs DC2TK_CORPUS"]` so a plain run skips them visibly, and they fail
   loudly when asked for without the variable. They check, among much else, that the whole disc
   round-trips (scripts, every asset kind, the data tables) and that the retail memory budgets
   still measure as `memory::RETAIL_*` say.
5. For a change that can reach what the tool writes, the golden snapshot (below).

## The golden snapshot

`tools/golden.sh` runs the release binary over the extracted disc and every mod, hashes each
file it writes and each disc image it builds, and compares two runs:

```
cargo build --release && tools/golden.sh golden/base             # a baseline, before the change
cargo build --release && tools/golden.sh golden/new golden/base  # after it
```

The comparison prints `disc images: all identical` (or `DISC IMAGE CHANGED: …`, which is never
acceptable) and then `IDENTICAL` or `DIFFERS … see diff.txt`. A difference outside the images
must be one the change meant; `diff -r golden/base/logs golden/new/logs` shows the text of the
reports and messages. It covers `pack`, `decompile` of every script, `verify`, the asset tree
(every texture, model, sound, map and data table), `maps`, `flags`, `map`, `xref`, `mod list`,
`mod check`, `mod test`, a `build` of each mod and `build --full`.

A run takes about 15 minutes and needs about 6 GB free while it writes each image. It uses the
binary as built, so build release first, and don't rebuild while it runs (or point `DC2TK` at a
copy of the binary). The retail image it builds from (`golden/packed.iso`) and a copy of the mods
(`golden/mods`) are made on the first run; `golden/` is ignored by git. Keep the image hashes of a
baseline known to match retail (`golden/retail-images/*.sha1`, copied from its `logs/`): when a
deliberate text change makes you take a new baseline, its images must still equal those.

## Tests

- **Unit tests** sit beside the code (`#[cfg(test)] mod tests`), with names that state the
  behaviour they hold: `nesting_deeper_than_the_limit_is_an_error_not_a_crash`.
- **Integration tests** in `src/crates/*/tests/` drive the libraries the way the commands do,
  on small samples the tests build themselves (`dc2tk_disc::samples`, behind the crate's
  `test-support` feature, which other crates turn on in their `[dev-dependencies]`).
- **The C89 suite** (`src/crates/dc2tk-script/tests/c89/`): one C file per language feature, each
  marked with what the compiler must do with it. [c-standard.md](../../docs/coding/c-standard.md) is its account.
- **The malformed-input sweep** (`dc2tk-assets/src/malformed.rs`) parses every prefix and
  single-byte flips of a sample of each binary format and asserts errors, never a panic. With
  `MALFORMED_TRACE` set it names each case before it runs, to find one that aborts.
- **The API pages** (`docs/coding/api/`) are written from the SDK's headers by `dc2tk api-docs`;
  `the_api_pages_are_the_ones_the_headers_give` fails when a header changed and they were not
  written again.
- **The corpus tests** (above) and the **golden snapshot** check the real disc.
- **In the game**: a change that should show in play is tried in PCSX2 by hand (`dc2tk build …
  --play`, or `dc2tk play image.iso`). Nothing automates PCSX2.

A bug fix comes with the test that would have caught it.

## Conventions

**Input is never trusted.** Files from the disc, from mods and from users' hand edits are read
with checked offsets and lengths (`dc2tk_assets::bytes`, `bytes::Reader`), sizes are checked
before anything is allocated, and every failure is an error, never a panic, a silent truncation
or a value quietly ignored. A value of the wrong type in a mod's JSON is an error naming the mod,
the file, the record and the field; a field left out means no change. `unwrap` and `expect` are
for invariants the code itself guarantees.

**Errors say where.** Each library crate has its own `Error` (`thiserror`), the binary uses
`anyhow`. A file that cannot be read or written is named in the message (`dc2tk_disc::files`
wraps `std::fs` for that, and `Error::File` carries the path). `Context` traits in
`dc2tk-assets` and `dc2tk-script` (`.context("what")`) put the thing being read in front of a
cause, and `dc2tk_script::Error::At` carries a source line. Two mods changing one thing in
different ways are refused with one message: `… is given two ways, by mod a and mod b`.

**Text the tools write** has `\n` line endings on every system (files are read with either), JSON
is serde_json's pretty form (two-space indent, non-ASCII written as itself), floats are Rust's
shortest text that reads back to the same bits (and valid C literals in C), and paths written
into files use `/`. Game files keep their own bytes, line endings included.

**Limits are named and tested.** The parser nests 200 deep (`parse::NESTING`), which fits a 1 MB
stack (the Windows main thread) even in a debug build. Recursive passes are written as thin
dispatchers so their frames stay small. The test VM stops a test after `vm::STEPS` steps or
`vm::CALLS` nested calls.

**Types over strings.** Kinds, modes, opcodes and states are enums (`stb::Op`, `stb::Mode`,
`replace::LineKind`, `numbers::NumberKind`, `soundpass::PortValue`, `map_export::PackType`, …)
and node kinds are the `node::kinds` constants. One reader per format: `maptable::rows` for map
tables, `archive::Pack` for packs, `sequence::read_vlq` for variable-length numbers,
`DiscSource` for "an image or an extracted folder".

**Comments say why.** The code explains what is not obvious, and cites the engine: the function
and source line of the ChronicleTwo decompilation a behaviour comes from (`EnterIMGFile,
mg_texture.cpp:1026`). When code moves, its comments move with it. Functions stay short enough
to read, under clippy's 100-line threshold.

## Commits

Commits go straight to `main`. A subject is a sentence saying what is now true ("A build never
writes its image over the one it reads"); the body says why, and names any deliberate change in
what the tools write.

## Environment variables

| Variable | Read by | Meaning |
|---|---|---|
| `DC2TK_ROOT` | dc2tk | The repository folder (for `sdk/`, `definitions/`, `src/natives/`) when the binary is not inside it |
| `PCSX2` | `play`, `--play` | PCSX2's executable, when not given by `--pcsx2` or found on `PATH` |
| `SOURCE_DATE_EPOCH` | the C preprocessor | The time `__DATE__` and `__TIME__` give, so builds repeat |
| `DC2TK_CORPUS` | corpus tests | An extracted retail disc (`extracted/disc`) |
| `MALFORMED_TRACE` | the malformed-input sweep | Names each case before it runs |
| `DC2TK_SHOW_LETTERS` | a font test | Draws the composed letters it checks |
| `DC2TK`, `GOLDEN_DIR`, `GOLDEN_MODS` | `tools/golden.sh` | The binary, the snapshot's folder, the mods to build |

## Developing on Windows

- In Git Bash, a program argument that starts with `//` arrives as `/`, and a doubled backslash
  in a heredoc can arrive single. Don't pass source text through shell arguments or heredocs; edit
  files directly.
- Git's `core.autocrlf` checks the sources out with CRLF line endings. Edits that match text
  exactly must allow for it; scripts that rewrite files must open them without newline
  translation.
- Each disc image is 4.7 GB. Build test images where there is room, and delete old ones.

## History

The tool was written in Python first, then ported to Rust, and for a while the Rust version
reproduced the Python one's output byte for byte. In October 2026 a review of the whole workspace
([history/2026-10-code-review.md](history/2026-10-code-review.md)) led to a series of changes
that removed that legacy (the `dc2tk-py` crate, the hidden parity commands and every
Python-shaped behaviour), split the large modules, moved the build stages from the binary into
the libraries, replaced strings with types, made malformed input an error everywhere, and moved
the memory checks onto the game's own map tables. Through all of it the disc images stayed
identical to retail, checked by the golden snapshot at every step.
