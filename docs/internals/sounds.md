# Dark Chronicle sound banks (`.snd`)

This page documents how the PAL disc stores sound effects inside `.snd` archives and how `play_se(bank, n)` turns entry `n` into SPU2 hardware voices. For a practical guide to replacing and adding sound effects in a mod, see [editing sounds](../guide/sounds.md). For details on the `dc2tk` sound converter implementation, see [src/docs/assets/sounds.md](../../src/docs/assets/sounds.md). For the background music and ambient loop sequences stored inside sound banks, see [music](music.md).

The findings on this page come from three sources:
- **Engine**: Decompiled Emotion Engine (EE) code in `finaldc2/ChronicleTwo/ps2` (`src/snd_mngr.cpp`, `src/sound.cpp`, `src/snd_seseq.cpp`, `src/scenesnd.cpp`, the assembly of `LoadSeInfoTxt`, `LoadVolInfoTxt`, `GetLine`, `SearchSeq`, `sndLoadSound`, `sndSePlaySeID`, and `sndCSeSeq::NoteOn`, and notes in `re/docs/snd_mngr` and `re/docs/snd_seseq`).
- **Data**: Corpus statistics across all 861 `.snd` banks on the PAL disc (`extracted/disc/**/NAME.snd`, including 20 duplicate copies inside menu and fishing packs outside `snd2/`).
- **Sony SDK**: The `.hd` file uses Sony's standard hardware-synth bank format, parsed on the IOP by Sony's `modhsyn` module through the EZMIDI sample driver (the EE `CSound` class sends MIDI messages to it). Structure and field names follow Sony's `sceHardSynth*` SDK headers. Every record across all 861 banks parses cleanly with exact size and offset agreement, though the exact runtime effect of rarely varied fields was not tested on hardware.

## 1. The `.snd` pack

A `.snd` file uses the same 16-byte-aligned chain pack container as the game's other archives (see [the disc](disc.md)):

| Offset | Size | Field |
|---|---|---|
| `0x00` | 64 | Member filename, NUL-terminated CP932 string. **Bytes after the first NUL contain leftover exporter memory** (never zero-filled on the disc). |
| `0x40` | 4 | Data offset from the start of the slot header (always `0x50`). |
| `0x44` | 4 | Member data byte size. |
| `0x48` | 4 | Offset to the next slot header (`0x50 + align16(size)`). |
| `0x4C` | 4 | Unused word. Contains the same leftover value in every slot of a pack (`0`, `1`, `3`, or a large number). |
| `0x50` | `size` | Member payload bytes, followed by zero-padding to a 16-byte boundary. |

The chain ends with a 0x50-byte terminator slot whose filename starts with a NUL byte (`0x00`) and whose header words hold leftover values (`.. 50 00 00 00 ff ff ff ff ff ff ff ff xx xx xx xx`).

When `sndLoadSound` loads a `.snd` pack, it looks up members by extension using `GetPackFileExt`. This function compares extensions case-insensitively and returns matching members in pack order. Although 28 different member orderings appear across the disc, pack order does not affect the engine except for the relative order of `.sq` and `.mid` members.

| Extension | Count per Bank | Purpose |
|---|---|---|
| `cfg` | 1 (required; loading fails without it) | Sound-effect entry table. |
| `vol` | 1 (required) | Default per-entry volumes and port reverb settings. |
| `hd` | 1 (optional in engine code; present in all 861 banks) | Instrument and sample header, uploaded to the IOP heap. |
| `bd` | 1 (optional in engine code; present in all 861 banks) | ADPCM sample data body, DMA-transferred to SPU2 RAM. |
| `sq` | `0..8` | Sony MIDI sequences, indexed in pack order (see [music](music.md)). The engine loads `.sq` members only on game ports `0`, `2`, `4`, and `11` and retains at most 32 names. The IOP driver holds at most 10 sequences per port (`MIDI_PORT_SEQ_MAX`) and only logs a warning at 16, so keep banks to at most 10 `.sq` files. Six `SR_*` banks on the disc have 8. |
| `mid` | `0..48` | Standard MIDI File (SMF) sound-effect sequences, converted to engine events at load time. |

