# Text and Menus

`dc2tk setup` exports all menu, shop, and system message files (`.mes`) to `extracted/assets/text/` as UTF-8 `.txt` files.

## Quick steps

1. Copy a text asset into your mod folder (language prefixes: `en`, `fr`, `de`, `it`, `es`):
   ```sh
   dc2tk mod asset dc2.iso my-mod text:en/shopname
   ```
2. Edit `mods/my-mod/text/en/shopname.txt` in a text editor and keep only the `@<id>` entries you want to change.
3. Verify the entry in `mods/my-mod/mod.json`:
   ```json
   {
     "replace": {
       "text:en/shopname": "text/en/shopname.txt"
     }
   }
   ```
4. Build the ISO with `dc2tk build`.

## Where text is stored

| Text Type | Location | How to Edit |
| --- | --- | --- |
| **Menu, shop, and system strings (`.mes`)** | `extracted/assets/text/` | Copy with `dc2tk mod asset dc2.iso <mod> text:<lang>/<name>`. |
| **Item names and descriptions** | `extracted/data/items.json` | Edit `data:items` in JSON (`dc2tk build` updates `.mes` files automatically; see [Game Data](data.md)). |
| **Cutscene and NPC dialogue** | `extracted/scripts/` | Override the script function (see [Change What an NPC Says](../scripting/tutorials/npc-line.md)). |

## The text form

Each entry starts with `@<id>` on its own line, followed by the message lines:

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

The retail English font lacks accented characters (such as `é`, `ü`, or `ō`). When an `en/` text file or item name contains accented characters, `dc2tk build` automatically composes up to 32 accented glyphs into unused slots of the English font texture (`font_e.img`).

See [DC2 Internals: Message Text](../internals/text.md) for `.mes` binary specifications.
