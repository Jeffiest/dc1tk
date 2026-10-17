//! The SDK's reference pages (docs/scripting/api/, written by `dc2tk api-docs`): every native,
//! helper, program, constant and value kind a table's header and its helpers declare, with their
//! doc comments, and the functions of the modkit libraries, as Markdown. The headers are read as
//! the text people write them in, line by line: a run of `//` lines documents the declaration
//! after it, and a line the reader does not know is an error, so no declaration goes unlisted.

use std::fmt::Write as _;
use std::path::Path;

use dc2tk_disc::files;

use super::{files_under, read_text};
use crate::{Error, Result};

/// The native tables in the order the index lists them, with what runs each one's scripts.
pub const TABLES: [(&str, &str); 5] = [
    ("event", "Event scripts"),
    ("monster", "Monster AI scripts"),
    ("action", "Action scripts"),
    ("effect", "Effect scripts"),
    ("arg", "Argument files"),
];

/// The pages, by file name: the index, README.md, then a page a table and modkit.md.
pub fn pages(sdk_dir: &Path) -> Result<Vec<(String, String)>> {
    let mut pages = Vec::new();
    let mut rows = Vec::new();
    for (table, title) in TABLES {
        let main = scan(&read_text(&sdk_dir.join(format!("{table}.h")))?, &format!("{table}.h"))?;
        let helpers_path = sdk_dir.join("helpers").join(format!("{table}.h"));
        let helpers = match helpers_path.is_file() {
            true => Some(scan(&read_text(&helpers_path)?, &format!("helpers/{table}.h"))?),
            false => None,
        };
        let page = TablePage::new(table, title, main, helpers);
        rows.push(page.row());
        pages.push((format!("{table}.md"), page.text()));
    }
    let mut libraries = Vec::new();
    for relative in files_under(&sdk_dir.join("modkit"), "c")? {
        let name = format!("modkit/{relative}");
        libraries.push((name.clone(), scan(&read_text(&sdk_dir.join("modkit").join(&relative))?, &name)?));
    }
    let functions = libraries.iter().map(|(_, library)| library.items.len()).sum::<usize>();
    pages.push(("modkit.md".to_string(), modkit_page(&libraries)));
    pages.insert(0, ("README.md".to_string(), index_page(&rows, functions)));
    Ok(pages)
}

/// Writes the pages into a folder, and returns their file names.
pub fn write_pages(sdk_dir: &Path, out: &Path) -> Result<Vec<String>> {
    files::create_dir_all(out)?;
    let mut written = Vec::new();
    for (name, text) in pages(sdk_dir)? {
        files::write(out.join(&name), text)?;
        written.push(name);
    }
    Ok(written)
}

/// A header's declarations, with the comment that opens the file.
#[derive(Default)]
struct Header {
    intro: Vec<String>,
    items: Vec<Item>,
}

/// A declaration and the comment lines before it (without their `// `).
struct Item {
    doc: Vec<String>,
    kind: ItemKind,
}

enum ItemKind {
    Native {
        name: String,
        number: String,
        text: String,
    },
    /// An inline helper, or a modkit library's function.
    Function {
        name: String,
        text: String,
    },
    Enum {
        name: Option<String>,
        members: Vec<Member>,
    },
    Define {
        name: String,
        value: String,
    },
    Kind {
        name: String,
        base: String,
    },
}

struct Member {
    doc: Vec<String>,
    name: String,
    value: String,
}

fn comment(line: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix("//")?;
    Some(rest.strip_prefix(' ').unwrap_or(rest).trim_end().to_string())
}

fn invalid(file: &str, line: usize, what: &str) -> Error {
    Error::invalid(format!("{file}:{line}: {what}"))
}

/// Lines from `start` up to and including the first that `last` accepts, joined with newlines,
/// and the index after it.
fn through(lines: &[&str], start: usize, file: &str, last: impl Fn(&str) -> bool) -> Result<(String, usize)> {
    let end = (start..lines.len())
        .find(|&at| last(lines[at]))
        .ok_or_else(|| invalid(file, start + 1, "the declaration never ends"))?;
    Ok((lines[start..=end].join("\n"), end + 1))
}

/// The name a declaration's text gives before its first `(`.
fn declared_name(text: &str) -> String {
    let before = text.split('(').next().unwrap_or("");
    before.split_whitespace().last().unwrap_or("").trim_start_matches('*').to_string()
}

