// modkit text: message window helpers for event (map) scripts. Include with
// `#include "modkit/text.c"`.
//
// Displays text immediately in the top hint slot without a button prompt.
// Supports standard message formatting codes: `[Number1]` to `[Number16]`
// (set with `set_message_value` before displaying the text), button icons such
// as `(X)`, `(O)`, `[r1]`, and `[select]`, `\n` for line breaks, and `<page>`
// for page breaks (see `docs/internals/running.md`).

#include <event.h>
#include "modkit/input.c"

// Configures `window` for script-controlled text display.
void modkit_text_open(int window)
{
    apply_message_preset(window, MES_PRESET_WINDOW);
    set_message_window_mode(window, MES_WIN_VERSATILE_1);
    set_message_speed(window, 0);
    set_message_prompt(window, 0);
}

// Displays `text` in `window`, replacing any existing text.
void modkit_text_show(int window, string text)
{
    show_message(window, text);
    show_message(window, -1);
    set_message_slot(window, 8);
    place_message_bubble(window, 0, 0, 0, 0);
}

// Closes `window` and restores the default button prompt.
void modkit_text_close(int window)
{
    set_message_prompt(window, 1);
    set_message_cursor(window, -1);
    close_message(window);
    set_message_slot(window, 0);
}

// Displays `text` in window 0 until the player presses (X) or (O), then closes the window.
void modkit_say(string text)
{
    modkit_text_open(0);
    modkit_text_show(0, text);
    modkit_wait(10);
    modkit_wait_for_confirm();
    modkit_text_close(0);
}

// Displays `text` in window 0 and keeps it on screen during gameplay until `modkit_text_close(0)` is called.
void modkit_caption(string text)
{
    modkit_text_open(0);
    modkit_text_show(0, text);
}