Every one of the 861 banks on the disc contains exactly one `.hd`, `.bd`, `.cfg`, and `.vol` member. Across the entire disc, those banks also contain 447 `.sq` members and 2,146 `.mid` members.

### Which copy the game reads

Static analysis of `_SND_LOAD_SOUND`, `GetDefEventSeFile`, `GetLoadBGBuff`, `LoadFile2`, `CDRead`, `InitCDFile`, and `MainLoop` shows how the engine resolves `.snd` file paths:

- **Event sound banks**: When an event script calls `load_sound(set, &id)`, the engine builds the path `snd2/event/EV_<a>_<b>.snd` from the sound set's event column (`GetDefEventSeFile`, format string at `at_1132__4`) and passes it to `GetLoadBGBuff`. `GetLoadBGBuff` checks three sources in order:
  1. An active background read for the same path (`start_background_load`, reading the same disc file).
  2. The preloaded pack from `load_pack_file` (only map `d01b01` uses this, and that file is not on the disc).
  3. `LoadFile2`, which prepends the current directory, looks up the path case-insensitively in `DATA.HD4` (`SearchFile`, returning the first match; only `wep_t/ws050_item.chr` appears twice in `DATA.HD4`), and reads the corresponding sectors from `DATA.DAT`.
  String paths passed to `load_sound` follow the exact same lookup path. Because the `LoadFile2` RAM cache holds only villager models (`src/scenevillager.cpp`) and hard-disk paths are stubbed out in the PAL release, scripts always read the bank directly from `DATA.DAT`.
- **System bank**: At boot (`MainLoop`, `at_1212`), the engine loads `snd2/SY_000.snd` into game port `6`. The duplicate file `snd2/system/SY_000.snd` (an older version) is never read.
- **Field and menu banks**: Field banks load from `snd2/bs/BS_*.snd`, `snd2/fg/FG_*.snd`, `snd2/env/SR_*.snd`, and `snd2/ob/OB_*.snd` (`GetSe*File`). Menu banks load both from `snd2/sp/` and from the embedded copies inside menu packs (`menu/0/inv0_bg.pac/SP_001.snd`, etc.).
- **Other archives**: `SOUND.DAT` and `SOUND.HD3` hold streamed `.wav` voice files, while `DATA.HD3` is an unused legacy index. Neither contains `.snd` banks.

Note that some duplicate banks on the disc differ between copies: `SP_001`, `SP_002`, and `SP_003` inside the menu `.pac` archives have different `.vol` or `.hd` members from their counterparts in `snd2/sp/`, the two `SY_000.snd` files differ, and `BG_193.snd` inside the fishing packs differs from `snd2/bgm/BG_193.snd`.

## 2. From `play_se(bank, n)` to voices

`sndLoadSound(port, pack)` registers the bank on `port` and returns a 32-bit sound handle:

```
sound_id = (port << 24) | (bank << 16)
```

where `bank` is the sequential slot index (`0..15`, max 16 banks) on that port. Calling `play_se(id, n)` invokes `sndSePlaySeID(id, n, velocity = -1, vol = -1, pan = 0x40, pitch = 0x2000, voice)`, which looks up entry `n` in the bank's 12-byte (`0x0C`) `sndSeInfo` runtime table (`type`, `prog`, `key`, `flag`, `def_vol`) built from `.cfg` and `.vol`:
- If `vol < 0`, the engine substitutes the entry's `def_vol` from `.vol`.
- **Type `1` (`KeyOn`)**: Calls `CSound::SE_Play(port, bank, prog, key, ...)`, which sends a MIDI sequence to the IOP:
  - `Bn 00 bank` (Bank Select)
  - `Cn prog` (Program Change)
  - Two `F9` hardware-synth messages (volume/pan and pitch)
  - `FD 10 key id velocity` (Key-On, converting `velocity = -1` to `127`)
