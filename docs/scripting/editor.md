# Editor setup

This guide explains how to configure a C editor with `clangd` to read decompiled scripts and mod patches, and how to run `clang` manually to check script syntax. For language details, see [language.md](language.md); for SDK headers, see [sdk.md](sdk.md); for mod patches, see [mods.md](../guide/mods.md).

## Configuring `clangd` for `extracted/scripts`

When you run `dc2tk setup` or `dc2tk decompile`, the tool generates a `compile_flags.txt` file at the root of `extracted/scripts/`. Any editor with `clangd` support (such as VS Code with the `clangd` extension, Neovim, Emacs, or Zed) automatically reads `compile_flags.txt` for every script in the tree. This provides:

- Code completion for all natives, inline helpers, and constants.
- Hover documentation for native signatures and descriptions.
- Jump-to-definition across scripts, libraries, and SDK headers.

`compile_flags.txt` contains absolute paths to the repository's `sdk/` and `definitions/` directories. If you move the repository to a different directory, run `dc2tk decompile` again or update the paths in `compile_flags.txt`.

### Expected warnings in retail scripts and libraries

- **Standalone scripts (`map/`, `dungeon/`, `mainchr/`)**: Parse without errors. Any remaining `clang` warnings indicate argument type mismatches present in the original retail scripts.
- **Shared libraries (`lib/`)**: When opened directly, library files may report undeclared identifier errors for global variables and functions declared in the parent scripts that `#include` them. This is normal because libraries are compiled inline inside their parent scripts.

## Configuring `clangd` for mod patches

A mod patch references functions and global variables from its target script without redeclaring them, because `dc2tk build` merges the patch into the target script at build time.

To give `clangd` full type information for a mod's patches:

1. Run `dc2tk mod override` when creating a patch, or run the following command at any time:

   ```bash
   dc2tk mod header extracted/scripts <mod-name>
   ```

2. `dc2tk` generates a hidden `.editor/` directory inside the mod containing:
   - A generated header file declaring the functions and globals of each target script.
   - A `compile_commands.json` file that configures `clangd` to parse each patch with the SDK headers, the extracted tree's headers, the `mods/` directory (for shared mod headers), and the target script's generated header.

With `.editor/compile_commands.json` present, patches show no false undeclared-identifier errors, and hovering over target script functions displays their doc comments. If you re-decompile `extracted/scripts/`, run `dc2tk mod header` again to refresh the generated headers.

## Checking script syntax with `clang`

Because `compile_flags.txt` resides at the root of `extracted/scripts/`, tools such as `clangd` and `clang-tidy` pick up the required flags automatically. To syntax-check an individual script from the command line, run:

```bash
clang -m32 -fsyntax-only -x c -std=c2x -I sdk -I definitions/pal/include -I extracted/scripts -I extracted/scripts/include -Wno-unknown-pragmas extracted/scripts/<path>.c
```

`sdk/dc2tk.h` defines the script language extensions (`string`, `any`, `PROGRAM`, `MAIN`, `wait`, `end`, `skip_end`) for standard C compilers.
