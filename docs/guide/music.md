# Music (MIDI)

`dc2tk setup` exports every background music track (`BG_*`) and ambient loop (`EN_*`) to `extracted/assets/music/` as a `.mid` file paired with a `.sf2` SoundFont of its instrument samples.

## Quick steps

1. Copy a music track and its SoundFont into your mod folder:
   ```sh
   dc2tk mod asset dc2.iso my-mod music:BG_000_000
   ```
2. Edit `mods/my-mod/music/BG_000_000.mid` in a DAW or MIDI editor (load `BG_000_000.sf2` to preview the track's instruments).
3. Verify the entry in `mods/my-mod/mod.json`:
   ```json
   {
     "replace": {
       "music:BG_000_000": "music/BG_000_000.mid"
     }
   }
   ```
4. Build the ISO with `dc2tk build`.

## Looping and custom instruments

| Feature | How to Configure |
| --- | --- |
| **Track loop points** | Add MIDI markers (or text events) named `loopStart` and `loopEnd` in the `.mid` file. |
| **Custom WAV instruments** | Attach `.wav` samples to MIDI program numbers (`0`–`127`) under `"instruments"` in `mod.json`. |
| **New music bank (`BG_<n>`)** | Add `"music:BG_<n>"` under `"add"` with `"like"` set to an existing `BG_*` bank, then call `load_bgm(<n>); play_bgm(0);` in scripts. |

### Example: Custom instruments and new music bank

```json
{
  "replace": {
    "music:BG_000_000": {
      "file": "music/custom_title.mid",
      "instruments": {
        "80": {"file": "music/lead_synth.wav", "root_key": 60, "loop": true}
      }
    }
  },
  "add": {
    "music:BG_201": {
      "file": "music/cult_theme.mid",
      "like": "BG_193",
      "instruments": {
        "100": {"file": "music/drone.wav", "root_key": 48, "loop": true}
      }
    }
  }
}
```

## Limits

| Bank Type | SPU2 Sample RAM Limit |
| --- | --- |
| **Background music (`BG_*`)** | `546,096 bytes` |
| **Ambient loop (`EN_*`)** | `236,336 bytes` |

See [DC2 Internals: Music Sequences](../internals/music.md) for `.sq`, `.hed`, and `.bd` binary specifications.
