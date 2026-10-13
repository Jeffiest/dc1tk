//! What each instruction does to the VM stack, to find the instruction that pushed a native call's
//! number.

use std::collections::HashMap;

use crate::stb::{Instruction, Script};

pub const CONST_INT: i32 = 1;
const INDEXED_MODES: [i32; 4] = [0x2, 0x4, 0x10, 0x20];

fn fixed_effect(name: &str) -> i32 {
    match name {
        "LOAD" | "LOAD_ADDR" | "PUSH_CONST" => 1,
        "POP" | "STORE" | "ADD" | "SUB" | "MUL" | "DIV" | "MOD" | "CMP" | "RET" | "JMP_FALSE" | "JMP_TRUE" | "AND"
        | "OR" => -1,
        _ => 0,
    }
}

/// Each function's argument count, by its offset from the code section.
pub fn function_args(script: &Script) -> HashMap<i64, i32> {
    let code = script.header.as_ref().map_or(0, |header| header.code) as i64;
    script.functions().map(|function| (function.offset as i64 - code, function.arg)).collect()
}

pub fn effect(item: &Instruction, args: &HashMap<i64, i32>) -> i32 {
    let name = item.name();
    match name {
        "CALL" => 1 - args.get(&(item.arg2 as i64)).copied().unwrap_or(0),
        "LOAD" | "LOAD_ADDR" if INDEXED_MODES.contains(&item.arg2) => 0,
        "PRINT" | "EXT" => -item.arg1,
        _ => fixed_effect(name),
    }
}

/// The instruction that pushed the first of a native call's arguments (its number), counting back
/// from the EXT at index.
pub fn native_push(instructions: &[Instruction], index: usize, args: &HashMap<i64, i32>) -> Option<usize> {
    let need = instructions[index].arg1;
    let mut total = 0;
    for position in (0..index).rev() {
        total += effect(&instructions[position], args);
        if total == need {
            return Some(position);
        }
        if total > need {
            return None;
        }
    }
    None
}

pub fn is_int(item: &Instruction) -> bool {
    item.name() == "PUSH_CONST" && item.arg1 == CONST_INT
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeCall {
    /// The function's offset in the file.
    pub function: usize,
    pub index: usize,
    pub args: i32,
    pub no: Option<i32>,
}

pub fn native_calls(script: &Script) -> Vec<NativeCall> {
    let args = function_args(script);
    let mut calls = Vec::new();
    for function in script.functions() {
        let instructions = &function.instructions;
        for (index, item) in instructions.iter().enumerate() {
            if item.name() != "EXT" {
                continue;
            }
            let mut number = None;
            if let Some(push) = native_push(instructions, index, &args) {
                if is_int(&instructions[push]) {
                    let mut no = instructions[push].arg2;
                    if is_int(&instructions[push + 1]) && instructions[push + 2].name() == "ADD" {
                        no = no.wrapping_add(instructions[push + 1].arg2);
                    }
                    number = Some(no);
                }
            }
            calls.push(NativeCall { function: function.offset, index, args: item.arg1 - 1, no: number });
        }
    }
    calls
}
