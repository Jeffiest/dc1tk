# Message files (`.mes`)

The menus, shops, maps, items, NPC lines and system messages the game draws outside event scripts
come from **message files**: a table of message ids and, for each, a run of 16-bit codes that are
glyphs of the language's font or layout codes. Event scripts' own text (`.txt` beside the
scripts, quoted in the decompiled source) is a different, cp932 format; the codes here are the
ones its tags turn into (see [message text](running.md#message-text)). Editing the message files
as text is in [text in a mod](../modding/text.md), and the code that reads and writes them in
[the text code](../../src/docs/assets/text.md).

The counts below come from scripts run over every message file of the PAL disc while the format
was worked out.

Evidence:

- **engine**: the decompiled EE code in `finaldc2/ChronicleTwo/ps2`: `src/nd_meswin.cpp`
  (`ClsMes::GetTextLineDataTop`, `GetTextLineDataTop_system`, `SetMesWinTbl`,
  `MakeMesWinTbl_item`, `MyTextureMake_sub`; `MakeMesWinTbl__6ClsMesFi` in the asm),
  `src/font.cpp` (`GetRectFontTex`, `GetFontNo` in the asm, `GetAlphabeticalFontNo_us`,
  `GetFontNoFromFontGaijiCode`, `DrawGaiji_sub`, the tables `FconvCodeTbl`, `FontGaijiConvTbl`,
  `at_1120` in `asm/pal/data/font.data.s`), `src/sysmes.cpp` (`LoadSystemMes`), `src/helpmes.cpp`
  (`LoadHelpMes`), `src/menumain.cpp` (`GetMenuMainMessageBuffer`), `src/mainloop.cpp`
  (`LoadFontTexture`);
- **executable**: the paths the PAL executable (`SCES_511.90`) names;
- **data**: every message file of the PAL disc, and the font textures and tables in `meswin/`.

All values are little-endian.

## Corpus at a glance

| | count |
|---|---|
| message files | 310: 302 `.mes`, and 8 `fconv*.bin` laid out the same way |
| `allmenu.mes` | 129 (one per menu pack, six language folders, and `menu/allmenu.mes`) |
| `systree.mes`, `npcmsg.mes`, `mapname.mes` | 42, 26, 25 |
| `geomenu`, `menumos`, `shopname`, `system`, `sysmes` `.mes` | 6 or 7 each |
| `system_N`, `sysmes_N`, `d01_N`, `helpN`, `hddchkN`, `gyoreN`, `fconvN` | one per language (N = 1 to 6, or 0 to 6) |
| others | `sysmes0`-`3`, `mons_name_0`/`_1`, `geomes0`, `fishing0`, `gyore`, `event/fconv.bin` |
| messages (table entries) | 162,954; at most 989 in a file |
| codes in messages | 5,064,394 |
| files over 64 KB | 100 (the `allmenu.mes` of `menu/1` to `menu/5`, up to 85,544 bytes) |

Codes by kind, over every message's text:

| codes | count |
|---|---|
| glyphs (below 0xF000; at most 1941) | 4,119,187 |
| FF02 space | 533,942 |
| FF00 line break | 205,539 |
| FAEA-FAF9 text slots | 97,408 |
| FFA0-FFFF accented letters | 56,084 |
| item slots | 21,740 |
| number slots | 18,518 |
| FD00-FD31 icons | 9,239 |
| FDE0-FDF7 full-width symbols | 1,479 |
| F8xx gap, FCxx colour, F9xx move | 609, 434, 146 |
| FF03 page break | 69 |

No message uses the name slots, waits, colour components, justification or voice codes; their
meaning is from the engine alone.

## Which files the game reads

The executable names these; each language's file is picked by the `LanguageCode` (0 Japanese, 1
English, 2 French, 3 German, 4 Italian, 5 Spanish; the PAL release plays in 1 to 5).

| file | loaded by | into |
|---|---|---|
| `meswin/system_N.mes` (`system.mes` for Japanese) | `LoadSystemMes` | `SystemMesBuffer`, **0xD000 bytes**: the "system" table item slots and the menus look names up in |
| `meswin/sysmes_N.mes` (`sysmes.mes`) | `LoadSystemMes` | `SysMesBuffer`, **0x13880 bytes** |
| `etc/helpN.mes` | `LoadHelpMes` | `HelpMesBuff`, **0x1000 bytes**; a larger file is not loaded |
| `dungeon/msg/d01_N.mes` | the dungeon's start | the dungeon's heap |
| `title/hddchkN.mes`, `sg/gyo/gyoreN.mes`, `menu/aqua/mes/fconvN.bin` | the title's HDD check, the fish race, its menu | |
| `menu/N/*.pac` members `allmenu`, `systree`, `npcmsg`, `shopname`, `mapname`, `menumos`, `geomenu` | `GetPackFile` by name (`GetMenuMainMessageBuffer` for `allmenu.mes`) | the menu's memory, with the rest of the pack |

No path in the executable reaches the old copies under `meswin/eu/`, `meswin/usa/`,
`meswin/kor/` and `meswin/try/`, nor `meswin/sysmes0`-`3`, `menu/allmenu.mes`,
`etc/geomes0.mes`, `sg/fish/fishing0.mes`, `dungeon/msg/mons_name_*.mes`, `event/fconv.bin` or
`sg/gyo/gyore.mes`. Language 6 is Korean (its files use the 968 glyphs of
`meswin/fonttbl_6.bin`, as `meswin/kor/` does), whatever the debug menu calls it.

## Layout

| word | field |
|---|---|
| 0 | `count`: entries in the table |
| 1 | `size`: bytes of the whole file, **cut to 16 bits** |
| 2 + 2i | entry i's message id |
| 3 + 2i | entry i's offset, in words from word `1 + count` |
| 2 + 2·count | `FF01 FF00` in all 310 files |
| ... | the messages: each its codes, then `FF01 FF00` |

`GetTextLineDataTop(id)` walks the table for the first entry with the id and returns `table + 1 +
count + offset`; the window reads codes from there to the first `FF01`. Nothing reads `size`: in
the 100 files over 64 KB it is the length less 0x10000. Four old Japanese files
(`meswin/sysmes0`-`3`) have 39,200 bytes after the point `size` gives, which no offset reaches.
Offsets are 16-bit words from the table, so the messages can grow to about 128 KB.

- Ids are unique in every file; the table is in id order in 74 files, and in the order the
  messages were written in the rest. Offsets ascend in table order in every file. Two entries
  share one offset once (`mons_name_0.mes`).
- 161,660 messages end `FF01 FF00` before the next one starts. 885 are followed by messages no
  entry points to (left in by the tool that made the files). The last entry of 309 files points
  past the last message, to the file's last line breaks: none (134 files), one `FF00` (159), two
  (6) or five (10), with no `FF01`, so it runs to the end of the file.
- `meswin/eu/fra/system.mes` (an old French file the game does not read) has 5 entries whose
  offsets point past the end, and 86 that point into the middle of another message.

## Codes

### Glyphs (below 0xF000)

A glyph is a number into the language's font texture: 32 glyphs of 16 by 20 pixels a row
(`GetRectFontTex`). The PAL game loads one page: `meswin/FontTex_0.tm2` for Japanese,
`FontTex_1_0.tm2` for English and `FontTex_2_0.tm2` for the others (`LoadFontTexture`).

| language | glyphs | what they draw |
|---|---|---|
| all | 0-93 | printable ASCII from the space, **without the backslash** (`!` is 1, `[` 0x3B, `]` 0x3C, `a` 0x40) |
| English | 94-287 | the English font table `meswin/fonttbl_1.bin` from entry 94 (full-width punctuation, symbols, digits and letters, kana): 288 glyphs is what `FontTex_1_0.tm2` (9 rows) holds |
| French, German, Italian, Spanish | 0x5E-0x9C, 0x9D-0xB4 | the accented letters and the full-width symbols below, which the text mostly writes with their own codes instead |
| Japanese | 94-1943 | the full-width space, punctuation, symbols, digits and letters, kana, ─ ━ Ⅱ Ⅲ Ⅳ, then 1,564 kanji from glyph 380: in Shift-JIS order to glyph 1921 (腕, the last JIS level 1 kanji), then 22 added later in Unicode order (儚 剝 哺 ... 騙) |
| Korean | 94-967 | the Korean font table `meswin/fonttbl_6.bin` from entry 94: 12 symbols in Shift-JIS (‘ ’ “ ” 〔 〕 ° ♪ Ⅱ Ⅲ Ⅳ 象, drawn as empty boxes) and 862 Hangul syllables as their Unicode code, big-endian |

`fonttbl_1.bin` is the table event text is drawn by in English (`LoadFontTblBin`): 4 words (94
half-width glyphs, kanji from 385, 985 codes), then each glyph's big-endian Shift-JIS code in
order, ASCII as the character and a space. The menus' glyph numbers follow it, as the English
texture shows. `fonttbl_6.bin` is laid out the same way (no kanji, 968 codes) and the Korean pages
`FontTex_6_0.tm2`/`_6_1` (each in five weights, `_b`, `_l`, `_m`, `_ms`) draw it: glyph 106 is 가
(U+AC00). The PAL game never loads either (`LoadFontTexture` picks `FontTex_2_0.tm2` for any
language past English, `LoadFontTblBin` `fonttbl_2.bin`).

**Japanese.** The disc has no table of the Japanese glyphs: `fonttbl.bin` holds the 94 ASCII codes
only, and the PAL game loads only page 0 (`FontTex_0.tm2`) of the four the Japanese game drew from
(`FontTex_%d.tm2`, pages of 0x260 glyphs, `GetRectFontTex`): `FontTex_0` to `FontTex_3` are those
pages, with ink in glyphs 95 to 1943. The numbering was read off the pages: every glyph matched
against a system font's drawing of the Shift-JIS characters, aligned in code order (the engine's
`GetFontNo` looks codes up by binary search, so its table was sorted), the text the disc's event
scripts and data files write in Shift-JIS used to settle close calls, and then every glyph checked
by eye; the last 22 are not in code order (characters added after the table was made) and were
read one by one. Every glyph the game's Japanese message files use has its character (shop names
read `ミレーネの武器屋`, `ポリーズベーカリー`), and the pages have ink in exactly those cells.
`geomenu.mes` follows this numbering in every language.

