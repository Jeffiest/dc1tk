# Sound Effects (WAV)

`dc2tk setup` exports all sound effects from the game's `.snd` banks into `extracted/assets/sounds/` as `.wav` files.

## Quick steps

1. Copy a sound effect into your mod folder:
   ```sh
   dc2tk mod asset dc2.iso my-mod sound:SE_CURSOR_MOVE
   ```
2. Replace `mods/my-mod/sounds/SE_CURSOR_MOVE.wav` with your edited `.wav` file.
3. Verify the entry in `mods/my-mod/mod.json`:
   ```json
   {
     "replace": {
       "sound:SE_CURSOR_MOVE": "sounds/SE_CURSOR_MOVE.wav"
     }
   }
   ```
4. Build the ISO with `dc2tk build`. `dc2tk` converts the WAV to PlayStation 2 SPU2 ADPCM audio and updates every sound bank that contains that sound.

## Add a new sound effect to a bank

Add a `"sound:<NAME>"` entry under `"add"` in `mod.json` to register a new sound effect:

```json
{
  "add": {
    "sound:SE_MOD_CALC_KEY": {
      "bank": "SP_042",
      "file": "sounds/key.wav",
      "volume": 127
    }
  }
}
```

`dc2tk build` assigns a free sound ID in `"bank"` and defines `SE_MOD_CALC_KEY` for `play_se(sound_handle, SE_MOD_CALC_KEY)` in C scripts (see [`calculator.c`](../scripting/examples/calculator/calculator.c)).

## WAV requirements and limits

| Property | Requirement |
| --- | --- |
| **Format** | Standard PCM `.wav` (8/16/24/32-bit integer or 32-bit float; stereo is mixed down to mono). |
| **Loops** | Standard `.wav` `smpl` loop chunks are preserved in SPU2 ADPCM. |
| **Memory** | Active sound banks share 2 MB of SPU2 RAM. Trim silence or downsample (`22050 Hz` or `11025 Hz`) if `dc2tk build` reports a memory overflow. |

See [DC2 Internals: Sound Banks](../internals/sounds.md) for `.snd` binary specifications.
