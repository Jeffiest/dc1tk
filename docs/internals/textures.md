# Dark Chronicle texture archives (`.img`: IM3, IM2, IMG)

A `.img` file is an archive of **standard TIM2 images** preceded by a name table. At runtime, `mgCTextureManager::EnterIMGFile` loads an entire `.img` archive and registers each picture under its name inside a texture block. From that point on, engine code, 3D models, and animation scripts look up textures **by name**. Three format versions exist: `IM3`, `IM2`, and `IMG`. Almost all archives on the disc use `IM3`.

This page documents the binary `.img` format and how the engine loads it. For a practical guide to editing textures as PNGs, replacing entries by ID, and adding custom textures, see [editing textures](../guide/textures.md). For details on the `dc2tk` texture converter implementation, see [src/docs/assets/textures.md](../../src/docs/assets/textures.md).

**Engine sources**:
- Struct declarations: `ps2/include/mg_texture.hpp` (lines 54–174)
- Loader and manager logic: `ps2/src/mg_texture.cpp` (`EnterTexture` lines 483–750, `EnterIMGFile` lines 754–900, `GetIMGVersion` / `mgGetIMGHeaderNum` / `mgGetIMGHeader` lines 910–1011, `ReloadTexture` lines 1189–1290, `ReloadCLUT` lines 1303–1325, and `BlockConv32to8` / `PageConv32to8` / `Conv32To8` lines 1427–1580)
- Swizzle lookup tables: `ps2/asm/pal/data/mg_texture.data.s` (`lut_1246`, `block_table8_1266`, and `block_table32_1267`)

The corpus statistics below cover all 5,217 `*.img` files extracted from the PAL disc (`extracted/disc/`). All multi-byte numbers in `.img` and TIM2 files are little-endian.

## 1. Corpus at a glance

| Category | Count |
|---|---|
| Total `.img` files | 5,217 |
| Magic `IM3\0` | 5,155 |
| Magic `IM2\0` | 58 |
| Magic `IMG\0` | 3 |
| Empty file (`menu/2/op_bg.img`, 0 bytes) | 1 |
| Total entries (all versions) | 16,406 |
| Texture animation scripts (`#texanime`) | 1,178 |
| TIM2 pictures | 15,228 |
| …Header-only pictures (`no_image = 1`) | 228 (224 `PSMCT32`, 4 `PSMCT24`) |
| …Pictures with pixel data | 15,000 |
| `PSMT8` (8-bit indexed) | 14,402 (31 of these have 3 mip levels) |
| `PSMCT32` (32-bit RGBA) | 310 |
| `PSMT4` (4-bit indexed) | 278 |
| `PSMCT24` (24-bit RGB) | 10 |
| `PSMCT16` (16-bit RGBA) | 0 (supported by the engine, but unused on the disc) |

> **Note**
> The 209 `.map` files that start with the 4 characters `"IMG "` (with a trailing space) are text map files, not binary `.img` texture archives.

## 2. Archive header (0x10 bytes)

The archive begins with a 16-byte `mgIMG_FILE_HEADER` structure:

| Offset | Type | `IM3` | `IM2` / `IMG` |
|---|---|---|---|
| `0x00` | `char[4]` | `"IM3\0"` | `"IM2\0"` / `"IMG\0"` |
| `0x04` | `u32` | Always `0x10` (header byte size; never read by the engine) | **Entry count** |
| `0x08` | `u32` | **Entry count** | `0` (one `IMG` file has `0x40`) |
| `0x0C` | `u32` | `0` | `0` (one `IMG` file has `0x21`) |

`GetIMGVersion` compares only the first 3 bytes of the magic string. `mgGetIMGHeaderNum` reads the entry count from word 1 (`0x04`) for `IMG` and `IM2` archives, and from word 2 (`0x08`) for `IM3` archives.

The entry table starts immediately at offset `0x10`. The first TIM2 picture or script starts right after the entry table. In all 5,214 non-empty archives with entries, the first entry's data offset equals the end of the entry table and aligns to 16 bytes.

## 3. Entries

### IM3 entry (0x40 bytes, `mgIMG_HEADER`)

