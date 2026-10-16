// dc2tk SDK helpers for monster scripts. Each helper stands for the native calls in its body; the
// compiler expands it in place, so a helper call produces the same bytecode as writing the calls
// out, and the decompiler folds every run it matches. Included by monster.h.
#ifndef DC2TK_HELPERS_MONSTER_H
#define DC2TK_HELPERS_MONSTER_H

// Clears mask flag bits 1 and 2, so the monster can guard hits and dodge attacks again.
static inline void allow_guard_and_dodge(void)
{
    set_mask_flag(MASK_NO_GUARD, 0);
    set_mask_flag(MASK_NO_DODGE, 0);
}

// Sets mask flag bits 1 and 2, so the monster can neither guard hits nor dodge attacks.
static inline void forbid_guard_and_dodge(void)
{
    set_mask_flag(MASK_NO_GUARD, 1);
    set_mask_flag(MASK_NO_DODGE, 1);
}

// Sets (on = 1) or clears (on = 0) attribute bits 4, 256 and 8 together: walking replaces
// velocity, ground search is skipped and map collision is ignored, so a script can move the
// monster freely.
static inline void set_scripted_movement(int on)
{
    set_attrib(MONSTER_ATTRIB_SET_VELOCITY, on);
    set_attrib(MONSTER_ATTRIB_SKIP_GROUND, on);
    set_attrib(MONSTER_ATTRIB_NO_MAP_HIT, on);
}

// Stores state in monster variable 0, the behaviour bits other monster scripts read, and loops a
// motion at its own speed.
static inline void enter_state(int state, string motion)
{
    set_monster_var(0, state);
    set_motion(motion, -1, 0);
}

// Stops the current and chained motions, then plays a motion at its own speed with the given
// flags.
static inline void reset_and_play_motion(string motion, enum motion_flags flags)
{
    reset_motion();
    set_motion(motion, -1, flags);
}

// Plays a hit reaction motion at its own speed and starts knocking the monster back.
static inline void play_knockback(string motion, enum motion_flags flags, float speed, float decel, int frames)
{
    set_motion(motion, -1, flags);
    start_knockback(speed, decel, frames);
}

// Writes the monster's position and rotation into six float out parameters.
static inline void get_pos_and_rot(float *x, float *y, float *z, float *rot_x, float *rot_y, float *rot_z)
{
    get_pos(x, y, z);
    get_rot(rot_x, rot_y, rot_z);
}

// Starts an effect script and sets its first vector (usually the position) and second vector.
static inline void spawn_effect(string effect_name, float x, float y, float z, float x2, float y2, float z2)
{
    create_effect(effect_name);
    set_effect_vec1(-1, x, y, z);
    set_effect_vec2(-1, x2, y2, z2);
}

// Starts an effect script and sets its first vector, usually its position.
static inline void spawn_effect_at(string effect_name, float x, float y, float z)
{
    create_effect(effect_name);
    set_effect_vec1(-1, x, y, z);
}

// Clears the no-damage attribute, stops moving and ends the thrown state, as a monster does when
// it lands after being thrown.
static inline void recover_from_throw(void)
{
    set_attrib(MONSTER_ATTRIB_NO_DAMAGE, 0);
    stop_moving();
    end_throw();
}

// Writes the unit vector pointing from the from point to the to point into x, y, z.
static inline void get_unit_direction(float *x, float *y, float *z, float to_x, float to_y, float to_z, float from_x, float from_y, float from_z)
{
    copy_vector(x, y, z, to_x, to_y, to_z);
    subtract_vector(x, y, z, from_x, from_y, from_z);
    normalize_vector(x, y, z);
}

// Replaces the point held in x, y, z with the unit vector pointing to it from the from point.
static inline void make_direction_from(float *x, float *y, float *z, float from_x, float from_y, float from_z)
{
    subtract_vector(x, y, z, from_x, from_y, from_z);
    normalize_vector(x, y, z);
}

// Replaces the point held in x, y, z with the vector of the given length pointing to it from the
// from point.
static inline void make_offset_from(float *x, float *y, float *z, float from_x, float from_y, float from_z, float length)
{
    subtract_vector(x, y, z, from_x, from_y, from_z);
    normalize_vector(x, y, z);
    scale_vector(x, y, z, length);
}

// Moves the point held in x, y, z along the line from the from point so it lies length units from
// that point.
static inline void place_at_distance_from(float *x, float *y, float *z, float from_x, float from_y, float from_z, float length)
{
    subtract_vector(x, y, z, from_x, from_y, from_z);
    normalize_vector(x, y, z);
    scale_vector(x, y, z, length);
    add_vector(x, y, z, from_x, from_y, from_z);
}

// Scales the vector held in x, y, z to the given length, keeping its direction.
static inline void set_vector_length(float *x, float *y, float *z, float length)
{
    normalize_vector(x, y, z);
    scale_vector(x, y, z, length);
}

// Loops a motion at its own speed (speed -1, flags 0).
static inline void loop_motion(string motion)
{
    set_motion(motion, -1, 0);
}

// Loops a motion at its own speed, for scripts that pass the speed as the float -1.0.
static inline void loop_motion_f(string motion)
{
    set_motion(motion, -1.0, 0);
}

// Reads the yaw toward the target into angle and starts turning toward it at turn_speed radians
// per frame.
static inline void start_turning_to_target(float *angle, float turn_speed)
{
    *angle = get_target_angle();
    turn_to(*angle, turn_speed);
}

// Reads another monster's position into x, y, z and the yaw toward it into angle, and starts
// turning toward it at turn_speed radians per frame.
static inline void start_turning_to_monster(int monster, float *x, float *y, float *z, float *angle, float turn_speed)
{
    get_active_monster_pos(monster, x, y, z);
    *angle = get_angle_to(*x, *y, *z);
    turn_to(*angle, turn_speed);
}

// Reads the yaw toward the target into angle and sets the monster's rotation to face it at once.
static inline void snap_to_target_yaw(float *angle)
{
    *angle = get_target_angle();
    set_rot(0.0, *angle, 0.0);
}

// Reads the monster's position into x, y, z and makes it the monster's home position.
static inline void set_home_here(float *x, float *y, float *z)
{
    get_pos(x, y, z);
    set_home_pos(*x, *y, *z);
}

// Reads the home position and the monster's position, and writes the distance between them into
// dist.
static inline void get_home_distance(float *home_x, float *home_y, float *home_z, float *x, float *y, float *z, float *dist)
{
    get_home_pos(home_x, home_y, home_z);
    get_pos(x, y, z);
    *dist = get_distance_between(*x, *y, *z, *home_x, *home_y, *home_z);
}

// Writes how far the monster can see ahead along its facing, up to 100 units, before map
// collision.
static inline void measure_clear_ahead(float *clear_dist)
{
    measure_clear_distance(100.0, 0.0, clear_dist);
}

#endif
