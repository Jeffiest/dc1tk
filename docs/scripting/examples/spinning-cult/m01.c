// Mod spinning-cult: Palm Brinks' side. The door into the Fish Room (make_mod.py adds the point
// door_fish; mod.json's "programs" gives it ENTER_FISH_ROOM), the five townspeople the gŷros
// turns, the town going dark once three have turned, and the way they stand after the ending.
//
// Palm Brinks' script is near the most a map's script may be (a map's script shares a 320,000-byte
// buffer with the interpreter's tables), so this patch is kept small: it talks through the
// script's own helpers (show_message_auto, start_npc_talk), and keeps the cult's state as plain
// bits (cult.h's CULT_* macros, the same bits fishroom.c reads as bit-fields).
#pragma target map m01

#include <event.h>
#include <disc.h>
#include "cult.h"

PROGRAM(ENTER_FISH_ROOM, enter_fish_room);
PROGRAM(BACK_FROM_FISH_ROOM, arrive_from_fish_room);

// The five who can be turned, in bit order.
int convert_npcs[CONVERT_COUNT] = {NPC_PARN, NPC_SHERIFF_BLINKHORN, NPC_GILES, NPC_AEGIRA, NPC_FOURNET};

// A line from a townsperson, in the window at the top of the screen (the slot the game's hints
// use), which a bubble from someone at the edge of the picture would run off.
static void line(string text)
{
    apply_message_preset(0, MES_PRESET_WINDOW);
    set_message_window_mode(0, MES_WIN_VERSATILE_1);
    show_message(0, text);
    show_message_at(0, -1, 8, 0, 0);
}

static void frames(int count)
{
    while (count > 0) {
        count = count - 1;
        WAIT();
    }
}

// Holds a townsperson where they stand (the engine stops walking them) in a motion.
static void hold(int slot, string motion)
{
    set_villager_ai(0, slot);
    set_chara_motion(slot, motion);
}

// The town as the cult has left it: the turned turning, and the dark once three have turned.
static void apply_town_state(void)
{
    int cult;
    int which;
    int slot;

    cult = get_counter(CULT_COUNTER);
    for (which = 0; which < CONVERT_COUNT; which++) {
        slot = find_villager_slot(convert_npcs[which]);
        if ((cult & (1 << which)) && slot > 0) {
            hold(slot, "spin");
        }
    }
    if (cult & CULT_DARK_BIT) {
        set_time_flow(0);
        set_map_light(TOWN_DARK_LIGHT);
        stop_bgm(0);
        allow_bgm_load();
        load_bgm(MUSIC_TURNING);
        play_bgm(0, 50);
    }
}

// A new game starts at progress 1, when nobody is out in Palm Brinks; the town's chapter-one
// people come out from progress 3, so the first arrival moves the story on and enters again.
#pragma override
int play_map_start_arrival(void)
{
    int result;
    int progress;

    get_save_value(SAVE_GET_PROGRESS, &progress);
    if (progress < 3) {
        set_save_value(SAVE_SET_PROGRESS, 3);
        jump_to_map(-1, "m01", map_start);
        WAIT();
        return 0;
    }
    result = original();
    apply_town_state();
    return result;
}

// The door's program: the door has opened and the screen faded out (its point's flags).
void enter_fish_room(void)
{
    jump_to_map(-1, "fishroom", map_start);
    WAIT();
}

// Back from the Fish Room: the town's own arrival, then the player comes out of the door.
void arrive_from_fish_room(void)
{
    play_map_start_arrival();
    place_player_reset(TOWN_DOOR_X, TOWN_DOOR_Y, TOWN_DOOR_Z - 40.0, 3.14);
    reset_camera(CAMERA_RESET_BEHIND, 0.0, 10.0);
}

// What they say before it takes them.
static void before(int which)
{
    switch (which) {
    case 0:
        line("Is that for me? Let me see it.\nIt's like a painting\nthat never stops.");
        break;
    case 1:
        line("Hand that over, son.\nThat's evidence.");
        break;
    case 2:
        line("Huh? What's that thing?");
        break;
    case 3:
        line("A toy! My little brother\nhad one just like it.");
        break;
    default:
        line("Keep that away from me.\nI said keep it away--");
        break;
    }
}

// What they say once it has (and every time after).
static void after(int which)
{
    switch (which) {
    case 0:
        line("I should paint it.\nI should paint it.\nI should paint it.");
        break;
    case 1:
        line("...Round.\nAnd round.\nCase closed.");
        break;
    case 2:
        line("Can you hear it?\nUnder the street.");
        break;
    case 3:
        line("...I don't have\na little brother.");
        break;
    default:
        line("...");
        break;
    }
}

// Max shows a townsperson the gŷros. The music drains away, the picture wavers, a red flash,
// and they turn. The third to turn takes the town's light with them.
static void turn_one(int which, int slot, int cult)
{
    start_npc_talk(slot, 1, 0);
    before(which);
    fade_out_bgm(90);
    start_screen_wave(2.0, 0.05, 8.0, 120);
    frames(90);
    fade_out(6, 120.0, 0.0, 0.0);
    frames(6);
    set_chara_motion(slot, "spin");
    fade_in(40, 120.0, 0.0, 0.0);
    frames(60);
    stop_screen_wave(2.0, 0.05, 8.0, 60);
    after(which);
    cult = (cult | (1 << which)) + CULT_ONE_MORE;
    if (CULT_COUNT(cult) >= 3 && (cult & CULT_DARK_BIT) == 0) {
        cult = cult | CULT_DARK_BIT;
        set_counter(CULT_COUNTER, cult);
        fade_out(60, 0.0, 0.0, 0.0);
        frames(60);
        apply_town_state();
        fade_in(90, 0.0, 0.0, 0.0);
        frames(90);
    }
    set_counter(CULT_COUNTER, cult);
}

// Talking to someone: the five, when Max has the gŷros or they have turned; anyone else as
// the game has them.
#pragma override
int talk_to_villager(void)
{
    int slot;
    int npc;
    int which;
    int cult;

    get_event_data(EVENT_DATA_CHARA_SLOT, &slot);
    get_event_data(EVENT_DATA_CHARA_NO, &npc);
    for (which = 0; which < CONVERT_COUNT && convert_npcs[which] != npc; which++) {
    }
    cult = get_counter(CULT_COUNTER);
    if (which == CONVERT_COUNT) {
        return original();
    }
    if (cult & (1 << which)) {
        // turned: they keep turning and do not look round
        init_event();
        set_villager_ai(0, slot);
        after(which);
        apply_town_state();
        reset_camera(CAMERA_RESET_BEHIND, 0.0, 10.0);
        finish_event();
        return 0;
    }
    if (get_item_count(ITEM_GYROS) == 0) {
        return original();
    }
    init_event();
    set_villager_ai(0, slot);
    turn_one(which, slot, cult);
    reset_camera(CAMERA_RESET_BEHIND, 0.0, 10.0);
    finish_event();
    return 0;
}
