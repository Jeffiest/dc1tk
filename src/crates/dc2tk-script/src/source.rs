//! Writing a program tree as the C-like source the decompiler emits: statements with the fewest
//! parentheses that keep the meaning, docs and notes wrapped as comments, and the pragmas that
//! record what the compiler needs to give back the original bytes.

use std::path::Path;

use crate::asm;
use crate::node::{Map, Node};
use crate::{Error, Result};

/// Precedences of what is not a binary operator (parse::precedence has those): an assignment's
/// is below them all, a unary operator's and then a primary expression's above.
const ASSIGN: i32 = 1;
const UNARY: i32 = 12;
const PRIMARY: i32 = 13;
pub const INDENT: &str = "    ";
/// Extension of decompiled source files, so editors read them as C.
pub const EXTENSION: &str = ".c";
/// Written at the root of a decompiled tree. clangd (and editors built on it) reads it for every
/// file below, so scripts get the SDK's declarations, helpers and constants.
pub const EDITOR_FLAGS: &str = "compile_flags.txt";

fn precedence(op: &str) -> Option<i32> {
    crate::parse::precedence(op).map(|level| level as i32)
}

/// node[first] as text when it is set, else node[second].
fn first_or(node: &Node, first: &str, second: &str) -> String {
    match node.get(first) {
        Some(value) if value.is_set() => value.plain_text(),
        _ => node[second].plain_text(),
    }
}

fn int(node: &Node) -> Result<i64> {
    node.as_i64().ok_or_else(|| Error::invalid(format!("{node} is not a number")))
}

fn text_of<'a>(node: &'a Node, key: &str) -> Result<&'a str> {
    node[key].as_str().ok_or_else(|| Error::invalid(format!("{key} is not text in {}", node.kind())))
}

/// The flags that let clang read a decompiled tree: the SDK and each definition set's include
/// folder, the tree's own include folder (disc.h) and its root (for "lib/..." includes). Functions
/// fall off their end into the compiler's implicit return 0, so that warning is off. Some retail
/// calls pass a string or pointer where the native reads an int; clang 15 and later make that an
/// error by default, which here would hide every later diagnostic, so it stays a warning. For a
/// mod's files, tree is the decompiled tree it sits on: its own libraries come first, then the
/// tree's.
pub fn editor_flags(sdk_dir: &Path, definition_sets: &[std::path::PathBuf], tree: Option<&Path>) -> String {
    let mut folders = vec![sdk_dir.to_path_buf()];
    folders.extend(definition_sets.iter().map(|directory| directory.join("include")));
    let mut flags: Vec<String> =
        ["-xc", "-std=c2x", "-m32", "-Wno-unknown-pragmas", "-Wno-return-type", "-Wno-error=int-conversion"]
            .iter()
            .map(|flag| flag.to_string())
            .collect();
    for folder in folders.iter().filter(|folder| folder.is_dir()) {
        flags.push(format!("-I{}", crate::definitions::resolved_posix(folder)));
    }
    match tree {
        None => flags.extend(["-Iinclude".to_string(), "-I.".to_string()]),
        Some(tree) => {
            let base = crate::definitions::resolved_posix(tree);
            flags.extend([format!("-I{base}/include"), "-I.".to_string(), format!("-I{base}")]);
        }
    }
    flags.join("\n") + "\n"
}

pub fn float_literal(bits: i64) -> Result<String> {
    asm::float_text(bits as i32).ok_or_else(|| Error::invalid("float constant without a decimal form"))
}

fn wrap(text: String, precedence: i32, needed: i32) -> String {
    if precedence < needed {
        format!("({text})")
    } else {
        text
    }
}

fn arguments(nodes: &[Node]) -> Result<String> {
    Ok(nodes.iter().map(|node| expression(node).map(|(text, _)| text)).collect::<Result<Vec<_>>>()?.join(", "))
}

