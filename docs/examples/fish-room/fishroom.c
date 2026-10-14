// Mod fish-room: the event script of the new map fishroom (The Fish Room), a big black room with a
// giant fish spinning in the middle. mod.json gives it as the map's "script"; build compiles it in
// every language.

#include <event.h>
#include <disc.h>
#include "fishroom.h"

PROGRAM(map_start, arrive_in_room);
PROGRAM(LEAVE_FISH_ROOM, leave_room);
MAIN(do_nothing);

void do_nothing(void)
{
}

// The map's start event, which every way in runs: the room's own tune, and the player stands just
// inside the door, facing the fish. No text: the room speaks for itself.
void arrive_in_room(void)
{
    init_event();
    allow_bgm_load();
    load_bgm(FISH_ROOM_MUSIC);
    play_bgm(0);
    place_chara(CHARA_PLAYER, ROOM_ARRIVE_X, ROOM_ARRIVE_Y, ROOM_ARRIVE_Z, 3.14);
    set_chara_motion(CHARA_PLAYER, MOTION_STAND_2, CHARA_MOTION_RESTART);
    update_chara_motion(CHARA_PLAYER);
    reset_chara_cloth(CHARA_PLAYER);
    reset_camera(CAMERA_RESET_BEHIND, 0.0, 10.0);
    fade_in(30);
}

// The room's door (point door_out, given this program by mod.json's "programs"): back to Palm
// Brinks, in front of its door, by the town's program BACK_FROM_FISH_ROOM.
void leave_room(void)
{
    jump_to_map(-1, "m01", BACK_FROM_FISH_ROOM);
    wait;
}
