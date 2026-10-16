# Make an enemy behave differently

The pattern for "this enemy is too easy / should also do X". How monster scripts are built is
described in [monsters](../README.md#monsters); the other walkthroughs are in the
[tutorials](README.md).

## Make the Golem hit harder and react sooner

1. **Find its AI and who shares it.**

   ```text
   dc2tk xref extracted/scripts monster golem
   ```

   This prints the model, the AI file (`dungeon/monster/e33a_golem.c`), and the other kinds that
   run the same AI (Rave Golem, Rock Taster, Titan). Your change will reach all four unless you
   branch on `get_monster_kind()`.
2. **Copy it into a mod.**

   ```text
   dc2tk mod new extracted/scripts stronger-golem dungeon/monster/e33a_golem.c
   ```

3. **Change what's in the script itself.** The useful knobs:
   - Hit size and timing: the radius and the start/end fractions in `add_attack_hit` in
     `on_init`.
   - Hit type: swap `HIT_KNOCKBACK_ATTACK` for another `HIT_*` from `disc.h`. Only existing
     damage settings work.
   - When it engages: the distances in `choose_behavior_by_aggro` (`280.0 + 50.0`, `230.0`).
   - Toughness: `set_defense_rate`, `set_guard_rate`.
   - New moves: write a function like `chase_and_swing_at_target` using animations the Golem's
     model has (use the `MOTION_*` names already in its script) and call it from the behaviour
     choice.
4. **If you need to change a library function** (say `init_monster_ai`'s aggro range), fork the
   library for this script only. Editing `monster_ai_full_toolkit.c` itself would change 36
   monsters:

   ```text
   dc2tk fork extracted/scripts lib/dungeon/monster/monster_ai_full_toolkit.c lib/dungeon/monster/golem_toolkit.c dungeon/monster/e33a_golem.c
   ```

5. **Build, then find a Golem.** They roam random floors, so either play to one or put some in a
   scripted fight you can reach with `--start-at` (the next walkthrough,
   [a new enemy in a fight](new-enemy.md)).

> **Not in a script's reach**
> HP, attack power, the ABS and money it gives, and drops are in the engine's monster table.
> Scripts can scale how a monster takes damage (`set_defense_rate`) and read its power
> (`get_attack()`), but can't edit the table. A mod can, beside its scripts:
> `dc2tk mod asset dc2.iso stronger-golem data:monsters` writes the table into the mod as
> `data/monsters.json` with its `mod.json` line, and every value you change there (`"hp"` of
> `{"id": 168, ...}`) is patched into the built image's executable; the rest changes nothing.
> That changes every monster of kind 168, wherever it appears
> ([game data](../../modding/data.md)).
