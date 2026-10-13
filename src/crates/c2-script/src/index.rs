//! Reads a decompiled source tree as text, for the commands that answer questions about it (flags,
//! xref, fork, changes): the calls each file makes with their arguments, its PROGRAM lines and
//! includes, and the constants its headers define. It does not compile anything, so it also reads
//! trees that would not compile.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use indexmap::IndexMap;
use regex::Regex;

use crate::definitions::Definitions;
use crate::{sdk, source, Result};

fn regex(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("a valid pattern"))
}

fn constant_pattern() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    regex(
        &CELL,
        r"(?m)^\s*(?:#define\s+([A-Za-z_]\w*)\s+|([A-Za-z_]\w*)\s*=\s*)(-?(?:0x[0-9A-Fa-f]+|\d+))\s*,?\s*(?://.*)?$",
    )
}

fn definition_pattern() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    regex(&CELL, r"(?m)^(?:void|int|float|string)\s+\**(\w+)\s*\(")
}

fn program_pattern() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    regex(&CELL, r"(?m)^PROGRAM\(\s*([^,]+?)\s*,\s*(\w+)\s*\);")
}

pub fn include_pattern() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    regex(&CELL, r#"(?m)^#include\s+"([^"]+)""#)
}

/// The decompiler's comment on a call written for an older native table: the engine runs the
/// native the call names, but the script meant another.
fn old_numbering() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    regex(&CELL, r"//.*\bold numbering, meant as (\w+)")
}

/// Every integer constant the headers define: enum members (including program names such as
/// story_50) and #defines.
pub fn constants<'a>(texts: impl IntoIterator<Item = &'a String>) -> IndexMap<String, i64> {
    let mut found = IndexMap::new();
    for text in texts {
        for capture in constant_pattern().captures_iter(text) {
            let name = capture.get(1).or_else(|| capture.get(2)).unwrap().as_str();
            if let Some(value) = c2_py::parse_int(&capture[3]) {
                found.insert(name.to_string(), value);
            }
        }
    }
    found
}

/// The values of one enum the headers declare, such as the program numbers of enum program.
pub fn enum_values<'a>(texts: impl IntoIterator<Item = &'a String>, enum_name: &str) -> BTreeSet<i64> {
    let pattern = Regex::new(&format!(r"(?s)enum\s+{}\s*\{{(.*?)\}}", regex::escape(enum_name))).unwrap();
    let mut values = BTreeSet::new();
    for text in texts {
        for capture in pattern.captures_iter(text) {
            let body = capture[1].to_string();
            values.extend(constants([&body]).into_values());
        }
    }
    values
}

/// The SDK, each definition set's include folder and the tree's own include folder (disc.h).
pub fn tree_headers(tree: &Path, defs: &Definitions, sdk_dir: &Path) -> Result<IndexMap<String, String>> {
    let mut texts = sdk::header_texts(defs, sdk_dir)?;
    let include = tree.join("include");
    if include.is_dir() {
        texts.extend(sdk::load_texts(&include)?);
    }
    Ok(texts)
}

/// Whether a quote at a position opens or closes a string (one not escaped by the byte before).
fn is_quote(bytes: &[u8], position: usize) -> bool {
    bytes[position] == b'"' && (position == 0 || bytes[position - 1] != b'\\')
}

pub fn without_comments(text: &str) -> String {
    let lines: Vec<&str> = text
        .split('\n')
        .map(|line| {
            let bytes = line.as_bytes();
            let mut quoted = false;
            for position in 0..bytes.len() {
                if is_quote(bytes, position) {
                    quoted = !quoted;
                } else if !quoted && bytes[position..].starts_with(b"//") {
                    return &line[..position];
                }
            }
            line
        })
        .collect();
    lines.join("\n")
}

/// Whether the end of this text (the start of a line up to a position) is inside a string literal.
pub fn in_string(text: &str) -> bool {
    let bytes = text.as_bytes();
    (0..bytes.len()).filter(|&position| is_quote(bytes, position)).count() % 2 == 1
}

