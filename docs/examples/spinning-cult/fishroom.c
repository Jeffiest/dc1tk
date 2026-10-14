// Mod spinning-cult: the Fish Room's script (the new map fishroom; mod.json gives it as the map's
// "script"). Brother Gill waits under the fish, turning with it; he gives Max the camera and a
// scrap of metal, asks him to take the fish's likeness and make a shape the town can hold (the
// Whirligig, an invention), and to show it to them. When all five have turned, the fish stops,
// and looks.

#include <event.h>
#include <disc.h>
#include "modkit/input.c"
#include "modkit/text.c"
#include "cult.h"
#include "speech.h"

PROGRAM(map_start, arrive_in_room);
PROGRAM(talk_or_dungeon_start, talk_in_room);
PROGRAM(LEAVE_FISH_ROOM, leave_room);
MAIN(do_nothing);

#define GILL 40
#define GILL_X 0.0
#define GILL_Z 70.0
#define FIRST_CONVERT 41

void do_nothing(void)
{
}

// The five, as the ending stands them in a ring round the fish, 250 from the middle, the side
// towards the door left open: their packs, and where they stand.
struct ring_place {
    string pack;
    float x;
    float z;
};

struct ring_place ring[CONVERT_COUNT] = {
    {"chara/p01_10a.chr", 216.5, 125.0},
    {"chara/p01_05a.chr", 191.5, -160.7},
    {"chara/p01_19a.chr", 0.0, -250.0},
    {"chara/p01_20a.chr", -191.5, -160.7},
    {"chara/p01_22a.chr", -216.5, 125.0},
};

// Shows the fish as it is now: turning, or (after the ending) awake and watching.
static void show_fish(int awake)
{
    bind_handle_map_part(1, "fr_fish");
    set_handle_visible(1, awake == 0);
    bind_handle_map_part(2, "fr_fish_awake");
    set_handle_visible(2, awake != 0);
}

static void place_player_at_door(void)
{
    place_chara(CHARA_PLAYER, 0.0, 0.0, ROOM_ARRIVE_Z, 3.14);
    set_chara_motion(CHARA_PLAYER, MOTION_STAND_2, CHARA_MOTION_RESTART);
    update_chara_motion(CHARA_PLAYER);
    reset_chara_cloth(CHARA_PLAYER);
    reset_camera(CAMERA_RESET_BEHIND, 0.0, 10.0);
}

// Gill as he waits: under the fish, his back to the door, turning slowly with it.
static void load_gill(void)
{
    load_chara(MEMORY_STACK_EVENT, "mods/gill/gill.chr", "info.cfg", GILL);
    set_chara_active(GILL);
    place_chara(GILL, GILL_X, 0.0, GILL_Z, 3.14);
    set_chara_motion(GILL, "spin");
}

static void play_room_music(void)
{
    stop_bgm(0);
    allow_bgm_load();
    load_bgm(MUSIC_TURNING);
    play_bgm(0);
}

static void play_ending(void);

// Arriving: the room as the cult has it.
void arrive_in_room(void)
{
    union cult_state state;

    state = read_cult();
    init_event();
    reset_memory_stack(MEMORY_STACK_EVENT);
    place_player_at_door();
    if (state.bits.stage == STAGE_ENDED) {
        // it is awake now, and it follows you; nothing plays
        stop_bgm(0);
        show_fish(1);
        fade_in(60);
        finish_event();
        return;
    }
    show_fish(0);
    play_room_music();
    if (state.bits.turned == ALL_TURNED) {
        play_ending();
        return;
    }
    load_gill();
    fade_in(60);
    finish_event();
}

// --- Gill ------------------------------------------------------------------------------------

// He turns his head before he turns round: too far, and too slowly.
static void gill_turns_round(void)
{
    set_chara_motion(GILL, "turn_head", CHARA_MOTION_HOLD);
    modkit_wait(70);
    face_chara(GILL, CHARA_PLAYER);
    face_chara(CHARA_PLAYER, GILL);
    set_chara_motion(CHARA_PLAYER, MOTION_STAND_2);
    set_chara_motion(GILL, "idle");
    close_on(GILL, CHARA_PLAYER, 80.0, 30.0);
    modkit_wait(30);
}

