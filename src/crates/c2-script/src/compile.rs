//! Compiling a resolved program tree to the text assembly, the way the original compiler laid out
//! its code (labels as NOP instructions, the string buffer's padding), so a decompiled script
//! compiles back to its own bytes.

use std::collections::HashMap;

use indexmap::IndexMap;

use crate::asm;
use crate::natives::Table;
use crate::node;
use crate::node::Node;
use crate::source::py_str;
use crate::stb;
use crate::{Error, Result};

fn fail<T>(reason: impl Into<String>) -> Result<T> {
    Err(Error::invalid(reason))
}

fn scalar_modes(kind: &str) -> &'static str {
    if kind == "float" {
        "local_f"
    } else {
        "local"
    }
}

fn array_modes(kind: &str) -> &'static str {
    if kind == "float" {
        "local_idx_f"
    } else {
        "local_idx"
    }
}

fn pointer_modes(kind: &str) -> &'static str {
    if kind == "float" {
        "ptr_idx_f"
    } else {
        "ptr_idx"
    }
}

fn global_modes(kind: &str) -> &'static str {
    if kind == "float" {
        "global_f"
    } else {
        "global"
    }
}

fn binary_mnemonic(op: &str) -> Option<&'static str> {
    Some(match op {
        "+" => "add",
        "-" => "sub",
        "*" => "mul",
        "/" => "div",
        "%" => "mod",
        "&" => "and",
        "|" => "or",
        _ => return None,
    })
}

fn compare_mnemonic(op: &str) -> Option<&'static str> {
    Some(match op {
        "==" => "eq",
        "!=" => "ne",
        "<" => "lt",
        "<=" => "le",
        ">" => "gt",
        ">=" => "ge",
        _ => return None,
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    Global,
    Scalar,
    Pointer,
    Array,
}

#[derive(Clone)]
struct Symbol {
    slot: i64,
    kind: String,
    scope: Scope,
}

fn list<'a>(node: &'a Node, key: &str) -> &'a [Node] {
    node[key].as_array().map(Vec::as_slice).unwrap_or(&[])
}

fn text<'a>(node: &'a Node, key: &str) -> &'a str {
    node[key].as_str().unwrap_or("")
}

fn int(node: &Node) -> Result<i64> {
    node.as_i64().ok_or_else(|| Error::invalid(format!("{node} is not a number")))
}

fn global_symbols(program: &Node) -> HashMap<String, Symbol> {
    list(program, "globals")
        .iter()
        .enumerate()
        .map(|(slot, declared)| {
            let symbol = Symbol { slot: slot as i64, kind: text(declared, "type").to_string(), scope: Scope::Global };
            (text(declared, "name").to_string(), symbol)
        })
        .collect()
}

fn frame(function: &Node, globals: &HashMap<String, Symbol>) -> Result<(HashMap<String, Symbol>, i64)> {
    let mut symbols = globals.clone();
    let mut slot = 0;
    for param in list(function, "params") {
        let scope = if crate::source::truthy(&param["array"]) { Scope::Pointer } else { Scope::Scalar };
        symbols.insert(text(param, "name").to_string(), Symbol { slot, kind: text(param, "type").to_string(), scope });
        slot += 1;
    }
    for declared in list(function, "locals") {
        let pointer = declared.get("pointer").is_some_and(crate::source::truthy);
        let scope = if pointer {
            Scope::Pointer
        } else if declared["size"].is_null() {
            Scope::Scalar
        } else {
            Scope::Array
        };
        symbols.insert(
            text(declared, "name").to_string(),
            Symbol { slot, kind: text(declared, "type").to_string(), scope },
        );
        slot += if declared["size"].is_null() { 1 } else { int(&declared["size"])? };
    }
    Ok((symbols, slot))
}

#[derive(Clone, Copy)]
struct Loop {
    continue_to: Option<i64>,
    break_to: Option<i64>,
}

struct State<'a> {
    function: String,
    symbols: HashMap<String, Symbol>,
    functions: &'a IndexMap<String, i64>,
    natives: &'a HashMap<String, i64>,
    lines: Vec<String>,
    strings: Vec<String>,
    next_label: i64,
    next_slot: i64,
    loops: Vec<Loop>,
    notes: Option<IndexMap<String, Node>>,
    /// The labels of goto, by name, numbered as the compiler's own when first met.
    labels: HashMap<String, i64>,
}