fn scan(text: &str, file: &str) -> Result<Header> {
    let lines: Vec<&str> = text.lines().collect();
    let mut header = Header::default();
    let mut pending: Vec<String> = Vec::new();
    let mut at = 0;
    while at < lines.len() {
        let line = lines[at];
        if let Some(text) = comment(line).filter(|_| !line.starts_with(' ')) {
            pending.push(text);
            at += 1;
            continue;
        }
        let doc = std::mem::take(&mut pending);
        // the comment that opens the file, ended by a blank line or the include guard
        let opens = header.intro.is_empty() && header.items.is_empty();
        if !doc.is_empty() && opens && (line.trim().is_empty() || line.starts_with("#ifndef")) {
            header.intro = doc;
            at += 1;
            continue;
        }
        if line.trim().is_empty() {
            if !doc.is_empty() {
                return Err(invalid(file, at, "a comment documents nothing (a blank line follows it)"));
            }
            at += 1;
            continue;
        }
        // what a header gives C tools only (dc2tk.h defines DC2TK_C_TOOLS; dc2tk does not)
        if line.trim_end() == "#ifdef DC2TK_C_TOOLS" {
            if !doc.is_empty() {
                return Err(invalid(file, at + 1, "a comment documents a directive"));
            }
            while at < lines.len() && !matches!(lines[at].trim_end(), "#else" | "#endif") {
                at += 1;
            }
            at += 1;
            continue;
        }
        let kind = if let Some(rest) = line.strip_prefix("#define ") {
            let (name, value) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            at += 1;
            if name.starts_with("DC2TK_") || value.trim().is_empty() {
                continue;
            }
            ItemKind::Define { name: name.to_string(), value: value.trim().to_string() }
        } else if line.starts_with('#') {
            if !doc.is_empty() {
                return Err(invalid(file, at + 1, "a comment documents a directive"));
            }
            at += 1;
            continue;
        } else if let Some(rest) = line.strip_prefix("typedef ") {
            let rest = rest.trim_end().trim_end_matches(';');
            let (base, name) =
                rest.rsplit_once(' ').ok_or_else(|| invalid(file, at + 1, "a typedef without a name"))?;
            at += 1;
            ItemKind::Kind { name: name.to_string(), base: base.to_string() }
        } else if line.starts_with("enum ") || line.starts_with("enum{") {
            let (text, next) = through(&lines, at, file, |line| line.starts_with("};"))?;
            let name = line.strip_prefix("enum").unwrap().trim().trim_end_matches('{').trim();
            let members = enum_members(&text, file, at)?;
            at = next;
            ItemKind::Enum { name: (!name.is_empty()).then(|| name.to_string()), members }
        } else if line.starts_with("extern ") {
            let (text, next) = through(&lines, at, file, |line| line.trim_end().ends_with(';'))?;
            let number = text
                .split_once("NATIVE(")
                .and_then(|(_, rest)| rest.split_once(')'))
                .map(|(number, _)| number.replace(',', " +"))
                .ok_or_else(|| invalid(file, at + 1, "an extern without NATIVE(...)"))?;
            at = next;
            ItemKind::Native { name: declared_name(&text), number, text }
        } else if line.contains('(') && !line.trim_end().ends_with(';') {
            // an inline helper or a library's function: its definition, through the closing brace
            let (text, next) = through(&lines, at, file, |line| line == "}")?;
            at = next;
            ItemKind::Function { name: declared_name(&text), text }
        } else {
            return Err(invalid(file, at + 1, &format!("not a declaration the reference reads: {line}")));
        };
        header.items.push(Item { doc, kind });
    }
    Ok(header)
}

/// An enum's members, with the comments before them; a member without `=` is one more than the
/// one before.
fn enum_members(text: &str, file: &str, start: usize) -> Result<Vec<Member>> {
    let mut members = Vec::new();
    let mut doc = Vec::new();
    let mut next: Option<i64> = Some(0);
    for (offset, line) in text.lines().enumerate().skip(1) {
        let line = line.trim();
        if line.starts_with("};") || line.is_empty() {
            continue;
        }
        if let Some(text) = comment(line) {
            doc.push(text);
            continue;
        }
        let (code, trailing) = match line.split_once("//") {
            Some((code, trailing)) => (code.trim(), Some(trailing.trim().to_string())),
            None => (line, None),
        };
        let code = code.trim_end_matches(',').trim();
        let (name, value) = match code.split_once('=') {
            Some((name, value)) => (name.trim(), value.trim().to_string()),
            None => (code, next.map(|value| value.to_string()).unwrap_or_default()),
        };
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(invalid(file, start + offset + 1, &format!("not an enum member: {line}")));
        }
        next = parse_int(&value).map(|value| value + 1);
        doc.extend(trailing);
        members.push(Member { doc: std::mem::take(&mut doc), name: name.to_string(), value });
    }
    Ok(members)
}

