// c2s SDK. Natives for effect scripts (dungeon/eff_script, dungeon/articles, dungeon/sphida).
//
// Natives are engine functions, declared extern with their number in
// NATIVE(...). A native with one result returns it (count = get_counter(3););
// natives with several results take pointers (get_actor_pos(0, &x, &y, &z);).
// Natives push nothing on the VM stack: a native in an expression is called
// first, into a hidden local, and the expression reads that.
// OPTIONAL(...) lists arguments a call may leave out. Natives never wait; scripts
// wait with `wait;`, which yields one frame (60 per second). While the player
// skips a cutscene the engine does not run natives until the script reaches
// `skip_end;`.
//
// NATIVE(B, N) means the game's scripts call the native as B + N. A call can
// pick another form with an AT(B) or DIRECT prefix.

#ifndef DC2TK_EFFECT_H
#define DC2TK_EFFECT_H

#include <c2s.h>
#pragma natives effect

enum program {
    // Present in every effect script but never selected by the engine: new effects start at 200 and
    // only 300 is requested later (effscript.cpp), and every retail body is empty.
    unused = 100,
    // Runs when the effect is created (effscript.cpp sets prog_no 200 on a new effect script) and is
    // resumed every frame; the effect is deleted when it ends.
    play = 200,
    // Runs when the owner asks the effect to wind down through _ESM_FINISH from action, monster or
    // event scripts or when a held item is thrown (SetScriptProgNo(300)); retail bodies fade the
    // sprites out before ending.
    finish = 300,
};

#define PI 3.1415927

// Sprite alpha blend modes passed to mgCDrawEnv::SetAlpha (mg_drawenv.hpp mgAlphaMacroID); 0 keeps
// the current mode.
enum blend_mode {
    MG_ALPHA_MACRO_NONE = 0,
    MG_ALPHA_MACRO_BLEND = 1,
    MG_ALPHA_MACRO_ADD = 2,
    MG_ALPHA_MACRO_SUB = 3,
    MG_ALPHA_MACRO_OPAQUE = 4,
    MG_ALPHA_MACRO_ADD_FIX = 5,
};

// Collision query bits matched against CCPoly::ignore_mask; polygons with a matching bit let that
// query through (1 player moves, 2 monster moves, 4 shots and thrown objects, 8 the fishing line).
enum coll_query {
    COLL_QUERY_PLAYER = 1,
    COLL_QUERY_MONSTER = 2,
    COLL_QUERY_PROJECTILE = 4,
    COLL_QUERY_FISHING_LINE = 8,
};

// Motion playback flags (character.hpp CharaMotionFlag): pause, stop on the last frame, restart
// without blending.
enum motion_flags {
    CHARA_MOTION_PAUSE = 1,
    CHARA_MOTION_HOLD = 2,
    CHARA_MOTION_RESTART = 4,
};

// Writes 0.0 to all three out variables. Fails unless given three arguments.
// Original name: _ZERO_VECTOR.
extern void clear_vector(float *x, float *y, float *z) NATIVE(0);

// Scales the vector held in the three variables to length 1 in place. Pass the variables as &x,
// &y, &z; they are read and written back.
// Original name: _NORMAL_VECTOR.
extern void normalize_vector(float *x, float *y, float *z) NATIVE(1);

// Copies src_x, src_y, src_z into the three out variables.
// Original name: _COPY_VECTOR.
extern void copy_vector(float *x, float *y, float *z, float src_x, float src_y, float src_z) NATIVE(2);

// Adds dx, dy, dz to the vector held in the three variables, in place.
// Original name: _ADD_VECTOR.
extern void add_vector(float *x, float *y, float *z, float dx, float dy, float dz) NATIVE(3);

// Subtracts dx, dy, dz from the vector held in the three variables, in place.
// Original name: _SUB_VECTOR.
extern void subtract_vector(float *x, float *y, float *z, float dx, float dy, float dz) NATIVE(4);

// Multiplies the vector held in the three variables by factor, in place.
// Original name: _SCALE_VECTOR.
extern void scale_vector(float *x, float *y, float *z, float factor) NATIVE(5);

// Divides the vector held in the three variables by divisor, in place. Fails and leaves the vector
// unchanged when divisor is 0.
// Original name: _DIV_VECTOR.
extern void divide_vector(float *x, float *y, float *z, float divisor) NATIVE(6);

// Returns the length of the vector (x, y, z).
// Original name: _DIST_VECTOR.
extern float get_vector_length(float x, float y, float z) NATIVE(7);

// Returns the distance between point (x1, y1, z1) and point (x2, y2, z2).
// Original name: _DIST_VECTOR2.
extern float get_distance(float x1, float y1, float z1, float x2, float y2, float z2) NATIVE(8);

// Returns the square root of value.
// Original name: _SQRT.
extern float get_square_root(float value) NATIVE(9);

