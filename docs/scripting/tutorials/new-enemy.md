# Add a new enemy to a fight

The engine's monster table has a fixed number of rows, and `dc2tk build` only compiles `.stb` script files that exist on the disc. While a mod cannot append a brand-new monster kind ID, you can place any existing monster kind into a fight or repurpose an event-only monster kind. For background on how scripted fights work, see [Story fights and boss arenas](../systems.md#story-fights-and-boss-arenas). For other guides, see [Scripting tutorials](README.md).

## Approaches for adding or replacing enemies

| Goal | Implementation method |
| --- | --- |
| Spawn a different existing monster in a scripted fight | Load and place the existing monster kind ID in the fight's map script. |
| Create a custom enemy with unique AI behavior | Use an event-only monster kind (ID `500` or above, which has its own `b*` AI script) and modify its AI script and `data:monsters` stats so standard enemies elsewhere remain unchanged. |
| Spawn extra enemies on a random dungeon floor | Call `load_monster_data(kind, 0)` and `place_monster(kind, ...)` inside the dungeon's `floor_start` handler after the standard floor spawn calls. |

## Procedure: Replace the castle prologue soldiers with Golems

1. **Locate all functions that set up the fight**: Run `dc2tk xref` on monster kind `523` (the prologue Griffon Soldier):

   ```bash
   dc2tk xref extracted/scripts monster 523
   ```

   The output shows that monster `523` is spawned in two functions:
   - `start_castle_battle` (program `500` in `map/s/s51_raybrandt_castle/s51_1.c` through `_5.c`, reached from dungeon `d01`).
   - `start_monica_gear_battle` (program `story_50` in `lib/map/s/s51_raybrandt_castle/monica_gear_battle.c`).

   Copy the five `s51` scripts and the `monica_gear_battle.c` library into a new mod so you can update both entry paths.
2. **Load and spawn the new monster kind**: Replace the `load_monster_data` and `place_monster` calls with `MONSTER_GOLEM`. Passing `1` as the second argument to `load_monster_data` clears previously loaded monster data:

   ```c
   load_monster_data(MONSTER_GOLEM, 1);
   place_monster(MONSTER_GOLEM, -382.5, -0.2, 120.2, 0.0, 1.61, 0.0);   // Position (x, y, z) and rotation (rx, ry, rz)
   place_monster(MONSTER_GOLEM, -428.0, -0.2, 202.7, 0.0, 1.71, 0.0);
   ...
   ```

   To spawn multiple monster kinds in the same fight, pass `1` to the first `load_monster_data` call and `0` to subsequent `load_monster_data` calls. Note that `place_monster` works only on maps loaded in dungeon mode.
3. **Configure the battle completion trigger**: In the original fight, the Griffon Soldier's AI (`b254a_griffon_soldier.c`) increments a kill counter on death and calls `run_event_script(510)` when the fourth soldier dies. Because `e33a_golem.c` does not call `run_event_script(510)`, the battle would never end after defeating the Golems.

   To fix this, register a `monsters_cleared` (`1500`) program in the `s51` map scripts so the engine automatically starts the post-fight function when all monsters on the floor are defeated:

   ```c
   PROGRAM(monsters_cleared, start_georama_with_starter_items);   // Add alongside PROGRAM(510, ...)
   ```

   Alternatively, you can edit `e33a_golem.c` to count deaths and call `run_event_script(510)` when a fight-specific save flag is set.
4. **Build and test the fight directly**: Use `--start-at s51:story_50` to jump directly into `start_monica_gear_battle` (which also equips Monica's starting weapons):

   ```bash
   dc2tk build extracted/scripts dc2.iso -o test.iso --mod castle --start-at s51:story_50 --play
   ```

To customize the AI of the Golems in this fight without changing Golems on random dungeon floors, combine this guide with [Change enemy behavior](enemy-behaviour.md): set a save flag when starting the castle fight and check `get_dungeon_flag` inside `e33a_golem.c`.
