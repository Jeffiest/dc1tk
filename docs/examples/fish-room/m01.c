// Mod fish-room: Palm Brinks' side of the door into the new map fishroom (The Fish Room).
// make_mod.py adds the door point door_fish to the town's scene; mod.json's "programs" gives it
// ENTER_FISH_ROOM, which this patch adds.
#pragma target map m01

#include <event.h>
#include "fishroom.h"

NEW_FLAG(FISH_ROOM_HINT_SHOWN);

PROGRAM(ENTER_FISH_ROOM, enter_fish_room);
PROGRAM(BACK_FROM_FISH_ROOM, arrive_from_fish_room);

// The door's program: the door has opened and the screen faded out (its point's flags), so the
// player goes to the new map at its map_start, which places the player there.
void enter_fish_room(void)
{
    jump_to_map(-1, "fishroom", map_start);  // the new map, by name (or by number: MAP_FISHROOM)
    wait;
}

// Stands the player at the new door, facing it (facing) or with the door behind (not facing), with
// the camera behind the player.
void stand_at_fish_door(int facing)
{
    float yaw;

    yaw = 3.14;
    if (facing) {
        yaw = 0.0;
    }
    init_event();
    place_chara(CHARA_PLAYER, TOWN_DOOR_X, TOWN_DOOR_Y, TOWN_DOOR_Z - 40.0, yaw);
    set_chara_motion(CHARA_PLAYER, MOTION_STAND_2, CHARA_MOTION_RESTART);
    update_chara_motion(CHARA_PLAYER);
    reset_chara_cloth(CHARA_PLAYER);
    set_camera_target(TOWN_DOOR_X, TOWN_DOOR_Y + 30.0, TOWN_DOOR_Z - 40.0);
    set_camera_pos(TOWN_DOOR_X, TOWN_DOOR_Y + 60.0, TOWN_DOOR_Z - 180.0);
}

// The town's arrival (map_start), then, the first time, the player stands facing the new door.
#pragma override
int play_map_start_arrival(void)
{
    int result;

    result = original();
    if (get_flag(FISH_ROOM_HINT_SHOWN) == 0) {
        set_flag(FISH_ROOM_HINT_SHOWN, 1);
        stand_at_fish_door(1);
    }
    return result;
}

// Arriving from the fish room: the town's own arrival (music, sounds), then the player stands at
// the door, coming out of it.
void arrive_from_fish_room(void)
{
    play_map_start_arrival();
    stand_at_fish_door(0);
}
