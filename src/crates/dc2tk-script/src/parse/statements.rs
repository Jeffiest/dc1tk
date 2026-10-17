//! Statements: blocks, if, the loops (for and do read as while), switch, labels and goto, and
//! the statements the decompiler writes.

use super::*;

// The statements, as the expressions, keep the functions on the parser's way down small: each
// form is read by a function of its own.

pub(super) fn braced(state: &mut State) -> Result<Vec<Node>> {
    state.nested(|state| {
        state.expect("{")?;
        state.open_block();
        let mut statements = Vec::new();
        while !state.at("}", 0) {
            statements.extend(statement(state)?);
        }
        state.close_block();
        state.expect("}")?;
        Ok(statements)
    })
}

/// A loop or branch body: a braced block or one statement.
pub(super) fn body(state: &mut State) -> Result<Vec<Node>> {
    if state.at("{", 0) {
        braced(state)
    } else {
        statement(state)
    }
}

pub(super) fn conditional(state: &mut State) -> Result<Node> {
    state.nested(|state| {
        let line = state.expect("if")?.line;
        state.expect("(")?;
        let cond = expression(state)?;
        state.expect(")")?;
        let then = body(state)?;
        let otherwise = if state.at("else", 0) { otherwise(state)? } else { Node::Null };
        Ok(node!({"kind": "if", "cond": cond, "then": then, "else": otherwise, "line": line}))
    })
}

/// An if's else, after the else: another if, or a body.
pub(super) fn otherwise(state: &mut State) -> Result<Node> {
    state.take();
    if state.at("if", 0) {
        return Ok(Node::List(vec![conditional(state)?]));
    }
    Ok(Node::List(body(state)?))
}

/// Puts extra statements in front of every continue of a loop body (not of loops inside it), so a
/// for loop's step and a do loop's test run when an iteration is cut short.
pub(super) fn before_continue(nodes: &[Node], extra: &[Node]) -> Vec<Node> {
    let mut out = Vec::new();
    for node in nodes {
        let node = match node.kind() {
            "continue" => {
                out.extend_from_slice(extra);
                node.clone()
            }
            "if" => {
                let otherwise = if node["else"].is_null() {
                    Node::Null
                } else {
                    Node::List(before_continue(node.list("else"), extra))
                };
                node.clone().with("then", before_continue(node.list("then"), extra)).with("else", otherwise)
            }
            "switch" => {
                let cases: Vec<Node> = node
                    .list("cases")
                    .iter()
                    .map(|case| {
                        let body = before_continue(case.list("body"), extra);
                        case.clone().with("body", body)
                    })
                    .collect();
                node.clone().with("cases", cases)
            }
            _ => node.clone(),
        };
        out.push(node);
    }
    out
}

pub(super) fn expression_list(state: &mut State, closing: &str) -> Result<Vec<Node>> {
    let mut found = Vec::new();
    while !state.at(closing, 0) {
        let node = assignment(state)?;
        found.push(expression_statement(node));
        if !state.at(closing, 0) {
            state.expect(",")?;
        }
    }
    state.expect(closing)?;
    Ok(found)
}

pub(super) fn while_loop(state: &mut State) -> Result<Vec<Node>> {
    state.nested(|state| {
        state.take();
        state.expect("(")?;
        let cond = expression(state)?;
        state.expect(")")?;
        let body = body(state)?;
        Ok(vec![node!({"kind": "while", "cond": cond, "body": body})])
    })
}

/// for (init; cond; step) body becomes init; while (cond) { body step }, with the step also run
/// before each continue.
pub(super) fn for_loop(state: &mut State) -> Result<Vec<Node>> {
    state.nested(|state| {
        let line = state.expect("for")?.line;
        state.expect("(")?;
        // a declaration in the first part belongs to the loop (as C99 has it)
        state.open_block();
        let init = if declaration_ahead(state) { declaration(state)? } else { expression_list(state, ";")? };
        let cond = if state.at(";", 0) { one() } else { expression(state)? };
        state.expect(";")?;
        let step = expression_list(state, ")")?;
        let body = body(state)?;
        state.close_block();
        Ok(for_nodes(init, cond, step, &body, line))
    })
}

