//! Turning a script's VM code back into structured statements: the stack machine's expressions,
//! if/else and while from the jumps the original compiler emitted, and the types of locals,
//! parameters and globals from how they are loaded. A function that does not fit the patterns
//! stays assembly (with the reason), so every script still compiles back to its own bytes.
//!
//! The program is a tree of nodes (node.rs), its keys in the order the source writer and the
//! compiler read them.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::node;
use crate::node::{Map, Node};

use crate::asm::{self, hex};
use crate::fingerprint;
use crate::natives::{Native, Table};
use crate::stack;
use crate::stb::{self, Compare, Function, Instruction, Mode, Op, Place, Region, Script};
use crate::stb::{CONST_FLOAT, CONST_INT, CONST_STR};

const MAX_GLOBALS: i64 = 0x10000;

type Fail<T> = std::result::Result<T, String>;

/// A LOAD's addressing mode when it is one of a place (None for none, another place, or a mode the
/// VM does not have).
fn mode_at(mode: Option<i32>, place: Place) -> Option<Mode> {
    mode.and_then(Mode::from_bit).filter(|mode| mode.place == place)
}

pub fn function_name(offset: i64) -> String {
    format!("f_{offset:04x}")
}

/// Sorted numbers as the reason a function stays assembly lists them: [1, 8].
fn int_list(values: &BTreeSet<i32>) -> String {
    format!("[{}]", values.iter().map(i32::to_string).collect::<Vec<_>>().join(", "))
}

/// Each slot's addressing mode, in the order the slots are first used. The globals' are left out;
/// a mode the VM does not have is kept (and refused by declarations).
fn slot_modes(function: &Function) -> Fail<Vec<(i32, i32)>> {
    let mut modes: Vec<(i32, BTreeSet<i32>)> = Vec::new();
    for item in &function.instructions {
        let loads = matches!(item.opcode(), Some(Op::Load | Op::LoadAddr));
        if loads && mode_at(Some(item.arg2), Place::Global).is_none() {
            match modes.iter_mut().find(|(slot, _)| *slot == item.arg1) {
                Some((_, found)) => {
                    found.insert(item.arg2);
                }
                None => modes.push((item.arg1, BTreeSet::from([item.arg2]))),
            }
        }
    }
    for (slot, found) in &modes {
        if found.len() != 1 {
            return Err(format!("slot {} used with modes {}", slot, int_list(found)));
        }
    }
    Ok(modes.into_iter().map(|(slot, found)| (slot, *found.iter().next().unwrap())).collect())
}

fn declarations(function: &Function) -> Fail<(Vec<Node>, Vec<Node>)> {
    let modes = slot_modes(function)?;
    let mode_of = |slot: i32| modes.iter().find(|(at, _)| *at == slot).map(|(_, mode)| *mode);
    let mut params = Vec::new();
    for slot in 0..function.arg {
        let mode = mode_of(slot);
        let declared = if (slot as usize) < stb::FUNCDATA_ARGS { function.arg_types[slot as usize] } else { 1 };
        if let Some(pointer) = mode_at(mode, Place::Pointer) {
            params.push(node!({"name": format!("a{slot}"), "type": pointer.kind(), "array": true}));
            continue;
        }
        let scalar = mode_at(mode, Place::Local);
        if let Some(other) = mode.filter(|_| scalar.is_none()) {
            return Err(format!("parameter {slot} uses mode {other}"));
        }
        params.push(node!({
            "name": format!("a{slot}"),
            "type": scalar.map_or("int", Mode::kind),
            "array": mode.is_none() && declared == 0,
        }));
    }
    let mut locals = Vec::new();
    let mut slot = function.arg;
    while slot < function.local {
        let mode = mode_of(slot);
        if let Some(array) = mode_at(mode, Place::Array) {
            let mut end = slot + 1;
            while end < function.local && mode_of(end).is_none() {
                end += 1;
            }
            locals.push(node!({"name": format!("v{slot}"), "type": array.kind(), "size": end - slot}));
            slot = end;
            continue;
        }
        let scalar = mode_at(mode, Place::Local);
        if let Some(other) = mode.filter(|_| scalar.is_none()) {
            return Err(format!("local {slot} uses mode {other}"));
        }
        locals.push(node!({"name": format!("v{slot}"), "type": scalar.map_or("int", Mode::kind), "size": null}));
        slot += 1;
    }
    for (found, _) in &modes {
        if *found >= function.local || *found < 0 {
            return Err(format!("slot {found} outside the frame"));
        }
    }
    Ok((params, locals))
}

fn arg_types_for(params: &[Node]) -> Vec<i32> {
    let mut types: Vec<i32> =
        params.iter().take(stb::FUNCDATA_ARGS).map(|param| if param.flag("array") { 0 } else { 1 }).collect();
    types.resize(stb::FUNCDATA_ARGS, 0);
    types
}

fn variable_name(slot: i32, mode: i32, function: &Function) -> String {
    if mode_at(Some(mode), Place::Global).is_some() {
        format!("g{slot}")
    } else if slot < function.arg {
        format!("a{slot}")
    } else {
        format!("v{slot}")
    }
}