static void gill_turns_back(void)
{
    set_chara_rot(GILL, 0.0, 3.14, 0.0);
    set_chara_motion(GILL, "spin");
    reset_camera(CAMERA_RESET_BEHIND, 0.0, 10.0);
}

static void meet_gill(void)
{
    union cult_state state;

    gill_turns_round();
    say(GILL, "You came to watch it too.");
    say(GILL, "It was turning before\nthe town was built.\nIt will be turning after.");
    say(GILL, "They have all forgotten\nhow to look at it.");
    set_chara_motion(GILL, "beckon", CHARA_MOTION_HOLD);
    say(GILL, "Help them remember.");
    set_chara_motion(GILL, "idle");
    say(GILL, "Take its likeness first.\nThen give it a shape\nthey can hold.");
    add_item(ITEM_CAMERA, 1);
    add_item(ITEM_INSIDE_SCOOP_MEMO, 1);
    add_item(ITEM_PHOTO_ALBUM, 1);
    add_item(ITEM_SCRAP_OF_METAL, 1);
    modkit_say("Received a Camera, a notebook,\na photo album and a Scrap of Metal.");
    modkit_say("Set the Camera in an item slot\nto take photos with it.");
    state = read_cult();
    state.bits.stage = STAGE_MET;
    write_cult(state);
}

// What Gill says while Max has no Whirligig: the likeness, then the shape.
static void gill_hints(void)
{
    int photos = 0;

    count_photos_of(IDEA_ENDLESS_TURNING, PHOTO_SUBJECT_IDEA, &photos);
    gill_turns_round();
    if (photos == 0) {
        say(GILL, "Its likeness.\nTake it.");
    } else {
        say(GILL, "Yes. You have it now.");
        say(GILL, "Give it a shape they can hold.\nA flag. A post.\nSomething that turns.");
    }
}

// What Gill says as they turn.
static void gill_counts(int count)
{
    gill_turns_round();
    switch (count) {
    case 0:
        say(GILL, "Show them.\nOne at a time.");
        break;
    case 1:
        say(GILL, "One remembers.");
        break;
    case 2:
        say(GILL, "Two.\nCan you feel the town\nleaning towards us?");
        break;
    case 3:
        say(GILL, "The lights have gone out\nup there. Good.");
        break;
    default:
        say(GILL, "One more.\nThen it will look.");
        break;
    }
}

// Talking in the room: only Gill answers.
void talk_in_room(void)
{
    int slot;
    union cult_state state;

    get_event_data(EVENT_DATA_CHARA_SLOT, &slot);
    if (slot != GILL) {
        return;
    }
    init_event();
    state = read_cult();
    if (state.bits.stage == STAGE_NONE) {
        meet_gill();
    } else if (get_item_count(ITEM_WHIRLIGIG) == 0) {
        gill_hints();
    } else if (state.bits.stage == STAGE_MET) {
        gill_turns_round();
        say(GILL, "Yes.\nThat is its shape.");
        say(GILL, "Show it to them.\nThey will know what to do.");
        state.bits.stage = STAGE_WHIRLIGIG;
        write_cult(state);
    } else {
        gill_counts(state.bits.count);
    }
    gill_turns_back();
    finish_event();
}

// --- the ending --------------------------------------------------------------------------------

// The fish slows to a stop: its turning point off, the script turns it, less each frame.
static void slow_the_fish(void)
{
    float x;
    float y;
    float z;
    float speed = 1.5;
    int frame;
    int which;

    set_func_point_enabled("fr_fish", "fish_spin", 0);
    bind_handle_map_part(0, "fr_fish", "fr_fish-m");
    get_handle_rot(0, &x, &y, &z);
    for (frame = 0; frame < 240; frame++) {
        y = y + speed;
        speed = speed * 0.982;
        set_handle_rot(0, x, y, z);
        // one by one, they stop
        which = frame / 40;
        if (frame % 40 == 0 && which < CONVERT_COUNT) {
            set_chara_motion(FIRST_CONVERT + which, "stare");
        }
        wait;
    }
}

