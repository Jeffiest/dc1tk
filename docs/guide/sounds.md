# Sound Effects (WAV)

`dc2tk setup` exports all sound effects from the game's `.snd` banks into `extracted/assets/sounds/` as standard `.wav` files.

## Replace an existing sound effect

1. Find the sound in `extracted/assets/sounds/` (such as `SE_CURSOR_MOVE.wav`).
2. Copy it into your mod folder with `dc2tk mod asset`:
   ```sh
   dc2tk mod asset dc2.iso my-mod sound:SE_CURSOR_MOVE
   ```
3. Replace the `.wav` file with your own audio file.
4. Build your ISO. `dc2tk build` converts your WAV into PlayStation 2 SPU2 ADPCM audio and updates every sound bank that contains that sound.

In `mod.json`:

```json
{
  "replace": {
    "sound:SE_CURSOR_MOVE": "sounds/SE_CURSOR_MOVE.wav"
  }
}
```

## Add a new sound effect to a bank

To add a brand-new sound effect that your scripts can play with `play_se`, add a `"sound:<NAME>"` entry under `"add"` in `mod.json`:

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

`dc2tk build` assigns a free sound ID in that bank and exposes `SE_MOD_CALC_KEY` as a constant you can pass to `play_se(sound_handle, SE_MOD_CALC_KEY)` in your C scripts (see [`calculator.c`](../scripting/examples/calculator/calculator.c)).

## WAV requirements and limits

- **Format**: Standard PCM `.wav` (8/16/24/32-bit integer or 32-bit float, mono or stereo; stereo is mixed down to mono).
- **Loops**: If your `.wav` contains a standard `smpl` loop chunk, `dc2tk build` preserves the loop in SPU2 ADPCM.
- **Memory**: The PlayStation 2 has a fixed 2 MB SPU2 sound RAM budget divided across active sound banks. Keep replacement WAV files short or lower their sample rate (for example, `22050 Hz` or `11025 Hz`) if `dc2tk build` reports a sound memory overflow.
- For binary details on `.snd` banks, see [DC2 Internals: Sound Banks](../internals/sounds.md).