fn parse_int(text: &str) -> Option<i64> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let value = match digits.strip_prefix("0x").or_else(|| digits.strip_prefix("0X")) {
        Some(hex) => i64::from_str_radix(hex, 16).ok()?,
        None => digits.parse().ok()?,
    };
    Some(if negative { -value } else { value })
}

/// Text for Markdown: outside `code`, the characters that would start emphasis, HTML or a link
/// are escaped, and in a table the column bars too.
fn escaped(text: &str, in_table: bool) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut code = false;
    for (at, &c) in chars.iter().enumerate() {
        let word = |at: Option<usize>| at.and_then(|at| chars.get(at)).is_some_and(|c| c.is_alphanumeric());
        let escape = !code
            && match c {
                '*' | '<' | '>' | '[' | '\\' => true,
                '_' => !word(at.checked_sub(1)) || !word(Some(at + 1)),
                _ => false,
            };
        if c == '`' {
            code = !code;
        }
        if escape {
            out.push('\\');
        }
        if c == '|' && in_table {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Comment lines as paragraphs: an empty line or an "Original name:" line starts a new one, and
/// the original name is shown as code.
fn paragraphs(doc: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    for line in doc {
        if (line.is_empty() || line.starts_with("Original name:")) && !current.is_empty() {
            out.push(current.join(" "));
            current.clear();
        }
        if let Some(name) = line.strip_prefix("Original name: ") {
            out.push(format!("Original name: `{}`.", name.trim_end_matches('.')));
        } else if !line.is_empty() {
            current.push(line.clone());
        }
    }
    if !current.is_empty() {
        out.push(current.join(" "));
    }
    out
}

fn prose(doc: &[String]) -> String {
    paragraphs(doc).iter().map(|paragraph| escaped(paragraph, false)).collect::<Vec<_>>().join("\n\n")
}

/// The whole comment on one line, for a table cell.
fn cell(doc: &[String]) -> String {
    escaped(&paragraphs(doc).join(" "), true)
}

/// The comment's first sentence, for a table cell.
fn summary(doc: &[String]) -> String {
    let all = paragraphs(doc).into_iter().next().unwrap_or_default();
    let mut end = all.len();
    let mut code = false;
    for (at, c) in all.char_indices() {
        if c == '`' {
            code = !code;
        }
        if !code && c == '.' && all[at + 1..].starts_with(' ') {
            end = at + 1;
            break;
        }
    }
    escaped(&all[..end], true)
}

/// The anchor GitHub gives a heading: lower case, spaces as `-`, punctuation but `-` and `_`
/// dropped.
fn anchor(heading: &str) -> String {
    heading
        .to_lowercase()
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
            _ => None,
        })
        .collect()
}

const GENERATED: &str = "Generated by `dc2tk api-docs` from the SDK's headers; change the headers, not this page.";

struct TablePage {
    table: &'static str,
    title: &'static str,
    main: Header,
    helpers: Vec<Item>,
}

/// The index's row of a table: its header, what runs it, and its counts.
struct Row {
    table: &'static str,
    about: String,
    natives: usize,
    helpers: usize,
    programs: usize,
    constants: usize,
}

impl TablePage {
    fn new(table: &'static str, title: &'static str, main: Header, helpers: Option<Header>) -> TablePage {
        // a helpers header repeats a value kind of the main header's so it reads alone
        let kinds: Vec<String> = main.items.iter().filter_map(|item| kind_name(&item.kind)).collect();
        let helpers = helpers
            .map(|header| header.items)
            .unwrap_or_default()
            .into_iter()
            .filter(|item| kind_name(&item.kind).is_none_or(|name| !kinds.contains(&name)))
            .collect();
        TablePage { table, title, main, helpers }
    }