- **Type `2` (`sq`)**: Calls `SQ_Play(sq_port, prog = sequence_index, vol)` to start a Sony `.sq` sequence.
- **Type `3` (`seseq`)**: Calls `PlaySeSeq` to run the converted `.mid` sequence on the EE. Each MIDI note-on event calls `SE_Play` using the track's program, the note's key, and the **note's own velocity** (verified in the assembly of `sndCSeSeq::NoteOn`).
- **Type `0` (None)**: Does nothing.

On the IOP, the program number selects `Prog[prog]` inside the bank's `.hd` chunk. Sony's hardware synth then triggers one SPU2 voice for every split in `Prog[prog]` whose key range (`range_low..range_high`) includes `key`, and for every sample inside that split's sample set (`Sset`) whose velocity range (`vel_range_low..vel_range_high`) includes `velocity`:

```
cfg row n --KeyOn--> prog, key, velocity 127
          --.mid---> per note: track prog, note key, note velocity
          --.sq----> Sony sequence (selects programs internally)

prog -> Prog[prog] -> splits where range_low <= key <= range_high
     -> Sset[split.sample_set] (vel_limit_low..vel_limit_high)
     -> Smpl[i] for each sample in Sset where vel_range_low <= velocity <= vel_range_high
     -> Vagi[sample.vag_index] -> .bd byte offset, sample rate, loop flag
```

**Corpus statistics across all 16,127 `.cfg` entries**:
- **By entry type**: 13,284 `KeyOn` entries, 2,142 `.mid` entries, 447 `.sq` entries, and 254 type `0` entries (referencing a `.mid` or `.sq` file absent from the pack).
- **Silent `KeyOn` rows**: 9,503 of the 13,284 `KeyOn` entries produce no sound because they are empty template placeholders (`-` in 7,904 rows, e.g. map base banks sharing the `BS_999` template or monster banks sharing generic slots). Specifically, 9,236 rows reference an absent program, 256 reference a program with no split covering the requested key, and 11 reference samples whose VAG index is `0xFFFF`.
- **Playable entries**: 4,262 entries trigger a single voice, while 1,661 entries trigger 2 or more voices (via multi-split programs, velocity layers, or sequences).
- **Sample sharing**: 1,531 VAG samples are shared by multiple `.cfg` entries. Out of 7,397 total VAG samples on the disc, 2,487 are never referenced directly by a `KeyOn` row; across the 841 banks in `snd2/`, 2,009 of those are played exclusively by `.sq` or `.mid` sequences, and 310 are completely unused.

## 3. `.cfg` (sound-effect table)

The `.cfg` member is a CP932 text file whose lines end with `\r\n` (782 banks) or `\r` (79 banks). The engine's `GetLine` function accepts `\r\n`, `\r`, or `\n` line endings and splits columns on tabs or on single spaces not followed by another space (collapsing runs of spaces).

`LoadSeInfoTxt` parses the file in two passes:
1. It counts every line before the line whose first column is `END` (including the initial header line) and allocates `se_num` entries.
2. It parses each line in order:
   - `END` stops parsing.
   - `REVERBE type depth` configures hardware reverb on the port's SPU2 core (`Room`, `Studio_A`, `Studio_B`, `Studio_C`, `Hall`, `Space`, `Echo`, `Delay`, `Pipe`, `Max` = `1..10`; unused in all `.cfg` files on the disc, as `.vol` sets reverb instead).
   - Any line whose first character is an ASCII digit (`'0'..'9'`) populates **the next sequential table slot**. Because the parser ignores the numeric value of column 0, entry `n` is always the `n`-th digit-leading line in the file.
   - All other lines (such as the header line) are skipped.

Every `.cfg` file on the disc begins with a header line followed by rows numbered `0, 1, 2, ...` with 8 columns per row:

