// A made-up cutscene: the player waits at the town gate, an old friend walks over, the two talk,
// and a save flag remembers that the scene has played.
//
// Read greeting.c first for how programs, `wait;` and actors work. This example adds moving
// actors and the camera over time, waiting for queued commands to finish, and fades.

#include <event.h>
// modkit_say shows a line and waits for the player (see greeting.c).
#include "modkit/text.c"

// Animation names from the characters' model files (the game names them in Japanese; see
// greeting.c).
#define MOTION_STAND "立ち"
#define MOTION_WALK "歩き"
#define MOTION_NOD "うなずく"
#define FACE_SMILE "笑顔"
#define EYES_BLINK "まばたき"

// The scene character slot the friend is in. In a town the engine puts villagers in slots 8 and
// up; a mod adding its own character would load it into a free slot with load_scene_chara first.
#define FRIEND 8

// A save flag: one bit in the save data that stays set after saving and loading. Flags 825 to 2046
// are not used by the game (`python cli.py flags decompiled` lists the free ones).
#define FLAG_REUNION_SEEN 1200

PROGRAM(map_start, play_reunion);
MAIN(play_reunion);

// Waits until the actor has carried out every move, turn and motion queued on it. Actor commands
// return at once and run over the following frames, so a scene that must wait for a walk to end
// polls check_actor_busy once a frame.
void wait_for_actor(int actor)
{
    int busy;

    busy = 1;
    while (busy) {
        wait;
        busy = check_actor_busy(actor);
    }
}

// The scene, run when the player enters the map.
void play_reunion(void)
{
    int frames;

    // Only once: the flag is set at the end of the scene.
    if (get_flag(FLAG_REUNION_SEEN)) {
        return;
    }

    // Start from a clean event state (see greeting.c).
    init_event();

    // Actor 0 drives the player's character (CHARA_PLAYER is slot 0, always the character the
    // player controls) and actor 1 the friend.
    init_chara_actor(0, CHARA_PLAYER);
    init_chara_actor(1, FRIEND);
    // The player at the gate facing +z; the friend 240 units further on, turned to face back.
    place_actor(0, 0.0, 0.0, 0.0, 0.0);
    place_actor(1, 0.0, 0.0, 240.0, PI);
    loop_actor_motion(0, MOTION_STAND);
    loop_actor_motion(1, MOTION_WALK);

    // The first shot, then a fade in from black over 30 frames. fade_in counts 60 frames to the
    // second (the PAL game converts), so 30 is half a second.
    set_camera_view(80.0, 45.0, -60.0, 0.0, 20.0, 80.0);
    fade_in(30);

    // The friend walks to 50 units from the player over 120 frames, staying on the ground (the
    // last argument, 1, makes the height follow the ground). At the same time the camera eye and
    // look-at point glide to a closer shot over the same 120 frames. Both calls only queue the
    // movement: the script goes straight on to wait_for_actor.
    move_actor_to(1, 0.0, 0.0, 50.0, 120, 1);
    move_camera_to(50.0, 35.0, -30.0, 0.0, 20.0, 25.0, 120);
    wait_for_actor(1);
    loop_actor_motion(1, MOTION_STAND);

    // The conversation. Each line waits for the player before the next. (The game's own scenes
    // put lines in speech bubbles pointing at the speaker, with place_message_bubble(0, speaker,
    // listener) before the line.) Between lines the player nods once, goes back to standing, and
    // smiles.
    modkit_say("There you are! I thought you'd forgotten.");
    play_actor_motion_then_loop(0, MOTION_NOD, MOTION_STAND);
    set_actor_face(0, FACE_SMILE, EYES_BLINK);
    modkit_say("Never. It's good to see you.");
    modkit_say("Come on, the others are waiting.");

    // Fade to black and give the fade its 30 frames before the event ends.
    fade_out(30);
    frames = 30;
    while (frames > 0) {
        wait;
        frames = frames - 1;
    }

    // Remember that the scene has played, in the save.
    set_flag(FLAG_REUNION_SEEN, 1);
}