// Returns atan2(y, x) in radians. Scripts pass (dx, dz) to get a yaw angle.
// Original name: _ATAN2F.
extern float get_atan2(float y, float x) NATIVE(10);

// Compares two angles in radians after wrapping their difference into -pi..pi. Returns 0 when a
// and b differ by no more than tolerance, 1 when a is larger, -1 when it is smaller.
// Original name: _ANGLE_CMP.
extern int compare_angles(float a, float b, float tolerance) NATIVE(11);

// Wraps the angle held in the variable into the range -pi..pi radians, in place.
// Original name: _ANGLE_LIMIT.
extern void wrap_angle(float *angle) NATIVE(12);

// Writes a random number from 0 up to but not including range into value. A float range gives a
// float; an int range gives an int.
// Original name: _GET_RAND.
extern void get_random(any range, void *value) NATIVE(13);

// Computes the rotation in radians that faces from point (from_x, from_y, from_z) toward point
// (to_x, to_y, to_z). With three out parameters it writes pitch, yaw and 0; with only one it
// writes the yaw.
// Original name: _GET_REF_ROT.
extern void get_rot_toward(float from_x, float from_y, float from_z, float to_x, float to_y, float to_z, float *rot_x, OPTIONAL(float *rot_y, float *rot_z)) NATIVE(14);

// Turns a rotation in radians into a unit direction: the +Z axis rotated by rot_x (pitch) and
// rot_y (yaw). rot_z is ignored.
// Original name: _GET_DIR_VECTOR.
extern void get_dir_from_rot(float rot_x, float rot_y, float rot_z, float *dir_x, float *dir_y, float *dir_z) NATIVE(15);

// Sets the origin of this effect. The effect's model, its copies and its sprites are drawn at
// their positions plus this origin.
// Original name: _SET_ORIGIN.
extern void set_effect_origin(float x, float y, float z) NATIVE(50);

// Writes the origin of this effect into the three out variables.
// Original name: _GET_ORIGIN.
extern void get_effect_origin(float *x, float *y, float *z) NATIVE(51);

// With on = 1 the effect's drawing origin also follows its target scene character (see
// get_target_id), or that character's frame named frame_name when given. With on = 0 it stops
// following.
// Original name: _AUTO_SET_OFFSET.
extern void set_origin_follow(int on, OPTIONAL(string frame_name)) NATIVE(52);

// Writes the first work vector that the effect's starter passed in (for example a start position)
// into the three out variables. It is (0, 0, 0) when none was set.
// Original name: _GET_WORK_VECT1.
extern void get_work_vector1(float *x, float *y, float *z) NATIVE(53);

// Writes the second work vector that the effect's starter passed in into the three out variables.
// It is (0, 0, 0) when none was set.
// Original name: _GET_WORK_VECT2.
extern void get_work_vector2(float *x, float *y, float *z) NATIVE(54);

// Returns the scene character slot that this effect aims at or follows, or -1 when none was set.
// Original name: _GET_TARGET_ID.
extern int get_target_id(void) NATIVE(55);

// Returns the owner of this effect: the scene character slot that started it, whose sound bank and
// attack identity the effect uses.
// Original name: _GET_USER_ID.
extern int get_owner_id(void) NATIVE(56);

// Reads shared value slot index (0 to 7) of this effect into value, as an int or a float according
// to the type of the variable passed. The starter fills these slots with set_child_value.
// Original name: _GET_VALUE.
extern void get_effect_value(int index, void *value) NATIVE(57);

// Stores value, int or float, in value slot index (0 to 7) of this effect.
// Original name: _SET_VALUE.
extern void set_effect_value(int index, any value) NATIVE(58);

// Shows (1) or hides (0) the effect's model. With fade = 1 it fades in from transparent or out
// from opaque at a fixed 0.1 alpha per frame; fade_speed is read and ignored. Fails when the
// effect has no model.
// Original name: _CHR_SET_SHOW.
extern void set_model_visible(int visible, OPTIONAL(int fade, float fade_speed)) NATIVE(100);

// Writes whether the effect's model is shown into visible, and its fade setting into fade when
// given.
// Original name: _CHR_GET_SHOW.
extern void get_model_visible(int *visible, OPTIONAL(int *fade)) NATIVE(101);

// Sets the position of the effect's model, relative to the effect origin. New effects place it at
// (0, -10000, 0).
// Original name: _CHR_SET_POS.
extern void set_model_pos(float x, float y, float z) NATIVE(102);

// Writes the position of the effect's model into the three out variables.
// Original name: _CHR_GET_POS.
extern void get_model_pos(float *x, float *y, float *z) NATIVE(103);

// Turns the effect's model toward rotation (x, y, z) in radians. With steps it moves only 1/steps
// of the remaining shortest angle on each call, so calling it every frame turns the model
// smoothly; without it the rotation is set at once.
// Original name: _CHR_SET_ROT.
extern void set_model_rot(float x, float y, float z, OPTIONAL(int steps)) NATIVE(104);

