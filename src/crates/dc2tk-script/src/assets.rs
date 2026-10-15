//! English names for the game's asset names. Scripts pick motions, texture animations, effects,
//! map parts and damage settings by name, and the game's files name them in Japanese ("立ち" is
//! the standing motion), so a script has to pass those exact strings. A whole-disc decompile writes
//! a #define for each one the scripts pass to such a parameter into disc.h, named from the
//! definition set's English gloss (MOTION_STAND), and the scripts use the names. The Japanese comes
//! from the disc and the English from the set, as with the item and monster names in disc.h.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use indexmap::IndexMap;
use regex::Regex;

use crate::node::Node;
use crate::sdk::Context;
use crate::{asm, notes};

const MOST_WORDS: usize = 6;

/// The kind of asset a native parameter takes, by the parameter's name in the SDK.
fn prefix_of(param: &str) -> Option<&'static str> {
    Some(match param {
        "motion_name" | "talk_motion" | "idle_motion" => "MOTION",
        "anim_name" | "talk_anim" | "idle_anim" => "ANIM",
        "effect_name" | "effect" => "EFFECT",
        "part" => "PART",
        "hit_name" | "damage_name" => "HIT",
        "texture_name" => "TEXTURE",
        _ => return None,
    })
}

/// Whether text has kana, kanji or full-width forms.
fn has_japanese(text: &str) -> bool {
    text.chars().any(|c| matches!(c, '\u{3040}'..='\u{30ff}' | '\u{4e00}'..='\u{9fff}' | '\u{ff00}'..='\u{ffef}'))
}

/// The asset strings a native call passes: (position, prefix, text).
fn asset_args(node: &Node, found: &Context) -> Vec<(usize, &'static str, String)> {
    let Some(declared) = found.by_original.get(node["name"].as_str().unwrap_or("")) else {
        return Vec::new();
    };
    let params = declared.list("params");
    let mut out = Vec::new();
    for (position, arg) in node.list("args").iter().enumerate() {
        if position >= params.len() || arg.kind() != "string" {
            continue;
        }
        let text = arg["text"].as_str().unwrap_or("");
        if let Some(prefix) = prefix_of(params[position]["name"].as_str().unwrap_or("")) {
            if has_japanese(text) && !text.contains('/') {
                out.push((position, prefix, text.to_string()));
            }
        }
    }
    out
}

fn walk(nodes: &[Node], found: &Context, out: &mut BTreeSet<(String, String)>) {
    for node in nodes {
        if node.kind() == "native" {
            for (_, prefix, text) in asset_args(node, found) {
                out.insert((prefix.to_string(), text));
            }
        }
        for key in ["then", "else", "body"] {
            if let Some(items) = node.get(key).and_then(Node::as_array) {
                walk(items, found, out);
            }
        }
    }
}

/// Adds the asset names a program passes to found_names, as (prefix, text).
pub fn survey(found_names: &mut BTreeSet<(String, String)>, program: &Node, found: Option<&Context>) {
    let Some(found) = found else {
        return;
    };
    if program.get("asm").is_some() {
        return;
    }
    for function in program.list("functions").iter().filter(|function| function.kind() == "function") {
        walk(function.list("body"), found, found_names);
    }
}

fn identifier(gloss: &str) -> String {
    static SEPARATORS: LazyLock<Regex> = LazyLock::new(|| Regex::new("[^A-Za-z0-9]+").unwrap());
    let spaced = SEPARATORS.replace_all(gloss, " ");
    spaced.split_whitespace().take(MOST_WORDS).collect::<Vec<_>>().join("_").to_uppercase()
}

/// {(prefix, text): name} for the asset names that have a gloss. Two names that would read the
/// same are numbered in the order of their Japanese text, so a disc always gets the same names.
pub fn names(
    found_names: &BTreeSet<(String, String)>,
    glosses: &IndexMap<String, String>,
    taken: &std::collections::HashSet<String>,
) -> IndexMap<(String, String), String> {
    let mut out = IndexMap::new();
    let mut used = taken.clone();
    for (prefix, text) in found_names {
        let Some(gloss) = glosses.get(&notes::string_key(text)).filter(|gloss| !gloss.is_empty()) else {
            continue;
        };
        let words = identifier(gloss);
        if words.is_empty() {
            continue;
        }
        let base = format!("{prefix}_{words}");
        let mut name = base.clone();
        let mut copy = 1;
        while used.contains(&name) {
            copy += 1;
            name = format!("{base}_{copy}");
        }
        used.insert(name.clone());
        out.insert((prefix.clone(), text.clone()), name);
    }
    out
}

pub fn header_text(asset_names: &IndexMap<(String, String), String>) -> String {
    let mut lines = vec![
        String::new(),
        "// English names for the game's asset names (motions, texture animations, effects, map parts,".to_string(),
        "// damage settings), which the game's files name in Japanese.".to_string(),
    ];
    let mut sorted: Vec<(&(String, String), &String)> = asset_names.iter().collect();
    sorted.sort_by(|a, b| a.1.cmp(b.1));
    for ((_, text), name) in sorted {
        lines.push(format!("#define {} {}", name, asm::quote(text)));
    }
    lines.join("\n") + "\n"
}

/// disc.h with the asset names added before its #endif.
pub fn with_header(generated: Option<String>, asset_names: &IndexMap<(String, String), String>) -> Option<String> {
    let generated = generated?;
    if asset_names.is_empty() {
        return Some(generated);
    }
    Some(crate::disc::with_lines_before_endif(&generated, &header_text(asset_names)))
}

/// Puts the names in for the asset strings a function passes to natives (before the natives are
/// shown with their SDK names, while their arguments still line up with the declarations).
pub fn named(nodes: &Node, found: &Context) -> Node {
    if found.asset_names.is_empty() {
        return nodes.clone();
    }
    let Some(items) = nodes.as_array() else {
        return nodes.clone();
    };
    items
        .iter()
        .map(|node| {
            let mut node = node.clone();
            if node.kind() == "native" {
                let mut args = node.list("args").to_vec();
                for (position, prefix, text) in asset_args(&node, found) {
                    if let Some(name) = found.asset_names.get(&(prefix.to_string(), text)) {
                        args[position] = node!({"kind": "var", "name": name.clone()});
                    }
                }
                node = node.with("args", args);
            }
            match node.kind() {
                "if" => {
                    let otherwise = if node["else"].is_null() { Node::Null } else { named(&node["else"], found) };
                    let then = named(&node["then"], found);
                    node.with("then", then).with("else", otherwise)
                }
                "while" => {
                    let body = named(&node["body"], found);
                    node.with("body", body)
                }
                _ => node,
            }
        })
        .collect()
}
