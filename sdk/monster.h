// c2s SDK. Natives for monster AI scripts (dungeon/monster). They act on the monster running the script.
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

#ifndef DC2TK_MONSTER_H
#define DC2TK_MONSTER_H

#include <c2s.h>
#pragma natives monster

enum program {
    // Runs once when the monster is placed, right after its script is loaded (monster.cpp sets
    // req_prog MONSTER_PROG_INIT and runs it before switching to 200).
    init = 100,
    // Normal behaviour loop: resumed every frame and restarted whenever the running program ends
    // (runscript_opcodes.cpp CMonsterMan::RunScript, MONSTER_PROG_MAIN).
    update = 200,
    // Runs when a hit lands on a monster under the stop status or inflicts it (monster.cpp reaction 3,
    // status bits 0x8/0x20, MONSTER_PROG_STATUS); retail bodies hold the damage pose until the status
    // clears.
    stopped = 400,
    // Runs when a hit staggers the monster (monster.cpp reaction 2 once the stagger total reaches the
    // table limit, MONSTER_PROG_DAMAGE).
    damage = 500,
    // Runs when a hit with the knockdown flag knocks the monster over (monster.cpp reaction 4,
    // MONSTER_PROG_KNOCK).
    knockdown = 600,
    // Runs when the monster guards a hit or takes no damage from it (monster.cpp MONSTER_PROG_GUARD).
    guard = 700,
    // Runs when the monster rolls its escape_rate0 chance to dodge an attack that its target started
    // with murderous mode 0, which the action scripts set for melee swings (monster.cpp
    // MONSTER_PROG_ESCAPE_0).
    dodge_melee = 800,
    // Runs when the monster rolls its escape_rate1 chance to dodge an attack that its target started
    // with murderous mode 1, which the action scripts set when firing a gun (monster.cpp
    // MONSTER_PROG_ESCAPE_1).
    dodge_shot = 900,
    // Runs when a hit brings the monster's life to zero or a death request arrives (monster.cpp
    // reaction 6 and damage_req 4, MONSTER_PROG_DEAD).
    death = 1000,
    // Runs when the player picks the monster up and carries it (monster.cpp sets catch_state 1 and
    // req_prog 1100).
    lifted = 1100,
    // Runs when a thrown monster hits the ground (monster.cpp catch_state 2 on landing,
    // MONSTER_PROG_LAND).
    land = 1200,
    // Runs when the monster is stunned by a stun hit, a nearby shock or a thrown monster colliding
    // with it (monster.cpp MONSTER_PROG_PIYORI, sets piyori_time).
    stun = 1400,
};

#define PI 3.1415927

// Dungeon battle-area camera modes handled in dng_main.cpp.
enum camera_mode {
    CAMERA_MODE_PLAYER = 0,
    CAMERA_MODE_BOSS = 1,
    CAMERA_MODE_ARENA = 2,
    CAMERA_MODE_PLAYER_LOW = 4,
};

// Bits selecting a monster's per-attack-kind damage percentages (ext_param, indexed through
// vs_attk_index by DamageKind).
enum damage_kind_mask {
    DAMAGE_MASK_MAX_MELEE = 1,
    DAMAGE_MASK_MAX_GUN = 2,
    DAMAGE_MASK_LASER_GUN = 4,
    DAMAGE_MASK_GRENADE = 8,
    DAMAGE_MASK_MONICA_MELEE = 16,
    DAMAGE_MASK_MONICA_MAGIC = 32,
    DAMAGE_MASK_RIDEPOD_PUNCH = 64,
    DAMAGE_MASK_RIDEPOD_SWORD = 128,
    DAMAGE_MASK_RIDEPOD_GUN = 256,
    DAMAGE_MASK_KIND_14 = 512,
    DAMAGE_MASK_MONSTER = 1024,
    DAMAGE_MASK_ITEM = 2048,
};

// Colour of a popped-up damage number in _SET_DAMAGE_SCORE.
enum damage_number_color {
    DAMAGE_NUMBER_GREEN = 1,
};

// Selectors of _GET_EVENT_INFO.
enum event_info {
    EVENT_INFO_STOPWATCH_LIMIT = 0,
};

// How the player picks a target (scenesnd.hpp lock_on_mode): nearest monster or cycling selection.
enum lock_on_mode {
    LOCK_ON_MODE_NEAREST = 0,
    LOCK_ON_MODE_SELECT = 2,
};

// Monster mask_flag bits tested in monster.cpp: 1 cancels the guard roll, 2 cancels the dodge
// roll.
enum mask_flags {
    MASK_NO_GUARD = 1,
    MASK_NO_DODGE = 2,
};

// Behaviour bits of an active monster (monster.hpp MONSTER_ATTRIB); bit 2 skips the death smoke in
// _SET_DEAD_OFF and bit 0x80 stops hits pushing the monster in CheckDamage.
enum monster_attrib {
    MONSTER_ATTRIB_NO_LOCK_ON = 1,
    MONSTER_ATTRIB_NO_DEATH_SMOKE = 2,
    MONSTER_ATTRIB_SET_VELOCITY = 4,
    MONSTER_ATTRIB_NO_MAP_HIT = 8,
    MONSTER_ATTRIB_NO_BODY_HIT = 16,
    MONSTER_ATTRIB_NO_DAMAGE = 32,
    MONSTER_ATTRIB_QUICK_DEAD = 64,
    MONSTER_ATTRIB_NO_HIT_PUSH = 128,
    MONSTER_ATTRIB_SKIP_GROUND = 256,
    MONSTER_ATTRIB_ALWAYS_VIEW = 512,
};

// Status bits that hits leave on a monster (monster.hpp MONSTER_STATUS_ATTR); 0x8 and 0x20 are the
// stop status for 300 and 900 steps.
enum monster_status {
    MONSTER_STATUS_POISON = 1,
    MONSTER_STATUS_SLOW = 2,
    MONSTER_STATUS_STOP = 8,
    MONSTER_STATUS_STOP_LONG = 32,
};

// Motion playback flags (character.hpp CharaMotionFlag): pause, stop on the last frame, restart
// without blending.
enum motion_flags {
    CHARA_MOTION_PAUSE = 1,
    CHARA_MOTION_HOLD = 2,
    CHARA_MOTION_RESTART = 4,
};

