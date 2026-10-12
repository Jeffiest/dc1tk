#include <monster.h>

PROGRAM(init, on_init);
PROGRAM(update, on_update);
PROGRAM(damage, on_damage);
MAIN(on_init);

void func_1(void)
{
    int is_done;

    is_done = 0;
    while (!is_done) {
        wait;
        check_motion_end(&is_done);
    }
}

void on_init(void)
{
    float x;
    float y;
    float z;

    set_home_here(&x, &y, &z);
    add_attack_hit("通常攻撃", 12.0, "攻撃", 0.4, 0.7, 1);
    loop_motion("立ち");
}

void func_2(void)
{
    float home_x;
    float home_y;
    float home_z;
    float random_float;
    float random_float_2;
    float target_dist;
    int v6;
    int move_end;

    get_home_pos(&home_x, &home_y, &home_z);
    random_float = get_random_float(200.0);
    random_float_2 = get_random_float(200.0);
    loop_motion("歩き");
    move_to(home_x + random_float - 100.0, home_y, home_z + random_float_2 - 100.0, 1.5, 10.0);
    v6 = 180;
    move_end = 0;
    while (!move_end && v6 > 0) {
        wait;
        v6 = v6 - 1;
        move_end = check_move_end();
        target_dist = get_target_dist();
        if (target_dist < 150.0) {
            break;
        }
    }
    stop_moving();
    loop_motion("立ち");
}

void on_update(void)
{
    float target_dist;
    float target_angle;

    target_dist = get_target_dist();
    if (target_dist < 40.0) {
        reset_and_play_motion("攻撃", CHARA_MOTION_HOLD);
        func_1();
        loop_motion("立ち");
    } else if (target_dist < 150.0) {
        start_turning_to_target(&target_angle, 0.1);
        wait;
    } else {
        func_2();
    }
}

void on_damage(void)
{
    stop_moving();
    play_knockback("ダメージ", CHARA_MOTION_HOLD, 3.0, 0.2, 10);
    func_1();
    loop_motion("立ち");
}