static void play_ending(void)
{
    int which;
    float z;
    union cult_state state;

    // the five in a ring round the fish, turning; Gill in the middle, arms raised
    for (which = 0; which < CONVERT_COUNT; which++) {
        load_chara(MEMORY_STACK_EVENT, ring[which].pack, "info.cfg", FIRST_CONVERT + which);
        set_chara_active(FIRST_CONVERT + which);
        place_chara(FIRST_CONVERT + which, ring[which].x, 0.0, ring[which].z, 0.0);
        set_chara_motion(FIRST_CONVERT + which, "spin");
    }
    load_gill();
    set_chara_rot(GILL, 0.0, 0.0, 0.0);
    set_chara_motion(GILL, "raise", CHARA_MOTION_HOLD);
    add_chara_motions(CHARA_PLAYER, "event/e700/e728/e728_mints.chr");
    set_camera_pos(0.0, 320.0, 980.0);
    set_camera_target(0.0, 120.0, 0.0);
    fade_in(90);
    modkit_wait(60);
    // Max walks in, not quite of his own accord
    set_chara_motion(CHARA_PLAYER, MOTION_WALK);
    for (z = ROOM_ARRIVE_Z; z > 380.0; z = z - 1.6) {
        set_chara_pos(CHARA_PLAYER, 0.0, 0.0, z);
        wait;
    }
    set_chara_motion(CHARA_PLAYER, MOTION_STAND_2);
    close_on(GILL, CHARA_PLAYER, 120.0, 34.0);
    say(GILL, "Kneel.");
    set_camera_pos(60.0, 40.0, 470.0);
    set_camera_target(0.0, 20.0, 380.0);
    set_chara_motion(CHARA_PLAYER, MOTION_CROUCH_2);
    modkit_wait(40);
    set_chara_motion(CHARA_PLAYER, MOTION_CROUCH_LOOP_2);
    modkit_wait(30);
    set_camera_pos(0.0, 60.0, 520.0);
    set_camera_target(0.0, 200.0, 0.0);
    say_from(GILL, CHARA_PLAYER, "It is going to look at you now.");
    // the fish stops; the music with it; nothing, for a while
    fade_out_bgm(200);
    slow_the_fish();
    stop_bgm(0);
    modkit_wait(120);
    // and then it is looking at you
    fade_out(3, 255.0, 255.0, 255.0);
    modkit_wait(3);
    show_fish(1);
    set_map_light(ROOM_RED_LIGHT);
    for (which = 0; which < CONVERT_COUNT; which++) {
        face_chara(FIRST_CONVERT + which, CHARA_PLAYER);
    }
    set_camera_pos(0.0, 30.0, 420.0);
    set_camera_target(0.0, FISH_HEIGHT, 0.0);
    fade_in(3, 255.0, 255.0, 255.0);
    shake_camera(3.0, 3.0, 3.0, 40);
    modkit_wait(150);
    close_on(GILL, CHARA_PLAYER, 110.0, 34.0);
    say(GILL, "It sees you.");
    set_chara_motion(GILL, "vanish", CHARA_MOTION_HOLD);
    modkit_wait(70);
    set_chara_visible(GILL, 0);
    set_camera_pos(0.0, 30.0, 420.0);
    set_camera_target(0.0, FISH_HEIGHT, 0.0);
    modkit_wait(180);
    state = read_cult();
    state.bits.stage = STAGE_ENDED;
    write_cult(state);
    fade_out(120, 0.0, 0.0, 0.0);
    modkit_wait(140);
    delete_chara_ext_motion(CHARA_PLAYER);
    jump_to_map(-1, "m01", BACK_FROM_FISH_ROOM);
    wait;
}

// The room's door (point door_out, given this program by mod.json's "programs").
void leave_room(void)
{
    jump_to_map(-1, "m01", BACK_FROM_FISH_ROOM);
    wait;
}