// Dungeon battle-area pause bits (scenesnd.hpp DNG_PAUSE_*), each stopping one dungeon system
// while set.
enum pause_flags {
    DNG_PAUSE_MONSTER_AI = 1,
    DNG_PAUSE_PLAYER_STEP = 2,
    DNG_PAUSE_PLAYER_CONTROL = 4,
    DNG_PAUSE_MONSTER_DRAW = 16,
    DNG_PAUSE_SKY_DRAW = 32,
    DNG_PAUSE_MAP_DRAW = 128,
    DNG_PAUSE_MINIMAP = 256,
    DNG_PAUSE_PICKUPS = 512,
    DNG_PAUSE_PLAYER_STATUS = 1024,
    DNG_PAUSE_EXIT_HEAL = 2048,
    DNG_PAUSE_ENEMY_STEP = 4096,
    DNG_PAUSE_WEAPON_DRAW = 8192,
    DNG_PAUSE_BATTLE_MUSIC = 16384,
    DNG_PAUSE_PAD_RESET = 32768,
    DNG_PAUSE_MONSTER_NAMES = 65536,
};

// Drawing bits of a scene character slot (scenevillager.hpp SCENE_CHARA_STATUS).
enum scene_chara_status {
    SCENE_CHARA_NO_SHADOW = 8,
    SCENE_CHARA_HIDE = 16,
    SCENE_CHARA_EXCLAMATION = 32,
    SCENE_CHARA_NO_LIGHTING = 64,
    SCENE_CHARA_NO_FADE = 128,
    SCENE_CHARA_NO_DIST = 256,
    SCENE_CHARA_NO_MODEL = 512,
};

// How a monster's sound effects are mixed (character.hpp se_positional).
enum se_mode {
    SE_MODE_POSITIONAL = 1,
    SE_MODE_FIXED = 2,
};

// Scales the vector held in three float variables to length 1, in place.
// Original name: _NORMAL_VECTOR.
extern void normalize_vector(float *x, float *y, float *z) NATIVE(0, 0);

// Copies src_x, src_y, src_z into the three variables x, y, z.
// Original name: _COPY_VECTOR.
extern void copy_vector(float *x, float *y, float *z, float src_x, any src_y, any src_z) NATIVE(0, 1);

// Adds add_x, add_y, add_z to the vector held in variables x, y, z, in place.
// Original name: _ADD_VECTOR.
extern void add_vector(float *x, float *y, float *z, float add_x, float add_y, float add_z) NATIVE(0, 2);

// Subtracts sub_x, sub_y, sub_z from the vector held in variables x, y, z, in place.
// Original name: _SUB_VECTOR.
extern void subtract_vector(float *x, float *y, float *z, float sub_x, float sub_y, float sub_z) NATIVE(0, 3);

// Multiplies the vector held in variables x, y, z by scale, in place.
// Original name: _SCALE_VECTOR.
extern void scale_vector(float *x, float *y, float *z, float scale) NATIVE(0, 4);

// Divides the vector held in variables x, y, z by divisor, in place. A divisor of 0 fails and
// leaves the vector unchanged.
// Original name: _DIV_VECTOR.
extern void divide_vector(float *x, float *y, float *z, float divisor) NATIVE(5);

// Compares two angles in radians, wrapping the difference into -pi to pi. Returns 0 when angle is
// within tolerance of target, 1 when it is further ahead (positive difference) and -1 when it is
// further behind.
// Original name: _ANGLE_CMP.
extern int compare_angles(float angle, float target, float tolerance) NATIVE(6);

// Wraps the angle held in a float variable into the range -pi to pi radians, in place.
// Original name: _ANGLE_LIMIT.
extern void wrap_angle(float *angle) NATIVE(0, 7);

// Returns the square root of value.
// Original name: _SQRT.
extern float calc_square_root(float value) NATIVE(0, 8);

// Returns atan2(y, x) in radians. Scripts pass an X offset and a Z offset to get a yaw.
// Original name: _ATAN2F.
extern float calc_atan2(float y, float x) NATIVE(0, 9);

// Does nothing and reports success. Disc scripts pass a number and a variable, but the native
// reads and writes neither.
// Original name: _ND_TEST.
extern void run_nd_test(any value, any result) NATIVE(10);

// Writes the length of the vector x, y, z into length.
// Original name: _GET_DIST_VECTOR.
extern void get_vector_length(float x, any y, OPTIONAL(float z, float *length)) NATIVE(11);

// Returns the distance between point 1 and point 2.
// Original name: _GET_DIST_VECTOR2.
extern float get_distance_between(float x1, float y1, float z1, float x2, float y2, float z2) NATIVE(0, 12);

// Intersects a circle on the ground plane (X and Z only) with the line through points A and B.
// hit_count receives 0, 1 or 2; hit1 is written when there is at least one hit and hit2 only when
// there are two.
// Original name: _CALC_IP_CIRCLE_LINE.
extern void calc_circle_line_hits(float center_x, any center_z, OPTIONAL(float radius, float ax, float az, float bx, float bz, int *hit_count, float *hit1_x, float *hit1_z, float *hit2_x, float *hit2_z)) NATIVE(13);

// Turns two yaw angles in radians into horizontal unit directions and writes their dot product
// into dot: 1 when they face the same way, -1 when opposite. Fails unless all three arguments are
// given.
// Original name: _GET_ANGLE_INNER.
extern void calc_angle_dot(float angle_a, float angle_b, OPTIONAL(float *dot)) NATIVE(14);

// Plays a sound effect from this monster's sound bank, with volume and pan set from the monster's
// position (full volume within 160 units, silent beyond 1200).
// Original name: _MY_SE_PLAY.
extern void play_monster_se(int se, OPTIONAL(any extra2, any extra3)) NATIVE(20, 1);

// Stops a sound effect from this monster's sound bank.
// Original name: _MY_SE_STOP.
extern void stop_monster_se(int se, OPTIONAL(any extra2, any extra3)) NATIVE(20, 2);

// Plays a sound effect from another monster's sound bank at fixed volume. Monster handles are
// character numbers 24 to 47 (24 plus the active slot), as get_monster_handle and
// get_monster_in_range give them. Fails unless both arguments are given.
// Original name: _MONS_SE_PLAY.
extern void play_se_on_monster(any monster, OPTIONAL(int se)) NATIVE(20, 3);

// Stops a sound effect from another monster's sound bank. Monster handles are character numbers 24
// to 47 (24 plus the active slot), as get_monster_handle and get_monster_in_range give them.
// Original name: _MONS_SE_STOP.
extern void stop_se_on_monster(int monster, int se) NATIVE(20, 4);

// Turns off camera follow and player control of the active camera and sets the point it moves to
// look at, in world units.
// Original name: _SET_CAMERA_NEXT_REF.
extern void set_camera_look_at(float x, float y, float z) NATIVE(25);

// With on = 1 the active camera follows the player again and takes player control; with 0 both are
// turned off so scripts can place it.
// Original name: _SET_CAMERA_FOLLOW.
extern void set_camera_follow(int on) NATIVE(25, 1);

// Turns off camera follow and player control of the active camera and sets the eye position it
// moves to, in world units.
// Original name: _SET_CAMERA_NEXT_POS.
extern void move_camera_to(float x, float y, float z) NATIVE(25, 2);

