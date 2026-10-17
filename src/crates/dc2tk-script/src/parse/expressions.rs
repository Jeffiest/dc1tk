//! Expressions: the parser's descent from the comma operator to primary expressions, sizeof
//! and the types of expressions it needs, and the folding of integer constant expressions.

use super::*;

/// An integer literal's value: hexadecimal after 0x, octal after a leading 0, as in C.
pub(super) fn int_value(text: &str) -> Result<i128> {
    let hexadecimal = text.starts_with("0x") || text.starts_with("0X");
    if !hexadecimal && text.len() > 1 && text.starts_with('0') && text.chars().all(|c| c.is_ascii_digit()) {
        return int_in_base(&text[1..], 8);
    }
    int_in_base(text, if hexadecimal { 16 } else { 10 })
}

/// An unsigned int literal, held as the VM's 32 bits.
pub(super) fn unsigned_literal(value: i128) -> Result<Node> {
    Ok(int_literal(value as u32 as i32 as i128)?.with("unsigned", true))
}

pub(super) fn literal(state: &State, token: &Token) -> Result<Node> {
    if let Kind::Int { unsigned } = token.kind {
        let value = int_value(token.text()).or_else(|error| lex_fail(token.line, error))?;
        // C89: a literal too large for int (and long) is unsigned; one too large for that has no type
        if value > u32::MAX as i128 {
            return state.fail(format!("{} does not fit in 32 bits", token.text()));
        }
        if unsigned || value > i32::MAX as i128 {
            return unsigned_literal(value);
        }
        return int_literal(value);
    }
    let bits = asm::float_bits(token.text()).or_else(|error| lex_fail(token.line, error))?;
    Ok(node!({"kind": "float", "bits": bits}))
}

pub(super) fn arguments(state: &mut State) -> Result<Vec<Node>> {
    state.expect("(")?;
    let mut args = Vec::new();
    while !state.at(")", 0) {
        args.push(assignment(state)?);
        if !state.at(")", 0) {
            state.expect(",")?;
        }
    }
    state.expect(")")?;
    Ok(args)
}

// The expression parser recurses once for each level of nesting, so the functions on its way
// down are kept small (a debug build gives a function's every temporary a place of its own in its
// frame): each form is read by a function of its own, and the nodes are built apart from the
// recursion where they are large.

/// A primary expression, and the postfix operators after it.
pub(super) fn primary(state: &mut State) -> Result<Node> {
    let token = state.take();
    match token.kind {
        Kind::Int { .. } | Kind::Float => {
            let number = literal(state, &token)?;
            // 1[a] is a[1]
            subscripted(state, number)
        }
        Kind::Str { wide: true } => {
            let wide = string_literal(state, &token)?;
            postfix_operators(state, wide)
        }
        Kind::Str { .. } => {
            // "abc"[1]: the literal as an array
            let string = string_literal(state, &token)?;
            subscripted(state, string)
        }
        Kind::Op if token.text() == "(" => parenthesised(state),
        Kind::Name => named_primary(state, token.text()),
        _ => not_an_expression(state, &token),
    }
}

pub(super) fn not_an_expression(state: &mut State, token: &Token) -> Result<Node> {
    state.pos -= 1;
    let found = state.shown(token)?;
    state.fail(format!("expected an expression, found {found}"))
}

/// A literal followed by [, which subscripts it; else the literal alone.
pub(super) fn subscripted(state: &mut State, node: Node) -> Result<Node> {
    if state.at("[", 0) {
        return postfix_operators(state, node);
    }
    Ok(node)
}

/// A string literal and those right after it, which are one: plain, or wide (L"...").
pub(super) fn string_literal(state: &mut State, token: &Token) -> Result<Node> {
    let mut text = token.text().to_string();
    while state.token(0).kind.is_string() {
        let next = state.take();
        if next.wide() != token.wide() {
            return state.fail("a wide string literal cannot be joined to a plain one");
        }
        text.push_str(next.text());
    }
    if token.wide() {
        return wide_string(state, &text);
    }
    Ok(node!({"kind": "string", "text": text}))
}

/// ( expression ) after its (, and the postfix operators after it.
pub(super) fn parenthesised(state: &mut State) -> Result<Node> {
    let node = expression(state)?;
    state.expect(")")?;
    postfix_operators(state, node)
}