// Writes the rotation of the effect's model in radians into the three out variables.
// Original name: _CHR_GET_ROT.
extern void get_model_rot(float *x, float *y, float *z) NATIVE(105);

// Sets the x, y and z scale of the effect's model.
// Original name: _CHR_SET_SCALE.
extern void set_model_scale(float x, float y, float z) NATIVE(106);

// Writes the scale of the effect's model into the three out variables.
// Original name: _CHR_GET_SCALE.
extern void get_model_scale(float *x, float *y, float *z) NATIVE(107);

// Starts the named motion on the effect's model. speed sets the frames advanced per step; a
// negative value (the default) keeps the current speed. flags: 1 pauses, 2 holds the last frame
// instead of looping, 4 restarts from the first frame without blending.
// Original name: _CHR_SET_MOTION.
extern void play_model_motion(string motion_name, OPTIONAL(float speed, enum motion_flags flags)) NATIVE(108);

// Sets the frames the effect model's motion advances each step.
// Original name: _CHR_SET_MOT_STEP.
extern void set_model_motion_speed(float speed) NATIVE(109);

// Writes the unit direction the effect's model faces, from its pitch and yaw, into the three out
// variables.
// Original name: _CHR_GET_DIR_VECTOR.
extern void get_model_dir(float *x, float *y, float *z) NATIVE(110);

// Computes the rotation in radians that faces from the effect's model toward point (x, y, z). With
// three out parameters it writes pitch, yaw and 0; with only one it writes the yaw.
// Original name: _CHR_GET_REF_ROT.
extern void get_model_rot_toward(float x, float y, float z, float *rot_x, OPTIONAL(float *rot_y, float *rot_z)) NATIVE(111);

// Adds (dx, dy, dz) to the position of the effect's model.
// Original name: _CHR_ADD_POS.
extern void move_model_by(float dx, float dy, float dz) NATIVE(112);

// Adds (dx, dy, dz) radians to the rotation of the effect's model and wraps each angle into
// -pi..pi.
// Original name: _CHR_ADD_ROT.
extern void rotate_model_by(float dx, float dy, float dz) NATIVE(113);

// Meant to add to the scale of the effect's model, but the engine discards the result, so it has
// no effect.
// Original name: _CHR_ADD_SCALE.
extern void scale_model_by(float dx, float dy, float dz) NATIVE(114);

// Creates count extra copies (at most 4) of the effect's model, numbered 0 to count - 1, for the
// *_model_copy natives. They are drawn with the same origin as the model.
// Original name: _CHR_COPY_CHARA.
extern void create_model_copies(int count) NATIVE(125);

// Sets the position of model copy number copy, relative to the effect origin.
// Original name: _CHR_SET_POS2.
extern void set_model_copy_pos(int copy, float x, float y, float z) NATIVE(126);

// Sets the rotation in radians of model copy number copy.
// Original name: _CHR_SET_ROT2.
extern void set_model_copy_rot(int copy, float x, float y, float z) NATIVE(127);

// Sets the scale of model copy number copy.
// Original name: _CHR_SET_SCALE2.
extern void set_model_copy_scale(int copy, float x, float y, float z) NATIVE(128);

// Starts the named motion on model copy number copy, with speed (negative keeps the current speed)
// and flags as in play_model_motion. The engine checks the argument count without counting copy,
// so always pass all four arguments.
// Original name: _CHR_SET_MOTION2.
extern void play_model_copy_motion(int copy, string motion_name, float speed, enum motion_flags flags) NATIVE(129);

// Adds (dx, dy, dz) to the position of model copy number copy.
// Original name: _CHR_ADD_POS2.
extern void move_model_copy_by(int copy, float dx, float dy, float dz) NATIVE(130);

// Adds (dx, dy, dz) radians to the rotation of model copy number copy, wrapping each angle into
// -pi..pi.
// Original name: _CHR_ADD_ROT2.
extern void rotate_model_copy_by(int copy, float dx, float dy, float dz) NATIVE(131);

// Adds (dx, dy, dz) to the scale of model copy number copy.
// Original name: _CHR_ADD_SCALE2.
extern void scale_model_copy_by(int copy, float dx, float dy, float dz) NATIVE(132);

// Returns how far the effect model's current motion has played, from 0.0 to 1.0.
// Original name: _CHR_GET_MOT_WAIT.
extern float get_model_motion_progress(void) NATIVE(133);

// Shows (1) or hides (0) model copy number copy, fading at a fixed 0.1 alpha per frame when fade =
// 1. fade_speed is read and ignored.
// Original name: _CHR_SET_SHOW2.
extern void set_model_copy_visible(int copy, int visible, OPTIONAL(int fade, float fade_speed)) NATIVE(134);

