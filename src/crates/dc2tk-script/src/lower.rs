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

use crate::ctype::IntType;
use crate::node::{kinds, Map, Node};
use crate::sdk::Context;
use crate::{Error, Result};

/// The hidden function that sets initialised globals, and its flag.
pub const INITIALISE: &str = "__initialise";
pub const INITIALISED: &str = "__initialised";

/// The first part of an initialiser that is not a constant expression (C 3.4): a variable, a
/// call, an assignment, an address of something but a global (or a function, a function pointer's
/// value).
fn not_constant<'a>(
    node: &'a Node,
    found: &Context,
    variables: &HashSet<&str>,
    functions: &HashSet<&str>,
) -> Option<&'a Node> {
    let not_constant = |node: &'a Node| not_constant(node, found, variables, functions);
    match node.kind() {
        kinds::INT | kinds::FLOAT | kinds::STRING => None,
        kinds::VAR if functions.contains(node.text("name")) && !variables.contains(node.text("name")) => None,
        kinds::VAR => {
            let name = node.text("name");
            (variables.contains(name) || !found.constants.contains_key(name)).then_some(node)
        }
        // an address constant: of a global, a member or element of one at constant offsets
        kinds::ADDR if !address_constant(&node["target"], variables, functions) => Some(node),
        kinds::ADDR => index_constant(&node["target"]).and_then(not_constant),
        kinds::UNARY | kinds::CAST => not_constant(&node["operand"]),
        kinds::BINARY | kinds::LOGICAL => not_constant(&node["left"]).or_else(|| not_constant(&node["right"])),
        kinds::TERNARY => {
            not_constant(&node["cond"]).or_else(|| not_constant(&node["then"])).or_else(|| not_constant(&node["else"]))
        }
        _ => Some(node),
    }
}

/// Whether an address is a constant one: of a global or function, or a member or element of a
/// global (whose indexes index_constant checks).
fn address_constant(target: &Node, variables: &HashSet<&str>, functions: &HashSet<&str>) -> bool {
    match target.kind() {
        kinds::VAR => variables.contains(target.text("name")) || functions.contains(target.text("name")),
        kinds::MEMBER if target["arrow"] != Node::Bool(true) => {
            address_constant(&target["object"], variables, functions)
        }
        kinds::SUBSCRIPT => address_constant(&target["array"], variables, functions),
        kinds::INDEX => variables.contains(target.text("name")),
        _ => false,
    }
}

/// The index in an address constant's target, which must be a constant too.
fn index_constant(target: &Node) -> Option<&Node> {
    match target.kind() {
        kinds::SUBSCRIPT => Some(&target["index"]),
        kinds::INDEX => Some(&target["index"]),
        kinds::MEMBER => index_constant(&target["object"]),
        _ => None,
    }
}

fn shown(node: &Node) -> String {
    match node.kind() {
        kinds::VAR | kinds::CALL => node.text("name").to_string(),
        kinds::ASSIGN | kinds::COMPOUND => "an assignment".into(),
        kinds::ADDR => "an address".into(),
        other => format!("a {other}"),
    }
}

/// Globals given values (int g = 5;, and static locals) set by a hidden function that every entry
/// function calls first, once per load.
pub fn initialised_globals(mut program: Node, found: &Context) -> Result<Node> {
    let globals = program.take("globals").into_list();
    let given = |global: &Node| global.get("init").is_some() || global.get("inits").is_some();
    if !globals.iter().any(|global| given(global) || global.get("const").is_some()) {
        program["globals"] = Node::List(globals);
        return Ok(program);
    }
    let strict = globals.iter().any(|global| global["std"] == "c89");
    let (plain, body) = assignments(globals, program.list("functions"), found)?;
    if body.is_empty() {
        program["globals"] = Node::List(plain);
        return Ok(program);
    }
    let mut plain = plain;
    plain.push(node!({"name": INITIALISED, "type": "int"}));
    program["globals"] = Node::List(plain);
    let functions = program.take("functions").into_list();
    program["functions"] = Node::List(with_initialise(&program, functions)?);
    let initialise = initialise_function(body, strict);
    program["functions"].as_array_mut().expect("the functions are a list").push(initialise);
    let count: i64 = program.list("globals").iter().map(|global| global["size"].as_i64().unwrap_or(1)).sum();
    let declared = program["global_num"].as_i64().unwrap_or(0);
    Ok(program.with("global_num", count.max(declared)))
}

