// Mod map-edit: Barga's Valley edited in Blender (make_mod.py makes the edits and exports them into
// the mod as maps/s17/). This patch only adds the program a new game starts at (mod.json's
// start_at): the player stands facing the new crate, with the autumn grass around.
#pragma target map s17

#include <event.h>
#include "modkit/input.c"
#include "modkit/text.c"

// The map's "talk" point; the crate stands 30 units in front of it (make_mod.py).
#define TALK_X 68.04
#define TALK_Y 41.85
#define TALK_Z 1180.61

// The program a new game starts at (mod.json's start_at), numbered by build.
NEW_PROGRAM(MAPEDIT_START);
PROGRAM(MAPEDIT_START, start_map_edit);

void start_map_edit(void)
{
    init_event();
    // the valley's ways out lead into the story; this start is for looking around
    set_func_point_enabled("", "exit", 0);
    set_func_point_enabled("", "branch", 0);
    place_chara(0, TALK_X, TALK_Y, TALK_Z + 80.0, 3.14);
    set_camera_pos(TALK_X - 0.16, TALK_Y + 48.0, TALK_Z + 180.0);
    set_camera_target(TALK_X, TALK_Y + 20.0, TALK_Z + 60.0);
    reset_camera(CAMERA_RESET_ACTIVE);
    fade_in(30);
    modkit_wait(30);
    modkit_say("Barga's Valley, edited in Blender.\nThe grass has turned, and a crate\nstands in the way: walk into it.");
    finish_event();
}
