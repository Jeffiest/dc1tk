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
        r"(?m)^\s*(?:#define\s+([A-Za-z_][A-Za-z0-9_]*)\s+|([A-Za-z_][A-Za-z0-9_]*)\s*=\s*)(-?(?:0x[0-9A-Fa-f]+|[0-9]+))\s*,?\s*(?://.*)?$",
    )
}

fn definition_pattern() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    regex(&CELL, r"(?m)^(?:void|int|float|string)\s+\**([A-Za-z0-9_]+)\s*\(")
}

fn program_pattern() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    regex(&CELL, r"(?m)^PROGRAM\(\s*([^,]+?)\s*,\s*([A-Za-z0-9_]+)\s*\);")
}

pub fn include_pattern() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    regex(&CELL, r#"(?m)^#include\s+"([^"]+)""#)
}

/// The decompiler's comment on a call written for an older native table: the engine runs the
/// native the call names, but the script meant another.
fn old_numbering() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    regex(&CELL, r"//.*\bold numbering, meant as ([A-Za-z0-9_]+)")
}

/// Every integer constant the headers define: enum members (including program names such as
/// story_50) and #defines.
pub fn constants<'a>(texts: impl IntoIterator<Item = &'a String>) -> IndexMap<String, i64> {
    let mut found = IndexMap::new();
    for text in texts {
        for capture in constant_pattern().captures_iter(text) {
            let name = capture.get(1).or_else(|| capture.get(2)).unwrap().as_str();
            if let Some(value) = integer(&capture[3]) {
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
            args.push(text[start..position].trim().to_string());
            start = position + 1;
        }
    }
    let last = text[start..].trim().to_string();
    if !last.is_empty() || !args.is_empty() {
        args.push(last);
    }
    args
}

pub fn closing_paren(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let (mut depth, mut quoted) = (0, false);
    for position in start..bytes.len() {
        if is_quote(bytes, position) {
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

/// The value of a constant expression as C folds it: integer and character constants, header
/// constants, and parentheses, with C's arithmetic, bitwise, shift, comparison and logical
/// operators at C's precedence (parse::precedence); or string literals, adjacent ones joined. None
/// when it depends on a variable or anything else, or does not read as C.
pub fn value(expr: &str, consts: &IndexMap<String, i64>) -> Option<Folded> {
    let tokens = constant::tokens(expr)?;
    let mut folder = constant::Folder { tokens, position: 0, consts };
    let found = folder.binary(0)?;
    if folder.position != folder.tokens.len() {
        return None;
    }
    found
}

/// An integer constant as C writes one: decimal, 0x hexadecimal or 0 octal, with any u and l
/// suffixes, after an optional minus; None for anything else, or one beyond an i64.
pub fn integer(text: &str) -> Option<i64> {
    let (negative, unsigned) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let value = constant::integer(unsigned)?;
    i64::try_from(if negative { -value } else { value }).ok()
}

mod constant {
    //! Reading and folding the constant expressions index::value takes, by C's rules.

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

    /// The operators that fold, the two-character ones first so they are read whole.
    const OPERATORS: [&str; 20] =
        ["<<", ">>", "<=", ">=", "==", "!=", "&&", "||", "+", "-", "*", "/", "%", "&", "|", "^", "~", "!", "<", ">"];

    /// An integer constant without a sign: decimal, 0x hexadecimal or 0 octal, with any u and l
    /// suffixes.
    pub fn integer(word: &str) -> Option<i128> {
        let digits = word.trim_end_matches(['u', 'U', 'l', 'L']);
        let (radix, digits) = match digits.strip_prefix("0x").or_else(|| digits.strip_prefix("0X")) {
            Some(hexadecimal) => (16, hexadecimal),
            None if digits.len() > 1 && digits.starts_with('0') => (8, &digits[1..]),
            None => (10, digits),
        };
        if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
            return None;
        }
        i128::from_str_radix(digits, radix).ok()
    }

    /// A string literal from its opening quote: its text with C's escapes read, and where it ends.
    fn string_literal(chars: &[char], mut at: usize) -> Option<(String, usize)> {
        at += 1;
        let mut out = String::new();
        loop {
            match *chars.get(at)? {
                '"' => return Some((out, at + 1)),
                '\n' => return None,
                '\\' => {
                    let (value, length) = crate::cpp::escape_value(&chars[at + 1..])?;
                    out.push(char::from_u32(value)?);
                    at += 1 + length;
                }
                c => {
                    out.push(c);
                    at += 1;
                }
            }
        }
    }

    /// Where a run of characters that pass ends.
    fn run_end(chars: &[char], at: usize, passes: impl Fn(char) -> bool) -> usize {
        chars[at..].iter().position(|&c| !passes(c)).map_or(chars.len(), |length| at + length)
    }

    pub fn tokens(text: &str) -> Option<Vec<Token>> {
        let chars: Vec<char> = text.chars().collect();
        let mut out = Vec::new();
        let mut at = 0;
        while at < chars.len() {
            let c = chars[at];
            if c.is_whitespace() {
                at += 1;
            } else if c == '"' {
                let (text, end) = string_literal(&chars, at)?;
                at = end;
                match out.last_mut() {
                    Some(Token::Text(previous)) => previous.push_str(&text),
                    _ => out.push(Token::Text(text)),
                }
            } else if c == '\'' {
                let (value, length) = crate::cpp::character_constant(&chars[at..]).ok()?;
                out.push(Token::Int(value as i128));
                at += length;
            } else if c.is_ascii_digit() || c == '.' {
                // a number runs on through letters and points, so a float does not read as an int
                let end = run_end(&chars, at, |c| c.is_ascii_alphanumeric() || c == '.' || c == '_');
                let word: String = chars[at..end].iter().collect();
                out.push(Token::Int(integer(&word)?));
                at = end;
            } else if c.is_ascii_alphabetic() || c == '_' {
                let end = run_end(&chars, at, |c| c.is_ascii_alphanumeric() || c == '_');
                out.push(Token::Name(chars[at..end].iter().collect()));
                at = end;
            } else if c == '(' {
                out.push(Token::Open);
                at += 1;
            } else if c == ')' {
                out.push(Token::Close);
                at += 1;
            } else {
                let next: String = chars[at..chars.len().min(at + 2)].iter().collect();
                let op = OPERATORS.into_iter().find(|op| next.starts_with(op))?;
                out.push(Token::Op(op));
                at += op.len();
            }
        }
        Some(out)
    }

    pub struct Folder<'a> {
        pub tokens: Vec<Token>,
        pub position: usize,
        pub consts: &'a IndexMap<String, i64>,
    }

    /// An expression's value: None for one that reads but does not fold.
    type Value = Option<Folded>;

    fn ints(left: Value, right: Value) -> Option<(i128, i128)> {
        match (left?, right?) {
            (Folded::Int(left), Folded::Int(right)) => Some((left, right)),
            _ => None,
        }
    }

    /// a op b, as C folds it: / and % towards zero; None for a division by zero or an overflow.
    fn apply(op: &str, a: i128, b: i128) -> Option<i128> {
        let truth = |holds: bool| holds as i128;
        Some(match op {
            "*" => a.checked_mul(b)?,
            "/" => a.checked_div(b)?,
            "%" => a.checked_rem(b)?,
            "+" => a.checked_add(b)?,
            "-" => a.checked_sub(b)?,
            "<<" => a.checked_shl(u32::try_from(b).ok()?)?,
            ">>" => a >> u32::try_from(b).ok()?.min(127),
            "<" => truth(a < b),
            "<=" => truth(a <= b),
            ">" => truth(a > b),
            ">=" => truth(a >= b),
            "==" => truth(a == b),
            "!=" => truth(a != b),
            "&" => a & b,
            "^" => a ^ b,
            "|" => a | b,
            "&&" => truth(a != 0 && b != 0),
            "||" => truth(a != 0 || b != 0),
            _ => return None,
        })
    }

    impl Folder<'_> {
        fn peek_op(&self) -> Option<&'static str> {
            match self.tokens.get(self.position) {
                Some(Token::Op(op)) => Some(op),
                _ => None,
            }
        }

        /// A binary operator next, binding at least as tightly as lowest, and its precedence.
        fn binary_op(&self, lowest: u32) -> Option<(&'static str, u32)> {
            let op = self.peek_op()?;
            crate::parse::precedence(op).filter(|&level| level >= lowest).map(|level| (op, level))
        }

        /// The operators binding at least as tightly as lowest, left to right; None when the text
        /// does not read.
        pub fn binary(&mut self, lowest: u32) -> Option<Value> {
            let mut left = self.unary()?;
            while let Some((op, level)) = self.binary_op(lowest) {
                self.position += 1;
                let right = self.binary(level + 1)?;
                left = ints(left, right).and_then(|(a, b)| apply(op, a, b)).map(Folded::Int);
            }
            Some(left)
        }

        fn unary(&mut self) -> Option<Value> {
            let Some(op @ ("-" | "+" | "~" | "!")) = self.peek_op() else {
                return self.atom();
            };
            self.position += 1;
            Some(match self.unary()? {
                Some(Folded::Int(value)) => Some(Folded::Int(match op {
                    "-" => -value,
                    "+" => value,
                    "~" => !value,
                    _ => (value == 0) as i128,
                })),
                _ => None,
            })
        }

        fn atom(&mut self) -> Option<Value> {
            let token = self.tokens.get(self.position)?.clone();
            self.position += 1;
            let found = match token {
                Token::Int(value) => Some(Folded::Int(value)),
                Token::Text(text) => Some(Folded::Text(text)),
                Token::Name(name) => self.consts.get(&name).map(|value| Folded::Int(*value as i128)),
                Token::Open => {
                    let inner = self.binary(0)?;
                    if self.tokens.get(self.position) != Some(&Token::Close) {
                        return None;
                    }
                    self.position += 1;
                    inner
                }
                _ => return None,
            };
            // a call makes something that does not fold
            if self.tokens.get(self.position) == Some(&Token::Open) {
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
    let text = Arc::new(without_comments(&dc2tk_disc::textfile::read_text(path)?));
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
            .map(|capture| (capture[1].trim().to_string(), capture[2].to_string()))
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
    fn constant_expressions_fold_as_c_folds_them() {
        let consts = IndexMap::from([("STORY".to_string(), 50), ("FLAG".to_string(), 3)]);
        let cases = [
            ("1 + 2 * 3", Some(Folded::Int(7))),
            ("(1 + 2) * 3", Some(Folded::Int(9))),
            ("1 | 2 << 3", Some(Folded::Int(17))),
            ("6 ^ 3 & 1", Some(Folded::Int(7))),
            (" STORY + 1 ", Some(Folded::Int(51))),
            ("-7 % 3", Some(Folded::Int(-1))),
            ("7 / -2", Some(Folded::Int(-3))),
            ("0x10 - 1", Some(Folded::Int(15))),
            ("010 + 1u", Some(Folded::Int(9))),
            ("'A' + 1", Some(Folded::Int(66))),
            ("~1", Some(Folded::Int(-2))),
            ("!FLAG || 2 < 3", Some(Folded::Int(1))),
            ("\"e50_1.txt\"", Some(Folded::Text("e50_1.txt".into()))),
            ("\"a\\n\" \"b\"", Some(Folded::Text("a\nb".into()))),
            ("x + 1", None),
            ("\"a\" + 1", None),
            ("f(1)", None),
            ("1 +", None),
            ("1.5", None),
            ("1e3", None),
            ("1 / 0", None),
            ("2 ** 3", None),
            ("a = 1", None),
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

    #[test]
    fn header_constants_are_read_as_c_integers() {
        let header = "#define A 0x10\n#define B -3\n    C = 010,\n    D = 12, // twelve\n#define E 1.5\n".to_string();
        let found = constants([&header]);
        assert_eq!(
            found.into_iter().collect::<Vec<_>>(),
            [("A".into(), 16), ("B".into(), -3), ("C".into(), 8), ("D".into(), 12)]
        );
        assert_eq!(integer("9223372036854775808"), None);
        assert_eq!(integer("0x"), None);
    }
}