// Writes the world position of the frame (bone) named frame_name in the effect's model into the
// three out variables. Fails when there is no such frame.
// Original name: _CHR_GET_FRAME_POS.
extern void get_model_frame_pos(string frame_name, float *x, float *y, float *z) NATIVE(135);

// Sets the colour attribute (red, green, blue, alpha) on every frame of the effect's model. When
// no_light is given it also sets whether the model ignores scene lighting (nonzero) or not (0).
// Original name: _CHR_SET_LIGHT_COLOR.
extern void set_model_light_color(float red, float green, float blue, float alpha, OPTIONAL(int no_light)) NATIVE(136);

// Allocates count billboard sprites for this effect, numbered 0 to count - 1. Fails when the
// effect already has sprites. ok, when given, receives 1 on success and 0 when memory ran out.
// Original name: _SPT_ASSIGN_SPRITE.
extern void allocate_sprites(int count, OPTIONAL(int *ok)) NATIVE(150);

// Frees this effect's sprites so allocate_sprites can be called again.
// Original name: _SPT_DELETE_SPRITE.
extern void free_sprites(void) NATIVE(151);

// Sets the texture, by name in the effect's texture block, that all of this effect's sprites are
// drawn with.
// Original name: _SPT_SET_TEXNAME.
extern void set_sprite_texture(string texture_name) NATIVE(152);

// Sets the blend mode of sprites: 1 translucent, 2 additive, 3 subtractive, 4 opaque, 5 additive
// ignoring alpha; 0 keeps the screen's current mode. Affects count sprites starting at first
// (count defaults to 1); it stops and fails at the first index past the effect's sprite count. The
// disc once calls it with only first, which leaves the mode unspecified.
// Original name: _SPT_SET_ALPHAB.
extern void set_sprite_blend(int first, OPTIONAL(enum blend_mode blend_mode, int count)) NATIVE(153);

// Resets sprites to defaults: hidden, translucent blend, at (0, 0, 0), colour 128 on all channels,
// scale 1, zero size and UV, and every velocity, acceleration, easing and blink cleared. Affects
// count sprites starting at first (count defaults to 1); it stops and fails at the first index
// past the effect's sprite count.
// Original name: _SPT_INIT_SPRITE.
extern void reset_sprites(int first, OPTIONAL(int count)) NATIVE(154);

// Shows (nonzero) or hides (0) sprites. Affects count sprites starting at first (count defaults to
// 1); it stops and fails at the first index past the effect's sprite count.
// Original name: _SPT_SET_DRAW_FLAG.
extern void set_sprite_visible(int first, int visible, OPTIONAL(int count)) NATIVE(155);

// Returns the visibility flag of sprite index.
// Original name: _SPT_GET_DRAW_FLAG.
extern int get_sprite_visible(int index) NATIVE(156);

// Sets the texture rectangle drawn on sprites: left u, top v, width and height in texture pixels.
// Affects count sprites starting at first (count defaults to 1); it stops and fails at the first
// index past the effect's sprite count.
// Original name: _SPT_SET_UV_SIZE.
extern void set_sprite_uv(int first, float u, float v, float width, float height, OPTIONAL(int count)) NATIVE(157);

// Sets the drawn width and height of sprites in world units, before their scale is applied.
// Affects count sprites starting at first (count defaults to 1); it stops and fails at the first
// index past the effect's sprite count.
// Original name: _SPT_SET_PUT_SIZE.
extern void set_sprite_size(int first, float width, float height, OPTIONAL(int count)) NATIVE(158);

// Sets the position of sprites, relative to the effect origin. Affects count sprites starting at
// first (count defaults to 1); it stops and fails at the first index past the effect's sprite
// count.
// Original name: _SPT_SET_POS.
extern void set_sprite_pos(int first, float x, float y, float z, OPTIONAL(int count)) NATIVE(159);

// Writes the position of sprite index into the three out variables.
// Original name: _SPT_GET_POS.
extern void get_sprite_pos(int index, float *x, float *y, float *z) NATIVE(160);

// Sets the screen-plane rotation of sprites in radians. Affects count sprites starting at first
// (count defaults to 1); it stops and fails at the first index past the effect's sprite count.
// Original name: _SPT_SET_ROTZ.
extern void set_sprite_angle(int first, float angle, OPTIONAL(int count)) NATIVE(161);

// Returns the screen-plane rotation in radians of sprite index.
// Original name: _SPT_GET_ROTZ.
extern float get_sprite_angle(int index) NATIVE(162);

// Sets the horizontal and vertical scale of sprites; the drawn size is size times scale. Affects
// count sprites starting at first (count defaults to 1); it stops and fails at the first index
// past the effect's sprite count.
// Original name: _SPT_SET_SCALE.
extern void set_sprite_scale(int first, float scale_x, float scale_y, OPTIONAL(int count)) NATIVE(163);

