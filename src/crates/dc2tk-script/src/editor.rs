//! Editor support for a mod's patches. A patch calls the functions and uses the globals of the
//! scripts it targets, which it never declares (build merges it into them), so clangd would mark
//! them unknown. For each patch this writes a header of its target's declarations into FOLDER in
//! the mod (.editor/s51.h for s51.c): the target's own #include <...> lines, its globals as extern,
//! a prototype with the doc comment of every function it defines or gets from its libraries, and
//! its PROGRAM table as a comment. COMMANDS, the mod's compile_commands.json, gives each of the
//! mod's sources the flags of the tree's compile_flags.txt, -include of the SDK's c2s.h (for a
//! header of NEW_FLAG lines) and the mods folder as an include folder, where
//! `#include "base/names.h"` finds a mod it requires, and each patch -include of its header. Only editors read these files: the patch's text is unchanged, and the folder, hidden, is
//! not one of the mod's sources.

use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use serde_json::Value;

use crate::compile::Toolchain;
use crate::node::Node;
use crate::patches::{self, Scripts};
use crate::source::{self, py_str, truthy};
use crate::{definitions, library, mods, parse, sdk, Error, Result};

pub const FOLDER: &str = ".editor";
pub const COMMANDS: &str = "compile_commands.json";

/// A value kind (message, flag) where the source gives one, else the type.
fn kind_of(declared: &Node) -> String {
    match declared.get("kind") {
        Some(value) if truthy(value) && value.as_str().is_some() => py_str(value),
        _ => py_str(&declared["type"]),
    }
}

/// A tree script's program with its libraries included, as build sees it before patches.
pub fn script_program(tree: &Path, script: &str, tools: &Toolchain) -> Result<Node> {
    let path = tree.join(script);
    let program = parse::parse(&dc2tk_disc::textfile::read_text(&path)?)?;
    let mut headers = tools.headers.clone();
    headers.extend(definitions::source_headers(&path)?);
    let includes = library::read_includes(&program, &path, None, tools.sdk_dir, &[])?;
    library::expand(program, &includes, &headers)
}

/// The script's own #include <...> lines, in order.
fn system_includes(text: &str) -> Vec<String> {
    text.split('\n')
        .map(str::trim_end)
        .filter(|line| line.starts_with("#include") && line["#include".len()..].trim_start().starts_with('<'))
        .map(str::to_string)
        .collect()
}

/// What the header declares for one script: lines of globals, functions and programs, each kept
/// once by name across the scripts a patch targets.
#[derive(Default)]
struct Declared {
    includes: Vec<String>,
    globals: IndexMap<String, String>,
    functions: IndexMap<String, Vec<String>>,
    programs: Vec<String>,
}

impl Declared {
    fn add(&mut self, program: &Node, text: &str) {
        for line in system_includes(text) {
            if !self.includes.contains(&line) {
                self.includes.push(line);
            }
        }
        for declared in program.list("globals") {
            let name = py_str(&declared["name"]);
            let size =
                if declared["size"].is_null() { String::new() } else { format!("[{}]", py_str(&declared["size"])) };
            self.globals.entry(name.clone()).or_insert_with(|| format!("extern {} {name}{size};", kind_of(declared)));
        }
        for function in program.list("functions") {
            let name = py_str(&function["name"]);
            if self.functions.contains_key(&name) {
                continue;
            }
            let mut lines = Vec::new();
            if let Some(doc) = function.get("doc").filter(|doc| truthy(doc)) {
                lines.extend(source::doc_lines(&py_str(doc)));
            }
            match function.kind() {
                "function" => {
                    let params: Vec<String> = function
                        .list("params")
                        .iter()
                        .map(|param| {
                            let array = if param.flag("array") { "[]" } else { "" };
                            format!("{} {}{array}", kind_of(param), py_str(&param["name"]))
                        })
                        .collect();
                    let params = if params.is_empty() { "void".to_string() } else { params.join(", ") };
                    let returns = function.get("returns").map_or_else(|| "int".to_string(), py_str);
                    lines.push(format!("{returns} {name}({params});"));
                }
                "asm" => {
                    // kept as assembly: its arguments' types are not known
                    let count = function["arg"].as_i64().unwrap_or(0);
                    let params: Vec<String> = (1..=count).map(|number| format!("any arg{number}")).collect();
                    let params = if params.is_empty() { "void".to_string() } else { params.join(", ") };
                    lines.push(format!("int {name}({params});"));
                }
                _ => continue,
            }
            self.functions.insert(name, lines);
        }
        for entry in program.list("programs") {
            let line = format!("//   PROGRAM({}, {});", py_str(&entry["no"]), py_str(&entry["function"]));
            if !self.programs.contains(&line) {
                self.programs.push(line);
            }
        }
    }
}

