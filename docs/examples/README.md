# Examples

Each example is a made-up script, written for these docs, shown at every
stage between bytecode and readable source. No retail code appears here.
`src/tests/test_examples.py` rebuilds every stage from the script and fails
if a file below is out of date, so these are exactly what the tools print.

## greeting

| File | What it shows |
|---|---|
| [`greeting.asm`](greeting/greeting.asm) | `cli.py disassemble`: the bytecode as text assembly |
| [`greeting.bare.c`](greeting/greeting.bare.c) | `cli.py decompile --defs none --sdk <empty>`: structure recovered, but natives keep their original Level-5 names and everything else is named by position |
| [`greeting.sdk.c`](greeting/greeting.sdk.c) | `cli.py decompile --defs none`: the SDK names natives, folds helpers (`set_camera_view`, `place_actor`, `set_actor_face`), names constants and entry points, and names locals after the natives that fill them |
| [`greeting.c`](greeting/greeting.c) | `cli.py decompile --defs definitions`: the example's definition set adds function and global names, docs and translations of the Japanese asset names |

All four compile to the same bytes.

The [definition set](greeting/definitions) was not written by hand: it is
what `cli.py defs learn greeting.c definitions` records from the
annotated script. Decompiling the bytecode with it gives the annotated
script back.

## cutscene

A one-off scene in an event script: the screen fades in on the town gate,
a friend walks over to the player while the camera follows, the two trade
a few lines in speech bubbles, and a story flag records that the scene has
played. It shows actors bound to scene characters, queued moves and motions,
and polling natives (`check_actor_busy`, `is_message_closed`) in wait loops.

| File | What it shows |
|---|---|
| [`cutscene.asm`](cutscene/cutscene.asm) | `cli.py disassemble`: the bytecode as text assembly |
| [`cutscene.bare.c`](cutscene/cutscene.bare.c) | `cli.py decompile --defs none --sdk <empty>`: the original `_OBJS_*`, `_CMRS_*` and `_EOH_*` natives written out one by one |
| [`cutscene.sdk.c`](cutscene/cutscene.sdk.c) | `cli.py decompile --defs none`: helpers fold the runs back (`init_chara_actor`, `place_actor`, `set_camera_view`, `play_actor_motion_then_loop`, `set_actor_face`) and `CHARA_PLAYER` names scene character 0 |
| [`cutscene.c`](cutscene/cutscene.c) | `cli.py decompile --defs definitions`: function names and docs, and parameters declared `chara_slot`, so calls to `say` name the player too |

## patrol

A monster AI script (`#include <monster.h>`). `init` makes the spawn point
home, `update` runs every frame to swipe at a target in reach, turn toward
one nearby or wander near home, and `damage` plays a knockback when a hit
staggers the monster. Each attack and reaction waits for its motion to end.

| File | What it shows |
|---|---|
| [`patrol.asm`](patrol/patrol.asm) | `cli.py disassemble`: the bytecode as text assembly, against the monster native table |
| [`patrol.bare.c`](patrol/patrol.bare.c) | `cli.py decompile --defs none --sdk <empty>`: monster natives by their original names, most prefixed with the native group (`AT(70)`, `AT(180)`) that a header would supply, and results read through pointers (`_GET_TARGET_DIST(&v0)`) |
| [`patrol.sdk.c`](patrol/patrol.sdk.c) | `cli.py decompile --defs none`: `monster.h` names the programs (`init`, `update`, `damage`), turns single results into return values and folds the monster helpers (`set_home_here`, `loop_motion`, `reset_and_play_motion`, `start_turning_to_target`, `play_knockback`) |
| [`patrol.c`](patrol/patrol.c) | `cli.py decompile --defs definitions`: function and local names, docs and English glosses of the motion and hit names |

## shopkeeper

A town talk handler: the `talk_or_dungeon_start` program branches on a
story flag and a save counter, gives an item once and marks it given with a
second flag, and counts talks in a global. Flags and counters live in the
save; the global starts at 0 each time the map's script loads, so it only
counts talks during this stay.

| File | What it shows |
|---|---|
| [`shopkeeper.asm`](shopkeeper/shopkeeper.asm) | `cli.py disassemble`: the bytecode as text assembly |
| [`shopkeeper.bare.c`](shopkeeper/shopkeeper.bare.c) | `cli.py decompile --defs none --sdk <empty>`: `_GET_FLAG`, `_GET_CNT` and `_ADD_ITEM` under their original names, with the global as `g0` |
| [`shopkeeper.sdk.c`](shopkeeper/shopkeeper.sdk.c) | `cli.py decompile --defs none`: natives renamed and locals named after the natives that fill them (`flag`, `counter`, `flag_2`) |
| [`shopkeeper.c`](shopkeeper/shopkeeper.c) | `cli.py decompile --defs definitions`: names that say what each flag and counter means (`bridge_repaired`, `friendship`, `gift_given`, `visits`) |

As with greeting, every stage of each example compiles to the same bytes,
and each definition set is what `cli.py defs learn` recorded from the
annotated script, plus a hand-written `strings/en.json` for the glosses.
