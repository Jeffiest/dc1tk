# Change enemy behavior

This guide explains how to modify a monster's combat AI without unintentionally changing other monsters that share the same script or library. For background on monster scripts, see [Monsters](../systems.md#monsters). For other guides, see [Scripting tutorials](README.md).

## Procedure: Make the Golem attack harder and engage from farther away

1. **Identify the monster's AI script and shared variants**: Run `dc2tk xref` to inspect the Golem:

   ```bash
   dc2tk xref extracted/scripts monster golem
   ```

   The output lists the 3D model, the AI script path (`dungeon/monster/e33a_golem.c`), and all monster kinds that share this AI script (Golem, Rave Golem, Rock Taster, and Titan). Any unconditional change to `e33a_golem.c` affects all four variants unless you branch on `get_monster_kind()`.
2. **Copy the monster AI script into a new mod**:

   ```bash
   dc2tk mod new extracted/scripts stronger-golem dungeon/monster/e33a_golem.c
   ```

3. **Modify the combat parameters in the script**: Open `mods/stronger-golem/files/dungeon/monster/e33a_golem.c` and adjust the desired parameters:
   - **Hitbox radius and active frames**: Change the radius and the start/end animation progress fractions (`0.0` to `1.0`) in `add_attack_hit` inside `on_init`.
   - **Hit reaction type**: Replace `HIT_KNOCKBACK_ATTACK` with another `HIT_*` constant from `disc.h`.
   - **Engagement distance**: Increase the distance thresholds in `choose_behavior_by_aggro` (for example, `280.0 + 50.0` and `230.0`).
   - **Defense and guard rates**: Call `set_defense_rate` or `set_guard_rate`.
   - **New attack patterns**: Write a new attack function modeled on `chase_and_swing_at_target` using `MOTION_*` constants already supported by the Golem's model, and call it from the behavior selection function.
4. **Fork the shared toolkit library if needed**: Thirty-six monster scripts include `lib/dungeon/monster/monster_ai_full_toolkit.c`. If you need to modify a toolkit function (such as the default aggro range in `init_monster_ai`) for the Golem only, fork the library so other monsters are unaffected:

   ```bash
   dc2tk fork extracted/scripts lib/dungeon/monster/monster_ai_full_toolkit.c lib/dungeon/monster/golem_toolkit.c dungeon/monster/e33a_golem.c
   ```

5. **Build and test the mod**: Build the modded ISO and test the Golem in combat. Because Golems spawn on random dungeon floors, you can either play to a floor where Golems appear or place a Golem into a scripted battle reachable with `--start-at` (see [Add a new enemy to a fight](new-enemy.md)).

> **Note**
> **Modifying base monster stats (HP, attack, ABS, Gilda, and item drops)**
> Base stats and item drops reside in the engine's monster table rather than in `.stb` scripts. While a script can scale incoming damage with `set_defense_rate` and read attack power with `get_attack()`, changing base stats requires editing the `data:monsters` table in your mod:
>
> ```bash
> dc2tk mod asset dc2.iso stronger-golem data:monsters
> ```
>
> This command exports `data/monsters.json` into your mod and registers it in `mod.json`. Any fields you edit in `data/monsters.json` (for example, `"hp"` on entry `{"id": 168, ...}`) are patched into the executable when you run `dc2tk build` (see [game data](../../guide/data.md)).
