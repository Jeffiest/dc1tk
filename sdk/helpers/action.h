// c2s SDK helpers for action scripts. Each helper stands for the native calls in its body; the
// compiler expands it in place, so a helper call produces the same bytecode as writing the calls
// out, and the decompiler folds every run it matches. Included by action.h.
#ifndef DC2TK_HELPERS_ACTION_H
#define DC2TK_HELPERS_ACTION_H

// Stores the next combo step, clears the attack press history and sets menace level 0 for 3
// frames, as each hit of a combo does when it starts.
static inline void advance_combo(int step)
{
    set_saved_value(step);
    reset_attack_presses();
    set_menace(MENACE_MELEE, 3);
}

// Starts the next hit of a sword combo: plays its motion once from the first frame, turns toward a
// target within reach, stores the combo step, clears the attack press history and sets menace.
static inline void start_combo_hit(string motion, int step)
{
    play_motion(motion, -1, CHARA_MOTION_RESTART | CHARA_MOTION_HOLD);
    turn_to_target(2.0 * 20.0, 2.0);
    set_saved_value(step);
    reset_attack_presses();
    set_menace(MENACE_MELEE, 3);
}

// Plays a motion once at its own speed, from its first frame without blending, stopping on its
// last frame (flags 4 | 2).
static inline void play_motion_once(string motion)
{
    play_motion(motion, -1, CHARA_MOTION_RESTART | CHARA_MOTION_HOLD);
}

// Schedules a sword trail in slot 0 between the blade frames "dcol00" and "dcol01" with length 77,
// while the motion plays between start_ratio and end_ratio.
static inline void add_blade_trail(string motion_name, float start_ratio, float end_ratio, int hold_frames, int fade_frames)
{
    add_sword_trail(0, motion_name, start_ratio, end_ratio, "dcol00", "dcol01", 77, hold_frames, fade_frames);
}

#endif
