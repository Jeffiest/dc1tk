//! The engine's native function tables (event, monster, action, effect, arg), generated from the
//! ChronicleTwo decompilation into JSON, and which table a script calls into.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use dc2tk_disc::files;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::Result;

pub const TABLES: [&str; 5] = ["event", "monster", "action", "effect", "arg"];

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Native {
    pub no: i32,
    pub name: String,
    pub accessors: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Table {
    pub table: String,
    pub source: String,
    pub array: String,
    pub natives: Vec<Native>,
    #[serde(flatten)]
    pub old: OldTable,
}

/// How scripts built for an older native table call it, worked out by hand from those scripts
/// rather than from the decompilation (monster and action have them; see docs/internals/scripts.md,
/// "Older native numberings").
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct OldTable {
    /// Groups of an older native table, by base: the base of the same group in this one.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub old_groups: IndexMap<String, i64>,
    /// Groups that are current groups too, read as old in scripts built for the older table.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub old_table_groups: IndexMap<String, i64>,
    /// Numbers of the older table called without a group, by the number they stand for now.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub old_direct: IndexMap<String, i64>,
}

impl Table {
    /// The natives by number.
    pub fn numbered(&self) -> BTreeMap<i32, &Native> {
        self.natives.iter().map(|native| (native.no, native)).collect()
    }
}

/// The tables in a folder, by name; a table without a file is left out.
pub fn load(directory: &Path) -> Result<BTreeMap<String, Table>> {
    let mut tables = BTreeMap::new();
    for name in TABLES {
        let path = directory.join(format!("{name}.json"));
        if path.exists() {
            let table: Table = serde_json::from_str(&files::read_to_string(&path)?)
                .map_err(|error| crate::Error::invalid(format!("{}: {error}", path.display())))?;
            tables.insert(name.to_string(), table);
        }
    }
    Ok(tables)
}

/// The table a script calls into, by the first rule (path prefix, table) its path fits; "event"
/// when none does.
pub fn table_for<'a>(path: &str, rules: &'a [(String, String)]) -> &'a str {
    let text = path.replace('\\', "/");
    for (prefix, table) in rules {
        if text.starts_with(prefix.as_str()) || text.contains(&format!("/{prefix}")) {
            return table;
        }
    }
    "event"
}

/// Where a table comes from in a ChronicleTwo checkout: the source holding its array of
/// RS_EXTFUNC_INFO rows, and the header whose enum numbers them.
#[derive(Clone, Copy, Debug)]
pub struct Spec {
    pub table: &'static str,
    pub source: &'static str,
    pub header: &'static str,
    pub array: &'static str,
}

pub const SPECS: [Spec; 5] = [
    Spec {
        table: "event",
        source: "ps2/src/event_func.cpp",
        header: "ps2/include/event_func.hpp",
        array: "ext_func_info",
    },
    Spec {
        table: "monster",
        source: "ps2/src/runscript_opcodes.cpp",
        header: "ps2/include/runscript_opcodes.hpp",
        array: "ext_func_info",
    },
    Spec {
        table: "action",
        source: "ps2/src/actscript.cpp",
        header: "ps2/include/actscript.hpp",
        array: "ext_func_info",
    },
    Spec {
        table: "effect",
        source: "ps2/src/effscript.cpp",
        header: "ps2/include/effscript.hpp",
        array: "ext_func_info",
    },
    Spec {
        table: "arg",
        source: "ps2/src/event_func.cpp",
        header: "ps2/include/event_func.hpp",
        array: "esa_ext_func_info",
    },
];

/// The stack accessors a native's body calls, and what each reads or writes; in the order the
/// pattern tries them.
const ACCESSORS: [(&str, &str); 7] = [
    ("GetStackInt", "int"),
    ("GetStackFloat", "float"),
    ("GetStackString", "string"),
    ("GetStackVector", "vector"),
    ("GetStack", "any"),
    ("SetStack", "out"),
    ("SetStackVector", "out_vector"),
];

/// A generated table, as definitions/natives/<table>.json holds it.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Generated {
    pub table: String,
    pub source: String,
    pub array: String,
    pub natives: Vec<GeneratedNative>,
    /// Kept from the file the table replaces, since the decompilation cannot give it.
    #[serde(flatten)]
    pub old: OldTable,
}

/// Ordered by number, then name, then accessors (the derived order of the fields).
#[derive(Clone, Debug, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct GeneratedNative {
    pub no: i128,
    pub name: String,
    /// None when the checkout has no body for the native.
    pub accessors: Option<Vec<String>>,
}

/// The patterns a checkout's C++ is read with; names are C identifiers, ASCII.
struct Patterns {
    enum_value: regex::Regex,
    table_row: regex::Regex,
    accessor_call: regex::Regex,
}

