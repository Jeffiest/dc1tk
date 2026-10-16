# Scripting tutorials

These step-by-step guides show how to write `#pragma override` patches and `mod.json` files for common scripting tasks. Each guide builds on the [Scripting Guide](../README.md).

- [Change what an NPC says](npc-line.md): Override an NPC dialogue function in Palm Brinks across all five PAL languages.
- [Change enemy behavior](enemy-behaviour.md): Patch a monster's attack speed, recovery pause, and hitbox radius with `#pragma target monster`.
- [Add a new enemy to a fight](new-enemy.md): Spawn extra or replacement monsters in a scripted battle and configure the victory trigger.
- [Add a cutscene to a town](cutscene.md): Run a one-time welcome cutscene when the player enters Sindain.
- [Tune the player](tune-player.md): Widen Max's melee combo input windows and adjust attack hitboxes in `mainchr/c01_max.c`.
- [Show live captions during a fight](captions.md): Trigger timed, non-blocking captions during combat with `#pragma every_frame`.
