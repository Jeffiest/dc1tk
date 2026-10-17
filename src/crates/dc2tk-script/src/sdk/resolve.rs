//! Resolving, in the compile direction: the readable source to what the VM runs (helpers
//! expanded, natives by their canonical names, their results and calls in expressions hoisted).

use super::*;

/// A tree with the variables that name constants replaced by the constants' values, except the
/// shadowed ones.
pub fn with_constants(node: &Node, constants: &IndexMap<String, Node>, shadowed: &HashSet<String>) -> Node {
    match node {
        Node::List(items) => Node::List(items.iter().map(|item| with_constants(item, constants, shadowed)).collect()),
        Node::Map(map) => {
            if node.kind() == kinds::VAR {
                let name = node.text("name");
                if let Some(value) = constants.get(name).filter(|_| !shadowed.contains(name)) {
                    return value.clone();
                }
            }
            Node::Map(
                map.entries().map(|(key, value)| (key.clone(), with_constants(value, constants, shadowed))).collect(),
            )
        }
        other => other.clone(),
    }
}

fn variable_names(function: &Node, global_names: &HashSet<String>) -> HashSet<String> {
    let mut found: HashSet<String> =
        function.list("params").iter().map(|param| param.text("name").to_string()).collect();
    found.extend(function.list("locals").iter().map(|declared| declared.text("name").to_string()));
    found.extend(global_names.iter().cloned());
    found
}

pub fn is_native(name: &str, found: &Context) -> bool {
    found.natives.contains_key(name) || found.originals.contains(name) || ext_number(name).is_some()
}

pub fn has_result(declared: &Node) -> bool {
    declared.list("params").last().is_some_and(|param| param["result"].is_set())
}

/// A native call as the VM makes it: its canonical name and group base.
fn canonical_native(name: &str, at: &Node, args: Node, found: &Context) -> Node {
    if let Some(declared) = found.natives.get(name) {
        let base = if at.is_null() {
            declared["base"].clone()
        } else if at == "direct" {
            Node::Null
        } else {
            at.clone()
        };
        return node!({"kind": "native", "name": declared["canonical"].clone(), "base": base, "args": args});
    }
    let base = if at.is_null() || at == "direct" { Node::Null } else { at.clone() };
    node!({"kind": "native", "name": name, "base": base, "args": args})
}

fn address_of(target: Node) -> Node {
    if target.kind() == kinds::DEREF {
        return target["operand"].clone();
    }
    node!({"kind": "addr", "target": target})
}

fn substituted(node: &Node, bindings: &HashMap<String, Node>, outs: &HashSet<String>) -> Node {
    match node {
        Node::List(items) => Node::List(items.iter().map(|item| substituted(item, bindings, outs)).collect()),
        Node::Map(map) => {
            if node.kind() == kinds::VAR {
                if let Some(bound) = bindings.get(node.text("name")) {
                    return bound.clone();
                }
            }
            if node.kind() == kinds::DEREF
                && node["operand"].kind() == kinds::VAR
                && outs.contains(node["operand"].text("name"))
            {
                return bindings[node["operand"].text("name")]["target"].clone();
            }
            Node::Map(
                map.entries()
                    .filter(|(key, _)| key != "line" && key != "start")
                    .map(|(key, value)| (key.clone(), substituted(value, bindings, outs)))
                    .collect(),
            )
        }
        other => other.clone(),
    }
}

fn expanded_inline(inline: &Node, args: &[Node]) -> Result<Vec<Node>> {
    let params = inline.list("params");
    let name = inline.text("name");
    if args.len() != params.len() {
        return Err(Error::invalid(format!("{} takes {} arguments", name, params.len())));
    }
    let bindings: HashMap<String, Node> =
        params.iter().zip(args).map(|(param, arg)| (param.text("name").to_string(), arg.clone())).collect();
    let outs: HashSet<String> =
        params.iter().filter(|param| param["out"].is_set()).map(|param| param.text("name").to_string()).collect();
    for param in params.iter().filter(|param| param["out"].is_set()) {
        let out = param.text("name");
        if bindings[out].kind() != kinds::ADDR {
            return Err(Error::invalid(format!("{name}: pass &variable for pointer parameter {out}")));
        }
    }
    Ok(substituted(&inline["body"], &bindings, &outs).as_array().cloned().unwrap_or_default())
}

