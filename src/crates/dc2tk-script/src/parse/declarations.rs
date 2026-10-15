//! Declarations: locals and globals with their initializers (C's brace elision included),
//! typedefs, parameters, and function definitions.

use super::*;

pub(super) fn type_name(state: &mut State, allowed: &[&str]) -> Result<String> {
    let kind = state.name()?;
    if !allowed.contains(&kind.as_str()) {
        state.pos -= 1;
        return state.fail(format!("unknown type {kind}"));
    }
    Ok(kind)
}

/// An array's size: none, a number, or a constant's name.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Size {
    None,
    Number(i128),
    Named(String),
}

impl Size {
    pub(super) fn node(&self) -> Result<Node> {
        match self {
            Size::None => Ok(Node::Null),
            Size::Number(value) => int_node(*value),
            Size::Named(name) => Ok(Node::from(name)),
        }
    }
}

/// An array size: a number, or a constant's name. A name this file does not define is kept and
/// resolved with the constants of the headers and libraries the script includes (sdk::resolve).
/// Any other integer constant expression is worked out here.
pub(super) fn array_size(state: &mut State) -> Result<Size> {
    let token = state.token(0);
    let alone = state.token(1).kind == Kind::Op && state.token(1).text() == "]";
    if alone && (token.kind.is_int() || token.kind == Kind::Name) {
        let token = state.take();
        return Ok(match token.kind {
            Kind::Int { .. } => Size::Number(int_value(token.text())?),
            _ => match state.constants.get(token.text()) {
                Some(value) => Size::Number(*value),
                None => Size::Named(token.text().to_string()),
            },
        });
    }
    if token.kind.is_int() || matches!(token.kind, Kind::Name | Kind::Op) {
        return Ok(Size::Number(constant_expression(state)? as i128));
    }
    state.fail("an array size is a number or a constant")
}

/// An initializer: an expression, or a braced list of initializers.
#[derive(Clone, Debug)]
pub(super) enum Init {
    Value(Node),
    List(Vec<Init>),
}

pub(super) fn initializer(state: &mut State) -> Result<Init> {
    state.nested(|state| {
        if !state.at("{", 0) {
            return Ok(Init::Value(assignment(state)?));
        }
        state.expect("{")?;
        let mut values = Vec::new();
        while !state.at("}", 0) {
            values.push(initializer(state)?);
            if !state.at("}", 0) {
                state.expect(",")?;
            }
        }
        state.expect("}")?;
        Ok(Init::List(values))
    })
}

pub(super) struct Declared {
    pub(super) name: String,
    /// The array's sizes, the first Number(-1) when the initializer gives it.
    pub(super) dims: Vec<Size>,
    pub(super) init: Option<Init>,
    pub(super) line: usize,
    pub(super) pointer: bool,
    /// int *const p: the pointer itself is const.
    pub(super) pointer_const: bool,
    /// The type, for a declarator of C's other types (complex_ahead); the fields above are the
    /// plain forms'.
    pub(super) ty: Option<Ty>,
}

impl Declared {
    pub(super) fn size(&self) -> &Size {
        self.dims.first().unwrap_or(&Size::None)
    }
}

/// The *, and const after it, before a declarator's name.
pub(super) fn pointer_part(state: &mut State) -> Result<(bool, bool)> {
    let pointer = state.at("*", 0);
    let mut constant = false;
    if pointer {
        state.take();
        while state.at("const", 0) || state.at("volatile", 0) {
            constant |= state.take().text() == "const";
        }
        if state.at("*", 0) {
            return state.fail("pointers to pointers are not supported yet: the VM has one level of indirection");
        }
    }
    Ok((pointer, constant))
}

pub(super) fn declarators(state: &mut State, spec: &Spec) -> Result<Vec<Declared>> {
    let mut found = Vec::new();
    loop {
        let line = state.line();
        if complex_ahead(state, spec) {
            let base = base_ty(state, spec)?;
            let (name, ty) = declarator(state, base)?;
            if name.is_empty() {
                let found = state.shown(state.token(0))?;
                return state.fail(format!("expected a name, found {found}"));
            }
            let mut init = None;
            if state.at("=", 0) {
                state.take();
                init = Some(initializer(state)?);
            }
            let (pointer, pointer_const, dims) = (false, false, Vec::new());
            found.push(Declared { name, dims, init, line, pointer, pointer_const, ty: Some(ty) });
            if !state.at(",", 0) {
                break;
            }
            state.take();
            continue;
        }
        let (pointer, pointer_const) = pointer_part(state)?;
        let name = state.name()?;
        let mut dims = Vec::new();
        while state.at("[", 0) {
            state.take();
            let open = dims.is_empty() && state.at("]", 0);
            dims.push(if open { Size::Number(-1) } else { array_size(state)? });
            state.expect("]")?;
        }
        let mut init = None;
        if state.at("=", 0) {
            state.take();
            init = Some(initializer(state)?);
        }
        if dims.first() == Some(&Size::Number(-1)) && init.is_none() && spec.storage.as_deref() != Some("extern") {
            return state.fail(format!("{name}[] needs a size or a list of values"));
        }
        if matches!(init, Some(Init::List(_))) && dims.is_empty() && pointer {
            return state.fail(format!("too many values for {name}"));
        }
        found.push(Declared { name, dims, init, line, pointer, pointer_const, ty: None });
        if !state.at(",", 0) {
            break;
        }
        state.take();
    }
    state.expect(";")?;
    Ok(found)
}

pub(super) fn declaration_ahead(state: &State) -> bool {
    let token = state.token(0);
    if token.kind != Kind::Name {
        return false;
    }
    if token.text() == "enum" {
        let defined = |offset: usize| state.token(offset).kind == Kind::Op && state.token(offset).text() == "{";
        return defined(1) || (state.token(1).kind == Kind::Name && (state.token(2).kind == Kind::Name || defined(2)));
    }
    let following = state.token(1);
    if TYPES.contains(&token.text()) {
        // int (*f)(int): a declarator in parentheses
        return following.kind == Kind::Name || (following.kind == Kind::Op && ["*", "("].contains(&following.text()));
    }
    if starts_type(state, 0) {
        return true;
    }
    if STATEMENT_WORDS.contains(&token.text()) {
        return false;
    }
    // a value kind or typedef from a header (bgm_file track;), which sdk::resolve reads as its base type
    following.kind == Kind::Name
        && state.token(2).text.as_deref().is_some_and(|text| [";", "=", "[", ","].contains(&text))
}

