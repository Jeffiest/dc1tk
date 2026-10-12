// modkit text: message windows for event (map) scripts. Include with #include "modkit/text.c".
//
// Text is shown at once, without the game's button prompt, in the top slot the game uses for its
// hints. It can hold the codes the game's own text uses: [Number1] to [Number16] for numbers set
// with set_message_value, button icons such as (X), (O), [r1] and [select], \n for a new line, and
// <page> for a page break (see docs/modding.md, "Message text"). Window 0 is the event window,
// which stays on screen after an event ends until closed (see the modding guide).

#include <event.h>
#include "modkit/input.c"

// Gets window ready for text the script controls.
void modkit_text_open(int window)
{
    apply_message_preset(window, MES_PRESET_WINDOW);
    set_message_window_mode(window, MES_WIN_VERSATILE_1);
    set_message_speed(window, 0);
    set_message_prompt(window, 0);
}

// Shows text in window, replacing what it showed.
void modkit_text_show(int window, string text)
{
    show_message(window, text);
    show_message(window, -1);
    set_message_slot(window, 8);
    place_message_bubble(window, 0, 0, 0, 0);
}

// Closes window and gives back the game's default button prompt.
void modkit_text_close(int window)
{
    set_message_prompt(window, 1);
    set_message_cursor(window, -1);
    close_message(window);
    set_message_slot(window, 0);
}

// Shows text in window 0 until the player presses (X) or (O), then closes it.
void modkit_say(string text)
{
    modkit_text_open(0);
    modkit_text_show(0, text);
    modkit_wait(10);
    modkit_wait_for_confirm();
    modkit_text_close(0);
}

// Shows text in window 0 and leaves it there, for as long as the scene needs it: it stays up after
// the event ends, while the player plays, until modkit_text_close(0).
void modkit_caption(string text)
{
    modkit_text_open(0);
    modkit_text_show(0, text);
}