| Offset | Type | Field | Values in the Corpus |
|---|---|---|---|
| `0x00` | `char[0x20]` | Entry name (NUL-terminated) | 1 to 19 characters. In 6,207 of 16,406 entries, the bytes after the first NUL contain uninitialised junk. |
| `0x20` | `u32` | `unk_20` | Always `0x40` (the entry header size). Never read by the engine. |
| `0x24` | `s32` | Data offset from start of `.img` file | Strictly ascending in entry order, contiguous, and 16-byte aligned. |
| `0x28` | `s32` | `swizzled` | `1` when 8-bit pixels are stored in `PSMCT32` swizzled order (see section 6.1). Equals `1` in 16,171 entries and `0` in 123 entries. |
| `0x2C` | `s32` | `block` | Texture-block group offset (`0..7`), relative to the base block where the archive is loaded. |
| `0x30` | `s16` | `no_image` | When `1` (228 entries), the engine registers the texture and reserves GS VRAM, but no pixel or palette bytes are stored in the file. |
| `0x32` | `s16` | `unk_32` | `0`, except for 3 junk values in `d01e01.img`. |
| `0x34` | `s32` | `size` | Byte length of the TIM2 or script. Matches the exact TIM2 byte length in 15,105 of 15,116 picture entries (11 entries are short by 276 or 302 bytes due to exporter bugs). The engine reads `size` **only** for `#` script entries. |
| `0x38` | `u64` | GS `CLAMP` register value | `0x5` (clamp both U and V) in 13,297 entries; `0x0` (repeat both) in 1,813 entries; 6 entries contain junk values. |

An entry whose name starts with `#` is a **texture animation script** rather than a TIM2 image. Every script entry on the disc is named `#texanime`. It contains Shift-JIS text commands such as:

```
SRC_TEX "g0301_17",0,0,64,64;
DEST_TEX "g0301_15",0,0;
```

The engine parses this script via `LoadCFGFile(img + offset, size, ...)`. Because these scripts reference textures by name and specify pixel-coordinate rectangles, **renaming or resizing a texture referenced by `#texanime` will break its animation**.

### IM2 / IMG entry (0x30 bytes, `mgIMG1_HEADER`)

| Offset | Type | Field |
|---|---|---|
| `0x00` | `char[0x20]` | Entry name (NUL-terminated; junk bytes after the NUL are common). |
| `0x20` | `s32` | Data offset of the TIM2 picture from the start of the `.img` file. |
| `0x24` | `u8[12]` | Unused padding. 19 of the 111 `IM2` entries contain leftover junk bytes here. |

`IM2` archives implicitly mark every picture as swizzled (`mgGetIMGHeader` sets `swizzled = 1`, and `EnterIMGFile` passes `not_im2 == 0`). `IMG` archives implicitly mark every picture as unswizzled (`swizzled = 0`). Neither `IM2` nor `IMG` stores `block`, `clamp`, `no_image`, or `size` fields. The engine loads all entries from an `IM2` or `IMG` archive into a single texture block and spills into the next block if GS VRAM fills up.

Two of the three `IMG` files on the disc (`item/atgrow.chr.d/atgrow.img` and `item/atluck.chr.d/atluck.img`, both paired with 0-byte `.mds` files) have an entry count of **`0`** in their archive header. Although both files contain an entry record and a TIM2 image, the engine loads 0 pictures from them.

### Data layout

Each entry's payload starts at its `offset` and extends to the next entry's `offset` (or to the end of the `.img` file). Across all 15,228 TIM2 pictures on the disc:
- There are **0 bytes of slack** between consecutive entries.
- No two entries ever share an offset.
- Every offset aligns to a 16-byte boundary.

The byte length of a normal TIM2 entry is `16 + header_size + image_size + clut_size`. The byte length of a `no_image` TIM2 entry is `16 + header_size`, because its pixel and palette sections are omitted.

## 4. TIM2 picture

Each picture entry embeds a standard single-picture TIM2 container (`TM2_head` followed by one `TM2_PICTURE` header). The offsets below are relative to the start of the TIM2 data:

