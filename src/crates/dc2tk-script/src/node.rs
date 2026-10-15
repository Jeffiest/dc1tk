//! The program tree: JSON-like values, cheap to build. An object keeps its keys in insertion
//! order (the order the tree is written in) in a small vector, since tree nodes have a
//! handful of keys; keys are mostly the literals the code writes, so they are not allocated. The
//! API follows serde_json's Value (`node!` takes json!'s syntax), and serializes to the same JSON.

use std::borrow::Cow;
use std::fmt;
use std::ops::{Index, IndexMut};

use serde::ser::{Serialize, SerializeMap, SerializeSeq, Serializer};

pub type Key = Cow<'static, str>;

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Node {
    #[default]
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    List(Vec<Node>),
    Map(Map),
}

/// An object: keys in insertion order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Map {
    entries: Vec<(Key, Node)>,
}

impl Map {
    pub fn new() -> Map {
        Map { entries: Vec::new() }
    }

    pub fn with_capacity(capacity: usize) -> Map {
        Map { entries: Vec::with_capacity(capacity) }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, key: &str) -> Option<&Node> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, value)| value)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Node> {
        self.entries.iter_mut().find(|(k, _)| k == key).map(|(_, value)| value)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    /// Sets a key: in its place when the object has it, else last (as dict assignment does).
    pub fn insert(&mut self, key: impl Into<Key>, value: Node) -> Option<Node> {
        let key = key.into();
        match self.entries.iter().position(|(k, _)| *k == key) {
            Some(at) => Some(std::mem::replace(&mut self.entries[at].1, value)),
            None => {
                self.entries.push((key, value));
                None
            }
        }
    }

    /// Removes a key, keeping the others in order.
    pub fn remove(&mut self, key: &str) -> Option<Node> {
        let at = self.entries.iter().position(|(k, _)| k == key)?;
        Some(self.entries.remove(at).1)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &Node)> {
        self.entries.iter().map(|(key, value)| (key.as_ref(), value))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&str, &mut Node)> {
        self.entries.iter_mut().map(|(key, value)| (&**key, value))
    }

    /// The keys and values as stored, to copy keys without allocating.
    pub fn entries(&self) -> impl Iterator<Item = &(Key, Node)> {
        self.entries.iter()
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(key, _)| key.as_ref())
    }

    pub fn values(&self) -> impl Iterator<Item = &Node> {
        self.entries.iter().map(|(_, value)| value)
    }

    pub fn retain(&mut self, mut keep: impl FnMut(&str, &mut Node) -> bool) {
        self.entries.retain_mut(|(key, value)| keep(key, value));
    }
}

impl IntoIterator for Map {
    type Item = (Key, Node);
    type IntoIter = std::vec::IntoIter<(Key, Node)>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.into_iter()
    }
}

impl<K: Into<Key>> FromIterator<(K, Node)> for Map {
    fn from_iter<I: IntoIterator<Item = (K, Node)>>(items: I) -> Map {
        let mut map = Map::new();
        for (key, value) in items {
            map.insert(key, value);
        }
        map
    }
}

/// The kinds of the program tree's nodes, as the parser and the decompiler write them. A pass that
/// tells nodes apart by kind names them here, so a misspelt kind is a compile error rather than a
/// kind no node has.
pub mod kinds {
    // the program and what it holds
    pub const PROGRAM: &str = "program";
    pub const FUNCTION: &str = "function";
    /// A function kept as assembly.
    pub const ASM: &str = "asm";
    pub const INCLUDE: &str = "include";

    // statements
    pub const EXPR: &str = "expr";
    pub const RETURN: &str = "return";
    pub const NATIVE: &str = "native";
    pub const PRINT: &str = "print";
    pub const WAIT: &str = "wait";
    pub const END: &str = "end";
    pub const SKIP_END: &str = "skip_end";
    pub const LABEL: &str = "label";
    pub const GOTO: &str = "goto";
    pub const IF: &str = "if";
    pub const WHILE: &str = "while";
    pub const SWITCH: &str = "switch";
    pub const BREAK: &str = "break";
    pub const CONTINUE: &str = "continue";

