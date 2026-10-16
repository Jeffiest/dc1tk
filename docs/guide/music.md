# Music

This guide explains how to replace the game's background music and ambient loops with MIDI files, add custom WAV instruments to a track, and create new music banks for your mod.

For details on how the disc stores music and how the engine plays it, see [music and ambient loops](../internals/music.md). For details on the sound banks that hold the instruments, see [sounds](sounds.md) and [the sound bank format](../internals/sounds.md). For `mod.json` syntax, see [mods](mods.md).

## Replacing a tune

All background music and ambient loops are sequence files (`.sq`) stored inside sound banks (`.snd`). They play using the instruments stored in the same sound bank. The disc contains 437 distinct sequences, which correspond to the 437 `music:` asset IDs.

To replace a music track by its asset ID:

1. Export the music track and its SoundFont:
   ```sh
   dc2tk mod asset dc2.iso mymod music:BG_000_000
   ```
   This command writes the MIDI file `music/BG_000_000.mid` and the bank's instruments as `music/BG_000.sf2`, and adds `"music:BG_000_000": "music/BG_000_000.mid"` to `mod.json`.
   (Alternatively, run `dc2tk mod asset dc2.iso mymod snd2/bgm/BG_000.snd --as mid` to export a folder `BG_000.snd.mid/` containing every sequence in that bank. Sequence IDs match the sequence names: for example, Palm Brinks' streets play `BG_000`, and each map's music is listed by its sound set in `snd2/MAP_index.txt`.)
2. Open the MIDI file in any MIDI sequencer or DAW and load the `.sf2` SoundFont to hear the track with the game's instruments.
3. Edit the MIDI file or replace it with your own MIDI file (Format 0 or Format 1, any tempo and tick division).
4. Set a MIDI Program Change on every active channel to select one of the bank's instruments. Program numbers refer to the bank's own instruments, not General MIDI presets.
5. Set the loop points using the MIDI markers `loopStart` and `loopEnd`. If you omit these markers, `dc2tk build` loops the entire track if the original sequence looped.
6. Run `dc2tk build`.

`dc2tk build` formats the sequence to match the game's layout: channel 0 (the first channel) holds the loop markers, so any notes on channel 0 move to an unused channel, and every channel receives initial volume, expression, pan, and pitch bend events before its first note. Unedited MIDI files build back to the exact bytes from the disc.

> **Note**
> `SOUND.DAT` (streamed voice audio) cannot be modified in a mod.

## The MIDI file

An exported sequence is a standard Format 0 MIDI file with a single track. It preserves the sequence's original tick division, exact events (note-off events use velocity 64), the sequence name as the track name, and `loopStart` and `loopEnd` markers at the loop points.

Because program numbers correspond to the bank's programs rather than General MIDI instruments, load the exported `.sf2` SoundFont in your MIDI player or DAW to hear the correct instruments. In the SoundFont, preset `n` in both bank 0 and bank 128 maps to program `n`, so channel 10 also plays the correct instrument in General MIDI players. The SoundFont is a preview approximation: LFOs, velocity curves, key follow, and custom pitch bend ranges (stored as 256, approximated as Sony's default of 2 semitones) are not included.

## How `build` makes the tune

`dc2tk build` compiles the `.sq` sequence from a MIDI file using the original sequence on the disc as a template:

- **Unedited files**: If the MIDI events match the template sequence's events, `dc2tk build` writes the exact bytes from the disc. All 447 sequences on the disc export and rebuild to their original bytes.
- **Supported MIDI formats**: Accepts Format 0 or Format 1 MIDI files (or RIFF `.rmi` files) with any tick division. `dc2tk build` merges all tracks by time (breaking ties in track order) and scales event times to 480 ticks per quarter note, which is the only division used on the disc.
  - **Kept events**: Notes (note-on with velocity 0 becomes note-off), controllers, program changes, pitch bends, and tempo events.
  - **Omitted events**: Polyphonic and channel pressure (aftertouch), SysEx messages, other meta events, Bank Select controllers (CC 0 and CC 32), and RPN/NRPN controllers with their Data Entry events (CC 98 to 101, 6, 38, 96, 97), because the sound driver interprets NRPNs as loop commands. Only the disc's original loop NRPNs are kept.
- **The loop**:
  - Uses the disc's loop NRPNs if they are still present at the same positions.
  - Uses the `loopStart` and `loopEnd` markers or text events (case-insensitive, ignoring spaces and `_`). When the markers differ from the NRPN positions, the markers win, so moving the markers in a MIDI editor moves the loop.
  - Otherwise, if the template sequence looped, `dc2tk build` loops the entire track from start to end-of-track.
  - `dc2tk build` writes the loop commands on channel 0 and places the end-of-track event one beat after `loopEnd`.