impl State<'_> {
    fn lookup(&self, name: &str) -> Result<Symbol> {
        self.symbols.get(name).cloned().ok_or_else(|| Error::invalid(format!("unknown variable {name}")))
    }

    fn new_label(&mut self) -> i64 {
        let number = self.next_label;
        self.next_label += 1;
        number
    }

    fn label_name(&self, number: i64) -> String {
        format!("L_{}_{}", self.function, number)
    }

    fn emit(&mut self, line: impl AsRef<str>) {
        self.lines.push(format!("    {}", line.as_ref()));
    }

    /// The number of a goto label.
    fn user_label(&mut self, name: &str) -> i64 {
        if let Some(&number) = self.labels.get(name) {
            return number;
        }
        let number = self.new_label();
        self.labels.insert(name.to_string(), number);
        number
    }

    fn place_label(&mut self, number: i64) {
        let name = self.label_name(number);
        self.lines.push(format!("{name}:"));
        self.emit(format!("nop {number}"));
    }

    /// A hidden local for the compiler's own use, past the function's declared locals.
    fn temporary(&mut self) -> i64 {
        let slot = self.next_slot;
        self.next_slot += 1;
        slot
    }
}

fn scalar_mode(symbol: &Symbol, name: &str) -> Result<&'static str> {
    match symbol.scope {
        Scope::Global => Ok(global_modes(&symbol.kind)),
        Scope::Scalar => Ok(scalar_modes(&symbol.kind)),
        // The slot holds the pointer itself; an int mode moves it without retyping it as a float.
        Scope::Pointer => Ok("local"),
        Scope::Array => fail(format!("{name} is an array")),
    }
}

fn indexed_mode(symbol: &Symbol, name: &str) -> Result<&'static str> {
    match symbol.scope {
        Scope::Array => Ok(array_modes(&symbol.kind)),
        Scope::Pointer if symbol.kind == "void" => fail(format!("{name} is a void pointer; give it int or float")),
        Scope::Pointer => Ok(pointer_modes(&symbol.kind)),
        _ => fail(format!("{name} is not an array")),
    }
}

fn access(state: &mut State, node: &Node, mnemonic: &str) -> Result<()> {
    match node.kind() {
        "deref" => {
            let operand = &node["operand"];
            if operand.kind() != "var" {
                return fail("* works on a pointer or array variable; write p[i] for the others");
            }
            let name = text(operand, "name");
            let symbol = state.lookup(name)?;
            state.emit("push 0");
            state.emit(format!("{} {} {}", mnemonic, indexed_mode(&symbol, name)?, symbol.slot));
        }
        "var" => {
            let name = text(node, "name");
            let symbol = state.lookup(name)?;
            state.emit(format!("{} {} {}", mnemonic, scalar_mode(&symbol, name)?, symbol.slot));
        }
        "index" => {
            let name = text(node, "name");
            let symbol = state.lookup(name)?;
            expression(state, &node["index"])?;
            state.emit(format!("{} {} {}", mnemonic, indexed_mode(&symbol, name)?, symbol.slot));
        }
        other => return fail(format!("cannot take the address of {other}")),
    }
    Ok(())
}

pub fn float_literal(bits: i64) -> Result<String> {
    asm::float_text(bits as i32).ok_or_else(|| Error::invalid("float constant without a decimal form"))
}

