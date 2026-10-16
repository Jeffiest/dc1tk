# Text and Menus

The game stores text in two places:

1. **Menu, item, shop, and system messages (`.mes` files)**: Exported to `extracted/assets/text/` as UTF-8 `.txt` files.
2. **Cutscene and NPC dialogue**: Embedded directly in the decompiled `.c` scripts in `extracted/scripts/` (see [Change What an NPC Says](../scripting/tutorials/npc-line.md)).

> **Note:** If you only want to rename an item or change its description, edit `data:items` in JSON instead—`dc2tk build` updates all item `.mes` files automatically (see [Game Data](data.md)).

## Edit a menu or system message file

1. Copy the text asset into your mod with `dc2tk mod asset`:
   ```sh
   dc2tk mod asset dc2.iso my-mod text:en/shopname
   ```
   Language prefixes are `en` (English), `fr` (French), `de` (German), `it` (Italian), and `es` (Spanish).
2. Open the exported `.txt` file in your mod folder and edit the messages you want to change. You can delete unchanged `@id` entries from your mod's file so it only contains your edits.

## The text form

Each entry starts with `@<id>` on its own line, followed by the message text:

```text
@100
 Weapon Shop

@101
 Do you want to buy this?[next]
 (X) Yes  (O) No
```

### Formatting tags

| Tag | Meaning |
| --- | --- |
| `<page>` | Wait for a button press and clear the window |
| `[next]` | Wait for a button press without clearing |
| `(O)` `(X)` `(A)` `(#)` | Circle, Cross, Triangle, and Square button icons |
| `[l1]` `[r1]` `[l2]` `[r2]` `[start]` `[select]` | Shoulder, Start, and Select button icons |
| `[heart]` `[hp]` `[mp]` `[weapon]` `[fish]` | Game font status icons |
| `{red}` `{blue}` `{green}` `{yellow}` `{white}` | Change text color |

### Letters the English font lacks

The retail English font lacks accented characters (such as `é`, `ü`, or `ō`). If you use accented characters in an `en/` text file or item name, `dc2tk build` automatically composes up to 32 accented glyphs into unused slots of the English font texture (`font_e.img`).

For binary details on `.mes` files and font tables, see [DC2 Internals: Message Text](../internals/text.md).
