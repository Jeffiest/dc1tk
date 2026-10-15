//! Prints how every one- and two-byte code decodes, strictly and with replacement, for comparing
//! with Python's cp932 codec: "hex<TAB>strict code points or ERR<TAB>lossy code points". With
//! "encode", every character below U+10000 as "code point<TAB>hex bytes or ERR".

use dc2tk_disc::text;

fn points(text: &str) -> String {
    text.chars().map(|c| format!("{:04X}", c as u32)).collect::<Vec<_>>().join(" ")
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("encode") {
        for point in 0u32..0x10000 {
            if let Some(c) = char::from_u32(point) {
                let bytes = text::encode(&c.to_string());
                let hex = bytes
                    .map(|b| b.iter().map(|b| format!("{b:02X}")).collect::<String>())
                    .unwrap_or_else(|| "ERR".into());
                println!("{point:04X}	{hex}");
            }
        }
        return;
    }
    let mut codes: Vec<Vec<u8>> = (0u8..=255).map(|b| vec![b]).collect();
    for lead in 0x80u8..=0xFF {
        for trail in 0x00u8..=0xFF {
            codes.push(vec![lead, trail]);
        }
    }
    for code in codes {
        let hex: String = code.iter().map(|b| format!("{b:02X}")).collect();
        let strict = text::decode_strict(&code).map(|t| points(&t)).unwrap_or_else(|| "ERR".into());
        println!("{hex}\t{strict}\t{}", points(&text::decode_lossy(&code)));
    }
}