/// The zero an element of a type starts at when its array's initializer leaves it out.
pub(super) fn zero(kind: &str) -> Node {
    if kind == "float" {
        node!({"kind": "float", "bits": 0})
    } else {
        node!({"kind": "int", "value": 0})
    }
}

/// A string's bytes as the elements of a char array (with its 0).
pub(super) fn string_elements(state: &State, text: &str, kind: &str) -> Result<Vec<Node>> {
    let bytes = stb::encode_string(text).or_else(|error| state.fail(error))?;
    Ok(bytes
        .into_iter()
        .map(|byte| node!({"kind": "int", "value": if kind == "char" { byte as i8 as i64 } else { byte as i64 }}))
        .collect())
}

pub(super) fn is_string(init: &Init) -> Option<&str> {
    match init {
        Init::Value(node) if node.kind() == "string" => node["text"].as_str(),
        _ => None,
    }
}

/// Puts an initializer list's values into an array of the sizes given, from base, reading the
/// list from at with C's brace elision (a list for each row, or the values in a row). Gives how
/// many of the first size's rows it reached; a first size of -1 takes as many as the list has.
pub(super) fn fill(
    state: &State,
    items: &[Init],
    at: &mut usize,
    dims: &[i64],
    base: i64,
    kind: &str,
    out: &mut std::collections::BTreeMap<i64, Node>,
) -> Result<i64> {
    let stride: i64 = dims[1..].iter().product();
    let chars = matches!(kind, "char" | "unsigned char");
    let mut row = 0;
    while (dims[0] < 0 || row < dims[0]) && *at < items.len() {
        let item = &items[*at];
        if dims.len() == 1 {
            let value = match item {
                Init::Value(node) => node.clone(),
                Init::List(inner) => match inner.as_slice() {
                    [Init::Value(node)] => node.clone(),
                    _ => return state.fail("a scalar takes one value"),
                },
            };
            out.insert(base + row, value);
            *at += 1;
        } else if let (true, 2, Some(text)) = (chars, dims.len(), is_string(item)) {
            put_string(state, text, dims[1], base + row * stride, kind, out)?;
            *at += 1;
        } else if let Init::List(inner) = item {
            let mut inner_at = 0;
            fill(state, inner, &mut inner_at, &dims[1..], base + row * stride, kind, out)?;
            if inner_at < inner.len() {
                return state.fail("too many values in a row");
            }
            *at += 1;
        } else {
            fill(state, items, at, &dims[1..], base + row * stride, kind, out)?;
        }
        row += 1;
    }
    Ok(row)
}

/// A string into a char array of length places (its 0 left out when the array is exactly its
/// length, as C allows); gives the elements it took.
pub(super) fn put_string(
    state: &State,
    text: &str,
    length: i64,
    base: i64,
    kind: &str,
    out: &mut std::collections::BTreeMap<i64, Node>,
) -> Result<i64> {
    let mut elements = string_elements(state, text, kind)?;
    if length >= 0 && elements.len() as i64 == length + 1 {
        elements.pop();
    }
    if length >= 0 && elements.len() as i64 > length {
        return state.fail("the string is longer than the array");
    }
    let count = elements.len() as i64;
    for (offset, value) in elements.into_iter().enumerate() {
        out.insert(base + offset as i64, value);
    }
    Ok(count)
}

/// The values an array's initializer gives its elements, by offset.
pub(super) type Elements = Vec<(i64, Node)>;

/// An array's sizes as numbers, and the assignments its initializer makes: every element, the
/// ones the list leaves out set to 0 as C sets them.
pub(super) fn array_values(state: &State, declared: &Declared, kind: &str) -> Result<(Vec<i64>, Elements)> {
    let mut dims = Vec::new();
    for (at, size) in declared.dims.iter().enumerate() {
        match size {
            Size::Number(value) => dims.push(*value as i64),
            Size::Named(name) if state.enumerators.contains_key(name) => dims.push(state.enumerators[name] as i64),
            Size::Named(_) if declared.dims.len() == 1 && declared.init.is_none() => {
                // resolved with the headers' constants later
                return Ok((Vec::new(), Vec::new()));
            }
            Size::Named(name) if state.constant(name).is_some() => dims.push(state.constant(name).unwrap() as i64),
            Size::Named(name) => {
                return state.fail(format!(
                    "{}: size {} of {} must be a constant this file defines or includes",
                    declared.name,
                    at + 1,
                    name
                ))
            }
            Size::None => {}
        }
    }
    let Some(init) = &declared.init else {
        if dims.iter().any(|size| *size <= 0) {
            return state.fail(format!("{} needs a size of at least 1", declared.name));
        }
        return Ok((dims, Vec::new()));
    };
    let mut out = std::collections::BTreeMap::new();
    let chars = matches!(kind, "char" | "unsigned char");
    let rows = match (init, is_string(init)) {
        (_, Some(text)) if chars && dims.len() == 1 => put_string(state, text, dims[0], 0, kind, &mut out)?,
        (Init::List(items), _) => {
            let mut at = 0;
            let rows = fill(state, items, &mut at, &dims, 0, kind, &mut out)?;
            if at < items.len() {
                return state.fail(format!("too many values for {}", declared.name));
            }
            rows
        }
        (Init::Value(_), _) => return state.fail(format!("{} is an array; give it a list of values", declared.name)),
    };
    if dims[0] < 0 {
        dims[0] = rows;
    }
    if dims.iter().any(|size| *size <= 0) {
        return state.fail(format!("{} needs a size of at least 1", declared.name));
    }
    let total: i64 = dims.iter().product();
    let values = (0..total).map(|offset| (offset, out.remove(&offset).unwrap_or_else(|| zero(kind)))).collect();
    Ok((dims, values))
}

