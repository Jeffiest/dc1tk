// The calculator's logic: a cursor on the key grid of ui/calc_body.png, the keys, and an 8-digit
// integer display. Nothing here draws or plays a sound, so tests.c runs it without the game;
// calculator.c (the patch) draws the state and plays the sound each press asks for.

#include <event.h>

#define CALC_MAX 99999999

// Keys: the digits are their own values.
#define KEY_ADD 10
#define KEY_SUB 11
#define KEY_MUL 12
#define KEY_DIV 13
#define KEY_EQUALS 14
#define KEY_CLEAR 15
#define KEY_ERASE 16
#define KEY_NEG 17
#define KEY_CLOSE 18

#define OP_NONE 0
#define OP_ADD 1
#define OP_SUB 2
#define OP_MUL 3
#define OP_DIV 4

// The state, one array so the functions can change it.
#define CALC_TOTAL 0
#define CALC_ENTRY 1
#define CALC_OP 2
#define CALC_ERROR 3
#define CALC_TYPED 4
#define CALC_ROW 5
#define CALC_COL 6
#define CALC_STATE 7

#define CALC_ROWS 5
#define CALC_COLUMNS 4

// What a press did, so the caller knows what to redraw and which sound to play.
#define CALC_IGNORED 0
#define CALC_MOVED 1
#define CALC_KEYED 2
#define CALC_EQUALED 3
#define CALC_FAILED 4
#define CALC_CLEARED 5
#define CALC_CLOSE 6

// The key at a grid cell, as ui/calc_layout.json lays them out; the 0 key spans the first two
// columns of the last row.
int calc_key_at(int row, int col)
{
    switch (row) {
    case 0:
        return col == 0 ? KEY_CLOSE : col == 1 ? KEY_CLEAR : col == 2 ? KEY_ERASE : KEY_DIV;
    case 1:
        return col == 3 ? KEY_MUL : 7 + col;
    case 2:
        return col == 3 ? KEY_SUB : 4 + col;
    case 3:
        return col == 3 ? KEY_ADD : 1 + col;
    }
    return col <= 1 ? 0 : col == 2 ? KEY_NEG : KEY_EQUALS;
}

void calc_reset(int state[])
{
    state[CALC_TOTAL] = state[CALC_ENTRY] = state[CALC_ERROR] = state[CALC_TYPED] = 0;
    state[CALC_OP] = OP_NONE;
}

// A new calculator: cleared, the cursor on the 7 key.
void calc_start(int state[])
{
    calc_reset(state);
    state[CALC_ROW] = 1;
    state[CALC_COL] = 0;
}

// The number the display shows: the one being typed, else the total.
int calc_shown(int state[])
{
    return state[CALC_TYPED] ? state[CALC_ENTRY] : state[CALC_TOTAL];
}

// Moves the cursor one step, wrapping at the edges. The wide 0 key is one cell: moving onto its
// second column lands on its first, and moving right from it skips its second.
void calc_move(int state[], int pressed)
{
    int row = state[CALC_ROW];
    int col = state[CALC_COL];

    if (pressed & PAD_UP) {
        row = (row + CALC_ROWS - 1) % CALC_ROWS;
    } else if (pressed & PAD_DOWN) {
        row = (row + 1) % CALC_ROWS;
    } else if (pressed & PAD_LEFT) {
        col = (col + CALC_COLUMNS - 1) % CALC_COLUMNS;
        if (row == 4 && col == 1) {
            col = 0;
        }
    } else if (pressed & PAD_RIGHT) {
        col = (row == 4 && col == 0) ? 2 : (col + 1) % CALC_COLUMNS;
    }
    if (row == 4 && col == 1) {
        col = 0;
    }
    state[CALC_ROW] = row;
    state[CALC_COL] = col;
}

// The total after applying op to total and entry; with no op pending the entry becomes the total.
// error[0] is set on dividing by zero (which would stop the game's VM) or a result too long for
// the display.
int calc_apply(int total, int entry, int op, int error[])
{
    int result = entry;
    int size;
    int factor;

    switch (op) {
    case OP_ADD:
        result = total + entry;
        break;
    case OP_SUB:
        result = total - entry;
        break;
    case OP_MUL:
        // checked before multiplying: the game's ints are 32 bits, and the product could wrap
        size = total < 0 ? -total : total;
        factor = entry < 0 ? -entry : entry;
        if (factor > 1 && size > CALC_MAX / factor) {
            error[0] = 1;
            return total;
        }
        result = total * entry;
        break;
    case OP_DIV:
        if (entry == 0) {
            error[0] = 1;
            return total;
        }
        result = total / entry;
        break;
    }
    if (result > CALC_MAX || result < -CALC_MAX) {
        error[0] = 1;
        return total;
    }
    return result;
}