struct State<'a> {
    ins: &'a [Instruction],
    function: &'a Function,
    start: i64,
    functions: &'a HashMap<i64, i32>,
    natives: HashSet<usize>,
    native_table: &'a BTreeMap<i32, &'a Native>,
    strings: std::collections::VecDeque<String>,
}

fn target(state: &State, index: usize) -> Fail<usize> {
    let offset = state.ins[index].arg1 as i64;
    let delta = offset - state.start;
    let position = delta.div_euclid(stb::VMCODE_SIZE as i64);
    if delta.rem_euclid(stb::VMCODE_SIZE as i64) != 0 || !(0 <= position && position < state.ins.len() as i64) {
        return Err("jump into the middle of an instruction".into());
    }
    let position = position as usize;
    if state.ins[position].opcode() != Some(Op::Nop) {
        return Err("jump to an instruction that is not a label".into());
    }
    Ok(position)
}

fn pop(values: &mut Vec<Node>, count: usize) -> Fail<Vec<Node>> {
    if values.len() < count {
        return Err("stack underflow".into());
    }
    Ok(values.split_off(values.len() - count))
}

fn pop1(values: &mut Vec<Node>) -> Fail<Node> {
    Ok(pop(values, 1)?.remove(0))
}

fn pop2(values: &mut Vec<Node>) -> Fail<(Node, Node)> {
    let mut taken = pop(values, 2)?;
    let right = taken.pop().unwrap();
    Ok((taken.pop().unwrap(), right))
}

/// Where an expression stopped: the values it leaves, the index it stopped at, and the instruction
/// there (None when it ran to its end).
struct Stopped {
    values: Vec<Node>,
    at: usize,
    item: Option<Instruction>,
}

/// The expression from index on, to stop or the first instruction that is not part of one.
fn expression(state: &mut State, mut index: usize, stop: usize) -> Fail<Stopped> {
    let mut values: Vec<Node> = Vec::new();
    while index < stop {
        let item = state.ins[index];
        let op = item.opcode().expect("the reader keeps only the VM's opcodes");
        match op {
            Op::Load | Op::LoadAddr => {
                let name = variable_name(item.arg1, item.arg2, state.function);
                let mut node = node!({"kind": "var", "name": name});
                if Mode::from_bit(item.arg2).is_some_and(Mode::is_indexed) {
                    let index_value = pop1(&mut values)?;
                    node = node!({"kind": "index", "name": node["name"].clone(), "index": index_value});
                }
                if op == Op::LoadAddr {
                    node = node!({"kind": "addr", "target": node});
                }
                values.push(node);
            }
            Op::PushConst => values.push(constant(&item, index, state)?),
            Op::Store => {
                let (destination, value) = pop2(&mut values)?;
                if destination.kind() != "addr" {
                    return Err("store through a computed address".into());
                }
                values.push(node!({"kind": "assign", "target": destination["target"].clone(), "value": value}));
            }
            Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod | Op::And | Op::Or => {
                let (left, right) = pop2(&mut values)?;
                let op = op.binary_operator().expect("a binary instruction is a C operator");
                values.push(node!({"kind": "binary", "op": op, "left": left, "right": right}));
            }
            Op::Cmp => {
                let Some(compare) = Compare::from_number(item.arg1) else {
                    return Err("unknown comparison".into());
                };
                let (left, right) = pop2(&mut values)?;
                values.push(node!({"kind": "binary", "op": compare.operator(), "left": left, "right": right}));
            }
            Op::Neg | Op::Not => {
                let operand = pop1(&mut values)?;
                let op = if op == Op::Neg { "-" } else { "!" };
                values.push(node!({"kind": "unary", "op": op, "operand": operand}));
            }
            Op::Itof | Op::Ftoi => {
                let operand = pop1(&mut values)?;
                let cast = if op == Op::Itof { "float" } else { "int" };
                values.push(node!({"kind": "cast", "type": cast, "operand": operand}));
            }
            Op::Sin | Op::Cos => {
                let operand = pop1(&mut values)?;
                let name = if op == Op::Sin { "sin" } else { "cos" };
                values.push(node!({"kind": "builtin", "name": name, "operand": operand}));
            }
            Op::Call => {
                let callee = item.arg2 as i64;
                let Some(&count) = state.functions.get(&callee) else {
                    return Err("call to something that is not a function".into());
                };
                let args = pop(&mut values, usize::try_from(count).unwrap_or(usize::MAX))?;
                values.push(node!({"kind": "call", "name": function_name(callee), "args": args}));
            }
            Op::JmpFalse | Op::JmpTrue if item.arg2 == 1 => {
                let left = pop1(&mut values)?;
                let (logical, next) = short_circuit(state, op, left, index, stop)?;
                values.push(logical);
                index = next;
                continue;
            }
            _ => return Ok(Stopped { values, at: index, item: Some(item) }),
        }
        index += 1;
    }
    Ok(Stopped { values, at: index, item: None })
}

