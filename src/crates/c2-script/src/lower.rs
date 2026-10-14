//! Standard C the VM has no form for, lowered to forms it has, before the program is resolved
//! (sdk::resolve). Only code the decompiler never writes takes these paths, so retail scripts
//! compile to their own bytes.
//!
//! Initialised globals and static locals: the VM's globals start at 0 when the script loads and
//! cannot hold data, so a hidden function, __initialise, sets them, and every function the engine
//! can start (the main function and each program's) calls it first. A hidden global flag makes it
//! run once per load: the first program to start sets the values, before any of its own code, and
//! the rest find the flag set. A script reloaded starts again from 0, as its globals do. The cost
//! is one global, a call and a test at the start of each entry function, and only in scripts with
//! initialised globals.
//!
//! C's integer types besides int (integer_types): see that function.

use std::collections::{HashMap, HashSet};

use crate::node;
use crate::node::{Map, Node};
use crate::sdk::Context;
use crate::{Error, Result};

/// The hidden function that sets initialised globals, and its flag.
pub const INITIALISE: &str = "__initialise";
pub const INITIALISED: &str = "__initialised";

fn list<'a>(node: &'a Node, key: &str) -> &'a [Node] {
    node[key].as_array().map(Vec::as_slice).unwrap_or(&[])
}

fn text<'a>(node: &'a Node, key: &str) -> &'a str {
    node[key].as_str().unwrap_or("")
}

/// The first part of an initialiser that is not a constant expression (C 3.4): a variable, a
/// call, an assignment or an address.
fn not_constant<'a>(node: &'a Node, found: &Context, variables: &HashSet<&str>) -> Option<&'a Node> {
    match node.kind() {
        "int" | "float" | "string" => None,
        "var" => {
            let name = text(node, "name");
            (variables.contains(name) || !found.constants.contains_key(name)).then_some(node)
        }
        "unary" | "cast" => not_constant(&node["operand"], found, variables),
        "binary" | "logical" => {
            not_constant(&node["left"], found, variables).or_else(|| not_constant(&node["right"], found, variables))
        }
        "ternary" => not_constant(&node["cond"], found, variables)
            .or_else(|| not_constant(&node["then"], found, variables))
            .or_else(|| not_constant(&node["else"], found, variables)),
        _ => Some(node),
    }
}

fn shown(node: &Node) -> String {
    match node.kind() {
        "var" | "call" => text(node, "name").to_string(),
        "assign" | "compound" => "an assignment".into(),
        "addr" => "an address".into(),
        other => format!("a {other}"),
    }
}

/// Globals given values (int g = 5;, and static locals) set by a hidden function that every entry
/// function calls first, once per load.
pub fn initialised_globals(program: &Node, found: &Context) -> Result<Node> {
    let globals = list(program, "globals");
    if !globals.iter().any(|global| global.get("init").is_some() || global.get("const").is_some()) {
        return Ok(program.clone());
    }
    let names: HashSet<&str> = globals.iter().map(|global| text(global, "name")).collect();
    let mut body = Vec::new();
    let mut plain = Vec::new();
    for global in globals {
        let mut entry = global.clone();
        if let Some(map) = entry.as_object_mut() {
            map.remove("init");
            map.remove("const");
        }
        plain.push(entry);
        let Some(value) = global.get("init") else {
            continue;
        };
        if let Some(part) = not_constant(value, found, &names) {
            return Err(Error::invalid(format!(
                "the value of global {} must be a constant expression, not {}",
                text(global, "name"),
                shown(part)
            )));
        }
        let target = node!({"kind": "var", "name": text(global, "name")});
        body.push(node!({"kind": "expr", "expr": {"kind": "assign", "target": target, "value": value.clone()}}));
    }
    if body.is_empty() {
        return Ok(program.clone().with("globals", plain));
    }
    plain.push(node!({"name": INITIALISED, "type": "int"}));
    let flag = node!({"kind": "var", "name": INITIALISED});
    let mut statements = vec![
        node!({"kind": "if", "cond": flag.clone(), "then": [{"kind": "return", "value": null}], "else": null}),
        node!({"kind": "expr", "expr": {"kind": "assign", "target": flag, "value": {"kind": "int", "value": 1}}}),
    ];
    statements.extend(body);
    let initialise = node!({
        "kind": "function",
        "name": INITIALISE,
        "returns": "void",
        "params": [],
        "locals": [],
        "body": statements,
        "unused_strings": [],
        "padding": null,
    });
    let mut entries: HashSet<String> =
        list(program, "programs").iter().map(|entry| text(entry, "function").to_string()).collect();
    if let Some(main) = program["main"].as_str() {
        entries.insert(main.to_string());
    }
    let call = node!({"kind": "expr", "expr": {"kind": "call", "name": INITIALISE, "args": []}});
    let mut functions = Vec::new();
    for function in list(program, "functions") {
        if !entries.contains(text(function, "name")) {
            functions.push(function.clone());
            continue;
        }
        if function.kind() == "asm" {
            return Err(Error::invalid(format!(
                "{} is written in assembly, so it cannot set the script's initialised globals first",
                text(function, "name")
            )));
        }
        let mut body = vec![call.clone()];
        body.extend(list(function, "body").iter().cloned());
        functions.push(function.clone().with("body", body));
    }
    functions.push(initialise);
    let program = program.clone().with("globals", plain).with("functions", functions);
    let count = list(&program, "globals").len();
    let declared = program["global_num"].as_i64().unwrap_or(0) as usize;
    Ok(program.with("global_num", count.max(declared)))
}

