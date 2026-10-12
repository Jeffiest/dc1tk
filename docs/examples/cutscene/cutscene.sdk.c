#include <event.h>

PROGRAM(map_start, on_map_start);
MAIN(on_map_start);

void func_1(int a0)
{
    int actor_busy;

    actor_busy = 1;
    while (actor_busy) {
        wait;
        actor_busy = check_actor_busy(a0);
    }
}

void func_2(int a0, int a1, int a2)
{
    int message_closed;

    place_message_bubble(0, a0, a1);
    show_message(0, a2);
    message_closed = 0;
    while (!message_closed) {
        wait;
        message_closed = is_message_closed(0);
    }
}

void on_map_start(void)
{
    int v0;

    init_event();
    init_chara_actor(0, CHARA_PLAYER);
    init_chara_actor(1, 8);
    place_actor(0, 0.0, 0.0, 0.0, 0.0);
    place_actor(1, 0.0, 0.0, 240.0, PI);
    loop_actor_motion(0, "立ち");
    loop_actor_motion(1, "歩き");
    set_camera_view(80.0, 45.0, -60.0, 0.0, 20.0, 80.0);
    fade_in(30);
    move_actor_to(1, 0.0, 0.0, 50.0, 120, 1);
    move_camera_to(50.0, 35.0, -30.0, 0.0, 20.0, 25.0, 120);
    func_1(1);
    loop_actor_motion(1, "立ち");
    func_2(8, 0, 200);
    play_actor_motion_then_loop(0, "うなずく", "立ち");
    set_actor_face(0, "笑顔", "まばたき");
    func_2(0, 8, 210);
    func_2(8, 0, 220);
    fade_out(30);
    v0 = 30;
    while (v0 > 0) {
        wait;
        v0 = v0 - 1;
    }
    set_flag(1200, 1);
}
