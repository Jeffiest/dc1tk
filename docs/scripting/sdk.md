# SDK headers and modkit

The `sdk/` directory contains the C headers and runtime helper libraries for all script types:

- **Native table headers** (`sdk/event.h`, `sdk/monster.h`, `sdk/action.h`, `sdk/effect.h`, `sdk/arg.h`): Declare the engine natives, constants, and program numbers for each script type. Each script includes its corresponding table header instead of writing `#pragma natives`.
- **Inline helper headers** (`sdk/helpers/<table>.h`): Define `static inline` wrapper functions. Each table header automatically includes its matching helper header.
- **Compiler and IDE prelude** (`sdk/dc2tk.h`): Defines script language extensions for standard C compilers and language servers.
- **Freestanding C89 headers** (`sdk/limits.h`, `sdk/float.h`, `sdk/stddef.h`, `sdk/stdarg.h`): Define standard C89 types and macros for the VM (see [c-standard.md](c-standard.md)).

Every SDK header uses an `#ifndef` include guard (`DC2TK_<NAME>_H`, `DC2TK_HELPERS_<NAME>_H`, or `DC2TK_H`) to prevent duplicate inclusion.

## Header declarations

```c
enum program {
    // Start event of a town, field, or interior map ...
    map_start = 100,
};

// Returns the 16-bit save data counter at index.
// Original name: _GET_CNT.
extern int get_counter(int index) NATIVE(16);

// Queues turning a texture animation on the actor on or off. ...
// Original name: _OBJS_TEX_ANIME.
extern void set_actor_texture_anim(int actor, any on, OPTIONAL(string anim_name)) NATIVE(471);

extern float get_target_angle(...) NATIVE(70, 15);

static inline void start_turning_to_target(float *angle, float turn_speed)
{
    *angle = get_target_angle();
    turn_to(*angle, turn_speed);
}
```

- **Native declarations (`extern T name(params) NATIVE(n);`)**: Declare and document native function `n`.
  - A non-`void` return type corresponds to the native's final output parameter. When a script assigns the return value (`x = get_counter(0);`), the compiler passes `&x` as the final argument.
  - Pointer parameters (`int *`, `float *`, `void *`) represent output parameters written by the engine.
  - `OPTIONAL(...)` marks trailing arguments that callers may omit.
  - `...` marks a variable-length argument list.
  - `any` accepts `int`, `float`, `string`, or any pointer type.
  - `void *` accepts either an `int *` or `float *` output address without requiring a cast in standard C.
  - The script compiler treats parameter types as documentation and does not reject type mismatches, because several retail scripts pass mismatched argument types that the engine ignores at runtime.
- **Grouped native numbers (`NATIVE(B, N)`)**: Used in `monster.h` and `action.h` to record that retail scripts encode native `B + N` using group base `B` and offset `N`. Calls compile to that group encoding by default. Prefixing a call with `AT(b)` or `DIRECT` selects a different encoding when needed for byte-for-byte matching.
- **Program enumerations (`enum program`)**: Map symbolic program names to integer IDs so scripts can write `PROGRAM(map_start, on_map_start);`. The decompiler names entry functions `on_<program_name>`.
- **Inline helpers (`static inline void name(params) { ... }`)**: Define fixed sequences of native calls that the compiler expands inline at the call site. Pointer parameters act as output variables (`&variable` at the call site, dereferenced with `*p` inside the helper). During decompilation, the decompiler folds matching sequences of native calls into helper calls, choosing the combination that produces the fewest statements while preserving exact bytecode.
- **C toolchain prelude (`sdk/dc2tk.h`)**: Ignored by the `dc2tk` compiler, but included automatically when running `clang` or `clangd`. It defines `string`, `any` (as a `transparent_union`), `NATIVE`, `OPTIONAL`, `PROGRAM`, `MAIN`, `AT`, `DIRECT`, `wait`, `end`, `skip_end`, `print`, `sin`, and `cos` so standard C tools can type-check scripts with `clang -m32 -fsyntax-only -x c -std=c2x -I sdk` (`-m32` matches 32-bit PS2 pointer sizes). For mod patches, `dc2tk.h` also declares placeholder definitions for `NEW_FLAG`, `NEW_COUNTER`, `NEW_PROGRAM`, `NEW_MONSTER_VAR`, `original(...)`, and `LANGUAGE` constants (see [mods.md](../guide/mods.md)).