// Writes the horizontal and vertical scale of sprite index into scale_x and scale_y.
// Original name: _SPT_GET_SCALE.
extern void get_sprite_scale(int index, float *scale_x, float *scale_y) NATIVE(164);

// Sets the colour of sprites, each channel 0 to 255 with 128 as normal brightness and full alpha.
// Affects count sprites starting at first (count defaults to 1); it stops and fails at the first
// index past the effect's sprite count.
// Original name: _SPT_SET_COLOR.
extern void set_sprite_color(int first, float red, float green, float blue, float alpha, OPTIONAL(int count)) NATIVE(165);

// Writes the colour channels of sprite index into the four out variables.
// Original name: _SPT_GET_COLOR.
extern void get_sprite_color(int index, float *red, float *green, float *blue, float *alpha) NATIVE(166);

// Places a run of sprites along a curve: the first gets (x, y, z), and each next one is offset by
// a step vector that starts at (vx, vy, vz) and grows by (ax, ay, az) per sprite. Affects count
// sprites starting at first (count defaults to 1); it stops and fails at the first index past the
// effect's sprite count.
// Original name: _SPT_VAN_SET_POS.
extern void spread_sprite_pos(int first, float x, float y, float z, float vx, float vy, float vz, float ax, float ay, float az, OPTIONAL(int count)) NATIVE(167);

// Sets screen-plane angles across a run of sprites: the first gets angle, and each next one adds a
// step that starts at velocity and grows by accel per sprite, all in radians. Affects count
// sprites starting at first (count defaults to 1); it stops and fails at the first index past the
// effect's sprite count.
// Original name: _SPT_VAN_SET_ROT.
extern void spread_sprite_angle(int first, float angle, float velocity, float accel, OPTIONAL(int count)) NATIVE(168);

// Adds (dx, dy, dz) to the position of sprites. Affects count sprites starting at first (count
// defaults to 1); it stops and fails at the first index past the effect's sprite count.
// Original name: _SPT_ADD_POS.
extern void move_sprite_by(int first, float dx, float dy, float dz, OPTIONAL(int count)) NATIVE(169);

// Adds angle radians to the screen-plane rotation of sprites, wrapping into -pi..pi. Affects count
// sprites starting at first (count defaults to 1); it stops and fails at the first index past the
// effect's sprite count.
// Original name: _SPT_ADD_ROTZ.
extern void rotate_sprite_by(int first, float angle, OPTIONAL(int count)) NATIVE(170);

// Adds to the colour channels of sprites and clamps each to 0..255. Affects count sprites starting
// at first (count defaults to 1); it stops and fails at the first index past the effect's sprite
// count.
// Original name: _SPT_ADD_COLOR.
extern void add_sprite_color(int first, float red, float green, float blue, float alpha, OPTIONAL(int count)) NATIVE(171);

// Rotates the positions of sprites by angle radians around the Y axis through the effect origin.
// Affects count sprites starting at first (count defaults to 1); it stops and fails at the first
// index past the effect's sprite count.
// Original name: _SPT_WORLD_ROT.
extern void rotate_sprites_around_y(int first, float angle, OPTIONAL(int count)) NATIVE(172);

// Unsupported: always fails and does nothing.
// Original name: _SPT_SET_LIFE.
extern void set_sprite_life(void) NATIVE(173);

// Sets the per-frame movement of sprites; the engine adds it to their position every frame.
// Affects count sprites starting at first (count defaults to 1); it stops and fails at the first
// index past the effect's sprite count.
// Original name: _SPT_SET_VELO_POS.
extern void set_sprite_velocity(int first, float vx, float vy, float vz, OPTIONAL(int count)) NATIVE(174);

// Sets the per-frame change of sprite velocity; the engine adds it to their velocity every frame.
// Affects count sprites starting at first (count defaults to 1); it stops and fails at the first
// index past the effect's sprite count.
// Original name: _SPT_SET_ACC_POS.
extern void set_sprite_accel(int first, float ax, float ay, float az, OPTIONAL(int count)) NATIVE(175);

// Sets the radians per frame that sprites turn in the screen plane. Affects count sprites starting
// at first (count defaults to 1); it stops and fails at the first index past the effect's sprite
// count.
// Original name: _SPT_SET_VELO_ROTZ.
extern void set_sprite_spin(int first, float velocity, OPTIONAL(int count)) NATIVE(176);

// Sets the per-frame change of sprite spin, in radians; spin is capped at 2 pi per frame. Affects
// count sprites starting at first (count defaults to 1); it stops and fails at the first index
// past the effect's sprite count. A fourth argument seen on the disc is ignored.
// Original name: _SPT_SET_ACC_ROTZ.
extern void set_sprite_spin_accel(int first, float accel, OPTIONAL(int count, any unused)) NATIVE(177);

