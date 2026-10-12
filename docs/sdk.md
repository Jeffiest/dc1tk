# The SDK

`sdk/<table>.h` (`event`, `monster`, `action`, `effect`, `arg`) is the
scripting SDK, `sdk/helpers/<table>.h` holds its inline helpers (each table
header includes its own), and `sdk/c2s.h` is the C prelude. A script includes its table's
header instead of naming a native table.

```c
enum program {
    // Start event of a town, field or interior map ...
    map_start = 100,
};

// Returns the 16-bit save data counter at index.
// Original name: _GET_CNT.
extern int get_counter(int index) NATIVE(16);

// Queues turning a texture animation on the actor on or off. ...
// Original name: _OBJS_TEX_ANIME.
extern void set_actor_texture_anim(int actor, int on, OPTIONAL(string anim_name)) NATIVE(471);

extern float get_target_angle(...) NATIVE(70, 15);

static inline void start_turning_to_target(float *angle, float turn_speed)
{
    *angle = get_target_angle();
    turn_to(*angle, turn_speed);
}
```

- `extern T name(params) NATIVE(n);` names and documents native `n`. A
  non-void return type is the native's last argument, passed by the
  compiler as `&variable` when the result is assigned. Pointer parameters
  are out parameters. `OPTIONAL(...)` lists trailing arguments a call may
  leave out and `...` an open-ended list. `any` accepts int, float, string
  or a pointer. Parameter types are documentation: the compiler does not
  enforce them, because retail scripts call some natives with arguments the
  engine rejects.
- `NATIVE(B, N)` records that the game's scripts call native `B + N` as base
  `B` plus `N` (monster scripts). Calls compile to that form by default;
  `AT(b)` and `DIRECT` prefixes pick another.
- `enum program { name = n, };` names program numbers, so scripts write
  `PROGRAM(map_start, on_map_start);`. Entry functions are named `on_`
  plus the program's name.
- `static inline void name(params) { ... }` is a helper: a fixed run of
  native calls the compiler expands in place. Pointer parameters are out
  parameters: the call passes `&variable`, `*p` reads or assigns it inside
  the body. The decompiler folds every run a helper matches, choosing the
  combination that leaves the fewest statements, so the bytecode stays the
  same.
- `c2s.h` is never read by the c2s compiler. It defines `string`, `any`
  (a transparent union), `NATIVE`, `OPTIONAL`, `PROGRAM`, `MAIN`, `AT`,
  `DIRECT`, `wait`, `end`, `skip_end`, `print`, `sin` and `cos` so that
  `clang -m32 -fsyntax-only -x c -std=c2x -I sdk` reads a script. `-m32`
  matches the PS2 and lets `any` hold a pointer.

Signatures accept every call the retail scripts make, so the decompiled disc
passes clang's type checks. Where a retail call disagrees with what the
engine reads, the header follows the call and the doc comment keeps the
engine's view:

- A parameter that some call passes a pointer to, or a value where the
  native writes through a pointer, or a string for a number, is `any`.
- Parameters after the fewest arguments any call passes are `OPTIONAL`;
  extra arguments some call passes become `OPTIONAL(any extraN)`.
- A native with a result also takes `...`, because some calls pass the
  result variable by value (`get_random_float(range, angle)`) or leave it
  out. The decompiler still writes `x = get_random_float(range);` wherever
  the call has the usual shape.
- An enum or value-kind parameter stays typed even where one call misuses
  it, since the type is what names the constants; the decompiler prints
  such a call as it is.
- A helper folds only where each argument's pointer-ness matches the
  helper's parameter type, so helpers never hide a mismatch.

Many of these calls come from the older native numbering described below,
and run a different native from the one their arguments suit.

The descriptions were written from each native's implementation in the
ChronicleTwo decomp. Natives with one result were turned into returning
functions unless the result is also an input (`wrap_angle(&angle)` wraps in
place) or its type is `any`.

Locals take their names from the native results they receive:
`sound = load_sound(2809);` rather than `v1 = ...`.

Monster scripts mix numberings. Scripts built for an older native table
call some natives with group 60 where the current table uses group 70.
The engine runs whatever native the number selects, and the decompiler
shows that, with a comment naming the native the call was written for:

```c
AT(60) get_pos(&target_angle);  // old numbering, meant as get_target_angle
```

The rule (old group 60 is group 70, so the native ten numbers on) fits the
arguments of all 197 such calls on the disc, and functions with a modern
twin elsewhere confirm it; `old_groups` in `src/c2s/tables/monster.json`
records it.

The scripts with those calls (`b07a_atora_yellow`, `e08a`, `e09a` and the
libraries only they use) also call groups 110, 140 and 160 where the current table
uses 120, 150 and 180: same-shaped functions elsewhere on the disc confirm
every such call. Those groups are also current groups, used correctly by
every other script, so a definition set marks the scripts built for the
older table (`"numbering": "old"` in their `scripts/` entry), and only in
them, and in libraries only they use, are these calls commented
(`old_table_groups` in `monster.json`).

