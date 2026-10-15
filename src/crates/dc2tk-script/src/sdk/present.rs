//! Presenting, in the decompile direction: the VM's calls back into the readable source.

use super::*;

/// What a helper's parameter matches: an out-parameter only an address, an "any" anything, an in
/// parameter anything but an address.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Param {
    Out,
    Any,
    In,
}

fn matches(pattern: &Node, node: &Node, params: &HashMap<String, Param>, bindings: &mut HashMap<String, Node>) -> bool {
    match pattern {
        Node::List(items) => {
            let Some(others) = node.as_array() else {
                return false;
            };
            items.len() == others.len()
                && items.iter().zip(others).all(|(item, other)| matches(item, other, params, bindings))
        }
        Node::Map(map) => {
            let Some(other) = node.as_object() else {
                return false;
            };
            if pattern.kind() == kinds::VAR {
                if let Some(param) = params.get(pattern.text("name")) {
                    if *param == Param::Out && node.get("kind").and_then(Node::as_str) != Some("addr") {
                        return false;
                    }
                    if *param == Param::In && node.get("kind").and_then(Node::as_str) == Some("addr") {
                        return false;
                    }
                    return bound(pattern.text("name"), node, bindings);
                }
            }
            if pattern.kind() == kinds::DEREF
                && pattern["operand"].kind() == kinds::VAR
                && params.get(pattern["operand"].text("name")) == Some(&Param::Out)
            {
                if !matches!(node.get("kind").and_then(Node::as_str), Some("var" | "index")) {
                    return false;
                }
                return bound(
                    pattern["operand"].text("name"),
                    &node!({"kind": "addr", "target": node.clone()}),
                    bindings,
                );
            }
            // the same keys, line and start aside
            let counted = |map: &Map| map.keys().filter(|key| *key != "line" && *key != "start").count();
            if counted(map) != counted(other)
                || !map.keys().filter(|key| *key != "line" && *key != "start").all(|key| other.contains_key(key))
            {
                return false;
            }
            map.iter()
                .filter(|(key, _)| *key != "line" && *key != "start")
                .all(|(key, value)| matches(value, other.get(key).unwrap(), params, bindings))
        }
        other => other.same_value(node),
    }
}

fn bound(name: &str, node: &Node, bindings: &mut HashMap<String, Node>) -> bool {
    match bindings.get(name) {
        Some(known) => known.same_value(node),
        None => {
            bindings.insert(name.to_string(), node.clone());
            true
        }
    }
}

/// The helpers, longest body first (then by name), as folding tries them.
fn ordered_inlines(found: &Context) -> Vec<&Node> {
    let mut ranked: Vec<(i64, &String)> =
        found.inlines.iter().map(|(name, inline)| (-(inline.list("body").len() as i64), name)).collect();
    ranked.sort();
    ranked.into_iter().map(|(_, name)| &found.inlines[name]).collect()
}

fn helper_call(inline: &Node, nodes: &[Node], position: usize) -> Option<(Node, Vec<Node>)> {
    let body = inline.list("body");
    let params: HashMap<String, Param> = inline
        .list("params")
        .iter()
        .map(|param| {
            let kind = if param["out"].is_set() {
                Param::Out
            } else if param["type"] == "any" {
                Param::Any
            } else {
                Param::In
            };
            (param.text("name").to_string(), kind)
        })
        .collect();
    let mut bindings = HashMap::new();
    let window = &nodes[position..(position + body.len()).min(nodes.len())];
    let matched = body.len() == window.len()
        && body.iter().zip(window).all(|(pattern, node)| matches(pattern, node, &params, &mut bindings));
    if !matched || bindings.len() != params.len() {
        return None;
    }
    let args: Vec<Node> = inline.list("params").iter().map(|param| bindings[param.text("name")].clone()).collect();
    let call = node!({"kind": "expr", "expr": {"kind": "call", "name": inline["name"].clone(), "args": args}});
    Some((call, inline.list("params").to_vec()))
}