/// What a name starts: sin(x) or cos(x), a call (of a function, or through a function pointer
/// variable), offsetof, va_arg or va_start, or a variable; and the postfix operators after it.
pub(super) fn named_primary(state: &mut State, name: &str) -> Result<Node> {
    if !state.at("(", 0) {
        let node = named(state, name)?;
        return postfix_operators(state, node);
    }
    if BUILTINS.contains(&name) {
        return builtin(state, name);
    }
    match name {
        "__offsetof" => return offsetof(state),
        "__va_arg" => return va_arg(state),
        "__va_start" => return va_start(state),
        _ => {}
    }
    let call = call(state, name)?;
    postfix_operators(state, call)
}

/// sin(x) or cos(x), which the VM has instructions for.
pub(super) fn builtin(state: &mut State, name: &str) -> Result<Node> {
    state.expect("(")?;
    let operand = assignment(state)?;
    state.expect(")")?;
    Ok(node!({"kind": "builtin", "name": name, "operand": operand}))
}

/// name(args): a call of a function, or through a function pointer the name is a variable of.
pub(super) fn call(state: &mut State, name: &str) -> Result<Node> {
    let storage = state.storage_name(name);
    let pointer = state.variable(&storage).is_some();
    let args = arguments(state)?;
    Ok(if pointer {
        node!({"kind": "call_ptr", "callee": {"kind": "var", "name": storage}, "args": args})
    } else {
        node!({"kind": "call", "name": name, "args": args})
    })
}

/// The postfix operators after a primary expression: s.m, p->m, a[i] (of any expression), a call
/// through a function pointer, x++ and x--. Each nests what comes before it.
pub(super) fn postfix_operators(state: &mut State, node: Node) -> Result<Node> {
    state.chain(|state| {
        let mut node = node;
        loop {
            let Some(op) = [".", "->", "[", "(", "++", "--"].into_iter().find(|op| state.at(op, 0)) else {
                return Ok(node);
            };
            state.link()?;
            node = postfix_operator(state, op, node)?;
        }
    })
}

/// One postfix operator applied to what comes before it.
pub(super) fn postfix_operator(state: &mut State, op: &str, node: Node) -> Result<Node> {
    match op {
        "." | "->" => {
            state.take();
            let name = state.name()?;
            Ok(node!({"kind": "member", "object": node, "name": name, "arrow": op == "->"}))
        }
        "[" => {
            state.take();
            let index = expression(state)?;
            state.expect("]")?;
            Ok(node!({"kind": "subscript", "array": node, "index": index}))
        }
        "(" => {
            let args = arguments(state)?;
            Ok(node!({"kind": "call_ptr", "callee": node, "args": args}))
        }
        _ => {
            if !is_target(&node) {
                return state.fail("++ and -- need a variable");
            }
            state.take();
            writable(state, &node)?;
            Ok(postfix(node, op))
        }
    }
}

/// L"...": the characters' Shift-JIS codes, then 0, as an array of wchar_t (composite.rs gives it
/// storage).
pub(super) fn wide_string(state: &State, text: &str) -> Result<Node> {
    let mut codes = Vec::new();
    for character in text.chars() {
        let code = if (character as u32) < 0x80 {
            character as u32
        } else {
            match dc2tk_disc::text::encode(&character.to_string()) {
                Some(bytes) => bytes.iter().fold(0u32, |code, byte| code << 8 | u32::from(*byte)),
                // an escape's own value
                None if (character as u32) < 0x100 => character as u32,
                None => return state.fail(format!("{character:?} has no Shift-JIS code")),
            }
        };
        codes.push(Node::from(code as i64));
    }
    codes.push(Node::from(0));
    Ok(node!({"kind": "wide", "codes": codes}))
}

