# Text

This guide explains how to edit menu text—including item, shop, map, and NPC names, descriptions, and system messages—as UTF-8 text files.

For details on the binary `.mes` format, control codes, and engine behavior, see [message files](../internals/text.md). Dialogue text shown by scripts (`show_message`) is stored directly inside the script files using the tags described in [message text](../internals/running.md#message-text). For `mod.json` syntax, see [mods](mods.md).

## Editing a message file

Menu names, descriptions, NPC lines, and system messages are stored in message files (`.mes`). Each `.mes` file contains a table of numeric message IDs mapped to sequences of font glyph codes and layout control codes.

To edit a message file in a mod:

1. Export the message file by its asset ID:
   ```sh
   dc2tk mod asset dc2.iso mymod text:en/shopname
   ```
   This command writes `text/en/shopname.txt` and adds the `"text:en/shopname"` entry to `mod.json`.
   (Asset IDs use the format `text:<language>/<name>`, where `<language>` is `ja`, `en`, `fr`, `de`, `it`, `es`, or `ko`, followed by the file stem, such as `text:fr/system` or `text:de/allmenu`. Alternatively, run `dc2tk mod asset dc2.iso mymod menu/1/shop.pac.d/shopname.mes --as txt` to write `shopname.mes.txt` into `disc/`.)
2. Open the `.txt` file in any UTF-8 text editor. Each message follows a header line with `@` and its numeric ID (for example, `@0`).
3. Edit the messages you want to change. You can delete unchanged messages from the `.txt` file, or leave them in place. To add a new message, add a new `@<id>` header and its text.
4. Run `dc2tk build`.

### Formatting and multi-file updates

- **Line and page breaks**: Line breaks inside a message block become line breaks in the game. Write `<page>` for a page break.
- **Icons and variables**: Write button icons and variable slots using the same tags as event script text: `(O)`, `[heart]`, `[Number1]`, `[Item1]`, `[String1]`. Write other control codes in braces, such as `{wait 12}`, or `{0400}` for a raw hex code.
- **Updating all copies on the disc**: Replacing a `text:` ID updates every copy and version of that file for that language across the entire disc. For example, English `allmenu.mes` appears in 20 menu packs; each copy receives the messages you changed and keeps its own remaining messages. Unedited text builds back to the exact bytes from the disc.
- **Supported characters**: `dc2tk build` rejects any character that the target language's font does not support:
  - **English**: Supports ASCII, full-width punctuation and kana, plus up to 32 additional accented letters that `dc2tk build` automatically draws into the English font when your mod's English text uses them. These 32 letters can be any letter from the French font or any letter composed from an ASCII letter and a grave, acute, circumflex, tilde, diaeresis, ring, or cedilla mark (such as `ŷ`, `ĉ`, or `ş`). Because the engine advances 15 pixels (a full cell) for every glyph code above ASCII (compared to 8 pixels for ASCII letters), added accented letters in English appear slightly spaced apart from neighboring letters.
  - **French, German, Italian, and Spanish**: Support ASCII and the accented letters and symbols in the European font.
  - **Japanese and Korean**: Support their fonts' kana, kanji, and Hangul.

## Ids

`dc2tk assets` exports one text file per file stem and language to `text/<language>/<name>.txt`. Mods reference these assets as `text:<language>/<name>` (`ja`, `en`, `fr`, `de`, `it`, `es`, `ko`).

`dc2tk` determines the language from the `menu/N/` folder number or the language number in the filename (`system_2.mes`, `help2.mes`, `d01_2.mes`). Files with neither are Japanese (`ja`). The `<name>` is the filename stem without the language number (`text:fr/system`, `text:de/help`, `text:en/allmenu`).

Each `text:` ID covers every location of that name and language on the disc, across **every version** of the file. For example, English `allmenu.mes` appears in 20 menu packs across 3 slightly different versions (the fish selection menu and one save menu have shorter tables). A single `text:en/allmenu` entry updates your changed messages across all 20 packs while each pack keeps the rest of its own version. The exported `.txt` file uses the most common version on the disc (`versions` in the asset manifest counts them, and the `places` list puts the most common version first), which `dc2tk build` also uses as the reference to detect which messages you edited.

Unused legacy files in old language folders have separate IDs with an `@<folder>` suffix (such as `text:en/system@meswin-usa` and `text:fr/npcmsg@meswin-eu-fra`), as do the unused Japanese files stored alongside the European menu files (`text:ja/allmenu@menu`). On the PAL disc, 310 `.mes` files map to 120 `text:` asset IDs.

## The text form

An exported message text file is a UTF-8 file containing `#` comment lines followed by `@<id>` message blocks in table order. Each message's text runs from the line after `@<id>` up to the next `@<id>` line. The line break immediately before the next `@<id>` line and the final line break at the end of the file are not part of the message.

`dc2tk` exports files with LF (`\n`) line breaks and a top comment naming the asset ID, language, and source path. When reading files back, `dc2tk` accepts both LF and CRLF (`\r\n`) line endings and strips any leading UTF-8 byte order mark (BOM), so files saved with Windows line endings build identically.

Message IDs must be between `0` and `65535`. `dc2tk build` rejects duplicate `@<id>` entries or any non-comment, non-blank text before the first `@<id>` line, reporting the line number. If a message entry on the disc has an offset pointing outside the message data (no text behind its offset), `dc2tk` omits it from the `.txt` file and preserves its original offset on build.

### Control codes and tags

| In the text file | Compiled `.mes` code |
|---|---|
| A character | The font glyph code for that character; in European languages, the `FFxx` and `FDEx` codes for accented letters and symbols ([glyphs](../internals/text.md#glyphs-below-0xf000)) |
| A space | `FF02` (glyph `0`, which is also a space, is written `{0000}`) |
| A line break | `FF00` |
| `<page>` | `FF03` |
| `(O)` `(X)` `(A)` `(#)` `[l1]` `[select]` `[heart]` ... | Button and symbol icons, written the same way as event script text ([message text](../internals/running.md#message-text)) |
| `[Number1]` ... `[Number16]`, `[Number]` | Number variable slots (`[数値１]` is also accepted) |
| `[Item1]` ... `[Item16]` | Item name slots (`[アイテム１]` is also accepted) |
| `[String1]` ... `[String16]` | Text variable slots (`[文字列１]` is also accepted) |
| `[Name1]` ... `[Name6]` | Character name registry slots |
| `{wait N}`, `{colour N}`, `{gap N}`, `{move N}`, `{justify N}`, `{red N}`, `{green N}`, `{blue N}`, `{alpha N}` | `FExx`, `FCxx`, `F8xx`, `F9xx`, `F7xx`, `F5xx`, `F4xx`, `F3xx`, `F2xx` with `xx = N` (`0` to `255`) |
| `{voice 0}`, `{voice 1}`, `{voice 2}` | `FF05`, `FF04`, `FF06` |
| `{XXXX}` (four hex digits) | That exact 16-bit code, used for a glyph with no character mapping or an unnamed control code |
| `{c}` (one character) | Escapes a literal character: `{{}` for `{`, `{[}` for `[` when it would start a tag, `{@}` for `@` at the start of a line; in European languages, writes a letter or symbol using its direct glyph index instead of its `FFxx` or `FDEx` code (for example, `{é}` writes glyph `0x88`, whereas `é` writes `FFE9`) |

For example, a shop name file looks like this:

```text
# text:en/shopname: English text from menu/1/shop.pac.d/shopname.mes. ...
@0
Bakery
@1
Lamp Shop [heart]
@2
Press (O) to buy[Item1]
```

Every message of every file on the disc round-trips back to its exact original codes, and the exporter automatically escapes any text that would otherwise be read as a tag. On the PAL disc, raw `{XXXX}` hex codes appear only in `geomenu` (Japanese numbering), in unused debug menu leftovers in English `allmenu`, and in the old Japanese `meswin/sysmes0`–`3` files; see [the text code](../../src/docs/assets/text.md#status) for counts.

## Adding and changing messages

When `dc2tk build` compiles a `.txt` file into a `.mes` file, it compares your text against a **reference** (the file from which the text was exported) and applies changes to the **template** (the target `.mes` file on the disc):

- Each message whose compiled codes differ from both the reference and the template replaces that message in the template.
- Any `@<id>` that the template does not have is appended to the end of the message table.
- All other messages in the template remain unchanged. If no messages changed, `dc2tk build` outputs the template's exact original bytes.

Because unchanged messages stay intact, your mod's `.txt` file only needs to contain the `@<id>` blocks that you change or add. Existing messages cannot be deleted from the table. If a message contains a character that the font lacks, `dc2tk build` fails and reports the message ID along with the characters supported by the font.

## Letters the English font lacks

The English font texture has room for one additional row of glyphs: **32 letters** at glyph indices `288` to `319` (see [room in the English font](../internals/text.md#glyphs-below-0xf000)). When the English text modified by a build uses accented letters or symbols outside the standard English table, `dc2tk build` populates that row automatically:

- **European font glyphs**: If the character exists in the European font (`FontTex_2_0.tm2`, glyph `0x5E` onward: `¡ª°º¿`, `À`–`Ý` and `à`–`ÿ` except `Ð Ø Þ ð ø þ`, plus `Æ æ ß Œ œ`), `dc2tk build` copies that glyph image directly.
- **Composed glyphs**: If the European font lacks the character, `dc2tk build` composes it from the European font's ASCII base letter and a diacritic mark copied from another letter, following Unicode decomposition (for example, `ŷ` combines `y` and a circumflex):
  - Grave and acute (copied from `è é`, `È É`)
  - Circumflex (copied from `ê Ê`)
  - Tilde (copied from `õ Õ`)
  - Diaeresis (copied from `ë Ë`)
  - Ring above (copied from `å Å`)
  - Cedilla (copied from `ç Ç`)
  `dc2tk build` copies the mark's pixels above the donor letter's base rows (or below them for the cedilla) and places the mark at the same vertical distance and horizontal offset from the new base letter's ink. Lowercase letters take the lowercase mark and uppercase letters take the uppercase mark (or fall back to the other if the mark does not fit in the cell). Lowercase `i` and `j` lose their dot under a top mark.
- **Supported composed letters beyond Latin-1**: `Ć ć Ĉ ĉ Ĝ ĝ Ĥ ĥ Ĩ ĩ Ĵ ĵ Ķ ķ Ĺ ĺ Ļ ļ Ń ń Ņ ņ Ŕ ŕ Ŗ ŗ Ś ś Ŝ ŝ Ş ş Ţ ţ Ũ ũ Ů ů Ŵ ŵ Ŷ ŷ Ÿ Ź ź`.
- **Refused characters**: Letters with a macron, breve, dot above, double acute, caron, or ogonek (`ā ă ċ ő č ą`), `ģ` (its cedilla does not fit under `g`), and any other character absent from both fonts (`Ø`, `ł`, `€`) are rejected with an error naming the mark or character.
- **Palette and spacing**: Every added glyph is mapped to the English palette and centered horizontally in its cell, because the game engine advances the cursor by a full cell width (15 pixels) for every English glyph index above `93`.

### Which letters get which glyph slots

By default, exporting English text decodes glyph indices `288` to `319` as the 32 most common French, German, Italian, and Spanish letters:

```text
¡¿ÀÁÄÇÈÉÑÖÜßàáâäçèéêëíîïñóôöùúûü
```

During a build, `dc2tk build` collects all extra English letters used across every active mod: both changed messages in `text:en/...` files and English item names and descriptions in `data:items` (which are written into `system_1.mes` and English `allmenu.mes`; see [data tables](data.md)).

- Any of the 32 default letters above that are used keep their default glyph index.
- Other extra letters take the remaining unused glyph slots in Unicode code point order. This ensures that the same letters always receive the same glyph indices, any combination of up to 32 extra letters fits, and text using only the 32 default letters builds with identical codes.
- If all active mods together use more than 32 extra letters in English, `dc2tk build` stops with an error naming every extra letter and the lines and mods that use them.
- Unedited messages keep their original disc codes: `dc2tk build` compares messages using the default export mapping before assigning custom glyph slots.

### Notes on English extra glyphs

- **Event script text**: English dialogue in event scripts (`show_message`) cannot use glyphs `288` to `319` because those entries in the event text table map to kana.
- **Path-based builds**: A text file built by raw disc path (such as `shopname.mes.txt` in `disc/`) cannot use these extra letters because path builds do not update the font texture.
- **Debug leftovers in `allmenu.mes`**: Unused Japanese debug strings in English `allmenu.mes` contain glyph indices in the `288`–`319` range, so they export as characters from the 32 default letters above. If left unedited, they keep their original glyph numbers (drawing whatever letters the build places in those slots, whereas on the retail disc they drew nothing), and their characters only count toward the 32-letter limit if a mod edits those debug messages.
- **Re-exporting from a built disc**: Exporting English text from an already-built ISO always decodes glyphs `288` to `319` as the 32 default letters above regardless of which letters `dc2tk build` drew into the font texture, because the `.mes` file does not store the Unicode mapping.

## Limits

- **Fixed buffer limits**: `meswin/system_N.mes`, `sysmes_N.mes`, and `etc/helpN.mes` must not outgrow the fixed memory buffers that the game loads them into (`0xD000`, `0x13880`, and `0x1000` bytes respectively; see [limits](../internals/text.md#limits)). `dc2tk build` refuses any file that exceeds its buffer.
- **Menu and dungeon message files**: The exact free space for menu pack `.mes` files and the dungeon `.mes` file is not known. `dc2tk build` does not enforce a size cap on them, and prints each replaced file with its new and old byte sizes (`replaced: ... N bytes, was M`).
- **Line width warnings**: Menu windows draw only as many lines as fit vertically, and item names must fit within their column width. Because window widths are set in each menu's compiled code, `dc2tk build` checks line widths against the original disc text and warns when:
  - A line is wider than any line in that file on the disc, or
  - In fixed-width menus (all files except `sysmes`, `npcmsg`, and `help`, whose windows grow to fit their text), a line is more than one cell wider than that message's own widest line on the disc.
  Line width calculations follow the engine's spacing rules ([limits](../internals/text.md#limits)), counting `{gap N}` and `{move N}` as `N` pixels wide.
- **Font table limits**: Glyph codes past the end of the font texture draw nothing. English text has only the 288 glyphs on the disc plus up to 32 extra letters added by `dc2tk build`. European languages have only the ASCII characters, accented letters, and symbols in their font table.

## Checking

- Run `dc2tk mod check SOURCE NAME --disc dc2.iso` to verify that every entry in `mod.json` resolves and compiles cleanly (see [commands](commands.md)).
- `dc2tk build` refuses (with the message ID or line number) any unsupported character, duplicate message ID, buffer overflow, or set of more than 32 extra English letters. It also warns when a line is likely too wide for its window and prints the new and old byte sizes of every replaced message file.
