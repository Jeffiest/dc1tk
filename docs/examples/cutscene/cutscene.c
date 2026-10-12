// A made-up cutscene: the player waits at the town gate, an old friend walks over to meet him, and
// a story flag remembers that the scene has played.

#include <event.h>

PROGRAM(map_start, play_reunion);
MAIN(play_reunion);

// Waits until the actor has run every move, turn and motion queued on it.
void wait_for_actor(int actor)
{
    int busy;

    busy = 1;
    while (busy) {
        wait;
        busy = check_actor_busy(actor);
    }
}

// Shows a line in a speech bubble pointing from the speaker to the listener, and waits until the
// player closes it.
void say(chara_slot speaker, chara_slot listener, int line)
{
    int closed;

    place_message_bubble(0, speaker, listener);
    show_message(0, line);
    closed = 0;
    while (!closed) {
        wait;
        closed = is_message_closed(0);
    }
}

// Fades in on the gate, walks the friend (scene character 8) over to the player, plays their
// exchange and fades out.
void play_reunion(void)
{
    int frames;

    init_event();
    init_chara_actor(0, CHARA_PLAYER);
    init_chara_actor(1, 8);
    place_actor(0, 0.0, 0.0, 0.0, 0.0);
    place_actor(1, 0.0, 0.0, 240.0, PI);
    loop_actor_motion(0, "立ち");  // stand
    loop_actor_motion(1, "歩き");  // walk
    set_camera_view(80.0, 45.0, -60.0, 0.0, 20.0, 80.0);
    fade_in(30);
    move_actor_to(1, 0.0, 0.0, 50.0, 120, 1);
    move_camera_to(50.0, 35.0, -30.0, 0.0, 20.0, 25.0, 120);
    wait_for_actor(1);
    loop_actor_motion(1, "立ち");  // stand
    say(8, CHARA_PLAYER, 200);
    play_actor_motion_then_loop(0, "うなずく", "立ち");  // nod; stand
    set_actor_face(0, "笑顔", "まばたき");  // smile; blink
    say(CHARA_PLAYER, 8, 210);
    say(8, CHARA_PLAYER, 220);
    fade_out(30);
    frames = 30;
    while (frames > 0) {
        wait;
        frames = frames - 1;
    }
    set_flag(1200, 1);
}
