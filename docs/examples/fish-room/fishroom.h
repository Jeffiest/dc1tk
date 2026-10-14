// Mod fish-room: what Palm Brinks' patch (m01.c) and the new map's own script (fishroom.c) share.

// Program numbers build gives the mod (kept in the mods folder's numbers.json).
NEW_PROGRAM(ENTER_FISH_ROOM);     // m01: the new door's program (mod.json gives it to point door_fish)
NEW_PROGRAM(BACK_FROM_FISH_ROOM); // m01: arriving from the fish room, in front of that door
NEW_PROGRAM(LEAVE_FISH_ROOM);     // fishroom: the room's door (point door_out, by "programs")

// Where make_mod.py puts Palm Brinks' new door: the closed door ("door10") of the house nearest to
// where Palm Brinks puts the player, whose point it takes over. Its front faces -Z.
#define TOWN_DOOR_X 139.869
#define TOWN_DOOR_Y 6.49539
#define TOWN_DOOR_Z 142.74

// The room (make_mod.py): its floor at height 0, its walls at +-900, the door in the +Z wall and
// the fish floating over the middle. The player arrives a little in front of the door, facing the
// fish (yaw 3.14 faces -Z).
#define ROOM_ARRIVE_X 0.0
#define ROOM_ARRIVE_Y 0.0
#define ROOM_ARRIVE_Z 790.0

// The room's own music: a music file the mod adds (mod.json's "music:BG_200").
#define FISH_ROOM_MUSIC 200
