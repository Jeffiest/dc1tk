# Dark Chronicle music and ambient loops (`.sq`)

How the PAL disc stores its background music and ambient loops, how the game
plays them, and how a mod replaces them with MIDI files. `sequence.rs` (in
`src/crates/dc2tk-assets/src/`) reads and writes the `.sq` files, `music.rs`
converts them to and from standard MIDI files, `soundfont.rs` writes a bank's
instruments as a SoundFont to hear them with. The sound banks that hold the
sequences and their instruments are in [the sound bank notes](sounds.md).

Evidence, as there: **engine** (the decompiled EE code in
`finaldc2/ChronicleTwo/ps2`: `src/sound.cpp`, `src/scenesnd.cpp`,
`src/ezbgm.cpp`, `src/event_func.cpp` and the asm of `PlayBGM`), **data**
(statistics over the 447 `.sq` files of `extracted/disc`, all of which the
code reads and writes back byte for byte) and **Sony** (the IOP plays
sequences with Sony's MIDI sequencer modules, which were not decompiled; what
their events mean is the MIDI standard's and, for the loop controllers,
inferred from where the disc puts them).

## 1. What is music on the disc

All background music is sequenced: each tune is a `.sq` member of a sound
bank (`.snd`), played by the IOP with that bank's instruments. Nothing musical
is streamed.

| Where | Banks | Sequences | Use |
|---|---|---|---|
| `snd2/bgm/BG_NNN.snd` | 73 | 76 (70 banks one, 3 two) | background music, one tune a bank |
| `snd2/env/SR_*.snd` | 96 | 352 (1 to 8 a bank; 66 banks four) | ambient loops (wind, water, crowds) |
| `snd2/event/EV_*.snd` | 6 | 7 | event sounds played as sequences |
| `sg/fish/fishing2*.pak` | 6 copies of `BG_193.snd` | 12 | the fishing music, a second version of both tunes |

447 places, 435 names, 437 distinct sequences (278 by their bytes alone: many
ambient loops are copied under other names; the fishing packs' copies of
`BG_193_000` and `BG_193_001` differ from `snd2/bgm`'s): the 437 `music:`
assets.

**Background music** (engine): `CScene::LoadBGM` reads
`snd2/bgm/BG_%03d.snd` (`GetBgmFile`) and loads it on game port 0 or 11 (two
slots, for changing tunes), whose sequences go to IOP memory (`LoadSeq`, a
block of the file's size + 256). `PlayBGM(entry, volume, fade)` plays an
entry of that bank as `play_se` does (`sndSePlayV`): the bank's `.cfg` row
names the `.sq` (`BG_000_000.sq`), so the entry plays the sequence with
`SQ_Play`, at the row's default volume from the `.vol` (`BG_000`: 45) unless
the caller gives one. A map's sound set (the map list's
column and `snd2/MAP_index.txt`) names its default tune (`GetDefBgmNo`);
scripts start others (`start_bgm`). Palm Brinks' streets m01 to m05 use set
1000, whose tune is `BG_000`.

**Ambient loops**: the sound set's ambient bank (`SR_*`, port 2) and the
sequence the set names (`SR_010_000`), or one the time of day picks; event
banks (port 4) play theirs as entries.

**Streamed audio** (engine, data): `SOUND.DAT` (969,644,032 bytes) holds
3,102 RIFF WAVE files, 16-bit PCM mono at 48 kHz with `bext`, `minf` and
`elmo` chunks, indexed by `SOUND.HD3` (16-byte records: name offset, size,
sector, sector count; names `0010010.wav`, numbered by voice). The IOP's
EZBGM server streams them (`CSound::StreamOpenFast`, `ezBgm`): event dialogue
(`CommandStreamOpen`), the chapter titles' narration (`menucapt.cpp`), the
robot's voice (`CRoboVoiceSystem`) and the minigames' voices. They are voices,
not music; the movies (`MOVIE/*.PSS`) carry their own sound. `SOUND.DAT`
cannot go in a mod.

## 2. The `.sq` file

Little-endian, Sony's chunk layout (as the `.hd`'s): creator `IECS`, the
chunk type reversed, the chunk's size. Every file on the disc has exactly
this header; `Sequence::read` refuses anything else.

