//! Prints how every one- and two-byte code decodes, strictly and with replacement, for comparing
//! with Python's cp932 codec: "hex<TAB>strict code points or ERR<TAB>lossy code points". With
//! "encode", every character below U+10000 as "code point<TAB>hex bytes or ERR"; with "whatwg", the
//! same from WHATWG Shift_JIS alone (encoding_rs, without the cp932 fixes), which
//! examples/cp932_fixes.py turns into src/cp932_fixes.rs.

use dc2tk_disc::text;

fn points(text: &str) -> String {
    text.chars().map(|c| format!("{:04X}", c as u32)).collect::<Vec<_>>().join(" ")
}

/// The bytes in hex, or ERR for none.
fn hex(bytes: Option<&[u8]>) -> String {
    bytes.map(|b| b.iter().map(|b| format!("{b:02X}")).collect::<String>()).unwrap_or_else(|| "ERR".into())
}

/// Every character below U+10000 with its bytes from encode.
fn encodings(encode: impl Fn(&str) -> Option<Vec<u8>>) {
    for point in 0u32..0x10000 {
        if let Some(c) = char::from_u32(point) {
            println!("{point:04X}\t{}", hex(encode(&c.to_string()).as_deref()));
        }
    }
}

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("encode") => return encodings(text::encode),
        Some("whatwg") => {
            return encodings(|text| {
                let (bytes, _, unmappable) = encoding_rs::SHIFT_JIS.encode(text);
                (!unmappable).then(|| bytes.into_owned())
            })
        }
        _ => {}
    }
    let mut codes: Vec<Vec<u8>> = (0u8..=255).map(|b| vec![b]).collect();
    for lead in 0x80u8..=0xFF {
        for trail in 0x00u8..=0xFF {
            codes.push(vec![lead, trail]);
        }
    }
    for code in codes {
        let strict = text::decode_strict(&code).map(|t| points(&t)).unwrap_or_else(|| "ERR".into());
        println!("{}\t{strict}\t{}", hex(Some(&code)), points(&text::decode_lossy(&code)));
    }
}