/// A local declaration anywhere in a function. Every local gets a slot in the function's frame, so a
/// name declared again in another block reuses the slot when the type matches. Initializers become
/// assignments where the declaration stands. A static local is a hidden global, set once before
/// the script runs (see lower.rs).
pub(super) fn declaration(state: &mut State) -> Result<Vec<Node>> {
    if state.scope.is_none() {
        return state.fail("declarations belong inside functions");
    }
    let spec = specifiers(state, true)?;
    if spec.storage.as_deref() == Some("typedef") {
        return local_typedef(state, &spec);
    }
    if state.at(";", 0) && (matches!(spec.ty, Some(Ty::Struct(_))) || spec.kind.starts_with("enum ")) {
        // struct T { ... }; or struct T;: declares the tag, in this block
        state.take();
        return Ok(Vec::new());
    }
    // a typedef of this function's stands for its type
    let kind =
        state.kinds.get(&spec.kind).filter(|_| local_kind(state, &spec.kind)).cloned().unwrap_or(spec.kind.clone());
    if kind == "any" || (kind == "void" && !complex_ahead(state, &spec)) {
        return state.fail(format!("unknown type {kind}"));
    }
    let mut statements = Vec::new();
    for declared in declarators(state, &spec)? {
        if spec.storage.as_deref() == Some("extern") {
            // names a global this file declares
            continue;
        }
        let static_aggregate =
            spec.storage.as_deref() == Some("static") && (declared.pointer || !declared.dims.is_empty());
        let wide = matches!(&declared.init, Some(Init::Value(value)) if value.kind() == "wide");
        if declared.ty.is_some() || static_aggregate || wide {
            statements.extend(typed_local(state, &spec, declared, &kind)?);
            continue;
        }
        let line = declared.line;
        let (dims, values) =
            if declared.dims.is_empty() { (Vec::new(), Vec::new()) } else { array_values(state, &declared, &kind)? };
        let size = match dims.as_slice() {
            [] => declared.size().node()?,
            dims => int_node(dims.iter().product::<i64>() as i128)?,
        };
        let is_static = spec.storage.as_deref() == Some("static");
        let storage = storage_for(state, &declared.name, &kind, &size, &Node::Null, is_static)?;
        if spec.storage.as_deref() == Some("static") {
            static_local(state, &declared, &storage, &kind, &spec)?;
            continue;
        }
        let mut entry = node!({"name": &storage, "type": &kind, "size": size});
        if declared.pointer {
            entry["pointer"] = Node::Bool(true);
        }
        if dims.len() > 1 {
            entry["dims"] = Node::List(dims.iter().map(|size| Node::from(*size)).collect());
        }
        if (spec.constant && !declared.pointer) || declared.pointer_const {
            entry["const"] = Node::Bool(true);
        }
        if spec.constant && declared.pointer {
            entry["pointee_const"] = Node::Bool(true);
        }
        if spec.storage.as_deref() == Some("register") {
            entry["register"] = Node::Bool(true);
        }
        let scope = state.scope.as_mut().unwrap();
        if !scope.locals.contains(&entry) {
            scope.locals.push(entry);
        }
        let assign = |target: Node, value: Node| node!({"kind": "expr", "expr": {"kind": "assign", "target": target, "value": value}, "line": line});
        match declared.init {
            Some(Init::Value(value)) if declared.dims.is_empty() => {
                statements.push(assign(node!({"kind": "var", "name": &storage}), value));
            }
            Some(Init::List(items)) if declared.dims.is_empty() => match items.as_slice() {
                // int x = {5};
                [Init::Value(value)] => {
                    statements.push(assign(node!({"kind": "var", "name": &storage}), value.clone()));
                }
                _ => return state.fail(format!("too many values for {}", declared.name)),
            },
            Some(_) => {
                for (offset, value) in values {
                    let cell = node!({"kind": "index", "name": &storage, "index": {"kind": "int", "value": offset}});
                    statements.push(assign(cell, value));
                }
            }
            None => {}
        }
    }
    Ok(statements)
}

/// A declaration's type when its declarator has the plain fields only (a static or global array
/// or pointer, an array a wide string fills).
pub(super) fn plain_type(state: &State, declared: &Declared, kind: &str) -> Result<Ty> {
    let kind = state.base_kind(kind);
    let scalar = if kind.starts_with("enum ") { ctype::int() } else { Ty::Scalar(kind) };
    if declared.pointer {
        return Ok(ctype::pointer(scalar));
    }
    let mut ty = scalar;
    for size in declared.dims.iter().rev() {
        let size = match size {
            Size::Number(size) => *size as i64,
            Size::Named(name) => match state.constant(name) {
                Some(size) => size as i64,
                None => {
                    return state.fail(format!(
                        "{}: size {name} must be a constant this file defines or includes",
                        declared.name
                    ))
                }
            },
            Size::None => -1,
        };
        ty = Ty::Array(Box::new(ty), size);
    }
    Ok(ty)
}

/// An array's open size ([]) given by its initializer: as many elements as the list fills, or the
/// string's characters and its 0.
pub(super) fn sized_by(state: &State, ty: Ty, init: Option<&Init>) -> Result<Ty> {
    let Ty::Array(of, -1) = &ty else {
        return Ok(ty);
    };
    let count = match init {
        Some(Init::List(items)) => {
            let mut values = std::collections::BTreeMap::new();
            fill_typed(state, &ty, items, &mut 0, 0, &mut values)?
        }
        Some(Init::Value(node)) if node.kind() == "wide" => node["codes"].as_array().map_or(1, Vec::len) as i64,
        Some(Init::Value(node)) if node.kind() == "string" => {
            stb::encode_string(node.text("text")).or_else(|error| state.fail(error))?.len() as i64 + 1
        }
        _ => return Ok(ty),
    };
    Ok(Ty::Array(of.clone(), count))
}

/// Values by slot, as an initializer fills them.
pub(super) type Slots = std::collections::BTreeMap<i64, Node>;

/// One element (or member) an initializer list fills: its type, the slot it starts at, and its
/// bits for a bit-field.
pub(super) struct Part {
    pub(super) ty: Ty,
    pub(super) slot: i64,
    pub(super) bits: Option<ctype::Bits>,
}

