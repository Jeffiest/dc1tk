//! A bank's programs as a SoundFont 2 (.sf2), to hear its sequences' MIDI files (music.rs) with
//! the game's own instruments in any SoundFont player: preset n of banks 0 and 128 (a General MIDI
//! player takes channel 10 from bank 128) is program n.
//!
//! An approximation for listening, never read back: every split of a program is a zone over its
//! key range, every sample of its sample set a zone over its velocity range, with the VAG decoded
//! at its own rate and loop, the sample's base note as the root key, transposes as coarse tune,
//! detunes as 1/128 semitone (Sony's unit, assumed), volumes as attenuation (128 = unity) and the
//! SPU2 envelope (ADSR1, ADSR2) turned into SoundFont times by running it at 48 kHz.

use std::collections::BTreeMap;

use crate::soundbank::Bank;
use crate::Result;

pub const SUFFIX: &str = ".sf2";
const RATE: f64 = 48_000.0;
/// The longest envelope phase a SoundFont holds (8000 timecents, about 100 s).
const LONGEST: f64 = 100.0;
const LEVEL_MAX: i32 = 0x7FFF;

/// Seconds the SPU2 envelope takes from `from` to `to` at a rate (0..127: shift = rate >> 2,
/// step from rate & 3), as the hardware steps it once a sample at 48 kHz; at most LONGEST.
fn envelope_seconds(rate: u32, decreasing: bool, exponential: bool, from: i32, to: i32) -> f64 {
    let shift = (rate >> 2) as i32;
    let base = (rate & 3) as i64;
    let mut step: i64 = if decreasing { -8 + base } else { 7 - base };
    let mut cycles: i64 = 1;
    if shift < 11 {
        step <<= 11 - shift;
    } else {
        cycles = 1 << (shift - 11).min(30);
    }
    let limit = (LONGEST * RATE) as i64;
    let (mut level, mut samples) = (from as i64, 0i64);
    while samples < limit && if decreasing { level > to as i64 } else { level < to as i64 } {
        let mut wait = cycles;
        let mut delta = step;
        if exponential && !decreasing && level > 0x6000 {
            wait *= 4;
        }
        if exponential && decreasing {
            delta = (step * level) >> 15;
        }
        samples += wait;
        level = (level + delta).clamp(0, LEVEL_MAX as i64);
    }
    samples.min(limit) as f64 / RATE
}

fn timecents(seconds: f64) -> i16 {
    if seconds <= 0.001 {
        return -12000;
    }
    (1200.0 * seconds.log2()).round().clamp(-12000.0, 8000.0) as i16
}

/// Centibels of attenuation for a level out of `full`.
fn centibels(level: f64, full: f64) -> i16 {
    if level <= 0.0 {
        return 1440;
    }
    (200.0 * (full / level).log10()).round().clamp(0.0, 1440.0) as i16
}

/// (attack, decay, sustain level in cB, release) of the SPU2's ADSR1 and ADSR2 registers.
fn envelope(adsr1: i64, adsr2: i64) -> (i16, i16, i16, i16) {
    let (adsr1, adsr2) = (adsr1 as u32, adsr2 as u32);
    let attack = envelope_seconds((adsr1 >> 8) & 0x7F, false, adsr1 & 0x8000 != 0, 0, LEVEL_MAX);
    let sustain_level = (((adsr1 & 0xF) + 1) * 0x800).min(LEVEL_MAX as u32) as i32;
    let decay = envelope_seconds(((adsr1 >> 4) & 0xF) * 4, true, true, LEVEL_MAX, sustain_level);
    let sustain_rate = (adsr2 >> 6) & 0x7F;
    let sustain_exponential = adsr2 & 0x8000 != 0;
    let release = envelope_seconds((adsr2 & 0x1F) * 4, true, adsr2 & 0x20 != 0, LEVEL_MAX, 0);
    if adsr2 & 0x4000 != 0 && sustain_rate < 0x7F {
        // a sustain that falls: the SoundFont decays on to silence
        let fall = envelope_seconds(sustain_rate, true, sustain_exponential, sustain_level, 0);
        return (timecents(attack), timecents((decay + fall).min(LONGEST)), 1440, timecents(release));
    }
    (timecents(attack), timecents(decay), centibels(sustain_level as f64, LEVEL_MAX as f64), timecents(release))
}

fn name_field(name: &str) -> [u8; 20] {
    let mut out = [0u8; 20];
    for (slot, byte) in out.iter_mut().zip(name.bytes().filter(u8::is_ascii).take(19)) {
        *slot = byte;
    }
    out
}

fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = id.to_vec();
    out.extend((body.len() as u32).to_le_bytes());
    out.extend(body);
    if body.len() % 2 == 1 {
        out.push(0);
    }
    out
}