| Offset | Type | Field | Values in the Corpus | Read by Engine? |
|---|---|---|---|---|
| `0x00` | `char[4]` | `"TIM2"` | Always `"TIM2"` | No |
| `0x04` | `u8` | Version | `3` (12,284) or `4` (2,716) | No |
| `0x05` | `u8` | Alignment format | `0` (16-byte alignment) | No |
| `0x06` | `u16` | Picture count | `1` | No (engine always reads the first picture) |
| `0x08` | `u8[8]` | Reserved | All `0` | No |
| `0x10` | `u32` | Total picture byte size | See converter quirks below | No |
| `0x14` | `u32` | CLUT byte size | `1024` (8-bit), `64` (4-bit), or `0` | No |
| `0x18` | `u32` | Image byte size (sum of all mip levels) | Exact byte count | **Yes** (used to locate the palette after the pixels) |
| `0x1C` | `u16` | Picture header size | `0x30` (1 mip level) or `0x50` (with mip table) | **Yes** (used to locate the start of pixel data) |
| `0x1E` | `u16` | CLUT colour count | `256`, `16`, or `0` | No (engine assumes `256` for 8-bit and `16` for 4-bit) |
| `0x20` | `u8` | Picture format | `5` (11,761) or `0` | No |
| `0x21` | `u8` | Mipmap level count | `1`, or `3` (in 31 `PSMT8` pictures) | **Yes** |
| `0x22` | `u8` | CLUT type | `0x03` (32-bit RGBA, `CSM1`, bit 7 clear = swizzled CLUT) or `0` | No |
| `0x23` | `u8` | Image type | `1` = 16-bit, `2` = 24-bit, `3` = 32-bit, `4` = 4-bit indexed, `5` = 8-bit indexed | **Yes** |
| `0x24` | `u16` | Width in pixels | `32..1024` | **Yes** |
| `0x26` | `u16` | Height in pixels | `32..1024` | **Yes** |
| `0x28` | `u64` | GS `TEX0` register | `0` in v3; v4 sets `PSM`, `TW`, and `TH` only (`TBW = 0`) | No (engine constructs its own `TEX0`) |
| `0x30` | `u64` | GS `TEX1` register | v3 stores `K` in high word (e.g. `0xffffff8f00000000`); v4 stores `0x260` | **Yes** (reads `L` and `K` only when `mip_levels > 1`) |
| `0x38` | `u32` | `GsTexaFbaPabe` | `0` | No |
| `0x3C` | `u32` | `GsTexClut` | `0` | No |
| `0x40` | `u64, u64` | `MipTBP1`, `MipTBP2` (present when `header_size > 0x30`) | `0` | No |
| `0x50` | `u32[n]` | Byte size of each mip level | Exact for the `n` levels; trailing bytes up to `header_size` can contain junk | **Yes** |

Pixel data starts at TIM2 offset `0x10 + header_size`. Mipmap levels are stored back-to-back from largest to smallest, and the CLUT palette immediately follows the last mipmap level.

**Exporter quirks in `total_size` (`0x10`)**: Four combinations of `(version, picture_format, total_size)` appear on the disc:
- `(3, 5, header_size + 4 * total_pixels)`: 11,761 pictures
- `(4, 0, exact_byte_size)`: 2,716 pictures
- `(3, 0, exact_byte_size)`: 314 pictures
- `(3, 0, header_size + 4 * total_pixels)`: 209 pictures

In the `4 * total_pixels` variants, the exporter multiplied every pixel across all mip levels by 4 bytes as if the image were `PSMCT32`. Because the engine never reads `total_size`, both conventions work identically at runtime.

## 5. Pixel formats

`EnterTexture` maps the TIM2 image type (`0x23`) to bits per pixel and a PS2 GS pixel storage mode (`PSM`):

| Image Type | Bits/Pixel | GS `PSM` | Pixel Storage in the File |
|---|---|---|---|
| `5` | 8 | `PSMT8` | 1 byte per palette index. Stored in **swizzled `PSMCT32` order** when the entry's `swizzled` flag is `1` (see section 6.1), or in linear raster order when `swizzled` is `0`. |
| `4` | 4 | `PSMT4` | Linear raster order, 2 pixels per byte, **low nibble = left pixel**. Never swizzled: the engine ignores `swizzled` for 4-bit textures (even though 275 of 278 entries set `swizzled = 1`). |
| `3` | 32 | `PSMCT32` | Linear raster order, 4 bytes per pixel (`R, G, B, A`). |
| `2` | 24 | `PSMCT24` | Linear raster order, 3 bytes per pixel (`R, G, B`). |
| `1` | 16 | `PSMCT16` | Linear raster order, `u16` `A1B5G5R5` (Red in bits `0..4`). Unused on the disc. |

When building the GS `TEX0` register, the engine always sets **`TCC = 1`** (include texture alpha) and **`TFX = 0`** (modulate texture with vertex colour). Indexed textures set `CPSM = PSMCT32`, `CSM = CSM1`, and `CLD = 1`:

```c
SCE_GS_SET_TEX0(0, tbw, psm, tw, th, 1, 0, 0, 0, 0, 0, 1)
```

