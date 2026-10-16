// modkit input: button polling and waiting helpers for event (map) scripts.
// Include with `#include "modkit/input.c"`. Button masks use the `PAD_`
// constants from `<event.h>`.

#include <event.h>

// Waits the given number of frames.
void modkit_wait(int frames)
{
    while (frames-- > 0) {
        wait;
    }
}

// Returns the buttons in `mask` pressed this frame, or 0 if none were pressed.
int modkit_pressed(int mask)
{
    return get_pad_pressed() & mask;
}

// Waits until a button in `mask` is newly pressed and returns the pressed button mask.
int modkit_wait_for_button(int mask)
{
    int pressed;

    wait;
    while (!(pressed = get_pad_pressed() & mask)) {
        wait;
    }
    return pressed;
}

// Waits for (X) or (O).
void modkit_wait_for_confirm(void)
{
    modkit_wait_for_button(PAD_CROSS | PAD_CIRCLE);
}