Six monster scripts (`b02a1` to `b02a4`, `e236a`, `p00_23a`) call natives
by raw number with no group, from an older, smaller table; they show as
`DIRECT` calls, which no other script needs. No single offset maps that
table, but same-shaped twins pin down 18 of its numbers (raw 52 to 61 are
the current 77 to 86, for example), and `old_direct` in `monster.json`
records them. A `DIRECT` call to one of those numbers gets the same
comment anywhere; in the scripts a definition set marks `"numbering":
"raw"` (`b02a1` to `b02a7`, `e236a`, `p00_23a`) and the libraries only they
use, so does a call that lands on a native the header gives no group:

```c
DIRECT get_monster_var2(&turn_target);  // old numbering, meant as get_target_angle
set_dungeon_flag(turn_target, speed);  // old numbering, meant as turn_to
```

## Constants

Besides the enums taken from the ChronicleTwo decomp, the SDK names the
engine's own fixed numbers: `enum memory_stack` (the scene memory stacks
loads allocate from, `MEMORY_STACK_EVENT` for event data), `enum
user_chara` (Max, Monica, the Ridepod and Monica's monster form in party
data), `enum equip_slot`, `enum stream_channel`, the 512 x 416 screen
(`SCREEN_WIDTH`, `SCREEN_HEIGHT`) and `COLOR_FULL`, the 128 that is full
colour and alpha for event sprites. Where a value has no name in the
decomp (footstep sounds, quest numbers, music tracks), it stays a number.

Headers name values with C enums and `#define`s:

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

- A constant compiles to its value: `CHARA_MOTION_HOLD | CHARA_MOTION_RESTART`
  compiles exactly like `2 | 4`. A local or global with the same name
  shadows a constant, as in C.
- The decompiler names a literal only where it is an argument to an
  enum-typed parameter and the enum has exactly that value, and only
  literal for literal, so `4 | 2` prints as the two names joined by `|` but
  a single `6` stays `6`. Float `#define`s replace every float literal with
  the same bits, so `3.1415927` prints as `PI`.
- `enum program { name = number, };` names program numbers. Program names
  are constants too, so map jumps print as `jump_to_map(0, "s05", map_start)`.
- About 1,000 constants come from the ChronicleTwo decomp's own enums and
  `#define`s; where the decomp has none, values are named after what the
  engine does with them.
- An inline helper's parameter that goes straight into an enum-typed native
  parameter is typed with that enum too.

## Value kinds

`typedef any message;` declares a kind: a name for what a value means. A
kind reads as its base type, so C tools and the compiler treat it
normally, and native parameters typed with it tell the decompiler what the
value is.

- `message`, `message_file` and `directory` drive the dialogue comments,
  and `item_id` and `map_id` comment ids with the item and place names
  from the disc (see [definitions](definitions.md)).
- A kind can have constants: for a parameter of kind `K`, the decompiler
  uses the constants of every enum named `K` or starting with `K_`. That
  lets several headers contribute names for one kind. `flag` (`set_flag`,
  `get_flag`) takes its names from the SDK's `enum flag_engine` (flags the
  engine itself names) and from a definition set's own `enum flag_...`,
  such as story flags. `bgm_file` (`load_bgm`, `load_bgm_pack`) and
  `sound_set` (`load_sound_set`) take theirs from the PAL set's
  `enum bgm_file_pal` and `enum sound_set_pal`, named from the music files'
  cfg titles and the rows of `snd2/MAP_index.txt`; `system_se`
  (`play_system_se`) from `enum system_se_pal`.
- `play_se(id, n)` plays entry n of the bank `id` was read from, so the
  decompiler follows the variable within a function: after
  `id = get_system_sound_id()` or `get_sound_id(SE_BANK_BASE, &id)` (or
  `SE_BANK_BATTLE`), n prints from `system_se`, `base_se` or `battle_se`
  (`play_se(snd_id, SE_TREASURE_GET)`).

## Unregistered numbers

Natives the scripts call but the engine never registers are declared too,
as `extern void _EXT_67(...) NATIVE(60, 7);`, so the code stays valid C. At
runtime the engine prints "not found ext" and skips them.

## modkit

`sdk/modkit/` holds libraries for mods, written for this project rather than
taken from the game. Any script includes them with a quoted include, which
is looked for in the script's folders and then in `sdk/`:

| Include | For | Functions |
| --- | --- | --- |
| `"modkit/input.c"` | event scripts | `modkit_wait(frames)`, `modkit_pressed(mask)`, `modkit_wait_for_button(mask)`, `modkit_wait_for_confirm()` |
| `"modkit/text.c"` | event scripts | `modkit_text_open(window)`, `modkit_text_show(window, text)`, `modkit_text_close(window)`, `modkit_say(text)`, `modkit_caption(text)` |
| `"modkit/tick.c"` | monster scripts | `modkit_tick(flag, frames, program)` |

Button masks are the `PAD_` constants of `<event.h>`, and frames are 50 a
second on a PAL console. Text can hold the codes listed in
[the modding guide](modding.md#message-text). `modkit_tick`, called from the
AI of the monsters a map places, asks the map for a program on a timer while
the player plays; the [modding guide](modding.md) explains the pattern.
