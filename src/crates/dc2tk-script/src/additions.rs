//! A mod's additions: changes it makes to scripts without carrying copies of them. mod.json lists
//! them under "additions", each picking scripts by map or by path and saying what to add:
//!
//!   {"map": "s17",                                   every script of map s17 (its language variants)
//!    "scripts": ["map/s/s17/s17_?.c"],               or paths without readable names, with * and ?
//!    "programs": {"9000": "start", "treasure_chest": "open_chest"},   new PROGRAM lines
//!    "includes": ["lib/mods/calculator/calculator.c"],               libraries to include
//!    "wrap": {"map_start": {"before": ["setup"], "after": ["greet"]}}}   run around a program
//!
//! Since only the lines a mod adds change, several mods can add to the same script, and a mod
//! still fits the scripts after the decompiler improves. A program a script already has can be
//! wrapped but not added again. A wrapped program's "after" functions run when its function
//! returns, which a program that warps to another map never does.

use std::collections::HashMap;
use std::sync::OnceLock;

use indexmap::IndexMap;
use regex::Regex;

use crate::index::{self, Folded};
use crate::node::Node;
use crate::{Error, Result};

fn program_line() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| Regex::new(r"(?m)^PROGRAM\(\s*([^,]+?)\s*,\s*(\w+)\s*\);$").unwrap())
}

/// Whether a script, by its path without names, belongs to a map: in a folder named after it, or a
/// file named after it (s17.c, s17_1.c, s17b_1.c).
pub fn in_map(path: &str, map_name: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    let file = parts[parts.len() - 1];
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    parts[..parts.len() - 1].contains(&map_name)
        || stem == map_name
        || stem.starts_with(&format!("{map_name}_"))
        || (stem.starts_with(map_name) && stem[map_name.len()..].chars().next().is_some_and(dc2tk_py::is_alpha))
}

/// fnmatch.fnmatchcase: * any run, ? any one character, [...] a set.
pub fn fnmatch(name: &str, pattern: &str) -> bool {
    let mut regex = String::from("^(?s:");
    let chars: Vec<char> = pattern.chars().collect();
    let mut at = 0;
    while at < chars.len() {
        match chars[at] {
            '*' => regex.push_str(".*"),
            '?' => regex.push('.'),
            '[' => match chars[at + 1..].iter().position(|&c| c == ']').map(|end| at + 1 + end) {
                Some(end) if end > at + 1 || chars.get(at + 2) == Some(&']') => {
                    let mut set: String = chars[at + 1..end].iter().collect();
                    if let Some(rest) = set.strip_prefix('!') {
                        set = format!("^{rest}");
                    }
                    regex.push('[');
                    regex.push_str(&set.replace('\\', "\\\\"));
                    regex.push(']');
                    at = end;
                }
                _ => regex.push_str("\\["),
            },
            c => regex.push_str(&regex::escape(&c.to_string())),
        }
        at += 1;
    }
    regex.push_str(")$");
    Regex::new(&regex).is_ok_and(|compiled| compiled.is_match(name))
}

pub fn matches(addition: &Node, plain: &str) -> bool {
    if plain.starts_with("lib/") {
        return false;
    }
    if let Some(map_name) = addition["map"].as_str() {
        if in_map(plain, map_name) {
            return true;
        }
    }
    addition["scripts"]
        .as_array()
        .is_some_and(|patterns| patterns.iter().filter_map(Node::as_str).any(|pattern| fnmatch(plain, pattern)))
}

/// A program number as the script writes it: its value when it folds, else the text.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum ProgramKey {
    Number(i128),
    Text(String),
}

fn number_of(text: &str, consts: &IndexMap<String, i64>) -> ProgramKey {
    match index::value(text, consts) {
        Some(Folded::Int(value)) => ProgramKey::Number(value),
        _ => ProgramKey::Text(dc2tk_py::strip(text).to_string()),
    }
}

fn identifier(text: &str) -> String {
    text.chars().map(|c| if dc2tk_py::is_word(c) { c } else { '_' }).collect()
}

fn insert_programs(text: &str, lines: &[String]) -> String {
    static MAIN: OnceLock<Regex> = OnceLock::new();
    let main = MAIN.get_or_init(|| Regex::new(r"(?m)^MAIN\(").unwrap());
    let position = match main.find(text) {
        Some(found) => found.start(),
        None => program_line().find_iter(text).last().map_or(0, |found| (found.end() + 1).min(text.len())),
    };
    let added: String = lines.iter().map(|line| format!("{line}\n")).collect();
    format!("{}{}{}", &text[..position], added, &text[position..])
}

