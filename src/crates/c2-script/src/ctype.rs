//! C's types beyond the VM's scalars, as the parser declares them and composite.rs lowers them:
//! pointers of any depth, arrays, structs and unions, and functions (for function pointers).
//!
//! The VM's unit of storage is the slot, which holds any one value: a scalar, a pointer or a
//! function's number is one slot, an array its elements' slots in a row, a struct its members' in
//! order and a union as many as its largest member (all of them starting at its first). Pointer
//! arithmetic and indexing count slots. sizeof and offsetof count C's bytes, as the implementation
//! defines them (char 1, short 2, the rest 4, each aligned to its size, a struct to its largest
//! member's alignment), so code that divides sizes gets C's answers; nothing reads memory by bytes.
//!
//! A type is held in the tree as a node: a scalar or void as its name ("int", "float", "string",
//! "unsigned char", a value kind), {"pointer": T}, {"array": T, "size": N}, {"struct": TAG}, and
//! {"function": RETURNS, "params": [T...], "variadic": bool}. A program's structs and unions are
//! its "structs": {TAG: {"union": bool, "members": [{"name": NAME, "type": T}...]}}.

use indexmap::IndexMap;

use crate::node;
use crate::node::{Map, Node};

#[derive(Clone, Debug, PartialEq)]
pub enum Ty {
    Void,
    /// int, float, string, char, short and their unsigned kinds, or a value kind as written.
    Scalar(String),
    Pointer(Box<Ty>),
    /// An array and its length; -1 while an initializer has not given it.
    Array(Box<Ty>, i64),
    /// A struct or union, by tag.
    Struct(String),
    /// What a function returns, its parameters, and whether it takes more (...).
    Function(Box<Ty>, Vec<Ty>, bool),
}

/// A struct's or union's members, in order.
#[derive(Clone, Debug, PartialEq)]
pub struct Composite {
    pub union: bool,
    pub members: Vec<(String, Ty)>,
}

pub type Structs = IndexMap<String, Composite>;

pub fn int() -> Ty {
    Ty::Scalar("int".into())
}

pub fn pointer(to: Ty) -> Ty {
    Ty::Pointer(Box::new(to))
}

impl Ty {
    pub fn to_node(&self) -> Node {
        match self {
            Ty::Void => Node::from("void"),
            Ty::Scalar(name) => Node::from(name.as_str()),
            Ty::Pointer(to) => node!({"pointer": to.to_node()}),
            Ty::Array(of, size) => node!({"array": of.to_node(), "size": *size}),
            Ty::Struct(tag) => node!({"struct": tag.as_str()}),
            Ty::Function(returns, params, variadic) => node!({
                "function": returns.to_node(),
                "params": params.iter().map(Ty::to_node).collect::<Vec<_>>(),
                "variadic": *variadic,
            }),
        }
    }

    pub fn from_node(node: &Node) -> Ty {
        if let Some(name) = node.as_str() {
            return if name == "void" { Ty::Void } else { Ty::Scalar(name.to_string()) };
        }
        if let Some(to) = node.get("pointer") {
            return pointer(Ty::from_node(to));
        }
        if let Some(of) = node.get("array") {
            return Ty::Array(Box::new(Ty::from_node(of)), node["size"].as_i64().unwrap_or(-1));
        }
        if let Some(tag) = node.get("struct").and_then(Node::as_str) {
            return Ty::Struct(tag.to_string());
        }
        if let Some(returns) = node.get("function") {
            let params =
                node["params"].as_array().map_or(Vec::new(), |params| params.iter().map(Ty::from_node).collect());
            return Ty::Function(Box::new(Ty::from_node(returns)), params, node["variadic"] == Node::Bool(true));
        }
        int()
    }

    /// A pointer to data (not to a function, whose value is the function's number).
    pub fn is_pointer(&self) -> bool {
        matches!(self, Ty::Pointer(to) if !matches!(**to, Ty::Function(..)))
    }