/// offsetof(type, member) from <stddef.h>: the member's offset in C's bytes, a size_t constant. The
/// member may be a path (a.b[2].c).
pub(super) fn offsetof(state: &mut State) -> Result<Node> {
    state.expect("(")?;
    let mut ty = abstract_type(state)?;
    state.expect(",")?;
    let mut bytes = 0;
    loop {
        let name = state.name()?;
        let Ty::Struct(tag) = &ty else {
            return state.fail(format!("offsetof needs a struct or union, not {}", ty.shown()));
        };
        let Some(ctype::Member { byte: offset, ty: member, bits, .. }) = ctype::member(&state.structs, tag, &name)
        else {
            return state.fail(format!("{} has no member {name}", ty.shown()));
        };
        if bits.is_some() {
            return state.fail(format!("offsetof of bit-field {name}, which has no byte of its own"));
        }
        bytes += offset;
        ty = member;
        while state.at("[", 0) {
            state.take();
            let index = constant_expression(state)?;
            state.expect("]")?;
            let Ty::Array(of, _) = &ty else {
                return state.fail(format!("{name} is not an array"));
            };
            bytes += index * of.bytes(&state.structs);
            ty = (**of).clone();
        }
        if !state.at(".", 0) {
            break;
        }
        state.take();
    }
    state.expect(")")?;
    Ok(size_literal(bytes))
}

/// va_arg(ap, type) from <stdarg.h>: the next of a variadic function's other arguments.
pub(super) fn va_arg(state: &mut State) -> Result<Node> {
    state.expect("(")?;
    let list = assignment(state)?;
    state.expect(",")?;
    let ty = abstract_type(state)?;
    state.expect(")")?;
    Ok(node!({"kind": "va_arg", "list": list, "ctype": ty.to_node()}))
}

/// va_start(ap, last) from <stdarg.h>: ap at the first of the arguments after last, which must be
/// the function's last parameter.
pub(super) fn va_start(state: &mut State) -> Result<Node> {
    state.expect("(")?;
    let list = assignment(state)?;
    state.expect(",")?;
    let last = state.name()?;
    state.expect(")")?;
    let params = state.scope.as_ref().map_or(&[][..], |scope| scope.params.as_slice());
    if params.last().is_none_or(|param| param["name"] != *last) {
        return state.fail(format!("va_start names the last parameter before the ..., not {last}"));
    }
    Ok(node!({"kind": "va_start", "list": list}))
}

/// A variable, or an element of an array, by the name code wrote: x, a[i], m[i][j].
#[inline(never)]
pub(super) fn named(state: &mut State, name: &str) -> Result<Node> {
    let name = state.storage_name(name);
    if !state.at("[", 0) {
        return Ok(node!({"kind": "var", "name": name}));
    }
    if state.variable(&name).is_some_and(|entry| entry.get("ctype").is_some()) {
        // a variable of C's other types: each [] subscripts what the one before gave
        return state.chain(|state| {
            let mut node = node!({"kind": "var", "name": name});
            while state.at("[", 0) {
                state.link()?;
                state.take();
                let index = expression(state)?;
                state.expect("]")?;
                node = node!({"kind": "subscript", "array": node, "index": index});
            }
            Ok(node)
        });
    }
    // element() nests a term for each index
    let mut indices = state.chain(|state| {
        let mut indices = Vec::new();
        while state.at("[", 0) {
            state.link()?;
            state.expect("[")?;
            indices.push(expression(state)?);
            state.expect("]")?;
        }
        Ok(indices)
    })?;
    if indices.len() == 1 && state.variable(&name).is_none_or(|entry| entry.get("dims").is_none()) {
        return Ok(node!({"kind": "index", "name": name, "index": indices.pop().unwrap()}));
    }
    element(state, &name, &indices)
}

/// The inner sizes of a multi-dimensional array: all of a local's after the first, all of a
/// parameter's (whose first is the pointer's).
pub(super) fn inner_sizes(entry: &Node) -> Vec<i64> {
    let dims: Vec<i64> =
        entry["dims"].as_array().map_or(Vec::new(), |dims| dims.iter().filter_map(Node::as_i64).collect());
    if entry.get("array").is_some() {
        dims
    } else {
        dims.into_iter().skip(1).collect()
    }
}

