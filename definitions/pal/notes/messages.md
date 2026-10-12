# Message text on the PAL disc (SCES_511.90)

Decomp paths are relative to `decomp-bench/artifacts/chronicletwo/ps2/`. Rodata strings were decoded from `asm/pal/nonmatchings/**/at_*__DATA.s` with `rodata.py`.

Files in this folder:

- `mestext.py`: the reader (standard library only). `python3 -I mestext.py <extracted> <file> [ids...]`. It reads `.txt` event text, binary `.mes`, item names, map names and system names.
- `survey.py`: resolves every 5 to 7 digit literal passed to a `*message*`/`*voice*` call in src3 against the files the script loads.
- `inline_census.py`, `align_check.py`: count inline strings and check the language variant alignment.
- `rodata.py`, `tables.py`: decode decomp rodata (format strings, the gaiji tag tables).

## 1. Message ids

### Where `show_message(win, id)` gets its text

`_MES_MAKE` (src/event_func.cpp:8352) with an int `no`:

1. `mes->mes_data` set: `GetBuffMesIdPtr(mes_data, size, no)`. `mes_data` is the text file that `load_message_file` (`_LOAD_MES`, event_func.cpp:9073) loaded through `_LOAD_MES_sub` → `GetLoadBGBuff` (event_func.cpp:3509), which calls `FileNameConvLanguage` on the path first.
2. Else, window 0 with NPC talk loaded: `EdEventInfo.npc_talk_text`, filled by `LoadNpcTalkMes` (src/event.cpp:47) from `event/talk/npc_talk_c<chapter>_<lang>.txt`, or `event/talk/npc_talk_c2_<lang>.txt` when that is missing.
3. Else `MakeMesWin(no)` looks the id up in the window's binary buffer (`ClsMes::buff`):
   - town windows 1 to 3 are the system messages, with `buff = meswin/sysmes_<L>.mes` and `buff_system = meswin/system_<L>.mes` (src/sysmes.cpp:44, 91)
   - dungeon window 0 is `dungeon/msg/d01_<lang>.mes` (src/dng_main.cpp:489)
   - town window 0 (EventMes1, editloop.cpp:561) has no binary buffer, so this path shows nothing for it

`id == -1` lays out nothing. If the window already holds that id, the window moves to its next page.

In practice cutscene text comes from per-event text files that the script loads by name, usually after `change_dir("event/eX00/eNNN/")`. Selection works like this:

- **Per event**: one file per event (`event/e200/e215/e215_1.txt`), loaded and unloaded around each cutscene (`load_message_file(0, -1)`). `event/opening/p1/op1chN_1.txt` holds the prologue, `event/boss_btl_lose_1.txt` the game-over lines, and `dungeon/msg/mostalk<dng>_<lang>.txt` (via `_LOAD_MES_MONS_TALK`) the dungeon monster talk.
- **Per language**: a `_<L>` suffix (section 2).
- **Global**: no event text is global. Only the binary system files are global.

### `.txt` format

Raw bytes with no header. The engine parses it like this:

- **Record start**: `GetBuffMesIdPtr` (src/nd_meswin.cpp:706) scans every `@` in the buffer, reads `atoi` of what follows, and the first match wins. The text starts after the next `\n`.
- **Record end**: `PreMesMake` (nd_meswin.cpp:3361) copies until `\n@` or `\r\n@` or NUL. It turns the two characters `\n` (backslash-n) and CRLF into newlines. The buffer is 512 bytes.
- **Comments**: `MakeMesWinTbl_str` (nd_meswin.cpp:1714) treats `//` anywhere as a comment up to and including the newline. Every record keeps the original Japanese as `//` lines above the translation.
- **Terminator**: `@9999999` closes the file. `$~name.txt` files are editor backups, so ignore them.
- **Encoding**:
  - Japanese (`_0` or no suffix) and English (`_1`): Shift-JIS. English uses `meswin/fonttbl_1.bin`, which has 985 glyphs including kana and kanji, and full-width `！？’` do occur.
  - FR/DE/IT/ES: ASCII plus `[UNIxxxx]` escapes (9 bytes, `GetAlphabeticalFontNo_cp`, font.cpp:373). Five codes are remapped: 011d→è, 0129→ê, 0155→à, 0171→û, 017f→ù. There are also raw Latin-1 bytes (`GetAlphabeticalFontNo_uc`, table `at_1041__5`: 0xBD→Œ, 0xBE/0x9C→œ).
  - Both escape paths are disabled when `LanguageCode == 1`.
  - The 63 accented glyphs are font numbers 94..156: ¡ª°º¿ÀÁÂÃÄÅÆÇÈÉÊËÌÍÎÏÑÒÓÔÕÖÙÚÛÜÝßàáâãäåæçèéêëìíîïñòóôõöùúûüýÿŒœ.
