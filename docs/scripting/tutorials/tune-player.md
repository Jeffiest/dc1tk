# Tune the player

This guide explains how to adjust Max's melee combo timing windows, attack hitboxes, and charge attack threshold. For background on player action scripts, see [The player](../systems.md#the-player). For other guides, see [Scripting tutorials](README.md).

## Procedure: Make Max's melee combo windows more forgiving

1. **Copy Max's action script into a new mod**:

   ```bash
   dc2tk mod new extracted/scripts max-combo mainchr/c01_max.c
   ```

2. **Widen the combo chain input windows**: Open `mods/max-combo/files/mainchr/c01_max.c` and locate `update_sword_combo_model_0` (as well as `_1` and `_2`, which correspond to each weapon model class). Each combo hit checks the animation progress fraction with a condition such as `progress >= 0.33 && progress <= 0.45`. Decrease the start fraction or increase the end fraction to give the player a wider window to press Attack for the next combo hit.
3. **Adjust attack damage, reach, and active frames**: Locate `setup_max_attacks` in `c01_max.c`. In each `add_attack_hit("dcol00", "dcol01", HIT_..., radius, power_rate, MOTION_..., start, end)` call:
   - Increase `power_rate` to deal more damage.
   - Increase `radius` to expand the hitbox reach.
   - Widen the `start` and `end` animation fractions to keep the hitbox active longer.

   Update the corresponding blocks for each weapon model class (`0`, `1`, and `2`).
4. **Adjust the charge attack hold threshold (optional)**: Max begins charging an attack when the Attack button is held past a frame threshold (`get_attack_hold_frames() > 8` during the first combo hit, or `> 12` from idle standing). Lower or raise these frame counts to change how quickly the charge attack activates. The charge attack state machine itself lives in `lib/max_charge_attack.c`, which is shared with the Max action scripts in `dungeon/act_script/`.