// ---- integer types -------------------------------------------------------------------------

/// The runtime the integer types need, in c2s: unsigned division, remainder and right shift, and
/// conversions between unsigned int and float, on the VM's signed 32-bit ints (whose arithmetic
/// wraps). Each function is added to a program that uses it, under its name with c89_ made __.
const RUNTIME: &str = r#"
// The unsigned quotient of a and b.
int c89_udiv(int a, int b)
{
    int quotient;
    int rest;

    if (b < 0) {
        // b is 2^31 or more: the quotient is 1 when a is at least b
        return a + (-2147483647 - 1) >= b + (-2147483647 - 1);
    }
    if (a >= 0) {
        return a / b;
    }
    // a is 2^31 or more: divide half of it, then put back what the halving lost
    quotient = ((a + (-2147483647 - 1)) / 2 + 1073741824) / b * 2;
    rest = a - quotient * b;
    if (rest + (-2147483647 - 1) >= b + (-2147483647 - 1)) {
        quotient = quotient + 1;
    }
    return quotient;
}

// The unsigned remainder of a and b.
int c89_umod(int a, int b)
{
    return a - c89_udiv(a, b) * b;
}

// a shifted right by count bits, 0 coming in from the left.
int c89_ushr(int a, int count)
{
    if (count <= 0) {
        return a;
    }
    if (a < 0) {
        a = (a + (-2147483647 - 1)) / 2 + 1073741824;
        count = count - 1;
    }
    while (count > 0) {
        a = a / 2;
        count = count - 1;
    }
    return a;
}

// An unsigned int as a float.
float c89_utof(int value)
{
    if (value >= 0) {
        return (float)value;
    }
    return (float)(value + (-2147483647 - 1)) + 2147483648.0;
}

// A float as an unsigned int.
int c89_ftou(float value)
{
    if (value < 2147483648.0) {
        return (int)value;
    }
    return (int)(value - 2147483648.0) + (-2147483647 - 1);
}
"#;

/// The bit pattern int arithmetic adds to unsigned values to compare them as signed ones.
const BIAS: i64 = -2147483648;

fn is_integer(kind: &str) -> bool {
    kind == "int" || crate::parse::INTEGER_TYPES.contains(&kind)
}

fn is_narrow(kind: &str) -> bool {
    matches!(kind, "char" | "unsigned char" | "short" | "unsigned short")
}

/// The type an integer type takes part in arithmetic as: char and short promote to int.
fn promoted(kind: &str) -> &str {
    if is_narrow(kind) {
        "int"
    } else {
        kind
    }
}

/// C's usual arithmetic conversions: float with a float; else unsigned int with an unsigned int;
/// else int. Any other type (a string, a pointer, what a native gives) stays as it is.
fn common(left: &str, right: &str) -> String {
    let (left, right) = (promoted(left), promoted(right));
    if left == "float" || right == "float" {
        "float".into()
    } else if left == "unsigned int" || right == "unsigned int" {
        if is_integer(left) && is_integer(right) {
            "unsigned int".into()
        } else {
            "any".into()
        }
    } else if left == "int" && right == "int" {
        "int".into()
    } else {
        "any".into()
    }
}