fn expression(state: &mut State, node: &Node) -> Result<()> {
    let kind = node.kind();
    match kind {
        "int" => state.emit(format!("push {}", py_str(&node["value"]))),
        "float" => state.emit(format!("push {}", float_literal(int(&node["bits"])?)?)),
        "string" => {
            state.emit(format!("push string {}", state.strings.len()));
            state.strings.push(text(node, "text").to_string());
        }
        "var" if state.lookup(text(node, "name"))?.scope == Scope::Array => {
            // An array used as a value is the address of its first element, as in C.
            let symbol = state.lookup(text(node, "name"))?;
            state.emit("push 0");
            state.emit(format!("addr {} {}", array_modes(&symbol.kind), symbol.slot));
        }
        "var" | "index" | "deref" => access(state, node, "load")?,
        "addr" => access(state, &node["target"], "addr")?,
        "assign" => {
            access(state, &node["target"], "addr")?;
            expression(state, &node["value"])?;
            state.emit("store");
        }
        "binary" if node["op"] == "^" => exclusive_or(state, node)?,
        "binary" if node["op"] == "<<" || node["op"] == ">>" => shift(state, node)?,
        "binary" => {
            expression(state, &node["left"])?;
            expression(state, &node["right"])?;
            let op = text(node, "op");
            match (compare_mnemonic(op), binary_mnemonic(op)) {
                (Some(compare), _) => state.emit(format!("cmp {compare}")),
                (None, Some(mnemonic)) => state.emit(mnemonic),
                _ => return fail(format!("unknown operator {op}")),
            }
        }
        "comma" => {
            expression(state, &node["left"])?;
            state.emit("pop");
            expression(state, &node["right"])?;
        }
        "unary" => {
            expression(state, &node["operand"])?;
            match text(node, "op") {
                "-" => state.emit("neg"),
                "!" => state.emit("not"),
                // C's unary + only promotes, which the VM's values need not
                "+" => {}
                op => return fail(format!("unknown operator {op}")),
            }
        }
        "cast" => {
            expression(state, &node["operand"])?;
            match text(node, "type") {
                "float" => state.emit("itof"),
                "int" => state.emit("ftoi"),
                // a pointer cast, or an address cast to an enum, passes the address unchanged
                other if other.ends_with(" *") => {}
                // the value is worked out and dropped
                "void" => {}
                other if other.starts_with("enum ") && node["operand"].kind() == "addr" => {}
                other => return fail(format!("unknown cast to {other}")),
            }
        }
        "builtin" => {
            expression(state, &node["operand"])?;
            state.emit(text(node, "name"));
        }
        "call" => {
            let name = text(node, "name");
            let Some(&count) = state.functions.get(name) else {
                return fail(format!("unknown function {name}"));
            };
            let args = list(node, "args");
            if args.len() as i64 != count {
                return fail(format!("{name} takes {count} arguments"));
            }
            for arg in args {
                expression(state, arg)?;
            }
            state.emit(format!("call {name}"));
        }
        "logical" => {
            expression(state, &node["left"])?;
            let label = state.new_label();
            let jump = if node["op"] == "&&" { "jf_push" } else { "jt_push" };
            let target = state.label_name(label);
            state.emit(format!("{jump} {target}"));
            expression(state, &node["right"])?;
            state.place_label(label);
        }
        "compound" => compound(state, node)?,
        "narrow" => {
            expression(state, &node["operand"])?;
            narrow(state, text(node, "type"));
        }
        "via" => {
            state.emit("push 0");
            state.emit(format!("load {} {}", pointer_modes(text(node, "type")), int(&node["slot"])?));
        }
        "ternary" => {
            expression(state, &node["cond"])?;
            let otherwise = state.new_label();
            let end = state.new_label();
            let (otherwise_name, end_name) = (state.label_name(otherwise), state.label_name(end));
            state.emit(format!("jf {otherwise_name}"));
            expression(state, &node["then"])?;
            state.emit(format!("jmp {end_name}"));
            state.place_label(otherwise);
            expression(state, &node["else"])?;
            state.place_label(end);
        }
        _ => return fail(format!("unknown expression {kind}")),
    }
    Ok(())
}

/// A value wrapped to the width of a narrow integer type: masked for an unsigned one, and for a
/// signed one offset into the unsigned range, masked and offset back.
fn narrow(state: &mut State, kind: &str) {
    let (mask, offset) = match kind {
        "char" => (255, Some(128)),
        "unsigned char" => (255, None),
        "short" => (65535, Some(32768)),
        _ => (65535, None),
    };
    if let Some(offset) = offset {
        state.emit(format!("push {offset}"));
        state.emit("add");
    }
    state.emit(format!("push {mask}"));
    state.emit("and");
    if let Some(offset) = offset {
        state.emit(format!("push {offset}"));
        state.emit("sub");
    }
}