    // expressions
    pub const INT: &str = "int";
    pub const FLOAT: &str = "float";
    pub const STRING: &str = "string";
    /// L"...": an array of wchar_t.
    pub const WIDE: &str = "wide";
    pub const VAR: &str = "var";
    /// a[i] of an array or pointer variable of the VM's own types.
    pub const INDEX: &str = "index";
    pub const DEREF: &str = "deref";
    pub const ADDR: &str = "addr";
    pub const ASSIGN: &str = "assign";
    /// target op= value.
    pub const COMPOUND: &str = "compound";
    pub const BINARY: &str = "binary";
    /// && and ||.
    pub const LOGICAL: &str = "logical";
    pub const UNARY: &str = "unary";
    pub const CAST: &str = "cast";
    /// sin and cos, which the VM has instructions for.
    pub const BUILTIN: &str = "builtin";
    pub const CALL: &str = "call";
    /// A call through a function pointer.
    pub const CALL_PTR: &str = "call_ptr";
    pub const COMMA: &str = "comma";
    pub const TERNARY: &str = "ternary";
    /// s.m and p->m.
    pub const MEMBER: &str = "member";
    /// e[i] of any expression of C's other types.
    pub const SUBSCRIPT: &str = "subscript";
    pub const VA_ARG: &str = "va_arg";
    pub const VA_START: &str = "va_start";

    // the forms lowering gives (composite.rs, lower.rs) and the compiler's own
    /// A slot of a variable's storage at a constant offset.
    pub const FIELD: &str = "field";
    /// An element through a pointer an expression gives.
    pub const AT: &str = "at";
    /// A value wrapped to a narrow integer type's width.
    pub const NARROW: &str = "narrow";
    /// An element through the address a hidden local holds (compile.rs's compound assignments).
    pub const VIA: &str = "via";
}

/// A program with nothing in it yet, its keys in the order the tree has them: the header's fields
/// as the original compiler wrote them (no main yet, the global count from the globals), no
/// globals, programs or functions, no declarations.
pub fn program_skeleton() -> Node {
    let header_padding = "00".repeat(crate::stb::HEADER_SIZE - crate::stb::HEADER_FIELDS_SIZE);
    crate::node!({
        "kind": kinds::PROGRAM,
        "natives": null,
        "magic": "SB2\0",
        "unk_14": 0,
        "header_padding": header_padding,
        "globals": [],
        "global_num": null,
        "main": null,
        "programs": [],
        "functions": [],
        "string_pool": false,
        "padding_of": {},
        "declarations": {"natives": [], "programs": [], "inlines": [], "constants": [], "kinds": {}},
    })
}

static NULL: Node = Node::Null;