// Sets the amount added to each sprite colour channel every frame. Affects count sprites starting
// at first (count defaults to 1); it stops and fails at the first index past the effect's sprite
// count.
// Original name: _SPT_SET_VELO_COL.
extern void set_sprite_color_velocity(int first, float red, float green, float blue, float alpha, OPTIONAL(int count)) NATIVE(178);

// Sets the per-frame change of the sprite colour velocity. Affects count sprites starting at first
// (count defaults to 1); it stops and fails at the first index past the effect's sprite count.
// Original name: _SPT_SET_ACC_COL.
extern void set_sprite_color_accel(int first, float red, float green, float blue, float alpha, OPTIONAL(int count)) NATIVE(179);

// Makes sprites pulse: each drawn frame adds sin(phase) times the given amplitude to each colour
// channel, and phase advances by speed radians. speed = 0 turns blinking off. Affects count
// sprites starting at first (count defaults to 1); it stops and fails at the first index past the
// effect's sprite count.
// Original name: _SPT_SET_BLINKING.
extern void set_sprite_blink(int first, float red, float green, float blue, float alpha, float speed, OPTIONAL(int count)) NATIVE(180);

// Sets colours across a run of sprites: the first gets (red, green, blue, alpha), and each next
// one adds a step that starts at the velocity colour and grows by the accel colour per sprite.
// Starts at sprite first; pass count, since the engine reads it whenever there are 11 or more
// arguments.
// Original name: _SPT_VAN_SET_COL.
extern void spread_sprite_color(int first, float red, float green, float blue, float alpha, float velocity_red, float velocity_green, float velocity_blue, float velocity_alpha, float accel_red, float accel_green, float accel_blue, float accel_alpha, int count) NATIVE(181);

// Sets the amount added to the horizontal and vertical scale of sprites every frame. Affects count
// sprites starting at first (count defaults to 1); it stops and fails at the first index past the
// effect's sprite count. A fifth argument seen on the disc is ignored.
// Original name: _SPT_SET_VELO_SCL.
extern void set_sprite_scale_velocity(int first, float scale_x, float scale_y, OPTIONAL(int count, any unused)) NATIVE(182);

// Sets the per-frame change of the sprite scale velocity. Affects count sprites starting at first
// (count defaults to 1); it stops and fails at the first index past the effect's sprite count.
// Original name: _SPT_SET_ACC_SCL.
extern void set_sprite_scale_accel(int first, float scale_x, float scale_y, OPTIONAL(int count)) NATIVE(183);

// Sets scales across a run of sprites: the first gets (scale_x, scale_y), and each next one adds a
// step that starts at (vx, vy) and grows by (ax, ay) per sprite. Affects count sprites starting at
// first (count defaults to 1); it stops and fails at the first index past the effect's sprite
// count.
// Original name: _SPT_VAN_SET_SCL.
extern void spread_sprite_scale(int first, float scale_x, float scale_y, float vx, float vy, float ax, float ay, OPTIONAL(int count)) NATIVE(184);

// Eases the scale of sprites toward (scale_x, scale_y): every frame they move 1/divisor of the
// remaining distance. A divisor of 0 or less stops easing. Affects count sprites starting at first
// (count defaults to 1); it stops and fails at the first index past the effect's sprite count.
// Original name: _SPT_SCALE_CONV.
extern void ease_sprite_scale(int first, float scale_x, float scale_y, float divisor, OPTIONAL(int count)) NATIVE(185);

// Eases the colour of sprites toward (red, green, blue, alpha): every frame each channel moves
// 1/divisor of the remaining distance. A divisor of 0 or less stops easing. Affects count sprites
// starting at first (count defaults to 1); it stops and fails at the first index past the effect's
// sprite count.
// Original name: _SPT_COLOR_CONV.
extern void ease_sprite_color(int first, float red, float green, float blue, float alpha, float divisor, OPTIONAL(int count)) NATIVE(186);

// Writes the position of the scene character in slot chara (for example the owner or target id)
// into the three out variables. Fails when the slot is empty.
// Original name: _SCN_GET_CHR_POS.
extern void get_scene_chara_pos(int chara, float *x, float *y, float *z) NATIVE(200);

// Writes the rotation in radians of the scene character in slot chara. With three out parameters
// it writes x, y and z; with only one it writes the yaw (y).
// Original name: _SCN_GET_CHR_ROT.
extern void get_scene_chara_rot(int chara, float *rot_x, OPTIONAL(float *rot_y, float *rot_z)) NATIVE(201);

// Writes the world position of the frame (bone) named frame_name on the scene character in slot
// chara into the three out variables.
// Original name: _SCN_GET_CHR_FRM_POS.
extern void get_chara_frame_pos(int chara, string frame_name, float *x, float *y, float *z) NATIVE(202);