/// A scalar's one value: a value, or a list of one.
pub(super) fn one_value(state: &State, item: &Init) -> Result<Node> {
    match item {
        Init::Value(node) => Ok(node.clone()),
        Init::List(inner) => match inner.as_slice() {
            [Init::Value(node)] => Ok(node.clone()),
            _ => state.fail("a scalar takes one value"),
        },
    }
}

/// Puts an initializer list's values into a value of a type, from slot base, reading items from at
/// with C's brace elision; gives how many elements of an array it filled (-1 size: all it can).
pub(super) fn fill_typed(
    state: &State,
    ty: &Ty,
    items: &[Init],
    at: &mut usize,
    base: i64,
    out: &mut Slots,
) -> Result<i64> {
    let Some(parts) = parts(state, ty, items.len(), base)? else {
        // a scalar
        let Some(item) = items.get(*at) else {
            return Ok(0);
        };
        out.insert(base, one_value(state, item)?);
        *at += 1;
        return Ok(1);
    };
    let mut filled = 0;
    for part in parts {
        if *at >= items.len() {
            break;
        }
        fill_part(state, &part, items, at, out)?;
        filled += 1;
    }
    Ok(filled)
}

/// What an array's or struct's initializer fills, in order (None for a scalar): each element, or
/// each named member (a union's first). An open size takes at most an element per value.
pub(super) fn parts(state: &State, ty: &Ty, values: usize, base: i64) -> Result<Option<Vec<Part>>> {
    let structs = &state.structs;
    Ok(Some(match ty {
        Ty::Array(of, size) => {
            let step = of.slots(structs);
            let count = if *size < 0 { values as i64 } else { *size };
            (0..count).map(|index| Part { ty: (**of).clone(), slot: base + index * step, bits: None }).collect()
        }
        Ty::Struct(tag) => {
            let Some(found) = structs.get(tag) else {
                return state.fail(format!("{} is not defined", ty.shown()));
            };
            let mut offset = base;
            let mut parts = Vec::new();
            for (at, (name, member)) in found.members.iter().enumerate() {
                let bits = found.bits_of(at);
                // a bit-field that shares a slot is in the one the last started
                let slot = if bits.is_some_and(|bits| bits.offset > 0) { offset - 1 } else { offset };
                if !name.is_empty() {
                    // an unnamed bit-field takes no value
                    parts.push(Part { ty: member.clone(), slot, bits });
                    if found.union {
                        // a union's initializer sets its first member
                        break;
                    }
                }
                offset += found.slots_of(at, structs);
            }
            parts
        }
        _ => return Ok(None),
    }))
}

/// One part of an aggregate filled from the values at at: a bit-field's bits in its slot, a string
/// in an array of characters, an aggregate from its braces or (their braces elided) from the
/// values here, or a scalar.
pub(super) fn fill_part(state: &State, part: &Part, items: &[Init], at: &mut usize, out: &mut Slots) -> Result<()> {
    let offset = part.slot;
    if let Some(bits) = part.bits {
        // the fields that share a slot are put together in its one value
        let packed = bits.packed(one_value(state, &items[*at])?);
        let value = match out.remove(&offset) {
            Some(known) => ctype::Bits::joined(known, packed),
            None => packed,
        };
        out.insert(offset, value);
        *at += 1;
        return Ok(());
    }
    let characters =
        |of: &Ty| matches!(of, Ty::Scalar(kind) if kind.contains("char") || kind.contains("short") || kind == "int");
    match (&part.ty, &items[*at]) {
        // a string or wide string for an array of characters
        (Ty::Array(of, length), Init::Value(node)) if matches!(node.kind(), "string" | "wide") && characters(of) => {
            put_codes(state, node, of, *length, offset, out)?;
            *at += 1;
        }
        (ty, Init::List(inner)) if ty.is_aggregate() => {
            let mut inner_at = 0;
            fill_typed(state, ty, inner, &mut inner_at, offset, out)?;
            if inner_at < inner.len() {
                return state.fail("too many values in a brace");
            }
            *at += 1;
        }
        (ty, _) if ty.is_aggregate() => {
            fill_typed(state, ty, items, at, offset, out)?;
        }
        (_, item) => {
            out.insert(offset, one_value(state, item)?);
            *at += 1;
        }
    }
    Ok(())
}

/// A string's characters (or a wide string's codes) into an array of length of them from offset:
/// its 0 left out when the array is exactly the string's length, as C allows.
pub(super) fn put_codes(state: &State, node: &Node, of: &Ty, length: i64, offset: i64, out: &mut Slots) -> Result<()> {
    let codes: Vec<Node> = if node.kind() == "wide" {
        node.list("codes").iter().map(|code| node!({"kind": "int", "value": code.clone()})).collect()
    } else {
        string_elements(state, node.text("text"), &of.slot_kind())?
    };
    let fits = if length >= 0 && codes.len() as i64 == length + 1 { codes.len() - 1 } else { codes.len() };
    if length >= 0 && fits as i64 > length {
        return state.fail("the string is longer than the array");
    }
    for (index, code) in codes.into_iter().take(fits).enumerate() {
        out.insert(offset + index as i64, code);
    }
    Ok(())
}

/// The values an initializer gives an object of a type, by slot: every scalar slot, those it
/// leaves out 0, as C sets them (only the given ones for a global, whose slots start at 0).
pub(super) fn initial_values(
    state: &State,
    name: &str,
    ty: &Ty,
    init: &Init,
    every: bool,
) -> Result<Vec<(i64, Ty, Node)>> {
    let mut values = std::collections::BTreeMap::new();
    match init {
        Init::List(items) => {
            let mut at = 0;
            fill_typed(state, ty, items, &mut at, 0, &mut values)?;
            if at < items.len() {
                return state.fail(format!("too many values for {name}"));
            }
        }
        Init::Value(node) if matches!(node.kind(), "string" | "wide") && matches!(ty, Ty::Array(..)) => {
            // char s[] = "...": the array is the string's one place
            let whole = Ty::Array(Box::new(ty.clone()), 1);
            fill_typed(state, &whole, std::slice::from_ref(init), &mut 0, 0, &mut values)?;
        }
        Init::Value(_) => return state.fail(format!("{name} takes a list of values")),
    }
    let mut slots = Vec::new();
    ctype::scalar_slots(ty, &state.structs, 0, &mut slots);
    let mut found = Vec::new();
    for (slot, scalar) in slots {
        match values.remove(&slot) {
            Some(value) => found.push((slot, scalar, value)),
            None if every => {
                let zero = zero(&scalar.slot_kind());
                found.push((slot, scalar, zero));
            }
            None => {}
        }
    }
    Ok(found)
}