/// target op= value. A variable compiles as target = target op value. An element's address is
/// worked out once, into a hidden local, and the element is read and written through it, so an
/// index with side effects (a[i++] += 1) runs them once. "convert": "int" truncates a float result
/// for an int target (strict mode), before a narrow type wraps it.
fn compound(state: &mut State, node: &Node) -> Result<()> {
    let target = &node["target"];
    if target.kind() == "var" {
        access(state, target, "addr")?;
        expression(
            state,
            &node!({"kind": "binary", "op": node["op"].clone(), "left": target.clone(), "right": node["value"].clone()}),
        )?;
        if node["convert"] == "int" {
            state.emit("ftoi");
        }
        if let Some(kind) = node["narrow"].as_str() {
            narrow(state, kind);
        }
        state.emit("store");
        return Ok(());
    }
    let name = if target.kind() == "index" { text(target, "name") } else { text(&target["operand"], "name") };
    let symbol = state.lookup(name)?;
    let slot = state.temporary();
    state.emit(format!("addr local {slot}"));
    access(state, target, "addr")?;
    state.emit("store");
    state.emit("pop");
    state.emit("push 0");
    state.emit(format!("addr ptr_idx {slot}"));
    let via = node!({"kind": "via", "slot": slot, "type": symbol.kind});
    expression(
        state,
        &node!({"kind": "binary", "op": node["op"].clone(), "left": via, "right": node["value"].clone()}),
    )?;
    if node["convert"] == "int" {
        state.emit("ftoi");
    }
    if let Some(kind) = node["narrow"].as_str() {
        narrow(state, kind);
    }
    state.emit("store");
    Ok(())
}

fn store_temporary(state: &mut State, slot: i64, node: &Node) -> Result<()> {
    state.emit(format!("addr local {slot}"));
    expression(state, node)?;
    state.emit("store");
    state.emit("pop");
    Ok(())
}

/// a ^ b, which the VM has no instruction for, as (a | b) - (a & b), with a and b worked out once.
fn exclusive_or(state: &mut State, node: &Node) -> Result<()> {
    let (left, right) = (state.temporary(), state.temporary());
    store_temporary(state, left, &node["left"])?;
    store_temporary(state, right, &node["right"])?;
    for op in ["or", "and"] {
        state.emit(format!("load local {left}"));
        state.emit(format!("load local {right}"));
        state.emit(op);
    }
    state.emit("sub");
    Ok(())
}

/// a << n and a >> n as multiplying or dividing by 2 to the n. The VM's division rounds toward
/// zero, so >> of a negative number rounds up where an arithmetic shift rounds down.
fn shift(state: &mut State, node: &Node) -> Result<()> {
    let op = if node["op"] == "<<" { "mul" } else { "div" };
    let count = &node["right"];
    if count.kind() == "int" {
        let shift = u32::try_from(int(&count["value"])?).ok().filter(|shift| *shift < 32);
        let shift = shift.ok_or_else(|| Error::invalid("a shift count out of range (0 to 31)"))?;
        expression(state, &node["left"])?;
        if shift == 31 && op == "div" {
            // only -2^31 divided by 2^31, which an int cannot hold, is not 0
            state.emit("push -2147483648");
            state.emit("cmp eq");
            state.emit("neg");
            return Ok(());
        }
        // 2^31 as an int is -2^31, which multiplies the same in 32 bits
        state.emit(format!("push {}", (1i64 << shift) as i32));
        state.emit(op);
        return Ok(());
    }
    let (value, times) = (state.temporary(), state.temporary());
    store_temporary(state, value, &node["left"])?;
    store_temporary(state, times, count)?;
    let (top, end) = (state.new_label(), state.new_label());
    state.place_label(top);
    state.emit(format!("load local {times}"));
    state.emit("push 0");
    state.emit("cmp gt");
    let end_name = state.label_name(end);
    state.emit(format!("jf {end_name}"));
    for (slot, step) in [(value, ["push 2", op]), (times, ["push 1", "sub"])] {
        state.emit(format!("addr local {slot}"));
        state.emit(format!("load local {slot}"));
        for line in step {
            state.emit(line);
        }
        state.emit("store");
        state.emit("pop");
    }
    let top_name = state.label_name(top);
    state.emit(format!("jmp {top_name}"));
    state.place_label(end);
    state.emit(format!("load local {value}"));
    Ok(())
}

