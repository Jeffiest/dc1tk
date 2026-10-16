# A caption every five seconds during a fight

A mod that shows a caption every five seconds while a fight goes on, with the player in control.
It puts together the rules in [how scripts run](../../game/running.md):
[an event that never waits costs nothing](../../game/running.md#an-event-that-never-waits-costs-nothing),
[message window 0 stays on screen](../../game/running.md#message-window-0-stays-on-screen),
[monsters can start map events](../../game/running.md#monsters-can-start-map-events) and
[shared state](../../game/running.md#shared-state). Patches, `#pragma every_frame` and
`NEW_FLAG` are in [mods](../../modding/mods.md).

`sdk/modkit/` ships this pattern ready to use: `modkit_caption` and
`modkit_text_show` (`"modkit/text.c"`) on the map side and `modkit_tick`
(`"modkit/tick.c"`) on the monster side (see [modkit](../sdk.md#modkit)). This made-up example
spells it out with natives, to show how it works, and puts the rules above together. A map
program shows the next caption and never waits, so it runs within a frame while the player keeps
control. The monsters in the fight provide the clock, because event scripts
cannot run beside play and monster scripts do.

## The numbers

The mod's two patches share a header, so they agree on the numbers `build`
gives them:

```c
// captions.h
NEW_FLAG(CAPTIONS_ON);          // set while the fight runs
NEW_PROGRAM(SHOW_NEXT_CAPTION); // the map program the monsters ask for
```

## The map's patch

The map's patch (`#pragma target map s51` reaches every language variant):

```c
#pragma target map s51
#include <event.h>
#include "captions.h"

int caption;

PROGRAM(SHOW_NEXT_CAPTION, show_next_caption);

// Shows the next caption in window 0. It never waits, so it runs within one frame.
void show_next_caption(void)
{
    caption = caption + 1;
    set_message_window_mode(0, MES_WIN_VERSATILE_1);
    set_message_prompt(0, 0);
    if (caption == 1) {
        show_message(0, "The first caption");
    }
    if (caption == 2) {
        show_message(0, "The second caption");
    }
    if (caption <= 2) {
        show_message(0, -1);
        set_message_slot(0, 8);
        place_message_bubble(0, 0, 0, 0, 0);
    }
    if (caption == 3) {
        close_message(0);
        set_message_prompt(0, 1);
    }
}

// Starts the captions once the fight's monsters are placed.
#pragma override
void start_castle_battle(void)
{
    original();
    set_flag(CAPTIONS_ON, 1);
    reset_dungeon_timer();
    caption = 0;
    show_next_caption();
}
```

Clear the flag and close the window where the fight ends and in the
`player_dead` program's function, overriding them the same way.

## The monsters' patch

The monsters' patch runs a function every frame, whatever wait their AI is in:

```c
#pragma target monster golem
#include <monster.h>
#include "captions.h"

// While the fight's flag is set, asks the map for the next caption every 125 frames (5 seconds
// at 25 frames a second). The timer is shared, so with several monsters only the first to see it run out asks.
#pragma every_frame
void tick_captions(void)
{
    int fight;
    int timer;

    fight = get_dungeon_flag(CAPTIONS_ON);
    if (fight) {
        timer = get_floor_timer();
        if (timer >= 125) {
            reset_floor_timer();
            run_event_script(SHOW_NEXT_CAPTION);
        }
    }
}
```

The flag keeps the same monsters elsewhere in the game from asking.
