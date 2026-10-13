//! Comments on statements from the definition set's string translations (strings/<language>.json,
//! keyed by a hash of the Japanese text), and from the same script in another language: what the
//! statement's strings say in English.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use indexmap::IndexMap;

use crate::node::Node;
use crate::{Error, Result};

/// The key a translation is stored under: the first 16 hex digits of the text's SHA-1.
pub fn string_key(text: &str) -> String {
    sha1_smol::Sha1::from(text.as_bytes()).digest().to_string()[..16].to_string()
}

/// The translations in a folder for a language: {key: English}.
pub fn load(directory: &Path, language: &str) -> Result<IndexMap<String, String>> {
    let path = directory.join(format!("{language}.json"));
    if !path.is_file() {
        return Ok(IndexMap::new());
    }
    let mut value: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path)?)
        .map_err(|error| Error::invalid(format!("{}: {error}", path.display())))?;
    let serde_json::Value::Object(strings) = value["strings"].take() else {
        return Err(Error::invalid(format!("{} has no strings", path.display())));
    };
    Ok(strings.into_iter().filter_map(|(key, value)| value.as_str().map(|text| (key, text.to_string()))).collect())
}

fn walk_strings(node: &Node, out: &mut Vec<String>) {
    match node {
        Node::List(items) => items.iter().for_each(|item| walk_strings(item, out)),
        Node::Map(map) => {
            if node.kind() == "string" {
                if let Some(text) = node["text"].as_str() {
                    out.push(text.to_string());
                }
            }
            map.values().for_each(|value| walk_strings(value, out));
        }
        _ => {}
    }
}

/// The strings a tree pushes, in order.
pub fn strings_in(node: &Node) -> Vec<String> {
    let mut out = Vec::new();
    walk_strings(node, &mut out);
    out
}

fn translated(texts: &[String], translations: &IndexMap<String, String>) -> String {
    let mut found: Vec<&String> = Vec::new();
    for text in texts {
        if let Some(english) = translations.get(&string_key(text)) {
            if !found.contains(&english) {
                found.push(english);
            }
        }
    }
    found.iter().map(|text| text.as_str()).collect::<Vec<_>>().join("; ")
}

fn comment_of(node: &Node) -> Option<String> {
    node.get("comment").filter(|comment| crate::source::truthy(comment)).map(crate::source::py_str)
}

fn joined(parts: [Option<String>; 2]) -> String {
    parts.into_iter().flatten().filter(|part| !part.is_empty()).collect::<Vec<_>>().join("; ")
}

fn list<'a>(node: &'a Node, key: &str) -> &'a [Node] {
    node[key].as_array().map(Vec::as_slice).unwrap_or(&[])
}

/// Statements with their nested blocks rebuilt by recurse and every other statement passed to
/// leaf.
fn each_statement(statements: &[Node], recurse: &dyn Fn(&[Node]) -> Node, leaf: &dyn Fn(Node) -> Node) -> Node {
    statements
        .iter()
        .map(|node| match node.kind() {
            "if" => {
                let otherwise = match node["else"].as_array() {
                    Some(items) => recurse(items),
                    None => Node::Null,
                };
                node.clone().with("then", recurse(list(node, "then"))).with("else", otherwise)
            }
            "while" => node.clone().with("body", recurse(list(node, "body"))),
            _ => leaf(node.clone()),
        })
        .collect()
}

fn annotated(statements: &[Node], translations: &IndexMap<String, String>) -> Node {
    each_statement(statements, &|inner| annotated(inner, translations), &|node| {
        let comment = translated(&strings_in(&node), translations);
        if comment.is_empty() {
            return node;
        }
        let old = comment_of(&node);
        node.with("comment", joined([Some(comment), old]))
    })
}

fn sibling_note(node: &Node, siblings: &HashMap<String, String>) -> String {
    let mut found: Vec<String> = Vec::new();
    for text in strings_in(node) {
        if let Some(english) = siblings.get(&text) {
            if *english != text {
                let replaced = english.replace("<page>", "\n");
                let flat: Vec<&str> = replaced.split('\n').map(c2_py::strip).filter(|part| !part.is_empty()).collect();
                let quoted = format!("\"{}\"", flat.join(" "));
                if !flat.is_empty() && !found.contains(&quoted) {
                    found.push(quoted);
                }
            }
        }
    }
    found.join("; ")
}

fn with_siblings(statements: &[Node], siblings: &HashMap<String, String>) -> Node {
    each_statement(statements, &|inner| with_siblings(inner, siblings), &|node| {
        let comment = sibling_note(&node, siblings);
        if comment.is_empty() {
            return node;
        }
        let old = comment_of(&node);
        node.with("comment", joined([old, Some(comment)]))
    })
}

fn on_bodies(functions: &[Node], change: &dyn Fn(&[Node]) -> Node) -> Vec<Node> {
    functions
        .iter()
        .map(|function| {
            if function.kind() == "function" {
                function.clone().with("body", change(list(function, "body")))
            } else {
                function.clone()
            }
        })
        .collect()
}

pub fn annotate_siblings(functions: Vec<Node>, siblings: &HashMap<String, String>) -> Vec<Node> {
    if siblings.is_empty() {
        return functions;
    }
    on_bodies(&functions, &|body| with_siblings(body, siblings))
}

/// The English the same script in another language gives each of this one's strings, where the
/// two push the same number of strings and a string always meets the same English.
pub fn sibling_strings(strings: &[String], other: &[String]) -> HashMap<String, String> {
    if strings.len() != other.len() {
        return HashMap::new();
    }
    let mut found: IndexMap<&String, &String> = IndexMap::new();
    let mut clashing = Vec::new();
    for (text, english) in strings.iter().zip(other) {
        if *found.entry(text).or_insert(english) != english {
            clashing.push(text);
        }
    }
    found
        .into_iter()
        .filter(|(text, english)| !clashing.contains(text) && text != english)
        .map(|(text, english)| (text.clone(), english.clone()))
        .collect()
}

pub fn annotate_functions(functions: Vec<Node>, translations: &IndexMap<String, String>) -> Vec<Node> {
    if translations.is_empty() {
        return functions;
    }
    on_bodies(&functions, &|body| annotated(body, translations))
}

pub fn annotate(program: Node, translations: &IndexMap<String, String>) -> Node {
    if program.get("asm").is_some() {
        return program;
    }
    let functions = annotate_functions(list(&program, "functions").to_vec(), translations);
    program.with("functions", functions)
}
