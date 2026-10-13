//! Sound effects as WAV files for mods: a folder named after the bank plus .wav (sp_000.snd.wav/)
//! with one WAV per entry it changes, named by the entry's number (003.wav, or 003_door.wav).
//! Entries the folder leaves out stay as they are, and a WAV numbered on from the bank's last entry
//! adds one. A changed entry's samples are overwritten in place when no other entry plays them;
//! when others share them, the entry gets samples of its own.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::soundbank::{self, Bank, EntryKind, Mode, Sound};
use crate::{bail, Error, Result};

pub const SUFFIX: &str = ".wav";
pub const NEW_ENTRY_NAME: &str = "MOD_SE";

pub fn file_name(index: usize) -> String {
    format!("{index:03}{SUFFIX}")
}

/// (file name, WAV) for each entry that plays one sound, as the sample it plays. Entries that play
/// a sequence are left out.
pub fn export(data: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
    let bank = Bank::from_bytes(data)?;
    let mut out = Vec::new();
    for entry in bank.entries()? {
        let voices = bank.voices(&entry)?;
        if entry.kind == EntryKind::KeyOn && !voices.is_empty() {
            out.push((file_name(entry.index), soundbank::wav_bytes(&bank.decode_vag(voices[0].vag)?)?));
        }
    }
    Ok(out)
}

/// The number and name a WAV's file name gives (003.wav, 003_door.wav), as NAME matches it.
fn parse_name(name: &str) -> Option<(usize, Option<String>)> {
    let digits = name.chars().take_while(|c| c.is_ascii_digit()).count();
    let ends_right =
        name.len() >= digits + 4 && name.get(name.len() - 4..).is_some_and(|end| end.eq_ignore_ascii_case(SUFFIX));
    if digits == 0 || !ends_right {
        return None;
    }
    let middle = &name[digits..name.len() - 4];
    let label = if middle.is_empty() {
        None
    } else if let Some(label) = middle.strip_prefix('_') {
        Some(label.to_string())
    } else {
        return None;
    };
    Some((name[..digits].parse().ok()?, label))
}

/// The WAVs of a folder: (entry number, name, path), sorted by number.
pub fn numbered(folder: &Path) -> Result<Vec<(usize, Option<String>, PathBuf)>> {
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(folder)? {
        paths.push(entry?.path());
    }
    let mut found = Vec::new();
    for path in crate::sorted_paths(paths) {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let suffix = match name.rfind('.') {
            Some(dot) if dot > 0 && dot + 1 < name.len() => name[dot..].to_lowercase(),
            _ => String::new(),
        };
        if path.is_file() && suffix == SUFFIX {
            let Some((number, label)) = parse_name(&name) else {
                bail!("{name}: name a WAV by the entry number it replaces or adds (003.wav, 003_door.wav)");
            };
            found.push((number, label, path));
        }
    }
    let numbers: BTreeSet<usize> = found.iter().map(|(n, _, _)| *n).collect();
    if numbers.len() != found.len() {
        bail!("an entry is given by more than one WAV");
    }
    found.sort_by_key(|(n, _, _)| *n);
    Ok(found)
}

pub fn same_sound(a: &Sound, b: &Sound) -> bool {
    a.rate == b.rate && a.samples == b.samples && a.loop_start == b.loop_start
}

