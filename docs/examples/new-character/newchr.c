// Mod newchr: a character made in Blender (its own skeleton of seven bones, a skinned mesh, a
// texture and one motion, "wave") stands in Barga's Valley and waves.
//
// It is a patch of every language of map s17's script, adding the start program the mod's
// start_at warps a new game to. The character's pack (mods/newchr/newchr.chr) is made by build
// from model/newchr.gltf, like the villager p01_01a's pack.
#pragma target map s17

#include <event.h>
#include <pal.h>
#include <disc.h>
#include "modkit/input.c"
#include "modkit/text.c"

NEW_PROGRAM(NEWCHR_START);
PROGRAM(NEWCHR_START, start_new_character);

// The map's "talk" point, where the calculator example's kiosk stands.
#define SPOT_X 68.04
#define SPOT_Y 41.85
#define SPOT_Z 1180.61
#define NEWCHR_SLOT 20

// Program NEWCHR_START: load the new character on the stack that outlives the event, stand it
// at the spot facing the player, and start its motion.
void start_new_character(void)
{
    init_event();
    set_func_point_enabled("", "exit", 0);
    set_func_point_enabled("", "branch", 0);
    set_func_point_enabled("p01_s17", "invent_30001-0", 0);
    reset_memory_stack(MEMORY_STACK_SUB_CHARA);
    load_chara(MEMORY_STACK_SUB_CHARA, "mods/newchr/newchr.chr", "info.cfg", NEWCHR_SLOT);
    set_chara_active(NEWCHR_SLOT);
    place_chara(NEWCHR_SLOT, SPOT_X, SPOT_Y, SPOT_Z, 0.0);
    set_chara_motion(NEWCHR_SLOT, "wave");
    place_chara(0, SPOT_X, SPOT_Y, SPOT_Z + 60.0, 3.14);
    set_camera_pos(SPOT_X, SPOT_Y + 30.0, SPOT_Z + 110.0);
    set_camera_target(SPOT_X, SPOT_Y + 14.0, SPOT_Z);
    reset_camera(CAMERA_RESET_ACTIVE);
    fade_in(30);
    modkit_wait(30);
    modkit_say("A new character, made in Blender,\nis waving at you.");
    finish_event();
}
