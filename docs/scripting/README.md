# Scripting Guide

This section explains how to read, modify, and write script code for Dark Chronicle (Dark Cloud 2). The decompiled game scripts live in `extracted/scripts/`.

## Section contents

- [language.md](language.md): Script language syntax, types, statements, pragmas, and VM behavior.
- [c-standard.md](c-standard.md): C89 standard support, implementation-defined behavior, and lowering rules.
- [sdk.md](sdk.md): SDK headers, native declarations, value kinds, and the `modkit` library.
- [editor.md](editor.md): Editor setup for `clangd` code completion and type checking.
- [tutorials/](tutorials/README.md): Step-by-step guides for common scripting tasks.
- [api/](api/README.md): Reference documentation for every native function by header:
  - [event.md](api/event.md)
  - [monster.md](api/monster.md)
  - [action.md](api/action.md)
  - [effect.md](api/effect.md)
  - [arg.md](api/arg.md)
  - [modkit.md](api/modkit.md)
- Example mods:
  - [examples/calculator/calculator.c](examples/calculator/calculator.c): Interactive calculator mod with a custom model, display, and sound effects.
  - [examples/spinning-cult/README.md](examples/spinning-cult/README.md): Story mod with a custom map, character, invention, animations, and music.

> **Note**
> This repository does not store proprietary game code or dialogue text. Excerpts from `extracted/scripts/` on this page are abridged (`...` indicates omitted lines) and omit dialogue strings. Each excerpt gives its file path so you can read the full file in your local `extracted/scripts/` tree. Code in the tutorials is original code written for this documentation.

## The big picture

The game architecture separates the **engine** (the PS2 executable) from the **scripts** (approximately 2,500 C files compiled to `.stb` bytecode):

- The **engine** renders graphics, runs physics and collision detection, plays animations and audio, and owns core data tables.
- The **scripts** control gameplay logic: cutscenes, NPC conversations, doors, treasure chests, monster AI, player combat combos, and visual effects.

Scripts call engine functions through approximately 1,100 **natives**. The engine calls scripts by starting numbered **programs** when game events occur.

```text
Engine (PS2 executable: scripts cannot modify engine code)
    rendering · collision · animation · sound · monster stat table · floor monster lists ·
    chest contents · effect name table

        |  starts programs:                       ^  calls natives:
        |  map_start, update, death, play …       |  place_monster, set_motion …
        v                                         |

Scripts: Map scripts · Monster AI · Player actions · Effects · Arg tables
```

There are five script types. Each script type has its own SDK header in `sdk/` and its own native function table:

| Script type | Directory and SDK header | Purpose |
| --- | --- | --- |
| Map scripts | `map/`, `event.h` | Control towns, dungeons, cutscenes, and story progression. |
| Monster AI | `dungeon/monster/`, `monster.h` | Control monster behavior. One script instance runs per active monster. |
| Player actions | `mainchr/` and `dungeon/act_script/`, `action.h` | Control combat actions for Max, Monica, monster transformations, and the Ridepod. |
| Effects | `dungeon/eff_script/`, `effect.h` | Control visual effects, projectiles, beams, shockwaves, and effect hitboxes. |
| Arg tables | `event/`, `arg.h` | Load parameter data records for specific scenes. |

Scripts can read and trigger engine data tables, but scripts cannot modify those tables at runtime. To change monster stats, dungeon floor spawn lists, or chest contents, use `data:` tables in `mod.json`.

### What lives where

| Feature or behavior | Location | How to modify it |
| --- | --- | --- |
| Cutscenes, NPC dialogue, door destinations, and story progression | Map scripts (`map/`) | Edit or patch the map script. |
| Monster movement, attack selection, hit reactions, death logic, and boss phases | Monster AI scripts (`dungeon/monster/`) | Edit or patch the monster AI script. |
| Player combo timing, hit windows, charge attacks, ranged shots, and magic | Player action scripts (`mainchr/`) | Edit or patch the player action script. |
| Visual effect movement, animations, and shockwave damage | Effect scripts (`dungeon/eff_script/`) | Edit or patch the effect script. |
| Total list of monster kinds | Engine monster table | Fixed. Mods cannot add new monster rows. |
| Monster model, HP, stats, item drops, and assigned AI script | Engine monster table | Use the `data:monsters` table in `mod.json` (patched into the executable). |
| Random dungeon floor monster lists and floor chest contents | Engine floor data files | Use the `data:floors` and `data:chests` tables in `mod.json`, or spawn extra monsters and chests from a map script. |
| 3D models, animations, textures, sound banks, music, and message files | Disc data files | Reference existing assets from scripts, or replace and add assets in `mod.json`. |

## Rules every script follows

All five script types share the same file structure. Below is an abridged excerpt from the Golem AI script in `dungeon/monster/e33a_golem.c`:

```c
/* 1 */ // Monster script for the Golem family (Golem, Rave Golem, Rock Taster, Titan; kinds 168 to 171).
// It swings, slams a Vanguard shockwave into the ground, guards and taunts, ...

/* 2 */ #include <monster.h>
#include <pal.h>
#include <disc.h>

/* 3 */ float spawn_x;
float spawn_y;
float spawn_z;

/* 4 */ PROGRAM(init, on_init);
PROGRAM(guard, on_guard);
PROGRAM(damage, play_damage_blowback);
PROGRAM(death, play_death_motion);
PROGRAM(update, on_update);
...
MAIN(start);

/* 5 */ #include "lib/dungeon/monster/monster_ai_full_toolkit.c"
...

void on_update(void)
{
    set_mask_flag(MASK_NO_GUARD, 1);
    set_mask_flag(MASK_NO_DODGE, 0);
    set_act_status(0);
    while (1) {
        /* 6 */ wait_frame_update();          // waits one frame (and updates aggro)
        choose_behavior_by_aggro();
    }
}
```