| Column | Stack Buffer | Content |
|---|---|---|
| `0` | 8 bytes | Entry number (for human reference only; not parsed as an index). |
| `1` | 64 bytes | Category label in Japanese (`-` on placeholder rows). |
| `2` | 64 bytes | Sound name in Japanese. |
| `3` | 8 bytes | `0` or `*` (never read by the engine). |
| `4` | 64 bytes | Trigger type: `KeyOn`, `KeyON`, `Keyon`, `-`, empty, or a sequence filename (`X.sq` / `X.mid`). |
| `5` | 8 bytes | Program number for `KeyOn` rows; ignored for `.sq` / `.mid` rows (overwritten with the sequence index; written as `*` on `.sq` rows). |
| `6` | 8 bytes | MIDI key number (`60` in 15,581 rows; `62`, `49`, `35`, `53`, and `58` also occur). |
| `7` | 12 bytes | `○` or empty -> stored in the runtime `flag` byte, which the engine never reads. |

The header line contains the bank prefix (e.g. `SP`), bank title, two integers, and empty columns. The terminator line is `END` followed by seven tabs (846 banks) or `END` alone (15 banks).

> **Caution**
> `LoadSeInfoTxt` copies each column into a fixed-size stack buffer without bounds checking. Column strings longer than their buffer size (63 bytes for names/categories, 7 bytes for numbers) will overflow the EE stack.

`SearchSeq(col4)` determines the entry type from column 4:
- Exact match `"KeyOn"` -> type `1` (`KeyOn`)
- Case-insensitive match to a loaded `.sq` member name -> type `2` (`sq`), setting `prog` to the `.sq` index
- Case-insensitive match to a loaded `.mid` member name -> type `3` (`seseq`), setting `prog` to the `.mid` index
- Any other string containing `.` -> type `0` (missing sequence file, silent)
- Anything else (`KeyON`, `Keyon`, `-`, or empty) -> type `1` (`KeyOn`)

`prog` and `key` are parsed via `atoi` into signed bytes (`s8`). Because `SE_Play` masks `prog` to 7 bits (`0..127`), **a sound bank can address at most 128 programs (`0..127`)**.

## 4. `.vol` (default volumes)

The `.vol` member is an ASCII text file with `\n` (725 banks), `\r\n` (135 banks), or `\r` (1 bank) line endings and tab- or space-separated columns (for example, `0       127`). `LoadVolInfoTxt` processes each line:
- `END` stops parsing.
- `REVERB type depth` (numeric arguments, present in 79 banks, e.g. `REVERB 4 32`) calls `SetReverb` on SPU2 core 0 when the bank's driver port is `0`, or on core 1 when the driver port is `7` (ignored on other ports).
- Any other line parses `index = atoi(col0)` and `vol = atoi(col1)`, setting `se[index].def_vol = vol` if `index < se_num`.

Any entry not listed in `.vol` keeps the default volume `0x40` (`64`). Because `LoadSeInfoTxt` includes the `.cfg` header line when counting `se_num`, `se_num` is 1 greater than the number of sound rows; accordingly, 851 banks list one extra dummy volume row at the end, 6 banks list the exact row count, and 4 banks list 55 extra rows.

## 5. `.hd` (instrument header)

The `.hd` member is a little-endian Sony hardware-synth bank header consisting of six chunks in a fixed order: `Vers`, `Head`, `Prog`, `Sset`, `Smpl`, and `Vagi`. Every chunk begins with a 12-byte header:
- `0x00` (`char[4]`): Creator tag `"IECS"` (`"SCEI"` byte-reversed as a little-endian word).
- `0x04` (`char[4]`): Chunk type byte-reversed (`"sreV"` = `Vers`, `"daeH"` = `Head`, `"gorP"` = `Prog`, `"tesS"` = `Sset`, `"lpmS"` = `Smpl`, `"igaV"` = `Vagi`).
- `0x08` (`u32`): Total chunk byte size, including the 12-byte header and trailing padding.

