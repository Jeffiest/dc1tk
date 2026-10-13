# Dark Chronicle texture archives (`.img`: IM3, IM2, IMG)

A `.img` file is a small archive of **standard TIM2 images** with a
name table in front. The engine loads one whole archive with
`mgCTextureManager::EnterIMGFile`, registers every picture under its name
in a texture block, and from then on code and models find textures **by
name**. The format has three versions. IM3 is the one used almost
everywhere.

Sources: engine `ps2/include/mg_texture.hpp` (structs, lines 54-174) and
`ps2/src/mg_texture.cpp` (`EnterTexture` 483-750, `EnterIMGFile` 754-900,
`GetIMGVersion`/`mgGetIMGHeaderNum`/`mgGetIMGHeader` 910-1011,
`ReloadTexture` 1189-1290, `ReloadCLUT` 1303-1325, `BlockConv32to8`/
`PageConv32to8`/`Conv32To8` 1427-1580). Swizzle tables:
`ps2/asm/pal/data/mg_texture.data.s` (`lut_1246`, `block_table8_1266`,
`block_table32_1267`). The corpus is all 5,217 `*.img` files under
`extracted/`; the counts below come from scripts run over all of them
while the format was worked out. `im3.rs` (in `src/crates/c2-assets/src/`)
implements it, and `textures.rs` turns it into the PNG folders mods use.

All values are little-endian.

## 1. Corpus at a glance

| | count |
|---|---|
| `.img` files | 5,217 |
| tag `IM3\0` | 5,155 |
| tag `IM2\0` | 58 |
| tag `IMG\0` | 3 |
| empty file (`menu/2/op_bg.img`, 0 bytes) | 1 |
| entries (all versions) | 16,406 |
| texture animation scripts (`#texanime`) | 1,178 |
| TIM2 pictures | 15,228 |
| …header-only (`no_image`) | 228 (224 PSMCT32, 4 PSMCT24) |
| …with pixels | 15,000 |
| PSMT8 (8-bit indexed) | 14,402 (31 of them have 3 mip levels) |
| PSMCT32 (RGBA) | 310 |
| PSMT4 (4-bit indexed) | 278 |
| PSMCT24 (RGB) | 10 |
| PSMCT16 | 0 (the engine supports it) |

The 209 `.map` files that start with `IMG ` (with a space) are text, not
texture archives.

## 2. Archive header (0x10 bytes)

`mgIMG_FILE_HEADER`:

| off | type | IM3 | IM2 / IMG |
|---|---|---|---|
| 0x00 | char[4] | `"IM3\0"` | `"IM2\0"` / `"IMG\0"` |
| 0x04 | u32 | always 0x10 (header size; never read) | **entry count** |
| 0x08 | u32 | **entry count** | 0 (one IMG has 0x40) |
| 0x0C | u32 | 0 | 0 (one IMG has 0x21) |

`GetIMGVersion` compares only the first 3 bytes. `mgGetIMGHeaderNum` reads
the count from word 1 for IMG/IM2 and from word 2 for IM3.

The entry table starts at 0x10. The first picture begins straight after the
table: in all 5,214 non-empty archives, first offset = table end, and that is
16-aligned.

## 3. Entries

### IM3 entry (0x40 bytes, `mgIMG_HEADER`)

| off | type | field | values in the corpus |
|---|---|---|---|
| 0x00 | char[0x20] | name, NUL-terminated | 1-19 chars. In 6,207 of 16,406 entries the bytes after the NUL are junk; write keeps them. |
| 0x20 | u32 | `unk_20` | always 0x40 (the entry size); never read |
| 0x24 | s32 | offset of the TIM2 (or script) from the start of the file | ascending in entry order, contiguous, 16-aligned |
| 0x28 | s32 | `swizzled`: 8-bit pixels are stored in PSMCT32 order (see 6.1) | 1 for 16,171; 0 for 123 |
| 0x2C | s32 | `block`: texture-block group, relative to the block the archive is entered at | 0-7 |
| 0x30 | s16 | `no_image`: register the texture and reserve VRAM, but no pixels are stored | 228 × 1 |
| 0x32 | s16 | `unk_32` | 0, apart from 3 junk values in `d01e01.img` |
| 0x34 | s32 | `size`: byte length of the TIM2 or script | = TIM2 length in 15,105 of 15,116 picture entries. 11 entries are short by 276 or 302, apparently tool junk. The engine reads `size` only for scripts. |
| 0x38 | u64 | GS `CLAMP` register value for the texture | 0x5 (clamp U and V) 13,297; 0x0 (repeat) 1,813; 6 junk values |