impl Patterns {
    fn new() -> Patterns {
        let build = |pattern: &str| regex::Regex::new(pattern).expect("the natives patterns compile");
        let names: Vec<&str> = ACCESSORS.iter().map(|(name, _)| *name).collect();
        Patterns {
            enum_value: build(r"(?m)^\s*([A-Z][A-Z0-9_]*)\s*=\s*(-?(?:0x[0-9a-fA-F]+|[0-9]+))\s*,"),
            table_row: build(r"\{\s*([A-Za-z0-9_]+)\s*,\s*([A-Za-z0-9_]+)\s*\}"),
            // the word boundary before the name is checked by hand (accessors)
            accessor_call: build(&format!(r"({})\s*\(", names.join("|"))),
        }
    }
}

/// A source file's text, with what is not UTF-8 replaced and \r\n read as \n.
fn read_source(path: &Path) -> Result<String> {
    let bytes = files::read(path)?;
    Ok(String::from_utf8_lossy(&bytes).replace("\r\n", "\n"))
}

/// The enum constants a header defines (NAME = value,), by name; a later one wins.
fn parse_enum_values(patterns: &Patterns, text: &str) -> Result<HashMap<String, i128>> {
    let mut values = HashMap::new();
    for found in patterns.enum_value.captures_iter(text) {
        let value = &found[2];
        let number = crate::index::integer(value).ok_or_else(|| {
            crate::Error::invalid(format!("{} = {value}: not an integer the tools can hold", &found[1]))
        })?;
        values.insert(found[1].to_string(), number as i128);
    }
    Ok(values)
}

/// The (function, number) rows of an RS_EXTFUNC_INFO array, without the NULL that ends it.
fn parse_table_rows(patterns: &Patterns, text: &str, array: &str) -> Result<Vec<(String, String)>> {
    let pattern = format!(r"(?s)static RS_EXTFUNC_INFO {}\[[0-9]*\] = \{{(.*?)\n\}};", regex::escape(array));
    let table = regex::Regex::new(&pattern).expect("the table pattern compiles");
    let Some(found) = table.captures(text) else {
        return Err(crate::Error::invalid(format!("table {array} not found")));
    };
    Ok(patterns
        .table_row
        .captures_iter(&found[1])
        .filter(|row| &row[1] != "NULL")
        .map(|row| (row[1].to_string(), row[2].to_string()))
        .collect())
}

/// Whether the character before a byte offset is a C identifier's, so a name there would not
/// start a word.
fn word_before(text: &str, at: usize) -> bool {
    text[..at].chars().next_back().is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn skip_space(text: &str, at: usize) -> usize {
    text[at..].find(|c: char| !c.is_whitespace()).map_or(text.len(), |offset| at + offset)
}

/// Where `\s+NAME\s*\(\s*RS_STACKDATA[^)]*\)\s*\{` matches from the start of rest, as the offset
/// after the brace. Each part of it can match only one way, so it needs no backtracking.
fn function_header(rest: &str, name: &str) -> Option<usize> {
    let mut at = skip_space(rest, 0);
    if at == 0 || !rest[at..].starts_with(name) {
        return None;
    }
    at = skip_space(rest, at + name.len());
    if !rest[at..].starts_with('(') {
        return None;
    }
    at = skip_space(rest, at + 1);
    if !rest[at..].starts_with("RS_STACKDATA") {
        return None;
    }
    at += "RS_STACKDATA".len();
    at += rest[at..].find(')')? + 1;
    at = skip_space(rest, at);
    rest[at..].starts_with('{').then_some(at + 1)
}

/// The body of `int NAME(RS_STACKDATA ...) { ... }`, by matching braces; None when there is no
/// such function. A body whose braces never close runs to the text's end.
pub fn function_body<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let mut from = 0;
    let start = loop {
        let at = from + text[from..].find("int")?;
        from = at + 1;
        if word_before(text, at) {
            continue;
        }
        if let Some(end) = function_header(&text[at + 3..], name) {
            break at + 3 + end;
        }
    };
    let mut depth = 1;
    for (offset, c) in text[start..].char_indices() {
        if c == '{' {
            depth += 1;
        }
        if c == '}' {
            depth -= 1;
        }
        if depth == 0 {
            return Some(&text[start..start + offset]);
        }
    }
    Some(&text[start..])
}

/// What a body reads and writes with the stack accessors, in the order it calls them.
fn accessors(patterns: &Patterns, body: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(call) = patterns.accessor_call.captures_at(body, at) {
        let whole = call.get(0).expect("a match has a whole");
        if word_before(body, whole.start()) {
            // \b fails here, so the search goes on from the next character (the names are ASCII)
            at = whole.start() + 1;
            continue;
        }
        let kind = ACCESSORS.iter().find(|(name, _)| *name == &call[1]).map(|(_, kind)| *kind);
        found.push(kind.expect("the pattern matches only accessors").to_string());
        at = whole.end();
    }
    found
}

