# Add a cutscene to a town

This guide shows how to create a self-contained mod that adds a new cutscene to a town map using one custom library file and `mod.json`, without copying any retail script files. For background on retail cutscene structure, see [Cutscenes](../README.md#cutscenes). For other guides, see [Scripting tutorials](README.md).

## Procedure: Play a welcome cutscene the first time the player enters Sindain

1. **Select an unused save flag**: The base game uses save flags up to `824`. Verify that flag `1300` is unused by running:

   ```bash
   dc2tk flags extracted/scripts --flag 1300
   ```

   (Note: In mod patches and headers outside `files/`, you can declare `NEW_FLAG(FLAG_SINDAIN_HELLO_SEEN);` to let `dc2tk build` allocate and persist a free flag number automatically in `mods/numbers.json`. Because whole files under `files/` compile without `mods/numbers.json`, a standalone library file under `files/lib/` defines its flag number explicitly with `#define`.)
2. **Inspect the target map's existing entry behavior**: Run `dc2tk map` to inspect Sindain (`g01`):

   ```bash
   dc2tk map extracted/scripts g01
   ```

   This command lists the map's programs, trigger points, and unused program numbers (`9000` and above). Check `map_start` in `g01_1.c` to see which story cutscenes already run on entry so your condition does not conflict with them.
3. **Write the cutscene library**: Create `mods/sindain-hello/files/lib/mods/sindain-hello/hello.c`:

   ```c
   // Plays a one-time welcome cutscene in Sindain: moves the camera around the player,
   // plays a wave animation, and displays a message window.

   #include <event.h>
   #include "modkit/text.c"
   #include "modkit/input.c"

   #define FLAG_SINDAIN_HELLO_SEEN 1300

   // Waits until the specified actor finishes all queued commands.
   void hello_wait_for_actor(int actor)
   {
       while (check_actor_busy(actor)) {
           wait;
       }
   }

   // Runs after g01's map_start program once the player has seen the Rococo encounter.
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
       play_actor_motion_then_loop(0, MOTION_WAVE, MOTION_STAND_2);   // Motion constants from disc.h
       hello_wait_for_actor(0);

       modkit_say("Sindain looks busier today.");

       reset_camera(CAMERA_RESET_ACTIVE);
       finish_event();
   }
   ```

   This function follows the standard cutscene lifecycle:
   - Set the completion flag first.
   - Call `init_event()` and bind actor `0` to `CHARA_PLAYER`.
   - Queue the camera and animation commands and wait for completion.
   - Display the dialogue line with `modkit_say`.
   - Restore the gameplay camera with `reset_camera(CAMERA_RESET_ACTIVE)` and call `finish_event()`.
4. **Register the library and wrap `map_start` in `mod.json`**: Create `mods/sindain-hello/mod.json` to include `hello.c` and run `play_sindain_hello` after `map_start` across all five language files of `g01`:

   ```json
   {
    "additions": [
     {"scripts": ["map/g/g01/g01_?.c"],
      "includes": ["lib/mods/sindain-hello/hello.c"],
      "wrap": {"map_start": {"after": ["play_sindain_hello"]}}}
    ]
   }
   ```

   - `"scripts"` matches internal disc script paths, where `?` matches the language digit (`1`–`5`). Using `"scripts": ["map/g/g01/g01_?.c"]` targets only the main exterior `g01` scripts, whereas `"map": "g01"` would also target alternate scripts (`g01b`, `g01c`, `rush_g01`) and interior buildings (`g01i07`).
   - Note that an `"after"` wrapper function does not run if the original `map_start` function warps to another map before returning.
   - To trigger the cutscene from a warp or trigger point instead, register a new program ID with `"programs": {"9000": "play_sindain_hello"}` and call `jump_to_map(-1, "g01", 9000)`.
5. **Build and test the mod**: Because `play_sindain_hello` checks `FLAG_ROCOCO_ENCOUNTER_SEEN`, starting a fresh save with `--start-at g01` will not trigger the scene (all story flags start at `0`). Load a save file or PCSX2 save state after meeting Rococo and enter Sindain:

   ```bash
   dc2tk build extracted/scripts dc2.iso -o modded.iso --mod sindain-hello --play
   ```

> **Tip**
> **Adding custom characters, scene skipping, and unit tests**
> - To include additional characters in a cutscene, load their `.chr` models into unused scene slots with `load_inactive_chara` or `load_scene_chara`, bind actors to those slots, and call `delete_chara` during teardown.
> - To make the cutscene skippable with START, place `enable_scene_skip();` before the camera timeline and `skip_end; disable_scene_skip();` before teardown. Keep all `set_flag`, warp, and `delete_chara` calls outside the skip window.
> - For a complete example mod that adds a new map, custom Blender character models, and unit tests (`dc2tk mod test`), see [The Spinning Cult](../examples/spinning-cult/README.md).
