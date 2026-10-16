# Writing script code

This section is about writing script code: reading the game's own scripts in
`extracted/scripts/` and changing or adding to them in a mod. This page is a tour of the
game's scripts. The section's other pages:

- [language.md](language.md): the script language
- [c-standard.md](c-standard.md): standard C
- [sdk.md](sdk.md): the SDK headers and modkit
- [editor.md](editor.md): editor setup
- [tutorials/](tutorials/README.md): task walkthroughs
- [api/](api/README.md): every native, by header: [event.md](api/event.md),
  [monster.md](api/monster.md), [action.md](api/action.md), [effect.md](api/effect.md),
  [arg.md](api/arg.md), [modkit.md](api/modkit.md)
- [examples/calculator/calculator.c](examples/calculator/calculator.c) and
  [examples/spinning-cult/README.md](examples/spinning-cult/README.md): the two example mods

Welcome. You've been handed `extracted/scripts/`: about 2,500 C files that make up the gameplay
code of Dark Chronicle (Dark Cloud 2). This page walks through that codebase the way you'd
learn any game's scripting layer: what each system is, where its code lives, how the pieces
call each other, and how to make the changes you'll actually be asked for.

> **About the code on this page**
> The repository never stores the game's own code or text, so excerpts from
> `extracted/scripts/` here are short and abridged (`...` marks cuts) with dialogue left out.
> Each excerpt names its file so you can read the whole thing in your own tree. Code in the task
> walkthroughs is new code written for these pages.

## The big picture

The game is split in two. The **engine** (the PS2 executable) draws the world, runs physics and
collision, plays animations and sound, and owns the data tables. The **scripts** decide what
happens: every cutscene, conversation, door, chest, monster brain, the player's sword combo, and
how each spell or explosion moves. Scripts call into the engine through about 1,100 engine
functions called *natives*, and the engine calls into scripts by starting their *programs* when
something happens.

```text
Engine (game executable: scripts can't change it)
    rendering · collision · animation · sound · monster stat table · floor monster lists ·
    chest contents · effect name table

        |  starts programs:                       ^  natives:
        |  map_start, update, death, play …       |  place_monster, set_motion …
        v                                         |

Scripts: Map scripts · Monster AI · Player actions · Effects · Arg tables
```

| Script | Folder and header | What it is |
| --- | --- | --- |
| Map scripts | `map/`, `event.h` | towns, dungeons, cutscenes, story |
| Monster AI | `dungeon/monster/` | one script runs per live monster |
| Player actions | `mainchr/`, `action.h` | Max, Monica, her forms, the Ridepod |
| Effects | `dungeon/eff_script/` | sparks, beams, shockwaves, hits |
| Arg tables | `event/`, `arg.h` | data records a few scenes load |

Five kinds of script, each with its own header in `sdk/` and its own set of natives. The
engine-side tables (listed under the engine above) are out of a script's reach: scripts can read
and trigger them but not change them. Keep that line in mind; it decides what a script can and
cannot do. A mod can still change several of those tables as data, beside its scripts: the
monster stats, the floors' monster lists and the chests' contents are `data:` tables in
`mod.json` (see the table below).

### What lives where

| Behaviour | Lives in | Can you change it? |
| --- | --- | --- |
| What a scene shows, what an NPC says, where a door leads, when the story advances | map scripts | Yes |
| How a monster moves, picks attacks, reacts, dies; boss phases | monster AI scripts | Yes |
| Combo timing, hit windows, charge attack, guns, magic | player action scripts | Yes |
| How an effect looks and moves, the damage a shockwave does | effect scripts | Yes |
| Which monster kinds exist | engine monster table | No: a mod can't add a kind |
| A kind's model, HP and stats, drops, which AI script it runs | engine monster table | Not from a script; yes from a mod's `data:monsters` table, patched into the executable |
| Which monsters roam a random dungeon floor, what a floor chest contains | floor data the engine reads | Not from a script (but scripts can add monsters and chests on top); yes from a mod's `data:floors` and `data:chests` tables |
| Models, animations, textures, sound banks, music, the text of message files | data files on the disc | Scripts pick from what exists; a mod replaces or adds them by asset id in `mod.json` |

## Rules every script follows

All five kinds share one shape. Learn it once and every file reads the same way.

`dungeon/monster/e33a_golem.c` (abridged): the Golem's brain

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

1. **Every file and function has a comment** saying what it does. They were written by this
   project (the shipped bytecode had none). Read them first.
2. **The first header says which kind of script this is** (`monster.h`, `event.h`, `action.h`,
   `effect.h`, `arg.h`) and declares every native it can call. `pal.h` names the save flags,
   music and sounds; `disc.h` names monster kinds, items, NPCs, animations and effects.
3. **Globals** live as long as the script stays loaded and start at 0 each load. Note that the
   Golem declares `spawn_x` only because a library it includes writes to it.
4. **The `PROGRAM` table is the script's list of entry points.** The engine starts `init` when
   the monster spawns, `update` every frame after, `damage` when it's hit hard, and so on.
   Nothing else in the file runs unless one of these calls it.
