# Map, Monster, and Player Scripts

This page explains how each script type in `extracted/scripts/` works. For the execution model (`PROGRAM`, `WAIT();`, and state), see [How Scripts Work & Lifecycle](README.md).

## Talking to people

In a town (`map/g/`, `map/m/`, `map/i/`, `map/f/`) or story map (`map/s/`), pressing confirm next to an NPC starts `PROGRAM(talk_or_dungeon_start, ...)`. The handler identifies the NPC with `get_event_data(EVENT_DATA_CHARA_NO, &talk_chara)` (or `get_talk_chara()`) and checks story progress with `get_flag()`:

```c
void on_talk(void)
{
    switch (get_talk_chara()) {
    case NPC_GORDON:
        if (get_flag(FLAG_GEOSTONE_PLAN_SCENE_DONE)) {
            modkit_say("The trees are looking healthy.");
        } else {
            modkit_say("Hello! I'm Gordon, the gardener.");
        }
        break;
    }
}
```

- `show_message(window, text_or_id)` lays out a string literal or a numbered message line from the scene's dialogue table, and `talk_to_npc` or `show_message_auto` displays the speech bubble and waits for input.
- In a mod, you can pass any string literal to `show_message(0, "Your text")`, or include `"modkit/text.c"` and call `modkit_say("Your text")` to open window `0`, wait for confirm, and close the window. See [Change what an NPC says](tutorials/npc-line.md).

## Cutscenes

A cutscene is an event function that controls the camera and moves characters using **actors** (event controllers bound to character slots):

```c
#include <event.h>
#include <disc.h>
#include "modkit/text.c"

void play_welcome_scene(void)
{
    float x, y, z;

    init_event();                           // 1. Enter event mode
    init_chara_actor(0, CHARA_PLAYER);      // 2. Bind actor 0 to the player character

    get_chara_pos(CHARA_PLAYER, &x, &y, &z);
    cut_camera(x + 100.0, y + 50.0, z + 100.0, x, y + 20.0, z); // 3. Place camera

    set_actor_motion(0, MOTION_WAVE, CHARA_MOTION_RESTART | CHARA_MOTION_HOLD);
    while (check_actor_busy(0)) {
        WAIT();
    }

    modkit_say("Here we are!");             // 4. Show dialogue

    reset_camera(CAMERA_RESET_ACTIVE);      // 5. Restore gameplay camera
    finish_event();                         // 6. Exit event mode
}
```

- **Actors vs. character slots**: Characters occupy scene slots (`CHARA_PLAYER` is the active player; townspeople use slots `8` and above; custom models load into free slots with `load_scene_chara` or `load_inactive_chara`). `init_chara_actor(actor, slot)` binds an actor controller to a character slot.
- **Skipping cutscenes**: Wrap the skippable timeline between `enable_scene_skip();` and `SKIP_END(); disable_scene_skip();`. Pressing Start jumps to `SKIP_END();`. Keep all `set_flag`, character cleanup, and map warps after `SKIP_END();` so they always run.
- **Warping to another map**: Call `jump_to_map(area, "m01", program_number)` to load another map and start `program_number` (`map_start` is `100`). See [Add a cutscene to a town](tutorials/cutscene.md).

## Story fights and boss arenas

A scripted fight in a story map (`map/s/`) or boss arena (`map/d/.../bNN_.../`) loads monster data into memory and places each monster with coordinates and rotation:

```c
void start_castle_battle(void)
{
    play_event_50();
    load_monster_data(MONSTER_GRIFFON_SOLDIER_523, 1);
    place_monster(MONSTER_GRIFFON_SOLDIER_523, -382.5, -0.2, 120.2, 0.0, 1.61, 0.0);
    place_monster(MONSTER_GRIFFON_SOLDIER_523, -428.0, -0.2, 202.7, 0.0, 1.71, 0.0);
    WAIT();
    fade_in(20);
    finish_event();
}
```

A scripted battle ends in one of two ways:

- **Engine clear trigger**: If the map registers `PROGRAM(monsters_cleared, on_win)` (`1500`), the engine starts `on_win` when the last monster dies.
- **Monster death script**: Some event monsters increment a shared monster variable (`get_monster_var(127, &kill_count)`) in their `death` program and call `run_event_script(510)` when the final enemy dies. See [Add a new enemy to a fight](tutorials/new-enemy.md).

## Dungeons (`<event.h>`)

Each dungeon chapter (`d01` to `d05` under `map/d/`) uses one script (`d01_1.c` to `d01_5.c`) across all generated floors of that dungeon:

- `floor_start` (`1010`): Runs when a floor loads to place random floor monsters, the key item, and the Geostone.
- `treasure_chest` (`1100`): Runs when the player opens a chest.
- `monsters_cleared` (`1500`): Runs when all monsters on the floor are defeated, unlocking the exit gate.

> **Note:** Random floor spawn tables and chest item tables are stored in `data:floors` and `data:chests`, not in `.stb` scripts. See [Game Data](../guide/data.md).

## Monsters

Monster AI scripts live in `dungeon/monster/` (`e*.c` for standard enemies, `b*.c` for bosses and event enemies). Check `extracted/scripts/include/disc.h` or run `dc2tk xref extracted/scripts monster <name>` to find a monster's AI script and 3D model.

```c
#include <monster.h>
#include <pal.h>
#include <disc.h>

PROGRAM(init, on_init);
PROGRAM(guard, on_guard);
PROGRAM(damage, play_damage_blowback);
PROGRAM(death, play_death_motion);
PROGRAM(update, on_update);
MAIN(start);

#include "lib/dungeon/monster/monster_ai_full_toolkit.c"

void on_init(void)
{
    add_attack_hit(HIT_KNOCKBACK_ATTACK, 6.5, MOTION_ATTACK_1_2, 0.3, 0.37, 0);
    init_monster_ai();
}

void chase_and_swing_at_target(void)
{
    loop_motion(MOTION_WALK);
    chase_target_steps(1.25, 3.0, 62.0 + 4.0, 90);
    turn_to_target_wait(3.0);
    set_motion(MOTION_ATTACK_1_2, -1, CHARA_MOTION_HOLD);
    wait_motion_end_update();
    loop_motion(MOTION_STAND_2);
    wait_frames_update(6);
}
```

- **Patching one monster**: Most monsters include `lib/dungeon/monster/monster_ai_full_toolkit.c`. A patch with `#pragma target monster golem` and `#pragma override` modifies only `e33a_golem.c` without affecting the other monsters that include the same library. See [Change enemy behavior](tutorials/enemy-behaviour.md).
- **Monster stats**: Base HP, attack, defense, drops, and AI script assignments live in `data:monsters` (see [Game Data](../guide/data.md)).

## The player

- **Player actions (`mainchr/c01_max.c`, `c02_monica.c`, `dungeon/act_script/`)**: Control melee combos, charge attacks, ranged attacks, and Ridepod/monster form actions. In `init` (`setup_max_attacks`), `add_attack_hit` registers the hitbox radius, damage multiplier, and active animation progress window (`0.0` to `1.0`) for each attack motion. During `update`, the combo state machine checks `get_motion_progress(&progress)` and `get_attack_presses()` to chain into the next swing. See [Tune the player](tutorials/tune-player.md).
- **Effects (`dungeon/eff_script/`)**: Control particles, shockwaves, and projectiles spawned by monsters or players.
