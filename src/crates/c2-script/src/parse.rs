//! The C-like script language: the tokenizer (comments, #if/#ifdef/#ifndef/#elif/#else/#endif,
//! #define/#undef, #include, #pragma, asm blocks) and the recursive-descent parser that turns a
//! file into a program tree, as parse.py does, key for key.
//!
//! Where parse.py keeps a #pragma's words as asm.tokenize gives them, a plain word is a string and
//! a quoted one {"string": text}.
//!
//! Errors carry parse.py's ValueError messages. Where parse.py fails with another exception (a
//! token with no text quoted in a message, a bad string in a #pragma, a #pragma missing its value,
//! recursion past Python's limit), this fails too, with its own message. Python's integers have no
//! limit; here an integer in the tree past 64 bits, or one in an #if condition past 128, is an
//! error.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use c2_py::{ascii_digits, int_base, is_space, is_word, repr, split, strip, IntError};

use crate::node::Node;
use crate::{asm, stb, Error, Result};

/// How parse.py fails: with a ValueError (its message), or with another exception, which nothing
/// in parse.py catches.
#[derive(Debug)]
enum Fault {
    Value(String),
    Crash(String),
}

impl From<Fault> for Error {
    fn from(fault: Fault) -> Error {
        match fault {
            Fault::Value(message) | Fault::Crash(message) => Error::invalid(message),
        }
    }
}

type Parsed<T> = std::result::Result<T, Fault>;

/// Python's default recursion limit: nesting deep enough that parse.py's functions would hold more
/// frames fails here too, where Python raises RecursionError. Every function parse.py calls on the
/// way down is counted; CALLER_FRAMES are the frames below parse.parse, as the old parity check called it
/// (the point moves by one for each frame another caller has more or fewer).
const PYTHON_FRAMES: usize = 1000;
const CALLER_FRAMES: usize = 4;

const TYPES: [&str; 3] = ["int", "float", "string"];
/// Words that start a statement, which a value kind's name can never be.
const STATEMENT_WORDS: [&str; 22] = [
    "return", "if", "else", "while", "for", "do", "switch", "case", "default", "break", "continue", "goto", "wait",
    "end", "skip_end", "print", "sizeof", "AT", "DIRECT", "void", "const", "static",
];
const CAST_TYPES: [&str; 2] = ["int", "float"];
const RETURN_TYPES: [&str; 4] = ["void", "int", "float", "string"];
const NATIVE_RETURN_TYPES: [&str; 6] = ["void", "int", "float", "string", "string", "any"];
const NATIVE_TYPES: [&str; 4] = ["int", "float", "string", "any"];
const CONDITIONALS: [&str; 6] = ["if", "ifdef", "ifndef", "elif", "else", "endif"];
const SIMPLE: [&str; 5] = ["wait", "end", "skip_end", "break", "continue"];
const BUILTINS: [&str; 2] = ["sin", "cos"];
/// What a mod patch's #pragma target can pick scripts by.
pub const TARGET_KINDS: [&str; 4] = ["map", "script", "monster", "library"];
const LONG_OPERATORS: [&str; 3] = ["...", "<<=", ">>="];
const OPERATORS: [&str; 18] =
    ["==", "!=", "<=", ">=", "&&", "||", "<<", ">>", "++", "--", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^="];
const SINGLES: &str = "(){}[],;=<>+-*/%&|!^~?:";
const NOTE: &str = "note:";

fn precedence(op: &str) -> Option<u32> {
    Some(match op {
        "||" => 2,
        "&&" => 3,
        "|" => 4,
        "^" => 5,
        "&" => 6,
        "==" | "!=" => 7,
        "<" | "<=" | ">" | ">=" => 8,
        "<<" | ">>" => 9,
        "+" | "-" => 10,
        "*" | "/" | "%" => 11,
        _ => return None,
    })
}

/// The operator of a compound assignment (+= gives +).
fn compound_op(op: &str) -> Option<&'static str> {
    Some(match op {
        "+=" => "+",
        "-=" => "-",
        "*=" => "*",
        "/=" => "/",
        "%=" => "%",
        "&=" => "&",
        "|=" => "|",
        "^=" => "^",
        "<<=" => "<<",
        ">>=" => ">>",
        _ => return None,
    })
}

/// The numbers a mod can ask build for (NEW_FLAG(name);), by what they give.
fn request(word: &str) -> Option<&'static str> {
    Some(match word {
        "NEW_FLAG" => "flag",
        "NEW_PROGRAM" => "program",
        "NEW_MONSTER_VAR" => "monster_var",
        _ => return None,
    })
}

/// A string or character literal's escapes.
fn escape(c: Option<char>) -> Option<char> {
    Some(match c? {
        '\\' => '\\',
        '"' => '"',
        '\'' => '\'',
        'n' => '\n',
        'r' => '\r',
        't' => '\t',
        '0' => '\0',
        _ => return None,
    })
}

/// The escapes of asm.tokenize's quoted words.
fn asm_escape(c: char) -> Option<char> {
    Some(match c {
        '\\' => '\\',
        '"' => '"',
        'n' => '\n',
        'r' => '\r',
        't' => '\t',
        _ => return None,
    })
}

fn lex_fail<T>(line: usize, reason: impl std::fmt::Display) -> Parsed<T> {
    Err(Fault::Value(format!("line {line}: {reason}")))
}

/// repr() as Python's "%.200R" shows it in an error message.
fn short_repr(text: &str) -> String {
    repr(text).chars().take(200).collect()
}

/// int(text, base), failing with Python's message.
fn python_int(text: &str, base: u32) -> Parsed<i128> {
    int_base(text, base).map_err(|error| match error {
        IntError::Invalid => Fault::Value(format!("invalid literal for int() with base {base}: {}", short_repr(text))),
        IntError::TooLarge => Fault::Crash(format!("{text} is too large a number")),
    })
}

/// chr(int(digits, 16)) for a \x escape.
fn hex_escape(digits: &[char]) -> Parsed<char> {
    let digits: String = digits.iter().collect();
    let value = python_int(&digits, 16)?;
    u32::try_from(value)
        .ok()
        .and_then(char::from_u32)
        .ok_or_else(|| Fault::Value("chr() arg not in range(0x110000)".into()))
}

/// A 64-bit integer node value.
fn int_node(value: i128) -> Parsed<Node> {
    i64::try_from(value).map(Node::Int).map_err(|_| Fault::Crash(format!("{value} does not fit in 64 bits")))
}

fn int_literal(value: i128) -> Parsed<Node> {
    Ok(node!({"kind": "int", "value": int_node(value)?}))
}

fn one() -> Node {
    node!({"kind": "int", "value": 1})
}

// ---- characters ----------------------------------------------------------------------------

fn text_of(chars: &[char]) -> String {
    chars.iter().collect()
}

fn starts_with(chars: &[char], position: usize, prefix: &str) -> bool {
    (position..).zip(prefix.chars()).all(|(at, c)| chars.get(at) == Some(&c))
}

fn find(chars: &[char], from: usize, pattern: &str) -> Option<usize> {
    (from..chars.len()).find(|&at| starts_with(chars, at, pattern))
}

fn line_end(chars: &[char], position: usize) -> usize {
    (position..chars.len()).find(|&at| chars[at] == '\n').unwrap_or(chars.len())
}

fn count_lines(chars: &[char], from: usize, to: usize) -> usize {
    chars[from.min(to)..to].iter().filter(|&&c| c == '\n').count()
}

/// text.split("//")[0]
fn before_comment(text: &str) -> &str {
    text.split("//").next().unwrap_or("")
}

/// text.split(None, 1): the first word and the rest (its trailing whitespace kept).
fn split_first_word(text: &str) -> Vec<&str> {
    let rest = text.trim_start_matches(is_space);
    if rest.is_empty() {
        return Vec::new();
    }
    match rest.find(is_space) {
        None => vec![rest],
        Some(at) => {
            let remainder = rest[at..].trim_start_matches(is_space);
            if remainder.is_empty() {
                vec![&rest[..at]]
            } else {
                vec![&rest[..at], remainder]
            }
        }
    }
}

// ---- #if conditions ------------------------------------------------------------------------

/// How a condition fails to fold: it is not one (parse.py's "cannot read the condition"), or it
/// goes where Python raises something else.
enum Unreadable {
    Condition,
    Crash(String),
}

/// The value of an #if or #elif condition: integers, defined(NAME), names #defined to integers
/// (any other name is 0, as in C), ! && || ?, comparisons and arithmetic. As in parse.py, the text
/// becomes a Python expression, read as Python reads it. frames: Python frames in use, this
/// function's included.
fn condition_value(text: &str, defines: &HashMap<String, String>, line: usize, frames: usize) -> Parsed<bool> {
    let mut expression = replace_defined(before_comment(text), defines);
    for _ in 0..16 {
        let replaced = replace_names(&expression, defines);
        if replaced == expression {
            break;
        }
        expression = replaced;
    }
    let expression = strip_suffixes(&expression).replace("&&", " and ").replace("||", " or ");
    let expression = replace_bangs(&expression);
    let source = match strip(&expression) {
        "" => "0",
        source => source,
    };
    match python_condition(source, PYTHON_FRAMES.saturating_sub(frames)) {
        Ok(value) => Ok(value != 0),
        Err(Unreadable::Condition) => lex_fail(line, format!("cannot read the condition {}", repr(strip(text)))),
        Err(Unreadable::Crash(message)) => Err(Fault::Crash(format!("line {line}: {message}"))),
    }
}