- `TBW` (texture buffer width in 64-pixel units) is `ceil(width / 64)`.
- `TW` and `TH` are `ceil(log2(width))` and `ceil(log2(height))`. Non-power-of-two dimensions are valid and occur in 194 widths and 915 heights across the corpus.
- `TBP` (texture base pointer) and `CBP` (CLUT base pointer) are filled in when the texture is uploaded to GS VRAM.
- UV wrapping/clamping uses the 64-bit `CLAMP` register value from the `IM3` entry header.

### 6.1 The 8-bit swizzle (`swizzled = 1`)

A swizzled `w × h` `PSMT8` mip level is stored on disk as **a `(w / 2) × (h / 2)` `PSMCT32` image**. When the engine DMA-uploads that 32-bit image into GS VRAM using `PSMCT32` layout and then samples the same VRAM pages using `PSMT8` layout, the GS hardware reads the pixels back in the exact `w × h` 8-bit order. `ReloadTexture` (`src/mg_texture.cpp:1252-1258`) performs the upload with `width / 2`, `height / 2`, `PSM = PSMCT32`, and `max(1, TBW / 2)`. Consequently, the file stores bytes in linear raster order of the `(w / 2) × (h / 2)` 32-bit image (a row stride of `2 * w` bytes).

For small single-level textures (`w * h <= 0x10000`, i.e., up to 65,536 pixels), `EnterTexture` unswizzles the pixels **in place** into linear raster order at load time via `Conv32To8` -> `PageConv32to8` -> `BlockConv32to8`. After this in-place conversion, the engine clears the runtime `swizzled` flag, uploads the texture as standard linear `PSMT8`, and allows it to be cached in unused Z-buffer VRAM (`CheckCopyToZBufVram`). Both upload paths produce the exact same visual image on screen.

The swizzle permutation operates purely within 16×16 pixel blocks and does not depend on page boundaries. In GS memory, `PSMT8` and `PSMCT32` use the exact same block arrangement inside a page (`block_table8_1266` and `block_table32_1267` are identical: `0 1 4 5 16 17 20 21 / 2 3 6 7 18 19 22 23 / 8 9 12 13 24 25 28 29 / 10 11 14 15 26 27 30 31`). Thus, an 8-bit block `(bx, by)` of 16×16 pixels occupies the exact same 256-byte GS block as a 32-bit block `(bx, by)` of 8×8 pixels, and only the byte order inside the 256-byte block changes.

For an 8-bit pixel at coordinates `(x, y)`:

```python
bx, ix = divmod(x, 16)
by, iy = divmod(y, 16)
j      = iy * 16 + ix                     # Pixel index inside the 16x16 PSMT8 block
i      = BLOCK8_INV[j]                    # Byte index inside the 8x8 PSMCT32 block (32 bytes per row)
offset = (by * 8 + i // 32) * (2 * w) + bx * 32 + i % 32
```

Here, `BLOCK8_INV` is the inverse of the forward table lookup used in `BlockConv32to8`:

```python
j = (i // 64) * 64 + lut_1246[((i // 64) & 1) * 64 + (i % 64)]   # for i in 0..255
```

Each 64-byte `PSMCT32` column (8×2 pixels) maps to a 16×4 region of 8-bit pixels, with odd columns using the second 64-byte half of `lut_1246`. Expressed as a closed-form bitwise formula, this is the standard PS2 `PSMCT32`-to-`PSMT8` swizzle:

```python
block  = (y & ~15) * w + (x & ~15) * 2
swap   = (((y + 2) >> 2) & 1) * 4
posY   = (((y & ~3) >> 1) + (y & 1)) & 7
column = posY * w * 2 + ((x + swap) & 7) * 4
byte   = ((y >> 1) & 1) + ((x >> 2) & 2)
offset = block + column + byte
```

Verification scripts confirmed that the lookup table derivation, the bitwise formula, and a full emulation of `PageConv32to8` over a 128×64 page produce identical results across all image dimensions from 16×16 to 640×192.

**Dimension constraints**:
- `w` must be a multiple of 16.
- When `h` is not a multiple of 16, the final row of blocks is partially filled. In the corpus, non-multiple-of-16 heights occur on 128×876, 128×680, and 128×748 textures (where `h` is still a multiple of 4, so no pixels are lost) and on four **256×255** textures. In those 256×255 textures, the 65,280 stored bytes cover all except 128 pixels in the last two rows (which decode as index `0`), while 128 stored bytes map to row 255 outside the visible height.
- For mipmapped swizzled textures, each mip level is swizzled independently at its own dimensions. Because `mip_levels > 1`, the engine skips `Conv32To8` and uploads each mip level directly as `PSMCT32` with `TBW` halved at each level.

