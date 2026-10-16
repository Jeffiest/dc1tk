# Game data tables: the code

Which modules read and write the game's data tables, the choices they make, how they are tested
and what is not done yet. The tables' layouts are in [game data](../../../docs/internals/data.md);
how a mod edits them is in [editing game data](../../../docs/guide/data.md).

## Modules

In `dc2tk-assets`:

- `elf.rs` finds an executable address through the program header that loads it and refuses an
  executable without the table there ("not the PAL executable"); its patches are made in place,
  the file's size unchanged, and two mods' patches giving one value different bytes are refused.
- `cfg_text.rs` reads and writes the cfg scripts (below); `info_cfg.rs` reads a character's or
  pack's `info.cfg` through `cfg_text::number`.
- `data_table.rs` is shared by every kind but items and shops (the item table uses its kinds of
  value and how they are written and compared; layouts and chests, rewritten whole, have their
  own); `game_data.rs` lists the kinds and what each reads.
- The per-table modules:

| table | module |
|---|---|
| `data:items` | `item_data.rs` |
| `data:shops` | `shop_data.rs` |
| `data:monsters` | `monster_data.rs` |
| `data:items` `spectrumise`, `data:shops` `donny`, `data:characters`, `data:gifts`, `data:spheda`, `data:breeding` | `exe_tables.rs` |
| `data:inventions` | `invention_data.rs`, `idea_data.rs` |
| `data:georama` (with rivers and water) | `georama_data.rs` |
| `data:medals`, `data:floors` | `dungeon_data.rs` |
| `data:fishing` | `fishing_data.rs` |
| `data:layouts`, `data:parts` | `dungeon_layout.rs` ([dungeons](../../../docs/internals/dungeons.md)) |
| `data:chests` | `chest_data.rs` |
| `data:requests`, `data:townspeople` | `town_data.rs` |

`data:monsters` and the other executable tables are written into the built image's `SCES_511.90`
with `rebuild::overwrite_iso_file`, the file's size and layout as they were. A paletted PNG with
the item icon sheet's palette is taken as it is by `im3::from_png`, so a mod drawing one cell
changes only that cell.

In `dc2tk-script`, the build's side is `datatables.rs` (what `mod asset` writes and what `build`
and `mod check --disc` make of the mods' tables) and `itemtext.rs` (the item names and
descriptions put into the message files, in `text.rs`'s message format). It applies the tables
in one order, `replace::DATA_TABLES` (items, shops, monsters, inventions, georama, medals, floors,
fishing, characters, parts, layouts, chests, requests, townspeople, gifts, spheda, breeding),
each reading the files as the tables before it left them; `parts` comes before `layouts`, whose
cells name the parts.

## The JSON layout

`mod asset` writes each table as `json::json_text` lays it out for a person to edit: a 2-space
indent, an object a member a line, a list of numbers on one line, and a small entry of a list (at
most three plain members, `{"item": 210, "count": 5}`) on one line; a newline at the end.

## Settled (engine code and whole-corpus checks)

Every cfg script the tables come from is read by `dc2tk-assets`' `cfg_text.rs` (statements with
the spaces, line breaks and comments around every argument) and written back byte for byte; an
edit replaces only the arguments it changes, and a new statement or copied record takes the
file's own line break (`Script::newline`: `\r\n` in every cfg script of the disc, and for a file
of one line). It reads each argument as the engine's `GetArg` does
([game data](../../../docs/internals/data.md#how-the-engine-reads-a-cfg-script)). A character's or
pack's `info.cfg` is read by the same rules (`info_cfg.rs`, through `cfg_text::number`; a float
keeps its text's own value there, not the f32 the engine makes of it). Binary scripts (`BIN`
first) and comments inside a statement are refused: the engine blanks a comment even inside
quotes, which a kept file could not show. `cargo test --release -- --include-ignored corpus` with
`DC2TK_CORPUS` set checks all three versions of the item tables (`menu/cfg5`, `cfg6`, `cfg7`:
1,260 records), the 16 shop lists, the monster table and every kind above (136 recipes in 19
copies, 53 scoops, 218 Georama parts with 5 fixed and 29 starting parts, 161 floors' medals, 129
floors' monsters, 22 fishing spots, 216 race fish in 7 languages, 22 prize groups, 18 species and
race kinds, the character tables): every field's text or bytes are the ones its value writes, and
each exported table applied back changes nothing. A build with every table edited reads back as
edited from the image (packs holding `inv6.lst` rebuilt around it).

The added kinds' corpus tests (`cargo test --release -- --include-ignored corpus` with
`DC2TK_CORPUS`) check the same for the requests (54 in 7 languages), the 177 townspeople in 7
languages, rivers and water, gifts, clubs and breeding, and add a record of every kind that takes
them (items up to the 432, recipes up to the 234, a Georama part in every language, a spot and
fish, a request in its town's block, a townsperson), reading each back.

## Not yet

Tried in the game (a test image adds a weapon sold in Palm Brinks). Scoop names stay text
(`neta2.lst`, `scoop.cfg`).
