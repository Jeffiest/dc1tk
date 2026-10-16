// c2s SDK. Natives for character action scripts (mainchr, dungeon/act_script). They act on the character running the script.
//
// Natives are engine functions, declared extern with their number in
// NATIVE(...). A native with one result returns it (count = get_counter(3););
// natives with several results take pointers (get_actor_pos(0, &x, &y, &z);).
// Natives push nothing on the VM stack: a native in an expression is called
// first, into a hidden local, and the expression reads that.
// OPTIONAL(...) lists arguments a call may leave out. Natives never wait; scripts
// wait with `wait;`, which yields one frame (25 a second on PAL). While the player
// skips a cutscene the engine does not run natives until the script reaches
// `skip_end;`.
//
// NATIVE(B, N) means the game's scripts call the native as B + N. A call can
// pick another form with an AT(B) or DIRECT prefix.

#ifndef DC2TK_ACTION_H
#define DC2TK_ACTION_H

#include <c2s.h>
#pragma natives action

enum program {
    // Runs once when the character's action script is set up (actionchara.cpp
    // CActionChara::InitScript, ACTION_PROG_INIT), before 200 takes over.
    init = 100,
    // Runs when the character is put back into its normal state (actionchara.cpp
    // CActionChara::ResetAction, ACTION_PROG_RESET, also reached from the event native that resets a
    // character), before 200 takes over.
    reset = 150,
    // Normal control loop: resumed every frame and restarted whenever the running program ends
    // (actionchara.cpp CActionChara::RunScript, ACTION_PROG_MAIN).
    update = 200,
    // Runs when a hit asks for a small damage reaction (actionchara.cpp ACTION_DAMAGE_REQ_SMALL,
    // ACTION_PROG_DAMAGE_SMALL).
    damage = 500,
    // Runs when the character is frozen by the stop status, from a hit with that reaction or from the
    // menu while the status is set (actionchara.cpp SetHold and ACTION_DAMAGE_REQ_HOLD, status bits
    // 0x8/0x20); retail bodies hold the damage pose until it clears.
    stopped = 550,
    // Runs when a hit asks for a large damage reaction (actionchara.cpp ACTION_DAMAGE_REQ_LARGE,
    // ACTION_PROG_DAMAGE_LARGE); retail bodies blow the character away and get up.
    knockdown = 600,
    // Runs after the character's melee hit is guarded by a monster or deals no damage (monster.cpp
    // sets melee_hit, actionchara.cpp switches to 700); retail bodies play the after-guard recoil
    // motion.
    blocked = 700,
    // Runs when the character's HP reaches zero (actionchara.cpp ACTION_DAMAGE_REQ_DEAD,
    // ACTION_PROG_DEAD), which also queues dungeon event 1200.
    death = 1400,
    // Runs when Max or Monica lands hard after falling at 3.5 units per frame or faster
    // (actionchara.cpp ACTION_PROG_LAND).
    land = 1500,
};

#define PI 3.1415927

// Charge effect modes of _SET_ACCUME_FLAG: 0 stops it and clears the hold counter, 1 starts it.
enum charge_effect_mode {
    CHARGE_EFFECT_OFF = 0,
    CHARGE_EFFECT_START = 1,
};

// Equipment slots of a character (userdata.hpp CHARA_DATA::equip); 0 and 1 are the two weapons, 2
// the hat model.
enum equip_slot {
    EQUIP_MAIN_WEAPON = 0,
    EQUIP_SUB_WEAPON = 1,
    EQUIP_HAT = 2,
    EQUIP_SHOES = 3,
    EQUIP_BODY = 4,
};

// Murderous mode that picks which escape rate the target monster rolls (escape_rate0 for melee,
// escape_rate1 for shots).
enum menace_level {
    MENACE_MELEE = 0,
    MENACE_SHOT = 1,
};

// Motion playback flags (character.hpp CharaMotionFlag): pause, stop on the last frame, restart
// without blending.
enum motion_flags {
    CHARA_MOTION_PAUSE = 1,
    CHARA_MOTION_HOLD = 2,
    CHARA_MOTION_RESTART = 4,
};