pub fn split_args(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let (mut args, mut depth, mut quoted, mut start) = (Vec::new(), 0, false, 0);
    for position in 0..bytes.len() {
        if is_quote(bytes, position) {
            quoted = !quoted;
        } else if quoted {
            continue;
        } else if bytes[position] == b'(' {
            depth += 1;
        } else if bytes[position] == b')' {
            depth -= 1;
        } else if bytes[position] == b',' && depth == 0 {
            args.push(c2_py::strip(&text[start..position]).to_string());
            start = position + 1;
        }
    }
    let last = c2_py::strip(&text[start..]).to_string();
    if !last.is_empty() || !args.is_empty() {
        args.push(last);
    }
    args
}

pub fn closing_paren(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let (mut depth, mut quoted) = (0, false);
    for position in start..bytes.len() {
        // the byte before the first one is the text's last, as Python's text[-1]
        let before = if position == 0 { bytes[bytes.len() - 1] } else { bytes[position - 1] };
        if bytes[position] == b'"' && before != b'\\' {
            quoted = !quoted;
        } else if quoted {
            continue;
        } else if bytes[position] == b'(' {
            depth += 1;
        } else if bytes[position] == b')' {
            depth -= 1;
            if depth == 0 {
                return Some(position);
            }
        }
    }
    None
}

/// A folded constant: an integer or a string literal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Folded {
    Int(i128),
    Text(String),
}

/// The value of a constant expression as the compiler would fold it: integer literals, header
/// constants, + - * / % & | << >> and parentheses, or a string literal. None when it depends on a
/// variable or anything else. (The Python tools read the expression as Python; this reads the
/// part of Python's expression syntax that can fold to a value, which gives the same values.)
pub fn value(expr: &str, consts: &IndexMap<String, i64>) -> Option<Folded> {
    let tokens = pyexpr::tokens(c2_py::strip(expr))?;
    let mut parser = pyexpr::Parser { tokens, position: 0, consts };
    let found = parser.or_expr()?;
    if parser.position != parser.tokens.len() {
        return None;
    }
    found
}

mod pyexpr {
    //! The part of Python's expression grammar index::value folds, with Python's precedence:
    //! | then & then << >> then + - then * / % then unary - (the rest of Python's operators,
    //! which fold to nothing, are refused, as is anything else).

    use indexmap::IndexMap;

    use super::Folded;

    #[derive(Clone, Debug, PartialEq)]
    pub enum Token {
        Int(i128),
        Text(String),
        Name(String),
        Op(&'static str),
        Open,
        Close,
    }

    fn string_literal(chars: &[char], at: &mut usize) -> Option<String> {
        let quote = chars[*at];
        let triple = chars.get(*at + 1) == Some(&quote) && chars.get(*at + 2) == Some(&quote);
        let width = if triple { 3 } else { 1 };
        *at += width;
        let mut out = String::new();
        loop {
            let c = *chars.get(*at)?;
            if c == quote && (!triple || (chars.get(*at + 1) == Some(&quote) && chars.get(*at + 2) == Some(&quote))) {
                *at += width;
                return Some(out);
            }
            if c == '\n' && !triple {
                return None;
            }
            if c != '\\' {
                out.push(c);
                *at += 1;
                continue;
            }
            let escape = *chars.get(*at + 1)?;
            *at += 2;
            match escape {
                '\n' => {}
                '\\' => out.push('\\'),
                '\'' => out.push('\''),
                '"' => out.push('"'),
                'a' => out.push('\u{7}'),
                'b' => out.push('\u{8}'),
                'f' => out.push('\u{c}'),
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                'v' => out.push('\u{b}'),
                'x' | 'u' | 'U' => {
                    let count = match escape {
                        'x' => 2,
                        'u' => 4,
                        _ => 8,
                    };
                    let digits: String = chars.get(*at..*at + count)?.iter().collect();
                    let value = u32::from_str_radix(&digits, 16).ok()?;
                    out.push(char::from_u32(value)?);
                    *at += count;
                }
                '0'..='7' => {
                    let mut value = escape.to_digit(8)?;
                    for _ in 0..2 {
                        match chars.get(*at).and_then(|c| c.to_digit(8)) {
                            Some(digit) => {
                                value = value * 8 + digit;
                                *at += 1;
                            }
                            None => break,
                        }
                    }
                    out.push(char::from_u32(value)?);
                }
                'N' => return None,
                other => {
                    // an unknown escape keeps its backslash
                    out.push('\\');
                    out.push(other);
                }
            }
        }
    }

