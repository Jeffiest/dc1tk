# Dark Chronicle sound banks (`.snd`)

How the PAL disc stores sound effects, how `play_se(bank, n)` turns entry
`n` into SPU2 voices, and what a mod must keep consistent to replace or add
sounds. `soundbank.rs` (in `src/crates/c2-assets/src/`) implements all of it,
and `sounds.rs` turns it into the WAV folders mods use; the counts
below come from a check run over every bank on the disc while the format
was worked out.

Evidence is of three kinds, marked where it matters:

- **engine**: the decompiled EE code in `finaldc2/ChronicleTwo/ps2`
  (`src/snd_mngr.cpp`, `src/sound.cpp`, `src/snd_seseq.cpp`,
  `src/scenesnd.cpp`, the asm of the not-yet-matched functions
  `LoadSeInfoTxt`, `LoadVolInfoTxt`, `GetLine`, `SearchSeq`, `sndLoadSound`,
  `sndSePlaySeID`, `sndCSeSeq::NoteOn`), and `re/docs/snd_mngr|snd_seseq`;
- **data**: statistics over all 861 banks (`extracted/disc/**/NAME.snd`, 20
  outside `snd2/`: menu and fishing packs hold copies);
- **Sony**: the `.hd` is Sony's hardware-synth bank format, read on the IOP
  by Sony's `modhsyn` through the EZMIDI sample driver (the EE side,
  `CSound`, sends MIDI messages to it). The field names below are Sony's
  `sceHardSynth*` structure names from memory of the SDK headers; the
  layout is confirmed by the data (every record parses, sizes and offsets
  add up in all 861 banks), the meaning of rarely-varying fields is not.

## 1. The `.snd` pack

The same chain format as the game's other packs (`c2_disc::archive::parse_pack`):

| Offset | Size | Field |
|---|---|---|
| 0x00 | 64 | member name, NUL-terminated cp932; **bytes after the NUL are stale memory** (never zero on the disc) |
| 0x40 | 4 | data offset, always 0x50 |
| 0x44 | 4 | data size |
| 0x48 | 4 | offset of the next slot = 0x50 + size padded to 16 |
| 0x4C | 4 | unused; the same leftover value in every slot of a pack (0, 1, 3 or a large number) |
| 0x50 | size | member data, then zero padding to 16 |

The chain ends with a slot whose name starts with NUL (0x50 bytes, also
holding leftovers: `.. 50 00 00 00 ff ff ff ff ff ff ff ff xx xx xx xx`).
Byte-exact rewriting keeps each slot's 64-byte name field, its fourth word
and the end slot as found; new members get zeros (as the project's
`discfiles.rs` writes them).

Members (engine, `sndLoadSound`): looked up by extension with
`GetPackFileExt`, which compares extensions ignoring case and returns
members in pack order. Member order on the disc varies (28 different
orders); it does not matter to the engine except for the order of `.sq`
and `.mid` members.

| Ext | Count per bank | Use |
|---|---|---|
| `cfg` | 1, required (load fails without) | sound-effect table |
| `vol` | 1, required | default volumes, reverb |
| `hd` | 1 (optional to the engine; all banks have it) | instrument header, IOP heap |
| `bd` | 1 (same) | ADPCM sample body, DMA'd to SPU2 RAM |
| `sq` | 0..; loaded only for game ports 0, 2, 4, 11; at most 32 names kept; the driver holds 10 sequences per port (`MIDI_PORT_SEQ_MAX`) and only warns at 16, so keep to 10; the disc has up to 8 per bank (six `SR_*` banks) | Sony sequences, by index ([music](music.md)) |
| `mid` | 0..48 | SMF sound-effect sequences, converted on load |

All 861 banks have exactly one hd/bd/cfg/vol; 447 `.sq` and 2,146 `.mid`.

## 2. From `play_se(bank, n)` to voices

`sndLoadSound(port, pack)` returns a sound ID `port << 24 | bank << 16`
(bank = how many banks the port already held, max 16). `play_se(id, n)`
goes to `sndSePlaySeID(id, n, velocity -1, vol -1, pan 0x40, pitch 0x2000,
voice)`, which reads entry `n` of the bank's `sndSeInfo` table (0xC bytes:
type, prog, key, flag, def_vol) built from the `.cfg` and `.vol`:

