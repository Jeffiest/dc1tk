//! Carried over from the Python tool's test_asm.py

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
    assert_eq!(asm::float_text(bits(1e20)).as_deref(), Some("1e+20"));
    assert_eq!(asm::float_text(0x7FC00000), None);
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