- **Tags** (literal strings `at_2366..at_2398` in nd_meswin):

| Tag | Meaning |
|---|---|
| `<page>` | page break |
| `[数値1]`..`[数値10]`, `[Number1]`..`[Number10]` | number slots (`set_message_value`) |
| `[アイテム1]`..`[アイテム16]` | system message or item name slots (`set_message_sysmes`, `set_message_item`) |
| `[文字列1]`..`[文字列16]` | string slots (`set_message_string`) |
| gaiji tags (FconvCodeTbl → 0xFD00..0xFD31) | `[select] [start] [l1] [r1] [l2] [r2] (O) (A) (X) (#) [+] [-] [!] [heart] [clef] [dame] [sita] [weapon] [protector] [material] [tool] [parts] [hari] [button] [fish] [(!)] [bulb] [*] [cross] [hp] [mp] [lv] (L) (R) (L3) (R3) (regi)` and a few more |
| full-width ○●↑↓←→０-９：＋－・“”‘’ | font gaiji |

- **No colour in text**: colour, wait and justify exist only as 16-bit codes in binary `.mes` files. The text path has no syntax for them.

For comments, keep tags as written and turn newlines into `\n` or ` / `.

### Binary `.mes` format

Used for `sysmes_*.mes`, `system_*.mes`, `d01_*.mes`, `npcmsg.mes`, `etc/help*.mes`, `menu/<L>/*.mes` and others. See `ClsMes::GetTextLineDataTop`, nd_meswin.cpp:2259.

- **Index**: `u16 count`, `u16 ?`, then `count` pairs of `(u16 id, u16 offset)` at byte 4.
- **Text location**: the text of entry `i` starts at u16 index `1 + count + offset`.
- **Characters**: each is a u16 font number, an index into the font table. English uses `fonttbl_1.bin`; FR/DE/IT/ES use `fonttbl_2.bin` + the 63 accented glyphs. The table is `u16 half, kanji_top, count, 0` followed by big-endian Shift-JIS codes; ASCII is stored as `c,0x20`.
- **Control codes** (`MesCode` in include/nd_meswin.hpp:100):

| Code | Meaning |
|---|---|
| FF01 | end |
| FF00 | newline |
| FF02 | space |
| FF03 | page |
| FF04-06 | voice blip |
| FC00 / FC01 | colour default / highlight |
| F2xx-F5xx | RGBA |
| F7/F8/F9xx | justify, space width, move x |
| FExx | wait |
| FD00-FD31 | gaiji |
| FDE0-FDF7 | font gaiji |
| FBFB-FBFE, FBE7-FBF2 | item slots 1..4 and 5..16 |
| FBFF, FBF3-FBFA, FBDF-FBE6 | value slots |
| FAEA-FAF9 | string slots |
| FAFA-FAFF | registered names |
| FFA0+ | Latin-1 byte |

- **Japanese binary files**: these do not decode with either PAL font table. The Japanese table is not on the disc, which is fine because Japanese is not selectable.

### Proof on s05

`map/s/s05/s05_1.c2s` includes `lib/map/s/s05/s05_6.c2s`, which calls `load_message_file(0, 5, "event/e200/e215/e215_1.txt")` and then `play_voice_message(2150010)`.

Reading ids 2150010 and 2150020 from `event/e200/e215/e215_1.txt` gives the
two English lines the scene shows. Id 2350010, which `s05b_*` passes to
`play_voice_message_for(2350010, 120)`, reads from `e235_1.txt` in English and
from `e235_2.txt` in French. (The lines themselves are game text and are not
reproduced here.)

`survey.py` resolves literal message and voice ids over the whole decompiled disc:

| Result | Count | Share |
|---|---|---|
| Resolved through the script's own loaded text files | 18,999 | 98.3% |
| Resolved through npc_talk | 4 | |
| Unresolved | 327 | |

The unresolved ids break down as:

- 216 `play_voice_only`: voice with no text, such as 8500160 or 8510170 grunts.
- `start_voice_stream`.
- Wrappers fed by a separate inline `show_message` string (d02 `run_voice_message_auto(2900010)` follows an inline English line).
- About 30 ids that no loaded file contains.

**Data quirk**: `e215_1.txt` record 2150090 has an uncommented Japanese line, and the English game really shows it. The reader keeps it. If you want to hide such lines, drop lines with kana or kanji in non-Japanese files.