// Presses a key. Returns what happened, as CALC_KEYED for a plain key.
int calc_key(int state[], int key)
{
    int op;

    if (key == KEY_CLOSE) {
        return CALC_CLOSE;
    }
    if (key == KEY_CLEAR) {
        calc_reset(state);
        return CALC_CLEARED;
    }
    if (state[CALC_ERROR]) {
        return CALC_FAILED;
    }
    if (key <= 9) {
        if (!state[CALC_TYPED]) {
            state[CALC_ENTRY] = 0;
        }
        if (state[CALC_ENTRY] > CALC_MAX / 10 || state[CALC_ENTRY] < -CALC_MAX / 10) {
            return CALC_FAILED;
        }
        state[CALC_ENTRY] = state[CALC_ENTRY] < 0 ? state[CALC_ENTRY] * 10 - key : state[CALC_ENTRY] * 10 + key;
        state[CALC_TYPED] = 1;
        return CALC_KEYED;
    }
    if (key == KEY_ERASE) {
        if (!state[CALC_TYPED]) {
            return CALC_FAILED;
        }
        state[CALC_ENTRY] /= 10;
        return CALC_KEYED;
    }
    if (key == KEY_NEG) {
        if (state[CALC_TYPED]) {
            state[CALC_ENTRY] = -state[CALC_ENTRY];
        } else {
            state[CALC_TOTAL] = -state[CALC_TOTAL];
        }
        return CALC_KEYED;
    }
    op = key == KEY_ADD ? OP_ADD : key == KEY_SUB ? OP_SUB : key == KEY_MUL ? OP_MUL : key == KEY_DIV ? OP_DIV : OP_NONE;
    // an operator pressed again before typing only changes the pending one, so 7 + = keeps 7
    if (state[CALC_TYPED]) {
        int error[1] = {0};

        state[CALC_TOTAL] = calc_apply(state[CALC_TOTAL], state[CALC_ENTRY], state[CALC_OP], error);
        state[CALC_ERROR] = error[0];
        state[CALC_ENTRY] = state[CALC_TYPED] = 0;
    }
    state[CALC_OP] = op;
    if (state[CALC_ERROR]) {
        return CALC_FAILED;
    }
    return key == KEY_EQUALS ? CALC_EQUALED : CALC_KEYED;
}

// Applies one frame's pressed buttons (a get_pad_pressed bitmask): the D-pad moves the cursor,
// (X) presses the key under it, (O) and [select] close, [start] clears, (#) erases, and the
// shoulder buttons are the operators ([r1] + [l1] - [r2] x [l2] /), (^) is =.
int calc_press(int state[], int pressed)
{
    if (pressed & (PAD_CIRCLE | PAD_SELECT)) {
        return CALC_CLOSE;
    }
    if (pressed & PAD_START) {
        return calc_key(state, KEY_CLEAR);
    }
    if (pressed & (PAD_UP | PAD_DOWN | PAD_LEFT | PAD_RIGHT)) {
        calc_move(state, pressed);
        return CALC_MOVED;
    }
    if (pressed & PAD_CROSS) {
        return calc_key(state, calc_key_at(state[CALC_ROW], state[CALC_COL]));
    }
    if (pressed & PAD_SQUARE) {
        return calc_key(state, KEY_ERASE);
    }
    if (pressed & PAD_TRIANGLE) {
        return calc_key(state, KEY_EQUALS);
    }
    if (pressed & PAD_R1) {
        return calc_key(state, KEY_ADD);
    }
    if (pressed & PAD_L1) {
        return calc_key(state, KEY_SUB);
    }
    if (pressed & PAD_R2) {
        return calc_key(state, KEY_MUL);
    }
    if (pressed & PAD_L2) {
        return calc_key(state, KEY_DIV);
    }
    return CALC_IGNORED;
}