/// The globals as plain declarations (their values, consts and std left out), and an assignment
/// for each value one is given: a scalar's, or each slot an array's or struct's list gives. Every
/// value must be a constant expression, as C requires of static storage.
fn assignments(globals: Vec<Node>, functions: &[Node], found: &Context) -> Result<(Vec<Node>, Vec<Node>)> {
    let names: HashSet<String> = globals.iter().map(|global| global.text("name").to_string()).collect();
    let names: HashSet<&str> = names.iter().map(String::as_str).collect();
    let functions: HashSet<&str> = functions.iter().map(|function| function.text("name")).collect();
    let mut body = Vec::new();
    let mut plain = Vec::new();
    for mut global in globals {
        let name = global.text("name").to_string();
        let (init, inits) = (global.remove("init"), global.remove("inits"));
        global.remove("const");
        global.remove("std");
        plain.push(global);
        let values: Vec<(Node, Node)> = match (init, inits) {
            (Some(value), _) => vec![(node!({"kind": "var", "name": name.as_str()}), value)],
            (None, inits) => inits
                .map(Node::into_list)
                .unwrap_or_default()
                .into_iter()
                .map(|mut slot| {
                    let target = node!({
                        "kind": "field",
                        "name": name.as_str(),
                        "offset": slot.take("offset"),
                        "type": slot.take("type"),
                        "ctype": slot.take("ctype"),
                    });
                    (target, slot.take("value"))
                })
                .collect(),
        };
        for (target, value) in values {
            if let Some(part) = not_constant(&value, found, &names, &functions) {
                return Err(Error::invalid(format!(
                    "the value of global {name} must be a constant expression, not {}",
                    shown(part)
                )));
            }
            body.push(node!({"kind": "expr", "expr": {"kind": "assign", "target": target, "value": value}}));
        }
    }
    Ok((plain, body))
}

/// The hidden function that sets the globals, once per load: it returns at once when its flag is
/// set, and sets the flag first.
fn initialise_function(body: Vec<Node>, strict: bool) -> Node {
    let flag = node!({"kind": "var", "name": INITIALISED});
    let mut statements = vec![
        node!({"kind": "if", "cond": flag.clone(), "then": [{"kind": "return", "value": null}], "else": null}),
        node!({"kind": "expr", "expr": {"kind": "assign", "target": flag, "value": {"kind": "int", "value": 1}}}),
    ];
    statements.extend(body);
    let mut initialise = node!({
        "kind": "function",
        "name": INITIALISE,
        "returns": "void",
        "params": [],
        "locals": [],
        "body": statements,
        "unused_strings": [],
        "padding": null,
    });
    if strict {
        // a value given under #pragma std c89 converts to its global's type
        initialise["std"] = Node::from("c89");
    }
    initialise
}

/// The functions with a call of the hidden function first in each the engine can start: the main
/// function and each program's.
fn with_initialise(program: &Node, functions: Vec<Node>) -> Result<Vec<Node>> {
    let mut entries: HashSet<&str> = program.list("programs").iter().map(|entry| entry.text("function")).collect();
    if let Some(main) = program["main"].as_str() {
        entries.insert(main);
    }
    let call = node!({"kind": "expr", "expr": {"kind": "call", "name": INITIALISE, "args": []}});
    functions
        .into_iter()
        .map(|mut function| {
            if !entries.contains(function.text("name")) {
                return Ok(function);
            }
            if function.kind() == kinds::ASM {
                return Err(Error::invalid(format!(
                    "{} is written in assembly, so it cannot set the script's initialised globals first",
                    function.text("name")
                )));
            }
            let mut body = vec![call.clone()];
            body.extend(function.take("body").into_list());
            Ok(function.with("body", body))
        })
        .collect()
}