/// A slot of an aggregate's storage at a constant offset (composite.rs reads it), with its type.
pub(super) fn slot_field(storage: &str, slot: i64, ty: &Ty) -> Node {
    node!({"kind": "field", "name": storage, "offset": slot, "type": ty.slot_kind(), "ctype": ty.to_node()})
}

/// The fields a local's or global's declaration has for a type of C's: its storage (slots, and
/// whether the slot holds a pointer the VM indexes through) for compile.rs, and the type itself.
pub(super) fn typed_entry(state: &State, name: &str, ty: &Ty) -> Node {
    let ctype = ty.to_node();
    match ty {
        Ty::Array(..) | Ty::Struct(_) => {
            node!({"name": name, "type": "int", "size": ty.slots(&state.structs), "ctype": ctype})
        }
        Ty::Pointer(to) if ty.is_pointer() => {
            let kind = if matches!(**to, Ty::Scalar(_)) { to.slot_kind() } else { "int".into() };
            node!({"name": name, "type": kind, "size": null, "pointer": true, "ctype": ctype})
        }
        _ => node!({"name": name, "type": ty.slot_kind(), "size": null, "ctype": ctype}),
    }
}

/// A local of C's other types (a struct, a pointer to a pointer, a function pointer...) or a
/// static array or pointer: its slots, and assignments for its initializer. A static one is a
/// hidden global, as for static scalars.
pub(super) fn typed_local(state: &mut State, spec: &Spec, declared: Declared, kind: &str) -> Result<Vec<Node>> {
    let ty = match &declared.ty {
        Some(ty) => ty.clone(),
        None => plain_type(state, &declared, kind)?,
    };
    let ty = sized_by(state, ty, declared.init.as_ref())?;
    complete(state, &declared.name, &ty)?;
    let is_static = spec.storage.as_deref() == Some("static");
    let entry = typed_entry(state, "", &ty);
    let storage = storage_for(state, &declared.name, entry.text("type"), &entry["size"], &entry["ctype"], is_static)?;
    let entry = entry.with("name", storage.as_str());
    if is_static {
        let mut global = entry.clone();
        if let Some(map) = global.as_object_mut() {
            map.remove("pointer");
        }
        let global = global_values(state, global, &declared, &ty)?;
        state.statics.push(global.clone());
        state.scope.as_mut().unwrap().statics.insert(storage, global);
        return Ok(Vec::new());
    }
    let scope = state.scope.as_mut().unwrap();
    if !scope.locals.contains(&entry) {
        scope.locals.push(entry);
    }
    let line = declared.line;
    let assign = |target: Node, value: Node| node!({"kind": "expr", "expr": {"kind": "assign", "target": target, "value": value}, "line": line});
    let Some(init) = &declared.init else {
        return Ok(Vec::new());
    };
    let variable = node!({"kind": "var", "name": &storage});
    match (&ty, init) {
        (Ty::Array(..), _) | (Ty::Struct(_), Init::List(_)) => {
            let values = initial_values(state, &declared.name, &ty, init, true)?;
            Ok(values.into_iter().map(|(slot, ty, value)| assign(slot_field(&storage, slot, &ty), value)).collect())
        }
        // a scalar or pointer, or a struct from another (copied, composite.rs)
        (_, Init::Value(value)) => Ok(vec![assign(variable, value.clone())]),
        (_, Init::List(items)) => match items.as_slice() {
            [Init::Value(value)] => Ok(vec![assign(variable, value.clone())]),
            _ => state.fail(format!("too many values for {}", declared.name)),
        },
    }
}

/// A global of C's other types: an array (its slots one after another), a pointer, a struct, a
/// function pointer. Its initializer is set once per load, as a scalar's.
pub(super) fn typed_global(state: &mut State, program: &mut Node, spec: &Spec, declared: Declared) -> Result<()> {
    let ty = match &declared.ty {
        Some(ty) => ty.clone(),
        None => plain_type(state, &declared, &spec.kind)?,
    };
    if let Ty::Function(..) = ty {
        // a function's prototype written as a declarator
        return Ok(());
    }
    let ty = sized_by(state, ty, declared.init.as_ref())?;
    complete(state, &declared.name, &ty)?;
    let mut entry = typed_entry(state, &declared.name, &ty);
    if let Some(map) = entry.as_object_mut() {
        map.remove("pointer");
    }
    let mut entry = global_values(state, entry, &declared, &ty)?;
    if spec.constant {
        entry["const"] = Node::Bool(true);
    }
    let globals = program["globals"].as_array_mut().unwrap();
    match globals.iter_mut().find(|global| global["name"] == *declared.name) {
        Some(existing) if existing["ctype"] == entry["ctype"] => {
            let given = |entry: &Node| entry.get("init").is_some() || entry.get("inits").is_some();
            if given(&entry) {
                if given(existing) {
                    return state.fail(format!("{} is given a value twice", declared.name));
                }
                *existing = entry.clone();
            }
        }
        Some(_) => return state.fail(format!("{} is declared again with another type", declared.name)),
        None => globals.push(entry.clone()),
    }
    state.globals.insert(declared.name.clone(), entry);
    Ok(())
}