/// Statements with runs that a helper expands to folded back into calls of the helper, choosing
/// the fewest statements (then the fewest native calls).
fn folded(nodes: &[Node], found: &Context, inlines: &[&Node], pointers: &HashSet<String>) -> Vec<Node> {
    let mut best: Vec<(usize, usize)> = vec![(0, 0); nodes.len() + 1];
    let mut choice: Vec<Option<(Node, Vec<Node>, usize)>> = vec![None; nodes.len()];
    // a helper can only start where its first statement's kind (and native) is
    let first: Vec<Option<(&str, &Node)>> = inlines
        .iter()
        .map(|inline| {
            let pattern = inline.list("body").first()?;
            let kind = pattern.get("kind")?.as_str()?;
            (!matches!(kind, "var" | "deref")).then(|| (kind, &pattern["name"]))
        })
        .collect();
    for position in (0..nodes.len()).rev() {
        let (statements_after, natives_after) = best[position + 1];
        best[position] = (statements_after + 1, natives_after + usize::from(nodes[position].kind() == kinds::NATIVE));
        let here = &nodes[position];
        for (inline, first) in inlines.iter().zip(&first) {
            if let Some((kind, name)) = first {
                if here.get("kind").and_then(Node::as_str) != Some(kind)
                    || (*kind == kinds::NATIVE && !name.same_value(&here["name"]))
                {
                    continue;
                }
            }
            let size = inline.list("body").len();
            if position + size > nodes.len() {
                continue;
            }
            let cost = (best[position + size].0 + 1, best[position + size].1);
            if cost >= best[position] {
                continue;
            }
            if let Some((call, params)) = helper_call(inline, nodes, position) {
                best[position] = cost;
                choice[position] = Some((call, params, size));
            }
        }
    }
    let mut out = Vec::new();
    let mut position = 0;
    while position < nodes.len() {
        match choice[position].take() {
            None => {
                out.push(nested(&nodes[position], found, inlines, pointers));
                position += 1;
            }
            Some((call, params, size)) => {
                let args = named_args(call["expr"].list("args"), &params, found);
                let expr = call["expr"].clone().with("args", args);
                let mut call = call.with("expr", expr);
                let comments: Vec<String> = nodes[position..position + size]
                    .iter()
                    .filter_map(|node| node.get("comment").filter(|comment| comment.is_set()).map(Node::plain_text))
                    .collect();
                if !comments.is_empty() {
                    call = call.with("comment", comments.join("; "));
                }
                out.push(call);
                position += size;
            }
        }
    }
    out
}

fn nested(node: &Node, found: &Context, inlines: &[&Node], pointers: &HashSet<String>) -> Node {
    match node.kind() {
        kinds::IF => {
            let otherwise = match node["else"].as_array() {
                Some(items) => Node::List(folded(items, found, inlines, pointers)),
                None => Node::Null,
            };
            let then = folded(node.list("then"), found, inlines, pointers);
            node.clone().with("then", then).with("else", otherwise)
        }
        kinds::WHILE => {
            let body = folded(node.list("body"), found, inlines, pointers);
            node.clone().with("body", body)
        }
        kinds::NATIVE => readable_native(node, found, pointers),
        _ => node.clone(),
    }
}

fn result_target(arg: &Node, pointers: &HashSet<String>) -> Option<Node> {
    if arg.kind() == kinds::ADDR && matches!(arg["target"].kind(), kinds::VAR | kinds::INDEX) {
        return Some(arg["target"].clone());
    }
    if arg.kind() == kinds::VAR && pointers.contains(arg.text("name")) {
        return Some(node!({"kind": "deref", "operand": arg.clone()}));
    }
    None
}

/// The members of an enum and of its sub-enums (name_*), the first name of a value winning.
pub fn enum_values(name: &str, found: &Context) -> IndexMap<i64, String> {
    let mut values = IndexMap::new();
    let mut enums: Vec<(&String, &IndexMap<i64, String>)> = found.enums.iter().collect();
    enums.sort_by(|a, b| a.0.cmp(b.0));
    let prefix = format!("{name}_");
    for (enum_name, members) in enums {
        if enum_name == name || enum_name.starts_with(&prefix) {
            for (value, constant) in members {
                values.entry(*value).or_insert_with(|| constant.clone());
            }
        }
    }
    values
}

/// A number written as the enum's constant for it (also inside | + - * /).
pub fn named_literal(node: &Node, enum_name: &str, found: &Context) -> Node {
    let names = enum_values(enum_name, found);
    if node.kind() == kinds::INT {
        if let Some(name) = node["value"].as_i64().and_then(|value| names.get(&value)) {
            return node!({"kind": "var", "name": name.clone()});
        }
    }
    if node.kind() == kinds::BINARY && matches!(node.text("op"), "|" | "+" | "-" | "*" | "/") {
        let left = named_literal(&node["left"], enum_name, found);
        let right = named_literal(&node["right"], enum_name, found);
        return node.clone().with("left", left).with("right", right);
    }
    node.clone()
}