// Sets the dungeon camera mode. 0 is the normal player camera, 1 sits behind the player looking at
// the first monster (the boss), 2 circles a fixed arena point, 4 is a variant of the normal
// camera, and other values leave the camera to scripts.
// Original name: _SET_CAMERA_MODE.
extern void set_camera_mode(OPTIONAL(enum camera_mode mode)) NATIVE(25, 3);

// Sets how fast the active camera moves toward its next eye position and look point.
// Original name: _SET_CAMERA_SPEED.
extern void set_camera_speed(float speed) NATIVE(25, 4);

// Shakes the dungeon camera with strength power, fading linearly to nothing over frames.
// Original name: _CAMERA_QUAKE.
extern void shake_camera(float power, int frames) NATIVE(25, 5);

// Sets the active camera's minimum and maximum distance from the player and its eye height when
// near and far (defaults 100, 160, 18, 10). Pass -99999.9 to leave a value unchanged. The engine
// rejects calls with 1 to 4 arguments, so a fifth, ignored value is needed.
// Original name: _SET_CAMERA_CTRL_PARAM1.
extern void set_camera_distance_limits(float min_dist, float max_dist, float near_height, float far_height, any ignored) NATIVE(31);

// Sets the active camera's height limits and how far it stays above the ground (defaults height
// -15, max 40, min -15, resting max 20, resting min -15, ground space 25). Pass -99999.9 to leave
// a value unchanged. The engine rejects calls with 1 to 6 arguments, so a seventh, ignored value
// is needed.
// Original name: _SET_CAMERA_CTRL_PARAM2.
extern void set_camera_height_limits(float height, float max_height, float min_height, float rest_max_height, float rest_min_height, float ground_space, any ignored) NATIVE(32);

// Restores the active camera's distance and height limits to their defaults (distance 100 to 160,
// near height 18, far height 10, height -15, max 40, min -15, resting max 20, resting min -15,
// ground space 25).
// Original name: _RESET_CAMERA_CTRL_PARAM.
extern void reset_camera_limits(void) NATIVE(25, 8);

// Writes a random whole number from 0 to range - 1 into value. Fails unless both arguments are
// given.
// Original name: _GET_RND.
extern void get_random_int(any range, OPTIONAL(any value)) NATIVE(35);

// Returns a random number from 0 up to range, truncated to a whole number and stored as a float.
// Original name: _GET_RNDF.
extern float get_random_float(float range, ...) NATIVE(35, 1);

// Stores value in a monster variable. Indexes 0 to 7 belong to this monster; 8 to 135 are shared
// by all monsters on the floor. An int value is stored as an int and a float as a float.
// Original name: _V_PUSH.
extern void set_monster_var(int index, any value) NATIVE(35, 2);

// Reads a monster variable into value, as an int or a float to match the receiving variable's
// type. Indexes 0 to 7 belong to this monster; 8 to 135 are shared by all monsters on the floor.
// Original name: _V_POP.
extern void get_monster_var(int index, void *value) NATIVE(35, 3);

// Returns the number of script-driven monsters active on the floor, this one included.
// Original name: _GET_MONSTER_NUM.
extern int get_monster_count(void) NATIVE(35, 4);

// Returns this monster's kind number, the number create_monster and get_kind_pos take.
// Original name: _GET_MONSTER_INDEX.
extern int get_monster_kind(void) NATIVE(35, 5);

// Returns the slot of the floor's loaded monster kinds that this monster was made from.
// Original name: _GET_MONSTER_ID.
extern int get_monster_kind_slot(void) NATIVE(41);

// Returns this monster's handle (24 plus its active slot), the number other natives take to pick a
// monster.
// Original name: _GET_USERID.
extern int get_monster_handle(...) NATIVE(35, 7);

// When the player is transformed into a monster and that form matches the one this monster kind
// recognises as its own, returns the form's id; otherwise returns -1.
// Original name: _GET_USER_MONS_ID.
extern int get_player_disguise(void) NATIVE(35, 8);

// Sets the dungeon floor timer back to 0. The timer counts frames.
// Original name: _RESET_TIMER.
extern void reset_floor_timer(void) NATIVE(35, 9);

// Returns the dungeon floor timer, in frames since it was last reset.
// Original name: _GET_TIMER.
extern int get_floor_timer(void) NATIVE(35, 10);

// Spawns a monster of the given kind, which must be loaded on this floor, at x, y, z (the origin
// when omitted). With five arguments the fifth is the yaw; with seven the last three are the X, Y
// and Z rotation in radians.
// Original name: _CREATE_MONSTER.
extern void create_monster(int kind, OPTIONAL(float x, float y, float z, float yaw_or_rot_x, float rot_y, float rot_z)) NATIVE(35, 11);

// Asks the dungeon to run the event script with this number. The dungeon picks the request up on
// its next check and clears it.
// Original name: _RUN_EVENT_SCRIPT.
extern void run_event_script(int event_no) NATIVE(35, 12);

// Writes the world position of the named frame in this monster's model into x, y, z. Fails if no
// frame has that name.
// Original name: _GET_FRAME_POS.
extern void get_frame_pos(string frame_name, float *x, float *y, float *z) NATIVE(35, 13);

// Writes the world position of the named frame, searched across this monster and its linked parts,
// into x, y, z. With chara_name only the linked part with that name is searched. Fails unless
// given four or five arguments.
// Original name: _GET_OBJ_POS.
extern void get_object_pos(any object_name, OPTIONAL(float *x, float *y, float *z, string chara_name)) NATIVE(49);

// Writes the world position of the named frame on the map piece this monster is linked to into x,
// y, z. Fails if the monster is not linked to a map piece or the frame is missing.
// Original name: _GET_MAPOBJ_POS.
extern void get_map_object_pos(any frame_name, float *x, float *y, OPTIONAL(float *z)) NATIVE(50);

// Sets (on = 1) or clears (on = 0) dungeon pause bits given in mask: 1 monster AI, 2 player
// movement, 4 player control, 0x10 monster drawing, 0x20 sky, 0x80 map, 0x100 minimap, 0x200
// pickups, 0x400 player status, 0x800 exit heal, 0x1000 enemy movement, 0x2000 weapon drawing,
// 0x4000 battle music, 0x8000 pad reset, 0x10000 monster names.
// Original name: _SET_PAUSE.
extern void set_pause_flags(enum pause_flags mask, int on, OPTIONAL(any extra3)) NATIVE(51);

// Returns which of the dungeon pause bits in mask are set (the bitwise AND); the bits are the ones
// set_pause_flags lists.
// Original name: _CHECK_PAUSE.
extern int check_pause_flags(enum pause_flags mask, ...) NATIVE(52);

// Returns a saved dungeon bit flag (0 or 1).
// Original name: _GET_BIT_FLAG.
extern int get_dungeon_flag(int flag, ...) NATIVE(53);

