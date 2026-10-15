//! A function's fingerprint: a hash of its code that does not depend on where the function or its
//! jump targets are, so the definitions can name a function wherever a disc puts it. The loose
//! fingerprint also ignores the text of its strings, for a script translated into another language.
//!
//! The hash is of the payload as compact JSON (no spaces, text as it is), written here directly so
//! that fingerprints stored in definitions and mods keep matching:
//! {"version":1,"table":..,"arg":..,"local":..,"arg_types":[..],"unused":[..],"code":[[op,arg1,arg2],..]}
//! with jumps as ["jump", offset in the function], calls as ["call", callee's fingerprint or
//! offset] and pushed strings as ["string", text or offset].

use std::collections::BTreeMap;
use std::fmt::Write;

use crate::asm;
use crate::stb::{self, Function, Op, Script};

pub const VERSION: i64 = 1;
const CONST_STR: i32 = 3;

/// A JSON string as compact JSON writes it: quotes, backslashes and control characters escaped,
/// everything else as it is.
pub fn push_string(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// The payload's JSON text; loose leaves out unused strings and the text of pushed ones.
pub fn payload_json(function: &Function, code: i64, table: &str, known: &BTreeMap<i64, String>, loose: bool) -> String {
    let start = (function.offset + stb::FUNCDATA_SIZE) as i64 - code;
    let texts: Vec<(i64, &String)> =
        asm::string_offsets(function).into_iter().map(|offset| offset - code).zip(&function.strings).collect();
    let mut pushed = Vec::new();
    let mut out = String::with_capacity(64 + function.instructions.len() * 16);
    let _ = write!(out, "{{\"version\":{VERSION},\"table\":");
    push_string(&mut out, table);
    let _ = write!(out, ",\"arg\":{},\"local\":{},\"arg_types\":[", function.arg, function.local);
    for (at, value) in function.arg_types.iter().enumerate() {
        if at > 0 {
            out.push(',');
        }
        let _ = write!(out, "{value}");
    }
    let mut code_text = String::with_capacity(function.instructions.len() * 16);
    for (at, item) in function.instructions.iter().enumerate() {
        if at > 0 {
            code_text.push(',');
        }
        let _ = write!(code_text, "[{},", item.op);
        match item.opcode() {
            Some(Op::Jmp | Op::JmpFalse | Op::JmpTrue) => {
                let _ = write!(code_text, "[\"jump\",{}],{}]", item.arg1 as i64 - start, item.arg2);
            }
            Some(Op::Call) => {
                let _ = write!(code_text, "{},[\"call\",", item.arg1);
                match known.get(&(item.arg2 as i64)) {
                    Some(found) => push_string(&mut code_text, found),
                    None => {
                        let _ = write!(code_text, "{}", item.arg2);
                    }
                }
                code_text.push_str("]]");
            }
            Some(Op::PushConst) if item.arg1 == CONST_STR => {
                pushed.push(item.arg2 as i64);
                let _ = write!(code_text, "{},[\"string\",", item.arg1);
                match texts.iter().find(|(at, _)| *at == item.arg2 as i64) {
                    _ if loose => code_text.push_str("\"\""),
                    Some((_, text)) => push_string(&mut code_text, text),
                    None => {
                        let _ = write!(code_text, "{}", item.arg2);
                    }
                }
                code_text.push_str("]]");
            }
            _ => {
                let _ = write!(code_text, "{},{}]", item.arg1, item.arg2);
            }
        }
    }
    out.push_str("],\"unused\":[");
    if !loose {
        let mut first = true;
        for (at, text) in &texts {
            if !pushed.contains(at) {
                if !first {
                    out.push(',');
                }
                first = false;
                push_string(&mut out, text);
            }
        }
    }
    out.push_str("],\"code\":[");
    out.push_str(&code_text);
    out.push_str("]}");
    out
}

/// The first 16 hex digits of the SHA-1 of a payload's JSON.
pub fn digest(json: &str) -> String {
    let hex = sha1_smol::Sha1::from(json.as_bytes()).digest().to_string();
    hex[..16].to_string()
}

/// The exact and loose fingerprints of every function, by offset from the code section.
pub struct Fingerprints {
    pub exact: BTreeMap<i64, String>,
    pub loose: BTreeMap<i64, String>,
    /// The functions' offsets in file order.
    pub order: Vec<i64>,
}

pub fn both_fingerprints(script: &Script, table: &str) -> Fingerprints {
    let mut found = Fingerprints { exact: BTreeMap::new(), loose: BTreeMap::new(), order: Vec::new() };
    let Some(header) = &script.header else {
        return found;
    };
    let code = header.code as i64;
    for function in script.functions() {
        let offset = function.offset as i64 - code;
        let exact = digest(&payload_json(function, code, table, &found.exact, false));
        let loose = digest(&payload_json(function, code, table, &found.loose, true));
        found.exact.insert(offset, exact);
        found.loose.insert(offset, loose);
        found.order.push(offset);
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stb::{Instruction, FUNCDATA_SIZE, VMCODE_SIZE};

    /// A function at offset (from a code section at 0) that pushes text, jumps forward within
    /// itself and calls the function at callee.
    fn function(offset: usize, text: &str, callee: i32) -> Function {
        let start = offset + FUNCDATA_SIZE;
        let instructions = vec![
            Instruction::new(Op::PushConst, CONST_STR, (start + 4 * VMCODE_SIZE) as i32),
            Instruction::new(Op::JmpFalse, (start + 3 * VMCODE_SIZE) as i32, 0),
            Instruction::new(Op::Call, 0, callee),
            Instruction::new(Op::FuncEnd, 0, 0),
        ];
        Function {
            offset,
            size: 0,
            addr: start as i32,
            name: 0,
            local: 0,
            arg: 0,
            arg_types: Vec::new(),
            instructions,
            strings: vec![text.to_string()],
            padding: Vec::new(),
        }
    }

    fn prints(function: &Function, known: &BTreeMap<i64, String>) -> (String, String) {
        let print = |loose| digest(&payload_json(function, 0, "event", known, loose));
        (print(false), print(true))
    }

    #[test]
    fn a_function_has_one_fingerprint_wherever_it_sits() {
        let known = BTreeMap::new();
        let (here, there) =
            (prints(&function(0x100, "hello", 0x40), &known), prints(&function(0x900, "hello", 0x40), &known));
        assert_eq!(here, there);
        assert_eq!(here.0.len(), 16);
        assert!(here.0.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn the_loose_fingerprint_ignores_the_text_of_strings() {
        let known = BTreeMap::new();
        let (english, french) =
            (prints(&function(0x100, "hello", 0x40), &known), prints(&function(0x100, "bonjour", 0x40), &known));
        assert_ne!(english.0, french.0);
        assert_eq!(english.1, french.1);
    }

    #[test]
    fn a_call_is_to_its_callees_fingerprint_where_it_is_known() {
        // the same callee at two offsets: the callers agree once the callee is known
        let at = |offset: i64| BTreeMap::from([(offset, "c0ffee".to_string())]);
        let (first, second) =
            (prints(&function(0x100, "hi", 0x40), &at(0x40)), prints(&function(0x100, "hi", 0x80), &at(0x80)));
        assert_eq!(first, second);
        // unknown, the call is to an offset, which tells them apart
        let unknown = BTreeMap::new();
        assert_ne!(prints(&function(0x100, "hi", 0x40), &unknown), prints(&function(0x100, "hi", 0x80), &unknown));
    }

    #[test]
    fn strings_are_written_as_compact_json_writes_them() {
        for text in ["plain", "quote \" and \\ back", "tab\there\nnew\r\u{8}\u{c}\u{1}", "ジ ü 😀"] {
            let mut out = String::new();
            push_string(&mut out, text);
            assert_eq!(out, serde_json::to_string(text).unwrap(), "{text:?}");
        }
    }
}