/// The bank with the entries a folder of WAVs gives in place of the original's, and the entries
/// it adds.
pub fn build(folder: &Path, original: &[u8]) -> Result<Vec<u8>> {
    let mut bank = Bank::from_bytes(original)?;
    let mut changed = false;
    for (number, name, path) in numbered(folder)? {
        let file = path.file_name().unwrap().to_string_lossy().to_string();
        let sound = soundbank::read_wav(&std::fs::read(&path)?).map_err(|error| match error {
            Error::Invalid(message) => Error::invalid(format!("{file}: {message}")),
            other => other,
        })?;
        let entries = bank.entries()?;
        if number >= entries.len() {
            if number != entries.len() {
                bail!(
                    "{}: the bank has entries 0 to {}, so a new one is {}",
                    file,
                    entries.len() as i64 - 1,
                    file_name(entries.len())
                );
            }
            let label = name.filter(|n| !n.is_empty()).unwrap_or_else(|| NEW_ENTRY_NAME.to_string());
            bank.add_entry(&sound, &label, "MOD", 100, 60)?;
            changed = true;
            continue;
        }
        let entry = &entries[number];
        let voices = if entry.kind == EntryKind::KeyOn { bank.voices(entry)? } else { Vec::new() };
        if !voices.is_empty() && same_sound(&bank.decode_vag(voices[0].vag)?, &sound) {
            continue;
        }
        // in place only for an entry that plays one sound no other entry shares; a sequence entry
        // becomes the one sound
        let vags: BTreeSet<usize> = voices.iter().map(|v| v.vag).collect();
        let mut shared = false;
        for &vag in &vags {
            if bank.users_of_vag(vag)?.iter().any(|&user| user != number) {
                shared = true;
                break;
            }
        }
        let old: BTreeSet<usize> = bank.voices(entry)?.iter().map(|v| v.vag).collect();
        let mode = if !voices.is_empty() && !shared { Mode::InPlace } else { Mode::Detach };
        bank.replace_entry(number, &sound, mode, None)?;
        free_unused(&mut bank, &old)?;
        changed = true;
    }
    if changed {
        bank.build()
    } else {
        Ok(original.to_vec())
    }
}

/// Cuts the samples an entry played before it got samples of its own down to a silent stub when no
/// entry plays them any more. Not in a bank with .sq sequences: which samples those play is not
/// decoded.
pub fn free_unused(bank: &mut Bank, vags: &BTreeSet<usize>) -> Result<()> {
    if !bank.pack.find("sq").is_empty() {
        return Ok(());
    }
    let stub = soundbank::encode_adpcm(&[0; 28], None, true)?;
    for &vag in vags {
        if bank.vag_data[vag].len() > stub.len() && bank.users_of_vag(vag)?.is_empty() {
            bank.vag_data[vag] = stub.clone();
            bank.hd.vags[vag].set("attribute", 0);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("c2-assets-sounds-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn wavs_are_named_by_entry_number_with_an_optional_name() {
        let folder = temp_folder("named");
        for name in ["003.wav", "012_door.WAV", "notes.txt"] {
            std::fs::write(folder.join(name), b"").unwrap();
        }
        let found: Vec<(usize, Option<String>)> =
            numbered(&folder).unwrap().into_iter().map(|(number, name, _)| (number, name)).collect();
        std::fs::remove_dir_all(folder).unwrap();
        assert_eq!(found, vec![(3, None), (12, Some("door".to_string()))]);
        assert_eq!(file_name(7), "007.wav");
    }

    #[test]
    fn a_wav_not_named_by_number_is_refused() {
        let folder = temp_folder("unnumbered");
        std::fs::write(folder.join("door.wav"), b"").unwrap();
        let error = numbered(&folder).unwrap_err().to_string();
        std::fs::remove_dir_all(folder).unwrap();
        assert!(error.contains("entry number"), "{error}");
    }

    #[test]
    fn one_entry_given_twice_is_refused() {
        let folder = temp_folder("twice");
        std::fs::write(folder.join("003.wav"), b"").unwrap();
        std::fs::write(folder.join("003_door.wav"), b"").unwrap();
        let error = numbered(&folder).unwrap_err().to_string();
        std::fs::remove_dir_all(folder).unwrap();
        assert!(error.contains("more than one"), "{error}");
    }

    #[test]
    fn wav_round_trip_keeps_rate_samples_and_loop() {
        let sound = Sound { rate: 22050, samples: [0, 1000, -1000, 32767, -32768].repeat(10), loop_start: Some(5) };
        let again = soundbank::read_wav(&soundbank::wav_bytes(&sound).unwrap()).unwrap();
        assert!(same_sound(&sound, &again));
    }

    #[test]
    fn adpcm_encodes_and_decodes_close_to_the_source() {
        let samples: Vec<i64> = (0..28 * 40).map(|i| (10000.0 * (i as f64 / 7.0).sin()) as i64).collect();
        let (decoded, loop_start) =
            soundbank::decode_adpcm(&soundbank::encode_adpcm(&samples, None, true).unwrap(), true).unwrap();
        assert_eq!(loop_start, None);
        assert!(soundbank::snr_db(&samples, &decoded[..samples.len()]) > 30.0);
    }
}
