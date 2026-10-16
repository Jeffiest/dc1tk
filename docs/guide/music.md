# Music (MIDI)

`dc2tk setup` exports every music sequence (437 in all: background music `BG_*`, ambient loops `SR_*`, and event tunes `EV_*`) to `extracted/assets/music/` as a `.mid` file, with each bank's instruments as a `.sf2` SoundFont.

## Quick steps

1. Copy a music track and its SoundFont into your mod folder:
   ```sh
   dc2tk mod asset dc2.iso my-mod music:BG_000_000
   ```
   This writes `mods/my-mod/music/BG_000_000.mid` and the bank's instruments as `mods/my-mod/music/BG_000.sf2`, and adds the line to `mod.json`:
   ```json
   {
     "replace": {
       "music:BG_000_000": "music/BG_000_000.mid"
     }
   }
   ```
2. Edit the `.mid` file in a DAW or MIDI editor, or replace it with your own (Format 0 or 1, any tempo and tick division). Load `BG_000.sf2` to hear the game's instruments.
3. Build the ISO with `dc2tk build`.

Each map's music is listed by its sound set in `snd2/MAP_index.txt`; Palm Brinks' streets play `BG_000`.

## MIDI rules

| Rule | Details |
| --- | --- |
| **Instruments** | Set a Program Change on every channel that plays notes. Program numbers are the bank's own instruments, not General MIDI. |
| **Loop points** | Add markers (or text events) named `loopStart` and `loopEnd`. Without them, a track that looped on the disc loops from start to end. |
| **Channels** | Channel 0 carries the loop commands, so `dc2tk build` moves its notes to an unused channel. At most 15 channels can play notes. |
| **Ignored events** | Aftertouch, SysEx, Bank Select, and RPN/NRPN controllers. |

Unedited MIDI files build back to the exact bytes from the disc.

## Custom instruments

Add WAV files as instruments under `"instruments"` on a `music:` line, keyed by MIDI program number (`0`–`127`):

```json
{
  "replace": {
    "music:BG_000_000": {
      "file": "music/custom_title.mid",
      "instruments": {
        "80": { "file": "music/lead_synth.wav", "root": 60, "loop": true }
      }
    }
  }
}
```

| Option | Meaning |
| --- | --- |
| `"file"` | The WAV sample (as for [sound effects](sounds.md)). An instrument can also be a plain path, or a list of objects with `"keys"` for a multi-sample split. |
| `"root"` | The MIDI note at which the sample plays at its own pitch (default `60`). |
| `"loop"` | `true` loops the whole sample; otherwise it loops at its WAV `smpl` loop points, if any. |
| `"keys"` | The key range `[low, high]` a sample covers (default `[0, 127]`). |
| `"volume"` | `0` to `128` (default `128`). |
| `"like"` | A program of the bank to copy the envelope, pan, and priority from. |

A program number the bank already has is replaced.

## Add a new music file

Add `"music:BG_<n>"` under `"add"` with `"like"` naming an existing `BG_*` bank, to give a map or scene music of its own without replacing a tune used elsewhere:

```json
{
  "add": {
    "music:BG_201": {
      "file": "music/cult_theme.mid",
      "like": "BG_193",
      "instruments": {
        "100": { "file": "music/drone.wav", "root": 36, "loop": true }
      }
    }
  }
}
```

`dc2tk build` writes `snd2/bgm/BG_201.snd` with the template bank's instruments plus yours. `<n>` must be three digits and not already on the disc (the disc uses `0` to `195`). Play it from a script:

```c
load_bgm(201);
play_bgm(0);
```

The [spinning-cult example](../scripting/examples/spinning-cult/README.md) adds `BG_201` this way.

## Limits

A background music bank's samples must fit the music region of sound RAM: `0x78000` = 491,520 bytes. `dc2tk build` reports each bank's size, trims samples the new tune no longer plays, and refuses a bank that still does not fit. Most banks have room to spare (Palm Brinks' `BG_000` leaves about 265 KB free).

See [DC2 Internals: Music Sequences](../internals/music.md) for `.sq`, `.hd`, and `.bd` binary specifications.
