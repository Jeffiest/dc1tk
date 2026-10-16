# Creating Mods

This guide explains how to create and manage mods in `dc2tk`, configure `mod.json`, patch scripts, allocate new IDs, replace game assets, run unit tests, and test scenes quickly.

For setup instructions, see [Getting Started](README.md). For details on editing `.c` files, see [Editing Scripts](scripts.md). For a list of all CLI commands, see [CLI Reference](commands.md). For details on how the game executes scripts, see [How Scripts Run](../internals/running.md).

## Mod folders

Each mod lives in its own subfolder inside `mods/` (for example, `mods/castle/`). Keep your extracted source tree (`extracted/scripts/`) unmodified and apply one or more mods when you build an ISO:

```sh
dc2tk mod override extracted/scripts castle --map s51 start_castle_battle
dc2tk build extracted/scripts dc2.iso -o modded.iso --mod castle --mod other
```

Pass `--mods DIR` if you store your mods in a folder other than `mods/`. Because mods can contain decompiled game scripts, the `mods/` directory is ignored by Git in this repository.

### Example mods

Two working example mods are included in `docs/scripting/examples/`:

- **[calculator](../scripting/examples/calculator/calculator.c)**: An in-game calculator with a custom 3D model, 2D textures, sound effects, and a `start_at` configuration in `mod.json`. Build it with:
  ```sh
  dc2tk build extracted/scripts dc2.iso -o calculator.iso --mod calculator --mods docs/scripting/examples
  ```
- **[spinning-cult](../scripting/examples/spinning-cult/README.md)**: A full mod that adds a custom map, a door leading to it, a custom character, a new item, modified data tables, and custom background music. Read its README for a step-by-step walkthrough of how it was built.

## Patches

A mod uses **patches** to modify game scripts. A patch is a `.c` file inside `mods/<name>/` that identifies which target scripts it modifies and contains only the functions you want to replace or add.

Because patches contain only your changes, multiple mods can patch the same script, and your patches continue to work even if you re-extract `extracted/scripts` after a `dc2tk` update.

```c
// mods/castle/s51.c
#pragma target map s51          // Targets s51_1.c through s51_5.c (all 5 languages)

#include <event.h>
#include <disc.h>

NEW_FLAG(FIGHT_ON);             // Allocates a free save flag for this mod

#pragma override                // Replaces start_castle_battle in the target script
void start_castle_battle(void)
{
    original();                 // Calls the original start_castle_battle function
    set_flag(FIGHT_ON, 1);
}
```

### Patch pragmas and declarations

- **`#pragma target`**: Selects which scripts the patch applies to. You can use four target types:
  - `#pragma target map s51`: All language variants of map `s51`.
  - `#pragma target script dungeon/monster/e33a.c`: Specific script paths (without readable suffixes; supports `*` and `?` wildcards).
  - `#pragma target monster golem`: A monster kind's AI script (using its short name or `MONSTER_GOLEM` from `disc.h`).
  - `#pragma target library lib/...`: Every script that includes the specified shared library.
  
  A patch file can contain multiple `#pragma target` lines. Any `.c` file in the mod folder that does not have a `#pragma target` line acts as a shared library file that your patches can `#include`.

- **`#pragma override`**: Placed directly before a function definition to replace the function of the same name in each target script.
  - This replaces the function only for the targeted scripts, even if the target script originally included that function from a shared library in `lib/` (you do not need to run `dc2tk fork`).
  - Call `original(...)` inside your function to run the original function that was replaced.
  - Multiple mods can override the same function as long as the later mod calls `original()`. If two mods override the same function and neither calls `original()`, `dc2tk build` stops with an error and names both mods.
  - **Function fingerprints**: `mod override`, `mod check`, and `build` save the bytecode fingerprint of each overridden function in `mods/<name>/fingerprints.json`. If a future definition update renames that function in `extracted/scripts`, `build` and `mod check` automatically locate it by its fingerprint, print the new name, and compile the override. Run `dc2tk mod check extracted/scripts <mod> --rename` to update the function name inside your patch file. If multiple functions in the target script have the exact same bytecode fingerprint, `dc2tk` lists them so you can choose which one to override.