/// An expression's text and its precedence.
pub fn expression(node: &Node) -> Result<(String, i32)> {
    let kind = node.kind();
    Ok(match kind {
        "int" => (node["value"].plain_text(), PRIMARY),
        "float" => (float_literal(int(&node["bits"])?)?, PRIMARY),
        "string" => (asm::quote(text_of(node, "text")?), PRIMARY),
        "var" => (node["name"].plain_text(), PRIMARY),
        "index" => (format!("{}[{}]", node["name"].plain_text(), expression(&node["index"])?.0), PRIMARY),
        "addr" => {
            if node["target"].kind() == "deref" {
                return expression(&node["target"]["operand"]);
            }
            (format!("&{}", expression(&node["target"])?.0), UNARY)
        }
        "deref" => {
            let (operand, found) = expression(&node["operand"])?;
            (format!("*{}", wrap(operand, found, UNARY)), UNARY)
        }
        "assign" => {
            let target = expression(&node["target"])?.0;
            let (value, found) = expression(&node["value"])?;
            (format!("{} = {}", target, wrap(value, found, ASSIGN)), ASSIGN)
        }
        "binary" | "logical" => {
            let op = text_of(node, "op")?;
            let needed = precedence(op).ok_or_else(|| Error::invalid(format!("unknown operator {op}")))?;
            let (left, left_found) = expression(&node["left"])?;
            let (right, right_found) = expression(&node["right"])?;
            (format!("{} {} {}", wrap(left, left_found, needed), op, wrap(right, right_found, needed + 1)), needed)
        }
        "unary" => {
            let (operand, mut found) = expression(&node["operand"])?;
            if operand.starts_with(['-', '!']) {
                found = 0;
            }
            (format!("{}{}", node["op"].plain_text(), wrap(operand, found, UNARY)), UNARY)
        }
        "cast" => {
            let (operand, found) = expression(&node["operand"])?;
            (format!("({}){}", node["type"].plain_text(), wrap(operand, found, UNARY)), UNARY)
        }
        "builtin" => (format!("{}({})", node["name"].plain_text(), expression(&node["operand"])?.0), PRIMARY),
        "call" => (format!("{}({})", node["name"].plain_text(), arguments(node.list("args"))?), PRIMARY),
        _ => return Err(Error::invalid(format!("unknown expression {kind}"))),
    })
}

pub fn statement(node: &Node, depth: usize) -> Result<Vec<String>> {
    let pad = INDENT.repeat(depth);
    let mut lines = bare_statement(node, depth)?;
    let note = node.get("note").filter(|note| note.is_set());
    if let Some(note) = note {
        let width = 100 - pad.len() - "// ".len();
        let rows = wrap_words(&format!("note: {}", note.plain_text()), width);
        let mut noted: Vec<String> = rows.into_iter().map(|row| format!("{pad}// {row}")).collect();
        noted.extend(lines);
        lines = noted;
    }
    if let Some(comment) = node.get("comment").filter(|comment| comment.is_set()) {
        if lines.len() == 1 {
            lines = vec![format!("{}  // {}", lines[0], comment.plain_text())];
        } else {
            // a blank comment line keeps a note its own paragraph, which is how the parser finds it
            let mut commented = vec![format!("{pad}// {}", comment.plain_text())];
            if note.is_some() {
                commented.push(format!("{pad}//"));
            }
            commented.extend(lines);
            lines = commented;
        }
    }
    Ok(lines)
}

fn bare_statement(node: &Node, depth: usize) -> Result<Vec<String>> {
    let pad = INDENT.repeat(depth);
    let kind = node.kind();
    Ok(match kind {
        "expr" => vec![format!("{pad}{};", expression(&node["expr"])?.0)],
        "return" => {
            if node["value"].is_null() {
                vec![format!("{pad}return;")]
            } else {
                vec![format!("{pad}return {};", expression(&node["value"])?.0)]
            }
        }
        "native" => vec![format!("{pad}{}", native_text(node)?)],
        "print" => vec![format!("{pad}print({});", arguments(node.list("args"))?)],
        "wait" | "end" | "skip_end" | "break" | "continue" => vec![format!("{pad}{kind};")],
        "if" => conditional(node, depth, &format!("{pad}if"))?,
        "while" => {
            let mut lines = vec![format!("{pad}while ({}) {{", expression(&node["cond"])?.0)];
            lines.extend(block(node.list("body"), depth + 1)?);
            lines.push(format!("{pad}}}"));
            lines
        }
        _ => return Err(Error::invalid(format!("unknown statement {kind}"))),
    })
}

