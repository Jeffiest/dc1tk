# Music (MIDI)

Background music (`BG_*`) and ambient loops (`EN_*`) in *Dark Chronicle* are MIDI-like sequences (`.sq`) paired with an instrument sample bank (`.hed`/`.bd`), not streamed audio files. `dc2tk setup` exports every track to `extracted/assets/music/` as a standard `.mid` MIDI file alongside a `.sf2` SoundFont of its instruments.

## Replace an existing music track

1. Copy the track into your mod with `dc2tk mod asset`:
   ```sh
   dc2tk mod asset dc2.iso my-mod music:BG_000_000
   ```
   This copies both the `.mid` file and its `.sf2` SoundFont into your mod folder.
2. Open the `.mid` file in any DAW or MIDI editor (such as Reaper, FL Studio, or MuseScore) and load the `.sf2` SoundFont to preview how it sounds with that track's instruments.
3. Save your edited `.mid` file and run `dc2tk build`.

In `mod.json`:

```json
{
  "replace": {
    "music:BG_000_000": "music/BG_000_000.mid"
  }
}
```

### Looping a track

To make a MIDI track loop seamlessly in-game, add two MIDI markers (or text events): `loopStart` where the loop begins, and `loopEnd` where it jumps back.

## Add custom WAV instruments to a track

If your MIDI uses instruments that are not in the original track's bank, you can attach `.wav` samples to specific MIDI program numbers (`0`–`127`):

```json
{
  "replace": {
    "music:BG_000_000": {
      "file": "music/custom_title.mid",
      "instruments": {
        "80": {"file": "music/lead_synth.wav", "root_key": 60, "loop": true}
      }
    }
  }
}
```

## Add a new music track (`BG_<n>`)

You can add a brand-new music bank (such as `BG_201`) so your custom maps or cutscenes can call `load_bgm(201); play_bgm(0);` without overwriting any retail music track:

```json
{
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

- All instrument samples in a `BG_*` bank must fit within **546,096 bytes** of SPU2 sound RAM (`236,336 bytes` for `EN_*` ambient banks). `dc2tk build` checks this automatically.
- For binary details on `.sq` sequences and instrument banks, see [DC2 Internals: Music Sequences](../internals/music.md).
