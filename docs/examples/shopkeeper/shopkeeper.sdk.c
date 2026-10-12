#include <event.h>

int g0;

PROGRAM(map_start, on_map_start);
PROGRAM(talk_or_dungeon_start, on_talk_or_dungeon_start);
MAIN(on_map_start);

void func_1(int a0)
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
    init_chara_actor(0, 8);
    place_actor(0, 120.0, 0.0, -40.0, PI);
    loop_actor_motion(0, "立ち");
}

void func_2(void)
{
    play_actor_motion_then_loop(0, "お辞儀", "立ち");
    add_item(150);
    set_message_item(0, 0, 150);
    func_1(320);
    set_flag(1210, 1);
}

void on_talk_or_dungeon_start(void)
{
    int flag;
    int counter;
    int flag_2;

    g0 = g0 + 1;
    flag = get_flag(1205);
    if (!flag) {
        func_1(300);
    } else {
        counter = get_counter(40);
        if (counter < 3) {
            set_counter(40, counter + 1);
            func_1(310);
        } else {
            flag_2 = get_flag(1210);
            if (!flag_2) {
                func_2();
            } else {
                func_1(330);
            }
        }
    }
    if (g0 >= 3) {
        func_1(340);
    }
}