fn native_number(state: &State, name: &str) -> Result<i64> {
    if let Some(&number) = state.natives.get(name) {
        return Ok(number);
    }
    if let Some(digits) = name.strip_prefix("_EXT_") {
        if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
            return digits.parse().map_err(|_| Error::invalid(format!("unknown native {name}")));
        }
    }
    fail(format!("unknown native {name}"))
}

fn statement(state: &mut State, node: &Node) -> Result<()> {
    if let Some(note) = node.get("note").filter(|note| crate::source::truthy(note)) {
        if let Some(notes) = &mut state.notes {
            // a line note, at the index of the statement's first instruction
            let at = state.lines[1..].iter().filter(|line| line.starts_with("    ")).count();
            notes.insert(at.to_string(), note.clone());
        }
    }
    match statement_code(state, node) {
        Err(Error::Invalid(reason)) if !node["line"].is_null() && !reason.starts_with("line ") => {
            Err(Error::invalid(format!("line {}: {}", py_str(&node["line"]), reason)))
        }
        other => other,
    }
}

fn statement_code(state: &mut State, node: &Node) -> Result<()> {
    let kind = node.kind();
    match kind {
        "expr" => {
            expression(state, &node["expr"])?;
            state.emit("pop");
        }
        "return" => {
            if node["value"].is_null() {
                state.emit("push 0");
            } else {
                expression(state, &node["value"])?;
            }
            state.emit("ret");
        }
        "native" => {
            let number = native_number(state, text(node, "name"))?;
            if node["base"].is_null() {
                state.emit(format!("push {number}"));
            } else {
                let base = int(&node["base"])?;
                state.emit(format!("push {base}"));
                state.emit(format!("push {}", number - base));
                state.emit("add");
            }
            let args = list(node, "args");
            for arg in args {
                expression(state, arg)?;
            }
            state.emit(format!("ext {}", args.len() + 1));
        }
        "print" => {
            let args = list(node, "args");
            for arg in args {
                expression(state, arg)?;
            }
            state.emit(format!("print {}", args.len()));
        }
        "wait" | "end" | "skip_end" => state.emit(kind),
        "label" => {
            let number = state.user_label(text(node, "name"));
            state.place_label(number);
        }
        "goto" => {
            let number = state.user_label(text(node, "name"));
            let name = state.label_name(number);
            state.emit(format!("jmp {name}"));
        }
        "if" => conditional(state, node)?,
        "while" => while_loop(state, node)?,
        "switch" => switch(state, node)?,
        "break" | "continue" => {
            let target =
                state
                    .loops
                    .iter()
                    .rev()
                    .find_map(|entry| if kind == "break" { entry.break_to } else { entry.continue_to });
            let Some(target) = target else {
                let place = if kind == "continue" { " outside a loop" } else { " outside a loop or switch" };
                return fail(format!("{kind}{place}"));
            };
            let name = state.label_name(target);
            state.emit(format!("jmp {name}"));
        }
        _ => return fail(format!("unknown statement {kind}")),
    }
    Ok(())
}

fn block(state: &mut State, statements: &[Node]) -> Result<()> {
    for node in statements {
        statement(state, node)?;
    }
    Ok(())
}

fn conditional(state: &mut State, node: &Node) -> Result<()> {
    expression(state, &node["cond"])?;
    let otherwise = state.new_label();
    let otherwise_name = state.label_name(otherwise);
    state.emit(format!("jf {otherwise_name}"));
    block(state, list(node, "then"))?;
    let Some(else_block) = node["else"].as_array() else {
        state.place_label(otherwise);
        return Ok(());
    };
    let end = state.new_label();
    let end_name = state.label_name(end);
    state.emit(format!("jmp {end_name}"));
    state.place_label(otherwise);
    block(state, else_block)?;
    state.place_label(end);
    Ok(())
}