A name beginning with `#` marks a **texture animation script**, not a
picture. Every one is called `#texanime`: Shift-JIS text such as
`SRC_TEX "g0301_17",0,0,64,64; DEST_TEX "g0301_15",0,0;`, run by
`LoadCFGFile(img+offset, size, …)`. The scripts name textures and give pixel
rectangles, so **renaming or resizing a texture that a script uses breaks
the animation**.

### IM2 / IMG entry (0x30 bytes, `mgIMG1_HEADER`)

| off | type | field |
|---|---|---|
| 0x00 | char[0x20] | name (junk after the NUL is common) |
| 0x20 | s32 | offset of the TIM2 |
| 0x24 | 12 bytes | unused. 19 of 111 IM2 entries hold junk here. |

IM2 means "every picture is swizzled" (`mgGetIMGHeader` sets
`swizzled = 1`; `EnterIMGFile` passes `not_im2 == 0`). IMG means
"none is swizzled". IM2 and IMG have no block, clamp, no_image or size
fields. The engine enters everything into one block, spilling into the
next block when VRAM runs out.

Two of the three IMG files (`item/atgrow.chr.d/atgrow.img` and
`item/atluck.chr.d/atluck.img`, which come with empty `.mds` files) have an
entry count of **0**. They hold an entry and a TIM2, but the engine sees no
pictures. `read()` follows the engine and returns none; `write()` keeps
the bytes.

### Data layout

