// c2s SDK helpers for event scripts. Each helper stands for the native calls in its body; the
// compiler expands it in place, so a helper call produces the same bytecode as writing the calls
// out, and the decompiler folds every run it matches. Included by event.h.
#ifndef DC2TK_HELPERS_EVENT_H
#define DC2TK_HELPERS_EVENT_H

// The value kind of a scene character slot, as event.h declares it.
typedef int chara_slot;

// Clears the actor's sequence and makes it drive the event object handle with the same number.
static inline void init_actor(int actor)
{
    clear_actor_seq(actor);
    bind_actor_object(actor, actor);
}

// Binds a handle to a scene character and sets up the actor of the same number to drive it.
static inline void init_chara_actor(int actor, chara_slot chara)
{
    bind_handle_chara(actor, chara);
    clear_actor_seq(actor);
    bind_actor_object(actor, actor);
}

// Binds a handle to an event sprite and sets up the actor of the same number to drive it.
static inline void init_sprite_actor(int actor, int sprite)
{
    bind_handle_sprite(actor, sprite);
    clear_actor_seq(actor);
    bind_actor_object(actor, actor);
}

// Binds a handle to a named piece of a map part and sets up the actor of the same number to drive
// it.
static inline void init_map_piece_actor(int actor, any part, string piece)
{
    bind_handle_map_part(actor, part, piece);
    clear_actor_seq(actor);
    bind_actor_object(actor, actor);
}

// Binds a handle to a map part and sets up the actor of the same number to drive it.
static inline void init_map_part_actor(int actor, any part)
{
    bind_handle_map_part(actor, part);
    clear_actor_seq(actor);
    bind_actor_object(actor, actor);
}

// Binds a handle to a scene character and sets up an actor with a different number to drive that
// handle.
static inline void bind_actor_to_chara(int actor, int handle, chara_slot chara)
{
    bind_handle_chara(handle, chara);
    clear_actor_seq(actor, chara);
    bind_actor_object(actor, handle);
}

// Activates a scene character, binds a handle to it and sets up the actor of the same number to
// drive it.
static inline void activate_chara_actor(int actor, chara_slot chara)
{
    set_chara_active(chara);
    bind_handle_chara(actor, chara);
    clear_actor_seq(actor);
    bind_actor_object(actor, actor);
}

// Loads a character pack on memory stack 5, activates it and sets up the handle and actor of the
// same number to drive it.
static inline void load_chara_actor(int actor, string pack, chara_slot chara)
{
    load_chara(5, pack, "info.cfg", chara);
    set_chara_active(chara);
    bind_handle_chara(actor, chara);
    clear_actor_seq(actor);
    bind_actor_object(actor, actor);
}

// Loads a character pack into a scene character slot on memory stack 5.
static inline void load_scene_chara(chara_slot chara, string pack)
{
    load_chara(5, pack, "info.cfg", chara);
}

// Loads a character pack into a scene character slot on memory stack 5 and activates it.
static inline void load_active_chara(chara_slot chara, string pack)
{
    load_chara(5, pack, "info.cfg", chara);
    set_chara_active(chara);
}

// Loads a character pack into a scene character slot on memory stack 5 and leaves it inactive.
static inline void load_inactive_chara(chara_slot chara, string pack)
{
    load_chara(5, pack, "info.cfg", chara);
    set_chara_active(0, chara);
}

// Loads an extra motion pack into a scene character on memory stack 5.
static inline void add_chara_motions(chara_slot chara, string pack)
{
    load_chara_motion(5, pack, "info.cfg", chara);
}

// Loads a party member's current skins onto an event character on memory stack 5, and its weapon
// as character weapon.
static inline void load_party_equipment(int member, chara_slot chara, int weapon)
{
    load_equip_model(5, member, 4, "info.cfg", chara);
    load_equip_model(5, member, 3, "info.cfg", chara);
    load_equip_model(5, member, 2, "info.cfg", chara, weapon);
}

// Loads a party member's base pack into a scene character on memory stack 5 with its current
// skins, and its weapon as character weapon.
static inline void load_party_member(int member, string pack, chara_slot chara, int weapon)
{
    load_chara(5, pack, "info.cfg", chara);
    load_equip_model(5, member, 4, "info.cfg", chara);
    load_equip_model(5, member, 3, "info.cfg", chara);
    load_equip_model(5, member, 2, "info.cfg", chara, weapon);
}

// Makes a scene character active.
static inline void activate_chara(chara_slot chara)
{
    set_chara_active(1, chara);
}

// Makes a scene character inactive, so it is neither stepped nor drawn.
static inline void deactivate_chara(chara_slot chara)
{
    set_chara_active(0, chara);
}