/// a && b or a || b: a jump that keeps its tested value pushed (left) past the right operand to a
/// label. The node and the index after the label.
fn short_circuit(state: &mut State, op: Op, left: Node, index: usize, stop: usize) -> Fail<(Node, usize)> {
    let label_index = target(state, index)?;
    if label_index <= index || label_index >= stop {
        return Err("short-circuit jump leaves its expression".into());
    }
    let mut right = expression(state, index + 1, label_index)?;
    if right.item.is_some() || right.values.len() != 1 {
        return Err("short-circuit operand is not one expression".into());
    }
    let logical = node!({
        "kind": "logical",
        "op": if op == Op::JmpFalse { "&&" } else { "||" },
        "left": left,
        "right": right.values.remove(0),
        "label": state.ins[label_index].arg1,
    });
    Ok((logical, label_index + 1))
}

fn constant(item: &Instruction, index: usize, state: &mut State) -> Fail<Node> {
    let kind = item.arg1;
    if (kind == CONST_INT || kind == CONST_FLOAT) && item.arg2 < 0 && !state.natives.contains(&index) {
        return Err("negative constant".into());
    }
    if kind == CONST_INT {
        if state.natives.contains(&index) {
            return Ok(node!({"kind": "native_number", "value": item.arg2}));
        }
        return Ok(node!({"kind": "int", "value": item.arg2}));
    }
    if kind == CONST_FLOAT {
        if asm::float_text(item.arg2).is_none() {
            return Err("float constant without a decimal form".into());
        }
        return Ok(node!({"kind": "float", "bits": item.arg2}));
    }
    if kind == CONST_STR {
        let Some(text) = state.strings.pop_front() else {
            return Err("string pushes do not follow the string block".into());
        };
        return Ok(node!({"kind": "string", "text": text}));
    }
    Err("unknown constant type".into())
}

fn native_name(number: i64, state: &State) -> String {
    match i32::try_from(number).ok().and_then(|number| state.native_table.get(&number)) {
        Some(known) => known.name.clone(),
        None => format!("_EXT_{number}"),
    }
}

fn simple_statement(mut values: Vec<Node>, item: &Instruction, state: &State) -> Fail<Node> {
    match item.opcode() {
        Some(Op::Pop) => {
            if values.len() != 1 {
                return Err("expression statement leaves values".into());
            }
            Ok(node!({"kind": "expr", "expr": values.remove(0)}))
        }
        Some(Op::Ret) => {
            if values.len() != 1 {
                return Err("return leaves values".into());
            }
            Ok(node!({"kind": "return", "value": values.remove(0)}))
        }
        Some(Op::Ext) => {
            if values.len() as i64 != item.arg1 as i64 || item.arg2 != 0 {
                return Err("native call inside an expression".into());
            }
            let number = values.remove(0);
            if number.kind() == "native_number" {
                let value = number["value"].as_i64().unwrap();
                return Ok(node!({"kind": "native", "name": native_name(value, state), "base": null, "args": values}));
            }
            if number.kind() == "binary"
                && number["op"] == "+"
                && number["left"].kind() == "native_number"
                && number["right"].kind() == "int"
            {
                let base = number["left"]["value"].as_i64().unwrap();
                let total = base + number["right"]["value"].as_i64().unwrap();
                return Ok(node!({"kind": "native", "name": native_name(total, state), "base": base, "args": values}));
            }
            Err("native number is not a constant".into())
        }
        Some(Op::Print) => {
            if values.len() as i64 != item.arg1 as i64 || item.arg2 != 0 {
                return Err("print inside an expression".into());
            }
            Ok(node!({"kind": "print", "args": values}))
        }
        Some(op @ (Op::Wait | Op::End | Op::SkipEnd)) => {
            if !values.is_empty() || item.arg1 != 0 || item.arg2 != 0 {
                return Err("statement with operands".into());
            }
            Ok(node!({"kind": op.mnemonic()}))
        }
        _ => Err(format!("unexpected {}", item.name())),
    }
}

#[derive(Clone, Copy)]
struct Loop {
    top: i32,
    end: i32,
}

fn block(state: &mut State, mut index: usize, stop: usize, loops: &[Loop]) -> Fail<Vec<Node>> {
    let mut statements = Vec::new();
    while index < stop {
        // where each statement starts, as an instruction index in the function, for line notes
        let start = index;
        if state.ins[index].opcode() == Some(Op::Nop) {
            let (statement, next) = while_loop(state, index, stop, loops)?;
            statements.push(statement.with("start", node!(start)));
            index = next;
            continue;
        }
        let Stopped { mut values, at, item } = expression(state, index, stop)?;
        index = at;
        let Some(item) = item else {
            return Err("expression runs into the end of a block".into());
        };
        let op = item.opcode();
        if op == Some(Op::JmpFalse) && item.arg2 == 0 {
            if values.len() != 1 {
                return Err("condition leaves values".into());
            }
            let (statement, next) = conditional(state, values.remove(0), index, stop, loops)?;
            statements.push(statement.with("start", node!(start)));
            index = next;
            continue;
        }
        if op == Some(Op::Jmp) {
            let Some(inner) = loops.last() else {
                return Err("jump outside a loop".into());
            };
            if !values.is_empty() || item.arg2 != 0 {
                return Err("jump outside a loop".into());
            }
            let label = state.ins[target(state, index)?].arg1;
            if label == inner.end {
                statements.push(node!({"kind": "break", "start": start}));
            } else if label == inner.top {
                statements.push(node!({"kind": "continue", "start": start}));
            } else {
                return Err("jump that is neither break nor continue".into());
            }
            index += 1;
            continue;
        }
        statements.push(simple_statement(values, &item, state)?.with("start", node!(start)));
        index += 1;
    }
    Ok(statements)
}

