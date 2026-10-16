# The disc

How the PAL disc stores the game's files: the ISO, the archive `DATA.DAT` and its index, and the
pack files inside it. Everything here comes from the
[ChronicleTwo](https://github.com/TheMoonPeople/ChronicleTwo) decompilation, checked against the
PAL disc; paths are relative to a ChronicleTwo checkout. The scripts inside these files are in
[scripts](scripts.md); the other kinds of file have pages of their own (see
[the game section](README.md)).

## Disc layout

- The disc is ISO 9660. Almost every game file lives in `DATA.DAT`, indexed by
  `DATA.HD4` (`ps2/src/dataread.cpp`, `InitCDFile`).
- `DATA.HD4` is an array of 12-byte records `{name_offset, size, sector}`,
  little-endian. The record count is the first word divided by 12, and the
  last record is a terminator whose name is empty. Names are null-terminated,
  use `\` as the separator, and may contain Shift-JIS. The PAL disc has 8,752
  files.
- The engine reads `DATA.HD4` into a buffer of 0x50000 bytes
  (`dataread.cpp`, `header_buff`), so the index can never be larger. A new
  file costs its 12-byte record and its name plus a NUL, so new files belong
  in packs instead.
- A file's data starts at `sector * 2048` inside `DATA.DAT`.
- Movies (`\MOVIE\`), voice (`\VOICE\`, `\V\`) and IOP modules (`\MODULES\`)
  sit outside the archive.

## Pack files

A pack is a chain of entries (`ps2/include/dataread.hpp`, `PACK_ENTRY`):

```
char name[64]
int  offset   data, relative to this entry
int  size
int  next     next entry, relative to this entry
```

On the PAL disc every entry's data starts at 0x50 (the 0x4C-byte entry
rounded up to 16), `next` is 0x50 plus the size rounded up to 16, and the
chain ends with a 0x50-byte entry whose name is empty. A script's header and
program table can read as one pack entry named `SB2`, so only a file that
matches all of that is a pack. Characters (`.chr`), map packs (`.mpk`),
sound banks (`.snd`) and others use this format.