// ---- integer types -------------------------------------------------------------------------

/// The runtime the integer types need, in the scripts: unsigned division, remainder and right shift, and
/// conversions between unsigned int and float, on the VM's signed 32-bit ints (whose arithmetic
/// wraps). Each function is added to a program that uses it, under its name with c89_ made __.
const RUNTIME: &str = r"
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
";

/// The bit pattern int arithmetic adds to unsigned values to compare them as signed ones.
const BIAS: i64 = -2147483648;

fn is_integer(kind: &str) -> bool {
    kind == "int" || crate::parse::INTEGER_TYPES.contains(&kind)
}

fn is_narrow(kind: &str) -> bool {
    IntType::narrow(kind).is_some()
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
    /// #pragma std c89: C's conversions on assignment, and && and || giving 0 or 1.
    strict: bool,
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
fn narrowed(value: Node, from: &str, kind: IntType) -> Node {
    let value = if from == "float" { node!({"kind": "cast", "type": "int", "operand": value}) } else { value };
    if let Some(constant) = value.get("value").filter(|_| value.kind() == kinds::INT).and_then(Node::as_i64) {
        return int_node(kind.wrap(constant));
    }
    node!({"kind": "narrow", "type": kind.name(), "operand": value})
}

/// A value converted for a variable or parameter of a type: narrowed to a narrow type; an unsigned
/// value made a float for a float. Anything else keeps its type, as the VM's values do, except in
/// strict mode, where C's conversions on assignment apply: an int stored in a float becomes a float
/// and a float stored in an int is truncated.
fn converted(value: Node, from: &str, to: &str, strict: bool, used: &mut Vec<&'static str>) -> Node {
    if let Some(narrow) = IntType::narrow(to) {
        narrowed(value, from, narrow)
    } else if to == "float" && from == "unsigned int" {
        call(UTOF, vec![value], used)
    } else if strict && to == "float" && is_integer(from) {
        as_float(value)
    } else if strict && to == "unsigned int" && from == "float" {
        call(FTOU, vec![value], used)
    } else if strict && is_integer(to) && from == "float" {
        as_int(value)
    } else {
        value
    }
}

/// (int)value: a float constant truncated here, anything else by FTOI.
fn as_int(value: Node) -> Node {
    if value.kind() == kinds::FLOAT {
        if let Some(bits) = value["bits"].as_i64() {
            let float = f32::from_bits(bits as u32);
            if float.abs() < 2147483648.0 {
                return int_node(float as i64);
            }
        }
    }
    node!({"kind": "cast", "type": "int", "operand": value})
}

/// (float)value: an int constant a float holds exactly made one here, anything else by ITOF.
fn as_float(value: Node) -> Node {
    if let Some(constant) = literal(&value).filter(|_| value.get("unsigned").is_none()) {
        let constant = constant as i32;
        if constant.unsigned_abs() < 1 << 24 {
            return node!({"kind": "float", "bits": (constant as f32).to_bits() as i32 as i64});
        }
    }
    node!({"kind": "cast", "type": "float", "operand": value})
}

/// !!value: 0 or 1, as C's && and || give.
fn truth(value: Node) -> Node {
    node!({"kind": "unary", "op": "!", "operand": {"kind": "unary", "op": "!", "operand": value}})
}

/// Whether a function was written under #pragma std c89, where C's own semantics apply.
pub fn is_strict(function: &Node) -> bool {
    function["std"] == "c89"
}

const UDIV: &str = "__udiv";
const UMOD: &str = "__umod";
const USHR: &str = "__ushr";
const UTOF: &str = "__utof";
const FTOU: &str = "__ftou";

fn literal(node: &Node) -> Option<u32> {
    (node.kind() == kinds::INT).then(|| node["value"].as_i64()).flatten().map(|value| value as u32)
}

/// An expression lowered, and its type.
type Lowered = Result<(Node, String)>;

impl Types<'_> {
    /// The type of a variable named in code, and whether it is an array or pointer of that type.
    fn variable(&self, name: &str) -> Option<(String, bool)> {
        self.variables.get(name).cloned().or_else(|| self.globals.get(name).map(|kind| (kind.clone(), false)))
    }

    /// An expression lowered, and its type.
    fn expression(&mut self, node: Node) -> Lowered {
        self.typed(node, false)
    }

    /// An expression whose value is only tested (a condition): && and || need not give 0 or 1.
    fn condition(&mut self, node: Node) -> Lowered {
        self.typed(node, true)
    }

    /// The child under a key lowered in its place, and its type.
    fn child(&mut self, node: &mut Node, key: &str) -> Result<String> {
        let (lowered, kind) = self.expression(node.take(key))?;
        node[key] = lowered;
        Ok(kind)
    }

    // The passes recurse once for each level of the tree, so typed only dispatches: each kind is
    // lowered by a method of its own, and a debug build's frame of typed stays small.
    fn typed(&mut self, node: Node, condition: bool) -> Lowered {
        match node.kind() {
            kinds::INT | kinds::FLOAT | kinds::STRING | kinds::VAR | kinds::FIELD | kinds::DEREF => self.leaf(node),
            kinds::INDEX => self.index(node),
            kinds::AT => self.at(node),
            kinds::ADDR => self.address(node),
            kinds::ASSIGN => self.assign(node),
            kinds::COMPOUND => self.compound(node),
            kinds::UNARY => self.unary(node),
            kinds::BINARY => self.binary(node),
            kinds::LOGICAL => self.logical(node, condition),
            kinds::TERNARY => self.ternary(node),
            kinds::CAST => self.cast(node),
            kinds::COMMA => self.comma(node),
            kinds::CALL => self.call(node),
            kinds::BUILTIN => self.builtin(node),
            _ => Ok((node, "any".into())),
        }
    }

    /// What has no expression in it to lower: a constant, a variable, a field, *p.
    fn leaf(&mut self, node: Node) -> Lowered {
        let kind = match node.kind() {
            kinds::INT if node.get("unsigned").is_some() => "unsigned int".into(),
            kinds::INT => "int".into(),
            kinds::FLOAT => "float".into(),
            kinds::STRING => "string".into(),
            kinds::VAR => match self.variable(node.text("name")) {
                Some((_, true)) => "pointer".into(),
                Some((kind, false)) => kind,
                None => "int".into(),
            },
            // a slot of C's other types (composite.rs), of the type it says
            kinds::FIELD => node.text("type").to_string(),
            _ => self.variable(node["operand"].text("name")).map_or("any".into(), |(kind, _)| kind),
        };
        Ok((node, kind))
    }

    fn builtin(&mut self, mut node: Node) -> Lowered {
        self.child(&mut node, "operand")?;
        Ok((node, "float".into()))
    }

    fn index(&mut self, mut node: Node) -> Lowered {
        self.child(&mut node, "index")?;
        let kind = match node["type"].as_str() {
            Some(kind) => kind.to_string(),
            None => self.variable(node.text("name")).map_or("any".into(), |(kind, _)| kind),
        };
        Ok((node, kind))
    }

    fn at(&mut self, mut node: Node) -> Lowered {
        self.child(&mut node, "pointer")?;
        self.child(&mut node, "index")?;
        let kind = node.text("type").to_string();
        Ok((node, kind))
    }

    fn address(&mut self, mut node: Node) -> Lowered {
        self.child(&mut node, "target")?;
        Ok((node, "pointer".into()))
    }

    fn assign(&mut self, mut node: Node) -> Lowered {
        let to = self.child(&mut node, "target")?;
        let (value, from) = self.expression(node.take("value"))?;
        node["value"] = converted(value, &from, &to, self.strict, self.used);
        Ok((node, to))
    }

    fn unary(&mut self, mut node: Node) -> Lowered {
        let not = node.text("op") == "!";
        let operand = node.take("operand");
        let (operand, operand_kind) = if not { self.condition(operand)? } else { self.expression(operand)? };
        node["operand"] = operand;
        let result = if not { "int".to_string() } else { promoted(&operand_kind).to_string() };
        Ok((node, result))
    }

    fn logical(&mut self, mut node: Node, condition: bool) -> Lowered {
        let (left, _) = self.condition(node.take("left"))?;
        let (right, _) = self.typed(node.take("right"), condition)?;
        node["left"] = left;
        node["right"] = right;
        // the VM's && and || leave the deciding value; C's give 0 or 1
        Ok((if self.strict && !condition { truth(node) } else { node }, "int".into()))
    }

    fn ternary(&mut self, mut node: Node) -> Lowered {
        let (cond, _) = self.condition(node.take("cond"))?;
        let (then, then_kind) = self.expression(node.take("then"))?;
        let (otherwise, otherwise_kind) = self.expression(node.take("else"))?;
        let kind = common(&then_kind, &otherwise_kind);
        let float = |types: &mut Self, value: Node, from: &str| {
            if kind == "float" {
                converted(value, from, "float", types.strict, types.used)
            } else {
                value
            }
        };
        node["cond"] = cond;
        node["then"] = float(self, then, &then_kind);
        node["else"] = float(self, otherwise, &otherwise_kind);
        Ok((node, kind))
    }

    fn comma(&mut self, mut node: Node) -> Lowered {
        self.child(&mut node, "left")?;
        let kind = self.child(&mut node, "right")?;
        Ok((node, kind))
    }

    /// A call: its arguments converted to the function's parameters' types (or, in strict mode,
    /// a native's), and the type it returns.
    fn call(&mut self, mut node: Node) -> Lowered {
        let name = node.text("name").to_string();
        let params = match self.functions.get(&name) {
            Some((_, params)) => params.clone(),
            None if self.strict => native_params(self.found, &name),
            None => Vec::new(),
        };
        let mut args = Vec::new();
        for (at, arg) in node.take("args").into_list().into_iter().enumerate() {
            let (arg, from) = self.expression(arg)?;
            args.push(match params.get(at) {
                Some(to) => converted(arg, &from, to, self.strict, self.used),
                None => arg,
            });
        }
        node["args"] = Node::List(args);
        let returns = match self.functions.get(&name) {
            Some((returns, _)) => returns.clone(),
            None => self.found.natives.get(&name).map_or("any".into(), |native| native.text("returns").to_string()),
        };
        Ok((node, returns))
    }

    fn cast(&mut self, mut node: Node) -> Lowered {
        let (operand, from) = self.expression(node.take("operand"))?;
        let to = node.text("type").to_string();
        if let Some(narrow) = IntType::narrow(&to) {
            return Ok((narrowed(operand, &from, narrow), to));
        }
        let lowered = match (to.as_str(), from.as_str()) {
            ("float", "unsigned int") => call(UTOF, vec![operand], self.used),
            ("unsigned int", "float") => call(FTOU, vec![operand], self.used),
            // the bits are the value's already
            ("unsigned int", _) => operand,
            _ => {
                node["operand"] = operand;
                node
            }
        };
        Ok((lowered, to))
    }

    fn binary(&mut self, mut node: Node) -> Lowered {
        // x++ is the stepped value worked back to the old one: the variable's type
        let stepped = node.get("postfix").map(|_| node["left"]["target"].clone());
        let left = self.expression(node.take("left"))?;
        let right = self.expression(node.take("right"))?;
        let stepped = match stepped {
            Some(target) => Some(self.expression(target)?.1),
            None => None,
        };
        self.binary_of(node, left, right, stepped)
    }

    /// A binary operation of lowered operands, with their types (and for x++, x's).
    fn binary_of(
        &mut self,
        node: Node,
        left: (Node, String),
        right: (Node, String),
        stepped: Option<String>,
    ) -> Lowered {
        let ((left, left_kind), (right, right_kind)) = (left, right);
        let op = node.text("op").to_string();
        let postfix = stepped.is_some();
        let shift = op == "<<" || op == ">>";
        let kind = if shift { promoted(&left_kind).to_string() } else { common(&left_kind, &right_kind) };
        let compared = matches!(op.as_str(), "==" | "!=" | "<" | "<=" | ">" | ">=");
        let result = if compared { "int".to_string() } else { kind.clone() };
        let plain = |mut node: Node, left: Node, right: Node| {
            node["left"] = left;
            node["right"] = right;
            node
        };
        if let Some(stepped) = stepped {
            // for a narrow x the old value wraps back too (an unsigned char at 255 steps to 0, and
            // 0 - 1 is 255 again)
            if let Some(narrow) = IntType::narrow(&stepped) {
                return Ok((narrowed(plain(node, left, right), "int", narrow), stepped));
            }
        }
        if kind == "float" {
            // an unsigned operand of float arithmetic converts by its value
            let left = converted(left, &left_kind, "float", false, self.used);
            let right = converted(right, &right_kind, "float", false, self.used);
            return Ok((plain(node, left, right), result));
        }
        if kind != "unsigned int" {
            return Ok((plain(node, left, right), result));
        }
        let lowered = self.unsigned(&op, node, left, right);
        let lowered = if postfix { lowered.with("postfix", true) } else { lowered };
        Ok((lowered, result))
    }

    /// An operation on unsigned ints: division, remainder and right shift by the runtime's
    /// functions, comparisons of the values offset into the signed range, and constants folded.
    fn unsigned(&mut self, op: &str, mut node: Node, left: Node, right: Node) -> Node {
        let folded = literal(&left).zip(literal(&right));
        match op {
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
                    node["left"] = biased(left);
                    node["right"] = biased(right);
                    node
                }
            },
            _ => {
                node["left"] = left;
                node["right"] = right;
                node
            }
        }
    }

    /// target op= value: worked out in the target's type, and narrowed to it when stored.
    fn compound(&mut self, mut node: Node) -> Lowered {
        let (target, to) = self.expression(node.take("target"))?;
        let (value, from) = self.expression(node.take("value"))?;
        let op = node.text("op").to_string();
        let kind = if op == "<<" || op == ">>" { promoted(&to).to_string() } else { common(&to, &from) };
        if kind == "unsigned int" && matches!(op.as_str(), "/" | "%" | ">>") {
            // an unsigned quotient, remainder or shift is a call: target = op(target, value)
            if contains_effects(&target) {
                return Err(Error::invalid(
                    "an unsigned /=, %= or >>= on an element whose index has side effects is not supported",
                ));
            }
            let binary = node!({"kind": "binary", "op": op, "left": target.clone(), "right": value});
            let (operation, kind) = self.binary(binary)?;
            let stored = converted(operation, &kind, &to, self.strict, self.used);
            return Ok((node!({"kind": "assign", "target": target, "value": stored}), to));
        }
        if self.strict && to == "unsigned int" && kind == "float" {
            // the float result converts by its value
            if contains_effects(&target) {
                return Err(Error::invalid(
                    "an unsigned op= of a float on an element whose index has side effects is not supported",
                ));
            }
            let binary = node!({"kind": "binary", "op": op, "left": target.clone(), "right": value});
            let stored = call(FTOU, vec![binary], self.used);
            return Ok((node!({"kind": "assign", "target": target, "value": stored}), to));
        }
        node["target"] = target;
        node["value"] = value;
        if self.strict && is_integer(&to) && kind == "float" {
            node["convert"] = Node::from("int");
        }
        if is_narrow(&to) {
            node["narrow"] = Node::from(to.as_str());
        }
        Ok((node, to))
    }

    /// A statement, and those in it, lowered.
    fn statement(&mut self, mut node: Node) -> Result<Node> {
        match node.kind() {
            kinds::EXPR => {
                self.child(&mut node, "expr")?;
                Ok(node)
            }
            kinds::RETURN if !node["value"].is_null() => self.return_value(node),
            kinds::IF => self.conditional(node),
            kinds::WHILE => self.while_loop(node),
            kinds::SWITCH => self.switch(node),
            kinds::PRINT | kinds::NATIVE => self.arguments(node),
            _ => Ok(node),
        }
    }

    fn return_value(&mut self, mut node: Node) -> Result<Node> {
        let (value, from) = self.expression(node.take("value"))?;
        let to = self.returns.clone();
        node["value"] = converted(value, &from, &to, self.strict, self.used);
        Ok(node)
    }

    fn conditional(&mut self, mut node: Node) -> Result<Node> {
        let (cond, _) = self.condition(node.take("cond"))?;
        node["cond"] = cond;
        node["then"] = Node::List(self.statements(node.take("then").into_list())?);
        if node["else"].as_array().is_some() {
            node["else"] = Node::List(self.statements(node.take("else").into_list())?);
        }
        Ok(node)
    }

    fn while_loop(&mut self, mut node: Node) -> Result<Node> {
        let (cond, _) = self.condition(node.take("cond"))?;
        node["cond"] = cond;
        node["body"] = Node::List(self.statements(node.take("body").into_list())?);
        Ok(node)
    }

    fn switch(&mut self, mut node: Node) -> Result<Node> {
        self.child(&mut node, "value")?;
        let mut cases = Vec::new();
        for mut case in node.take("cases").into_list() {
            case["body"] = Node::List(self.statements(case.take("body").into_list())?);
            if !case["value"].is_null() {
                self.child(&mut case, "value")?;
            }
            cases.push(case);
        }
        node["cases"] = Node::List(cases);
        Ok(node)
    }

    /// A print's or a native call's arguments.
    fn arguments(&mut self, mut node: Node) -> Result<Node> {
        let mut args = Vec::new();
        for arg in node.take("args").into_list() {
            args.push(self.expression(arg)?.0);
        }
        node["args"] = Node::List(args);
        Ok(node)
    }

    fn statements(&mut self, nodes: Vec<Node>) -> Result<Vec<Node>> {
        nodes.into_iter().map(|node| self.statement(node)).collect()
    }
}