// Queues putting the actor at a world position facing the yaw angle in radians.
static inline void place_actor(int actor, float x, float y, float z, float yaw)
{
    set_actor_pos(actor, x, y, z);
    set_actor_rot(actor, 0.0, yaw, 0.0);
}

// Queues putting the actor at a world position facing the yaw angle in radians and settles its
// hair and cloth there.
static inline void warp_actor(int actor, float x, float y, float z, float yaw)
{
    set_actor_pos(actor, x, y, z);
    set_actor_rot(actor, 0.0, yaw, 0.0);
    reset_actor_dynamics(actor);
}

// Queues turning the actor to a yaw angle in radians, level on the other axes.
static inline void set_actor_yaw(int actor, float yaw)
{
    set_actor_rot(actor, 0.0, yaw, 0.0);
}

// Queues turning the actor to a yaw angle in radians over a number of frames, level on the other
// axes.
static inline void turn_actor_to_yaw(int actor, float yaw, int frames)
{
    turn_actor_to(actor, 0.0, yaw, 0.0, frames);
}

// Places a scene character at a world position facing the yaw angle in radians.
static inline void place_chara(chara_slot chara, float x, float y, float z, float yaw)
{
    set_chara_pos(chara, x, y, z);
    set_chara_rot(chara, 0.0, yaw, 0.0);
}

// Turns a scene character to a yaw angle in radians, level on the other axes.
static inline void set_chara_yaw(chara_slot chara, float yaw)
{
    set_chara_rot(chara, 0.0, yaw, 0.0);
}

// Moves the object bound to a handle to a position facing the yaw angle in radians.
static inline void place_handle(int handle, float x, float y, float z, float yaw)
{
    set_handle_pos(handle, x, y, z);
    set_handle_rot(handle, 0.0, yaw, 0.0);
}

// Turns the object bound to a handle to a yaw angle in radians, level on the other axes.
static inline void set_handle_yaw(int handle, float yaw)
{
    set_handle_rot(handle, 0.0, yaw, 0.0);
}

// Starts a motion from its first frame on the character bound to a handle.
static inline void play_handle_motion(int handle, string motion)
{
    set_handle_motion(handle, motion, CHARA_MOTION_RESTART);
}

// Queues putting the camera eye and look-at point at two world positions at once.
static inline void set_camera_view(float x, float y, float z, float ref_x, float ref_y, float ref_z)
{
    set_camera_seq_pos(x, y, z);
    set_camera_seq_ref(ref_x, ref_y, ref_z);
}

// Drops all queued camera commands, then queues putting the camera eye and look-at point at two
// world positions at once.
static inline void cut_camera(float x, float y, float z, float ref_x, float ref_y, float ref_z)
{
    clear_camera_seq();
    set_camera_seq_pos(x, y, z);
    set_camera_seq_ref(ref_x, ref_y, ref_z);
}

// Queues clearing the camera path and setting how many frames the camera takes to run the new one.
static inline void begin_camera_path(int frames)
{
    clear_camera_path();
    set_camera_path_frames(frames);
}

// Queues a motion on the actor with the given flags and the motion that follows it with its own
// flags.
static inline void play_actor_motions(int actor, string motion, enum chara_motion_flag flags, string next, enum chara_motion_flag next_flags)
{
    set_actor_motion(actor, motion, flags);
    queue_actor_motion(actor, next, next_flags);
}

// Queues a motion on the actor from its first frame, stopping on its last frame, then loops the
// next motion from its first frame.
static inline void play_actor_motion_then_loop(int actor, string motion, string next)
{
    set_actor_motion(actor, motion, CHARA_MOTION_RESTART | CHARA_MOTION_HOLD);
    queue_actor_motion(actor, next, CHARA_MOTION_RESTART);
}

// Queues a looping motion on the actor from its first frame.
static inline void loop_actor_motion(int actor, string motion)
{
    set_actor_motion(actor, motion, CHARA_MOTION_RESTART);
}

// Queues a motion on the actor from its first frame that stops on its last frame.
static inline void play_actor_motion_once(int actor, string motion)
{
    set_actor_motion(actor, motion, CHARA_MOTION_RESTART | CHARA_MOTION_HOLD);
}

// Queues a looping motion on the actor from its first frame and shows its first pose at once.
static inline void snap_actor_motion(int actor, string motion)
{
    set_actor_motion(actor, motion, CHARA_MOTION_RESTART, -1.0, 1);
}