    fn items(&self) -> impl Iterator<Item = &Item> {
        self.main.items.iter().chain(&self.helpers)
    }

    fn natives(&self) -> Vec<&Item> {
        self.items().filter(|item| matches!(item.kind, ItemKind::Native { .. })).collect()
    }

    fn functions(&self) -> Vec<&Item> {
        self.items().filter(|item| matches!(item.kind, ItemKind::Function { .. })).collect()
    }

    fn programs(&self) -> Vec<&Member> {
        self.items()
            .filter_map(|item| match &item.kind {
                ItemKind::Enum { name: Some(name), members } if name == "program" => Some(members),
                _ => None,
            })
            .flatten()
            .collect()
    }

    fn enums(&self) -> Vec<&Item> {
        self.items()
            .filter(|item| match &item.kind {
                ItemKind::Enum { name, .. } => name.as_deref() != Some("program"),
                _ => false,
            })
            .collect()
    }

    fn defines(&self) -> Vec<&Item> {
        self.items().filter(|item| matches!(item.kind, ItemKind::Define { .. })).collect()
    }

    fn kinds(&self) -> Vec<&Item> {
        self.items().filter(|item| matches!(item.kind, ItemKind::Kind { .. })).collect()
    }

    fn constant_count(&self) -> usize {
        let members: usize = self
            .enums()
            .iter()
            .map(|item| match &item.kind {
                ItemKind::Enum { members, .. } => members.len(),
                _ => 0,
            })
            .sum();
        members + self.defines().len()
    }

    fn row(&self) -> Row {
        let first = paragraphs(&self.main.intro).into_iter().next().unwrap_or_default();
        let about = first.strip_prefix("dc2tk SDK. ").unwrap_or(&first).to_string();
        Row {
            table: self.table,
            about: escaped(&about, true),
            natives: self.natives().len(),
            helpers: self.functions().len(),
            programs: self.programs().len(),
            constants: self.constant_count(),
        }
    }

    fn text(&self) -> String {
        let mut out = String::new();
        let table = self.table;
        let _ = writeln!(out, "# {}: `<{table}.h>`\n", self.title);
        let intro = paragraphs(&self.main.intro);
        let mut intro = intro.iter().map(|paragraph| escaped(paragraph, false));
        if let Some(first) = intro.next() {
            let first = first.strip_prefix("dc2tk SDK. ").unwrap_or(&first).to_string();
            let _ = writeln!(out, "{first}\n");
        }
        for paragraph in intro {
            let _ = writeln!(out, "{paragraph}\n");
        }
        let helpers = match self.helpers.is_empty() {
            true => String::new(),
            false => format!(" and `sdk/helpers/{table}.h`"),
        };
        let _ = writeln!(
            out,
            "Declared in `sdk/{table}.h`{helpers}. See [C Language, SDK, and Patches](../language.md) for SDK syntax \
             and [API Reference](README.md) for all headers. {GENERATED}\n"
        );
        let sections = [
            ("Natives", self.natives().len()),
            ("Helpers", self.functions().len()),
            ("Programs", self.programs().len()),
            ("Constants", self.constant_count()),
            ("Value kinds", self.kinds().len()),
        ];
        for (section, count) in sections.iter().filter(|(_, count)| *count > 0) {
            let _ = writeln!(out, "- [{section}](#{}): {count}", anchor(section));
        }
        out.push('\n');
        self.write_natives(&mut out);
        self.write_helpers(&mut out);
        self.write_programs(&mut out);
        self.write_constants(&mut out);
        self.write_kinds(&mut out);
        format!("{}\n", out.trim_end())
    }

    fn write_natives(&self, out: &mut String) {
        let natives = self.natives();
        if natives.is_empty() {
            return;
        }
        out.push_str("## Natives\n\n| Native | Number | What it does |\n| --- | --- | --- |\n");
        for item in &natives {
            if let ItemKind::Native { name, number, .. } = &item.kind {
                let _ = writeln!(out, "| [`{name}`](#{}) | {number} | {} |", anchor(name), summary(&item.doc));
            }
        }
        out.push('\n');
        for item in &natives {
            if let ItemKind::Native { name, text, .. } = &item.kind {
                let _ = writeln!(out, "### {name}\n\n```c\n{text}\n```\n");
                let doc = prose(&item.doc);
                if !doc.is_empty() {
                    let _ = writeln!(out, "{doc}\n");
                }
            }
        }
    }