// Tests the line from start to end against the scene's collision polygons, skipping polygons whose
// ignore bits match ignore_mask, and writes the nearest hit. The outputs depend on how many follow
// the 7 inputs: 1-3 outs get hit_index (polygon index or -1), area_kind, foot_sound; 4-6 outs get
// hit x, y, z, then those three; 7-9 outs get hit x, y, z, the unit reflection direction x, y, z,
// then those three. Other outputs are only valid when hit_index is 0 or more.
// Original name: _INTERSECTION_POINT.
extern void get_line_hit(enum coll_query ignore_mask, float start_x, float start_y, float start_z, float end_x, float end_y, float end_z, void *hit_x, OPTIONAL(void *hit_y, void *hit_z, void *reflect_x, void *reflect_y, void *reflect_z, void *hit_index, void *area_kind, int *foot_sound)) NATIVE(203);

// Disabled in this build: prints an error and fails without creating anything or writing the
// result. Use create_attack instead.
// Original name: _CREATE_DAMAGE.
extern int create_damage(string damage_name, float radius, int owner) NATIVE(204);

// Disabled in this build: does nothing and fails.
// Original name: _DELETE_DAMAGE.
extern void delete_damage(int handle) NATIVE(205);

// Disabled in this build: does nothing and fails.
// Original name: _DMG_SET_POS.
extern void set_damage_pos(int handle, float x, float y, float z) NATIVE(206);

// Disabled in this build: does nothing and fails.
// Original name: _DMG_SET_FRONT_VECT.
extern void set_damage_dir(int handle, float x, float y, float z) NATIVE(207);

// Disabled in this build: prints an error and fails.
// Original name: _DMG_SET_DAMAGE.
extern void set_damage_amount(int handle, int damage) NATIVE(208);

// Gives this effect an attack shape set up from the damage table row named damage_name, replacing
// any attack it had. owner defaults to the effect's owner; for rows that pick targets by owner,
// owner 0 hits monsters and any other owner hits the player. Place it with set_attack_shape.
// Original name: _COLPRIM_CREATE.
extern void create_attack(string damage_name, OPTIONAL(int owner)) NATIVE(210);

// Places this effect's attack. Four forms: (x, y, z, radius) a sphere; (start x, y, z, end x, y,
// z, radius) a line with that radius; (frame_name, radius) a sphere following that frame of the
// effect's model; (frame_name, end_frame_name, radius) a line between two frames of the model.
// Original name: _COLPRIM_SET_COORD.
extern void set_attack_shape(any start_x, any start_y, OPTIONAL(any start_z, float radius_or_end_x, float end_y, float end_z, float radius)) NATIVE(211);

// Removes this effect's attack. The argument is read and ignored; scripts pass -1.
// Original name: _COLPRIM_DELETE.
extern void delete_attack(int unused) NATIVE(212);

// Returns how many hits this effect's attack has dealt.
// Original name: _COLPRIM_GET_HITCNT.
extern int get_attack_hit_count(void) NATIVE(213);

// Sets the damage each hit of this effect's attack deals, overriding the damage table value.
// Original name: _COLPRIM_SET_DAMAGE.
extern void set_attack_damage(int damage) NATIVE(214);

// Writes the position of the last hit of this effect's attack into the three out variables.
// Original name: _COLPRIM_GET_HIT_POS.
extern void get_attack_hit_pos(float *x, float *y, float *z) NATIVE(215);

// Marks this effect's attack as carrying a gift of three item numbers. A monster accepts the gift
// only when all three equal the gift item it wants.
// Original name: _COLPRIM_GET_GIFT.
extern void set_attack_gift(int item1, int item2, int item3) NATIVE(216);

// Writes nonzero into knocked_back when another attack has knocked this effect's attack back, and
// the knockback direction into dir_x, dir_y, dir_z when given.
// Original name: _COLPRIM_GET_REVCNT.
extern void get_attack_knockback(int *knocked_back, OPTIONAL(float *dir_x, float *dir_y, float *dir_z)) NATIVE(217);

// Plays sound effect se from the owner character's sound bank. With x, y, z the volume and pan
// come from that position, fading out between 160 and 1200 world units. Fails when the effect has
// no owner.
// Original name: _MON_SE_PLAY.
extern void play_owner_se(int se, OPTIONAL(float x, float y, float z)) NATIVE(220);

// Stops sound effect se in the owner character's sound bank.
// Original name: _MON_SE_STOP.
extern void stop_owner_se(int se) NATIVE(221);

// Plays sound effect se from the scene's battle sound bank. With x, y, z the volume and pan come
// from that position, fading out between 160 and 1200 world units.
// Original name: _BTL_SE_PLAY.
extern void play_battle_se(int se, OPTIONAL(float x, float y, float z)) NATIVE(222);

// Stops sound effect se in the scene's battle sound bank.
// Original name: _BTL_SE_STOP.
extern void stop_battle_se(int se) NATIVE(223);

// Plays sound effect se from the scene's base sound bank.
// Original name: _BSE_SE_PLAY.
extern void play_base_se(int se) NATIVE(224);