/// The types code sees: the variables of the function, the globals, the functions and natives.
struct Types<'a> {
    variables: HashMap<String, (String, bool)>,
    globals: HashMap<String, String>,
    functions: &'a HashMap<String, (String, Vec<String>)>,
    found: &'a Context,
    returns: String,
    used: &'a mut Vec<&'static str>,
}

/// A declared type as the base type it stands for (a typedef's, int for an enum).
fn base(kind: &str, kinds: &Map) -> String {
    if kind.starts_with("enum ") {
        return "int".into();
    }
    match kinds.get(kind).and_then(Node::as_str) {
        Some(found) => found.to_string(),
        None => kind.to_string(),
    }
}

fn int_node(value: i64) -> Node {
    node!({"kind": "int", "value": value})
}

fn call(name: &'static str, args: Vec<Node>, used: &mut Vec<&'static str>) -> Node {
    if !used.contains(&name) {
        used.push(name);
    }
    node!({"kind": "call", "name": name, "args": args})
}

/// A value stored in a variable of a narrow type, wrapped to its width (a float truncated first).
fn narrowed(value: Node, from: &str, kind: &str) -> Node {
    let value = if from == "float" { node!({"kind": "cast", "type": "int", "operand": value}) } else { value };
    if let Some(constant) = value.get("value").filter(|_| value.kind() == "int").and_then(Node::as_i64) {
        return int_node(match kind {
            "char" => constant as i8 as i64,
            "unsigned char" => constant as u8 as i64,
            "short" => constant as i16 as i64,
            _ => constant as u16 as i64,
        });
    }
    node!({"kind": "narrow", "type": kind, "operand": value})
}

/// A value converted for a variable or parameter of a type: narrowed to a narrow type; an unsigned
/// value made a float for a float. Anything else keeps its type, as the VM's values do.
fn converted(value: Node, from: &str, to: &str, used: &mut Vec<&'static str>) -> Node {
    if is_narrow(to) {
        narrowed(value, from, to)
    } else if to == "float" && from == "unsigned int" {
        call(UTOF, vec![value], used)
    } else {
        value
    }
}

const UDIV: &str = "__udiv";
const UMOD: &str = "__umod";
const USHR: &str = "__ushr";
const UTOF: &str = "__utof";
const FTOU: &str = "__ftou";

fn literal(node: &Node) -> Option<u32> {
    (node.kind() == "int").then(|| node["value"].as_i64()).flatten().map(|value| value as u32)
}

