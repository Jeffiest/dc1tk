# Add a cutscene to a town

A complete mod that touches no game file: one new library and a `mod.json`. How retail scenes
are built is described in [cutscenes](../README.md#cutscenes); the other walkthroughs are in the
[tutorials](README.md).

## Play a short scene the first time the player enters Sindain after this point

1. **Pick a free flag.** The game uses save flags up to 824. Check yours is free with
   `dc2tk flags extracted/scripts --flag 1300`. Or let build choose:
   `NEW_FLAG(FLAG_SINDAIN_HELLO_SEEN);` in one of the mod's patches or headers (outside `files/`)
   gets a number nothing else uses, kept from build to build in `mods/numbers.json`
   ([mods](../../modding/mods.md)). Only the mod's sources outside `files/` see that name: build
   compiles whole files under `files/`, like the library below, without the mods' numbers and
   refuses one that asks for a number, so the library writes its flag by hand.
2. **See what the map already does on entry.** `dc2tk map extracted/scripts g01` lists its
   programs, trigger points and free program numbers (9000 and up). Read its `map_start` so your
   scene doesn't fight a story scene that also auto-plays there.
3. **Write the scene as a library.** `mods/sindain-hello/files/lib/mods/sindain-hello/hello.c`:

   `lib/mods/sindain-hello/hello.c` (new code)

   ```c
   // A short welcome scene in Sindain, played once: the camera swings round to the player, who
   // waves, and a line of text appears.

   #include <event.h>
   #include "modkit/text.c"
   #include "modkit/input.c"

   #define FLAG_SINDAIN_HELLO_SEEN 1300

   // Waits until actor has finished everything queued on it.
   void hello_wait_for_actor(int actor)
   {
       while (check_actor_busy(actor)) {
           wait;
       }
   }

   // Runs after g01's own map_start. Plays once, and not before the player has met Rococo.
   void play_sindain_hello(void)
   {
       float x;
       float y;
       float z;

       if (get_flag(FLAG_SINDAIN_HELLO_SEEN) || !get_flag(FLAG_ROCOCO_ENCOUNTER_SEEN)) {
           return;
       }
       set_flag(FLAG_SINDAIN_HELLO_SEEN, 1);
       init_event();
       init_chara_actor(0, CHARA_PLAYER);

       get_chara_pos(CHARA_PLAYER, &x, &y, &z);
       cut_camera(x + 120.0, y + 60.0, z + 120.0, x, y + 20.0, z);
       move_camera_to(x + 60.0, y + 30.0, z + 80.0, x, y + 20.0, z, 60);
       play_actor_motion_then_loop(0, MOTION_WAVE, MOTION_STAND_2);   // names from disc.h
       hello_wait_for_actor(0);

       modkit_say("Sindain looks busier today.");

       reset_camera(CAMERA_RESET_ACTIVE);
       finish_event();
   }
   ```

   The shape follows [cutscenes](../README.md#cutscenes), cut down: a flag set before anything
   can fail, `init_event`, actors, camera, a line, then camera back and `finish_event`. The
   `modkit` libraries give you text and waits without depending on a map's runtime library.
   Check every native and helper against `sdk/event.h` as you write (hover in the editor); the
   motion names must be ones the player's model has.
4. **Hook it in with `mod.json`.** Wrapping `map_start` runs your function after the game's own,
   in the five language scripts of g01, without copying any of them:

   ```json
   {
    "additions": [
     {"scripts": ["map/g/g01/g01_?.c"],
      "includes": ["lib/mods/sindain-hello/hello.c"],
      "wrap": {"map_start": {"after": ["play_sindain_hello"]}}}
    ]
   }
   ```

   `"scripts"` takes disc paths (codes without the readable names) and `?` stands for the
   language digit. The shorter `"map": "g01"` would also reach every other script of the map: the
   g01b and g01c alternates, the rush script, and the interiors in its folder, so the scene would
   play inside the Dwarves' house too. An `"after"` function never runs if the program it wraps
   warps to another map, so check what `map_start` does first. To trigger it from somewhere else
   instead, add it as a program (`"programs": {"9000": "play_sindain_hello"}`) and warp to it:
   `jump_to_map(-1, "g01", 9000)`.
5. **Build and play.** `--start-at g01` would start a new game in Sindain, but with no story
   flags set, so this scene (which waits for the Rococo flag) wouldn't play; play to Sindain or
   use a save state:

   ```text
   dc2tk build extracted/scripts dc2.iso -o modded.iso --mod sindain-hello --play
   ```

> **Going further**
> For a scene with its own characters, follow the retail setup phase: load models into free scene
> slots, bind actors, and tear everything down after. If the scene must be skippable, wrap the
> timeline in `enable_scene_skip()` … `skip_end;` `disable_scene_skip();` and keep every flag and
> cleanup outside it. The [spinning-cult](../examples/spinning-cult/README.md) example is a fuller
> mod (a new map with a script of its own behind a door, a character made in Blender), and
> `dc2tk mod test` runs a mod's logic without the game.
