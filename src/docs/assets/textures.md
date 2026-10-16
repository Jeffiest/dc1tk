# Textures: the code

How dc2tk reads and writes the game's texture archives. The format itself is
[the texture format](../../../docs/game/textures.md) (section numbers below are that page's), and
how mods use it is [editing textures](../../../docs/modding/textures.md).

`im3.rs` (in `src/crates/dc2tk-assets/src/`) implements the format, with `png.rs` (the PNG
codec) and `quantise.rs` (the palettes of PNGs of more colours), and `textures.rs` turns it into
the PNG folders mods use.

## Implementation choices

What the code keeps that the engine does not need, so that a file read and written back is the
file read:

* Name bytes after the NUL (junk in 6,207 of 16,406 entries): `write` keeps them, as it keeps
  every other junk field the format page lists under its open problems, unexplained as they are.
* The two IMG files with an entry count of 0 (`atgrow`, `atluck`): `read()` follows the engine
  and returns none, so their TIM2 is not returned; `write()` keeps the bytes.
* TIM2 total size (the converter quirks of section 4): `im3.rs` keeps whichever rule the original
  used; regenerating every header from the picture under that rule reproduces all 15,000 headers
  exactly.
* The 256×255 pictures (section 6.1): `im3.rs` keeps the 128 stored bytes that belong to no
  visible pixel from the original.

## `im3.rs`

`dc2tk_assets::im3`, in [`src/crates/dc2tk-assets/src/im3.rs`](../../crates/dc2tk-assets/src/im3.rs).

* `read(data) -> Result<Vec<Image>>` returns one `Image` per picture entry (scripts
  skipped), with `name`, `index` (entry number), `format`, `width`,
  `height`, `palette` (RGBA entries in index order, CSM1 undone, PS2
  alpha), `indices` (paletted, one byte per pixel) or `pixels` (RGBA bytes),
  `rgba()` (base level expanded), `mips`, `swizzled`, `no_image`, `block`,
  `clamp`, and `tim2` (the original bytes). An empty file gives no pictures.
* `to_png(image) -> Result<Vec<u8>>` writes the base level. Paletted
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

The PNG codec is `png.rs` (`write_png` writes 8-bit RGB, RGBA and indexed
PNGs; `read_png` reads every colour type, bit depth and interlacing), and the
median-cut palette is `quantise.rs` (`quantise(rgba, n)`).

## `textures.rs`: the PNG folders mods use

How the folders are named and what they accept is in
[editing textures](../../../docs/modding/textures.md); these are the functions behind it.

* `export(data)` gives a PNG per picture with pixels, named after the picture.
* `build(folder, original)` / `build_from(pngs, original)` put the PNGs a
  folder gives (`c02a.img.png/c02a01.png`) in place of those pictures, each
  through `from_png` with the original as `like`; pictures the folder leaves
  out stay as they are.
* `new_file(pngs, like, power_of_two)` makes a texture file of its own
  (`build_new`, for a mod's `file:` line with `like` giving a PNG or a folder
  of PNGs; `models::new_pack` for a new character's), always an IM3, every
  picture in the format of a template picture (`template_picture`: an
  entry, or the first with pixels). `with_new_pictures(pngs, original)` adds pictures at the end of an
  existing file in the format of its first picture with pixels, keeping its
  `#texanime` scripts (a map's new pictures, [editing maps](../../../docs/modding/maps.md)).
  `MAX_SIDE` is 1024.
* `same_pixels(a, b)` compares two PNGs by size and RGBA, however they are
  encoded (what a map's import uses to tell an edited picture).

## Verification

Across all 5,217 `.img` files (15,228 pictures, 15,000 with pixels):

| check | result |
|---|---|
| `write(&read(data)?, Some(data)) == data` | **5,217 / 5,217** files |
| indexed/true-colour PNG export → `from_png(png, &like)` → write == data | **5,217 / 5,217** files (15,000 / 15,000 pictures identical; 228 `no_image` have no pixels to export) |
| paletted pictures exported as plain RGBA PNG → colour-matched back → write == data | **5,217 / 5,217** files |
| TIM2 header regenerated from the picture (rules of [§4](../../../docs/game/textures.md#4-tim2-picture)) == original | 15,000 / 15,000 |
| functional, 40 files: fresh archive reads back; RGBA edit (invert) → quantise → write → read; half-size resize keeps other pictures | 40/40, 40/40 (max error ≤ 8), 40/40 |

The whole check takes about 100 s on 16 processes.

## Open problems

* **Not tested on hardware or an emulator.** The byte-exact round trip
  proves that the decode and encode are inverses, and pictures decode
  cleanly. Edited or resized files have only been checked against the
  engine source, not run in the game.
* The 105 all-zero-alpha PSMCT32 pictures: the PNG export shows them opaque, and importing an
  opaque PNG gives alpha 0 again.
* The 256×255 pictures: `im3.rs` decodes the 128 pixels of the last rows that have no stored
  byte as index 0 (the game shows junk there).
* PSMCT16 is implemented (A1B5G5R5) but untested. Other CLUT types are rejected.
