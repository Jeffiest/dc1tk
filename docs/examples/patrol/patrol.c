// A made-up monster: it wanders around the point where it was placed, turns to face the player who
// comes near, swipes at one in reach, and reels back from a staggering hit.

#include <monster.h>

PROGRAM(init, on_init);
PROGRAM(update, on_update);
PROGRAM(damage, on_damage);
MAIN(on_init);

// Waits until the current motion reaches its last frame.
void wait_for_motion(void)
{
    int done;

    done = 0;
    while (!done) {
        wait;
        check_motion_end(&done);
    }
}

// Makes the spot where the monster was placed its home and gives its swipe a hit.
void on_init(void)
{
    float x;
    float y;
    float z;

    set_home_here(&x, &y, &z);
    add_attack_hit("通常攻撃", 12.0, "攻撃", 0.4, 0.7, 1);  // normal attack; attack
    loop_motion("立ち");  // stand
}

// Walks toward a random point near home until it arrives, the target comes close or three seconds
// pass.
void wander(void)
{
    float home_x;
    float home_y;
    float home_z;
    float offset_x;
    float offset_z;
    float target_dist;
    int frames;
    int arrived;

    get_home_pos(&home_x, &home_y, &home_z);
    offset_x = get_random_float(200.0);
    offset_z = get_random_float(200.0);
    loop_motion("歩き");  // walk
    move_to(home_x + offset_x - 100.0, home_y, home_z + offset_z - 100.0, 1.5, 10.0);
    frames = 180;
    arrived = 0;
    while (!arrived && frames > 0) {
        wait;
        frames = frames - 1;
        arrived = check_move_end();
        target_dist = get_target_dist();
        if (target_dist < 150.0) {
            break;
        }
    }
    stop_moving();
    loop_motion("立ち");  // stand
}

// Each frame: swipe at a target in reach, turn toward one that is near, or wander.
void on_update(void)
{
    float target_dist;
    float angle;

    target_dist = get_target_dist();
    if (target_dist < 40.0) {
        reset_and_play_motion("攻撃", CHARA_MOTION_HOLD);  // attack
        wait_for_motion();
        loop_motion("立ち");  // stand
    } else if (target_dist < 150.0) {
        start_turning_to_target(&angle, 0.1);
        wait;
    } else {
        wander();
    }
}

// Reels back from a staggering hit and waits until the reaction ends.
void on_damage(void)
{
    stop_moving();
    play_knockback("ダメージ", CHARA_MOTION_HOLD, 3.0, 0.2, 10);  // hurt
    wait_for_motion();
    loop_motion("立ち");  // stand
}