fn list(kind: &[u8; 4], parts: &[Vec<u8>]) -> Vec<u8> {
    let mut body = kind.to_vec();
    for part in parts {
        body.extend(part);
    }
    chunk(b"LIST", &body)
}

/// A generator: its number and 16-bit amount.
fn generator(out: &mut Vec<u8>, number: u16, amount: i16) {
    out.extend(number.to_le_bytes());
    out.extend(amount.to_le_bytes());
}

fn range(low: i64, high: i64) -> i16 {
    i16::from_le_bytes([low.clamp(0, 127) as u8, high.clamp(0, 127) as u8])
}

/// The SoundFont of a bank's programs (see the module's doc).
pub fn write(bank: &Bank, name: &str) -> Result<Vec<u8>> {
    let hd = &bank.hd;
    // each VAG a program plays, decoded once: its sample header and whether it loops
    let mut samples: BTreeMap<usize, (usize, bool)> = BTreeMap::new();
    let (mut smpl, mut shdr) = (Vec::<u8>::new(), Vec::<u8>::new());
    let (mut inst, mut ibag, mut igen) = (Vec::new(), Vec::new(), Vec::new());
    let (mut phdr, mut pbag, mut pgen) = (Vec::new(), Vec::new(), Vec::new());
    let mut instruments = 0u16;
    let (mut ibags, mut igens, mut pbags, mut pgens) = (0u16, 0u16, 0u16, 0u16);
    let mut presets = Vec::new();
    for (number, program) in hd.programs.iter().enumerate() {
        let Some(program) = program else {
            continue;
        };
        inst.extend(name_field(&format!("{name} {number}")));
        inst.extend(ibags.to_le_bytes());
        for split in &program.splits {
            let Some(Some(set)) = hd.sample_sets.get(split.get("sample_set") as usize) else {
                continue;
            };
            for &index in &set.samples {
                let Some(Some(sample)) = hd.samples.get(index as usize) else {
                    continue;
                };
                let vag = sample.get("vag_index") as usize;
                if vag >= hd.vags.len() {
                    continue;
                }
                if !samples.contains_key(&vag) {
                    let sound = bank.decode_vag(vag)?;
                    samples.insert(vag, (samples.len(), sound.loop_start.is_some()));
                    let start = (smpl.len() / 2) as u32;
                    let mut frames: Vec<i16> = sound.samples.iter().map(|&s| s.clamp(-32768, 32767) as i16).collect();
                    // a SoundFont sample has at least 48 frames past its loop
                    while frames.len() < 48 {
                        frames.push(0);
                    }
                    for frame in &frames {
                        smpl.extend(frame.to_le_bytes());
                    }
                    smpl.extend([0u8; 92]);
                    let end = start + frames.len() as u32;
                    let loop_start =
                        start + sound.loop_start.map_or(0, |at| at.clamp(0, frames.len() as i64 - 8) as u32);
                    shdr.extend(name_field(&format!("{name} vag {vag}")));
                    for value in [start, end, loop_start, end] {
                        shdr.extend(value.to_le_bytes());
                    }
                    shdr.extend((sound.rate as u32).to_le_bytes());
                    shdr.extend([60, 0]);
                    shdr.extend(0u16.to_le_bytes());
                    shdr.extend(1u16.to_le_bytes());
                }
                let (sample_id, looped) = samples[&vag];
                let (attack, decay, sustain, release) = envelope(sample.get("adsr1"), sample.get("adsr2"));
                let volume = |value: i64| value.clamp(0, 128) as f64 / 128.0;
                let gain =
                    volume(program.record.get("volume")) * volume(split.get("volume")) * volume(sample.get("volume"));
                let pan =
                    (program.record.get("panpot") - 64) + (split.get("panpot") - 64) + (sample.get("panpot") - 64);
                let coarse = program.record.get("transpose") + split.get("transpose");
                let fine = program.record.get("detune") + split.get("detune") + sample.get("detune");
                ibag.extend(igens.to_le_bytes());
                ibag.extend(0u16.to_le_bytes());
                ibags += 1;
                let mut zone = Vec::new();
                generator(&mut zone, 43, range(split.get("range_low"), split.get("range_high")));
                let velocity_low = sample.get("vel_range_low").max(set.vel_limit_low);
                let velocity_high = sample.get("vel_range_high").min(set.vel_limit_high);
                generator(&mut zone, 44, range(velocity_low, velocity_high));
                generator(&mut zone, 17, ((pan * 500) / 64).clamp(-500, 500) as i16);
                generator(&mut zone, 34, attack);
                generator(&mut zone, 36, decay);
                generator(&mut zone, 37, sustain);
                generator(&mut zone, 38, release);
                generator(&mut zone, 48, centibels(gain, 1.0));
                generator(&mut zone, 51, coarse.clamp(-120, 120) as i16);
                generator(&mut zone, 52, (fine * 100 / 128).clamp(-99, 99) as i16);
                generator(&mut zone, 54, i16::from(looped));
                generator(&mut zone, 58, sample.get("base_note").clamp(0, 127) as i16);
                generator(&mut zone, 53, sample_id as i16);
                igens += (zone.len() / 4) as u16;
                igen.extend(zone);
            }
        }
        presets.push((number as u16, instruments));
        instruments += 1;
    }
    for drum_bank in [0u16, 128] {
        for &(number, instrument) in &presets {
            phdr.extend(name_field(&format!("{name} {number}")));
            phdr.extend(number.to_le_bytes());
            phdr.extend(drum_bank.to_le_bytes());
            phdr.extend(pbags.to_le_bytes());
            phdr.extend([0u8; 12]);
            pbag.extend(pgens.to_le_bytes());
            pbag.extend(0u16.to_le_bytes());
            pbags += 1;
            generator(&mut pgen, 41, instrument as i16);
            pgens += 1;
        }
    }
    // the terminal records
    phdr.extend(name_field("EOP"));
    phdr.extend([0u8; 4]);
    phdr.extend(pbags.to_le_bytes());
    phdr.extend([0u8; 12]);
    pbag.extend(pgens.to_le_bytes());
    pbag.extend(0u16.to_le_bytes());
    generator(&mut pgen, 0, 0);
    inst.extend(name_field("EOI"));
    inst.extend(ibags.to_le_bytes());
    ibag.extend(igens.to_le_bytes());
    ibag.extend(0u16.to_le_bytes());
    generator(&mut igen, 0, 0);
    shdr.extend(name_field("EOS"));
    shdr.extend([0u8; 26]);
    let info = list(
        b"INFO",
        &[
            chunk(b"ifil", &[2, 0, 1, 0]),
            chunk(b"isng", b"EMU8000\0"),
            chunk(b"INAM", format!("{name}\0").as_bytes()),
            chunk(b"ISFT", b"c2\0"),
        ],
    );
    let sdta = list(b"sdta", &[chunk(b"smpl", &smpl)]);
    let pdta = list(
        b"pdta",
        &[
            chunk(b"phdr", &phdr),
            chunk(b"pbag", &pbag),
            chunk(b"pmod", &[0; 10]),
            chunk(b"pgen", &pgen),
            chunk(b"inst", &inst),
            chunk(b"ibag", &ibag),
            chunk(b"imod", &[0; 10]),
            chunk(b"igen", &igen),
            chunk(b"shdr", &shdr),
        ],
    );
    let mut body = b"sfbk".to_vec();
    body.extend(info);
    body.extend(sdta);
    body.extend(pdta);
    Ok(chunk(b"RIFF", &body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_envelope_runs_as_the_spu_steps_it() {
        // the fastest attack is a few samples; rate 0x7F (shift 31) never gets there
        assert!(envelope_seconds(0, false, false, 0, LEVEL_MAX) < 0.001);
        assert_eq!(envelope_seconds(0x7F, false, false, 0, LEVEL_MAX), LONGEST);
        // a linear rise at shift 11, step 7: 0x7FFF / 7 samples
        let seconds = envelope_seconds(44, false, false, 0, LEVEL_MAX);
        assert!((seconds - (0x7FFF as f64 / 7.0).ceil() / RATE).abs() < 1e-9, "{seconds}");
        // the disc's commonest envelope (0x80FF, 0x5FEC): instant attack, full sustain held
        let (attack, _, sustain, release) = envelope(0x80FF, 0x5FEC);
        assert_eq!((attack, sustain), (-12000, 0));
        assert!(release > -12000);
        assert_eq!(timecents(1.0), 0);
        assert_eq!(centibels(0.5, 1.0), 60);
    }

    /// Every bank with sequences on an extracted disc (C2S_CORPUS) gives a SoundFont whose chunks
    /// add up.
    #[test]
    fn corpus_banks_give_soundfonts() {
        let Some(corpus) = std::env::var_os("C2S_CORPUS") else {
            return;
        };
        let folder = std::path::Path::new(&corpus).join("snd2");
        let (mut count, mut bytes) = (0, 0);
        for path in crate::files_under(&folder).unwrap() {
            if !path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("snd")) {
                continue;
            }
            let bank = Bank::from_bytes(&std::fs::read(&path).unwrap()).unwrap();
            if bank.pack.find("sq").is_empty() {
                continue;
            }
            let font = write(&bank, "BG").unwrap();
            assert_eq!(&font[..4], b"RIFF");
            assert_eq!(u32::from_le_bytes(font[4..8].try_into().unwrap()) as usize + 8, font.len());
            count += 1;
            bytes += font.len();
        }
        assert!(count > 0);
        eprintln!("{count} banks with sequences give SoundFonts of {bytes} bytes");
    }
}
