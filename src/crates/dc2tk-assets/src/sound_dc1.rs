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

/// The pack's banks and sequences as WAV, SoundFont and MIDI files, by path in the folder.
pub fn export(data: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
    let mut out = Vec::new();
    for pair in Bank::pairs(data)? {
        let bank = Bank::from_pair(data, &pair).context(&pair)?;
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
