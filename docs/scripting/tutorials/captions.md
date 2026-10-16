# Show live captions during a fight

This guide shows how to display a new caption every five seconds during an active battle without pausing player control.

This pattern combines four engine rules explained in [How scripts run](../../internals/running.md):

1. [A map event that never executes `wait;` completes within a single frame without freezing the player](../../internals/running.md#an-event-that-never-waits-costs-nothing).
2. [Message window `0` remains visible on screen in dungeon mode after the event ends](../../internals/running.md#message-window-0-stays-on-screen).
3. [Monster AI scripts run every frame during combat and can trigger map programs with `run_event_script`](../../internals/running.md#monsters-can-start-map-events).
4. [Map and monster scripts share save flags and the floor timer](../../internals/running.md#shared-state).

For background on mod patches, `#pragma every_frame`, and `NEW_FLAG`, see [mods.md](../../guide/mods.md).

> **Tip**
> **Ready-to-use `modkit` helpers**
> `sdk/modkit/` provides built-in functions for this exact pattern: `modkit_caption` and `modkit_text_show` in `"modkit/text.c"` for the map script, and `modkit_tick` in `"modkit/tick.c"` for the monster script (see [modkit](../language.md#modkit-helper-libraries)). The example below writes out the underlying native calls directly to show how the mechanism works.

## 1. Declare shared IDs in a header

Create a shared header file (`captions.h`) in your mod directory so both the map patch and the monster patch use the same allocated flag and program numbers:

```c
// captions.h
NEW_FLAG(CAPTIONS_ON);          // Active while the battle is running
NEW_PROGRAM(SHOW_NEXT_CAPTION); // Map program triggered by the monster timer
```

## 2. Write the map patch

Create the map patch targeting `s51` (`#pragma target map s51` applies the patch to all five language files of `s51`):

```c
#pragma target map s51
#include <event.h>
#include "captions.h"

int caption;

PROGRAM(SHOW_NEXT_CAPTION, show_next_caption);

// Displays the next caption in message window 0 and returns immediately without waiting.
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

// Enables the caption timer and shows the first caption as soon as the battle starts.
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

Also `#pragma override` the battle-end function and the `player_dead` handler to clear `CAPTIONS_ON` (`set_flag(CAPTIONS_ON, 0);`) and close window `0` (`close_message(0);`).

## 3. Write the monster patch

Because map event scripts do not run in the background during active combat, the monsters in the battle act as the timer. Create a monster patch with `#pragma every_frame` so `tick_captions` runs once per frame regardless of which `wait;` loop the monster AI is currently in:

```c
#pragma target monster golem
#include <monster.h>
#include "captions.h"

// When CAPTIONS_ON is set, triggers SHOW_NEXT_CAPTION every 125 frames (5 seconds at 25 FPS).
// Because the floor timer is shared across all monsters on the floor, only the first monster
// to observe timer >= 125 resets the timer and starts the map event.
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

Checking `get_dungeon_flag(CAPTIONS_ON)` ensures that Golems on other floors do not trigger `SHOW_NEXT_CAPTION`.