fn native_values(node: &Node, found: &Context, out: &mut Vec<String>) {
    match node {
        Node::List(items) => items.iter().for_each(|item| native_values(item, found, out)),
        Node::Map(map) => {
            if node.kind() == kinds::CALL && is_native(node.text("name"), found) {
                out.push(node.text("name").to_string());
            }
            map.values().for_each(|value| native_values(value, found, out));
        }
        _ => {}
    }
}

fn checked(node: &Node, found: &Context) -> Result<Node> {
    let mut misplaced = Vec::new();
    native_values(node, found, &mut misplaced);
    if let Some(first) = misplaced.first() {
        return Err(Error::invalid(format!(
            "native {first} has no value; call it as a statement or assign its result to a variable"
        )));
    }
    Ok(node.clone())
}

/// Whether an expression calls a native with a result, which hoisting runs before the expression.
fn calls_native(node: &Node, found: &Context) -> bool {
    match node {
        Node::List(items) => items.iter().any(|item| calls_native(item, found)),
        Node::Map(map) => {
            (node.kind() == kinds::CALL && is_native(node.text("name"), found))
                || map.values().any(|value| calls_native(value, found))
        }
        _ => false,
    }
}

fn hidden_value(temps: &mut Vec<String>) -> String {
    let value = format!("__value_{}", temps.len());
    temps.push(value.clone());
    value
}

fn assigned(name: &str, value: Node) -> Node {
    node!({"kind": "expr", "expr": {"kind": "assign", "target": {"kind": "var", "name": name}, "value": value}})
}

fn truth(value: Node) -> Node {
    node!({"kind": "unary", "op": "!", "operand": {"kind": "unary", "op": "!", "operand": value}})
}

/// Whether an operand C evaluates only some of the time holds a native call, which is then called
/// only when C would evaluate it (sequenced).
fn is_sequenced(node: &Node, found: &Context) -> bool {
    match node.kind() {
        kinds::LOGICAL | kinds::COMMA => calls_native(&node["right"], found),
        kinds::TERNARY => calls_native(&node["then"], found) || calls_native(&node["else"], found),
        _ => false,
    }
}

/// An operand C evaluates only some of the time, holding a native call: the
/// operands it depends on are worked out first into a hidden local, and the native is called in an
/// if, so it runs only when C would evaluate it. a && b is t = !!a; if (t) { t = !!b; } and a || b
/// the same with if (!t); c ? x : y is if (c) { t = x; } else { t = y; }; in a, b the a is a
/// statement of its own before b's natives. The node is one is_sequenced takes. Each form is
/// lowered by a function of its own, so the frames of a deep expression stay small.
fn sequenced(node: &Node, found: &Context, temps: &mut Vec<String>, before: &mut Vec<Node>) -> Result<Node> {
    match node.kind() {
        kinds::LOGICAL => sequenced_logical(node, found, temps, before),
        kinds::TERNARY => sequenced_ternary(node, found, temps, before),
        _ => sequenced_comma(node, found, temps, before),
    }
}

/// One branch of a sequenced operand: its natives called, then its value (made into what the
/// operator gives) put in the hidden local.
fn sequenced_branch(
    part: &Node,
    found: &Context,
    temps: &mut Vec<String>,
    value: &str,
    made: fn(Node) -> Node,
) -> Result<Vec<Node>> {
    let mut inner = Vec::new();
    let part = hoisted(part, found, temps, &mut inner)?;
    inner.push(assigned(value, made(part)));
    Ok(inner)
}

fn sequenced_logical(node: &Node, found: &Context, temps: &mut Vec<String>, before: &mut Vec<Node>) -> Result<Node> {
    let left = hoisted(&node["left"], found, temps, before)?;
    let value = hidden_value(temps);
    before.push(assigned(&value, truth(left)));
    let then = sequenced_branch(&node["right"], found, temps, &value, truth)?;
    let tested = node!({"kind": "var", "name": value.clone()});
    let cond = if node["op"] == "&&" { tested } else { node!({"kind": "unary", "op": "!", "operand": tested}) };
    before.push(node!({"kind": "if", "cond": cond, "then": then, "else": null}));
    Ok(node!({"kind": "var", "name": value}))
}

