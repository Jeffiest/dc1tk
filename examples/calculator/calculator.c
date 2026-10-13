// Mod calculator: a desk calculator stands in Barga's Valley. Walk up to it, press the action
// button at the "!" and it opens: a calculator face with a cursor on its keys, an LCD of our own
// digits, and sound effects for every key.
//
// It is a patch of every language of map s17's script. It adds the start program the mod's
// start_at warps a new game to, and replaces program 500, which runs when the player presses the
// action button at the map's "talk" point (the Spheda trial's, unused while the mod's game starts
// here). Its files come from mod.json: the model (mods/calculator/calc.chr), the face, cursor and
// digits (mods/calculator/calc_ui.img) and the sounds, added to bank SP_042, which nothing else
// loads. The logic is in logic.c, which tests.c runs without the game.
#pragma target map s17

#include <event.h>
#include <pal.h>
#include <disc.h>
#include "modkit/input.c"
#include "modkit/text.c"
#include "logic.c"

// The program a new game starts at (mod.json's start_at), numbered by build.
NEW_PROGRAM(CALC_START);
PROGRAM(CALC_START, start_calculator_valley);

// The map's "talk" point, where the calculator stands.
#define KIOSK_X 68.04
#define KIOSK_Y 41.85
#define KIOSK_Z 1180.61
#define KIOSK_SLOT 20

// The face is drawn at 1.5 times its 256x256 picture, centred on the 512x448 screen: picture
// pixel (x, y) is screen (FACE_LEFT + x * 3 / 2, FACE_TOP + y * 3 / 2).
#define FACE_LEFT 64
#define FACE_TOP 32
#define SPRITE_FACE 0
#define SPRITE_CURSOR 1
#define SPRITE_DIGIT 2
#define DISPLAY_CELLS 9
#define GLYPH_MINUS 10
#define GLYPH_E 11
#define GLYPH_BLANK 12
#define UI_IMAGE 1

// Program CALC_START: a new game with this mod begins at the calculator. The model is loaded on
// the stack that outlives the event, so it stays standing after finish_event.
void start_calculator_valley(void)
{
    init_event();
    set_func_point_enabled("", "exit", 0);
    set_func_point_enabled("", "branch", 0);
    set_func_point_enabled("p01_s17", "invent_30001-0", 0);
    reset_memory_stack(MEMORY_STACK_SUB_CHARA);
    load_chara(MEMORY_STACK_SUB_CHARA, "mods/calculator/calc.chr", "info.cfg", KIOSK_SLOT);
    set_chara_active(KIOSK_SLOT);
    place_chara(KIOSK_SLOT, KIOSK_X, KIOSK_Y, KIOSK_Z, 0.0);
    place_chara(0, KIOSK_X, KIOSK_Y, KIOSK_Z + 80.0, 3.14);
    set_camera_pos(KIOSK_X - 0.16, KIOSK_Y + 48.0, KIOSK_Z + 180.0);
    set_camera_target(KIOSK_X, KIOSK_Y + 20.0, KIOSK_Z + 60.0);
    reset_camera(CAMERA_RESET_ACTIVE);
    fade_in(30);
    modkit_wait(30);
    modkit_say("Welcome to Barga's Valley.\nThere is a calculator ahead.\nWalk up to it and press [cross] at the !");
    finish_event();
}

// Screen x and y of a point of the face picture.
int face_x(int x)
{
    return FACE_LEFT + x * 3 / 2;
}

int face_y(int y)
{
    return FACE_TOP + y * 3 / 2;
}

// Shows width by height of the UI picture texture at u, v, 1.5 times as large, with its top left
// at face picture point (x, y).
void place_sprite(int sprite, int u, int v, int width, int height, int x, int y)
{
    set_sprite_uv(sprite, u, v, width, height);
    set_sprite_size(sprite, width * 3 / 2, height * 3 / 2);
    set_sprite_pos(sprite, face_x(x) + width * 3 / 4, face_y(y) + height * 3 / 4);
}