/// Arguments written with the constants of the enum (or kind) each parameter takes.
fn named_args(args: &[Node], params: &[Node], found: &Context) -> Vec<Node> {
    args.iter()
        .enumerate()
        .map(|(position, arg)| {
            let param = params.get(position);
            let kind = param.and_then(|param| {
                param
                    .get("enum")
                    .filter(|value| value.is_set())
                    .or_else(|| param.get("kind").filter(|value| value.is_set()))
            });
            let enumeration = param.and_then(|param| param.get("enum")).filter(|value| value.is_set());
            match (kind, enumeration) {
                // an address where the native takes an enum (a call written for an older native
                // table): C needs the conversion written out; it compiles to nothing
                (_, Some(enumeration)) if arg.kind() == kinds::ADDR => {
                    node!({"kind": "cast", "type": format!("enum {}", enumeration.plain_text()), "operand": arg.clone()})
                }
                (Some(kind), _) => named_literal(arg, &kind.plain_text(), found),
                (None, _) => arg.clone(),
            }
        })
        .collect()
}

fn named_floats(node: &Node, found: &Context) -> Node {
    match node {
        Node::List(items) => Node::List(items.iter().map(|item| named_floats(item, found)).collect()),
        Node::Map(map) => {
            if node.kind() == kinds::FLOAT {
                if let Some(name) = node["bits"].as_i64().and_then(|bits| found.float_names.get(&bits)) {
                    return node!({"kind": "var", "name": name.clone()});
                }
            }
            Node::Map(map.entries().map(|(key, value)| (key.clone(), named_floats(value, found))).collect())
        }
        other => other.clone(),
    }
}

/// A native call by its SDK name, with named arguments and its result as an assignment.
fn readable_native(node: &Node, found: &Context, pointers: &HashSet<String>) -> Node {
    let Some(declared) = found.by_original.get(node.text("name")) else {
        return node.clone().with("result", Node::Null);
    };
    let params = declared.list("params");
    let args = named_args(node.list("args"), params, found);
    let base = if node["base"].same_value(&declared["base"]) {
        Node::Null
    } else if node["base"].is_null() {
        Node::from("direct")
    } else {
        node["base"].clone()
    };
    let mut args = args;
    let mut result = Node::Null;
    if has_result(declared) && args.len() == params.len() {
        if let Some(target) = result_target(args.last().unwrap(), pointers) {
            result = target;
            args.pop();
        }
    }
    node.clone().with("args", args).with("name", declared["name"].clone()).with("base", base).with("result", result)
}

/// Kinds whose constant, when a native is called with one, names what the native's result is
/// about: get_flag(FLAG_GRIFFON_DEFEATED) reads griffon_defeated, get_chara_pos(CHARA_PLAYER, ...)
/// the player's x, y and z.
fn subject_prefix(kind: &str) -> Option<&'static str> {
    match kind {
        "flag" => Some("FLAG_"),
        "chara_slot" => Some("CHARA_"),
        _ => None,
    }
}

/// What a vector getter reads (home for get_home_pos), as get_([a-z][a-z_]*?)_(pos|rot|vec|dir|scale)
/// matches it.
fn vector_subject(name: &str) -> Option<&str> {
    let rest = name.strip_prefix("get_")?;
    let (thing, part) = rest.rsplit_once('_')?;
    let thing_ok = thing.starts_with(|c: char| c.is_ascii_lowercase())
        && thing.bytes().all(|b| b.is_ascii_lowercase() || b == b'_');
    (thing_ok && ["pos", "rot", "vec", "dir", "scale"].contains(&part)).then_some(thing)
}

fn call_subject(node: &Node, declared: &Node, found: &Context) -> Option<(String, String)> {
    let params = declared.list("params");
    for (position, arg) in node.list("args").iter().enumerate() {
        if position >= params.len() || arg.kind() != kinds::INT {
            continue;
        }
        let kind = params[position].get("kind").and_then(Node::as_str).unwrap_or("");
        if let Some(prefix) = subject_prefix(kind) {
            let constant = arg["value"].as_i64().and_then(|value| enum_values(kind, found).get(&value).cloned());
            if let Some(constant) = constant.filter(|constant| constant.starts_with(prefix)) {
                return Some((kind.to_string(), constant[prefix.len()..].to_lowercase()));
            }
        }
    }
    None
}