The four table chunks (`Prog`, `Sset`, `Smpl`, `Vagi`) follow the 12-byte header with:
- `0x0C` (`u32`): `max_index` (`count - 1`).
- `0x10` (`u32[count]`): Offset table relative to the start of the chunk (`0xFFFFFFFF` marks an absent record).
- Record payloads placed back-to-back **in ascending index order**, followed by `0xFF` padding to a 16-byte boundary.

All 861 `.hd` files on the disc follow this layout with zero gaps and no shared record offsets.

### Vers (0x10)

| Offset | Type | Field | Value |
|---|---|---|---|
| `0x0C` | `u16` | Reserved | `0` |
| `0x0E` | `u8` | Version major | `2` |
| `0x0F` | `u8` | Version minor | `0` |

### Head (0x40), at 0x10

| Offset | Type | Field |
|---|---|---|
| `0x0C` | `u32` | `.hd` total file byte size |
| `0x10` | `u32` | `.bd` total file byte size |
| `0x14` | `u32` | File offset of `Prog` chunk |
| `0x18` | `u32` | File offset of `Sset` chunk |
| `0x1C` | `u32` | File offset of `Smpl` chunk |
| `0x20` | `u32` | File offset of `Vagi` chunk |
| `0x24` | `u32` | SE timbre chunk offset (`0xFFFFFFFF` = none) |
| `0x28` | `u32[6]` | Reserved (`0xFFFFFFFF`) |

### Vagi (VAG infos), at 0x50; record 8 bytes

| Offset | Type | Field | Corpus Values |
|---|---|---|
| `0x00` | `u32` | Byte offset of the VAG sample in `.bd` | Ascending, starts at `0`, 16-byte aligned, contiguous. |
| `0x04` | `u16` | Base sample rate in Hz | `28000` (2,765), `32000` (1,520), `44100` (652), `24000`, `44066`, `22050`, etc. |
| `0x06` | `u8` | Loop attribute (`1` = looped, `0` = one-shot) | 2,106 looped, 5,291 one-shot. Always matches the ADPCM block flags in `.bd`. |
| `0x07` | `u8` | Reserved | Always `0xFF`. |

No `Vagi` table contains absent (`0xFFFFFFFF`) entries. A VAG sample's byte length is not stored explicitly; it runs from its offset to the next VAG's offset (or to the end of `.bd`).

### Smpl (samples, Sony `SampleParam`); record 0x2A bytes

| Offset | Type | Field | Typical Corpus Values |
|---|---|---|
| `0x00` | `u16` | VAG index (`0xFFFF` = no VAG, present in 111 samples across 35 banks; silent) | `0..` |
| `0x02` | `u8` | Velocity range low | `1` (or `11`, `21`, etc. in multi-layer sample sets) |
| `0x03` | `u8` | Velocity cross-fade start | `64` |
| `0x04` | `u8` | Velocity range high | `127` |
| `0x05` | `s8, u8, u8` | Velocity-follow pitch, centre, and curve | `0, 1, 0` |
| `0x08` | `s8, u8, u8` | Velocity-follow amplitude, centre, and curve | `0, 1, 0` (curve `2` in 19 samples) |
| `0x0B` | `u8` | Base MIDI note (plays at the `Vagi` sample rate) | `60`, `48`, `72`, etc. |
| `0x0C` | `s8` | Fine detune | `0` |
| `0x0D` | `u8` | Panpot (`64` = centre) | `64` |
| `0x0E` | `u8` | Voice group | `0..112` |
| `0x0F` | `u8` | Priority | `10`, `100` |
| `0x10` | `u8` | Sample volume | `128` |
| `0x11` | `u8` | Reserved | `0xFF` |
| `0x12` | `u16` | `ADSR1` (raw SPU2 register value) | `0x80FF` |
| `0x14` | `u16` | `ADSR2` (raw SPU2 register value) | `0x5FEC` |
| `0x16` | `5 × (s8, u8)` | Key-follow `AR`, `DR`, `SR`, `RR`, `SL` and their centre notes | `0, 60` |
| `0x20` | `u16[4]` | Pitch LFO delay and fade; amplitude LFO delay and fade | `0` |
| `0x28` | `u8` | LFO attributes | `0`, `1`, `16`, `17` |
| `0x29` | `u8` | SPU attributes bitfield | `0x2F` (`0x1F`, `0x23`, etc.) |

