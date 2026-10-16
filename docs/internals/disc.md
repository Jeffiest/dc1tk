# The disc

This page documents how the PAL disc stores game files across the ISO 9660 filesystem, the `DATA.DAT` archive, the `DATA.HD4` index, and internal pack files. All specifications come from the [ChronicleTwo](https://github.com/TheMoonPeople/ChronicleTwo) engine decompilation and verification against the PAL disc (source paths are relative to a ChronicleTwo checkout).

- For `.stb` scripts stored inside these archives, see [scripts](scripts.md).
- For other file formats, see [DC2 Internals](README.md).

## Disc layout

- **Filesystem and main archive**: The disc uses the ISO 9660 filesystem. The archive `DATA.DAT` stores almost every game file, and `DATA.HD4` stores the archive index (`ps2/src/dataread.cpp`, `InitCDFile`).
- **External ISO files**: Movies (`\MOVIE\`), voice audio (`\VOICE\` and `\V\`), and IOP modules (`\MODULES\`) sit directly in the ISO 9660 filesystem outside `DATA.DAT`.
- **`DATA.HD4` layout**: The index begins with a little-endian array of 12-byte records, followed by the file name string table:

| Offset | Size | Type | Field | Description |
|---|---|---|---|---|
| `0x00` | 4 | `u32` | `name_offset` | Byte offset to the null-terminated file path in the string table. |
| `0x04` | 4 | `u32` | `size` | File size in bytes. |
| `0x08` | 4 | `u32` | `sector` | Starting sector in `DATA.DAT`. Data starts at byte offset `sector * 2048`. |

- **Record count and terminator**: The first 32-bit word (`name_offset` of the first record) divided by 12 gives the total number of records. The final record is a terminator whose name is an empty string.
- **File paths**: File names are null-terminated strings. They use `\` as the directory separator and may contain Shift-JIS characters. The PAL disc stores 8,752 files in `DATA.DAT`.
- **Index size limit (`0x50000` bytes)**: The engine reads `DATA.HD4` into a fixed `0x50000`-byte buffer (`dataread.cpp`, `header_buff`). Each added file consumes 12 bytes for its index record plus the length of its path and a terminating NUL byte. To conserve space in `DATA.HD4`, pack new assets into pack files instead of adding top-level archive entries.

## Pack files

Character archives (`.chr`), map packs (`.mpk`), sound banks (`.snd`), and several other container files use the pack format. A pack file is a linked chain of entries (`ps2/include/dataread.hpp`, `PACK_ENTRY`):

| Offset | Size | Type | Field | Description |
|---|---|---|---|---|
| `0x00` | 64 | `char[64]` | `name` | Null-terminated entry name. Empty in the terminator entry. |
| `0x40` | 4 | `int` | `offset` | Byte offset to the entry data, relative to this entry (`0x50`). |
| `0x44` | 4 | `int` | `size` | Entry data size in bytes. |
| `0x48` | 4 | `int` | `next` | Byte offset to the next entry, relative to this entry. |

On the PAL disc, every pack entry follows three alignment rules:

1. Entry data starts at relative offset `0x50` (the `0x4C`-byte `PACK_ENTRY` header rounded up to a 16-byte boundary).
2. The `next` field equals `0x50` plus `size` rounded up to a 16-byte boundary.
3. The chain ends with a `0x50`-byte terminator entry whose `name` is empty.

An `.stb` script file starts with the magic string `"SB2"`, so its header and program table can resemble a single pack entry named `SB2` if a parser checks only the entry name. To distinguish a pack file from a script, a parser must verify that every entry satisfies all three rules above.