fn build_with(patterns: &Patterns, root: &Path, spec: &Spec) -> Result<Generated> {
    let source = read_source(&root.join(spec.source))?;
    let values = parse_enum_values(patterns, &read_source(&root.join(spec.header))?)?;
    let mut natives = Vec::new();
    for (func, number) in parse_table_rows(patterns, &source, spec.array)? {
        let Some(&no) = values.get(&number) else {
            return Err(crate::Error::invalid(format!("{}: no value for {}", spec.table, number)));
        };
        let calls = function_body(&source, &func).map(|body| accessors(patterns, body));
        natives.push(GeneratedNative { no, name: func, accessors: calls });
    }
    natives.sort();
    Ok(Generated {
        table: spec.table.to_string(),
        source: spec.source.to_string(),
        array: spec.array.to_string(),
        natives,
        old: OldTable::default(),
    })
}

/// A table from a ChronicleTwo checkout: its natives by number, with the accessors each calls.
pub fn build_table(root: &Path, spec: &Spec) -> Result<Generated> {
    build_with(&Patterns::new(), root, spec)
}

/// The hand-recorded part of a table file already written, so regenerating keeps it; none when
/// there is no file.
fn recorded(path: &Path) -> Result<OldTable> {
    if !path.exists() {
        return Ok(OldTable::default());
    }
    serde_json::from_str(&files::read_to_string(path)?)
        .map_err(|error| crate::Error::invalid(format!("{}: {error}", path.display())))
}