/// a[i][j] of a multi-dimensional array: the element at i * columns + j, or, with fewer indices
/// than the array has sizes, the address of that row (a[i] passes a row).
pub(super) fn element(state: &State, name: &str, indices: &[Node]) -> Result<Node> {
    let Some(entry) = state.variable(name) else {
        return state.fail(format!("unknown variable {name}"));
    };
    let inner = inner_sizes(&entry);
    if indices.len() > inner.len() + 1 {
        return state.fail(format!("{name} has {} sizes, not {}", inner.len() + 1, indices.len()));
    }
    let mut total = Node::Null;
    for (at, index) in indices.iter().enumerate() {
        let stride: i64 = inner[at..].iter().product();
        let term = if stride == 1 {
            index.clone()
        } else {
            node!({"kind": "binary", "op": "*", "left": index.clone(), "right": {"kind": "int", "value": stride}})
        };
        total = if total.is_null() { term } else { node!({"kind": "binary", "op": "+", "left": total, "right": term}) };
    }
    let cell = node!({"kind": "index", "name": name, "index": total});
    if indices.len() == inner.len() + 1 {
        return Ok(cell);
    }
    let row: i64 = inner[indices.len() - 1..].iter().product();
    Ok(node!({"kind": "addr", "target": cell, "row": row}))
}

/// target op= value, as in x += 2. The compiler works out the target's address once, so an index
/// with side effects (a[i++] += 1) runs them once.
pub(super) fn compound(target: Node, op: &str, value: Node) -> Node {
    node!({"kind": "compound", "op": op, "target": target, "value": value})
}

pub(super) fn stepped(target: Node, op: &str) -> Node {
    compound(target, op, one())
}

/// x++ and x--: the variable steps, and the expression's value is the old one, worked back from the
/// new so no temporary is needed. As a statement only the step remains.
pub(super) fn postfix(target: Node, text: &str) -> Node {
    let (op, undo) = if text == "++" { ("+", "-") } else { ("-", "+") };
    node!({"kind": "binary", "op": undo, "left": stepped(target, op), "right": one(), "postfix": true})
}

pub(super) fn cast_ahead(state: &State) -> bool {
    state.at("(", 0)
        && state.token(1).kind == Kind::Name
        && (CAST_TYPES.contains(&state.token(1).text()) || state.token(1).text() == "any")
        && state.at(")", 2)
}

pub(super) fn pointer_cast_ahead(state: &State) -> bool {
    state.at("(", 0)
        && state.token(1).kind == Kind::Name
        && CAST_TYPES.contains(&state.token(1).text())
        && state.at("*", 2)
        && state.at(")", 3)
}

pub(super) fn is_target(node: &Node) -> bool {
    matches!(node.kind(), "var" | "index" | "deref" | "member" | "subscript")
}

/// The variable an assignment target writes: x, a[i] or *p.
pub(super) fn target_name(target: &Node) -> &str {
    match target.kind() {
        "deref" => target["operand"].text("name"),
        _ => target.text("name"),
    }
}

/// Refuses writing a const object: a const variable, an element of a const array, or what a
/// pointer to const points at.
pub(super) fn writable(state: &State, target: &Node) -> Result<()> {
    let name = target_name(target);
    let Some(entry) = state.variable(name) else {
        return Ok(());
    };
    let pointer = entry.flag("pointer") || entry.flag("array");
    let constant = if target.kind() == "var" || !pointer {
        entry.get("const").is_some()
    } else {
        entry.get("pointee_const").is_some()
    };
    if constant {
        let shown = if target.kind() == "var" { name.to_string() } else { format!("what {name} points at") };
        return state.fail(format!("{shown} is const"));
    }
    Ok(())
}

/// An integer constant expression (an array size): numbers, the integer #defines and enum members
/// of this file and the files it includes, sizeof, and C's operators.
pub(super) fn constant_expression(state: &mut State) -> Result<i64> {
    let node = ternary(state)?;
    let found = |name: &str| state.constant(name).map(|value| value as i64);
    match folded(&node, &found) {
        Some((value, _)) => Ok(value),
        None => state.fail("expected an integer constant"),
    }
}

/// The bytes a value of a type (or a typedef of one) takes: the VM's slot holds 32 bits, and char and
/// short are C's 1 and 2.
pub(super) fn type_size(state: &State, kind: &str) -> i64 {
    let kind = state.base_kind(kind);
    match kind.as_str() {
        "char" | "unsigned char" => 1,
        "short" | "unsigned short" => 2,
        _ => 4,
    }
}