5. **`#include "lib/..."` pastes shared code in at that spot**, exactly as the original source
   did. Most of what a script does is in its libraries. Libraries may use the including
   script's globals and functions.
6. **`wait;` is how time passes.** A program runs until it waits, gives up the rest of the
   frame, and carries on from the same spot next frame. The helper here wraps a `wait;`.

### Programs are coroutines

Each running program executes a slice every frame until its next `wait;`:

```text
frame           | n            | n+1          | n+2          | n+3          | n+4
map program     | ######       | ###          | ###          | #######      |
  a cutscene    |       wait;  |    wait;     |    wait;     |     returns  |
monster update  |       ====   |       ====   |       ====   |        ===   |    ====
  while (1) {…} |              |              |              |              |
```

The engine does everything else (physics, animation, drawing) in the gaps.

So "wait two seconds" is a loop of `wait;`, and "wait for the walk to finish" is a loop that
checks each frame. Natives never wait: `set_motion` starts an animation and returns at once.

> **25 frames a second**
> The PAL game draws a frame every other 50 Hz vertical blank (`mgFrameRate` 2), so `wait;` is
> 1/25 s; the scripts were written for the NTSC game's 30. Some retail map libraries'
> `wait_frames(n)` quietly scale `n` by 50/60 (5/6), monster libraries use
> `scale_frames_to_pal`, and the fades take 5/6 of their frames too. The mod library's
> `modkit_wait(n)` does *not* scale. When numbers feel off by 20%, this is why.

### Where state lives

