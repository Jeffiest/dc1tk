# Editing textures

This guide explains how to replace and add textures in a mod. You can edit a texture archive as PNG files, replace a single texture by its asset ID, or create a new texture archive and load it from a script.

For details on the binary format and engine rules, see [the texture format](../internals/textures.md). For implementation details, see [src/docs/assets/textures.md](../../src/docs/assets/textures.md).

## Textures as PNG

A texture archive (`.img`) holds multiple named textures. 3D models and menus find their textures by name.

To edit a texture archive by disc path:

1. Export the texture archive as a folder of PNG files:
   ```sh
   dc2tk mod asset ISO NAME chara/c02a.chr.d/c02a.img --as png
   ```
   This command writes a folder named `c02a.img.png/` with one PNG file per texture (for example, `c02a01.png`).
2. Edit the PNG files in any image editor.
3. Delete any PNG files that you did not change so that those textures stay unmodified.
4. Run `dc2tk build` to pack the edited PNG files back into the `.img` archive.

When `dc2tk build` compiles a PNG file, it matches pixel colors to the original palette if all colors fit. Otherwise, `dc2tk build` reduces the image to the original palette size (16 or 256 colors) and regenerates its mipmap levels. You can also change the width and height of the PNG image. Unedited PNG files compile back to the exact bytes from the disc.

### File naming rules

- Each PNG file has the same name as its texture entry (for example, `c02a01.png`).
- Characters that are not valid in file names become `_`.
- If two entries share the same name, the second file gets a `_${entry}` suffix (`_<entry>`).
- Render target entries (`no_image`) have no pixel data and are not exported.
- `dc2tk build` reports an error if a PNG file does not match any texture name in the archive, or if it targets a `no_image` render target.

## Replacing a picture by id

You can replace a single texture across the entire disc by referencing its asset ID in `mod.json`.

1. Place your replacement PNG file anywhere inside your mod folder.
2. Add a `"texture:<name>"` entry under `"replace"` in `mod.json`:
   ```json
   "replace": {
     "texture:e254a1": "art/soldier_green.png"
   }
   ```
3. Run `dc2tk build`. The tool replaces that texture in every archive on the disc that contains it.

For more information on asset IDs and `mod.json`, see [mods](mods.md).

## Pictures of a mod's own

Scripts load texture archives by their disc path. For example, `load_image(stack, "mods/calculator/calc_ui.img", 0, &block)` loads a texture archive, and `set_image_texture(image, "calc_body")` selects a texture from that archive by name.

You can build a new `.img` file from PNG files by adding a `file:` entry under `"add"` in `mod.json`. Use the `"like"` field to name an existing texture or `.img` file on the disc as a format template.

The calculator example ([`docs/scripting/examples/calculator`](../scripting/examples/calculator/)) builds its user interface texture archive with this entry:

```json
"add": {
  "file:mods/calculator/calc_ui.img": {"file": "ui", "like": "texture:logo_d00"}
}
```

### Rules for new texture archives

- **Input files**: The `"file"` field can point to a folder of PNG files or to a single PNG file. Each PNG file becomes one texture entry named after its filename without the extension (for example, `ui/calc_body.png` becomes `calc_body`).
- **Format template**: The `"like"` field must name a single texture ID (`texture:NAME`) or the disc path of a `.img` file. It cannot be a wildcard pattern. (The `"like"` field can also create a `.chr` model archive from a glTF file; see [editing models](models.md). All other file types are rejected.)
- **2D UI textures**: Using `"like": "texture:logo_d00"` (the title logo) creates 8-bit paletted textures with up to 256 RGBA colors, which matches the game's 2D UI images. If a PNG has more than 256 colors, `dc2tk build` reduces it to 256 colors and preserves transparency. `texture:sphida_bar` and `texture:e100_2d01` use this same format.
- **Template inheritance**: Every texture in a new `.img` file uses the format of the template texture. If `"like"` names a `.img` file path, `dc2tk build` uses the first texture entry in that file that has pixel data.
- **Texture names**: Each new texture name must be 1 to 31 Latin-1 characters long and must be unique (ignoring letter case).
- **Dimensions**: Width and height must be between 1 and 1024 pixels (the PS2 `TEX0` register supports `TW` and `TH` up to $2^{10}$). Swizzled 8-bit textures must have a width that is a multiple of 16. Textures for 3D models and maps must have power-of-two widths and heights, because the hardware wraps textures at the next power of two above their size.
- **Verification**: `dc2tk build` lists new archives in its output. You can extract a new archive from a built ISO to inspect it:
  ```sh
  dc2tk mod asset built.iso NAME mods/calculator/calc_ui.img --as png
  ```
  For design details, see [the design](../../src/docs/design.md#2-mods-by-name) ("Mods by name").

### Loading them from scripts

In a script, call `load_image(stack_no, file_name, image_no)` to load an `.img` file into event texture block `image_no` on memory stack `stack_no` and register it as event sprite image `image_no`. When you pass a fourth argument (`&block`), `load_image` writes the allocated texture block number to that variable instead (see [the SDK](../scripting/sdk.md)).

In `calculator.c`, the calculator mod loads its `.img` file once and displays the textures as screen sprites by name:

```c
load_image(MEMORY_STACK_EVENT, "mods/calculator/calc_ui.img", UI_IMAGE, &block);
open_sprite(SPRITE_FACE, "calc_body", block);       // init_screen_sprite(sprite, texture, block)
set_sprite_texture(SPRITE_CURSOR, "calc_cursor_wide", block);
delete_image(UI_IMAGE);
```

Keep custom textures small. The engine loads the `.img` file into the memory stack that the script specifies. Each 8-bit texture uses 1 byte per pixel plus 1 KB for its palette. For example, one 256x128 texture and one 32x32 texture produce a file of about 36 KB.

## Limits

The game engine finds textures by name. In `#texanime` animation scripts, the engine also addresses textures by pixel coordinates. Every edit must follow the rules in [the texture format](../internals/textures.md#8-writing-rules-what-a-modded-file-must-satisfy):

- Keep the original name of every replaced texture.
- Keep the original pixel dimensions of any texture that a `#texanime` script animates.
- Making a texture larger can push later textures in the same archive into the next VRAM texture block ("texture over"). `dc2tk build` checks VRAM block budgets automatically. See [the memory budgets](memory.md) for the block limits of each load layer.

## Checking

- Unedited PNG files build back to the exact bytes from the disc, so exporting a folder and building it without edits changes nothing.
- Run `dc2tk mod check` to verify that every entry in `mod.json` resolves (see [commands](commands.md)).
- Run `dc2tk mod asset built.iso NAME PATH --as png` to extract a texture archive from a built disc image and inspect the textures as the game will load them.
