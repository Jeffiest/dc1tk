# Music

How a mod replaces the game's music and ambient loops with MIDI files, gives a tune new
instruments, and adds a music file of its own. How the disc stores the music and how the game
plays it is in [music and ambient loops](../game/music.md); the sound banks that hold the tunes'
instruments are in [sounds](sounds.md) and [the sound bank format](../game/sounds.md); `mod.json`
itself is in [mods](mods.md).

## Replacing a tune

All the background music, and the ambient loops, are sequences (`.sq`) inside sound banks, played
with the bank's instruments. The disc's 437 distinct sequences are the 437 `music:` assets. `dc2tk
mod asset dc2.iso mymod music:BG_000_000` writes `music/BG_000_000.mid` and the bank's
instruments as `music/BG_000.sf2`, with the line `"music:BG_000_000": "music/BG_000_000.mid"`;
`dc2tk mod asset dc2.iso mymod snd2/bgm/BG_000.snd --as mid` writes a folder `BG_000.snd.mid/`
with every sequence of the bank instead (ids are the sequence's name: Palm Brinks' streets play
`BG_000`, a map's tune is its sound set's in `snd2/MAP_index.txt`). Open the MIDI file in any
sequencer with the SoundFont loaded to hear it as the game plays it, edit it, or replace it with
your own tune (format 0 or 1, any tempo and division). Program numbers are the bank's
instruments, not General MIDI's: give every channel a program change to one the bank has. `build`
lays the tune out as the game's are (channel 0, the first, keeps the loop, so its notes move to a
free channel; each channel is given volume, expression, pan and bend before its first note, which
comes after them). The loop is between the markers `loopStart` and `loopEnd` (move them to move
it); a tune without them loops whole where the game's did. Unedited files build back to the disc's
own bytes.

`SOUND.DAT`, the streamed voices, cannot go in a mod.

## The MIDI file