/// (variable, wanted name, the parameter's own name) for each variable a native's out-parameter
/// fills.
fn out_uses(nodes: &[Node], found: &Context, out: &mut Vec<(String, String, String)>) {
    for node in nodes {
        match node.kind() {
            kinds::NATIVE => {
                let Some(declared) = found.by_original.get(node.text("name")) else {
                    continue;
                };
                let params = declared.list("params");
                let subject = call_subject(node, declared, found);
                let outs: Vec<(usize, &Node)> = node
                    .list("args")
                    .iter()
                    .enumerate()
                    .filter(|(position, arg)| {
                        *position < params.len()
                            && params[*position]["out"].is_set()
                            && arg.kind() == kinds::ADDR
                            && arg["target"].kind() == kinds::VAR
                    })
                    .collect();
                // get_home_pos(&x, &y, &z) reads the home's position: home_x, home_y, home_z
                let thing = vector_subject(declared.text("name")).map(str::to_string);
                for (position, arg) in &outs {
                    let generic = params[*position].text("name").to_string();
                    let mut wanted = generic.clone();
                    match &subject {
                        Some((kind, subject)) if kind == "flag" && outs.len() == 1 => wanted = subject.clone(),
                        Some((kind, subject)) if kind == "chara_slot" => wanted = format!("{subject}_{wanted}"),
                        _ => {
                            if let Some(thing) = thing
                                .as_ref()
                                .filter(|thing| !["chara", "actor", "handle", "object"].contains(&thing.as_str()))
                            {
                                if ["x", "y", "z"].contains(&wanted.as_str()) {
                                    wanted = format!("{thing}_{wanted}");
                                }
                            }
                        }
                    }
                    out.push((arg["target"].text("name").to_string(), wanted, generic));
                }
            }
            kinds::IF => {
                out_uses(node.list("then"), found, out);
                out_uses(node.list("else"), found, out);
            }
            kinds::WHILE => out_uses(node.list("body"), found, out),
            _ => {}
        }
    }
}

/// The native parameter each variable is first passed to as a plain value: {variable: parameter}.
fn in_uses(nodes: &[Node], found: &Context, out: &mut HashMap<String, String>) {
    for node in nodes {
        if node.kind() == kinds::NATIVE {
            let declared = found.by_original.get(node.text("name"));
            for (position, arg) in node.list("args").iter().enumerate() {
                if let Some(declared) = declared {
                    let params = declared.list("params");
                    if position < params.len() && !params[position]["out"].is_set() && arg.kind() == kinds::VAR {
                        out.entry(arg.text("name").to_string())
                            .or_insert_with(|| params[position].text("name").to_string());
                    }
                }
            }
        }
        for key in ["then", "else", "body"] {
            if let Some(items) = node.get(key).and_then(Node::as_array) {
                in_uses(items, found, out);
            }
        }
    }
}

/// The variables a body reads and writes. A write is an assignment's target, a native's result, or
/// an argument passed by address (an out-parameter); anything else is a read.
fn variable_access(node: &Node, reads: &mut HashSet<String>, writes: &mut HashSet<String>) {
    match node {
        Node::List(items) => items.iter().for_each(|item| variable_access(item, reads, writes)),
        Node::Map(map) => {
            let kind = node.kind();
            if kind == kinds::VAR {
                reads.insert(node.text("name").to_string());
                return;
            }
            if (kind == kinds::ADDR || kind == kinds::ASSIGN)
                && matches!(node["target"].kind(), kinds::VAR | kinds::INDEX)
            {
                writes.insert(node["target"].text("name").to_string());
                if let Some(index) = node["target"].get("index") {
                    variable_access(index, reads, writes);
                }
                if kind == kinds::ASSIGN {
                    variable_access(&node["value"], reads, writes);
                }
                return;
            }
            if kind == kinds::NATIVE
                && node.get("result").is_some_and(|result| !result.is_null() && result.kind() == kinds::VAR)
            {
                writes.insert(node["result"].text("name").to_string());
            }
            for (key, value) in map.iter() {
                if key != "result" {
                    variable_access(value, reads, writes);
                }
            }
        }
        _ => {}
    }
}

fn digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}

/// unused, unused3, unused_value or unused_2.
fn is_unused_local(name: &str) -> bool {
    match name.strip_prefix("unused") {
        Some("" | "_value") => true,
        Some(rest) => digits(rest) || rest.strip_prefix('_').is_some_and(digits),
        None => false,
    }
}

/// The name, or the name with _N after it.
fn is_name_or_numbered(name: &str, base: &str) -> bool {
    match name.strip_prefix(base) {
        Some("") => true,
        Some(rest) => rest.strip_prefix('_').is_some_and(digits),
        None => false,
    }
}

fn default_local(name: &str) -> bool {
    name.len() > 1 && name.starts_with('v') && name[1..].bytes().all(|b| b.is_ascii_digit())
}