/// The header of one patch: the declarations of the scripts its #pragma target lines pick, each
/// read from one script per line (the English one of a map's languages).
pub fn header(patch_text: &str, patch_name: &str, tree: &Path, scripts: &Scripts, tools: &Toolchain) -> Result<String> {
    let mut declared = Declared::default();
    let mut read = Vec::new();
    for target in patches::targets_in(patch_text)? {
        let picked: Vec<String> = scripts.picked(&target)?.into_iter().collect();
        let Some(model) = picked
            .iter()
            .find(|path| scripts.plain[*path].ends_with(&format!("_1{}", source::EXTENSION)))
            .or(picked.first())
        else {
            continue;
        };
        let text = dc2tk_disc::textfile::read_text(&tree.join(model))?;
        declared.add(&script_program(tree, model, tools)?, &text);
        read.push(format!("{model} (#pragma target {} {})", target.kind, target.values.join(" ")));
    }
    let mut lines = vec![
        format!("// The declarations of what {patch_name} patches, written by dc2tk mod header for editors: they"),
        "// show the target's functions and globals in the patch. build does not read this file, and a".into(),
        "// new decompile can change it: write it again with dc2tk mod header. Read from:".into(),
    ];
    lines.extend(read.iter().map(|line| format!("//   {line}")));
    lines.push("#pragma once".into());
    lines.push(String::new());
    lines.extend(declared.includes);
    if !declared.programs.is_empty() {
        lines.push(String::new());
        lines.push("// Its programs:".into());
        lines.extend(declared.programs);
    }
    if !declared.globals.is_empty() {
        lines.push(String::new());
        lines.extend(declared.globals.into_values());
    }
    for function in declared.functions.into_values() {
        lines.push(String::new());
        lines.extend(function);
    }
    Ok(lines.join("\n") + "\n")
}

fn posix(path: &Path) -> String {
    definitions::resolved_posix(path)
}

/// The header of each patch of the mod, under FOLDER, and the mod's COMMANDS. Returns the
/// headers written.
pub fn write(
    mod_dir: &Path,
    tree: &Path,
    tools: &Toolchain,
    sdk_dir: &Path,
    definition_sets: &[PathBuf],
) -> Result<Vec<PathBuf>> {
    let mut flags: Vec<String> = source::editor_flags(sdk_dir, definition_sets, Some(tree))
        .split('\n')
        .filter(|flag| !flag.is_empty())
        .map(str::to_string)
        .collect();
    if let Some(mods_dir) = mod_dir.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        flags.push(format!("-I{}", posix(mods_dir)));
    }
    // c2s's extensions (NEW_FLAG and the like) in headers and libraries that include nothing
    let prelude = sdk_dir.join(sdk::PRELUDE);
    if prelude.is_file() {
        flags.extend(["-include".to_string(), posix(&prelude)]);
    }
    let scripts = Scripts::new(tree)?;
    let mut written = Vec::new();
    let mut commands = Vec::new();
    for (path, text) in mods::sources(mod_dir)? {
        let mut arguments: Vec<String> = vec!["clang".into()];
        arguments.extend(flags.iter().cloned());
        if patches::is_patch(&text) {
            let relative = path.strip_prefix(mod_dir).map_err(|_| Error::invalid("a patch outside its mod"))?;
            let header_path = mod_dir.join(FOLDER).join(relative).with_extension("h");
            let name = relative.to_string_lossy().replace('\\', "/");
            std::fs::create_dir_all(header_path.parent().unwrap())?;
            dc2tk_disc::textfile::write_text(&header_path, &header(&text, &name, tree, &scripts, tools)?)?;
            arguments.extend(["-include".to_string(), posix(&header_path)]);
            written.push(header_path);
        }
        arguments.push(posix(&path));
        commands.push(serde_json::json!({"directory": posix(mod_dir), "file": posix(&path), "arguments": arguments}));
    }
    let text = dc2tk_disc::textfile::json_text(&Value::Array(commands));
    dc2tk_disc::textfile::write_text(&mod_dir.join(COMMANDS), &text)?;
    // the compile_flags.txt mod override wrote before: clang's tools other than clangd would take it
    // over compile_commands.json
    let flags = mod_dir.join(source::EDITOR_FLAGS);
    if flags.is_file() && dc2tk_disc::textfile::read_text(&flags)?.starts_with("-xc\n") {
        std::fs::remove_file(&flags)?;
    }
    Ok(written)
}