    /// A raw string: backslashes kept, though one still stops a quote from ending it.
    fn raw_literal(chars: &[char], at: &mut usize) -> Option<String> {
        let quote = chars[*at];
        let triple = chars.get(*at + 1) == Some(&quote) && chars.get(*at + 2) == Some(&quote);
        let width = if triple { 3 } else { 1 };
        *at += width;
        let mut out = String::new();
        loop {
            let c = *chars.get(*at)?;
            if c == quote && (!triple || (chars.get(*at + 1) == Some(&quote) && chars.get(*at + 2) == Some(&quote))) {
                *at += width;
                return Some(out);
            }
            if c == '\n' && !triple {
                return None;
            }
            out.push(c);
            *at += 1;
            if c == '\\' {
                out.push(*chars.get(*at)?);
                *at += 1;
            }
        }
    }

    pub fn tokens(text: &str) -> Option<Vec<Token>> {
        let chars: Vec<char> = text.chars().collect();
        let mut out = Vec::new();
        let mut at = 0;
        while at < chars.len() {
            let c = chars[at];
            if c == ' ' || c == '\t' || c == '\x0c' {
                at += 1;
            } else if c == '"' || c == '\'' {
                let text = string_literal(&chars, &mut at)?;
                // adjacent literals join
                match out.last_mut() {
                    Some(Token::Text(previous)) => previous.push_str(&text),
                    _ => out.push(Token::Text(text)),
                }
            } else if c.is_ascii_digit() || (c == '.' && chars.get(at + 1).is_some_and(char::is_ascii_digit)) {
                let start = at;
                while at < chars.len() && (chars[at].is_alphanumeric() || chars[at] == '_' || chars[at] == '.') {
                    if (chars[at] == 'e' || chars[at] == 'E')
                        && matches!(chars.get(at + 1), Some('+') | Some('-'))
                        && !chars[start..at].iter().any(|c| matches!(c, 'x' | 'X' | 'b' | 'B' | 'o' | 'O'))
                    {
                        at += 1;
                    }
                    at += 1;
                }
                let literal: String = chars[start..at].iter().collect();
                // a float or imaginary literal folds to nothing
                let value = c2_py::parse_int(&literal);
                if value.is_none() || literal.contains(['.', 'j', 'J']) {
                    return None;
                }
                out.push(Token::Int(value? as i128));
            } else if c.is_alphabetic() || c == '_' {
                let start = at;
                while at < chars.len() && (chars[at].is_alphanumeric() || chars[at] == '_') {
                    at += 1;
                }
                let name: String = chars[start..at].iter().collect();
                // a u or r prefix still makes a str; other prefixes (b, f) and keywords make other
                // kinds of expression
                if matches!(chars.get(at), Some('"') | Some('\'')) && matches!(name.as_str(), "u" | "U" | "r" | "R") {
                    let text = if matches!(name.as_str(), "r" | "R") {
                        raw_literal(&chars, &mut at)?
                    } else {
                        string_literal(&chars, &mut at)?
                    };
                    match out.last_mut() {
                        Some(Token::Text(previous)) => previous.push_str(&text),
                        _ => out.push(Token::Text(text)),
                    }
                    continue;
                }
                if matches!(chars.get(at), Some('"') | Some('\''))
                    || matches!(
                        name.as_str(),
                        "and"
                            | "or"
                            | "not"
                            | "if"
                            | "else"
                            | "lambda"
                            | "in"
                            | "is"
                            | "None"
                            | "True"
                            | "False"
                            | "await"
                            | "yield"
                    )
                {
                    return None;
                }
                out.push(Token::Name(name));
            } else if c == '(' {
                out.push(Token::Open);
                at += 1;
            } else if c == ')' {
                out.push(Token::Close);
                at += 1;
            } else {
                let rest: String = chars[at..chars.len().min(at + 2)].iter().collect();
                let op = ["<<", ">>", "**", "//"].into_iter().find(|op| rest.starts_with(op)).or_else(|| {
                    [
                        "+", "-", "*", "/", "%", "&", "|", "^", "~", "<", ">", "@", "=", "!", ",", "[", "]", "{", "}",
                        ".", ":",
                    ]
                    .into_iter()
                    .find(|op| rest.starts_with(op))
                })?;
                out.push(Token::Op(op));
                at += op.len();
            }
        }
        Some(out)
    }