// Sets a saved dungeon bit flag to value (0 or 1).
// Original name: _SET_BIT_FLAG.
extern void set_dungeon_flag(int flag, int value) NATIVE(54);

// Returns the kind of the last attack that hit this monster.
// Original name: _GET_ATT_TYPE.
extern int get_last_hit_type(...) NATIVE(35, 20);

// Returns the status bits of the character the player controls: 1 poison, 2 slow, 0x10 power
// boost, and 4, 8, 0x20, 0x40 for other conditions.
// Original name: _GET_USER_ATTR.
extern int get_player_status(void) NATIVE(56);

// Copies reserved image 0 or 1, loaded earlier by the monster's script, over the monster's first
// texture image. Fails if that image was not loaded.
// Original name: _TRANS_RESERV_IMG.
extern void apply_reserved_texture(OPTIONAL(int image)) NATIVE(57);

// Returns which of this monster's status bits in mask are set: 1 poison, 2 slow, 8 and 0x20 greyed
// out by a hit.
// Original name: _GET_STS_ATTR.
extern int get_status_flags(enum monster_status mask, ...) NATIVE(35, 23);

// Stores value in this monster's second variable set, indexes 0 to 31. An int value is stored as
// an int and a float as a float. Fails unless both arguments are given.
// Original name: _V_PUSH2.
extern void set_monster_var2(any index, OPTIONAL(any value)) NATIVE(35, 24);

// Reads this monster's second variable set, indexes 0 to 31, into value as an int or a float to
// match the receiving variable's type. Fails unless both arguments are given.
// Original name: _V_POP2.
extern void get_monster_var2(any index, OPTIONAL(void *value)) NATIVE(35, 25);

// Sets how the player picks a target: 0 locks onto the nearest monster, 2 uses the cycling target
// selection.
// Original name: _SET_LOCKON_MODE.
extern void set_lockon_mode(enum lock_on_mode mode, OPTIONAL(any extra2, any extra3, any extra4, any extra5)) NATIVE(60, 1);

// Sets the scene's motion blur amount; 0 turns it off.
// Original name: _SET_MOTION_BLUR.
extern void set_motion_blur(any amount, OPTIONAL(any extra2, any extra3)) NATIVE(60, 2);

// Shows (show = 1) or hides (show = 0) a piece of a named part of the active maps. Called with two
// arguments (parts_name, show) it shows or hides the whole part.
// Original name: _SET_MAPOBJ_SHOW.
extern void show_map_part(any parts_name, any piece_name, int show) NATIVE(35, 28);

// Returns event state chosen by selector. Only selector 0 is supported: the stopwatch time limit,
// 0 when the stopwatch counts up.
// Original name: _GET_EVENT_INFO.
extern int get_event_info(enum event_info selector, ...) NATIVE(64);

// Plays a looping sound effect from a monster's sound bank for keep_frames frames, then stops it.
// Calling again before then restarts the count, so scripts call it every frame to keep the sound
// going. Monster handles are character numbers 24 to 47 (24 plus the active slot), as
// get_monster_handle and get_monster_in_range give them.
// Original name: _MONS_SE_LOOP.
extern void loop_se_on_monster(any monster, any se, any keep_frames, OPTIONAL(any extra4)) NATIVE(60, 5);

// Sets how this monster's sound effects are mixed: 1 sets volume and pan from its position, 2
// plays them at fixed volume. Any other value lasts one frame, after which the monster goes back
// to 1.
// Original name: _MONS_VOL_CTRL.
extern void set_se_positional(any mode, OPTIONAL(any extra2, any extra3)) NATIVE(35, 31);

// Returns the distance from this monster to the point x, y, z.
// Original name: _GET_DIST.
extern float get_distance_to(float x, float y, float z, ...) NATIVE(60, 10);

// Casts a ray from 100 units above this monster along its facing turned by angle radians, up to
// distance units, and writes how far it gets before hitting map collision into clear_dist
// (distance when nothing is hit).
// Original name: _SEARCH_AREA.
extern void measure_clear_distance(any distance, OPTIONAL(float angle, float *clear_dist)) NATIVE(70, 1);

// Writes this monster's home position, where it was placed unless a script moved it, into x, y, z.
// Original name: _GET_PLACE_POS.
extern void get_home_pos(OPTIONAL(float *x, float *y, float *z)) NATIVE(70, 2);

// Sets this monster's home position.
// Original name: _SET_PLACE_POS.
extern void set_home_pos(any x, any y, any z) NATIVE(70, 3);

// Finds an active monster of the given kind number and writes its position into x, y, z. Only the
// first N active slots are checked, where N is the monster count, and it fails if none matches.
// Original name: _GET_INDEX_POS.
extern void get_kind_pos(any kind, OPTIONAL(float *x, float *y, float *z)) NATIVE(60, 14);

// Writes this monster's position into x, y, z. Fails unless all three are given.
// Original name: _GET_POS.
extern void get_pos(float *x, OPTIONAL(float *y, float *z)) NATIVE(70, 5);

// Moves this monster straight to x, y, z.
// Original name: _SET_POS.
extern void set_pos(any x, any y, any z, OPTIONAL(any extra4, any extra5)) NATIVE(70, 6);

// Writes this monster's rotation in radians into rot_x, rot_y (yaw), rot_z. Called with four
// arguments, the first is an active monster slot (0 to 23, the handle minus 24) and that monster's
// rotation is written to the other three.
// Original name: _GET_ROT.
extern void get_rot(any rot_x, float *rot_y, float *rot_z, OPTIONAL(any extra4)) NATIVE(70, 7);

// Sets this monster's rotation in radians at once and cancels any turn started by turn_to. Fails
// unless all three are given.
// Original name: _SET_ROT.
extern void set_rot(float rot_x, float rot_y, OPTIONAL(float rot_z)) NATIVE(70, 8);

// Starts this monster turning toward yaw (radians) by turn_speed radians per frame. The turn ends
// when the monster faces yaw; get_rot shows progress.
// Original name: _SET_NEXT_ROT.
extern void turn_to(OPTIONAL(float yaw, float turn_speed)) NATIVE(70, 9);

// Starts this monster walking toward x, y, z at speed units per frame. The walk ends within
// arrive_dist units of the point (20 when omitted), and ends at once if the monster is already
// that close; check_move_end polls it.
// Original name: _SET_NEXT_POS.
extern void move_to(any x, OPTIONAL(float y, float z, any speed, float arrive_dist)) NATIVE(70, 10);

// Returns 1 when this monster is within the arrive distance of its move_to point, else 0.
// Original name: _CHK_MOVE_END.
extern int check_move_end(void) NATIVE(70, 11);

// Stops the walk started by move_to by setting the move speed to 0.
// Original name: _RESET_MOVE.
extern void stop_moving(void) NATIVE(70, 12);