impl Types<'_> {
    /// The type of a variable named in code, and whether it is an array or pointer of that type.
    fn variable(&self, name: &str) -> Option<(String, bool)> {
        self.variables.get(name).cloned().or_else(|| self.globals.get(name).map(|kind| (kind.clone(), false)))
    }

    /// An expression lowered, and its type.
    fn expression(&mut self, node: &Node) -> Result<(Node, String)> {
        let kind = node.kind();
        Ok(match kind {
            "int" => (node.clone(), if node.get("unsigned").is_some() { "unsigned int" } else { "int" }.into()),
            "float" => (node.clone(), "float".into()),
            "string" => (node.clone(), "string".into()),
            "var" => match self.variable(text(node, "name")) {
                Some((_, true)) => (node.clone(), "pointer".into()),
                Some((kind, false)) => (node.clone(), kind),
                None => (node.clone(), "int".into()),
            },
            "index" => {
                let (index, _) = self.expression(&node["index"])?;
                let kind = self.variable(text(node, "name")).map_or("any".into(), |(kind, _)| kind);
                (node.clone().with("index", index), kind)
            }
            "deref" => {
                let kind = self.variable(text(&node["operand"], "name")).map_or("any".into(), |(kind, _)| kind);
                (node.clone(), kind)
            }
            "addr" => {
                let (target, _) = self.expression(&node["target"])?;
                (node.clone().with("target", target), "pointer".into())
            }
            "assign" => {
                let (target, to) = self.expression(&node["target"])?;
                let (value, from) = self.expression(&node["value"])?;
                let value = converted(value, &from, &to, self.used);
                (node.clone().with("target", target).with("value", value), to)
            }
            "compound" => self.compound(node)?,
            "unary" => {
                let (operand, operand_kind) = self.expression(&node["operand"])?;
                let result =
                    if text(node, "op") == "!" { "int".to_string() } else { promoted(&operand_kind).to_string() };
                (node.clone().with("operand", operand), result)
            }
            "binary" => self.binary(node)?,
            "logical" => {
                let (left, _) = self.expression(&node["left"])?;
                let (right, _) = self.expression(&node["right"])?;
                (node.clone().with("left", left).with("right", right), "int".into())
            }
            "ternary" => {
                let (cond, _) = self.expression(&node["cond"])?;
                let (then, then_kind) = self.expression(&node["then"])?;
                let (otherwise, otherwise_kind) = self.expression(&node["else"])?;
                let kind = common(&then_kind, &otherwise_kind);
                let then = if kind == "float" { converted(then, &then_kind, "float", self.used) } else { then };
                let otherwise =
                    if kind == "float" { converted(otherwise, &otherwise_kind, "float", self.used) } else { otherwise };
                (node.clone().with("cond", cond).with("then", then).with("else", otherwise), kind)
            }
            "cast" => self.cast(node)?,
            "comma" => {
                let (left, _) = self.expression(&node["left"])?;
                let (right, kind) = self.expression(&node["right"])?;
                (node.clone().with("left", left).with("right", right), kind)
            }
            "call" => {
                let name = text(node, "name");
                let mut args = Vec::new();
                let params = self.functions.get(name).map(|(_, params)| params.clone()).unwrap_or_default();
                for (at, arg) in list(node, "args").iter().enumerate() {
                    let (arg, from) = self.expression(arg)?;
                    args.push(match params.get(at) {
                        Some(to) => converted(arg, &from, to, self.used),
                        None => arg,
                    });
                }
                let returns = match self.functions.get(name) {
                    Some((returns, _)) => returns.clone(),
                    None => {
                        self.found.natives.get(name).map_or("any".into(), |native| text(native, "returns").to_string())
                    }
                };
                (node.clone().with("args", args), returns)
            }
            "builtin" => {
                let (operand, _) = self.expression(&node["operand"])?;
                (node.clone().with("operand", operand), "float".into())
            }
            _ => (node.clone(), "any".into()),
        })
    }

    fn cast(&mut self, node: &Node) -> Result<(Node, String)> {
        let (operand, from) = self.expression(&node["operand"])?;
        let to = text(node, "type").to_string();
        if is_narrow(&to) {
            return Ok((narrowed(operand, &from, &to), to));
        }
        let lowered = match (to.as_str(), from.as_str()) {
            ("float", "unsigned int") => call(UTOF, vec![operand], self.used),
            ("unsigned int", "float") => call(FTOU, vec![operand], self.used),
            // the bits are the value's already
            ("unsigned int", _) => operand,
            _ => node.clone().with("operand", operand),
        };
        Ok((lowered, to))
    }

    fn binary(&mut self, node: &Node) -> Result<(Node, String)> {
        let op = text(node, "op");
        let (left, left_kind) = self.expression(&node["left"])?;
        let (right, right_kind) = self.expression(&node["right"])?;
        let shift = op == "<<" || op == ">>";
        let kind = if shift { promoted(&left_kind).to_string() } else { common(&left_kind, &right_kind) };
        let compared = matches!(op, "==" | "!=" | "<" | "<=" | ">" | ">=");
        let result = if compared { "int".to_string() } else { kind.clone() };
        let plain = |left: Node, right: Node| node.clone().with("left", left).with("right", right);
        if node.get("postfix").is_some() {
            // x++ is the stepped value worked back to the old one, which for a narrow x wraps
            // back too (an unsigned char at 255 steps to 0, and 0 - 1 is 255 again)
            let (_, stepped) = self.expression(&node["left"]["target"])?;
            if is_narrow(&stepped) {
                return Ok((narrowed(plain(left, right), "int", &stepped), stepped));
            }
        }
        if kind == "float" {
            // an unsigned operand of float arithmetic converts by its value
            let left = converted(left, &left_kind, "float", self.used);
            let right = converted(right, &right_kind, "float", self.used);
            return Ok((plain(left, right), result));
        }
        if kind != "unsigned int" {
            return Ok((plain(left, right), result));
        }
        let folded = literal(&left).zip(literal(&right));
        let lowered = match op {
            "/" | "%" => match folded {
                Some((a, b)) if b != 0 => int_node((if op == "/" { a / b } else { a % b }) as i32 as i64),
                _ => call(if op == "/" { UDIV } else { UMOD }, vec![left, right], self.used),
            },
            ">>" => match folded {
                Some((a, b)) if b < 32 => int_node((a >> b) as i32 as i64),
                _ => call(USHR, vec![left, right], self.used),
            },
            "<" | "<=" | ">" | ">=" => match folded {
                Some((a, b)) => int_node(i64::from(match op {
                    "<" => a < b,
                    "<=" => a <= b,
                    ">" => a > b,
                    _ => a >= b,
                })),
                None => {
                    let biased = |side: Node| match literal(&side) {
                        Some(value) => int_node((value as i64 + BIAS) as i32 as i64),
                        None => node!({"kind": "binary", "op": "+", "left": side, "right": int_node(BIAS)}),
                    };
                    plain(biased(left), biased(right))
                }
            },
            _ => plain(left, right),
        };
        let lowered = if node.get("postfix").is_some() { lowered.with("postfix", true) } else { lowered };
        Ok((lowered, result))
    }

    /// target op= value: worked out in the target's type, and narrowed to it when stored.
    fn compound(&mut self, node: &Node) -> Result<(Node, String)> {
        let (target, to) = self.expression(&node["target"])?;
        let (value, from) = self.expression(&node["value"])?;
        let op = text(node, "op");
        let kind = if op == "<<" || op == ">>" { promoted(&to).to_string() } else { common(&to, &from) };
        if kind == "unsigned int" && matches!(op, "/" | "%" | ">>") {
            // an unsigned quotient, remainder or shift is a call: target = op(target, value)
            if contains_effects(&target) {
                return Err(Error::invalid(
                    "an unsigned /=, %= or >>= on an element whose index has side effects is not supported",
                ));
            }
            let binary = node!({"kind": "binary", "op": op, "left": target.clone(), "right": value});
            let (operation, kind) = self.binary(&binary)?;
            let stored = converted(operation, &kind, &to, self.used);
            return Ok((node!({"kind": "assign", "target": target, "value": stored}), to));
        }
        let lowered = node.clone().with("target", target).with("value", value);
        Ok((if is_narrow(&to) { lowered.with("narrow", to.clone()) } else { lowered }, to))
    }

    /// A statement, and those in it, lowered.
    fn statement(&mut self, node: &Node) -> Result<Node> {
        let mut out = node.clone();
        match node.kind() {
            "expr" => {
                let (expression, _) = self.expression(&node["expr"])?;
                out = out.with("expr", expression);
            }
            "return" if !node["value"].is_null() => {
                let (value, from) = self.expression(&node["value"])?;
                let to = self.returns.clone();
                out = out.with("value", converted(value, &from, &to, self.used));
            }
            "if" => {
                let (cond, _) = self.expression(&node["cond"])?;
                out = out.with("cond", cond).with("then", self.statements(list(node, "then"))?);
                if let Some(otherwise) = node["else"].as_array() {
                    out = out.with("else", self.statements(otherwise)?);
                }
            }
            "while" => {
                let (cond, _) = self.expression(&node["cond"])?;
                out = out.with("cond", cond).with("body", self.statements(list(node, "body"))?);
            }
            "switch" => {
                let (value, _) = self.expression(&node["value"])?;
                let mut cases = Vec::new();
                for case in list(node, "cases") {
                    let mut case = case.clone().with("body", self.statements(list(case, "body"))?);
                    if !case["value"].is_null() {
                        let (value, _) = self.expression(&case["value"])?;
                        case = case.with("value", value);
                    }
                    cases.push(case);
                }
                out = out.with("value", value).with("cases", cases);
            }
            "print" | "native" => {
                let mut args = Vec::new();
                for arg in list(node, "args") {
                    args.push(self.expression(arg)?.0);
                }
                out = out.with("args", args);
            }
            _ => {}
        }
        Ok(out)
    }

    fn statements(&mut self, nodes: &[Node]) -> Result<Vec<Node>> {
        nodes.iter().map(|node| self.statement(node)).collect()
    }
}

