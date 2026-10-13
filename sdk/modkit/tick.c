// modkit tick: lets a map run code on a timer while the player plays, for monster scripts. Include
// with #include "modkit/tick.c" in a patch of the AI of a monster the map places, and call
// modkit_tick from a #pragma every_frame function, which runs before each of the monster's waits.
//
// Event scripts cannot run beside play (the player waits while an event runs), but monster scripts
// do, and a monster can ask the map for a program with run_event_script. A map program that never
// waits runs within one frame without stopping play, so it can update a caption, spawn something
// or keep score. See the modding guide, "A pattern: a caption every five seconds".

#include <monster.h>

// While save flag flag is set, asks the map for program every frames frames (250 is five seconds
// at 50 Hz). The dungeon frame timer is shared by every monster on the floor, so with several
// monsters ticking only the first to see it run out asks. The map's program should not wait.
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
