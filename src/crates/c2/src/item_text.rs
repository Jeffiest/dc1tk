//! The message files an item table's names and descriptions go into (docs/assets/data.md,
//! "Items"): each item's name is system message 10000 + its message number in every language's
//! `meswin/system_N.mes` (what item slots and the menus show), its description the allmenu.mes
//! message of that number in the language's menu packs.

use std::fs;

use anyhow::{anyhow, bail, Context, Result};

use c2_assets::item_data::{self, ItemText};
use c2_assets::text;
use c2_disc::rebuild;

use crate::map_cmd::Disc;

/// The system message file of a language (LoadSystemMes: system.mes for Japanese).
pub fn system_path(language: usize) -> String {
    if language == 0 {
        "meswin/system.mes".to_string()
    } else {
        format!("meswin/system_{language}.mes")
    }
}

/// The menu packs of a language's folder (`menu/N/*.pac`), by DATA.DAT path.
pub fn menu_packs(disc: &Disc, language: usize) -> Vec<String> {
    let folder = format!("menu/{language}/");
    let mut out: Vec<String> = match disc {
        Disc::Image(..) => disc
            .entries()
            .iter()
            .map(|entry| entry.path.clone())
            .filter(|path| {
                let lower = path.to_lowercase();
                lower.starts_with(&folder) && lower.ends_with(".pac") && !lower[folder.len()..].contains('/')
            })
            .collect(),
        Disc::Folder(root, _) => match fs::read_dir(root.join(&folder)) {
            Ok(entries) => entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().to_string())
                .filter(|name| name.to_lowercase().ends_with(".pac") && root.join(&folder).join(name).is_file())
                .map(|name| format!("{folder}{name}"))
                .collect(),
            Err(_) => Vec::new(),
        },
    };
    out.sort();
    out
}

/// The first item description, which marks the allmenu.mes files that have them (a few menus'
/// shorter versions do not, and are left as they are).
const FIRST_DESCRIPTION: u16 = (item_data::MESSAGE_BASE + 1) as u16;

/// The message files with the item table's names and descriptions in them: names into each
/// language's system file, descriptions into every allmenu.mes of the language's menu packs that
/// has the item descriptions. `current` gives a file as the build has it so far. Returns the disc
/// files that change.
pub fn item_messages(
    disc: &Disc,
    current: &dyn Fn(&str) -> Result<Vec<u8>>,
    texts: &[ItemText],
) -> Result<Vec<(String, Vec<u8>)>> {
    let fonts = text::Fonts { english_table: disc.read(text::ENGLISH_FONT).ok() };
    let mut out: Vec<(String, Vec<u8>)> = Vec::new();
    for language in 0..item_data::LANGUAGE_NAMES.len() {
        let font = fonts.font(text::LANGUAGES[language]);
        for file in ["system", "allmenu"] {
            let mine: Vec<&ItemText> =
                texts.iter().filter(|text| text.language == language && text.file == file).collect();
            if mine.is_empty() {
                continue;
            }
            // (top file, members) of each place
            let places: Vec<(String, Vec<String>)> = if file == "system" {
                vec![(system_path(language), Vec::new())]
            } else {
                menu_packs(disc, language).into_iter().map(|pack| (pack, vec!["allmenu.mes".to_string()])).collect()
            };
            for (top, members) in places {
                let holder = match out.iter().position(|(known, _)| *known == top) {
                    Some(at) => out.remove(at).1,
                    None => match current(&top) {
                        Ok(data) => data,
                        // a language without a system file (none on the PAL disc)
                        Err(_) => continue,
                    },
                };
                let Some(old) = rebuild::read_member(&holder, &members).map(<[u8]>::to_vec) else {
                    out.push((top, holder));
                    continue;
                };
                let mut mes = text::Mes::parse(&old).with_context(|| top.clone())?;
                if file == "allmenu" && mes.get(FIRST_DESCRIPTION).is_none() {
                    out.push((top, holder));
                    continue;
                }
                let mut changed = false;
                for item_text in &mine {
                    let codes = match &item_text.text {
                        Some(body) => text::encode(body, &font)
                            .map_err(|error| anyhow!("{} (mod {}): {error}", item_text.what, item_text.module))?,
                        None => match mes.get(item_text.copy) {
                            Some(codes) => codes.to_vec(),
                            None => continue,
                        },
                    };
                    if mes.get(item_text.id) == Some(&codes[..]) {
                        continue;
                    }
                    mes.set(item_text.id, codes);
                    changed = true;
                }
                if !changed {
                    out.push((top, holder));
                    continue;
                }
                let place = if members.is_empty() { top.clone() } else { format!("{top}.d/{}", members.join("/")) };
                let built = mes.write().with_context(|| place.clone())?;
                if let Some(limit) = text::buffer_size(&place) {
                    if built.len() > limit {
                        bail!("{place}: {} bytes with the item names; the game loads it into {limit}", built.len());
                    }
                }
                out.push((top, rebuild::replaced_member(&holder, &members, &built)?));
            }
        }
    }
    let mut changed = Vec::new();
    for (top, data) in out {
        if current(&top).ok().as_deref() != Some(&data[..]) {
            changed.push((top, data));
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_files_are_where_load_system_mes_reads() {
        assert_eq!(system_path(0), "meswin/system.mes");
        assert_eq!(system_path(3), "meswin/system_3.mes");
        assert_eq!(FIRST_DESCRIPTION, 10001);
    }
}
