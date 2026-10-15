//! Sound memory (SPU2): the regions sound banks load into.

use super::*;

/// SPU2 regions a bank's samples (.bd) load into, each bank taking bd + 16 bytes (sndLoadSound's
/// LoadHdBd and LoadHdBdAdd place the next bank 16 bytes past the last's samples). The bases are
/// CSound::Init's (sound.cpp:271-283).
/// Music (BG_*), game ports 0 and 11, from 0x5210 up to the field banks at 0x7D210.
pub const SPU_MUSIC: u64 = 0x7_8000;
/// The field banks stacked up from 0x7D210 (base BS_*, battle FG_*, ambient SR_*, objects OB_*,
/// one event bank EV_*) and the monster banks (EN_*) stacked down from 0x18AE20 share this.
pub const SPU_FIELD: u64 = 0x10_DC10;
/// The system sounds SY_000 from 0x18AE20 up to the menu banks at 0x1A82E0.
pub const SPU_SYSTEM: u64 = 0x1_D4C0;
/// A menu bank (SP_*) from 0x1A82E0 up to core 1's reverb work area at 0x1D9020 (its start is
/// estimated from LIBSD's work-area size for mode 4, which every reverb setting on the disc uses).
pub const SPU_MENU: u64 = 0x3_0D40;
/// The player character's bank (CH_*) from 0x1E0000 up to core 0's reverb work area at 0x1F9020
/// (estimated as SPU_MENU's end).
pub const SPU_CHARACTER: u64 = 0x1_9020;
/// The IOP staging buffer every .bd is copied through (sound.cpp:122-159): a .bd up to 0x6DE00
/// goes in one piece and must fit its 0x6DD00 bytes, a larger one in two and may be 0xDBA00.
pub const IOP_STAGING: u64 = 0x6_DD00;
/// The fullest field set the scripts make on the disc: s01's set 1600 with the event bank
/// EV_100_155 (s01.c play_blackstone_railroad_arrival_scene) needs 0x10E180, 1,392 bytes over,
/// and overwrites the start of SY_000's samples; the next fullest (d03b02's 3691 with
/// EV_115_158) leaves 5,792 bytes.
pub const SPU_FIELD_RETAIL_FREE: i64 = 5_792;

/// The size of a bank's samples: its .bd member.
pub fn bank_samples(bank: &[u8]) -> Option<u64> {
    members(bank).into_iter().find(|(name, _)| name.to_lowercase().ends_with(".bd")).map(|(_, data)| data.len() as u64)
}

/// The banks of each sound set of snd2/MAP_index.txt (tab-separated: set, name, kind, text,
/// BG, BS, FG, SR, SR sequence, six OB columns, EV, ...), as CScene::LoadSound loads its field
/// layers: (set, [bank names]). A '*' (keep what is loaded) or '-' names nothing; a '*' among
/// the object columns ends them (LoadSndFileInfo).
pub(super) fn sound_sets(index: &[u8]) -> Vec<(i64, Vec<String>)> {
    let mut out = Vec::new();
    for line in text::decode_lossy(index).lines() {
        let fields: Vec<String> = line.split('\t').map(|field| field.replace(' ', "")).collect();
        let Ok(set) = fields.first().map_or("", |field| field.as_str()).parse::<i64>() else { continue };
        let mut banks = Vec::new();
        let named = |field: Option<&String>, prefix: &str| {
            field.filter(|field| field.starts_with(prefix) && field.len() >= 6).map(|field| field[..6].to_string())
        };
        banks.extend(named(fields.get(5), "BS"));
        banks.extend(named(fields.get(6), "FG"));
        banks.extend(named(fields.get(7), "SR"));
        for column in 9..15 {
            match fields.get(column) {
                Some(field) if field.starts_with('*') => break,
                field => {
                    if let Some(bank) = named(field, "OB") {
                        if !banks.contains(&bank) {
                            banks.push(bank);
                        }
                    }
                }
            }
        }
        out.push((set, banks));
    }
    out
}

