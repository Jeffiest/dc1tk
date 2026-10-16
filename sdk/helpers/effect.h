// dc2tk SDK helpers for effect scripts. Each helper stands for the native calls in its body; the
// compiler expands it in place, so a helper call produces the same bytecode as writing the calls
// out, and the decompiler folds every run it matches. Included by effect.h.
#ifndef DC2TK_HELPERS_EFFECT_H
#define DC2TK_HELPERS_EFFECT_H

// Starts a child effect and sets its first and second work vectors, which it reads with
// get_work_vector1 and get_work_vector2.
static inline void spawn_child_effect(string effect_name, float x, float y, float z, float x2, float y2, float z2)
{
    start_child_effect(effect_name);
    set_child_vector1(x, y, z);
    set_child_vector2(x2, y2, z2);
}

// Starts a child effect and sets its first work vector, usually where it appears.
static inline void spawn_child_effect_at(string effect_name, float x, float y, float z)
{
    start_child_effect(effect_name);
    set_child_vector1(x, y, z);
}

// Reads both work vectors the effect's starter passed in.
static inline void get_work_vectors(float *x, float *y, float *z, float *x2, float *y2, float *z2)
{
    get_work_vector1(x, y, z);
    get_work_vector2(x2, y2, z2);
}

// Sets the position of the effect's model and turns it to a rotation at once.
static inline void place_model(float x, float y, float z, float rot_x, float rot_y, float rot_z)
{
    set_model_pos(x, y, z);
    set_model_rot(rot_x, rot_y, rot_z);
}

// Sets the effect model's scale to the same value on all three axes and fades the model in.
static inline void fade_in_model_at_scale(float scale, float fade_speed)
{
    set_model_scale(scale, scale, scale);
    set_model_visible(1, 1, fade_speed);
}

// Gives this effect an attack from the named damage table row and sets the damage each hit deals.
static inline void create_attack_with_damage(string damage_name, int damage)
{
    create_attack(damage_name);
    set_attack_damage(damage);
}

// Sets the sprite texture, allocates count sprites, resets them and shows them all with one blend
// mode.
static inline void setup_sprites(string texture_name, int count, enum blend_mode blend_mode)
{
    set_sprite_texture(texture_name);
    allocate_sprites(count);
    reset_sprites(0, count);
    set_sprite_blend(0, blend_mode, count);
    set_sprite_visible(0, 1, count);
}

// Resets the first count sprites and shows them with one blend mode.
static inline void show_sprites(int count, enum blend_mode blend_mode)
{
    reset_sprites(0, count);
    set_sprite_blend(0, blend_mode, count);
    set_sprite_visible(0, 1, count);
}

// Writes the unit direction that a rotation in radians faces into x, y, z.
static inline void get_unit_dir_from_rot(float rot_x, float rot_y, float rot_z, float *x, float *y, float *z)
{
    get_dir_from_rot(rot_x, rot_y, rot_z, x, y, z);
    normalize_vector(x, y, z);
}

// Writes the unit vector pointing from the from point to the to point into x, y, z.
static inline void get_unit_direction(float *x, float *y, float *z, float to_x, float to_y, float to_z, float from_x, float from_y, float from_z)
{
    copy_vector(x, y, z, to_x, to_y, to_z);
    subtract_vector(x, y, z, from_x, from_y, from_z);
    normalize_vector(x, y, z);
}

// Writes the vector from the from point to the to point, divided by divisor, into x, y, z, such as
// the step that covers that distance in divisor frames.
static inline void get_offset_step(float *x, float *y, float *z, float to_x, float to_y, float to_z, float from_x, float from_y, float from_z, float divisor)
{
    copy_vector(x, y, z, to_x, to_y, to_z);
    subtract_vector(x, y, z, from_x, from_y, from_z);
    divide_vector(x, y, z, divisor);
}

// Writes the vector src_x, src_y, src_z multiplied by factor into x, y, z.
static inline void get_scaled_vector(float *x, float *y, float *z, float src_x, float src_y, float src_z, float factor)
{
    copy_vector(x, y, z, src_x, src_y, src_z);
    scale_vector(x, y, z, factor);
}

// Scales the vector held in x, y, z to the given length, keeping its direction.
static inline void set_vector_length(float *x, float *y, float *z, float length)
{
    normalize_vector(x, y, z);
    scale_vector(x, y, z, length);
}

// Turns the model to face the point x, y, z from where it stands, writing that rotation into
// rot_x, rot_y, rot_z, then moves it there.
static inline void move_model_facing(float x, float y, float z, float *rot_x, float *rot_y, float *rot_z)
{
    get_model_rot_toward(x, y, z, rot_x, rot_y, rot_z);
    set_model_pos(x, y, z);
    set_model_rot(*rot_x, *rot_y, *rot_z);
}

// Adds dx, dy, dz to the position held in x, y, z, then moves the model there facing its direction
// of travel, writing that rotation into rot_x, rot_y, rot_z.
static inline void step_model_facing(float *x, float *y, float *z, float dx, float dy, float dz, float *rot_x, float *rot_y, float *rot_z)
{
    add_vector(x, y, z, dx, dy, dz);
    get_model_rot_toward(*x, *y, *z, rot_x, rot_y, rot_z);
    set_model_pos(*x, *y, *z);
    set_model_rot(*rot_x, *rot_y, *rot_z);
}

// Copies the point held in x, y, z into old_x, old_y, old_z, then adds dx, dy, dz to it.
static inline void advance_point(float *old_x, float *old_y, float *old_z, float *x, float *y, float *z, float dx, float dy, float dz)
{
    copy_vector(old_x, old_y, old_z, *x, *y, *z);
    add_vector(x, y, z, dx, dy, dz);
}

#endif