// Stops sound effect se in the scene's base sound bank.
// Original name: _BSE_SE_STOP.
extern void stop_base_se(int se) NATIVE(225);

// Plays sound effect se from the sound bank of the scene character in slot chara. With x, y, z the
// volume and pan come from that position, fading out between 160 and 1200 world units.
// Original name: _MON_SE_PLAY2.
extern void play_chara_se(int chara, int se, OPTIONAL(float x, float y, float z)) NATIVE(226);

// Stops sound effect se in the sound bank of the scene character in slot chara.
// Original name: _MON_SE_STOP2.
extern void stop_chara_se(int chara, int se) NATIVE(227);

// With on = 1 this effect's sprites take their red, green and blue from the scene light and
// ambient colour instead of their own; alpha is kept. 0 turns it off.
// Original name: _SET_LIGHT_FLAG.
extern void set_sprite_lighting(int on) NATIVE(230);

// Writes the world position of entry point index (an attachment frame such as a weapon mount) on
// the scene character in slot chara, or the character's own position when it has none.
// Original name: _SCN_GET_CHR_ENTOBJ_POS.
extern void get_chara_entry_pos(int chara, int index, float *x, float *y, float *z) NATIVE(231);

// Writes the world direction of the +Z axis of the frame named frame_name on the scene character
// in slot chara into the three out variables.
// Original name: _SCN_GET_CHR_FRM_DIR.
extern void get_chara_frame_dir(int chara, string frame_name, float *x, float *y, float *z) NATIVE(232);

// Writes the pitch, yaw and 0, in radians, that face along the +Z axis of the frame named
// frame_name on the scene character in slot chara.
// Original name: _SCN_GET_CHR_FRM_ROT.
extern void get_chara_frame_rot(int chara, string frame_name, float *rot_x, float *rot_y, float *rot_z) NATIVE(233);

// Same as get_chara_entry_pos, but index must be 0 or 1.
// Original name: _SCN_GET_ENTRY_OBJ_POS.
extern void get_entry_object_pos(int chara, int index, float *x, float *y, float *z) NATIVE(234);

// Starts the effect named effect_name, owned by this effect's owner; it runs its program 200. With
// handle it is also entered in the owner's slot table (8 slots) and handle receives its slot
// number, or -1 on failure. The set_child_* natives address it by that handle, or without one by
// the effect started most recently.
// Original name: _ES_CREATE.
extern void start_child_effect(string effect_name, OPTIONAL(int *handle)) NATIVE(240);

// Sets the first work vector of a child effect, read there with get_work_vector1. Pass (x, y, z)
// for the most recently started effect, or (handle, x, y, z) for the effect in that slot.
// Original name: _ES_SET_VECT1.
extern void set_child_vector1(any handle_or_x, float x_or_y, float y_or_z, OPTIONAL(float z)) NATIVE(241);

// Sets the second work vector of a child effect, read there with get_work_vector2. Pass (x, y, z)
// for the most recently started effect, or (handle, x, y, z) for the effect in that slot.
// Original name: _ES_SET_VECT2.
extern void set_child_vector2(any handle_or_x, float x_or_y, float y_or_z, OPTIONAL(float z)) NATIVE(242);

// Sets the target scene character slot of a child effect, read there with get_target_id. Pass
// (target) for the most recently started effect, or (handle, target) for the effect in that slot.
// Original name: _ES_SET_TARGET_ID.
extern void set_child_target(int handle_or_target, OPTIONAL(int target)) NATIVE(243);

// Gives the most recently started child effect a pointer to this effect's attack.
// Original name: _ES_SET_COLPRIM.
extern void share_attack_with_child(void) NATIVE(244);

// Stores an int or float in value slot index (0 to 7) of a child effect, read there with
// get_effect_value. Pass (index, value) for the most recently started effect, or (handle, index,
// value) for the effect in that slot.
// Original name: _ES_SET_VALUE.
extern void set_child_value(int handle_or_index, any index_or_value, OPTIONAL(any value)) NATIVE(245);

// Writes the position of whatever event object handle number handle refers to into the three out
// variables.
// Original name: _GET_EOH_POS.
extern void get_event_object_pos(int handle, float *x, float *y, float *z) NATIVE(251);

// Shows (1) or hides (0) the frame named frame_name in the effect's model. With include_children
// nonzero the change also applies to the frames under it.
// Original name: _CHR_SET_FRAME_SHOW.
extern void set_model_frame_visible(string frame_name, int visible, int include_children) NATIVE(252);

// Returns 1 when the effect model's motion reaches its last frame within the next two steps or no
// motion plays, else 0.
// Original name: _CHR_CHK_MOT_END.
extern int check_model_motion_end(void) NATIVE(253);

// Numbers the disc calls that this table never registers. The engine prints "not found ext" and
// moves on, so these calls do nothing.
extern void _EXT_250(...) NATIVE(250);

#include <helpers/effect.h>

#endif
