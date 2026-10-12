#include <event.h>

int g0;

PROGRAM(map_start, on_map_start);
PROGRAM(talk_or_dungeon_start, on_talk_or_dungeon_start);
MAIN(on_map_start);

void func_1(int a0)
{
    while (a0 > 0) {
        wait;
        a0 = a0 - 1;
    }
}

void func_2(int a0)
{
    int message_closed;

    show_message(0, a0);
    message_closed = 0;
    while (!message_closed) {
        wait;
        message_closed = is_message_closed(0);
    }
}

void on_map_start(void)
{
    init_event();
    set_camera_view(10.0, 20.0, 30.0, 0.0, 15.0, 0.0);
    place_actor(0, 0.0, 0.0, 0.0, PI);
    place_actor(1, 40.0, 0.0, 0.0, 0.0);
    set_actor_face(0, "笑顔", "まばたき");
    loop_actor_motion(1, "立ち");
}

void on_talk_or_dungeon_start(void)
{
    g0 = g0 + 1;
    set_actor_motion(0, "手を振る", CHARA_MOTION_HOLD | CHARA_MOTION_RESTART);
    func_1(30);
    if (g0 == 1) {
        func_2(100);
    } else {
        func_2(110);
    }
}
