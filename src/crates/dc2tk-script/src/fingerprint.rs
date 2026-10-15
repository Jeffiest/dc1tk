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
use crate::stb::{self, Function, Script};

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
        match item.name() {
            "JMP" | "JMP_FALSE" | "JMP_TRUE" => {
                let _ = write!(code_text, "[\"jump\",{}],{}]", item.arg1 as i64 - start, item.arg2);
            }
            "CALL" => {
                let _ = write!(code_text, "{},[\"call\",", item.arg1);
                match known.get(&(item.arg2 as i64)) {
                    Some(found) => push_string(&mut code_text, found),
                    None => {
                        let _ = write!(code_text, "{}", item.arg2);
                    }
                }
                code_text.push_str("]]");
            }
            "PUSH_CONST" if item.arg1 == CONST_STR => {
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
