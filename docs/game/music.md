# Music and ambient loops (`.sq`)

How the PAL disc stores its background music and ambient loops, and how the game plays them. The
sound banks that hold the sequences and their instruments are in [the sound banks](sounds.md);
replacing a tune with a MIDI file, or adding one, is in [music in a mod](../modding/music.md), and
the code that reads and writes the format in [the music code](../../src/docs/assets/music.md).

Evidence, as there: **engine** (the decompiled EE code in `finaldc2/ChronicleTwo/ps2`:
`src/sound.cpp`, `src/scenesnd.cpp`, `src/ezbgm.cpp`, `src/event_func.cpp` and the asm of
`PlayBGM`), **data** (statistics over the 447 `.sq` files of the PAL disc) and **Sony** (the IOP
plays sequences with Sony's MIDI sequencer modules, which were not decompiled; what their events
mean is the MIDI standard's and, for the loop controllers, inferred from where the disc puts them).

## What is music on the disc

All background music is sequenced: each tune is a `.sq` member of a sound bank (`.snd`), played by
the IOP with that bank's instruments. Nothing musical is streamed.

| Where | Banks | Sequences | Use |
|---|---|---|---|
| `snd2/bgm/BG_NNN.snd` | 73 | 76 (70 banks one, 3 two) | background music, one tune a bank |
| `snd2/env/SR_*.snd` | 96 | 352 (1 to 8 a bank; 66 banks four) | ambient loops (wind, water, crowds) |
| `snd2/event/EV_*.snd` | 6 | 7 | event sounds played as sequences |
| `sg/fish/fishing2*.pak` | 6 copies of `BG_193.snd` | 12 | the fishing music, a second version of both tunes |

447 places, 435 names, 437 distinct sequences (278 by their bytes alone: many ambient loops are
copied under other names; the fishing packs' copies of `BG_193_000` and `BG_193_001` differ from
`snd2/bgm`'s).

**Background music** (engine): `CScene::LoadBGM` reads `snd2/bgm/BG_%03d.snd` (`GetBgmFile`) and
loads it on game port 0 or 11 (two slots, for changing tunes), whose sequences go to IOP memory
(`LoadSeq`, a block of the file's size + 256). `PlayBGM(entry, volume, fade)` plays an entry of
that bank as `play_se` does (`sndSePlayV`): the bank's `.cfg` row names the `.sq`
(`BG_000_000.sq`), so the entry plays the sequence with `SQ_Play`, at the row's default volume
from the `.vol` (`BG_000`: 45) unless the caller gives one. A map's sound set (the map list's
column and `snd2/MAP_index.txt`) names its default tune (`GetDefBgmNo`); scripts start others
(`start_bgm`). Palm Brinks' streets m01 to m05 use set 1000, whose tune is `BG_000`.

**Ambient loops**: the sound set's ambient bank (`SR_*`, port 2) and the sequence the set names
(`SR_010_000`), or one the time of day picks; event banks (port 4) play theirs as entries.

**Streamed audio** (engine, data): `SOUND.DAT` (969,644,032 bytes) holds 3,102 RIFF WAVE files,
16-bit PCM mono at 48 kHz with `bext`, `minf` and `elmo` chunks, indexed by `SOUND.HD3` (16-byte
records: name offset, size, sector, sector count; names `0010010.wav`, numbered by voice). The
IOP's EZBGM server streams them (`CSound::StreamOpenFast`, `ezBgm`): event dialogue
(`CommandStreamOpen`), the chapter titles' narration (`menucapt.cpp`), the robot's voice
(`CRoboVoiceSystem`) and the minigames' voices. They are voices, not music; the movies
(`MOVIE/*.PSS`) carry their own sound.

## How a script loads a music file

What the engine reads (engine): `load_bgm(n)` (`_LOAD_BGM`) goes to `CScene::LoadBGM`, which makes
the path `snd2/bgm/BG_%s.snd` from `n` padded to three digits (`GetBgmFile`, `GetNumber3`), reads
it with `LoadFile2` (looked up by name in `DATA.HD4`) and loads it as a bank on the music port
(`LoadBGMPack`, `sndLoadSound`). Nothing else is kept per number: no table of music files, no
limit but `n >= 0` (a negative number loads nothing; `CheckLoadBGM` also skips the number already
loaded), and no other file. `play_bgm(0, volume)` (`PlayBGM`) plays entry 0 of the loaded bank as
`play_se` does: the bank's `.cfg` row 0 names its `.sq` member, and with no volume given, the
`.vol`'s default for entry 0 (`sndGetSeDefVol`). The bank's `.vol` also carries its reverb. So a
music file is one `.snd`, whole.

## The `.sq` file

Little-endian, Sony's chunk layout (as the `.hd`'s): creator `IECS`, the chunk type reversed, the
chunk's size. Every file on the disc has exactly this header.

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

MIDI events, each a delta time (a variable-length quantity, always in as few bytes as it needs)
and a message, compressed three ways:

- **Running status**: a channel message whose status byte equals the last one written leaves it
  out; a meta event ends running status (the next channel message writes its status). It is used
  wherever it can be (182,192 times).
- **Note-offs carry only the key**: `8n kk`, no velocity. Notes end with note-offs, never note-ons
  of velocity 0.
- **Zero delta times left out**: a channel message that wrote its status byte sets bit 7 of its
  last data byte when the next event's delta is 0, and that delta is not written. A message in
  running status never sets it (its first byte would then read as a status byte, for a one-byte
  message), so after one a delta of 0 is written out; a meta event never sets it either. Over the
  disc 174,966 zero deltas are left out and 29,088 are written after a message in running status.
  This rule leaves nothing to chance: a sequence is its events, and writing them gives the disc's
  bytes for all 447 files.

Meta events are `FF type length data` (length a variable-length quantity): tempo (`FF 51 03`,
microseconds per quarter note; 473, at tick 0 in 443 sequences, none in 4 ambient ones) and end of
track (`FF 2F 00`). No system exclusive, no other meta event.

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

No poly or channel pressure, no bank select, no other controller. Sizes run from 128 bytes to
44,128 (`BG_118_000`).

**How a sequence is laid out** (data, the 435 sequences of `snd2/`, 2,024 channels that play
notes):

- channel 0 never plays a note: it carries the loop's NRPNs (or nothing), and no sequence plays
  notes on its loop's channel;
- every note channel is set up before its first note, spread over ticks 10 to 50: program change,
  expression (controller 11; 127 in 1,974), pan (10) in all 2,024; volume (7) in 2,005; pitch bend
  centre in 2,019. Only 58 put any of it at the first note's tick;
- the first note comes at tick 50 or later (120 in 298 sequences, 60 in 72, never 0); the loop
  starts after the set-up in 328 of 422, often just after the first note (10 ticks after it in
  283).

Which of these the IOP's sequencer needs is not known (`MODMIDI.IRX`, `MODHSYN.IRX` and the game's
`EZMIDI.IRX` server are not decompiled; their strings show loop tables, "not found loop start",
running status and tempo handling, nothing on channel defaults). A tune with notes on channel 0, no
expression and its first notes and set-up at tick 0 with the loop's start played silent in Palm
Brinks, so a new tune is laid out as the disc's are ([music in a mod](../modding/music.md) says
how).