fn conditional(state: &mut State, condition: Node, index: usize, stop: usize, loops: &[Loop]) -> Fail<(Node, usize)> {
    let else_index = target(state, index)?;
    if !(index < else_index && else_index < stop) {
        return Err("if jumps outside its block".into());
    }
    let before = state.ins[else_index - 1];
    if before.opcode() == Some(Op::Jmp) && before.arg2 == 0 && else_index - 1 > index {
        let end_index = target(state, else_index - 1)?;
        if else_index < end_index && end_index < stop {
            let then = block(state, index + 1, else_index - 1, loops)?;
            let otherwise = block(state, else_index + 1, end_index, loops)?;
            let statement = node!({
                "kind": "if",
                "cond": condition,
                "then": then,
                "else": otherwise,
                "labels": [state.ins[else_index].arg1, state.ins[end_index].arg1],
            });
            return Ok((statement, end_index + 1));
        }
    }
    let then = block(state, index + 1, else_index, loops)?;
    let statement = node!({
        "kind": "if",
        "cond": condition,
        "then": then,
        "else": null,
        "labels": [state.ins[else_index].arg1],
    });
    Ok((statement, else_index + 1))
}

fn while_loop(state: &mut State, index: usize, stop: usize, loops: &[Loop]) -> Fail<(Node, usize)> {
    let top = state.ins[index].arg1;
    let Stopped { mut values, at: jump, item } = expression(state, index + 1, stop)?;
    match item {
        Some(item) if item.opcode() == Some(Op::JmpFalse) && item.arg2 == 0 && values.len() == 1 => {}
        _ => return Err("label that does not start a loop".into()),
    }
    let end_index = target(state, jump)?;
    if !(jump < end_index && end_index < stop) {
        return Err("loop exit outside its block".into());
    }
    let back = state.ins[end_index - 1];
    if back.opcode() != Some(Op::Jmp) || back.arg2 != 0 || target(state, end_index - 1)? != index {
        return Err("loop without a jump back to its top".into());
    }
    let end = state.ins[end_index].arg1;
    let mut inner = loops.to_vec();
    inner.push(Loop { top, end });
    let body = block(state, jump + 1, end_index - 1, &inner)?;
    Ok((node!({"kind": "while", "cond": values.remove(0), "body": body, "labels": [top, end]}), end_index + 1))
}

fn expression_labels(node: &Node, out: &mut Vec<i64>) {
    match node.kind() {
        "logical" => {
            expression_labels(&node["left"], out);
            out.push(node["label"].as_i64().unwrap());
            expression_labels(&node["right"], out);
        }
        "binary" => {
            expression_labels(&node["left"], out);
            expression_labels(&node["right"], out);
        }
        "unary" | "cast" | "builtin" => expression_labels(&node["operand"], out),
        "assign" => {
            expression_labels(&node["target"], out);
            expression_labels(&node["value"], out);
        }
        "index" => expression_labels(&node["index"], out),
        "addr" => expression_labels(&node["target"], out),
        "call" => {
            for arg in node["args"].as_array().unwrap() {
                expression_labels(arg, out);
            }
        }
        _ => {}
    }
}

fn statement_labels(statements: &[Node], out: &mut Vec<i64>) {
    for statement in statements {
        match statement.kind() {
            "expr" => expression_labels(&statement["expr"], out),
            "return" => expression_labels(&statement["value"], out),
            "native" | "print" => {
                for arg in statement["args"].as_array().unwrap() {
                    expression_labels(arg, out);
                }
            }
            "if" => {
                expression_labels(&statement["cond"], out);
                out.push(statement["labels"][0].as_i64().unwrap());
                statement_labels(statement["then"].as_array().unwrap(), out);
                if !statement["else"].is_null() {
                    out.push(statement["labels"][1].as_i64().unwrap());
                    statement_labels(statement["else"].as_array().unwrap(), out);
                }
            }
            "while" => {
                out.push(statement["labels"][0].as_i64().unwrap());
                out.push(statement["labels"][1].as_i64().unwrap());
                expression_labels(&statement["cond"], out);
                statement_labels(statement["body"].as_array().unwrap(), out);
            }
            _ => {}
        }
    }
}

