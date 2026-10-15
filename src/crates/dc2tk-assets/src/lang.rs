//! The game's languages as the data tables keep them: by LanguageCode (0 Japanese to 6 Korean),
//! one file each, the English one labelling the records; and names in a language's own spelling,
//! as the cfg scripts hold them.

/// The language codes (text.rs' short names) by LanguageCode.
pub const LANGUAGE_NAMES: [&str; 7] = ["ja", "en", "fr", "de", "it", "es", "ko"];
/// LanguageCode 1: the files whose texts label a table's records.
pub const ENGLISH: usize = 1;

/// The first `count` languages with English first, then the rest in order: the order a record's
/// copies are kept in, so its first copy (the one shown) is the English one.
pub fn english_first(count: usize) -> Vec<usize> {
    std::iter::once(ENGLISH).chain((0..count).filter(|&language| language != ENGLISH)).collect()
}

/// An item name as a `comdatmes` string of a language: the engine's own characters (Shift-JIS),
/// and in French, German, Italian and Spanish the Latin-1 letters as the `[UNI00xx]` codes
/// ConvertFontCode turns into one byte. In English a letter Shift-JIS lacks that the build draws
/// into the English font for the menus' names (font::drawable: é, ŷ) is its plain spelling
/// (font::folded: e, y), as nothing turns codes into those glyphs there.
pub fn name_bytes(text: &str, language: usize) -> Option<Vec<u8>> {
    if language == ENGLISH {
        let mut out = Vec::new();
        for c in text.chars() {
            match dc2tk_disc::text::encode(c.encode_utf8(&mut [0; 4])) {
                Some(bytes) => out.extend(bytes),
                None if crate::font::drawable(c) => out.extend(crate::font::folded(c)?.bytes()),
                None => return None,
            }
        }
        return Some(out);
    }
    if !(2..=5).contains(&language) {
        return dc2tk_disc::text::encode(text);
    }
    let mut out = Vec::new();
    for c in text.chars() {
        match c as u32 {
            0x20..=0x7E => out.push(c as u8),
            code @ 0xA0..=0xFF => out.extend(format!("[UNI00{code:02x}]").bytes()),
            _ => return None,
        }
    }
    Some(out)
}

/// The bytes a name takes once ConvertFontCode has turned each 9-character code into one byte.
pub fn converted_len(bytes: &[u8], language: usize) -> usize {
    if !(2..=5).contains(&language) {
        return bytes.len();
    }
    let mut length = 0;
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'[' && bytes.get(at + 8) == Some(&b']') && !bytes[at + 1..at + 8].contains(&b']') {
            at += 9;
        } else {
            at += 1;
        }
        length += 1;
    }
    length
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_spelt_as_each_language_keeps_them() {
        assert_eq!(name_bytes("Clé", 2).unwrap(), b"Cl[UNI00e9]");
        assert_eq!(converted_len(b"Cl[UNI00e9]", 2), 3);
        assert_eq!(converted_len(b"Cl[UNI00e9]", 1), 11);
        assert_eq!(name_bytes("Clé", 1).unwrap(), b"Cle");
        assert_eq!(name_bytes("木", 2), None);
        assert_eq!(english_first(4), [1, 0, 2, 3]);
    }
}