/// Writes every table into out_dir as <table>.json, one after another (a table that fails leaves
/// the ones before it written), and returns them. What a file there already records of the older
/// tables (`OldTable`) is kept.
pub fn generate(root: &Path, out_dir: &Path) -> Result<Vec<Generated>> {
    files::create_dir_all(out_dir)?;
    let patterns = Patterns::new();
    let mut tables = Vec::new();
    for spec in &SPECS {
        let path = out_dir.join(format!("{}.json", spec.table));
        let table = Generated { old: recorded(&path)?, ..build_with(&patterns, root, spec)? };
        dc2tk_disc::textfile::write_text(&path, &dc2tk_disc::textfile::json_text(&table))?;
        tables.push(table);
    }
    Ok(tables)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;

    const HEADER: &str = "
enum TestCommand {
    TEST_EXT_END = -1, /**< Ends the table. */
    TEST_EXT_GET_FLAG = 0x0C, /**< Runs _GET_FLAG. */
    TEST_EXT_SET_FLAG = 3,
};
";

    const SOURCE: &str = "
int _SET_FLAG(RS_STACKDATA *stack, int arg_count) {
    int bit = GetStackInt(stack++);
    float value = GetStackFloat(stack);
    if (bit) {
        value = 0;
    }
    return 1;
}

int _GET_FLAG(RS_STACKDATA *stack, int arg_count) {
    char *name = GetStackString(stack++);
    SetStack(stack, 1);
    return 1;
}

static RS_EXTFUNC_INFO ext_func_info[3] = {
    {_GET_FLAG, TEST_EXT_GET_FLAG},
    {_SET_FLAG, TEST_EXT_SET_FLAG},
    {NULL,      TEST_EXT_END     },
};
";

    const SPEC: Spec =
        Spec { table: "test", source: "src/test.cpp", header: "include/test.hpp", array: "ext_func_info" };

    fn repository() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
    }

    fn native(no: i128, name: &str, accessors: &[&str]) -> GeneratedNative {
        GeneratedNative {
            no,
            name: name.into(),
            accessors: Some(accessors.iter().map(|kind| kind.to_string()).collect()),
        }
    }

    #[test]
    fn builds_table_from_enum_and_rows() {
        let root = std::env::temp_dir().join(format!("dc2tk-script-natives-{}", std::process::id()));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("include")).unwrap();
        dc2tk_disc::textfile::write_text(&root.join("src/test.cpp"), SOURCE).unwrap();
        dc2tk_disc::textfile::write_text(&root.join("include/test.hpp"), HEADER).unwrap();
        let table = build_table(&root, &SPEC).unwrap();
        fs::remove_dir_all(&root).unwrap();
        assert_eq!(
            table.natives,
            vec![native(3, "_SET_FLAG", &["int", "float"]), native(12, "_GET_FLAG", &["string", "out"])]
        );
    }

    #[test]
    fn regenerating_keeps_what_the_tables_record_of_the_older_ones() {
        let root = std::env::temp_dir().join(format!("dc2tk-script-natives-keep-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for spec in &SPECS {
            let arg_table = SOURCE.replace("ext_func_info", "esa_ext_func_info");
            for (path, text) in [(spec.source, format!("{SOURCE}{arg_table}")), (spec.header, HEADER.to_string())] {
                fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
                fs::write(root.join(path), text).unwrap();
            }
        }
        let out = root.join("natives");
        fs::create_dir_all(&out).unwrap();
        let old = r#"{"table": "monster", "source": "", "array": "", "natives": [],
            "old_groups": {"60": 70}, "old_table_groups": {"110": 120}, "old_direct": {"10": 35}}"#;
        fs::write(out.join("monster.json"), old).unwrap();
        generate(&root, &out).unwrap();
        let tables = load(&out).unwrap();
        fs::remove_dir_all(&root).unwrap();
        let monster = &tables["monster"].old;
        assert_eq!(monster.old_groups, IndexMap::from([("60".to_string(), 70)]));
        assert_eq!(monster.old_table_groups, IndexMap::from([("110".to_string(), 120)]));
        assert_eq!(monster.old_direct, IndexMap::from([("10".to_string(), 35)]));
        assert_eq!(tables["monster"].natives.len(), 2);
        assert_eq!(tables["event"].old, OldTable::default());
    }

    #[test]
    fn generated_tables_write_back_as_they_are() {
        let folder = repository().join("definitions").join("natives");
        for name in TABLES {
            let text = fs::read_to_string(folder.join(format!("{name}.json"))).unwrap().replace("\r\n", "\n");
            let table: Table = serde_json::from_str(&text).unwrap();
            let generated = Generated {
                table: table.table.clone(),
                source: table.source.clone(),
                array: table.array.clone(),
                natives: table
                    .natives
                    .iter()
                    .map(|native| GeneratedNative {
                        no: native.no.into(),
                        name: native.name.clone(),
                        accessors: native.accessors.clone(),
                    })
                    .collect(),
                old: table.old.clone(),
            };
            assert_eq!(dc2tk_disc::textfile::json_text(&generated), text, "{name}");
        }
    }

    #[test]
    fn function_body_matches_braces() {
        let body = function_body(SOURCE, "_SET_FLAG").unwrap();
        assert!(body.contains("value = 0;"));
        assert!(!body.contains("_GET_FLAG"));
        assert_eq!(function_body(SOURCE, "_MISSING"), None);
    }

    #[test]
    fn function_body_without_its_closing_brace_runs_to_the_end() {
        assert_eq!(function_body("int _F(RS_STACKDATA *s) { a(); }", "_F"), Some(" a(); "));
        assert_eq!(function_body("int _F(RS_STACKDATA *s) { a(); {", "_F"), Some(" a(); {"));
        assert_eq!(function_body("int _F(RS_STACKDATA *s) {", "_F"), Some(""));
        assert_eq!(function_body("uint _F(RS_STACKDATA *s) { }", "_F"), None);
    }

    #[test]
    fn accessors_need_a_word_boundary() {
        let patterns = Patterns::new();
        let body =
            "xGetStackInt(s); SetStackVector (s); GetStack\u{3000}(s); GetStackIntx(s); _SetStack(s); SetStack(s)";
        assert_eq!(accessors(&patterns, body), ["out_vector", "any", "out"]);
    }

    #[test]
    fn table_for_path() {
        let pal = repository().join("definitions").join("pal");
        let rules = crate::definitions::load(&[pal]).unwrap().tables;
        assert_eq!(table_for("map/s/s40/s40.stb", &rules), "event");
        assert_eq!(table_for("dungeon/sys_script/d05.stb", &rules), "event");
        assert_eq!(table_for("dungeon/monster/b02a.stb", &rules), "monster");
        assert_eq!(table_for("dungeon/articles/a01.stb", &rules), "effect");
        assert_eq!(table_for("mainchr/c01.stb", &rules), "action");
        assert_eq!(table_for("event/test_arg.stb", &rules), "arg");
        assert_eq!(table_for("event/tbox/bomb.pack.d/tbox_exp.stb", &rules), "effect");
    }

    #[test]
    fn generated_tables_are_present() {
        let tables = load(&repository().join("definitions").join("natives")).unwrap();
        let counts: Vec<(&str, usize)> =
            TABLES.iter().map(|name| (*name, tables.get(*name).map_or(0, |table| table.natives.len()))).collect();
        assert_eq!(counts, [("event", 696), ("monster", 173), ("action", 82), ("effect", 128), ("arg", 2)]);
        for table in tables.values() {
            let numbers: std::collections::HashSet<i32> = table.natives.iter().map(|native| native.no).collect();
            let names: std::collections::HashSet<&str> =
                table.natives.iter().map(|native| native.name.as_str()).collect();
            assert_eq!(numbers.len(), table.natives.len());
            assert_eq!(names.len(), table.natives.len());
        }
        assert_eq!(tables["event"].numbered()[&12].name, "_ASSIGN_STACK");
    }
}