/// Names locals after the natives that fill them: a slot-numbered local takes the name of the
/// result it first holds, and a local with only that generic name (flag, flag_2) takes the name of
/// what it reads when every read agrees (griffon_defeated for get_flag(FLAG_GRIFFON_DEFEATED)).
pub fn named_locals(function: Node, found: &Context, function_names: &HashSet<String>) -> Node {
    let mut uses = Vec::new();
    out_uses(function.list("body"), found, &mut uses);
    let mut by_variable: HashMap<&str, HashSet<(&str, &str)>> = HashMap::new();
    for (variable, wanted, generic) in &uses {
        by_variable.entry(variable).or_default().insert((wanted, generic));
    }
    let locals = function.list("locals");
    let mut generic_names = HashSet::new();
    for declared in locals {
        let name = declared.text("name");
        if let Some(seen) = by_variable.get(name).filter(|seen| seen.len() == 1) {
            let (wanted, generic) = *seen.iter().next().unwrap();
            if wanted != generic && is_name_or_numbered(name, generic) {
                generic_names.insert(name.to_string());
            }
        }
    }
    // a local called unused that the function reads after all: unset when nothing writes it (the
    // function reads a slot it never sets), otherwise named like any other
    let (mut reads, mut writes) = (HashSet::new(), HashSet::new());
    variable_access(&function["body"], &mut reads, &mut writes);
    let misnamed: HashSet<String> = locals
        .iter()
        .map(|declared| declared.text("name"))
        .filter(|name| is_unused_local(name) && reads.contains(*name))
        .map(str::to_string)
        .collect();
    let mut unnamed: HashSet<String> = locals
        .iter()
        .map(|declared| declared.text("name"))
        .filter(|name| default_local(name))
        .map(str::to_string)
        .collect();
    unnamed.extend(generic_names);
    unnamed.extend(misnamed.iter().cloned());
    if unnamed.is_empty() {
        return function;
    }
    let mut taken: HashSet<String> =
        function.list("params").iter().map(|param| param.text("name").to_string()).collect();
    taken.extend(locals.iter().map(|declared| declared.text("name").to_string()));
    taken.extend(names::variables(&function["body"]));
    // the SDK's names are taken too; checked in place, as there are thousands
    taken.extend(function_names.iter().cloned());
    let mut mapping: IndexMap<String, String> = IndexMap::new();
    for (variable, wanted, _) in &uses {
        if !unnamed.contains(variable) || mapping.contains_key(variable) {
            continue;
        }
        let mut candidate = wanted.clone();
        let mut copy = 1;
        while taken.contains(&candidate) || found.reserved.contains(&candidate) {
            copy += 1;
            candidate = format!("{wanted}_{copy}");
        }
        taken.insert(candidate.clone());
        mapping.insert(variable.clone(), candidate);
    }
    let mut passed = HashMap::new();
    in_uses(function.list("body"), found, &mut passed);
    let mut leftover: Vec<&String> = misnamed.iter().filter(|name| !mapping.contains_key(*name)).collect();
    leftover.sort();
    for variable in leftover {
        // named for the parameter it is first passed to (sound_id for play_se), or by what it is
        let mut wanted = passed
            .get(variable)
            .cloned()
            .unwrap_or_else(|| if writes.contains(variable) { "value" } else { "unset" }.to_string());
        if !writes.contains(variable) && wanted != "unset" {
            wanted = format!("unset_{wanted}");
        }
        let mut candidate = wanted.clone();
        let mut copy = 1;
        while taken.contains(&candidate) || found.reserved.contains(&candidate) {
            copy += 1;
            candidate = format!("{wanted}_{copy}");
        }
        taken.insert(candidate.clone());
        mapping.insert(variable.clone(), candidate);
    }
    if mapping.is_empty() {
        return function;
    }
    let mapping: HashMap<String, String> = mapping.into_iter().collect();
    let locals: Node = locals
        .iter()
        .map(|declared| {
            let name = declared.text("name");
            declared.clone().with("name", mapping.get(name).cloned().unwrap_or_else(|| name.to_string()))
        })
        .collect();
    let body = names::renamed_variables(&function["body"], &mapping);
    function.with("locals", locals).with("body", body)
}

fn call_named(node: &Node, found: &Context, function_kinds: &HashMap<String, BTreeMap<usize, String>>) -> Node {
    match node {
        Node::List(items) => Node::List(items.iter().map(|item| call_named(item, found, function_kinds)).collect()),
        Node::Map(map) => {
            let mut out = Node::Map(
                map.entries().map(|(key, value)| (key.clone(), call_named(value, found, function_kinds))).collect(),
            );
            if node.kind() == kinds::CALL {
                if let Some(kinds) = function_kinds.get(node.text("name")) {
                    let args: Vec<Node> = out
                        .list("args")
                        .iter()
                        .enumerate()
                        .map(|(position, arg)| match kinds.get(&position) {
                            Some(kind) => named_literal(arg, kind, found),
                            None => arg.clone(),
                        })
                        .collect();
                    out = out.with("args", args);
                }
            }
            out
        }
        other => other.clone(),
    }
}

/// The name an enum type's constants are looked up by: enum se_bank reads as the kind se_bank.
fn kind_name(kind: &str) -> String {
    kind.strip_prefix("enum ").unwrap_or(kind).to_string()
}

