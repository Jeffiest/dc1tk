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

use std::collections::HashSet;

use crate::node;
use crate::node::Node;
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