/// The types of the arguments a native's call passes (its result, which the call returns, left
/// out): a pointer for an out parameter, else the declared type.
fn native_params(found: &Context, name: &str) -> Vec<String> {
    let Some(declared) = found.natives.get(name) else {
        return Vec::new();
    };
    declared
        .list("params")
        .iter()
        .filter(|param| param["result"] != Node::Bool(true))
        .map(
            |param| {
                if param["out"] == Node::Bool(true) {
                    "pointer".to_string()
                } else {
                    param.text("type").to_string()
                }
            },
        )
        .collect()
}

/// Whether an expression assigns or calls.
fn contains_effects(node: &Node) -> bool {
    match node {
        Node::List(items) => items.iter().any(contains_effects),
        Node::Map(map) => {
            matches!(node.kind(), kinds::ASSIGN | kinds::COMPOUND | kinds::CALL) || map.values().any(contains_effects)
        }
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
pub fn integer_types(mut program: Node, kinds: &Map, found: &Context) -> Result<Node> {
    let unsigned_constants = found.constants.values().any(|constant| constant.get("unsigned").is_some());
    let used_anywhere = unsigned_constants
        || uses_integer_types(&program["functions"], kinds)
        || uses_integer_types(&program["globals"], kinds);
    let strict_anywhere = program.list("functions").iter().any(is_strict);
    if program.get("asm").is_some() || !(used_anywhere || strict_anywhere) {
        return Ok(program);
    }
    let globals: HashMap<String, String> = program
        .list("globals")
        .iter()
        .map(|global| (global.text("name").to_string(), base(global.text("type"), kinds)))
        .collect();
    let functions: HashMap<String, (String, Vec<String>)> = program
        .list("functions")
        .iter()
        .filter(|function| function.kind() == kinds::FUNCTION)
        .map(|function| {
            let params = function
                .list("params")
                .iter()
                .map(|param| if param.flag("array") { "pointer".into() } else { base(param.text("type"), kinds) })
                .collect();
            (function.text("name").to_string(), (base(function.text("returns"), kinds), params))
        })
        .collect();
    let mut used = Vec::new();
    let mut lowered = Vec::new();
    for mut function in program.take("functions").into_list() {
        if function.kind() != kinds::FUNCTION || !(used_anywhere || is_strict(&function)) {
            lowered.push(function);
            continue;
        }
        let mut variables = HashMap::new();
        let mut shadowed: HashSet<String> = globals.keys().cloned().collect();
        for param in function.list("params") {
            shadowed.insert(param.text("name").to_string());
            let array = param.flag("array");
            variables.insert(param.text("name").to_string(), (base(param.text("type"), kinds), array));
        }
        for local in function.list("locals") {
            shadowed.insert(local.text("name").to_string());
            let array = !local["size"].is_null() || local.flag("pointer");
            variables.insert(local.text("name").to_string(), (base(local.text("type"), kinds), array));
        }
        let mut types = Types {
            variables,
            globals: globals.clone(),
            functions: &functions,
            found,
            returns: base(function.text("returns"), kinds),
            used: &mut used,
            strict: is_strict(&function),
        };
        // constants as their values, so an unsigned one is typed as one
        let body = crate::sdk::with_constants(&function["body"], &found.constants, &shadowed);
        function["body"] = Node::List(types.statements(body.into_list())?);
        lowered.push(function);
    }
    if !used.is_empty() {
        lowered.extend(runtime(&mut used)?);
    }
    program["functions"] = Node::List(lowered);
    Ok(program)
}

/// The runtime's functions a call was made to, and those they call, under their hidden names.
fn runtime(used: &mut Vec<&'static str>) -> Result<Vec<Node>> {
    if used.contains(&UMOD) && !used.contains(&UDIV) {
        used.push(UDIV);
    }
    let runtime = crate::parse::parse(RUNTIME)?;
    let mut functions = Vec::new();
    for function in runtime.list("functions") {
        let name = function.text("name").replacen("c89_", "__", 1);
        if used.contains(&name.as_str()) {
            let mut function = function.clone().with("name", name).with("body", rename_calls(&function["body"]));
            function.remove("doc");
            functions.push(function);
        }
    }
    Ok(functions)
}

/// The runtime's calls of each other, by their hidden names.
fn rename_calls(node: &Node) -> Node {
    match node {
        Node::List(items) => Node::List(items.iter().map(rename_calls).collect()),
        Node::Map(map) => {
            let mut out = Map::with_capacity(map.len());
            for (key, value) in map.entries() {
                let value = if key == "name" && node.kind() == kinds::CALL {
                    Node::from(node.text("name").replacen("c89_", "__", 1))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn float(value: f32) -> Node {
        node!({"kind": "float", "bits": value.to_bits() as i32 as i64})
    }

    #[test]
    fn a_float_constant_converts_to_int_here_when_an_int_holds_it() {
        assert_eq!(as_int(float(2.75)), int_node(2));
        assert_eq!(as_int(float(-2.75)), int_node(-2));
        assert_eq!(as_int(float(2147483520.0)), int_node(2147483520));
        // 2^31 and NaN are left to FTOI, as is anything but a float constant
        for value in [float(2147483648.0), float(f32::NAN), node!({"kind": "var", "name": "x"})] {
            assert_eq!(as_int(value.clone()), node!({"kind": "cast", "type": "int", "operand": value}));
        }
    }

    #[test]
    fn an_int_constant_converts_to_float_here_when_a_float_holds_it_exactly() {
        assert_eq!(as_float(int_node(3)), float(3.0));
        assert_eq!(as_float(int_node(-16777215)), float(-16777215.0));
        // 2^24 and beyond may round, an unsigned constant is a large one, and the rest are ITOF's
        let unsigned = node!({"kind": "int", "value": -1, "unsigned": true});
        for value in [int_node(16777216), unsigned, node!({"kind": "var", "name": "x"})] {
            assert_eq!(as_float(value.clone()), node!({"kind": "cast", "type": "float", "operand": value}));
        }
    }
}