### Loops

A loop is two NRPNs (data, positions): NRPN 0 (`B0 63 00`, then `B0 06 00` ten ticks later) marks
the start, NRPN 1 (`B0 63 01`, `B0 06 00` and `B0 26 00`, ten ticks apart) the end. In the music
the span from the start's data entry to the end's LSB is whole bars (`BG_193_000`: 53,760 ticks,
28 bars of 4/4), so those two events are taken as the loop points; the sequencer jumps back on
reaching the end and plays on forever. 430 sequences put the loop on channel 0, 6 on channel 3, 1
on 12.

436 sequences loop; `BG_060_001` has only a start; 10 have none and play once (`BG_100_000`,
`BG_111_000`, `BG_115_000`, `BG_118_000`, `SR_003_001` and the five event sequences). The end of
track comes 505 to 18,482 ticks after the loop's end (1,942 in 309), room for the last notes'
release.

### Programs

A program change selects a program (`Prog` record) of the bank holding the sequence (its `.hd`,
see [the sound banks](sounds.md)); the key picks the program's splits and the velocity its
samples, as for sound effects. There is no bank select: a port holds one bank. 4 ambient sequences
name programs their banks lack, parts that stay silent: `SR_248_000` (program 1), `SR_272_001`
(43, 44), `SR_273_000` (2). Over the 841 banks of `snd2/`, 2,009 of the 7,215 VAGs are played only
by sequences, 310 by nothing.

## What is not known

1. **The driver is not decompiled.** That the loop points are the data entry after NRPN 0 and the
   LSB after NRPN 1, that a sequence without a tempo plays at 120, how notes held over the loop's
   end behave, and which part of the layout above a sequence needs to sound are read from the
   data, not from Sony's IOP modules.
2. **Voices.** How many SPU2 voices the music may use while effects play was not established; a
   dense tune may lose notes.
