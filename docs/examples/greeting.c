// A made-up event script for a town square: two characters stand in a camera shot, and talking to
// them gets a greeting that changes on the second visit.
//
// How an event script runs: the engine starts one of the script's programs (listed below with
// PROGRAM) when something happens on the map: the map is entered, the player talks to someone,
// steps on a trigger. The program's function runs until it returns. `wait;` hands control back to
// the engine for one frame (the game runs at 50 frames a second on a PAL console), so a loop with
// `wait;` in it is how a script waits for something without freezing the game.

// The SDK header for event scripts: it declares the engine functions (natives) a map script can
// call, helpers built from them, and constants such as PI and the program names.
#include <event.h>
// modkit is the SDK's library for mods. Its text helpers show lines you write yourself in a
// message window: modkit_say opens window 0 on the text, waits for the player to press (X) or (O)
// and closes it. (The game's own lines come from text files by number, with show_message.)
#include "modkit/text.c"

// Motion and face names. The engine looks animations up by name in the character's model file,
// and the game's model files name them in Japanese, so these strings have to be exactly those
// bytes. Giving them English names here keeps the rest of the script readable.
#define MOTION_STAND "立ち"
#define MOTION_WAVE "手を振る"
#define FACE_SMILE "笑顔"
#define EYES_BLINK "まばたき"

// A global keeps its value between programs for as long as the map's script stays loaded. It
// starts at 0 each time the map loads; anything that must survive a save goes in a save flag or
// counter instead (see shopkeeper.c).
int visits;

// The program table: which function the engine runs for each event. map_start runs when the player
// enters the map; talk_or_dungeon_start runs when the player presses the confirm button next to a
// character. The names come from enum program in event.h.
PROGRAM(map_start, on_map_start);
PROGRAM(talk_or_dungeon_start, on_talk);
// The main function the script's header names. Every script has one; event scripts usually name
// their start program.
MAIN(on_map_start);

// Waits the given number of frames. `wait;` ends this frame and carries on from here on the next.
void wait_frames(int frames)
{
    while (frames > 0) {
        wait;
        frames = frames - 1;
    }
}

// Sets the scene up when the player enters the map.
void on_map_start(void)
{
    // Clears what an earlier event may have left behind (queued actor commands, sprites, effects)
    // and stops the camera following the player. Call it at the start of every event.
    init_event();

    // Actors are the event system's puppets: each one drives an object (here, a character) through
    // a queue of commands, and runs them over the following frames while the script goes on.
    // Characters live in numbered scene slots. In a town the engine fills slots 8 and up with the
    // map's villagers; a scene can also load its own character into a slot first, with
    // load_scene_chara(slot, "path/to/model.chr"). Here actor 0 drives the character in slot 8 and
    // actor 1 the one in slot 9.
    init_chara_actor(0, 8);
    init_chara_actor(1, 9);

    // Camera eye at (10, 20, 30), looking at (0, 15, 0). Positions are in world units; y is up.
    set_camera_view(10.0, 20.0, 30.0, 0.0, 15.0, 0.0);
    // Actor 0 at the origin, turned half a circle (angles are in radians: PI is 180 degrees).
    place_actor(0, 0.0, 0.0, 0.0, PI);
    place_actor(1, 40.0, 0.0, 0.0, 0.0);
    // Actor 0 smiles and blinks (face and eye animations on the model's textures).
    set_actor_face(0, FACE_SMILE, EYES_BLINK);
    // Actor 1 plays its standing animation on a loop.
    loop_actor_motion(1, MOTION_STAND);
}

// Greets the player; the first visit and later ones get different lines.
void on_talk(void)
{
    visits = visits + 1;
    // Waves once and holds the last frame: CHARA_MOTION_RESTART starts the motion from its first
    // frame, CHARA_MOTION_HOLD stops it on its last frame instead of looping.
    set_actor_motion(0, MOTION_WAVE, CHARA_MOTION_HOLD | CHARA_MOTION_RESTART);
    // Lets the wave play for 30 frames (0.6 seconds) before the message opens.
    wait_frames(30);
    if (visits == 1) {
        modkit_say("Hello! First time in the square?");
    } else {
        modkit_say("Back again? Welcome!");
    }
}