fn strip_labels(node: &Node) -> Node {
    match node {
        Node::List(items) => Node::List(items.iter().map(strip_labels).collect()),
        Node::Map(object) => Node::Map(
            object
                .entries()
                .filter(|(key, _)| key != "labels" && key != "label")
                .map(|(key, value)| (key.clone(), strip_labels(value)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn unused_strings(function: &Function, code: i64) -> Fail<Vec<String>> {
    let refs: Vec<i64> = function
        .instructions
        .iter()
        .filter(|item| item.opcode() == Some(Op::PushConst) && item.arg1 == CONST_STR)
        .map(|item| code + item.arg2 as i64)
        .collect();
    let offsets = asm::string_offsets(function);
    if offsets.len() < refs.len() || offsets[offsets.len() - refs.len()..] != refs[..] {
        return Err("string pushes do not follow the string block".into());
    }
    Ok(function.strings[..offsets.len() - refs.len()].to_vec())
}

struct Context<'a> {
    code: i64,
    functions: HashMap<i64, i32>,
    native_table: BTreeMap<i32, &'a Native>,
    global_types: HashMap<String, String>,
    returns: HashMap<String, String>,
}

fn structured(function: &Function, context: &mut Context) -> Fail<Node> {
    let instructions = &function.instructions;
    if instructions.len() < 3 {
        return Err("function too short".into());
    }
    let tail = &instructions[instructions.len() - 3..];
    if !(tail[0].is_int()
        && tail[0].arg2 == 0
        && tail[1].opcode() == Some(Op::Ret)
        && tail[1].arg1 == 0
        && tail[1].arg2 == 0)
    {
        return Err("function does not end in return 0".into());
    }
    if function.name != 0 {
        return Err("function record has a name".into());
    }
    let code = context.code;
    let unused = unused_strings(function, code)?;
    let (params, locals) = declarations(function)?;
    if arg_types_for(&params) != function.arg_types {
        return Err("argument types differ from the declarations".into());
    }
    let mut state = State {
        ins: instructions,
        function,
        start: (function.offset + stb::FUNCDATA_SIZE) as i64 - code,
        functions: &context.functions,
        natives: asm::native_positions(instructions, &context.functions),
        native_table: &context.native_table,
        strings: function.strings[unused.len()..].iter().cloned().collect(),
    };
    let body = block(&mut state, 0, instructions.len() - 3, &[])?;
    if !state.strings.is_empty() {
        return Err("unused strings in the string block".into());
    }
    let mut labels = Vec::new();
    statement_labels(&body, &mut labels);
    if labels.iter().enumerate().any(|(at, label)| *label != at as i64) {
        return Err("labels are not numbered in allocation order".into());
    }
    let mut types = context.global_types.clone();
    for declared in params.iter().chain(&locals) {
        types.insert(declared["name"].as_str().unwrap().to_string(), declared["type"].as_str().unwrap().to_string());
    }
    let name = function_name(function.offset as i64 - code);
    let returns = return_type(&body, &types, &context.returns);
    context.returns.insert(name.clone(), returns.to_string());
    Ok(node!({
        "kind": "function",
        "name": name,
        "returns": returns,
        "params": params,
        "locals": locals,
        "body": strip_labels(&Node::List(body)),
        "unused_strings": unused,
        "padding": hex(&function.padding),
    }))
}

fn value_type(node: &Node, types: &HashMap<String, String>, returns: &HashMap<String, String>) -> String {
    let found = node.kind();
    match found {
        "int" | "float" | "string" => found.to_string(),
        "var" | "index" => types.get(node["name"].as_str().unwrap()).cloned().unwrap_or_else(|| "int".into()),
        "assign" => value_type(&node["target"], types, returns),
        "binary" => {
            let op = node["op"].as_str().unwrap();
            if matches!(op, "==" | "!=" | "<" | "<=" | ">" | ">=" | "&" | "|") {
                return "int".into();
            }
            let left = value_type(&node["left"], types, returns);
            let right = value_type(&node["right"], types, returns);
            if left == "float" || right == "float" { "float" } else { "int" }.into()
        }
        "unary" => {
            if node["op"] == "-" {
                value_type(&node["operand"], types, returns)
            } else {
                "int".into()
            }
        }
        "cast" => node["type"].as_str().unwrap().to_string(),
        "builtin" => "float".into(),
        "call" => match returns.get(node["name"].as_str().unwrap()).map(String::as_str) {
            None | Some("void") => "int".into(),
            Some(found) => found.to_string(),
        },
        _ => "int".into(),
    }
}

fn returned_values<'a>(statements: &'a [Node], out: &mut Vec<&'a Node>) {
    for node in statements {
        match node.kind() {
            "return" => out.push(&node["value"]),
            "if" => {
                returned_values(node["then"].as_array().unwrap(), out);
                if let Some(otherwise) = node["else"].as_array() {
                    returned_values(otherwise, out);
                }
            }
            "while" => returned_values(node["body"].as_array().unwrap(), out),
            _ => {}
        }
    }
}

fn return_type(body: &[Node], types: &HashMap<String, String>, returns: &HashMap<String, String>) -> &'static str {
    let mut values = Vec::new();
    returned_values(body, &mut values);
    if values.is_empty() {
        return "void";
    }
    if values.iter().any(|value| value_type(value, types, returns) == "float") {
        return "float";
    }
    "int"
}

/// The functions called for their value (not as a statement of their own).
fn valued_calls(node: &Node, out: &mut Vec<String>) {
    match node {
        Node::List(items) => {
            for item in items {
                valued_calls(item, out);
            }
        }
        Node::Map(object) => {
            if node.get("kind") == Some(&node!("expr")) && node["expr"].kind() == "call" {
                valued_calls(&node["expr"]["args"], out);
                return;
            }
            if node.get("kind") == Some(&node!("call")) {
                out.push(node["name"].as_str().unwrap().to_string());
            }
            for value in object.values() {
                valued_calls(value, out);
            }
        }
        _ => {}
    }
}

/// Whether a statement or expression reads, writes or takes the address of a variable.
fn mentions(node: &Node, name: &str) -> bool {
    match node {
        Node::List(items) => items.iter().any(|item| mentions(item, name)),
        Node::Map(object) => {
            (matches!(node.kind(), "var" | "index") && node["name"] == *name)
                || object.values().any(|value| mentions(value, name))
        }
        _ => false,
    }
}

/// The positions of a function's pointer parameters its body never uses. Nothing in the bytecode
/// gives them an element type (the function record only says they are pointers), so it comes from
/// what callers pass.
pub fn unused_pointer_params(function: &Node) -> Vec<usize> {
    if function.kind() != "function" {
        return Vec::new();
    }
    let body = function.get("body").unwrap_or(&Node::Null);
    function
        .list("params")
        .iter()
        .enumerate()
        .filter(|(_, param)| param.flag("array") && !mentions(body, param["name"].as_str().unwrap_or("")))
        .map(|(position, _)| position)
        .collect()
}

/// The type of each variable a function sees: its parameters and locals over the globals.
fn variable_types(function: &Node, globals: &[Node]) -> HashMap<String, String> {
    let declared = globals.iter().chain(function.list("params")).chain(function.list("locals"));
    declared
        .filter_map(|variable| Some((variable["name"].as_str()?.to_string(), variable["type"].as_str()?.to_string())))
        .collect()
}

/// The element type of an address argument (&x, &a[i]), from the variable's declaration.
fn address_type(arg: &Node, types: &HashMap<String, String>) -> Option<String> {
    if arg.kind() != "addr" || !matches!(arg["target"].kind(), "var" | "index") {
        return None;
    }
    types.get(arg["target"]["name"].as_str()?).filter(|found| matches!(found.as_str(), "int" | "float")).cloned()
}

fn calls_in<'a>(node: &'a Node, out: &mut Vec<&'a Node>) {
    match node {
        Node::List(items) => items.iter().for_each(|item| calls_in(item, out)),
        Node::Map(object) => {
            if node.kind() == "call" {
                out.push(node);
            }
            object.values().for_each(|value| calls_in(value, out));
        }
        _ => {}
    }
}

/// The element types a program's calls pass for each unused pointer parameter, by callee name and
/// position.
pub fn pointer_evidence(functions: &[Node], globals: &[Node]) -> BTreeMap<(String, usize), BTreeSet<String>> {
    let unused: HashMap<&str, Vec<usize>> = functions
        .iter()
        .map(|function| (function["name"].as_str().unwrap_or(""), unused_pointer_params(function)))
        .filter(|(_, positions)| !positions.is_empty())
        .collect();
    let mut found: BTreeMap<(String, usize), BTreeSet<String>> = BTreeMap::new();
    for function in functions.iter().filter(|function| function.kind() == "function") {
        let types = variable_types(function, globals);
        let mut calls = Vec::new();
        calls_in(function.get("body").unwrap_or(&Node::Null), &mut calls);
        for call in calls {
            let callee = call["name"].as_str().unwrap_or("");
            let Some(positions) = unused.get(callee) else {
                continue;
            };
            for &position in positions {
                if let Some(element) = call.list("args").get(position).and_then(|arg| address_type(arg, &types)) {
                    found.entry((callee.to_string(), position)).or_default().insert(element);
                }
            }
        }
    }
    found
}

/// The type an unused pointer parameter takes from what its callers pass: theirs when they agree,
/// void (printed void *) when they do not; None without callers.
pub fn passed_type(passed: &BTreeSet<String>) -> Option<&str> {
    match passed.len() {
        0 => None,
        1 => passed.iter().next().map(String::as_str),
        _ => Some("void"),
    }
}

/// Wraps address arguments whose variable's type differs from the int or float pointer parameter
/// they are passed for in a pointer cast ((int *)&distance). The bytecode passes the address as
/// it is; the cast only says so in C.
fn cast_addresses(node: &mut Node, params: &HashMap<String, Vec<Option<String>>>, types: &HashMap<String, String>) {
    match node {
        Node::List(items) => items.iter_mut().for_each(|item| cast_addresses(item, params, types)),
        Node::Map(object) => {
            object.iter_mut().for_each(|(_, value)| cast_addresses(value, params, types));
            if node.kind() != "call" {
                return;
            }
            let Some(expected) = node["name"].as_str().and_then(|name| params.get(name)).cloned() else {
                return;
            };
            let Some(args) = node.as_object_mut().and_then(|call| call.get_mut("args")).and_then(Node::as_array_mut)
            else {
                return;
            };
            for (arg, expected) in args.iter_mut().zip(expected) {
                let (Some(expected), Some(passed)) = (expected, address_type(arg, types)) else {
                    continue;
                };
                if passed != expected {
                    *arg = node!({"kind": "cast", "type": format!("{expected} *"), "operand": arg.clone()});
                }
            }
        }
        _ => {}
    }
}

/// Types the program's unused pointer parameters from their callers, and casts the addresses calls
/// pass for a pointer of another type.
fn type_pointer_params(functions: &mut [Node], globals: &[Node]) {
    for ((callee, position), passed) in pointer_evidence(functions, globals) {
        let Some(element) = passed_type(&passed) else {
            continue;
        };
        if let Some(function) = functions.iter_mut().find(|function| function["name"] == *callee.as_str()) {
            function["params"][position]["type"] = node!(element);
        }
    }
    let params: HashMap<String, Vec<Option<String>>> = functions
        .iter()
        .filter(|function| function.kind() == "function")
        .map(|function| {
            let types = function
                .list("params")
                .iter()
                .map(|param| {
                    let element = param["type"].as_str().filter(|found| matches!(*found, "int" | "float"));
                    element.filter(|_| param.flag("array")).map(str::to_string)
                })
                .collect();
            (function["name"].as_str().unwrap_or("").to_string(), types)
        })
        .collect();
    for function in functions.iter_mut().filter(|function| function.kind() == "function") {
        let types = variable_types(function, globals);
        if let Some(body) = function.as_object_mut().and_then(|object| object.get_mut("body")) {
            cast_addresses(body, &params, &types);
        }
    }
}

fn fallback(function: &Function, context: &Context, reason: String) -> Node {
    let functions: HashSet<i64> = context.functions.keys().copied().collect();
    let lines = asm::function_text(function, context.code, &functions, &context.native_table, &context.functions);
    node!({
        "kind": "asm",
        "name": function_name(function.offset as i64 - context.code),
        "arg": function.arg,
        "reason": reason,
        "lines": lines,
    })
}

fn strings_of(value: &Node) -> Vec<String> {
    value
        .as_array()
        .map(|items| items.iter().map(|item| item.as_str().unwrap().to_string()).collect())
        .unwrap_or_default()
}

/// What the compiler's string buffer predicts for each function's block, as (strings before the
/// pushed ones, padding), with the end-of-file zero fill on the last (see stb::block_padding).
fn predicted(functions: &[&Function], decompiled: &[Node], code: i64, pool: bool) -> Vec<(Vec<String>, Vec<u8>)> {
    let mut buffer: Vec<u8> = Vec::new();
    let mut out = Vec::new();
    for (position, (function, found)) in functions.iter().zip(decompiled).enumerate() {
        let unused = strings_of(&found["unused_strings"]);
        let (prefix, mut padding) = if pool {
            let pushed = &function.strings[unused.len().min(function.strings.len())..];
            let (prefix, padding, next) = stb::pool_block(&buffer, pushed).expect("strings the file holds encode");
            buffer = next;
            (prefix, padding)
        } else {
            let (padding, next) =
                stb::block_padding(&buffer, &function.strings).expect("strings the file holds encode");
            buffer = next;
            (unused, padding)
        };
        if position == functions.len() - 1 {
            let end = function.offset + function.size - function.padding.len() + padding.len();
            padding.extend(std::iter::repeat_n(0, stb::tail_padding(end, code as usize)));
        }
        out.push((prefix, padding));
    }
    out
}

fn matches(functions: &[&Function], decompiled: &[Node], code: i64, pool: bool) -> Vec<bool> {
    functions
        .iter()
        .zip(decompiled)
        .zip(predicted(functions, decompiled, code, pool))
        .map(|((function, found), (prefix, padding))| {
            found.kind() == "function" && strings_of(&found["unused_strings"]) == prefix && function.padding == padding
        })
        .collect()
}

/// Whether the file's string buffer was never reset between functions: only when that predicts
/// every block and resetting does not.
fn string_pool(functions: &[&Function], decompiled: &[Node], code: i64) -> bool {
    if decompiled.iter().any(|found| found.kind() != "function")
        || matches(functions, decompiled, code, false).into_iter().all(|matched| matched)
    {
        return false;
    }
    matches(functions, decompiled, code, true).into_iter().all(|matched| matched)
}

/// Drops the padding the buffer predicts, and in a pool file the leading strings it implies.
fn predicted_blocks(functions: &[&Function], decompiled: Vec<Node>, code: i64, pool: bool) -> Vec<Node> {
    let matched = matches(functions, &decompiled, code, pool);
    decompiled
        .into_iter()
        .zip(matched)
        .map(|(found, matched)| {
            if !matched {
                return found;
            }
            let unused = if pool { node!([]) } else { found["unused_strings"].clone() };
            found.with("padding", Node::Null).with("unused_strings", unused)
        })
        .collect()
}

fn global_types(script: &Script) -> BTreeMap<i32, BTreeSet<&'static str>> {
    let mut types: BTreeMap<i32, BTreeSet<&'static str>> = BTreeMap::new();
    for function in script.functions() {
        for item in &function.instructions {
            if let Some(global) = item.mode().filter(|mode| mode.place == Place::Global) {
                types.entry(item.arg1).or_default().insert(global.kind());
            }
        }
    }
    types
}

