# Examples

Made-up scripts written for these docs, to show how to write each kind of
script with the SDK. No retail code appears here. Each compiles as it is
(`python3 cli.py compile docs/examples/greeting.c -o greeting.stb`), and
`src/tests/test_examples.py` checks that they all still do.

The examples define English names for the few Japanese asset names they
use (`#define MOTION_STAND "立ち"`). Inside a decompiled tree you do not
need to: `include/disc.h` defines a name for every motion, texture
animation, effect, map part and damage setting the game's scripts use, and
every script includes it.

## [greeting.c](greeting.c)

An event script for a town square: two actors placed in a camera shot, a
greeting that changes on the second visit, and a global that counts visits.
It explains programs, `MAIN`, `wait;`, actors and scene character slots,
and shows helpers (`set_camera_view`, `place_actor`, `set_actor_face`) and
your own text in a message window with `modkit_say`.

## [cutscene.c](cutscene.c)

A one-off scene: the screen fades in on the town gate, a friend walks over
to the player while the camera follows, the two trade a few lines, and a
save flag records that the scene has played. It shows actors bound to scene
characters (`CHARA_PLAYER`), queued moves and motions, waiting for them with
`check_actor_busy`, fades, and playing a scene only once.

## [patrol.c](patrol.c)

A monster AI script (`#include <monster.h>`). `init` makes the spawn point
home, `update` runs every frame to swipe at a target in reach, turn toward
one nearby or wander near home, and `damage` plays a knockback when a hit
staggers the monster. Each attack and reaction waits for its motion to end.

## [shopkeeper.c](shopkeeper.c)

A town talk handler: the `talk_or_dungeon_start` program branches on a
story flag and a save counter, gives an item once and marks it given with a
second flag, and counts talks in a global. Flags and counters live in the
save; the global starts at 0 each time the map's script loads, so it only
counts talks during this stay.