fn while_loop(state: &mut State, node: &Node) -> Result<()> {
    let top = state.new_label();
    let end = state.new_label();
    state.place_label(top);
    expression(state, &node["cond"])?;
    let end_name = state.label_name(end);
    state.emit(format!("jf {end_name}"));
    state.loops.push(Loop { continue_to: Some(top), break_to: Some(end) });
    block(state, list(node, "body"))?;
    state.loops.pop();
    let top_name = state.label_name(top);
    state.emit(format!("jmp {top_name}"));
    state.place_label(end);
    Ok(())
}

/// The value is worked out once into a hidden local, then compared with each case in order; cases
/// fall through into the next as in C, and break leaves the switch.
fn switch(state: &mut State, node: &Node) -> Result<()> {
    let slot = state.temporary();
    store_temporary(state, slot, &node["value"])?;
    let cases = list(node, "cases");
    let labels: Vec<i64> = cases.iter().map(|_| state.new_label()).collect();
    let end = state.new_label();
    let mut fallback = end;
    for (case, &label) in cases.iter().zip(&labels) {
        if crate::source::truthy(&case["default"]) {
            fallback = label;
            continue;
        }
        state.emit(format!("load local {slot}"));
        expression(state, &case["value"])?;
        state.emit("cmp eq");
        let name = state.label_name(label);
        state.emit(format!("jt {name}"));
    }
    let fallback_name = state.label_name(fallback);
    state.emit(format!("jmp {fallback_name}"));
    state.loops.push(Loop { continue_to: None, break_to: Some(end) });
    for (case, &label) in cases.iter().zip(&labels) {
        state.place_label(label);
        block(state, list(case, "body"))?;
    }
    state.loops.pop();
    state.place_label(end);
    Ok(())
}

fn arg_types(params: &[Node]) -> Vec<i32> {
    let mut types: Vec<i32> =
        params.iter().take(10).map(|param| if crate::source::truthy(&param["array"]) { 0 } else { 1 }).collect();
    while types.last() == Some(&0) {
        types.pop();
    }
    types
}

struct Context<'a> {
    globals: HashMap<String, Symbol>,
    functions: IndexMap<String, i64>,
    natives: HashMap<String, i64>,
    buffer: Vec<u8>,
    pool: bool,
    notes: Option<&'a mut IndexMap<String, Node>>,
}

fn strings_of(node: &Node) -> Vec<String> {
    node.as_array().map(|items| items.iter().map(py_str).collect()).unwrap_or_default()
}

fn function_lines(function: &Node, context: &mut Context) -> Result<Vec<String>> {
    let (symbols, size) = frame(function, &context.globals)?;
    let name = text(function, "name").to_string();
    let params = list(function, "params");
    let mut header = format!(".function {} local {} arg {}", name, size, params.len());
    let types = arg_types(params);
    if !types.is_empty() {
        header += &format!(" types {}", types.iter().map(i32::to_string).collect::<Vec<_>>().join(" "));
    }
    // in a pool file the block starts with the file's strings so far (stb::pool_block)
    let mut strings = if context.pool { stb::split_strings(&context.buffer)? } else { Vec::new() };
    strings.extend(strings_of(&function["unused_strings"]));
    let mut state = State {
        function: name.clone(),
        symbols,
        functions: &context.functions,
        natives: &context.natives,
        lines: vec![header.clone()],
        strings,
        next_label: 0,
        next_slot: size,
        loops: Vec::new(),
        notes: context.notes.is_some().then(IndexMap::new),
        labels: HashMap::new(),
    };
    block(&mut state, list(function, "body"))?;
    if let (Some(notes), Some(all)) = (state.notes.take(), context.notes.as_deref_mut()) {
        if !notes.is_empty() {
            all.insert(name.clone(), Node::Map(notes.into_iter().collect()));
        }
    }
    state.emit("push 0");
    state.emit("ret");
    if state.next_slot != size {
        state.lines[0] = header.replacen(&format!(" local {size} "), &format!(" local {} ", state.next_slot), 1);
    }
    let mut padding = Vec::new();
    let mut buffer = context.buffer.clone();
    if context.pool {
        let prefix = stb::split_strings(&context.buffer)?.len();
        let (_, found_padding, next) =
            stb::pool_block(&context.buffer, &state.strings[prefix.min(state.strings.len())..])?;
        padding = found_padding;
        buffer = next;
        if prefix == state.strings.len() {
            // a function that pushes nothing writes the pool as padding, not as strings
            state.strings.clear();
        }
    }
    for text in &state.strings {
        state.lines.push(format!(".string {}", asm::quote(text)));
    }
    if !context.pool {
        let (found_padding, next) = stb::block_padding(&context.buffer, &state.strings)?;
        padding = found_padding;
        buffer = next;
    }
    context.buffer = buffer;
    let padding = match function["padding"].as_str() {
        Some(given) => given.to_string(),
        None => asm::hex(&padding),
    };
    if !padding.is_empty() {
        state.lines.push(format!(".padding {padding}"));
    }
    Ok(state.lines)
}