/// Code appended before the trailing #pragma padding_of lines.
pub fn append_code(text: &str, code: &str) -> String {
    static PRAGMAS: OnceLock<Regex> = OnceLock::new();
    let pragmas = PRAGMAS.get_or_init(|| Regex::new(r"(?m)^#pragma padding_of ").unwrap());
    let found = pragmas.find(text);
    let position = found.map_or(text.len(), |found| found.start());
    let tail = if found.is_some() { format!("\n{}", &text[position..]) } else { String::new() };
    format!("{}\n\n{}\n{}", text[..position].trim_end_matches('\n'), code.trim_end_matches('\n'), tail)
}

fn existing(text: &str, consts: &IndexMap<String, i64>) -> HashMap<ProgramKey, (String, String, usize, usize)> {
    let mut found = HashMap::new();
    for capture in program_line().captures_iter(text) {
        let whole = capture.get(0).unwrap();
        found.insert(
            number_of(&capture[1], consts),
            (capture[1].to_string(), capture[2].to_string(), whole.start(), whole.end()),
        );
    }
    found
}

fn strings(node: &Node) -> Vec<String> {
    node.as_array().map(|items| items.iter().filter_map(Node::as_str).map(str::to_string).collect()).unwrap_or_default()
}

/// The script text with one addition applied. mod_name names the mod, for generated function
/// names and messages; place maps a mod file's include path to where this tree keeps it.
pub fn applied(
    text: &str,
    addition: &Node,
    mod_name: &str,
    consts: &IndexMap<String, i64>,
    place: &IndexMap<String, String>,
) -> Result<String> {
    let mut text = text.to_string();
    let mut programs = existing(&text, consts);
    if let Some(wraps) = addition["wrap"].as_object() {
        for (program, wrap) in wraps.iter() {
            let key = number_of(program, consts);
            let mut calls: Vec<String> = strings(&wrap["before"]).iter().map(|name| format!("    {name}();")).collect();
            let mut wrapper = format!("mod_{}_{}", identifier(mod_name), identifier(program));
            let mut number = 2;
            while Regex::new(&format!(r"\b{}\s*\(", regex::escape(&wrapper))).unwrap().is_match(&text) {
                wrapper = format!("mod_{}_{}_{}", identifier(mod_name), identifier(program), number);
                number += 1;
            }
            match programs.get(&key) {
                Some((written, original, start, end)) => {
                    text = format!("{}PROGRAM({}, {});{}", &text[..*start], written, wrapper, &text[*end..]);
                    calls.push(format!("    {original}();"));
                }
                None => text = insert_programs(&text, &[format!("PROGRAM({program}, {wrapper});")]),
            }
            calls.extend(strings(&wrap["after"]).iter().map(|name| format!("    {name}();")));
            text = append_code(
                &text,
                &format!(
                    "// Added by mod {mod_name}: runs around program {program}.\nvoid {wrapper}(void)\n{{\n{}\n}}\n",
                    calls.join("\n")
                ),
            );
            programs = existing(&text, consts);
        }
    }
    let mut added = Vec::new();
    if let Some(new) = addition["programs"].as_object() {
        for (program, function) in new.iter() {
            let key = number_of(program, consts);
            if let Some((written, _, _, _)) = programs.get(&key) {
                return Err(Error::invalid(format!(
                    "mod {mod_name} adds program {program}, which the script already has (as {written}); wrap it instead"
                )));
            }
            added.push(format!("PROGRAM({}, {});", program, crate::source::py_str(function)));
        }
    }
    if !added.is_empty() {
        text = insert_programs(&text, &added);
    }
    for path in strings(&addition["includes"]) {
        let line = format!("#include \"{}\"", place.get(&path).unwrap_or(&path));
        if !text.split('\n').any(|existing| existing == line) {
            text = append_code(&text, &line);
        }
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnmatch_like_python() {
        assert!(fnmatch("map/s/s17/s17_1.c", "map/s/s17/s17_?.c"));
        assert!(fnmatch("dungeon/monster/e33a.c", "dungeon/*/e33a.c"));
        assert!(fnmatch("a/b.c", "*"));
        assert!(!fnmatch("map/s/s17/s17_10.c", "map/s/s17/s17_?.c"));
        assert!(fnmatch("x1", "x[0-9]"));
        assert!(!fnmatch("x1", "x[!0-9]"));
    }

    #[test]
    fn in_map_like_python() {
        assert!(in_map("map/s/s17/s17_1.c", "s17"));
        assert!(in_map("map/s17b_1.c", "s17"));
        assert!(!in_map("map/s/s170/s170_1.c", "s17"));
    }
}