/// A global's (or static local's hidden global's) initializer, as its "init" (a scalar's value) or
/// "inits" (each slot an aggregate's list gives), set once per load (lower.rs).
pub(super) fn global_values(state: &State, mut entry: Node, declared: &Declared, ty: &Ty) -> Result<Node> {
    let Some(init) = &declared.init else {
        return Ok(entry);
    };
    if ty.is_aggregate() {
        let values = initial_values(state, &declared.name, ty, init, false)?;
        let inits: Vec<Node> = values
            .into_iter()
            .map(|(slot, ty, value)| node!({"offset": slot, "type": ty.slot_kind(), "ctype": ty.to_node(), "value": value}))
            .collect();
        entry["inits"] = Node::List(inits);
    } else {
        entry["init"] = match init {
            Init::Value(value) => value.clone(),
            Init::List(items) => match items.as_slice() {
                [Init::Value(value)] => value.clone(),
                _ => return state.fail(format!("too many values for {}", declared.name)),
            },
        };
    }
    if state.strict {
        entry["std"] = Node::from("c89");
    }
    Ok(entry)
}

/// Whether a typedef name is one a function declared, which a declaration reads as its base type
/// (a file's typedefs reach sdk::resolve through the program's kinds).
pub(super) fn local_kind(state: &State, name: &str) -> bool {
    state.scope.as_ref().is_some_and(|scope| scope.typedefs.iter().any(|known| known == name))
}

/// typedef of a type beyond the scalars (a struct, a pointer, an array, a function): the name
/// stands for the type from here on. false, reading nothing, for a scalar's typedef.
pub(super) fn typed_typedef(state: &mut State, spec: &Spec) -> Result<bool> {
    let plain = spec.ty.is_none() && state.token(0).kind == Kind::Name && state.at(";", 1);
    if plain {
        return Ok(false);
    }
    let base = base_ty(state, spec)?;
    let (name, ty) = declarator(state, base)?;
    if name.is_empty() {
        return state.fail("a typedef needs a name");
    }
    state.expect(";")?;
    state.types.insert(name, ty);
    Ok(true)
}

/// typedef inside a function: the name stands for the type in the declarations after it.
pub(super) fn local_typedef(state: &mut State, spec: &Spec) -> Result<Vec<Node>> {
    if typed_typedef(state, spec)? {
        return Ok(Vec::new());
    }
    let base = base_type(state, &spec.kind)?;
    let name = state.name()?;
    state.expect(";")?;
    state.kinds.insert(name.clone(), base);
    state.scope.as_mut().unwrap().typedefs.push(name);
    Ok(Vec::new())
}

/// The type a typedef stands for: int, float, string, an integer type, or another typedef's.
pub(super) fn base_type(state: &State, kind: &str) -> Result<String> {
    let base =
        state.kinds.get(kind).or_else(|| state.header_kinds.get(kind)).cloned().unwrap_or_else(|| kind.to_string());
    let base = if base.starts_with("enum ") { "int".to_string() } else { base };
    if TYPES.contains(&base.as_str()) || INTEGER_TYPES.contains(&base.as_str()) || base == "any" {
        return Ok(base);
    }
    state.fail(format!("cannot typedef {kind}"))
}

/// The name a local declared in the innermost block is held under: its own, or, where that would
/// share a slot with a variable it must not (one it hides, one of another type, a static), its name
/// numbered. A static local's is the hidden global's. Refuses a name declared twice in a block.
pub(super) fn storage_for(
    state: &mut State,
    name: &str,
    kind: &str,
    size: &Node,
    ctype: &Node,
    is_static: bool,
) -> Result<String> {
    let scope = state.scope.as_ref().unwrap();
    let innermost = scope.blocks.len() - 1;
    if scope.blocks[innermost].contains_key(name) {
        return state.fail(format!("{name} is declared twice in one block"));
    }
    if innermost == 1 && scope.blocks[0].contains_key(name) {
        return state.fail(format!("{name} is already a parameter"));
    }
    let visible = scope.blocks.iter().any(|block| block.values().any(|storage| storage == name));
    let taken = |candidate: &str| {
        scope.params.iter().chain(&scope.locals).any(|entry| entry["name"] == *candidate)
            || scope.statics.contains_key(candidate)
            || scope.blocks.iter().any(|block| block.values().any(|storage| storage == candidate))
    };
    let reusable = !is_static
        && !visible
        && !scope.params.iter().any(|param| param["name"] == *name)
        && scope.locals.iter().any(|local| {
            local["name"] == *name && local["type"] == *kind && local["size"] == *size && local["ctype"] == *ctype
        });
    let storage = if is_static {
        let mut hidden = format!("__static_{}_{}", scope.function, name);
        let mut count = 2;
        while scope.statics.contains_key(&hidden) {
            hidden = format!("__static_{}_{}_{}", scope.function, name, count);
            count += 1;
        }
        hidden
    } else if reusable || !taken(name) {
        name.to_string()
    } else {
        let mut count = 2;
        while taken(&format!("{name}__{count}")) {
            count += 1;
        }
        format!("{name}__{count}")
    };
    state.scope.as_mut().unwrap().blocks[innermost].insert(name.to_string(), storage.clone());
    Ok(storage)
}

/// A static local: a hidden global that keeps its value between calls, which code in the function
/// reaches by the local's name.
pub(super) fn static_local(
    state: &mut State,
    declared: &Declared,
    hidden: &str,
    kind: &str,
    spec: &Spec,
) -> Result<()> {
    if !declared.dims.is_empty() {
        return state
            .fail(format!("static arrays are not supported: the script VM cannot index a global ({})", declared.name));
    }
    if declared.pointer {
        return state.fail(format!(
            "static pointers are not supported: the script VM cannot index through a global ({})",
            declared.name
        ));
    }
    let mut entry = node!({"name": hidden, "type": kind});
    match &declared.init {
        Some(Init::Value(value)) => entry["init"] = value.clone(),
        Some(Init::List(items)) => match items.as_slice() {
            [Init::Value(value)] => entry["init"] = value.clone(),
            _ => return state.fail(format!("too many values for {}", declared.name)),
        },
        None => {}
    }
    if spec.constant {
        entry["const"] = Node::Bool(true);
    }
    if state.strict && entry.get("init").is_some() {
        entry["std"] = Node::from("c89");
    }
    state.statics.push(entry.clone());
    state.scope.as_mut().unwrap().statics.insert(hidden.to_string(), entry);
    Ok(())
}

pub(super) fn no_parameters(state: &mut State) -> bool {
    if state.at("void", 0) && state.at(")", 1) {
        state.take();
        return true;
    }
    state.at(")", 0)
}