pub(super) fn for_nodes(mut init: Vec<Node>, cond: Node, step: Vec<Node>, body: &[Node], line: usize) -> Vec<Node> {
    let mut body = before_continue(body, &step);
    body.extend(step);
    init.push(node!({"kind": "while", "cond": cond, "body": body, "line": line}));
    init
}

/// do body while (cond); becomes while (1) { body if (!cond) break; }, with the test also run before
/// each continue.
pub(super) fn do_loop(state: &mut State) -> Result<Vec<Node>> {
    state.nested(|state| {
        let line = state.expect("do")?.line;
        let body = body(state)?;
        state.expect("while")?;
        state.expect("(")?;
        let cond = expression(state)?;
        state.expect(")")?;
        state.expect(";")?;
        Ok(do_nodes(&body, cond, line))
    })
}

pub(super) fn do_nodes(body: &[Node], cond: Node, line: usize) -> Vec<Node> {
    let leave = node!({
        "kind": "if",
        "cond": {"kind": "unary", "op": "!", "operand": cond},
        "then": [{"kind": "break"}],
        "else": null,
    });
    let mut body = before_continue(body, std::slice::from_ref(&leave));
    body.push(leave);
    vec![node!({"kind": "while", "cond": one(), "body": body, "line": line})]
}

pub(super) fn switch(state: &mut State) -> Result<Node> {
    state.nested(|state| {
        let line = state.expect("switch")?.line;
        state.expect("(")?;
        let value = expression(state)?;
        state.expect(")")?;
        state.expect("{")?;
        let mut cases: Vec<Node> = Vec::new();
        // the case values known here, which C allows once each
        let mut numbers: Vec<i64> = Vec::new();
        while !state.at("}", 0) {
            if state.at("case", 0) || state.at("default", 0) {
                let case = case_label(state, &cases, &mut numbers)?;
                cases.push(case);
            } else if cases.is_empty() {
                return state.fail("expected case or default");
            } else {
                let found = statement(state)?;
                cases.last_mut().unwrap()["body"].as_array_mut().unwrap().extend(found);
            }
        }
        state.expect("}")?;
        Ok(node!({"kind": "switch", "value": value, "cases": cases, "line": line}))
    })
}

/// case value: or default:, as a case of the switch with no statements yet.
pub(super) fn case_label(state: &mut State, cases: &[Node], numbers: &mut Vec<i64>) -> Result<Node> {
    if state.take().text() == "default" {
        state.expect(":")?;
        if cases.iter().any(|case| case.flag("default")) {
            return state.fail("a switch has one default");
        }
        return Ok(node!({"value": null, "default": true, "body": []}));
    }
    let value = ternary(state)?;
    let found = |name: &str| state.constant(name).map(|value| value as i64);
    if let Some((number, _)) = folded(&value, &found) {
        if numbers.contains(&number) {
            return state.fail(format!("case {number} is in the switch twice"));
        }
        numbers.push(number);
    }
    state.expect(":")?;
    Ok(node!({"value": value, "default": false, "body": []}))
}

pub(super) fn statement(state: &mut State) -> Result<Vec<Node>> {
    state.nested(|state| {
        let line = state.line();
        // a "// note: ..." comment right above a statement is a line note the definitions keep
        let doc = state.token(0).doc.clone();
        let nodes = statement_nodes(state)?;
        Ok(placed(nodes, line, doc.as_deref()))
    })
}

/// A statement's nodes with its line, where they have none, and the first with its note: the last
/// paragraph of the comment above it that starts "note:".
pub(super) fn placed(nodes: Vec<Node>, line: usize, doc: Option<&str>) -> Vec<Node> {
    let mut nodes: Vec<Node> =
        nodes.into_iter().map(|node| if node.get("line").is_some() { node } else { node.with("line", line) }).collect();
    let note = doc
        .unwrap_or_default()
        .split('\n')
        .rfind(|paragraph| paragraph.get(..NOTE.len()).is_some_and(|start| start.eq_ignore_ascii_case(NOTE)));
    if let (Some(first), Some(note)) = (nodes.first_mut(), note) {
        *first = std::mem::take(first).with("note", note[NOTE.len()..].trim());
    }
    nodes
}