void show_glyph(int cell, int glyph)
{
    place_sprite(SPRITE_DIGIT + cell, glyph * 16, 0, 16, 32, 40 + cell * 20, 37);
}

// The display: the number right-aligned in the cells, a minus before it, or E on an error.
void draw_display(int state[])
{
    int value = calc_shown(state);
    int negative = value < 0;
    int cell = DISPLAY_CELLS - 1;

    if (negative) {
        value = -value;
    }
    do {
        show_glyph(cell--, value % 10);
        value /= 10;
    } while (value > 0 && cell >= 0);
    if (negative && cell >= 0) {
        show_glyph(cell--, GLYPH_MINUS);
    }
    while (cell >= 0) {
        show_glyph(cell--, GLYPH_BLANK);
    }
    if (state[CALC_ERROR]) {
        show_glyph(0, GLYPH_E);
    }
}

// The cursor's frame around the key it is on (the wide one on the 0 key).
void draw_cursor(int state[], int block)
{
    int row = state[CALC_ROW];
    int col = state[CALC_COL];
    int x = 20 + col * 56;
    int y = 92 + row * 31;

    if (row == 4 && col == 0) {
        set_sprite_texture(SPRITE_CURSOR, "calc_cursor_wide", block);
        place_sprite(SPRITE_CURSOR, 0, 0, 128, 32, x - 2, y - 2);
    } else {
        set_sprite_texture(SPRITE_CURSOR, "calc_cursor", block);
        place_sprite(SPRITE_CURSOR, 0, 0, 64, 32, x - 2, y - 2);
    }
}

void open_sprite(int sprite, string texture, int block)
{
    init_screen_sprite(sprite, texture, block);
    set_sprite_alpha(sprite, 128);
    set_sprite_draw(sprite, EVENT_SPRITE2_DRAW_NORMAL);
}

// The sound for what a press did.
void play_press(int sound, int result)
{
    switch (result) {
    case CALC_MOVED:
    case CALC_KEYED:
        play_se(sound, SE_MOD_CALC_KEY);
        break;
    case CALC_EQUALED:
        play_se(sound, SE_MOD_CALC_EQUALS);
        break;
    case CALC_FAILED:
        play_se(sound, SE_MOD_CALC_ERROR);
        break;
    case CALC_CLEARED:
        play_se(sound, SE_MOD_CALC_CLEAR);
        break;
    }
}

// The calculator, until (O), [select] or its power key closes it.
void use_calculator(void)
{
    int state[CALC_STATE];
    int sound;
    int block;
    int result;
    int cell;

    init_event();
    show_status_bar(0, 1.0);
    load_sound(SND_PORT_MENU, "snd2/sp/sp_042.snd", &sound);
    reset_memory_stack(MEMORY_STACK_EVENT);
    load_image(MEMORY_STACK_EVENT, "mods/calculator/calc_ui.img", UI_IMAGE, &block);
    open_sprite(SPRITE_FACE, "calc_body", block);
    place_sprite(SPRITE_FACE, 0, 0, 256, 256, 0, 0);
    open_sprite(SPRITE_CURSOR, "calc_cursor", block);
    for (cell = 0; cell < DISPLAY_CELLS; cell++) {
        open_sprite(SPRITE_DIGIT + cell, "calc_digits", block);
    }
    calc_start(state);
    draw_display(state);
    draw_cursor(state, block);
    play_se(sound, SE_MOD_CALC_OPEN);
    // the press that opened it is not a key
    modkit_wait(10);
    while ((result = calc_press(state, get_pad_pressed())) != CALC_CLOSE) {
        if (result != CALC_IGNORED) {
            play_press(sound, result);
            draw_display(state);
            draw_cursor(state, block);
        }
        wait;
    }
    play_se(sound, SE_MOD_CALC_CLOSE);
    modkit_wait(10);
    delete_image(UI_IMAGE);
    show_status_bar(1, 1.0);
    finish_event();
}

// Program 500 (the "talk" point): the calculator instead of the Spheda trial.
#pragma override
int talk_to_monica_spheda_trial(void)
{
    use_calculator();
    return 0;
}