    pub fn is_function_pointer(&self) -> bool {
        matches!(self, Ty::Pointer(to) if matches!(**to, Ty::Function(..)))
    }

    /// What a pointer or array points at.
    pub fn target(&self) -> Option<&Ty> {
        match self {
            Ty::Pointer(to) | Ty::Array(to, _) => Some(to),
            _ => None,
        }
    }

    /// A struct, union or array: several slots, copied or reached by address.
    pub fn is_aggregate(&self) -> bool {
        matches!(self, Ty::Array(..) | Ty::Struct(_))
    }

    pub fn is_struct(&self) -> bool {
        matches!(self, Ty::Struct(_))
    }

    /// The slots a value of the type takes.
    pub fn slots(&self, structs: &Structs) -> i64 {
        match self {
            Ty::Array(of, size) => of.slots(structs) * (*size).max(0),
            Ty::Struct(tag) => match structs.get(tag) {
                Some(found) => {
                    let sizes = found.members.iter().map(|(_, ty)| ty.slots(structs));
                    if found.union {
                        sizes.max().unwrap_or(0)
                    } else {
                        sizes.sum()
                    }
                }
                None => 0,
            },
            _ => 1,
        }
    }

    /// C's size in bytes, and its alignment.
    pub fn layout(&self, structs: &Structs) -> (i64, i64) {
        match self {
            Ty::Scalar(name) => match name.as_str() {
                "char" | "unsigned char" | "signed char" => (1, 1),
                "short" | "unsigned short" => (2, 2),
                _ => (4, 4),
            },
            Ty::Array(of, size) => {
                let (bytes, align) = of.layout(structs);
                (bytes * (*size).max(0), align)
            }
            Ty::Struct(tag) => {
                let Some(found) = structs.get(tag) else {
                    return (0, 1);
                };
                let (mut size, mut align) = (0, 1);
                for (_, ty) in &found.members {
                    let (bytes, member_align) = ty.layout(structs);
                    align = align.max(member_align);
                    if found.union {
                        size = size.max(bytes);
                    } else {
                        size = aligned(size, member_align) + bytes;
                    }
                }
                (aligned(size, align), align)
            }
            _ => (4, 4),
        }
    }

    pub fn bytes(&self, structs: &Structs) -> i64 {
        self.layout(structs).0
    }

    /// The kind of slot the type's scalar is read as: float, a narrow integer, or int (an int,
    /// pointer or anything copied as it is).
    pub fn slot_kind(&self) -> String {
        match self {
            Ty::Scalar(name) => name.clone(),
            _ => "int".into(),
        }
    }

    /// The type as C writes it, for messages.
    pub fn shown(&self) -> String {
        match self {
            Ty::Void => "void".into(),
            Ty::Scalar(name) => name.clone(),
            Ty::Pointer(to) => match &**to {
                Ty::Function(..) => format!("pointer to {}", to.shown()),
                Ty::Array(..) => format!("pointer to {}", to.shown()),
                _ => format!("{} *", to.shown()),
            },
            Ty::Array(of, size) if *size >= 0 => format!("{}[{size}]", of.shown()),
            Ty::Array(of, _) => format!("{}[]", of.shown()),
            Ty::Struct(tag) if tag.starts_with("__anonymous") => "an unnamed struct".into(),
            Ty::Struct(tag) => format!("struct {tag}"),
            Ty::Function(returns, params, variadic) => {
                let mut shown: Vec<String> = params.iter().map(Ty::shown).collect();
                if *variadic {
                    shown.push("...".into());
                }
                format!("function ({}) returning {}", shown.join(", "), returns.shown())
            }
        }
    }
}

/// value rounded up to a multiple of align.
fn aligned(value: i64, align: i64) -> i64 {
    (value + align - 1) / align * align
}

