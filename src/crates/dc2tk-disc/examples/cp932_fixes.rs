//! Writes src/cp932_fixes.rs: the characters whose cp932 bytes differ from what WHATWG Shift_JIS
//! (encoding_rs) gives them, which text::encode corrects. From the crate's folder:
//!
//!     cargo run -q --example cp932_fixes > src/cp932_fixes.rs
//!
//! The table has three parts:
//! - seven characters cp932 maps itself (OWN below): WHATWG encodes them as other bytes, or has
//!   bytes for them where cp932 has none;
//! - cp932's user-defined area (lead bytes 0xF0 to 0xF9), which is U+E000 to U+E757 and which
//!   WHATWG decodes but does not encode;
//! - the NEC-selected IBM extension rows (lead bytes 0xED and 0xEE): their characters are also in
//!   the IBM extension rows (0xFA to 0xFC), which WHATWG encodes them as. The tools have always
//!   written the NEC-selected codes.
//!
//! The four private use characters cp932 gives single bytes (U+F8F0 to U+F8F3) are left out:
//! text.rs handles them with the single bytes it decodes.

use encoding_rs::SHIFT_JIS;

/// Written for a character cp932 has no bytes for (WHATWG has).
const NO_BYTES: u16 = 0xFFFF;

/// cp932's own bytes for characters WHATWG encodes otherwise: ¢ £ ¬ ‖ and 〜 as their fullwidth
/// forms' codes, and none for ¥ and ‾ (which WHATWG writes as the ASCII backslash and tilde).
const OWN: [(u16, u16); 7] = [
    (0x00A2, 0x8191),
    (0x00A3, 0x8192),
    (0x00A5, NO_BYTES),
    (0x00AC, 0x81CA),
    (0x2016, 0x8161),
    (0x203E, NO_BYTES),
    (0x301C, 0x8160),
];

/// The second bytes of a two-byte code.
fn trails() -> impl Iterator<Item = u8> {
    (0x40..=0x7E).chain(0x80..=0xFC)
}

/// WHATWG's bytes for a character, as the table writes them (big-endian, one byte below 0x100).
fn whatwg(c: char) -> u16 {
    let mut utf8 = [0; 4];
    let (bytes, _, unmappable) = SHIFT_JIS.encode(c.encode_utf8(&mut utf8));
    match &bytes[..] {
        _ if unmappable => NO_BYTES,
        [one] => *one as u16,
        [lead, trail] => u16::from_be_bytes([*lead, *trail]),
        _ => NO_BYTES,
    }
}

fn fixes() -> Vec<(u16, u16)> {
    let mut found: Vec<(u16, u16)> = OWN.to_vec();
    let mut add = |c: char, code: u16| {
        let point = c as u32 as u16;
        if whatwg(c) != code && !found.iter().any(|&(taken, _)| taken == point) {
            found.push((point, code));
        }
    };
    // the user-defined area, in code order
    let mut point = 0xE000;
    for lead in 0xF0..=0xF9u8 {
        for trail in trails() {
            add(char::from_u32(point).unwrap(), u16::from_be_bytes([lead, trail]));
            point += 1;
        }
    }
    // the NEC-selected IBM extensions, the first code of a character where it has two
    for lead in [0xEDu8, 0xEE] {
        for trail in trails() {
            let code = [lead, trail];
            let (text, malformed) = SHIFT_JIS.decode_without_bom_handling(&code);
            let mut chars = text.chars();
            // only over an IBM extension code: ￢, which these rows also have, stays 0x81CA
            if let (false, Some(c), None) = (malformed, chars.next(), chars.next()) {
                if matches!(whatwg(c) >> 8, 0xFA..=0xFC) {
                    add(c, u16::from_be_bytes(code));
                }
            }
        }
    }
    found.sort();
    found
}

fn main() {
    let found = fixes();
    println!("// The characters whose cp932 bytes differ from what WHATWG Shift_JIS (encoding_rs) gives, with");
    println!("// cp932's bytes as a big-endian u16 (one byte below 0x100) or NO_BYTES where cp932 has none.");
    println!("// Sorted by code point. Written by examples/cp932_fixes.rs, which says where each comes from.");
    println!();
    println!("pub(crate) const NO_BYTES: u16 = 0x{NO_BYTES:X};");
    println!();
    println!("pub(crate) const ENCODE_FIXES: [(u16, u16); {}] = [", found.len());
    for (point, code) in found {
        println!("    (0x{point:04X}, 0x{code:04X}),");
    }
    println!("];");
}
