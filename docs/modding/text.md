# Text

How a mod edits the menus' item, shop, map and NPC names, their lines and the system messages, as
UTF-8 text. The message files those come from, their codes and what the game reads are in
[message files](../game/text.md); text that scripts show (`show_message`) is written in the
scripts themselves, with the codes in [message text](../game/running.md#message-text). `mod.json`
itself is in [mods](mods.md).

## Editing a message file

The menus' item, shop, map and NPC names, their lines and the system messages are message files
(`.mes`): a table of message ids, each a run of the font's glyphs and layout codes. `dc2tk mod
asset dc2.iso mymod text:en/shopname` writes `text/en/shopname.txt` and its line (ids are `text:`
and the language, `ja` `en` `fr` `de` `it` `es` `ko`, then the file's name: `text:fr/system`,
`text:de/allmenu`); `dc2tk mod asset dc2.iso mymod menu/1/shop.pac.d/shopname.mes --as txt` writes
`shopname.mes.txt` into `disc/` instead. Each message follows a line `@` and its id; line breaks
are the message's, `<page>` a page break, and the icons and slots are written as event text writes
them (`(O)`, `[heart]`, `[Number1]`, `[Item1]`, `[String1]`), other codes in braces (`{wait 12}`,
`{0400}` for one with no name). Change the messages you want and drop the rest; a new `@id` adds a
message. An id reaches every copy of the file in that language, in every version (English
`allmenu.mes` is in 20 menu packs): each gets the messages whose text you changed and keeps its own
others. Unedited text builds back to the disc's own bytes.

A character the language's font lacks is refused (English has ASCII, full-width punctuation and
kana, and up to 32 letters more, which `build` draws into the English font when the mods' English
text uses them: any the French font has, and letters it makes of an ASCII letter and a grave,
acute, circumflex, tilde, diaeresis, ring or cedilla (`ŷ`, `ĉ`, `ş`); French, German, Italian and
Spanish ASCII and their accented letters; Japanese and Korean their fonts' kana, kanji and
Hangul). The added letters in English take a whole cell each (15 pixels, an ASCII letter 8), as
the game spaces every glyph past ASCII, so they stand a little apart from their neighbours.

## Ids

`dc2tk assets` writes one text per name and language to `text/<language>/<name>.txt`, and mods
name them `text:<language>/<name>` (`ja`, `en`, `fr`, `de`, `it`, `es`, `ko`). The language comes
from the `menu/N/` folder or the number in the file name (`system_2.mes`, `help2.mes`,
`d01_2.mes`), and is Japanese for a file with neither; the name is the file name without the
number (`text:fr/system`, `text:de/help`, `text:en/allmenu`).

An id covers every place of its name and language, **every version** of it: English `allmenu.mes`
is in 20 menu packs in 3 versions (the fish selection's and one save menu's are shorter), and one
line changes a message in all of them, each keeping the rest of its own version. The text is the
version found at most places (`versions` in the manifest counts them; the places list it first),
and is the reference a build compares with. The old language folders' files are other ids, with
`@` and their folder (`text:en/system@meswin-usa`, `text:fr/npcmsg@meswin-eu-fra`), as are the
Japanese files beside the game's own (`text:ja/allmenu@menu`). The PAL disc gives 120 ids from 310
files.

## The text form

A message file as text is UTF-8: `#` comment lines, then each message after a line `@` and its
id, in the table's order. The text runs to the next `@id` line; the line break before it, and the
one at the end of the file, are not part of the message. `dc2tk` writes it with LF line breaks and
a comment line naming the id, language and file; it reads CRLF the same (and drops a leading byte
order mark), so a text an editor saved with Windows line endings builds as it was. Ids are 0 to
65535; an id given twice, and text before the first `@id` line that is not a `#` comment or blank,
are refused with the line's number. A message with no text behind its offset (one pointing outside
the messages) is not in the text and keeps its offset. Each code reads as:

| in the text | code |
|---|---|
| a character | the glyph the font draws it with; in the European languages the FFxx and FDEx codes for the letters and symbols ([glyphs](../game/text.md#glyphs-below-0xf000)) |
| a space | FF02 (glyph 0, also a space, is `{0000}`) |
| a line break | FF00 |
| `<page>` | FF03 |
| `(O)` `(X)` `(A)` `(#)` `[l1]` `[select]` `[heart]` ... | the icons, as event text writes them ([message text](../game/running.md#message-text)) |
| `[Number1]` ... `[Number16]`, `[Number]` | number slots (`[数値１]` is read too) |
| `[Item1]` ... `[Item16]` | item slots (`[アイテム１]` is read too) |
| `[String1]` ... `[String16]` | text slots (`[文字列１]` is read too) |
| `[Name1]` ... `[Name6]` | the name registry |
| `{wait N}`, `{colour N}`, `{gap N}`, `{move N}`, `{justify N}`, `{red N}`, `{green N}`, `{blue N}`, `{alpha N}` | FExx, FCxx, F8xx, F9xx, F7xx, F5xx, F4xx, F3xx, F2xx with xx = N (0 to 255) |
| `{voice 0}`, `{voice 1}`, `{voice 2}` | FF05, FF04, FF06 |
| `{XXXX}`, four hex digits | that code as it is: a glyph with no character, or a code with no name |
| `{c}`, one character | the character itself: `{{}` for a brace, `{[}` for a bracket that would start a tag, `{@}` for an `@` starting a line; in the European languages, a letter or symbol written with its glyph number instead of its FFxx or FDEx code (`{é}` is glyph 0x88, `é` code FFE9) |

So a made-up shop list reads:

```
# text:en/shopname: English text from menu/1/shop.pac.d/shopname.mes. ...
@0
Bakery
@1
Lamp Shop [heart]
@2
Press (O) to buy[Item1]
```

Every message of every file reads back to its codes; the decoder escapes what would read
otherwise. On the PAL disc `{XXXX}` is left only in `geomenu` (Japanese numbering), in a debug
menu's leftovers in English `allmenu`, and in the old Japanese `meswin/sysmes0`-`3`; the counts are
in [the text code](../../src/docs/assets/text.md#status).

## Adding and changing messages

Building a text into a message file takes the disc's file as the template and a **reference**,
the file the text was exported from. Each message whose codes differ from the reference's (and
from the template's own) replaces the template's, an id the template lacks is added at the end,
and everything else stays; with nothing changed the template's bytes come back as they are. A
mod's text may keep only the messages it changes, and messages cannot be removed. A character the
font lacks is refused with the message's id and the characters the font has.

## Letters the English font lacks

The English font has room for one more row of glyphs, **32 letters** (288 to 319; [room in the
English font](../game/text.md#glyphs-below-0xf000)). When the English text a build changes uses
letters the table lacks, `build` writes that row with them. Each letter is the European font's own
picture (`FontTex_2_0.tm2`, glyph 0x5E on: `¡ª°º¿`, `À`-`Ý` and `à`-`ÿ` but `Ð Ø Þ ð ø þ`, `Æ æ
ß Œ œ`), or, for a letter it lacks, **composed**: the European font's ASCII letter with a mark
copied off one of its letters, as Unicode decomposes the letter (`ŷ` is `y` and a circumflex). The
marks are the ones its letters have: grave and acute (from `è é`, `È É`), circumflex (`ê Ê`),
tilde (`õ Õ`), diaeresis (`ë Ë`), ring (`å Å`) and cedilla (`ç Ç`). The mark is that letter's ink
above its base letter's rows (below them, for the cedilla), set as far from the new letter's ink
and as far off its middle as it was; a small letter takes the small letter's mark and a capital
the capital's, the other when that does not fit in the cell, and `i` and `j` lose their dot under
a mark. That covers, past Latin-1, `Ć ć Ĉ ĉ Ĝ ĝ Ĥ ĥ Ĩ ĩ Ĵ ĵ Ķ ķ Ĺ ĺ Ļ ļ Ń ń Ņ ņ Ŕ ŕ Ŗ ŗ Ś ś Ŝ
ŝ Ş ş Ţ ţ Ũ ũ Ů ů Ŵ ŵ Ŷ ŷ Ÿ Ź ź`. A letter with a macron, breve, dot above, double acute, caron or
ogonek (`ā ă ċ ő č ą`), `ģ` (its cedilla does not fit under the `g`), and anything else neither
font has (`Ø`, `ł`, `€`) are refused, naming the mark or the character. Every glyph is mapped to
the English palette and centred in its cell, since the engine advances the pen a whole cell for
every English glyph past 93.

**Which letters, which glyphs.** An exported text reads glyphs 288 to 319 as
`¡¿ÀÁÄÇÈÉÑÖÜßàáâäçèéêëíîïñóôöùúûü`, the letters the French, German, Italian and Spanish text uses
most. A build gathers the letters past the table that the messages it changes use, across every
mod together: the `text:en/...` lines' messages whose text differs from the disc's, and the items'
English names and descriptions (`data:items`, written into `system_1.mes` and the English
`allmenu.mes`; see [data tables](data.md)). Each of the 32 above that is used keeps its glyph, and
the other letters take the glyphs of the ones not used, in code order, so the same letters always
get the same glyphs, any 32 letters fit, and text using only the 32 above builds as it always has.
Past 32 the build stops, naming every letter and the lines and mods that use them. A message whose
text is the disc's keeps its codes: it is compared as the export reads it, before the letters are
given glyphs.

English event text never reaches these glyphs: its table's entries 288 to 319 are kana. A text
built by path (`shopname.mes.txt` in `disc/`) cannot use them, as nothing then adds the texture. A
debug menu's leftover Japanese in English `allmenu.mes` uses glyph numbers in this range and so
reads as these letters; left as it is, it keeps its glyph numbers (and draws whatever letters the
build put there, as on the disc it drew nothing), and the letters it reads as count only in a
message a mod changes. Read back from a built disc, the glyphs read as the 32 above whatever the
build drew, as the export has nothing to tell it otherwise.

## Limits

- `meswin/system_N.mes`, `sysmes_N.mes` and `etc/helpN.mes` may not outgrow the buffers the game
  loads them into (0xD000, 0x13880 and 0x1000 bytes; [limits](../game/text.md#limits)); a build
  refuses a file that outgrows its buffer.
- For the menu files and the dungeon file, whose free room is not known, `build` checks nothing
  and only prints each file it replaces with its new size and the old (`replaced: ... N bytes, was
  M`).
- A menu draws only as many lines as its window has room for, and an item name as wide as its
  column. The windows' own widths are set by each menu's code, so `build` warns by the disc's
  text: of a line wider than any line of the file on the disc, and, in menus (all but `sysmes`,
  `npcmsg` and `help`, whose windows grow with their text), of a line more than a cell wider than
  that message's own widest line. How wide a line is follows the engine's rules
  ([limits](../game/text.md#limits)), with `{gap N}` a space N pixels wide and `{move N}` N.
- Glyphs past what the font's texture holds draw nothing; the English text has only the 288
  glyphs of the disc's font and the 32 letters `build` adds (the letters past the table that all
  the mods' English text uses, at most 32), and the European languages the ASCII, accented letters
  and symbols.

## Checking

`dc2tk mod check SOURCE NAME --disc dc2.iso` checks every line resolves ([commands](commands.md)).
`build` refuses, with the message's id or the line's number, a character the font lacks, an id
given twice, a file that outgrows its buffer and more than 32 letters added to the English font;
it warns of a line wider than its window is likely to take, and prints each message file it
replaces with its new size and the old.