fn asm_strings(lines: &[String]) -> Result<Vec<String>> {
    let mut strings = Vec::new();
    for line in lines {
        let tokens = asm::tokenize(line)?;
        if tokens.first() == Some(&asm::Token::Word(".string".into())) {
            if let Some(asm::Token::Text(text)) = tokens.get(1) {
                strings.push(text.clone());
            }
        }
    }
    Ok(strings)
}

/// The bytes an assembly function's block holds, which is the pool after it in a pool file.
fn asm_block(lines: &[String]) -> Result<Vec<u8>> {
    let mut padding = String::new();
    for line in lines {
        let tokens = asm::tokenize(line)?;
        if tokens.first() == Some(&asm::Token::Word(".padding".into())) {
            if let Some(asm::Token::Word(hex)) = tokens.get(1) {
                padding = hex.clone();
            }
        }
    }
    let mut out = Vec::new();
    for text in asm_strings(lines)? {
        out.extend(stb::encode_string(&text)?);
    }
    for at in (0..padding.len()).step_by(2) {
        out.push(u8::from_str_radix(&padding[at..at + 2], 16).map_err(|_| Error::invalid("bad padding"))?);
    }
    Ok(out)
}

fn magic_text(magic: &str) -> String {
    let bytes: Vec<u8> = magic.chars().map(|c| c as u32 as u8).collect();
    match <[u8; 4]>::try_from(bytes.as_slice()) {
        Ok(four) => asm::magic_text(&four),
        Err(_) => format!("0x{}", asm::hex(&bytes)),
    }
}

fn header_line(program: &Node) -> Result<String> {
    let mut line = format!(
        ".header {} main {} globals {}",
        magic_text(text(program, "magic")),
        py_str(&program["main"]),
        int(&program["global_num"])?
    );
    if int(&program["unk_14"])? != 0 {
        line += &format!(" unk_14 {}", int(&program["unk_14"])?);
    }
    let padding = text(program, "header_padding");
    if padding != "00".repeat(stb::HEADER_SIZE - stb::HEADER_FIELDS_SIZE) {
        line += &format!(" padding {padding}");
    }
    Ok(line)
}

