//! C's pointers and composite types lowered to what the VM has (docs/c-standard.md), after the
//! initialised globals (lower::initialised_globals) and before the integer types. Every expression
//! is typed; the forms the compiler had before come out as they went in, so retail scripts keep
//! their bytes, and the new ones become the VM's slots:
//!
//! - **An lvalue** is a place: a variable, a slot of a variable's storage at a constant offset
//!   ("field": the VM's own local or global mode, no index), a slot of a local's storage or
//!   through a pointer local at an index ("index", the indexed modes), or a slot through a pointer
//!   an expression gives ("at": the pointer is stored in a hidden local first, as the VM indexes
//!   through a local only). Each carries the slot's type, for the float modes.
//! - **Pointers** are the VM's own values: an address from LOAD_ADDR. p + n is &p[n] (counting
//!   slots), *p is p[0]. A pointer to a global indexes through it as through one to a local: the
//!   globals are the bottom of the value stack the frames are on, and a pointer is a place in it.
//!   That gives global and static arrays (an element at a constant index is the global slot
//!   itself, at a variable one through the array's address) and global pointers. The VM gives no
//!   address as a number, so a pointer compares only with a null pointer constant (!p, !!p), and
//!   two pointers neither compare nor subtract.
//! - **Structs and unions** are their members' slots; a member is a constant offset into them. A
//!   struct is copied slot by slot (each copied as it is), passed by its address and copied by the
//!   function it is passed to (into a hidden local, so the caller's stays as it was), and returned
//!   through a hidden first parameter, __result, that points where the caller wants it.
//! - **Function pointers** are numbers: a function whose address is taken gets one (from 1, so 0
//!   is null), and a call through a pointer calls __call_N, a function this pass adds for each
//!   number of arguments, which switches on the number to the function.
//! - **Variadic functions** take a hidden last parameter, __va, pointing at a hidden array in the
//!   caller's frame that holds the arguments after the named ones; va_list is a pointer into it,
//!   va_arg steps it.
//! - **String literals as arrays**: a string literal is the VM's string, which natives take, but
//!   one subscripted, dereferenced, or stored in, passed as or returned as a char pointer is an
//!   array of its characters and a 0, as in C; so is every wide string (L"..."). The array is a
//!   hidden global (a literal's storage lasts the program), set before the statement that uses
//!   it.
//!
//! In strict mode (#pragma std c89) C's diagnostics for mixing pointers and integers and for
//! incompatible pointers are given.

use std::collections::{BTreeSet, HashMap};

use indexmap::IndexMap;

use crate::ctype::{self, Structs, Ty};
use crate::node;
use crate::node::{Map, Node};
use crate::sdk::Context;
use crate::{Error, Result};

fn list<'a>(node: &'a Node, key: &str) -> &'a [Node] {
    node[key].as_array().map(Vec::as_slice).unwrap_or(&[])
}

fn text<'a>(node: &'a Node, key: &str) -> &'a str {
    node[key].as_str().unwrap_or("")
}

fn fail<T>(reason: impl Into<String>) -> Result<T> {
    Err(Error::invalid(reason))
}

fn int_node(value: i64) -> Node {
    node!({"kind": "int", "value": value})
}

fn var(name: &str) -> Node {
    node!({"kind": "var", "name": name})
}

fn assign(target: Node, value: Node) -> Node {
    node!({"kind": "assign", "target": target, "value": value})
}

fn address_of(target: Node) -> Node {
    node!({"kind": "addr", "target": target})
}

fn not(operand: Node) -> Node {
    node!({"kind": "unary", "op": "!", "operand": operand})
}

/// Expressions worked out in order, the last one's value the whole one's.
fn sequence(mut parts: Vec<Node>) -> Node {
    let mut whole = parts.pop().expect("a sequence has a part");
    while let Some(part) = parts.pop() {
        whole = node!({"kind": "comma", "left": part, "right": whole});
    }
    whole
}

/// index + more, folded when both are numbers.
fn plus(index: Node, more: Node) -> Node {
    match (index.kind(), more.kind()) {
        ("int", "int") if index.get("unsigned").is_none() && more.get("unsigned").is_none() => {
            int_node(index["value"].as_i64().unwrap_or(0) + more["value"].as_i64().unwrap_or(0))
        }
        ("int", _) if index["value"] == Node::Int(0) => more,
        (_, "int") if more["value"] == Node::Int(0) => index,
        _ => node!({"kind": "binary", "op": "+", "left": index, "right": more}),
    }
}

fn times(value: Node, factor: i64) -> Node {
    if factor == 1 {
        return value;
    }
    if value.kind() == "int" && value.get("unsigned").is_none() {
        return int_node(value["value"].as_i64().unwrap_or(0) * factor);
    }
    node!({"kind": "binary", "op": "*", "left": value, "right": int_node(factor)})
}

