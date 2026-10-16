# Message files (`.mes`)

All UI text drawn outside event scripts—including menus, shop lists, map names, item names and descriptions, townsperson dialogue, and system prompts—comes from binary **message files** (`.mes`). Each `.mes` file begins with an index table of message IDs, followed by streams of 16-bit codes that encode either font glyph indices or layout control commands.

Event scripts store their own dialogue in a separate CP932 `.txt` text format (shown inline in decompiled `.script` files). At runtime, the event text parser converts `[tag]` commands into the exact same 16-bit control codes documented here (see [message text](running.md#message-text)). For a practical guide to editing `.mes` files as plain text, see [text in a mod](../guide/text.md). For details on the `dc2tk` message converter implementation, see [the text code](../../src/docs/assets/text.md).

**Evidence sources**:
- **Engine**: Decompiled Emotion Engine (EE) code in `finaldc2/ChronicleTwo/ps2`:
  - `src/nd_meswin.cpp` (`ClsMes::GetTextLineDataTop`, `GetTextLineDataTop_system`, `SetMesWinTbl`, `MakeMesWinTbl_item`, `MyTextureMake_sub`, and the assembly of `MakeMesWinTbl__6ClsMesFi`)
  - `src/font.cpp` (`GetRectFontTex`, `GetFontNo` in assembly, `GetAlphabeticalFontNo_us`, `GetFontNoFromFontGaijiCode`, `DrawGaiji_sub`, and the lookup tables `FconvCodeTbl`, `FontGaijiConvTbl`, and `at_1120` in `asm/pal/data/font.data.s`)
  - `src/sysmes.cpp` (`LoadSystemMes`), `src/helpmes.cpp` (`LoadHelpMes`), `src/menumain.cpp` (`GetMenuMainMessageBuffer`), and `src/mainloop.cpp` (`LoadFontTexture`)
- **Executable**: File paths referenced in the PAL executable (`SCES_511.90`).
- **Data**: Corpus statistics across all 310 `.mes` and `fconv*.bin` files on the PAL disc, plus the font textures and mapping tables in `meswin/`.

All multi-byte values in `.mes` and `fonttbl*.bin` files are little-endian unless noted otherwise.

## Corpus at a glance

| Category | Count |
|---|---|
| Total message files | 310 (302 `.mes` files and 8 `fconv*.bin` files that use the identical binary format) |
| `allmenu.mes` | 129 (one per menu pack across six language folders, plus `menu/allmenu.mes`) |
| `systree.mes`, `npcmsg.mes`, `mapname.mes` | 42, 26, 25 |
| `geomenu.mes`, `menumos.mes`, `shopname.mes`, `system.mes`, `sysmes.mes` | 6 or 7 each |
| `system_N`, `sysmes_N`, `d01_N`, `helpN`, `hddchkN`, `gyoreN`, `fconvN` | One per language (`N = 1..6` or `0..6`) |
| Standalone / legacy files | `sysmes0`–`3`, `mons_name_0` / `_1`, `geomes0`, `fishing0`, `gyore`, `event/fconv.bin` |
| Total messages (table entries) | 162,954 (at most 989 entries in a single file) |
| Total 16-bit codes across all messages | 5,064,394 |
| Files larger than 64 KB | 100 (the `allmenu.mes` files inside `menu/1/` through `menu/5/`, up to 85,544 bytes) |

Breakdown of the 5,064,394 16-bit codes across all message bodies on the disc:

| Code Range | Meaning | Corpus Count |
|---|---|---|
| `0x0000..0xEFFF` | Font glyph indices (highest used on the disc is `1941`) | 4,119,187 |
| `0xFF02` | Space character | 533,942 |
| `0xFF00` | Line break | 205,539 |
| `0xFAEA..0xFAF9` | Dynamic text string slots (`1..16`) | 97,408 |
| `0xFFA0..0xFFFF` | European accented Latin-1 letters | 56,084 |
| `0xFBE7..0xFBF2`, `0xFBFB..0xFBFE` | Dynamic item-name slots (`1..16`) | 21,740 |
| `0xFBDF..0xFBE6`, `0xFBF3..0xFBFA`, `0xFBFF` | Dynamic number slots (`1..16` and single value) | 18,518 |
| `0xFD00..0xFD31` | Controller button and UI icons (`gaiji`) | 9,239 |
| `0xFDE0..0xFDF7` | European full-width symbols | 1,479 |
| `0xF8xx`, `0xFCxx`, `0xF9xx` | Space width (`609`), preset colour (`434`), horizontal pen offset (`146`) | 609, 434, 146 |
| `0xFF03` | Page break (wait for button press) | 69 |

No `.mes` file on the disc uses the player-renamed character slots (`0xFAFA..0xFAFF`), frame delays (`0xFExx`), per-channel RGBA colour codes (`0xF2xx..0xF5xx`), line justification (`0xF7xx`), or voice-blip codes (`0xFF04..0xFF06`). Their behaviour is documented below from the engine code.

## Which files the game reads

The engine selects localized `.mes` files using the global `LanguageCode` (`0` = Japanese, `1` = English, `2` = French, `3` = German, `4` = Italian, `5` = Spanish; the retail PAL release supports languages `1..5`):

| File Path | Loaded By | Target Memory Buffer |
|---|---|---|
| `meswin/system_N.mes` (`system.mes` when `N = 0`) | `LoadSystemMes` | `SystemMesBuffer` (**`0xD000` bytes** / 53,248 bytes). Holds item names and shared system strings referenced by item slots and menus. |
| `meswin/sysmes_N.mes` (`sysmes.mes` when `N = 0`) | `LoadSystemMes` | `SysMesBuffer` (**`0x13880` bytes** / 80,000 bytes). |
| `etc/helpN.mes` | `LoadHelpMes` | `HelpMesBuff` (**`0x1000` bytes** / 4,096 bytes; files larger than `0x1000` bytes are rejected). |
| `dungeon/msg/d01_N.mes` | Dungeon initialisation | Allocated on the dungeon heap. |
| `title/hddchkN.mes`, `sg/gyo/gyoreN.mes`, `menu/aqua/mes/fconvN.bin` | Title HDD check, fish race, aquarium menu | Scene-local buffers. |
| `menu/N/*.pac` members: `allmenu.mes`, `systree.mes`, `npcmsg.mes`, `shopname.mes`, `mapname.mes`, `menumos.mes`, `geomenu.mes` | `GetPackFile` by member name (`GetMenuMainMessageBuffer` for `allmenu.mes`) | Loaded into the menu buffer alongside the rest of the `.pac` archive. |

No code path in `SCES_511.90` ever loads the legacy subdirectories `meswin/eu/`, `meswin/usa/`, `meswin/kor/`, or `meswin/try/`, nor does it load `meswin/sysmes0`–`3.mes`, `menu/allmenu.mes`, `etc/geomes0.mes`, `sg/fish/fishing0.mes`, `dungeon/msg/mons_name_*.mes`, `event/fconv.bin`, or `sg/gyo/gyore.mes`. Language code `6` is Korean: its `.mes` files use the 968-glyph encoding of `meswin/fonttbl_6.bin` (matching `meswin/kor/`), regardless of the label shown in the debug menu.

## Layout

A `.mes` file is an array of 16-bit unsigned words (`u16`):

| Word Index | Field |
|---|---|
| `0` | `count`: Number of entries in the index table. |
| `1` | `size`: Total byte size of the file, **truncated to 16 bits** (`file_size & 0xFFFF`). |
| `2 + 2 * i` | `id`: Message ID of entry `i`. |
| `3 + 2 * i` | `offset`: Word offset of entry `i`'s text relative to word `1 + count`. |
| `2 + 2 * count` | Leading separator before the first message (`0xFF01, 0xFF00` in all 310 files). |
| `4 + 2 * count ..` | Message payloads: each message consists of its 16-bit codes followed by `0xFF01, 0xFF00`. |

When looking up a message, `GetTextLineDataTop(id)` linearly scans the index table for the first entry matching `id` and returns a pointer to word `(1 + count) + offset`. The message window then reads 16-bit codes starting at that address until it hits `0xFF01` (end of message).

The engine never reads the `size` word at index `1`. In the 100 files larger than 64 KB, `size` simply wraps around (`file_size - 0x10000`). Four legacy Japanese files (`meswin/sysmes0`–`3.mes`) contain 39,200 bytes of unreferenced trailing data past the byte count indicated by `size`. Because `offset` is an unsigned 16-bit word offset measured from word `1 + count`, the message payload area can grow up to approximately 128 KB (`65,535 * 2` bytes).

**Corpus structural properties**:
- **IDs and ordering**: Message IDs are unique within every file. The index table is sorted by ascending `id` in 74 files and kept in authoring order in the remaining 236 files. Entry `offset` values strictly increase in table order across all files (with one duplicate offset shared by two entries in the unused `mons_name_0.mes`).
- **Terminators and unreferenced messages**: 161,660 messages end with `0xFF01, 0xFF00` immediately before the next indexed message. Another 885 indexed messages are followed by orphaned, unindexed message strings left behind by the exporter tool. In 309 of the 310 files, the final table entry points past the last `0xFF01, 0xFF00` pair to trailing `0xFF00` line-break words at the very end of the file (no `0xFF00` in 134 files, one `0xFF00` in 159 files, two in 6 files, and five in 10 files) with no closing `0xFF01`.
- **Corrupted legacy file**: The unused file `meswin/eu/fra/system.mes` contains 5 table entries whose offsets point past the end of the file and 86 entries that point into the middle of an earlier message.

## Codes

### Glyphs (below 0xF000)

Any code below `0xF000` is a direct glyph index into the active language's font texture. Each font texture page arranges glyphs in a grid of 32 columns per row, with each glyph cell measuring 16×20 pixels (`GetRectFontTex`). At boot, `LoadFontTexture` loads a single font texture page in the PAL release:
- Language `0` (Japanese): `meswin/FontTex_0.tm2`
- Language `1` (English): `meswin/FontTex_1_0.tm2`
- Languages `2..5` (French, German, Italian, Spanish): `meswin/FontTex_2_0.tm2`

| Language | Glyph Range | Characters Drawn |
|---|---|---|
| All languages | `0..93` | Printable ASCII starting from space (`0` = space, `1` = `!`, `0x3B` = `[`, `0x3C` = `]`, `0x40` = `a`), **omitting the backslash (`\`)**. |
| English (`1`) | `94..287` | Entries `94..287` of `meswin/fonttbl_1.bin` (full-width punctuation, symbols, numerals, Latin letters, and kana). `FontTex_1_0.tm2` (9 rows of 32 cells) holds exactly these 288 glyphs (`0..287`). |
| French, German, Italian, Spanish (`2..5`) | `0x5E..0x9C`, `0x9D..0xB4` | Accented Latin-1 letters (`0x5E..0x9C`) and full-width symbols (`0x9D..0xB4`). In practice, `.mes` files usually encode these using the `0xFFA0..0xFFFF` and `0xFDE0..0xFDF7` control codes below rather than raw glyph indices. |
| Japanese (`0`) | `94..1943` | Full-width space, punctuation, symbols, numerals, Latin letters, hiragana, katakana, `─ ━ Ⅱ Ⅲ Ⅳ`, and 1,564 kanji starting at glyph `380`. Glyphs `380..1921` follow Shift-JIS code order up to `腕` (the last JIS X 0208 Level 1 kanji), followed by 22 additional kanji (`1922..1943`) appended in Unicode order (`儚 剝 哺 ... 騙`). |
| Korean (`6`, unused) | `94..967` | Entries `94..967` of `meswin/fonttbl_6.bin`: 12 Shift-JIS symbols (`‘ ’ “ ” 〔 〕 ° ♪ Ⅱ Ⅲ Ⅳ 象`, rendered as empty boxes in the texture) followed by 862 Hangul syllables sorted by big-endian Unicode code point (glyph `106` is `가`, `U+AC00`). |

**Font encoding tables (`fonttbl_*.bin`)**:
When rendering event script text in English, `LoadFontTblBin` loads `meswin/fonttbl_1.bin`. This file begins with four 16-bit header words (`94` half-width glyphs, kanji start index `385`, total count `985`, `0`), followed by each glyph's big-endian Shift-JIS code (with half-width ASCII entries padded with a trailing space byte `0x20`). English `.mes` files use the exact same glyph numbering. Similarly, `meswin/fonttbl_6.bin` (0 kanji, 968 entries) maps the Korean font textures `FontTex_6_0.tm2` and `FontTex_6_1.tm2` (stored in five weight variants: base, `_b`, `_l`, `_m`, `_ms`). In the PAL executable, `LoadFontTexture` always selects `FontTex_2_0.tm2` and `LoadFontTblBin` always selects `fonttbl_2.bin` for every language code above `1`, so the Korean assets are never loaded.

**Japanese glyph mapping**:
The PAL disc does not include the full Japanese `fonttbl.bin` (the file `meswin/fonttbl.bin` contains only the 94 ASCII entries), and the PAL executable loads only page 0 (`FontTex_0.tm2`) of the four Japanese font pages (`FontTex_0.tm2` through `FontTex_3.tm2`, each holding `0x260` = 608 glyphs, indexed by `GetRectFontTex`). However, all four `FontTex_0..3.tm2` textures remain on the PAL disc and contain rendered glyphs across cells `0..1943`. Because `GetFontNo` binary-searches the font table by character code, glyphs `94..1921` are strictly ordered by Shift-JIS code; the complete table was reconstructed and visually verified cell-by-cell against the four texture pages and Shift-JIS strings in event scripts (confirming Japanese `.mes` strings such as `ミレーネの武器屋` and `ポリーズベーカリー`). Note that `geomenu.mes` was left untranslated in all language folders and uses Japanese glyph indices in every language.

**Expanding the English font texture**:
`FontTex_1_0.tm2` is a 512×192 image containing 9 full rows of 32 glyphs (`0..287`) plus the top 12 pixels of a 10th row (clipped kana glyphs). At boot, `mainloop.cpp` copies `FontTex_1_0.tm2` into `font_buff`, a fixed buffer of `0xD000` bytes (53,248 bytes). Because a 512×200 `PSMT4` TIM2 file is 51,328 bytes, `FontTex_1_0.tm2` can grow by **at most 1 full row (8 pixels taller, to 512×200)**, adding **32 extra glyphs (`288..319`)** within `font_buff`. Note that `CheckHalfFont` advances the cursor by a full 15-pixel cell for every English glyph above `93` (European glyphs above `93` advance by half a cell only because `CheckHalfFont` branches on `LanguageCode > 1`).

### Layout codes

| Code | Meaning |
|---|---|
| `0xFF00` | Line break. |
| `0xFF01` | End of message. |
| `0xFF02` | Space character (advances by `font_w / 2`, or by the custom width set via `0xF8xx`). |
| `0xFF03` | Page break (pauses text output and waits for a button press). |
| `0xFF04`, `0xFF05`, `0xFF06` | Set character voice-blip sound during text reveal to type `1`, `0`, or `2`. |
| `0xFFA0..0xFFFF` | In French, German, Italian, and Spanish (`LanguageCode` `2..5`), draws the Latin-1 accented character corresponding to the low byte (`GetAlphabeticalFontNo_us`: `¡ ª ° º ¿ À–Ï Ñ–Ö Ù–Ý ß à–ï ñ–ö ù–ý ÿ`, with `0xBD` and `0xBE` mapped to `Œ` and `œ`), selecting glyphs `0x5E` upward. Unmapped codes draw glyph `0` (space). |
| `0xFExx` | Pause text reveal for `xx` frames after the preceding glyph. |
| `0xFDE0..0xFDF7` | In European languages (`2..5`), draws full-width symbols `○ ● ↑ ↓ ← → ０–９ ： ＋ － ・ “ ” ‘ ’` (mapped to glyphs `0x9D..0xB4` via `FontGaijiConvTbl`). Draws nothing in English or Japanese. |
| `0xFD00..0xFD31` | Draws a controller button or UI icon from the `gaiji` texture (`FconvCodeTbl`; `0xFD1B..0xFD20` have no text tag). Outside Japanese (`LanguageCode != 0`), `DrawGaiji_sub` swaps the icons drawn by `0xFD06` (Circle) and `0xFD08` (Cross). |
| `0xFCxx` | Select preset text colour: `0xFC00` restores the window's default colour, `0xFC01` selects dark red (`0x8022227F`), and other values leave the colour unchanged. |
| `0xFBFF` | Inserts the window's standalone numeric value (`ClsMes::value`, separate from the 16 number slots). |
| `0xFBFA..0xFBF3`, `0xFBE6..0xFBDF` | Inserts dynamic number slot `1..8` (`0xFBFA` down to `0xFBF3`) or `9..16` (`0xFBE6` down to `0xFBDF`). |
| `0xFBFE..0xFBFB`, `0xFBF2..0xFBE7` | Inserts dynamic item-name slot `1..4` (`0xFBFE` down to `0xFBFB`) or `5..16` (`0xFBF2` down to `0xFBE7`), looking up the item name in `system_N.mes`. |
| `0xFAFA..0xFAFF` | Inserts player-customised name `1..6` from the name registry. |
| `0xFAF9..0xFAEA` | Inserts dynamic string slot `1..16` (`0xFAF9` down to `0xFAEA`, set via `set_message_string`). |
| `0xF9xx` | Shift the horizontal pen position right by `xx` pixels. |
| `0xF8xx` | Set the pixel width of subsequent `0xFF02` spaces to `xx` pixels. |
| `0xF7xx` | Justify the current line to `xx * 4` pixels (`CalcSpaceW` expands spaces to fit). |
| `0xF5xx`, `0xF4xx`, `0xF3xx`, `0xF2xx` | Set the red (`0xF5xx`), green (`0xF4xx`), blue (`0xF3xx`), or alpha (`0xF2xx`) component of the text colour. |

## Limits

- **Fixed global message buffers**:
  - `meswin/system_N.mes` must fit in **`0xD000` bytes** (53,248 bytes; largest on the disc is 44,118 bytes).
  - `meswin/sysmes_N.mes` must fit in **`0x13880` bytes** (80,000 bytes; largest on the disc is 28,058 bytes).
  - `etc/helpN.mes` must fit in **`0x1000` bytes** (4,096 bytes; largest on the disc is 1,504 bytes).
- **Pack and heap message buffers**: Menu `.mes` files load alongside the rest of their `.pac` archive into the menu buffer, and `dungeon/msg/d01_N.mes` loads into the dungeon heap.
- **Line width and window clipping**: A fixed-size UI window draws only as many lines and horizontal pixels as fit inside its frame. `ClsMes::MakeMesWinTbl` and `GetMesWidth_system` compute line widths using a standard cell width of `font_w = 15` pixels (`CFont` clearance and window presets):
  - ASCII glyphs (`0..93`), European accented glyphs (`0x5E..0x9C`), and `0xFFA0..0xFFFF` codes advance by `int(font_w * 0.55) = 8` pixels.
  - Spaces (`0xFF02`) advance by `font_w / 2 = 7` pixels (or `xx` pixels after `0xF8xx`).
  - All other glyphs (`>= 94` in English/Japanese) and `0xFDE0..0xFDF7` symbols advance by a full cell (`15` pixels).
  - Icons (`0xFD00..0xFD31`) advance by their width in `GaijiDataTbl` (for example, `22` pixels for the Circle button).
  - Pen moves (`0xF9xx`) advance by `xx` pixels.
  - Dynamic slots (`0xFAxx` / `0xFBxx`) contribute `0` width during static table construction.
  While `sysmes`, `npcmsg`, and `help` dialogue boxes automatically resize to fit their text, fixed menu columns clip or overflow if an item or skill name exceeds its column width.
- **Available glyphs**: Any glyph index beyond the loaded font texture's dimensions renders blank. In the unmodded PAL release, English text is limited to glyphs `0..287` (`FontTex_1_0.tm2`), and French, German, Italian, and Spanish are limited to ASCII (`0..93`), the accented letters (`0xFFA0..0xFFFF`), and the European symbols (`0xFDE0..0xFDF7`).
