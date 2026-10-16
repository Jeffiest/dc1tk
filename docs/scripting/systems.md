# Map, Monster, and Player Scripts

This page explains how each type of script in `extracted/scripts/` works in practice. For the core execution model (`PROGRAM`, `wait;`, and state), see [How Scripts Work & Lifecycle](README.md).

## Towns and story maps (`<event.h>`)

In a town (`map/g/`, `map/m/`, `map/i/`, `map/f/`) or story map (`map/s/`), the player walks around freely until an event program starts.

### Talking to people

When the player presses confirm next to an NPC, the engine starts `PROGRAM(talk_or_dungeon_start, ...)`. The handler checks who the player talked to with `get_talk_chara()` and checks story flags with `get_flag()`:

```c
void on_talk(void)
{
    switch (get_talk_chara()) {
    case NPC_GORDON:
        if (get_flag(FLAG_MET_GORDON)) {
            show_message(0, "The trees are looking healthy.");
        } else {
            set_flag(FLAG_MET_GORDON, 1);
            show_message(0, "Hello! I'm Gordon, the gardener.");
        }
        break;
    }
}
```

- In retail scripts, `show_message(window, text_or_id)` displays either a literal string or a numbered line from the scene's external `.txt` dialogue file.
- In a mod, you can pass any string literal directly to `show_message(0, "Your text")`, or include `#include "modkit/text.c"` and call `modkit_say("Your text")` to open window `0`, wait for the player to press confirm, and close the window.

### Cutscenes

A cutscene is an event function that takes control of the camera and moves characters using **actors** (event puppets driven by command queues):

```c
#include <event.h>
#include "modkit/text.c"

void play_welcome_scene(void)
{
    float x, y, z;

    init_event();                           // 1. Enter event mode and clear old state
    init_chara_actor(0, CHARA_PLAYER);      // 2. Bind actor 0 to the player character

    get_chara_pos(CHARA_PLAYER, &x, &y, &z);
    cut_camera(x + 100.0, y + 50.0, z + 100.0, x, y + 20.0, z); // 3. Position camera

    set_actor_motion(0, "手を振る", CHARA_MOTION_RESTART | CHARA_MOTION_HOLD);
    while (check_actor_busy(0)) {
        wait;
    }

    modkit_say("Here we are!");             // 4. Show dialogue

    reset_camera(CAMERA_RESET_ACTIVE);      // 5. Return camera to gameplay
    finish_event();                         // 6. Exit event mode
}
```

- **Actors vs. character slots**: Characters live in scene slots (`CHARA_PLAYER` is the active hero; townspeople occupy slots `8` and up; custom scene models load into free slots with `load_scene_chara(slot, "chara/....chr")`). `init_chara_actor(actor, slot)` attaches an actor controller to that character slot.
- **Skipping cutscenes**: Retail cutscenes wrap their main action between `enable_scene_skip();` and `skip_end; disable_scene_skip();`. If the player presses Start to skip, the VM jumps directly to `skip_end;`. Keep all `set_flag(...)` calls and cleanup outside the skippable block so flags are always set.
- **Warping to another map**: Call `jump_to_map(area, "m01", program_number)` at the end of an event to load another map and start `program_number` there (`map_start` is `100`).

### Story fights and boss arenas

A scripted battle inside a story map (`map/s/`) or boss arena (`map/d/.../bNN_.../`) loads specific monsters, places them, and switches the map into dungeon combat mode:

```c
void start_ambush(void)
{
    init_dungeon_Map();
    load_monster_data(0, MONSTER_SEWER_RAT);
    place_monster(MONSTER_SEWER_RAT, 0.0, 0.0, 100.0, 0.0, 0.0, 0.0, 0);
    set_chara_pos(CHARA_PLAYER, 0.0, 0.0, -100.0);
    start_scripted_battle();
}
```

How the fight ends depends on the map:

- If the map defines `PROGRAM(monsters_cleared, on_win)` (`1500`), the engine runs `on_win` automatically when the last monster dies.
- In some story fights, a custom monster AI counts deaths in a shared monster variable (`get_monster_var(8)`) and calls `run_event_script(510)` when the last enemy falls. Check `dc2tk xref extracted/scripts map <map>` to see how a specific fight ends.

## Dungeons (`<event.h>`)

Each dungeon chapter (`d01` to `d05` under `map/d/`) has one main script (`d01_1.c` to `d01_5.c`) that runs on every generated floor of that dungeon:

- `floor_start` (`1010`): Runs when a floor loads. It places the dungeon's monsters (`load_monster_data(0); place_monster(-1);`), key item, and Georama stone.
- `treasure_chest` (`1100`): Runs when the player interacts with a chest.
- `monsters_cleared` (`1500`): Runs when all monsters on the floor are defeated, unlocking the exit floor gate.

> **Note:** Which monsters spawn randomly on each floor and what items chests contain are stored in the `data:floors` and `data:chests` JSON tables, not hardcoded in the script. See [Game Data](../guide/data.md).

## Monsters

Monster AI scripts live in `dungeon/monster/` (`e*.c` for regular enemies, `b*.c` for bosses). You can see which AI script and 3D model any monster uses in `extracted/scripts/include/disc.h` or by running `dc2tk xref extracted/scripts monster <name>`.

A monster script responds to engine state programs:

```c
#include <monster.h>
#include <disc.h>

PROGRAM(init, on_init);
PROGRAM(guard, on_guard);
PROGRAM(damage, on_damage);
PROGRAM(death, on_death);
PROGRAM(update, on_update);
MAIN(on_init);

#include "lib/dungeon/monster/monster_ai_full_toolkit.c"

void on_update(void)
{
    while (1) {
        wait_frame_update();            // Waits 1 frame and updates aggro/timers
        if (get_target_dist() < 60.0) {
            // Start an attack animation and active hitbox window
            set_motion("攻撃1", -1);
            set_attack_frames(10, 18);  // Hitbox deals damage between frames 10 and 18
            wait_motion_end();
        } else {
            chase_target_steps(20);
        }
    }
}
```

- **Shared monster libraries**: Most monsters include `lib/dungeon/monster/monster_ai_full_toolkit.c`. If you want to change a library function for only one monster, use `#pragma target monster golem` and `#pragma override` in a mod patch so you do not affect the other 35 monsters that include the same library.
- **Monster stats**: HP, attack, defense, drops, and which AI script a monster kind runs are stored in `data:monsters` (see [Game Data](../guide/data.md)).

## The player

- **Player actions (`mainchr/c01_max.c`, `c02_monica.c`, `dungeon/act_script/`)**: Control melee combo chains, charge attacks, gun/magic firing, and Ridepod/monster form attacks. During each swing, a loop checks `get_motion_frame()`, activates the weapon hitbox between start and end frames with `set_weapon_hit(1)`, and listens for the next `(X)` button press with `check_pad_on(PAD_CIRCLE)` to chain into the next combo step. See [Tune Player Combat](tutorials/tune-player.md).
- **Effects (`dungeon/eff_script/`)**: Control particles, shockwaves, and projectiles spawned by monsters or players (`run_effect_script`).
