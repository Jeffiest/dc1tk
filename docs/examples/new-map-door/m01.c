// Mod new-map-door: Palm Brinks' side of the door into the new map m05x (Palm Brinks Park II).
// make_mod.py adds the door point door_park to the town's scene; mod.json's "programs" gives it
// ENTER_PARK, which this patch adds.
#pragma target map m01

#include <event.h>
#include "modkit/text.c"
#include "park.h"

NEW_FLAG(PARK_HINT_SHOWN);

PROGRAM(ENTER_PARK, enter_park);
PROGRAM(BACK_FROM_PARK, arrive_from_park);

// The door's program: the door has opened and the screen faded out (its point's flags), so the
// player goes to the new map at its map_start, which places the player there.
void enter_park(void)
{
    jump_to_map(-1, "m05x", map_start);  // the new map, by name (or by number: MAP_M05X)
    wait;
}

// Stands the player at the new door, facing it (facing) or with the door behind (not facing), with
// the camera behind the player.
void stand_at_park_door(int facing)
{
    float yaw;
    float camera_z;

    yaw = 3.14;
    camera_z = PARK_DOOR_Z - 40.0 - 140.0;
    if (facing) {
        yaw = 0.0;
    }
    init_event();
    place_chara(CHARA_PLAYER, PARK_DOOR_X, PARK_DOOR_Y, PARK_DOOR_Z - 40.0, yaw);
    set_chara_motion(CHARA_PLAYER, MOTION_STAND_2, CHARA_MOTION_RESTART);
    update_chara_motion(CHARA_PLAYER);
    reset_chara_cloth(CHARA_PLAYER);
    set_camera_target(PARK_DOOR_X, PARK_DOOR_Y + 30.0, PARK_DOOR_Z - 40.0);
    set_camera_pos(PARK_DOOR_X, PARK_DOOR_Y + 60.0, camera_z);
}

// The town's arrival (map_start), then, the first time, the player stands at the new door and is
// told about it.
#pragma override
int play_map_start_arrival(void)
{
    int result;
    int shown;

    result = original();
    shown = get_flag(PARK_HINT_SHOWN);
    if (shown == 0) {
        set_flag(PARK_HINT_SHOWN, 1);
        stand_at_park_door(1);
        modkit_wait(30);
        modkit_say("This door now opens onto a new map,\nPalm Brinks Park II.\nWalk up to it and press (X).");
    }
    return result;
}

// Arriving from the park (its tree runs jump_to_map(-1, "m01", BACK_FROM_PARK)): the town's own
// arrival (music, sounds), then the player stands at the door, coming out of it.
void arrive_from_park(void)
{
    play_map_start_arrival();
    stand_at_park_door(0);
    modkit_wait(30);
    modkit_say("Back in Palm Brinks, through the park's door.");
}