- `vol < 0` -> the entry's default volume from the `.vol`;
- type 1 **KeyOn**: `CSound::SE_Play(port, bank, prog, key, ...)` sends
  `Bn 00 bank` (bank select), `Cn prog` (program change), two `F9`
  hardware-synth messages (volume, pan; pitch) and `FD 10 key id velocity`
  (key-on) to the IOP; velocity -1 becomes 127;
- type 2 **sq**: `SQ_Play(sq_port, prog = sequence index, vol)`;
- type 3 **seseq**: `PlaySeSeq` runs the converted `.mid` on the EE; each
  note-on calls `SE_Play` with the track's program, the note's key and the
  **note's velocity** (`sndCSeSeq::NoteOn` passes it, asm checked);
- type 0: nothing.

On the IOP the program number selects a `Prog` record of the bank's `.hd`;
Sony's hardware synth then starts one voice for every split of the program
whose key range holds the key, and within that split's sample set for every
sample whose velocity range holds the velocity:

```
cfg row n --KeyOn--> prog, key            (velocity 127)
          --.mid---> per note: track program, key, velocity
          --.sq----> sequence (programs inside the Sony sequence)
prog -> Prog[prog] -> splits with range_low <= key <= range_high
     -> Sset[split.sample_set] (vel_limit_low..high)
     -> Smpl[i] for i in set with vel_range_low <= velocity <= vel_range_high
     -> Vagi[sample.vag_index] -> .bd offset, sample rate, loop flag
```

`Bank.voices(entry)` returns these chains. Over the disc: 16,127 entries:
13,284 KeyOn, 2,142 `.mid`, 447 `.sq`, 254 naming a file the pack lacks
(type 0). 9,503 KeyOn entries reach no voice: they are placeholders
(`-`, 7,904 rows) or name programs the bank lacks, mostly in banks that
share one layout (all map base banks follow `BS_999`; monster banks list
generic slots): 9,236 name a program the bank lacks, 256 a program
with no split covering the key, 11 only samples without a VAG. 4,262
playable entries start one voice, 1,661 start 2 or more (multi-split
programs, sequences, velocity layers). 1,531 VAGs are reached from more
than one entry; 2,487 of the 7,397 VAGs are reached by no cfg row (music
and ambient sequences use them, or nothing does: over the 841 banks of
`snd2/`, counting a sequence entry's notes, 2,009 VAGs are played only by
sequences and 310 by nothing).

## 3. `.cfg` (sound-effect table)

cp932 text; lines end with `\r\n` (782 banks) or a lone `\r` (79). The
engine's `GetLine` ends a line at `\r\n`, `\r` or `\n` and splits columns at
a tab or at a space not followed by another space; other spaces are
dropped. `LoadSeInfoTxt`:

1. counts the lines before the line whose first column is `END`; that count
   (header line included) is the table size `se_num`;
2. for each line: `END` stops; `REVERBE type depth` sets reverb on the
   port's core (types `Room`, `Studio_A/B/C`, `Hall`, `Space`, `Echo`,
   `Delay`, `Pipe`, `Max` = 1..10; no cfg on the disc has one); a line
   whose first character is a digit fills **the next entry in order**:
   column 0 is not read as a number, so entry `n` is the `n`-th digit line.
   Other lines (the header) are skipped.

Every bank's file has a header line and rows numbered 0, 1, 2... in order,
8 columns each:

| Col | Buffer | Content |
|---|---|---|
| 0 | 8 bytes | entry number (documentation only) |
| 1 | 64 | category (Japanese; `-` in placeholder rows) |
| 2 | 64 | name (Japanese) |
| 3 | 8 | `0` or `*`, never read |
| 4 | 64 | `KeyOn`, `KeyON`, `Keyon`, `-`, empty, or a member name `X.sq` / `X.mid` |
| 5 | 8 | program (KeyOn); ignored for sq/mid (replaced by the sequence index); `*` in sq rows |
| 6 | 8 | key (60 in 15,581 rows; 62, 49, 35, 53, 58 also) |
| 7 | 12 | `○` or empty -> flag byte, never read |