// Writes the position of this monster's target, usually the player, into x, y, z, and with a
// fourth argument the distance to it into dist. Fails if there is no target.
// Original name: _GET_TARGET_POS.
extern void get_target_pos(any x, any y, any z, OPTIONAL(float *dist)) NATIVE(70, 13);

// Returns the distance from this monster to its target. Fails if there is no target.
// Original name: _GET_TARGET_DIST.
extern float get_target_dist(void) NATIVE(70, 14);

// Returns the world yaw in radians from this monster toward its target. Compare it with the
// monster's own yaw from get_rot. Fails if there is no target.
// Original name: _GET_TARGET_ANGLE.
extern float get_target_angle(...) NATIVE(70, 15);

// Writes the point distance units from this monster's target in the world direction angle (yaw in
// radians) into x, y, z. Fails if there is no target.
// Original name: _GET_TARGET_REF_POS.
extern void get_pos_near_target(float angle, float distance, float *x, float *y, float *z) NATIVE(70, 16);

// Returns where the point x, y, z lies around this monster's facing: 1 ahead (within 0.8 radians),
// 3 on the negative-angle side, 4 on the positive-angle side, 2 behind (beyond 3.0 radians). When
// the point is closer than distance the result is 0 and the code writes the direction one argument
// further on.
// Original name: _GET_REF_DIR.
extern int get_point_side(float x, float y, float z, float distance) NATIVE(87);

// Writes the point distance units from this monster along its facing turned by angle radians into
// x, y, z.
// Original name: _GET_REFANGLE_POS.
extern void get_pos_from_facing(float angle, float distance, float *x, float *y, float *z) NATIVE(70, 18);

// Writes the rotation of this monster's target in radians into rot_x, rot_y (yaw), rot_z. Fails if
// there is no target.
// Original name: _GET_TARGET_ROT.
extern void get_target_rot(float *rot_x, float *rot_y, float *rot_z) NATIVE(70, 19);

// Returns the world yaw in radians from this monster toward the point x, y, z.
// Original name: _GET_REF_ANGLE.
extern float get_angle_to(float x, float y, float z, ...) NATIVE(70, 20);

// Returns this monster's height above the ground, or 0 when it is standing on the ground.
// Original name: _GET_HIGH.
extern float get_air_height(...) NATIVE(70, 21);

// Writes the position of the nearest other script-driven monster into x, y, z and its distance
// into dist. With no other monster the position is 0, 0, 0 and dist is -1.
// Original name: _GET_NEAR_MONS_POS.
extern void get_nearest_monster(float *x, float *y, float *z, float *dist) NATIVE(70, 22);

// Writes the position this monster's target had on the previous frame into x, y, z. Fails if there
// is no target.
// Original name: _GET_TARGET_OLD_POS.
extern void get_target_old_pos(float *x, float *y, float *z) NATIVE(70, 23);

// Returns how far this monster's target moved since the previous frame, in units. Fails if there
// is no target.
// Original name: _GET_TARGET_SPEED.
extern float get_target_speed(void) NATIVE(70, 24);

// Moves step units from the from point toward the to point and writes the result into x, y, z. If
// the step reaches or passes the to point, x, y, z receive the to point and arrived receives 1,
// else 0.
// Original name: _CALC_MOVE_NEXT_POS.
extern void calc_step_toward(float from_x, float from_y, float from_z, float to_x, float to_y, float to_z, float step, float *x, float *y, float *z, int *arrived) NATIVE(70, 25);

// Gets the direction from the from point to the to point. With one out argument rot_x receives the
// yaw in radians; with three they receive pitch, yaw and 0.
// Original name: _GET_POSREF_ANGLE.
extern void get_angle_between(float from_x, float from_y, float from_z, float to_x, float to_y, float to_z, float *rot_x, OPTIONAL(float *rot_y, float *rot_z)) NATIVE(70, 26);

// Writes the position of the given monster into x, y, z, and with a fifth argument its distance
// from this monster into dist. Monster handles are character numbers 24 to 47 (24 plus the active
// slot), as get_monster_handle and get_monster_in_range give them.
// Original name: _GET_ACTIVE_MONS_POS.
extern void get_active_monster_pos(int monster, float *x, float *y, float *z, OPTIONAL(float *dist)) NATIVE(70, 27);

// Writes the rotation of the given monster in radians into rot_x, rot_y (yaw), rot_z. Monster
// handles are character numbers 24 to 47 (24 plus the active slot), as get_monster_handle and
// get_monster_in_range give them.
// Original name: _GET_ACTIVE_MONS_ROT.
extern void get_active_monster_rot(int monster, float *rot_x, float *rot_y, float *rot_z) NATIVE(98);

// Returns the distance from this monster to the given monster. Monster handles are character
// numbers 24 to 47 (24 plus the active slot), as get_monster_handle and get_monster_in_range give
// them.
// Original name: _GET_ACTIVE_MONS_DIST.
extern float get_active_monster_dist(int monster) NATIVE(99);

// Writes the yaw in radians of the given monster into yaw. Monster handles are character numbers
// 24 to 47 (24 plus the active slot), as get_monster_handle and get_monster_in_range give them.
// Fails unless both arguments are given.
// Original name: _GET_ACTIVE_MONS_ANGLE.
extern void get_active_monster_yaw(any monster, OPTIONAL(any yaw, any extra3)) NATIVE(100);

// Writes the rotation that faces this monster toward the point x, y, z into rot_x (pitch), rot_y
// (yaw) and rot_z (0), in radians. Fails unless all six arguments are given.
// Original name: _GET_REF_ROT.
extern void get_rot_to(any x, OPTIONAL(float y, float z, float *rot_x, float *rot_y, float *rot_z)) NATIVE(70, 31);

// Gets the direction from the from point to the to point (note the to point comes first). With one
// out argument dir_x receives the yaw in radians; with three they receive the unit direction
// vector.
// Original name: _GET_REF_ROT2.
extern void get_dir_between(float to_x, float to_y, float to_z, float from_x, float from_y, float from_z, float *dir_x, OPTIONAL(float *dir_y, float *dir_z)) NATIVE(70, 32);

// Casts a ray from this monster's position in the world direction yaw (radians, not relative to
// its facing), up to distance units, and writes how far it gets before hitting map collision into
// clear_dist (distance when nothing is hit).
// Original name: _FLYING_SEARCH_AREA.
extern void measure_flight_clearance(any distance, OPTIONAL(float yaw, float *clear_dist)) NATIVE(70, 33);

// Returns how high the point x, y, z is above the map ground below it. If no ground is found
// within 300 units below, it returns -3.4e38.
// Original name: _GET_HIGH2.
extern float get_height_above_ground(float x, float y, float z) NATIVE(70, 34);

// Ranks the other script-driven monsters within range units of this monster by distance and
// returns the handle of the one at position rank (0 is the nearest), or -1 if there are not that
// many.
// Original name: _GET_RANGE_MONS_ID.
extern int get_monster_in_range(float range, int rank) NATIVE(70, 35);