// Logical button numbers of CPadControl (padcontrol.hpp PadCtrlButton, bindings from mainloop.cpp
// pad_table).
enum pad_button {
    PAD_BTN_CONFIRM = 0,
    PAD_BTN_CANCEL = 1,
    PAD_BTN_R1_HELD = 2,
    PAD_BTN_L1_HELD = 3,
    PAD_BTN_L2 = 4,
    PAD_BTN_MENU = 5,
    PAD_BTN_RIGHT = 9,
    PAD_BTN_LEFT = 10,
    PAD_BTN_START = 15,
    PAD_BTN_PAUSE = 21,
    PAD_BTN_EVENT_SKIP = 22,
    PAD_BTN_QUICK_CHANGE = 23,
    PAD_BTN_ACTION_CONFIRM = 50,
    PAD_BTN_ACTION_SQUARE = 51,
    PAD_BTN_ACTION_CANCEL = 52,
    PAD_BTN_ACTION_L1_HELD = 53,
    PAD_BTN_ACTION_R1_HELD = 54,
    PAD_BTN_ACTION_SELECT = 55,
    PAD_BTN_ACTION_HELD = 56,
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

// Sound bank selector of _SE_PLAY and _SE_LOOP_PLAY; only -1, the character's own bank, plays.
enum se_bank {
    SE_BANK_OWN = -1,
};

// Clears the character's action setup: registered object slots, body collisions, damage entries,
// motion sound triggers and sword trails, and drops whatever it holds and any pending impulse or
// knockback. Arguments are ignored.
// Original name: _INIT_SCRIPT.
extern void reset_action_setup(OPTIONAL(void *ignored)) NATIVE(0);

// Stores a 16-bit value on the character that the action script keeps for its own use between
// program runs, such as a combo step counter. It starts at 0.
// Original name: _PROG_SET.
extern void set_saved_value(any value) NATIVE(1);

// Returns the value last stored with set_saved_value.
// Original name: _PROG_GET.
extern int get_saved_value(void) NATIVE(2);

// Returns the character's attack type, taken from its setup data. The robot script branches on it
// to pick the arm weapon's attacks (0, 20 and so on).
// Original name: _GET_ATTK_TYPE.
extern int get_attack_type(void) NATIVE(3);

// Returns the character's movement type: 0 on foot, 1 or 4 ridepod legs, 2 or 5 ridepod treads, 3
// ridepod wheels (or a monster), 6 or 7 ridepod flight.
// Original name: _GET_MOVE_TYPE.
extern int get_move_type(void) NATIVE(4);

// Sets the character's movement speed used by the stick movement handlers. A value of 0 or less
// restores the default of 3.0.
// Original name: _SET_MOVE_SPEED.
extern void set_move_speed(float speed) NATIVE(5);

// Pulses the character's lighting toward an RGB colour (0 to 255 each). pulses is how many pulses
// one cycle holds, frames is the cycle length (0 turns it off), and repeats is how many more
// cycles follow (-1 repeats forever, default 0).
// Original name: _SET_PALLET.
extern void flash_chara_color(int red, any green, OPTIONAL(int blue, int pulses, int frames, int repeats)) NATIVE(30);

// Returns the item number equipped in an equipment slot of the active character.
// Original name: _CHECK_EQUIP.
extern int get_equip_item(enum equip_slot slot, ...) NATIVE(31);

// Shakes the dungeon camera with the given strength, fading linearly to nothing over the given
// number of frames.
// Original name: _CAMERA_QUAKE.
extern void shake_camera(float power, any frames, OPTIONAL(any extra3, any extra4, any extra5, any extra6)) NATIVE(32);

// Returns the dungeon pause flags ANDed with mask, so it is nonzero when any of those flags is
// set. For example 8192 (0x2000) is set while weapons are hidden.
// Original name: _CHECK_PAUSE.
extern int check_pause_flag(enum pause_flags mask, ...) NATIVE(33);

// Returns the active character's status ailment bits (such as poison or slow).
// Original name: _GET_STATUS_ATTR.
extern int get_status_flags(void) NATIVE(34);

// Plays a sound effect from the character's own sound bank. bank must be -1 (the character's
// bank); any other value fails without playing.
// Original name: _SE_PLAY.
extern void play_chara_se(enum se_bank bank, int se_no, OPTIONAL(any extra3)) NATIVE(35);

// Keeps a looping sound effect from the character's own sound bank playing for keep_frames more
// frames; call it again before they run out to sustain it. bank must be -1 (the character's bank).
// Original name: _SE_LOOP_PLAY.
extern void play_chara_loop_se(OPTIONAL(enum se_bank bank, int se_no, int keep_frames)) NATIVE(36);

// Returns the attack type of the active character's sub weapon (equipment slot 1), for example 0
// or 11 normal gun, 10 grenade, 20 laser, 30 machine gun.
// Original name: _GET_SHOT_TYPE.
extern int get_shot_type(...) NATIVE(37);

// Returns the monster id of the active character's monster transformation, or -1 when the active
// character is not a transformed monster.
// Original name: _GET_MONS_ID.
extern int get_monster_form(void) NATIVE(38);

// Writes the character's facing direction vector into three float out parameters.
// Original name: _GET_FRONT_VEC.
extern void get_front_vec(float *x, float *y, float *z) NATIVE(39);

// Returns the bitmask of controller buttons currently held.
// Original name: _GET_PADON.
extern int get_pad_held(...) NATIVE(40);

// Returns the bitmask of controller buttons pressed this frame.
// Original name: _GET_PADDOWN.
extern int get_pad_pressed(void) NATIVE(41);

// Returns the bitmask of controller buttons released this frame.
// Original name: _GET_PADUP.
extern int get_pad_released(...) NATIVE(42);

// Returns the state of a logical game button (nonzero when it triggers this frame). Action buttons
// are 50 attack (confirm, pressed), 51 square, 52 cancel and 56 attack held.
// Original name: _GET_BTN.
extern int get_pad_button(enum pad_button button, ...) NATIVE(43);

// Returns the attack press history: every frame the attack button (logical button 50) state is
// ORed in, so it is nonzero if attack was pressed since the last reset_attack_presses.
// Original name: _GET_PAD_HISTORY.
extern int get_attack_presses(void) NATIVE(45);

// Clears the attack press history read by get_attack_presses. Arguments are ignored.
// Original name: _RESET_PAD_HISTORY.
extern void reset_attack_presses(OPTIONAL(any ignored)) NATIVE(46);

// Returns how many frames in a row the attack button (logical button 56) has been held; it drops
// to 0 when released.
// Original name: _GET_ACUMU_PAD.
extern int get_attack_hold_frames(void) NATIVE(47);

// Sets the attack hold frame counter read by get_attack_hold_frames back to 0.
// Original name: _RESET_ACUMU_PAD.
extern void reset_attack_hold_frames(void) NATIVE(48);

// Runs one frame of normal stick movement for the character: on-foot walking and running for a
// human, or monster-form movement for a monster type. Other movement types do nothing.
// Original name: _RUN_MAIN_MOVE.
extern void run_stick_move(void) NATIVE(49);

// Runs one frame of stick movement while carrying something overhead: slower movement with the
// lift-stand and lift-walk motions, turning toward the locked-on target when standing still.
// Original name: _RUN_SHROW_MOVE.
extern void run_carry_move(void) NATIVE(50);

// Runs one frame of slow stick movement while charging an attack, switching between the charge
// stand and charge walk motions.
// Original name: _RUN_TAME_MOVE.
extern void run_charge_move(void) NATIVE(51);

// Runs one frame of slow stick movement while aiming or guarding. It plays stand_motion and
// switches to move_motion while the stick moves, turning toward the locked-on target if there is
// one. Both names are required; calls without them do nothing.
// Original name: _RUN_HOLD_MOVE.
extern void run_aim_move(OPTIONAL(string stand_motion, string move_motion)) NATIVE(52);

// Writes the character's world position into three float out parameters.
// Original name: _GET_POS.
extern void get_chara_pos(float *x, float *y, float *z) NATIVE(53);

// Returns, as a float, the dot product of the camera-relative left stick direction and the
// character's facing: near 1 when pushing forward, near -1 when pushing backward.
// Original name: _CHECK_FRONT_KEY.
extern float get_stick_alignment(void) NATIVE(54);

// Returns 1 when the character is locked on and the left stick points within about 72 degrees of
// straight behind it, otherwise 0.
// Original name: _CHECK_BACK_KEY.
extern int check_back_key(void) NATIVE(55);

// Turns the character to face opposite its current knockback direction, toward where the hit came
// from.
// Original name: _SET_BLOW_ANGLE.
extern void face_blow_source(void) NATIVE(56);

// Pushes the character along its facing for frames frames, starting at speed and slowing by decel
// each frame. angle rotates the push direction from the facing, in degrees (default 0).
// Original name: _SET_BLOW_MOVE.
extern void set_move_impulse(float speed, float decel, int frames, OPTIONAL(float angle)) NATIVE(57);

// Starts knocking the character back along its knockback direction for frames frames. speed is
// scaled by the character's knockback rate and drops by decel each frame.
// Original name: _BLOW_START.
extern void start_knockback(float speed, float decel, int frames) NATIVE(58);

// Allows (1) or blocks (0) opening the menu and starting field events this frame. The engine
// clears it every frame, so the script sets it each frame it wants to allow them; holding
// something also blocks them.
// Original name: _SET_MENU_FLAG.
extern void set_menu_allowed(int allowed) NATIVE(59);

// Runs one frame of ridepod stick movement for the current movement type (legs, treads, wheels or
// flight). With all_parts nonzero the walk motions are applied to the whole linked model chain,
// otherwise only to the body.
// Original name: _RUN_ROBO_MOVE.
extern void run_ridepod_move(int all_parts) NATIVE(60);

// Writes the character's rotation into three float out parameters, in radians.
// Original name: _GET_ROT.
extern void get_chara_rot(float *x, float *y, float *z) NATIVE(61);

// Registers an attack hitbox swept between two model frames while a motion plays (up to 11).
// hit_name names the damage entry, radius is doubled for the hit size, power_rate scales damage,
// and start_ratio and end_ratio are positions in the motion from 0.0 to 1.0. chara_name picks a
// linked model part whose motion is checked instead of the character's.
// Original name: _SET_DMG2.
extern void add_attack_hit(string frame_a, string frame_b, string hit_name, float radius, float power_rate, string motion_name, float start_ratio, float end_ratio, OPTIONAL(string chara_name)) NATIVE(71);

// Registers the named model frame in object slot 0 to 7 (or the first free slot with -1). Other
// natives refer to the frame by this slot number.
// Original name: _SET_OBJ.
extern void set_object_slot(int slot, string frame_name) NATIVE(72);

// Adds a body collision sphere centred on a registered object slot. The radius argument is
// doubled.
// Original name: _SET_BODY.
extern void add_body_collision(int slot, float radius) NATIVE(73);

// Schedules a sword trail between two model frames while a motion plays between start_ratio and
// end_ratio (0.0 to 1.0 of the motion). trail_slot is 0 to 2, length is the trail's opaque point
// count, hold_frames delays the fade and fade_frames is how long it fades. chara_name picks a
// linked model part whose motion and frames are used.
// Original name: _SW_EFFECT.
extern void add_sword_trail(int trail_slot, string motion_name, float start_ratio, float end_ratio, string frame_a, string frame_b, int length, int hold_frames, int fade_frames, OPTIONAL(string chara_name)) NATIVE(75);

// Registers a sound trigger for the window between start_ratio and end_ratio (0.0 to 1.0) of a
// motion, up to 10. The disc uses ids like 400 for attacks and -2 or -3 on walk and run motions.
// chara_name picks a linked model part whose motion is used.
// Original name: _SET_SND.
extern void add_motion_se(int se_no, string motion_name, float start_ratio, float end_ratio, OPTIONAL(string chara_name)) NATIVE(76);

// Sets the object slot the charge effect is drawn at and its effect number, and marks the charge
// effect inactive.
// Original name: _SET_ACCUME_FX.
extern void set_charge_effect_slot(int slot, int effect_no) NATIVE(77);

// Changes the charge effect mode. 1 starts it at the slot from set_charge_effect_slot, 0 stops it
// and clears the attack hold counter, 3 and 4 set that mode and mark it inactive, other values
// only set the mode.
// Original name: _SET_ACCUME_FLAG.
extern void set_charge_effect_mode(enum charge_effect_mode mode) NATIVE(78);

// Returns the status value that the locked target monster's own script has published, or 0 when
// there is no target.
// Original name: _GET_MONSTER_NOWSTS.
extern int get_target_status(void) NATIVE(90);

// Sets a menace level (0 or 1) on the character for frames frames. While it lasts, the monster it
// targets may roll to flee using that monster's escape rate for the level.
// Original name: _SET_MURDEROUS.
extern void set_menace(enum menace_level level, int frames) NATIVE(91);

// Returns, as a float, the distance from the character to its target, or -1 when there is no
// target.
// Original name: _GET_TRG_DISTANCE.
extern float get_target_distance(void) NATIVE(92);

// Turns the character toward its target monster when it is closer than twice radius plus the
// monster's body width. Each call turns it 90/turn_divisor degrees toward the target, snapping
// when closer than that. Both arguments are required; calls with fewer do nothing.
// Original name: _SET_TRG_ANGLE.
extern void turn_to_target(OPTIONAL(float radius, float turn_divisor)) NATIVE(93);

// Turns guarding on (1) or off (0). While guarding, incoming damage is scaled by the hit's guard
// rate, unless the hit ignores guard, which also turns guarding off.
// Original name: _SET_GUARD_FLAG.
extern void set_guarding(int on) NATIVE(94);

// Makes the character invincible for the given number of frames; 0 ends it.
// Original name: _SET_MUTEKI.
extern void set_invincible(int frames) NATIVE(95);

// Returns what the character holds: 0 nothing, 1 an item to throw, 3 a caught monster, 4 a stone
// from the map.
// Original name: _CHECK_HAND_OBJ.
extern int get_held_type(void) NATIVE(96);

// Uses the active item and returns the result: 0 nothing happened, 1 the item was used (healing
// items flash the character), 2 the item is a throwable now held, 3 the active item slot is 3.
// Original name: _SET_ITEM_USED.
extern int use_active_item(void) NATIVE(97);

// Throws the held item 120 units ahead of the character and removes one from the inventory.
// Original name: _THROW_HAND_OBJECT.
extern void throw_held_item(void) NATIVE(98);

// With only object_name, grabs a monster or a map stone near that model frame if possible (a
// grabbed monster costs 5% HP). With all three arguments it checks for a stone within 30 units (40
// with pick_up nonzero, which also picks it up) and writes 1 or 0 into found.
// Original name: _CHECK_CATCH.
extern void try_grab(string object_name, OPTIONAL(int pick_up, int *found)) NATIVE(99);

// Lets go of whatever the character holds. With throw = 1 a caught monster is thrown ahead (up to
// 120 units, toward the locked target) and a held item is kept for the throw; with 0 or no
// argument a held item is discarded and a monster is dropped.
// Original name: _RELEASE_OBJ.
extern void release_held(OPTIONAL(int throw, any extra2, any extra3)) NATIVE(100);

// Fires the active character's sub weapon from an object slot: Max's gun by its type, or Monica's
// magic with damage scaled by scale (default 1.0). The second and third arguments are read but
// unused, and wait_frames sets the delay before the next shot. With an empty gun it plays a click
// instead.
// Original name: _SET_SHOT.
extern void fire_sub_weapon(any slot, OPTIONAL(string unused_name, int unused, int wait_frames, float scale)) NATIVE(101);

// Fires the ridepod's arm weapon (rocket, machine gun, missile, cannon or beam by attack type),
// alternating muzzles. For the beam type left nonzero picks the left muzzle.
// Original name: _SHOT.
extern void fire_ridepod_weapon(any left) NATIVE(102);

// Writes the world position of a named model frame of the character into three float out
// parameters.
// Original name: _GET_OBJECT_POS.
extern void get_object_pos(any object_name, OPTIONAL(float *x, float *y, float *z)) NATIVE(103);

// Lets the character aim its gun at the locked-on target this frame; the engine clears it every
// frame.
// Original name: _SET_DIR_GUN.
extern void enable_gun_aim(void) NATIVE(104);

// Fires the magic stored in the sword from the named model frame toward the target, using the
// stored element and power, then clears it. Does nothing when no magic is stored or a shot was
// fired in the last 5 frames.
// Original name: _SET_SPECIAL_SHOT.
extern void fire_sword_magic(any object_name) NATIVE(105);

// Returns, as a float, current HP divided by maximum HP. The division is done in integers, so it
// is 1.0 at full HP and 0.0 otherwise.
// Original name: _GET_NOW_HP_RATE.
extern float get_hp_rate(void) NATIVE(106);

// Sets the active character's HP to 5% of maximum.
// Original name: _SET_BOMB.
extern void set_low_hp(void) NATIVE(107);

// Returns the model number of the active character's main weapon.
// Original name: _GET_ACTION_CODE.
extern int get_weapon_model(void) NATIVE(108);

// Returns the attack stat of a weapon slot of the active character.
// Original name: _GET_ATTK_POINT.
extern int get_weapon_attack(enum equip_slot weapon_slot) NATIVE(109);

// Writes the RGB colour (0 to 255) of the element of the equipped magic ring into three out
// parameters.
// Original name: _GET_RING_COLOR.
extern void get_ring_color(int *red, int *green, int *blue) NATIVE(110);

// Plays a motion on the character and its linked model parts. speed above 0 sets the playback step
// (-1 keeps the motion's own). flags: 1 pause, 2 stop on the last frame instead of looping, 4
// restart from the first frame without blending. chara_name starts from that linked part instead.
// Original name: _SET_MOS.
extern void play_motion(string motion_name, OPTIONAL(float speed, enum motion_flags flags, string chara_name)) NATIVE(130);

// Writes 1.0 into a float out parameter when the current motion has reached its last frame (or
// none is set), otherwise 0.0. chara_name checks that linked model part instead.
// Original name: _CHECK_MOS_END.
extern void is_motion_done(void *is_done, OPTIONAL(string chara_name)) NATIVE(131);

// Writes how far the current motion has played into a float out parameter, from 0.0 at the start
// to 1.0 at the end (0.0 while blending in). chara_name reads that linked model part instead.
// Original name: _NOW_MOS_WAIT.
extern void get_motion_progress(float *progress, OPTIONAL(string chara_name)) NATIVE(132);

// Writes the motion state into the out parameter: 0 none, 1 on its first frame, 2 playing, 3
// blending in, 4 reached the end. chara_name reads that linked model part instead.
// Original name: _GET_MOS_STATUS.
extern void get_motion_status(int *status, OPTIONAL(string chara_name)) NATIVE(133);

// Sets how much the blend into the next motion advances each frame; 1.0 or more makes it switch at
// once.
// Original name: _SET_XCHG_STEP.
extern void set_motion_blend_speed(float speed) NATIVE(134);

// Sets the playback step of the character's current motion, in frames per update.
// Original name: _SET_MOS_STEP.
extern void set_motion_speed(float step) NATIVE(135);

// Tells the character's motion sequence to move on to its next step.
// Original name: _TRG_ON_MOS.
extern void advance_motion_sequence(void) NATIVE(136);

// Clears the current and pending motions of the character and its linked model parts.
// Original name: _RESET_MOS.
extern void reset_motion(void) NATIVE(137);

// Sets the motion the character returns to by default, such as its standing motion.
// Original name: _SET_DEFAULT_MOS.
extern void set_default_motion(string motion_name) NATIVE(138);

// Returns, as a float, the progress of the blend into the new motion, or -1.0 when not blending.
// Original name: _NOW_MOS_CHGWAIT.
extern float get_motion_blend(void) NATIVE(139);

// If the active character has the slow status, sets the current motion to play at 70% of its
// default speed.
// Original name: _SET_NEBA2.
extern void apply_slow_status(void) NATIVE(140);

// Starts the named effect script on the character. With the out parameter it writes the new
// effect's slot id, used by the other effect natives; it fails if no slot is free.
// Original name: _ESM_CREATE.
extern void create_effect(string effect_name, OPTIONAL(int *effect_id)) NATIVE(150);

// Sets the first vector of an effect, usually its position. effect_id -1 means the most recently
// created effect.
// Original name: _ESM_SET_VECT1.
extern void set_effect_vec1(int effect_id, float x, float y, float z) NATIVE(151);

// Sets the second vector of an effect, such as its direction or scale. effect_id -1 means the most
// recently created effect.
// Original name: _ESM_SET_VECT2.
extern void set_effect_vec2(int effect_id, float x, float y, float z) NATIVE(152);

// Switches an effect to its finishing program (300) so it can end on its own.
// Original name: _ESM_FINISH.
extern void finish_effect(int effect_id) NATIVE(153);

// Removes an effect immediately.
// Original name: _ESM_DELETE.
extern void delete_effect(int effect_id) NATIVE(154);

// Sets numbered value index of an effect to an int or float. effect_id -1 means the most recently
// created effect.
// Original name: _ESM_SET_VALUE.
extern void set_effect_value(int effect_id, int index, any value) NATIVE(155);

// Numbers the disc calls that this table never registers. The engine prints "not found ext" and
// moves on, so these calls do nothing.
extern void _EXT_6(...) NATIVE(6);
extern void _EXT_7(...) NATIVE(7);
extern void _EXT_13(...) NATIVE(13);
extern void _EXT_14(...) NATIVE(14);
extern void _EXT_44(...) NATIVE(44);
extern void _EXT_80(...) NATIVE(80);

#include <helpers/action.h>

#endif