1. **File and function comments**: Every decompiled file and function begins with a summary comment generated by `dc2tk`. Read these comments first to understand the file.
2. **SDK and definition headers**: The first `#include` selects the script type (`<monster.h>`, `<event.h>`, `<action.h>`, `<effect.h>`, or `<arg.h>`) and declares all available natives. `<pal.h>` defines save flags, music tracks, and sound effects. `<disc.h>` defines monster kinds, items, NPC IDs, animations, and effects extracted from the disc.
3. **Global variables**: File-scope variables persist for as long as the script stays loaded. All globals start at `0` when the script loads. A script sometimes declares globals (such as `spawn_x`) because an included library reads or writes them.
4. **`PROGRAM` and `MAIN` declarations**: The `PROGRAM(id, function)` table registers entry points that the engine starts when events occur. `MAIN(function)` sets the header's default entry function. Functions in the file run only when started by a `PROGRAM` entry, `MAIN`, or another function call.
5. **Library includes**: `#include "lib/..."` inserts shared C code directly into the script at compile time. Libraries can read and write the globals and functions of the script that includes them.
6. **Frame yielding with `wait;`**: The `wait;` statement pauses the current program for the rest of the frame and resumes execution at the same line on the next frame. Here, `wait_frame_update()` is a library function that executes `wait;` and updates aggro timers.

## Script lifetime

Every script follows a strict lifecycle from load to unload:

```text
1. Load .stb file        -> Engine allocates script VM state and zeroes all global slots.
2. Run initializers      -> First entry point runs hidden __initialise() once per load.
3. Start PROGRAM(id, fn) -> Engine starts entry function when an event occurs.
4. Execute and wait;     -> Coroutine runs until wait; (yields 1 frame) or return (ends program).
5. Unload .stb file      -> Engine frees script state when leaving the map or when the owner dies.
```

### 1. Loading and zeroing globals

- **Map scripts**: Load when the player enters a map, or when a script calls `load_event_script("path.stb", program)` to swap the active map script.
- **Monster AI scripts**: Load per monster instance when the monster spawns on a dungeon floor or battle map.
- **Player action scripts**: Load when the playable character initializes on a dungeon or battle map.
- **Effect scripts**: Load when a script spawns an effect instance.

When the engine loads a `.stb` file, the VM allocates the script's global variable slots at the bottom of the value stack and initializes every slot to `0`.

### 2. Running `MAIN` and global initializers

Every `.stb` header records one `MAIN(function)` entry point. In retail scripts, globals have no initializers and start at `0`. When you write custom C code with initialized globals (for example, `int lives = 3;`) or `static` local variables, the `dc2tk` compiler generates a hidden `__initialise` function guarded by a hidden global flag. Every `MAIN` function and `PROGRAM` entry point calls `__initialise` first. This ensures that global and `static` initializers run exactly once per script load before any script logic executes.

### 3. Starting `PROGRAM` entry points

The engine starts functions registered in the `PROGRAM` table when matching events occur:

- In **map scripts**, the engine starts `map_start` (`100`) when the map loads, `talk_or_dungeon_start` (`1000`) when the player presses Confirm next to an NPC, or a trigger point's program number when the player uses a door or steps into a trigger zone.
- In **monster scripts**, the engine starts `init` (`100`) once when the monster spawns, `update` (`200`) to run the main AI loop, and reaction programs such as `damage` (`500`), `guard` (`700`), or `death` (`1000`) when combat events occur. When a reaction program finishes, the engine restarts `update` from the beginning.
- In **effect scripts**, the engine starts `play` (`0`) when the effect spawns and `finish` (`1`) if the creator stops the effect early.

### 4. Coroutine execution and `wait;` across frames

Each running program is a coroutine. On each frame, the VM executes instructions in the active program until it encounters a `wait;` statement, an `end;` statement, or a `return` from the entry function:

```text
Frame           | n            | n+1          | n+2          | n+3          | n+4
Map program     | ######       | ###          | ###          | #######      |
  (cutscene)    |       wait;  |    wait;     |    wait;     |     returns  |
Monster update  |       ====   |       ====   |       ====   |        ===   |    ====
  while (1) {…} |              |              |              |              |
```

Between script slices, the engine runs physics, collision detection, animations, and rendering.

Native functions never pause script execution. For example, `set_motion` starts an animation and returns immediately on the same frame. To wait for an animation, camera movement, or timer to finish, a script must loop with `wait;` and check the state each frame.

> **Important**
> **PAL frame rate (25 FPS)**
> The PAL version renders one game frame every two 50 Hz vertical blanks (`mgFrameRate = 2`), so one `wait;` equals `1/25` second (`40 ms`). The original scripts were written for the 30 FPS NTSC version. Several retail map libraries scale `wait_frames(n)` by `50/60` (`5/6`), monster libraries call `scale_frames_to_pal`, and screen fades also scale by `5/6`. In contrast, `modkit_wait(n)` and raw `wait;` loops do not scale frame counts.

### 5. Unloading

- **Map scripts** unload when the player warps to another map (`jump_to_map`, `go_to_interior`, `go_to_outside`, `load_dungeon_map`) or when `load_event_script` loads a replacement `.stb` file. All global variables in the unloaded map script are lost.
- **Monster AI scripts** unload when the monster dies and its `death` (`1000`) program returns, or when the player leaves the floor.
- **Effect scripts** unload immediately when their `play` (`0`) or `finish` (`1`) program returns.

### Where state lives

Different state storage mechanisms have different lifetimes:

| Storage kind | Lifetime | Access functions | Typical use |
| --- | --- | --- | --- |
| Local variable | Single function call | Standard C variable | Loop counters, temporary coordinates, native outputs. |
| Global or `static` local variable | Until the `.stb` script unloads | Standard C variable | Track state during one map visit (for example, how many times the player talked to an NPC during this visit). |
| Local flags and counters | Current event or dungeon floor | `get_local_flag` / `set_local_flag`, `get_local_counter` / `set_local_counter` | Track temporary floor state (for example, whether gate 2 on the current floor is open). |
| Monster variables (`0`–`7`) | Single monster instance on the current floor | `get_monster_var` / `set_monster_var` | Per-monster state across AI functions. |
| Monster variables (`8`–`135`) | Shared by all monsters on the current floor | `get_monster_var` / `set_monster_var` | Shared battle counters (for example, counting how many monsters have died in a scripted fight). Reset on floor change. |
| Save flags (`2,048` bits) | Saved in the player's save file | `get_flag` / `set_flag` (map), `get_dungeon_flag` / `set_dungeon_flag` (monster) | Permanent story progress (cutscenes viewed, doors unlocked). Named in `pal.h`. Flags `825`–`2046` are unused in the base game. |
| Save counters (`16-bit`) | Saved in the player's save file | `get_counter` / `set_counter` | Multi-step quest counters and persistent state machines. |
| Quest, NPC, and chapter state | Saved in the player's save file | `get_quest_state`, `set_npc_status`, `get_save_value` | Side quest status, party companions, and current chapter. |

### Three layers of callable functions

Scripts call three kinds of functions:

1. **Natives** (`place_monster`, `show_message`): Built-in engine functions declared with `extern ... NATIVE(n);` in `sdk/*.h`.
2. **SDK helpers** (`init_chara_actor`, `set_camera_view`, `spawn_effect_at`): `static inline` C wrapper functions defined in `sdk/helpers/*.h` that expand to fixed sequences of native calls.
3. **Runtime library functions** (`wait_frames`, `talk_to_npc`, `play_voice_message`, `chase_target_steps`): Shared script functions defined in `extracted/scripts/lib/` or `sdk/modkit/`. A script can call these functions only if it includes the corresponding `.c` library file.

## Directory layout

Directories in `extracted/scripts/` use the game's internal short codes followed by descriptive English names. CLI commands accept the short codes (such as `g01` or `e33a`).

```text
extracted/scripts/
├── map/                          One subdirectory per map
│   ├── g/  g01_sindain/ …              Georama towns (present day); interiors in subfolders: i07_dwarves_house/
│   ├── m/  m01_palm_brinks/ …          Palm Brinks variants across story points
│   ├── i/  i04_houses/h01_…/ …         Palm Brinks interiors (shops, station, houses, clinic)
│   ├── f/  f01_jurak_mall/ …           Future-era towns
│   ├── s/  s51_raybrandt_castle/ …     Story maps: stations, chambers, flashbacks, event copies of towns
│   └── d/  d01_underground_channel/
│       ├── d01_1.c … d01_5.c          Main dungeon script: runs every floor of d01
│       ├── d01_sfida_1.c …            Spheda (golf) bonus mode script
│       ├── b01_channel_reservoir/     Fixed special map (boss arena)
│       └── e01_channel_entrance/      Dungeon entrance hub (behaves like a town map)
├── dungeon/
│   ├── monster/   e33a_golem.c …     144 monster AI scripts (e = enemy, b = boss/story enemy)
│   ├── eff_script/ m_shockwave.c …   Visual effect scripts
│   ├── act_script/ f10a.c …          Monica's monster forms, the Ridepod, and unused player builds
│   ├── articles/, sphida/          Additional effect scripts packed inside model files
│   └── sys_script/                   Unused legacy per-dungeon scripts
├── mainchr/  c01_max.c, c02_monica.c   Max and Monica combat action scripts
├── event/                            Arg data tables and chest explosion effect
├── lib/                              Shared library code mirroring the directories above
└── include/disc.h                    Constants extracted from the disc (monsters, items, NPCs, motions, effects)
```

### Five language copies of every map script

PAL map scripts have five language variants:

- `_1.c`: English
- `_2.c`: French
- `_3.c`: German
- `_4.c`: Italian
- `_5.c`: Spanish

Files without a language suffix (such as `g01.c`) are unused Japanese scripts left on the disc. Shared map libraries follow the same convention (`base_town_villagers_v1.c` through `_v5.c`), where `g01_2.c` includes `_v2.c`.

> **Important**
> Edit all five language files (`_1` through `_5`) or place your logic in a single shared library included by all five files. Changes made only to `g01_1.c` take effect only when the console language is set to English.

### Other file naming conventions

