# Change what an NPC says

The pattern for any "this character should say something else" ticket. How towns pick a line is
described in [talking to people](../README.md#talking-to-people); the other walkthroughs are in
the [tutorials](README.md).

## Change Gordon's early line in Palm Brinks

1. **Find the character's handler.** Search the tree for a phrase from the line, or for the
   character's name in function names (`talk_gordon`). If you only know the map, read its
   `talk_or_dungeon_start` program and follow it into the library.
2. **Find the right branch.** Read the `if` chain on flags and quest state (`talk_gordon` →
   `talk_gordon_early`) until you reach the `show_message` for the case you want.
3. **Check who else uses the code.** The library's top comment lists the scripts that include
   it. `base_town_villagers_v1.c` is included by several Palm Brinks maps, which is usually what
   you want for a villager: their line changes everywhere.
4. **Edit the text in every language copy.** Copy the five libraries into a mod and change the
   literal in each:

   ```text
   dc2tk mod new extracted/scripts gordon-line lib/map/base_town_villagers_v1.c lib/map/base_town_villagers_v2.c lib/map/base_town_villagers_v3.c lib/map/base_town_villagers_v4.c lib/map/base_town_villagers_v5.c
   ```

   Use `\n` for a new line and `<page>` for a page break; the
   [message text codes](../../game/running.md#message-text) list button icons and placeholders.
   Keep lines short enough for the bubble.
5. **If the line is a number, not text** (`show_message(0, 1090010)`), it comes from the message
   file the scene loaded, the event text beside the scripts on the disc, which these tools don't
   convert (a `file:` line in `mod.json` can only replace it whole). Replace the number with your
   own literal string. If it was voiced, also remove or replace the voice call, or the old audio
   plays under your new text. (The menus', shops' and system messages are the other kind of
   message file, `.mes`, which a mod does edit, by `text:` id:
   [message files](../../modding/text.md).)
6. **To change it on one map only**, give that map its own copy of the library first:
   `dc2tk fork extracted/scripts lib/map/base_town_villagers_v1.c lib/mods/gordon/villagers_m01_v1.c map/m/m01_palm_brinks/m01_1.c`.