### Sset (sample sets); record 4 + 2n bytes, not aligned

| Offset | Type | Field | Corpus Values |
|---|---|---|---|
| `0x00` | `u8` | Velocity curve | `0` or `2` |
| `0x01` | `u8` | Velocity limit low | `1` |
| `0x02` | `u8` | Velocity limit high | `127` |
| `0x03` | `u8` | Sample count `n` | `1` (12,868 sets), `4` (165), `6` (116), `13` (21), `2` (3) |
| `0x04` | `u16[n]` | Indices into the `Smpl` chunk | |

Sample sets with multiple samples partition the velocity range (e.g. `1..10`, `11..20`, etc.). Because `KeyOn` sound effects always use velocity `127`, only `.mid` and `.sq` sequences (which pass per-note velocities) can trigger the lower velocity layers.

### Prog (programs, Sony `ProgramParam`); record 0x24 + n x 0x14 bytes

| Offset | Type | Field | Typical Corpus Values |
|---|---|---|
| `0x00` | `u32` | Offset of split array from start of record | Always `0x24` |
| `0x04` | `u8` | Split count `n` | `1` (4,891 programs), `2` (1,119), `3` (461), up to `57` (4); `0` in 4 programs |
| `0x05` | `u8` | Split block byte size | Always `0x14` |
| `0x06` | `u8` | Program volume | `128` |
| `0x07` | `u8` | Panpot (`64` = centre) | `64` |
| `0x08` | `s8` | Transpose (semitones) | `0` |
| `0x09` | `s8` | Detune | `0` |
| `0x0A` | `s8, u8` | Key-follow pan and centre note | `0, 60` |
| `0x0C` | `u8` | Attribute | `0` (`1` in 29 programs) |
| `0x0D` | `u8` | Reserved | `0xFF` |
| `0x0E` | `u8[2]` | LFO waveforms 1 and 2 | `0`, `3`, `6` |
| `0x10` | `u8[2]` | LFO start phases | |
| `0x12` | `u8[2]` | LFO random phase flags | `0`, `254` |
| `0x14` | `u16[2]` | LFO cycle periods | `100`, `30000` |
| `0x18` | `s16[2]` | LFO pitch modulation depths | `128` |
| `0x1C` | `s16[2]` | LFO MIDI pitch modulation depths | `128` |
| `0x20` | `s8[4]` | LFO amplitude depths and MIDI amplitude depths | |

Each split block (`0x14` bytes) has the following layout:

| Offset | Type | Field | Typical Corpus Values |
|---|---|---|
| `0x00` | `u16` | Sample set index (`Sset`) | |
| `0x02` | `u8` | Key range low | `12`, `60`, etc. |
| `0x03` | `u8` | Key cross-fade | `60` |
| `0x04` | `u8` | Key range high | `119`, `59`, etc. |
| `0x05` | `u8` | Split index | `0..n-1` |
| `0x06` | `u16` | Pitch-bend range low (`256` = 1 semitone) | `256` |
| `0x08` | `u16` | Pitch-bend range high | `256` |
| `0x0A` | `3 × (s8, u8)` | Key-follow pitch, amplitude, and pan with centre notes | `0, 60` |
| `0x10` | `u8` | Split volume | `128` |
| `0x11` | `u8` | Split panpot | `64` |
| `0x12` | `s8` | Split transpose | `0` |
| `0x13` | `s8` | Split detune | `0` |

Programs, sample sets, and samples can be marked absent (`0xFFFFFFFF` in the chunk offset table): 506 banks contain absent programs (to preserve fixed template indices), and 421 banks contain absent sample sets or samples. The largest program table on the disc has 128 entries (`FG_000.snd`).

## 6. `.bd` (samples): PS2 SPU2 ADPCM