    pub struct Parser<'a> {
        pub tokens: Vec<Token>,
        pub position: usize,
        pub consts: &'a IndexMap<String, i64>,
    }

    /// An expression's value: None for one that parses but does not fold.
    type Value = Option<Folded>;

    fn ints(left: Value, right: Value) -> Option<(i128, i128)> {
        match (left?, right?) {
            (Folded::Int(left), Folded::Int(right)) => Some((left, right)),
            _ => None,
        }
    }

    impl Parser<'_> {
        fn peek_op(&self) -> Option<&'static str> {
            match self.tokens.get(self.position) {
                Some(Token::Op(op)) => Some(op),
                _ => None,
            }
        }

        /// Binary levels from loosest: returns None when the text does not parse.
        pub fn or_expr(&mut self) -> Option<Value> {
            let mut left = self.and_expr()?;
            while self.peek_op() == Some("|") {
                self.position += 1;
                let right = self.and_expr()?;
                left = ints(left, right).map(|(a, b)| Folded::Int(a | b));
            }
            Some(left)
        }

        fn and_expr(&mut self) -> Option<Value> {
            let mut left = self.shift_expr()?;
            while self.peek_op() == Some("&") {
                self.position += 1;
                let right = self.shift_expr()?;
                left = ints(left, right).map(|(a, b)| Folded::Int(a & b));
            }
            Some(left)
        }

        fn shift_expr(&mut self) -> Option<Value> {
            let mut left = self.sum()?;
            while let Some(op @ ("<<" | ">>")) = self.peek_op() {
                self.position += 1;
                let right = self.sum()?;
                left = ints(left, right).and_then(|(a, b)| {
                    let shift = u32::try_from(b).ok()?;
                    Some(Folded::Int(if op == "<<" { a.checked_shl(shift)? } else { a >> shift.min(127) }))
                });
            }
            Some(left)
        }

        fn sum(&mut self) -> Option<Value> {
            let mut left = self.term()?;
            while let Some(op @ ("+" | "-")) = self.peek_op() {
                self.position += 1;
                let right = self.term()?;
                left = ints(left, right).map(|(a, b)| Folded::Int(if op == "+" { a + b } else { a - b }));
            }
            Some(left)
        }

        fn term(&mut self) -> Option<Value> {
            let mut left = self.unary()?;
            while let Some(op @ ("*" | "/" | "%")) = self.peek_op() {
                self.position += 1;
                let right = self.unary()?;
                left = ints(left, right).and_then(|(a, b)| match op {
                    "*" => Some(Folded::Int(a * b)),
                    // int(a / b): true division, then towards zero
                    "/" => (b != 0).then(|| Folded::Int((a as f64 / b as f64) as i128)),
                    // Python's % takes the sign of the divisor
                    _ => (b != 0).then(|| Folded::Int(((a % b) + b) % b)),
                });
            }
            Some(left)
        }

        fn unary(&mut self) -> Option<Value> {
            match self.peek_op() {
                Some("-") => {
                    self.position += 1;
                    let inner = self.unary()?;
                    Some(match inner {
                        Some(Folded::Int(value)) => Some(Folded::Int(-value)),
                        _ => None,
                    })
                }
                Some("+") | Some("~") => {
                    self.position += 1;
                    self.unary()?;
                    Some(None)
                }
                _ => self.atom(),
            }
        }

        fn atom(&mut self) -> Option<Value> {
            let token = self.tokens.get(self.position)?.clone();
            self.position += 1;
            let found = match token {
                Token::Int(value) => Some(Folded::Int(value)),
                Token::Text(text) => Some(Folded::Text(text)),
                Token::Name(name) => self.consts.get(&name).map(|value| Folded::Int(*value as i128)),
                Token::Open => {
                    let inner = self.or_expr()?;
                    if self.tokens.get(self.position) != Some(&Token::Close) {
                        return None;
                    }
                    self.position += 1;
                    inner
                }
                _ => return None,
            };
            // a call, subscript, attribute or power makes something that does not fold
            if matches!(self.tokens.get(self.position), Some(Token::Open) | Some(Token::Op("[" | "." | "**"))) {
                return None;
            }
            Some(found)
        }
    }
}

