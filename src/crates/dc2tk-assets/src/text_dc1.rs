//! Dark Cloud 1's message text: the codes of its .mes files as the text upstream Chronicle writes
//! them (port/src/gametext.cpp, docs/LOCALIZATION.md "Control codes"), so a dc1tk export reads as
//! upstream's `--export-text` does and its `lang/` JSON can be built back.
//!
//! A code is a signed 16-bit word: the gaiji.img letter grid from -0x2DF, the accented letters
//! DrawGaijiFont builds (kExtras), the space and newline, named controls (`{page}`, `{cyan}`,
//! `{circle}`), numbered families (`{wait 12}`, `{value3}`, `{insert2}`), and any other code as its
//! signed number (`{-123}`, `{57}`). A `{` the text means as a letter is written `{{`.

use crate::{Error, Result};

/// gaiji.img's letter grid in code order, from code GRID_FIRST (GaijiDataTbl).
const GRID: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz'=\"!?#&+-*/%()@|<>{}[]:,.$0123456789";
const GRID_FIRST: i16 = -0x2DF;

const NEWLINE: i16 = -0x100;
const SPACE: i16 = -0xFE;

/// The rest of PAL's characters (gametext.cpp kExtras).
const EXTRAS: [(i16, char); 49] = [
    (-0x287, 'œ'), (-0x286, '¡'), (-0x285, '¿'), (-0x284, 'Ä'), (-0x283, 'Ç'), (-0x282, 'È'),
    (-0x281, 'É'), (-0x280, 'Ö'), (-0x27F, 'Ü'), (-0x27E, 'ß'), (-0x27D, 'à'), (-0x27C, 'á'),
    (-0x27B, 'â'), (-0x27A, 'ä'), (-0x279, 'ç'), (-0x278, 'è'), (-0x277, 'é'), (-0x276, 'ê'),
    (-0x275, 'ë'), (-0x274, 'ì'), (-0x273, 'í'), (-0x272, 'î'), (-0x271, 'ï'), (-0x270, 'ñ'),
    (-0x26F, 'ò'), (-0x26E, 'ó'), (-0x26D, 'ô'), (-0x26C, 'ö'), (-0x26B, 'ù'), (-0x26A, 'ú'),
    (-0x269, 'û'), (-0x268, 'ü'), (-0x267, 'Ú'), (-0x266, 'Á'), (-0x265, 'Œ'), (-0x264, 'Ó'),
    (-0x260, 'À'), (-0x25F, 'Â'), (-0x25E, 'Ï'), (-0x25D, 'Í'), (-0x25C, 'Ì'), (-0x25B, 'Î'),
    (-0x25A, 'Ù'), (-0x259, 'Û'), (-0x258, 'Ë'), (-0x257, 'Ê'), (-0x256, 'Ò'), (-0x255, 'Ô'),
    (-0x254, 'Ñ'),
];

/// Controls with a name of their own (gametext.cpp kNamed).
const NAMED: [(i16, &str); 43] = [
    (-0xFD, "page"), (-0x400, "/color"), (-0x3FF, "white"), (-0x3FE, "yellow"), (-0x3FD, "cyan"),
    (-0x3FC, "green"), (-0x3FB, "dark"), (-0x3FA, "gold"), (-0x3F9, "grey"), (-0x301, "highlight"),
    (-0x401, "value"), (-0x300, "select"), (-0x2FF, "start"), (-0x2FE, "L1"), (-0x2FD, "R1"),
    (-0x2FC, "L2"), (-0x2FB, "R2"), (-0x2FA, "circle"), (-0x2F9, "triangle"), (-0x2F8, "cross"),
    (-0x2F7, "square"), (-0x2F6, "dpad"), (-0x2F5, "dpad-updown"), (-0x2F4, "dpad-sides"),
    (-0x2F3, "red-slash"), (-0x2F2, "heart"), (-0x2F1, "note"), (-0x2F0, "red-x"),
    (-0x2EF, "yellow-arrow"), (-0x2EE, "icon-sword"), (-0x2ED, "icon-orb"), (-0x2EC, "icon-georama"),
    (-0x2EB, "icon-jar"), (-0x2EA, "orange-arrow"), (-0x2E9, "bait"), (-0x2E8, "word-button"),
    (-0x2E7, "monster"), (-0x2E6, "alert"), (-0x2E5, "up"), (-0x2E4, "right"), (-0x2E3, "down"),
    (-0x2E2, "left"), (-0x2E1, "hand"),
];

/// Numbered families: name, first code, last code, the number of the first, spaced
/// (gametext.cpp kFamilies). "value" counts up as its codes run down.
const FAMILIES: [(&str, i32, i32, i32, bool); 9] = [
    ("wait", -0x200, -0x101, 0, true),
    ("color", -0x3FF, -0x302, 2, true),
    ("gap", -0x700, -0x601, 0, true),
    ("spacing", -0x800, -0x701, 0, true),
    ("justify", -0x900, -0x801, 0, true),
    ("bubble", -0xA00, -0x901, 0, true),
    ("icon", -0x300, -0x2E0, 0, true),
    ("name", -0x506, -0x501, 1, false),
    ("value", -0x40D, -0x406, 1, false),
];

/// "insert" runs over two stretches of codes.
const INSERT_FIRST: [i32; 2] = [-0x402, -0x40E];
const INSERT_COUNT: [i32; 2] = [4, 6];

