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
//! its "structs": {TAG: {"union": bool, "members": [{"name": NAME, "type": T}...]}}; a bit-field
//! member also has "bits": [OFFSET, WIDTH].
//!
//! Bit-fields share an int slot, from its lowest bit: a bit-field that does not fit in the bits the
//! slot has left (or follows a member that is not one, or a :0) starts the next slot. A 32-bit one
//! is a whole slot, an ordinary member of its type. composite.rs reads one as (slot & mask) >> offset
//! and writes the slot with the field's bits replaced.

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
    /// Where each member's bits are in its slot when it is a bit-field: empty when none is, else one
    /// for each member.
    pub bits: Vec<Option<Bits>>,
}

/// A bit-field's place in its slot: its lowest bit, and how many bits it has (1 to 31).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bits {
    pub offset: i64,
    pub width: i64,
}

impl Bits {
    /// The slot's bits that are the field's.
    pub fn mask(self) -> i64 {
        (((1i64 << self.width) - 1) << self.offset) as i32 as i64
    }
}

fn int_node(value: i64) -> Node {
    node!({"kind": "int", "value": value as i32 as i64})
}

/// An int literal's value (not an unsigned one).
fn literal(node: &Node) -> Option<i64> {
    (node.kind() == "int" && node.get("unsigned").is_none()).then(|| node["value"].as_i64()).flatten()
}

/// left op right on ints, folded when both are literals.
fn int_op(op: &str, left: Node, right: Node) -> Node {
    if let (Some(a), Some(b)) = (literal(&left), literal(&right)) {
        let (a, b) = (a as i32, b as i32);
        let folded = match op {
            "&" => Some(a & b),
            "|" => Some(a | b),
            "^" => Some(a ^ b),
            "-" => Some(a.wrapping_sub(b)),
            "<<" => Some(a.wrapping_shl(b as u32)),
            // the VM's >> divides, rounding toward zero
            ">>" => Some(a.wrapping_div(1 << b)),
            _ => None,
        };
        if let Some(value) = folded {
            return int_node(i64::from(value));
        }
    }
    node!({"kind": "binary", "op": op, "left": left, "right": right})
}

/// A value as an int: a literal as it is, anything else converted (a float's value is truncated).
fn as_int(value: Node) -> Node {
    if literal(&value).is_some() {
        return value;
    }
    node!({"kind": "cast", "type": "int", "operand": value})
}

impl Bits {
    /// A value put in the field's bits of an otherwise empty slot: (value << offset) & mask.
    pub fn packed(self, value: Node) -> Node {
        let shifted = match self.offset {
            0 => as_int(value),
            offset => int_op("<<", as_int(value), int_node(offset)),
        };
        int_op("&", shifted, int_node(self.mask()))
    }

    /// Two slots' worth of bits put together (fields that share a slot).
    pub fn joined(left: Node, right: Node) -> Node {
        int_op("|", left, right)
    }

    /// The slot with the field's bits replaced by packed's: (slot & ~mask) | packed.
    pub fn stored(self, slot: Node, packed: Node) -> Node {
        int_op("|", int_op("&", slot, int_node(!self.mask())), packed)
    }

    /// The field's value from its slot's: (slot & mask) >> offset, and for a signed field its top bit
    /// spread up ((v ^ top) - top). The bits below the field are 0, so the VM's >> (a division) is
    /// exact; a field that reaches bit 31 comes out spread already, and an unsigned one is masked.
    pub fn read(self, slot: Node, signed: bool) -> Node {
        let mut value = int_op("&", slot, int_node(self.mask()));
        if self.offset > 0 {
            value = int_op(">>", value, int_node(self.offset));
        }
        let top_bit = self.offset + self.width == 32;
        if signed && !top_bit {
            let top = int_node(1 << (self.width - 1));
            value = int_op("-", int_op("^", value, top.clone()), top);
        } else if !signed && top_bit && self.offset > 0 {
            value = int_op("&", value, int_node((1 << self.width) - 1));
        }
        value
    }
}

impl Composite {
    pub fn new(union: bool, members: Vec<(String, Ty)>) -> Composite {
        Composite { union, members, bits: Vec::new() }
    }

    /// The bits of the member at, when it is a bit-field.
    pub fn bits_of(&self, at: usize) -> Option<Bits> {
        self.bits.get(at).copied().flatten()
    }

    /// The slots the member at takes in a struct: a bit-field that starts its slot takes the slot,
    /// one that shares it none.
    pub fn slots_of(&self, at: usize, structs: &Structs) -> i64 {
        match self.bits_of(at) {
            Some(bits) if bits.offset > 0 => 0,
            Some(_) => 1,
            None => self.members[at].1.slots(structs),
        }
    }

    /// C's bytes and alignment of the member at: a bit-field that starts its slot takes an int's 4
    /// bytes, one that shares it none.
    fn layout_of(&self, at: usize, structs: &Structs) -> (i64, i64) {
        match self.bits_of(at) {
            Some(bits) if bits.offset > 0 => (0, 4),
            Some(_) => (4, 4),
            None => self.members[at].1.layout(structs),
        }
    }

