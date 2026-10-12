// modkit input: buttons and waiting, for event (map) scripts. Include with
// #include "modkit/input.c". Button masks are the PAD_ constants of <event.h>; frames are 50 a
// second on a PAL console.

#include <event.h>

// Waits the given number of frames.
void modkit_wait(int frames)
{
    while (frames-- > 0) {
        wait;
    }
}

// Returns the buttons of mask pressed this frame, 0 when none.
int modkit_pressed(int mask)
{
    return get_pad_pressed() & mask;
}

// Waits until a button of mask is pressed and returns the buttons pressed. A button already held
// down when this starts has to be pressed again.
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
