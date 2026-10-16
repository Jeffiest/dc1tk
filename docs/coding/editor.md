# Editor setup

How an editor with clangd reads the decompiled scripts and a mod's patches, and how to check that
a C toolchain reads them. The language is in [the language](language.md) and the headers in
[the SDK](sdk.md); patches are in [mods](../modding/mods.md).

## clangd

`setup` and `decompile` write `compile_flags.txt` at the root of the tree.
clangd reads it for every file below, so an editor with clangd (VS Code's
clangd extension, Neovim, Emacs, Zed) knows every native, helper and
constant, with completion, hover docs and jump to definition. It points at
this checkout's `sdk/` and definition sets by absolute path, so move the
checkout and decompile again (or edit the file).

Scripts parse without errors. What clang still warns about are argument
mismatches in the retail scripts themselves. Library files show errors for
names their including scripts declare (a library can use the globals and
functions of the scripts that include it), which is expected.

## Patches

A patch uses the functions and globals of the scripts it targets without
declaring them, since `build` merges it into them. `mod override` (and
`dc2tk mod header extracted/scripts NAME` at any time) writes a header of each
patch's target into the mod's hidden `.editor/` folder, and a
`compile_commands.json` that has clangd read each patch with the SDK, the
tree's headers, the mods folder (for `#include "base/names.h"` from a mod
it requires) and its target's header first. The patch then shows no false
errors, and hover shows the doc comments of the target's functions.
Write them again after decompiling again, as the target's names can change.
A patch whose `#pragma target` lines pick several scripts gets the
declarations of one script per line.

## Checking a C toolchain can read the output

The tree's `compile_flags.txt` holds the flags, so clangd and `clang-tidy`
find them alone. By hand:

```
clang -m32 -fsyntax-only -x c -std=c2x -I sdk -I definitions/pal/include -I extracted/scripts -I extracted/scripts/include -Wno-unknown-pragmas extracted/scripts/<path>.c
```

`sdk/dc2tk.h` gives C tools a meaning for the scripts' few extensions. What clang
still reports are argument mismatches that exist in the retail scripts.
