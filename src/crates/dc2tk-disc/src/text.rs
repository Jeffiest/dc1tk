//! The game's names are cp932 (Microsoft's Shift-JIS). encoding_rs follows WHATWG Shift_JIS,
//! which agrees with cp932 on every two-byte code but not on four single bytes (private use
//! characters in cp932), on how much of a bad sequence one replacement character stands for, nor
//! on which bytes some characters encode to. These follow cp932 and the bytes the tools have always
//! written; the encoder's corrections are cp932_fixes, which examples/cp932_fixes.rs generates.

use encoding_rs::SHIFT_JIS;

use crate::cp932_fixes;

/// cp932's single bytes that WHATWG Shift_JIS has no character for, with the private use
/// characters cp932 gives them (both ways).
const PRIVATE_SINGLES: [(u8, char); 4] =
    [(0xA0, '\u{F8F0}'), (0xFD, '\u{F8F1}'), (0xFE, '\u{F8F2}'), (0xFF, '\u{F8F3}')];

fn single(byte: u8) -> Option<char> {
    PRIVATE_SINGLES.iter().find(|&&(single, _)| single == byte).map(|&(_, c)| c)
}

/// Decodes cp932, or None when the bytes are not valid cp932: runs
/// WHATWG decodes as cp932 does, and the single bytes it has no character for.
pub fn decode_strict(bytes: &[u8]) -> Option<String> {
    let mut out = String::new();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes[i];
        if is_lead(byte) {
            // a lead byte takes the next byte with it, whatever that is
            i += 2;
            continue;
        }
        if let Some(c) = single(byte) {
            out.push_str(&decode_run(&bytes[start..i])?);
            out.push(c);
            start = i + 1;
        }
        i += 1;
    }
    out.push_str(&decode_run(&bytes[start..bytes.len().min(i)])?);
    Some(out)
}

fn decode_run(bytes: &[u8]) -> Option<String> {
    SHIFT_JIS.decode_without_bom_handling_and_without_replacement(bytes).map(|text| text.into_owned())
}

fn is_lead(byte: u8) -> bool {
    matches!(byte, 0x81..=0x9F | 0xE0..=0xFC)
}

/// Decodes cp932 with U+FFFD for what is not valid: a lead byte that
/// does not make a character with the next byte becomes U+FFFD alone, and the next byte is decoded
/// on its own.
pub fn decode_lossy(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes[i];
        if is_lead(byte) {
            if let Some(pair) = bytes.get(i..i + 2).and_then(decode_strict) {
                out.push_str(&pair);
                i += 2;
                continue;
            }
        } else if let Some(one) = decode_strict(&[byte]) {
            out.push_str(&one);
            i += 1;
            continue;
        }
        out.push('\u{FFFD}');
        i += 1;
    }
    out
}

/// How cp932 encodes a character whose cp932 bytes differ from WHATWG's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fixed {
    One(u8),
    Two([u8; 2]),
    /// cp932 has no bytes for it (WHATWG has).
    None,
}

/// cp932's bytes for a character where they differ from WHATWG's: its one-byte private use
/// characters, and the table of the rest (cp932_fixes, sorted by code point).
fn fixed(c: char) -> Option<Fixed> {
    if let Some(&(byte, _)) = PRIVATE_SINGLES.iter().find(|&&(_, single)| single == c) {
        return Some(Fixed::One(byte));
    }
    let point = u16::try_from(c as u32).ok()?;
    let at = cp932_fixes::ENCODE_FIXES.binary_search_by_key(&point, |&(from, _)| from).ok()?;
    Some(match cp932_fixes::ENCODE_FIXES[at].1 {
        cp932_fixes::NO_BYTES => Fixed::None,
        byte @ 0..=0xFF => Fixed::One(byte as u8),
        pair => Fixed::Two(pair.to_be_bytes()),
    })
}

/// Encodes text as cp932, or None when it has a character cp932 lacks.
pub fn encode(text: &str) -> Option<Vec<u8>> {
    if text.is_ascii() {
        return Some(text.as_bytes().to_vec());
    }
    let mut out = Vec::new();
    let mut run = String::new();
    for c in text.chars() {
        let Some(fix) = fixed(c) else {
            run.push(c);
            continue;
        };
        out.extend(encode_run(&run)?);
        run.clear();
        match fix {
            Fixed::One(byte) => out.push(byte),
            Fixed::Two(pair) => out.extend(pair),
            Fixed::None => return None,
        }
    }
    out.extend(encode_run(&run)?);
    Some(out)
}

fn encode_run(text: &str) -> Option<Vec<u8>> {
    let (bytes, _, unmappable) = SHIFT_JIS.encode(text);
    if unmappable {
        None
    } else {
        Some(bytes.into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_and_kana_round_trip() {
        let bytes = b"snd2\\event\\EV_000_130.snd";
        assert_eq!(decode_strict(bytes).unwrap(), "snd2\\event\\EV_000_130.snd");
        let kana = [0x83, 0x82, 0x83, 0x6A, 0x83, 0x4A]; // モニカ
        assert_eq!(decode_strict(&kana).unwrap(), "モニカ");
        assert_eq!(encode("モニカ").unwrap(), kana);
    }

    #[test]
    fn single_bytes_follow_cp932() {
        assert_eq!(decode_strict(&[0x80]).unwrap(), "\u{80}");
        assert_eq!(decode_strict(&[0x41, 0xA0, 0xFF]).unwrap(), "A\u{F8F0}\u{F8F3}");
        assert_eq!(encode("A\u{F8F0}\u{F8F3}").unwrap(), vec![0x41, 0xA0, 0xFF]);
    }

    /// fixed() searches the table by code point, so it must be sorted, each character once.
    #[test]
    fn the_encode_fixes_are_sorted_and_unique() {
        let table = &cp932_fixes::ENCODE_FIXES;
        assert!(table.windows(2).all(|pair| pair[0].0 < pair[1].0));
        // the private use singles are handled apart, not in the table
        assert!(PRIVATE_SINGLES
            .iter()
            .all(|&(_, c)| table.binary_search_by_key(&(c as u16), |&(from, _)| from).is_err()));
    }

    #[test]
    fn cp932_bytes_where_whatwg_differs() {
        assert_eq!(encode("\u{A2}").unwrap(), [0x81, 0x91]); // ¢
        assert_eq!(encode("a\u{A5}"), None); // ¥: WHATWG gives 0x5C, cp932 has none
        assert_eq!(fixed('\u{F8F2}'), Some(Fixed::One(0xFE)));
        assert_eq!(fixed('A'), None);
    }

    #[test]
    fn invalid_bytes() {
        assert!(decode_strict(&[0x81]).is_none());
        assert_eq!(decode_lossy(&[0x41, 0x81]), "A\u{FFFD}");
        assert!(encode("\u{1F600}").is_none());
    }
}
