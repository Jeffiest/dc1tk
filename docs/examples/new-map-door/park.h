// Mod new-map-door: what Palm Brinks' patch (m01.c) and the new map's own script (m05x.c) share.

// Program numbers build gives the mod (kept in the mods folder's numbers.json).
NEW_PROGRAM(ENTER_PARK);     // m01: the new door's program (mod.json's "programs" gives it to point door_park)
NEW_PROGRAM(BACK_FROM_PARK); // m01: arriving from the park, in front of that door
NEW_PROGRAM(BACK_TO_TOWN);   // m05x: the park's big tree (point dun07, by "programs") goes back to Palm Brinks

// Where make_mod.py puts the new door: the closed door ("door10") of the house nearest to where
// Palm Brinks puts the player, whose point it takes over. Its front faces -Z.
#define PARK_DOOR_X 139.869
#define PARK_DOOR_Y 6.49539
#define PARK_DOOR_Z 142.74

// Where Palm Brinks puts the player back from a dungeon, in the park in front of its big tree
// (m01's arrival from a dungeon), which is in the park's own coordinates too.
#define PARK_ARRIVE_X -4197.9
#define PARK_ARRIVE_Y 350.0
#define PARK_ARRIVE_Z 2835.5