impl Node {
    pub fn is_null(&self) -> bool {
        matches!(self, Node::Null)
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Node::Str(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Node::Int(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Node::Float(value) => Some(*value),
            Node::Int(value) => Some(*value as f64),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Node::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&Vec<Node>> {
        match self {
            Node::List(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_array_mut(&mut self) -> Option<&mut Vec<Node>> {
        match self {
            Node::List(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&Map> {
        match self {
            Node::Map(map) => Some(map),
            _ => None,
        }
    }

    pub fn as_object_mut(&mut self) -> Option<&mut Map> {
        match self {
            Node::Map(map) => Some(map),
            _ => None,
        }
    }

    /// The value of a key of an object; None for a missing key or a node that is not an object.
    pub fn get(&self, key: &str) -> Option<&Node> {
        self.as_object()?.get(key)
    }

    /// The "kind" of a tree node, or "".
    pub fn kind(&self) -> &str {
        self.text("kind")
    }

    /// The list under a key; empty for a missing key or one that holds something else.
    pub fn list(&self, key: &str) -> &[Node] {
        self.get(key).and_then(Node::as_array).map_or(&[], Vec::as_slice)
    }

    /// The text under a key; "" for a missing key or one that holds something else.
    pub fn text(&self, key: &str) -> &str {
        self.get(key).and_then(Node::as_str).unwrap_or("")
    }

    /// The value under a key, Null left in its place (Null for a missing key, which stays
    /// missing): a child to rebuild without copying it, put back with node[key] = value.
    pub fn take(&mut self, key: &str) -> Node {
        self.as_object_mut().and_then(|map| map.get_mut(key)).map(std::mem::take).unwrap_or_default()
    }

    /// Removes a key of an object, keeping the others in order.
    pub fn remove(&mut self, key: &str) -> Option<Node> {
        self.as_object_mut()?.remove(key)
    }

    /// The items of a list; empty for anything else.
    pub fn into_list(self) -> Vec<Node> {
        match self {
            Node::List(items) => items,
            _ => Vec::new(),
        }
    }

    /// Whether a flag the parser sets is on: the key holds true. The parser writes a flag only
    /// when it is on ("pointer": true), so a missing key, false or any other value is off.
    pub fn flag(&self, key: &str) -> bool {
        matches!(self.get(key), Some(Node::Bool(true)))
    }

    /// Equal as values: objects whatever their key order, and numbers by value (1 == 1.0 ==
    /// True).
    pub fn same_value(&self, other: &Node) -> bool {
        let number = |node: &Node| match node {
            Node::Bool(value) => Some(*value as i64 as f64),
            Node::Int(value) => Some(*value as f64),
            Node::Float(value) => Some(*value),
            _ => None,
        };
        match (self, other) {
            (Node::Int(a), Node::Int(b)) => a == b,
            (Node::Null, Node::Null) => true,
            (Node::Str(a), Node::Str(b)) => a == b,
            (Node::List(a), Node::List(b)) => a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.same_value(y)),
            (Node::Map(a), Node::Map(b)) => {
                a.len() == b.len()
                    && a.iter().all(|(key, value)| b.get(key).is_some_and(|found| value.same_value(found)))
            }
            _ => match (number(self), number(other)) {
                (Some(a), Some(b)) => a == b,
                _ => false,
            },
        }
    }

    /// The node with a key set: in its place if the node has it, else last.
    pub fn with(mut self, key: impl Into<Key>, value: impl Into<Node>) -> Node {
        if let Node::Map(map) = &mut self {
            map.insert(key, value.into());
        }
        self
    }
}

impl Index<&str> for Node {
    type Output = Node;

    /// A missing key, or indexing something that is not an object, gives null (as serde_json).
    fn index(&self, key: &str) -> &Node {
        self.get(key).unwrap_or(&NULL)
    }
}

impl IndexMut<&str> for Node {
    /// Sets a key of an object, adding it last when new; a null node becomes an object.
    fn index_mut(&mut self, key: &str) -> &mut Node {
        if self.is_null() {
            *self = Node::Map(Map::new());
        }
        let Node::Map(map) = self else {
            panic!("cannot set {key:?} on a node that is not an object");
        };
        if !map.contains_key(key) {
            map.insert(key.to_string(), Node::Null);
        }
        map.get_mut(key).unwrap()
    }
}

impl Index<usize> for Node {
    type Output = Node;

    fn index(&self, index: usize) -> &Node {
        self.as_array().and_then(|items| items.get(index)).unwrap_or(&NULL)
    }
}

impl IndexMut<usize> for Node {
    fn index_mut(&mut self, index: usize) -> &mut Node {
        &mut self.as_array_mut().expect("indexing a node that is not a list")[index]
    }
}

macro_rules! from_int {
    ($($t:ty),*) => {$(
        impl From<$t> for Node {
            fn from(value: $t) -> Node {
                Node::Int(value as i64)
            }
        }
    )*};
}

from_int!(i8, i16, i32, i64, u8, u16, u32, usize, isize);

impl From<f64> for Node {
    fn from(value: f64) -> Node {
        Node::Float(value)
    }
}

impl From<bool> for Node {
    fn from(value: bool) -> Node {
        Node::Bool(value)
    }
}

impl From<&str> for Node {
    fn from(value: &str) -> Node {
        Node::Str(value.to_string())
    }
}

impl From<String> for Node {
    fn from(value: String) -> Node {
        Node::Str(value)
    }
}

impl From<&String> for Node {
    fn from(value: &String) -> Node {
        Node::Str(value.clone())
    }
}

impl From<Map> for Node {
    fn from(value: Map) -> Node {
        Node::Map(value)
    }
}

impl<T: Into<Node>> From<Vec<T>> for Node {
    fn from(items: Vec<T>) -> Node {
        Node::List(items.into_iter().map(Into::into).collect())
    }
}

impl<T: Clone + Into<Node>> From<&[T]> for Node {
    fn from(items: &[T]) -> Node {
        Node::List(items.iter().cloned().map(Into::into).collect())
    }
}

impl<T: Into<Node>> From<Option<T>> for Node {
    fn from(value: Option<T>) -> Node {
        value.map_or(Node::Null, Into::into)
    }
}

impl From<&Node> for Node {
    fn from(value: &Node) -> Node {
        value.clone()
    }
}

impl<T: Into<Node>> FromIterator<T> for Node {
    fn from_iter<I: IntoIterator<Item = T>>(items: I) -> Node {
        Node::List(items.into_iter().map(Into::into).collect())
    }
}

impl From<serde_json::Value> for Node {
    /// JSON as read from a file, objects in their order there.
    fn from(value: serde_json::Value) -> Node {
        match value {
            serde_json::Value::Null => Node::Null,
            serde_json::Value::Bool(value) => Node::Bool(value),
            serde_json::Value::Number(number) => match number.as_i64() {
                Some(value) => Node::Int(value),
                None => Node::Float(number.as_f64().unwrap_or(f64::NAN)),
            },
            serde_json::Value::String(text) => Node::Str(text),
            serde_json::Value::Array(items) => Node::List(items.into_iter().map(Node::from).collect()),
            serde_json::Value::Object(object) => {
                Node::Map(object.into_iter().map(|(key, value)| (key, Node::from(value))).collect())
            }
        }
    }
}

impl PartialEq<str> for Node {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == Some(other)
    }
}

impl PartialEq<&str> for Node {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == Some(*other)
    }
}

impl Serialize for Node {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Node::Null => serializer.serialize_unit(),
            Node::Bool(value) => serializer.serialize_bool(*value),
            Node::Int(value) => serializer.serialize_i64(*value),
            Node::Float(value) => serializer.serialize_f64(*value),
            Node::Str(text) => serializer.serialize_str(text),
            Node::List(items) => {
                let mut seq = serializer.serialize_seq(Some(items.len()))?;
                for item in items {
                    seq.serialize_element(item)?;
                }
                seq.end()
            }
            Node::Map(map) => {
                let mut out = serializer.serialize_map(Some(map.len()))?;
                for (key, value) in map.iter() {
                    out.serialize_entry(key, value)?;
                }
                out.end()
            }
        }
    }
}

impl fmt::Display for Node {
    /// Compact JSON: no spaces, and text as it is (not \u-escaped).
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&serde_json::to_string(self).map_err(|_| fmt::Error)?)
    }
}

/// A Node from JSON-like syntax, as serde_json's json!: `node!({"kind": "var", "name": name})`.
/// Object keys are string literals.
#[macro_export]
macro_rules! node {
    (null) => { $crate::node::Node::Null };
    ([]) => { $crate::node::Node::List(::std::vec::Vec::new()) };
    ([ $($tt:tt)+ ]) => { $crate::node::Node::List($crate::node_list!(@items [] $($tt)+)) };
    ({}) => { $crate::node::Node::Map($crate::node::Map::new()) };
    ({ $($tt:tt)+ }) => {{
        let mut map = $crate::node::Map::with_capacity(8);
        $crate::node_map!(@entries map $($tt)+);
        $crate::node::Node::Map(map)
    }};
    ($other:expr) => { $crate::node::Node::from($other) };
}

#[doc(hidden)]
#[macro_export]
macro_rules! node_list {
    // done
    (@items [$($done:expr),*]) => { vec![$($done),*] };
    // a value that is a list or object literal, or null
    (@items [$($done:expr),*] null $(, $($rest:tt)*)?) => {
        $crate::node_list!(@items [$($done,)* $crate::node::Node::Null] $($($rest)*)?)
    };
    (@items [$($done:expr),*] [$($inner:tt)*] $(, $($rest:tt)*)?) => {
        $crate::node_list!(@items [$($done,)* $crate::node!([$($inner)*])] $($($rest)*)?)
    };
    (@items [$($done:expr),*] {$($inner:tt)*} $(, $($rest:tt)*)?) => {
        $crate::node_list!(@items [$($done,)* $crate::node!({$($inner)*})] $($($rest)*)?)
    };
    // an expression: munch tokens up to the next top-level comma
    (@items [$($done:expr),*] $($rest:tt)+) => {
        $crate::node_list!(@expr [$($done),*] [] $($rest)+)
    };
    (@expr [$($done:expr),*] [$($e:tt)+] , $($rest:tt)*) => {
        $crate::node_list!(@items [$($done,)* $crate::node::Node::from($($e)+)] $($rest)*)
    };
    (@expr [$($done:expr),*] [$($e:tt)+]) => {
        $crate::node_list!(@items [$($done,)* $crate::node::Node::from($($e)+)])
    };
    (@expr [$($done:expr),*] [$($e:tt)*] $next:tt $($rest:tt)*) => {
        $crate::node_list!(@expr [$($done),*] [$($e)* $next] $($rest)*)
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! node_map {
    (@entries $map:ident) => {};
    (@entries $map:ident $key:literal : null $(, $($rest:tt)*)?) => {
        $map.insert($key, $crate::node::Node::Null);
        $crate::node_map!(@entries $map $($($rest)*)?);
    };
    (@entries $map:ident $key:literal : [$($inner:tt)*] $(, $($rest:tt)*)?) => {
        $map.insert($key, $crate::node!([$($inner)*]));
        $crate::node_map!(@entries $map $($($rest)*)?);
    };
    (@entries $map:ident $key:literal : {$($inner:tt)*} $(, $($rest:tt)*)?) => {
        $map.insert($key, $crate::node!({$($inner)*}));
        $crate::node_map!(@entries $map $($($rest)*)?);
    };
    (@entries $map:ident $key:literal : $($rest:tt)+) => {
        $crate::node_map!(@expr $map $key [] $($rest)+);
    };
    (@expr $map:ident $key:literal [$($e:tt)+] , $($rest:tt)*) => {
        $map.insert($key, $crate::node::Node::from($($e)+));
        $crate::node_map!(@entries $map $($rest)*);
    };
    (@expr $map:ident $key:literal [$($e:tt)+]) => {
        $map.insert($key, $crate::node::Node::from($($e)+));
    };
    (@expr $map:ident $key:literal [$($e:tt)*] $next:tt $($rest:tt)*) => {
        $crate::node_map!(@expr $map $key [$($e)* $next] $($rest)*);
    };
}

#[cfg(test)]
mod tests {
    #[test]
    fn builds_and_serializes_like_json() {
        let slot = 3;
        let inner = node!({"kind": "int", "value": 5});
        let node = node!({
            "kind": "index",
            "name": format!("v{slot}"),
            "index": inner,
            "labels": [1, 2, slot + 1],
            "else": null,
            "nested": {"a": [], "b": {}},
            "flag": slot == 3,
        });
        assert_eq!(
            node.to_string(),
            r#"{"kind":"index","name":"v3","index":{"kind":"int","value":5},"labels":[1,2,4],"else":null,"nested":{"a":[],"b":{}},"flag":true}"#
        );
        assert_eq!(node["name"], "v3");
        assert!(node["missing"].is_null());
        assert_eq!(node.kind(), "index");
    }

    #[test]
    fn with_keeps_places() {
        let node = node!({"kind": "if", "start": 1, "else": null});
        let node = node.with("start", 7).with("end", 9);
        assert_eq!(node.to_string(), r#"{"kind":"if","start":7,"else":null,"end":9}"#);
    }
}