/// A file's text without comments, read once per run however many questions ask about it.
pub fn code(path: &Path) -> Result<Arc<String>> {
    type Cache = Mutex<HashMap<(PathBuf, u128), Arc<String>>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let modified = std::fs::metadata(path)?
        .modified()
        .map(|time| time.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos())
        .unwrap_or(0);
    let key = (path.to_path_buf(), modified);
    let cache = CACHE.get_or_init(Default::default);
    if let Some(found) = cache.lock().unwrap().get(&key) {
        return Ok(found.clone());
    }
    let text = Arc::new(without_comments(&c2_py::read_text(path)?));
    cache.lock().unwrap().insert(key, text.clone());
    Ok(text)
}

/// Language variants of one script (s51_1.c to s51_5.c) share a key, so reports can list them
/// once.
pub fn variant_key(path: &str) -> String {
    static CELL: OnceLock<Regex> = OnceLock::new();
    regex(&CELL, r"_[0-9](\.[^./]+)$").replace(path, "_N$1").into_owned()
}

/// The tree's source files, by path from it, sorted.
pub fn source_files(tree: &Path) -> Vec<String> {
    sdk::files_under(tree, &source::EXTENSION[1..])
}

pub fn is_library(path: &str) -> bool {
    path.starts_with("lib/")
}

/// A call to one of the names: its file, line, enclosing function and argument texts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Call {
    pub path: String,
    pub line: usize,
    pub function: Option<String>,
    pub name: String,
    pub args: Vec<String>,
    pub text: String,
    pub meant: Option<String>,
}

/// One record per call to any of names.
pub fn calls(tree: &Path, names: &BTreeSet<String>) -> Result<Vec<Call>> {
    let alternatives: Vec<String> = names.iter().map(|name| regex::escape(name)).collect();
    let pattern = Regex::new(&format!(r"\b({})\s*\(", alternatives.join("|"))).unwrap();
    let mut found = Vec::new();
    for relative in source_files(tree) {
        let text = code(&tree.join(&relative))?;
        if !pattern.is_match(&text) {
            continue;
        }
        let definitions_at: Vec<(usize, String)> = definition_pattern()
            .captures_iter(&text)
            .map(|capture| (capture.get(0).unwrap().start(), capture[1].to_string()))
            .collect();
        for capture in pattern.captures_iter(&text) {
            let whole = capture.get(0).unwrap();
            let Some(end) = closing_paren(&text, whole.end() - 1) else {
                continue;
            };
            let line_start = text[..whole.start()].rfind('\n').map_or(0, |at| at + 1);
            let before = &text[line_start..whole.end()];
            if definition_pattern().find(before).is_some_and(|found| found.start() == 0)
                || in_string(&text[line_start..whole.start()])
            {
                continue;
            }
            let function = definitions_at
                .iter()
                .take_while(|(position, _)| *position <= whole.start())
                .last()
                .map(|(_, name)| name.clone());
            let line_end = text[end..].find('\n').map_or(text.len(), |at| end + at);
            let meant = old_numbering().captures(&text[end..line_end]).map(|found| found[1].to_string());
            found.push(Call {
                path: relative.clone(),
                line: text[..whole.start()].matches('\n').count() + 1,
                function,
                name: capture[1].to_string(),
                args: split_args(&text[whole.end()..end]),
                text: text[whole.start()..=end].to_string(),
                meant,
            });
        }
    }
    Ok(found)
}