// Writes the world position of entry frame 0 to 2, set up by the character data, of a monster into
// x, y, z. monster is a handle (24 to 47) or -1 for this monster. If the entry frame is missing
// the monster's own position is written.
// Original name: _GET_ENTRY_OBJ_POS.
extern void get_entry_object_pos(any monster, OPTIONAL(int entry, float *x, float *y, float *z)) NATIVE(100, 6);

// Casts a ray from the from point to the to point and returns how far it gets before hitting map
// collision (the full length when nothing is hit).
// Original name: _SEARCH_AREA2.
extern float measure_segment_clearance(float from_x, float from_y, float from_z, float to_x, float to_y, float to_z) NATIVE(70, 38);

// Registers the named frame of this monster's model as action object number index, used by natives
// that refer to objects by number. Fails if no frame has that name.
// Original name: _SET_OBJ.
extern void set_action_object(int index, string frame_name) NATIVE(110);

// Only prints a diagnostic message. The arguments disc scripts pass (an int and a float) are
// ignored.
// Original name: _SET_BODY.
extern void set_body(int index, float value) NATIVE(110, 1);

// Obsolete. Prints a debug message and does nothing else; use add_attack_hit instead.
// Original name: _SET_DMG.
extern void set_damage_legacy(void) NATIVE(112);

// Adds a damage hit to this monster that is active while a motion plays between two points, given
// as fractions 0.0 to 1.0 of the motion's length. The hit is a sphere at entry object index, or a
// capsule between index and second_index; its radius is twice the radius argument, or the entry
// object's own size when radius is 0 or less. hit_name names the damage settings, such as "通常攻撃".
// Original name: _SET_DMG2.
extern void add_attack_hit(string hit_name, float radius, string motion_name, float start_ratio, float end_ratio, int index, OPTIONAL(int second_index)) NATIVE(110, 3);

// Makes the named map part follow this monster. Does nothing outside a dungeon map.
// Original name: _LINK_MAP_TO_OBJECT.
extern void link_map_part(string part_name) NATIVE(110, 4);

// Makes this monster ride on the named piece of the named map part, so it moves with that piece.
// Original name: _LINK_OBJECT_TO_PIECE.
extern void ride_map_piece(string part_name, string piece_name) NATIVE(110, 5);

// Loads an effect base by number or name so effects can be created from it; does nothing if it is
// already loaded. level is the load level (-1 by default). is_loaded receives 1 when the base was
// newly loaded and 0 when it was already loaded or failed.
// Original name: _LOAD_EFFECT_SCRIPT.
extern void load_effect(any effect, OPTIONAL(int level, int *is_loaded)) NATIVE(110, 6);

// Sets when a photo of this monster can be taken. With one argument, photo_no, a photo can be
// taken at any time; with four arguments (motion_name, start_ratio, end_ratio, photo_no) only
// while that motion plays between start_ratio and end_ratio, fractions 0.0 to 1.0 of its length.
// Other argument counts do nothing.
// Original name: _SET_SCOOP.
extern void set_photo_window(any photo_no_or_motion, OPTIONAL(float start_ratio, float end_ratio, int photo_no)) NATIVE(110, 7);

// Reads an image file, such as "dungeon/monster/b13a01.img", into reserve slot 0 or 1 of this
// monster so a later texture swap can copy it into the model.
// Original name: _LOAD_RESERV_IMG.
extern void load_reserve_image(int slot, string path) NATIVE(110, 8);

// Sets how many of the monsters nearest their target are allowed to come into sight, from 0 to 23.
// Applies to all monsters on the floor.
// Original name: _SET_PRIORITY_LIMMIT.
extern void set_visible_monster_limit(int count, OPTIONAL(any extra2)) NATIVE(110, 9);

// Turns scene lighting on (1) or off (0) for this monster's model, or for one named frame of it.
// Off draws it with flat neutral colour 128.
// Original name: _SET_MODEL_LIGHT_SWITCH.
extern void set_model_lighting(int on, OPTIONAL(string frame_name)) NATIVE(120);

// Turns off lighting for this monster's model, or one named frame of it, and draws it with a flat
// colour. Components run 0 to 255 with 128 as neutral.
// Original name: _SET_MODEL_LIGHT_COLOR.
extern void set_model_color(float red, OPTIONAL(float green, float blue, float alpha, string frame_name)) NATIVE(120, 1);

// Sets the transparency this monster draws with, from 0.0 (invisible) to 1.0 (opaque).
// Original name: _SET_ALPHA.
extern void set_alpha(float alpha) NATIVE(120, 2);

// Sets this monster's scale on each axis; 1.0 is normal size.
// Original name: _SET_SCALE.
extern void set_scale(float x, float y, float z) NATIVE(120, 3);

// Sets the transparency, 0.0 to 1.0, of the first monster on the floor whose kind number is
// monster_kind.
// Original name: _SET_INDEX_ALPHA.
extern void set_monster_kind_alpha(int monster_kind, float alpha) NATIVE(124);

// Makes this monster's colour pulse toward red, green, blue (0 to 255) pulse_count times over each
// cycle of frames. repeats is the number of extra cycles, -1 to repeat forever, 0 by default.
// Original name: _SET_PALLET_ANIM.
extern void start_color_pulse(int red, int green, int blue, int pulse_count, int frames, OPTIONAL(int repeats)) NATIVE(120, 5);

// Stops the colour pulse started with start_color_pulse on this monster.
// Original name: _RESET_PALLET_ANIM.
extern void stop_color_pulse(OPTIONAL(any extra1)) NATIVE(120, 6);

// Sets (on = 1) or clears (on = 0) behaviour bits on this monster: 1 no lock-on, 4 walking
// replaces velocity, 8 ignore map collision, 16 not pushed by other monsters, 32 takes no damage
// and cannot be stunned, 64 vanishes at once on death, 256 skip ground search, 512 always counts
// as in sight.
// Original name: _SET_ATTRIB.
extern void set_attrib(OPTIONAL(enum monster_attrib mask, int on)) NATIVE(120, 7);

// Sets (on = 1, the default) or clears (on = 0) status bits on this monster's scene character
// slot.
// Original name: _SET_STATUS.
extern void set_scene_status(any mask, OPTIONAL(int on)) NATIVE(110, 18);

// Sets (on = 1) or clears (on = 0) mask flag bits on this monster. Bit 1 stops it guarding hits;
// bit 2 stops it dodging attacks.
// Original name: _SET_INT_FLAG.
extern void set_mask_flag(enum mask_flags mask, int on) NATIVE(120, 9);

// Sets the status value of this monster that other characters' action scripts read.
// Original name: _SET_ACT_STATUS.
extern void set_act_status(int status) NATIVE(120, 10);