/// Whether an expression assigns or calls.
fn contains_effects(node: &Node) -> bool {
    match node {
        Node::List(items) => items.iter().any(contains_effects),
        Node::Map(map) => matches!(node.kind(), "assign" | "compound" | "call") || map.values().any(contains_effects),
        _ => false,
    }
}

/// Whether a program uses C's other integer types: a variable, parameter, function or cast of one,
/// or an unsigned constant. Retail scripts use none, and are left as they are.
fn uses_integer_types(node: &Node, kinds: &Map) -> bool {
    match node {
        Node::List(items) => items.iter().any(|item| uses_integer_types(item, kinds)),
        Node::Map(map) => {
            let typed = ["type", "returns"].iter().any(|key| {
                node.get(key).and_then(Node::as_str).is_some_and(|kind| {
                    let kind = base(kind, kinds);
                    kind != "int" && is_integer(&kind)
                })
            });
            typed || node.get("unsigned").is_some() || map.values().any(|value| uses_integer_types(value, kinds))
        }
        _ => false,
    }
}

/// char, short, unsigned and their kin on the VM's 32-bit ints: a value stored in a char or short
/// is wrapped to its width with a mask; unsigned int keeps its bits in an int, and its division,
/// remainder, right shift, comparisons and conversions to and from float are worked out as C's
/// (the first three by functions of the runtime above). Conversions happen where C's would on
/// storing to a variable of a narrow type, passing an argument, returning, and in casts and
/// mixed arithmetic.
pub fn integer_types(program: &Node, kinds: &Map, found: &Context) -> Result<Node> {
    let unsigned_constants = found.constants.values().any(|constant| constant.get("unsigned").is_some());
    let used_anywhere = unsigned_constants
        || uses_integer_types(&program["functions"], kinds)
        || uses_integer_types(&program["globals"], kinds);
    if program.get("asm").is_some() || !used_anywhere {
        return Ok(program.clone());
    }
    let globals: HashMap<String, String> = list(program, "globals")
        .iter()
        .map(|global| (text(global, "name").to_string(), base(text(global, "type"), kinds)))
        .collect();
    let functions: HashMap<String, (String, Vec<String>)> = list(program, "functions")
        .iter()
        .filter(|function| function.kind() == "function")
        .map(|function| {
            let params = list(function, "params")
                .iter()
                .map(|param| {
                    if param["array"] == Node::Bool(true) {
                        "pointer".into()
                    } else {
                        base(text(param, "type"), kinds)
                    }
                })
                .collect();
            (text(function, "name").to_string(), (base(text(function, "returns"), kinds), params))
        })
        .collect();
    let mut used = Vec::new();
    let mut lowered = Vec::new();
    for function in list(program, "functions") {
        if function.kind() != "function" {
            lowered.push(function.clone());
            continue;
        }
        let mut variables = HashMap::new();
        let mut shadowed: HashSet<String> = globals.keys().cloned().collect();
        for param in list(function, "params") {
            shadowed.insert(text(param, "name").to_string());
            let array = param["array"] == Node::Bool(true);
            variables.insert(text(param, "name").to_string(), (base(text(param, "type"), kinds), array));
        }
        for local in list(function, "locals") {
            shadowed.insert(text(local, "name").to_string());
            let array = !local["size"].is_null() || local.get("pointer").is_some();
            variables.insert(text(local, "name").to_string(), (base(text(local, "type"), kinds), array));
        }
        let mut types = Types {
            variables,
            globals: globals.clone(),
            functions: &functions,
            found,
            returns: base(text(function, "returns"), kinds),
            used: &mut used,
        };
        // constants as their values, so an unsigned one is typed as one
        let body = crate::sdk::with_constants(&function["body"], &found.constants, &shadowed);
        let body = types.statements(body.as_array().map_or(&[][..], Vec::as_slice))?;
        lowered.push(function.clone().with("body", body));
    }
    if !used.is_empty() {
        // the runtime functions a call was made to, and those they call
        if used.contains(&UMOD) && !used.contains(&UDIV) {
            used.push(UDIV);
        }
        let runtime = crate::parse::parse(RUNTIME)?;
        for function in list(&runtime, "functions") {
            let name = text(function, "name").replacen("c89_", "__", 1);
            if used.contains(&name.as_str()) {
                let mut function = function.clone().with("name", name).with("body", rename_calls(&function["body"]));
                if let Some(map) = function.as_object_mut() {
                    map.remove("doc");
                }
                lowered.push(function);
            }
        }
    }
    Ok(program.clone().with("functions", lowered))
}

/// The runtime's calls of each other, by their hidden names.
fn rename_calls(node: &Node) -> Node {
    match node {
        Node::List(items) => Node::List(items.iter().map(rename_calls).collect()),
        Node::Map(map) => {
            let mut out = Map::with_capacity(map.len());
            for (key, value) in map.entries() {
                let value = if key == "name" && node.kind() == "call" {
                    Node::from(text(node, "name").replacen("c89_", "__", 1))
                } else {
                    rename_calls(value)
                };
                out.insert(key.clone(), value);
            }
            Node::Map(out)
        }
        other => other.clone(),
    }
}
