# Editing textures

How a mod changes the game's pictures: editing a texture file as PNGs, replacing one picture by
its id, making a picture file of the mod's own and loading it from a script, the limits a picture
must keep, and how to check the result. The format and the rules the engine sets are in
[the texture format](../game/textures.md); the code is described in
[src/docs/assets/textures.md](../../src/docs/assets/textures.md).

## Textures as PNG

A texture file (`.img`) holds many named pictures, and models find theirs by name. `mod asset ISO
NAME chara/c02a.chr.d/c02a.img --as png` writes a folder `c02a.img.png/` with one PNG per picture
(`c02a01.png`). Edit them in any image editor and keep only the ones you change; the rest stay as
they are. `build` matches colours to the picture's palette when it can and otherwise reduces the
picture to its palette size (16 or 256 colours), makes its mip levels again, and accepts a new
size. Unedited PNGs build back to the disc's own bytes.

Each PNG is named after its picture (`c02a01.png`; characters a file name cannot hold become `_`,
and a name given twice gets `_<entry>`). `no_image` pictures are left out. A PNG naming no
picture, or a `no_image` render target, is an error.

## Replacing a picture by id

A mod can also name the picture it replaces by its asset id in `mod.json`, with the PNG anywhere
in its folder, and `build` replaces it at every place the disc has it:

```json
"replace": {
  "texture:e254a1": "art/soldier_green.png"
}
```

How ids work, and the other lines of `mod.json`, are in [mods](mods.md).

## Pictures of a mod's own

Scripts load pictures by disc path: `load_image(stack, "mods/calculator/calc_ui.img", 0,
&block)` loads a texture file, whose pictures `set_image_texture(image, "calc_body")` then asks
for by name. A mod makes such files from PNGs with `file:` lines that say which of the disc's
files to make them like. The calculator example
([`docs/coding/examples/calculator`](../coding/examples/calculator/)) makes its UI this way:

```json
"add": {
  "file:mods/calculator/calc_ui.img": {"file": "ui", "like": "texture:logo_d00"}
}
```

- **A picture file** takes a folder of PNGs (or one PNG): one picture each,
  named after its file (`ui/calc_body.png` is `calc_body`). `texture:logo_d00`,
  the title logo, makes them as the game's 2D pictures are: 8-bit with a
  palette of 256 colours with alpha, so a PNG of more colours is reduced
  to 256, and see-through pixels stay see-through. Sides are up to 1024
  texels, the width a multiple of 16. `texture:sphida_bar` and
  `texture:e100_2d01` are the same format.
- `"like"` makes a `.img` like a texture (`texture:NAME` or a texture file's disc path), one id
  and not a pattern. It also makes a `.chr` like a model, from a glTF: see
  [editing models](models.md). Anything else is refused.
- `build` lists them as new files; `dc2tk mod asset built.iso NAME
  mods/calculator/calc_ui.img --as png` copies them back out of a built image to check. The
  rules are in [the design](../../src/docs/design.md#2-mods-by-name), "Mods by name".

Every picture of a new file is made in the format of a template picture: the entry `like` names,
or the first of its file with pixels. A new picture's name is 1 to 31 Latin-1 characters, unique
ignoring case; its sides are 1 to 1024 (TEX0's TW and TH go to 2^10); its width is a multiple of
16 for swizzled 8-bit; and its sides are powers of two for a model's or a map's pictures, which
wrap at the power of two above their size.

### Loading them from scripts

`load_image(stack_no, file_name, image_no)` loads an `.img` file into event texture block
`image_no`, using memory stack `stack_no`, and registers it as event sprite image `image_no`;
with a fourth argument the texture block number is written into it instead (see
[the SDK](../coding/sdk.md)). The calculator loads its file once and shows the pictures as screen
sprites by name (`calculator.c`):

```c
load_image(MEMORY_STACK_EVENT, "mods/calculator/calc_ui.img", UI_IMAGE, &block);
open_sprite(SPRITE_FACE, "calc_body", block);       // init_screen_sprite(sprite, texture, block)
set_sprite_texture(SPRITE_CURSOR, "calc_cursor_wide", block);
delete_image(UI_IMAGE);
```

Keep them small: a picture file loads into the memory stack the script
names, and an 8-bit picture takes a byte a texel and 1 KB of palette, so a
256x128 and a 32x32 picture make a file of about 36 KB.

## Limits

The engine finds a picture by name and, in a `#texanime` animation script, by pixel rectangle, so
the rules in [the texture format](../game/textures.md#8-writing-rules-what-a-modded-file-must-satisfy)
apply to every edit. In practice:

- Keep a picture's name, and keep the size of a picture a `#texanime` script animates.
- Growing a picture can push the later textures of its archive into the next texture block
  ("texture over", in the format page's open problems). How many blocks each load has is in
  [the memory budgets](memory.md), and `build` checks it.

## Checking

- Unedited PNGs build back to the disc's own bytes, so a folder exported and built unchanged
  changes nothing.
- `mod check` checks every line of `mod.json` resolves (see [commands](commands.md)).
- `dc2tk mod asset built.iso NAME PATH --as png` copies a texture file back out of a built image,
  to see the pictures as the game will have them.