/// Whether the script has the layout the original compiler always wrote: header, program table,
/// then only functions, with main and every program pointing at one.
fn regular(script: &Script) -> bool {
    let Some(header) = &script.header else {
        return false;
    };
    let kinds: Vec<&str> = script
        .regions
        .iter()
        .map(|region| match region {
            Region::Header(_) => "header",
            Region::Programs { .. } => "programs",
            Region::Function(_) => "function",
            Region::Raw { .. } => "raw",
        })
        .collect();
    if kinds.contains(&"raw") || kinds.len() < 2 || kinds[..2] != ["header", "programs"] {
        return false;
    }
    if header.prog != stb::HEADER_SIZE as i32
        || header.code as i64 != stb::HEADER_SIZE as i64 + header.prog_num as i64 * stb::PROGDATA_SIZE as i64
    {
        return false;
    }
    let functions: HashSet<i64> = script.functions().map(|function| function.offset as i64).collect();
    if !functions.contains(&(header.main as i64)) {
        return false;
    }
    match &script.regions[1] {
        Region::Programs { entries, .. } => entries.iter().all(|entry| functions.contains(&(entry.func as i64))),
        _ => false,
    }
}

fn as_assembly(script: &Script, table: &str, tables: &BTreeMap<String, Table>) -> Node {
    node!({"kind": "program", "asm": asm::disassemble(script, table, tables)})
}