/// A member of a struct or union: its offset in slots and in bytes, and its type.
pub fn member(structs: &Structs, tag: &str, name: &str) -> Option<(i64, i64, Ty)> {
    let found = structs.get(tag)?;
    let (mut slots, mut bytes) = (0, 0);
    for (member, ty) in &found.members {
        let (size, align) = ty.layout(structs);
        if !found.union {
            bytes = aligned(bytes, align);
        }
        if member == name {
            return Some((slots, bytes, ty.clone()));
        }
        if !found.union {
            slots += ty.slots(structs);
            bytes += size;
        }
    }
    None
}

/// Every scalar slot of a type, in order, with the type its value has: what an initializer fills
/// and what zeroing a local sets. A union's are its first member's, then plain slots to its size.
pub fn scalar_slots(ty: &Ty, structs: &Structs, base: i64, out: &mut Vec<(i64, Ty)>) {
    match ty {
        Ty::Array(of, size) => {
            let step = of.slots(structs);
            for at in 0..(*size).max(0) {
                scalar_slots(of, structs, base + at * step, out);
            }
        }
        Ty::Struct(tag) => {
            let Some(found) = structs.get(tag) else {
                return;
            };
            let mut at = base;
            for (_, member) in &found.members {
                scalar_slots(member, structs, at, out);
                if found.union {
                    let end = base + ty.slots(structs);
                    for rest in at + member.slots(structs)..end {
                        out.push((rest, int()));
                    }
                    return;
                }
                at += member.slots(structs);
            }
        }
        other => out.push((base, other.clone())),
    }
}

/// A program's structs from its tree.
pub fn structs_of(program: &Node) -> Structs {
    let mut found = Structs::new();
    if let Some(map) = program.get("structs").and_then(Node::as_object) {
        for (tag, composite) in map.iter() {
            let members = composite["members"].as_array().map_or(Vec::new(), |members| {
                members
                    .iter()
                    .map(|member| (member["name"].as_str().unwrap_or("").to_string(), Ty::from_node(&member["type"])))
                    .collect()
            });
            found.insert(tag.to_string(), Composite { union: composite["union"] == Node::Bool(true), members });
        }
    }
    found
}

/// Structs as a program's tree holds them.
pub fn structs_node(structs: &Structs) -> Node {
    let mut map = Map::new();
    for (tag, composite) in structs {
        let members: Vec<Node> =
            composite.members.iter().map(|(name, ty)| node!({"name": name.as_str(), "type": ty.to_node()})).collect();
        map.insert(tag.clone(), node!({"union": composite.union, "members": members}));
    }
    Node::Map(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts_follow_cs_alignment_and_slots_count_members() {
        let mut structs = Structs::new();
        let pair = Composite {
            union: false,
            members: vec![
                ("c".into(), Ty::Scalar("char".into())),
                ("i".into(), int()),
                ("s".into(), Ty::Scalar("short".into())),
            ],
        };
        structs.insert("pair".into(), pair);
        structs.insert(
            "value".into(),
            Composite {
                union: true,
                members: vec![("f".into(), Ty::Scalar("float".into())), ("a".into(), Ty::Array(Box::new(int()), 3))],
            },
        );
        let pair = Ty::Struct("pair".into());
        assert_eq!((pair.bytes(&structs), pair.slots(&structs)), (12, 3));
        assert_eq!(member(&structs, "pair", "i"), Some((1, 4, int())));
        assert_eq!(member(&structs, "pair", "s").map(|(slots, bytes, _)| (slots, bytes)), Some((2, 8)));
        let value = Ty::Struct("value".into());
        assert_eq!((value.bytes(&structs), value.slots(&structs)), (12, 3));
        let mut slots = Vec::new();
        scalar_slots(&value, &structs, 0, &mut slots);
        assert_eq!(slots, vec![(0, Ty::Scalar("float".into())), (1, int()), (2, int())]);
        let node = Ty::Function(Box::new(pointer(pair.clone())), vec![int()], true).to_node();
        assert_eq!(Ty::from_node(&node), Ty::Function(Box::new(pointer(pair)), vec![int()], true));
    }
}
