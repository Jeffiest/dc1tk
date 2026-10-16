# Music and ambient loops (`.sq`)

This page documents how the PAL disc stores background music and ambient sound loops inside `.sq` sequence files, and how the engine loads and plays them. For the `.snd` sound bank container and instrument definitions, see [the sound banks](sounds.md). For a practical guide to replacing or adding music using standard MIDI files, see [music in a mod](../guide/music.md). For details on the `dc2tk` sequence converter implementation, see [the music code](../../src/docs/assets/music.md).

The findings on this page come from three sources:
- **Engine**: Decompiled Emotion Engine (EE) code in `finaldc2/ChronicleTwo/ps2` (`src/sound.cpp`, `src/scenesnd.cpp`, `src/ezbgm.cpp`, `src/event_func.cpp`, and the assembly of `PlayBGM`).
- **Data**: Corpus statistics across all 447 `.sq` files extracted from the PAL disc.
- **Sony SDK and MIDI analysis**: The IOP plays `.sq` files using Sony's MIDI sequencer modules (`MODMIDI.IRX`, `MODHSYN.IRX`, and `EZMIDI.IRX`), which have not been decompiled. Standard MIDI status codes follow the MIDI 1.0 specification, while the NRPN loop markers and channel initialisation rules are derived from the 447 sequences on the disc.

## What is music on the disc

All background music in Dark Chronicle is sequenced MIDI audio. Each music track is a `.sq` member inside a `.snd` sound bank and is synthesized in real time on the IOP using that bank's `.hd` instruments and `.bd` samples. No background music is streamed.

| Location | Banks | Sequences | Purpose |
|---|---|---|---|
| `snd2/bgm/BG_NNN.snd` | 73 | 76 (70 banks have 1; 3 banks have 2) | Background music tracks (usually one tune per bank). |
| `snd2/env/SR_*.snd` | 96 | 352 (1 to 8 per bank; 66 banks have 4) | Ambient environmental loops (wind, water, crowds). |
| `snd2/event/EV_*.snd` | 6 | 7 | Event sound sequences triggered during cutscenes. |
| `sg/fish/fishing2*.pak` | 6 copies of `BG_193.snd` | 12 | Fishing minigame music (contains alternate versions of both `BG_193` tunes). |