    /// What the member at takes in a struct.
    pub fn takes(&self, at: usize, structs: &Structs) -> Takes {
        let (bytes, align) = self.layout_of(at, structs);
        Takes { slots: self.slots_of(at, structs), bytes, align }
    }
}

/// What a member takes in its struct: its slots, and C's bytes and alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Takes {
    pub slots: i64,
    pub bytes: i64,
    pub align: i64,
}

/// A member of a struct or union: where it starts, in slots and in C's bytes (a bit-field that
/// shares its slot, the slot the last one started), its type, and its bits when it is a bit-field.
#[derive(Clone, Debug, PartialEq)]
pub struct Member {
    pub slot: i64,
    pub byte: i64,
    pub ty: Ty,
    pub bits: Option<Bits>,
}

pub type Structs = IndexMap<String, Composite>;

/// C's integer types narrower than the VM's 32-bit int, which a value stored in a variable of one
/// is wrapped to (char is signed, as it is in the engine's compiler).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntType {
    Char,
    UnsignedChar,
    Short,
    UnsignedShort,
}

impl IntType {
    /// The narrow type a type's name is; None for any other type (int, float, a pointer...).
    pub fn narrow(kind: &str) -> Option<IntType> {
        Some(match kind {
            "char" => IntType::Char,
            "unsigned char" => IntType::UnsignedChar,
            "short" => IntType::Short,
            "unsigned short" => IntType::UnsignedShort,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            IntType::Char => "char",
            IntType::UnsignedChar => "unsigned char",
            IntType::Short => "short",
            IntType::UnsignedShort => "unsigned short",
        }
    }

    /// The bits a value of the type has.
    pub fn mask(self) -> i64 {
        match self {
            IntType::Char | IntType::UnsignedChar => 0xff,
            IntType::Short | IntType::UnsignedShort => 0xffff,
        }
    }

    pub fn signed(self) -> bool {
        matches!(self, IntType::Char | IntType::Short)
    }

    /// A value wrapped to the type: its bits, and for a signed type the top one spread up.
    pub fn wrap(self, value: i64) -> i64 {
        match self {
            IntType::Char => value as i8 as i64,
            IntType::UnsignedChar => value as u8 as i64,
            IntType::Short => value as i16 as i64,
            IntType::UnsignedShort => value as u16 as i64,
        }
    }
}

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
                    // the members' slots alone: their layouts would read their own members again
                    // (and theirs), twice for each struct inside another
                    let sizes = (0..found.members.len()).map(|at| found.slots_of(at, structs));
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
                for at in 0..found.members.len() {
                    let (bytes, member_align) = found.layout_of(at, structs);
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
            Ty::Struct(tag) => format!("struct {}", tag_shown(tag)),
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

/// A tag as the source wrote it: a block's own struct's tag (T#key_n, see parse::State::declared_tag) is T, and an
/// unnamed struct's made-up one "(unnamed)".
pub fn tag_shown(tag: &str) -> &str {
    if tag.starts_with("__anonymous") {
        return "(unnamed)";
    }
    tag.split('#').next().unwrap_or(tag)
}

/// value rounded up to a multiple of align.
fn aligned(value: i64, align: i64) -> i64 {
    (value + align - 1) / align * align
}

/// A member of a struct or union, by its name.
pub fn member(structs: &Structs, tag: &str, name: &str) -> Option<Member> {
    let found = structs.get(tag)?;
    let (mut slots, mut bytes) = (0, 0);
    for (at, (member, ty)) in found.members.iter().enumerate() {
        let takes = found.takes(at, structs);
        let bits = found.bits_of(at);
        if !found.union && takes.slots > 0 {
            bytes = aligned(bytes, takes.align);
        }
        if member == name && !name.is_empty() {
            // a bit-field that shares its slot is in the slot the last one started
            let shared = bits.is_some_and(|bits| bits.offset > 0) && !found.union;
            let (slot, byte) = if shared { (slots - 1, bytes - 4) } else { (slots, bytes) };
            return Some(Member { slot, byte, ty: ty.clone(), bits });
        }
        if !found.union {
            slots += takes.slots;
            bytes += takes.bytes;
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
            for (index, (_, member)) in found.members.iter().enumerate() {
                match found.bits_of(index) {
                    // a slot of bit-fields is an int; the fields that share it add nothing
                    Some(bits) if bits.offset > 0 => continue,
                    Some(_) => out.push((at, int())),
                    None => scalar_slots(member, structs, at, out),
                }
                if found.union {
                    let end = base + ty.slots(structs);
                    for rest in at + member.slots(structs)..end {
                        out.push((rest, int()));
                    }
                    return;
                }
                at += found.slots_of(index, structs);
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
            let listed = composite["members"].as_array().map_or(&[][..], Vec::as_slice);
            let members = listed
                .iter()
                .map(|member| (member["name"].as_str().unwrap_or("").to_string(), Ty::from_node(&member["type"])))
                .collect();
            let bits: Vec<Option<Bits>> = listed
                .iter()
                .map(|member| {
                    let pair = member.get("bits")?.as_array()?;
                    Some(Bits { offset: pair.first()?.as_i64()?, width: pair.get(1)?.as_i64()? })
                })
                .collect();
            let bits = if bits.iter().any(Option::is_some) { bits } else { Vec::new() };
            found.insert(tag.to_string(), Composite { union: composite["union"] == Node::Bool(true), members, bits });
        }
    }
    found
}

/// Structs as a program's tree holds them.
pub fn structs_node(structs: &Structs) -> Node {
    let mut map = Map::new();
    for (tag, composite) in structs {
        let members: Vec<Node> = composite
            .members
            .iter()
            .enumerate()
            .map(|(at, (name, ty))| {
                let member = node!({"name": name.as_str(), "type": ty.to_node()});
                match composite.bits_of(at) {
                    Some(bits) => member.with("bits", node!([bits.offset, bits.width])),
                    None => member,
                }
            })
            .collect();
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
        let pair = Composite::new(
            false,
            vec![
                ("c".into(), Ty::Scalar("char".into())),
                ("i".into(), int()),
                ("s".into(), Ty::Scalar("short".into())),
            ],
        );
        structs.insert("pair".into(), pair);
        structs.insert(
            "value".into(),
            Composite::new(
                true,
                vec![("f".into(), Ty::Scalar("float".into())), ("a".into(), Ty::Array(Box::new(int()), 3))],
            ),
        );
        let pair = Ty::Struct("pair".into());
        assert_eq!((pair.bytes(&structs), pair.slots(&structs)), (12, 3));
        assert_eq!(member(&structs, "pair", "i"), Some(Member { slot: 1, byte: 4, ty: int(), bits: None }));
        assert_eq!(member(&structs, "pair", "s").map(|found| (found.slot, found.byte)), Some((2, 8)));
        let value = Ty::Struct("value".into());
        assert_eq!((value.bytes(&structs), value.slots(&structs)), (12, 3));
        let mut slots = Vec::new();
        scalar_slots(&value, &structs, 0, &mut slots);
        assert_eq!(slots, vec![(0, Ty::Scalar("float".into())), (1, int()), (2, int())]);
        let node = Ty::Function(Box::new(pointer(pair.clone())), vec![int()], true).to_node();
        assert_eq!(Ty::from_node(&node), Ty::Function(Box::new(pointer(pair)), vec![int()], true));
    }

    #[test]
    fn structs_nested_deep_are_measured_in_time_linear_in_their_depth() {
        // each struct holds the one inside it and an int; measuring a level's slots once read its
        // members' layouts as well, which doubled the work at each level (2^80 here)
        let mut structs = Structs::new();
        structs.insert("s0".into(), Composite::new(false, vec![("i".into(), int())]));
        for level in 1..80 {
            let inner = Ty::Struct(format!("s{}", level - 1));
            structs.insert(format!("s{level}"), Composite::new(false, vec![("m".into(), inner), ("i".into(), int())]));
        }
        let outer = Ty::Struct("s79".into());
        assert_eq!((outer.slots(&structs), outer.bytes(&structs)), (80, 320));
        assert_eq!(member(&structs, "s79", "i").map(|found| (found.slot, found.byte)), Some((79, 316)));
    }

    #[test]
    fn bit_fields_share_slots_and_keep_their_bits_in_the_tree() {
        let unsigned = Ty::Scalar("unsigned int".into());
        let mut flags = Composite::new(
            false,
            vec![("a".into(), unsigned.clone()), ("b".into(), int()), ("c".into(), int()), ("d".into(), unsigned)],
        );
        flags.bits = vec![
            Some(Bits { offset: 0, width: 3 }),
            Some(Bits { offset: 3, width: 29 }),
            None,
            Some(Bits { offset: 0, width: 1 }),
        ];
        let mut structs = Structs::new();
        structs.insert("flags".into(), flags);
        let ty = Ty::Struct("flags".into());
        assert_eq!((ty.bytes(&structs), ty.slots(&structs)), (12, 3));
        let b = Member { slot: 0, byte: 0, ty: int(), bits: Some(Bits { offset: 3, width: 29 }) };
        assert_eq!(member(&structs, "flags", "b"), Some(b));
        assert_eq!(member(&structs, "flags", "c").map(|found| (found.slot, found.byte)), Some((1, 4)));
        assert_eq!(member(&structs, "flags", "d").map(|found| found.slot), Some(2));
        let mut slots = Vec::new();
        scalar_slots(&ty, &structs, 0, &mut slots);
        assert_eq!(slots.iter().map(|(slot, _)| *slot).collect::<Vec<_>>(), vec![0, 1, 2]);
        let program = node!({"structs": structs_node(&structs)});
        assert_eq!(structs_of(&program), structs);
        // a field that reaches bit 31 comes out of the VM's >> spread already
        assert_eq!(Bits { offset: 3, width: 29 }.mask(), -8);
        assert_eq!(Bits { offset: 0, width: 3 }.packed(node!({"kind": "int", "value": 9}))["value"], Node::Int(1));
    }
}