fn sequenced_ternary(node: &Node, found: &Context, temps: &mut Vec<String>, before: &mut Vec<Node>) -> Result<Node> {
    let cond = hoisted(&node["cond"], found, temps, before)?;
    let value = hidden_value(temps);
    let then = sequenced_branch(&node["then"], found, temps, &value, |part| part)?;
    let otherwise = sequenced_branch(&node["else"], found, temps, &value, |part| part)?;
    before.push(node!({"kind": "if", "cond": cond, "then": then, "else": otherwise}));
    Ok(node!({"kind": "var", "name": value}))
}

fn sequenced_comma(node: &Node, found: &Context, temps: &mut Vec<String>, before: &mut Vec<Node>) -> Result<Node> {
    let left = hoisted(&node["left"], found, temps, before)?;
    before.push(node!({"kind": "expr", "expr": left}));
    hoisted(&node["right"], found, temps, before)
}

/// Native calls inside an expression. The VM's natives push no value, so each one is called
/// first, into a hidden local (__value_N), and the expression reads that local. The calls are
/// added to before, in the order the expression would make them, except those in an operand C
/// evaluates only some of the time (sequenced). This only dispatches, so the frames of a deep
/// expression stay small.
fn hoisted(node: &Node, found: &Context, temps: &mut Vec<String>, before: &mut Vec<Node>) -> Result<Node> {
    match node {
        _ if is_sequenced(node, found) => sequenced(node, found, temps, before),
        Node::List(items) => hoisted_list(items, found, temps, before),
        Node::Map(map) if node.kind() == kinds::CALL => hoisted_call(node, map, found, temps, before),
        Node::Map(map) => hoisted_map(map, found, temps, before),
        other => Ok(other.clone()),
    }
}

fn hoisted_list(items: &[Node], found: &Context, temps: &mut Vec<String>, before: &mut Vec<Node>) -> Result<Node> {
    Ok(Node::List(items.iter().map(|item| hoisted(item, found, temps, before)).collect::<Result<_>>()?))
}

fn hoisted_map(map: &Map, found: &Context, temps: &mut Vec<String>, before: &mut Vec<Node>) -> Result<Node> {
    let mut out = Map::with_capacity(map.len());
    for (key, value) in map.entries() {
        out.insert(key.clone(), hoisted(value, found, temps, before)?);
    }
    Ok(Node::Map(out))
}

/// A call in an expression: a native's is called first into a hidden local the expression then
/// reads; a helper's is refused (helpers are statements); a function's stays.
fn hoisted_call(
    node: &Node,
    map: &Map,
    found: &Context,
    temps: &mut Vec<String>,
    before: &mut Vec<Node>,
) -> Result<Node> {
    let name = node.text("name");
    if found.inlines.contains_key(name) {
        return Err(Error::invalid(format!("helper {name} is a statement; call it on a line of its own")));
    }
    if !is_native(name, found) {
        return hoisted_map(map, found, temps, before);
    }
    if !found.natives.get(name).is_some_and(has_result) {
        return Err(Error::invalid(format!("native {name} returns nothing, so it has no value to use")));
    }
    let mut args = hoisted(&node["args"], found, temps, before)?;
    let value = hidden_value(temps);
    let target = node!({"kind": "addr", "target": {"kind": "var", "name": value.clone()}});
    if let Some(items) = args.as_array_mut() {
        items.push(target);
    }
    before.push(canonical_native(name, &Node::Null, args, found));
    Ok(node!({"kind": "var", "name": value}))
}