Across all 447 `.sq` members on the disc, there are 435 distinct filenames and 437 bank-unique sequences, representing 278 byte-distinct files (many ambient loops are reused across multiple `SR_*` banks under different filenames, while the fishing packs' copies of `BG_193_000.sq` and `BG_193_001.sq` differ from the copies in `snd2/bgm/BG_193.snd`).

- **Background music (`BG_*`)**: `CScene::LoadBGM` constructs the path `snd2/bgm/BG_%03d.snd` (`GetBgmFile`) and loads the bank into game port `0` or `11` (two alternating BGM slots used when switching tracks). During loading, `LoadSeq` copies each `.sq` member into an IOP heap block of `file_size + 256` bytes. Calling `PlayBGM(entry, volume, fade)` triggers `entry` in that bank just like `play_se` (`sndSePlayV`): the bank's `.cfg` row names the `.sq` member (for example, `BG_000_000.sq`), which calls `SQ_Play` using the row's default volume from `.vol` (`45` in `BG_000`) unless the caller overrides `volume`. Each map's sound set (defined by the map table and `snd2/MAP_index.txt`) specifies its default BGM number (`GetDefBgmNo`), while event scripts can start any track via `start_bgm`. For example, Palm Brinks streets `m01` through `m05` use sound set `1000`, which selects `BG_000`.
- **Ambient loops (`SR_*`)**: Loaded into game port `2` from the map's sound set. The engine plays either the specific sequence named by the sound set (such as `SR_010_000`) or a sequence chosen by the current time of day. Event banks (game port `4`) trigger their `.sq` sequences as standard sound-effect entries.
- **Streamed voice audio (`SOUND.DAT`)**: `SOUND.DAT` (969,644,032 bytes) stores 3,102 mono 16-bit PCM RIFF WAVE files sampled at 48 kHz (containing `bext`, `minf`, and `elmo` chunks). `SOUND.HD3` indexes these files using 16-byte records (`name_offset`, `byte_size`, `start_sector`, `sector_count`, with filenames such as `0010010.wav`). The IOP `EZBGM` driver streams them (`CSound::StreamOpenFast`, `ezBgm`) for cutscene voice acting (`CommandStreamOpen`), chapter title narration (`src/menucapt.cpp`), Ridepod voice lines (`CRoboVoiceSystem`), and minigame speech. `SOUND.DAT` contains only spoken dialogue, never music. Pre-rendered video files (`MOVIE/*.PSS`) carry their own interleaved audio streams.

## How a script loads a music file

When an event script calls `load_bgm(n)` (`_LOAD_BGM`), the engine invokes `CScene::LoadBGM(n)`:
1. `GetBgmFile` and `GetNumber3` format `n` as a zero-padded 3-digit string to build `snd2/bgm/BG_%s.snd`.
2. `LoadFile2` looks up that path case-insensitively in `DATA.HD4` and reads the `.snd` archive from `DATA.DAT`.
3. `LoadBGMPack` and `sndLoadSound` load the archive into the active music port.

The engine maintains no separate lookup table or upper limit for BGM numbers: any integer `n >= 0` whose `snd2/bgm/BG_NNN.snd` entry exists in `DATA.HD4` can be loaded (a negative `n` loads nothing, and `CheckLoadBGM` skips reloading if track `n` is already active). Calling `play_bgm(0, volume)` (`PlayBGM`) triggers entry `0` of the loaded bank: row `0` of `.cfg` names the `.sq` sequence, and entry `0` of `.vol` supplies the default volume (`sndGetSeDefVol`) and port reverb settings. Thus, every background music track is a self-contained `.snd` archive.

## The `.sq` file

A `.sq` file is a little-endian Sony sequence container that uses the same 12-byte chunk framing as `.hd` files (`"IECS"` creator tag, byte-reversed chunk identifier, and 32-bit chunk size). Every `.sq` file on the disc has the exact same 74-byte (`0x4A`) header before the MIDI event stream:

| Offset | Size | Field | Value |
|---|---|---|---|
| `0x00` | 16 | `Vers` chunk (`"IECS"`, `"sreV"`, size `0x10`, reserved `u16 = 0`, version `1.0`) | Fixed in all 447 files. |
| `0x10` | 12 | `Sequ` chunk header (`"IECS"`, `"uqeS"`, size `0x20`) | Fixed in all 447 files. |
| `0x1C` | 4 | Total `.sq` file byte size | Exact file size (including trailing `0xFF` padding). |
| `0x20` | 16 | Chunk offsets for `Song`, `Midi`, `SeSeq`, and `SeSong` (`u32[4]`) | Always `0xFFFFFFFF, 0x00000030, 0xFFFFFFFF, 0xFFFFFFFF`. |
| `0x30` | 12 | `Midi` chunk header (`"IECS"`, `"idiM"`, size = `file_size - 0x30`) | |
| `0x3C` | 4 | Maximum data block index | Always `0` (1 block). |
| `0x40` | 4 | Offset of block 0 from start of `Midi` chunk | Always `0x14`. |
| `0x44` | 4 | Offset of event stream from start of block 0 | Always `6`. |
| `0x48` | 2 | Time division (ticks per quarter note, `u16`) | Always `480` in all 447 files. |
| `0x4A` | var | Compressed single-track MIDI event stream, ending with End of Track (`FF 2F 00`) | |
| `end` | `0..15` | Trailing `0xFF` padding to a 16-byte boundary | |

### Events

Each event in the stream consists of a standard MIDI variable-length quantity (VLQ) delta time—always encoded using the minimum possible number of bytes—followed by a MIDI message. Sony's `.sq` format applies three byte-level compressions to standard MIDI events:

1. **Running status**: If a channel message has the exact same status byte as the previous channel message, the status byte is omitted. Any meta event (`0xFF`) cancels running status, forcing the next channel message to write its status byte explicitly. Running status is used wherever possible (182,192 times across the corpus).
2. **1-byte Note-Off payload**: Note-Off messages (`8n kk`) store only the key byte `kk` and omit the release velocity byte. Notes always end with an explicit `8n kk` Note-Off event, never with a velocity-0 Note-On (`9n kk 00`).
3. **Elided zero delta times (high-bit flag)**: When a channel message explicitly writes its status byte (i.e., is **not** using running status) and the **next** event in the track has a delta time of `0`, the encoder sets bit 7 (`0x80`) on the current message's **final data byte** and omits the next event's `0x00` delta-time byte.
   - A message encoded in running status **never** sets bit 7 on its last data byte (because doing so would cause the next event's running-status data byte to have bit 7 set and be misread as a new status byte). Consequently, a zero delta time following a running-status message is always written explicitly as `0x00`.
   - Meta events (`0xFF`) also never set this flag.
   Across the disc, this compression omits 174,966 zero deltas and writes 29,088 explicit `0x00` deltas after running-status messages. These three rules are completely deterministic: re-encoding the parsed events reproduces all 447 `.sq` files on the disc byte-for-byte.

Meta events follow standard MIDI framing (`FF type length data`, where `length` is a VLQ). Only two meta event types appear on the disc:
- **Tempo (`FF 51 03 tt tt tt`)**: Microseconds per quarter note (473 occurrences; placed at tick `0` in 443 sequences, and omitted in 4 ambient sequences).
- **End of Track (`FF 2F 00`)**: Present once at the end of all 447 sequences.

No SysEx messages or other meta events appear. Across all 447 files (ranging in size from 128 bytes to 44,128 bytes for `BG_118_000.sq`), the 527,181 total events break down as follows:

| Event Type | Corpus Count |
|---|---|
| Note-On (`9n kk vv`) | 132,552 |
| Note-Off (`8n kk`) | 132,552 |
| Control Change `10` (Pan) | 136,826 |
| Control Change `11` (Expression) | 110,360 |
| Control Change `7` (Channel Volume) | 2,073 |
| Control Change `99` (NRPN MSB), `6` (Data Entry MSB), `38` (Data Entry LSB) | `873`, `873`, `436` (loop markers) |
| Program Change (`Cn pp`) | 2,087 |
| Pitch Bend (`En ll mm`) | 7,629 |
| Tempo (`FF 51 03`), End of Track (`FF 2F 00`) | `473`, `447` |

No polyphonic aftertouch, channel pressure, bank select, or other MIDI controllers appear on the disc.

**Channel and timeline conventions**: Across the 435 sequences in `snd2/` (which contain 2,024 note-playing channels), every sequence follows strict structural conventions:
- **Channel 0 is reserved for loop control**: Channel 0 never plays notes. Instead, it carries the NRPN loop markers (or remains empty). Furthermore, no sequence ever plays notes on whichever channel carries its loop markers.
- **Explicit channel setup before the first note**: Every note-playing channel initializes its controllers before its first note—usually spread across ticks `10..50`:
  - Program Change (`Cn`), Expression (`CC 11`, set to `127` in 1,974 channels), and Pan (`CC 10`) appear on all 2,024 note-playing channels.
  - Channel Volume (`CC 7`) appears on 2,005 of 2,024 channels.
  - Centre Pitch Bend (`En 00 40`) appears on 2,019 of 2,024 channels.
  Only 58 channels place any of these setup events at the exact tick of their first note.
- **Lead-in delay before the first note**: The earliest note in a sequence always starts at tick `50` or later (`120` in 298 sequences, `60` in 72 sequences, and never at tick `0`). In 328 of 422 looped sequences, the loop start marker appears after the channel setup block—most commonly 10 ticks after the first note (283 sequences).

> **Important**
> In testing, a custom `.sq` file that placed notes on channel 0, omitted `CC 11` (Expression), and placed channel setup, the loop start marker, and the first notes all at tick `0` played back completely silent in Palm Brinks. When converting custom MIDI files to `.sq`, always follow the disc's channel setup and lead-in timing conventions (see [music in a mod](../guide/music.md)).

### Loops

The IOP sequencer marks loop boundaries using two Non-Registered Parameter Number (NRPN) controller sequences:
- **Loop start**: `Bn 63 00` (NRPN MSB = `0`), followed 10 ticks later by `Bn 06 00` (Data Entry MSB = `0`).
- **Loop end**: `Bn 63 01` (NRPN MSB = `1`), followed 10 ticks later by `Bn 06 00` (Data Entry MSB = `0`), and 10 ticks after that by `Bn 26 00` (Data Entry LSB = `0`).

In musical tracks, the tick span from the loop-start Data Entry (`Bn 06 00`) to the loop-end Data Entry LSB (`Bn 26 00`) equals an exact number of musical bars (for example, `BG_193_000.sq` spans `53,760` ticks = 28 bars of 4/4 at 480 ticks per beat). Those two Data Entry events therefore define the exact loop start and loop end ticks; when playback reaches the loop end, the sequencer jumps back to the loop start and repeats indefinitely. Across the corpus, 430 sequences place their loop markers on channel `0`, 6 use channel `3`, and 1 uses channel `12`.

Out of 447 sequences on the disc:
- 436 sequences contain both loop start and loop end markers.
- 1 sequence (`BG_060_001.sq`) contains only a loop start marker.
- 10 sequences contain no loop markers and play once as one-shots (`BG_100_000`, `BG_111_000`, `BG_115_000`, `BG_118_000`, `SR_003_001`, and 5 event sequences).

In looped sequences, the End of Track (`FF 2F 00`) event is placed `505` to `18,482` ticks after the loop end marker (`1,942` ticks in 309 sequences) to leave tail room for releasing notes.

### Programs

A MIDI Program Change (`Cn pp`) selects program `pp` (`Prog[pp]`) from the `.hd` instrument header of the parent `.snd` bank (see [the sound banks](sounds.md)). Each Note-On event's key and velocity then select the matching key splits (`Sset`) and velocity-layered samples (`Smpl`), exactly as in sound effects. Because each driver port holds a single `.snd` bank for `.sq` playback, `.sq` files never send Bank Select (`CC 0`) messages.

Four ambient sequences on the disc select program numbers that are absent (`0xFFFFFFFF`) in their bank's `.hd`, leaving those channels silent:
- `SR_248_000.sq` (program `1`)
- `SR_272_001.sq` (programs `43` and `44`)
- `SR_273_000.sq` (program `2`)

Across the 841 banks in `snd2/`, 2,009 of the 7,215 VAG samples are triggered exclusively by `.sq` or `.mid` sequences, while 310 VAG samples are unreferenced by any sound effect or sequence.

## What is not known

1. **IOP sequencer internals**: Because `MODMIDI.IRX`, `MODHSYN.IRX`, and `EZMIDI.IRX` have not been decompiled, details such as the exact tick of the NRPN loop jump, the default tempo of the 4 ambient sequences that omit a Tempo event (assumed to be 120 BPM), the handling of notes tied across the loop boundary, and which channel setup controllers are strictly required by the IOP driver are inferred from corpus analysis and in-game testing rather than IOP disassembly.
2. **SPU2 voice polyphony limits**: The maximum number of SPU2 hardware voices that a `.sq` track can use simultaneously alongside active field and combat sound effects has not been measured; overly dense MIDI arrangements may cause voice stealing.