The `.bd` member stores raw PS2 SPU2 ADPCM sample bodies concatenated back-to-back with no file header. Its total byte size is always a multiple of 16. Each VAG stream consists of 16-byte blocks encoding 28 16-bit PCM samples:

| Byte | Content |
|---|---|
| `0` | Low nibble: shift factor (`0..12`); high nibble: predictor filter (`0..4`). |
| `1` | Loop flags: bit 0 (`1`) = loop end, bit 1 (`2`) = loop repeat, bit 2 (`4`) = loop start. |
| `2..15` | 28 signed 4-bit ADPCM nibbles, **low nibble first**. |

Each 4-bit signed nibble decodes to a 16-bit PCM sample `s`:

```c
s = ((nibble << 12) >> shift) + ((h1 * c1 + h2 * c2 + 32) >> 6);
```

clamped to `[-32768, 32767]`, where `h1` and `h2` are the previous two decoded samples and `(c1, c2)` are the predictor filter coefficients:
- Filter `0`: `(0, 0)`
- Filter `1`: `(60, 0)`
- Filter `2`: `(115, -52)`
- Filter `3`: `(98, -55)`
- Filter `4`: `(122, -60)`

On the PAL disc, shifts always fall in `1..12` and filters in `0..4`.

Every VAG stream across all 7,397 samples on the disc follows one of four block-flag patterns (`+` denotes one or more blocks):

| Sample Type | Block Flag Sequence | Corpus Count |
|---|---|---|
| One-shot | `0  4  0+  1  7` | 5,273 |
| One-shot (single data block) | `0  5  7` | 18 |
| Looped | `0  2+  6  2+  3` | 1,678 (plus 398 `0 2 6 2+ 3` and 1 `0 2+ 6 3`) |
| Looped from first data block | `0  6  2+  3` | 29 |

- **Initial block (`0`)**: Always 16 zero bytes (28 samples of SPU2 lead-in silence).
- **One-shot stream**: The first data block sets the hardware loop address (flag `4`), and the final data block sets the end flag (`1`, causing the SPU2 to jump to the loop address and mute the voice). An unplayed 16-byte terminator block (`00 07 77 77 ... 77`) always follows the end block.
- **Looped stream**: Every data block sets the repeat bit (`2`), the loop-start block sets `6` (`4 | 2`), and the final block sets `3` (`2 | 1`: end and repeat, jumping back to the loop-start block without muting).

Consequently, loop points always align to 28-sample block boundaries: `loop_start = (loop_block_index - 1) * 28` samples after the initial zero block, and `loop_end` is the end of the VAG stream. The `Vagi` loop attribute (`0` or `1`) matches the ADPCM block flags in all 7,397 samples.

## 7. `.sq` and `.mid`

- **`.sq` (Sony sequence)**: Stores background music inside `BG_*` banks and ambient sound loops inside `snd2/env/SR_*.snd` (plus 7 sequences in `snd2/event/EV_*.snd`). The IOP plays `.sq` members via `SQ_Play` using the parent bank's `Prog` table. For the binary `.sq` format, see [music and ambient loops](music.md); for replacing sequences with MIDI files, see [editing music](../guide/music.md).
- **`.mid` (Standard MIDI File)**: All 2,146 `.mid` members on the disc are Format 0, single-track SMF files with a time division of `480` ticks per quarter note. At load time, `sndCSeSeqData::LoadSMF` converts each `.mid` into a compact array of 6-byte engine events supporting Note On, Note Off, Program Change, Pitch Bend, and Control Changes `7` (Volume), `10` (Pan), `11` (Expression), and `110` (Loop: value `0` marks loop start, value `127` jumps back to loop start). MIDI channels map to engine tracks `0..7` (channel 0 carries only CC `110` loop markers in these files). Each track initializes to program `0`, volume `127`, expression `127`, and pan `64`.

## 8. Memory the game gives a bank