/// A function's parameter list.
pub(super) struct Parameters {
    pub(super) params: Vec<Node>,
    /// A K&R identifier list (int f(a, b) int a; ...), whose types the declarations after the )
    /// give.
    pub(super) identifiers: bool,
    /// ... after the named parameters.
    pub(super) variadic: bool,
}

/// A function's parameters. A prototype's parameters may go unnamed.
pub(super) fn parameters(state: &mut State) -> Result<Parameters> {
    state.expect("(")?;
    let identifiers = state.token(0).kind == Kind::Name
        && !starts_type(state, 0)
        && state.token(1).kind == Kind::Op
        && [",", ")"].contains(&state.token(1).text());
    if identifiers {
        return identifier_list(state);
    }
    let mut params = Vec::new();
    let mut variadic = false;
    if !no_parameters(state) {
        loop {
            if state.at("...", 0) && !params.is_empty() {
                // the arguments after the named ones, which <stdarg.h> reads
                state.take();
                variadic = true;
                break;
            }
            params.push(parameter(state)?);
            if !state.at(",", 0) {
                break;
            }
            state.take();
        }
    }
    state.expect(")")?;
    Ok(Parameters { params, identifiers: false, variadic })
}

/// A K&R definition's parameter names, int until the declarations after the ) give their types.
pub(super) fn identifier_list(state: &mut State) -> Result<Parameters> {
    let mut params = Vec::new();
    loop {
        params.push(node!({"name": state.name()?, "type": "int", "array": false}));
        if !state.at(",", 0) {
            break;
        }
        state.take();
    }
    state.expect(")")?;
    Ok(Parameters { params, identifiers: true, variadic: false })
}

/// One parameter of a prototype's list: its type, then its name, which may go unnamed.
pub(super) fn parameter(state: &mut State) -> Result<Node> {
    let typed = state.at("void", 0) && state.at("*", 1) && state.at("*", 2);
    let void_pointer = !typed && state.at("void", 0) && state.at("*", 1);
    let spec = if void_pointer {
        // void *p: a pointer parameter the function never reads, so of no element type
        state.take();
        Spec { kind: "void".to_string(), ..Spec::default() }
    } else {
        specifiers(state, true)?
    };
    if spec.storage.as_ref().is_some_and(|storage| storage != "register") {
        return state.fail("a parameter's only storage class is register");
    }
    let pointer_ahead = state.at("*", 0);
    if spec.kind == "any" || (spec.kind == "void" && !pointer_ahead) {
        return state.fail(format!("unknown type {}", spec.kind));
    }
    if typed || (!void_pointer && complex_ahead(state, &spec)) {
        return typed_parameter_entry(state, &spec);
    }
    let (pointer, pointer_const) = if spec.kind == "void" && pointer_ahead {
        state.take();
        (true, false)
    } else {
        pointer_part(state)?
    };
    let name = if state.token(0).kind == Kind::Name { state.name()? } else { String::new() };
    let array = state.at("[", 0);
    let inner = if array { parameter_sizes(state)? } else { Vec::new() };
    let kind = if local_kind(state, &spec.kind) { state.kinds[&spec.kind].clone() } else { spec.kind.clone() };
    let mut param = node!({"name": name, "type": kind, "array": array || pointer});
    if !inner.is_empty() {
        param["dims"] = Node::List(inner.into_iter().map(Node::from).collect());
    }
    if (spec.constant && !pointer && !array) || pointer_const {
        param["const"] = Node::Bool(true);
    }
    if spec.constant && (pointer || array) {
        param["pointee_const"] = Node::Bool(true);
    }
    if spec.storage.is_some() {
        param["register"] = Node::Bool(true);
    }
    Ok(param)
}

/// An array parameter's [N][M]...: its first size, which is the pointer's, read and dropped, and
/// the inner ones, which must be constants.
pub(super) fn parameter_sizes(state: &mut State) -> Result<Vec<i64>> {
    state.take();
    if !state.at("]", 0) {
        array_size(state)?;
    }
    state.expect("]")?;
    let mut inner = Vec::new();
    while state.at("[", 0) {
        state.take();
        match array_size(state)? {
            Size::Number(size) => inner.push(size as i64),
            Size::Named(name) if state.constant(&name).is_some() => inner.push(state.constant(&name).unwrap() as i64),
            _ => return state.fail("a parameter's inner array sizes must be constants this file defines or includes"),
        }
        state.expect("]")?;
    }
    Ok(inner)
}

/// A parameter of C's other types: a pointer to a pointer or a struct, a struct (passed by value,
/// composite.rs), a function pointer; an array or function declared as one is its pointer.
pub(super) fn typed_parameter_entry(state: &mut State, spec: &Spec) -> Result<Node> {
    let base = base_ty(state, spec)?;
    let (name, ty) = declarator(state, base)?;
    let ty = adjusted(ty);
    if let Ty::Struct(_) = &ty {
        complete(state, &name, &ty)?;
    }
    let entry = typed_entry(state, &name, &ty);
    let pointer = entry.flag("pointer");
    Ok(node!({"name": name, "type": entry["type"].clone(), "array": pointer, "ctype": entry["ctype"].clone()}))
}

/// The declarations of a K&R definition's parameters, between its ) and its {.
pub(super) fn identifier_types(state: &mut State, params: &mut [Node]) -> Result<()> {
    while !state.at("{", 0) {
        let spec = specifiers(state, true)?;
        if spec.storage.as_ref().is_some_and(|storage| storage != "register") {
            return state.fail("a parameter's only storage class is register");
        }
        for declared in declarators(state, &spec)? {
            if declared.ty.is_some() {
                return state.fail(format!("{}: an old-style parameter list takes plain types", declared.name));
            }
            let Some(param) = params.iter_mut().find(|param| param["name"] == *declared.name) else {
                return state.fail(format!("{} is not a parameter", declared.name));
            };
            if declared.init.is_some() {
                return state.fail(format!("parameter {} cannot have a value", declared.name));
            }
            let array = declared.pointer || !declared.dims.is_empty();
            *param = node!({"name": &declared.name, "type": &spec.kind, "array": array});
            if spec.constant && !array {
                param["const"] = Node::Bool(true);
            }
        }
    }
    Ok(())
}

