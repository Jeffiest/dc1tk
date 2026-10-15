//! The text assembly: files disassemble and assemble back to their own bytes, and the
//! assembler's forms.

mod common;

use common::{sample_script, tables, Random};
use dc2tk_script::asm::{self, Token};
use dc2tk_script::stb::{self, Region};

fn round_trip(data: &[u8]) -> (Vec<u8>, String) {
    let text = asm::disassemble(&stb::read(data), "event", tables());
    (asm::assemble(&text, tables()).unwrap(), text)
}

fn function_offset(data: &[u8], index: usize) -> usize {
    match &stb::read(data).regions[index] {
        Region::Function(function) => function.offset,
        other => panic!("not a function: {other:?}"),
    }
}

#[test]
fn sample_round_trips_symbolically() {
    let data = sample_script();
    let (rebuilt, text) = round_trip(&data);
    assert_eq!(rebuilt, data);
    assert!(text.contains(".header SB2 main f_0000 globals 1"));
    assert!(text.contains("    native _ASSIGN_STACK"));
    assert!(text.contains("    push \"hello\""));
    assert!(text.contains("    jf L_00b0"));
    assert!(text.contains("    call f_00d8"));
    assert!(text.contains("    addr local 0"));
    assert!(text.contains(".function f_00d8 local 1 arg 1 types 1"));
    assert!(!text.contains(".op"));
    assert!(!text.contains("func_end"));
}

#[test]
fn assembly_is_editable() {
    let text = round_trip(&sample_script()).1.replace("push 5", "push 6");
    let script = stb::read(&asm::assemble(&text, tables()).unwrap());
    let Region::Function(main) = &script.regions[2] else { panic!("no main") };
    assert_eq!(main.instructions[3].arg2, 6);
}

#[test]
fn unusual_operands_fall_back_to_raw_ops() {
    let mut data = sample_script();
    let wait = function_offset(&data, 2) + stb::FUNCDATA_SIZE + 9 * stb::VMCODE_SIZE;
    data[wait + 4..wait + 8].copy_from_slice(&3i32.to_le_bytes());
    let (rebuilt, text) = round_trip(&data);
    assert_eq!(rebuilt, data);
    assert!(text.contains("    .op 23 3 0"));
}

#[test]
fn header_variants_round_trip() {
    let mut data = sample_script();
    data[0..4].copy_from_slice(b"STB\x00");
    data[0x1C..0x40].fill(0xcc);
    data[0x14..0x18].copy_from_slice(&7i32.to_le_bytes());
    let (rebuilt, text) = round_trip(&data);
    assert_eq!(rebuilt, data);
    assert!(text.contains("unk_14 7"));
    assert!(text.contains(&format!("padding {}", "cc".repeat(0x24))));
}

#[test]
fn float_text_is_shortest_exact() {
    let bits = |value: f32| value.to_bits() as i32;
    assert_eq!(asm::float_text(bits(0.1)).as_deref(), Some("0.1"));
    assert_eq!(asm::float_text(bits(1.0)).as_deref(), Some("1.0"));
    assert_eq!(asm::float_text(bits(-0.0)).as_deref(), Some("-0.0"));
    assert_eq!(asm::float_text(bits(1e20)).as_deref(), Some("1e20"));
    assert_eq!(asm::float_text(bits(1.5e-7)).as_deref(), Some("1.5e-7"));
    assert_eq!(asm::float_text(bits(0.0001)).as_deref(), Some("0.0001"));
    assert_eq!(asm::float_text(0x7FC00000), None);
    assert_eq!(asm::float_text(0x7F800000), None);
}

/// Every float the decompiler writes is a C decimal floating constant that the assembler and the
/// compiler (which share float_bits) read back as the same bits.
#[test]
fn float_text_reads_back_as_the_same_bits_and_is_a_c_literal() {
    let literal = regex::Regex::new(r"^-?[0-9]+(\.[0-9]+)?(e-?[0-9]+)?$").unwrap();
    let mut patterns: Vec<u32> = (0..=u32::MAX).step_by(65_521).collect();
    patterns.extend([1, 0x007F_FFFF, 0x0080_0000, 0x7F7F_FFFF, 0x8000_0001, 0x3DCC_CCCD, 0x4B80_0001]);
    for bits in patterns {
        let Some(text) = asm::float_text(bits as i32) else {
            assert!(!f32::from_bits(bits).is_finite(), "{bits:08x} has no text");
            continue;
        };
        assert!(literal.is_match(&text) && (text.contains('.') || text.contains('e')), "{text}");
        assert_eq!(asm::float_bits(&text).unwrap() as u32, bits, "{text}");
    }
}

/// The C parser reads the exponent forms float_text writes.
#[test]
fn the_parser_reads_floats_with_an_exponent_and_no_point() {
    let program = dc2tk_script::parse::parse("void f() { x = 1e20; y = 1.5e-7; }").unwrap();
    let body = program["functions"][0].list("body");
    let bits: Vec<i64> = body.iter().map(|line| line["expr"]["value"]["bits"].as_i64().unwrap()).collect();
    assert_eq!(bits, [1e20f32.to_bits() as i64, 1.5e-7f32.to_bits() as i64]);
}

#[test]
fn quoted_strings_survive_tokenizing() {
    let text = "a\\b \"c\"\n\t\x01 立ち";
    assert_eq!(
        asm::tokenize(&format!("push {}", asm::quote(text))).unwrap(),
        [Token::Word("push".into()), Token::Text(text.into())]
    );
}

#[test]
fn arbitrary_bytes_round_trip() {
    let mut generator = Random::new(3);
    let mut samples = vec![b"".to_vec(), b"SB2".to_vec(), generator.bytes(500)];
    for _ in 0..20 {
        let mut data = sample_script();
        for _ in 0..4 {
            let at = generator.below(data.len());
            data[at] = generator.below(256) as u8;
        }
        samples.push(data);
    }
    for data in samples {
        assert_eq!(round_trip(&data).0, data);
    }
}