**Empirical verification**: Neighbour-pixel RGB smoothness tests across 2,000 sampled 8-bit images and all 278 4-bit images confirm these rules:
- **8-bit with `swizzled = 1`**: 1,949 of 1,949 images are smoother when unswizzled than when read linearly.
- **8-bit with `swizzled = 0`**: 51 of 51 images are smoother when read linearly.
- **4-bit nibble order**: 278 of 278 images are smoother when read low-nibble-first.
- **4-bit linear vs. swizzled**: 276 of 277 images are smoother in linear order (and the single exception, the sparse glyph sheet `font16/font3`, is visually legible only in linear order).

### 6.2 Palettes (CLUT)

All palettes store 32-bit `R8G8B8A8` entries (`CPSM = PSMCT32`) immediately after the final mipmap level.

- **8-bit (`PSMT8`)**: Contains 256 entries (1,024 bytes). `ReloadCLUT` uploads the palette as a 16×16 `PSMCT32` image (`mgLoadImage(..., 0x40 qwords, 16, 16)`) and hardware-samples it in `CSM1` mode. Because of the GS `CSM1` layout, **palette index `i` is stored in the file at index `i` with bits 3 and 4 swapped**:
  ```python
  file_index = (i & 0xE7) | ((i & 8) << 1) | ((i & 16) >> 1)
  ```
  In every group of 32 palette entries, entries `8..15` and `16..23` swap places. The engine applies this `CSM1` layout to every 8-bit texture regardless of bit 7 of the TIM2 `clut_type` field (which is `0x03` in all 8-bit files on the disc). Neighbour-pixel smoothness prefers `CSM1` palette order on 1,971 of 2,000 sampled textures; visual inspection of the remaining 29 cases (such as `b02a*_01`, `mario02`, `e49c2`, and `e49d2`) confirms that `CSM1` order renders cleanly while raw order corrupts colours and transparency.
- **4-bit (`PSMT4`)**: Contains 16 entries (64 bytes), uploaded as an 8×2 `PSMCT32` image in plain linear order (`0..15`).

Only 16-entry (64-byte) and 256-entry (1,024-byte) 32-bit palettes are supported.

### 6.3 Alpha

The PS2 GS treats alpha `0x80` (`128`) as full opacity (`1.0`). With `TFX = 0` (modulate), the hardware computes `output = (texture * vertex_colour) / 0x80`. Across the corpus:
- **Paletted textures**: The maximum palette alpha is `128` in 11,964 pictures, `127` in 2,192 pictures, lower in the remaining pictures, and above `0x80` in only one texture (`map/s/s51/s51.ipk.d/s51.img` -> `s510121`, which uses `0..255`).
- **`PSMCT32` textures**: Alpha values lie in `0..0x80`, except for **105 textures whose alpha channel is `0` for every pixel** (77 of these contain non-black RGB data, and 6 are `*_wrk` render targets).
- **`PSMCT24` textures**: Have no stored alpha channel.

## 7. How the engine loads and finds textures

When `EnterIMGFile(img, block, stack, info)` loads an archive:
1. For `IM3` archives only, it **sorts the entry table in place** by `entry.block` (using an unstable exchange sort that moves picture entries while keeping `#` script entries fixed, treating negative `block` values as `0`).
2. It iterates through the entries:
   - Entries starting with `#` are passed to `LoadCFGFile` to register texture animations for the current block.
   - Picture entries are passed to `EnterTexture(block + spill + entry.block, name, tim2, swizzled, no_image)`. Afterward, `EnterIMGFile` copies `entry.clamp` into `texture->clamp` and overwrites `entry.swizzled` with `texture->swizzled` (which becomes `0` if `Conv32To8` unswizzled the image in place).
3. For VRAM accounting, the engine rounds each texture's allocation up to whole 32-block GS pages and adds 4 GS blocks per CLUT palette. If the current texture block runs out of VRAM (`GetRemainVRAM < 0`), the engine prints `"texture over"` and advances `spill`, moving that texture and all subsequent entries in the archive into the next texture block.

`EnterTexture` concatenates `name + name_suffix` into a 32-byte (`0x20`) buffer, so **every texture name plus any active runtime suffix must fit within 31 characters**. If the name is already registered, the engine logs a warning. Each registered texture is linked into its block's texture list and inserted into a 101-bucket global hash table using the hash function:

```c
u8 h = 0;
for (signed char c : name) {
    h = ((h << 8) + c) % 101;
}
```

