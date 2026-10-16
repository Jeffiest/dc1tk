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

/// The EE memory stacks a scene's sound layers keep their banks' tables in, in bytes (stSetBuffer
/// takes quadwords, scenesnd.cpp): base (BS_*, InitSeBas) and battle (FG_*, InitSeBattle) 0x200
/// quadwords each; ambient (SR_*, InitSeEnv), the object banks (OB_*, InitSeSrc, all of a set's
/// in one) and music (BG_*, its BGM_INFO's, InitBGM) 0x40.
pub fn ee_table_stack(bank: &str) -> Option<(u64, &'static str)> {
    match bank.get(..2)? {
        "BS" => Some((0x200 * QW, "base")),
        "FG" => Some((0x200 * QW, "battle")),
        "SR" => Some((0x40 * QW, "ambient")),
        "OB" => Some((0x40 * QW, "object")),
        "BG" => Some((0x40 * QW, "music")),
        _ => None,
    }
}

/// What a bank's tables take of its layer's EE stack, estimated from sndLoadSound's allocations
/// (snd_mngr.cpp; each mgCMemory::Alloc in quadwords, the tables with 2 more): the entry table
/// (12 bytes an entry, LoadSeInfoTxt), for music and ambient banks the .sq members' name list and
/// names, and the .mid sequences' table (16 bytes each), names and events (about 6 bytes each,
/// LoadSMF). None when the bank has no .cfg.
pub fn ee_tables(name: &str, bank: &[u8]) -> Option<u64> {
    let quads = |bytes: usize| bytes.div_ceil(16) as u64;
    let named = |name: &str| quads(text::encode(name).map_or(name.len(), |bytes| bytes.len()) + 1);
    let found = members(bank);
    let of = |ext: &str| -> Vec<&(String, &[u8])> {
        found.iter().filter(|(name, _)| name.to_lowercase().ends_with(&format!(".{ext}"))).collect()
    };
    let cfg = of("cfg").first()?.1;
    let mut quadwords = quads(dc2tk_assets::soundbank::TextFile::parse(cfg).end_index() * 12) + 2;
    let sq = of("sq");
    // the music, ambient and event ports load .sq sequences
    if !sq.is_empty() && matches!(name.get(..2), Some("BG" | "SR" | "EV")) {
        quadwords += quads(sq.len() * 4) + 2 + sq.iter().map(|(name, _)| named(name)).sum::<u64>();
    }
    let mid = of("mid");
    if !mid.is_empty() {
        quadwords += mid.len() as u64 + 2;
        for (name, data) in mid {
            quadwords += named(name) + quads(smf_events(data) * 6);
        }
    }
    Some(quadwords * QW)
}

/// The events LoadSMF keeps of a sequence: its first track's channel events and the end.
fn smf_events(data: &[u8]) -> usize {
    let Ok(smf) = dc2tk_assets::music::Smf::read(data) else { return 0 };
    let events = smf.tracks.first().map_or(0, |track| {
        track
            .events
            .iter()
            .filter(|(_, message)| !matches!(message, dc2tk_assets::sequence::Message::Meta { .. }))
            .count()
    });
    events + 1
}

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

/// A bank's path on the disc by its name (FG_000), in whichever folder of snd2/ it is.
fn bank_path<'a>(paths: &'a [String], bank: &str) -> Option<&'a String> {
    let wanted = format!("/{}.snd", bank.to_lowercase());
    paths.iter().find(|path| {
        let lower = path.to_lowercase();
        lower.starts_with("snd2/") && lower.ends_with(&wanted)
    })
}

/// A changed bank's tables against its layer's EE stack (ee_table_stack), when they grew: a note
/// of the use, and a warning past the stack (the use is estimated). A set's object banks share
/// one stack, so theirs count together.
fn check_ee_tables(original: &dyn Disc, modded: &dyn Disc, path: &str, stem: &str) -> Vec<Finding> {
    let Some((stack, layer)) = ee_table_stack(stem) else { return Vec::new() };
    let tables =
        |disc: &dyn Disc, path: &str, bank: &str| disc.read(path).as_deref().and_then(|data| ee_tables(bank, data));
    let Some(new) = tables(modded, path, stem) else { return Vec::new() };
    if tables(original, path, stem).is_some_and(|old| new <= old) {
        return Vec::new();
    }
    let (mut used, mut with) = (new, String::new());
    if let (true, Some(index)) = (layer == "object", modded.read("snd2/MAP_index.txt")) {
        let paths = modded.paths();
        for (set, banks) in sound_sets(&index) {
            if !banks.iter().any(|bank| bank.eq_ignore_ascii_case(stem)) {
                continue;
            }
            let objects = banks.iter().filter(|bank| bank.starts_with("OB"));
            let total: u64 =
                objects.filter_map(|bank| bank_path(&paths, bank).and_then(|path| tables(modded, path, bank))).sum();
            if total > used {
                (used, with) = (total, format!(" with sound set {set}'s other object banks"));
            }
        }
    }
    let mut out = vec![finding(
        Level::Note,
        format!(
            "{path}: its tables take about {} bytes{with} of the {layer} sounds' {}-byte EE stack",
            n(used),
            n(stack)
        ),
    )];
    if used > stack {
        out.push(finding(
            Level::Warning,
            format!(
                "{path}: its tables take about {} bytes{with}, more than the {} of the {layer} sounds' EE memory stack (CScene::InitSe*), so loading it overruns the stack (estimated from sndLoadSound's allocations)",
                n(used),
                n(stack)
            ),
        ));
    }
    out
}

pub(super) fn check_sound(original: &dyn Disc, modded: &dyn Disc, path: &str) -> Vec<Finding> {
    let mut out = check_read_whole(original, modded, path, "sound bank");
    let name = path.rsplit('/').next().unwrap_or(path).to_uppercase();
    let stem = name.strip_suffix(".SND").unwrap_or(&name);
    out.extend(check_ee_tables(original, modded, path, stem));
    let Some(new) = modded.read(path).as_deref().and_then(bank_samples) else { return out };
    let old = original.read(path).as_deref().and_then(bank_samples);
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
    let mut fullest: Option<(i64, u64)> = None;
    if let Some(index) = modded.read("snd2/MAP_index.txt") {
        let paths = modded.paths();
        let bank_size =
            |bank: &str| bank_path(&paths, bank).and_then(|path| modded.read(path).as_deref().and_then(bank_samples));
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
