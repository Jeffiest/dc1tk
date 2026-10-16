# The disc

This page documents how the PAL disc stores game files across the ISO 9660 filesystem, the `DATA.DAT` archive, the `DATA.HD4` index, and internal pack files. All specifications come from the [ChronicleTwo](https://github.com/TheMoonPeople/ChronicleTwo) engine decompilation and verification against the PAL disc; source paths are relative to a ChronicleTwo checkout. For the `.stb` scripts stored inside these archives, see [scripts](scripts.md). For other file formats, see [DC2 Internals](README.md).

## Disc layout

- The disc uses the ISO 9660 filesystem. The archive `DATA.DAT` stores almost every game file, and `DATA.HD4` stores the archive index (`ps2/src/dataread.cpp`, `InitCDFile`).
- `DATA.HD4` begins with a little-endian array of 12-byte records `{name_offset, size, sector}`, followed by the file name string table. The first 32-bit word (`name_offset` of the first record) divided by 12 gives the total number of records. The final record is a terminator whose name is an empty string.
- File names are null-terminated strings. They use `\` as the directory separator and may contain Shift-JIS characters. The PAL disc stores 8,752 files in `DATA.DAT`.
- The engine reads `DATA.HD4` into a fixed `0x50000`-byte buffer (`dataread.cpp`, `header_buff`), so the index file can never exceed `0x50000` bytes. Each added file consumes 12 bytes for its index record plus the length of its path and a terminating NUL byte. To conserve space in `DATA.HD4`, pack new assets into pack files instead of adding top-level archive entries.
- Inside `DATA.DAT`, each file's data starts at byte offset `sector * 2048`.
- Movies (`\MOVIE\`), voice audio (`\VOICE\` and `\V\`), and IOP modules (`\MODULES\`) sit directly in the ISO 9660 filesystem outside `DATA.DAT`.

## Pack files

A pack file is a linked chain of entries (`ps2/include/dataread.hpp`, `PACK_ENTRY`):

```
char name[64]
int  offset   data, relative to this entry
int  size
int  next     next entry, relative to this entry
```

On the PAL disc, every pack entry follows three alignment rules:

1. Entry data starts at relative offset `0x50` (the `0x4C`-byte `PACK_ENTRY` header rounded up to a 16-byte boundary).
2. The `next` field equals `0x50` plus `size` rounded up to a 16-byte boundary.
3. The chain ends with a `0x50`-byte terminator entry whose `name` is empty.

An `.stb` script file starts with the magic string `"SB2"`, so its header and program table can resemble a single pack entry named `SB2` if a parser only checks the entry name. To distinguish a pack file from a script, a parser must verify that every entry satisfies all three rules above. Character archives (`.chr`), map packs (`.mpk`), sound banks (`.snd`), and several other container files use this pack format.