**Texture lookup**: The engine resolves textures **exclusively by name**. `GetTexture(name, block)` calls `SearchTextureName` (which appends the manager's current `name_suffix`, used by event scripts for character/costume variants) and then searches the hash table via `SearchHash` using `strcmp` (`block < 0` searches across all blocks). 3D model materials (`src/mg_visual.cpp:187`), texture animation scripts (`src/mg_tanime.cpp:650, 676`), and C++ game code (`GetTexture("water", -1)`) all look up textures by name. Neither the on-disk entry order nor the entry index affects name lookups, though `mgGetIMGHeader(img, i)` can still access entries by index (for example in `src/character.cpp:2625`).

## 8. Writing rules (what a modded file must satisfy)

- **Preserve texture names, formats, and dimensions**: Keep each texture's name (`<= 31` bytes including any runtime suffix), pixel format, and—whenever possible—its width and height. Animations and UI code reference textures by name and pixel coordinates, and GS VRAM budgets are tightly capped per block.
- **Align all pixel data to 16 bytes**: Every mipmap level's start offset and byte size must be a multiple of 16. The engine uploads pixel data via DMA in 16-byte quadwords (`qwc = (bpp * w * h) / 128`, truncated). All 15,000 pixel-bearing textures on the disc satisfy this rule.
- **Format palettes correctly**: Palettes must contain exactly 16 (`PSMT4`, 64 bytes) or 256 (`PSMT8`, 1,024 bytes) `RGBA32` entries. Always store 8-bit palettes in `CSM1` swizzled entry order.
- **Satisfy 8-bit swizzle dimension rules (or write unswizzled)**: If you set `swizzled = 1` on a `PSMT8` texture, its width must be a multiple of 16 (and either `<= 128` or a multiple of 128). If the height exceeds 64, keep the height a multiple of 64 because `Conv32To8` steps in 64-row pages. Alternatively, set `swizzled = 0` and store `PSMT8` pixels in plain linear raster order—this avoids all swizzle dimension restrictions and enables Z-buffer VRAM caching.
- **Limit mipmaps to 4 levels**: The engine supports at most 4 mipmap levels (`MG_TEXTURE_LEVEL_MAX`). Each level must be half the width and height of the previous level, and the TIM2 header (`header_size = 0x50`) must list each level's byte size at offset `0x50`.
- **Update offsets and sizes when repacking**: If a TIM2 entry changes byte size, update the `offset` of every subsequent entry in the `.img` archive and keep all offsets 16-byte aligned. Update the `IM3` `size` field to match. Because `EnterIMGFile` sorts the `IM3` table by `block` in memory, on-disk entry order does not need to be pre-sorted by block.

## 9. Open problems

- **Alpha values above `0x80` and all-zero alpha**: Only one texture (`s510121`) contains palette alpha values above `0x80` (`0..255`); whether over-bright modulation was intentional there is unknown. For the 105 `PSMCT32` textures whose alpha is `0` everywhere, it is not yet traced whether their rendering paths disable alpha blending or draw additively.
- **The four 256×255 textures (`map/f/f04/f04.ipk.d/f0401.img`)**: At load time, `Conv32To8` processes full 64-row pages and reads 256 bytes past the end of the pixel array into the palette. As a result, the 128 unstored pixels in the bottom two rows display palette-byte garbage in-game.
- **Unexplained exporter junk**: Several minor exporter artifacts remain unexplained: the 11 `IM3` entries whose `size` is short by 276 or 302 bytes, junk values in `CLAMP` and `unk_32` (`e207a01.img` and `d01e01.img`), leftover bytes after the NUL terminator in entry names, junk in the 12-byte tail of `IM2` entries, leftover bytes after the mip size array, and header words `0x40` and `0x21` in `c14a01.img`.
- **Zero-entry `IMG` archives**: `atgrow.img` and `atluck.img` have an entry count of `0` and are ignored by the engine.
- **TIM2 `picture_format = 5`**: Appears in 11,761 TIM2 headers. It is not part of the standard Sony TIM2 specification, and the engine ignores it.
- **Unused colour modes**: `PSMCT16` images and 16-bit or 24-bit CLUT palettes never appear on the disc.
- **GS VRAM block spillover**: Each texture consumes whole 32-block GS pages plus 4 blocks for its palette. Increasing a texture's dimensions can push later textures in the archive into the next texture block (`"texture over"`). For per-scene texture block allocations, see [the memory budgets](memory.md).