/// The number of elements a local's or global's size gives (1 for a scalar).
pub(super) fn elements(state: &State, entry: &Node) -> Result<i64> {
    match &entry["size"] {
        Node::Null => Ok(1),
        Node::Str(name) => match state.constant(name) {
            Some(value) => Ok(value as i64),
            None => state.fail(format!("sizeof needs the value of {name}, which this file does not define or include")),
        },
        size => Ok(size.as_i64().unwrap_or(1)),
    }
}

/// The type of a variable's declaration: its own C type, or one its plain fields give.
pub(super) fn entry_type(state: &State, entry: &Node) -> Ty {
    if let Some(ctype) = entry.get("ctype") {
        return Ty::from_node(ctype);
    }
    let kind = entry.text("type");
    let scalar = Ty::Scalar(state.kinds.get(kind).cloned().unwrap_or_else(|| kind.to_string()));
    if entry.flag("pointer") || entry.flag("array") {
        return ctype::pointer(scalar);
    }
    match elements(state, entry) {
        Ok(count) if !entry["size"].is_null() => Ty::Array(Box::new(scalar), count),
        _ => scalar,
    }
}

/// The type of an expression of C's other types that sizeof is given (None for the forms the
/// compiler had before, whose sizes size_of works out as it did).
pub(super) fn static_type(state: &State, node: &Node) -> Option<Ty> {
    let typed = |name: &str| state.variable(name).filter(|entry| entry.get("ctype").is_some());
    match node.kind() {
        "var" => typed(node.text("name")).map(|entry| entry_type(state, &entry)),
        "deref" => static_type(state, &node["operand"]).and_then(|ty| ty.target().cloned()),
        "subscript" => {
            let array = static_type(state, &node["array"])
                .or_else(|| state.variable(node["array"].text("name")).map(|entry| entry_type(state, &entry)))?;
            array.target().cloned()
        }
        "member" => member_of(state, node).map(|(ty, _)| ty),
        "addr" => static_type(state, &node["target"]).map(ctype::pointer),
        "wide" => {
            let count = node["codes"].as_array().map_or(1, Vec::len) as i64;
            Some(Ty::Array(Box::new(Ty::Scalar("unsigned short".into())), count))
        }
        "cast" => node.get("ctype").map(Ty::from_node),
        _ => None,
    }
}

/// A member expression's type, and its bits when it is a bit-field.
pub(super) fn member_of(state: &State, node: &Node) -> Option<(Ty, Option<ctype::Bits>)> {
    let object = static_type(state, &node["object"])
        .or_else(|| state.variable(node["object"].text("name")).map(|entry| entry_type(state, &entry)))?;
    let object = if node["arrow"] == Node::Bool(true) { object.target()?.clone() } else { object };
    let Ty::Struct(tag) = object else {
        return None;
    };
    ctype::member(&state.structs, &tag, node.text("name")).map(|found| (found.ty, found.bits))
}

/// What sizeof gives for an expression, which it does not evaluate.
pub(super) fn size_of(state: &State, node: &Node) -> Result<i64> {
    if node.kind() == "member" && member_of(state, node).is_some_and(|(_, bits)| bits.is_some()) {
        return state.fail(format!("sizeof of bit-field {}, which has no size of its own", node.text("name")));
    }
    if let Some(ty) = static_type(state, node) {
        return Ok(ty.bytes(&state.structs));
    }
    let entry = |name: &str| state.variable(name);
    Ok(match node.kind() {
        "string" => match stb::encode_string(node.text("text")) {
            Ok(bytes) => bytes.len() as i64,
            Err(error) => return state.fail(error),
        },
        "var" => match entry(node.text("name")) {
            Some(found) if found.flag("pointer") || found.flag("array") => 4,
            Some(found) => elements(state, &found)? * type_size(state, found.text("type")),
            // sizeof(wchar_t): a header's typedef, which reads as a name here
            None if state.header_kinds.contains_key(node.text("name")) => type_size(state, node.text("name")),
            None => 4,
        },
        "index" => entry(node.text("name")).map_or(4, |found| type_size(state, found.text("type"))),
        "deref" => entry(node["operand"].text("name")).map_or(4, |found| type_size(state, found.text("type"))),
        "addr" if node.get("row").is_some() => {
            let row = node["row"].as_i64().unwrap_or(1);
            row * entry(node["target"].text("name")).map_or(4, |found| type_size(state, found.text("type")))
        }
        "cast" if node.text("type") == "void" => return state.fail("sizeof a void expression"),
        "cast" if node.text("type").ends_with('*') => 4,
        "cast" => type_size(state, node.text("type")),
        "assign" | "compound" => size_of(state, &node["target"])?,
        "comma" => size_of(state, &node["right"])?,
        "binary" if node.get("postfix").is_some() => size_of(state, &node["left"])?,
        _ => 4,
    })
}