fn resolved_statement(
    node: &Node,
    found: &Context,
    temps: &mut Vec<String>,
    before: &mut Vec<Node>,
) -> Result<Vec<Node>> {
    let kind = node.kind();
    let at = node.get("at").cloned().unwrap_or(Node::Null);
    let mut plain = node.clone();
    if let Some(map) = plain.as_object_mut() {
        map.remove("at");
    }
    let expression =
        if kind == kinds::EXPR { node.get("expr").filter(|expression| !expression.is_null()) } else { None };
    if let Some(expression) = expression.filter(|expression| expression.kind() == kinds::CALL) {
        let name = expression.text("name");
        if let Some(inline) = found.inlines.get(name) {
            if !at.is_null() {
                return Err(Error::invalid(format!("AT and DIRECT apply to native calls, not to helper {name}")));
            }
            let args = hoisted(&expression["args"], found, temps, before)?;
            return expanded_inline(inline, args.as_array().map(Vec::as_slice).unwrap_or(&[]));
        }
        if is_native(name, found) {
            let args = hoisted(&expression["args"], found, temps, before)?;
            return Ok(vec![canonical_native(name, &at, args, found)]);
        }
    }
    if let Some(expression) = expression {
        // x = native(); and x = FLOAT_AS_INT(native());: the native writes its result into x, as it is
        let value = match &expression["value"] {
            as_is if as_is.kind() == kinds::AS_IS => &as_is["operand"],
            value => value,
        };
        if expression.kind() == kinds::ASSIGN && value.kind() == kinds::CALL && is_native(value.text("name"), found) {
            let name = value.text("name");
            if !found.natives.get(name).is_some_and(has_result) {
                return Err(Error::invalid(format!(
                    "native {name} returns nothing; pass &variable instead of assigning"
                )));
            }
            let mut args = hoisted(&value["args"], found, temps, before)?;
            let target = address_of(hoisted(&expression["target"], found, temps, before)?);
            if let Some(items) = args.as_array_mut() {
                items.push(target);
            }
            return Ok(vec![canonical_native(name, &at, args, found)]);
        }
    }
    if !at.is_null() {
        return Err(Error::invalid("AT and DIRECT apply only to native calls"));
    }
    match kind {
        kinds::NATIVE => {
            let args = hoisted(&node["args"], found, temps, before)?;
            Ok(vec![canonical_native(node.text("name"), &node["base"], args, found)])
        }
        kinds::IF => {
            let otherwise = match node["else"].as_array() {
                Some(items) => statements(items, found, temps)?,
                None => Node::Null,
            };
            let cond = hoisted(&node["cond"], found, temps, before)?;
            let then = statements(node.list("then"), found, temps)?;
            Ok(vec![plain.with("cond", cond).with("then", then).with("else", otherwise)])
        }
        kinds::WHILE => {
            let mut inner = Vec::new();
            let cond = hoisted(&node["cond"], found, temps, &mut inner)?;
            let body = statements(node.list("body"), found, temps)?;
            if inner.is_empty() {
                return Ok(vec![plain.with("cond", cond).with("body", body)]);
            }
            // The condition's native calls run at the top of every iteration.
            let leave = node!({"kind": "if", "cond": {"kind": "unary", "op": "!", "operand": cond}, "then": [{"kind": "break"}], "else": null});
            inner.push(leave);
            inner.extend(body.as_array().cloned().unwrap_or_default());
            Ok(vec![plain.with("cond", node!({"kind": "int", "value": 1})).with("body", inner)])
        }
        kinds::SWITCH => {
            let mut cases = Vec::new();
            for case in node.list("cases") {
                let value = if case["value"].is_null() { Node::Null } else { checked(&case["value"], found)? };
                let body = statements(case.list("body"), found, temps)?;
                cases.push(case.clone().with("value", value).with("body", body));
            }
            let value = hoisted(&node["value"], found, temps, before)?;
            Ok(vec![plain.with("value", value).with("cases", cases)])
        }
        _ => Ok(vec![hoisted(&plain, found, temps, before)?]),
    }
}

fn statement(node: &Node, found: &Context, temps: &mut Vec<String>) -> Result<Vec<Node>> {
    let mut before = Vec::new();
    let resolved = resolved_statement(node, found, temps, &mut before)?;
    let mut result = before;
    result.extend(resolved);
    if let Some(note) = node.get("note").filter(|note| note.is_set()) {
        if let Some(first) = result.first_mut() {
            if !first.get("note").is_some_and(Node::is_set) {
                *first = std::mem::take(first).with("note", note.clone());
            }
        }
    }
    let line = &node["line"];
    if line.is_null() {
        return Ok(result);
    }
    Ok(result
        .into_iter()
        .map(|item| if item.get("line").is_some() { item } else { item.with("line", line.clone()) })
        .collect())
}