/// What #pragma lines have set for the next function.
#[derive(Default)]
pub(super) struct Pending {
    pub(super) strings: Option<Vec<Node>>,
    pub(super) padding: Option<Node>,
    pub(super) override_: bool,
    /// What #pragma override NAME names, when it names something: a native the function wraps.
    pub(super) override_of: Option<String>,
    pub(super) every_frame: bool,
}

pub(super) fn function_definition(
    state: &mut State,
    pending: &Pending,
    returns: &str,
    name: &str,
    doc: Option<&str>,
    returns_ty: Option<Ty>,
) -> Result<Option<Node>> {
    if name.starts_with('_') {
        state.pos -= 1;
        return state.fail("function names cannot start with _, which marks natives");
    }
    let Parameters { mut params, identifiers, variadic } = parameters(state)?;
    if state.at(";", 0) {
        // a prototype: calls are checked against the definition
        state.take();
        return Ok(None);
    }
    if identifiers {
        identifier_types(state, &mut params)?;
    }
    if params.iter().any(|param| param["name"] == "") {
        return state.fail(format!("{name}: every parameter of a definition needs a name"));
    }
    state.scope = Some(Scope::new(name, params.clone()));
    let body = braced(state)?;
    let scope = state.scope.take().unwrap();
    if let Some((label, line)) = scope.gotos.iter().find(|(label, _)| !scope.labels.contains(label)) {
        return lex_fail(*line, format!("goto {label}: no such label in {name}"));
    }
    let locals = scope.locals;
    let mut function = node!({
        "kind": "function",
        "name": name,
        "returns": returns,
        "params": params,
        "locals": locals,
        "body": body,
        "unused_strings": pending.strings.clone().unwrap_or_default(),
        "padding": pending.padding.clone(),
    });
    if let Some(doc) = doc.filter(|doc| !doc.is_empty()) {
        function["doc"] = Node::from(doc);
    }
    if pending.override_ {
        function["override"] = Node::Bool(true);
    }
    if let Some(of) = &pending.override_of {
        function["override_of"] = Node::from(of.as_str());
    }
    if pending.every_frame {
        function["every_frame"] = Node::Bool(true);
    }
    if variadic {
        function["variadic"] = Node::Bool(true);
    }
    if let Some(returns) = returns_ty {
        function["ctype_returns"] = returns.to_node();
    }
    if state.strict {
        function["std"] = Node::from("c89");
    }
    Ok(Some(function))
}

/// Whether an extern declares a native (extern int f(...) NATIVE(n);) rather than a C object or
/// function: NATIVE comes before the ;.
pub(super) fn native_ahead(state: &State) -> bool {
    (0..)
        .map(|offset| state.token(offset))
        .take_while(|token| token.kind != Kind::Eof && token.text() != ";")
        .any(|token| token.kind == Kind::Name && token.text() == "NATIVE")
}

/// A declaration at file scope: a function's definition or prototype, or globals. A global's value
/// is set once before the script runs (see lower.rs). Ok(true) for a function's definition.
pub(super) fn top_declaration(
    state: &mut State,
    program: &mut Node,
    pending: &Pending,
    doc: Option<&str>,
) -> Result<bool> {
    let spec = specifiers(state, false)?;
    if matches!(spec.storage.as_deref(), Some("auto" | "register")) {
        return state.fail("auto and register belong inside functions");
    }
    if state.at(";", 0) {
        state.take();
        return Ok(false);
    }
    // a function returning a pointer: its *s, then its name and (
    let stars = (0..).take_while(|offset| state.at("*", *offset)).count();
    if state.token(stars).kind == Kind::Name && state.at("(", stars + 1) {
        let mut returns_ty = spec.ty.clone();
        if stars > 0 {
            let mut ty = base_ty(state, &spec)?;
            for _ in 0..stars {
                state.take();
                ty = ctype::pointer(ty);
            }
            returns_ty = Some(ty);
        }
        let returns = if returns_ty.is_some() { "int".to_string() } else { spec.kind.clone() };
        let name = state.name()?;
        let function = function_definition(state, pending, &returns, &name, doc, returns_ty.clone())?;
        let Some(function) = function else {
            return Ok(false);
        };
        program["functions"].as_array_mut().unwrap().push(function);
        return Ok(true);
    }
    if spec.kind == "void" && !complex_ahead(state, &spec) {
        return state.fail("variables cannot be void");
    }
    for declared in declarators(state, &spec)? {
        if declared.ty.is_some() || declared.size() != &Size::None || declared.pointer {
            let open = declared.size() == &Size::Number(-1) || matches!(declared.ty, Some(Ty::Array(_, -1)));
            if spec.storage.as_deref() == Some("extern") && (open || state.globals.contains_key(&declared.name)) {
                // extern int table[];: declares a global another declaration defines
                continue;
            }
            typed_global(state, program, &spec, declared)?;
            continue;
        }
        let mut entry = node!({"name": &declared.name, "type": &spec.kind});
        match declared.init {
            Some(Init::Value(value)) => entry["init"] = value,
            Some(Init::List(items)) => match items.as_slice() {
                [Init::Value(value)] => entry["init"] = value.clone(),
                _ => return state.fail(format!("too many values for {}", declared.name)),
            },
            None => {}
        }
        if spec.constant {
            entry["const"] = Node::Bool(true);
        }
        if state.strict && entry.get("init").is_some() {
            entry["std"] = Node::from("c89");
        }
        let globals = program["globals"].as_array_mut().unwrap();
        match globals.iter_mut().find(|global| global["name"] == *declared.name) {
            // a tentative definition, or an extern declaration, of a global declared before
            Some(existing) if existing["type"] == entry["type"] => {
                if entry.get("init").is_some() {
                    if existing.get("init").is_some() {
                        return state.fail(format!("{} is given a value twice", declared.name));
                    }
                    *existing = entry.clone();
                }
            }
            Some(_) => return state.fail(format!("{} is declared again with another type", declared.name)),
            None => globals.push(entry.clone()),
        }
        state.globals.insert(declared.name.clone(), entry);
    }
    Ok(false)
}
