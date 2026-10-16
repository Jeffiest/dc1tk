# DC2-Toolkit

`dc2tk` is a Rust workspace (`src/crates/`). It extracts Dark Chronicle's (PAL) assets and scripts into editable forms, then rebuilds them and mods into a playable disc image. Read [src/docs/development.md](src/docs/development.md) before changing code; [src/docs/internals.md](src/docs/internals.md) maps the crates and modules.

- **The one hard rule:** a disc image the tools build must stay identical to retail wherever nothing was edited. Text outputs, reports, warnings and messages may change, as long as the change is deliberate and named in the commit message.
- **No Python legacy.** The tool was ported from Python. Don't reproduce Python behaviour. Follow C for the scripts, the engine for its files, and plain Rust everywhere else.
- **Checks before a commit:**
  - `cargo fmt --all`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
  - the corpus tests: `DC2TK_CORPUS="$PWD/extracted/disc" cargo test --release --workspace -- --include-ignored`
  - for anything that can reach the tool's output: `tools/golden.sh`, compared against a baseline taken before the change
- **Commits** go straight to `main`. The subject says what is now true, and the body says why.
- **Never run PCSX2.** Build the image, and the user plays it.
- **On Windows:**
  - Set `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=line-tables-only` on every cargo command, or debug builds fill the disk.
  - In Git Bash, `//` and doubled backslashes in arguments and heredocs get mangled.
  - Sources check out with CRLF line endings.