// Clears the lip sync setup, then makes the actor's talk animation the moving mouth.
static inline void switch_lip_sync_anim(int actor, string talk_anim)
{
    reset_lip_sync_anim();
    set_lip_sync_anim(actor, talk_anim);
}

// Stops the actor's texture animations, then plays a face and an eye animation.
static inline void set_actor_face(int actor, string face, string eyes)
{
    set_actor_texture_anim(actor, 0);
    set_actor_texture_anim(actor, 1, face);
    set_actor_texture_anim(actor, 1, eyes);
}

// Queues stopping all of the actor's texture animations.
static inline void stop_actor_texture_anims(int actor)
{
    set_actor_texture_anim(actor, 0);
}

// Stops the actor's texture animations, then plays one named animation.
static inline void switch_actor_texture_anim(int actor, string anim)
{
    set_actor_texture_anim(actor, 0);
    set_actor_texture_anim(actor, 1, anim);
}

// Queues playing a named texture animation on the actor, such as a face or eye-blink loop.
static inline void play_actor_texture_anim(int actor, string anim)
{
    set_actor_texture_anim(actor, 1, anim);
}

// Stops the texture animations of the character bound to a handle, then plays a face and an eye
// animation.
static inline void set_handle_face(int handle, string face, string eyes)
{
    set_handle_texture_anim(handle, 0);
    set_handle_texture_anim(handle, 1, face);
    set_handle_texture_anim(handle, 1, eyes);
}

// Stops all texture animations of the character bound to a handle.
static inline void stop_handle_texture_anims(int handle)
{
    set_handle_texture_anim(handle, 0);
}

// Stops the texture animations of the character bound to a handle, then plays one named animation.
static inline void switch_handle_texture_anim(int handle, string anim)
{
    set_handle_texture_anim(handle, 0);
    set_handle_texture_anim(handle, 1, anim);
}

// Plays a named texture animation on the character bound to a handle.
static inline void play_handle_texture_anim(int handle, string anim)
{
    set_handle_texture_anim(handle, 1, anim);
}

// Sets a message window's frame style and how many characters it reveals per frame.
static inline void set_message_style(int window, enum mes_window_mode mode, float speed)
{
    set_message_window_mode(window, mode);
    set_message_speed(window, speed);
}

// Binds a handle to a map part and shows it.
static inline void show_map_part(int handle, any part)
{
    bind_handle_map_part(handle, part);
    set_handle_visible(handle, 1);
}

// Binds a handle to a map part and hides it.
static inline void hide_map_part(int handle, any part)
{
    bind_handle_map_part(handle, part);
    set_handle_visible(handle, 0);
}

// Binds a handle to a named piece of a map part and shows it.
static inline void show_map_piece(int handle, any part, string piece)
{
    bind_handle_map_part(handle, part, piece);
    set_handle_visible(handle, 1);
}

// Binds a handle to a named piece of a map part and hides it.
static inline void hide_map_piece(int handle, any part, string piece)
{
    bind_handle_map_part(handle, part, piece);
    set_handle_visible(handle, 0);
}

// Resets an event sprite to a screen sprite showing a texture from a texture block.
static inline void init_screen_sprite(int sprite, string texture, int tex_block)
{
    init_sprite(sprite);
    set_sprite_type(sprite, EVENT_SPRITE2_TYPE_SCREEN);
    set_sprite_texture(sprite, texture, tex_block);
}

// Sets an event sprite to its full colour with the given alpha, 128 being opaque.
static inline void set_sprite_alpha(int sprite, float alpha)
{
    set_sprite_color(sprite, 128, 128, 128, alpha);
}

// Shows a width by height region of the sprite's texture starting at u, v, drawn at that same
// size.
static inline void crop_sprite(int sprite, int u, int v, int width, int height)
{
    set_sprite_uv(sprite, u, v, width, height);
    set_sprite_size(sprite, width, height);
}

// Queues clearing the actor's walking path and setting how many frames it takes to walk the new
// one.
static inline void begin_actor_path(int actor, int frames)
{
    clear_actor_path(actor);
    set_actor_path_frames(actor, frames);
}

// Queues a sound effect on the actor's sound effect track after waiting a number of frames.
static inline void play_actor_se_after(int actor, int frames, int sound, int se_no)
{
    delay_actor_se(actor, frames);
    play_actor_se(actor, sound, se_no);
}

// Gives a memory stack the free space below it and empties it.
static inline void reset_memory_stack(enum memory_stack stack_no)
{
    assign_memory_stack(stack_no);
    clear_memory_stack(stack_no);
}

// Loads a music file and plays its first track.
static inline void start_bgm(int bgm_file)
{
    load_bgm(bgm_file);
    play_bgm(0);
}

#endif