/// A file's PROGRAM lines and includes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outline {
    /// (number as written, function).
    pub programs: Vec<(String, String)>,
    pub includes: Vec<String>,
}

/// Every file's outline, by path from the tree.
pub fn outlines(tree: &Path) -> Result<IndexMap<String, Outline>> {
    let mut found = IndexMap::new();
    for relative in source_files(tree) {
        let text = code(&tree.join(&relative))?;
        let programs = program_pattern()
            .captures_iter(&text)
            .map(|capture| (c2_py::strip(&capture[1]).to_string(), capture[2].to_string()))
            .collect();
        let includes = include_pattern().captures_iter(&text).map(|capture| capture[1].to_string()).collect();
        found.insert(relative, Outline { programs, includes });
    }
    Ok(found)
}

/// For each library, the scripts that include it, directly or through other libraries.
pub fn includers(outline: &IndexMap<String, Outline>) -> BTreeMap<String, Vec<String>> {
    let mut direct: IndexMap<&String, HashSet<&String>> = IndexMap::new();
    for (path, entry) in outline {
        for included in &entry.includes {
            direct.entry(included).or_default().insert(path);
        }
    }
    let mut result = BTreeMap::new();
    for library in direct.keys() {
        let mut seen: HashSet<&String> = HashSet::new();
        let mut pending = vec![*library];
        while let Some(next) = pending.pop() {
            for user in direct.get(next).into_iter().flatten() {
                if seen.insert(user) {
                    pending.push(user);
                }
            }
        }
        let mut users: Vec<String> = seen.into_iter().filter(|user| !is_library(user)).cloned().collect();
        users.sort();
        result.insert((*library).clone(), users);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_like_python() {
        let consts = IndexMap::from([("STORY".to_string(), 50), ("FLAG".to_string(), 3)]);
        let cases = [
            ("1 + 2 * 3", Some(Folded::Int(7))),
            ("(1 + 2) * 3", Some(Folded::Int(9))),
            ("1 | 2 << 3", Some(Folded::Int(17))),
            ("STORY + 1", Some(Folded::Int(51))),
            ("-7 % 3", Some(Folded::Int(2))),
            ("7 / -2", Some(Folded::Int(-3))),
            ("0x10 - 1", Some(Folded::Int(15))),
            ("\"e50_1.txt\"", Some(Folded::Text("e50_1.txt".into()))),
            ("\"a\" \"b\"", Some(Folded::Text("ab".into()))),
            ("x + 1", None),
            ("1 ^ 2", None),
            ("f(1)", None),
            ("1 +", None),
            ("1.5", None),
            ("2 ** 3", None),
            ("~1", None),
        ];
        for (text, expected) in cases {
            assert_eq!(value(text, &consts), expected, "{text}");
        }
    }

    #[test]
    fn splits_arguments() {
        assert_eq!(split_args(r#" 1, f(2, 3), "a, b" "#), vec!["1", "f(2, 3)", "\"a, b\""]);
        assert_eq!(split_args("  "), Vec::<String>::new());
    }
}
