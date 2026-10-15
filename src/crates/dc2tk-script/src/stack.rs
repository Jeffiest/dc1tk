//! What each instruction does to the VM stack, to find the instruction that pushed a native call's
//! number.

use std::collections::HashMap;

use crate::stb::{Instruction, Mode, Op, Script};

/// Each function's argument count, by its offset from the code section.
pub fn function_args(script: &Script) -> HashMap<i64, i32> {
    let code = script.header.as_ref().map_or(0, |header| header.code) as i64;
    script.functions().map(|function| (function.offset as i64 - code, function.arg)).collect()
}

/// How many values an instruction leaves on the stack, less those it takes.
pub fn effect(item: &Instruction, args: &HashMap<i64, i32>) -> i32 {
    let Some(op) = item.opcode() else {
        return 0;
    };
    match op {
        Op::Call => 1 - args.get(&(item.arg2 as i64)).copied().unwrap_or(0),
        // an indexed load takes its index
        Op::Load | Op::LoadAddr if Mode::from_bit(item.arg2).is_some_and(Mode::is_indexed) => 0,
        Op::Load | Op::LoadAddr | Op::PushConst => 1,
        Op::Print | Op::Ext => -item.arg1,
        Op::Pop
        | Op::Store
        | Op::Add
        | Op::Sub
        | Op::Mul
        | Op::Div
        | Op::Mod
        | Op::Cmp
        | Op::Ret
        | Op::JmpFalse
        | Op::JmpTrue
        | Op::And
        | Op::Or => -1,
        _ => 0,
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
            if item.opcode() != Some(Op::Ext) {
                continue;
            }
            let mut number = None;
            if let Some(push) = native_push(instructions, index, &args) {
                if instructions[push].is_int() {
                    let mut no = instructions[push].arg2;
                    if instructions[push + 1].is_int() && instructions[push + 2].opcode() == Some(Op::Add) {
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
