# Music: the code

The code behind [music in a mod](../../../docs/guide/music.md); the format it reads is in
[music and ambient loops](../../../docs/internals/music.md).

## Modules

In `src/crates/dc2tk-assets/src/`:

- `sequence.rs` reads and writes the `.sq` files. All 447 on the disc are read and written back
  byte for byte. `Sequence::read` refuses anything whose header is not exactly the disc's.
- `music.rs` converts them to and from standard MIDI files: `music::export` writes a sequence as a
  MIDI file, `music::build` makes the `.sq` from one with the disc's sequence as the template,
  `music::build_with_instruments` puts a line's `"instruments"` into the bank before the tune is
  built (so its programs are checked against them), and `music::new_bank` makes a mod's own music
  bank.
- `soundfont.rs` writes a bank's instruments as a SoundFont to hear them with.
- `Bank::voices` resolves a sequence entry's notes to the samples they play; `sounds::free_unused`
  uses it to cut samples nothing plays any more.

In `src/crates/dc2tk-script/src/`, `replace::add_music` adds a mod's own music file (a `music:`
line under `add`) to the image.

## The SoundFont

`soundfont.rs` writes a bank as presets n of banks 0 and 128 for program n: every split a zone over
its key range, every sample a zone over its velocity range, the VAG at its own rate and loop, the
base note as root key, transposes as coarse tune, detunes in 1/128 semitone (assumed), volumes (128
unity) as attenuation, and the SPU2 envelope (ADSR1, ADSR2) turned into SoundFont times by running
it at 48 kHz. LFOs, velocity curves, key follow and bend ranges are left out.

## Verification

The corpus checks are `dc2tk-assets`' tests `corpus_sequences_round_trip` (`sequence.rs`),
`corpus_sequences_round_trip_through_midi` and `corpus_music_banks_make_new_banks` (`music.rs`),
`corpus_banks_give_soundfonts` (`soundfont.rs`) and
`corpus_sequences_are_music_in_the_tree_and_build_back` (`tree_check.rs`), run with
`DC2TK_CORPUS` set to the extracted disc and `cargo test --release -- --include-ignored corpus`
(they are ignored otherwise).

| Check | Result |
|---|---|
| `.sq` read -> write byte-exact | 447 / 447 (527,181 events) |
| export to MIDI -> build back byte-exact, programs named are in the bank | 447 / 447; the 4 sequences that name programs their banks lack ([programs](../../../docs/internals/music.md#programs)) |
| a bank's SoundFont: chunks add up, every zone's sample and loop in range | 175 banks with sequences (146,708,074 bytes) |
| asset tree: music ids at every place, tree and index agree, MIDI builds back | `BG_193` alone and in a pack, `SR_138`'s 8 |
| a new music bank made like each disc bank, of its own tune: one entry, the same sequence, no more samples | 73 / 73 |

A unit test builds a tune that names a program the bank lacks with a two-sample instrument (split
at middle C, one looped), a program replaced in place, and a bank that only fits once the old
samples are cut, or not at all. Another makes a new music bank of a two-tune bank (renamed
members, one entry, the old tune's sample cut), and one builds a mod's added music file into a
small disc image and reads it back, with the refusals. Unit tests also cover the compression rule,
a foreign MIDI file (format 1, 96 ticks a quarter note, bank select, an RPN, markers), refused
programs and channels, a tune looped whole, markers moved in an editor, and channel 0 freed or
refused; every disc sequence taken as a new tune comes out laid out as the disc's are.

Checked: `BG_193_001` (the fishing battle) added as `BG_200` like `BG_193` builds (64 KB of
samples cut, 0x67780 of 0x78000 bytes), and read back out of the image its MIDI file is the one
given but for the track's name. Not yet heard in the game.

## Open problems

The engine's own unknowns (the driver, the voices) are in
[music and ambient loops](../../../docs/internals/music.md#what-is-not-known).

1. **New instruments** come from WAVs
   ([new instruments](../../../docs/guide/music.md#new-instruments)), one sample a key range; velocity layers, an envelope other than a bank program's, and
   importing a SoundFont's instruments are not done. Whether the IOP's synthesizer reads the new
   programs as the disc's is from the format, not yet heard in the game (`text-music-test.iso`).
2. **The SoundFont is approximate** (above); nothing was compared with the game's output.
3. `sounds::free_unused` cuts a sample only when no entry plays it, a sequence's notes included
   (`Bank::voices` follows them); a `.sq` no `.cfg` row names is not seen (the disc's banks have
   447 `.sq` rows for their 447 members).
