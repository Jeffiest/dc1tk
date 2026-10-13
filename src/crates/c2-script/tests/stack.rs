//! Carried over from the Python tool's test_stack.py

mod common;

use c2_script::stack::{self, NativeCall};
use c2_script::stb::{self, Function, Instruction, Region, Script};
use common::sample_script;

fn item(name: &str, arg1: i32, arg2: i32) -> Instruction {
    Instruction::new(name, arg1, arg2)
}

/// A script of one function at offset 0, with the code section at 0.
fn script(instructions: Vec<Instruction>) -> Script {
    let function = Function {
        offset: 0,
        size: 0,
        addr: 0,
        name: 0,
        local: 0,
        arg: 0,
        arg_types: Vec::new(),
        instructions,
        strings: Vec::new(),
        padding: Vec::new(),
    };
    Script { size: 0, header: None, regions: vec![Region::Function(function)] }
}

#[test]
fn sample_native_call_skips_script_call_arguments() {
    let data = sample_script();
    let read = stb::read(&data);
    let Region::Function(main) = &read.regions[2] else { panic!("no main") };
    // stack.read_calls(data)
    assert_eq!(stack::native_calls(&read), [NativeCall { function: main.offset, index: 6, args: 2, no: Some(12) }]);
}

#[test]
fn indexed_address_keeps_depth() {
    let instructions =
        vec![item("PUSH_CONST", 1, 40), item("PUSH_CONST", 1, 0), item("LOAD_ADDR", 3, 0x2), item("EXT", 2, 0)];
    assert_eq!(stack::native_calls(&script(instructions))[0].no, Some(40));
}

#[test]
fn computed_native_number_is_unknown() {
    let instructions = vec![item("LOAD", 0, 1), item("PUSH_CONST", 1, 9), item("EXT", 2, 0)];
    assert_eq!(stack::native_calls(&script(instructions))[0].no, None);
}

#[test]
fn grouped_native_number_is_folded() {
    let instructions = vec![
        item("PUSH_CONST", 1, 70),
        item("PUSH_CONST", 1, 5),
        item("ADD", 0, 0),
        item("LOAD", 0, 1),
        item("EXT", 2, 0),
    ];
    assert_eq!(stack::native_calls(&script(instructions))[0].no, Some(75));
}
