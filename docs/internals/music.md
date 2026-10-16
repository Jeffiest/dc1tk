# Music and ambient loops (`.sq`)

This page documents how the PAL disc stores background music and ambient sound loops inside `.sq` sequence files, and how the engine loads and plays them.
- For the `.snd` sound bank container and instrument definitions, see [the sound banks](sounds.md).
- For replacing or adding music using standard MIDI files, see [music in a mod](../guide/music.md).
- For the `dc2tk` sequence converter implementation, see [the music code](../../src/docs/assets/music.md).

**Sources**:
- **Engine**: Decompiled EE code in `finaldc2/ChronicleTwo/ps2` (`src/sound.cpp`, `src/scenesnd.cpp`, `src/ezbgm.cpp`, `src/event_func.cpp`, and assembly of `PlayBGM`).
- **Data**: Corpus counts across all 447 `.sq` files on the PAL disc (`435` distinct names, `437` bank-unique sequences, `278` byte-distinct files).
- **Sony SDK and MIDI analysis**: The IOP plays `.sq` files via Sony's `MODMIDI.IRX`, `MODHSYN.IRX`, and `EZMIDI.IRX` modules (not decompiled).

## What is music on the disc

All background music is sequenced MIDI audio synthesized in real time on the IOP from a `.snd` bank's `.hd` instruments and `.bd` samples. No background music is streamed.

| Path / Archive | Banks | Sequences | Game Port | Loader & Playback Summary |
|---|---|---|---|---|
| `snd2/bgm/BG_NNN.snd` | `73` | `76` (`70` have 1; `3` have 2) | `0` or `11` (alternating) | `CScene::LoadBGM` (`GetBgmFile`). `LoadSeq` copies each `.sq` to an IOP heap block of `file_size + 256` B. `PlayBGM(entry, vol, fade)` triggers `.cfg` row `entry` (`SQ_Play`, default vol from `.vol`, e.g. `45` in `BG_000`). Map sound sets (`snd2/MAP_index.txt`, `GetDefBgmNo`) pick default BGM (e.g. set `1000` -> `BG_000` in `m01`..`m05`); scripts call `start_bgm`. |
| `snd2/env/SR_*.snd` | `96` | `352` (`1..8`/bank; `66` have 4) | `2` | Ambient environmental loops loaded from the map's sound set; played by name (e.g. `SR_010_000`) or by time of day. |
| `snd2/event/EV_*.snd` | `6` | `7` | `4` | Cutscene sequences triggered as standard sound-effect entries. |
| `sg/fish/fishing2*.pak` | `6` (`BG_193`) | `12` | `0` / `11` | Fishing minigame music (contains alternate versions of `BG_193_000.sq` and `BG_193_001.sq`). |
| `SOUND.DAT` / `SOUND.HD3` | `1` (`969,644,032` B) | `3,102` `.wav` files | IOP `EZBGM` | Spoken dialogue only (never music): mono 16-bit 48 kHz PCM RIFF WAVE (`bext`, `minf`, `elmo` chunks), indexed by 16-byte records (`name_offset`, `byte_size`, `start_sector`, `sector_count`) in `SOUND.HD3`. Streamed via `CSound::StreamOpenFast` (`ezBgm`, `CommandStreamOpen`, `src/menucapt.cpp`, `CRoboVoiceSystem`). Movies (`MOVIE/*.PSS`) use interleaved audio. |

## How a script loads a music file

When an event script calls `load_bgm(n)` (`_LOAD_BGM` -> `CScene::LoadBGM(n)`):
1. `GetBgmFile` and `GetNumber3` format `n` as 3 digits to build `snd2/bgm/BG_%03d.snd` (`n < 0` loads nothing; `CheckLoadBGM` skips if `n` is already loaded).
2. `LoadFile2` looks up the path case-insensitively in `DATA.HD4` and reads the archive from `DATA.DAT` (any integer `n >= 0` present in `DATA.HD4` is valid; there is no upper table limit).
3. `LoadBGMPack` and `sndLoadSound` load the archive into the active music port.
4. `play_bgm(0, volume)` (`PlayBGM`) triggers entry `0` of `.cfg` (which names the `.sq` file) using entry `0` of `.vol` (`sndGetSeDefVol`) for default volume and reverb.

## The `.sq` file

Every `.sq` file is a little-endian Sony sequence container with a fixed 74-byte (`0x4A`) header before the MIDI event stream:

| Offset | Size | Type | Field | Value |
|---|---|---|---|---|
| `0x00` | `16` | `u8[16]` | `Vers` chunk | `"IECS"`, `"sreV"`, size `0x10`, reserved `u16 = 0`, version `1.0` (fixed in all 447 files). |
| `0x10` | `12` | `u8[12]` | `Sequ` chunk header | `"IECS"`, `"uqeS"`, size `0x20` (fixed in all 447 files). |
| `0x1C` | `4` | `u32` | `file_size` | Exact `.sq` file byte size (including trailing `0xFF` padding; `128`..`44,128` B on disc). |
| `0x20` | `16` | `u32[4]` | Sub-chunk offsets | `Song`, `Midi`, `SeSeq`, `SeSong` = `0xFFFFFFFF, 0x00000030, 0xFFFFFFFF, 0xFFFFFFFF`. |
| `0x30` | `12` | `u8[12]` | `Midi` chunk header | `"IECS"`, `"idiM"`, size = `file_size - 0x30`. |
| `0x3C` | `4` | `u32` | `max_block` | Always `0` (1 block). |
| `0x40` | `4` | `u32` | `block0_ofs` | Offset of block 0 from `Midi` chunk start (`0x14`). |
| `0x44` | `4` | `u32` | `stream_ofs` | Offset of event stream from block 0 start (`6`). |
| `0x48` | `2` | `u16` | `division` | Ticks per quarter note (`480` in all 447 files). |
| `0x4A` | `var` | `u8[]` | Event stream | Compressed single-track MIDI stream ending with `FF 2F 00`, padded with `0xFF` to 16 bytes. |

