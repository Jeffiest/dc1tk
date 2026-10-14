//! Writing a program tree as the C-like source the decompiler emits: statements with the fewest
//! parentheses that keep the meaning, docs and notes wrapped as comments, and the pragmas that
//! record what the compiler needs to give back the original bytes.

use std::path::Path;

use crate::asm;
use crate::node::{Map, Node};
use crate::{Error, Result};

const ASSIGN: i32 = 1;
const UNARY: i32 = 10;
const PRIMARY: i32 = 11;
pub const INDENT: &str = "    ";
/// Extension of decompiled source files, so editors read them as C.
pub const EXTENSION: &str = ".c";
/// Written at the root of a decompiled tree. clangd (and editors built on it) reads it for every
/// file below, so scripts get the SDK's declarations, helpers and constants.
pub const EDITOR_FLAGS: &str = "compile_flags.txt";

fn precedence(op: &str) -> Option<i32> {
    Some(match op {
        "||" => 2,
        "&&" => 3,
        "|" => 4,
        "&" => 5,
        "==" | "!=" => 6,
        "<" | "<=" | ">" | ">=" => 7,
        "+" | "-" => 8,
        "*" | "/" | "%" => 9,
        _ => return None,
    })
}

/// A value as Python's str() or "%s" writes it.
pub fn py_str(node: &Node) -> String {
    match node {
        Node::Null => "None".into(),
        Node::Bool(value) => if *value { "True" } else { "False" }.into(),
        Node::Int(value) => value.to_string(),
        Node::Float(value) => c2_py::float_repr(*value),
        Node::Str(text) => text.clone(),
        other => other.to_string(),
    }
}

/// Python's truthiness of a value.
pub fn truthy(node: &Node) -> bool {
    match node {
        Node::Null => false,
        Node::Bool(value) => *value,
        Node::Int(value) => *value != 0,
        Node::Float(value) => *value != 0.0,
        Node::Str(text) => !text.is_empty(),
        Node::List(items) => !items.is_empty(),
        Node::Map(map) => !map.is_empty(),
    }
}

/// node.get(first) or node[second], as text.
fn first_or(node: &Node, first: &str, second: &str) -> String {
    match node.get(first) {
        Some(value) if truthy(value) => py_str(value),
        _ => py_str(&node[second]),
    }
}

fn int(node: &Node) -> Result<i64> {
    node.as_i64().ok_or_else(|| Error::invalid(format!("{node} is not a number")))
}

fn text_of<'a>(node: &'a Node, key: &str) -> Result<&'a str> {
    node[key].as_str().ok_or_else(|| Error::invalid(format!("{key} is not text in {}", node.kind())))
}

fn list<'a>(node: &'a Node, key: &str) -> &'a [Node] {
    node[key].as_array().map(Vec::as_slice).unwrap_or(&[])
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
        "int" => (py_str(&node["value"]), PRIMARY),
        "float" => (float_literal(int(&node["bits"])?)?, PRIMARY),
        "string" => (asm::quote(text_of(node, "text")?), PRIMARY),
        "var" => (py_str(&node["name"]), PRIMARY),
        "index" => (format!("{}[{}]", py_str(&node["name"]), expression(&node["index"])?.0), PRIMARY),
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
            (format!("{}{}", py_str(&node["op"]), wrap(operand, found, UNARY)), UNARY)
        }
        "cast" => {
            let (operand, found) = expression(&node["operand"])?;
            (format!("({}){}", py_str(&node["type"]), wrap(operand, found, UNARY)), UNARY)
        }
        "builtin" => (format!("{}({})", py_str(&node["name"]), expression(&node["operand"])?.0), PRIMARY),
        "call" => (format!("{}({})", py_str(&node["name"]), arguments(list(node, "args"))?), PRIMARY),
        _ => return Err(Error::invalid(format!("unknown expression {kind}"))),
    })
}