- **SPU2 RAM** (`CSound::Init`, `LoadHdBd2`): Each loaded bank occupies `bd_size + 16` bytes starting at its driver port's current SPU2 address. Game ports map to IOP driver ports and SPU2 memory regions as follows:

  | Game Port | Bank Category | Driver Port | SPU2 Base | Growth Direction and Bound |
  |---|---|---|---|---|
  | `0` / `11` | Music (`BG_*`) | `0` / `3` | `0x5210` | Upward to `0x7D210` (`0x78000` bytes; largest `BG_*` bank uses `0x77E80`). |
  | `3, 9, 2, 1, 4` | Base (`BS`), battle (`FG`), ambient (`SR`), objects (`OB`), event (`EV`) | `10, 8, 1, 15, 14` | `0x7D210` | Upward, stacked in listed order. Reloading an earlier layer resets the start address of all later layers in this group. |
  | `5` | Monsters (`EN_*`) | `13` | `0x18AE20` | Downward into the top of the field region. |
  | `6` | System (`SY_000`) | `12` | `0x18AE20` | Upward to `0x1A82E0` (`0x1D4C0` bytes). |
  | `8` | Menu (`SP_*`) | `11` | `0x1A82E0` | Upward toward Core 1 reverb work area (`0x1D9020`). |
  | `7` | Player character (`CH_*`) | `9` | `0x1E0000` | Upward toward Core 0 reverb work area (`0x1F9020`). |
  | `10` | Unused | `7` | `0x18AE20` | Upward (never loaded). |

  If a field or event bank's `.bd` crosses `0x18AE20` into the system sound region, `TransHdBd` prints `"SYS AREA HAKAI"`. Growing a bank's `.bd` reduces the SPU2 RAM available to banks stacked after it in the same region.
- **EE memory stack** (`mgCMemory`, configured in `CScene::InitSe*` and `InitBGM` in `src/scenesnd.cpp`):
  - BGM: `0x400` bytes (1,024)
  - Ambient (`SR`): `0x400` bytes (1,024)
  - Objects (`OB`): `0x400` bytes (1,024, shared across loaded object banks)
  - Base (`BS`): `0x2000` bytes (8,192)
  - Battle (`FG`): `0x2000` bytes (8,192)
  - Player character (`CH`): Allocated from the character's own memory stack (`src/dng_main.cpp`).
  In `sndLoadSound`, a bank allocates `16 * (ceil(se_num * 12 / 16) + 2)` bytes for its `sndSeInfo` entry table, plus the name table and strings for `.sq` members (on music, ambient, and event ports) and the header table, names, and converted 6-byte event streams for `.mid` members. Largest stack uses on the disc: `FG_000` uses 7,072 of 8,192 bytes; `BS_999` uses 4,576 of 8,192; `SR_120` uses 368 of 1,024; `OB_000` uses 176; `BG_031` uses 160. Each additional `.cfg` row costs 12 bytes.
- **IOP heap**: The `.hd` member is copied into an IOP heap allocation of `max(256, hd_size + 256)` bytes.
- **EE read buffer**: Before parsing, the engine reads the entire `.snd` file into the shared 3.2 MB `read_buffer` (see [the memory budgets](memory.md)).

## 9. Open problems

1. **Untested Sony `.hd` parameter semantics**: While the binary chunk layouts and record sizes are verified across all 861 banks, the exact runtime behaviour of velocity cross-fades, pitch-bend ranges (`256` = 1 semitone), velocity curves, LFO depths, SPU attribute bits, and pitch-follow formulas relies on Sony SDK documentation rather than hardware measurement.
2. **ADPCM decoder rounding**: The formula `(h1 * c1 + h2 * c2 + 32) >> 6` follows standard PS1/PS2 SPU documentation; exact 1-LSB hardware rounding and shift values `13..15` (which never appear on the disc) were not hardware-tested.
3. **SPU2 and EE memory budgets**: Complete SPU2 region bounds, reverb work areas, and worst-case combined bank sets are documented in [the memory budgets](memory.md). The specific EE stack used for monster (`EN_*`) and event (`EV_*`) `sndSeInfo` tables has not been traced.
