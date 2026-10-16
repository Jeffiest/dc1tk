# Change what an NPC says

This guide explains how to locate and modify an NPC dialogue line in a town script. For background on how town scripts select dialogue lines, see [Talking to people](../README.md#talking-to-people). For other guides, see [Scripting tutorials](README.md).

## Procedure: Change Gordon's early dialogue line in Palm Brinks

1. **Locate the NPC dialogue handler**: Search `extracted/scripts/` for a phrase from the existing dialogue line, or search for the character's name in function identifiers (for example, `talk_gordon`). If you know only the map code, inspect its `talk_or_dungeon_start` (`1000`) program and follow the function call into the included library.
2. **Find the target dialogue branch**: Trace the `if` conditions on save flags and quest state (for example, `talk_gordon` calls `talk_gordon_early`) until you find the `show_message` call for the dialogue line you want to change.
3. **Check which maps include the library**: Read the header comment at the top of the library file to see every map script that includes it. Several Palm Brinks map variants include `lib/map/base_town_villagers_v1.c`, so editing this library updates Gordon's dialogue across all of those maps.
4. **Copy all five language variants into a mod and edit the text**: Run `dc2tk mod new` to copy the five language libraries (`_v1.c` through `_v5.c`) into a new mod:

   ```bash
   dc2tk mod new extracted/scripts gordon-line lib/map/base_town_villagers_v1.c lib/map/base_town_villagers_v2.c lib/map/base_town_villagers_v3.c lib/map/base_town_villagers_v4.c lib/map/base_town_villagers_v5.c
   ```

   Edit the string literal in each of the five files under `mods/gordon-line/files/`. Use `\n` for a line break and `<page>` for a new page. For button icons and formatting codes, see [message text codes](../../internals/running.md#message-text). Keep lines short enough to fit inside the speech bubble.
5. **Replace numeric message IDs with string literals (if applicable)**: If the `show_message` call passes an integer ID instead of a string literal (for example, `show_message(0, 1090010)`), the engine reads the text from an external event message file loaded by the scene. Replace the integer ID with your own string literal (`show_message(0, "Your new text")`). If the original line played voice audio, remove or replace the voice playback call so the original audio does not play over your new text. (Note: menu, shop, and system `.mes` files use a different system and can be edited directly via `text:` entries in `mod.json`; see [message files](../../guide/text.md).)
6. **Restrict the change to a single map (optional)**: If you want the change to apply to only one map (`m01_1.c`) without affecting other maps that share the library, fork the library for that map first:

   ```bash
   dc2tk fork extracted/scripts lib/map/base_town_villagers_v1.c lib/mods/gordon/villagers_m01_v1.c map/m/m01_palm_brinks/m01_1.c
   ```