pub fn statement(node: &Node, depth: usize) -> Result<Vec<String>> {
    let pad = INDENT.repeat(depth);
    let mut lines = bare_statement(node, depth)?;
    let note = node.get("note").filter(|note| truthy(note));
    if let Some(note) = note {
        let width = 100 - pad.len() - "// ".len();
        let rows = c2_py::wrap(&format!("note: {}", py_str(note)), width);
        let mut noted: Vec<String> = rows.into_iter().map(|row| format!("{pad}// {row}")).collect();
        noted.extend(lines);
        lines = noted;
    }
    if let Some(comment) = node.get("comment").filter(|comment| truthy(comment)) {
        if lines.len() == 1 {
            lines = vec![format!("{}  // {}", lines[0], py_str(comment))];
        } else {
            // a blank comment line keeps a note its own paragraph, which is how the parser finds it
            let mut commented = vec![format!("{pad}// {}", py_str(comment))];
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
        "print" => vec![format!("{pad}print({});", arguments(list(node, "args"))?)],
        "wait" | "end" | "skip_end" | "break" | "continue" => vec![format!("{pad}{kind};")],
        "if" => conditional(node, depth, &format!("{pad}if"))?,
        "while" => {
            let mut lines = vec![format!("{pad}while ({}) {{", expression(&node["cond"])?.0)];
            lines.extend(block(list(node, "body"), depth + 1)?);
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
    let mut call = format!("{}({});", py_str(&node["name"]), arguments(list(node, "args"))?);
    if let Some(result) = node.get("result").filter(|result| !result.is_null()) {
        call = format!("{} = {}", expression(result)?.0, call);
    }
    Ok(prefix + &call)
}

fn conditional(node: &Node, depth: usize, opening: &str) -> Result<Vec<String>> {
    let pad = INDENT.repeat(depth);
    let mut lines = vec![format!("{} ({}) {{", opening, expression(&node["cond"])?.0)];
    lines.extend(block(list(node, "then"), depth + 1)?);
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
    if truthy(&param["array"]) && param["type"] == "void" {
        return format!("void *{}", py_str(&param["name"]));
    }
    let array = if truthy(&param["array"]) { "[]" } else { "" };
    format!("{} {}{}", first_or(param, "kind", "type"), py_str(&param["name"]), array)
}

fn parameter_list(texts: Vec<String>) -> String {
    if texts.is_empty() {
        "void".into()
    } else {
        texts.join(", ")
    }
}

pub fn doc_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        if !lines.is_empty() {
            lines.push("//".to_string());
        }
        // Breaking only at spaces lets the parser join the lines back with spaces (mid-motion,
        // not mid- motion).
        lines.extend(c2_py::wrap(paragraph, 96).into_iter().map(|row| format!("// {row}")));
    }
    lines
}

pub fn function_text(function: &Node) -> Result<Vec<String>> {
    let mut lines = Vec::new();
    let unused = list(function, "unused_strings");
    if !unused.is_empty() {
        let quoted: Vec<String> = unused.iter().map(|text| asm::quote(&py_str(text))).collect();
        lines.push(format!("#pragma strings {}", quoted.join(" ")));
    }
    if let Some(doc) = function.get("doc").filter(|doc| truthy(doc)) {
        lines.extend(doc_lines(&py_str(doc)));
    }
    let params = parameter_list(list(function, "params").iter().map(parameter).collect());
    let returns = function.get("returns").map_or_else(|| "int".to_string(), py_str);
    lines.push(format!("{} {}({})", returns, py_str(&function["name"]), params));
    lines.push("{".into());
    let locals = list(function, "locals");
    for declared in locals {
        let size = if declared["size"].is_null() { String::new() } else { format!("[{}]", int(&declared["size"])?) };
        lines.push(format!("{INDENT}{} {}{size};", first_or(declared, "kind", "type"), py_str(&declared["name"])));
    }
    let body = list(function, "body");
    if !locals.is_empty() && !body.is_empty() {
        lines.push(String::new());
    }
    lines.extend(block(body, 1)?);
    lines.push("}".into());
    Ok(lines)
}

fn asm_text(function: &Node) -> Vec<String> {
    let mut lines = vec![
        format!("// {}", py_str(&function["reason"])),
        format!("asm {} arg {} {{", py_str(&function["name"]), py_str(&function["arg"])),
    ];
    lines.extend(list(function, "lines").iter().map(py_str));
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
        return Ok(format!("#pragma assembly\n{}", py_str(assembly)));
    }
    let functions = list(program, "functions");
    let mut leading = 0;
    while leading < functions.len() && functions[leading].kind() == "include" && truthy(&functions[leading]["system"]) {
        leading += 1;
    }
    let mut lines = Vec::new();
    if let Some(doc) = program.get("doc").filter(|doc| truthy(doc)) {
        lines.extend(doc_lines(&py_str(doc)));
        lines.push(String::new());
    }
    lines.extend(functions[..leading].iter().map(include_line));
    if !program["natives"].is_null() {
        lines.push(format!("#pragma natives {}", py_str(&program["natives"])));
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
    let globals = list(program, "globals");
    if int(&program["global_num"])? != globals.len() as i64 {
        lines.push(format!("#pragma globals {}", int(&program["global_num"])?));
    }
    if program.get("string_pool").is_some_and(truthy) {
        // built by a compiler that never reset its string buffer between functions (stb::pool_block)
        lines.push("#pragma string_pool".into());
    }
    lines.push(String::new());
    for declared in globals {
        lines.push(format!("{} {};", py_str(&declared["type"]), py_str(&declared["name"])));
    }
    if !globals.is_empty() {
        lines.push(String::new());
    }
    for entry in list(program, "programs") {
        let line = format!("PROGRAM({}, {});", py_str(&entry["no"]), py_str(&entry["function"]));
        match entry.get("note").filter(|note| truthy(note)) {
            Some(note) => lines.push(format!("{line}  // {}", py_str(note))),
            None => lines.push(line),
        }
    }
    lines.push(format!("MAIN({});", py_str(&program["main"])));
    lines.extend(function_lines(&functions[leading..])?);
    let mut padding_of: Map = program["padding_of"].as_object().cloned().unwrap_or_default();
    for function in functions {
        if function.kind() == "function" && !function["padding"].is_null() {
            padding_of.insert(py_str(&function["name"]), function["padding"].clone());
        }
    }
    if !padding_of.is_empty() {
        lines.push(String::new());
        for (name, padding) in padding_of.iter() {
            let line = format!("#pragma padding_of {} {}", name, py_str(padding));
            lines.push(line.trim_end_matches(c2_py::is_space).to_string());
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
    if truthy(&item["system"]) {
        format!("#include <{}>", py_str(&item["path"]))
    } else {
        format!("#include \"{}\"", py_str(&item["path"]))
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
    let pointer = if truthy(&param["out"]) { "*" } else { "" };
    let kind = match param.get("enum").filter(|value| truthy(value)) {
        Some(name) => format!("enum {}", py_str(name)),
        None => first_or(param, "kind", "type"),
    };
    // a pointer to any is void * in C, which takes int * and float * alike
    let kind = if kind == "any" && !pointer.is_empty() { "void".to_string() } else { kind };
    format!("{} {}{}", kind, pointer, py_str(&param["name"]))
}

pub fn inline_text(inline: &Node) -> Result<Vec<String>> {
    let params = parameter_list(list(inline, "params").iter().map(typed_parameter).collect());
    let mut lines = vec![format!("static inline void {}({})", py_str(&inline["name"]), params), "{".to_string()];
    lines.extend(block(list(inline, "body"), 1)?);
    lines.push("}".into());
    Ok(lines)
}

pub fn native_declaration(declared: &Node) -> Result<String> {
    let params: Vec<&Node> = list(declared, "params").iter().filter(|param| !truthy(&param["result"])).collect();
    let mut texts: Vec<String> =
        params.iter().filter(|param| !truthy(&param["optional"])).map(|param| typed_parameter(param)).collect();
    let mut optional: Vec<String> =
        params.iter().filter(|param| truthy(&param["optional"])).map(|param| typed_parameter(param)).collect();
    let variadic = truthy(&declared["variadic"]);
    if !optional.is_empty() {
        if variadic {
            optional.push("...".into());
        }
        texts.push(format!("OPTIONAL({})", optional.join(", ")));
    } else if variadic {
        texts.push("...".into());
    }
    let number = if declared["base"].is_null() {
        py_str(&declared["no"])
    } else {
        let base = int(&declared["base"])?;
        format!("{}, {}", base, int(&declared["no"])? - base)
    };
    Ok(format!(
        "extern {} {}({}) NATIVE({});",
        py_str(&declared["returns"]),
        py_str(&declared["name"]),
        parameter_list(texts),
        number
    ))
}
