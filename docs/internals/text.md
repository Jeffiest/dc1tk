# Message files (`.mes`)

All UI text outside event scripts—menus, shop lists, map names, item names and descriptions, townsperson dialogue, and system prompts—comes from binary **message files** (`.mes`). Each `.mes` file starts with an index table of message IDs followed by streams of 16-bit glyph and layout codes.
- Event scripts use a separate CP932 `.txt` format (inline in `.script` files) whose `[tag]` commands compile at runtime into the same 16-bit codes (see [message text](running.md#message-text)).
- For editing `.mes` files as plain text, see [text in a mod](../guide/text.md).
- For the `dc2tk` message converter implementation, see [the text code](../../src/docs/assets/text.md).
- All multi-byte values in `.mes` and `fonttbl*.bin` files are little-endian unless noted otherwise.

**Sources**:
- **Engine** (`finaldc2/ChronicleTwo/ps2`):
  - `src/nd_meswin.cpp` (`ClsMes::GetTextLineDataTop`, `GetTextLineDataTop_system`, `SetMesWinTbl`, `MakeMesWinTbl_item`, `MyTextureMake_sub`, assembly of `MakeMesWinTbl__6ClsMesFi`)
  - `src/font.cpp` (`GetRectFontTex`, `GetFontNo`, `GetAlphabeticalFontNo_us`, `GetFontNoFromFontGaijiCode`, `DrawGaiji_sub`, and tables `FconvCodeTbl`, `FontGaijiConvTbl`, `at_1120` in `asm/pal/data/font.data.s`)
  - `src/sysmes.cpp` (`LoadSystemMes`), `src/helpmes.cpp` (`LoadHelpMes`), `src/menumain.cpp` (`GetMenuMainMessageBuffer`), `src/mainloop.cpp` (`LoadFontTexture`)
- **Executable & Data**: File paths in `SCES_511.90` and corpus counts across all 310 `.mes` and `fconv*.bin` files on the PAL disc.

## Corpus at a glance

| Category | Count |
|---|---|
| Total message files | `310` (`302` `.mes` files and `8` `fconv*.bin` files with identical binary format) |
| `allmenu.mes` | `129` (one per menu pack across 6 language folders, plus `menu/allmenu.mes`) |
| `systree.mes`, `npcmsg.mes`, `mapname.mes` | `42`, `26`, `25` |
| `geomenu.mes`, `menumos.mes`, `shopname.mes`, `system.mes`, `sysmes.mes` | `6` or `7` each |
| `system_N`, `sysmes_N`, `d01_N`, `helpN`, `hddchkN`, `gyoreN`, `fconvN` | One per language (`N = 1..6` or `0..6`) |
| Standalone / legacy files | `sysmes0`–`3`, `mons_name_0` / `_1`, `geomes0`, `fishing0`, `gyore`, `event/fconv.bin` |
| Total messages (table entries) | `162,954` (max `989` entries in a single file) |
| Total 16-bit codes across all messages | `5,064,394` |
| Files larger than 64 KB | `100` (`allmenu.mes` inside `menu/1/`..`menu/5/`, up to `85,544` B) |

**Distribution of the `5,064,394` 16-bit codes across the disc**:

| Code Range | Meaning | Corpus Count |
|---|---|---|
| `0x0000..0xEFFF` | Font glyph indices (max used on disc is `1941`) | `4,119,187` |
| `0xFF02` | Space character | `533,942` |
| `0xFF00` | Line break | `205,539` |
| `0xFAEA..0xFAF9` | Dynamic text string slots (`1..16`) | `97,408` |
| `0xFFA0..0xFFFF` | European accented Latin-1 letters | `56,084` |
| `0xFBE7..0xFBF2`, `0xFBFB..0xFBFE` | Dynamic item-name slots (`1..16`) | `21,740` |
| `0xFBDF..0xFBE6`, `0xFBF3..0xFBFA`, `0xFBFF` | Dynamic number slots (`1..16` and standalone value) | `18,518` |
| `0xFD00..0xFD31` | Controller button and UI icons (`gaiji`) | `9,239` |
| `0xFDE0..0xFDF7` | European full-width symbols | `1,479` |
| `0xF8xx`, `0xFCxx`, `0xF9xx` | Space width (`609`), preset colour (`434`), horizontal pen offset (`146`) | `609`, `434`, `146` |
| `0xFF03` | Page break (wait for button press) | `69` |
| `0xFAFA..0xFAFF`, `0xFExx`, `0xF2xx..0xF5xx`, `0xF7xx`, `0xFF04..0xFF06` | Player-renamed names, delays, RGBA channels, justification, voice blips | `0` in `.mes` (supported by engine) |

## Which files the game reads

The engine selects localized files via `LanguageCode` (`0` = Japanese, `1` = English, `2` = French, `3` = German, `4` = Italian, `5` = Spanish; retail PAL supports `1..5`, while `6` = unused Korean):

| File Path | Loader | Target Memory Buffer & Capacity |
|---|---|---|
| `meswin/system_N.mes` (`system.mes` when `N = 0`) | `LoadSystemMes` | `SystemMesBuffer`: **`0xD000` B** (`53,248` B; max on disc `44,118` B). Holds item names and shared system strings. |
| `meswin/sysmes_N.mes` (`sysmes.mes` when `N = 0`) | `LoadSystemMes` | `SysMesBuffer`: **`0x13880` B** (`80,000` B; max on disc `28,058` B). |
| `etc/helpN.mes` | `LoadHelpMes` | `HelpMesBuff`: **`0x1000` B** (`4,096` B; max on disc `1,504` B; larger files are rejected). |
| `dungeon/msg/d01_N.mes` | Dungeon init | Allocated on the dungeon heap. |
| `title/hddchkN.mes`, `sg/gyo/gyoreN.mes`, `menu/aqua/mes/fconvN.bin` | Scene loaders | Scene-local buffers. |
| `menu/N/*.pac` (`allmenu.mes`, `systree.mes`, `npcmsg.mes`, `shopname.mes`, `mapname.mes`, `menumos.mes`, `geomenu.mes`) | `GetPackFile` (`GetMenuMainMessageBuffer`) | Loaded into the menu buffer alongside the `.pac` archive. |

- **Unused legacy files on the disc**: `meswin/eu/`, `meswin/usa/`, `meswin/kor/`, `meswin/try/`, `meswin/sysmes0`–`3.mes`, `menu/allmenu.mes`, `etc/geomes0.mes`, `sg/fish/fishing0.mes`, `dungeon/msg/mons_name_*.mes`, `event/fconv.bin`, and `sg/gyo/gyore.mes` are never loaded by `SCES_511.90`.

## Layout

A `.mes` file is an array of little-endian 16-bit unsigned words (`u16`):

| Word Index | Byte Offset | Type | Field | Description |
|---|---|---|---|---|
| `0` | `0x00` | `u16` | `count` | Number of entries in the index table. |
| `1` | `0x02` | `u16` | `size` | `file_size & 0xFFFF` (wraps around above 64 KB; **never read** by the engine). |
| `2 + 2 * i` | `0x04 + 4 * i` | `u16` | `entry[i].id` | Message ID of entry `i` (unique within each file; sorted in 74 files, authoring order in 236). |
| `3 + 2 * i` | `0x06 + 4 * i` | `u16` | `entry[i].offset` | Word offset of message `i` relative to word `1 + count` (max payload ~128 KB = `65,535 * 2` B). |
| `2 + 2 * count` | `0x04 + 4 * count` | `u16[2]` | `lead_sep` | Leading separator before the first message (`0xFF01, 0xFF00` in all 310 files). |
| `4 + 2 * count ..` | `0x08 + 4 * count ..` | `u16[]` | `payloads` | Message code streams, each terminated by `0xFF01, 0xFF00`. |

- **Lookup (`GetTextLineDataTop(id)`)**: Linearly scans the index table for `id` and returns a pointer to word `(1 + count) + offset`. Reading stops at `0xFF01`.
- **Corpus quirks**:
  - `161,660` messages end with `0xFF01, 0xFF00` right before the next indexed message; `885` are followed by unindexed leftover strings.
  - In `309` of `310` files, the final table entry points past the last `0xFF01, 0xFF00` to `0..5` trailing `0xFF00` words with no closing `0xFF01`.
  - `meswin/sysmes0`–`3.mes` have `39,200` B of trailing data past `size`. The unused `meswin/eu/fra/system.mes` has 5 out-of-bounds offsets and 86 mid-message offsets.

## Codes

### Glyphs below 0xF000

Codes `< 0xF000` index directly into the active font texture (32 columns per row, 16×20-pixel cells, `GetRectFontTex`). At boot, `LoadFontTexture` loads one texture page and `LoadFontTblBin` loads the corresponding Shift-JIS lookup table:

| `LanguageCode` | Font Texture | Font Table (`fonttbl`) | Glyph Range | Characters Drawn |
|---|---|---|---|---|
| All (`0..6`) | All | All | `0..93` | Printable ASCII from space (`0` = `' '`, `1` = `'!'`, `0x3B` = `'['`, `0x3C` = `']'`, `0x40` = `'a'`), **omitting `\`**. |
| `1` (English) | `meswin/FontTex_1_0.tm2` (9 rows = 288 cells) | `meswin/fonttbl_1.bin` | `94..287` | Full-width punctuation, symbols, numerals, Latin letters, and kana. |
| `2..5` (FR, DE, IT, ES) | `meswin/FontTex_2_0.tm2` | `meswin/fonttbl_2.bin` | `0x5E..0x9C`, `0x9D..0xB4` | Accented Latin-1 (`0x5E..0x9C`, usually encoded via `0xFFA0..0xFFFF`) and full-width symbols (`0x9D..0xB4`, via `0xFDE0..0xFDF7`). |
| `0` (Japanese) | `meswin/FontTex_0.tm2` (PAL loads page 0 of `0..3`, 608 cells/page) | Reconstructed (`meswin/fonttbl.bin` has only 94 ASCII entries) | `94..1943` | Full-width symbols/kana (`94..379`), JIS Level 1 kanji in Shift-JIS order up to `腕` (`380..1921`), plus 22 kanji in Unicode order (`1922..1943`: `儚 剝 哺 ... 騙`). Used by untranslated `geomenu.mes`. |
| `6` (Korean, unused) | `FontTex_6_0.tm2`, `FontTex_6_1.tm2` (never loaded in PAL) | `meswin/fonttbl_6.bin` | `94..967` | 12 Shift-JIS symbols (`94..105`) + 862 Hangul syllables sorted by Unicode (`106` = `가`, `U+AC00`). |

- **`fonttbl_*.bin` binary header (`0x08` bytes)**: Four `u16` words (`half_width_count = 94`, `kanji_start`, `total_count`, `0`), followed by `total_count` big-endian 16-bit Shift-JIS codes (ASCII entries padded with trailing `0x20`).
- **Expanding `FontTex_1_0.tm2`**: `mainloop.cpp` copies `FontTex_1_0.tm2` (512×192 `PSMT4`) into `font_buff` (`0xD000` = `53,248` B). Growing the texture by **1 row (to 512×200 = `51,328` B)** adds **32 glyphs (`288..319`)** within `font_buff`.

### Layout codes

| Code | Meaning |
|---|---|
| `0xFF00` | Line break. |
| `0xFF01` | End of message. |
| `0xFF02` | Space character (advances by `font_w / 2` or the `0xF8xx` width). |
| `0xFF03` | Page break (waits for a button press). |
| `0xFF04..0xFF06` | Set text-reveal voice blip to type `1`, `0`, or `2`. |
| `0xFFA0..0xFFFF` | In languages `2..5`, draws Latin-1 character for the low byte (`GetAlphabeticalFontNo_us`: `¡ ª ° º ¿ À–Ï Ñ–Ö Ù–Ý ß à–ï ñ–ö ù–ý ÿ`, with `0xBD`/`0xBE` -> `Œ`/`œ`, mapping to glyphs `0x5E+`; unmapped draws space `0`). |
| `0xFExx` | Pause text reveal for `xx` frames after the preceding glyph. |
| `0xFDE0..0xFDF7` | In languages `2..5`, draws full-width symbols `○ ● ↑ ↓ ← → ０–９ ： ＋ － ・ “ ” ‘ ’` (`FontGaijiConvTbl` -> glyphs `0x9D..0xB4`). Draws nothing in English or Japanese. |
| `0xFD00..0xFD31` | Draws a controller button or UI icon (`FconvCodeTbl`; `0xFD1B..0xFD20` have no text tag). When `LanguageCode != 0`, `DrawGaiji_sub` swaps `0xFD06` (Circle) and `0xFD08` (Cross). |
| `0xFCxx` | Select preset colour: `0xFC00` = window default, `0xFC01` = dark red (`0x8022227F`). |
| `0xFBFF` | Inserts standalone window number (`ClsMes::value`). |
| `0xFBFA..0xFBF3`, `0xFBE6..0xFBDF` | Inserts dynamic number slot `1..8` (`0xFBFA`..`0xFBF3`) or `9..16` (`0xFBE6`..`0xFBDF`). |
| `0xFBFE..0xFBFB`, `0xFBF2..0xFBE7` | Inserts dynamic item-name slot `1..4` (`0xFBFE`..`0xFBFB`) or `5..16` (`0xFBF2`..`0xFBE7`) from `system_N.mes`. |
| `0xFAFA..0xFAFF` | Inserts player-customised name `1..6`. |
| `0xFAF9..0xFAEA` | Inserts dynamic string slot `1..16` (`0xFAF9`..`0xFAEA`, set via `set_message_string`). |
| `0xF9xx` | Shift horizontal pen position right by `xx` pixels. |
| `0xF8xx` | Set pixel width of subsequent `0xFF02` spaces to `xx` pixels. |
| `0xF7xx` | Justify current line to `xx * 4` pixels (`CalcSpaceW`). |
| `0xF5xx..0xF2xx` | Set text colour Red (`0xF5xx`), Green (`0xF4xx`), Blue (`0xF3xx`), or Alpha (`0xF2xx`). |

## Limits

**Character advance widths** (`ClsMes::MakeMesWinTbl` and `GetMesWidth_system` with standard cell width `font_w = 15` px):

| Glyph / Code Type | Advance Formula | Width at `font_w = 15` |
|---|---|---|
| ASCII (`0..93`), European accented (`0x5E..0x9C`, `0xFFA0..0xFFFF`) | `int(font_w * 0.55)` (`CheckHalfFont` branches on `LanguageCode > 1` for glyphs `> 93`) | `8` px |
| Space (`0xFF02`) | `font_w / 2` (or `xx` after `0xF8xx`) | `7` px |
| English/Japanese glyphs `>= 94`, European symbols (`0xFDE0..0xFDF7`) | `font_w` | `15` px |
| Icons (`0xFD00..0xFD31`) | `GaijiDataTbl` entry width | e.g. `22` px (Circle) |
| Pen offset (`0xF9xx`) | `xx` | `xx` px |
| Dynamic slots (`0xFAxx`, `0xFBxx`) | `0` during static table layout | `0` px |

- **Window clipping**: `sysmes`, `npcmsg`, and `help` boxes auto-resize to fit text, whereas fixed-width menu columns clip or overflow if a name exceeds its column width.
- **Buffer caps**: `system_N.mes` <= `0xD000` B (`53,248` B); `sysmes_N.mes` <= `0x13880` B (`80,000` B); `helpN.mes` <= `0x1000` B (`4,096` B).
- **Glyph bounds**: Any glyph index outside the loaded font texture renders blank (`0..287` in unmodded English).