    fn write_helpers(&self, out: &mut String) {
        let functions = self.functions();
        if functions.is_empty() {
            return;
        }
        out.push_str(
            "## Helpers\n\nInline helper functions that wrap common sequences of native calls.\n\n| Helper | What it does |\n| --- | --- |\n",
        );
        for item in &functions {
            if let ItemKind::Function { name, .. } = &item.kind {
                let _ = writeln!(out, "| [`{name}`](#{}) | {} |", anchor(name), summary(&item.doc));
            }
        }
        out.push('\n');
        for item in &functions {
            if let ItemKind::Function { name, text } = &item.kind {
                let _ = writeln!(out, "### {name}\n");
                let doc = prose(&item.doc);
                if !doc.is_empty() {
                    let _ = writeln!(out, "{doc}\n");
                }
                let _ = writeln!(out, "```c\n{text}\n```\n");
            }
        }
    }

    fn write_programs(&self, out: &mut String) {
        let programs = self.programs();
        if programs.is_empty() {
            return;
        }
        out.push_str(
            "## Programs\n\nProgram entry-point numbers started by the engine (`enum program`). Register an entry \
             point in a script with `PROGRAM(name, function);`.\n\n| Program | Number | When it runs |\n| --- | --- | --- |\n",
        );
        for member in programs {
            let _ = writeln!(out, "| `{}` | {} | {} |", member.name, member.value, cell(&member.doc));
        }
        out.push('\n');
    }

    fn write_constants(&self, out: &mut String) {
        let (enums, defines) = (self.enums(), self.defines());
        if enums.is_empty() && defines.is_empty() {
            return;
        }
        out.push_str("## Constants\n\n");
        if !defines.is_empty() {
            out.push_str("| Name | Value | What it is |\n| --- | --- | --- |\n");
            for item in &defines {
                if let ItemKind::Define { name, value } = &item.kind {
                    let _ = writeln!(out, "| `{name}` | `{value}` | {} |", cell(&item.doc));
                }
            }
            out.push('\n');
        }
        for item in enums {
            let ItemKind::Enum { name, members } = &item.kind else {
                continue;
            };
            match name {
                Some(name) => {
                    let _ = writeln!(out, "### enum {name}\n");
                }
                None => out.push_str("### Other constants\n\n"),
            }
            let doc = prose(&item.doc);
            if !doc.is_empty() {
                let _ = writeln!(out, "{doc}\n");
            }
            let described = members.iter().any(|member| !member.doc.is_empty());
            match described {
                true => out.push_str("| Name | Value | What it is |\n| --- | --- | --- |\n"),
                false => out.push_str("| Name | Value |\n| --- | --- |\n"),
            }
            for member in members {
                let value = match member.value.is_empty() {
                    true => String::new(),
                    false => format!("`{}`", member.value),
                };
                match described {
                    true => {
                        let _ = writeln!(out, "| `{}` | {value} | {} |", member.name, cell(&member.doc));
                    }
                    false => {
                        let _ = writeln!(out, "| `{}` | {value} |", member.name);
                    }
                }
            }
            out.push('\n');
        }
    }

    fn write_kinds(&self, out: &mut String) {
        let kinds = self.kinds();
        if kinds.is_empty() {
            return;
        }
        out.push_str(
            "## Value kinds\n\nTypedef aliases that indicate what a parameter value represents (see [Value kinds](../language.md#value-kinds)).\n\n| Kind | Type | What it \
             is |\n| --- | --- | --- |\n",
        );
        for item in kinds {
            if let ItemKind::Kind { name, base } = &item.kind {
                let _ = writeln!(out, "| `{name}` | `{base}` | {} |", cell(&item.doc));
            }
        }
        out.push('\n');
    }
}

fn kind_name(kind: &ItemKind) -> Option<String> {
    match kind {
        ItemKind::Kind { name, .. } => Some(name.clone()),
        _ => None,
    }
}