pub fn native_text(node: &Node) -> Result<String> {
    let base = &node["base"];
    let prefix = match base {
        Node::Null => String::new(),
        _ if base == "direct" => "DIRECT ".to_string(),
        _ => format!("AT({}) ", int(base)?),
    };
    let mut call = format!("{}({});", node["name"].plain_text(), arguments(node.list("args"))?);
    if let Some(result) = node.get("result").filter(|result| !result.is_null()) {
        call = format!("{} = {}", expression(result)?.0, call);
    }
    Ok(prefix + &call)
}

fn conditional(node: &Node, depth: usize, opening: &str) -> Result<Vec<String>> {
    let pad = INDENT.repeat(depth);
    let mut lines = vec![format!("{} ({}) {{", opening, expression(&node["cond"])?.0)];
    lines.extend(block(node.list("then"), depth + 1)?);
    let otherwise = &node["else"];
    let Some(otherwise) = otherwise.as_array() else {
        lines.push(format!("{pad}}}"));
        return Ok(lines);
    };
    if otherwise.len() == 1 && otherwise[0].kind() == "if" {
        lines.extend(conditional(&otherwise[0], depth, &format!("{pad}}} else if"))?);
        return Ok(lines);
    }
    lines.push(format!("{pad}}} else {{"));
    lines.extend(block(otherwise, depth + 1)?);
    lines.push(format!("{pad}}}"));
    Ok(lines)
}

pub fn block(statements: &[Node], depth: usize) -> Result<Vec<String>> {
    let mut lines = Vec::new();
    for node in statements {
        lines.extend(statement(node, depth)?);
    }
    Ok(lines)
}

fn parameter(param: &Node) -> String {
    if param.flag("array") && param["type"] == "void" {
        return format!("void *{}", param["name"].plain_text());
    }
    let array = if param.flag("array") { "[]" } else { "" };
    format!("{} {}{}", first_or(param, "kind", "type"), param["name"].plain_text(), array)
}

fn parameter_list(texts: Vec<String>) -> String {
    if texts.is_empty() {
        "void".into()
    } else {
        texts.join(", ")
    }
}

/// Text in lines of at most width characters, broken at whitespace, which a break takes the place
/// of; the words of a line are one space apart. A word longer than a line has a line of its own,
/// so no word is broken.
fn wrap_words(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    let mut length = 0;
    for word in text.split_whitespace() {
        let size = word.chars().count();
        if length > 0 && length + 1 + size > width {
            lines.push(std::mem::take(&mut line));
            length = 0;
        }
        if length > 0 {
            line.push(' ');
            length += 1;
        }
        line.push_str(word);
        length += size;
    }
    if length > 0 {
        lines.push(line);
    }
    lines
}

pub fn doc_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        if !lines.is_empty() {
            lines.push("//".to_string());
        }
        // Breaking only at spaces lets the parser join the lines back with spaces (mid-motion,
        // not mid- motion).
        lines.extend(wrap_words(paragraph, 96).into_iter().map(|row| format!("// {row}")));
    }
    lines
}

pub fn function_text(function: &Node) -> Result<Vec<String>> {
    let mut lines = Vec::new();
    let unused = function.list("unused_strings");
    if !unused.is_empty() {
        let quoted: Vec<String> = unused.iter().map(|text| asm::quote(&text.plain_text())).collect();
        lines.push(format!("#pragma strings {}", quoted.join(" ")));
    }
    if let Some(doc) = function.get("doc").filter(|doc| doc.is_set()) {
        lines.extend(doc_lines(&doc.plain_text()));
    }
    let params = parameter_list(function.list("params").iter().map(parameter).collect());
    let returns = function.get("returns").map_or_else(|| "int".to_string(), Node::plain_text);
    lines.push(format!("{} {}({})", returns, function["name"].plain_text(), params));
    lines.push("{".into());
    let locals = function.list("locals");
    for declared in locals {
        let size = if declared["size"].is_null() { String::new() } else { format!("[{}]", int(&declared["size"])?) };
        lines.push(format!("{INDENT}{} {}{size};", first_or(declared, "kind", "type"), declared["name"].plain_text()));
    }
    let body = function.list("body");
    if !locals.is_empty() && !body.is_empty() {
        lines.push(String::new());
    }
    lines.extend(block(body, 1)?);
    lines.push("}".into());
    Ok(lines)
}