// Makes this monster take no damage for the given number of steps; 0 ends it.
// Original name: _SET_MUTEKI.
extern void set_invincible_time(int frames) NATIVE(120, 11);

// Unused stub. Does nothing and reports failure.
// Original name: _SET_GRAVITY.
extern void set_gravity(void) NATIVE(132);

// Unused stub. Does nothing and reports failure.
// Original name: _SET_COLLISION.
extern void set_collision(void) NATIVE(133);

// Returns how many more hits it takes to enrage this monster; -1.0 for a boss.
// Original name: _GET_GEKIRIN.
extern float get_rage_hits_left(void) NATIVE(120, 14);

// Returns this monster's rank by distance to its target, 0 for the nearest monster, -1 for none.
// Original name: _GET_PRIORITY.
extern int get_priority(void) NATIVE(120, 15);

// Sets the distance within which this monster comes into sight and shows its life gauge. The
// argument is multiplied by 20 to give world units; scripts pass 999999.0 to always show it.
// Original name: _SET_CLIP_DIST.
extern void set_clip_dist(float dist) NATIVE(120, 16);

// Shows the stars circling this monster's head for the rest of its current stun.
// Original name: _SET_PIYORI_MARK.
extern void show_stun_stars(void) NATIVE(120, 17);

// Returns how many steps the stun stars over this monster still show; 0 when it is not stunned.
// Original name: _CHECK_PIYORI.
extern int get_stun_stars_left(void) NATIVE(120, 18);

// Reads this monster's scale on each axis.
// Original name: _GET_SCALE.
extern void get_scale(float *x, float *y, float *z) NATIVE(120, 19);

// Returns the radius this monster uses for map collision, or 15.0 when it has none.
// Original name: _GET_MONS_WIDTH.
extern float get_body_radius(...) NATIVE(140);

// Knocks this monster back at speed (scaled by its knockback rate), slowing by decel each step,
// for the given number of steps.
// Original name: _BLOW_START.
extern void start_knockback(float speed, float decel, int frames) NATIVE(150);

// Marks this monster dead, stops its colour pulse, and scatters its reward money, its
// transformation badge if it has one, and random drop items, playing a sound.
// Original name: _SET_DEAD_START.
extern void start_death_drops(OPTIONAL(any extra1)) NATIVE(150, 1);

// Starts this monster's fade-out, plays its death smoke unless attribute bit 2 is set, and
// scatters weapon experience pickups for the character that dealt the last hit.
// Original name: _SET_DEAD_OFF.
extern void finish_death(OPTIONAL(any extra1, any extra2)) NATIVE(150, 2);

// Clears this monster's held or thrown state after it has been caught or thrown.
// Original name: _SET_SHROW_END.
extern void end_throw(void) NATIVE(150, 3);

// Returns this monster's current attack power.
// Original name: _GET_BASE_ATTACK.
extern int get_attack(...) NATIVE(160);

// Sets this monster's defence to its table defence multiplied by rate.
// Original name: _SET_DEF_RATE.
extern void set_defense_rate(float rate, OPTIONAL(any extra2)) NATIVE(160, 1);

// Sets this monster's life. An int sets it directly; a float sets it as a fraction of max life.
// The result is clamped to 0 through max life, and a value above 0 marks the monster alive again.
// Original name: _SET_MONSTER_LIFE.
extern void set_life(any life, OPTIONAL(any extra2, any extra3)) NATIVE(160, 2);

// Reads this monster's life into an int variable, or its life as a fraction 0.0 to 1.0 of max life
// into a float variable.
// Original name: _GET_MONSTER_LIFE.
extern void get_life(void *life) NATIVE(160, 3);

// Returns how many hits on this monster did no damage.
// Original name: _GET_NO_DAMAGE_CNT.
extern int get_no_damage_count(...) NATIVE(160, 4);

// Returns a monster's current life. Scene character number of a monster (its slot plus 24), or -1
// for this monster.
// Original name: _GET_ACTIVE_MONS_LIFEI.
extern int get_monster_life(any monster, ...) NATIVE(160, 5);

// Returns a monster's life as a fraction 0.0 to 1.0 of its max life. Scene character number of a
// monster (its slot plus 24), or -1 for this monster.
// Original name: _GET_ACTIVE_MONS_LIFEF.
extern float get_monster_life_ratio(int monster) NATIVE(160, 6);

// Adds amount to a monster's life, negative to take life, clamped to 0 through max life. Scene
// character number of a monster (its slot plus 24), or -1 for this monster.
// Original name: _SET_ACTIVE_MONS_LIFEI.
extern void add_monster_life(int monster, int amount) NATIVE(160, 7);

// Heals a monster by a fraction of its max life, capped at max life; negative fractions do
// nothing. Scene character number of a monster (its slot plus 24), or -1 for this monster.
// Original name: _SET_ACTIVE_MONS_LIFEF.
extern void heal_monster_life_ratio(int monster, float ratio) NATIVE(160, 8);

// Returns a monster's max life. Scene character number of a monster (its slot plus 24), or -1 for
// this monster.
// Original name: _GET_ACTIVE_MONS_MAX_LIFE.
extern int get_monster_max_life(int monster) NATIVE(160, 9);

// Pops up a damage number above a monster. With color = 1 the number is drawn green. Scene
// character number of a monster (its slot plus 24), or -1 for this monster.
// Original name: _SET_DAMAGE_SCORE.
extern void show_damage_number(int monster, int value, OPTIONAL(enum damage_number_color color)) NATIVE(160, 10);

// Returns a monster's grade from its kind's table. Scene character number of a monster (its slot
// plus 24), or -1 for this monster.
// Original name: _GET_MONS_GRADE.
extern int get_monster_grade(any monster, ...) NATIVE(160, 11);

// Sets a monster's chance to guard a hit to its table chance multiplied by rate, capped at 100
// percent. Scene character number of a monster (its slot plus 24), or -1 for this monster.
// Original name: _SET_GUARD_RATE.
extern void set_guard_rate(int monster, float rate) NATIVE(160, 12);

// Sets both of a monster's chances to dodge its target's attacks to their table values multiplied
// by rate, capped at 100 percent. Scene character number of a monster (its slot plus 24), or -1
// for this monster.
// Original name: _SET_ESCAPE_RATE.
extern void set_dodge_rate(int monster, float rate) NATIVE(160, 13);

// Sets the damage percentages of the attack kinds picked by mask (bits 0 to 11) to their table
// values multiplied by rate, capped at 100. Scene character number of a monster (its slot plus
// 24), or -1 for this monster.
// Original name: _SET_EXT_PARAM_RATE.
extern void set_damage_kind_rate(int monster, enum damage_kind_mask mask, float rate, OPTIONAL(any extra4)) NATIVE(170, 4);