fn variable_kinds(function: &Node) -> HashMap<String, String> {
    function
        .list("params")
        .iter()
        .chain(function.list("locals"))
        .filter(|variable| variable.get("kind").is_some_and(Node::is_set))
        .map(|variable| (variable.text("name").to_string(), kind_name(&variable["kind"].plain_text())))
        .collect()
}

/// Names the numbers assigned to a variable with a value kind, and compared with it.
fn typed_literals(node: &Node, kinds: &HashMap<String, String>, found: &Context) -> Node {
    if kinds.is_empty() {
        return node.clone();
    }
    match node {
        Node::List(items) => Node::List(items.iter().map(|item| typed_literals(item, kinds, found)).collect()),
        Node::Map(map) => {
            let mut out = Node::Map(
                map.entries().map(|(key, value)| (key.clone(), typed_literals(value, kinds, found))).collect(),
            );
            match out.kind() {
                kinds::ASSIGN if out["target"].kind() == kinds::VAR => {
                    if let Some(kind) = kinds.get(out["target"].text("name")) {
                        let value = named_literal(&out["value"], kind, found);
                        out = out.with("value", value);
                    }
                }
                kinds::BINARY if matches!(out.text("op"), "==" | "!=" | "<" | "<=" | ">" | ">=") => {
                    for (side, other) in [("left", "right"), ("right", "left")] {
                        if out[side].kind() == kinds::VAR {
                            if let Some(kind) = kinds.get(out[side].text("name")) {
                                let value = named_literal(&out[other], kind, found);
                                out = out.with(other, value);
                            }
                        }
                    }
                }
                _ => {}
            }
            out
        }
        other => other.clone(),
    }
}

/// The parameters of each function that carry a kind: {function: {position: kind}}.
pub fn function_kinds_of(
    params_by_kind: &HashMap<String, crate::messages::ParamsByFunction>,
) -> HashMap<String, BTreeMap<usize, String>> {
    let mut found: HashMap<String, BTreeMap<usize, String>> = HashMap::new();
    // a parameter carries one kind, so the order the kinds are taken in does not matter
    for kind in params_by_kind.keys() {
        for (name, positions) in &params_by_kind[kind] {
            for position in positions {
                found.entry(name.clone()).or_default().insert(*position, kind.clone());
            }
        }
    }
    found
}

fn calls_made(node: &Node, out: &mut HashSet<(String, String)>) {
    match node {
        Node::List(items) => items.iter().for_each(|item| calls_made(item, out)),
        Node::Map(map) => {
            if matches!(node.kind(), kinds::NATIVE | kinds::CALL) {
                out.insert((node.kind().to_string(), node.text("name").to_string()));
            }
            map.values().for_each(|value| calls_made(value, out));
        }
        _ => {}
    }
}

/// The hidden functions the sound pass needs: those that call a sound native, or call a function
/// (shown or hidden) that does, directly or further down.
fn sound_related<'a>(hidden: &'a [Node], shown: &[Node], found: &Context) -> Vec<&'a Node> {
    let functions: Vec<&Node> =
        hidden.iter().chain(shown).filter(|function| function.kind() == kinds::FUNCTION).collect();
    let calls: Vec<(String, HashSet<(String, String)>)> = functions
        .iter()
        .map(|function| {
            let mut made = HashSet::new();
            calls_made(&function["body"], &mut made);
            (function.text("name").to_string(), made)
        })
        .collect();
    let mut related: HashSet<String> = calls
        .iter()
        .filter(|(_, made)| {
            made.iter().any(|(kind, callee)| {
                let name =
                    found.by_original.get(callee).and_then(|declared| declared["name"].as_str()).unwrap_or(callee);
                kind == kinds::NATIVE && crate::soundpass::is_sound_native(name)
            })
        })
        .map(|(name, _)| name.clone())
        .collect();
    let mut grown = true;
    while grown {
        grown = false;
        for (name, made) in &calls {
            if !related.contains(name)
                && made.iter().any(|(kind, callee)| kind == kinds::CALL && related.contains(callee))
            {
                related.insert(name.clone());
                grown = true;
            }
        }
    }
    hidden
        .iter()
        .filter(|function| function.kind() == kinds::FUNCTION && related.contains(function.text("name")))
        .collect()
}

fn prepared_function(function: &Node, found: &Context, function_names: &HashSet<String>, inlines: &[&Node]) -> Node {
    if function.kind() != kinds::FUNCTION {
        return function.clone();
    }
    let body = assets::named(&old_numbered(function.list("body"), found), found);
    let function = named_locals(function.clone().with("body", body), found, function_names);
    let body = folded(function.list("body"), found, inlines, &HashSet::new());
    function.with("body", body)
}