/// Whether an expression assigns or calls.
fn has_effects(node: &Node) -> bool {
    match node {
        Node::List(items) => items.iter().any(has_effects),
        Node::Map(map) => {
            matches!(node.kind(), "assign" | "compound" | "call" | "call_ptr" | "va_arg")
                || map.values().any(has_effects)
        }
        _ => false,
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Storage {
    /// Slots of the frame: a local, or a parameter holding a value.
    Local,
    /// A local or parameter holding a pointer the VM indexes through.
    Pointer,
    Global,
}

#[derive(Clone, Debug)]
struct Variable {
    ty: Ty,
    storage: Storage,
    /// The name its storage has in the tree (a struct parameter's copy is another local).
    name: String,
    /// Declared with a C type of its own (not one of the forms the compiler had before).
    typed: bool,
}

#[derive(Clone, Debug)]
struct Signature {
    returns: Ty,
    params: Vec<Ty>,
    variadic: bool,
}

/// What an lvalue designates.
#[derive(Clone, Debug)]
enum Place {
    /// A variable as a whole, by its storage's name.
    Var(String),
    /// A slot of a variable's storage at a constant offset.
    Field(String, i64),
    /// A slot of a local's storage, or through a pointer local, at an index.
    Index(String, Node),
    /// A slot through the pointer an expression gives, at an index.
    At(Node, Node),
    /// An access the compiler read before (var, index, deref of a variable), as it was.
    Plain(Node),
    /// A bit-field: some bits of an int slot.
    Bits(Box<Place>, ctype::Bits),
}

/// An expression lowered and its type; an array's value is its first element's address (typed as
/// a pointer to it), a struct's its address.
#[derive(Clone, Debug)]
struct Typed {
    node: Node,
    ty: Ty,
}

impl Typed {
    fn new(node: Node, ty: Ty) -> Typed {
        Typed { node, ty }
    }
}

/// The base of a scalar type's name: a typedef's type, int for an enum.
fn scalar_base(name: &str, kinds: &Map) -> String {
    if name.starts_with("enum ") {
        return "int".into();
    }
    kinds.get(name).and_then(Node::as_str).map_or_else(|| name.to_string(), str::to_string)
}

fn is_integer(ty: &Ty) -> bool {
    matches!(ty, Ty::Scalar(name) if name != "float" && name != "string" && name != "any")
}

fn variable_of(entry: &Node, global: bool, parameter: bool) -> Variable {
    let name = text(entry, "name").to_string();
    let kind = text(entry, "type");
    let typed = entry.get("ctype").is_some();
    let ty = match entry.get("ctype") {
        Some(ctype) => Ty::from_node(ctype),
        None if parameter && entry["array"] == Node::Bool(true) => {
            ctype::pointer(if kind == "void" { Ty::Void } else { Ty::Scalar(kind.into()) })
        }
        None if entry.get("pointer").is_some_and(|pointer| *pointer == Node::Bool(true)) => {
            ctype::pointer(Ty::Scalar(kind.into()))
        }
        None if !parameter && !entry["size"].is_null() => {
            Ty::Array(Box::new(Ty::Scalar(kind.into())), entry["size"].as_i64().unwrap_or(-1))
        }
        None => Ty::Scalar(kind.into()),
    };
    let pointer = if parameter { entry["array"] == Node::Bool(true) } else { entry.get("pointer").is_some() };
    let storage = if global {
        Storage::Global
    } else if pointer {
        Storage::Pointer
    } else {
        Storage::Local
    };
    Variable { ty, storage, name, typed }
}

fn signature_of(function: &Node) -> Signature {
    let returns = match function.get("ctype_returns") {
        Some(ctype) => Ty::from_node(ctype),
        None if function["returns"] == "void" => Ty::Void,
        None => Ty::Scalar(text(function, "returns").to_string()),
    };
    let params = list(function, "params").iter().map(|param| variable_of(param, false, true).ty).collect();
    Signature { returns, params, variadic: function["variadic"] == Node::Bool(true) }
}

struct Lowering<'a> {
    structs: Structs,
    kinds: &'a Map,
    found: &'a Context,
    globals: HashMap<String, Variable>,
    signatures: HashMap<String, Signature>,
    /// The current function's variables, by the name code uses.
    variables: HashMap<String, Variable>,
    /// The hidden locals the current function needs.
    hidden: Vec<Node>,
    /// Statements to run before the current one (a wide string's values).
    before: Vec<Node>,
    /// Functions whose address is taken, with their numbers.
    numbered: IndexMap<String, i64>,
    /// The numbers of arguments calls through pointers pass.
    arities: BTreeSet<usize>,
    /// The current function's name, and whether it is written in strict mode.
    function: String,
    strict: bool,
    /// Hidden globals: the arrays string literals become.
    literals: Vec<Node>,
}

/// The program with C's pointers and composite types lowered; one that uses none comes out as
/// it went in.
pub fn lower(program: &Node, kinds: &Map, found: &Context) -> Result<Node> {
    if program.get("asm").is_some() {
        return Ok(program.clone());
    }
    let globals = list(program, "globals")
        .iter()
        .map(|global| (text(global, "name").to_string(), variable_of(global, true, false)))
        .collect();
    let signatures = list(program, "functions")
        .iter()
        .filter(|function| function.kind() == "function")
        .map(|function| (text(function, "name").to_string(), signature_of(function)))
        .collect();
    let mut lowering = Lowering {
        structs: ctype::structs_of(program),
        kinds,
        found,
        globals,
        signatures,
        variables: HashMap::new(),
        hidden: Vec::new(),
        before: Vec::new(),
        numbered: IndexMap::new(),
        arities: BTreeSet::new(),
        function: String::new(),
        strict: false,
        literals: Vec::new(),
    };
    let mut functions = Vec::new();
    for function in list(program, "functions") {
        if function.kind() != "function" {
            functions.push(function.clone());
            continue;
        }
        functions.push(lowering.function(function)?);
    }
    for arity in lowering.arities.clone() {
        functions.push(lowering.dispatch(arity));
    }
    let mut program = program.clone().with("functions", functions);
    if !lowering.literals.is_empty() {
        let mut globals = list(&program, "globals").to_vec();
        globals.extend(lowering.literals);
        let slots: i64 = globals.iter().map(|global| global["size"].as_i64().unwrap_or(1)).sum();
        let declared = program["global_num"].as_i64().unwrap_or(0);
        program = program.with("globals", globals).with("global_num", slots.max(declared));
    }
    Ok(program)
}

/// Whether a type is a char's, which a string literal is an array of.
fn is_char(ty: &Ty, kinds: &Map) -> bool {
    matches!(ty, Ty::Scalar(name) if matches!(scalar_base(name, kinds).as_str(), "char" | "unsigned char" | "signed char"))
}

impl Lowering<'_> {
    fn lookup(&self, name: &str) -> Option<Variable> {
        self.variables.get(name).or_else(|| self.globals.get(name)).cloned()
    }

    /// The kind of slot a scalar type is read as (the compiler's float modes or not).
    fn slot_kind(&self, ty: &Ty) -> String {
        match ty {
            Ty::Scalar(name) => scalar_base(name, self.kinds),
            _ => "int".into(),
        }
    }

    fn slots(&self, ty: &Ty) -> i64 {
        ty.slots(&self.structs).max(1)
    }

    fn function(&mut self, function: &Node) -> Result<Node> {
        self.function = text(function, "name").to_string();
        self.strict = crate::lower::is_strict(function);
        self.variables.clear();
        self.hidden.clear();
        let signature = self.signatures[&self.function].clone();
        let mut params = Vec::new();
        let mut prologue = Vec::new();
        if signature.returns.is_struct() {
            // where the caller wants the struct the function returns
            params.push(node!({"name": "__result", "type": "int", "array": true}));
            let ty = ctype::pointer(signature.returns.clone());
            self.variables.insert(
                "__result".into(),
                Variable { ty, storage: Storage::Pointer, name: "__result".into(), typed: true },
            );
        }
        for param in list(function, "params") {
            let found = variable_of(param, false, true);
            if !found.ty.is_struct() {
                self.variables.insert(found.name.clone(), found);
                params.push(param.clone());
                continue;
            }
            // passed by its address, and copied, so the caller's stays as it was
            let copy = self.hidden_storage("copy", &found.ty);
            let pointer = ctype::pointer(found.ty.clone());
            params.push(param.clone().with("type", "int").with("array", true).with("ctype", pointer.to_node()));
            for slot in 0..self.slots(&found.ty) {
                let to = node!({"kind": "field", "name": copy.as_str(), "offset": slot, "type": "int"});
                let from =
                    node!({"kind": "index", "name": found.name.as_str(), "index": int_node(slot), "type": "int"});
                prologue.push(node!({"kind": "expr", "expr": assign(to, from)}));
            }
            self.variables.insert(found.name.clone(), Variable { name: copy, storage: Storage::Local, ..found });
        }
        if signature.variadic {
            params.push(node!({"name": "__va", "type": "int", "array": true}));
            let ty = ctype::pointer(ctype::int());
            self.variables
                .insert("__va".into(), Variable { ty, storage: Storage::Pointer, name: "__va".into(), typed: true });
        }
        for local in list(function, "locals") {
            let found = variable_of(local, false, false);
            self.variables.insert(found.name.clone(), found);
        }
        let body = self.statements(list(function, "body"))?;
        let mut lowered = function.clone();
        if !prologue.is_empty() || signature.returns.is_struct() || signature.variadic {
            lowered = lowered.with("params", params);
        }
        if signature.returns.is_struct() {
            lowered = lowered.with("returns", "void");
        }
        if !prologue.is_empty() {
            prologue.extend(body);
            lowered = lowered.with("body", prologue);
        } else {
            lowered = lowered.with("body", body);
        }
        if !self.hidden.is_empty() {
            let mut locals = list(function, "locals").to_vec();
            locals.extend(std::mem::take(&mut self.hidden));
            lowered = lowered.with("locals", locals);
        }
        Ok(lowered)
    }

    /// A hidden local of a struct's or array's slots.
    fn hidden_storage(&mut self, purpose: &str, ty: &Ty) -> String {
        let name = format!("__{purpose}_{}", self.hidden.len());
        self.hidden.push(node!({"name": name.as_str(), "type": "int", "size": self.slots(ty), "ctype": ty.to_node()}));
        let variable = Variable { ty: ty.clone(), storage: Storage::Local, name: name.clone(), typed: true };
        self.variables.insert(name.clone(), variable);
        name
    }

    /// A hidden local holding a pointer, which the VM indexes through.
    fn hidden_pointer(&mut self, ty: &Ty) -> String {
        let name = format!("__pointer_{}", self.hidden.len());
        self.hidden.push(node!({"name": name.as_str(), "type": "int", "size": null, "pointer": true}));
        let variable = Variable { ty: ty.clone(), storage: Storage::Pointer, name: name.clone(), typed: true };
        self.variables.insert(name.clone(), variable);
        name
    }

    /// A hidden int local.
    fn hidden_int(&mut self) -> String {
        let name = format!("__value_{}", self.hidden.len());
        self.hidden.push(node!({"name": name.as_str(), "type": "int", "size": null}));
        let variable = Variable { ty: ctype::int(), storage: Storage::Local, name: name.clone(), typed: true };
        self.variables.insert(name.clone(), variable);
        name
    }

    /// A bit-field stored: its slot's other bits kept and the new value's put in its own, the value
    /// worked out from the field's current one by value_of. The expression's value is the field's
    /// after the store, read back (so it is the value wrapped to the field's width).
    fn store_bits(&mut self, slot: Place, bits: ctype::Bits, ty: &Ty, value_of: impl FnOnce(Node) -> Node) -> Typed {
        let int = ctype::int();
        let signed = self.slot_kind(ty) != "unsigned int";
        let mut parts = Vec::new();
        let slot = self.stable(slot, &int, &mut parts);
        let current = bits.read(self.access(&slot, &int), signed);
        let mut value = value_of(current);
        if has_effects(&value) {
            // worked out before the slot is read for the store, which it may change
            let held = self.hidden_int();
            parts.push(assign(var(&held), value));
            value = var(&held);
        }
        let stored = bits.stored(self.access(&slot, &int), bits.packed(value));
        parts.push(assign(self.access(&slot, &int), stored));
        parts.push(bits.read(self.access(&slot, &int), signed));
        Typed::new(sequence(parts), ty.clone())
    }

    // ---- statements ------------------------------------------------------------------------

    fn statements(&mut self, nodes: &[Node]) -> Result<Vec<Node>> {
        let mut out = Vec::new();
        for node in nodes {
            match self.statement(node) {
                Ok(items) => out.extend(items),
                Err(Error::Invalid(reason)) if !node["line"].is_null() && !reason.starts_with("line ") => {
                    return fail(format!("line {}: {reason}", crate::source::py_str(&node["line"])));
                }
                Err(error) => return Err(error),
            }
        }
        Ok(out)
    }

    fn statement(&mut self, node: &Node) -> Result<Vec<Node>> {
        let saved = std::mem::take(&mut self.before);
        let lowered = match node.kind() {
            "expr" => node.clone().with("expr", self.rvalue(&node["expr"])?.node),
            "return" if !node["value"].is_null() => self.return_statement(node)?,
            "if" => {
                let cond = self.condition(&node["cond"])?;
                let mut out = node.clone().with("cond", cond).with("then", self.statements(list(node, "then"))?);
                if let Some(otherwise) = node["else"].as_array() {
                    out = out.with("else", self.statements(otherwise)?);
                }
                out
            }
            "while" => {
                let cond = self.condition(&node["cond"])?;
                node.clone().with("cond", cond).with("body", self.statements(list(node, "body"))?)
            }
            "switch" => {
                let value = self.scalar(&node["value"], "a switch's value")?;
                let mut cases = Vec::new();
                for case in list(node, "cases") {
                    let mut case = case.clone().with("body", self.statements(list(case, "body"))?);
                    if !case["value"].is_null() {
                        let value = self.rvalue(&case["value"])?.node;
                        case = case.with("value", value);
                    }
                    cases.push(case);
                }
                node.clone().with("value", value.node).with("cases", cases)
            }
            "print" | "native" => {
                let mut args = Vec::new();
                for arg in list(node, "args") {
                    args.push(self.scalar(arg, "an argument")?.node);
                }
                node.clone().with("args", args)
            }
            _ => node.clone(),
        };
        let mut out = std::mem::replace(&mut self.before, saved);
        out.push(lowered);
        Ok(out)
    }

    fn return_statement(&mut self, node: &Node) -> Result<Node> {
        let returns = self.signatures[&self.function].returns.clone();
        if returns.is_struct() {
            let value = self.rvalue(&node["value"])?;
            self.same_struct(&returns, &value.ty, "returning")?;
            let target = Place::Index("__result".into(), int_node(0));
            let copy = self.copy(target, value.node, &returns);
            self.before.push(node!({"kind": "expr", "expr": copy}));
            return Ok(node.clone().with("value", Node::Null));
        }
        let value = self.value_for(&node["value"], &returns)?;
        self.assignable(&returns, &value, "returning")?;
        Ok(node.clone().with("value", value.node))
    }

    /// A condition: a scalar or a pointer (tested as the VM tests it: a pointer is true).
    fn condition(&mut self, node: &Node) -> Result<Node> {
        Ok(self.scalar(node, "a condition")?.node)
    }

    /// An expression whose value must be one slot: not a struct.
    fn scalar(&mut self, node: &Node, what: &str) -> Result<Typed> {
        let value = self.rvalue(node)?;
        if value.ty.is_struct() {
            return fail(format!("{what} cannot be a struct"));
        }
        Ok(value)
    }

    // ---- places ----------------------------------------------------------------------------

    /// What an lvalue designates, and its type.
    fn place(&mut self, node: &Node) -> Result<(Place, Ty)> {
        match node.kind() {
            "var" => {
                let name = text(node, "name");
                match self.lookup(name) {
                    Some(found) => Ok((Place::Var(found.name.clone()), found.ty)),
                    None if self.signatures.contains_key(name) => fail(format!("{name} is a function, not a variable")),
                    None => Ok((Place::Plain(node.clone()), ctype::int())),
                }
            }
            "index" => {
                let name = text(node, "name");
                let found = self.lookup(name);
                match found {
                    Some(found) if !found.typed && found.storage != Storage::Global && found.ty.target().is_some() => {
                        let index = self.scalar(&node["index"], "an index")?.node;
                        let element = found.ty.target().cloned().unwrap_or_else(ctype::int);
                        Ok((Place::Plain(node.clone().with("index", index)), element))
                    }
                    Some(found) if found.ty.target().is_some() => {
                        let array = var(name);
                        self.subscript_place(&array, &node["index"])
                    }
                    Some(_) => {
                        // i[a] is a[i]
                        let index = var(name);
                        self.subscript_place(&node["index"], &index)
                    }
                    None => {
                        let index = self.scalar(&node["index"], "an index")?.node;
                        Ok((Place::Plain(node.clone().with("index", index)), ctype::int()))
                    }
                }
            }
            "deref" => match self.plain_deref(node) {
                Some(found) => Ok(found),
                None => {
                    let pointer = self.pointed(&node["operand"])?;
                    self.deref_place(pointer)
                }
            },
            "subscript" => self.subscript_place(&node["array"], &node["index"]),
            "member" => {
                let (base, ty) = if node["arrow"] == Node::Bool(true) {
                    let pointer = self.rvalue(&node["object"])?;
                    self.deref_place(pointer)?
                } else if matches!(node["object"].kind(), "var" | "member" | "subscript" | "deref" | "index") {
                    self.place(&node["object"])?
                } else {
                    // a struct a call or assignment gives: its address
                    let value = self.rvalue(&node["object"])?;
                    self.deref_place(Typed::new(value.node, ctype::pointer(value.ty)))?
                };
                let name = text(node, "name");
                let Ty::Struct(tag) = &ty else {
                    let shown = if node["arrow"] == Node::Bool(true) { "->" } else { "." };
                    return fail(format!("{shown}{name} of {}, which is not a struct or union", ty.shown()));
                };
                let Some((offset, _, member, bits)) = ctype::member_bits(&self.structs, tag, name) else {
                    return fail(format!("{} has no member {name}", ty.shown()));
                };
                let place = self.offset(base, offset, None);
                Ok(match bits {
                    Some(bits) => (Place::Bits(Box::new(place), bits), member),
                    None => (place, member),
                })
            }
            "field" => {
                // an initializer's slot (parse.rs), of its C type, read as its type's base
                let ty = node.get("ctype").map_or_else(|| Ty::Scalar(text(node, "type").to_string()), Ty::from_node);
                let mut plain = node.clone().with("type", self.slot_kind(&ty));
                if let Some(map) = plain.as_object_mut() {
                    map.remove("ctype");
                }
                Ok((Place::Plain(plain), ty))
            }
            _ => {
                let value = self.rvalue(node)?;
                if value.ty.is_struct() {
                    return self.deref_place(Typed::new(value.node, ctype::pointer(value.ty)));
                }
                fail("cannot assign to this expression, or take its address")
            }
        }
    }

    /// *p of a pointer or array variable of the forms the compiler had before, as it was.
    fn plain_deref(&self, node: &Node) -> Option<(Place, Ty)> {
        let operand = &node["operand"];
        if operand.kind() != "var" {
            return None;
        }
        let found = self.lookup(text(operand, "name"))?;
        if found.typed || found.storage == Storage::Global {
            return None;
        }
        Some((Place::Plain(node.clone()), found.ty.target()?.clone()))
    }

    /// array[index], either way round.
    fn subscript_place(&mut self, array: &Node, index: &Node) -> Result<(Place, Ty)> {
        let (mut array, mut index) = (self.pointed(array)?, self.pointed(index)?);
        if array.ty.target().is_none() && index.ty.target().is_some() {
            std::mem::swap(&mut array, &mut index);
        }
        if !is_integer(&index.ty) {
            return fail(format!("an index is an integer, not {}", index.ty.shown()));
        }
        let element = self.element(&array.ty)?;
        let (base, _) = self.deref_place(array)?;
        let step = self.slots(&element);
        Ok((self.offset(base, 0, Some(times(index.node, step))), element))
    }

    /// What a pointer points at, refusing what has no element.
    fn element(&self, ty: &Ty) -> Result<Ty> {
        match ty.target() {
            Some(Ty::Void) => fail("a void pointer has no element; cast it to a pointer of a type"),
            Some(Ty::Function(..)) => fail("a function pointer has no element"),
            Some(target) => Ok(target.clone()),
            None => fail(format!("{} is not a pointer or array", ty.shown())),
        }
    }

    /// *pointer.
    fn deref_place(&mut self, pointer: Typed) -> Result<(Place, Ty)> {
        let element = self.element(&pointer.ty)?;
        let place = self.place_of_address(&pointer.node).unwrap_or(Place::At(pointer.node, int_node(0)));
        Ok((place, element))
    }

    /// The place an address expression is the address of, when the expression says it.
    fn place_of_address(&self, node: &Node) -> Option<Place> {
        match node.kind() {
            "addr" => {
                let target = &node["target"];
                Some(match target.kind() {
                    "var" => Place::Var(text(target, "name").to_string()),
                    "field" => Place::Field(text(target, "name").to_string(), target["offset"].as_i64().unwrap_or(0)),
                    "index" => Place::Index(text(target, "name").to_string(), target["index"].clone()),
                    "at" => Place::At(target["pointer"].clone(), target["index"].clone()),
                    "deref" if target["operand"].kind() == "var" => {
                        Place::Index(text(&target["operand"], "name").to_string(), int_node(0))
                    }
                    _ => return None,
                })
            }
            "var" => {
                let found = self.lookup(text(node, "name"))?;
                match (found.storage, &found.ty) {
                    (Storage::Pointer, _) => Some(Place::Index(found.name, int_node(0))),
                    (Storage::Local, ty) if ty.is_aggregate() => Some(Place::Var(found.name)),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// A place moved on by slots: a constant offset and an index.
    fn offset(&self, place: Place, by: i64, index: Option<Node>) -> Place {
        // a constant index is an offset: a[2] of a global array is its slot
        let (by, index) = match index {
            Some(index) if index.kind() == "int" && index.get("unsigned").is_none() => {
                (by + index["value"].as_i64().unwrap_or(0), None)
            }
            other => (by, other),
        };
        let more = plus(index.clone().unwrap_or_else(|| int_node(0)), int_node(by));
        match place {
            Place::Var(_) | Place::Field(..) if index.is_some() => {
                let (name, start) = match place {
                    Place::Field(name, start) => (name, start),
                    Place::Var(name) => (name, 0),
                    _ => unreachable!(),
                };
                let global = self.lookup(&name).is_none_or(|found| found.storage == Storage::Global);
                if global {
                    Place::At(address_of(var(&name)), plus(more, int_node(start)))
                } else {
                    Place::Index(name, plus(more, int_node(start)))
                }
            }
            Place::Var(name) => Place::Field(name, by),
            Place::Field(name, start) => Place::Field(name, start + by),
            Place::Index(name, at) => Place::Index(name, plus(at, more)),
            Place::At(pointer, at) => Place::At(pointer, plus(at, more)),
            Place::Plain(node) => {
                let place = match node.kind() {
                    "var" => Place::Var(text(&node, "name").to_string()),
                    "index" => Place::Index(text(&node, "name").to_string(), node["index"].clone()),
                    "field" => Place::Field(text(&node, "name").to_string(), node["offset"].as_i64().unwrap_or(0)),
                    "deref" => Place::Index(text(&node["operand"], "name").to_string(), int_node(0)),
                    _ => return Place::At(address_of(node), more),
                };
                self.offset(place, by, index)
            }
            // a bit-field is a scalar: nothing is reached past it
            Place::Bits(slot, _) => self.offset(*slot, by, index),
        }
    }

    /// The node that reads or writes a place holding a scalar of a type.
    fn access(&self, place: &Place, ty: &Ty) -> Node {
        let kind = self.slot_kind(ty);
        match place {
            Place::Var(name) => var(name),
            Place::Field(name, offset) => {
                node!({"kind": "field", "name": name.as_str(), "offset": *offset, "type": kind})
            }
            Place::Index(name, index) => {
                node!({"kind": "index", "name": name.as_str(), "index": index.clone(), "type": kind})
            }
            Place::At(pointer, index) => {
                node!({"kind": "at", "pointer": pointer.clone(), "index": index.clone(), "type": kind})
            }
            Place::Plain(node) => node.clone(),
            // the slot the bits are in
            Place::Bits(slot, _) => self.access(slot, &ctype::int()),
        }
    }

    /// A place's address.
    fn address(&self, place: &Place) -> Node {
        if let Place::Var(name) = place {
            let found = self.lookup(name);
            if found.is_some_and(|found| found.storage == Storage::Local && found.ty.is_aggregate()) {
                // a local array's or struct's name is its address
                return var(name);
            }
        }
        address_of(self.access(place, &ctype::int()))
    }

    /// The value at a place: a scalar's, an array's or a struct's address.
    fn load(&self, place: &Place, ty: Ty) -> Typed {
        if let Place::Bits(slot, bits) = place {
            let signed = self.slot_kind(&ty) != "unsigned int";
            return Typed::new(bits.read(self.access(slot, &ctype::int()), signed), ty);
        }
        match ty {
            Ty::Array(of, _) => Typed::new(self.address(place), Ty::Pointer(of)),
            Ty::Struct(_) => Typed::new(self.address(place), ty),
            ty => Typed::new(self.access(place, &ty), ty),
        }
    }

    /// A place whose slots can be reached again and again without working anything out twice: a
    /// variable's own slots stay, anything else goes through its address in a hidden local.
    fn stable(&mut self, place: Place, ty: &Ty, parts: &mut Vec<Node>) -> Place {
        match place {
            Place::Var(_) | Place::Field(..) => place,
            Place::Index(name, index) if index.kind() == "int" => Place::Index(name, index),
            other => {
                let pointer = self.hidden_pointer(&ctype::pointer(ty.clone()));
                parts.push(assign(var(&pointer), self.address(&other)));
                Place::Index(pointer, int_node(0))
            }
        }
    }

    /// A struct copied slot by slot (each as it is) from the struct at an address; its value is the
    /// target's address.
    fn copy(&mut self, target: Place, from: Node, ty: &Ty) -> Node {
        let mut parts = Vec::new();
        let target = self.stable(target, ty, &mut parts);
        let source = match self.place_of_address(&from) {
            Some(place) => self.stable(place, ty, &mut parts),
            None => {
                let pointer = self.hidden_pointer(&ctype::pointer(ty.clone()));
                parts.push(assign(var(&pointer), from));
                Place::Index(pointer, int_node(0))
            }
        };
        let int = ctype::int();
        for slot in 0..ty.slots(&self.structs) {
            let to = self.access(&self.offset(target.clone(), slot, None), &int);
            let value = self.access(&self.offset(source.clone(), slot, None), &int);
            parts.push(assign(to, value));
        }
        parts.push(self.address(&target));
        sequence(parts)
    }

    // ---- values ----------------------------------------------------------------------------

    fn rvalue(&mut self, node: &Node) -> Result<Typed> {
        Ok(match node.kind() {
            "int" => Typed::new(
                node.clone(),
                Ty::Scalar(if node.get("unsigned").is_some() { "unsigned int" } else { "int" }.into()),
            ),
            "float" => Typed::new(node.clone(), Ty::Scalar("float".into())),
            "string" => Typed::new(node.clone(), Ty::Scalar("string".into())),
            "wide" => self.literal_array(node)?,
            "var" => self.variable(node)?,
            "deref" if self.plain_deref(node).is_none() => {
                let operand = self.pointed(&node["operand"])?;
                if operand.ty.is_function_pointer() || matches!(operand.ty, Ty::Function(..)) {
                    // *f is the function, which is its pointer again
                    return Ok(operand);
                }
                let (place, ty) = self.deref_place(operand)?;
                self.load(&place, ty)
            }
            "index" | "deref" | "subscript" | "member" | "field" => {
                let (place, ty) = self.place(node)?;
                if let Ty::Function(..) = ty {
                    return fail("a function is called, not read");
                }
                self.load(&place, ty)
            }
            "addr" => self.address_value(node)?,
            "assign" => self.assignment(node)?,
            "compound" => self.compound(node)?,
            "binary" => self.binary(node)?,
            "unary" => {
                let operand = self.scalar(&node["operand"], "an operand")?;
                let ty = if node["op"] == "!" { ctype::int() } else { operand.ty.clone() };
                if node["op"] != "!" && !matches!(operand.ty, Ty::Scalar(_)) {
                    return fail(format!("unary {} of {}", text(node, "op"), operand.ty.shown()));
                }
                Typed::new(node.clone().with("operand", operand.node), ty)
            }
            "logical" => {
                let left = self.scalar(&node["left"], "an operand of && or ||")?;
                let right = self.scalar(&node["right"], "an operand of && or ||")?;
                Typed::new(node.clone().with("left", left.node).with("right", right.node), ctype::int())
            }
            "ternary" => {
                let cond = self.scalar(&node["cond"], "a condition")?;
                let then = self.rvalue(&node["then"])?;
                let otherwise = self.rvalue(&node["else"])?;
                if then.ty.is_struct() || otherwise.ty.is_struct() {
                    // each branch's value is its struct's address, so the ?: gives one of the two
                    self.same_struct(&then.ty, &otherwise.ty, "a ?: of")?;
                    let node =
                        node.clone().with("cond", cond.node).with("then", then.node).with("else", otherwise.node);
                    return Ok(Typed::new(node, then.ty));
                }
                let ty = if then.ty.is_pointer() || matches!(then.ty, Ty::Scalar(ref name) if name == "float") {
                    then.ty.clone()
                } else {
                    otherwise.ty.clone()
                };
                Typed::new(
                    node.clone().with("cond", cond.node).with("then", then.node).with("else", otherwise.node),
                    ty,
                )
            }
            "cast" => self.cast(node)?,
            "comma" => {
                let left = self.rvalue(&node["left"])?;
                let right = self.rvalue(&node["right"])?;
                Typed::new(node.clone().with("left", left.node).with("right", right.node), right.ty)
            }
            "call" => self.call(node)?,
            "call_ptr" => self.call_through(node)?,
            "builtin" => {
                let operand = self.scalar(&node["operand"], "sin's or cos's angle")?;
                Typed::new(node.clone().with("operand", operand.node), Ty::Scalar("float".into()))
            }
            "va_arg" => self.va_arg(node)?,
            "va_start" => {
                let (place, ty) = self.place(&node["list"])?;
                if self.lookup("__va").is_none() {
                    return fail("va_start belongs in a function whose parameters end in ...");
                }
                Typed::new(assign(self.access(&place, &ty), var("__va")), Ty::Void)
            }
            "narrow" => {
                let operand = self.rvalue(&node["operand"])?;
                Typed::new(node.clone().with("operand", operand.node), Ty::Scalar(text(node, "type").into()))
            }
            _ => Typed::new(node.clone(), ctype::int()),
        })
    }

    /// A name's value: a variable's (an array's or struct's address), a function's number (its
    /// address), or a constant as it is.
    fn variable(&mut self, node: &Node) -> Result<Typed> {
        let name = text(node, "name");
        if let Some(found) = self.lookup(name) {
            let place = Place::Var(found.name.clone());
            let loaded = self.load(&place, found.ty.clone());
            // a variable the compiler had before stays as it was written
            let node = if found.name == name && !found.ty.is_aggregate() { node.clone() } else { loaded.node };
            return Ok(Typed::new(node, loaded.ty));
        }
        if let Some(signature) = self.signatures.get(name).cloned() {
            return self.function_number(name, &signature);
        }
        let ty = match self.found.constants.get(name).map(Node::kind) {
            Some("float") => Ty::Scalar("float".into()),
            Some("string") => Ty::Scalar("string".into()),
            _ => ctype::int(),
        };
        Ok(Typed::new(node.clone(), ty))
    }

    /// A function's address: its number, given the first time it is taken.
    fn function_number(&mut self, name: &str, signature: &Signature) -> Result<Typed> {
        let next = self.numbered.len() as i64 + 1;
        let number = *self.numbered.entry(name.to_string()).or_insert(next);
        let ty = ctype::pointer(Ty::Function(
            Box::new(signature.returns.clone()),
            signature.params.clone(),
            signature.variadic,
        ));
        Ok(Typed::new(int_node(number), ty))
    }

    fn address_value(&mut self, node: &Node) -> Result<Typed> {
        let target = &node["target"];
        if target.kind() == "var" && self.lookup(text(target, "name")).is_none() {
            if let Some(signature) = self.signatures.get(text(target, "name")).cloned() {
                return self.function_number(text(target, "name"), &signature);
            }
        }
        let (place, ty) = self.place(target)?;
        if let Place::Bits(..) = place {
            return fail("a bit-field has no address");
        }
        if let Place::Plain(plain) = &place {
            // &x, &a[i] and &*p of the variables the compiler had before, as they were
            return Ok(Typed::new(node.clone().with("target", plain.clone()), ctype::pointer(ty)));
        }
        Ok(Typed::new(self.address(&place), ctype::pointer(ty)))
    }

    /// A string literal as an array (its characters, signed as char is, and a 0) or a wide string
    /// (its codes and a 0): a hidden global, set before the statement.
    fn literal_array(&mut self, node: &Node) -> Result<Typed> {
        let (codes, element): (Vec<i64>, Ty) = if node.kind() == "wide" {
            let codes = list(node, "codes").iter().filter_map(Node::as_i64).collect();
            (codes, Ty::Scalar("unsigned short".into()))
        } else {
            let mut bytes = crate::stb::encode_string(text(node, "text"))?;
            bytes.push(0);
            (bytes.into_iter().map(|byte| byte as i8 as i64).collect(), Ty::Scalar("char".into()))
        };
        let name = format!("__literal_{}", self.literals.len());
        let ty = Ty::Array(Box::new(element), codes.len() as i64);
        self.literals.push(node!({"name": name.as_str(), "type": "int", "size": codes.len(), "ctype": ty.to_node()}));
        let variable = Variable { ty: ty.clone(), storage: Storage::Global, name: name.clone(), typed: true };
        self.globals.insert(name.clone(), variable);
        for (slot, code) in codes.into_iter().enumerate() {
            let cell = node!({"kind": "field", "name": name.as_str(), "offset": slot, "type": "int"});
            self.before.push(node!({"kind": "expr", "expr": assign(cell, int_node(code))}));
        }
        Ok(self.load(&Place::Var(name), ty))
    }

    /// A value given where a pointer of a type is wanted: a string literal is an array then, when
    /// the pointer is to char.
    fn value_for(&mut self, node: &Node, to: &Ty) -> Result<Typed> {
        if node.kind() == "string" && to.target().is_some_and(|target| is_char(target, self.kinds)) {
            return self.literal_array(node);
        }
        self.rvalue(node)
    }

    /// A value that is indexed or dereferenced: a string literal is an array then.
    fn pointed(&mut self, node: &Node) -> Result<Typed> {
        if node.kind() == "string" {
            return self.literal_array(node);
        }
        self.rvalue(node)
    }

    fn assignment(&mut self, node: &Node) -> Result<Typed> {
        let (place, ty) = self.place(&node["target"])?;
        if let Place::Bits(slot, bits) = place {
            let value = self.scalar(&node["value"], "a bit-field's value")?;
            self.assignable(&ty, &value, "assigning")?;
            return Ok(self.store_bits(*slot, bits, &ty, |_| value.node));
        }
        if let Ty::Array(..) = ty {
            return fail("an array cannot be assigned; assign its elements");
        }
        let value = self.value_for(&node["value"], &ty)?;
        if ty.is_struct() {
            self.same_struct(&ty, &value.ty, "assigning")?;
            return Ok(Typed::new(self.copy(place, value.node, &ty), ty));
        }
        if value.ty.is_struct() {
            return fail(format!("assigning {} to {}", value.ty.shown(), ty.shown()));
        }
        self.assignable(&ty, &value, "assigning")?;
        let target = self.access(&place, &ty);
        Ok(Typed::new(node.clone().with("target", target).with("value", value.node), ty))
    }

    /// target op= value; a pointer steps by elements: p += n is p = &p[n].
    fn compound(&mut self, node: &Node) -> Result<Typed> {
        let (place, ty) = self.place(&node["target"])?;
        let value = self.scalar(&node["value"], "an operand")?;
        if let Place::Bits(slot, bits) = place {
            let op = text(node, "op").to_string();
            return Ok(self.store_bits(
                *slot,
                bits,
                &ty,
                |current| node!({"kind": "binary", "op": op.as_str(), "left": current, "right": value.node}),
            ));
        }
        if !ty.is_pointer() {
            if ty.is_aggregate() {
                return fail(format!("{}= of {}", text(node, "op"), ty.shown()));
            }
            let target = self.access(&place, &ty);
            return Ok(Typed::new(node.clone().with("target", target).with("value", value.node), ty));
        }
        let op = text(node, "op");
        if op != "+" && op != "-" {
            return fail(format!("{op}= of a pointer"));
        }
        if !is_integer(&value.ty) {
            return fail(format!("a pointer steps by an integer, not {}", value.ty.shown()));
        }
        let step = if op == "-" { node!({"kind": "unary", "op": "-", "operand": value.node}) } else { value.node };
        let stepped = self.stepped(&place, &ty, step)?;
        let target = self.access(&place, &ty);
        Ok(Typed::new(assign(target, stepped), ty))
    }

    /// &place[step]: the pointer a place holds moved on by elements; the place is read again, so
    /// it must not have side effects.
    fn stepped(&mut self, place: &Place, ty: &Ty, step: Node) -> Result<Node> {
        let read = self.access(place, ty);
        if has_effects(&read) {
            return fail("a pointer stepped where its own place has side effects (p[i++]++) is not supported");
        }
        let element = self.element(ty)?;
        let (base, _) = self.deref_place(Typed::new(read, ty.clone()))?;
        let size = self.slots(&element);
        let moved = match step.kind() {
            "int" if step.get("unsigned").is_none() => {
                self.offset(base, step["value"].as_i64().unwrap_or(0) * size, None)
            }
            _ => self.offset(base, 0, Some(times(step, size))),
        };
        Ok(self.address(&moved))
    }

    fn binary(&mut self, node: &Node) -> Result<Typed> {
        let op = text(node, "op");
        if node.get("postfix").is_some() && node["left"].kind() == "compound" {
            // p++: the old value, kept in a hidden local, and the pointer stepped
            let target = &node["left"]["target"];
            let (place, ty) = self.place(target)?;
            if let Place::Bits(slot, bits) = place {
                // the old value, kept in a hidden local, and the field stepped
                let step = if node["left"]["op"] == "-" { "-" } else { "+" };
                let old = self.hidden_int();
                let mut stepped = self.store_bits(*slot, bits, &ty, |current| {
                    let kept = assign(var(&old), current);
                    node!({"kind": "binary", "op": step, "left": kept, "right": int_node(1)})
                });
                stepped.node = sequence(vec![stepped.node, var(&old)]);
                return Ok(stepped);
            }
            if ty.is_pointer() {
                let old = self.hidden_pointer(&ty);
                let read = self.access(&place, &ty);
                let step = if node["left"]["op"] == "-" { int_node(-1) } else { int_node(1) };
                let stepped = self.stepped(&place, &ty, step)?;
                let parts = vec![assign(var(&old), read), assign(self.access(&place, &ty), stepped), var(&old)];
                return Ok(Typed::new(sequence(parts), ty));
            }
        }
        let left = self.scalar(&node["left"], "an operand")?;
        let right = self.scalar(&node["right"], "an operand")?;
        let compared = matches!(op, "==" | "!=" | "<" | "<=" | ">" | ">=");
        let (left_pointer, right_pointer) = (left.ty.is_pointer(), right.ty.is_pointer());
        if compared && (left_pointer || right_pointer) {
            return self.pointer_comparison(op, left, right);
        }
        if (op == "+" || op == "-") && (left_pointer || right_pointer) {
            if left_pointer && right_pointer {
                return fail(
                    "the difference of two pointers is not supported: the VM gives no address as a number; keep an index",
                );
            }
            let (pointer, count) = if left_pointer { (left, right) } else { (right, left) };
            if op == "-" && !left_pointer {
                return fail("an integer minus a pointer");
            }
            if !is_integer(&count.ty) {
                return fail(format!("a pointer moves by an integer, not {}", count.ty.shown()));
            }
            let element = self.element(&pointer.ty)?;
            let ty = pointer.ty.clone();
            let (base, _) = self.deref_place(pointer)?;
            let size = self.slots(&element);
            let count = if op == "-" { node!({"kind": "unary", "op": "-", "operand": count.node}) } else { count.node };
            let moved = match count.kind() {
                "int" if count.get("unsigned").is_none() => {
                    self.offset(base, count["value"].as_i64().unwrap_or(0) * size, None)
                }
                _ => self.offset(base, 0, Some(times(count, size))),
            };
            return Ok(Typed::new(self.address(&moved), ty));
        }
        if left_pointer || right_pointer {
            return fail(format!("{op} of a pointer"));
        }
        let ty = if compared {
            ctype::int()
        } else if [&left.ty, &right.ty].iter().any(|ty| matches!(ty, Ty::Scalar(name) if name == "float")) {
            Ty::Scalar("float".into())
        } else {
            left.ty.clone()
        };
        Ok(Typed::new(node.clone().with("left", left.node).with("right", right.node), ty))
    }

    /// Whether a value is a null pointer constant: 0, a constant that is 0, either cast.
    fn is_null(&self, node: &Node) -> bool {
        match node.kind() {
            "int" => node["value"] == Node::Int(0),
            "cast" => self.is_null(&node["operand"]),
            "var" => {
                self.lookup(text(node, "name")).is_none()
                    && self.found.constants.get(text(node, "name")).is_some_and(|value| self.is_null(value))
            }
            _ => false,
        }
    }

    /// A pointer compared with a null pointer constant: !p or !!p, which the VM's truth gives.
    fn pointer_comparison(&mut self, op: &str, left: Typed, right: Typed) -> Result<Typed> {
        let (pointer, other) = if left.ty.is_pointer() { (left, right) } else { (right, left) };
        if !self.is_null(&other.node) {
            if other.ty.is_pointer() {
                return fail(
                    "comparing two pointers is not supported: the VM gives no address as a number; compare indexes, or with 0",
                );
            }
            return fail(format!(
                "comparing a pointer with {}: only 0 (a null pointer) compares with one",
                other.ty.shown()
            ));
        }
        Ok(match op {
            "==" => Typed::new(not(pointer.node), ctype::int()),
            "!=" => Typed::new(not(not(pointer.node)), ctype::int()),
            _ => return fail(format!("{op} of a pointer and a null pointer")),
        })
    }

    fn cast(&mut self, node: &Node) -> Result<Typed> {
        let operand = self.rvalue(&node["operand"])?;
        let kind = text(node, "type");
        let to = match node.get("ctype") {
            Some(ctype) => Ty::from_node(ctype),
            None if kind.ends_with(" *") => ctype::pointer(Ty::Scalar(kind.trim_end_matches(" *").to_string())),
            None if kind == "void" => Ty::Void,
            None => Ty::Scalar(kind.to_string()),
        };
        if operand.ty.is_struct() {
            return fail(format!("a cast of {}", operand.ty.shown()));
        }
        if to.is_pointer() || to.is_function_pointer() {
            let compatible =
                operand.ty.is_pointer() || operand.ty.is_function_pointer() || node["operand"].kind() == "addr";
            if !compatible && !self.is_null(&operand.node) && !(to.is_function_pointer() && is_integer(&operand.ty)) {
                return fail(format!(
                    "{} cannot be converted to a pointer: the VM's pointers are addresses it gives",
                    operand.ty.shown()
                ));
            }
            return Ok(Typed::new(node.clone().with("operand", operand.node), to));
        }
        if operand.ty.is_pointer() && kind != "void" && !kind.starts_with("enum ") {
            return fail(format!("a pointer cannot be converted to {kind}: the VM gives no address as a number"));
        }
        Ok(Typed::new(node.clone().with("operand", operand.node), to))
    }

    // ---- calls -----------------------------------------------------------------------------

    fn call(&mut self, node: &Node) -> Result<Typed> {
        let name = text(node, "name");
        if let Some(found) = self.lookup(name) {
            if found.ty.is_function_pointer() {
                let callee = var(name);
                return self.call_through(&node.clone().with("callee", callee));
            }
        }
        let Some(signature) = self.signatures.get(name).cloned() else {
            // a native, or a function the compiler reports
            let mut args = Vec::new();
            for arg in list(node, "args") {
                args.push(self.scalar(arg, "a native's argument")?.node);
            }
            let returns = self.found.natives.get(name).map_or("int", |native| text(native, "returns"));
            let ty = if returns == "void" { Ty::Void } else { Ty::Scalar(returns.to_string()) };
            return Ok(Typed::new(node.clone().with("args", args), ty));
        };
        let given = list(node, "args");
        let fixed = signature.params.len();
        if given.len() < fixed || (given.len() > fixed && !signature.variadic) {
            return fail(format!("{name} takes {fixed} arguments"));
        }
        let mut parts = Vec::new();
        let mut args = Vec::new();
        let mut result = None;
        if signature.returns.is_struct() {
            let storage = self.hidden_storage("result", &signature.returns);
            args.push(var(&storage));
            result = Some(storage);
        }
        for (arg, param) in given.iter().zip(&signature.params) {
            let value = self.value_for(arg, param)?;
            if param.is_struct() || value.ty.is_struct() {
                // the function copies the struct at the address it is given
                self.same_struct(param, &value.ty, &format!("passing to {name}"))?;
            } else {
                self.assignable(param, &value, &format!("passing to {name}"))?;
            }
            args.push(value.node);
        }
        if signature.variadic {
            args.push(self.extra_arguments(&given[fixed..], &mut parts)?);
        }
        let call = node.clone().with("args", args);
        parts.push(call);
        if let Some(storage) = result {
            parts.push(var(&storage));
        }
        let ty = signature.returns.clone();
        Ok(Typed::new(sequence(parts), ty))
    }

    /// The arguments after ... in a hidden array the function reads through __va, set by parts:
    /// the array's address.
    fn extra_arguments(&mut self, extra: &[Node], parts: &mut Vec<Node>) -> Result<Node> {
        let storage = self.hidden_storage("arguments", &Ty::Array(Box::new(ctype::int()), extra.len().max(1) as i64));
        for (slot, arg) in extra.iter().enumerate() {
            let value = self.scalar(arg, "an argument after ...")?;
            // stored as it is: of its own type, so strict mode converts nothing
            let kind = self.slot_kind(&value.ty);
            let cell = node!({"kind": "field", "name": storage.as_str(), "offset": slot, "type": kind});
            parts.push(assign(cell, value.node));
        }
        Ok(var(&storage))
    }

    /// A call through a function pointer: __call_N(number, arguments...), N the arguments the
    /// function takes in the VM: a struct's result's address first, a struct argument's address,
    /// and the hidden array of the arguments after ... last, as a direct call passes them.
    fn call_through(&mut self, node: &Node) -> Result<Typed> {
        let callee = &node["callee"];
        let designated = match callee.kind() {
            "var" => Some(callee),
            "deref" if callee["operand"].kind() == "var" => Some(&callee["operand"]),
            _ => None,
        };
        if let Some(name) = designated.map(|named| text(named, "name")) {
            if self.lookup(name).is_none() && self.signatures.contains_key(name) {
                // (f)(x) and (*f)(x) of a function f are plain calls
                let call = node!({"kind": "call", "name": name, "args": node["args"].clone()});
                return self.call(&call);
            }
        }
        let pointer = self.rvalue(callee)?;
        let function = match &pointer.ty {
            Ty::Pointer(to) => (**to).clone(),
            other => other.clone(),
        };
        let Ty::Function(returns, params, variadic) = function else {
            return fail(format!("{} is not a function or a function pointer", pointer.ty.shown()));
        };
        let given = list(node, "args");
        let wrong = if variadic { given.len() < params.len() } else { params.len() != given.len() };
        if !params.is_empty() && wrong {
            return fail(format!("the function pointer's function takes {} arguments", params.len()));
        }
        let mut parts = Vec::new();
        let mut args = vec![pointer.node];
        let mut result = None;
        if returns.is_struct() {
            let storage = self.hidden_storage("result", &returns);
            args.push(var(&storage));
            result = Some(storage);
        }
        let fixed = if variadic { params.len() } else { given.len() };
        for (at, arg) in given[..fixed].iter().enumerate() {
            let value = match params.get(at) {
                Some(param) => self.value_for(arg, param)?,
                None => self.rvalue(arg)?,
            };
            match params.get(at) {
                Some(param) if param.is_struct() || value.ty.is_struct() => {
                    // the function copies the struct at the address it is given
                    self.same_struct(param, &value.ty, "passing through a function pointer")?;
                }
                Some(param) => self.assignable(param, &value, "passing through a function pointer")?,
                None => {}
            }
            args.push(value.node);
        }
        if variadic {
            args.push(self.extra_arguments(&given[fixed..], &mut parts)?);
        }
        let arity = args.len() - 1;
        self.arities.insert(arity);
        parts.push(node!({"kind": "call", "name": format!("__call_{arity}"), "args": args}));
        if let Some(storage) = result {
            parts.push(var(&storage));
        }
        Ok(Typed::new(sequence(parts), *returns))
    }

    /// The function a call through a pointer with arity arguments calls: a switch on the number to
    /// each function whose address is taken and that takes that many.
    fn dispatch(&self, arity: usize) -> Node {
        let mut params = vec![node!({"name": "__function", "type": "int", "array": false})];
        let args: Vec<Node> = (1..=arity).map(|at| var(&format!("__argument_{at}"))).collect();
        for at in 1..=arity {
            params.push(node!({"name": format!("__argument_{at}"), "type": "int", "array": false}));
        }
        let mut cases = Vec::new();
        for (name, number) in &self.numbered {
            let signature = &self.signatures[name];
            let taken =
                signature.params.len() + usize::from(signature.returns.is_struct()) + usize::from(signature.variadic);
            if taken != arity {
                continue;
            }
            let call = node!({"kind": "call", "name": name.as_str(), "args": args.clone()});
            let body = if signature.returns.is_struct() || signature.returns == Ty::Void {
                // a function with no value (one returning a struct writes it through its pointer)
                node!([{"kind": "expr", "expr": call}, {"kind": "return", "value": null}])
            } else {
                node!([{"kind": "return", "value": call}])
            };
            cases.push(node!({"value": int_node(*number), "default": false, "body": body}));
        }
        let body = if cases.is_empty() {
            Vec::new()
        } else {
            vec![node!({"kind": "switch", "value": var("__function"), "cases": cases})]
        };
        node!({
            "kind": "function",
            "name": format!("__call_{arity}"),
            "returns": "int",
            "params": params,
            "locals": [],
            "body": body,
            "unused_strings": [],
            "padding": null,
        })
    }

    /// va_arg(ap, type): ap steps past the argument, which is read where it was.
    fn va_arg(&mut self, node: &Node) -> Result<Typed> {
        let ty = Ty::from_node(&node["ctype"]);
        if ty.is_aggregate() {
            return fail("va_arg of a struct or array is not supported");
        }
        // a char or short after ... was passed as an int (C's default argument promotions): it is
        // read as one and converted back
        let narrow = matches!(&ty, Ty::Scalar(name) if matches!(
            scalar_base(name, self.kinds).as_str(),
            "char" | "signed char" | "unsigned char" | "short" | "unsigned short"
        ));
        let (place, list_ty) = self.place(&node["list"])?;
        if !list_ty.is_pointer() {
            return fail("va_arg takes a va_list");
        }
        let step = self.stepped(&place, &list_ty, int_node(1))?;
        let stepped = assign(self.access(&place, &list_ty), step);
        let read = self.access(&place, &list_ty);
        let (base, _) = self.deref_place(Typed::new(read, list_ty))?;
        if narrow {
            let value = self.access(&self.offset(base, -1, None), &ctype::int());
            let converted = node!({"kind": "cast", "type": self.slot_kind(&ty), "operand": value});
            return Ok(Typed::new(sequence(vec![stepped, converted]), ty));
        }
        let value = self.access(&self.offset(base, -1, None), &ty);
        Ok(Typed::new(sequence(vec![stepped, value]), ty))
    }

    // ---- C's constraints -------------------------------------------------------------------

    fn same_struct(&self, to: &Ty, from: &Ty, what: &str) -> Result<()> {
        if to != from {
            return fail(format!("{what} {} to {}", from.shown(), to.shown()));
        }
        Ok(())
    }

    /// C's constraints on assignment (3.3.16.1), as argument passing and return have them: an
    /// integer is not a pointer (but for a null pointer constant), nor a pointer an integer, and
    /// pointers point at the same type or one of them at void. Strict mode gives them.
    fn assignable(&self, to: &Ty, value: &Typed, what: &str) -> Result<()> {
        if !self.strict {
            return Ok(());
        }
        let from = &value.ty;
        if matches!(from, Ty::Scalar(name) if name == "any") {
            // what a native gives
            return Ok(());
        }
        if to.is_pointer() || to.is_function_pointer() {
            if self.is_null(&value.node) || from == to {
                return Ok(());
            }
            if let (Some(a), Some(b)) = (to.target(), from.target()) {
                if a == b
                    || matches!(a, Ty::Void)
                    || matches!(b, Ty::Void)
                    || (to.is_pointer() && !from.is_function_pointer() && self.same_scalar(a, b))
                {
                    return Ok(());
                }
                return fail(format!("{what}: incompatible pointer types {} and {}", from.shown(), to.shown()));
            }
            if matches!(from, Ty::Scalar(name) if name == "string") {
                return fail(format!(
                    "{what}: a string literal is the VM's string, not a char array; {} needs one",
                    to.shown()
                ));
            }
            return fail(format!("{what} {} to {} needs a cast", from.shown(), to.shown()));
        }
        if is_integer(to) && (from.is_pointer() || from.is_function_pointer()) {
            return fail(format!("{what} {} to {} needs a cast", from.shown(), to.shown()));
        }
        Ok(())
    }

    /// Two scalar types that are one: a typedef and what it stands for.
    fn same_scalar(&self, a: &Ty, b: &Ty) -> bool {
        match (a, b) {
            (Ty::Scalar(a), Ty::Scalar(b)) => scalar_base(a, self.kinds) == scalar_base(b, self.kinds),
            _ => false,
        }
    }
}
