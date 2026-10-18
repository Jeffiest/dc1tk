//! Dark Cloud 1's sound packs (.snd): a .txt, sometimes a .cfg, one or more banks as NAME.hd and
//! NAME.bd pairs (Sony's hardware-synth header and VAG bodies, as DC2's), and .sq sequences. The
//! export writes each bank's samples as WAV files (`<bank>/<bank>_NNN.wav`) with a SoundFont of its
//! programs (`<bank>.sf2`) and each sequence as MIDI (`<sq>.mid`); the build takes the folder back,
//! putting in only the WAVs and MIDIs that differ from what the export writes, so an untouched
//! folder gives the pack's own bytes.

use std::path::Path;

use crate::soundbank::{self, Bank};
use crate::{music, soundfont, Context, Error, Result};

fn stem(name: &str) -> &str {
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}

fn wav_name(bank: &str, index: usize) -> String {
    format!("{bank}/{bank}_{index:03}.wav")
}

/// A Dark Cloud 1 pack's samples numbered as one list, as the asset tree gives them entry numbers:
/// (bank stem, sample) for entry 0, 1, ... over its banks in pack order. None when the pack is not
/// one (DC2's single banks read as soundbank::Bank::from_bytes).
pub fn samples(data: &[u8]) -> Option<Vec<(String, usize)>> {
    if Bank::from_bytes(data).is_ok() {
        return None;
    }
    let pack = soundbank::read_pack(data).ok()?;
    if soundbank::find(&pack, "hd").is_empty() {
        return None;
    }
    // a bank that does not read (sound/set/snd31.snd's s30a_g, cut short) keeps its bytes and has
    // no entries; a pack of empty placeholders (sound/special/sp10.snd) has none at all
    let mut out = Vec::new();
    for pair in Bank::pairs(data).ok()? {
        if let Ok(bank) = Bank::from_pair(data, &pair) {
            out.extend((0..bank.vag_data.len()).map(|vag| (pair.clone(), vag)));
        }
    }
    Some(out)
}

/// The pack with numbered WAVs (entry numbers as samples gives them) in place of its samples;
/// a WAV that is the export's own leaves its sample as it is.
pub fn build_numbered(given: &[(usize, std::path::PathBuf)], original: &[u8]) -> Result<Vec<u8>> {
    let Some(list) = samples(original) else {
        return Err(Error::invalid("not a Dark Cloud 1 sound pack"));
    };
    let mut data = original.to_vec();
    let pairs: Vec<String> = Bank::pairs(original)?;
    for pair in pairs {
        if !list.iter().any(|(stem, _)| *stem == pair) {
            continue;
        }
        let mut bank = Bank::from_pair(&data, &pair).context(&pair)?;
        let mut changed = false;
        for (number, path) in given {
            let Some((stem, vag)) = list.get(*number) else {
                return Err(Error::invalid(format!("entry {number}: the pack has {} samples", list.len())));
            };
            if *stem != pair {
                continue;
            }
            let wav = std::fs::read(path).map_err(|e| Error::invalid(format!("cannot read {}: {e}", path.display())))?;
            if wav == soundbank::wav_bytes(&bank.decode_vag(*vag)?)? {
                continue;
            }
            let sound = soundbank::read_wav(&wav).context(path.display())?;
            bank.replace_vag(*vag, &sound, None, None)?;
            changed = true;
        }
        if changed {
            data = bank.build()?;
        }
    }
    Ok(data)
}

/// The pack's banks and sequences as WAV, SoundFont and MIDI files, by path in the folder.
pub fn export(data: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
    let mut out = Vec::new();
    for pair in Bank::pairs(data)? {
        // a bank that does not read (snd31.snd's s30a_g, cut short) is left as it is
        let Ok(bank) = Bank::from_pair(data, &pair) else {
            continue;
        };
        for index in 0..bank.vag_data.len() {
            out.push((wav_name(&pair, index), soundbank::wav_bytes(&bank.decode_vag(index)?)?));
        }
        if bank.hd.programs.iter().any(Option::is_some) {
            if let Ok(font) = soundfont::write(&bank, &pair) {
                out.push((format!("{pair}{}", soundfont::SUFFIX), font));
            }
        }
    }
    let pack = soundbank::read_pack(data)?;
    for index in soundbank::find(&pack, "sq") {
        let member = &pack.members[index];
        if member.data.is_empty() {
            continue;
        }
        let name = stem(&member.name);
        out.push((format!("{name}{}", music::SUFFIX), music::export(&member.data, name).context(&member.name)?));
    }
    Ok(out)
}

/// The pack with the WAVs and MIDIs of a folder in place of the samples and sequences that differ
/// from the export; files the folder lacks keep the pack's own.
pub fn build(folder: &Path, original: &[u8]) -> Result<Vec<u8>> {
    let mut data = original.to_vec();
    for pair in Bank::pairs(original)? {
        if Bank::from_pair(original, &pair).is_err() {
            continue;
        }
        let mut bank = Bank::from_pair(&data, &pair).context(&pair)?;
        let mut changed = false;
        for index in 0..bank.vag_data.len() {
            let path = folder.join(wav_name(&pair, index));
            let Ok(wav) = std::fs::read(&path) else {
                continue;
            };
            if wav == soundbank::wav_bytes(&bank.decode_vag(index)?)? {
                continue;
            }
            let sound = soundbank::read_wav(&wav).context(&path.display().to_string())?;
            bank.replace_vag(index, &sound, None, None)?;
            changed = true;
        }
        if changed {
            data = bank.build()?;
        }
    }
    let mut pack = soundbank::read_pack(&data)?;
    let mut changed = false;
    let sequences: Vec<usize> =
        soundbank::find(&pack, "sq").into_iter().filter(|&i| !pack.members[i].data.is_empty()).collect();
    for index in sequences {
        let name = stem(&pack.members[index].name).to_string();
        let Ok(midi) = std::fs::read(folder.join(format!("{name}{}", music::SUFFIX))) else {
            continue;
        };
        let built = music::build(&midi, &pack.members[index].data, None).context(&name)?;
        if built != pack.members[index].data {
            pack.members[index].data = built;
            changed = true;
        }
    }
    if changed {
        data = pack.write().map_err(|e| Error::invalid(e.to_string()))?;
    }
    Ok(data)
}