Each entry's data starts at its offset and runs to the next offset (or to the
end of the file). In the corpus there is **never** slack after a TIM2
(0 bytes in all 15,228 pictures), offsets are never shared, and all are
16-byte aligned. A TIM2's length is `16 + header_size + image_size +
clut_size`, or `16 + header_size` for `no_image` entries, whose pixels and
palette are absent.

## 4. TIM2 picture

Each picture is a standard TIM2 file holding one picture (`TM2_head` /
`TM2_PICTURE`). The offsets below are from the start of the TIM2.

| off | type | field | corpus | read by the engine? |
|---|---|---|---|---|
| 0x00 | char[4] | `"TIM2"` | always | no |
| 0x04 | u8 | version | 3 (12,284) or 4 (2,716) | no |
| 0x05 | u8 | format (0 = 16-byte alignment) | 0 | no |
| 0x06 | u16 | picture count | 1 | no (only the first picture is used) |
| 0x08 | 8 bytes | reserved | 0 | no |
| 0x10 | u32 | total size | see the quirk below | no |
| 0x14 | u32 | CLUT size, bytes | 1024 (8-bit), 64 (4-bit), 0 | no |
| 0x18 | u32 | image size, bytes, all mip levels | exact | **yes**: where the palette starts |
| 0x1C | u16 | header size | 0x30, or 0x50 with mips | **yes**: where the pixels start |
| 0x1E | u16 | CLUT colours | 256 / 16 / 0 | no (the engine assumes 256 or 16) |
| 0x20 | u8 | picture format | 5 (11,761) or 0 | no |
| 0x21 | u8 | mip levels | 1, or 3 (31 PSMT8 pictures) | **yes** |
| 0x22 | u8 | CLUT type | 0x03 (32-bit, CSM1, bit 7 clear = swizzled order) or 0 | no |
| 0x23 | u8 | image type: 1=16, 2=24, 3=32, 4=4-bit idx, 5=8-bit idx | 5, 3, 4, 2 | **yes** |
| 0x24 | u16 | width | 32-1024 | **yes** |
| 0x26 | u16 | height | 32-1024 | **yes** |
| 0x28 | u64 | GS TEX0 | 0 (version 3); version 4 sets PSM, TW, TH only (TBW = 0) | no: the engine builds its own TEX0 |
| 0x30 | u64 | GS TEX1 | v3: K in the high word, e.g. `0xffffff8f00000000`; v4: `0x260` | only L and K, only for mip-mapped textures |
| 0x38 | u32 | GsTexaFbaPabe | 0 | no |
| 0x3C | u32 | GsTexClut | 0 | no |
| 0x40 | u64, u64 | MipTBP1/2 (when header size > 0x30) | 0 | no |
| 0x50 | u32[n] | byte size of each mip level | exact; bytes after the n sizes up to the header end can be junk | **yes** |

Pixels start at `0x10 + header_size`. The mip levels follow one another, and
the palette follows the last level.

**Converter quirks.** Three header styles exist:
`(version 3, pictformat 5, total = header_size + 4·pixels)` 11,761;
`(4, 0, total = exact)` 2,716; `(3, 0, exact)` 314; `(3, 0, header + 4·pixels)` 209.
"4·pixels" counts every pixel of every mip level at 4 bytes, as though the
picture were RGBA32. The engine ignores the total, so either is safe.
`im3.rs` keeps whichever rule the original used; regenerating every header
from the picture under that rule reproduces all 15,000 headers exactly.

## 5. Pixel formats

`EnterTexture` maps the image type to bits per pixel and GS PSM:

| type | bpp | PSM | storage in the file |
|---|---|---|---|
| 5 | 8 | PSMT8 | 1 byte = palette index. **Swizzled** when the entry says so (6.1), otherwise raster order. |
| 4 | 4 | PSMT4 | raster order, 2 pixels per byte, **low nibble = left pixel**. Never swizzled: the engine ignores the flag for 4-bit, and 275 of the 278 have it set. |
| 3 | 32 | PSMCT32 | R, G, B, A bytes, raster order |
| 2 | 24 | PSMCT24 | R, G, B bytes, raster order |
| 1 | 16 | PSMCT16 | u16 A1B5G5R5 (R in the low bits); none in the corpus |

The engine always builds TEX0 with **TCC = 1** (alpha comes from the
texture) and TFX = 0 (modulate). CLUT textures get CPSM = PSMCT32,
CSM = CSM1, CLD = 1 (`SCE_GS_SET_TEX0(0, tbw, psm, tw, th, 1, 0, 0, 0, 0, 0, 1)`).
Other TEX0 values: TBW = ceil(width/64), TW/TH = ceil(log2(size)) (so
sizes that are not powers of two are allowed: 194 widths and 915 heights in
the corpus), and TBP/CBP are assigned at upload time. The GS sampling mode
comes from the entry's CLAMP.

### 6.1 The 8-bit swizzle (`swizzled = 1`)

A swizzled level of w × h PSMT8 pixels is stored as **the (w/2) × (h/2)
PSMCT32 image that, written to GS memory as PSMCT32, reads back as the
w × h PSMT8 texture**. `ReloadTexture` (lines 1252-1258) uploads it exactly
that way: width and height halved, PSM = PSMCT32, TBW halved, minimum 1.
The bytes are therefore in raster order of the 32-bit image (row stride
2·w bytes).

Small single-level textures (w·h ≤ 0x10000) are instead converted in place to
raster order when loaded (`EnterTexture(TM2)` → `Conv32To8` →
`PageConv32to8` → `BlockConv32to8`). After conversion the texture is
uploaded as plain PSMT8 and is allowed into the Z-buffer VRAM
(`CheckCopyToZBufVram`). Both paths give the same picture.

The mapping does not depend on the page. In GS memory, PSMT8 and PSMCT32
use the same block arrangement within a page (`block_table8_1266` ==
`block_table32_1267` = `0 1 4 5 16 17 20 21 / 2 3 6 7 18 19 22 23 / 8 9 12 13
24 25 28 29 / 10 11 14 15 26 27 30 31`). So 8-bit block (bx, by) of 16×16
pixels is exactly the 32-bit block (bx, by) of 8×8 pixels (256 bytes), and
only the order inside a block changes. For 8-bit pixel (x, y):

```
bx, ix = divmod(x, 16);  by, iy = divmod(y, 16)
j      = iy*16 + ix                       # pixel inside the 16x16 block
i      = BLOCK8_INV[j]                    # byte inside the 8x8 PSMCT32 block (32 bytes per row)
offset = (by*8 + i//32) * (2*w) + bx*32 + i%32
```

`BLOCK8_INV` is the inverse of `j = (i//64)*64 + lut_1246[((i//64)&1)*64 + i%64]`
for i = 0..255. This is what `BlockConv32to8` does: each 64-byte PSMCT32
"column" (8×2 pixels) becomes 16×4 8-bit pixels, and odd columns use the
second half of the 128-byte LUT. Written as a formula, it is the well-known
PS2 one:

```
block  = (y & ~15)*w + (x & ~15)*2
swap   = (((y + 2) >> 2) & 1) * 4
posY   = (((y & ~3) >> 1) + (y & 1)) & 7
column = posY*w*2 + ((x + swap) & 7)*4
byte   = ((y >> 1) & 1) + ((x >> 2) & 2)
offset = block + column + byte
```

A research script checked that the engine-table derivation, the formula
and a full simulation of `PageConv32to8` over a 128×64 page all agree, for
sizes from 16×16 to 640×192.

Constraints: w must be a multiple of 16. When h is not a multiple of 16, the
last block row is partial. In the corpus this happens with 128×876, 128×680
and 128×748 (heights are multiples of 4, so nothing is lost) and with 4
pictures of **256×255**. For those, the stored 65,280 bytes cover all but
128 pixels of the last two rows (they read as 0), and 128 stored bytes belong
to no visible pixel. `im3.rs` keeps those bytes from the original.

Mip levels of a swizzled texture are each swizzled on their own: the upload
halves TBW per level, and these textures are not converted in place because
they have more than one level.

Evidence from neighbour-pixel RGB smoothness over a sample of 2,000 8-bit
pictures and all 278 4-bit ones:

* **8-bit, flag 1:** read unswizzled, 1,949 of 1,949 are smoother than in
  raster order.
* **8-bit, flag 0:** 51 of 51 are smoother in raster order.
* **4-bit nibble order:** 278 of 278 are smoother read low-nibble-first.
* **4-bit swizzle:** 276 of 277 are smoother linear than byte-swizzled. The
  one exception, the sparse font sheet `font16/font3`, is clearly linear when
  viewed.

Sample pictures (faces, wall textures, fonts) decode cleanly when viewed.

### 6.2 Palettes (CLUT)

Palettes are RGBA 8:8:8:8 (`CPSM = PSMCT32`), right after the last mip
level.

* **8-bit**: 256 entries (1,024 bytes), uploaded as a 16×16 PSMCT32 image
  (`ReloadCLUT`: `mgLoadImage(..., 0x40 qwords, 16, 16)`) and read with
  CSM1. So the colour of **index i is stored at entry `i` with bits 3 and 4
  swapped**: `(i & 0xE7) | ((i & 8) << 1) | ((i & 16) >> 1)`. Entries 8-15
  and 16-23 of each group of 32 trade places. The engine does this for every
  8-bit texture regardless of TIM2's CLUT-type bit 7, which is clear (0x03)
  in all of them. The decoded images agree: the RGB smoothness test prefers
  CSM1 order in 1,971 of 2,000 sampled pictures. Of the 29 exceptions, 22
  are near-ties on noisy logos and frames. The other 7 (`b02a*_01`,
  `mario02`, `e49c2`, `e49d2`) were rendered both ways: the CSM1 reading is
  clean, and the raw-order reading is noisy and loses its transparency. The
  metric is fooled there by the varied RGB of transparent background
  pixels.
* **4-bit**: 16 entries (64 bytes), uploaded as 8×2 PSMCT32. Linear order.

Only 16- and 256-entry 32-bit palettes are valid. The engine uploads a fixed
64 or 1,024 bytes.

### 6.3 Alpha

The PS2 treats alpha 0x80 as 1.0. With TFX = modulate, the result is
texture × vertex colour / 0x80. Palette alpha maximum: 128 in 11,964
pictures, 127 in 2,192, lower in the rest, and above 0x80 in exactly one
(`map/s/s51/s51.ipk.d/s51.img` → `s510121`, whose alphas run 0-255).
PSMCT32 pictures: alphas 0-0x80, except **105 whose alpha is 0 everywhere**
(77 of them have non-black RGB; 6 are `*_wrk` render targets). PSMCT24 has
no alpha.

## 7. How the engine loads and finds textures

`EnterIMGFile(img, block, stack, info)`:

1. IM3 only: **sorts the entry table in place** by `block`, moving pictures
   but not scripts, and treats negative blocks as 0.
2. For each entry: `#` scripts go to `LoadCFGFile` (texture animation of the
   block). Each picture goes to `EnterTexture(block + spill + entry.block,
   name, tim2, swizzled, no_image)`. Then `texture->clamp = entry.clamp`, and
   the entry's `swizzled` is overwritten with the texture's, which is 0 after
   an in-place conversion.
3. VRAM is rounded up to whole pages (32 GS blocks), plus 4 blocks per CLUT.
   If a texture block runs out (`GetRemainVRAM < 0`), the texture and every
   later one move to the next block ("texture over").

`EnterTexture` copies `name + name_suffix` into a 0x20 buffer, so **a name
plus any suffix must fit in 31 characters**. It warns when the name is
already registered. The texture goes into the block's list and into a
101-bucket hash table: `h = ((h << 8) + (signed char)c) % 101`, with h a u8.

Lookup is **by name only**. `GetTexture(name, block)` →
`SearchTextureName` (appends the manager's `name_suffix`, which event code
sets per character/label) → `SearchHash` (strcmp; `block < 0` = any
block). Models store texture names (`mg_visual.cpp:187`
`textures->GetTexture(src->texture, -1)`), and so do texture animation
scripts (`mg_tanime.cpp:650/676`) and game code (`GetTexture("water", -1)`).
Entry order and index do not matter to lookups. `mgGetIMGHeader(img, i)`
exposes entries by index (for example `character.cpp:2625`).

## 8. Writing rules (what a modded file must satisfy)

* Keep the name (≤ 31 bytes, including any runtime suffix), the format and,
  ideally, the dimensions. Scripts and code address textures by name and
  pixel rectangle, and VRAM budgets are per block.
* Pixel data must be 16-byte aligned and each level's byte size a multiple
  of 16. Uploads are DMA in quadwords: `qwc = bpp·w·h/128`, truncated. All
  corpus levels meet this.
* Palettes must be exactly 16 or 256 RGBA32 entries, with 8-bit palettes in
  CSM1 order.
* Swizzled 8-bit: width a multiple of 16 and (≤ 128 or a multiple of 128).
  For anything taller than 64, keep height a multiple of 64; `Conv32To8`
  walks whole 64-row pages. Clearing the swizzle flag (raster order) avoids
  all of this, because the engine itself converts to raster order.
* At most 4 levels (`MG_TEXTURE_LEVEL_MAX`), each half the size of the one
  before, with the mip sizes table filled.
* When a TIM2 changes length, every later entry's offset moves. IM3's
  `size` must follow it. Keep 16-byte alignment. The IM3 table is not
  sorted on disk, but the engine sorts it in memory, so entry order is free.

## 9. `im3.rs`

`c2_assets::im3`, in `src/crates/c2-assets/src/im3.rs`.

* `read(data) -> Result<Vec<Image>>` returns one `Image` per picture entry (scripts
  skipped), with `name`, `index` (entry number), `format`, `width`,
  `height`, `palette` (RGBA entries in index order, CSM1 undone, PS2
  alpha), `indices` (paletted, one byte per pixel) or `pixels` (RGBA bytes),
  `rgba()` (base level expanded), `mips`, `swizzled`, `no_image`, `block`,
  `clamp`, and `tim2` (the original bytes). An empty file gives no pictures.
* `to_png(image, indexed) -> Result<Vec<u8>>` writes the base level. Paletted
  pictures become indexed PNGs (PLTE + tRNS, in index order), and the rest
  become RGBA, or RGB when there is no meaningful alpha. Alpha mapping
  (`alpha_mode`, an `AlphaMode`):
  `Ps2` (every alpha ≤ 0x80): PNG = round(a·255/128), exactly invertible for 0..128;
  `Raw` (some alpha > 0x80; one picture in the corpus): PNG = a;
  `Zero` (all alpha 0; 105 PSMCT32 pictures): written as an opaque RGB PNG,
  so it can be seen and edited;
  `Opaque` (PSMCT24).
  Name, format and mode go into `tEXt` chunks for information only.
* `from_png(png, &like) -> Result<Image>` decodes any PNG: all colour types and
  bit depths, tRNS, Adam7. The format, flags and header come from `like`,
  and so does the alpha mode, so the export and import mappings match.
  Paletted targets work in this order:
  1. An indexed PNG with ≤ N colours is taken index for index, and its
     palette becomes the new palette. Short palettes are padded from `like`.
  2. Otherwise each colour is matched to `like`'s palette. Where duplicate
     entries exist, the original index at that pixel wins.
  3. Otherwise the picture is quantised with median cut in RGBA to N = 16 or
     256 colours, and pixels are mapped to the nearest colour.

  Mip levels are kept when the base level is unchanged, and otherwise
  rebuilt by 2×2 box filtering (then mapped to the palette).
* `write(images, Some(original)) -> Result<Vec<u8>>` re-encodes each picture into its
  entry. Unchanged headers, junk name bytes, scripts, entry fields, IM3 size
  quirks and the hidden swizzle bytes are all kept. When a TIM2 changes
  length, the following offsets (and the IM3 size field, by the same delta)
  are updated and 16-byte alignment is kept. Pictures with
  `index: None` are appended to IM3 archives. Pictures left out of the list
  are kept as they were; deletion is not supported. `write(images, None)`
  builds a fresh IM3 (version 3 TIM2 headers, exact total size).

## 10. Verification

Across all 5,217 `.img` files (15,228 pictures, 15,000 with pixels):

| check | result |
|---|---|
| `write(&read(data)?, Some(data)) == data` | **5,217 / 5,217** files |
| indexed/true-colour PNG export → `from_png(png, &like)` → write == data | **5,217 / 5,217** files (15,000 / 15,000 pictures identical; 228 `no_image` have no pixels to export) |
| paletted pictures exported as plain RGBA PNG → colour-matched back → write == data | **5,217 / 5,217** files |
| TIM2 header regenerated from the picture (rules of §4) == original | 15,000 / 15,000 |
| functional, 40 files: fresh archive reads back; RGBA edit (invert) → quantise → write → read; half-size resize keeps other pictures | 40/40, 40/40 (max error ≤ 8), 40/40 |

The whole check takes about 100 s on 16 processes.

## 11. Open problems

* **Not tested on hardware or an emulator.** The byte-exact round trip
  proves that the decode and encode are inverses, and pictures decode
  cleanly. Edited or resized files have only been checked against the
  engine source, not run in the game.
* **Alpha above 0x80** occurs in only one picture (`s510121`, alpha 0-255).
  The engine would use those values (> 1.0); whether that is intended is
  unknown. For the 105 all-zero-alpha PSMCT32 pictures, it is unclear
  whether alpha is really unused or the textures are drawn additively. The
  PNG export shows them opaque, and importing an opaque PNG gives alpha 0
  again.
* The **256×255** pictures (4, `map/f/f04/f04.ipk.d/f0401.img`): at load,
  `Conv32To8` walks whole 64-row pages and reads 256 bytes past the pixels
  (into the palette). The 128 pixels of the last rows that have no stored
  byte therefore show junk in the game. `im3.rs` decodes them as index 0 and
  keeps the 128 stored bytes that no pixel uses.
* **Junk fields** are preserved but unexplained: IM3 `size` short by 276 or
  302 (11 entries), CLAMP and `unk_32` junk (`e207a01`, `d01e01`), name
  bytes after the NUL, the IM2 entry tail, the bytes after the mip size
  table, and IMG header words 0x40 and 0x21 in `c14a01.img`.
* `IMG` archives with an entry count of 0 (`atgrow`, `atluck`) are unused
  by the engine. Their TIM2 is not returned by `read()`.
* TIM2 `picture format` = 5 (11,761 pictures) is not standard TIM2. The
  engine ignores it, and its meaning is unknown.
* PSMCT16 and 16- or 24-bit CLUTs never occur. PSMCT16 is implemented
  (A1B5G5R5) but untested. Other CLUT types are rejected.
* The engine needs about 4 VRAM blocks per palette plus whole pages per
  texture. Growing a texture can push later textures of the archive into
  the next block ("texture over"). Whether a given scene tolerates that
  has not been checked.
