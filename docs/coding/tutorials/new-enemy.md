# A new enemy in a fight

You can't add a monster kind: kinds are rows in the engine's table, which a mod can change
(`data:monsters`: stats, model, the AI script a kind runs) but not add to, and a new AI file is
never built. What you can do covers most requests. How fights are set up and ended is described
in [story fights and boss arenas](../README.md#story-fights-and-boss-arenas); the other
walkthroughs are in the [tutorials](README.md).

## What "new enemy" can mean here

| Request | How |
| --- | --- |
| "Put enemy X in this fight" | Place an existing kind from the fight's map script |
| "A new enemy with its own behaviour" | Place an existing kind and rewrite an AI it runs. Pick an event-only kind (500+ with its own `b` AI) so the ordinary version elsewhere is unchanged. |
| "Extra enemies on a random floor" | Call `load_monster_data(kind, 0)` and `place_monster(kind, …)` in the dungeon's floor setup, after the floor's own spawn |

## Recipe: replace the castle soldiers with Golems

1. **Find every place the fight is set up.** `dc2tk xref extracted/scripts monster 523` shows the
   soldiers placed in two functions: `start_castle_battle` (program 500 of
   `map/s/s51_raybrandt_castle/s51_1.c` to `_5.c`, reached from dungeon 1) and
   `start_monica_gear_battle` (program `story_50`, in
   `lib/map/s/s51_raybrandt_castle/monica_gear_battle.c`). Copy the five scripts and the library
   into a mod and change both.
2. **Swap the kind.** Load the new kind (the `1` clears what was loaded before), then place it:

   ```c
   load_monster_data(MONSTER_GOLEM, 1);
   place_monster(MONSTER_GOLEM, -382.5, -0.2, 120.2, 0.0, 1.61, 0.0);   // x, y, z, then rotation
   place_monster(MONSTER_GOLEM, -428.0, -0.2, 202.7, 0.0, 1.71, 0.0);
   ...
   ```

   To mix kinds, load the first with `1` and the rest with `0`. `place_monster` only works on
   dungeon-style maps.
3. **Fix how the fight ends.** This is the step people miss. The soldiers' own AI ran event 510
   when the fourth one died; Golems don't do that, so the fight would never end. Add a program
   the engine runs when the floor is empty:

   ```c
   PROGRAM(monsters_cleared, start_georama_with_starter_items);   // beside PROGRAM(510, ...)
   ```

   Or make the Golem's death program count kills and call `run_event_script(510)` like the
   soldier does, guarded by a flag that only this fight sets so Golems elsewhere aren't
   affected.
4. **Test it directly.** `story_50` runs `start_monica_gear_battle`, which also gives Monica her
   gear:

   ```text
   dc2tk build extracted/scripts dc2.iso -o test.iso --mod castle --start-at s51:story_50 --play
   ```

To give the castle Golems their own behaviour too, combine this with
[make an enemy behave differently](enemy-behaviour.md): copy `e33a_golem.c`, fork its toolkit,
and check your fight's flag with `get_dungeon_flag` wherever the castle version should act
differently.