- **Tempo**: If a MIDI file has no tempo event at tick 0, `dc2tk build` inserts a tempo of `500,000` microseconds per quarter note (120 BPM, the standard MIDI default).
- **Channel and tick layout** ([the layout](../internals/music.md#events)):
  - If channel 0 plays notes, `dc2tk build` moves channel 0's events to an unused channel (and fails if all 16 channels play notes).
  - If any note starts before tick 50, or if the track loops from start to end without markers, `dc2tk build` shifts all notes and subsequent events forward by 120 ticks, places initial channel setup events (program changes, controllers, pitch bends) at tick 10, and keeps the initial tempo at tick 0. This ensures that a whole-track loop restarts at tick 120, after the setup events.
  - Every note channel receives any missing setup controllers between ticks 20 and 50 before its first note: volume `100` (MIDI CC 7), expression `127` (CC 11), center pan, and center pitch bend.
- **Refused (build errors)**:
  - Selecting a program number that the bank does not have (unless the original sequence on the disc also selected it)
  - Playing a note on a channel that has not received a program change
  - Including only one of `loopStart` or `loopEnd`, or placing `loopEnd` before `loopStart`
  - Format 2 MIDI files or SMPTE time division

## In a mod

- **By ID**: Running `dc2tk mod asset dc2.iso mymod music:BG_000_000` writes `music/BG_000_000.mid` and `music/BG_000.sf2` into the mod folder and adds `"music:BG_000_000": "music/BG_000_000.mid"` under `"replace"` in `mod.json`. Replacing by ID updates every copy of that sequence on the disc (variants in the fishing packs have separate IDs such as `music:BG_193_000@fishing2`). In the exported asset tree, each sequence is stored at `music/<bank's folder>/<id>.mid` alongside `<bank>.sf2`, and `dc2tk mod make` picks up edited MIDI files from there automatically.
- **By path**: Running `dc2tk mod asset dc2.iso mymod snd2/bgm/BG_000.snd --as mid` creates `disc/snd2/bgm/BG_000.snd.mid/` containing one MIDI file per sequence and the bank's SoundFont. `dc2tk build` replaces each sequence that has a MIDI file in the folder and leaves the others untouched. You cannot use both a `.snd.mid` folder and a `.snd.wav` folder for the same sound bank (use `sound:` IDs in `mod.json` for sound effects instead).
- **Memory and voice limits**: Replacing only the MIDI sequence leaves the bank's audio samples and instrument programs unchanged, so SPU2 sound memory usage does not change. The `.sq` sequence loads into IOP memory (the largest on the disc is 44 KB). Music shares the PS2 SPU2 hardware voices with sound effects (exact voice reserve not measured), so stick to a small number of channels and moderate chord polyphony like the original tracks.

## New instruments

You can add or replace instrument programs in a music bank from WAV files by adding an `"instruments"` object to a `music:` entry in `mod.json`, keyed by the MIDI program number (`0` to `127`):

```json
"music:BG_000_000": {"file": "music/tune.mid",
  "instruments": {"100": "music/organ.wav",
                  "101": {"file": "music/pulse.wav", "root": 69, "loop": true, "volume": 100, "like": 1},
                  "102": [{"file": "music/low.wav", "keys": [0, 59], "root": 48},
                          {"file": "music/high.wav", "keys": [60, 127], "root": 72}]}}
```

### Instrument options

`dc2tk build` adds custom instruments to the bank before compiling the MIDI file, so your MIDI file can use the new program numbers immediately:

- **Key splits (`"keys"`)**: Each program has one split per sample over its `"keys"` range (defaults to `[0, 127]`, all keys). Pass a list of objects to create a multi-sample split instrument (like program `"102"` above).
- **Audio format and pitch (`"root"`)**: Each WAV file is encoded in the same way as sound effect WAV files, at its own sample rate up to 48 kHz. It plays at its original pitch at the MIDI note `"root"` (default `60`, middle C).
- **Looping (`"loop"`)**: A sample loops at the points in its WAV `smpl` chunk, or loops its entire length if `"loop": true` is set. `dc2tk build` aligns every loop to a whole number of 28-sample ADPCM blocks. A sample containing a whole number of wave periods whose length is a multiple of 28 samples loops cleanly without a click.
- **Volume (`"volume"`)**: Sets the sample volume from `0` to `128` (default `128`, the sample's full level).
- **Envelope and attributes (`"like"`)**: Copies the ADSR envelope, pan, and priority from the first split and sample of program `"like"` in the same bank. If `"like"` is omitted, `dc2tk build` uses Sony's default ADSR settings (the same defaults used by sound effects): attack `0`, full sustain held, and release rate `12`.
- **Replacing existing programs**: If the bank already has a program with that number, `dc2tk build` replaces it and trims its old samples to silent stubs if no other program or sound uses them.
- **Where supported**: Custom `"instruments"` can only be specified on `music:` entries in `mod.json`, not in a `.snd.mid` folder.

### Instrument memory budget

A background music (`BG`) bank loads its samples into the music region of SPU2 memory (`0x78000` bytes starting at `0x5210`), one bank at a time (`.bd` size + 16 bytes; see [memory](../internals/memory.md)). While `BG_056` uses `0x77E80` bytes, most banks leave plenty of free space (for example, `BG_000` in Palm Brinks uses `0x37170` bytes, leaving `0x40B50` bytes free).

`dc2tk build` reports the sample data size before and after adding instruments. If the new instruments do not fit in the music region, `dc2tk build` trims every unused sample in the bank (such as instruments from the old tune that your new MIDI file no longer uses) to a silent stub. If the bank still exceeds the region limit, `dc2tk build` refuses the build and reports how many bytes over budget it is.

## A music file of a mod's own

You can add a new music bank under `"add"` in `mod.json` using a MIDI file and a template (`"like"`) music bank from the disc. This lets a custom map or scene play its own music without replacing an existing track used elsewhere in the game:

```json
"add": {
  "music:BG_200": {"file": "music/fish.mid", "like": "BG_193"}
}
```

You can also include `"instruments"` just like on a `"replace"` entry. If you omit `"instruments"`, the new track uses the template bank's instruments. The [spinning-cult example](../scripting/examples/spinning-cult/) adds `BG_201` using `"like": "BG_193"` with five custom instruments (programs `100` to `104`).

### What `build` writes for a new music bank

- **New `.snd` archive**: Writes a new file `snd2/bgm/BG_200.snd` (stored as a `DATA.HD4` record, like new map files). It copies the `"like"` bank (`BG_193`, or any `snd2/bgm/BG_NNN.snd` on the disc), renames its internal members (`BG_200.hd`, `.bd`, `.cfg`, `.vol`), and compiles a single sequence `BG_200_000.sq` from your MIDI file using entry 0 of the `"like"` bank as its template.
- **Configuration (`.cfg`) and volume (`.vol`)**: Keeps the `.cfg` header (with the new bank number), row 0 (naming `BG_200_000.sq`), and `END`. Any other sequences and rows from the template bank (such as `BG_193_001`, the fishing battle tune) are removed, so `play_bgm(0)` is the only entry. The `.vol` member is copied from the template bank, which sets the default volume of entry 0 (`93` in `BG_193`, `45` in `BG_000`) and the reverb settings. Pick a template bank with a suitable default volume, or pass an explicit volume in your script (`play_bgm(0, 100)`).
- **Sample trimming**: Trims every sample that the new tune does not play down to a silent stub, so the bank uses only the SPU2 music memory that its tune requires. `dc2tk build` logs the sizes and checks the bank against the SPU2 music region and read buffer limits. Because the template bank's instrument programs remain intact, your MIDI file can use any instrument from the template bank (export its SoundFont with `dc2tk mod asset ... music:BG_193_000` to preview them) or any custom `"instruments"` you define.
- **Refused (build errors)**:
  - A bank name that is not `BG_` followed by three digits (`load_bgm` accepts `0` to `999`; the disc uses `0` to `195`, listed as `BGM_...` in `pal.h`)
  - A bank number that already exists on the disc (replace its tune with `music:BG_NNN_000` under `"replace"` instead)
  - A `"like"` bank that does not exist on the disc or whose entry 0 does not play a sequence
  - Two mods adding the same `BG_` number
  - A `file:` entry or `disc/` file targeting the same `snd2/bgm/BG_NNN.snd` path

### Playing it from scripts

A script loads and plays a custom music bank by the number in its name, just like built-in music:

```c
load_bgm(200);
play_bgm(0);
```

The game engine does not use a fixed lookup table for music files: `load_bgm(n)` loads `snd2/bgm/BG_<n>.snd` (zero-padded to at least three digits) by filename, and `play_bgm(0)` plays entry 0 at the default volume stored in `.vol` unless you pass a volume argument (`play_bgm(0, 100)`). For details, see [how the game loads it](../internals/music.md#how-a-script-loads-a-music-file).

Scripts pass the integer number directly (`load_bgm` accepts a plain `int` / `bgm_file`, including in `#pragma std c89` strict mode). For example, the spinning-cult mod defines `#define MUSIC_TURNING 201` in its header and switches music with:

```c
stop_bgm(0);
allow_bgm_load();
load_bgm(MUSIC_TURNING);
play_bgm(0);
```

When a map loads, the engine automatically loads the music specified by the map's sound set (or the `"like"` map's sound set for a new map). If a map should play your custom music immediately, call `load_bgm` inside `map_start`.

## Checking

- Run `dc2tk mod check --disc` to test-build the music bank and verify all limits.
- Run `dc2tk mod asset built.iso NAME music:BG_200_000` to extract the compiled tune back out of a built disc image.
