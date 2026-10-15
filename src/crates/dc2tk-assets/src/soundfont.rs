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

use crate::soundbank::{Bank, Program, Sample, SampleSet, Split};
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
fn envelope(adsr1: u16, adsr2: u16) -> (i16, i16, i16, i16) {
    let (adsr1, adsr2) = (u32::from(adsr1), u32::from(adsr2));
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

/// The SoundFont generators the zones set (SoundFont 2.04, section 8.1.2).
mod generator {
    pub const PAN: u16 = 17;
    pub const ATTACK_VOL_ENV: u16 = 34;
    pub const DECAY_VOL_ENV: u16 = 36;
    pub const SUSTAIN_VOL_ENV: u16 = 37;
    pub const RELEASE_VOL_ENV: u16 = 38;
    pub const INSTRUMENT: u16 = 41;
    pub const KEY_RANGE: u16 = 43;
    pub const VEL_RANGE: u16 = 44;
    pub const INITIAL_ATTENUATION: u16 = 48;
    pub const COARSE_TUNE: u16 = 51;
    pub const FINE_TUNE: u16 = 52;
    pub const SAMPLE_ID: u16 = 53;
    pub const SAMPLE_MODES: u16 = 54;
    pub const OVERRIDING_ROOT_KEY: u16 = 58;
    /// The generator of a list's terminal record (all zeros).
    pub const END: u16 = 0;
}

/// A generator: its number and 16-bit amount.
fn generator(out: &mut Vec<u8>, number: u16, amount: i16) {
    out.extend(number.to_le_bytes());
    out.extend(amount.to_le_bytes());
}

fn range(low: i64, high: i64) -> i16 {
    i16::from_le_bytes([low.clamp(0, 127) as u8, high.clamp(0, 127) as u8])
}

/// The SoundFont's sample data and lists as they are built: an instrument per program, a zone per
/// sample a split plays, a sample header per VAG.
#[derive(Default)]
struct Font {
    /// Each VAG a zone plays, decoded once: its sample header's number and whether it loops.
    samples: BTreeMap<usize, (usize, bool)>,
    smpl: Vec<u8>,
    shdr: Vec<u8>,
    inst: Vec<u8>,
    ibag: Vec<u8>,
    igen: Vec<u8>,
    ibags: u16,
    igens: u16,
    /// (program number, instrument) of every program.
    presets: Vec<(u16, u16)>,
}

impl Font {
    /// The sample header of a VAG (decoded and added on its first use): its number and whether it
    /// loops.
    fn sample(&mut self, bank: &Bank, vag: usize, name: &str) -> Result<(usize, bool)> {
        if let Some(&found) = self.samples.get(&vag) {
            return Ok(found);
        }
        let sound = bank.decode_vag(vag)?;
        let found = (self.samples.len(), sound.loop_start.is_some());
        self.samples.insert(vag, found);
        let start = (self.smpl.len() / 2) as u32;
        let mut frames: Vec<i16> = sound.samples.iter().map(|&s| s.clamp(-32768, 32767) as i16).collect();
        // a SoundFont sample has at least 48 frames past its loop
        while frames.len() < 48 {
            frames.push(0);
        }
        for frame in &frames {
            self.smpl.extend(frame.to_le_bytes());
        }
        self.smpl.extend([0u8; 92]);
        let end = start + frames.len() as u32;
        let loop_start = start + sound.loop_start.map_or(0, |at| at.clamp(0, frames.len() as i64 - 8) as u32);
        self.shdr.extend(name_field(&format!("{name} vag {vag}")));
        for value in [start, end, loop_start, end] {
            self.shdr.extend(value.to_le_bytes());
        }
        self.shdr.extend((sound.rate as u32).to_le_bytes());
        self.shdr.extend([60, 0]);
        self.shdr.extend(0u16.to_le_bytes());
        self.shdr.extend(1u16.to_le_bytes());
        Ok(found)
    }

    /// The zone of one sample of a split's sample set: its keys and velocities, the program's,
    /// split's and sample's volume, pan and tuning added up, the envelope, and the sample.
    fn zone(&mut self, program: &Program, split: &Split, set: &SampleSet, sample: &Sample, sample_id: (usize, bool)) {
        let (sample_id, looped) = sample_id;
        let record = &program.record;
        let (attack, decay, sustain, release) = envelope(sample.adsr1, sample.adsr2);
        let volume = |value: u8| value.min(128) as f64 / 128.0;
        let gain = volume(record.volume) * volume(split.volume) * volume(sample.volume);
        let centred = |panpot: u8| i64::from(panpot) - 64;
        let pan = centred(record.panpot) + centred(split.panpot) + centred(sample.panpot);
        let coarse = i64::from(record.transpose) + i64::from(split.transpose);
        let fine = i64::from(record.detune) + i64::from(split.detune) + i64::from(sample.detune);
        self.ibag.extend(self.igens.to_le_bytes());
        self.ibag.extend(0u16.to_le_bytes());
        self.ibags += 1;
        let mut zone = Vec::new();
        generator(&mut zone, generator::KEY_RANGE, range(split.range_low.into(), split.range_high.into()));
        let velocity_low = sample.vel_range_low.max(set.vel_limit_low);
        let velocity_high = sample.vel_range_high.min(set.vel_limit_high);
        generator(&mut zone, generator::VEL_RANGE, range(velocity_low.into(), velocity_high.into()));
        generator(&mut zone, generator::PAN, ((pan * 500) / 64).clamp(-500, 500) as i16);
        generator(&mut zone, generator::ATTACK_VOL_ENV, attack);
        generator(&mut zone, generator::DECAY_VOL_ENV, decay);
        generator(&mut zone, generator::SUSTAIN_VOL_ENV, sustain);
        generator(&mut zone, generator::RELEASE_VOL_ENV, release);
        generator(&mut zone, generator::INITIAL_ATTENUATION, centibels(gain, 1.0));
        generator(&mut zone, generator::COARSE_TUNE, coarse.clamp(-120, 120) as i16);
        generator(&mut zone, generator::FINE_TUNE, (fine * 100 / 128).clamp(-99, 99) as i16);
        generator(&mut zone, generator::SAMPLE_MODES, i16::from(looped));
        generator(&mut zone, generator::OVERRIDING_ROOT_KEY, sample.base_note.min(127) as i16);
        generator(&mut zone, generator::SAMPLE_ID, sample_id as i16);
        self.igens += (zone.len() / 4) as u16;
        self.igen.extend(zone);
    }

    /// The instrument of program `number`: a zone for every sample its splits play.
    fn instrument(&mut self, bank: &Bank, number: usize, program: &Program, name: &str) -> Result<()> {
        let hd = &bank.hd;
        self.inst.extend(name_field(&format!("{name} {number}")));
        self.inst.extend(self.ibags.to_le_bytes());
        for split in &program.splits {
            let Some(Some(set)) = hd.sample_sets.get(split.sample_set as usize) else {
                continue;
            };
            for &index in &set.samples {
                let Some(Some(sample)) = hd.samples.get(index as usize) else {
                    continue;
                };
                let vag = sample.vag_index as usize;
                if vag >= hd.vags.len() {
                    continue;
                }
                let sample_id = self.sample(bank, vag, name)?;
                self.zone(program, split, set, sample, sample_id);
            }
        }
        let instrument = self.presets.len() as u16;
        self.presets.push((number as u16, instrument));
        Ok(())
    }

    /// The whole file: a preset of each program in banks 0 and 128, each list's terminal record,
    /// the RIFF chunks.
    fn finish(mut self, name: &str) -> Vec<u8> {
        let (mut phdr, mut pbag, mut pgen) = (Vec::new(), Vec::new(), Vec::new());
        let (mut pbags, mut pgens) = (0u16, 0u16);
        for drum_bank in [0u16, 128] {
            for &(number, instrument) in &self.presets {
                phdr.extend(name_field(&format!("{name} {number}")));
                phdr.extend(number.to_le_bytes());
                phdr.extend(drum_bank.to_le_bytes());
                phdr.extend(pbags.to_le_bytes());
                phdr.extend([0u8; 12]);
                pbag.extend(pgens.to_le_bytes());
                pbag.extend(0u16.to_le_bytes());
                pbags += 1;
                generator(&mut pgen, generator::INSTRUMENT, instrument as i16);
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
        generator(&mut pgen, generator::END, 0);
        self.inst.extend(name_field("EOI"));
        self.inst.extend(self.ibags.to_le_bytes());
        self.ibag.extend(self.igens.to_le_bytes());
        self.ibag.extend(0u16.to_le_bytes());
        generator(&mut self.igen, generator::END, 0);
        self.shdr.extend(name_field("EOS"));
        self.shdr.extend([0u8; 26]);
        let info = list(
            b"INFO",
            &[
                chunk(b"ifil", &[2, 0, 1, 0]),
                chunk(b"isng", b"EMU8000\0"),
                chunk(b"INAM", format!("{name}\0").as_bytes()),
                chunk(b"ISFT", b"dc2tk\0"),
            ],
        );
        let sdta = list(b"sdta", &[chunk(b"smpl", &self.smpl)]);
        let pdta = list(
            b"pdta",
            &[
                chunk(b"phdr", &phdr),
                chunk(b"pbag", &pbag),
                chunk(b"pmod", &[0; 10]),
                chunk(b"pgen", &pgen),
                chunk(b"inst", &self.inst),
                chunk(b"ibag", &self.ibag),
                chunk(b"imod", &[0; 10]),
                chunk(b"igen", &self.igen),
                chunk(b"shdr", &self.shdr),
            ],
        );
        let mut body = b"sfbk".to_vec();
        body.extend(info);
        body.extend(sdta);
        body.extend(pdta);
        chunk(b"RIFF", &body)
    }
}

/// The SoundFont of a bank's programs (see the module's doc).
pub fn write(bank: &Bank, name: &str) -> Result<Vec<u8>> {
    let mut font = Font::default();
    for (number, program) in bank.hd.programs.iter().enumerate() {
        if let Some(program) = program {
            font.instrument(bank, number, program, name)?;
        }
    }
    Ok(font.finish(name))
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
            if crate::soundbank::find(&bank.pack, "sq").is_empty() {
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
