# Tune the player

Make Max's combo more forgiving. How the player scripts are built is described in
[the player](../README.md#the-player); the other walkthroughs are in the
[tutorials](README.md).

1. **Copy Max into a mod:** `dc2tk mod new extracted/scripts max-combo mainchr/c01_max.c`.
2. **Widen the chain windows.** In `update_sword_combo_model_0` (and `_1`, `_2`: one per weapon
   model), each hit has a test like `progress >= 0.33 && progress <= 0.45`. Lower the start or
   raise the end.
3. **Damage and reach** are in `setup_max_attacks`: in
   `add_attack_hit("dcol00", "dcol01", HIT_…, radius, power_rate, MOTION_…, start, end)`, raise
   `power_rate` for more damage, widen `start`–`end` for a longer active window. Again, there's a
   block per weapon model.
4. **The charge attack** starts when the button is held longer than a frame count
   (`get_attack_hold_frames() > 8` in the first hit, `> 12` from standing). Its states are in
   `lib/max_charge_attack.c`, shared with the other Max scripts in `dungeon/act_script/`.
