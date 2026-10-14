// Mod spinning-cult, a test of Brother Gill: a new game starts in Barga's Valley facing him, and
// he plays each of his motions in turn, named in a caption: idle, beckon, raise, spin, turn_head
// and vanish (after which he comes back). (X) plays them again, (O) ends the test.
//
// It is a patch of every language of map s17's script, adding the start program the mod's
// start_at warps a new game to. Gill's pack (mods/gill/gill.chr) is made by build from
// model/gill.gltf (make_gill.py), like the villager p01_01a's pack.
#pragma target map s17

#include <event.h>
#include <pal.h>
#include <disc.h>
#include "modkit/input.c"
#include "modkit/text.c"

NEW_PROGRAM(GILL_TEST);
PROGRAM(GILL_TEST, start_gill_test);

// The map's "talk" point, where the calculator example's kiosk stands.
#define SPOT_X 68.04
#define SPOT_Y 41.85
#define SPOT_Z 1180.61
#define GILL_SLOT 20
// modkit_wait counts 50 frames a second
#define SECONDS 50

// Shows the motion's name and plays it on Gill for a number of seconds: looping, or held on its
// last frame when hold is 1.
void play_gill(string name, string caption, int hold, int seconds)
{
    modkit_text_show(0, caption);
    if (hold) {
        set_chara_motion(GILL_SLOT, name, CHARA_MOTION_RESTART | CHARA_MOTION_HOLD);
    } else {
        set_chara_motion(GILL_SLOT, name, CHARA_MOTION_RESTART);
    }
    modkit_wait(seconds * SECONDS);
}

// Program GILL_TEST: load Gill on the stack that outlives the event, stand him at the spot facing
// the player, and play his motions until the player stops it.
void start_gill_test(void)
{
    int again;

    init_event();
    set_func_point_enabled("", "exit", 0);
    set_func_point_enabled("", "branch", 0);
    set_func_point_enabled("p01_s17", "invent_30001-0", 0);
    reset_memory_stack(MEMORY_STACK_SUB_CHARA);
    load_chara(MEMORY_STACK_SUB_CHARA, "mods/gill/gill.chr", "info.cfg", GILL_SLOT);
    set_chara_active(GILL_SLOT);
    place_chara(GILL_SLOT, SPOT_X, SPOT_Y, SPOT_Z, 0.0);
    set_chara_motion(GILL_SLOT, "idle");
    place_chara(0, SPOT_X, SPOT_Y, SPOT_Z + 60.0, 3.14);
    set_camera_pos(SPOT_X, SPOT_Y + 32.0, SPOT_Z + 125.0);
    set_camera_target(SPOT_X, SPOT_Y + 19.0, SPOT_Z);
    reset_camera(CAMERA_RESET_ACTIVE);
    fade_in(30);
    modkit_wait(30);
    modkit_text_open(0);
    again = 1;
    while (again) {
        play_gill("idle", "Brother Gill: idle", 0, 5);
        play_gill("beckon", "Brother Gill: beckon", 1, 4);
        play_gill("raise", "Brother Gill: raise", 1, 5);
        play_gill("spin", "Brother Gill: spin", 0, 8);
        play_gill("turn_head", "Brother Gill: turn_head", 0, 5);
        play_gill("vanish", "Brother Gill: vanish", 1, 3);
        play_gill("idle", "Brother Gill: back again", 0, 2);
        modkit_text_show(0, "(X) play the motions again\n(O) end the test");
        again = modkit_wait_for_button(PAD_CROSS | PAD_CIRCLE) & PAD_CROSS;
    }
    modkit_text_close(0);
    finish_event();
}