/// The context for a script its definition entry marks as built for an older native table, or a
/// library only such scripts use. "old": groups that are current groups too (old_table_groups) are
/// read as old there. "raw": every call by number without a group comes from the older, smaller
/// table, also those that land on a native the header gives no group (old_direct).
pub fn numbered(found: Option<Context>, numbering: Option<&str>) -> Option<Context> {
    let mut found = found?;
    match numbering {
        Some("raw") => found.raw_table = true,
        Some("old") => {
            let groups: Vec<(i64, i64)> = found.old_table_groups.iter().map(|(base, new)| (*base, *new)).collect();
            found.old_groups.extend(groups);
            found.old_table = true;
        }
        _ => {}
    }
    Some(found)
}

pub fn numbering_of(entry: &Node) -> Option<String> {
    entry.get("numbering").and_then(Node::as_str).map(str::to_string)
}

/// The native an old-numbered call was written for. A call with an old group base selects the
/// native that many numbers on (old_groups); a call by raw number where the header gives the
/// native a group (shown as DIRECT) comes from an older table, and old_direct maps the numbers
/// whose same-shaped twins elsewhere on the disc show what they stand for.
pub fn meant_native(node: &Node, found: &Context) -> Option<String> {
    let name = node.text("name");
    let (no, declared_base) = match found.by_original.get(name) {
        Some(declared) => (declared["no"].as_i64()?, declared["base"].clone()),
        // a number the engine does not register, as the decompiler names it
        None => (ext_number(name)?, Node::Null),
    };
    if node["base"].is_null() && found.old_table {
        if let Some(group) = found.old_groups.get(&no) {
            // a call to an old group's first number, made without a group
            return found.by_number.get(group).cloned();
        }
    }
    if node["base"].is_null() {
        if (declared_base.is_null() && !found.raw_table) || !found.old_direct.contains_key(&no) {
            return None;
        }
        return found.by_number.get(&found.old_direct[&no]).cloned();
    }
    let base = node["base"].as_i64()?;
    let group = found.old_groups.get(&base)?;
    found.by_number.get(&(no - base + group)).cloned()
}

fn old_numbered(nodes: &[Node], found: &Context) -> Node {
    nodes
        .iter()
        .map(|node| match node.kind() {
            kinds::IF => {
                let otherwise = match node["else"].as_array() {
                    Some(items) => old_numbered(items, found),
                    None => Node::Null,
                };
                node.clone().with("then", old_numbered(node.list("then"), found)).with("else", otherwise)
            }
            kinds::WHILE => node.clone().with("body", old_numbered(node.list("body"), found)),
            kinds::NATIVE => {
                let mut notes = Vec::new();
                if let Some(meant) = meant_native(node, found) {
                    notes.push(format!("old numbering, meant as {meant}"));
                }
                if node.text("name").starts_with("_EXT_") {
                    // the engine's table has no function at this number; the VM prints "not found
                    // ext" and goes on (CRunScript::ext, runscript.cpp)
                    notes.push("no native at this number, so the call does nothing".into());
                }
                if notes.is_empty() {
                    return node.clone();
                }
                if let Some(comment) = node.get("comment").filter(|comment| comment.is_set()) {
                    notes.push(comment.plain_text());
                }
                node.clone().with("comment", notes.join("; "))
            }
            _ => node.clone(),
        })
        .collect()
}

/// A context with the definition set's event bank of each sound set, keyed by number.
pub fn with_sound_sets(found: Option<Context>, defs: &Definitions) -> Option<Context> {
    let mut found = found?;
    if !defs.sound_set_banks.is_empty() {
        found.sound_set_banks = defs
            .sound_set_banks
            .iter()
            .filter_map(|(number, bank)| Some((number.parse().ok()?, bank.clone())))
            .collect();
    }
    Some(found)
}

fn shadowing(function: &Node, names_used: &[&String]) -> bool {
    let declared: HashSet<&str> =
        function.list("params").iter().chain(function.list("locals")).map(|variable| variable.text("name")).collect();
    names_used.iter().any(|name| declared.contains(name.as_str()))
}

pub fn present_inline(inline: &Node, found: &Context) -> Node {
    let pointers: HashSet<String> = inline
        .list("params")
        .iter()
        .filter(|param| param["out"].is_set())
        .map(|param| param.text("name").to_string())
        .collect();
    let body = folded(inline.list("body"), found, &[], &pointers);
    inline.clone().with("body", body)
}