**Id clashes**: ids are not globally unique. 155 ids repeat with different text, for example opening `op1_5_1.txt` against `mostalk6_1.txt`. Inside one script only 33 ids in 21 scripts clash, all d03 boss maps. Resolve against the `load_message_file` that is active at the call: track `change_dir`, `load_message_file(win, stack, name)` and `load_message_file(win, -1)` in program order within the function or caller. Fall back to the union of the script's loads and skip ids whose texts disagree.

### Voice and stream numbers

`open_stream_wav(ch, n)` is `_STREAM_OPEN3`, which builds `sprintf("%07d.wav", n)` (event_func.cpp:12104, `at_7117`). The files live in `SOUND.DAT`, indexed by `SOUND.HD3`: 3,100 names like `2150010.wav`, with no language variants (one voice track for all languages).

The lib wrappers (`play_voice_message`, `_for`, `_auto`, `_hold`, `run_voice_message_window` in `lib/map/map_27.c2s` and others) take one `voice` argument and pass it to both `open_stream_wav(1, voice)` and `show_message(0, voice)`. The wav number and the message id are therefore the same number.

The id layout is `event_no * 10000 + line*10` (plus odd steps like 5 or 3 for inserted lines):

- e215 → 2150010, 2150020, and so on
- the prologue uses 10010..10999
- NPC talk ids are 5 digits (10211)

Of 3,155 English event-text ids, 2,912 have a wav. 188 wavs have no text line (voice-only).

For comments, mark a function parameter as a message id when it flows into `show_message`'s second argument, then comment literal arguments at the call sites. Most ids sit in the 5,680 `play_voice_message(N)` calls, not in direct `show_message` calls.

## 2. Languages

`LanguageCode` (src/mainloop.cpp:1829; `LANG_*` in include/mainloop.hpp:56): 0 Japanese, 1 English, 2 French, 3 German, 4 Italian, 5 Spanish (6 Chinese and 7 Korean are named only in debug). Scripts read it with `get_language()` (native 550, `_GET_LANGUAGE`).

| What | Rule | Evidence |
|---|---|---|
| Map scripts | `LoadScript("X.stb")` tries `X_<L>.stb`, then falls back to `X.stb` | mapjump.cpp:301, `at_950__4 = "_%d.stb"` |
| Event scripts (`load_event_script`) | if the name lacks `_1`, `.stb` becomes `_1.stb`; then `FileNameConvLanguage` | event_func.cpp:5390 |
| Loaded `.txt`, `.img`, `.stb` | `FileNameConvLanguage`: `_1.<ext>` → `_<L>.<ext>` when L is 2..5; anything else is untouched | event_func.cpp:1944, `at_1103 = "_1.%s"`, `at_1104 = "_%d.%s"` |
| Dungeon scripts | `map/d/d0N/d0N_<L>.stb`, `d0N_sfida_<L>.stb` (`_0` Japanese exists) | dng_main.cpp:909 |
| NPC talk | `event/talk/npc_talk_c<ch>_<L>.txt` | event.cpp:56 |
| Monster talk | `dungeon/msg/mostalk<dng>_<L>.txt` | event_func.cpp:9094 |
| System messages | `meswin/system_<L>.mes`, `meswin/sysmes_<L>.mes` (Japanese = no suffix; `meswin/usa/`, `meswin/eu/<fra,ger,ita,spn>/` hold gaiji images and older copies) | sysmes.cpp:44 |
| Dungeon messages | `dungeon/msg/d01_<L>.mes` | dng_main.cpp:489 |
| Item names | `menu/cfg7/comdatmes<L>.cfg` | gamedata.cpp:670 |
| Map names | `map/map<L>.cfg` | mapselect.cpp:197 |
| Menus | `menu/<L>/...` folders | `menu/%d/` |

Consequences:

- **Base scripts with variants**: the base `X.stb` is the Japanese original. On PAL it runs only when no `X_<L>.stb` exists.
- **Variant scripts and shared libs**: `_1` to `_5` are the localized scripts, and all five share the libs that reference `..._1.txt`. The engine rewrites those paths at load time. So within a variant script the literal path already names the English file.
- **Base-script paths**: Japanese base scripts reference `eNNN.txt`, and the English file is `eNNN_1.txt` (append `_1`, don't strip trailing digits: `op1_5.txt` → `op1_5_1.txt`). `mestext.localized_path(path, 1)` does this.
- **`get_language()` branches**: 525 calls in 178 files, mostly `if (language == 0) load "e648.txt" else load "e648_1.txt"`, or inline JP/EN `show_message` pairs (s39, s56). For comments, take the non-zero branch.

**Script coverage** (manifest): 1,398 scripts.

- 159 bases have `_1`..`_5`: 145 with a base `X.stb`, 14 dungeon bases with `_0`..`_5` and no base. Three event `_arg` scripts exist only as `_1`.
- 426 dungeon act and sys scripts and about 150 map scripts have no variants.

**Inline text census** (`inline_census.py`; `show_message`, `set_message_string` and `set_movie_caption` strings; libs counted under the languages of the scripts that include them):

| Script language | Latin strings | Japanese strings |
|---|---|---|
| English `_1` (109 files) | 3,202 | 57 (debug leftovers) |
| French `_2` | 3,229 | 56 |
| German `_3` | 3,231 | 56 |
| Italian `_4` | 3,211 | 56 |
| Spanish `_5` | 3,181 | 56 |
| Japanese base (120 files) | 118 | 4,242 |
| Dungeon `_0` (18 files) | 4 | 242 |

- Small libs are shared across languages: 1+5 has 31 identical Latin strings, and 1+2+3+4+5 has 17 Japanese strings.
- Base-only scripts with Japanese inline text that also runs on PAL: `dungeon/sys_script/d03`, `d05`, `map/s/s02/s02_b`, `map/s/s56`.

**Finding the English for an inline string**:

- **EU variants** (`X_2`..`X_5`): all 604 variant scripts that have strings line up call for call with `X_1` after `#include` expansion. They have the same count and the same call name and non-string arguments in the same order (`align_check.py`). The k-th string call in `X_L` therefore pairs with the k-th in `X_1`.
- **Japanese base `X`**: only 6 of 152 pairs line up. The base and `_1` have different structure and counts (`d01e01`: 392 against 398), so there is no mechanical mapping. Leave those strings uncommented, or align by function and LCS if needed.
- **Message ids**: id-based comments work in every language (same ids in `eNNN.txt` and `eNNN_L.txt`).

## 3. Other text worth commenting

All of these are English on disc and are read by `mestext.py`:

- **Items**: `menu/cfg7/comdatmes1.cfg`, lines `MES_SYS <item_no>,"Battle Wrench";` (Shift-JIS; other languages use `comdatmes<L>.cfg` with `[UNI]` escapes). The same names are in `meswin/system_1.mes` at id `10000 + item` (420 of 423 match). Use them for `add_item`, `remove_item`, `get_item_count`, `place_treasure_box`, `set_message_item`, `equip_chara_item`, `open_use_item_menu` and similar. Reader: `read_item_names(extracted, 1)`.
- **System messages** (`set_message_sysmes(win, slot, n)` and windows 1 to 3): `meswin/sysmes_1.mes` for system lines such as 10 `Obtained "[item1]"` or 71 `Advance to next floor?`. `meswin/system_1.mes` holds names by id range (reader: `read_system_names`):

| Ids | Content |
|---|---|
| 10..20 | party (10 Maximilian, 11 Monica) |
| 5000.. | menu verbs and colours |
| 10001..10428 | items |
| 20000..20189 | maps and places (20000 Sindain, 20010 Palm Brinks) |
| 30000 + npc*100 + k | town NPCs: k = `typetbl_853[kind]` = 0,10,20,30,2,40,45,50,55,0,90,60,0,21,25 (kind 0 = name, kind 12 = 33000 + npc); `GetPartyCharaMessage`, npccfg.cpp:93; `get_npc_message_no` returns these ids |
| 33001.. | padded NPC names |

  The `set_message_sysmes(1, 1, item + 20000)` pattern in `lib/map/map_94`/`map_96` therefore inserts a place name, and the decompiled variable name `item` is wrong there.
- **Map names**: `map/map1.cfg`, lines `MAP_NAME "g01","Sindain","g00/g00", ... ;// 0`. The row index is the map number (185 named entries), for `set_current_map`, `go_to_edit_mode`, `get_map_type` and `jump_to_map` with an int map. Reader: `read_map_names(extracted, 1)`.
- **Dungeon floor messages**: `dungeon/msg/d01_1.mes` (dungeon window 0). **Monster names**: `dungeon/msg/mons_name_1.mes`.
- **Monster talk**: `dungeon/msg/mostalk<N>_1.txt`.
- **Other `.mes` files** (same binary format, not checked id by id): help text `etc/help1.mes`, georama `etc/geomes0.mes`, `menu/1/*.pac.d/*.mes`, `sg/gyo/gyore1.mes`.