Header line: bank prefix (`SP`), bank title, two numbers, empty columns.
The `END` line is `END` plus seven tabs (846) or `END` alone (15).

The buffers are fixed-size stack arrays: longer columns overflow the stack
(names are at most 63 bytes of cp932). `SearchSeq(col4)` decides the type:
exactly `KeyOn` -> 1; equal (ignoring case) to a `.sq` member name -> 2
with its index; to a `.mid` member name -> 3 with its index; any other name
containing `.` -> 0; anything else (`KeyON`, `Keyon`, `-`, empty) -> 1.
`prog` and `key` are stored with `atoi` into signed bytes; `SE_Play` masks
the program to 7 bits, so **a bank can address programs 0..127 only**.

## 4. `.vol` (default volumes)

Text, `\n` (725), `\r\n` (135) or `\r` (1) line ends; tab- or
space-separated (`0       127`). `LoadVolInfoTxt`: `END` stops; `REVERB
type depth` (numbers; 79 banks, e.g. `REVERB 4 32`) calls `SetReverb` on
core 0 when the bank's driver port is 0, on core 1 when it is 7, otherwise
ignored; any other line sets `se[atoi(col0)].def_vol = atoi(col1)` when the
index is within `se_num`. Entries without a line keep 0x40. 851 banks list
one more index than they have rows (the header line makes `se_num` one
larger; that extra entry has no sound), 6 list exactly the rows, 4 list 55
more.

## 5. `.hd` (instrument header)

Little-endian. Six chunks, always in this order, each starting with a
12-byte header: creator `IECS` (`SCEI` read as a word), chunk type
reversed (`sreV` = `Vers`), chunk size including the header. Table chunks
continue with `max index` (count - 1) and `count` offsets from the chunk
start (0xFFFFFFFF = absent), then the records back to back **in index
order**, then 0xFF padding to a multiple of 16. All 861 files follow this
exactly (no gaps, no shared records, records start right after the table),
which is what makes the rewrite byte-exact.

### Vers (0x10)

| Off | Type | Field | Value |
|---|---|---|---|
| 0x0C | u16 | reserved | 0 |
| 0x0E | u8 | version major | 2 |
| 0x0F | u8 | version minor | 0 |

### Head (0x40), at 0x10

| Off | Type | Field |
|---|---|---|
| 0x0C | u32 | header size = `.hd` file size |
| 0x10 | u32 | body size = `.bd` file size |
| 0x14 | u32 | offset of Prog |
| 0x18 | u32 | offset of Sset |
| 0x1C | u32 | offset of Smpl |
| 0x20 | u32 | offset of Vagi |
| 0x24 | u32 | SE timbre chunk offset: 0xFFFFFFFF (none) |
| 0x28 | 6 x u32 | reserved: 0xFFFFFFFF |

### Vagi (VAG infos), at 0x50; record 8 bytes

| Off | Type | Field | Data |
|---|---|---|---|
| 0 | u32 | offset of the VAG in the `.bd` | ascending, first 0, 16-aligned, contiguous |
| 4 | u16 | sample rate (Hz) the VAG plays at its base note | 28000 (2,765), 32000 (1,520), 44100 (652), 24000, 44066, 22050, ... |
| 6 | u8 | attribute: 1 = loops, 0 = one-shot | 2,106 looped, 5,291 one-shot; always agrees with the block flags |
| 7 | u8 | reserved | 0xFF |

There are no absent Vagi records. A VAG's length is not stored: it runs to
the next VAG's offset (or the end of the `.bd`).

### Smpl (samples, Sony `SampleParam`); record 0x2A bytes