/// Statements resolved to what the VM runs: helpers expanded, natives by their canonical names,
/// native results and calls in expressions hoisted into statements of their own.
pub fn statements(nodes: &[Node], found: &Context, temps: &mut Vec<String>) -> Result<Node> {
    let mut out = Vec::new();
    for node in nodes {
        let items = statement(node, found, temps).map_err(|error| error.at(&node["line"]))?;
        out.extend(items);
    }
    Ok(Node::List(out))
}

/// Locals whose array size names a constant get its value.
fn sized_locals(function: &Node, found: &Context) -> Result<Vec<Node>> {
    let mut sized = Vec::new();
    for declared in function.list("locals") {
        if let Some(size) = declared["size"].as_str() {
            let value = found.constants.get(size).filter(|value| value.is_set() && value.kind() == kinds::INT);
            let Some(value) = value else {
                return Err(Error::invalid(format!(
                    "{}: array size {} is not a known integer constant",
                    function.text("name"),
                    size
                )));
            };
            sized.push(declared.clone().with("size", value["value"].clone()));
        } else {
            sized.push(declared.clone());
        }
    }
    Ok(sized)
}

const BASE_TYPES: [&str; 3] = ["int", "float", "string"];

/// A type a variable can have as it is: int, float, string, or one of C's other integer types.
fn is_base_type(kind: &str) -> bool {
    BASE_TYPES.contains(&kind) || crate::parse::INTEGER_TYPES.contains(&kind)
}

/// A variable declared with a value kind (bgm_file) or an enum (enum se_bank) has the kind's base
/// type, int for an enum; the kind stays on it, for the decompiler's types.
fn typed(variable: &Node, kinds: &Map) -> Result<Node> {
    let kind = variable.text("type");
    // void *p: a pointer parameter the function never reads through
    if is_base_type(kind) || (kind == "void" && variable["array"].is_set()) {
        return Ok(variable.clone());
    }
    let base = if kind.starts_with("enum ") { Some("int") } else { kinds.get(kind).and_then(Node::as_str) };
    match base {
        Some(base) if is_base_type(base) => Ok(variable.clone().with("type", base).with("kind", kind)),
        _ => Err(Error::invalid(format!("{}: unknown type {}", variable.text("name"), kind))),
    }
}

/// The program as the VM runs it, ready for compile::to_asm.
pub fn resolve(program: &Node, tables: &BTreeMap<String, Table>) -> Result<Node> {
    if program.get("asm").is_some() {
        return Ok(program.clone());
    }
    let declarations = program.get("declarations").map(Declarations::of).unwrap_or_default();
    let found = context(&declarations, program.text("natives"), tables)?;
    let program = crate::lower::initialised_globals(program.clone(), &found)?;
    let program = crate::composite::lower(&program, &declarations.kinds, &found)?;
    let program = &crate::lower::integer_types(program, &declarations.kinds, &found)?;
    let global_names: HashSet<String> =
        program.list("globals").iter().map(|declared| declared.text("name").to_string()).collect();
    let mut functions = Vec::new();
    for function in program.list("functions") {
        let mut function = function.clone();
        if function.kind() == kinds::FUNCTION {
            let params: Vec<Node> =
                function.list("params").iter().map(|param| typed(param, &declarations.kinds)).collect::<Result<_>>()?;
            let locals: Vec<Node> = function
                .list("locals")
                .iter()
                .map(|declared| typed(declared, &declarations.kinds))
                .collect::<Result<_>>()?;
            function = function.with("params", params).with("locals", locals);
            let body = with_constants(&function["body"], &found.constants, &variable_names(&function, &global_names));
            let mut temps = Vec::new();
            let body = statements(body.as_array().map(Vec::as_slice).unwrap_or(&[]), &found, &mut temps)?;
            let mut locals = sized_locals(&function, &found)?;
            for name in temps {
                locals.push(node!({"name": name, "type": "int", "size": null}));
            }
            function = function.with("body", body).with("locals", locals);
        }
        functions.push(function);
    }
    let mut programs = Vec::new();
    for entry in program.list("programs") {
        let number = match &entry["no"] {
            Node::Str(name) => match found.programs.get(name) {
                Some(number) => Node::Int(*number),
                None => return Err(Error::invalid(format!("unknown program name {name}"))),
            },
            other => other.clone(),
        };
        programs.push(entry.clone().with("no", number));
    }
    Ok(program.clone().with("functions", functions).with("programs", programs))
}