**Room in the English font.** `FontTex_1_0.tm2` is 512 by 192 pixels: 9 rows of glyphs (0 to 287)
and the top 12 pixels of a 10th that the table's next entries (kana) would use, cut off. The game
copies it into `font_buff`, 0xD000 bytes (mainloop.cpp), so it can grow by one row and no more:
200 pixels, 51,328 bytes, **32 glyphs** (288 to 319). The engine advances the pen a whole cell for
every English glyph past 93 (`CheckHalfFont`; the European fonts' letters are half-width there
only because the code says so for languages past English).

`geomenu.mes` is the same file in every language, untranslated: its glyphs follow the Japanese
numbering, so in the other languages they draw other characters.

### Layout codes

| code | meaning |
|---|---|
| FF00 | line break |
| FF01 | end of the message |
| FF02 | space (half a glyph wide, or the `F8xx` gap) |
| FF03 | page break: the window waits for the button |
| FF04, FF05, FF06 | the voice blips as the text appears: type 1, 0, 2 |
| FFA0-FFFF | in French, German, Italian and Spanish, the Latin-1 letter of the low byte (`GetAlphabeticalFontNo_us`: ¡ ª ° º ¿ À-Ï Ñ-Ö Ù-Ý ß à-ï ñ-ö ù-ý ÿ, with 0xBD and 0xBE for Œ and œ), glyph 0x5E on; other codes draw glyph 0 |
| FExx | wait xx frames after the previous glyph |
| FDE0-FDF7 | in the European languages, ○ ● ↑ ↓ ← → ０-９ ： ＋ － ・ “ ” ‘ ’ (glyphs 0x9D-0xB4, `FontGaijiConvTbl`); nothing in English or Japanese |
| FD00-FD31 | icons from the `gaiji` texture: buttons, the D-pad, item kinds, HP and the like (`FconvCodeTbl`; FD1B-FD20 have no tag). Outside Japanese the engine swaps the pictures FD06 and FD08 draw (`DrawGaiji_sub`) |
| FCxx | colour: FC00 the window's own, FC01 dark red (0x8022227F); others leave it |
| FBFF | the window's single number (`ClsMes::value`, beside the 16 slots) |
| FBFA-FBF3, FBE6-FBDF | number slots 1 to 8, 9 to 16 |
| FBFE-FBFB, FBF2-FBE7 | item slots 1 to 4, 5 to 16: the name of the item set in the slot, from the system file |
| FAFA-FAFF | names 1 to 6 from the name registry (names the player gave) |
| FAF9-FAEA | text slots 1 to 16 (`set_message_string`) |
| F9xx | move the pen xx pixels right |
| F8xx | the width of a space, xx pixels |
| F7xx | justify the line to xx·4 pixels (`CalcSpaceW` widens the spaces) |
| F5xx, F4xx, F3xx, F2xx | the colour's red, green, blue, alpha byte |

## Limits

- The game loads `system_N.mes` into 0xD000 bytes, `sysmes_N.mes` into 0x13880 and `helpN.mes`
  into 0x1000. The largest on the disc are 44,118, 28,058 and 1,504 bytes.
- The menu files load with the rest of their pack into the menu's buffer and the dungeon file into
  the dungeon's heap; how much is free there is not known.
- A menu draws only as many lines as its window has room for, and an item name as wide as its
  column. How wide a line is follows `ClsMes::MakeMesWinTbl` (and `GetMesWidth_system`), with a
  cell of 15 pixels (`font_w`; CFont's clearance and the window presets): ASCII and the European
  letters (glyphs 0x5E-0x9C and the FFxx codes) `font_w × 0.55` cut to 8, the space `font_w / 2`
  = 7 (or the `F8xx` gap), any other glyph and the FDEx symbols a cell, an icon its own width (22
  for the circle button, `GaijiDataTbl`), a `F9xx` move its xx; slots count nothing. The
  windows' own widths are set by each menu's code (not traced for every menu). `sysmes`,
  `npcmsg` and `help` windows grow with their text.
- Glyphs past what the font's texture holds draw nothing; the English text has only the 288
  glyphs above, and the European languages the ASCII, accented letters and symbols.