- **`#pragma override NATIVE`**: Placed before a function definition to wrap an engine native function (for example, `place_monster`). Every call that the target scripts, their included libraries, or their patches make to that native will call your wrapper function instead. Inside the wrapper, call `original(...)` to invoke the actual engine native. See [Editing Scripts](scripts.md#changing-what-scripts-ask-the-engine-for) for more background.

  ```c
  #pragma target map s51
  #include <event.h>
  #include <disc.h>

  #pragma override place_monster
  void place_rats(monster_kind monster, float x, float y, float z, float rot_x, float rot_y, float rot_z,
                  int talk_param)
  {
      if (monster == MONSTER_GRIFFON_SOLDIER_523) {
          monster = MONSTER_SEWER_RAT;
      }
      original(monster, x, y, z, rot_x, rot_y, rot_z, talk_param);
  }
  ```

  Follow these rules when writing a native wrapper:
  - The wrapper function must declare all parameters of the native (including `OPTIONAL` parameters) and match the native's return type (or `void`).
  - When a script calls the native without optional arguments, the wrapper receives `0` for those omitted parameters, and `original(...)` forwards only the arguments that the original call provided. For example, `place_monster(-1)` (which places random dungeon floor monsters) still passes only one argument to the engine.
  - Give your wrapper function a distinct name (such as `place_rats` above). Although `dc2tk` accepts naming the wrapper `place_monster`, C language servers such as `clangd` warn when a function name matches an SDK native declaration.
  - Wrapping a native changes only the call sites in the script and appends the wrapper at the end of the compiled file (one copy per argument count used). You can also call the wrapper function directly by its own name.
  - Wrappers from multiple mods chain automatically: the later mod's wrapper runs first, and its `original()` calls the earlier mod's wrapper. Any direct call to the native inside the wrapper itself goes straight to the engine native.

- **`#pragma every_frame`**: Placed before a `void` function with no parameters. `build` inserts a call to this function before every `wait;` statement in the target script, so your function runs once per frame no matter where the script is waiting (for example, to update a custom timer in a monster script). An `every_frame` function must not contain a `wait;` statement.

- **New functions and global variables**: Any function or global variable without `#pragma override` is added to the target script.
  - If the target script already defines a symbol with that name, `build` stops with an error (since you likely intended to use `#pragma override`).
  - If two mods add a helper function or global variable with the same name, `build` renames one internally so the mods do not conflict.
  - Mod library files can also declare global variables.
  - Patches support the full C feature set of `dc2tk` (see [C Standard](../scripting/c-standard.md)): structs, unions, enums, typedefs, initialized global arrays and structs, function pointers, variadic functions, `static` functions and locals, `#pragma std c89`, and `#if` checks against SDK header macros. Defining a struct with different fields than a struct of the same name in the target script is rejected.

- **`PROGRAM` declarations**: Add new event programs to the target script. To change an existing event program, use `#pragma override` on the function that the existing program runs.

## Numbers of a mod's own

When your mod needs a new save flag, save counter, event program ID, or shared monster variable, use these macros in any `.c` or `.h` file in your mod (outside `files/`):

- `NEW_FLAG(name)`: Allocates an unused save flag (from `1024` upward; note that flag `2047` is reserved for `build --start-at`).
- `NEW_COUNTER(name)`: Allocates an unused 16-bit save counter (from `32` to `127`).
- `NEW_PROGRAM(name)`: Allocates an unused event program number (from `9000` upward).
- `NEW_MONSTER_VAR(name)`: Allocates an unused shared monster variable (from `8` to `135`).

`dc2tk build` automatically selects numbers that are not used by the retail game, the engine, or any other mod in `mods/` (including manually written numbers).

Allocated numbers are saved in `mods/numbers.json`. Because `dc2tk` remembers these assignments across builds, player save files stay valid when you add or remove mods. Even if you leave a mod out of a build, its numbers remain reserved in `mods/numbers.json`.

Use the declared name wherever a number is expected:
- In a map patch: `PROGRAM(SHOW_CAPTION, show_caption);`
- In a monster patch: `run_event_script(SHOW_CAPTION);`
- In `mod.json`: `"start_at": "s51:SHOW_CAPTION"`

> **Note**
> Whole script files inside `mods/<name>/files/` are compiled without automatic ID allocation. Whole files cannot declare or reference `NEW_FLAG`, `NEW_COUNTER`, `NEW_PROGRAM`, or `NEW_MONSTER_VAR`.

For details on how the retail game uses flags, counters, and monster variables, see [Shared state](../internals/running.md#shared-state).

## Text in each language

When `build` merges a patch into a script, it sets the `LANGUAGE` constant to the language of that target file:

- `LANGUAGE_ENGLISH` (`s51_1.c`) through `LANGUAGE_SPANISH` (`s51_5.c`) for map scripts.
- `-1` for scripts that do not have language variants.

You can check `LANGUAGE` in `#if` preprocessor blocks or in C `if` statements so a single patch file can hold dialogue text for all five languages.

## Mods that build on mods

You can make one mod depend on another or control the order in which mods are applied in `mod.json`:

- **`"requires": ["base"]`**: Declares that this mod requires the `base` mod.
  - Selecting this mod on the command line automatically includes `base` in the build and applies `base` first.
  - Your mod can include headers and libraries from `base` by prefixing the path with the mod folder name (for example, `#include "base/captions.h"` and `#include "base/show.c"`) and can use any `NEW_FLAG` or `NEW_PROGRAM` constants declared by `base`.
  - If both mods override the same function, your mod's override runs first, and calling `original()` runs the override from `base`.
  - If a required mod is missing from `mods/`, `build` stops with an error.
- **`"after": ["other"]`**: Controls load order without requiring `other`. If both mods are selected in the same build, this mod is applied after `other`.

If neither `"requires"` nor `"after"` specifies an order between two mods, `build` applies them in the order you listed them on the command line. Circular dependencies produce an error naming the mods involved.

### Example: Sharing a library mod

Suppose you create a `captions` mod containing `captions.h` (which declares `NEW_FLAG(CAPTIONS_ON);`) and a helper library `show.c`. Another mod can list `{"requires": ["captions"]}` in its `mod.json` and include those files directly:

```c
#pragma target map s17
#include <event.h>
#include "captions/captions.h"
#include "captions/show.c"
```

### How mods merge and when conflicts happen

When you build multiple mods together, `dc2tk` merges their changes automatically wherever possible:
- Multiple patches targeting the same script are merged.
- Multiple overrides of the same function are chained (when the later override calls `original()`).
- JSON data table edits are merged field by field.

If two mods change the same data table field to the exact same value, `build` keeps that value. If two mods change the same field to different values, `build` (and `mod check --disc`) stops with an error naming the field and both mods:

```text
d01f01 room 0 rate is given two ways, by mod a and mod b
```

Similarly, two mods cannot replace the same asset ID, the same texture or sound entry, or the same raw disc file.

## The mod commands

- **`dc2tk mod override SOURCE NAME --map s51 FUNCTION...`** (or `--script`, `--monster`, `--library`): Creates a new patch file (such as `mods/NAME/s51.c`) or appends to an existing one.
  - Copies the specified functions from the target script (using the English variant for map scripts) into the patch under `#pragma override`, along with the `#pragma target` line and `#include` headers.
  - Automatically runs `mod header` to generate editor support files.
  - If a map's other language variants have different lines in that function (such as translated dialogue strings), `mod override` wraps the differing lines in `#if LANGUAGE == ...` blocks (or emits full per-language copies if the function structure differs across languages).
  - If you pass an engine native name (such as `place_monster`) instead of a script function name, `mod override` generates a native wrapper (`place_monster_wrapper`) with the native's parameters, doc comment, and an `original(...)` call.
- **`dc2tk mod header SOURCE NAME`**: Generates `.editor/` header files (such as `mods/NAME/.editor/s51.h`) containing all function declarations, doc comments, library signatures, global variables, and `PROGRAM` tables from each patch's target script. It also writes `mods/NAME/compile_commands.json` so C editors using `clangd` recognize all target script symbols inside your patches. These files are only for code editors and do not affect `dc2tk build`. Run `mod header` again if you re-decompile the script tree (see [Editor Setup](../scripting/editor.md)).
- **`dc2tk mod check SOURCE NAME...`**: Compiles every script modified by the named mods without needing an ISO file. It prints any required mods added automatically, the mod application order, allocated ID numbers, and any overridden functions that were renamed in the script tree (pass `--rename` to update those function names in your patch files).
  - Pass `--disc DISC` (either `dc2.iso` or `extracted/disc`) to also verify that all asset IDs in `mod.json` exist, check that no two mods replace the same asset or data field, and verify all file paths loaded by your mod's scripts (`load_chara`, `load_sound`, `load_message_file`, `load_image`, `start_background_load`, and `info.cfg` inside character packs). If a script loads a file path that does not exist on the disc or in the mod, `mod check --disc` prints a warning (ignoring broken file loads that already exist in the unmodified retail game).
- **`dc2tk build ... --mod NAME`**: Applies the named mod during the ISO build (repeat `--mod` to include multiple mods). Lists each mod's patches and allocated IDs without modifying `extracted/scripts`.
- **`dc2tk mod list SOURCE`**: Lists every mod in `mods/`, showing its patches, overridden and added functions, `"requires"` and `"after"` settings, allocated IDs, and whole files.
- **`dc2tk mod test SOURCE NAME`**: Runs the unit tests in `mods/NAME/tests.c` (see [Tests](#tests)).

## Whole files

In addition to patches, a mod can replace or add complete `.c` script and library files under `mods/NAME/files/`, using the same folder structure as `extracted/scripts/`.

Use **patches** when changing existing retail scripts. Use **whole files** when adding brand-new scripts or mod-specific libraries.

- **`dc2tk mod new SOURCE NAME FILE...`**: Copies the specified files from `extracted/scripts` into `mods/NAME/files/` so you can edit them there (existing files in the mod are not overwritten). It also writes `mods/NAME/files/compile_flags.txt` so your code editor resolves libraries from the mod first and `extracted/scripts` second.
- **`dc2tk mod save SOURCE NAME`**: Copies every file you edited or added inside `extracted/scripts` into `mods/NAME/files/`.
- **`dc2tk build`**: Compiles whole files in `mods/NAME/files/` in place of the files in `extracted/scripts`. Note that whole files cannot use `NEW_FLAG` or other auto-allocated ID macros; you must write numeric IDs manually in whole files. Two mods in the same build cannot replace the same whole file. If the decompiler version has changed since a whole file was copied into the mod, `dc2tk mod list` marks that file as `stale` so you know to compare it against the new tree.
- **`dc2tk mod apply SOURCE NAME...`**: Copies a mod's whole files from `mods/NAME/files/` back into `extracted/scripts/` (patches are not copied, as patches are merged only during `build`).
- **`dc2tk mod slim SOURCE NAME`**: Converts copied whole files in `mods/NAME/files/` into compact patches (`mods/NAME/s51_1.c` with `#pragma target script` or `library`).
  - For each whole file, `mod slim` generates a patch containing `#pragma override` for modified functions along with any added functions, globals, `PROGRAM` entries, `#define` macros, and `#include` lines.
  - It then compiles every affected script using the whole file and again using the new patch. If both produce the exact same compiled bytes, `mod slim` deletes the whole file and keeps the patch.
  - If the compiled bytes differ (for example, because the whole file inserted a new function *above* existing functions, deleted a function or `PROGRAM` line, modified `MAIN`, or changed an `enum`), `mod slim` keeps the whole file and prints the reason.
  - Whole files that have no changes at all are deleted. Brand-new files that do not exist in `extracted/scripts` are kept as whole files.

Note: Whole files and `#include` paths work across both named script trees (`s51_raybrandt_castle/s51_1.c`) and unnamed script trees (`s51/s51_1.c`) automatically.

## Game assets by id

To replace existing textures, 3D models, sound effects, or menu text, list their asset IDs (from [The asset tree](commands.md#the-asset-tree)) under `"replace"` in `mods/NAME/mod.json`. To add new assets, list them under `"add"`:

```json
{
  "replace": {
    "texture:e254a1": "art/soldier_green.png",
    "model:c02fuku*": "models/monica/c02b.gltf",
    "sound:SE_EV_000_130_GRIFFON_BREAK_WINDOW": {"file": "sounds/boom.wav", "volume": 90},
    "text:en/shopname": "text/en/shopname.txt",
    "file:snd2/sp/sp_000.snd": "raw/sp_000.snd"
  },
  "add": {
    "sound:SE_MOD_BEEP": {"file": "sounds/beep.wav", "bank": "SP_004"},
    "file:mods/calculator/ui.img": {"file": "ui", "like": "texture:logo_d00"},
    "file:mods/calculator/calc.chr": {"file": "model/calc.gltf", "like": "model:uki"},
    "music:BG_200": {"file": "music/fish.mid", "like": "BG_193"}
  }
}
```

- **Replacing by ID (`"replace"`)**: `dc2tk build` updates every copy of that asset across the entire disc.
  - Replacing a sound effect (or a sound bank with `bank:SP_001`) updates both `snd2/sp/` and any duplicate copy stored inside a menu pack.
- **Adding new assets (`"add"`)**:
  - `sound:SE_MOD_BEEP`: Adds a new sound effect to bank `SP_004` and defines `SE_MOD_BEEP` as a constant containing the new sound's ID number for your mod's script patches.
  - `file:...` with `"like"`: Builds a new `.img` texture pack from PNGs or a new `.chr` model pack from a glTF file, using the format and settings of the retail asset named in `"like"` (see [Textures](textures.md) and [Models](models.md)).
  - `music:BG_200` with `"like"`: Builds a new music file (`snd2/bgm/BG_200.snd`) from a MIDI file using the instrument bank of `BG_193`. Scripts can play it with `load_bgm(200); play_bgm(0);` (see [Music](music.md)).

Use these commands to manage mod assets:
- `dc2tk mod asset dc2.iso NAME ID...`: Exports the specified assets into `mods/NAME/` and adds the corresponding `"replace"` entries to `mod.json`.
- `dc2tk mod check extracted/scripts NAME... --disc dc2.iso`: Verifies that every asset ID and file path in `mod.json` is valid.
- `dc2tk mod migrate dc2.iso NAME`: Converts raw files in `mods/NAME/disc/` into asset ID entries in `mod.json`.

Two mods in the same build cannot replace the same asset ID. For architecture details, see [Design: Mods by name](../../src/docs/design.md#2-mods-by-name).

## Game files

A mod can also replace or add raw disc files by placing them inside `mods/NAME/disc/`, matching the folder layout of `extracted/disc/`:

- Place top-level files from `DATA.DAT` at their disc path (for example, `mods/NAME/disc/snd2/sp/sp_000.snd`).
- To replace a file inside a pack archive (such as a `.chr` or `.mpk`), place it inside a folder named after the pack with `.d` appended. For example, `mods/NAME/disc/chara/c01_base.chr.d/c01b.img` replaces Max's outfit texture inside `c01_base.chr`. Nested packs inside packs use `.d` folders the same way.

How raw game files work in a mod:

- **Exporting files**: Run `dc2tk mod asset dc2.iso NAME PATH...` to copy raw files or pack members from the disc into `mods/NAME/disc/`. Pass `--as FORMAT` to export a binary game file in an editable format (such as PNG or glTF); `build` converts it back automatically using the original disc file as a template.
- **Building**: `dc2tk build` writes each file into the output ISO, repacks `.d` members into their pack archives, and reads them back to verify the output. Two mods in the same build cannot replace the same disc file.
- **Adding pack members**: If you place a new filename inside a `.d` pack folder, `build` appends it to the end of that pack so existing members keep their index positions. Because the game looks up pack members case-insensitively, `build` rejects new member names that differ only in letter case from an existing member.
- **Adding new top-level files**: If you add a file at a path that does not exist on the retail disc, `build` places it in free disc space and adds a new entry to `DATA.HD4` (the boot file index). Because the game loads `DATA.HD4` into a fixed memory buffer at boot, there is room for about 7.8 KB of new index entries on the PAL disc (13 bytes plus the path string per file, or roughly 150 new top-level files). `build` prints the remaining `DATA.HD4` space and fails if the index buffer overflows. Whenever possible, place new models, textures, and sounds inside existing pack archives, where new members use zero space in `DATA.HD4`.
- **Memory checks**: Because the PlayStation 2 loads maps, models, textures, scripts, and sound banks into fixed memory buffers, `build` checks every modified file against the game's [Memory Limits](memory.md) and prints `memory:` usage lines. Many retail assets are already close to their hardware limits: Max's model packs, weapons, hats, and shoes use nearly all of their memory stacks, the largest map script is within 3,152 bytes of the 312,816-byte script limit, and the largest combination of field sound banks leaves only 5,792 bytes free.

For detailed guides on each asset type, see:

- [Textures](textures.md): Editing 2D images and textures as PNG.
- [Sounds](sounds.md): Editing sound effects as WAV.
- [Music](music.md): Editing background music as MIDI with a SoundFont.
- [Text](text.md): Editing menu and system text as UTF-8 `.txt`.
- [Models](models.md): Editing 3D character models and animations as glTF in Blender.
- [Maps](maps.md): Editing map scenes in Blender with the `dc2tk_maps` add-on.
- [Dungeons](dungeons.md): Editing dungeon floor settings, parts, and room layouts.
- [Game Data](data.md): Editing items, shops, monsters, and data tables as JSON.
- [Memory Limits](memory.md): Understanding PlayStation 2 memory budgets.

## Additions

Older mods may use the `"additions"` array in `mod.json` to add programs or libraries to a script without a `.c` patch file. `dc2tk` continues to support this format:

```json
{
  "additions": [
    {
      "map": "s17",
      "programs": {"9000": "start_calculator_valley", "treasure_chest": "use_calculator"},
      "includes": ["lib/mods/calculator/calculator.c"]
    }
  ],
  "start_at": "s17:9000"
}
```

Each entry in `"additions"` selects target scripts using `"map"` (all language variants of a map) or `"scripts"` (paths without readable suffixes, supporting `*` and `?` wildcards) and can specify:
- `"programs"`: Adds new program entries to the script. Adding a program number that the script already has is rejected (use `"wrap"` instead).
- `"includes"`: Adds `#include` files to the script. Store mod libraries under `lib/mods/NAME/` so their paths do not depend on script tree naming.
- `"wrap"`: Runs helper functions `"before"` or `"after"` an existing program (for example, `{"map_start": {"before": ["my_setup"]}}`).

## Shared libraries

The `sdk/modkit/` folder provides ready-to-use helper libraries that any script or patch can include:

- `#include "modkit/text.c"`: Functions for displaying message windows (`modkit_say`, `modkit_caption`, `modkit_text_open`, `modkit_text_show`, `modkit_text_close`).
- `#include "modkit/input.c"`: Functions for reading controller input (`modkit_wait`, `modkit_pressed`, `modkit_wait_for_button`, `modkit_wait_for_confirm`).
- `#include "modkit/tick.c"`: Frame timer helper for monster scripts (`modkit_tick(flag, frames, program)`, used in the [Captions Tutorial](../scripting/tutorials/captions.md)).

For full documentation, see [The SDK: modkit](../scripting/sdk.md#modkit). Quoted `#include "..."` directives search upward from the script's directory and then search `sdk/`.

## Tests

You can write unit tests for your mod's C logic in `mods/NAME/tests.c` and run them instantly without starting the game or building an ISO:

```sh
dc2tk mod test extracted/scripts calculator
```

How `mod test` works:
- Every function in `tests.c` with the signature `int test_...(void)` runs as a test in `dc2tk`'s built-in script VM interpreter.
- A test passes when it returns a non-zero value (`1`) and fails when it returns `0`.
- If a test fails, `dc2tk` prints any output recorded by `print(...)` calls during that test.
- Engine native functions do nothing in the test interpreter and always return `0`. To make your code testable, write pure helper functions that accept inputs (such as button states) as parameters and return a result.
- If a test executes more than 10,000,000 instructions (`vm::STEPS`, such as an infinite loop) or exceeds 10,000 nested function calls (`vm::CALLS`, such as infinite recursion), the interpreter stops the test and reports an error.

## Trying a change quickly

Use these four techniques to test your changes in seconds:

1. **Test script logic in the terminal**: Run `dc2tk mod test extracted/scripts <mod>` to test helper functions in `tests.c` without launching PCSX2 (see [Tests](#tests)).
2. **Check compilation without building an ISO**: Run `dc2tk mod check extracted/scripts <mod>` to verify that all patches compile cleanly.
3. **Rely on incremental builds**: `dc2tk build` recompiles only the scripts affected by your changes, finishing in seconds.
4. **Warp directly to your scene on New Game**: Add `"start_at"` to your mod's `mod.json` (for example, `"start_at": "s17:9000"` or `"start_at": "s51:SHOW_NEXT_CAPTION"`), or pass `--start-at MAP:PROGRAM` on the command line, and pass `--play` to launch PCSX2 immediately:
   ```sh
   dc2tk build extracted/scripts dc2.iso -o test.iso --start-at s51:story_50 --play
   ```
   Pass `--no-start-at` when you want to build a normal ISO that starts at the regular opening cutscene.

### How `--start-at` warps work

In the retail game, selecting **New Game** starts in the Underground Channel (`d01`) script at `floor_start` (`title.cpp` and `EntryEventScript` in `dng_main.cpp`).

When you build with `--start-at` (or `"start_at"` in `mod.json`), `dc2tk build` injects a small startup function at the beginning of `floor_start`. The first time `floor_start` runs, it sets save flag `2047` and warps to your target map and program. Because flag `2047` is now set, any later visits to the Underground Channel play normally.

`dc2tk build` selects the warp method based on the target map:
- **Story scenes and dungeon-style maps**: Uses `load_dungeon_map` followed by `load_event_script` (matching how the retail game warps out of dungeon 1; run `dc2tk xref extracted/scripts map <map>` to see how a map is normally loaded). Use `MAP/SCRIPT:PROGRAM` if a map has multiple scripts (for example, `--start-at s06/s06b:500`), or `MAP` alone to start at `map_start`.
- **Towns, streets, and interiors**: Because the dungeon loop ignores `jump_to_map`, any other map in the disc's active map table (`map/map<language>.cfg`, such as `map1.cfg` for English; note that `map/map.cfg` is unused by the game) is opened using `go_to_edit_mode(map_number, program)`. For example, `--start-at m05` opens Palm Brinks (`m05`) at `map_start`.

> **Warning**
> `--start-at` begins with a brand-new party and inventory. Scenes that expect later story items, party members, or save flags may not behave normally. Use `--start-at` builds for testing only. For more details on town warps, see [Testing without replaying](commands.md#testing-without-replaying).

## A first test

Here are two simple script edits you can see within the first few minutes of starting a New Game:

### 1. Change the opening letter scene

1. Open `extracted/scripts/lib/map/m/m07_palm_brinks/base_opening_scenes_v1.c`.
2. Find the function `play_opening_letter_and_storm_scene`.
3. Reverse the order of the three `show_narration_line` calls (messages `10010`, `10020`, and `10030`).
4. Change `set_sprite_color(0, 0, 0, 0, 128)` to `set_sprite_color(0, 128, 0, 0, 128)`.
5. Build and run the ISO. When you start a New Game, the opening letter is narrated in reverse order over a red background instead of a black background.

### 2. Replace the soldiers in Monica's first fight (as a mod)

You can replace the Griffon Soldiers in Monica's Raybrandt Castle battle (`s51`) using either a function override or a native wrapper:

**Option A: Override `start_castle_battle`**

1. Create a patch for `start_castle_battle` on map `s51`:
   ```sh
   dc2tk mod override extracted/scripts first-fight --map s51 start_castle_battle
   ```
2. Open `mods/first-fight/s51.c`. Replace the four `MONSTER_GRIFFON_SOLDIER_523` references with other monster constants from `extracted/scripts/include/disc.h`. Make sure to load each monster kind before placing it:
   ```c
   load_monster_data(MONSTER_SEWER_RAT, 1);
   load_monster_data(MONSTER_FROGGY, 0);
   // ...
   place_monster(MONSTER_SEWER_RAT, ...);
   ```
3. In the unmodded game, the castle Griffon Soldier's own AI script triggers the end of the battle when the fourth soldier is defeated. Other monsters do not have that custom code. To end the fight when all monsters are defeated, add this line to your patch:
   ```c
   PROGRAM(monsters_cleared, start_georama_with_starter_items);
   ```
   The game engine automatically runs program `1500` (`monsters_cleared`) when no monsters remain.
4. Build and test the fight directly (the patch applies to all five languages of `s51`):
   ```sh
   dc2tk build extracted/scripts dc2.iso -o test.iso --mod first-fight --start-at s51:500 --play
   ```

**Option B: Wrap `load_monster_data` and `place_monster`**

Instead of copying `start_castle_battle`, you can intercept the engine calls that load and place monsters on map `s51`:

1. Generate native wrappers for `load_monster_data` and `place_monster`:
   ```sh
   dc2tk mod override extracted/scripts first-fight --map s51 load_monster_data place_monster
   ```
2. In `mods/first-fight/s51.c`, add this check before `original(...)` in both wrapper functions:
   ```c
   if (monster == MONSTER_GRIFFON_SOLDIER_523) {
       monster = MONSTER_SEWER_RAT;
   }
   ```
3. Add `PROGRAM(monsters_cleared, start_georama_with_starter_items);` so the fight ends when the rats are defeated, then build with `--mod first-fight --start-at s51:500 --play`.