/// The magic as Python reads it: four bytes as Latin-1 characters.
fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

/// A script as a program: functions as statements where they fit, assembly where not; the whole
/// file as assembly when its layout is not the original compiler's.
pub fn decompile(script: &Script, table: &str, tables: &BTreeMap<String, Table>) -> Node {
    let native_table = tables.get(table).map(Table::numbered).unwrap_or_default();
    if !regular(script) {
        return as_assembly(script, table, tables);
    }
    let header = script.header.as_ref().unwrap();
    let code = header.code as i64;
    let mut context = Context {
        code,
        functions: stack::function_args(script),
        native_table,
        global_types: HashMap::new(),
        returns: HashMap::new(),
    };
    let found = global_types(script);
    if let (Some(&low), Some(&high)) = (found.keys().next(), found.keys().next_back()) {
        if !(0 <= low && (high as i64) < MAX_GLOBALS) {
            return as_assembly(script, table, tables);
        }
    }
    let mut globals = Vec::new();
    let count = found.keys().next_back().map_or(0, |&high| high + 1);
    for slot in 0..count {
        let kind = match found.get(&slot) {
            None => "int",
            Some(types) if types.len() == 1 => types.iter().next().unwrap(),
            Some(_) => return as_assembly(script, table, tables),
        };
        globals.push(node!({"name": format!("g{slot}"), "type": kind}));
    }
    context.global_types = globals
        .iter()
        .map(|declared| {
            (declared["name"].as_str().unwrap().to_string(), declared["type"].as_str().unwrap().to_string())
        })
        .collect();
    let regions: Vec<&Function> = script.functions().collect();
    let mut functions = Vec::with_capacity(regions.len());
    for function in &regions {
        match structured(function, &mut context) {
            Ok(found) => functions.push(found),
            Err(reason) => functions.push(fallback(function, &context, reason)),
        }
    }
    let pool = string_pool(&regions, &functions, code);
    let mut functions = predicted_blocks(&regions, functions, code, pool);
    let prints = fingerprint::both_fingerprints(script, table);
    let mut used = Vec::new();
    valued_calls(
        &Node::List(functions.iter().map(|function| function.get("body").cloned().unwrap_or(node!([]))).collect()),
        &mut used,
    );
    type_pointer_params(&mut functions, &globals);
    for function in &mut functions {
        if function.get("returns") == Some(&node!("void"))
            && used.contains(&function["name"].as_str().unwrap().to_string())
        {
            function["returns"] = node!("int");
        }
    }
    let entries = match &script.regions[1] {
        Region::Programs { entries, .. } => entries,
        _ => unreachable!("a regular script has its program table second"),
    };
    let named = |found: &BTreeMap<i64, String>| -> Map {
        prints.order.iter().map(|offset| (function_name(*offset), node!(&found[offset]))).collect()
    };
    node!({
        "kind": "program",
        "natives": table,
        "magic": latin1(&header.magic),
        "unk_14": header.unk_14,
        "header_padding": hex(&header.padding),
        "globals": globals,
        "global_num": header.global_num,
        "main": function_name(header.main as i64 - code),
        "programs": entries.iter().map(|entry| node!({"no": entry.no, "function": function_name(entry.func as i64 - code)})).collect::<Vec<_>>(),
        "functions": functions,
        "string_pool": pool,
        "padding_of": {},
        "declarations": {"natives": [], "programs": [], "inlines": [], "constants": [], "kinds": {}},
        "fingerprints": named(&prints.exact),
        "loose_fingerprints": named(&prints.loose),
    })
}