## Events

Each event consists of a minimum-length MIDI variable-length quantity (VLQ) delta time followed by a MIDI message. Re-encoding with the three deterministic compression rules below reproduces all 447 `.sq` files byte-for-byte:

| Compression Rule | Encoding Behaviour | Corpus Count |
|---|---|---|
| 1. Running status | Omits the status byte when identical to the previous channel message. Any meta event (`0xFF`) cancels running status. | `182,192` |
| 2. 1-byte Note-Off | Note-Off (`8n kk`) stores only the key byte `kk` and omits release velocity (never uses `9n kk 00`). | `132,552` |
| 3. Elided zero delta (`0x80` bit) | When a channel message writes an explicit status byte (not running status) and the **next** event has `delta == 0`, sets bit 7 (`0x80`) on the current message's **last data byte** and omits the next event's `0x00` delta byte. Never set on running-status messages or `0xFF` meta events. | `174,966` elided; `29,088` explicit `0x00` after running status |

**Event distribution across all 447 `.sq` files (`527,181` total events)**:

| Event Type | MIDI Bytes | Corpus Count |
|---|---|---|
| Note-On | `9n kk vv` | `132,552` |
| Note-Off | `8n kk` (1 data byte) | `132,552` |
| Control Change `10` (Pan) | `Bn 0A vv` | `136,826` |
| Control Change `11` (Expression) | `Bn 0B vv` | `110,360` |
| Control Change `7` (Channel Volume) | `Bn 07 vv` | `2,073` |
| NRPN Loop Markers (`CC 99`, `CC 6`, `CC 38`) | `Bn 63`, `Bn 06`, `Bn 26` | `873`, `873`, `436` |
| Program Change | `Cn pp` | `2,087` |
| Pitch Bend | `En ll mm` | `7,629` |
| Meta: Tempo | `FF 51 03 tt tt tt` | `473` (at tick `0` in 443 files; omitted in 4 ambient files) |
| Meta: End of Track | `FF 2F 00` | `447` |

**Channel and timeline conventions** (across 435 sequences and 2,024 note-playing channels in `snd2/`):

| Convention | Rule | Corpus Evidence |
|---|---|---|
| Loop control channel | Channel `0` (or the loop-marker channel) never plays notes. | Loop markers sit on ch `0` (430 files), ch `3` (6), or ch `12` (1); 0 notes on those channels. |
| Channel setup (`ticks 10..50`) | Every note-playing channel initializes controllers before its first note. | `Cn` (Program), `CC 11` (Expression, `127` in 1,974 ch), and `CC 10` (Pan) in all `2,024` ch; `CC 7` (Volume) in `2,005` ch; centre Pitch Bend (`En 00 40`) in `2,019` ch. |
| Lead-in delay (`tick >= 50`) | First note starts at tick `>= 50` (never tick `0`). Loop start usually follows setup. | First note at tick `120` (298 files) or `60` (72 files); loop start 10 ticks after first note in 283 files. |

> [!IMPORTANT]
> A custom `.sq` that placed notes on channel 0, omitted `CC 11` (Expression), and placed setup, loop start, and first notes at tick `0` played back completely silent in-game. Always follow the channel setup and lead-in conventions above (see [music in a mod](../guide/music.md)).

### Loops

The IOP sequencer marks loops with NRPN controller sequences (where the Data Entry events define the exact bar-aligned loop boundaries):

| Marker | MIDI Controller Sequence | Corpus Count (`447` files) |
|---|---|---|
| Loop start | `Bn 63 00` (`CC 99 = 0`), then `+10` ticks `Bn 06 00` (`CC 6 = 0`) | `437` (`BG_060_001.sq` has only loop start) |
| Loop end | `Bn 63 01` (`CC 99 = 1`), `+10` ticks `Bn 06 00` (`CC 6 = 0`), `+10` ticks `Bn 26 00` (`CC 38 = 0`) | `436` (End of Track follows `505..18,482` ticks later; `1,942` in 309 files) |
| One-shot | No NRPN markers | `10` (`BG_100_000`, `BG_111_000`, `BG_115_000`, `BG_118_000`, `SR_003_001`, 5 `EV_*`) |

### Programs

- Program Change (`Cn pp`) selects `Prog[pp]` in the parent `.snd` bank's `.hd` header (see [the sound banks](sounds.md)). `.sq` files never send Bank Select (`CC 0`).
- Four ambient sequences select absent (`0xFFFFFFFF`) programs, leaving those channels silent: `SR_248_000.sq` (prog `1`), `SR_272_001.sq` (progs `43`, `44`), and `SR_273_000.sq` (prog `2`).

## What is not known

1. **IOP sequencer internals**: `MODMIDI.IRX`, `MODHSYN.IRX`, and `EZMIDI.IRX` are not decompiled, so exact NRPN loop jump ticks, the default tempo of the 4 tempo-less ambient files (assumed 120 BPM), tied notes across loops, and minimum required setup controllers are inferred from the corpus and in-game testing.
2. **SPU2 voice polyphony limits**: Maximum simultaneous voices usable by a `.sq` track alongside field and combat sound effects has not been measured.