| Kind | Lifetime | Access | Used for |
| --- | --- | --- | --- |
| Global variable | until the script unloads (leaving the map) | plain C | "talked twice this visit" |
| Save flag (2,048 bits) | saved with the game | `get_flag`/`set_flag` (map), `get_dungeon_flag`/`set_dungeon_flag` (monster) | story progress: "scene seen", "door unlocked". Named in `pal.h`. 825–2046 are free. |
| Save counter (16-bit) | saved | `get_counter`/`set_counter` | small saved state machines (an errand's step) |
| Quest / NPC / progress | saved | `get_quest_state`, `set_npc_status`, `get_save_value` | side quests, party joiners, chapter |
| Local flags and counters | the current event or floor | `get_local_flag`, `get_local_counter` | "gate 2 on this floor is unlocked" |
| Monster variables | the floor | `get_monster_var`/`set_monster_var` | 0–7 belong to one monster; 8–135 are shared by all monsters on the floor (kill counters) |

### Three layers of "function you can call"

- **Natives** (`place_monster`, `show_message`): engine functions, declared and documented in
  `sdk/*.h`. Hover them in your editor.
- **SDK helpers** (`init_chara_actor`, `set_camera_view`, `spawn_effect_at`): small inline
  wrappers in `sdk/helpers/`.
- **Runtime library functions** (`wait_frames`, `talk_to_npc`, `play_voice_message`,
  `chase_target_steps`): game code in `lib/`. These only exist in scripts that include the
  library, so code you move to a new file may need to include it too.

## Where everything lives

Folders keep the game's own codes, with an English name after them. Commands take the short code
(`g01`, `e33a`).

```text
extracted/scripts/
├── map/                          every place in the game, one script per map
│   ├── g/  g01_sindain/ …              Georama towns (present day); interiors in subfolders: i07_dwarves_house/
│   ├── m/  m01_palm_brinks/ …          Palm Brinks at different points in the story
│   ├── i/  i04_houses/h01_…/ …         Palm Brinks interiors (shops, station, houses, clinic)
│   ├── f/  f01_jurak_mall/ …           future towns
│   ├── s/  s51_raybrandt_castle/ …     story maps: stations, chambers, flashbacks, event copies of towns
│   └── d/  d01_underground_channel/
│       ├── d01_1.c … d01_5.c          THE dungeon script: runs every floor of d01
│       ├── d01_sfida_1.c …            Spheda (golf) bonus mode
│       ├── b01_channel_reservoir/     fixed special map: usually a boss arena
│       └── e01_channel_entrance/      entrance hub, runs like a town
├── dungeon/
│   ├── monster/   e33a_golem.c …     144 monster AIs (e = enemy, b = boss/story)
│   ├── eff_script/ m_shockwave.c …   effects
│   ├── act_script/ f10a.c …          Monica's monster forms, the Ridepod, old player builds
│   ├── articles/, sphida/          more effects, packed inside model files
│   └── sys_script/                   older per-dungeon scripts (see below)
├── mainchr/  c01_max.c, c02_monica.c   the two heroes in combat
├── event/                            arg data tables, the chest explosion effect
├── lib/                              shared code, mirroring the folders above
└── include/disc.h                    names from your disc: monsters, items, NPCs, motions, effects
```

### Five copies of every map script

Map scripts come per language: `g01_1.c` is English, `_2` to `_5` are French, German, Italian
and Spanish, and `g01.c` with no number is the original Japanese, which the PAL game never runs.
The game loads the one matching the console's language. The same goes for libraries:
`base_town_villagers_v1.c` to `_v5.c` are the same code with each language's text, and
`g01_2.c` includes `_v2`.

> **Change all five, or test in English**
> An edit to `g01_1.c` only shows on an English console. For anything you ship, make the change
> in all five files, or put it in one library that all five include.

### Other file-name patterns

- **`g01b_1.c`, `g01c_1.c`, `rush_g01_1.c`**: alternate scripts for the *same* map. A running
  script swaps one in for a story beat (`load_event_script("g01/g01b_1.stb", 501)`) and swaps
  back after.
- **`i04h01a` … `i04h01f`**: separate builds of one room (Max's room at different story points),
  each a real map.
- **`lib/map/g/g01_sindain/`**: code shared by the g01 scripts. `lib/map/` root: code shared
  across many maps (the villager talk runtime, game over, menus). Each library's top comment
  lists who uses it.
- **`dungeon/sys_script/`**: one script for each of d01–d05, without language variants. Nothing
  in the tree loads them and their chest code calls a native that does nothing in the retail
  game, so they look like an earlier generation of the dungeon scripts. The code that runs is
  `map/d/dNN/dNN_1.c`.

## Towns and story maps

A town script is a set of handlers. The player walks around freely; the engine calls the script
when they enter, talk to someone, use a door, stand in a time gate or save point. Each handler
runs as an *event*: player control pauses until it returns.

### The programs a town answers

| Program | Runs when | Typical job |
| --- | --- | --- |
| `map_start` (100) | the player arrives and no other program was named | place the player by where they came from (`get_prev_map_no`), show or hide objects and doors by flags, auto-play a pending story scene |
| `talk_or_dungeon_start` (1000) | confirm pressed next to a character | work out who it was and pick a line |
| `gate` (300) | standing in the time gate marker | warp to the other era |
| `save_point` (400) | at the save point | open the menu |
| `board_train` (250) | train door on station maps | ask, then jump |
| numbered (200, 500, 501 …) | a door or trigger point, or another map's warp | enter a house, play a scene |
| `story_N` (10000 + 10 × event + part) | a warp that names it | story scene N: `story_50` is 10500 |

### Talking to people

Most towns hand the whole of program 1000 to a shared library. The library asks the engine who
was talked to and dispatches:

`lib/map/base_town_villagers_v1.c` (abridged)

```c
void handle_villager_talk(void)
{
    int talk_chara;
    int talk_chara_id;
    int player;

    get_event_data(EVENT_DATA_CHARA_NO, &talk_chara);       // who: an NPC id (NPC_... in disc.h)
    get_event_data(EVENT_DATA_CHARA_SLOT, &talk_chara_id);  // which scene slot they're in
    player = get_player_chara();
    if (talk_chara <= 34) {
        talk_to_palm_brinks_villager(talk_chara_id, player);   // one branch per named villager
    } else {
        talk_villager_by_kind();
    }
}
```

Each named villager has a function family that picks the right conversation from story state. It
is plain `if` chains on flags, quest state and chapter:

`lib/map/base_town_villagers_v1.c` (abridged)

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

At the bottom of each branch the line itself is shown. In the PAL scripts most town talk is
literal text in the script, then a library call that opens a speech bubble from the speaker and
waits for the button:

```c
show_message(0, "...");             // the line, as text (with \n and <page> codes)
talk_to_npc(npc_slot, -1, 0, 0);    // -1 = show what's laid out; bubble, wait, finish_event
```

The other form is a number: `show_message(0, 500010)` shows line 500010 of the message file the
scene loaded. The decompiler puts that line's text in a comment beside the call so you can search
for it. Yes/no and menu choices are lines of one message, read back with `ask_question_at` or
`ask_npc_question`: 0 for the first option, -1 for cancel.

Georama towns (`g/`) use a different library, `lib/map/g/town_npc_talk_and_shop_v1.c`, with the
same idea.

### Doors and trigger points

Trigger points are drawn into each map's `.map` file: a door, a spot to step on, a spot to press
the button at. Each one names a program number. `dc2tk map extracted/scripts g01` lists them all,
for example "door to g01i07, runs program 200", with the part each belongs to: the placement that
carries it (a point of a template is listed once for every placement of it) or the map's own.
Scripts switch points on and off with `set_func_point_enabled(part, "door", on)` (part `""` for
the map's own points), which is how a house stays locked until a story flag is set. Inside the
program, `get_event_data(EVENT_DATA_ARG1, &x)` reads the point's argument.

### Moving between maps

Every warp is a request that takes effect when the current event ends, and its last argument
picks the program to run on arrival:

| Native | Goes to |
| --- | --- |
| `jump_to_map(-1, "s02")`, `jump_to_map(-1, "i03h01a", 500)` | another town or story map; program 100 unless one is given |
| `go_to_interior(-1, "i01h01", program, entrance)` / `go_to_outside(-1, "g01", program)` | into and out of buildings |
| `load_dungeon_map("s51"); load_event_script("s51/s51.stb", 500);` | a story map that runs dungeon-style (fights happen here) |
| `go_to_dungeon(0, -1, talk_or_dungeon_start)` | into a dungeon's floor menu |
| `go_to_edit_mode(map_id, program)` | from a dungeon back to a town-style map |
| `load_event_script("g01/g01b_1.stb", 501)` | same map, different script |

### How the story moves forward

There's no central story script. Progress is save flags, checked by whichever handler runs next.
One real chain:

```text
g01i07 · map_start
    if flag 247-seen set and
       time-gate-explained not:
      play the return-deal scene
      (sets flag 250)
      jump_to_map(-1, "s47", 501)
        |
        v
s47 (Sindain, event copy)
    PROGRAM(501, …)
      play the follow-up scene
      jump_to_map(-1, "s26", 501)
        |
        v
s26 · program 501
    next scene …

Back in g01i07 later, flag 250 stops the scene from replaying.
```

Files: `map/g/g01_sindain/i07_dwarves_house/g01i07_1.c`, `map/s/s47_sindain_event/s47_1.c` and
their libraries.

To trace a chain yourself: `dc2tk xref extracted/scripts map s47` lists every way into a map, and
`dc2tk xref extracted/scripts event 501 --map s47` lists everything that starts that program.

## Dungeons

Dungeon floors are random, so there is no script per floor. **One script runs a whole
dungeon**: `map/d/d01_underground_channel/d01_1.c` (and its four translations) handles every
floor of the Underground Channel: floor setup, gates, keys, chests, the floor-clear celebration,
game over, and the story floors that jump elsewhere.

### Setting up a floor

`map/d/d01_underground_channel/d01_1.c` (abridged), program `floor_start`

```c
int setup_dungeon_floor(void)
{
    ...
    init_event();
    fade_out(1);
    wait;
    dungeon_floor = get_dungeon_floor();
    if (dungeon_floor == 20) {                         // a story floor: hand over to s51
        skip_next_bgm_load();
        load_dungeon_map("s51");
        load_event_script("s51/s51.stb", 500);
        return 0;
    }
    ...
    load_dungeon_map("d01f01", "d01f01");             // build a random floor from the d01 parts
    load_monster_data(MONSTER_SEWER_RAT);             // one argument: "this floor's monster list"
    place_treasure_box(-1);                           // the floor's chests
    place_monster(-1);                                // the floor's monsters, at random spots
    ...
}
```

> **Floor monsters are data, not code**
> With one argument, `load_monster_data` ignores it and loads the current floor's monster list
> from engine data. `MONSTER_SEWER_RAT` is only how the decompiler prints the number 0. So you
> won't find "the Golem appears on floor 9" in any script, and `xref monster golem` reports no
> placements. Scripted fights are different: they name exact kinds (see below). The floor lists
> are `dungeon/cfg_file/mos_place<dungeon>.cfg`, which a mod changes as the `data:floors` table
> ([game data](../modding/data.md)).

### The programs a dungeon answers

| Program | Runs when | Retail behaviour |
| --- | --- | --- |
| `talk_or_dungeon_start` (1000) | entering the dungeon | floor select menu, then floor setup |
| `floor_start` (1010) | a floor loads with its number already chosen | the floor setup above |
| `gate` (300) | the floor exit | locked until the Key Handle is used, then on to the next floor |
| `key_door` (348) | a locked door | ask for the key item |
| `treasure_chest` (1100) | confirm at a chest | `open_treasure_chest()`: mimics, locks, traps, then the item. The chest stays shut unless the program opens it. |
| `monster_talk` (1150) | confirm by a talkable monster | monster conversation |
| `player_dead` (1200) | HP reaches 0 | game over (or switch character) |
| `monsters_cleared` (1500) | no monster left on the floor, once | victory pose, medals, open the exit |
| `random_circle`, `geostone`, `spheda_*`, help programs | the floor's features | coin circles, geostones, the golf minigame, tutorials |

### Event mode: why dungeon handlers must be short

When a dungeon program starts, the dungeon enters *event mode* until it ends, and the player
can't move in event mode. So a dungeon handler that loops with `wait;` freezes the player for as
long as it runs. A handler that never waits runs inside a single frame and play carries on.
Retail handlers also pause chosen parts of the dungeon with
`set_dungeon_pause(DNG_PAUSE_MONSTER_AI | ..., 1)` while a chest or door animation plays, and
unpause after.

### Story fights and boss arenas

Fixed fights happen on special maps: the `bNN_*/` folders inside a dungeon, or story maps like
`s51`. They load and place exact monster kinds, and they end when **a monster asks the map to run
a program**:

```text
Map script (s51_1.c)                       Monster AI (b254a_griffon…)

program 500
    load_monster_data(523, 1)
    place_monster(523, x,y,z…) ×4
        |
        |  spawns 4 monsters, each running the AI
        +--------------------------------> each soldier · death
                                               shared var 127 += 1
                                               if (== 4) run_event_script(510)
        +--------------------------------------+
        |  asks the dungeon to run 510
        v
program 510
    victory scene, set flags,
    warp onward
```

The castle prologue fight: `map/s/s51_raybrandt_castle/s51_1.c` and
`dungeon/monster/b254a_griffon_soldier.c`. Bosses work the same way: the Halloween boss in
`b01_channel_reservoir/` calls `run_event_script(520)` when it dies, and program 520 opens the
exit and plays the collapse scene.

`dungeon/monster/b254a_griffon_soldier.c` (abridged)

```c
// Plays the death motion and increments the shared kill counter in variable 127. When the fourth
// one dies it runs event 510.
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

The ending lives in the *monster's* script, which matters when you change a fight (see
[a new enemy in a fight](tutorials/new-enemy.md)). The other way a fight ends is a
`monsters_cleared` program in the map, which the engine runs once the floor is empty.

### Leaving, and game over

Boss rooms go back to the dungeon with
`load_event_script("d01/d01.stb", talk_or_dungeon_start)`; dungeons go to towns with
`go_to_edit_mode(map_id, program)`. Game over is `run_game_over(...)` in the
`lib/map/base_game_over_vN.c` runtime: it offers a character switch if one is possible, otherwise
shows the game over screen. Each map's `player_dead` program calls it.

## Cutscenes

A cutscene is just a long map-script function. They live in the map's libraries
(`lib/map/s/s51_raybrandt_castle/gaspard_time_gate_scene.c` holds `play_event_50`) and are
called from a program. Every retail scene has the same five phases:

The shape of a retail cutscene (abridged from `play_event_50` and the g01 scenes)

```c
void play_some_scene(void)
{
    /* 1 */ set_flag(FLAG_..._SEEN, 1);                  // first, so a skipped scene still counts
    init_event();                                    // clean slate: actor queues, sprites, effects
    load_bgm(BGM_...);
    change_dir("event/e00/e50/");                    // the scene's own folder on the disc
    load_message_file(0, MEMORY_STACK_EVENT, "e50_1.txt");
    load_inactive_chara(11, "e50_soldier.chr");      // models into scene slots
    init_chara_actor(0, CHARA_PLAYER);               // actor 0 drives the player
    init_chara_actor(1, 11);                         // actor 1 drives the soldier
    change_dir("");

    /* 2 */ enable_scene_skip();                         // from here, START skips

    /* 3 */ cut_camera(x, y, z, look_x, look_y, look_z); // first shot
    fade_in(30);
    move_actor_to(1, x, y, z, 240);                  // queued: returns at once
    move_camera_eased(..., 100, SCENE_SEQ_EASE_IN_OUT, 1.0);
    wait_object_script(1);                           // poll until actor 1 is done
    play_voice_message(500010);                      // a voiced line by id
    ...
    fade_out(30);
    wait_frames(30);

    /* 4 */ skip_end;                                    // a skip lands here
    disable_scene_skip();

    /* 5 */ delete_chara(11);                            // tear down what was loaded
    load_message_file(0, -1);                        // two args: unload
    activate_chara(CHARA_PLAYER);
    place_player_reset(...);
    reset_camera(CAMERA_RESET_ACTIVE);
    fade_in(30);
    finish_event();
}
```

1. **Setup.** Set the "seen" flag, reset the event state, load music, the message file and the
   models the scene needs, and bind actors to them.
2. **Open the skip window.**
3. **The timeline.** Camera moves, actor moves and animations are *queued* and play out over the
   following frames. The script waits on them with polling helpers (`wait_object_script`,
   `wait_camera_script`, `wait_frames`).
4. **`skip_end;`** While the player skips, the engine stops running natives until the script
   reaches this line. Anything between `enable_scene_skip` and here may not happen, so never put
   a flag, a warp or cleanup in that stretch.
5. **Teardown.** Unload what was loaded, give the player back their character and camera,
   `finish_event()`. Flags and warps that lead to the next scene usually live in the caller.

### Slots, handles and actors

- **Scene slots** hold models. Slot 0 (`CHARA_PLAYER`) is whoever the player controls, 1–5 are
  their weapons, 8–31 are the map's villagers, and scenes load their own models into free slots
  with `load_scene_chara`, `load_inactive_chara`, `copy_chara`.
- **Handles** (0–31) point at a slot or a map part (`bind_handle_map_part`: a door, a window, a
  sword on the wall).
- **Actors** are command queues that drive a handle. `init_chara_actor(a, slot)` sets up all
  three with the same number. Every `*_actor_*` call adds to the queue; `check_actor_busy(a)`
  says whether it's finished.

### Camera

Positions are the eye `(x, y, z)` then the point looked at, in world units with y up.
`cut_camera` cuts instantly (clearing queued moves); `move_camera_to(..., frames)` glides;
`move_camera_eased(..., frames, ease, rate)` eases; `attach_camera_to_actor` follows someone.
After the scene, `reset_camera(CAMERA_RESET_ACTIVE)` hands control back to gameplay.

### Text and voice

Scene lines are message ids from the file the scene loaded: id = event × 10000 + n (500010 is
event 50's first line). Voiced lines use the same number for the audio stream, and the runtime
library's `play_voice_message(id)` plays the voice, shows the caption, animates lips and waits.
Unvoiced town scenes often use literal text with `show_message(0, "...")` and a bubble helper.

## Monsters

### Kinds, models and AI scripts

Every monster in the game is a *kind*: a row in the engine's monster table giving its name,
model, stats and the AI script it runs. `include/disc.h` lists them all:

```c
MONSTER_GOLEM = 168,  // model e33a, AI e33a
MONSTER_RAVE_GOLEM = 169,  // model e33b, AI e33a
...
MONSTER_GRIFFON_SOLDIER = 37,  // model e254a, AI e254a
MONSTER_GRIFFON_SOLDIER_523 = 523,  // model e254a, AI b254a
```

- **File names are the AI code**: `e33a_golem.c` is AI `e33a`. `e` files are ordinary enemies,
  `b` files bosses and story fights. The letters `a`–`d` are a family's four colour variants,
  which usually all run the `a` AI.
- **One AI often serves many kinds.** Editing `e33a` changes all four golems; `element` runs
  twenty kinds. Branch on `get_monster_kind()` when you need one to differ.
- **Story-fight kinds often have their own AI.** Kind 523 looks like a Griffon Soldier but runs
  `b254a`, so the castle fight can count deaths without changing every Griffon Soldier in the
  game.
- **About 30 AI files are dead**: no kind runs them (for example e111a–e115a, whose models run
  `eJema`). Check with `dc2tk xref extracted/scripts monster NAME` before editing.

### Inside a monster script

| Program | When |
| --- | --- |
| `init` (100) | once, on spawn: register attacks, load effects, set up AI state |
| `update` (200) | the brain. Loops forever; restarted after any reaction program finishes |
| `damage` (500), `knockdown` (600), `stun` (1400), `lifted`/`land` | hit reactions |
| `guard` (700), `dodge_melee` (800), `dodge_shot` (900) | when the engine's guard or dodge roll succeeds |
| `death` (1000) | HP reaches 0 |

The Golem's `init` shows the setup pattern:

`dungeon/monster/e33a_golem.c` (abridged)

```c
void on_init(void)
{
    //             damage setting         radius  animation           active from..to  at point
    add_attack_hit(HIT_KNOCKBACK_ATTACK, 6.5, MOTION_ATTACK_1_2, 0.3, 0.37, 0);
    add_attack_hit(HIT_BEING_BLOWN_AWAY, 6.5, MOTION_ATTACK_1_2, 0.5, 0.6, 2);
    load_effect(EFFECT_VANGUARD_SHOCKWAVE);
    ...
    init_monster_ai();     // toolkit: aggro range 500, remember the spawn point, ...
}
```

`add_attack_hit` is the key idea: a hit is a damage volume attached to a point on the model that
is live *while a given animation plays between two fractions of its length*. Once registered, an
attack is just "play the animation":

`dungeon/monster/e33a_golem.c`

```c
void chase_and_swing_at_target(void)
{
    loop_motion(MOTION_WALK);
    chase_target_steps(1.25, 3.0, 62.0 + 4.0, 90);
    turn_to_target_wait(3.0);
    set_motion(MOTION_ATTACK_1_2, -1, CHARA_MOTION_HOLD);   // the hits above fire during this
    wait_motion_end_update();
    loop_motion(MOTION_STAND_2);
    wait_frames_update(6);
}
```

And the brain decides which attack to run each frame:

`dungeon/monster/e33a_golem.c` (abridged)

```c
void choose_behavior_by_aggro(void)
{
    rage_hits_left = get_rage_hits_left();
    if (rage_hits_left > 0) {
        target_dist = get_target_dist();
        if (!is_aggro()) {
            if (target_dist < 280.0 + 50.0) {
                fight_target_by_range();       // close / mid / far attack choice
            } else {
                choose_idle_wander_action();
            }
        } else {
            ...                                // aggro: guard up, walk in
        }
    } else {
        set_defense_rate(1.5);
        fight_while_enraged();
    }
}
```

### The monster toolkit

Most monsters include `lib/dungeon/monster/monster_ai_full_toolkit.c` (or a smaller variant). It
is the AI standard library:

| Area | Functions |
| --- | --- |
| Waiting (use instead of bare `wait;`, they also update aggro) | `wait_frame_update`, `wait_frames_update`, `wait_motion_end_update`, `wait_motion_time(fraction, …)` |
| Aggro | `init_monster_ai`, `is_aggro`, `set_aggro`, `get_aggro_range` |
| Turning | `turn_to_target_wait`, `turn_to_angle`, `track_target_until_motion_end` |
| Moving | `chase_target_steps`, `walk_to_target`, `walk_around_target`, `walk_in_direction`, `find_clear_direction` |
| Seeing | `is_target_in_view`, `can_detect_target`, `wait_target_in_view` |
| Interrupts | `lock_interrupts_stop`, `unlock_interrupts`, `stop_all_actions` |

Thirty-six monsters include this one file, so editing it changes all of them; see
[make an enemy behave differently](tutorials/enemy-behaviour.md) for how to avoid that.

### Tuning knobs

- `set_defense_rate`, `set_guard_rate(-1, r)`, `set_dodge_rate(-1, r)`: scale the table's values
  for this monster (-1 means "me").
- `set_mask_flag(MASK_NO_GUARD / MASK_NO_DODGE, on)`: switch off the engine's guard or dodge
  rolls. Scripts lock both during their own scripted moves.
- `get_rage_hits_left()`: hits left before the monster enrages (-1 for bosses).
- `get_life(&f)` (0–1) and `set_life`: bosses use these for phase changes.

### Bosses

Boss scripts (`b08a_emperor_griffon.c`, `b13a_arch_demon.c`) are long, 2,500–3,300 lines, and
add: a custom camera (`set_camera_mode(CAMERA_MODE_BOSS)`), helper monsters (`create_monster`),
phase changes checked inside every wait (when life drops below a mark, the boss calls
`run_event_script` for a mid-fight scene), and a death program that hands the ending to the map.

## The player

`mainchr/c01_max.c` and `c02_monica.c` are the heroes in combat, built like monster scripts but
driven by the pad. `init` registers the hit windows for each weapon model; `update` is a state
machine whose state is a 16-bit "saved value":

| State (Max) | Meaning |
| --- | --- |
| 0 | free: move with the stick, choose the next action from the buttons |
| 100–198 | sword combo, one value per hit |
| 200 / 300 | mid-range lunge / strike on a downed enemy |
| 800 / 820 / 840 | gun: single shot / machine gun / slow shot |
| 900, 1000, 1100 | guard and catch, item throw, lift and throw |
| 1200 | charge attack |
| 1300 | backflip |

Each combo hit checks how far its animation has played and opens a short window for the next
button press:

`mainchr/c01_max.c` (abridged)

```c
if (saved_value == 101) {
    get_motion_progress(&progress);
    ...
    if (progress >= 0.33 && progress <= 0.45) {          // the chain window
        if (get_attack_presses()) {
            start_combo_hit(MOTION_5_HIT_COMBO_2, saved_value + 1);
        }
    }
    ...
}
```

Monica's file has the same layout, with magic in 800. `dungeon/act_script/f*a.c` are Monica's
monster transformations (`f` + the monster's model code), `robo_ridepod.c` is the Ridepod, and
`c01win.c` / `f204a.c` are old builds whose engine calls no longer line up. Leave those two
alone.

## Effects

An effect script is a tiny program: `play` runs from the moment the effect is created and moves
its sprites or model every frame; the effect is deleted when `play` returns. `finish` runs if the
creator asks it to stop. The creator passes data in through two work vectors and eight values.
Effects can do damage themselves, which is how a monster's shockwave hurts the player.

The Golem's shockwave: the monster loads and spawns the effect, the engine maps its name to a
script, and the effect creates the attack.

```text
e33a_golem.c
    init:
      load_effect(EFFECT_VANGUARD_SHOCKWAVE)
    slam, at 0.64 of the swing:
      get_frame_pos("dcol0", …)
      spawn_effect_at(EFFECT_…, x,y,z)
      set_effect_value(-1, 0, attack)
        |
        v
engine effect table
    "ヴァンガード衝撃波"
    → script m_shockwave
        |
        v
eff_script/m_shockwave.c · play
    get_effect_value(0, &damage)
    get_work_vector1(…)  // where
    play the ring animation
    create_attack(HIT_WEAK_KNOCKBACK)
    set_attack_shape(…, 30.0)
    set_attack_damage(damage)
    … delete_attack, return
```

`EFFECT_*` names in `disc.h` are English names for the Japanese names the engine looks up; the
engine's own table maps each to a script. The Vanguard, Mammo, Elfas and others spawn the same
shockwave, so changing `m_shockwave.c` changes it for all of them.

Map scripts start effects too (`load_effect_script`, then `create_effect(name, owner, &slot)`),
as do the player scripts (Max's charge attack creates `EFFECT_MAX_CHARGE_ATTACK`). Many effect
bodies live in `lib/dungeon/*.c` because the same effect exists both in `eff_script/` and packed
into `articles/eff01.chr.d/`.

## Your working loop

Never edit `extracted/scripts/` in place for real work. Put changes in a **mod**, a folder in
`mods/` holding only the files you change or add. The tree stays clean, and mods can be
combined.

1. **Find the code.** Use the [lookup commands](#quick-reference), the comments, and your editor
   (open `extracted/scripts/` in an editor with clangd for hover docs and go-to-definition).
2. **Copy it into a mod.**

   ```text
   dc2tk mod new extracted/scripts my-change map/g/g01_sindain/g01_1.c
   ```

   Then edit under `mods/my-change/files/`.
3. **Or add without copying.** A mod's `mod.json` can add programs and includes to a map's
   scripts (all languages at once), or wrap an existing program with your functions before or
   after it. See [add a cutscene to a town](tutorials/cutscene.md). A *patch* goes further: a
   `.c` file with `#pragma target map g01` (or `script`, `monster`, `library`) holding only the
   functions it overrides or adds, merged into every script it targets.
   `dc2tk mod override extracted/scripts my-change FUNCTION --map g01` starts one, and
   `dc2tk mod slim` turns copied files into patches ([mods](../modding/mods.md)).
4. **Build and play.**

   ```text
   dc2tk build extracted/scripts dc2.iso -o modded.iso --mod my-change --play
   ```

   Only the scripts that change are recompiled, so this takes seconds.
5. **Jump straight to the scene.** `--start-at s51:story_50` (or `"start_at"` in `mod.json`)
   makes New Game start there, on maps loaded dungeon-style. A town map works too, entered as the
   game enters towns: `--start-at g01` starts in Sindain at `map_start`, as Max. Either way it is
   a new game's save, with no story flags set, so a scene that waits for one won't play; for
   those, keep a PCSX2 save state near the spot.

> **You can change files, not add them**
> `build` replaces scripts that exist on the disc. A brand-new script file (say
> `dungeon/monster/e999a.c`) is skipped with "not on the disc", because nothing in the game would
> load it. New *library* files are fine, since they're pasted into existing scripts. Keep yours
> under `lib/mods/NAME/`. The one new script a mod can have is a new map's own: a map added under
> `"add"` in `mod.json` takes a `"script"`, which build compiles where the engine looks for it
> ([maps](../modding/maps.md)).

The [tutorials](tutorials/README.md) walk through the changes you'll most often be asked for.

## Gotchas checklist

Run through this before calling a change done.

- **All five languages.** Did the change go into `_1` to `_5` (or `_v1` to `_v5`), or into one
  library they all include?
- **Library reach.** Did you edit a file in `lib/`? `build` lists the other scripts it reached.
  Fork it if that's not intended.
- **Shared AI.** Does your monster script run for other kinds too? Is it one of the dead AI
  files?
- **Fight endings.** If you changed who is in a fight, does something still end it (a monster's
  `run_event_script`, or `monsters_cleared`)?
- **Skip window.** No flag, warp or cleanup between `enable_scene_skip()` and `skip_end;`.
- **Natives never wait.** Every "wait until" is a loop with `wait;` in it.
- **Dungeon handlers freeze the player** while they wait.
- **Frame counts.** Retail `wait_frames` scales by 50/60; `modkit_wait` and bare loops don't.
- **Asset names are exact.** `MOTION_*`, `HIT_*` and `EFFECT_*` stand for Japanese strings the
  engine looks up. `MOTION_ATTACK_1` and `MOTION_ATTACK_1_2` are different strings (half- and
  full-width digits). Use the constants the model's own script uses; a wrong one silently does
  nothing.
- **Effects must be loaded** (`load_effect` in `init`) before they're spawned.
- **Flags.** New flags come from 825–2046 (`NEW_FLAG` hands them out from 1024); 2047 belongs to
  `--start-at`. Clear scene-only flags when the scene ends and on game over.
- **Message window 0 stays open** after an event ends in a dungeon until something closes it.
- **New script files aren't built.** Only new libraries included by existing scripts, and a new
  map's own script.

## Quick reference

### Finding things

| Question | Command |
| --- | --- |
| Every map, and which are towns or dungeon-style | `dc2tk maps extracted/scripts` |
| One map's programs, triggers, free numbers, player positions | `dc2tk map extracted/scripts g01` |
| How the game gets into a map | `dc2tk xref extracted/scripts map s47` |
| What starts or requests a program | `dc2tk xref extracted/scripts event 510 --map s51` |
| A monster's model, AI, sharers and placements | `dc2tk xref extracted/scripts monster golem` |
| Save flags in use, free ones, who uses one | `dc2tk flags extracted/scripts [--flag N]` |
| What you've changed in the tree | `dc2tk changes extracted/scripts` |

### Names you'll look up constantly

| Looking for | Where |
| --- | --- |
| What a native does | `sdk/event.h`, `monster.h`, `action.h`, `effect.h` (hover in the editor); [api/](api/README.md) |
| Program names and numbers | `enum program` in each of those headers |
| Monster kinds, items, NPC ids, motions, effects, hit types | `extracted/scripts/include/disc.h` |
| Story flags, music, sound sets | `definitions/pal/include/pal.h` |
| Text helpers and waits for mods | `sdk/modkit/` |
| Monster stats, floor monster lists, chest contents, items, shops (to change them) | `dc2tk mod asset dc2.iso NAME data:monsters` (`data:floors`, `data:chests`, `data:items`, ...); [modding/data.md](../modding/data.md) |

### Further reading

- [game/running.md](../game/running.md): engine rules behind event mode, message windows and
  monsters starting events.
- [tutorials/captions.md](tutorials/captions.md): a worked caption pattern.
- [modding/commands.md](../modding/commands.md): every command.
- [modding/mods.md](../modding/mods.md): mods, `mod.json` additions and tests.
- [examples/calculator/](examples/calculator/calculator.c): an example mod, a calculator with its
  own model, screen and sounds.
- [language.md](language.md): the C dialect and where it differs from C (values keep their type,
  `&&` returns a value).
- [sdk.md](sdk.md): the headers and modkit.