pub(super) fn check_sound(original: &dyn Disc, modded: &dyn Disc, path: &str) -> Vec<Finding> {
    let mut out = check_read_whole(original, modded, path, "sound bank");
    let Some(new) = modded.read(path).as_deref().and_then(bank_samples) else { return out };
    let old = original.read(path).as_deref().and_then(bank_samples);
    let name = path.rsplit('/').next().unwrap_or(path).to_uppercase();
    let grew = old.is_none_or(|old| new > old);
    if (IOP_STAGING + 1..=IOP_STAGING + 0x100).contains(&new) || new > 2 * IOP_STAGING {
        out.push(finding(
            Level::Error,
            format!(
                "{path}: {} bytes of samples; the IOP copies a bank's samples through a {}-byte buffer, in one piece up to {} bytes and in two above, so it takes at most {} or else {} to {} bytes",
                n(new),
                n(IOP_STAGING),
                n(IOP_STAGING + 0x100),
                n(IOP_STAGING),
                n(IOP_STAGING + 0x101),
                n(2 * IOP_STAGING)
            ),
        ));
    }
    let region = match name.get(..2) {
        Some("BG") => Some((SPU_MUSIC, "music", Level::Error)),
        Some("SY") => Some((SPU_SYSTEM, "system sound", Level::Error)),
        Some("SP") => Some((SPU_MENU, "menu sound", Level::Warning)),
        Some("CH") => Some((SPU_CHARACTER, "player character's sound", Level::Warning)),
        _ => None,
    };
    if let Some((budget, what, level)) = region {
        if new + 16 > budget && grew {
            out.push(finding(
                level,
                format!(
                    "{path}: {} bytes of samples (+16), more than the {} of the {what} region of sound memory it loads into",
                    n(new),
                    n(budget)
                ),
            ));
        }
        if grew {
            out.push(finding(Level::Note, format!("{path}: {what} region {} of {}", n(new + 16), n(budget))));
        }
        return out;
    }
    if !["BS", "FG", "SR", "OB", "EV", "EN"].contains(&name.get(..2).unwrap_or("")) {
        return out;
    }
    // the sets that load this bank with its neighbours, each a stack of banks loaded together
    let stem = name.strip_suffix(".SND").unwrap_or(&name);
    let mut fullest: Option<(i64, u64)> = None;
    if let Some(index) = modded.read("snd2/MAP_index.txt") {
        let paths = modded.paths();
        let bank_size = |bank: &str| {
            let wanted = format!("{}.snd", bank.to_lowercase());
            paths
                .iter()
                .find(|path| {
                    path.to_lowercase().starts_with("snd2/") && path.to_lowercase().ends_with(&format!("/{wanted}"))
                })
                .and_then(|path| modded.read(path).as_deref().and_then(bank_samples))
        };
        let mut sizes: HashMap<String, u64> = HashMap::new();
        for (set, banks) in sound_sets(&index) {
            if !banks.iter().any(|bank| bank.eq_ignore_ascii_case(stem)) {
                continue;
            }
            let mut total = 0;
            for bank in &banks {
                let size = *sizes.entry(bank.clone()).or_insert_with(|| bank_size(bank).unwrap_or(0));
                total += size + 16;
            }
            if fullest.is_none_or(|(_, most)| total > most) {
                fullest = Some((set, total));
            }
        }
    }
    if let Some((set, total)) = fullest {
        if total > SPU_FIELD {
            out.push(finding(
                Level::Error,
                format!(
                    "{path}: sound set {set} loads it with banks that need {} bytes together, more than the {} of the field region of sound memory",
                    n(total),
                    n(SPU_FIELD)
                ),
            ));
        }
    }
    if let (Some(old), true) = (old, grew) {
        out.push(finding(
            Level::Warning,
            format!(
                "{path}: its samples grew by {} bytes; field, event and monster banks share {} bytes of sound memory{}, and the fullest set the disc's scripts make leaves {} (or none: s01's set 1600 with EV_100_155 is 1,392 over already), so growth overwrites the system sounds after it (estimated from the scripts' loads)",
                n(new - old),
                n(SPU_FIELD),
                fullest.map_or(String::new(), |(set, total)| format!(" (its fullest set, {set}, takes {} without event and monster banks)", n(total))),
                n(SPU_FIELD_RETAIL_FREE as u64)
            ),
        ));
    }
    out
}