/// defined(NAME) and defined NAME become 1 or 0.
fn replace_defined(text: &str, defines: &HashMap<String, String>) -> String {
    let chars: Vec<char> = text.chars().collect();
    let at = |i: usize| chars.get(i).copied();
    let spaces = |mut i: usize| {
        while at(i).is_some_and(is_space) {
            i += 1;
        }
        i
    };
    let word = |mut i: usize| {
        while at(i).is_some_and(is_word) {
            i += 1;
        }
        i
    };
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if starts_with(&chars, i, "defined") {
            let after = i + "defined".len();
            let mut found = None;
            let open = spaces(after);
            if at(open) == Some('(') {
                let start = spaces(open + 1);
                let end = word(start);
                let close = spaces(end);
                if end > start && at(close) == Some(')') {
                    found = Some((start, end, close + 1));
                }
            }
            if found.is_none() && at(after).is_some_and(is_space) {
                let start = spaces(after);
                let end = word(start);
                if end > start {
                    found = Some((start, end, end));
                }
            }
            if let Some((start, end, next)) = found {
                out.push(if defines.contains_key(&text_of(&chars[start..end])) { '1' } else { '0' });
                i = next;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Every name (a word starting with a letter or _) becomes (its #define), or (0).
fn replace_names(text: &str, defines: &HashMap<String, String>) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if (c.is_ascii_alphabetic() || c == '_') && (i == 0 || !is_word(chars[i - 1])) {
            let mut end = i + 1;
            while end < chars.len() && is_word(chars[end]) {
                end += 1;
            }
            let value = defines.get(&text_of(&chars[i..end])).map(String::as_str).filter(|value| !value.is_empty());
            out.push('(');
            out.push_str(value.unwrap_or("0"));
            out.push(')');
            i = end;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// Integer suffixes (u, l) go.
fn strip_suffixes(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let suffix = |mut i: usize| {
        while chars.get(i).is_some_and(|c| "uUlL".contains(*c)) {
            i += 1;
        }
        i
    };
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if i == 0 || !is_word(chars[i - 1]) {
            let mut number = None;
            if chars[i] == '0' && chars.get(i + 1).is_some_and(|c| *c == 'x' || *c == 'X') {
                let mut end = i + 2;
                while chars.get(end).is_some_and(char::is_ascii_hexdigit) {
                    end += 1;
                }
                if end > i + 2 && suffix(end) > end {
                    number = Some((end, suffix(end)));
                }
            }
            if number.is_none() && c2_py::is_decimal(chars[i]) {
                let mut end = i;
                while chars.get(end).is_some_and(|c| c2_py::is_decimal(*c)) {
                    end += 1;
                }
                if suffix(end) > end {
                    number = Some((end, suffix(end)));
                }
            }
            if let Some((end, next)) = number {
                out.extend(&chars[i..end]);
                i = next;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// ! (not before =) becomes " not ".
fn replace_bangs(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    for (i, &c) in chars.iter().enumerate() {
        if c == '!' && chars.get(i + 1) != Some(&'=') {
            out.push_str(" not ");
        } else {
            out.push(c);
        }
    }
    out
}

/// A token of the Python expression a condition becomes: an integer (None past 128 bits), or an
/// operator or and/or/not.
#[derive(Clone, Copy, Debug, PartialEq)]
enum PyToken {
    Int(Option<i128>),
    Op(&'static str),
}

/// The Python operators a condition can fold, longest first.
const PY_OPERATORS: [&str; 18] =
    ["//", "<=", ">=", "==", "!=", "(", ")", "+", "-", "~", "*", "/", "%", "&", "|", "^", "<", ">"];

/// The longest-matching Python operator at i, among those a condition can fold.
fn py_operator(chars: &[char], i: usize) -> Option<(&'static str, usize)> {
    // these Python operators start like the ones above but never fold
    for long in ["**", "<<", ">>", "<>", "->", "//=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^="] {
        if starts_with(chars, i, long) {
            return None;
        }
    }
    PY_OPERATORS.iter().find(|op| starts_with(chars, i, op)).map(|op| (*op, op.len()))
}

/// An integer literal as Python's tokenizer reads it; None for a syntax error or a float.
fn py_number(chars: &[char], i: &mut usize) -> Option<Option<i128>> {
    let at = |k: usize| chars.get(k).copied().unwrap_or('\0');
    let mut value = Some(0i128);
    let add = |value: &mut Option<i128>, radix: u32, c: char| {
        *value = value.and_then(|v| v.checked_mul(radix as i128)?.checked_add(c.to_digit(radix).unwrap() as i128));
    };
    let decimal_tail = |i: &mut usize| -> Option<()> {
        loop {
            while at(*i).is_ascii_digit() {
                *i += 1;
            }
            if at(*i) != '_' {
                return Some(());
            }
            *i += 1;
            if !at(*i).is_ascii_digit() {
                return None;
            }
        }
    };
    let radix = match (at(*i), at(*i + 1)) {
        ('0', 'x' | 'X') => 16,
        ('0', 'o' | 'O') => 8,
        ('0', 'b' | 'B') => 2,
        _ => 10,
    };
    if radix != 10 {
        *i += 2;
        loop {
            if at(*i) == '_' {
                *i += 1;
            }
            if !at(*i).is_digit(radix) {
                return None;
            }
            while at(*i).is_digit(radix) {
                add(&mut value, radix, at(*i));
                *i += 1;
            }
            if at(*i) != '_' {
                break;
            }
        }
        if radix != 16 && at(*i).is_ascii_digit() {
            return None;
        }
        return Some(value);
    }
    let start = *i;
    if at(*i) == '0' {
        while at(*i) == '0' {
            *i += 1;
            if at(*i) == '_' {
                *i += 1;
                if !at(*i).is_ascii_digit() {
                    return None;
                }
            }
        }
        let nonzero = at(*i).is_ascii_digit();
        if nonzero {
            decimal_tail(i)?;
        }
        if ".eEjJ".contains(at(*i)) || nonzero {
            return None;
        }
        return Some(Some(0));
    }
    decimal_tail(i)?;
    if ".eEjJ".contains(at(*i)) {
        return None;
    }
    for &c in &chars[start..*i] {
        if c != '_' {
            add(&mut value, 10, c);
        }
    }
    Some(value)
}

/// The tokens of a condition's Python expression; None when it holds anything that cannot fold
/// (a float, a string, a name, an operator folded_condition does not take, a syntax error).
fn py_tokens(chars: &[char]) -> Option<Vec<PyToken>> {
    let mut tokens = Vec::new();
    let mut level = 0;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if matches!(c, ' ' | '\t' | '\x0c') {
            i += 1;
        } else if c == '#' {
            break;
        } else if c.is_ascii_digit() {
            tokens.push(PyToken::Int(py_number(chars, &mut i)?));
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            tokens.push(match text_of(&chars[start..i]).as_str() {
                "and" => PyToken::Op("and"),
                "or" => PyToken::Op("or"),
                "not" => PyToken::Op("not"),
                "True" => PyToken::Int(Some(1)),
                "False" => PyToken::Int(Some(0)),
                _ => return None,
            });
        } else {
            let (op, length) = py_operator(chars, i)?;
            if op == "(" {
                if level >= 200 {
                    return None;
                }
                level += 1;
            } else if op == ")" {
                if level == 0 {
                    return None;
                }
                level -= 1;
            }
            tokens.push(PyToken::Op(op));
            i += length;
        }
    }
    (level == 0).then_some(tokens)
}

/// A folded condition's tree: what ast.parse gives, for the nodes folded_condition takes. Deep
/// stands for a subtree past Python's recursion limit.
enum PyNode {
    Int(Option<i128>),
    Deep,
    All(Vec<PyNode>),
    Any(Vec<PyNode>),
    Unary(&'static str, Box<PyNode>),
    Binary(&'static str, Box<PyNode>, Box<PyNode>),
}

/// A subtree and its depth.
type Sized = (PyNode, usize);

/// Python's expression grammar, for the part a condition can use.
struct PyParser {
    tokens: Vec<PyToken>,
    pos: usize,
    /// The deepest tree folded_condition can recurse through within Python's recursion limit.
    limit: usize,
}

impl PyParser {
    fn sized(&self, node: PyNode, depth: usize) -> Sized {
        if depth > self.limit {
            (PyNode::Deep, depth)
        } else {
            (node, depth)
        }
    }

    fn eat(&mut self, op: &str) -> bool {
        let found = matches!(self.tokens.get(self.pos), Some(PyToken::Op(o)) if *o == op);
        if found {
            self.pos += 1;
        }
        found
    }

    fn next_op(&self, ops: &[&'static str]) -> Option<&'static str> {
        match self.tokens.get(self.pos) {
            Some(PyToken::Op(op)) => ops.iter().find(|o| **o == *op).copied(),
            _ => None,
        }
    }

    /// operand (word operand)*: one BoolOp, its values all folded.
    fn bool_op(&mut self, word: &'static str, operand: fn(&mut PyParser) -> Option<Sized>) -> Option<Sized> {
        let first = operand(self)?;
        if self.next_op(&[word]).is_none() {
            return Some(first);
        }
        let mut depth = first.1;
        let mut values = vec![first.0];
        while self.eat(word) {
            let (value, value_depth) = operand(self)?;
            depth = depth.max(value_depth);
            values.push(value);
        }
        Some(self.sized(if word == "and" { PyNode::All(values) } else { PyNode::Any(values) }, depth + 1))
    }

    fn disjunction(&mut self) -> Option<Sized> {
        self.bool_op("or", PyParser::conjunction)
    }

    fn conjunction(&mut self) -> Option<Sized> {
        self.bool_op("and", PyParser::inversion)
    }

    fn inversion(&mut self) -> Option<Sized> {
        let mut count = 0;
        while self.eat("not") {
            count += 1;
        }
        let operand = self.comparison()?;
        Some(self.prefixed(vec!["not"; count], operand))
    }

    /// Prefix operators (innermost last) around an operand.
    fn prefixed(&self, ops: Vec<&'static str>, (mut node, depth): Sized) -> Sized {
        let depth = depth + ops.len();
        if depth > self.limit {
            return (PyNode::Deep, depth);
        }
        for op in ops.into_iter().rev() {
            node = PyNode::Unary(op, Box::new(node));
        }
        (node, depth)
    }

    fn comparison(&mut self) -> Option<Sized> {
        let (left, left_depth) = self.bitwise(0)?;
        let Some(op) = self.next_op(&["==", "!=", "<", "<=", ">", ">="]) else {
            return Some((left, left_depth));
        };
        self.pos += 1;
        let (right, right_depth) = self.bitwise(0)?;
        if self.next_op(&["==", "!=", "<", "<=", ">", ">="]).is_some() {
            return None;
        }
        Some(self.sized(PyNode::Binary(op, Box::new(left), Box::new(right)), left_depth.max(right_depth) + 1))
    }

    /// | then ^ then & then + - then * / // %, each left-associative.
    fn bitwise(&mut self, level: usize) -> Option<Sized> {
        const LEVELS: [&[&str]; 5] = [&["|"], &["^"], &["&"], &["+", "-"], &["*", "/", "//", "%"]];
        let operand = |parser: &mut PyParser| {
            if level + 1 < LEVELS.len() {
                parser.bitwise(level + 1)
            } else {
                parser.factor()
            }
        };
        let (mut left, mut depth) = operand(self)?;
        while let Some(op) = self.next_op(LEVELS[level]) {
            self.pos += 1;
            let (right, right_depth) = operand(self)?;
            (left, depth) = self.sized(PyNode::Binary(op, Box::new(left), Box::new(right)), depth.max(right_depth) + 1);
        }
        Some((left, depth))
    }

    fn factor(&mut self) -> Option<Sized> {
        let mut ops = Vec::new();
        while let Some(op) = self.next_op(&["+", "-", "~"]) {
            self.pos += 1;
            ops.push(op);
        }
        let operand = self.atom()?;
        // a call: (1)(2)
        if self.next_op(&["("]).is_some() {
            return None;
        }
        Some(self.prefixed(ops, operand))
    }

    fn atom(&mut self) -> Option<Sized> {
        match self.tokens.get(self.pos).copied()? {
            PyToken::Int(value) => {
                self.pos += 1;
                Some((PyNode::Int(value), 1))
            }
            PyToken::Op("(") => {
                self.pos += 1;
                let inner = self.disjunction()?;
                self.eat(")").then_some(inner)
            }
            PyToken::Op(_) => None,
        }
    }
}

/// ast.parse(source, mode="eval") folded as folded_condition folds it, with room for a tree limit
/// deep.
fn python_condition(source: &str, limit: usize) -> std::result::Result<i128, Unreadable> {
    let chars: Vec<char> = source.chars().collect();
    let tokens = py_tokens(&chars).ok_or(Unreadable::Condition)?;
    let mut parser = PyParser { tokens, pos: 0, limit };
    let (tree, _) = parser.disjunction().ok_or(Unreadable::Condition)?;
    if parser.pos != parser.tokens.len() {
        return Err(Unreadable::Condition);
    }
    fold(&tree)
}

fn fold(node: &PyNode) -> std::result::Result<i128, Unreadable> {
    let too_large = || Unreadable::Crash("a number in the condition is too large".into());
    Ok(match node {
        PyNode::Int(value) => value.ok_or_else(too_large)?,
        PyNode::Deep => return Err(Unreadable::Crash("the condition nests too deeply".into())),
        PyNode::All(values) | PyNode::Any(values) => {
            let folded = values.iter().map(fold).collect::<std::result::Result<Vec<i128>, _>>()?;
            let all = matches!(node, PyNode::All(_));
            (if all { folded.iter().all(|&v| v != 0) } else { folded.iter().any(|&v| v != 0) }) as i128
        }
        PyNode::Unary(op, operand) => {
            let value = fold(operand)?;
            match *op {
                "not" => (value == 0) as i128,
                "-" => value.checked_neg().ok_or_else(too_large)?,
                "+" => value,
                _ => !value,
            }
        }
        PyNode::Binary(op, left, right) => {
            let (left, right) = (fold(left)?, fold(right)?);
            let checked = |value: Option<i128>| value.ok_or_else(too_large);
            match *op {
                "+" => checked(left.checked_add(right))?,
                "-" => checked(left.checked_sub(right))?,
                "*" => checked(left.checked_mul(right))?,
                "&" => left & right,
                "|" => left | right,
                "^" => left ^ right,
                "/" | "//" => {
                    if right == 0 {
                        return Err(Unreadable::Condition);
                    }
                    // int(left / right): true division, then toward zero
                    const EXACT: i128 = 1 << 53;
                    if left.abs() <= EXACT && right.abs() <= EXACT {
                        (left as f64 / right as f64).trunc() as i128
                    } else {
                        checked(left.checked_div(right))?
                    }
                }
                "%" => {
                    if right == 0 {
                        return Err(Unreadable::Condition);
                    }
                    let remainder = checked(left.checked_rem(right))?;
                    if remainder != 0 && (remainder < 0) != (right < 0) {
                        remainder + right
                    } else {
                        remainder
                    }
                }
                "==" => (left == right) as i128,
                "!=" => (left != right) as i128,
                "<" => (left < right) as i128,
                "<=" => (left <= right) as i128,
                ">" => (left > right) as i128,
                _ => (left >= right) as i128,
            }
        }
    })
}

/// An open #if: whether its enclosing code compiles, whether its current branch does, and whether
/// any branch has.
struct Block {
    parent: bool,
    on: bool,
    taken: bool,
    line: usize,
}

/// frames: Python frames in use, this function's included.
fn conditional_directive(
    word: &str,
    rest: &str,
    conditions: &mut Vec<Block>,
    defines: &HashMap<String, String>,
    line: usize,
    frames: usize,
) -> Parsed<()> {
    let compiling = conditions.last().is_none_or(|block| block.on);
    if matches!(word, "if" | "ifdef" | "ifndef") {
        let name = strip(before_comment(rest));
        let chosen = if word == "if" {
            compiling && condition_value(rest, defines, line, frames + 1)?
        } else {
            compiling && (defines.contains_key(name) == (word == "ifdef"))
        };
        conditions.push(Block { parent: compiling, on: chosen, taken: chosen || !compiling, line });
        return Ok(());
    }
    let Some(block) = conditions.last_mut() else {
        return lex_fail(line, format!("#{word} without #if"));
    };
    match word {
        "elif" => {
            block.on = block.parent && !block.taken && condition_value(rest, defines, line, frames + 1)?;
            block.taken = block.taken || block.on;
        }
        "else" => {
            block.on = block.parent && !block.taken;
            block.taken = true;
        }
        _ => {
            conditions.pop();
        }
    }
    Ok(())
}

// ---- tokens --------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Define,
    Include,
    Pragma,
    Assembly,
    Asm,
    Str,
    Int,
    Float,
    Name,
    Op,
    Eof,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Define => "define",
            Kind::Include => "include",
            Kind::Pragma => "pragma",
            Kind::Assembly => "assembly",
            Kind::Asm => "asm",
            Kind::Str => "string",
            Kind::Int => "int",
            Kind::Float => "float",
            Kind::Name => "name",
            Kind::Op => "op",
            Kind::Eof => "eof",
        }
    }
}

#[derive(Clone, Debug)]
struct Token {
    kind: Kind,
    line: usize,
    /// The text parse.py's token has; #define, #include, #pragma and asm tokens have none.
    text: Option<String>,
    /// The comment block right above a name.
    doc: Option<String>,
    /// A #define's words, or an asm block's header words.
    words: Vec<String>,
    /// A #pragma's words.
    pragma: Vec<Node>,
    /// An #include's path, and whether it was <path>.
    path: String,
    system: bool,
    /// An asm block's lines.
    lines: Vec<String>,
}

impl Token {
    fn new(kind: Kind, line: usize, text: Option<String>) -> Token {
        Token {
            kind,
            line,
            text,
            doc: None,
            words: Vec::new(),
            pragma: Vec::new(),
            path: String::new(),
            system: false,
            lines: Vec::new(),
        }
    }

    fn text(&self) -> &str {
        self.text.as_deref().unwrap_or("")
    }
}

/// A string literal from the opening quote: its text and the position after it.
fn read_string(text: &[char], position: usize, line: usize) -> Parsed<(String, usize)> {
    let mut out = String::new();
    let mut position = position + 1;
    while position < text.len() && text[position] != '"' {
        let c = text[position];
        if c == '\n' {
            return lex_fail(line, "string runs past the end of the line");
        }
        if c != '\\' {
            out.push(c);
            position += 1;
            continue;
        }
        let next = text.get(position + 1).copied();
        if next == Some('x') {
            out.push(hex_escape(&text[(position + 2).min(text.len())..(position + 4).min(text.len())])?);
            position += 4;
        } else if let Some(c) = escape(next) {
            out.push(c);
            position += 2;
        } else {
            return lex_fail(line, format!("unknown escape \\{}", next.map(String::from).unwrap_or_default()));
        }
    }
    if position >= text.len() {
        return lex_fail(line, "unterminated string");
    }
    Ok((out, position + 1))
}

/// A character literal, 'a' or '\n', as its character code.
fn read_char(text: &[char], position: usize, line: usize) -> Parsed<(u32, usize)> {
    let (value, position) = if text.get(position + 1) == Some(&'\\') {
        let next = text.get(position + 2).copied();
        let Some(value) = escape(next) else {
            return lex_fail(line, format!("unknown escape \\{}", next.map(String::from).unwrap_or_default()));
        };
        (Some(value), position + 3)
    } else {
        (text.get(position + 1).copied(), position + 2)
    };
    match value {
        Some(value) if text.get(position) == Some(&'\'') => Ok((value as u32, position + 1)),
        _ => lex_fail(line, "a character literal holds one character, as in 'a'"),
    }
}

/// asm.tokenize: words split at whitespace, a quoted word as {"string": text}.
fn asm_words(line: &[char]) -> Parsed<Vec<Node>> {
    if !line.contains(&'"') {
        return Ok(split(&text_of(line)).into_iter().map(Node::from).collect());
    }
    let ended = || Fault::Crash(format!("a string in #pragma or #include {} is not ended", repr(&text_of(line))));
    let mut words = Vec::new();
    let mut position = 0;
    while position < line.len() {
        let c = line[position];
        if is_space(c) {
            position += 1;
            continue;
        }
        if c != '"' {
            let start = position;
            while position < line.len() && !is_space(line[position]) {
                position += 1;
            }
            words.push(Node::from(text_of(&line[start..position])));
            continue;
        }
        let mut text = String::new();
        position += 1;
        loop {
            let c = *line.get(position).ok_or_else(ended)?;
            if c == '"' {
                break;
            }
            if c == '\\' {
                let next = *line.get(position + 1).ok_or_else(ended)?;
                if next == 'x' {
                    text.push(hex_escape(&line[(position + 2).min(line.len())..(position + 4).min(line.len())])?);
                    position += 4;
                    continue;
                }
                let c = asm_escape(next).ok_or_else(|| Fault::Crash(format!("unknown escape \\{next}")))?;
                text.push(c);
                position += 2;
                continue;
            }
            text.push(c);
            position += 1;
        }
        words.push(node!({"string": text}));
        position += 1;
    }
    Ok(words)
}

/// An asm block from after its "asm": the token and the position after its closing }.
fn read_asm(text: &[char], position: usize, line: usize) -> Parsed<(Token, usize)> {
    let Some(opening) = find(text, position, "{") else {
        return lex_fail(line, "asm block without {");
    };
    let words: Vec<String> = split(&text_of(&text[position..opening])).into_iter().map(String::from).collect();
    // the first line from the { on that is a } alone
    let closing = (opening + 1..text.len()).find_map(|start| {
        if text[start - 1] != '\n' || text[start] != '}' {
            return None;
        }
        let mut end = start + 1;
        while end < text.len() && matches!(text[end], ' ' | '\t') {
            end += 1;
        }
        (end == text.len() || text[end] == '\n').then_some((start, end))
    });
    let Some((start, end)) = closing else {
        return lex_fail(line, "asm block without a closing }");
    };
    let body = text_of(&text[line_end(text, opening) + 1..start]);
    let mut lines: Vec<String> = body.split('\n').map(String::from).collect();
    lines.pop();
    let mut token = Token::new(Kind::Asm, line, None);
    token.words = words;
    token.lines = lines;
    Ok((token, end))
}

/// The comment lines right above a line, as paragraphs (a blank comment line between them).
pub fn comment_block(comments: &[(usize, String)], line: usize) -> String {
    if comments.last().is_none_or(|(number, _)| number + 1 != line) {
        return String::new();
    }
    let mut block = Vec::new();
    let mut expected = line.wrapping_sub(1);
    for (number, text) in comments.iter().rev() {
        if *number != expected {
            break;
        }
        block.push(text.as_str());
        expected = expected.wrapping_sub(1);
    }
    block.reverse();
    let mut paragraphs: Vec<Vec<&str>> = vec![Vec::new()];
    for text in block {
        if text.is_empty() {
            paragraphs.push(Vec::new());
        } else {
            paragraphs.last_mut().unwrap().push(text);
        }
    }
    paragraphs
        .iter()
        .filter(|paragraph| !paragraph.is_empty())
        .map(|paragraph| joined(paragraph))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Joins wrapped comment lines. Older trees wrapped after a hyphen (mid-/motion), which joins
/// without a space.
fn joined(lines: &[&str]) -> String {
    let mut text = String::new();
    for line in lines {
        let mut end = text.chars().rev();
        let hyphen = end.next() == Some('-')
            && end.next().is_some_and(|c| c.is_ascii_alphabetic())
            && line.chars().next().is_some_and(c2_py::is_alpha);
        if !hyphen && !text.is_empty() {
            text.push(' ');
        }
        text.push_str(line);
    }
    text
}

/// #[ \t]*NAME[ \t]*REST at a #: the directive's name and the rest of its line.
fn directive_at(text: &[char], position: usize) -> Option<(String, String)> {
    let mut i = position + 1;
    while i < text.len() && matches!(text[i], ' ' | '\t') {
        i += 1;
    }
    let start = i;
    while i < text.len() && (text[i].is_ascii_alphabetic() || text[i] == '_') {
        i += 1;
    }
    if i == start {
        return None;
    }
    let word = text_of(&text[start..i]);
    while i < text.len() && matches!(text[i], ' ' | '\t') {
        i += 1;
    }
    Some((word, text_of(&text[i..line_end(text, i)])))
}

/// A number at a digit, as NUMBER matches it: (0x hex | decimal [. decimal] [e [+-] decimal])
/// then [fFuUlL]*, the decimals in any script. The word, the suffix and the end.
fn number_at(text: &[char], position: usize) -> Option<(String, String, usize)> {
    let at = |i: usize| text.get(i).copied().unwrap_or('\0');
    let decimal = |i: usize| c2_py::is_decimal(at(i));
    let mut end;
    if at(position) == '0' && matches!(at(position + 1), 'x' | 'X') && at(position + 2).is_ascii_hexdigit() {
        end = position + 2;
        while at(end).is_ascii_hexdigit() {
            end += 1;
        }
    } else if decimal(position) {
        end = position;
        while decimal(end) {
            end += 1;
        }
        if at(end) == '.' {
            end += 1;
            while decimal(end) {
                end += 1;
            }
        }
        if matches!(at(end), 'e' | 'E') {
            let mut exponent = end + 1;
            if matches!(at(exponent), '+' | '-') {
                exponent += 1;
            }
            if decimal(exponent) {
                while decimal(exponent) {
                    exponent += 1;
                }
                end = exponent;
            }
        }
    } else {
        return None;
    }
    let word_end = end;
    while "fFuUlL".contains(at(end)) {
        end += 1;
    }
    Some((text_of(&text[position..word_end]), text_of(&text[word_end..end]), end))
}

/// defines, when given, are #defines in force before the first line (a mod patch's LANGUAGE).
/// frames: Python frames in use, this function's included.
fn tokenize(text: &[char], mut defines: HashMap<String, String>, frames: usize) -> Parsed<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut comments: Vec<(usize, String)> = Vec::new();
    let mut conditions: Vec<Block> = Vec::new();
    let mut position = 0;
    let mut line = 1;
    while position < text.len() {
        let c = text[position];
        if c == '\n' {
            line += 1;
            position += 1;
            continue;
        }
        let directive = if c == '#' { directive_at(text, position) } else { None };
        if let Some((word, rest)) = &directive {
            if CONDITIONALS.contains(&word.as_str()) {
                conditional_directive(word, rest, &mut conditions, &defines, line, frames + 1)?;
                position = line_end(text, position);
                continue;
            }
        }
        if conditions.last().is_some_and(|block| !block.on) {
            position = line_end(text, position);
            continue;
        }
        if let Some((word, rest)) = &directive {
            if word == "undef" {
                defines.remove(strip(before_comment(rest)));
                position = line_end(text, position);
                continue;
            }
        }
        if is_space(c) {
            position += 1;
        } else if starts_with(text, position, "//") {
            let end = line_end(text, position);
            let start = (0..position).rev().find(|&at| text[at] == '\n').map_or(0, |at| at + 1);
            if strip(&text_of(&text[start..position])).is_empty() {
                comments.push((line, strip(&text_of(&text[position + 2..end])).to_string()));
            }
            position = end;
        } else if starts_with(text, position, "/*") {
            let Some(end) = find(text, position + 2, "*/") else {
                return lex_fail(line, "unterminated comment");
            };
            line += count_lines(text, position, end);
            position = end + 2;
        } else if starts_with(text, position, "#define") {
            let end = line_end(text, position);
            let rest = text_of(&text[position + "#define".len()..end]);
            let words: Vec<String> = split_first_word(&rest).into_iter().map(String::from).collect();
            if words.first().is_some_and(|word| word.contains('(')) {
                return lex_fail(line, "macros with parameters are not supported; write a function instead");
            }
            if let Some(name) = words.first() {
                let value = words.get(1).map_or("1", |value| strip(before_comment(value)));
                defines.insert(name.clone(), value.to_string());
            }
            let mut token = Token::new(Kind::Define, line, None);
            token.words = words;
            tokens.push(token);
            position = end;
        } else if starts_with(text, position, "#include") {
            let end = line_end(text, position);
            let rest = text_of(&text[position + "#include".len()..end]);
            let rest = strip(&rest);
            let mut token = Token::new(Kind::Include, line, None);
            if rest.starts_with('<') && rest.ends_with('>') {
                token.path = strip(&rest[1..rest.len() - 1]).to_string();
                token.system = true;
            } else {
                let words = asm_words(&rest.chars().collect::<Vec<char>>())?;
                match words.as_slice() {
                    [word] if word.as_object().is_some() => token.path = word["string"].as_str().unwrap().to_string(),
                    _ => return lex_fail(line, "expected #include \"path\" or #include <path>"),
                }
            }
            tokens.push(token);
            position = end;
        } else if starts_with(text, position, "#pragma") {
            let end = line_end(text, position);
            let words = asm_words(&text[position + "#pragma".len()..end])?;
            if words.first().is_some_and(|word| *word == "assembly") {
                let rest = text_of(&text[(end + 1).min(text.len())..]);
                tokens.push(Token::new(Kind::Assembly, line, Some(rest)));
                return Ok(tokens);
            }
            let mut token = Token::new(Kind::Pragma, line, None);
            token.pragma = words;
            tokens.push(token);
            position = end;
        } else if c == '"' {
            let (value, end) = read_string(text, position, line)?;
            tokens.push(Token::new(Kind::Str, line, Some(value)));
            position = end;
        } else if c2_py::is_digit(c) {
            let Some((mut word, suffix, end)) = number_at(text, position) else {
                return Err(Fault::Crash(format!("line {line}: cannot read the number at {}", repr(&c.to_string()))));
            };
            let lower = word.to_lowercase();
            let hexadecimal = lower.starts_with("0x");
            let floating =
                !hexadecimal && (word.contains('.') || lower.contains('e') || suffix.to_lowercase().contains('f'));
            if floating && !word.contains('.') && !lower.contains('e') {
                word.push_str(".0");
            }
            tokens.push(Token::new(if floating { Kind::Float } else { Kind::Int }, line, Some(word)));
            position = end;
        } else if c == '\'' {
            let (value, end) = read_char(text, position, line)?;
            tokens.push(Token::new(Kind::Int, line, Some(value.to_string())));
            position = end;
        } else if c.is_ascii_alphabetic() || c == '_' {
            let mut end = position + 1;
            while end < text.len() && (text[end].is_ascii_alphanumeric() || text[end] == '_') {
                end += 1;
            }
            let name = text_of(&text[position..end]);
            if name == "asm" {
                let (token, after) = read_asm(text, end, line)?;
                tokens.push(token);
                line += count_lines(text, position, after);
                position = after;
                continue;
            }
            let mut token = Token::new(Kind::Name, line, Some(name));
            let doc = comment_block(&comments, line);
            if !doc.is_empty() {
                token.doc = Some(doc);
            }
            tokens.push(token);
            position = end;
        } else if let Some(op) = LONG_OPERATORS.iter().find(|op| starts_with(text, position, op)) {
            tokens.push(Token::new(Kind::Op, line, Some(op.to_string())));
            position += 3;
        } else if let Some(op) = OPERATORS.iter().find(|op| starts_with(text, position, op)) {
            tokens.push(Token::new(Kind::Op, line, Some(op.to_string())));
            position += 2;
        } else if SINGLES.contains(c) {
            tokens.push(Token::new(Kind::Op, line, Some(c.to_string())));
            position += 1;
        } else {
            return lex_fail(line, format!("unexpected character {}", repr(&c.to_string())));
        }
    }
    if let Some(block) = conditions.last() {
        return lex_fail(block.line, "#if without #endif");
    }
    tokens.push(Token::new(Kind::Eof, line, Some(String::new())));
    Ok(tokens)
}

// ---- parsing -------------------------------------------------------------------------------

/// A function's parameters and the locals declared so far.
struct Scope {
    params: Vec<Node>,
    locals: Vec<Node>,
}

/// A token taken from the state's list, without copying it.
struct Taken {
    tokens: Rc<[Token]>,
    index: usize,
}

impl std::ops::Deref for Taken {
    type Target = Token;

    fn deref(&self) -> &Token {
        &self.tokens[self.index]
    }
}

struct State {
    tokens: Rc<[Token]>,
    pos: usize,
    /// Integer #defines so far, for array sizes.
    constants: HashMap<String, i128>,
    /// typedef'd value kinds and their base types.
    kinds: HashMap<String, String>,
    scope: Option<Scope>,
    /// Python frames in use, the current function's included.
    depth: usize,
    /// Whether a call went past Python's recursion limit: everything fails from there, as the
    /// RecursionError would.
    overflow: Cell<bool>,
}

impl State {
    fn new(tokens: Vec<Token>, depth: usize) -> State {
        let (constants, kinds) = (HashMap::new(), HashMap::new());
        State { tokens: tokens.into(), pos: 0, constants, kinds, scope: None, depth, overflow: Cell::new(false) }
    }

    /// A call of a parse.py helper that holds frames more frames at its deepest.
    fn leaf(&self, frames: usize) {
        if self.depth + frames > PYTHON_FRAMES {
            self.overflow.set(true);
        }
    }

    fn token(&self, offset: usize) -> &Token {
        &self.tokens[(self.pos + offset).min(self.tokens.len() - 1)]
    }

    fn peek(&self, offset: usize) -> &Token {
        self.leaf(1);
        self.token(offset)
    }

    fn at(&self, text: &str, offset: usize) -> bool {
        self.leaf(2);
        let token = self.token(offset);
        matches!(token.kind, Kind::Op | Kind::Name) && token.text() == text
    }

    fn take(&mut self) -> Taken {
        self.leaf(2);
        let index = (self.pos).min(self.tokens.len() - 1);
        self.pos += 1;
        Taken { tokens: Rc::clone(&self.tokens), index }
    }

    fn line(&self) -> usize {
        self.peek(0).line
    }

    fn fail<T>(&self, reason: impl std::fmt::Display) -> Parsed<T> {
        self.leaf(1);
        lex_fail(self.token(0).line, reason)
    }

    /// repr() of a token's text for a message; a token with none fails as Python's KeyError does.
    fn shown(&self, token: &Token) -> Parsed<String> {
        match &token.text {
            Some(text) => Ok(repr(text)),
            None => Err(Fault::Crash(format!("line {}: unexpected {} token", token.line, token.kind.name()))),
        }
    }

    fn expect(&mut self, text: &str) -> Parsed<Taken> {
        self.leaf(3);
        if !self.at(text, 0) {
            let found = self.shown(self.peek(0))?;
            return self.fail(format!("expected {}, found {found}", repr(text)));
        }
        Ok(self.take())
    }

    fn expect_kind(&mut self, kind: Kind) -> Parsed<Taken> {
        self.leaf(3);
        if self.peek(0).kind != kind {
            let found = self.shown(self.peek(0))?;
            return self.fail(format!("expected {}, found {found}", kind.name()));
        }
        Ok(self.take())
    }

    fn name(&mut self) -> Parsed<String> {
        Ok(self.expect_kind(Kind::Name)?.text().to_string())
    }

    /// Runs a parse.py function: one Python frame more.
    fn framed<T>(&mut self, function: impl FnOnce(&mut State) -> Parsed<T>) -> Parsed<T> {
        self.leaf(1);
        let result = if self.overflow.get() {
            None
        } else {
            self.depth += 1;
            let result = function(self);
            self.depth -= 1;
            Some(result)
        };
        match result {
            Some(result) if !self.overflow.get() => result,
            _ => Err(Fault::Crash(format!("line {}: nested too deeply", self.token(0).line))),
        }
    }
}

fn int_value(text: &str) -> Parsed<i128> {
    let hexadecimal = text.starts_with("0x") || text.starts_with("0X");
    python_int(text, if hexadecimal { 16 } else { 10 })
}

fn literal(state: &State, token: &Token) -> Parsed<Node> {
    state.leaf(2);
    if token.kind == Kind::Int {
        return int_literal(int_value(token.text())?);
    }
    let bits = asm::float_bits(&ascii_digits(token.text())).map_err(|error| Fault::Crash(error.to_string()))?;
    Ok(node!({"kind": "float", "bits": bits}))
}

fn arguments(state: &mut State) -> Parsed<Vec<Node>> {
    state.framed(|state| {
        state.expect("(")?;
        let mut args = Vec::new();
        while !state.at(")", 0) {
            args.push(assignment(state)?);
            if !state.at(")", 0) {
                state.expect(",")?;
            }
        }
        state.expect(")")?;
        Ok(args)
    })
}

fn primary(state: &mut State) -> Parsed<Node> {
    state.framed(|state| {
        let token = state.take();
        match token.kind {
            Kind::Int | Kind::Float => return literal(state, &token),
            Kind::Str => return Ok(node!({"kind": "string", "text": token.text()})),
            Kind::Op if token.text() == "(" => {
                let node = assignment(state)?;
                state.expect(")")?;
                return Ok(node);
            }
            Kind::Name => {}
            _ => {
                state.pos -= 1;
                let found = state.shown(&token)?;
                return state.fail(format!("expected an expression, found {found}"));
            }
        }
        let name = token.text();
        if BUILTINS.contains(&name) && state.at("(", 0) {
            state.expect("(")?;
            let operand = assignment(state)?;
            state.expect(")")?;
            return Ok(node!({"kind": "builtin", "name": name, "operand": operand}));
        }
        if state.at("(", 0) {
            return Ok(node!({"kind": "call", "name": name, "args": arguments(state)?}));
        }
        let node = if state.at("[", 0) {
            state.expect("[")?;
            let index = assignment(state)?;
            state.expect("]")?;
            node!({"kind": "index", "name": name, "index": index})
        } else {
            node!({"kind": "var", "name": name})
        };
        if state.at("++", 0) || state.at("--", 0) {
            let op = state.take();
            return Ok(postfix(state, node, op.text()));
        }
        Ok(node)
    })
}

/// target op= value, as in x += 2. The compiler works out the target's address once, so an index
/// with side effects (a[i++] += 1) runs them once.
fn compound(state: &State, target: Node, op: &str, value: Node) -> Node {
    state.leaf(1);
    node!({"kind": "compound", "op": op, "target": target, "value": value})
}

fn stepped(state: &State, target: Node, op: &str) -> Node {
    state.leaf(2);
    compound(state, target, op, one())
}

/// x++ and x--: the variable steps, and the expression's value is the old one, worked back from the
/// new so no temporary is needed. As a statement only the step remains.
fn postfix(state: &State, target: Node, text: &str) -> Node {
    state.leaf(3);
    let (op, undo) = if text == "++" { ("+", "-") } else { ("-", "+") };
    node!({"kind": "binary", "op": undo, "left": stepped(state, target, op), "right": one(), "postfix": true})
}

fn cast_ahead(state: &State) -> bool {
    state.leaf(3);
    state.at("(", 0)
        && state.peek(1).kind == Kind::Name
        && CAST_TYPES.contains(&state.peek(1).text())
        && state.at(")", 2)
}

fn pointer_cast_ahead(state: &State) -> bool {
    state.leaf(3);
    state.at("(", 0)
        && state.peek(1).kind == Kind::Name
        && CAST_TYPES.contains(&state.peek(1).text())
        && state.at("*", 2)
        && state.at(")", 3)
}

fn is_target(node: &Node) -> bool {
    matches!(node.kind(), "var" | "index" | "deref")
}

fn unary(state: &mut State) -> Parsed<Node> {
    state.framed(|state| {
        if state.at("-", 0) || state.at("!", 0) {
            let op = state.take();
            return Ok(node!({"kind": "unary", "op": op.text(), "operand": unary(state)?}));
        }
        if state.at("+", 0) {
            state.take();
            return unary(state);
        }
        if state.at("~", 0) {
            state.take();
            let operand = node!({"kind": "unary", "op": "-", "operand": unary(state)?});
            return Ok(node!({"kind": "binary", "op": "-", "left": operand, "right": one()}));
        }
        if state.at("++", 0) || state.at("--", 0) {
            let op = if state.take().text() == "++" { "+" } else { "-" };
            let target = unary(state)?;
            if !is_target(&target) {
                return state.fail("++ and -- need a variable");
            }
            return Ok(stepped(state, target, op));
        }
        if state.at("&", 0) {
            state.take();
            let target = unary(state)?;
            if !is_target(&target) {
                return state.fail("& needs a variable");
            }
            return Ok(node!({"kind": "addr", "target": target}));
        }
        if state.at("*", 0) {
            state.take();
            return Ok(node!({"kind": "deref", "operand": unary(state)?}));
        }
        if cast_ahead(state) {
            state.take();
            let kind = state.take();
            state.take();
            return Ok(node!({"kind": "cast", "type": kind.text(), "operand": unary(state)?}));
        }
        if state.at("(", 0) && state.at("enum", 1) && state.peek(2).kind == Kind::Name && state.at(")", 3) {
            // (enum flags)&x passes x's address where a native takes an enum, as it is
            state.take();
            state.take();
            let kind = format!("enum {}", state.take().text());
            state.take();
            let operand = unary(state)?;
            if operand.kind() != "addr" {
                return state.fail("a cast to an enum takes an address (&x)");
            }
            return Ok(node!({"kind": "cast", "type": kind, "operand": operand}));
        }
        if pointer_cast_ahead(state) {
            // (int *)&x passes x's address as it is: C's view of a pointer of another type
            state.take();
            let kind = state.take().text().to_string();
            state.take();
            state.take();
            let operand = unary(state)?;
            if operand.kind() != "addr" {
                return state.fail("a pointer cast takes an address (&x)");
            }
            return Ok(node!({"kind": "cast", "type": format!("{kind} *"), "operand": operand}));
        }
        primary(state)
    })
}

fn binary(state: &mut State, minimum: u32) -> Parsed<Node> {
    state.framed(|state| {
        let mut left = unary(state)?;
        while state.peek(0).kind == Kind::Op && precedence(state.peek(0).text()).unwrap_or(0) >= minimum {
            let op = state.take().text().to_string();
            let right = binary(state, precedence(&op).unwrap() + 1)?;
            let kind = if op == "&&" || op == "||" { "logical" } else { "binary" };
            left = node!({"kind": kind, "op": op, "left": left, "right": right});
        }
        Ok(left)
    })
}

fn ternary(state: &mut State) -> Parsed<Node> {
    state.framed(|state| {
        let cond = binary(state, 2)?;
        if !state.at("?", 0) {
            return Ok(cond);
        }
        state.take();
        let then = assignment(state)?;
        state.expect(":")?;
        Ok(node!({"kind": "ternary", "cond": cond, "then": then, "else": ternary(state)?}))
    })
}

fn assignment(state: &mut State) -> Parsed<Node> {
    state.framed(|state| {
        let left = ternary(state)?;
        let token = state.peek(0);
        if token.kind != Kind::Op || (token.text() != "=" && compound_op(token.text()).is_none()) {
            return Ok(left);
        }
        if !is_target(&left) {
            return state.fail("cannot assign to this expression");
        }
        let op = state.take();
        let value = assignment(state)?;
        if let Some(op) = compound_op(op.text()) {
            return Ok(compound(state, left, op, value));
        }
        Ok(node!({"kind": "assign", "target": left, "value": value}))
    })
}

fn expression_statement(state: &State, node: Node) -> Node {
    state.leaf(1);
    let node = if node["postfix"] == Node::Bool(true) { node["left"].clone() } else { node };
    node!({"kind": "expr", "expr": node})
}

fn braced(state: &mut State) -> Parsed<Vec<Node>> {
    state.framed(|state| {
        state.expect("{")?;
        let mut statements = Vec::new();
        while !state.at("}", 0) {
            statements.extend(statement(state)?);
        }
        state.expect("}")?;
        Ok(statements)
    })
}

/// A loop or branch body: a braced block or one statement.
fn body(state: &mut State) -> Parsed<Vec<Node>> {
    state.framed(|state| if state.at("{", 0) { braced(state) } else { statement(state) })
}

fn conditional(state: &mut State) -> Parsed<Node> {
    state.framed(|state| {
        let line = state.expect("if")?.line;
        state.expect("(")?;
        let cond = assignment(state)?;
        state.expect(")")?;
        let then = body(state)?;
        let mut otherwise = Node::Null;
        if state.at("else", 0) {
            state.take();
            otherwise =
                if state.at("if", 0) { Node::List(vec![conditional(state)?]) } else { Node::List(body(state)?) };
        }
        Ok(node!({"kind": "if", "cond": cond, "then": then, "else": otherwise, "line": line}))
    })
}

/// Puts extra statements in front of every continue of a loop body (not of loops inside it), so a
/// for loop's step and a do loop's test run when an iteration is cut short.
fn before_continue(nodes: &[Node], extra: &[Node]) -> Vec<Node> {
    let list = |node: &Node| node.as_array().map_or(&[][..], Vec::as_slice).to_vec();
    let mut out = Vec::new();
    for node in nodes {
        let node = match node.kind() {
            "continue" => {
                out.extend_from_slice(extra);
                node.clone()
            }
            "if" => {
                let otherwise = if node["else"].is_null() {
                    Node::Null
                } else {
                    Node::List(before_continue(&list(&node["else"]), extra))
                };
                node.clone().with("then", before_continue(&list(&node["then"]), extra)).with("else", otherwise)
            }
            "switch" => {
                let cases: Vec<Node> = list(&node["cases"])
                    .into_iter()
                    .map(|case| {
                        let body = before_continue(&list(&case["body"]), extra);
                        case.with("body", body)
                    })
                    .collect();
                node.clone().with("cases", cases)
            }
            _ => node.clone(),
        };
        out.push(node);
    }
    out
}

fn expression_list(state: &mut State, closing: &str) -> Parsed<Vec<Node>> {
    state.framed(|state| {
        let mut found = Vec::new();
        while !state.at(closing, 0) {
            let node = assignment(state)?;
            found.push(expression_statement(state, node));
            if !state.at(closing, 0) {
                state.expect(",")?;
            }
        }
        state.expect(closing)?;
        Ok(found)
    })
}

/// for (init; cond; step) body becomes init; while (cond) { body step }, with the step also run
/// before each continue.
fn for_loop(state: &mut State) -> Parsed<Vec<Node>> {
    state.framed(|state| {
        let line = state.expect("for")?.line;
        state.expect("(")?;
        let mut init = if declaration_ahead(state) { declaration(state)? } else { expression_list(state, ";")? };
        let cond = if state.at(";", 0) { one() } else { assignment(state)? };
        state.expect(";")?;
        let step = expression_list(state, ")")?;
        let mut loop_body = before_continue(&body(state)?, &step);
        loop_body.extend(step);
        init.push(node!({"kind": "while", "cond": cond, "body": loop_body, "line": line}));
        Ok(init)
    })
}

/// do body while (cond); becomes while (1) { body if (!cond) break; }, with the test also run before
/// each continue.
fn do_loop(state: &mut State) -> Parsed<Vec<Node>> {
    state.framed(|state| {
        let line = state.expect("do")?.line;
        let loop_body = body(state)?;
        state.expect("while")?;
        state.expect("(")?;
        let cond = assignment(state)?;
        state.expect(")")?;
        state.expect(";")?;
        let leave = node!({
            "kind": "if",
            "cond": {"kind": "unary", "op": "!", "operand": cond},
            "then": [{"kind": "break"}],
            "else": null,
        });
        let mut body = before_continue(&loop_body, std::slice::from_ref(&leave));
        body.push(leave);
        Ok(vec![node!({"kind": "while", "cond": one(), "body": body, "line": line})])
    })
}

fn switch(state: &mut State) -> Parsed<Node> {
    state.framed(|state| {
        let line = state.expect("switch")?.line;
        state.expect("(")?;
        let value = assignment(state)?;
        state.expect(")")?;
        state.expect("{")?;
        let mut cases: Vec<Node> = Vec::new();
        while !state.at("}", 0) {
            if state.at("case", 0) {
                state.take();
                cases.push(node!({"value": ternary(state)?, "default": false, "body": []}));
                state.expect(":")?;
            } else if state.at("default", 0) {
                state.take();
                state.expect(":")?;
                if cases.iter().any(|case| case["default"] == Node::Bool(true)) {
                    return state.fail("a switch has one default");
                }
                cases.push(node!({"value": null, "default": true, "body": []}));
            } else if cases.is_empty() {
                return state.fail("expected case or default");
            } else {
                let found = statement(state)?;
                cases.last_mut().unwrap()["body"].as_array_mut().unwrap().extend(found);
            }
        }
        state.expect("}")?;
        Ok(node!({"kind": "switch", "value": value, "cases": cases, "line": line}))
    })
}

fn statement(state: &mut State) -> Parsed<Vec<Node>> {
    state.framed(|state| {
        let line = state.line();
        // a "// note: ..." comment right above a statement is a line note the definitions keep
        let doc = state.peek(0).doc.clone().unwrap_or_default();
        let mut nodes: Vec<Node> = statement_nodes(state)?
            .into_iter()
            .map(|node| if node.get("line").is_some() { node } else { node.with("line", line) })
            .collect();
        // the last such paragraph
        let note = doc
            .split('\n')
            .rfind(|paragraph| paragraph.get(..NOTE.len()).is_some_and(|start| start.eq_ignore_ascii_case(NOTE)));
        if let (Some(first), Some(note)) = (nodes.first_mut(), note) {
            *first = std::mem::take(first).with("note", strip(&note[NOTE.len()..]));
        }
        Ok(nodes)
    })
}

fn statement_nodes(state: &mut State) -> Parsed<Vec<Node>> {
    state.framed(|state| {
        let token = state.peek(0);
        let word = if token.kind == Kind::Name { token.text().to_string() } else { String::new() };
        match word.as_str() {
            "AT" => {
                state.take();
                state.expect("(")?;
                let base = int_node(int_value(state.expect_kind(Kind::Int)?.text())?)?;
                state.expect(")")?;
                return Ok(statement(state)?.into_iter().map(|node| node.with("at", base.clone())).collect());
            }
            "DIRECT" => {
                state.take();
                return Ok(statement(state)?.into_iter().map(|node| node.with("at", "direct")).collect());
            }
            _ => {}
        }
        if state.at("{", 0) {
            return braced(state);
        }
        if state.at(";", 0) {
            state.take();
            return Ok(Vec::new());
        }
        if declaration_ahead(state) {
            return declaration(state);
        }
        match word.as_str() {
            "if" => return Ok(vec![conditional(state)?]),
            "while" => {
                state.take();
                state.expect("(")?;
                let cond = assignment(state)?;
                state.expect(")")?;
                return Ok(vec![node!({"kind": "while", "cond": cond, "body": body(state)?})]);
            }
            "for" => return for_loop(state),
            "do" => return do_loop(state),
            "switch" => return Ok(vec![switch(state)?]),
            "return" => {
                state.take();
                if state.at(";", 0) {
                    state.take();
                    return Ok(vec![node!({"kind": "return", "value": null})]);
                }
                let value = assignment(state)?;
                state.expect(";")?;
                return Ok(vec![node!({"kind": "return", "value": value})]);
            }
            "print" => {
                state.take();
                let args = arguments(state)?;
                state.expect(";")?;
                return Ok(vec![node!({"kind": "print", "args": args})]);
            }
            word if SIMPLE.contains(&word) => {
                state.take();
                state.expect(";")?;
                return Ok(vec![node!({"kind": word})]);
            }
            _ => {}
        }
        let node = assignment(state)?;
        state.expect(";")?;
        Ok(vec![expression_statement(state, node)])
    })
}

fn type_name(state: &mut State, allowed: &[&str]) -> Parsed<String> {
    let kind = state.name()?;
    if !allowed.contains(&kind.as_str()) {
        state.pos -= 1;
        return state.fail(format!("unknown type {kind}"));
    }
    Ok(kind)
}

/// An array's size: none, a number, or a constant's name.
#[derive(Clone, Debug, PartialEq)]
enum Size {
    None,
    Number(i128),
    Named(String),
}

impl Size {
    fn node(&self) -> Parsed<Node> {
        match self {
            Size::None => Ok(Node::Null),
            Size::Number(value) => int_node(*value),
            Size::Named(name) => Ok(Node::from(name)),
        }
    }
}

/// An array size: a number, or a constant's name. A name this file does not define is kept and
/// resolved with the constants of the headers and libraries the script includes (sdk.resolve).
fn array_size(state: &mut State) -> Parsed<Size> {
    state.leaf(3);
    let token = state.take();
    match token.kind {
        Kind::Int => Ok(Size::Number(int_value(token.text())?)),
        Kind::Name => Ok(match state.constants.get(token.text()) {
            Some(value) => Size::Number(*value),
            None => Size::Named(token.text().to_string()),
        }),
        _ => {
            state.pos -= 1;
            state.fail("an array size is a number or a constant")
        }
    }
}

fn initial_values(state: &mut State) -> Parsed<Vec<Node>> {
    state.framed(|state| {
        state.expect("{")?;
        let mut values = Vec::new();
        while !state.at("}", 0) {
            values.push(assignment(state)?);
            if !state.at("}", 0) {
                state.expect(",")?;
            }
        }
        state.expect("}")?;
        Ok(values)
    })
}

enum Init {
    None,
    Value(Node),
    List(Vec<Node>),
}

struct Declared {
    name: String,
    size: Size,
    init: Init,
    line: usize,
    pointer: bool,
}

fn declarators(state: &mut State) -> Parsed<Vec<Declared>> {
    state.framed(|state| {
        let mut found = Vec::new();
        loop {
            let line = state.line();
            let pointer = state.at("*", 0);
            if pointer {
                state.take();
            }
            let name = state.name()?;
            let mut size = Size::None;
            if state.at("[", 0) {
                state.take();
                let given = if state.at("]", 0) { None } else { Some(array_size(state)?) };
                state.expect("]")?;
                size = given.unwrap_or(Size::Number(-1));
            }
            let mut init = Init::None;
            if state.at("=", 0) {
                state.take();
                init =
                    if state.at("{", 0) { Init::List(initial_values(state)?) } else { Init::Value(assignment(state)?) };
            }
            if size == Size::Number(-1) {
                let Init::List(values) = &init else {
                    return state.fail(format!("{name}[] needs a size or a list of values"));
                };
                size = Size::Number(values.len() as i128);
            }
            if let Init::List(values) = &init {
                if size == Size::None || matches!(size, Size::Number(count) if values.len() as i128 > count) {
                    return state.fail(format!("too many values for {name}"));
                }
            }
            found.push(Declared { name, size, init, line, pointer });
            if !state.at(",", 0) {
                break;
            }
            state.take();
        }
        state.expect(";")?;
        Ok(found)
    })
}

fn declaration_ahead(state: &State) -> bool {
    state.leaf(2);
    let token = state.peek(0);
    if token.kind != Kind::Name {
        return false;
    }
    if token.text() == "enum" {
        return state.peek(1).kind == Kind::Name && state.peek(2).kind == Kind::Name;
    }
    let following = state.peek(1);
    if TYPES.contains(&token.text()) {
        return following.kind == Kind::Name || (following.kind == Kind::Op && following.text() == "*");
    }
    if STATEMENT_WORDS.contains(&token.text()) {
        return false;
    }
    // a value kind or typedef from a header (bgm_file track;), which sdk.resolve reads as its base type
    following.kind == Kind::Name
        && state.peek(2).text.as_deref().is_some_and(|text| [";", "=", "[", ","].contains(&text))
}

/// A variable's type: int, float or string, or a value kind or enum (enum se_bank) that sdk.resolve
/// turns into the type it stands for.
fn variable_type(state: &mut State) -> Parsed<String> {
    state.leaf(4);
    if state.at("enum", 0) {
        state.take();
        return Ok(format!("enum {}", state.name()?));
    }
    let kind = state.name()?;
    if kind == "void" || kind == "any" {
        state.pos -= 1;
        return state.fail(format!("unknown type {kind}"));
    }
    Ok(kind)
}

/// A local declaration anywhere in a function. Every local gets a slot in the function's frame, so a
/// name declared again in another block reuses the slot when the type matches. Initializers become
/// assignments where the declaration stands.
fn declaration(state: &mut State) -> Parsed<Vec<Node>> {
    state.framed(|state| {
        if state.scope.is_none() {
            return state.fail("declarations belong inside functions");
        }
        let kind = variable_type(state)?;
        let mut statements = Vec::new();
        for declared in declarators(state)? {
            let mut entry = node!({"name": &declared.name, "type": &kind, "size": declared.size.node()?});
            if declared.pointer {
                entry["pointer"] = Node::Bool(true);
            }
            let scope = state.scope.as_ref().unwrap();
            if scope.params.iter().any(|param| param["name"] == *declared.name) {
                return state.fail(format!("{} is already a parameter", declared.name));
            }
            match scope.locals.iter().find(|local| local["name"] == *declared.name) {
                Some(existing) if *existing != entry => {
                    return state.fail(format!("{} is declared again with another type", declared.name));
                }
                Some(_) => {}
                None => state.scope.as_mut().unwrap().locals.push(entry),
            }
            match declared.init {
                Init::List(values) => {
                    for (position, value) in values.into_iter().enumerate() {
                        let cell =
                            node!({"kind": "index", "name": &declared.name, "index": {"kind": "int", "value": position}});
                        statements.push(node!({
                            "kind": "expr",
                            "expr": {"kind": "assign", "target": cell, "value": value},
                            "line": declared.line,
                        }));
                    }
                }
                Init::Value(value) => {
                    let target = node!({"kind": "var", "name": &declared.name});
                    statements.push(node!({
                        "kind": "expr",
                        "expr": {"kind": "assign", "target": target, "value": value},
                        "line": declared.line,
                    }));
                }
                Init::None => {}
            }
        }
        Ok(statements)
    })
}

fn no_parameters(state: &mut State) -> bool {
    if state.at("void", 0) && state.at(")", 1) {
        state.take();
        return true;
    }
    state.at(")", 0)
}

fn parameters(state: &mut State) -> Parsed<Vec<Node>> {
    state.expect("(")?;
    let mut params = Vec::new();
    if !no_parameters(state) {
        loop {
            // void *p: a pointer parameter the function never reads, so of no element type
            let kind = if state.at("void", 0) && state.at("*", 1) {
                state.take();
                "void".to_string()
            } else {
                variable_type(state)?
            };
            let pointer = state.at("*", 0);
            if pointer {
                state.take();
            }
            let name = state.name()?;
            let array = state.at("[", 0);
            if array {
                state.take();
                state.expect("]")?;
            }
            params.push(node!({"name": name, "type": kind, "array": array || pointer}));
            if !state.at(",", 0) {
                break;
            }
            state.take();
        }
    }
    state.expect(")")?;
    Ok(params)
}

/// What #pragma lines have set for the next function.
#[derive(Default)]
struct Pending {
    strings: Option<Vec<Node>>,
    padding: Option<Node>,
    override_: bool,
    /// What #pragma override NAME names, when it names something: a native the function wraps.
    override_of: Option<String>,
    every_frame: bool,
}

fn function_definition(
    state: &mut State,
    pending: &Pending,
    returns: &str,
    name: &str,
    doc: Option<&str>,
) -> Parsed<Node> {
    if name.starts_with('_') {
        state.pos -= 1;
        return state.fail("function names cannot start with _, which marks natives");
    }
    let params = parameters(state)?;
    state.scope = Some(Scope { params: params.clone(), locals: Vec::new() });
    let body = braced(state)?;
    let locals = state.scope.take().unwrap().locals;
    let mut function = node!({
        "kind": "function",
        "name": name,
        "returns": returns,
        "params": params,
        "locals": locals,
        "body": body,
        "unused_strings": pending.strings.clone().unwrap_or_default(),
        "padding": pending.padding.clone(),
    });
    if let Some(doc) = doc.filter(|doc| !doc.is_empty()) {
        function["doc"] = Node::from(doc);
    }
    if pending.override_ {
        function["override"] = Node::Bool(true);
    }
    if let Some(of) = &pending.override_of {
        function["override_of"] = Node::from(of.as_str());
    }
    if pending.every_frame {
        function["every_frame"] = Node::Bool(true);
    }
    Ok(function)
}

fn result_name(name: &str) -> String {
    let words: Vec<&str> = name.split('_').collect();
    if words.len() > 1 {
        words[1..].join("_")
    } else {
        format!("{name}_value")
    }
}

fn typed_parameter(state: &mut State) -> Parsed<Node> {
    let mut enumeration = None;
    let mut value_kind = None;
    let kind;
    if state.at("enum", 0) {
        state.take();
        enumeration = Some(state.name()?);
        kind = "int".to_string();
    } else if state.at("void", 0) && state.at("*", 1) {
        // an out parameter that takes a pointer to anything: any *, as C reads int * and float *
        state.take();
        kind = "any".to_string();
    } else if state.peek(0).kind == Kind::Name && state.kinds.contains_key(state.peek(0).text()) {
        let name = state.take().text().to_string();
        kind = state.kinds[&name].clone();
        value_kind = Some(name);
    } else {
        kind = type_name(state, &NATIVE_TYPES)?;
    }
    let out = state.at("*", 0);
    if out {
        state.take();
    }
    let mut found = node!({"name": state.name()?, "type": kind, "out": out, "optional": false});
    if let Some(enumeration) = enumeration {
        found["enum"] = Node::from(enumeration);
    }
    if let Some(value_kind) = value_kind {
        found["kind"] = Node::from(value_kind);
    }
    Ok(found)
}

fn native_declaration(state: &mut State) -> Parsed<Node> {
    let returns = type_name(state, &NATIVE_RETURN_TYPES)?;
    let name = state.name()?;
    state.expect("(")?;
    let mut params = Vec::new();
    let mut variadic = false;
    if !no_parameters(state) {
        loop {
            if state.at("...", 0) {
                state.take();
                variadic = true;
            } else if state.at("OPTIONAL", 0) {
                state.take();
                state.expect("(")?;
                loop {
                    if state.at("...", 0) {
                        state.take();
                        variadic = true;
                    } else {
                        params.push(typed_parameter(state)?.with("optional", true));
                    }
                    if !state.at(",", 0) {
                        break;
                    }
                    state.take();
                }
                state.expect(")")?;
            } else {
                params.push(typed_parameter(state)?);
            }
            if !state.at(",", 0) {
                break;
            }
            state.take();
        }
    }
    state.expect(")")?;
    state.expect("NATIVE")?;
    state.expect("(")?;
    let mut number = int_value(state.expect_kind(Kind::Int)?.text())?;
    let mut base = Node::Null;
    if state.at(",", 0) {
        state.take();
        base = int_node(number)?;
        number += int_value(state.expect_kind(Kind::Int)?.text())?;
    }
    state.expect(")")?;
    state.expect(";")?;
    if returns != "void" {
        params.push(
            node!({"name": result_name(&name), "type": &returns, "out": true, "optional": false, "result": true}),
        );
    }
    Ok(node!({
        "no": int_node(number)?,
        "base": base,
        "name": name,
        "params": params,
        "variadic": variadic,
        "returns": returns,
    }))
}

fn inline_definition(state: &mut State) -> Parsed<Node> {
    let returns = type_name(state, &RETURN_TYPES)?;
    if returns != "void" {
        return state.fail("inline helpers are statements and return void");
    }
    let name = state.name()?;
    state.expect("(")?;
    let mut params = Vec::new();
    if !no_parameters(state) {
        loop {
            params.push(typed_parameter(state)?);
            if !state.at(",", 0) {
                break;
            }
            state.take();
        }
    }
    state.expect(")")?;
    Ok(node!({"name": name, "params": params, "body": braced(state)?}))
}

fn enum_value(state: &mut State) -> Parsed<i128> {
    let negative = state.at("-", 0);
    if negative {
        state.take();
    }
    let value = int_value(state.expect_kind(Kind::Int)?.text())?;
    Ok(if negative { -value } else { value })
}

fn declarations<'a>(program: &'a mut Node, key: &str) -> &'a mut Vec<Node> {
    program["declarations"][key].as_array_mut().unwrap()
}

fn enumeration(state: &mut State, program: &mut Node) -> Parsed<()> {
    let enumeration = state.name()?;
    state.expect("{")?;
    let mut value = 0i128;
    while !state.at("}", 0) {
        let name = state.name()?;
        if state.at("=", 0) {
            state.take();
            value = enum_value(state)?;
        }
        if enumeration == "program" {
            declarations(program, "programs").push(node!({"name": name, "no": int_node(value)?}));
        } else {
            declarations(program, "constants").push(node!({
                "name": name,
                "value": int_literal(value)?,
                "enum": &enumeration,
            }));
        }
        value += 1;
        if !state.at(",", 0) {
            break;
        }
        state.take();
    }
    state.expect("}")?;
    state.expect(";")?;
    Ok(())
}

/// A #define whose value is a number or a string: a constant, and an integer one sizes arrays.
fn definition(state: &mut State, token: &Token, program: &mut Node) -> Parsed<()> {
    let [name, value] = token.words.as_slice() else {
        return Ok(());
    };
    let chars: Vec<char> = value.chars().collect();
    // definition's own frame, then tokenize's
    let mut inner = State::new(tokenize(&chars, HashMap::new(), state.depth + 2)?, state.depth + 1);
    let mut value = match unary(&mut inner) {
        Ok(value) => value,
        Err(Fault::Value(_)) => return Ok(()),
        Err(crash) => return Err(crash),
    };
    if inner.peek(0).kind != Kind::Eof {
        return Ok(());
    }
    if value.kind() == "unary" && value["op"] == "-" && value["operand"].kind() == "int" {
        let negated = -(value["operand"]["value"].as_i64().unwrap() as i128);
        value = int_literal(negated)?;
    }
    if matches!(value.kind(), "int" | "float" | "string") {
        let integer = value["value"].as_i64().filter(|_| value.kind() == "int");
        declarations(program, "constants").push(node!({"name": name, "value": value, "enum": null}));
        if let Some(integer) = integer {
            state.constants.insert(name.clone(), integer as i128);
        }
    }
    Ok(())
}

fn program_entry(state: &mut State, program: &mut Node) -> Parsed<()> {
    state.expect("(")?;
    let token = state.take();
    let number = match token.kind {
        Kind::Int => int_node(int_value(token.text())?)?,
        Kind::Name => Node::from(token.text()),
        _ => {
            state.pos -= 1;
            return state.fail("expected a program number or name");
        }
    };
    state.expect(",")?;
    let function = state.name()?;
    program["programs"].as_array_mut().unwrap().push(node!({"no": number, "function": function}));
    state.expect(")")?;
    state.expect(";")?;
    Ok(())
}

fn asm_function(state: &mut State, token: &Token) -> Parsed<Node> {
    let words = &token.words;
    if words.len() != 3 || words[1] != "arg" {
        state.pos -= 1;
        return state.fail("asm block header must be: asm NAME arg COUNT {");
    }
    let arg = int_node(python_int(&words[2], 10)?)?;
    Ok(node!({"kind": "asm", "name": &words[0], "arg": arg, "reason": "", "lines": token.lines.clone()}))
}

/// asm.magic_bytes: 0x and hex digits, or ASCII text and a 0 byte; as a str of bytes, as latin-1
/// decodes them.
fn magic(token: &str) -> Parsed<Node> {
    let bytes = if let Some(digits) = token.strip_prefix("0x") {
        from_hex(digits).map_err(|position| {
            Fault::Value(format!("non-hexadecimal number found in fromhex() arg at position {position}"))
        })?
    } else {
        let chars: Vec<char> = token.chars().collect();
        if let Some(start) = chars.iter().position(|c| !c.is_ascii()) {
            let end = (start..chars.len()).find(|&at| chars[at].is_ascii()).unwrap_or(chars.len());
            let message = if end == start + 1 {
                let c = chars[start] as u32;
                let shown = match c {
                    0..=0xff => format!("\\x{c:02x}"),
                    0x100..=0xffff => format!("\\u{c:04x}"),
                    _ => format!("\\U{c:08x}"),
                };
                format!("'ascii' codec can't encode character '{shown}' in position {start}: ordinal not in range(128)")
            } else {
                format!(
                    "'ascii' codec can't encode characters in position {start}-{}: ordinal not in range(128)",
                    end - 1
                )
            };
            return Err(Fault::Value(message));
        }
        let mut bytes = token.as_bytes().to_vec();
        bytes.push(0);
        bytes
    };
    Ok(Node::from(bytes.iter().map(|&byte| byte as char).collect::<String>()))
}

/// bytes.fromhex(): pairs of hex digits, whitespace between pairs; Err(position) where it fails.
fn from_hex(text: &str) -> std::result::Result<Vec<u8>, usize> {
    let chars: Vec<char> = text.chars().collect();
    if let Some(position) = chars.iter().position(|c| !c.is_ascii()) {
        return Err(position);
    }
    let digit = |i: usize| chars.get(i).and_then(|c| c.to_digit(16)).ok_or(i);
    let mut bytes = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if matches!(chars[i], ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c') {
            while i < chars.len() && matches!(chars[i], ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c') {
                i += 1;
            }
            if i >= chars.len() {
                break;
            }
        }
        let top = digit(i)?;
        let bottom = digit(i + 1)?;
        bytes.push((top * 16 + bottom) as u8);
        i += 2;
    }
    Ok(bytes)
}

fn pragma(state: &mut State, token: &Token, program: &mut Node, pending: &mut Pending) -> Parsed<()> {
    let words = &token.pragma;
    if words.is_empty() {
        return state.fail("empty pragma");
    }
    let line = token.line;
    let crash = |what: &str| Fault::Crash(format!("line {line}: #pragma {what}"));
    let rest = &words[1..];
    let first = || rest.first().ok_or_else(|| crash("is missing its value"));
    let word = |node: &Node| node.as_str().map(String::from).ok_or_else(|| crash("takes a word, not a string"));
    let integer = |node: &Node| -> Parsed<Node> { int_node(python_int(&word(node)?, 0)?) };
    let Some(key) = words[0].as_str() else {
        return Err(crash("name is a string"));
    };
    match key {
        "natives" => program["natives"] = first()?.clone(),
        "magic" => program["magic"] = magic(&word(first()?)?)?,
        "unk_14" => program["unk_14"] = integer(first()?)?,
        "header_padding" => program["header_padding"] = first()?.clone(),
        "globals" => program["global_num"] = integer(first()?)?,
        "strings" => {
            let strings =
                rest.iter().map(|word| word.get("string").cloned().ok_or_else(|| crash("strings are quoted")));
            pending.strings = Some(strings.collect::<Parsed<Vec<Node>>>()?);
        }
        "padding" => pending.padding = Some(rest.first().cloned().unwrap_or_else(|| Node::from(""))),
        "padding_of" => {
            let name = word(first()?)?;
            program["padding_of"][name.as_str()] = rest.get(1).cloned().unwrap_or_else(|| Node::from(""));
        }
        "string_pool" => program["string_pool"] = Node::Bool(true),
        "target" => {
            // the scripts a mod patch changes: #pragma target map s51 (see patches.py)
            let values: Vec<Node> =
                rest.iter().skip(1).map(|word| word.get("string").unwrap_or(word).clone()).collect();
            let kind = rest.first().and_then(Node::as_str).filter(|kind| TARGET_KINDS.contains(kind));
            let Some(kind) = kind.filter(|_| !values.is_empty()) else {
                return state.fail(format!("#pragma target takes {}, then what to pick", TARGET_KINDS.join(", ")));
            };
            if program.get("targets").is_none() {
                program["targets"] = Node::List(Vec::new());
            }
            program["targets"].as_array_mut().unwrap().push(node!({"kind": kind, "values": values}));
        }
        "override" => {
            // #pragma override, or #pragma override place_monster before a function of another name
            pending.override_ = true;
            pending.override_of = rest.first().map(word).transpose()?;
        }
        "every_frame" => pending.every_frame = true,
        "once" => {}
        _ => return state.fail(format!("unknown pragma {key}")),
    }
    Ok(())
}

/// The comment lines that open a file, when a blank line follows them.
fn file_doc(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut comments = Vec::new();
    for (number, line) in lines.iter().enumerate() {
        let stripped = strip(line);
        let Some(comment) = stripped.strip_prefix("//") else {
            break;
        };
        comments.push((number + 1, strip(comment).to_string()));
    }
    if comments.is_empty() || (lines.len() > comments.len() && !strip(lines[comments.len()]).is_empty()) {
        return String::new();
    }
    comment_block(&comments, comments.len() + 1)
}

/// parse.parse(text): the program a script's source holds.
pub fn parse(text: &str) -> Result<Node> {
    parse_with(text, &HashMap::new())
}

/// parse.parse(text, defines): with #defines in force before the first line (a mod patch's
/// LANGUAGE).
pub fn parse_with(text: &str, defines: &HashMap<String, String>) -> Result<Node> {
    Ok(parse_program(text, defines)?)
}

fn parse_program(text: &str, defines: &HashMap<String, String>) -> Parsed<Node> {
    let chars: Vec<char> = text.chars().collect();
    let mut state = State::new(tokenize(&chars, defines.clone(), CALLER_FRAMES + 2)?, CALLER_FRAMES + 1);
    if state.tokens[0].kind == Kind::Assembly {
        return Ok(node!({"kind": "program", "asm": state.tokens[0].text()}));
    }
    let header_padding = "00".repeat(stb::HEADER_SIZE - stb::HEADER_FIELDS_SIZE);
    let mut program = node!({
        "kind": "program",
        "natives": null,
        "magic": "SB2\0",
        "unk_14": 0,
        "header_padding": header_padding,
        "globals": [],
        "global_num": null,
        "main": null,
        "programs": [],
        "functions": [],
        "string_pool": false,
        "padding_of": {},
        "declarations": {"natives": [], "programs": [], "inlines": [], "constants": [], "kinds": {}},
    });
    let doc = file_doc(text);
    if !doc.is_empty() {
        program["doc"] = Node::from(doc);
    }
    let mut pending = Pending::default();
    while state.peek(0).kind != Kind::Eof {
        let token = state.take();
        let word = if token.kind == Kind::Name { token.text() } else { "" };
        match token.kind {
            Kind::Pragma => pragma(&mut state, &token, &mut program, &mut pending)?,
            Kind::Asm => {
                let function = asm_function(&mut state, &token)?;
                program["functions"].as_array_mut().unwrap().push(function);
            }
            Kind::Include => program["functions"].as_array_mut().unwrap().push(node!({
                "kind": "include",
                "path": &token.path,
                "system": token.system,
            })),
            Kind::Assembly => return state.fail("#pragma assembly must start the file"),
            Kind::Define => definition(&mut state, &token, &mut program)?,
            _ if word == "enum" => enumeration(&mut state, &mut program)?,
            _ if word == "PROGRAM" => program_entry(&mut state, &mut program)?,
            _ if request(word).is_some() => {
                // NEW_FLAG(name): a number build gives the mod (numbers.py)
                state.expect("(")?;
                let name = state.name()?;
                state.expect(")")?;
                state.expect(";")?;
                if program.get("requests").is_none() {
                    program["requests"] = Node::List(Vec::new());
                }
                program["requests"].as_array_mut().unwrap().push(node!({"kind": request(word), "name": name}));
            }
            _ if word == "MAIN" => {
                state.expect("(")?;
                program["main"] = Node::from(state.name()?);
                state.expect(")")?;
                state.expect(";")?;
            }
            _ if word == "typedef" => {
                let base = type_name(&mut state, &NATIVE_TYPES)?;
                let name = state.name()?;
                state.expect(";")?;
                program["declarations"]["kinds"][name.as_str()] = Node::from(&base);
                state.kinds.insert(name, base);
            }
            _ if word == "extern" => {
                let native = native_declaration(&mut state)?;
                declarations(&mut program, "natives").push(native);
            }
            _ if word == "static" => {
                state.expect("inline")?;
                let inline = state.framed(inline_definition)?;
                declarations(&mut program, "inlines").push(inline);
            }
            _ if RETURN_TYPES.contains(&word) => {
                let name = state.name()?;
                if state.at("(", 0) {
                    let doc = token.doc.as_deref();
                    let function = state.framed(|state| function_definition(state, &pending, word, &name, doc))?;
                    program["functions"].as_array_mut().unwrap().push(function);
                    pending = Pending::default();
                } else if word == "void" {
                    return state.fail("variables cannot be void");
                } else {
                    state.pos -= 1;
                    for declared in declarators(&mut state)? {
                        if declared.size != Size::None {
                            return state.fail("global arrays are not supported: the script VM cannot index a global");
                        }
                        if declared.pointer {
                            return state.fail(
                                "global pointers are not supported: the script VM cannot index through a global",
                            );
                        }
                        if !matches!(declared.init, Init::None) {
                            return state.fail(format!(
                                "globals start at 0 when the script loads; set {} in a program instead",
                                declared.name
                            ));
                        }
                        program["globals"].as_array_mut().unwrap().push(node!({"name": declared.name, "type": word}));
                    }
                }
            }
            _ => {
                state.pos -= 1;
                let found = state.shown(&token)?;
                return state.fail(format!("expected a declaration, found {found}"));
            }
        }
    }
    if program["global_num"].is_null() {
        program["global_num"] = Node::from(program["globals"].as_array().unwrap().len());
    }
    Ok(with_own_padding(program))
}

/// Functions take the padding #pragma padding_of gave them by name; padding_of keeps the rest.
fn with_own_padding(mut program: Node) -> Node {
    let mut padding_of = program["padding_of"].as_object().cloned().unwrap_or_default();
    let functions: Vec<Node> = std::mem::take(&mut program["functions"])
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|function| {
            if function.kind() != "function" {
                return function;
            }
            match function["name"].as_str().and_then(|name| padding_of.remove(name)) {
                Some(padding) => function.with("padding", padding),
                None => function,
            }
        })
        .collect();
    program["functions"] = Node::List(functions);
    program["padding_of"] = Node::Map(padding_of);
    program
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dump(text: &str) -> String {
        parse(text).unwrap().to_string()
    }

    fn error(text: &str) -> String {
        parse(text).unwrap_err().to_string()
    }

    const HEADER: &str = r#"{"kind":"program","natives":null,"magic":"SB2\u0000","unk_14":0,"header_padding":"000000000000000000000000000000000000000000000000000000000000000000000000","#;

    #[test]
    fn for_loops_continue_and_notes() {
        // the step runs before the continue; a note above a line goes to every statement that
        // starts on it, as parse.py's per-name docs give it
        let text = "// Doc.\n\nint g;\nvoid f(int a) {\n  // note: first\n  for (int i = 0; i < a; i++) { if (i) continue; g += i; }\n}\n";
        let want = r#""globals":[{"name":"g","type":"int"}],"global_num":1,"main":null,"programs":[],"functions":[{"kind":"function","name":"f","returns":"void","params":[{"name":"a","type":"int","array":false}],"locals":[{"name":"i","type":"int","size":null}],"body":[{"kind":"expr","expr":{"kind":"assign","target":{"kind":"var","name":"i"},"value":{"kind":"int","value":0}},"line":6,"note":"first"},{"kind":"while","cond":{"kind":"binary","op":"<","left":{"kind":"var","name":"i"},"right":{"kind":"var","name":"a"}},"body":[{"kind":"if","cond":{"kind":"var","name":"i"},"then":[{"kind":"expr","expr":{"kind":"compound","op":"+","target":{"kind":"var","name":"i"},"value":{"kind":"int","value":1}}},{"kind":"continue","line":6,"note":"first"}],"else":null,"line":6,"note":"first"},{"kind":"expr","expr":{"kind":"compound","op":"+","target":{"kind":"var","name":"g"},"value":{"kind":"var","name":"i"}},"line":6,"note":"first"},{"kind":"expr","expr":{"kind":"compound","op":"+","target":{"kind":"var","name":"i"},"value":{"kind":"int","value":1}}}],"line":6}],"unused_strings":[],"padding":null}],"string_pool":false,"padding_of":{},"declarations":{"natives":[],"programs":[],"inlines":[],"constants":[],"kinds":{}},"doc":"Doc."}"#;
        assert_eq!(dump(text), format!("{HEADER}{want}"));
    }

    #[test]
    fn defines_enums_and_literals() {
        let text = "#define N 3\nenum e { A, B = -2 };\nvoid f() { int t[N] = {1, 2}; x = 'a' + 0x10 + 1.5f; }\n";
        let want = r#""globals":[],"global_num":0,"main":null,"programs":[],"functions":[{"kind":"function","name":"f","returns":"void","params":[],"locals":[{"name":"t","type":"int","size":3}],"body":[{"kind":"expr","expr":{"kind":"assign","target":{"kind":"index","name":"t","index":{"kind":"int","value":0}},"value":{"kind":"int","value":1}},"line":3},{"kind":"expr","expr":{"kind":"assign","target":{"kind":"index","name":"t","index":{"kind":"int","value":1}},"value":{"kind":"int","value":2}},"line":3},{"kind":"expr","expr":{"kind":"assign","target":{"kind":"var","name":"x"},"value":{"kind":"binary","op":"+","left":{"kind":"binary","op":"+","left":{"kind":"int","value":97},"right":{"kind":"int","value":16}},"right":{"kind":"float","bits":1069547520}}},"line":3}],"unused_strings":[],"padding":null}],"string_pool":false,"padding_of":{},"declarations":{"natives":[],"programs":[],"inlines":[],"constants":[{"name":"N","value":{"kind":"int","value":3},"enum":null},{"name":"A","value":{"kind":"int","value":0},"enum":"e"},{"name":"B","value":{"kind":"int","value":-2},"enum":"e"}],"kinds":{}}}"#;
        assert_eq!(dump(text), format!("{HEADER}{want}"));
    }

    #[test]
    fn pragmas_asm_and_do_loops() {
        let text = "#pragma magic 0x53423300\n#pragma padding_of f 00\nasm g arg 1 {\n  ret\n}\nvoid f() { do x--; while (x); }\n";
        let program = parse(text).unwrap();
        assert_eq!(program["magic"], "SB3\0");
        assert_eq!(program["padding_of"].to_string(), "{}");
        assert_eq!(
            program["functions"].to_string(),
            r#"[{"kind":"asm","name":"g","arg":1,"reason":"","lines":["  ret"]},{"kind":"function","name":"f","returns":"void","params":[],"locals":[],"body":[{"kind":"while","cond":{"kind":"int","value":1},"body":[{"kind":"expr","expr":{"kind":"compound","op":"-","target":{"kind":"var","name":"x"},"value":{"kind":"int","value":1}},"line":6},{"kind":"if","cond":{"kind":"unary","op":"!","operand":{"kind":"var","name":"x"}},"then":[{"kind":"break"}],"else":null}],"line":6}],"unused_strings":[],"padding":"00"}]"#
        );
    }

    #[test]
    fn conditions_read_as_python_reads_them() {
        let text = "#if LANGUAGE == LANGUAGE_FRENCH\nint fr;\n#elif LANGUAGE\nint other;\n#else\nint none;\n#endif\n";
        let defines: HashMap<String, String> =
            [("LANGUAGE", "2"), ("LANGUAGE_FRENCH", "2")].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let program = parse_with(text, &defines).unwrap();
        assert_eq!(program["globals"].to_string(), r#"[{"name":"fr","type":"int"}]"#);
        // Python's precedence, not C's: (3 & 1) == 1, not (2 == 1); / truncates
        for (condition, value) in [("3 & 1 == 1", true), ("!2 == 1", true), ("-7 / 2 == -3", true), ("1and 0", false)] {
            let globals = dump(&format!("#if {condition}\nint yes;\n#endif\n"));
            assert_eq!(globals.contains("\"yes\""), value, "{condition}");
        }
    }

    #[test]
    fn errors_keep_python_messages() {
        let cases = [
            ("void f() { x = 1 }", "line 1: expected ';', found '}'"),
            ("void f() { s = \"\\q\"; }", "line 1: unknown escape \\q"),
            ("void f() { s = \"\\xzz\"; }", "invalid literal for int() with base 16: 'zz'"),
            ("#if 1\nint a;\n", "line 1: #if without #endif"),
            ("/* open", "line 1: unterminated comment"),
            ("#if (1\n#endif\n", "line 1: cannot read the condition '(1'"),
            (
                "#pragma magic S\u{e9}\n",
                "'ascii' codec can't encode character '\\xe9' in position 1: ordinal not in range(128)",
            ),
            ("void f() { x = \u{3042}; }", "line 1: unexpected character '\u{3042}'"),
        ];
        for (text, message) in cases {
            assert_eq!(error(text), message, "{text:?}");
        }
    }

    #[test]
    fn nesting_stops_where_python_recursion_does() {
        // on a thread with a 1 MB stack, as Windows gives a main thread
        let thread = std::thread::Builder::new().stack_size(1 << 20);
        thread
            .spawn(|| {
                let nots = |count: usize| format!("void f() {{ x = {}1; }}", "!".repeat(count));
                assert!(parse(&nots(983)).is_ok());
                assert_eq!(error(&nots(984)), "line 1: nested too deeply");
                let parens = |count: usize| format!("void f() {{ x = {}1{}; }}", "(".repeat(count), ")".repeat(count));
                assert!(parse(&parens(196)).is_ok());
                assert!(parse(&parens(197)).is_err());
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