pub(super) fn statement_nodes(state: &mut State) -> Result<Vec<Node>> {
    let token = state.token(0);
    let word = if token.kind == Kind::Name { token.text().to_string() } else { String::new() };
    if word == "AT" || word == "DIRECT" {
        return placed_at(state);
    }
    if state.at("{", 0) {
        return braced(state);
    }
    if state.at(";", 0) {
        state.take();
        return Ok(Vec::new());
    }
    if declaration_ahead(state) {
        return declaration(state);
    }
    if state.token(0).kind == Kind::Name
        && !STATEMENT_WORDS.contains(&word.as_str())
        && state.token(1).kind == Kind::Op
        && state.token(1).text() == ":"
    {
        return label(state, word);
    }
    match word.as_str() {
        "if" => Ok(vec![conditional(state)?]),
        "while" => while_loop(state),
        "for" => for_loop(state),
        "do" => do_loop(state),
        "switch" => Ok(vec![switch(state)?]),
        "goto" => goto(state),
        "return" => return_statement(state),
        "PRINT" => print(state),
        word if VM_STATEMENTS.iter().any(|(name, _)| *name == word) => {
            let kind = VM_STATEMENTS.iter().find(|(name, _)| *name == word).map(|(_, kind)| *kind).unwrap();
            state.take();
            state.expect("(")?;
            state.expect(")")?;
            state.expect(";")?;
            Ok(vec![node!({"kind": kind})])
        }
        word if SIMPLE.contains(&word) => {
            state.take();
            state.expect(";")?;
            Ok(vec![node!({"kind": word})])
        }
        _ => {
            let node = expression(state)?;
            state.expect(";")?;
            Ok(expression_statements(node))
        }
    }
}

/// AT(base) statement or DIRECT statement: the native call the statement makes, at a base or
/// direct (sdk::resolve).
pub(super) fn placed_at(state: &mut State) -> Result<Vec<Node>> {
    let at = if state.take().text() == "DIRECT" {
        Node::from("direct")
    } else {
        state.expect("(")?;
        let base = int_node(int_value(state.expect_int()?.text())?)?;
        state.expect(")")?;
        base
    };
    Ok(statement(state)?.into_iter().map(|node| node.with("at", at.clone())).collect())
}

/// A label: the statement after it follows it in the list.
pub(super) fn label(state: &mut State, word: String) -> Result<Vec<Node>> {
    state.take();
    state.take();
    let Some(scope) = state.scope.as_mut() else {
        return state.fail("labels belong in functions");
    };
    if scope.labels.contains(&word) {
        return state.fail(format!("label {word} is defined twice"));
    }
    scope.labels.push(word.clone());
    let mut found = vec![node!({"kind": "label", "name": word})];
    if !state.at("}", 0) {
        found.extend(statement(state)?);
    }
    Ok(found)
}

pub(super) fn goto(state: &mut State) -> Result<Vec<Node>> {
    let line = state.take().line;
    let label = state.name()?;
    state.expect(";")?;
    let Some(scope) = state.scope.as_mut() else {
        return state.fail("goto belongs in functions");
    };
    scope.gotos.push((label.clone(), line));
    Ok(vec![node!({"kind": "goto", "name": label})])
}

pub(super) fn return_statement(state: &mut State) -> Result<Vec<Node>> {
    state.take();
    if state.at(";", 0) {
        state.take();
        return Ok(vec![node!({"kind": "return", "value": null})]);
    }
    let value = expression(state)?;
    state.expect(";")?;
    Ok(vec![node!({"kind": "return", "value": value})])
}

pub(super) fn print(state: &mut State) -> Result<Vec<Node>> {
    state.take();
    let args = arguments(state)?;
    state.expect(";")?;
    Ok(vec![node!({"kind": "print", "args": args})])
}