| Off | Size | Field | Value |
|---|---|---|---|
| 0x00 | 16 | `Vers` chunk: `IECS` `sreV` 0x10, reserved u16, version 1.0 | fixed |
| 0x10 | 12 | `Sequ` chunk header: `IECS` `uqeS` 0x20 | fixed |
| 0x1C | 4 | file size | |
| 0x20 | 16 | offsets of the song, MIDI, SE-sequence and SE-song chunks | 0xFFFFFFFF, 0x30, 0xFFFFFFFF, 0xFFFFFFFF |
| 0x30 | 12 | `Midi` chunk header: `IECS` `idiM`, size = file size - 0x30 | |
| 0x3C | 4 | max index of the data blocks | 0: one block |
| 0x40 | 4 | its offset from the chunk | 0x14 |
| 0x44 | 4 | the events' offset from the block | 6 |
| 0x48 | 2 | division (ticks per quarter note) | 480 in all 447 |
| 0x4A | | the events, one track, ending with end of track | |
| | | 0xFF padding to a multiple of 16 bytes | |

### Events

MIDI events, each a delta time (a variable-length quantity, always in as few
bytes as it needs) and a message, compressed three ways:

- **Running status**: a channel message whose status byte equals the last one
  written leaves it out; a meta event ends running status (the next channel
  message writes its status). It is used wherever it can be (182,192 times).
- **Note-offs carry only the key**: `8n kk`, no velocity. Notes end with
  note-offs, never note-ons of velocity 0.
- **Zero delta times left out**: a channel message that wrote its status byte
  sets bit 7 of its last data byte when the next event's delta is 0, and that
  delta is not written. A message in running status never sets it (its first
  byte would then read as a status byte, for a one-byte message), so after one
  a delta of 0 is written out; a meta event never sets it either. Over the
  disc 174,966 zero deltas are left out and 29,088 are written after a
  message in running status. This rule leaves nothing to chance: a sequence is
  its events, and writing them gives the disc's bytes for all 447 files.

Meta events are `FF type length data` (length a variable-length quantity):
tempo (`FF 51 03`, microseconds per quarter note; 473, at tick 0 in 443
sequences, none in 4 ambient ones) and end of track (`FF 2F 00`). No system
exclusive, no other meta event.

What the 527,181 events are:

| Message | Count |
|---|---|
| note-on / note-off | 132,552 each |
| controller 10 (pan) | 136,826 |
| controller 11 (expression) | 110,360 |
| controller 7 (volume) | 2,073 |
| controller 99 (NRPN MSB), 6 (data entry), 38 (data entry LSB) | 873, 873, 436: the loops |
| program change | 2,087 |
| pitch bend | 7,629 |
| tempo, end of track | 473, 447 |

No poly or channel pressure, no bank select, no other controller. Sizes run
from 128 bytes to 44,128 (`BG_118_000`).

**How a sequence is laid out** (data, the 435 sequences of `snd2/`, 2,024
channels that play notes):

- channel 0 never plays a note: it carries the loop's NRPNs (or nothing),
  and no sequence plays notes on its loop's channel;
- every note channel is set up before its first note, spread over ticks 10
  to 50: program change, expression (controller 11; 127 in 1,974), pan (10)
  in all 2,024; volume (7) in 2,005; pitch bend centre in 2,019. Only 58 put
  any of it at the first note's tick;
- the first note comes at tick 50 or later (120 in 298 sequences, 60 in 72,
  never 0); the loop starts after the set-up in 328 of 422, often just after
  the first note (10 ticks after it in 283).

Which of these the IOP's sequencer needs is not known (`MODMIDI.IRX`,
`MODHSYN.IRX` and the game's `EZMIDI.IRX` server are not decompiled; their
strings show loop tables, "not found loop start", running status and tempo
handling, nothing on channel defaults). A tune with notes on channel 0, no
expression and its first notes and set-up at tick 0 with the loop's start
played silent in Palm Brinks, so a MIDI file is built to this layout
(section 3).

### Loops

A loop is two NRPNs (data, positions): NRPN 0 (`B0 63 00`, then `B0 06 00`
ten ticks later) marks the start, NRPN 1 (`B0 63 01`, `B0 06 00` and
`B0 26 00`, ten ticks apart) the end. In the music the span from the start's
data entry to the end's LSB is whole bars (`BG_193_000`: 53,760 ticks, 28 bars
of 4/4), so those two events are taken as the loop points; the sequencer
jumps back on reaching the end and plays on forever. 430 sequences put the
loop on channel 0, 6 on channel 3, 1 on 12.

436 sequences loop; `BG_060_001` has only a start; 10 have none and play once
(`BG_100_000`, `BG_111_000`, `BG_115_000`, `BG_118_000`, `SR_003_001` and the
five event sequences). The end of track comes 505 to 18,482 ticks after the
loop's end (1,942 in 309), room for the last notes' release.

### Programs