// Returns nonzero when this monster is a boss, whose life shows across the foot of the screen.
// Original name: _GET_BOSS_FLAG.
extern int is_boss(void) NATIVE(160, 15);

// Sets the size of one of this monster's entry objects, the frames used as hit points; index -1
// means 0.
// Original name: _SET_INDEXOBJ_SIZE.
extern void set_entry_object_size(int index, float size, OPTIONAL(any extra3, any extra4)) NATIVE(170, 6);

// Returns the size of one of this monster's entry objects; index -1 means 0.
// Original name: _GET_INDEXOBJ_SIZE.
extern float get_entry_object_size(int index) NATIVE(160, 17);

// Stops the motion of this monster and every part chained to it.
// Original name: _RESET_MOTION.
extern void reset_motion(void) NATIVE(180);

// Plays the named motion on this monster and every part chained to it. speed is frames advanced
// per step; -1 keeps the current speed. flags: 1 pause, 2 hold on the last frame instead of
// looping, 4 restart without blending.
// Original name: _SET_MOS.
extern void set_motion(any motion_name, OPTIONAL(float speed, enum motion_flags flags)) NATIVE(180, 2);

// Reads 1 into is_done when the motion of this monster, or of its named chained part, reaches its
// last frame within two steps or no motion plays.
// Original name: _CHECK_MOS_END.
extern void check_motion_end(int *is_done, OPTIONAL(string part_name)) NATIVE(180, 3);

// Reads how far the current motion of this monster, or of its named chained part, has played, from
// 0.0 to 1.0.
// Original name: _NOW_MOS_WAIT.
extern void get_motion_progress(float *progress, OPTIONAL(string part_name)) NATIVE(180, 4);

// Reads the motion state of this monster, or of its named chained part: 0 none, 1 on first frame,
// 2 playing, 3 blending, 4 reached last frame.
// Original name: _GET_MOS_STATUS.
extern void get_motion_status(int *status, OPTIONAL(string part_name)) NATIVE(180, 5);

// Starts the named effect, owned by this monster. With slot given it receives the effect's slot
// number for the other effect natives.
// Original name: _ESM_CREATE.
extern void create_effect(string effect_name, OPTIONAL(int *slot)) NATIVE(190, 1);

// Tells this monster's effect in slot to run its finishing program (300) so it ends on its own.
// Original name: _ESM_FINISH.
extern void finish_effect(int slot) NATIVE(190, 2);

// Removes this monster's effect in slot at once.
// Original name: _ESM_DELETE.
extern void delete_effect(int slot) NATIVE(193);

// Sets the first work vector of this monster's effect in slot, -1 for the last started one;
// effects use it as a position or direction.
// Original name: _ESM_SET_VECT1.
extern void set_effect_vec1(int slot, float x, float y, float z) NATIVE(190, 4);

// Reads the first work vector of this monster's effect in slot, -1 for the last started one.
// Original name: _ESM_GET_VECT1.
extern void get_effect_vec1(int slot, float *x, float *y, float *z) NATIVE(195);

// Sets the second work vector of this monster's effect in slot, -1 for the last started one.
// Original name: _ESM_SET_VECT2.
extern void set_effect_vec2(int slot, float x, float y, float z) NATIVE(190, 6);

// Reads the second work vector of this monster's effect in slot, -1 for the last started one.
// Original name: _ESM_GET_VECT2.
extern void get_effect_vec2(int slot, float *x, float *y, float *z) NATIVE(197);

// Sets the target character (scene character number) of this monster's effect in slot, -1 for the
// last started one.
// Original name: _ESM_SET_TARGET_ID.
extern void set_effect_target(int slot, int target) NATIVE(190, 8);

// Returns the target character of this monster's effect in slot, -1 for the last started one.
// Original name: _ESM_GET_TARGET_ID.
extern int get_effect_target(int slot) NATIVE(199);

// Sets the owner id of this monster's effect in slot, -1 for the last started one.
// Original name: _ESM_SET_USER_ID.
extern void set_effect_owner(int slot, int owner) NATIVE(190, 10);

// Returns the owner id of this monster's effect in slot, -1 for the last started one.
// Original name: _ESM_GET_USER_ID.
extern int get_effect_owner(int slot) NATIVE(201);

// Sets script value number index of this monster's effect in slot, -1 for the last started one, to
// an int or float.
// Original name: _ESM_SET_VALUE.
extern void set_effect_value(int slot, int index, any value) NATIVE(190, 12);

// Adds a sword trail to this monster using its trail effect 0 to 2. The trail starts while
// motion_name plays between start_ratio and end_ratio (fractions 0.0 to 1.0 of its length) and is
// drawn between the frames named start_frame and end_frame. length is the number of recorded
// positions drawn, hold_frames the steps before it fades, fade_frames the steps the fade takes.
// Original name: _SW_EFFECT.
extern void add_sword_trail(int trail, string motion_name, float start_ratio, float end_ratio, string start_frame, string end_frame, int length, int hold_frames, int fade_frames) NATIVE(190, 13);

// Returns the next free effect texture block, or -1 when the pool is full.
// Original name: _ESM_GET_NOTUESD_TEXB.
extern int get_free_texture_block(void) NATIVE(190, 14);

// Marks the next effect texture block as used by the current load level.
// Original name: _ESM_ADD_TEXB.
extern void use_texture_block(void) NATIVE(190, 15);

// Fires a homing rocket from x, y, z in direction dir_x, dir_y, dir_z. speed defaults to 12.0,
// homing_delay to 6 steps, homing_time to 45 steps and damage to this monster's attack power; give
// all four or none.
// Original name: _SHOT_ROCKET_LAUNCHER.
extern void fire_rocket(float x, float y, float z, float dir_x, float dir_y, float dir_z, OPTIONAL(float speed, int homing_delay, int homing_time, int damage)) NATIVE(190, 16);

// Stops every effect that belongs to the given owner id.
// Original name: _ESM_ALL_CLEAR.
extern void clear_owner_effects(int owner) NATIVE(190, 17);

// Numbers the disc calls that this table never registers. The engine prints "not found ext" and
// moves on, so these calls do nothing.
extern void _EXT_17(...) NATIVE(17);
extern void _EXT_18(...) NATIVE(18);
extern void _EXT_20(...) NATIVE(20);
extern void _EXT_67(...) NATIVE(60, 7);
extern void _EXT_68(...) NATIVE(60, 8);
extern void _EXT_69(...) NATIVE(60, 9);
extern void _EXT_141(...) NATIVE(140, 1);
extern void _EXT_142(...) NATIVE(140, 2);
extern void _EXT_143(...) NATIVE(140, 3);
extern void _EXT_154(...) NATIVE(154);
extern void _EXT_155(...) NATIVE(155);
extern void _EXT_157(...) NATIVE(157);
extern void _EXT_159(...) NATIVE(159);

#include <helpers/monster.h>

#endif
