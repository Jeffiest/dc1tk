// A made-up event script: two actors in a town square, and a greeting that changes on the second
// visit.

#include <event.h>

int visits;

PROGRAM(map_start, on_map_start);
PROGRAM(talk_or_dungeon_start, on_talk);
MAIN(on_map_start);

// Waits the given number of frames.
void wait_frames(int frames)
{
    while (frames > 0) {
        wait;
        frames = frames - 1;
    }
}

// Shows a message and waits until the player closes it.
void say(int line)
{
    int closed;

    show_message(0, line);
    closed = 0;
    while (!closed) {
        wait;
        closed = is_message_closed(0);
    }
}

// Points the camera at the square and puts the two actors in place.
void on_map_start(void)
{
    init_event();
    set_camera_view(10.0, 20.0, 30.0, 0.0, 15.0, 0.0);
    place_actor(0, 0.0, 0.0, 0.0, PI);
    place_actor(1, 40.0, 0.0, 0.0, 0.0);
    set_actor_face(0, "笑顔", "まばたき");  // smile; blink
    loop_actor_motion(1, "立ち");  // stand
}

// Greets the player; the second visit gets a different line.
void on_talk(void)
{
    visits = visits + 1;
    set_actor_motion(0, "手を振る", CHARA_MOTION_HOLD | CHARA_MOTION_RESTART);  // wave
    wait_frames(30);
    if (visits == 1) {
        say(100);
    } else {
        say(110);
    }
}