/// A sizeof's value: an int constant of size_t, which is unsigned int.
pub(super) fn size_literal(size: i64) -> Node {
    node!({"kind": "int", "value": size, "unsigned": true})
}

pub(super) fn unary(state: &mut State) -> Result<Node> {
    state.nested(prefixed)
}

/// A unary expression: an operator and its operand, sizeof, a cast, or a primary expression.
pub(super) fn prefixed(state: &mut State) -> Result<Node> {
    if ["-", "!", "+", "~", "*"].iter().any(|op| state.at(op, 0)) {
        return prefix_operator(state);
    }
    if state.at("sizeof", 0) {
        return sizeof(state);
    }
    if state.at("++", 0) || state.at("--", 0) {
        return increment(state);
    }
    if state.at("&", 0) {
        return address(state);
    }
    if cast_ahead(state) {
        return plain_cast(state);
    }
    if state.at("(", 0) && state.at("enum", 1) && state.token(2).kind == Kind::Name && state.at(")", 3) {
        return enum_cast(state);
    }
    if pointer_cast_ahead(state) {
        return pointer_cast(state);
    }
    if state.at("(", 0) && starts_type(state, 1) {
        return cast(state);
    }
    primary(state)
}

/// -x, !x, +x, ~x (-x - 1, the VM having no instruction for it) and *p.
pub(super) fn prefix_operator(state: &mut State) -> Result<Node> {
    let op = state.take();
    let operand = unary(state)?;
    Ok(prefix_node(op.text(), operand))
}

pub(super) fn prefix_node(op: &str, operand: Node) -> Node {
    match op {
        "~" => {
            node!({"kind": "binary", "op": "-", "left": {"kind": "unary", "op": "-", "operand": operand}, "right": one()})
        }
        "*" => node!({"kind": "deref", "operand": operand}),
        _ => node!({"kind": "unary", "op": op, "operand": operand}),
    }
}

/// ++x and --x: the variable steps, and the expression's value is the new one.
pub(super) fn increment(state: &mut State) -> Result<Node> {
    let op = if state.take().text() == "++" { "+" } else { "-" };
    let target = unary(state)?;
    if !is_target(&target) {
        return state.fail("++ and -- need a variable");
    }
    writable(state, &target)?;
    Ok(stepped(target, op))
}

/// The decompiler's casts: (int)x and (float)x, which convert between the VM's ints and floats,
/// and (any)x, which keeps the value as it is.
pub(super) fn plain_cast(state: &mut State) -> Result<Node> {
    state.take();
    let kind = state.take();
    state.take();
    let operand = unary(state)?;
    Ok(node!({"kind": "cast", "type": kind.text(), "operand": operand}))
}

/// (enum flags)&x passes x's address where a native takes an enum, as it is.
pub(super) fn enum_cast(state: &mut State) -> Result<Node> {
    state.take();
    state.take();
    let kind = format!("enum {}", state.take().text());
    state.take();
    let operand = unary(state)?;
    if operand.kind() != "addr" {
        return state.fail("a cast to an enum takes an address (&x)");
    }
    Ok(node!({"kind": "cast", "type": kind, "operand": operand}))
}

/// (int *)&x passes x's address as it is: C's view of a pointer of another type.
pub(super) fn pointer_cast(state: &mut State) -> Result<Node> {
    state.take();
    let kind = state.take().text().to_string();
    state.take();
    state.take();
    let operand = unary(state)?;
    Ok(node!({"kind": "cast", "type": format!("{kind} *"), "operand": operand}))
}

