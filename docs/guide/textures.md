# Textures (PNG)

`dc2tk setup` exports all game textures as `.png` files in `extracted/assets/textures/`.

## Quick steps

1. Copy a texture into your mod folder by its asset ID (listed in `extracted/assets/manifest.json`):
   ```sh
   dc2tk mod asset dc2.iso my-mod texture:e254a1
   ```
2. Edit `mods/my-mod/textures/e254a1.png` in an image editor.
3. Verify the entry in `mods/my-mod/mod.json`:
   ```json
   {
     "replace": {
       "texture:e254a1": "textures/e254a1.png"
     }
   }
   ```
4. Build the ISO with `dc2tk build`. `dc2tk` converts the PNG to TIM2 format, regenerates mipmaps, and updates all copies on the disc.

## Export an entire `.img` archive

To export all textures from one `.img` archive as a folder of PNGs:

```sh
dc2tk mod asset dc2.iso my-mod chara/c02a.chr.d/c02a.img --as png
```

## Add a new texture archive (`.img`)

Create a new `.img` archive from a folder of PNGs using `"add"` in `mod.json` (used with `load_image` and `open_sprite` in scripts):

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

| Field | Description |
| --- | --- |
| `"file": "ui"` | Packs each `.png` in `mods/<name>/ui/` as a named texture (for example, `ui/calc_body.png` becomes `"calc_body"`). |
| `"like": "texture:logo_d00"` | Formats textures as 8-bit 256-color paletted images with transparency for 2D UI sprites. |

See [`calculator.c`](../scripting/examples/calculator/calculator.c) for a working example.

## Rules and limits

| Texture Type | Requirement |
| --- | --- |
| **3D model and map textures** | Power-of-two width and height (`64x64`, `128x128`, `256x256`). |
| **2D UI textures** | Width must be a multiple of 16, up to `1024x1024`. |
| **Animated map textures (`#texanime`)** | Keep the original pixel dimensions. |

See [DC2 Internals: Textures](../internals/textures.md) for `.img` and TIM2 binary specifications.