fn asm_text(function: &Node) -> Vec<String> {
    let mut lines = vec![
        format!("// {}", function["reason"].plain_text()),
        format!("asm {} arg {} {{", function["name"].plain_text(), function["arg"].plain_text()),
    ];
    lines.extend(function.list("lines").iter().map(Node::plain_text));
    lines.push("}".into());
    lines
}

/// The magic as the assembly writes it, from the Latin-1 text the tree holds.
fn magic_text(magic: &str) -> String {
    let bytes: Vec<u8> = magic.chars().map(|c| c as u32 as u8).collect();
    match <[u8; 4]>::try_from(bytes.as_slice()) {
        Ok(four) => asm::magic_text(&four),
        Err(_) => format!("0x{}", asm::hex(&bytes)),
    }
}

/// A program as source text.
pub fn text(program: &Node) -> Result<String> {
    if let Some(assembly) = program.get("asm") {
        return Ok(format!("#pragma assembly\n{}", assembly.plain_text()));
    }
    let functions = program.list("functions");
    let mut leading = 0;
    while leading < functions.len() && functions[leading].kind() == "include" && functions[leading]["system"].is_set() {
        leading += 1;
    }
    let mut lines = Vec::new();
    if let Some(doc) = program.get("doc").filter(|doc| doc.is_set()) {
        lines.extend(doc_lines(&doc.plain_text()));
        lines.push(String::new());
    }
    lines.extend(functions[..leading].iter().map(include_line));
    if !program["natives"].is_null() {
        lines.push(format!("#pragma natives {}", program["natives"].plain_text()));
    }
    let magic = magic_text(text_of(program, "magic")?);
    if magic != "SB2" {
        lines.push(format!("#pragma magic {magic}"));
    }
    if int(&program["unk_14"])? != 0 {
        lines.push(format!("#pragma unk_14 {}", int(&program["unk_14"])?));
    }
    let header_padding = text_of(program, "header_padding")?;
    if header_padding != "00".repeat(crate::stb::HEADER_SIZE - crate::stb::HEADER_FIELDS_SIZE) {
        lines.push(format!("#pragma header_padding {header_padding}"));
    }
    let globals = program.list("globals");
    if int(&program["global_num"])? != globals.len() as i64 {
        lines.push(format!("#pragma globals {}", int(&program["global_num"])?));
    }
    if program.get("string_pool").is_some_and(Node::is_set) {
        // built by a compiler that never reset its string buffer between functions (stb::pool_block)
        lines.push("#pragma string_pool".into());
    }
    lines.push(String::new());
    for declared in globals {
        lines.push(format!("{} {};", declared["type"].plain_text(), declared["name"].plain_text()));
    }
    if !globals.is_empty() {
        lines.push(String::new());
    }
    for entry in program.list("programs") {
        let line = format!("PROGRAM({}, {});", entry["no"].plain_text(), entry["function"].plain_text());
        match entry.get("note").filter(|note| note.is_set()) {
            Some(note) => lines.push(format!("{line}  // {}", note.plain_text())),
            None => lines.push(line),
        }
    }
    lines.push(format!("MAIN({});", program["main"].plain_text()));
    lines.extend(function_lines(&functions[leading..])?);
    let mut padding_of: Map = program["padding_of"].as_object().cloned().unwrap_or_default();
    for function in functions {
        if function.kind() == "function" && !function["padding"].is_null() {
            padding_of.insert(function["name"].plain_text(), function["padding"].clone());
        }
    }
    if !padding_of.is_empty() {
        lines.push(String::new());
        for (name, padding) in padding_of.iter() {
            let line = format!("#pragma padding_of {} {}", name, padding.plain_text());
            lines.push(line.trim_end().to_string());
        }
    }
    Ok(lines.join("\n") + "\n")
}

