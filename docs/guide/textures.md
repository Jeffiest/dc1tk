# Textures (PNG)

`dc2tk setup` exports all game textures as `.png` files in `extracted/assets/textures/`.

## Replace an existing texture

1. Find the texture in `extracted/assets/textures/` (or look up its `texture:<name>` ID in `extracted/assets/manifest.json`).
2. Copy it into your mod folder with `dc2tk mod asset`:
   ```sh
   dc2tk mod asset dc2.iso my-mod texture:e254a1
   ```
3. Edit the `.png` file in any image editor (such as Paint.NET, GIMP, or Aseprite).
4. Build your ISO. `dc2tk build` automatically converts your PNG back into the game's TIM2 palette format, regenerates mipmaps, and replaces the texture everywhere it appears on the disc.

In `mod.json`, a texture replacement looks like this:

```json
{
  "replace": {
    "texture:e254a1": "textures/e254a1.png"
  }
}
```

You can also export all textures from a specific `.img` archive into a folder of PNGs:

```sh
dc2tk mod asset dc2.iso my-mod chara/c02a.chr.d/c02a.img --as png
```

## Add a new texture archive (`.img`)

If your mod's script loads a custom 2D UI sprite sheet (with `load_image` and `open_sprite`), create a new `.img` archive from a folder of PNGs using `"add"` in `mod.json`:

```json
{
  "add": {
    "file:mods/calculator/calc_ui.img": {
      "file": "ui",
      "like": "texture:logo_d00"
    }
  }
}
```

- Each `.png` file in the `ui/` folder becomes one named texture in `calc_ui.img` (for example, `ui/calc_body.png` becomes `"calc_body"`).
- `"like": "texture:logo_d00"` formats the textures as 8-bit 256-color paletted images with transparency, matching the game's 2D UI sprites.
- See [`calculator.c`](../scripting/examples/calculator/calculator.c) for a complete example of loading and drawing custom UI sprites in a script.

## Rules and limits

- **3D model and map textures**: Must have power-of-two width and height (such as `64x64`, `128x128`, or `256x256`).
- **2D UI textures**: Width must be a multiple of 16, up to `1024x1024`.
- **Animated map textures (`#texanime`)**: Keep the original pixel dimensions for textures that a map animates by rectangle coordinates.
- For binary details on `.img` and TIM2 files, see [DC2 Internals: Textures](../internals/textures.md).