/// sizeof(type) or sizeof expression.
#[inline(never)]
pub(super) fn sizeof(state: &mut State) -> Result<Node> {
    state.take();
    if state.at("(", 0) && starts_type(state, 1) {
        state.take();
        let ty = abstract_type(state)?;
        state.expect(")")?;
        match &ty {
            Ty::Void => return state.fail("sizeof(void)"),
            Ty::Function(..) => return state.fail("sizeof a function"),
            Ty::Struct(_) => complete(state, "sizeof", &ty)?,
            _ => {}
        }
        return Ok(size_literal(ty.bytes(&state.structs)));
    }
    let operand = unary(state)?;
    Ok(size_literal(size_of(state, &operand)?))
}

/// Any cast but the decompiler's: (unsigned char)x, (void)f(), (long)y, (const int)z.
#[inline(never)]
pub(super) fn cast(state: &mut State) -> Result<Node> {
    state.take();
    let ty = abstract_type(state)?;
    state.expect(")")?;
    let operand = unary(state)?;
    match &ty {
        Ty::Array(..) => state.fail("a cast cannot give an array"),
        Ty::Struct(_) => state.fail("a cast cannot give a struct"),
        Ty::Function(..) => state.fail("a cast cannot give a function"),
        // the VM's pointers are values of their own: the cast passes the value as it is, and its
        // type is the compiler's (composite.rs)
        Ty::Pointer(to) => {
            Ok(node!({"kind": "cast", "type": format!("{} *", to.shown()), "operand": operand, "ctype": ty.to_node()}))
        }
        Ty::Void => Ok(node!({"kind": "cast", "type": "void", "operand": operand})),
        Ty::Scalar(kind) => Ok(node!({"kind": "cast", "type": kind.as_str(), "operand": operand})),
    }
}

/// &x; a register variable has no address.
pub(super) fn address(state: &mut State) -> Result<Node> {
    state.take();
    let target = unary(state)?;
    if !is_target(&target) {
        return state.fail("& needs a variable");
    }
    if target.kind() == "var"
        && state.variable(target.text("name")).is_some_and(|entry| entry.get("register").is_some())
    {
        return state.fail(format!("{} is a register variable, which has no address", target.text("name")));
    }
    Ok(node!({"kind": "addr", "target": target}))
}

pub(super) fn binary(state: &mut State, minimum: u32) -> Result<Node> {
    state.chain(|state| {
        let mut left = unary(state)?;
        while state.token(0).kind == Kind::Op && precedence(state.token(0).text()).unwrap_or(0) >= minimum {
            state.link()?;
            let op = state.take();
            let right = binary(state, precedence(op.text()).unwrap() + 1)?;
            left = binary_node(op.text(), left, right);
        }
        Ok(left)
    })
}

pub(super) fn binary_node(op: &str, left: Node, right: Node) -> Node {
    let kind = if op == "&&" || op == "||" { "logical" } else { "binary" };
    node!({"kind": kind, "op": op, "left": left, "right": right})
}

pub(super) fn ternary(state: &mut State) -> Result<Node> {
    state.nested(|state| {
        let cond = binary(state, 2)?;
        if !state.at("?", 0) {
            return Ok(cond);
        }
        choice(state, cond)
    })
}

/// cond ? then : else, after cond.
pub(super) fn choice(state: &mut State, cond: Node) -> Result<Node> {
    state.take();
    let then = assignment(state)?;
    state.expect(":")?;
    let otherwise = ternary(state)?;
    Ok(node!({"kind": "ternary", "cond": cond, "then": then, "else": otherwise}))
}

pub(super) fn assignment(state: &mut State) -> Result<Node> {
    state.nested(|state| {
        let left = ternary(state)?;
        let token = state.token(0);
        if token.kind != Kind::Op || (token.text() != "=" && compound_op(token.text()).is_none()) {
            return Ok(left);
        }
        assigned(state, left)
    })
}

/// target = value or target op= value, after the target.
pub(super) fn assigned(state: &mut State, target: Node) -> Result<Node> {
    if !is_target(&target) {
        return state.fail("cannot assign to this expression");
    }
    writable(state, &target)?;
    let op = state.take();
    let value = assignment(state)?;
    Ok(match compound_op(op.text()) {
        Some(op) => compound(target, op, value),
        None => node!({"kind": "assign", "target": target, "value": value}),
    })
}

