# Sound Effects (WAV)

`dc2tk setup` exports all sound effects from the game's `.snd` banks into `extracted/assets/sounds/<bank>/` as `.wav` files.

## Quick steps

1. Copy a sound effect into your mod folder:
   ```sh
   dc2tk mod asset dc2.iso my-mod sound:SE_CURSOR_MOVE
   ```
   This writes `mods/my-mod/sounds/SY_000/000_CURSOR_MOVE.wav` (entry `0` of the system bank `SY_000`) and adds its line to `mod.json`:
   ```json
   {
     "replace": {
       "sound:SE_CURSOR_MOVE": "sounds/SY_000/000_CURSOR_MOVE.wav"
     }
   }
   ```
2. Replace the `.wav` file with your own sound.
3. Build the ISO with `dc2tk build`. `dc2tk` converts the WAV to PlayStation 2 SPU2 ADPCM audio and updates every copy of that sound on the disc.

A line can also set the sound's volume (`0` to `127`): `"sound:SE_CURSOR_MOVE": {"file": "sounds/click.wav", "volume": 90}`.

To edit a whole bank, copy it with `dc2tk mod asset dc2.iso my-mod bank:SP_003`, which writes one WAV per entry (`000.wav`, `001.wav`, ...) into `mods/my-mod/sounds/SP_003/`. Unedited WAV files build back to the exact bytes from the disc.

## Add a new sound effect to a bank

Add a `"sound:<NAME>"` entry under `"add"` in `mod.json` to append a new sound to a bank:

```json
{
  "add": {
    "sound:SE_MOD_CALC_KEY": {
      "bank": "SP_042",
      "file": "sounds/key.wav"
    }
  }
}
```

`dc2tk build` appends the sound to every copy of the bank and defines `SE_MOD_CALC_KEY` as its entry number, so your script patches can play it with `play_se(sound_handle, SE_MOD_CALC_KEY)` (see [`calculator.c`](../scripting/examples/calculator/calculator.c)). `dc2tk mod check` lists the constants it defines, and refuses a name that `pal.h` already gives to another entry.

## WAV requirements and limits

| Property | Requirement |
| --- | --- |
| **Format** | PCM `.wav` (8/16/24/32-bit integer or 32-bit float). Multi-channel files are mixed down to mono. |
| **Sample rate** | Up to `48000 Hz` (higher rates are resampled to 48 kHz). |
| **Loops** | A `smpl` loop chunk becomes the sound's loop. A WAV without loop points that replaces a looping sound loops whole. |
| **Memory** | Field, event, and monster banks share one region of sound RAM with almost no spare room. Keep a replaced bank near its original size: trim silence, or downsample to `22050 Hz` or `11025 Hz`. `dc2tk build` warns when a bank outgrows its region. |

See [DC2 Internals: Sound Banks](../internals/sounds.md) for `.snd` binary specifications.