/// A resolved program as assembly text. notes, when given, collects each function's line notes:
/// {function: {instruction index: text}}.
pub fn to_asm(
    program: &Node,
    tables: &std::collections::BTreeMap<String, Table>,
    notes: Option<&mut IndexMap<String, Node>>,
) -> Result<String> {
    if let Some(assembly) = program.get("asm") {
        return Ok(py_str(assembly));
    }
    if program["main"].is_null() {
        return fail("no main function declared");
    }
    let functions = list(program, "functions");
    if functions.iter().any(|function| function.kind() == "include") {
        return fail("includes must be expanded before compiling");
    }
    let natives_name = text(program, "natives");
    let natives = tables
        .get(natives_name)
        .map(|table| table.natives.iter().map(|native| (native.name.clone(), native.no as i64)).collect())
        .unwrap_or_default();
    let mut context = Context {
        globals: global_symbols(program),
        functions: functions
            .iter()
            .map(|function| {
                let count = if function.kind() == "asm" {
                    int(&function["arg"]).unwrap_or(0)
                } else {
                    list(function, "params").len() as i64
                };
                (text(function, "name").to_string(), count)
            })
            .collect(),
        natives,
        buffer: Vec::new(),
        pool: program.get("string_pool").is_some_and(crate::source::truthy),
        notes,
    };
    for entry in list(program, "programs") {
        let function = text(entry, "function");
        if !context.functions.contains_key(function) {
            return fail(format!("PROGRAM({}, {function}): no function {function}", py_str(&entry["no"])));
        }
    }
    let main = py_str(&program["main"]);
    if !context.functions.contains_key(&main) {
        return fail(format!("MAIN({main}): no function {main}"));
    }
    let mut lines = vec![format!(".natives {natives_name}"), header_line(program)?];
    for entry in list(program, "programs") {
        lines.push(format!(".program {} {}", int(&entry["no"])?, text(entry, "function")));
    }
    lines.push(".code".into());
    for function in functions {
        lines.push(String::new());
        if function.kind() == "asm" {
            let body = strings_of(&function["lines"]);
            if context.pool {
                context.buffer = asm_block(&body)?;
            } else {
                context.buffer = stb::block_padding(&context.buffer, &asm_strings(&body)?)?.1;
            }
            lines.extend(body);
        } else {
            lines.extend(function_lines(function, &mut context)?);
        }
    }
    // the code section ends zero-filled to 16, unless the last block was given in full
    let last = functions.iter().rev().find(|function| matches!(function.kind(), "function" | "asm"));
    if let Some(last) = last {
        if last.kind() == "function" && last["padding"].is_null() {
            lines.push(".tail".into());
        }
    }
    Ok(lines.join("\n") + "\n")
}

/// A program (includes expanded) compiled to script bytes.
pub fn compile_program(program: &Node, tables: &std::collections::BTreeMap<String, Table>) -> Result<Vec<u8>> {
    let resolved = crate::sdk::resolve(program, tables)?;
    asm::assemble(&to_asm(&resolved, tables, None)?, tables)
}

/// What compiling a source file needs besides the file: the SDK and definition headers, the
/// native table rules, the native tables and the SDK folder (for its libraries).
pub struct Toolchain<'a> {
    pub headers: &'a IndexMap<String, String>,
    pub rules: &'a [(String, String)],
    pub tables: &'a std::collections::BTreeMap<String, Table>,
    pub sdk_dir: &'a std::path::Path,
}

/// How to compile one source file: its text when it is not the file's own, the path that picks its
/// native table (its disc path, in a tree with readable names), files that stand in for its
/// includes, and the table to use when the file names none.
#[derive(Default)]
pub struct Options<'a> {
    pub text: Option<&'a str>,
    pub table_path: Option<&'a str>,
    pub overlay: Option<&'a IndexMap<String, String>>,
    pub table: Option<&'a str>,
}

/// A source file compiled to script bytes: its includes read from beside it and the SDK, the
/// native table given (or the one the definitions' path rules give it) when the file names none.
pub fn compile_file(path: &std::path::Path, tools: &Toolchain, options: &Options) -> Result<Vec<u8>> {
    let owned;
    let text = match options.text {
        Some(text) => text,
        None => {
            owned = c2_py::read_text(path)?;
            &owned
        }
    };
    let mut headers = tools.headers.clone();
    headers.extend(crate::definitions::source_headers(path)?);
    let mut program = crate::parse::parse_source(text, crate::parse::UNNAMED_FILE, &headers)?;
    if let Some(table) = options.table.filter(|_| program.get("asm").is_none() && program["natives"].is_null()) {
        program = program.with("natives", table);
    }
    let includes = crate::library::read_includes(&program, path, options.overlay, tools.sdk_dir, &[])?;
    let mut program = crate::library::expand(program, &includes, &headers)?;
    if program.get("asm").is_none() && program["natives"].is_null() {
        let table_path = match options.table_path {
            Some(path) => path.to_string(),
            None => crate::definitions::resolved_posix(path),
        };
        program = program.with("natives", crate::natives::table_for(&table_path, tools.rules));
    }
    compile_program(&program, tools.tables)
}