/// The functions as the decompiler shows them. hidden: the included library functions of a
/// script, which are not shown but whose sound flow counts. sounds: what the scripts that include
/// a library pass it, and where to put what this script passes the hidden functions.
pub fn present_functions(
    functions: &[Node],
    found: &Context,
    params_by_kind: &HashMap<String, crate::messages::ParamsByFunction>,
    hidden: &[Node],
    sounds: &mut Sounds,
) -> Vec<Node> {
    let mut function_kinds = function_kinds_of(params_by_kind);
    // parameters a definition set gives a value kind (types)
    for function in functions.iter().filter(|function| function.kind() == kinds::FUNCTION) {
        for (position, param) in function.list("params").iter().enumerate() {
            if let Some(kind) = param.get("kind").filter(|kind| kind.is_set()) {
                function_kinds
                    .entry(function.text("name").to_string())
                    .or_default()
                    .insert(position, kind_name(&kind.plain_text()));
            }
        }
    }
    let functions: Vec<Node> = if function_kinds.is_empty() {
        functions.to_vec()
    } else {
        functions
            .iter()
            .map(|function| {
                if function.kind() == kinds::FUNCTION {
                    function.clone().with("body", call_named(&function["body"], found, &function_kinds))
                } else {
                    function.clone()
                }
            })
            .collect()
    };
    let inlines = ordered_inlines(found);
    let function_names: HashSet<String> = functions
        .iter()
        .filter(|function| function.kind() == kinds::FUNCTION)
        .map(|function| function.text("name").to_string())
        .collect();
    let prepared: Vec<Node> =
        functions.iter().map(|function| prepared_function(function, found, &function_names, &inlines)).collect();
    // included code is the same in every script that includes it, so it is prepared once per set
    // of global names (sounds.keys, into sounds.cache)
    let mut prepared_hidden = Vec::new();
    for function in sound_related(hidden, &functions, found) {
        let name = function.text("name");
        match sounds.keys.get(name).cloned() {
            None => prepared_hidden.push(prepared_function(function, found, &function_names, &inlines)),
            Some(key) => {
                if !sounds.cache.contains_key(&key) {
                    let ready = prepared_function(function, found, &function_names, &inlines);
                    sounds.cache.insert(key.clone(), ready);
                }
                prepared_hidden.push(sounds.cache[&key].clone());
            }
        }
    }
    let all: Vec<Node> = prepared.iter().chain(&prepared_hidden).cloned().collect();
    let (known, seen) = crate::soundpass::program_sounds(&all, found, sounds.seed.as_ref(), sounds.entries.as_ref());
    if sounds.want_out {
        sounds.out = Some(crate::soundpass::passed_sounds(&prepared_hidden, &known, &seen));
    }
    let float_names: Vec<&String> = found.float_names.values().collect();
    let presented: Vec<Node> = prepared
        .into_iter()
        .map(|function| {
            if function.kind() != kinds::FUNCTION {
                return function;
            }
            let mut banks = crate::soundpass::sound_banks(&function, &known);
            let flow = crate::soundpass::sound_flow(&function, &known);
            let body = crate::soundpass::named_sounds(function.list("body"), found, &mut banks, Some(&flow), None);
            let mut body = typed_literals(&body, &variable_kinds(&function), found);
            if !found.float_names.is_empty() && !shadowing(&function, &float_names) {
                body = named_floats(&body, found);
            }
            function.with("body", body)
        })
        .collect();
    crate::slotflow::named_slots(presented, hidden, found)
}

/// A decompiled program as readable source: natives by their SDK names, helpers, constants,
/// named results, locals and sounds; the SDK header included in place of the native table.
pub fn present(
    program: Node,
    found: Option<&Context>,
    params_by_kind: &HashMap<String, crate::messages::ParamsByFunction>,
    hidden: &[Node],
    sounds: &mut Sounds,
) -> Node {
    let Some(found) = found else {
        return program;
    };
    if program.get("asm").is_some() {
        return program;
    }
    let programs: Vec<Node> = program
        .list("programs")
        .iter()
        .map(|entry| {
            let number = entry["no"]
                .as_i64()
                .and_then(|no| found.program_names.get(&no))
                .map_or_else(|| entry["no"].clone(), Node::from);
            entry.clone().with("no", number)
        })
        .collect();
    // the functions the engine starts, which find the event port as the last event left it
    let mut entries: HashSet<String> =
        program.list("programs").iter().map(|entry| entry.text("function").to_string()).collect();
    if let Some(main) = program["main"].as_str() {
        entries.insert(main.to_string());
    }
    sounds.entries = Some(entries);
    let mut functions: Vec<Node> =
        found.includes.iter().map(|path| node!({"kind": "include", "path": path.clone(), "system": true})).collect();
    functions.extend(present_functions(program.list("functions"), found, params_by_kind, hidden, sounds));
    program.with("natives", Node::Null).with("programs", programs).with("functions", functions)
}
