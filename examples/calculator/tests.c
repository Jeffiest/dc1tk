// Tests for the calculator's logic, run without the game with:
//   c2 mod test extracted/scripts calculator --mods examples
// Each test moves the cursor and presses keys through calc_press, as the pad would.

#include <event.h>
#include "logic.c"

// Puts the cursor on a key and presses (X) on it.
int press_key(int state[], int key)
{
    int row;
    int col;

    for (row = 0; row < CALC_ROWS; row++) {
        for (col = 0; col < CALC_COLUMNS; col++) {
            if (calc_key_at(row, col) == key) {
                state[CALC_ROW] = row;
                state[CALC_COL] = col;
                return calc_press(state, PAD_CROSS);
            }
        }
    }
    print("no key", key);
    return CALC_IGNORED;
}

// Types a number on the keys (negative ones with the +/- key after).
void type_number(int state[], int number)
{
    int digits[8];
    int count = 0;
    int negative = number < 0;

    if (negative) {
        number = -number;
    }
    do {
        digits[count++] = number % 10;
        number /= 10;
    } while (number > 0);
    while (count > 0) {
        press_key(state, digits[--count]);
    }
    if (negative) {
        press_key(state, KEY_NEG);
    }
}

// The display after a op b =.
int worked(int a, int op_key, int b)
{
    int state[CALC_STATE];

    calc_start(state);
    type_number(state, a);
    press_key(state, op_key);
    type_number(state, b);
    press_key(state, KEY_EQUALS);
    return calc_shown(state);
}

int test_the_four_operations(void)
{
    return worked(12, KEY_ADD, 30) == 42 && worked(7, KEY_SUB, 10) == -3 && worked(6, KEY_MUL, 7) == 42
        && worked(100, KEY_DIV, 7) == 14 && worked(-5, KEY_MUL, 3) == -15;
}

int test_the_cursor_wraps_and_the_zero_key_is_wide(void)
{
    int state[CALC_STATE];

    calc_start(state);
    calc_press(state, PAD_UP);
    calc_press(state, PAD_UP);
    if (state[CALC_ROW] != 4 || calc_key_at(state[CALC_ROW], state[CALC_COL]) != 0) {
        return 0;
    }
    calc_press(state, PAD_RIGHT);
    if (calc_key_at(state[CALC_ROW], state[CALC_COL]) != KEY_NEG) {
        return 0;
    }
    calc_press(state, PAD_LEFT);
    calc_press(state, PAD_LEFT);
    // left of the 0 key wraps to =
    return calc_key_at(state[CALC_ROW], state[CALC_COL]) == KEY_EQUALS;
}

int test_dividing_by_zero_is_an_error_until_cleared(void)
{
    int state[CALC_STATE];

    calc_start(state);
    type_number(state, 9);
    press_key(state, KEY_DIV);
    type_number(state, 0);
    if (press_key(state, KEY_EQUALS) != CALC_FAILED || !state[CALC_ERROR] || press_key(state, 5) != CALC_FAILED) {
        return 0;
    }
    return calc_press(state, PAD_START) == CALC_CLEARED && !state[CALC_ERROR] && calc_shown(state) == 0;
}

int test_results_too_long_for_the_display_are_errors(void)
{
    int state[CALC_STATE];

    calc_start(state);
    type_number(state, 99999999);
    if (press_key(state, 9) != CALC_FAILED || calc_shown(state) != 99999999) {
        return 0;
    }
    press_key(state, KEY_MUL);
    type_number(state, 10);
    if (press_key(state, KEY_EQUALS) != CALC_FAILED || !state[CALC_ERROR]) {
        return 0;
    }
    // a negative factor too
    calc_start(state);
    type_number(state, 50000);
    press_key(state, KEY_MUL);
    type_number(state, -5000);
    return press_key(state, KEY_EQUALS) == CALC_FAILED && worked(-9999, KEY_MUL, -9999) == 99980001;
}

int test_an_operator_after_equals_keeps_the_result(void)
{
    int state[CALC_STATE];

    calc_start(state);
    type_number(state, 5);
    press_key(state, KEY_ADD);
    press_key(state, KEY_EQUALS);
    press_key(state, KEY_ADD);
    type_number(state, 3);
    return press_key(state, KEY_EQUALS) == CALC_EQUALED && calc_shown(state) == 8;
}

int test_erase_shoulder_buttons_and_closing(void)
{
    int state[CALC_STATE];

    calc_start(state);
    type_number(state, 123);
    calc_press(state, PAD_SQUARE);
    if (calc_shown(state) != 12) {
        return 0;
    }
    calc_press(state, PAD_R1);
    type_number(state, 8);
    calc_press(state, PAD_TRIANGLE);
    return calc_shown(state) == 20 && calc_press(state, PAD_CIRCLE) == CALC_CLOSE && press_key(state, KEY_CLOSE) == CALC_CLOSE;
}