### Compatibility with retail script calls

SDK function signatures are designed so that every decompiled retail script passes `clang` type checking without errors. Where a retail script call disagrees with what the engine actually reads, the SDK signature accommodates the call while the doc comment explains the engine's actual behavior:

- Parameters that receive a pointer in some calls and a scalar or string in others have type `any`.
- Output pointer parameters that receive `int *` in some calls and `float *` in others have type `void *`.
- Parameters beyond the minimum argument count used across the disc are wrapped in `OPTIONAL(...)`. Extra unused arguments passed by retail scripts appear as `OPTIONAL(any extraN)`.
- Value-returning natives also accept `...` when retail calls pass the output variable by value (such as `get_random_float(range, angle)`) or omit it completely. Whenever a call has the standard output-pointer form, the decompiler emits `x = get_random_float(range);`.
- Enum and value-kind parameters keep their specific type even when a retail call passes an invalid argument, so symbolic constant names still resolve. If a retail call passes a pointer to an enum parameter, the decompiler emits an explicit cast such as `check_pause_flags((enum pause_flags)&rot_x, &rot_y, &rot_z)`.
- Inline helpers fold only when every argument's pointer depth matches the helper parameter type.

Some mismatched calls in monster and player action scripts originate from older native tables used earlier in the game's development (see [older native numberings](../internals/scripts.md#older-native-numberings)). At runtime, the engine executes whatever native matches the current table number, and the decompiler adds a comment noting the originally intended native:

```c
AT(60) get_pos(&target_angle);  // old numbering, meant as get_target_angle
```

All native descriptions and parameter names are derived from the engine implementation in the ChronicleTwo decompilation project. The decompiler also names local variables after the native outputs assigned to them (for example, `sound = load_sound(2809);` instead of `v1 = ...`).

For a complete list of all natives across every header, see the [API Reference](api/README.md).

## Constants

SDK headers define engine constants using C `enum` and `#define` declarations:

- `enum memory_stack`: Scene memory allocation stacks (such as `MEMORY_STACK_EVENT`).
- `enum user_chara`: Party character identifiers (Max, Monica, Ridepod, and Monster Transform).
- `enum equip_slot` and `enum stream_channel`: Equipment and audio stream slots.
- `SCREEN_WIDTH` (`512`), `SCREEN_HEIGHT` (`416`), and `COLOR_FULL` (`128`, representing full RGBA intensity for event sprites).

```c
#define PI 3.1415927

enum chara_motion_flag {
    CHARA_MOTION_PAUSE = 1,
    CHARA_MOTION_HOLD = 2,
    CHARA_MOTION_RESTART = 4,
};

extern void set_actor_motion(int actor, string motion_name,
                             OPTIONAL(enum chara_motion_flag flags, float speed, int apply_now)) NATIVE(465);
```

How the compiler and decompiler handle constants:

- Symbolic constants compile to their exact literal values (`CHARA_MOTION_HOLD | CHARA_MOTION_RESTART` compiles identically to `2 | 4`). Local or global variables with the same name shadow constants according to standard C scope rules.
- During decompilation, integer literals are replaced with enum constant names only when passed to an enum-typed parameter that has a matching enumerator. Bitwise OR expressions (`4 | 2`) format as `CHARA_MOTION_RESTART | CHARA_MOTION_HOLD`, whereas a pre-combined `6` remains `6`.
- Floating-point `#define` constants (such as `PI`) replace any float literal with identical 32-bit IEEE 754 bits.
- Program names from `enum program` are also constants, allowing map transitions to print as `jump_to_map(0, "s05", map_start)`.

## Value kinds

A **value kind** is a `typedef` of `any` (for example, `typedef any message;`) that annotates the semantic meaning of a parameter or variable while compiling as its underlying base type:

- **Annotation kinds** (`message`, `message_file`, `directory`, `item_id`, `map_id`): Instruct the decompiler to add inline comments showing the corresponding dialogue line, item name, or map title from the disc (see [definitions](../../definitions/README.md)).
- **Multi-header enum kinds** (`flag`, `bgm_file`, `sound_set`, `system_se`): For a parameter of value kind `K`, the decompiler resolves constants from every `enum` named `K` or prefixed with `K_`. This allows multiple headers to contribute constants to the same parameter type:
  - `flag` (`set_flag`, `get_flag`) combines engine flags from `enum flag_engine` in the SDK with story flags from `enum flag_...` in `pal.h`.
  - `bgm_file` (`load_bgm`, `load_bgm_pack`), `sound_set` (`load_sound_set`), and `system_se` (`play_system_se`) resolve names from `enum bgm_file_pal`, `enum sound_set_pal`, and `enum system_se_pal` in `pal.h`.
- **Scene character slots (`chara_slot`)**: Resolves engine-populated slots from `enum chara_slot_engine` (`CHARA_PLAYER`, time gate, and save point) and map-specific character slots generated in `disc.h` during full-disc decompilation:
  - Slots that a map folder always loads with the same character model use `CHARA_<MAP>_<NAME>` (such as `CHARA_I01_H01_STEVE`).
  - Slots loaded dynamically within a function resolve to `enum loaded_slots_<map>` (for example, `load_scene_chara(8, "e438_paw.chr")` names slot `8` as `CHARA_S27_PAW` until another load or delete call).
  - Town villager slots populated by the engine resolve to `enum villager_slots_<map>` when constant across the game, or receive an inline comment listing the possible villagers by chapter and time of day (see [character notes](../../definitions/pal/notes/characters.md)).
- **Context-sensitive sound effect IDs (`play_se(id, n)`)**: Because `play_se(id, n)` plays sound entry `n` from whichever sound bank handle `id` was loaded from, the decompiler performs dataflow analysis on `id` across local variables, branches, globals, function calls, and shared libraries:
  - When `id` comes from `get_system_sound_id()` or `get_sound_id(SE_BANK_BASE / SE_BANK_BATTLE, &id)`, entry `n` resolves from `system_se`, `base_se`, or `battle_se` (such as `SE_TREASURE_GET`).
  - When `id` comes from `load_sound(..., &id)`, entry `n` resolves from that sound bank's `se_<bank>` enum.
- **Event sound port tracking (`get_sound_port_bank(SND_PORT_EVENT)`)**: The decompiler tracks the active sound bank loaded into `SND_PORT_EVENT` across functions. When the specific bank cannot be determined statically in a battle BGM volume helper, entry `0` formats as `SE_EVENT_PORT_BATTLE_MUSIC`.

## Unregistered native numbers

Retail scripts occasionally call native numbers that the PS2 executable never registers. The SDK declares these as `extern void _EXT_67(...) NATIVE(60, 7);` so the decompiled scripts remain valid C. At runtime, the engine logs `"not found ext"` and ignores the call.

## modkit

`sdk/modkit/` provides reusable C libraries written specifically for mods. Include them using quotes (for example, `#include "modkit/text.c"`):

| Header / Library | Script type | Functions |
| --- | --- | --- |
| `"modkit/input.c"` | Map event scripts (`event.h`) | `modkit_wait(frames)`, `modkit_pressed(mask)`, `modkit_wait_for_button(mask)`, `modkit_wait_for_confirm()` |
| `"modkit/text.c"` | Map event scripts (`event.h`) | `modkit_text_open(window)`, `modkit_text_show(window, text)`, `modkit_text_close(window)`, `modkit_say(text)`, `modkit_caption(text)` |
| `"modkit/tick.c"` | Monster AI scripts (`monster.h`) | `modkit_tick(flag, frames, program)` |

- **Button masks**: Pass `PAD_*` constants from `<event.h>` to `modkit_pressed` and `modkit_wait_for_button`.
- **Text formatting**: Strings passed to `modkit_text_show`, `modkit_say`, and `modkit_caption` support all control codes documented in [message text](../internals/running.md#message-text).
- **Periodic combat timers (`modkit_tick`)**: Call `modkit_tick` from a `#pragma every_frame` function in a monster patch to trigger a map program at a fixed frame interval during active combat. See [Show live captions during a fight](tutorials/captions.md) and the [modkit API reference](api/modkit.md) for details.
