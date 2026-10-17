// modkit tick: runs a map event program on a repeating timer during combat.
// Include with `#include "modkit/tick.c"` in a monster AI patch, and call
// `modkit_tick` from a `#pragma every_frame` function.
//
// Because a map event program that does not execute `WAIT();` finishes in a
// single frame without pausing player control, monster scripts can use
// `run_event_script` to update live HUD captions or run periodic checks
// (see `docs/scripting/tutorials/captions.md`).

#include <monster.h>

// While save flag `flag` is non-zero, triggers map event `program` every `frames` frames (25 FPS on PAL).
void modkit_tick(int flag, int frames, int program)
{
    int on;
    int timer;

    on = get_dungeon_flag(flag);
    if (on) {
        timer = get_floor_timer();
        if (timer >= frames) {
            reset_floor_timer();
            run_event_script(program);
        }
    }
}