fn char_of(code: i16) -> Option<char> {
    match code {
        SPACE => return Some(' '),
        NEWLINE => return Some('\n'),
        _ => {}
    }
    let at = code as i32 - GRID_FIRST as i32;
    if at >= 0 {
        if let Some(c) = GRID.chars().nth(at as usize) {
            return Some(c);
        }
    }
    EXTRAS.iter().find(|(known, _)| *known == code).map(|(_, c)| *c)
}

fn code_of(c: char) -> Option<i16> {
    match c {
        ' ' => return Some(SPACE),
        '\n' => return Some(NEWLINE),
        _ => {}
    }
    if let Some(at) = GRID.chars().position(|known| known == c) {
        return Some(GRID_FIRST + at as i16);
    }
    EXTRAS.iter().find(|(_, known)| *known == c).map(|(code, _)| *code)
}

fn control_name(code: i16) -> Option<String> {
    if let Some((_, name)) = NAMED.iter().find(|(known, _)| *known == code) {
        return Some(name.to_string());
    }
    let code = code as i32;
    for (name, first, last, origin, spaced) in FAMILIES {
        if (first..=last).contains(&code) {
            let number = if name == "value" { last - code + origin } else { code - first + origin };
            return Some(format!("{name}{}{number}", if spaced { " " } else { "" }));
        }
    }
    for stretch in 0..2 {
        let first = INSERT_FIRST[stretch];
        if code <= first && code > first - INSERT_COUNT[stretch] {
            return Some(format!("insert{}", if stretch == 0 { 0 } else { INSERT_COUNT[0] } + (first - code) + 1));
        }
    }
    None
}

fn number(text: &str) -> Option<i32> {
    (!text.is_empty() && text.len() <= 4 && text.bytes().all(|b| b.is_ascii_digit())).then(|| text.parse().ok())?
}

fn control_code(token: &str) -> Option<i16> {
    let name = token.to_ascii_lowercase();
    if let Some((code, _)) = NAMED.iter().find(|(_, known)| known.to_ascii_lowercase() == name) {
        return Some(*code);
    }
    for (prefix, first, last, origin, spaced) in FAMILIES {
        let Some(rest) = name.strip_prefix(prefix).filter(|rest| !rest.is_empty()) else {
            continue;
        };
        if spaced != rest.starts_with(' ') {
            continue;
        }
        let Some(n) = number(if spaced { &rest[1..] } else { rest }) else {
            continue;
        };
        let value = if prefix == "value" { last - (n - origin) } else { first + (n - origin) };
        if (first..=last).contains(&value) {
            return Some(value as i16);
        }
    }
    if let Some(n) = name.strip_prefix("insert").and_then(number) {
        if (1..=INSERT_COUNT[0] + INSERT_COUNT[1]).contains(&n) {
            let index = n - 1;
            let code = if index < INSERT_COUNT[0] {
                INSERT_FIRST[0] - index
            } else {
                INSERT_FIRST[1] - (index - INSERT_COUNT[0])
            };
            return Some(code as i16);
        }
    }
    let signed: i32 = token.parse().ok().filter(|_| token.len() <= 7)?;
    i16::try_from(signed).ok()
}

/// A message's codes (without its end code) as text.
pub fn decode(codes: &[u16]) -> String {
    let mut out = String::new();
    for &code in codes {
        let code = code as i16;
        match char_of(code) {
            Some('{') => out.push_str("{{"),
            Some(c) => out.push(c),
            None => out.push_str(&format!("{{{}}}", control_name(code).unwrap_or_else(|| code.to_string()))),
        }
    }
    out
}

/// Text back to codes (without an end code). Curly quotes and dashes become the plain ones, as the
/// port reads them; a character the font cannot draw is refused.
pub fn encode(text: &str) -> Result<Vec<u16>> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if c == '{' {
            if rest[1..].starts_with('{') {
                out.push(code_of('{').expect("the grid has '{'") as u16);
                rest = &rest[2..];
                continue;
            }
            if let Some(close) = rest.find('}').filter(|&close| close <= 24) {
                if let Some(code) = control_code(&rest[1..close]) {
                    out.push(code as u16);
                    rest = &rest[close + 1..];
                    continue;
                }
            }
        }
        let plain = match c {
            '\u{2018}' | '\u{2019}' => '\'',
            '\u{201C}' | '\u{201D}' => '"',
            '\u{2013}' | '\u{2014}' => '-',
            '\u{3000}' => ' ',
            other => other,
        };
        let Some(code) = code_of(plain) else {
            return Err(Error::invalid(format!("{c:?} is not a character Dark Cloud 1's font draws")));
        };
        out.push(code as u16);
        rest = &rest[c.len_utf8()..];
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_controls_and_numbers_round_trip() {
        let codes: Vec<u16> =
            [-0x2DF + 26, -0xFE, -0x2DF, -0xFD, -0x3FD, -0x400, -0x150, -0x403, -0x40F, -0x40D, 57, -0x27D, -0x2DF + 70]
                .iter()
                .map(|&c: &i32| c as i16 as u16)
                .collect();
        let text = decode(&codes);
        assert_eq!(text, "a A{page}{cyan}{/color}{wait 176}{insert2}{insert6}{value8}{57}à{{");
        assert_eq!(encode(&text).unwrap(), codes);
    }
}