fn index_page(rows: &[Row], functions: usize) -> String {
    let mut out = String::from("# API Reference\n\n");
    out.push_str(
        "This section lists all built-in engine natives, inline helpers, `PROGRAM` entry points, constants, and \
         `modkit` functions declared in the `dc2tk` SDK. Each script includes the header for its script category \
         (see [How Scripts Work](../README.md) and [C Language, SDK, and Patches](../language.md)). ",
    );
    let _ = writeln!(out, "{GENERATED}\n");
    out.push_str(
        "| Header | Scripts | Natives | Helpers | Programs | Constants |\n| --- | --- | --- | --- | --- | --- |\n",
    );
    for row in rows {
        let _ = writeln!(
            out,
            "| [`<{0}.h>`]({0}.md) | {1} | {2} | {3} | {4} | {5} |",
            row.table, row.about, row.natives, row.helpers, row.programs, row.constants
        );
    }
    let _ = writeln!(
        out,
        "\n- **[`modkit`](modkit.md)**: Helper libraries for mods (`#include \"modkit/....c\"`, {functions} functions)."
    );
    out
}

fn modkit_page(libraries: &[(String, Header)]) -> String {
    let mut out = String::from("# modkit\n\n");
    out.push_str(
        "`sdk/modkit/` contains helper libraries for mods. Include a library with a quoted `#include` \
         (for example, `#include \"modkit/text.c\"`). ",
    );
    let _ = writeln!(out, "{GENERATED}\n");
    for (name, _) in libraries {
        let _ = writeln!(out, "- [`{name}`](#{})", anchor(name));
    }
    out.push('\n');
    for (name, library) in libraries {
        let _ = writeln!(out, "## {name}\n");
        let intro = prose(&library.intro);
        if !intro.is_empty() {
            let _ = writeln!(out, "{intro}\n");
        }
        for item in &library.items {
            if let ItemKind::Function { name, text } = &item.kind {
                let _ = writeln!(out, "### {name}\n");
                let doc = prose(&item.doc);
                if !doc.is_empty() {
                    let _ = writeln!(out, "{doc}\n");
                }
                let _ = writeln!(out, "```c\n{text}\n```\n");
            }
        }
    }
    format!("{}\n", out.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repository() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
    }

    #[test]
    fn a_header_reads_as_its_declarations() {
        let text = "// dc2tk SDK. Natives for tests.\n//\n// More.\n\n#ifndef DC2TK_T_H\n#define DC2TK_T_H\n\n\
                    // A kind.\ntypedef int flag;\n\n#define PI 3.1415927\n\n// Programs.\nenum program {\n    // \
                    Starts.\n    map_start = 100,\n    next,\n};\n\n// Returns a\n// counter.\n// Original name: \
                    _GET_CNT.\nextern int get_counter(int index,\n    OPTIONAL(int x)) NATIVE(70, 15);\n\n// Helps.\n\
                    static inline void help(int a)\n{\n    get_counter(a);\n}\n\n#endif\n";
        let header = scan(text, "t.h").unwrap();
        assert_eq!(header.intro, ["dc2tk SDK. Natives for tests.", "", "More."]);
        assert_eq!(header.items.len(), 5);
        let ItemKind::Enum { members, .. } = &header.items[2].kind else { panic!() };
        assert_eq!((members[1].name.as_str(), members[1].value.as_str()), ("next", "101"));
        let ItemKind::Native { name, number, .. } = &header.items[3].kind else { panic!() };
        assert_eq!((name.as_str(), number.as_str()), ("get_counter", "70 + 15"));
        assert_eq!(paragraphs(&header.items[3].doc), ["Returns a counter.", "Original name: `_GET_CNT`."]);
    }

    #[test]
    fn a_line_the_reader_does_not_know_is_an_error() {
        assert!(scan("int x;\n", "t.h").is_err());
        assert!(scan("// A comment.\n\n// Another, about nothing.\n\nint f(void)\n{\n}\n", "t.h").is_err());
    }

    #[test]
    fn markdown_characters_are_escaped_outside_code() {
        assert_eq!(escaped("*p and <page> but `*p` | _GET_X", true), "\\*p and \\<page\\> but `*p` \\| \\_GET_X");
        assert_eq!(summary(&["Ends here. Not this.".to_string()]), "Ends here.");
    }

    #[test]
    fn the_api_pages_are_the_ones_the_headers_give() {
        let repository = repository();
        for (name, text) in pages(&repository.join("sdk")).unwrap() {
            let path = repository.join("docs/scripting/api").join(&name);
            let written = read_text(&path).unwrap_or_default();
            assert!(written == text, "docs/scripting/api/{name} is out of date with sdk/: run dc2tk api-docs");
        }
    }
}