| Off | Type | Field | Data (most common) |
|---|---|---|---|
| 0x00 | u16 | VAG index; 0xFFFF = no VAG (111 samples in 35 banks; `voices()` skips them, presumably silent on the IOP) | |
| 0x02 | u8 | velocity range low | 1 (also 11, 21, ... in velocity layers) |
| 0x03 | u8 | velocity cross-fade | 64 |
| 0x04 | u8 | velocity range high | 127 |
| 0x05 | s8,u8,u8 | velocity follow pitch, centre, curve | 0, 1, 0 |
| 0x08 | s8,u8,u8 | velocity follow amp, centre, curve | 0, 1, 0 (curve 2 in 19) |
| 0x0B | u8 | base note (key that plays at the VAG's rate) | 60, 48, 72, ... |
| 0x0C | s8 | detune | 0 |
| 0x0D | u8 | panpot (64 centre) | 64 |
| 0x0E | u8 | group | 0..112 |
| 0x0F | u8 | priority | 10, 100 |
| 0x10 | u8 | volume | 128 |
| 0x11 | u8 | reserved | 0xFF |
| 0x12 | u16 | ADSR1 (SPU2 register) | 0x80FF |
| 0x14 | u16 | ADSR2 (SPU2 register) | 0x5FEC |
| 0x16 | 5 x (s8,u8) | key follow AR, DR, SR, RR, SL and their centres | 0, 60 |
| 0x20 | u16 x 4 | pitch LFO delay, fade; amp LFO delay, fade | 0 |
| 0x28 | u8 | LFO attributes | 0, 1, 16, 17 |
| 0x29 | u8 | SPU attributes (bits; values 0x2F, 0x1F, 0x23, ...) | 0x2F |

### Sset (sample sets); record 4 + 2n bytes, not aligned

| Off | Type | Field | Data |
|---|---|---|---|
| 0 | u8 | velocity curve | 0 or 2 |
| 1 | u8 | velocity limit low | 1 |
| 2 | u8 | velocity limit high | 127 |
| 3 | u8 | sample count n | 1 (12,868 sets), 4 (165), 6 (116), 13 (21), 2 (3) |
| 4 | u16 x n | sample indices | |

Sets with several samples split the velocity range (1-10, 11-20, ...):
only `.mid` sequences, which pass note velocities, reach the lower layers;
a KeyOn entry always plays velocity 127.

### Prog (programs, Sony `ProgramParam`); record 0x24 + n x 0x14 bytes

| Off | Type | Field | Data |
|---|---|---|---|
| 0x00 | u32 | split blocks offset from the record | always 0x24 |
| 0x04 | u8 | split count n | 1 (4,891 programs), 2 (1,119), 3 (461), ... 57 (4); 0 in 4 |
| 0x05 | u8 | split block size | always 0x14 |
| 0x06 | u8 | volume | 128 |
| 0x07 | u8 | panpot | 64 |
| 0x08 | s8 | transpose | 0 |
| 0x09 | s8 | detune | 0 |
| 0x0A | s8,u8 | key follow pan, centre | 0, 60 |
| 0x0C | u8 | attribute | 0 (1 in 29) |
| 0x0D | u8 | reserved | 0xFF |
| 0x0E | u8 x 2 | LFO waves 1, 2 | 0, 3, 6 |
| 0x10 | u8 x 2 | LFO start phases | |
| 0x12 | u8 x 2 | LFO phase random | 0, 254 |
| 0x14 | u16 x 2 | LFO cycles | 100, 30000 |
| 0x18 | s16 x 2 | LFO pitch depths | 128 |
| 0x1C | s16 x 2 | LFO MIDI pitch depths | 128 |
| 0x20 | s8 x 4 | LFO amp depths, MIDI amp depths | |

Split block (0x14):

| Off | Type | Field | Data |
|---|---|---|---|
| 0x00 | u16 | sample set index | |
| 0x02 | u8 | key range low | 12, 60, ... |
| 0x03 | u8 | cross-fade | 60 |
| 0x04 | u8 | key range high | 119, 59, ... |
| 0x05 | u8 | split number | 0.. |
| 0x06 | u16 | bend range low (1/256 semitone?) | 256 |
| 0x08 | u16 | bend range high | 256 |
| 0x0A | 3 x (s8,u8) | key follow pitch, amp, pan and centres | 0, 60 |
| 0x10 | u8 | volume | 128 |
| 0x11 | u8 | panpot | 64 |
| 0x12 | s8 | transpose | 0 |
| 0x13 | s8 | detune | 0 |

Programs, samples and sets can be absent (offset 0xFFFFFFFF): 506 banks
have absent programs (the shared layouts), 421 absent samples/sets. The
largest program table has 128 entries (`FG_000`).

## 6. `.bd` (samples): PS2 SPU2 ADPCM

The VAG bodies back to back, no header, size a multiple of 16. Each VAG is
16-byte blocks of 28 samples:

| Byte | Content |
|---|---|
| 0 | low nibble: shift 0..12; high nibble: filter 0..4 |
| 1 | flags: bit 0 end, bit 1 repeat, bit 2 loop start |
| 2..15 | 28 signed 4-bit samples, low nibble first |

Decoding (`decode_adpcm`): `s = (nibble << 12 >> shift) + ((h1*c1 + h2*c2
+ 32) >> 6)`, clamped to 16 bits, with (c1, c2) = (0,0), (60,0),
(115,-52), (98,-55), (122,-60). On this disc shifts are 1..12 and filters
0..4.

Every VAG on the disc has one of these layouts (block flags, `+` =
repeated):

| Kind | Flags | Count |
|---|---|---|
| one-shot | `0 4 0+ 1 7` | 5,273 |
| one-shot, one data block | `0 5 7` | 18 |
| looped | `0 2+ 6 2+ 3` | 1,678 + 398 (`0 2 6 2+ 3`) + 1 (`0 2+ 6 3`) |
| looped from the first block | `0 6 2+ 3` | 29 |

- the first block is 16 zero bytes (28 samples of silence on the SPU);
- a one-shot's first data block sets the loop address (flag 4), its last
  has flag 1 (end: the SPU jumps to the loop address and mutes the voice),
  and it is followed by the block `00 07 77 77 ... 77` that is never
  played;
- a looped VAG has flag 2 on every data block, 6 on the loop-start block,
  3 on the last (end + repeat: jump back to the loop start).

So loop points are whole blocks: loop start = (block - 1) x 28 samples
after the zero block, loop end = the end of the VAG. The Vagi attribute
matches the flags in all 7,397 VAGs.

`encode_adpcm` writes exactly these layouts (same lengths and same flag
bytes as Sony's encoder for every VAG on the disc). Per block it tries all
5 filters x 13 shifts closed-loop (predicting from its own reconstructed
history, as the SPU does) and keeps the least squared error.

## 7. `.sq` and `.mid`

`.sq` (Sony sequence): the music of the `BG_*` banks and the ambient loops
of `snd2/env` (and 7 in `snd2/event`), played through `SQ_Play` with the same
bank's `Prog` table. Decoded completely, edited as MIDI: see [music and
ambient loops](music.md).

`.mid`: standard MIDI file, format 0, one track, division 480 (all 2,146).
`sndCSeSeqData::LoadSMF` converts it to 6-byte events: note on/off,
program change, controllers 7 (volume), 10 (pan), 11 (expression), 110
(loop: value 0 marks the loop start, 127 loops), pitch bend. Channels are
tracks 0..7 (channel 0 carries only CC 110 here); each track starts at
program 0, volume/expression 127, pan 64.

## 8. Memory the game gives a bank

- **SPU2 RAM** (`CSound::Init`, `LoadHdBd2`): each bank takes `bd size + 16`
  at its driver port's next address. Game port -> driver port -> region:

  | Game port | Use | Driver | Start | Direction |
  |---|---|---|---|---|
  | 0 / 11 | music `BG_*` | 0 / 3 | 0x5210 | up, region ends 0x7D210 (0x78000 bytes; largest BG bank uses 0x77E80) |
  | 3, 9, 2, 1, 4 | base, battle (`FG`), ambient (`SR`), objects (`OB`), event | 10, 8, 1, 15, 14 (+2) | 0x7D210 | up, stacked in that order: loading one restarts the later ones right after it |
  | 5 | monsters `EN_*` | 13 | 0x18AE20 | down |
  | 6 | system `SY_000` | 12 | 0x18AE20 | up |
  | 8 | menu `SP_*` | 11 | 0x1A82E0 | up |
  | 7 | player character `CH_*` | 9 | 0x1E0000 | up |
  | 10 | ? | 7 | 0x18AE20 | up |

  `TransHdBd` prints "SYS AREA HAKAI" when a body crosses 0x18AE20, where
  the system sounds live. A bank whose `.bd` grows takes SPU memory from
  the banks loaded after it in the same region (field banks) or from the
  next region. Keep a modded bank's `.bd` at most its original size unless
  you know the region has room.
- **EE memory stack** for the tables (`mgCMemory`, sizes in
  `CScene::InitSe*`): BGM 0x400 bytes, ambient 0x400, objects 0x400
  (shared by the object banks), base 0x2000, battle 0x2000; the player
  character's bank uses the character's memory (`dng_main.cpp`). A bank uses `16 x
  (ceil(se_num x 12 / 16) + 2)` for the entry table, plus for `.sq` and
  `.mid` members their name strings, a table and the converted events
  (`Bank.ee_table_bytes()`). Largest on the disc: `FG_000` 7,072 of 8,192;
  `SR_*` 368 of 1,024. Each added entry costs 12 bytes. (Where monster
  banks' tables go was not checked.)
- **IOP**: the `.hd` is copied to an IOP heap block of `max(256, size +
  256)`.
- **EE read buffer**: the whole `.snd` is first read into a shared buffer
  whose size this research did not establish; stay within the size of the
  largest bank loaded the same way.

## 9. Changing a bank with `soundbank.rs`

`c2_assets::soundbank` (`src/crates/c2-assets/src/soundbank.rs`); mods
reach it through `sounds.rs` (the WAV folders) and `replace` lines by
sound id.

```rust
use c2_assets::soundbank::{self as sb, Bank, Mode};
use std::fs;

let mut bank = Bank::from_bytes(&fs::read("extracted/disc/snd2/SY_000.snd")?)?;
for e in bank.entries()? {
    println!("{} {:?} {} {:?}", e.index, e.kind, e.name, bank.voices(&e)?);
}
let first = bank.voices(&bank.entries()?[0])?[0];
fs::write("cursor.wav", sb::wav_bytes(&bank.decode_vag(first.vag)?)?)?;

// replace entry 0 only (adds a VAG, sample, set and program; cfg row 0
// becomes KeyOn of the new program; nothing else changes)
bank.replace_entry(0, &sb::read_wav(&fs::read("my_cursor.wav")?)?, Mode::Detach, None)?;
// or overwrite the VAG(s) in place: every entry sharing them changes,
// the bank grows only by the size difference
bank.replace_entry(2, &sb::read_wav(&fs::read("x.wav")?)?, Mode::InPlace, None)?;
// new entry, played by play_se(id, n): name, category, volume, key
let n = bank.add_entry(&sb::read_wav(&fs::read("new.wav")?)?, "MY_SOUND", "MOD", 100, 60)?;
fs::write("SY_000.snd", bank.build()?)?;
```

- WAV input: PCM 8/16/24/32-bit or float, any channel count (averaged to
  mono), any rate (kept, or resampled with `replace_vag`'s `rate`; above
  48000 is resampled to 48000). A `smpl` chunk loop is honoured: the start is moved
  to a block boundary by leading silence, and the loop body is made a whole
  number of blocks by repeating it (when 4 copies or fewer suffice) or
  stretching it by resampling (pitch change under 28 / body length).
  Without a loop point, a sound replacing a looped VAG loops whole.
- New chains copy the replaced entry's first voice (ADSR, priority, group,
  SPU attributes, program LFO and volume) but set base note = key and every
  transpose/detune to 0, so the WAV plays at its own rate (assuming the
  hardware synth's pitch is rate x 2^((key + transposes - base note)/12),
  as documented by Sony; not measured on hardware).
- New program numbers avoid every number a KeyOn row or a `.mid` names,
  even when the bank lacks that program: such rows are silent on purpose in
  shared layouts and must stay so. Numbers past the table come first, then
  holes; 0..127 only.
- `add_entry` inserts the row before `END` and sets (or adds) its `.vol`
  line; banks whose `.vol` already lists the index (the extra entry from
  the header line) get that line updated.
- The script compiler names entries from `pal.h` enums `se_<bank>_pal`;
  a new entry's number is what `add_entry` returns.

## 10. Verification

All 861 banks, 16 processes, about 6 minutes (`check_results.txt`):

| Check | Result |
|---|---|
| `.snd` parse -> build byte-exact | 861 / 861 |
| `.hd`, `.bd`, `.cfg`, `.vol` rebuilt from the parsed structures equal the members, and the `NAME.snd.d` files | 861 / 861 each |
| every VAG ends at its end block (+ silent block), flags agree with the Vagi loop bit | 7,397 VAGs (2,106 looped), 222,046,748 samples decoded |
| re-encoding a decoded VAG gives Sony's length and flag bytes | 7,397 / 7,397 |
| decode -> re-encode -> decode | 7,395 VAGs reproduced exactly; the other 2 at 53.5 and 55.6 dB |
| replace an entry (detach) + add an entry, build, re-parse: build idempotent, the two entries play exactly the two new VAGs, every other entry plays identical bytes and parameters, volumes kept, Head sizes and VAG offsets consistent, `c2_disc::archive::parse_pack` accepts the pack | 861 / 861 |
| in-place VAG replacement, re-parse, other VAGs unchanged | 861 / 861 |

Decode -> re-encode is near-lossless only because the input already lies on
the ADPCM grid. The encoder's real error is measured off the grid: the
first 8,192 samples of every VAG delayed by 14 samples (so blocks do not
line up with Sony's) give SNR min 21.6, 5th percentile 25.0, median 33.3,
95th percentile 54.4 dB, sample-weighted mean 36.1 dB over 7,373 VAGs. On
synthetic signals (full search / fast search): sine 440 Hz 56.8 / 56.1 dB,
sine 5 kHz 37.6 / 37.6, chirp 50 Hz-15 kHz 26.9 / 23.7, white noise
21.3 / 21.3, decaying noise 19.6 / 17.4 dB; about 50-80 k samples/s per
core in full search (a 2-second 44.1 kHz effect: 1-2 s). A stereo 24-bit
WAV chirp at 32 kHz, read and encoded as a replacement, decodes at 42.8 dB
against the mono mix. Replacing one entry and adding one grew the `.bd` by
3,536-7,264 bytes in the test.

## 11. Open problems

1. **`.sq` sequences are decoded** (see [music](music.md)): `voices()`
   resolves a sequence entry's notes to the samples they play, and
   `free_program` avoids the programs sequences name. `free_unused` still
   leaves banks with sequences alone.
2. **Field meanings from Sony's names, not measured.** Layout and sizes
   are certain; the semantics of cross-fades, bend ranges (u16, 256 =
   1 semitone?), velocity curves, LFO fields, the SPU-attribute bits and
   the exact pitch formula (base note, transposes, detunes, key follow) are
   Sony's documented meanings assumed, not tested on hardware or in an
   emulator. Nothing here was played back in PCSX2.
3. **Decoder rounding.** `(h1*c1 + h2*c2 + 32) >> 6` follows the PS1/PS2
   SPU descriptions; the hardware's exact rounding (1 LSB) and shifts
   13-15 (never on this disc) are not verified.
4. **SPU memory accounting** is now in [the memory budgets](memory.md#6-sound-memory-spu2):
   a set's event column is never loaded with its field banks (a script's
   `load_sound(set)` loads it alone, over the set already loaded), which
   explains row 1837; the regions' ends, the reverb work areas and the
   fullest sets the scripts make are there. Port 10 is never loaded.
5. **EE read buffer.** The buffer a `.snd` is read into before
   `sndLoadSound` was not sized; where monster banks' tables are allocated
   was not checked.
6. **Detaching a `.mid` entry loses the sequence.** `replace_entry` turns a
   sequence entry into one KeyOn sound; writing new `.mid` files (several
   notes, velocity layers, loops via CC 110) is not implemented, though the
   format is plain SMF.
7. **Velocity selection** ignores cross-fades; a KeyOn always plays
   velocity 127, so sets with velocity layers only sound in full from
   sequences.
8. Two VAGs of the 7,397 are not reproduced bit-exactly by re-encoding
   (equal-error ties between filter/shift choices that lead to different
   later history); harmless, noted for completeness.
