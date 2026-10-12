// A made-up monster AI: it wanders around the spot where it was placed, turns to face a player who
// comes near, swipes at one in reach, and reels back from a hit.
//
// How a monster script runs: every monster on a dungeon floor runs its own copy of its AI script.
// The engine runs the init program once when the monster is set up, then runs the update program
// every frame: when update returns, it starts again on the next frame, so update is the monster's
// main loop. Other programs interrupt it when something happens to the monster (a hit, a dodge, a
// stun) and update starts again after them. `wait;` hands control back to the engine for one frame.
// The monster natives in monster.h act on the monster running the script, and its target is the
// player character it is after.

#include <monster.h>

// Animation and hit names from the monster's model file, which the game names in Japanese; the
// engine looks them up by these exact strings.
#define MOTION_STAND "立ち"
#define MOTION_WALK "歩き"
#define MOTION_ATTACK "攻撃"
#define MOTION_HURT "ダメージ"
// The damage settings a hit uses, by name.
#define HIT_NORMAL "通常攻撃"

// The programs this script answers (names from enum program in monster.h): init once at set-up,
// update every frame, damage when a hit makes the monster flinch.
PROGRAM(init, on_init);
PROGRAM(update, on_update);
PROGRAM(damage, on_damage);
MAIN(on_init);

// Waits until the motion playing on the monster reaches its last frame.
void wait_for_motion(void)
{
    int done;

    done = 0;
    while (!done) {
        wait;
        check_motion_end(&done);
    }
}

// Set-up: remember where the monster was placed, give its swipe a hitbox, and stand.
void on_init(void)
{
    float x;
    float y;
    float z;

    // Reads the monster's position into x, y, z and makes that its home, which wander() circles.
    set_home_here(&x, &y, &z);
    // A hit with the normal damage settings, radius 12 (the sphere is twice that), active from 40%
    // to 70% of the attack animation, on the model's object 1 (its swiping arm in this made-up
    // model).
    add_attack_hit(HIT_NORMAL, 12.0, MOTION_ATTACK, 0.4, 0.7, 1);
    loop_motion(MOTION_STAND);
}

// Walks toward a random point within 100 units of home, until it gets there, a target comes within
// 150 units, or 180 frames (3.6 seconds) pass.
void wander(void)
{
    float home_x;
    float home_y;
    float home_z;
    float offset_x;
    float offset_z;
    int frames;
    int arrived;

    get_home_pos(&home_x, &home_y, &home_z);
    // Random whole numbers from 0 up to 200, shifted by -100 below to land either side of home.
    offset_x = get_random_float(200.0);
    offset_z = get_random_float(200.0);
    loop_motion(MOTION_WALK);
    // Starts walking at 1.5 units a frame; the walk counts as arrived within 10 units.
    move_to(home_x + offset_x - 100.0, home_y, home_z + offset_z - 100.0, 1.5, 10.0);
    frames = 180;
    arrived = 0;
    while (!arrived && frames > 0) {
        wait;
        frames = frames - 1;
        arrived = check_move_end();
        if (get_target_dist() < 150.0) {
            break;
        }
    }
    // move_to keeps going until told otherwise.
    stop_moving();
    loop_motion(MOTION_STAND);
}

// The main loop, run every frame: swipe at a target in reach, turn toward one that is near, or
// wander about.
void on_update(void)
{
    float target_dist;
    float angle;

    target_dist = get_target_dist();
    if (target_dist < 40.0) {
        // Stops any motion and plays the attack once, holding its last frame
        // (CHARA_MOTION_HOLD), then waits for it; the hit added in on_init is live during it.
        reset_and_play_motion(MOTION_ATTACK, CHARA_MOTION_HOLD);
        wait_for_motion();
        loop_motion(MOTION_STAND);
    } else if (target_dist < 150.0) {
        // Starts turning toward the target at 0.1 radians (about 6 degrees) a frame; angle receives
        // the direction it turns to. One frame later update runs again and turns some more.
        start_turning_to_target(&angle, 0.1);
        wait;
    } else {
        wander();
    }
}

// A hit made the monster flinch: stop, play the hurt animation while sliding back (3 units a
// frame, slowing by 0.2 each frame, for 10 frames), and wait for the animation to end.
void on_damage(void)
{
    stop_moving();
    play_knockback(MOTION_HURT, CHARA_MOTION_HOLD, 3.0, 0.2, 10);
    wait_for_motion();
    loop_motion(MOTION_STAND);
}
