// Mod new-map-door: the event script of the new map m05x (Palm Brinks Park II, a copy of the park
// m05, which has no script of its own). mod.json gives it as the map's "script": build compiles it
// in every language and adds it as map/m/m05/m05x.stb, where the engine loads a map's script.
// It is a whole script: its PROGRAM table, MAIN and functions.

#include <event.h>
#include <disc.h>
#include "modkit/text.c"
#include "park.h"

PROGRAM(map_start, arrive_in_park);
PROGRAM(BACK_TO_TOWN, go_back_to_town);
MAIN(do_nothing);

void do_nothing(void)
{
}

// The map's start event, which every way in runs unless it names another (jump_to_map,
// go_to_edit_mode): the player stands in front of the park's big tree, facing it.
void arrive_in_park(void)
{
    init_event();
    place_chara(CHARA_PLAYER, PARK_ARRIVE_X, PARK_ARRIVE_Y, PARK_ARRIVE_Z, 0.0);
    set_chara_motion(CHARA_PLAYER, MOTION_STAND_2, CHARA_MOTION_RESTART);
    update_chara_motion(CHARA_PLAYER);
    reset_chara_cloth(CHARA_PLAYER);
    reset_camera(CAMERA_RESET_BEHIND, 0.0, 10.0);
    fade_in(30);
    modkit_wait(30);
#if LANGUAGE == LANGUAGE_FRENCH
    modkit_say("Palm Brinks Park II: une nouvelle carte,\navec son propre script.\nL'arbre ramene a Palm Brinks: (X).");
#else
    modkit_say("Palm Brinks Park II: a new map,\nwith a script of its own.\nPress (X) at the big tree to go back.");
#endif
}

// The big tree (point dun07, given this program by mod.json's "programs"): back to Palm Brinks, in
// front of the door, by the town's program BACK_FROM_PARK.
void go_back_to_town(void)
{
    init_event();
    fade_out(30);
    modkit_wait(30);
    jump_to_map(-1, "m01", BACK_FROM_PARK);
    wait;
}
