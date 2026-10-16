# Sound banks: the code

How dc2tk reads, changes and writes the game's sound banks. The format is
[the sound bank format](../../../docs/internals/sounds.md) (section numbers below are that page's), and
how mods use it is [editing sounds](../../../docs/guide/sounds.md).

`soundbank.rs` (in `src/crates/dc2tk-assets/src/`) implements all of it, and `sounds.rs` turns it
into the WAV folders mods use. Mods reach it through `sounds.rs` (the WAV folders) and `replace`
lines by sound id.

## Reading and writing the pack

The `.snd` pack is the game's chain format (`dc2tk_disc::archive`: `parse_pack` finds the
members, `Pack` reads one whole to write it back byte for byte; `soundbank::read_pack` reads a
bank's). Byte-exact rewriting keeps each slot's 64-byte name field, its fourth word
and the end slot as found (`archive::Pack::write`); new members get zeros
after the name and in the fourth word (`archive::PackMember::new`,
`write_slot`; a mod's new member goes after the last,
`dc2tk_disc::rebuild::added_member`). Data that does not chain slot by slot
to an end slot right at its end is not a pack, and a member name with `/`
or `\`, or `.` or `..`, is refused. A bank is read strictly: every member's
padding must be zeros (as on the disc), so the bank written back is the file
read, and it must have exactly one `.hd`, `.bd`, `.cfg` and `.vol`, the Head's
body size the `.bd`'s and the VAG offsets ascending from 0
(`Bank::from_bytes`); anything else is an error, never a panic.

The `.hd` rewrite is byte-exact because all 861 files lay out their chunks exactly as section 5
describes (no gaps, no shared records, records start right after the table).

Where copies of a bank's name differ (section 1), `tree::bank_sounds` makes an entry that plays
the same sound in several of them one sound with all their places.

## `soundbank.rs`

`dc2tk_assets::soundbank`
([`src/crates/dc2tk-assets/src/soundbank.rs`](../../crates/dc2tk-assets/src/soundbank.rs)).

`Bank::voices(&entry)` returns the chains of section 2 (`Voice`: program, split, sample set,
sample, VAG, key, velocity); an entry's `kind` is an `EntryKind`: `None`, `KeyOn { prog, key }`,
`Sq { seq }` or `SeSeq { seq }`. `voices()` skips samples without a VAG (VAG index 0xFFFF).

`decode_adpcm` decodes the `.bd` as section 6 describes. `encode_adpcm` writes exactly the
layouts of section 6 (same lengths and same flag bytes as Sony's encoder for every VAG on the
disc). Per block it tries all 5 filters x 13 shifts closed-loop (predicting from its own
reconstructed history, as the SPU does) and keeps the least squared error.

```rust
use dc2tk_assets::soundbank::{self as sb, Bank, Mode};
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

- WAV input is what [editing sounds](../../../docs/guide/sounds.md#what-a-wav-may-be) lists;
  the resampling rate is `replace_vag`'s `rate`.
- New chains copy the replaced entry's first voice, as the modding page describes.
- New program numbers avoid every number a KeyOn row or a `.mid` names,
  even when the bank lacks that program: such rows are silent on purpose in
  shared layouts and must stay so. Numbers past the table come first, then
  holes; 0..127 only.
- `add_entry` inserts the row before `END` (tab-separated, name and category
  at most 63 bytes of cp932) and sets (or adds) its `.vol` line, each in its
  file's own line ending (`\r\n` for a file without one); banks whose `.vol`
  already lists the index (the extra entry from the header line) get that
  line updated. A new entry's number is what `add_entry` returns.
- `memory::ee_tables` estimates the EE memory a bank's tables use (section 8), and `build` warns
  when a changed bank's tables outgrow its layer's stack.

## `sounds.rs`: the WAV folders mods use

A mod's WAV folder is `sounds::build_entries`, following the rules on
[editing sounds](../../../docs/guide/sounds.md#how-build-changes-a-bank): a KeyOn entry whose
samples no other entry plays is overwritten with `Mode::InPlace`, any other gets samples of its
own with `Mode::Detach`, and the samples it played before are cut by `free_unused`. The
constants a mod's named WAVs give (`SE_SP_004_BEEP = 5`) come from `replace::wav_constants`.

Decompiled scripts show entry numbers by name through `dc2tk-script`'s `soundpass.rs`; what it
names and how it follows ids is on the modding page.

## Verification

All 861 banks, in the check run while the format was worked out (16
processes, about 6 minutes; its results file is not kept). What runs now:
`soundbank.rs`' unit tests (records the sizes Sony gives, a bank written
back the same, the engine's column split, WAV and ADPCM round trips, a loop
kept through the encoder, the notes a `.mid` plays), `dc2tk assets` and the
hidden `dc2tk assets-check DISC TREE` (every bank's WAV folder, unedited,
builds back to the disc's bytes at every place it is), and the music corpus
tests ([music](music.md)). The run's results:

| Check | Result |
|---|---|
| `.snd` parse -> build byte-exact | 861 / 861 |
| `.hd`, `.bd`, `.cfg`, `.vol` rebuilt from the parsed structures equal the members, and the `NAME.snd.d` files | 861 / 861 each |
| every VAG ends at its end block (+ silent block), flags agree with the Vagi loop bit | 7,397 VAGs (2,106 looped), 222,046,748 samples decoded |
| re-encoding a decoded VAG gives Sony's length and flag bytes | 7,397 / 7,397 |
| decode -> re-encode -> decode | 7,395 VAGs reproduced exactly; the other 2 at 53.5 and 55.6 dB |
| replace an entry (detach) + add an entry, build, re-parse: build idempotent, the two entries play exactly the two new VAGs, every other entry plays identical bytes and parameters, volumes kept, Head sizes and VAG offsets consistent, `dc2tk_disc::archive::parse_pack` accepts the pack | 861 / 861 |
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

## Open problems

1. **`.sq` sequences are decoded** (see [music](../../../docs/internals/music.md)): `voices()`
   resolves a sequence entry's notes to the samples they play,
   `free_program` avoids the programs sequences name, and `free_unused` cuts
   a VAG only when no entry plays it, a sequence's notes included (a member
   no `.cfg` row names is not seen; the disc's banks have 447 `.sq` rows for
   their 447 `.sq` members).
2. **Detaching a `.mid` entry loses the sequence.** `replace_entry` turns a
   sequence entry into one KeyOn sound; writing new `.mid` files (several
   notes, velocity layers, loops via CC 110) is not implemented, though the
   format is plain SMF.
3. **Velocity selection** ignores cross-fades; a KeyOn always plays
   velocity 127, so sets with velocity layers only sound in full from
   sequences.
4. **Not yet heard in the game.** A first test in event 50 (s51) replaced
   entries 4 and 13 of `EV_000_130` and heard the old sounds. The engine
   reads the one copy `build` rewrites (section 1 of the format page, "Which copy the game
   reads"), and that bank's rows 4 and 13 were KeyOn rows of new programs
   with samples of their own. What was heard is most likely the test
   sounds: entry 4's was a 40-140 Hz boom at the entry's default volume, 44
   of 127; entry 6, played a second later as a soldier lands from the
   window, still plays the original glass sample (VAG 14, which the `.mid`
   rows 4, 6, 10 and 11 share); entry 13 plays just before the fade out,
   after which the scene stops every entry. `sound-test.iso` (system
   entries 0 and 1, event 50's entries 4, 6 and 13 as loud test tones at
   volume 127) is to be tried.
5. Two VAGs of the 7,397 are not reproduced bit-exactly by re-encoding
   (equal-error ties between filter/shift choices that lead to different
   later history); harmless, noted for completeness.