An exported sequence is a standard MIDI file of format 0: one track, the sequence's division, its
events as they are (a note-off with velocity 64), its name as the track name, and markers
`loopStart` and `loopEnd` at the loop's points. Program numbers are the bank's programs, not
General MIDI's: to hear the file as the game plays it, load the SoundFont written for the bank
(preset n of banks 0 and 128 is program n, so channel 10 sounds the same in a General MIDI
player). It is an approximation for listening: LFOs, velocity curves, key follow and bend ranges
(256, taken as Sony's 2 semitones) are left out.

## How `build` makes the tune

`build` makes the `.sq` from a MIDI file, with the disc's sequence it replaces as the template:

- **Unedited** (the events read back equal the template's), the disc's bytes. Every one of the 447
  sequences exported and built back gives its own bytes.
- **Any MIDI file** of format 0 or 1 (or RIFF `.rmi`), any division: the tracks are merged by time
  (ties in track order) and the times scaled to 480 ticks a quarter note, the only division the
  disc uses. Kept: notes (a note-on of velocity 0 becomes a note-off), controllers, program
  changes, pitch bends and tempo. Left out: poly and channel pressure, system exclusive, other meta
  events, bank select (0, 32), and RPNs and NRPNs with their data entries (98 to 101, 6, 38, 96,
  97), which the driver would read as loop commands; only the disc's loop NRPNs are kept.
- **The loop**: the disc's NRPNs where the file has them; markers (or text events) `loopStart` and
  `loopEnd` (any case, any spaces or `_`), which win when they are not where the NRPNs are, so
  moving the markers in an editor moves the loop; else, where the disc's sequence looped, the
  whole tune from its start to its end of track. A loop is written as the disc writes it, on
  channel 0, and the end of track one beat after its end.
- **Tempo**: a file with no tempo at its start plays at 120 quarter notes a minute, as MIDI says,
  so a tempo event of 500,000 is put there.
- **Laid out as the disc's sequences** ([the layout](../game/music.md#events)): channel 0's
  messages move to a channel the file does not use when channel 0 plays notes (refused when all 16
  are used); when a note comes before tick 50, or the tune loops whole, the notes and everything
  after the start move 120 ticks on, the channels' set-up at the start (program changes,
  controllers, bends) goes to tick 10 and a tempo there stays (so a tune looped whole loops from
  tick 120, after its set-up); every note channel is given what it lacks before its first note of
  volume 100 (MIDI's), expression 127, pan centre and pitch bend centre, at ticks 20 to 50.
- **Refused**: a program the bank lacks (unless the disc's sequence names it too), a note on a
  channel that has had no program change, one loop marker without the other, an end before the
  start, format 2, SMPTE time.

## In a mod

- **By id**: `dc2tk mod asset dc2.iso mymod music:BG_000_000` writes `music/BG_000_000.mid` and
  `music/BG_000.sf2` into the mod and adds `"music:BG_000_000": "music/BG_000_000.mid"` to its
  `replace`. The replacement reaches every place the id has (the fishing packs' versions are
  `music:BG_193_000@fishing2`). In the asset tree each sequence is `music/<bank's
  folder>/<id>.mid` beside `<bank>.sf2`; `dc2tk mod make` takes an edited MIDI file from there.
- **By path**: `dc2tk mod asset dc2.iso mymod snd2/bgm/BG_000.snd --as mid` writes
  `disc/snd2/bgm/BG_000.snd.mid/` with a MIDI file per sequence (named after it) and the
  SoundFont; build replaces each sequence whose file is there and reads nothing else. A `.snd.mid`
  folder and a `.snd.wav` folder for one bank cannot both be given (use `sound:` ids for the
  effects).
- A sequence's samples and programs stay the bank's, so its sound memory does not change; the
  sequence itself goes to IOP memory, as the disc's biggest (44 KB) do. A tune plays with the
  SPU2's voices the game leaves the music (not measured): keep to the disc's few channels and
  short chords.

## New instruments

A `music:` line's `"instruments"` gives programs of the bank from WAV files, by the number the
MIDI file's program changes name (0 to 127):

```json
"music:BG_000_000": {"file": "music/tune.mid",
  "instruments": {"100": "music/organ.wav",
                  "101": {"file": "music/pulse.wav", "root": 69, "loop": true, "volume": 100, "like": 1},
                  "102": [{"file": "music/low.wav", "keys": [0, 59], "root": 48},
                          {"file": "music/high.wav", "keys": [60, 127], "root": 72}]}}
```

Each is put in before the tune is built, so its programs are checked against them: a program of
one split per sample over its `keys` (default all), each sample (encoded as the sound effects'
WAVs are, at its own rate up to 48 kHz) sounding at its own pitch at `root` (default 60), looping
where its WAV's `smpl` loop says or, with `"loop": true`, whole (a loop is made a whole number of
28-sample ADPCM blocks; a sample of whole periods and a multiple of 28 samples loops without a
click), at `volume` (0 to 128, default 128, the sample's own level). The envelope, pan and
priority are those of the bank's program `"like"` names (its first split and sample), else Sony's
defaults the sound effects use: attack 0, full sustain held, release rate 12. A number the bank
has already is replaced, and its samples cut to silent stubs when nothing plays them any more.
Instruments go in through `music:` lines only, not a `.snd.mid` folder.

**Memory.** A BG bank's samples load into the music region of SPU2 memory, 0x78000 bytes from
0x5210, one bank at a time (bd + 16 bytes; [memory](../game/memory.md)). `BG_056` takes 0x77E80
of it, but most banks leave far more: `BG_000` (Palm Brinks) 0x37170, 0x40B50 free. `build` notes
the samples' size before and after; when the new instruments do not fit, it cuts every sample
nothing plays any more (the old tune's programs, once the new tune no longer names them) to a
stub, and refuses the build if that is still not enough, saying by how much. The build's memory
check then counts the bank against the region as for any changed bank.

## A music file of a mod's own

A `music:` line under `add` makes a new music file, made from a MIDI file and one of the disc's
music banks, whose instruments it plays, so a mod's map or scene can play a tune of its own
without replacing one the game plays elsewhere:

```json
"add": {
  "music:BG_200": {"file": "music/fish.mid", "like": "BG_193"}
}
```

`"instruments"` may be given as on a `music:` replace line (above); left out, the tune plays the
like bank's own programs. The [spinning-cult example](../coding/examples/spinning-cult/) adds
`BG_201` like `BG_193`, with five instruments of its own (programs 100 to 104).

What `build` writes:

- A new file `snd2/bgm/BG_200.snd` (a `DATA.HD4` record, as a new map's files are), a copy of the
  like bank (`BG_193`, any of the disc's `snd2/bgm/BG_NNN.snd`) with its members renamed after it
  (`BG_200.hd`, `.bd`, `.cfg`, `.vol`) and one sequence, `BG_200_000.sq`: the MIDI file built with
  the like bank's entry 0 sequence as the template, exactly as a `music:` replace line builds it
  (the layout, the loop, the tempo, the program check against the bank and the instruments given).
- The `.cfg` keeps its header (with the new number) and `END`, and row 0, naming
  `BG_200_000.sq`; the like bank's other sequences and their rows (`BG_193_001`, the fishing
  battle) are dropped, so `play_bgm(0)` is the only entry. The `.vol` is the like bank's: the
  default volume of entry 0 (`BG_193`: 93, `BG_000`: 45) and the reverb. Pick a like bank whose
  default volume suits, or give one in the script (`play_bgm(0, 100)`).
- Every sample the new tune does not play is cut to a silent stub (as when a replace line's
  instruments need the room), so the bank takes no more music memory than its tune needs; `build`
  notes the sizes and checks the bank against the music region and the read buffer as for any
  changed bank. The like bank's programs stay, so the MIDI file plays the like bank's instruments
  (its SoundFont, `dc2tk mod asset ... music:BG_193_000`, is the one to hear it with) or ones the
  line gives.

Refused: a name that is not `BG_` and three digits (`load_bgm` takes 0 to 999 that way; the disc's
are 0 to 195, listed as `BGM_...` in `pal.h`), a number the disc has (replace its tune with a
`music:BG_NNN_000` line instead), a like bank the disc lacks or whose entry 0 plays no sequence,
two lines (two mods) adding one number, and a `file:` line or `disc/` file giving the same path.

### Playing it from scripts

A script plays it as it plays the game's music, by the number in its name:

```c
load_bgm(200);
play_bgm(0);
```

The engine keeps no table of music files: `load_bgm(n)` reads `snd2/bgm/BG_<n>.snd` (three digits
at least) by name, and `play_bgm(0)` plays its entry 0 at the default volume of the like bank's
`.vol` unless a volume is given (`play_bgm(0, 100)`);
[how the game loads it](../game/music.md#how-a-script-loads-a-music-file). Scripts write the number itself: it is in the line's key already, and `load_bgm` takes a plain
`int` (`bgm_file`), in strict mode (`#pragma std c89`) too. The spinning-cult example names it in
its header (`#define MUSIC_TURNING 201`) and switches to it with `stop_bgm(0);
allow_bgm_load(); load_bgm(MUSIC_TURNING); play_bgm(0);`. A map loads its sound set's tune when it
loads (the like map's, for a new map), so a map that should play the mod's tune calls `load_bgm`
in `map_start`.

## Checking

`dc2tk mod check --disc` builds the bank to check it. `dc2tk mod asset built.iso NAME
music:BG_200_000` copies the tune back out of a built image.