- **`g01b_1.c`, `g01c_1.c`, `rush_g01_1.c`**: Alternate scripts for the same map. A running script can swap to an alternate script for a specific story sequence (`load_event_script("g01/g01b_1.stb", 501)`) and swap back afterward.
- **`i04h01a` through `i04h01f`**: Separate map builds of the same room (Max's room at different story stages). Each build is a distinct map ID.
- **`lib/map/g/g01_sindain/`**: Shared libraries used by `g01` scripts. Libraries in the root `lib/map/` folder are shared across many maps (such as villager dialogue, game-over handling, and menus). The header comment of each library file lists every script that includes it.
- **`dungeon/sys_script/`**: Legacy scripts for dungeons `d01`–`d05` without language suffixes. No retail file loads these scripts, and their chest code calls a no-op native. Active dungeon scripts live in `map/d/dNN/dNN_1.c`.

## Towns and story maps

A town script is a collection of event handlers. While the player walks around the map, the engine starts a program from the map script when an interaction occurs. Each handler runs as an **event** that pauses player movement until the handler returns.

### Standard town programs

| Program name and ID | Trigger condition | Typical behavior |
| --- | --- | --- |
| `map_start` (`100`) | Player enters the map without specifying another program ID | Position the player based on `get_prev_map_no`, show or hide objects and doors based on save flags, or start a pending story cutscene. |
| `talk_or_dungeon_start` (`1000`) | Player presses Confirm next to an NPC | Identify the target NPC and display the appropriate dialogue line. |
| `gate` (`300`) | Player steps into a time gate marker | Warp the player to the corresponding map in another era. |
| `save_point` (`400`) | Player interacts with a save point | Open the save menu. |
| `board_train` (`250`) | Player interacts with a train door on a station map | Prompt the player and warp to the selected destination. |
| Numbered (`200`, `500`, `501`, …) | Player uses a door, steps on a trigger point, or warps in from another map | Enter an interior building or play a scripted scene. |
| `story_N` (`10000 + 10 * event + part`) | Another script warps to this program ID | Play story cutscene `N` (for example, `story_50` is `10500`). |

### Talking to people

Most town scripts delegate program `1000` (`talk_or_dungeon_start`) to a shared library. The library queries the engine for the NPC ID and scene slot, then dispatches to a character-specific function:

`lib/map/base_town_villagers_v1.c` (abridged):

```c
void handle_villager_talk(void)
{
    int talk_chara;
    int talk_chara_id;
    int player;

    get_event_data(EVENT_DATA_CHARA_NO, &talk_chara);       // NPC ID (NPC_... in disc.h)
    get_event_data(EVENT_DATA_CHARA_SLOT, &talk_chara_id);  // Scene slot index
    player = get_player_chara();
    if (talk_chara <= 34) {
        talk_to_palm_brinks_villager(talk_chara_id, player);   // Branch per named villager
    } else {
        talk_villager_by_kind();
    }
}
```

Each named NPC has a function hierarchy that checks save flags, quest state, and chapter numbers using `if` statements:

`lib/map/base_town_villagers_v1.c` (abridged):

```c
int talk_gordon(int npc, int player)
{
    met_flag = get_flag(FLAG_GEOSTONE_PLAN_SCENE_DONE);
    if (!met_flag) {
        talk_gordon_early(npc, player);
        return 0;
    }
    quest_state = get_quest_state(QUEST_GET_STATUS, 0);
    if (quest_state == 0) {
        talk_gordon_quest_offer(npc, player);
        return 0;
    }
    ...
}
```

At the leaf of each branch, the script displays the dialogue line. In PAL town scripts, most NPC dialogue uses inline string literals followed by `talk_to_npc`, which opens a speech bubble and waits for the player to press Confirm:

```c
show_message(0, "...");             // Lay out the string (supports \n and <page> codes)
talk_to_npc(npc_slot, -1, 0, 0);    // -1 displays the prepared string, waits for input, and ends the event
```

Alternatively, `show_message(0, 500010)` displays line `500010` from the loaded event message file. The decompiler adds the message text as a comment beside the call so you can search for dialogue strings across the codebase. Yes/No prompts and selection menus reside inside a single message entry and return the selected index through `ask_question_at` or `ask_npc_question` (`0` for the first option, `-1` when canceled).

Georama towns (`map/g/`) use a separate shared library (`lib/map/g/town_npc_talk_and_shop_v1.c`) that follows the same dispatch structure.

### Doors and trigger points

Each map's `.map` file defines spatial trigger points for doors, step triggers, and interaction zones. Each trigger point specifies a program number to start. Run `dc2tk map extracted/scripts g01` to list all trigger points on `g01` (for example, `door to g01i07, runs program 200`) and the map placement or part that owns each point.

Scripts enable or disable trigger points at runtime with `set_func_point_enabled(part, "door", on)` (pass `""` for `part` to target the map's own points). This is how buildings remain locked until a story flag is set. Inside the triggered program, call `get_event_data(EVENT_DATA_ARG1, &x)` to read the trigger point's argument.

### Moving between maps

Map transition natives schedule a warp that executes when the current event finishes. The final argument selects the program ID to run on the destination map:

| Native call | Destination and behavior |
| --- | --- |
| `jump_to_map(-1, "s02")` or `jump_to_map(-1, "i03h01a", 500)` | Warp to another town or story map. Runs `map_start` (`100`) if no program ID is passed. |
| `go_to_interior(-1, "i01h01", program, entrance)` / `go_to_outside(-1, "g01", program)` | Transition into or out of an interior building. |
| `load_dungeon_map("s51"); load_event_script("s51/s51.stb", 500);` | Load a story map in dungeon mode (used for scripted combat scenes). |
| `go_to_dungeon(0, -1, talk_or_dungeon_start)` | Open a dungeon floor selection menu. |
| `go_to_edit_mode(map_id, program)` | Return from a dungeon map to a town or Georama map. |
| `load_event_script("g01/g01b_1.stb", 501)` | Keep the current map loaded, swap to a different `.stb` script, and start program `501`. |

### Story progression chains

The game has no central story script. Instead, map scripts check save flags and chain transitions from one map program to the next:

```text
g01i07 · map_start
    if flag 247 is set and
       time-gate-explained flag is not set:
      play the return-deal scene
      set flag 250
      jump_to_map(-1, "s47", 501)
        |
        v
s47 (Sindain event copy)
    PROGRAM(501, …)
      play the follow-up scene
      jump_to_map(-1, "s26", 501)
        |
        v
s26 · program 501
    play the next scene …

When the player later returns to g01i07, flag 250 prevents the scene from playing again.
```

See `map/g/g01_sindain/i07_dwarves_house/g01i07_1.c`, `map/s/s47_sindain_event/s47_1.c`, and their included libraries.

To trace story chains with the CLI:

- Run `dc2tk xref extracted/scripts map s47` to list every script that warps into `s47`.
- Run `dc2tk xref extracted/scripts event 501 --map s47` to list every call that starts program `501` on `s47`.

## Dungeons

Randomly generated dungeon floors do not have individual scripts. **One script controls an entire dungeon**. For example, `map/d/d01_underground_channel/d01_1.c` (and its language variants `_2.c`–`_5.c`) handles every floor of the Underground Channel, including floor generation, gates, key doors, treasure chests, floor-clear sequences, game-over handling, and transitions to story floors.

### Setting up a floor

`map/d/d01_underground_channel/d01_1.c` (abridged), program `floor_start`:

```c
int setup_dungeon_floor(void)
{
    ...
    init_event();
    fade_out(1);
    wait;
    dungeon_floor = get_dungeon_floor();
    if (dungeon_floor == 20) {                         // Story floor: transfer to s51
        skip_next_bgm_load();
        load_dungeon_map("s51");
        load_event_script("s51/s51.stb", 500);
        return 0;
    }
    ...
    load_dungeon_map("d01f01", "d01f01");             // Generate a random floor from d01 parts
    load_monster_data(MONSTER_SEWER_RAT);             // 1 argument: load this floor's monster table
    place_treasure_box(-1);                           // Spawn the floor's treasure chests
    place_monster(-1);                                // Spawn the floor's monsters at random positions
    ...
}
```

> **Note**
> **Random floor monster lists live in data tables, not scripts**
> When called with one argument, `load_monster_data` ignores the argument value and loads the current floor's monster list from the engine's floor configuration (`MONSTER_SEWER_RAT` is simply how the decompiler formats the constant `0`). Because random spawns are data-driven, `dc2tk xref extracted/scripts monster golem` will not list random floor spawns. To modify random floor spawns, edit the `data:floors` table (`dungeon/cfg_file/mos_place<dungeon>.cfg`) in `mod.json` (see [game data](../guide/data.md)). Scripted fights pass two arguments to `load_monster_data` to load specific monster kinds.

### Standard dungeon programs

| Program name and ID | Trigger condition | Retail behavior |
| --- | --- | --- |
| `talk_or_dungeon_start` (`1000`) | Player enters the dungeon | Open the floor selection menu, then run floor setup. |
| `floor_start` (`1010`) | A dungeon floor loads with a selected floor number | Generate the floor, chests, and monsters (as shown above). |
| `gate` (`300`) | Player interacts with the floor exit | Check for the Key Handle item, then advance to the next floor. |
| `key_door` (`348`) | Player interacts with a locked door | Prompt the player to use the required key item. |
| `treasure_chest` (`1100`) | Player presses Confirm at a chest | Call `open_treasure_chest()` to handle mimics, locks, traps, and item rewards. |
| `monster_talk` (`1150`) | Player presses Confirm near a talkable monster | Run the monster dialogue interaction. |
| `player_dead` (`1200`) | Player HP reaches `0` | Run the game-over handler or prompt to switch characters. |
| `monsters_cleared` (`1500`) | All monsters on the floor are defeated (runs once) | Play the victory pose, check medals, and unlock the floor exit. |
| `random_circle`, `geostone`, `spheda_*`, help programs | Player interacts with floor features | Handle mystery circles, geostones, Spheda golf, and tutorial prompts. |

### Event mode: why dungeon handlers must not block

When any dungeon program starts, the engine puts the dungeon into **event mode** until that program returns. While in event mode, the player cannot move or attack.

- If a dungeon program loops with `wait;`, the player remains frozen for the entire loop.
- If a dungeon program executes without calling `wait;` and returns immediately, it completes within a single frame and player control continues uninterrupted.

When a dungeon handler plays an animation (such as opening a chest or unlocking a door), retail scripts explicitly pause monsters and projectiles with `set_dungeon_pause(DNG_PAUSE_MONSTER_AI | ..., 1)` and unpause them before returning.

### Story fights and boss arenas

Fixed battles take place on special maps: `bNN_*/` boss folders inside a dungeon directory, or story maps loaded in dungeon mode (such as `s51`). These scripts load and place specific monster kinds, and the battle ends when **a monster's death script triggers a map program**:

```text
Map script (s51_1.c)                       Monster AI (b254a_griffon…)

program 500
    load_monster_data(523, 1)
    place_monster(523, x,y,z…) ×4
        |
        |  spawns 4 monsters running b254a
        +--------------------------------> each soldier · death (1000)
                                               shared monster var 127 += 1
                                               if (var 127 == 4) run_event_script(510)
        +--------------------------------------+
        |  requests map program 510
        v
program 510
    play victory cutscene, set flags,
    warp to next map
```

You can inspect this pattern in `map/s/s51_raybrandt_castle/s51_1.c` and `dungeon/monster/b254a_griffon_soldier.c`. Bosses use the same mechanism: the Halloween boss in `b01_channel_reservoir/` calls `run_event_script(520)` when defeated, which runs program `520` on the map script to unlock the exit and play the boss collapse cutscene.

`dungeon/monster/b254a_griffon_soldier.c` (abridged):

```c
// Plays the death motion and increments the shared kill counter in variable 127. When the fourth
// soldier dies, it starts map event 510.
void on_death(void)
{
    start_death();
    get_monster_var(127, &kill_count);
    kill_count = kill_count + 1;
    set_monster_var(127, kill_count);
    ...
    if (kill_count == 4) {
        run_event_script(510);
    }
    ...
}
```

Because the battle-end trigger is inside the monster's `death` program, replacing the monster kind in a scripted fight requires updating how the fight ends (see [Add a new enemy to a fight](tutorials/new-enemy.md)). Alternatively, a map script can define a `PROGRAM(monsters_cleared, ...)` handler, which the engine starts automatically when the last monster on the floor dies.

### Leaving a dungeon and game over

- Boss arenas return to the parent dungeon by calling `load_event_script("d01/d01.stb", talk_or_dungeon_start)`.
- Dungeons return to town maps by calling `go_to_edit_mode(map_id, program)`.
- When player HP reaches `0`, the map's `player_dead` (`1200`) program calls `run_game_over(...)` from `lib/map/base_game_over_vN.c`. This function offers a character switch if an ally is available, or displays the Game Over screen.

## Cutscenes

A cutscene is a function in a map script or map library (for example, `play_event_50` in `lib/map/s/s51_raybrandt_castle/gaspard_time_gate_scene.c`) called from a `PROGRAM` entry point. Every retail cutscene follows a five-phase structure:

```c
void play_some_scene(void)
{
    /* 1. Setup */
    set_flag(FLAG_..._SEEN, 1);                      // Set flag first so skipping still records progress
    init_event();                                    // Clear actor queues, sprites, and effects
    load_bgm(BGM_...);
    change_dir("event/e00/e50/");                    // Switch to the scene's disc directory
    load_message_file(0, MEMORY_STACK_EVENT, "e50_1.txt");
    load_inactive_chara(11, "e50_soldier.chr");      // Load character models into scene slots
    init_chara_actor(0, CHARA_PLAYER);               // Bind actor 0 to the player slot
    init_chara_actor(1, 11);                         // Bind actor 1 to scene slot 11
    change_dir("");

    /* 2. Open skip window */
    enable_scene_skip();                             // Allow the START button to skip from here

    /* 3. Timeline */
    cut_camera(x, y, z, look_x, look_y, look_z);     // Set initial camera shot
    fade_in(30);
    move_actor_to(1, x, y, z, 240);                  // Queue movement (returns immediately)
    move_camera_eased(..., 100, SCENE_SEQ_EASE_IN_OUT, 1.0);
    wait_object_script(1);                           // Wait until actor 1 finishes its queue
    play_voice_message(500010);                      // Play voiced line and wait for completion
    ...
    fade_out(30);
    wait_frames(30);

    /* 4. Close skip window */
    skip_end;                                        // Skipping jumps execution to this point
    disable_scene_skip();

    /* 5. Teardown */
    delete_chara(11);                                // Unload temporary models
    load_message_file(0, -1);                        // Unload the event message file
    activate_chara(CHARA_PLAYER);
    place_player_reset(...);
    reset_camera(CAMERA_RESET_ACTIVE);
    fade_in(30);
    finish_event();
}
```

1. **Setup**: Set the story completion flag first, call `init_event()` to reset the event state, load music, load the message file and character models into scene slots, and bind actors to those slots.
2. **Open the skip window**: Call `enable_scene_skip()` so pressing START skips the timeline.
3. **Timeline**: Queue camera movements, actor movements, and animations, and poll until they complete using helper functions (`wait_object_script`, `wait_camera_script`, `wait_frames`).
4. **`skip_end;`**: When the player presses START to skip a cutscene, the engine suppresses native calls until execution reaches `skip_end;`. Never place `set_flag`, map warps, or resource cleanup between `enable_scene_skip()` and `skip_end;`.
5. **Teardown**: Delete temporary characters, unload the message file, restore the player character and gameplay camera, and call `finish_event()`.

### Slots, handles, and actors

- **Scene slots** hold loaded character models:
  - Slot `0` (`CHARA_PLAYER`) is the active player character.
  - Slots `1`–`5` hold the player's equipped weapons.
  - Slots `8`–`31` hold town villagers or scene-specific models loaded with `load_scene_chara`, `load_inactive_chara`, or `copy_chara`.
- **Handles** (`0`–`31`) reference either a scene character slot or a map geometry part (bound with `bind_handle_map_part`, such as a door or prop).
- **Actors** (`0`–`31`) are command queues that drive handles across frames. Calling `init_chara_actor(a, slot)` binds handle `a` and actor `a` to `slot`. Calling `*_actor_*` functions appends commands to the actor's queue, and `check_actor_busy(a)` returns non-zero while commands are still running.

### Camera control

Camera coordinates specify the camera eye position `(x, y, z)` followed by the look-at target `(look_x, look_y, look_z)` in world units (`y` is up):

- `cut_camera`: Move the camera immediately and clear queued camera movements.
- `move_camera_to(..., frames)`: Interpolate the camera linearly over `frames`.
- `move_camera_eased(..., frames, ease, rate)`: Interpolate the camera with easing.
- `attach_camera_to_actor`: Lock the camera to follow an actor.
- `reset_camera(CAMERA_RESET_ACTIVE)`: Return camera control to the player after a scene.

### Text and voice playback

Cutscene dialogue uses integer message IDs from the loaded event message file (`id = event * 10000 + n`; for example, `500010` is the first line of event `50`). Voiced lines use the same integer ID for the audio stream. The runtime library function `play_voice_message(id)` starts the voice audio, displays the subtitle, animates character lip-sync, and waits for completion. Unvoiced town scenes pass string literals directly to `show_message(0, "...")`.

## Monsters

### Monster kinds, models, and AI scripts

Each monster in the game is defined by a **monster kind** row in the engine's monster table. Each row specifies the monster's display name, 3D model, combat stats, and AI script name. All monster kinds from your disc are listed in `extracted/scripts/include/disc.h`:

```c
MONSTER_GOLEM = 168,                // model e33a, AI e33a
MONSTER_RAVE_GOLEM = 169,           // model e33b, AI e33a
...
MONSTER_GRIFFON_SOLDIER = 37,       // model e254a, AI e254a
MONSTER_GRIFFON_SOLDIER_523 = 523,  // model e254a, AI b254a
```

Key relationships between monster kinds and AI scripts:

- **File names match the AI code**: `e33a_golem.c` compiles to AI script `e33a`. Files starting with `e` are standard enemies; files starting with `b` are bosses or story-battle enemies. Suffixes `a`–`d` denote the four color variants of a monster family, which usually share the `a` AI script.
- **One AI script often controls multiple monster kinds**: Editing `e33a_golem.c` affects all four Golem variants (`168`–`171`), and `element.c` controls 20 elemental kinds. Call `get_monster_kind()` inside the script to branch for a specific variant.
- **Story battles use dedicated monster kinds**: Monster kind `523` uses the same 3D model (`e254a`) as a standard Griffon Soldier (`37`), but runs `b254a_griffon_soldier.c` so its death handler can count kills for the castle prologue fight without affecting standard Griffon Soldiers.
- **Unused AI scripts**: Approximately 30 files in `dungeon/monster/` are unused by any monster kind in the table (for example, `e111a`–`e115a`, whose monster kinds instead run `eJema`). Always run `dc2tk xref extracted/scripts monster NAME` to verify which monster kinds use an AI script before editing it.

### Inside a monster script

| Program name and ID | When the engine starts it |
| --- | --- |
| `init` (`100`) | Once when the monster spawns. Registers attack hitboxes, preloads visual effects, and initializes AI state. |
| `update` (`200`) | Runs the main AI loop every frame. If interrupted by a reaction program, the engine restarts `update` from the top after the reaction program finishes. |
| `damage` (`500`), `knockdown` (`600`), `stun` (`1400`), `lifted`, `land` | When the monster takes a heavy hit, knockdown, stun, or throw. |
| `guard` (`700`), `dodge_melee` (`800`), `dodge_shot` (`900`) | When the engine's guard or dodge probability check succeeds. |
| `death` (`1000`) | When the monster's HP reaches `0`. |

The Golem's `on_init` function demonstrates how monsters register attacks:

`dungeon/monster/e33a_golem.c` (abridged):

```c
void on_init(void)
{
    //             damage setting        radius  animation           active from..to  model point
    add_attack_hit(HIT_KNOCKBACK_ATTACK, 6.5,    MOTION_ATTACK_1_2,  0.3, 0.37,       0);
    add_attack_hit(HIT_BEING_BLOWN_AWAY, 6.5,    MOTION_ATTACK_1_2,  0.5, 0.6,        2);
    load_effect(EFFECT_VANGUARD_SHOCKWAVE);
    ...
    init_monster_ai();     // Toolkit helper: sets aggro range to 500 and records spawn coordinates
}
```

`add_attack_hit` attaches a spherical damage hitbox to a bone point on the monster's 3D model. The hitbox is active automatically whenever the specified animation plays between the start and end progress fractions (`0.0` to `1.0`). Once registered in `on_init`, performing the attack only requires playing the animation:

`dungeon/monster/e33a_golem.c`:

```c
void chase_and_swing_at_target(void)
{
    loop_motion(MOTION_WALK);
    chase_target_steps(1.25, 3.0, 62.0 + 4.0, 90);
    turn_to_target_wait(3.0);
    set_motion(MOTION_ATTACK_1_2, -1, CHARA_MOTION_HOLD);   // Activates the registered hitboxes
    wait_motion_end_update();
    loop_motion(MOTION_STAND_2);
    wait_frames_update(6);
}
```

On each iteration of `on_update`, the AI selects an action based on rage status, aggro state, and distance to the player:

`dungeon/monster/e33a_golem.c` (abridged):

```c
void choose_behavior_by_aggro(void)
{
    rage_hits_left = get_rage_hits_left();
    if (rage_hits_left > 0) {
        target_dist = get_target_dist();
        if (!is_aggro()) {
            if (target_dist < 280.0 + 50.0) {
                fight_target_by_range();       // Choose close, mid, or far attack
            } else {
                choose_idle_wander_action();
            }
        } else {
            ...                                // Aggro active: raise guard and approach
        }
    } else {
        set_defense_rate(1.5);
        fight_while_enraged();
    }
}
```

### The monster AI toolkit

Most monster scripts include `lib/dungeon/monster/monster_ai_full_toolkit.c` (or a smaller variant). This library provides standard AI routines:

| Category | Toolkit functions |
| --- | --- |
| Frame waiting (use instead of raw `wait;` to keep aggro timers updated) | `wait_frame_update`, `wait_frames_update`, `wait_motion_end_update`, `wait_motion_time(fraction, …)` |
| Aggro management | `init_monster_ai`, `is_aggro`, `set_aggro`, `get_aggro_range` |
| Turning | `turn_to_target_wait`, `turn_to_angle`, `track_target_until_motion_end` |
| Movement | `chase_target_steps`, `walk_to_target`, `walk_around_target`, `walk_in_direction`, `find_clear_direction` |
| Target detection | `is_target_in_view`, `can_detect_target`, `wait_target_in_view` |
| Interrupt control | `lock_interrupts_stop`, `unlock_interrupts`, `stop_all_actions` |

Because 36 monster scripts include `monster_ai_full_toolkit.c`, editing that file directly modifies all 36 monsters. Use `dc2tk fork` to create a dedicated copy of the library when modifying toolkit behavior for a single monster (see [Change enemy behavior](tutorials/enemy-behaviour.md)).

### Monster combat tuning natives

- `set_defense_rate(rate)`, `set_guard_rate(-1, rate)`, `set_dodge_rate(-1, rate)`: Multiply the monster table's defense, guard, or dodge rates for this instance (`-1` targets the calling monster).
- `set_mask_flag(MASK_NO_GUARD, on)`, `set_mask_flag(MASK_NO_DODGE, on)`: Disable or enable the engine's automatic guard and dodge rolls. Scripts disable both during scripted attack sequences.
- `get_rage_hits_left()`: Return the remaining hits before the monster enters its enraged state (`-1` for bosses).
- `get_life(&fraction)` (`0.0` to `1.0`) and `set_life(fraction)`: Read or set the monster's current HP fraction. Bosses use these to trigger phase transitions.

### Bosses

Boss scripts (such as `b08a_emperor_griffon.c` and `b13a_arch_demon.c`) are 2,500 to 3,300 lines long and use additional features:

- Custom boss camera mode (`set_camera_mode(CAMERA_MODE_BOSS)`).
- Summoning helper monsters (`create_monster`).
- HP threshold checks inside every wait loop to trigger mid-fight cutscenes via `run_event_script`.
- A `death` program that starts the post-fight map program.

## The player

`mainchr/c01_max.c` and `mainchr/c02_monica.c` control Max and Monica in combat. They use the same program structure as monster scripts (`init` registers weapon hitboxes; `update` runs every frame) but read controller input to transition between states stored in a 16-bit `saved_value`:

| State value (Max) | Action |
| --- | --- |
| `0` | Idle/movement: move with the analog stick and check button presses for the next action. |
| `100`–`198` | Melee wrench/hammer combo states (one state value per combo hit). |
| `200` / `300` | Mid-range lunge attack / downward strike on a knocked-down enemy. |
| `800` / `820` / `840` | Ranged gun attacks: single shot, machine gun, and slow beam shot. |
| `900`, `1000`, `1100` | Guard and catch, item throw, and object lift/throw. |
| `1200` | Charge attack. |
| `1300` | Backflip evade. |

During each melee combo hit, the script reads the animation progress (`0.0` to `1.0`) and opens an input window to chain the next combo hit:

`mainchr/c01_max.c` (abridged):

```c
if (saved_value == 101) {
    get_motion_progress(&progress);
    ...
    if (progress >= 0.33 && progress <= 0.45) {          // Combo chain input window
        if (get_attack_presses()) {
            start_combo_hit(MOTION_5_HIT_COMBO_2, saved_value + 1);
        }
    }
    ...
}
```

`c02_monica.c` follows the same state layout and uses state `800` for magic armband spells. In `dungeon/act_script/`, `f*a.c` files control Monica's Monster Badge transformations (`f` followed by the monster model code), and `robo_ridepod.c` controls the Ridepod. Note that `c01win.c` and `f204a.c` are unused early builds compiled against older native tables; do not use them.

## Effects

An effect script (`dungeon/eff_script/*.c`, using `<effect.h>`) controls a visual effect instance:

- The engine starts `play` (`0`) when the effect spawns. `play` updates the effect's sprites, models, and hitboxes each frame, and the engine deletes the effect instance as soon as `play` returns.
- The engine starts `finish` (`1`) if the parent script stops the effect early.
- The parent script passes parameters to the effect through two 3D work vectors (`set_work_vector1`, `set_work_vector2`) and eight scalar values (`set_effect_value`).
- Effects can create their own attack hitboxes (`create_attack`, `set_attack_shape`, `set_attack_damage`).

Below is the call flow for the Golem's ground-slam shockwave:

```text
e33a_golem.c
    on_init:
      load_effect(EFFECT_VANGUARD_SHOCKWAVE)
    slam attack (at animation progress 0.64):
      get_frame_pos("dcol0", …)
      spawn_effect_at(EFFECT_VANGUARD_SHOCKWAVE, x, y, z)
      set_effect_value(-1, 0, attack)
        |
        v
Engine effect table
    Maps "ヴァンガード衝撃波"
    to script m_shockwave
        |
        v
eff_script/m_shockwave.c · play (0)
    get_effect_value(0, &damage)
    get_work_vector1(…)  // Spawn coordinates
    play expanding ring animation
    create_attack(HIT_WEAK_KNOCKBACK)
    set_attack_shape(…, 30.0)
    set_attack_damage(damage)
    … delete_attack, return
```

The `EFFECT_*` constants in `disc.h` provide readable English names for the Japanese effect strings used by the engine's effect lookup table. Because multiple monsters (including the Golem, Vanguard, Mammo, and Elfas) spawn `EFFECT_VANGUARD_SHOCKWAVE`, modifying `m_shockwave.c` affects all of them.

Map scripts (`load_effect_script` and `create_effect`) and player scripts (such as `EFFECT_MAX_CHARGE_ATTACK` in `c01_max.c`) also spawn effects. Many effect implementations live in `lib/dungeon/*.c` because the same effect bytecode is compiled into both `dungeon/eff_script/` and model archives in `dungeon/articles/eff01.chr.d/`.

## Mod development workflow

Do not edit files directly inside `extracted/scripts/`. Instead, place all changes inside a **mod** directory under `mods/`. This keeps your extracted source tree clean and lets `dc2tk build` combine multiple mods.

1. **Locate the target script or function**: Use the [lookup commands](#quick-reference), file header comments, and an editor configured with `clangd` (see [editor.md](editor.md)).
2. **Create a mod and copy the target file**:

   ```text
   dc2tk mod new extracted/scripts my-change map/g/g01_sindain/g01_1.c
   ```

   Edit the copied file under `mods/my-change/files/`.
3. **Or write a non-destructive addition or patch**:
   - Use `"additions"` in `mod.json` to include a library, register a new program, or wrap an existing program (`"before"` / `"after"`) across all five language files at once (see [Add a cutscene to a town](tutorials/cutscene.md)).
   - Write a **patch** `.c` file with `#pragma target map g01` (or `script`, `monster`, `library`) containing only the functions you want to add or `#pragma override`. Run `dc2tk mod override extracted/scripts my-change FUNCTION --map g01` to generate a patch, or `dc2tk mod slim` to convert copied files into patches (see [mods](../guide/mods.md)).
4. **Build the ISO and launch the emulator**:

   ```text
   dc2tk build extracted/scripts dc2.iso -o modded.iso --mod my-change --play
   ```

   `dc2tk build` recompiles only the modified scripts and patches the ISO in seconds.
5. **Test a specific map or program directly**: Pass `--start-at s51:story_50` (or set `"start_at"` in `mod.json`) to make **New Game** warp straight to a dungeon-mode map and program, or pass `--start-at g01` to start in a town map at `map_start`. Note that `--start-at` starts with a fresh save file where all story flags are `0`. To test scenes that depend on mid-game story flags, load a PCSX2 save state instead.

> **Warning**
> **Standalone script files vs. libraries**
> `dc2tk build` replaces `.stb` files that already exist on the disc. If you create a brand-new standalone script file (such as `dungeon/monster/e999a.c`), `dc2tk build` skips it with a `"not on the disc"` warning because the engine has no table entry to load it.
> - **New library files** under `lib/mods/NAME/` work because existing scripts `#include` them at compile time.
> - **New map scripts** work only when registered under `"add"` in `mod.json` for a custom map (see [maps](../guide/maps.md)).

## Checklist before testing

Verify these items when writing or reviewing script changes:

- **All five languages**: Did you apply map or dialogue changes to all five language files (`_1` to `_5` or `_v1` to `_v5`), or place the shared logic in a single library or `#pragma target map` patch?
- **Shared library impact**: If you edited a file in `lib/`, check the `dc2tk build` output to see which other scripts include it. Use `dc2tk fork` if the change should apply to only one script.
- **Shared or unused monster AI**: Does your monster AI script control multiple monster kinds? Did you verify with `dc2tk xref` that the AI file is actually used by a monster kind?
- **Scripted battle endings**: If you replaced the monsters in a scripted fight, does the fight still have a completion trigger (either `run_event_script` in the monster's `death` program or a `monsters_cleared` program on the map)?
- **Cutscene skip window**: Ensure that no `set_flag`, map warp, or character cleanup calls appear between `enable_scene_skip()` and `skip_end;`.
- **Natives do not block**: Every wait condition requires a loop containing `wait;`.
- **Dungeon event mode**: Remember that any `wait;` inside a dungeon map program freezes player movement until the program returns.
- **PAL frame scaling**: Retail `wait_frames(n)` scales `n` by `50/60`, whereas `modkit_wait(n)` and raw `wait;` loops count exact 25 FPS PAL frames.
- **Exact Japanese asset strings**: Constants such as `MOTION_*`, `HIT_*`, and `EFFECT_*` expand to exact Shift-JIS strings stored inside model and engine tables. For example, `MOTION_ATTACK_1` and `MOTION_ATTACK_1_2` use different character widths (half-width vs. full-width digits). Always use the constants already present in that model's script; a mismatched string fails silently.
- **Preload effects**: Call `load_effect` in `on_init` before spawning an effect in a monster script.
- **Save flag allocation**: Allocate custom save flags from `825`–`2046` (or use `NEW_FLAG`, which assigns IDs starting at `1024`). Flag `2047` is reserved for `--start-at`. Clear temporary combat flags when the fight ends and in `player_dead`.
- **Message window 0 in dungeons**: In dungeon mode, message window `0` remains visible after an event finishes unless explicitly closed with `close_message(0)`.

## Quick reference

### Lookup commands

| Task | Command |
| --- | --- |
| List all maps and whether they run in town or dungeon mode | `dc2tk maps extracted/scripts` |
| Inspect one map's programs, triggers, free program IDs, and spawn positions | `dc2tk map extracted/scripts g01` |
| Find every script that warps into a specific map | `dc2tk xref extracted/scripts map s47` |
| Find every call that starts a specific map program | `dc2tk xref extracted/scripts event 510 --map s51` |
| Inspect a monster's model, AI script, shared kinds, and scripted placements | `dc2tk xref extracted/scripts monster golem` |
| List used and free save flags, or find where a flag is read and set | `dc2tk flags extracted/scripts [--flag N]` |
| List modified files in `extracted/scripts` | `dc2tk changes extracted/scripts` |

### Header and asset locations

| Information | Location |
| --- | --- |
| Native function signatures and documentation | `sdk/event.h`, `sdk/monster.h`, `sdk/action.h`, `sdk/effect.h`, `sdk/arg.h`, and [api/README.md](api/README.md) |
| Standard program names and numbers | `enum program` in each SDK header |
| Monster kinds, item IDs, NPC IDs, motion names, effect names, and hit types | `extracted/scripts/include/disc.h` |
| Save flag names, BGM tracks, and sound sets | `definitions/pal/include/pal.h` |
| Mod helper functions for text, input, and timers | `sdk/modkit/` and [api/modkit.md](api/modkit.md) |
| Monster stats, floor spawn lists, chest contents, items, and shops | `dc2tk mod asset dc2.iso NAME data:monsters` (`data:floors`, `data:chests`, `data:items`, …); see [guide/data.md](../guide/data.md) |

### Further reading

- [internals/running.md](../internals/running.md): Engine execution rules for event mode, message windows, and monster-triggered map events.
- [tutorials/README.md](tutorials/README.md): Step-by-step tutorials for common scripting tasks.
- [tutorials/captions.md](tutorials/captions.md): How to display timed captions during combat using monster frame ticks.
- [guide/commands.md](../guide/commands.md): Complete CLI command reference.
- [guide/mods.md](../guide/mods.md): Guide to creating mods, patches, `mod.json` additions, and VM tests.
- [examples/calculator/calculator.c](examples/calculator/calculator.c): Complete calculator mod example.
- [examples/spinning-cult/README.md](examples/spinning-cult/README.md): Complete story mod example.
- [language.md](language.md): Detailed reference for the script C dialect and VM semantics.
- [c-standard.md](c-standard.md): C89 conformance details and compiler lowering rules.
- [sdk.md](sdk.md): SDK headers, value kinds, and `modkit` reference.