A program change selects a program (`Prog` record) of the bank holding the
sequence (its `.hd`, see [the sound banks](sounds.md#5-hd-instrument-header));
the key picks the program's splits and the velocity its samples, as for
sound effects. There is no bank select: a port holds one bank. 4 ambient
sequences name programs their banks lack, parts that stay silent:
`SR_248_000` (program 1), `SR_272_001` (43, 44), `SR_273_000` (2).
`Bank::voices` resolves a sequence entry's notes to the samples they play:
over the 841 banks of `snd2/`, 2,009 of the 7,215 VAGs are played only by
sequences, 310 by nothing.

## 3. As MIDI

`music::export` writes a sequence as a standard MIDI file of format 0: one
track, the sequence's division, its events as they are (a note-off with
velocity 64), its name as the track name, and markers `loopStart` and
`loopEnd` at the loop's points. Program numbers are the bank's programs, not
General MIDI's: to hear the file as the game plays it, load the SoundFont
`soundfont.rs` writes for the bank (preset n of banks 0 and 128 is program n,
so channel 10 sounds the same in a General MIDI player): every split a zone
over its key range, every sample a zone over its velocity range, the VAG at
its own rate and loop, the base note as root key, transposes as coarse tune,
detunes in 1/128 semitone (assumed), volumes (128 unity) as attenuation, and
the SPU2 envelope (ADSR1, ADSR2) turned into SoundFont times by running it at
48 kHz. An approximation for listening: LFOs, velocity curves, key follow and
bend ranges (256, taken as Sony's 2 semitones) are left out.

`music::build` makes the `.sq` from a MIDI file, with the disc's sequence it
replaces as the template:

- **Unedited** (the events read back equal the template's), the disc's bytes.
  Every one of the 447 sequences exported and built back gives its own bytes.
- **Any MIDI file** of format 0 or 1 (or RIFF `.rmi`), any division: the
  tracks are merged by time (ties in track order) and the times scaled to
  480 ticks a quarter note, the only division the disc uses. Kept: notes (a
  note-on of velocity 0 becomes a note-off), controllers, program changes,
  pitch bends and tempo. Left out: poly and channel pressure, system
  exclusive, other meta events, bank select (0, 32), and RPNs and NRPNs with
  their data entries (98 to 101, 6, 38, 96, 97), which the driver would read
  as loop commands; only the disc's loop NRPNs are kept.
- **The loop**: the disc's NRPNs where the file has them; markers (or text
  events) `loopStart` and `loopEnd` (any case, any spaces or `_`), which win
  when they are not where the NRPNs are, so moving the markers in an editor
  moves the loop; else, where the disc's sequence looped, the whole tune from
  its start to its end of track. A loop is written as the disc writes it, on
  channel 0, and the end of track one beat after its end.
- **Tempo**: a file with no tempo at its start plays at 120 quarter notes a
  minute, as MIDI says, so a tempo event of 500,000 is put there.
- **Laid out as the disc's sequences** (section 2): channel 0's messages move
  to a channel the file does not use when channel 0 plays notes (refused when
  all 16 are used); when a note comes before tick 50, or the tune loops whole,
  the notes and everything after the start move 120 ticks on, the channels'
  set-up at the start (program changes, controllers, bends) goes to tick 10
  and a tempo there stays (so a tune looped whole loops from tick 120, after
  its set-up); every note channel is given
  what it lacks before its first note of volume 100 (MIDI's), expression 127,
  pan centre and pitch bend centre, at ticks 20 to 50.
- **Refused**: a program the bank lacks (unless the disc's sequence names it
  too), a note on a channel that has had no program change, one loop marker
  without the other, an end before the start, format 2, SMPTE time.

## 4. In a mod

- **By id**: `dc2tk mod asset dc2.iso mymod music:BG_000_000` writes
  `music/BG_000_000.mid` and `music/BG_000.sf2` into the mod and adds
  `"music:BG_000_000": "music/BG_000_000.mid"` to its `replace`. The
  replacement reaches every place the id has (the fishing packs' versions are
  `music:BG_193_000@fishing2`). In the asset tree each sequence is
  `music/<bank's folder>/<id>.mid` beside `<bank>.sf2`; `dc2tk mod make` takes an
  edited MIDI file from there.
- **By path**: `dc2tk mod asset dc2.iso mymod snd2/bgm/BG_000.snd --as mid`
  writes `disc/snd2/bgm/BG_000.snd.mid/` with a MIDI file per sequence (named
  after it) and the SoundFont; build replaces each sequence whose file is
  there and reads nothing else. A `.snd.mid` folder and a `.snd.wav` folder for
  one bank cannot both be given (use `sound:` ids for the effects).
- A sequence's samples and programs stay the bank's, so its sound memory does
  not change; the sequence itself goes to IOP memory, as the disc's biggest
  (44 KB) do. A tune plays with the SPU2's voices the game leaves the music
  (not measured): keep to the disc's few channels and short chords.
- **New instruments**: a `music:` line's `"instruments"` gives programs
  of the bank from WAV files, by the number the MIDI file's program changes
  name (0 to 127):

  ```json
  "music:BG_000_000": {"file": "music/tune.mid",
    "instruments": {"100": "music/organ.wav",
                    "101": {"file": "music/pulse.wav", "root": 69, "loop": true, "volume": 100, "like": 1},
                    "102": [{"file": "music/low.wav", "keys": [0, 59], "root": 48},
                            {"file": "music/high.wav", "keys": [60, 127], "root": 72}]}}
  ```

  `music::build_with_instruments` puts each in before the tune is built, so
  its programs are checked against them: a program of one split per sample
  over its `keys` (default all), each sample (encoded as the sound
  effects' WAVs are, at its own rate up to 48 kHz) sounding at its own pitch
  at `root` (default 60), looping where its WAV's `smpl` loop says or, with
  `"loop": true`, whole (a loop is made a whole number of 28-sample ADPCM
  blocks; a sample of whole periods and a multiple of 28 samples loops
  without a click), at `volume` (0 to 128, default 128, the sample's own
  level). The envelope, pan and priority are those of the bank's program
  `"like"` names (its first split and sample), else Sony's defaults the
  sound effects use: attack 0, full sustain held, release rate 12. A
  number the bank has already is replaced, and its samples cut to silent
  stubs when nothing plays them any more.
- **Memory.** A BG bank's samples load into the music region of SPU2
  memory, 0x78000 bytes from 0x5210, one bank at a time (bd + 16 bytes;
  [memory](memory.md#6-sound-memory-spu2)). `BG_056` takes 0x77E80 of it,
  but most banks leave far more: `BG_000` (Palm Brinks) 0x37170, 0x40B50
  free. `build` notes the samples' size before and after; when the new
  instruments do not fit, it cuts every sample nothing plays any more (the
  old tune's programs, once the new tune no longer names them) to a stub,
  and refuses the build if that is still not enough, saying by how much.
  The build's memory check then counts the bank against the region as for
  any changed bank. Instruments go in through `music:` lines only, not a
  `.snd.mid` folder.

### A music file of a mod's own

A `music:` line under `add` makes a new music file, so a mod's map or scene
can play a tune of its own without replacing one the game plays elsewhere:

```json
"add": {
  "music:BG_200": {"file": "music/fish.mid", "like": "BG_193"}
}
```

`"instruments"` may be given as on a `music:` replace line (above); left
out, the tune plays the like bank's own programs. A script plays it as it
plays the game's music:

```c
load_bgm(200);
play_bgm(0);
```

What the engine reads (engine): `load_bgm(n)` (`_LOAD_BGM`) goes to
`CScene::LoadBGM`, which makes the path `snd2/bgm/BG_%s.snd` from `n`
padded to three digits (`GetBgmFile`, `GetNumber3`), reads it with
`LoadFile2` (looked up by name in `DATA.HD4`) and loads it as a bank on the
music port (`LoadBGMPack`, `sndLoadSound`). Nothing else is kept per number:
no table of music files, no limit but `n >= 0` (a negative number loads
nothing; `CheckLoadBGM` also skips the number already loaded), and no other
file. `play_bgm(0, volume)` (`PlayBGM`) plays entry 0 of the loaded bank as
`play_se` does: the bank's `.cfg` row 0 names its `.sq` member, and with no
volume given, the `.vol`'s default for entry 0 (`sndGetSeDefVol`). The
bank's `.vol` also carries its reverb. So a music file is one `.snd`, whole.

What `build` writes (`music::new_bank`, `replace::add_music`):

- A new file `snd2/bgm/BG_200.snd` (a `DATA.HD4` record, as a new map's
  files are), a copy of the like bank (`BG_193`, any of the disc's
  `snd2/bgm/BG_NNN.snd`) with its members renamed after it (`BG_200.hd`,
  `.bd`, `.cfg`, `.vol`) and one sequence, `BG_200_000.sq`: the MIDI file
  built with the like bank's entry 0 sequence as the template, exactly as a
  `music:` replace line builds it (the layout, the loop, the tempo, the
  program check against the bank and the instruments given).
- The `.cfg` keeps its header (with the new number) and `END`, and row 0,
  naming `BG_200_000.sq`; the like bank's other sequences and their rows
  (`BG_193_001`, the fishing battle) are dropped, so `play_bgm(0)` is the
  only entry. The `.vol` is the like bank's: the default volume of entry 0
  (`BG_193`: 93, `BG_000`: 45) and the reverb. Pick a like bank whose
  default volume suits, or give one in the script (`play_bgm(0, 100)`).
- Every sample the new tune does not play is cut to a silent stub (as when a
  replace line's instruments need the room), so the bank takes no more
  music memory than its tune needs; `build` notes the sizes and checks the
  bank against the music region and the read buffer as for any changed
  bank. The like bank's programs stay, so the MIDI file plays the like
  bank's instruments (its SoundFont, `dc2tk mod asset ... music:BG_193_000`,
  is the one to hear it with) or ones the line gives.

Refused: a name that is not `BG_` and three digits (`load_bgm` takes 0 to
999 that way; the disc's are 0 to 195), a number the disc has (replace its
tune with a `music:BG_NNN_000` line instead), a like bank the disc lacks or
whose entry 0 plays no sequence, two lines (two mods) adding one number, and
a `file:` line or `disc/` file giving the same path. `dc2tk mod check --disc`
builds the bank to check it. Scripts write the number itself: it is in the
line's key already, and `load_bgm` takes a plain `int` (`bgm_file`), in
strict mode (`#pragma std c89`) too. `dc2tk mod asset built.iso NAME
music:BG_200_000` copies the tune back out of a built image.

Checked: `BG_193_001` (the fishing battle) added as `BG_200` like `BG_193`
builds (64 KB of samples cut, 0x67780 of 0x78000 bytes), and read back out
of the image its MIDI file is the one given but for the track's name. Not
yet heard in the game.

## 5. Verification

The corpus checks are `dc2tk-assets`' tests `corpus_sequences_round_trip`
(`sequence.rs`), `corpus_sequences_round_trip_through_midi` and
`corpus_music_banks_make_new_banks` (`music.rs`),
`corpus_banks_give_soundfonts` (`soundfont.rs`) and
`corpus_sequences_are_music_in_the_tree_and_build_back` (`tree_check.rs`),
run with `DC2TK_CORPUS` set to the extracted disc and `cargo test --release
-- --include-ignored corpus` (they are ignored otherwise).

| Check | Result |
|---|---|
| `.sq` read -> write byte-exact | 447 / 447 (527,181 events) |
| export to MIDI -> build back byte-exact, programs named are in the bank | 447 / 447; the 4 sequences above name programs their banks lack |
| a bank's SoundFont: chunks add up, every zone's sample and loop in range | 175 banks with sequences (146,708,074 bytes) |
| asset tree: music ids at every place, tree and index agree, MIDI builds back | `BG_193` alone and in a pack, `SR_138`'s 8 |
| a new music bank made like each disc bank, of its own tune: one entry, the same sequence, no more samples | 73 / 73 |

A unit test builds a tune that names a program the bank lacks with a
two-sample instrument (split at middle C, one looped), a program replaced
in place, and a bank that only fits once the old samples are cut, or not
at all. Another makes a new music bank of a two-tune bank (renamed
members, one entry, the old tune's sample cut), and one builds a mod's
added music file into a small disc image and reads it back, with the
refusals. Unit tests also cover the compression rule, a foreign MIDI file (format 1, 96
ticks a quarter note, bank select, an RPN, markers), refused programs and
channels, a tune looped whole, markers moved in an editor, and channel 0
freed or refused; every disc sequence taken as a new tune comes out laid out
as the disc's are.

## 6. Open problems

1. **The driver is not decompiled.** That the loop points are the data entry
   after NRPN 0 and the LSB after NRPN 1, that a sequence without a tempo
   plays at 120, how notes held over the loop's end behave, and which part of
   the layout above a sequence needs to sound are read from the data, not from
   Sony's IOP modules.
2. **Voices.** How many SPU2 voices the music may use while effects play was
   not established; a dense tune may lose notes.
3. **New instruments** come from WAVs (section 4), one sample a key range;
   velocity layers, an envelope other than a bank program's, and importing
   a SoundFont's instruments are not done. Whether the IOP's synthesizer
   reads the new programs as the disc's is from the format, not yet heard in
   the game (`text-music-test.iso`).
4. **The SoundFont is approximate** (above); nothing was compared with the
   game's output.
5. `sounds::free_unused` cuts a sample only when no entry plays it, a
   sequence's notes included (`Bank::voices` follows them); a `.sq` no
   `.cfg` row names is not seen (the disc's banks have 447 `.sq` rows for
   their 447 members).