pub fn function_lines(functions: &[Node]) -> Result<Vec<String>> {
    let mut lines = Vec::new();
    let mut previous = "";
    for function in functions {
        if function.kind() == "include" {
            if previous != "include" {
                lines.push(String::new());
            }
            lines.push(include_line(function));
        } else {
            lines.push(String::new());
            if function.kind() == "asm" {
                lines.extend(asm_text(function));
            } else {
                lines.extend(function_text(function)?);
            }
        }
        previous = function.kind();
    }
    Ok(lines)
}

pub fn include_line(item: &Node) -> String {
    if item["system"].is_set() {
        format!("#include <{}>", item["path"].plain_text())
    } else {
        format!("#include \"{}\"", item["path"].plain_text())
    }
}

pub fn library_text(table: &str, functions: &[Node], header: Option<&str>, doc: &str) -> Result<String> {
    let first = match header.filter(|header| !header.is_empty()) {
        Some(header) => format!("#include <{header}>"),
        None => format!("#pragma natives {table}"),
    };
    let mut lines = Vec::new();
    if !doc.is_empty() {
        lines.extend(doc_lines(doc));
        lines.push(String::new());
    }
    lines.push(first);
    lines.extend(function_lines(functions)?);
    Ok(lines.join("\n") + "\n")
}

pub fn typed_parameter(param: &Node) -> String {
    let pointer = if param["out"].is_set() { "*" } else { "" };
    let kind = match param.get("enum").filter(|value| value.is_set()) {
        Some(name) => format!("enum {}", name.plain_text()),
        None => first_or(param, "kind", "type"),
    };
    // a pointer to any is void * in C, which takes int * and float * alike
    let kind = if kind == "any" && !pointer.is_empty() { "void".to_string() } else { kind };
    format!("{} {}{}", kind, pointer, param["name"].plain_text())
}

pub fn inline_text(inline: &Node) -> Result<Vec<String>> {
    let params = parameter_list(inline.list("params").iter().map(typed_parameter).collect());
    let mut lines = vec![format!("static inline void {}({})", inline["name"].plain_text(), params), "{".to_string()];
    lines.extend(block(inline.list("body"), 1)?);
    lines.push("}".into());
    Ok(lines)
}

pub fn native_declaration(declared: &Node) -> Result<String> {
    let params: Vec<&Node> = declared.list("params").iter().filter(|param| !param["result"].is_set()).collect();
    let mut texts: Vec<String> =
        params.iter().filter(|param| !param["optional"].is_set()).map(|param| typed_parameter(param)).collect();
    let mut optional: Vec<String> =
        params.iter().filter(|param| param["optional"].is_set()).map(|param| typed_parameter(param)).collect();
    let variadic = declared["variadic"].is_set();
    if !optional.is_empty() {
        if variadic {
            optional.push("...".into());
        }
        texts.push(format!("OPTIONAL({})", optional.join(", ")));
    } else if variadic {
        texts.push("...".into());
    }
    let number = if declared["base"].is_null() {
        declared["no"].plain_text()
    } else {
        let base = int(&declared["base"])?;
        format!("{}, {}", base, int(&declared["no"])? - base)
    };
    Ok(format!(
        "extern {} {}({}) NATIVE({});",
        declared["returns"].plain_text(),
        declared["name"].plain_text(),
        parameter_list(texts),
        number
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operators_are_parenthesised_by_the_parsers_precedence() {
        let text = "void f() { x = (a ^ b) & c << 2; y = a - (b - c); z = (a | b) == c; }";
        let program = crate::parse::parse(text).unwrap();
        let written: Vec<String> =
            program["functions"][0].list("body").iter().map(|line| expression(&line["expr"]).unwrap().0).collect();
        assert_eq!(written, ["x = (a ^ b) & c << 2", "y = a - (b - c)", "z = (a | b) == c"]);
    }

    #[test]
    fn text_wraps_at_whitespace_and_never_inside_a_word() {
        assert_eq!(wrap_words("note: a b c", 7), ["note: a", "b c"]);
        assert_eq!(wrap_words("  lead and\tdouble  spaces ", 10), ["lead and", "double", "spaces"]);
        assert_eq!(wrap_words("abcdefghij mid-motion", 4), ["abcdefghij", "mid-motion"]);
        assert!(wrap_words(" \t ", 10).is_empty());
    }
}