/// An expression with C's comma operator: a, b works out a, then gives b.
pub(super) fn expression(state: &mut State) -> Result<Node> {
    state.chain(|state| {
        let mut node = assignment(state)?;
        while state.at(",", 0) {
            state.link()?;
            state.take();
            let right = assignment(state)?;
            node = comma(node, right);
        }
        Ok(node)
    })
}

pub(super) fn comma(left: Node, right: Node) -> Node {
    node!({"kind": "comma", "left": discarded(left), "right": right})
}

/// An expression whose value is not used: x++ is only its step.
pub(super) fn discarded(node: Node) -> Node {
    if node["postfix"] == Node::Bool(true) {
        node["left"].clone()
    } else {
        node
    }
}

pub(super) fn expression_statement(node: Node) -> Node {
    node!({"kind": "expr", "expr": discarded(node)})
}

/// The statements of an expression statement: one for each operand of a comma at its top.
pub(super) fn expression_statements(mut node: Node) -> Vec<Node> {
    if node.kind() == "comma" {
        let mut found = expression_statements(std::mem::take(&mut node["left"]));
        found.extend(expression_statements(std::mem::take(&mut node["right"])));
        return found;
    }
    vec![expression_statement(node)]
}

/// An integer constant expression's value, wrapped to 32 bits, and whether it is unsigned; None
/// when it is not one (names give their value through names). Unsigned division, remainder,
/// comparison and right shifts are left to the compiler.
pub(super) fn folded(node: &Node, names: &dyn Fn(&str) -> Option<i64>) -> Option<(i64, bool)> {
    let wrap = |value: i64| value as i32 as i64;
    match node.kind() {
        "int" => Some((wrap(node["value"].as_i64()?), node.get("unsigned").is_some())),
        "var" => names(node.text("name")).map(|value| (wrap(value), false)),
        "cast" if matches!(node.text("type"), "int" | "long") => folded(&node["operand"], names),
        "unary" => {
            let (value, unsigned) = folded(&node["operand"], names)?;
            Some(match node.text("op") {
                "-" => (wrap(value.wrapping_neg()), unsigned),
                "+" => (value, unsigned),
                "!" => (i64::from(value == 0), false),
                _ => return None,
            })
        }
        "ternary" => {
            let (cond, _) = folded(&node["cond"], names)?;
            let (then, then_unsigned) = folded(&node["then"], names)?;
            let (otherwise, otherwise_unsigned) = folded(&node["else"], names)?;
            Some((if cond != 0 { then } else { otherwise }, then_unsigned || otherwise_unsigned))
        }
        "logical" => {
            let (left, _) = folded(&node["left"], names)?;
            let (right, _) = folded(&node["right"], names)?;
            let truth = if node["op"] == "&&" { left != 0 && right != 0 } else { left != 0 || right != 0 };
            Some((i64::from(truth), false))
        }
        "binary" => {
            let (left, left_unsigned) = folded(&node["left"], names)?;
            let (right, right_unsigned) = folded(&node["right"], names)?;
            let unsigned = left_unsigned || right_unsigned;
            let op = node.text("op");
            if unsigned && matches!(op, "/" | "%" | "<" | "<=" | ">" | ">=" | ">>") {
                return None;
            }
            let (a, b) = (left as i32, right as i32);
            let value = match op {
                "+" => a.wrapping_add(b),
                "-" => a.wrapping_sub(b),
                "*" => a.wrapping_mul(b),
                "/" if b != 0 => a.wrapping_div(b),
                "%" if b != 0 => a.wrapping_rem(b),
                "&" => a & b,
                "|" => a | b,
                "^" => a ^ b,
                "<<" if (0..32).contains(&b) => a.wrapping_shl(b as u32),
                ">>" if (0..32).contains(&b) => a >> b,
                "==" => i32::from(a == b),
                "!=" => i32::from(a != b),
                "<" => i32::from(a < b),
                "<=" => i32::from(a <= b),
                ">" => i32::from(a > b),
                ">=" => i32::from(a >= b),
                _ => return None,
            };
            let compared = matches!(op, "==" | "!=" | "<" | "<=" | ">" | ">=");
            Some((value as i64, unsigned && !compared))
        }
        _ => None,
    }
}
