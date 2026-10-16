# Message text: the code

The code behind [text in a mod](../../../docs/modding/text.md); the format it reads is in
[message files](../../../docs/game/text.md).

## Modules

In `src/crates/dc2tk-assets/src/`:

- `text.rs` reads and writes the format and turns it into the text files mods edit. `Mes` is the
  reader and writer; `text::build` builds a text into a message file against the disc's file (the
  template) and the file the text was exported from (the reference); with nothing changed the
  template's bytes come back as they are.
- `font.rs` holds the fonts' numbering and draws the English font's added letters:
  `font::JAPANESE` is the Japanese numbering, a list of characters (the pictures stay on the
  disc); `font::ENGLISH_EXTRA` the 32 letters an export reads glyphs 288 to 319 as;
  `font::english_slots` gives the letters a build uses past the table their glyphs, the 32 above
  keeping theirs and the others taking the unused ones in code order.

## Reading and writing

The reader (`Mes`) cuts the words into spans at the offsets: a span is a message's codes up to its
`FF01`, and the words after them up to the next span (normally `FF01 FF00`; the orphaned messages
ride along). An entry pointing outside the messages keeps its offset. The bytes after the point
`size` gives in the four old Japanese files (`meswin/sysmes0`-`3`) are kept as they are. Written
back unchanged, every file is its own bytes.

A message given new codes gets them in its own span, with `FF01 FF00` after; one sharing its span
with another id, or a new id, gets a span at the end (the last message, when it had no `FF01`, is
given one so it keeps its text). The table's offsets are recomputed. Offsets are 16-bit words from
the table, so `Mes::write` refuses a last message starting past their reach, more than 65535
messages, and (for the four files with bytes after their messages) messages past what `size` can
say. `Mes::parse` refuses a file of an odd size or under 4 bytes, and one whose table runs past
where its messages end, with an error rather than a panic.

## Verification

The corpus tests (`cargo test --release -- --include-ignored corpus` with `DC2TK_CORPUS` set to
the extracted disc) read every message file and write it back, directly and through its text,
byte for byte (`corpus_message_files_round_trip`), and check the Japanese and Korean files read as
text (`corpus_japanese_and_korean_read_as_text`): every glyph the game's Japanese message files
use has its character, and the font pages have ink in exactly those cells. The corpus test
`corpus_letters_compose` draws every composed English letter off the PAL disc's font, and prints
each as text art with `DC2TK_SHOW_LETTERS` set.

## Status

Every message of every file reads back to its codes; the decoder escapes what would read
otherwise. Over the PAL disc's 310 files, the `{XXXX}` left:

| language | before the Japanese and Korean fonts | now | where |
|---|---|---|---|
| Japanese | 529,668 | 152 | the old `meswin/sysmes0`-`3`, data after their messages read as text |
| Korean | 15,149 | 0 | |
| English | 267 | 267 | `geomenu` (249, Japanese numbering) and a debug menu's leftovers in `allmenu` (18) |
| French, German, Italian, Spanish | 952 to 1,035 | 708 each | `geomenu` alone; the rest were glyph-numbered letters, now `{é}` |
