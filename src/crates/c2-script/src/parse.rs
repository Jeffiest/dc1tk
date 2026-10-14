//! The C-like script language: the tokenizer, which is also the preprocessor (comments, trigraphs,
//! #if/#ifdef/#ifndef/#elif/#else/#endif, #define/#undef and macros, #include, #pragma, #line,
//! #error, asm blocks; see cpp.rs), and the recursive-descent parser that turns a file into a
//! program tree, as parse.py did, key for key, for the forms the decompiler writes. C89's other
//! forms parse to trees of forms the compiler has, or to the ones it lowers (lower.rs, and
//! composite.rs for C's pointers and composite types, which declarations give their C types,
//! ctype.rs); see docs/c-standard.md.
//!
//! Where parse.py keeps a #pragma's words as asm.tokenize gives them, a plain word is a string and
//! a quoted one {"string": text}.
//!
//! Errors carry parse.py's ValueError messages. Where parse.py fails with another exception (a
//! token with no text quoted in a message, a bad string in a #pragma, a #pragma missing its value,
//! recursion past Python's limit), this fails too, with its own message. An integer literal past
//! 32 bits is an error, as C has no type for it here.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use c2_py::{ascii_digits, int_base, is_space, repr, split, strip, IntError};
use indexmap::IndexMap;

use crate::cpp::{self, Macros};
use crate::ctype::{self, Composite, Structs, Ty};
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
const OPERATORS: [&str; 19] =
    ["==", "!=", "<=", ">=", "&&", "||", "<<", ">>", "++", "--", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "->"];
const SINGLES: &str = "(){}[],;=<>+-*/%&|!^~?:.";
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

/// An open #if: whether its enclosing code compiles, whether its current branch does, and whether
/// any branch has.
struct Block {
    parent: bool,
    on: bool,
    taken: bool,
    line: usize,
    /// whether its #else has been read, after which only #endif may come
    otherwise: bool,
}

/// The value of an #if or #elif condition, as C works it out (cpp::condition).
fn condition_value(text: &str, macros: &Macros, line: usize) -> Parsed<bool> {
    cpp::condition(text, macros)
        .or_else(|reason| lex_fail(line, format!("cannot read the condition {}: {reason}", repr(strip(text)))))
}

fn conditional_directive(
    word: &str,
    rest: &str,
    conditions: &mut Vec<Block>,
    macros: &Macros,
    line: usize,
) -> Parsed<()> {
    let compiling = conditions.last().is_none_or(|block| block.on);
    if matches!(word, "if" | "ifdef" | "ifndef") {
        let name = strip(before_comment(rest));
        let chosen = if word == "if" {
            compiling && condition_value(rest, macros, line)?
        } else {
            compiling && (macros.defined(name) == (word == "ifdef"))
        };
        conditions.push(Block { parent: compiling, on: chosen, taken: chosen || !compiling, line, otherwise: false });
        return Ok(());
    }
    let Some(block) = conditions.last_mut() else {
        return lex_fail(line, format!("#{word} without #if"));
    };
    if block.otherwise && word != "endif" {
        return lex_fail(line, format!("#{word} after #else"));
    }
    match word {
        "elif" => {
            block.on = block.parent && !block.taken && condition_value(rest, macros, line)?;
            block.taken = block.taken || block.on;
        }
        "else" => {
            block.on = block.parent && !block.taken;
            block.taken = true;
            block.otherwise = true;
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
    /// A string literal written L"...".
    wide: bool,
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
            wide: false,
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
        } else if let Some((value, length)) = cpp::escape_value(&text[position + 1..]) {
            out.push(char::from_u32(value).unwrap_or('\0'));
            position += 1 + length;
        } else {
            return lex_fail(line, format!("unknown escape \\{}", next.map(String::from).unwrap_or_default()));
        }
    }
    if position >= text.len() {
        return lex_fail(line, "unterminated string");
    }
    Ok((out, position + 1))
}

/// A character literal, 'a' or '\n', as its character code: an escape's value is a char's, so
/// '\377' is -1 (char is signed), and another character is its code.
fn read_char(text: &[char], position: usize, line: usize) -> Parsed<(i64, usize)> {
    match cpp::character_constant(&text[position..]) {
        Ok((value, length)) if text[position + 1] == '\\' && value < 256 => {
            Ok((value as u8 as i8 as i64, position + length))
        }
        Ok((value, length)) => Ok((value as i64, position + length)),
        Err(reason) => lex_fail(line, reason),
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

/// The end of a directive's line, read on past a backslash that ends a line (C's line splicing),
/// and how many lines it joins.
fn directive_end(text: &[char], position: usize) -> (usize, usize) {
    let mut at = position;
    let mut joined = 0;
    loop {
        let end = line_end(text, at);
        let before = text[at..end].iter().rposition(|c| *c != '\r').map(|offset| at + offset);
        if end < text.len() && before.is_some_and(|last| text[last] == '\\') {
            joined += 1;
            at = end + 1;
        } else {
            return (end, joined);
        }
    }
}

/// #[ \t]*NAME[ \t]*REST at a #: the directive's name and the rest of its line, its spliced lines
/// joined.
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
    let (end, joined) = directive_end(text, i);
    let mut rest = text_of(&text[i.min(end)..end]);
    if joined > 0 {
        rest = rest.replace("\\\r\n", "").replace("\\\n", "");
    }
    Some((word, rest))
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
    } else if decimal(position) || (at(position) == '.' && decimal(position + 1)) {
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

/// The refusal of double, the one part of C89 this implementation leaves out.
const NO_DOUBLE: &str = "double is not supported: the VM has 32-bit floats only; use float";

/// The SDK's and definitions' headers by path, when the preprocessor reads them for their macros.
type Headers<'a> = Option<&'a IndexMap<String, String>>;

/// How deep headers may include headers.
const INCLUDE_DEPTH: usize = 32;

thread_local! {
    /// The macros a header leaves defined, by its path, its text and the macros before it.
    static HEADER_MACROS: RefCell<HashMap<u64, Macros>> = RefCell::new(HashMap::new());
}

/// The macros after #include <path>: the header read as text, as C reads it, from the macros
/// before it, so its #defines reach the including file's #if. Its declarations are read apart, as
/// before (library::expand), and its constants stay names in code, so nothing else changes. The
/// prelude (c2s.h, for C tools only) and a header the headers do not have (an error later) add
/// none.
fn header_macros(path: &str, headers: &IndexMap<String, String>, macros: Macros, depth: usize) -> Parsed<Macros> {
    let Some(text) = headers.get(path).filter(|_| path != crate::sdk::PRELUDE) else {
        return Ok(macros);
    };
    if depth > INCLUDE_DEPTH {
        return Err(Fault::Value(format!("#include <{path}>: headers include each other too deeply")));
    }
    let key = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (path, text).hash(&mut hasher);
        let mut defined: Vec<_> = macros.table.iter().collect();
        defined.sort_by(|a, b| a.0.cmp(b.0));
        for (name, definition) in defined {
            (name, &definition.params, &definition.body, macros.constants.contains(name)).hash(&mut hasher);
        }
        hasher.finish()
    };
    if let Some(found) = HEADER_MACROS.with(|cache| cache.borrow().get(&key).cloned()) {
        return Ok(found);
    }
    let chars: Vec<char> = text.chars().collect();
    let mut found = macros;
    tokenize_with(&chars, &mut found, path, CALLER_FRAMES + 2, Some(headers), depth + 1)
        .map_err(|fault| Fault::Value(format!("in <{path}>: {}", Error::from(fault))))?;
    HEADER_MACROS.with(|cache| cache.borrow_mut().insert(key, found.clone()));
    Ok(found)
}

/// The tokens of a file. macros are the #defines in force before the first line (a mod patch's
/// LANGUAGE), which stay constants in code; file is the name __FILE__ gives until a #line names
/// another. frames: Python frames in use, this function's included.
fn tokenize(source: &[char], mut macros: Macros, file: &str, frames: usize) -> Parsed<Vec<Token>> {
    tokenize_with(source, &mut macros, file, frames, None, 0)
}

/// tokenize, reading the headers given for their macros; macros ends as the file leaves them.
fn tokenize_with(
    source: &[char],
    macros: &mut Macros,
    file: &str,
    frames: usize,
    headers: Headers,
    depth: usize,
) -> Parsed<Vec<Token>> {
    // the text grows and shrinks as macros are replaced in it
    let mut text = cpp::trigraphs(source);
    let mut file = file.to_string();
    let mut regions: Vec<cpp::Region> = Vec::new();
    // __DATE__ and __TIME__, read from the clock when first used
    let mut when: Option<(String, String)> = None;
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
        let directive = if c == '#' { directive_at(&text, position) } else { None };
        if let Some((word, rest)) = &directive {
            if CONDITIONALS.contains(&word.as_str()) {
                conditional_directive(word, rest, &mut conditions, macros, line)?;
                let (end, joined) = directive_end(&text, position);
                line += joined;
                position = end;
                continue;
            }
        }
        if conditions.last().is_some_and(|block| !block.on) {
            position = line_end(&text, position);
            continue;
        }
        if let Some((word, rest)) = &directive {
            let (end, joined) = directive_end(&text, position);
            let mut next_line = line + joined;
            match word.as_str() {
                "undef" => macros.undefine(strip(before_comment(rest))),
                "define" => {
                    let (name, definition) = cpp::parse_define(rest).or_else(|reason| lex_fail(line, reason))?;
                    if cpp::PREDEFINED.contains(&name.as_str()) || name == "defined" {
                        return lex_fail(line, format!("{name} cannot be #defined"));
                    }
                    let same = |old: &cpp::Macro| {
                        let spaced = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
                        old.params == definition.params && spaced(&old.body) == spaced(&definition.body)
                    };
                    if macros.table.get(&name).is_some_and(|old| !same(old)) {
                        return lex_fail(line, format!("{name} is #defined again differently; #undef it first"));
                    }
                    if definition.params.is_some() {
                        macros.define(&name, definition, false);
                    } else {
                        let words: Vec<String> = split_first_word(rest).into_iter().map(String::from).collect();
                        let constant = match words.get(1) {
                            Some(value) => constant_of(value, frames + 1)?.is_some(),
                            None => false,
                        };
                        macros.define(&name, definition, constant);
                        let mut token = Token::new(Kind::Define, line, None);
                        token.words = words;
                        tokens.push(token);
                    }
                }
                "include" => {
                    let rest = strip(rest);
                    let mut token = Token::new(Kind::Include, line, None);
                    if rest.starts_with('<') && rest.ends_with('>') {
                        token.path = strip(&rest[1..rest.len() - 1]).to_string();
                        token.system = true;
                        if let Some(headers) = headers {
                            *macros = header_macros(&token.path, headers, std::mem::take(macros), depth)?;
                        }
                    } else {
                        let words = asm_words(&rest.chars().collect::<Vec<char>>())?;
                        match words.as_slice() {
                            [word] if word.as_object().is_some() => {
                                token.path = word["string"].as_str().unwrap().to_string()
                            }
                            _ => return lex_fail(line, "expected #include \"path\" or #include <path>"),
                        }
                    }
                    tokens.push(token);
                }
                "pragma" => {
                    let words = asm_words(&rest.chars().collect::<Vec<char>>())?;
                    if words.first().is_some_and(|word| *word == "assembly") {
                        let rest = text_of(&text[(end + 1).min(text.len())..]);
                        tokens.push(Token::new(Kind::Assembly, line, Some(rest)));
                        return Ok(tokens);
                    }
                    let mut token = Token::new(Kind::Pragma, line, None);
                    token.pragma = words;
                    tokens.push(token);
                }
                "error" => return lex_fail(line, format!("#error {}", strip(rest))),
                "line" => {
                    let expanded =
                        cpp::expand_text(rest, macros, true, &[]).or_else(|reason| lex_fail(line, reason))?;
                    let words = asm_words(&expanded.replace(cpp::PAINT, " ").chars().collect::<Vec<char>>())?;
                    let number = words.first().and_then(Node::as_str).and_then(|word| word.parse::<usize>().ok());
                    let name = words.get(1).map(|word| word.get("string").and_then(Node::as_str));
                    match (number, name, words.len()) {
                        (Some(number), None, 1) => next_line = number.saturating_sub(1),
                        (Some(number), Some(Some(name)), 2) => {
                            next_line = number.saturating_sub(1);
                            file = name.to_string();
                        }
                        _ => return lex_fail(line, "#line takes a line number and, after it, a file name in quotes"),
                    }
                }
                other => return lex_fail(line, format!("unknown directive #{other}")),
            }
            line = next_line;
            position = end;
            continue;
        }
        if c == '#' && text[position + 1..line_end(&text, position)].iter().all(|c| is_space(*c)) {
            // the null directive
            position = line_end(&text, position);
            continue;
        }
        if is_space(c) {
            position += 1;
        } else if starts_with(&text, position, "//") {
            let end = line_end(&text, position);
            let start = (0..position).rev().find(|&at| text[at] == '\n').map_or(0, |at| at + 1);
            if strip(&text_of(&text[start..position])).is_empty() {
                comments.push((line, strip(&text_of(&text[position + 2..end])).to_string()));
            }
            position = end;
        } else if starts_with(&text, position, "/*") {
            let Some(end) = find(&text, position + 2, "*/") else {
                return lex_fail(line, "unterminated comment");
            };
            line += count_lines(&text, position, end);
            position = end + 2;
        } else if c == '"' {
            let (value, end) = read_string(&text, position, line)?;
            tokens.push(Token::new(Kind::Str, line, Some(value)));
            position = end;
        } else if c2_py::is_digit(c) || (c == '.' && text.get(position + 1).is_some_and(|next| next.is_ascii_digit())) {
            let Some((mut word, suffix, end)) = number_at(&text, position) else {
                return Err(Fault::Crash(format!("line {line}: cannot read the number at {}", repr(&c.to_string()))));
            };
            let lower = word.to_lowercase();
            let hexadecimal = lower.starts_with("0x");
            let floating =
                !hexadecimal && (word.contains('.') || lower.contains('e') || suffix.to_lowercase().contains('f'));
            if floating && !word.contains('.') && !lower.contains('e') {
                word.push_str(".0");
            }
            if floating && suffix.to_lowercase().contains('l') {
                return lex_fail(line, NO_DOUBLE.replace("double is", "long double is"));
            }
            let mut token = Token::new(if floating { Kind::Float } else { Kind::Int }, line, Some(word));
            token.words = vec![suffix];
            tokens.push(token);
            position = end;
        } else if c == '\'' {
            let (value, end) = read_char(&text, position, line)?;
            tokens.push(Token::new(Kind::Int, line, Some(value.to_string())));
            position = end;
        } else if c == 'L' && text.get(position + 1) == Some(&'\'') {
            // a wide character: its Shift-JIS code, which wchar_t holds
            let (value, end) =
                cpp::character_constant(&text[position + 1..]).or_else(|reason| lex_fail(line, reason))?;
            let value = match char::from_u32(value).filter(|_| text[position + 2] != '\\') {
                Some(character) => c2_disc::text::encode(&character.to_string())
                    .ok_or_else(|| Fault::Value(format!("line {line}: {character:?} has no Shift-JIS code")))?
                    .iter()
                    .fold(0u32, |code, byte| code << 8 | u32::from(*byte)),
                None => value,
            };
            tokens.push(Token::new(Kind::Int, line, Some(value.to_string())));
            position += 1 + end;
        } else if c == 'L' && text.get(position + 1) == Some(&'"') {
            // a wide string: an array of wchar_t, each a character's Shift-JIS code
            let (value, end) = read_string(&text, position + 1, line)?;
            let mut token = Token::new(Kind::Str, line, Some(value));
            token.wide = true;
            tokens.push(token);
            position = end;
        } else if c == cpp::PAINT || c.is_ascii_alphabetic() || c == '_' {
            // a painted name is one a macro's own replacement held, which is never expanded
            let painted = c == cpp::PAINT;
            let start = position + usize::from(painted);
            let end = cpp::word_end(&text, start);
            let name = text_of(&text[start..end]);
            if name == "asm" && !painted {
                let (token, after) = read_asm(&text, end, line)?;
                tokens.push(token);
                line += count_lines(&text, position, after);
                position = after;
                continue;
            }
            if !painted && cpp::expandable(macros, &name, false).is_some() {
                regions.retain(|region| region.end > position);
                if !cpp::disabled(&regions, &name, position)
                    && cpp::expand_at(&mut text, position, end, macros, false, &mut regions)
                        .or_else(|reason| lex_fail(line, reason))?
                {
                    continue;
                }
            }
            position = end;
            let token = match name.as_str() {
                "" => continue,
                "__LINE__" => Token::new(Kind::Int, line, Some(line.to_string())),
                "__FILE__" => Token::new(Kind::Str, line, Some(file.clone())),
                "__DATE__" | "__TIME__" => {
                    let (date, time) = when.get_or_insert_with(cpp::translation_time);
                    Token::new(Kind::Str, line, Some(if name == "__DATE__" { date.clone() } else { time.clone() }))
                }
                "__STDC__" => Token::new(Kind::Int, line, Some("1".into())),
                "double" => return lex_fail(line, NO_DOUBLE),
                _ => {
                    let mut token = Token::new(Kind::Name, line, Some(name));
                    let doc = comment_block(&comments, line);
                    if !doc.is_empty() {
                        token.doc = Some(doc);
                    }
                    token
                }
            };
            tokens.push(token);
        } else if let Some(op) = LONG_OPERATORS.iter().find(|op| starts_with(&text, position, op)) {
            tokens.push(Token::new(Kind::Op, line, Some(op.to_string())));
            position += 3;
        } else if let Some(op) = OPERATORS.iter().find(|op| starts_with(&text, position, op)) {
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
    function: String,
    params: Vec<Node>,
    locals: Vec<Node>,
    /// The blocks open where the parser is, outermost (the parameters) first: the names declared
    /// in each and the variables they stand for. A local gets a slot for the whole function, so a
    /// name declared again in a block beside another reuses its slot; one that hides a name of an
    /// enclosing block gets a slot of its own, under another name.
    blocks: Vec<HashMap<String, String>>,
    /// static locals' hidden globals, by name
    statics: HashMap<String, Node>,
    /// labels defined and used, with the line of their first use
    labels: Vec<String>,
    gotos: Vec<(String, usize)>,
    /// typedefs declared in the function
    typedefs: Vec<String>,
}

impl Scope {
    fn new(function: &str, params: Vec<Node>) -> Scope {
        let names = params.iter().map(|param| (field(param, "name").to_string(), field(param, "name").to_string()));
        Scope {
            function: function.to_string(),
            blocks: vec![names.collect()],
            params,
            locals: Vec::new(),
            statics: HashMap::new(),
            labels: Vec::new(),
            gotos: Vec::new(),
            typedefs: Vec::new(),
        }
    }
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
    /// The enumerators so far, for constant expressions.
    enumerators: HashMap<String, i128>,
    /// Enumerators an enum defined in a declaration gave, for the program's constants.
    constants_found: Vec<Node>,
    /// typedef'd value kinds and their base types.
    kinds: HashMap<String, String>,
    scope: Option<Scope>,
    /// The globals declared so far, by name, and the hidden globals of static locals.
    globals: HashMap<String, Node>,
    statics: Vec<Node>,
    /// Python frames in use, the current function's included.
    depth: usize,
    /// Whether a call went past Python's recursion limit: everything fails from there, as the
    /// RecursionError would.
    overflow: Cell<bool>,
    /// #pragma std c89 has been read: the functions and globals after it get C's semantics.
    strict: bool,
    /// The structs and unions declared so far, by tag.
    structs: Structs,
    /// typedefs of types beyond the scalars (pointers, structs, arrays, functions).
    types: HashMap<String, Ty>,
    /// Unnamed structs so far, for their tags.
    anonymous: usize,
    /// The typedefs of the headers included so far, read when the headers are given (parse_source):
    /// what a type's size and slots need. They are not type words here, as a script's names may
    /// be the same (sdk::resolve reads the headers' kinds for the rest).
    header_kinds: HashMap<String, String>,
}

impl State {
    fn new(tokens: Vec<Token>, depth: usize) -> State {
        let (constants, kinds) = (HashMap::new(), HashMap::new());
        State {
            tokens: tokens.into(),
            pos: 0,
            constants,
            enumerators: HashMap::new(),
            constants_found: Vec::new(),
            kinds,
            scope: None,
            globals: HashMap::new(),
            statics: Vec::new(),
            depth,
            overflow: Cell::new(false),
            strict: false,
            structs: Structs::new(),
            // what <stdarg.h>'s va_list stands for: a pointer into a variadic call's arguments
            types: HashMap::from([("__va_list".to_string(), ctype::pointer(ctype::int()))]),
            anonymous: 0,
            header_kinds: HashMap::new(),
        }
    }

    /// A variable's declaration, by the name code uses for it (storage_name): a parameter, a local,
    /// a static local's hidden global, or a global.
    fn variable(&self, name: &str) -> Option<Node> {
        if let Some(scope) = &self.scope {
            if let Some(hidden) = scope.statics.get(name) {
                return Some(hidden.clone());
            }
            let local = scope.params.iter().chain(&scope.locals).find(|entry| entry["name"] == *name);
            if let Some(local) = local {
                return Some(local.clone());
            }
        }
        self.globals.get(name).cloned()
    }

    /// The name code uses for a variable it names: the variable the innermost block declaring the
    /// name gives it (another name for one that hides an outer one, or a static local's hidden
    /// global), or the name itself.
    fn storage_name(&self, name: &str) -> String {
        let blocks = self.scope.as_ref().map_or(&[][..], |scope| scope.blocks.as_slice());
        blocks.iter().rev().find_map(|block| block.get(name)).cloned().unwrap_or_else(|| name.to_string())
    }

    /// A scalar type's base: a typedef's (this file's or a header's), or the name.
    fn base_kind(&self, kind: &str) -> String {
        self.kinds.get(kind).or_else(|| self.header_kinds.get(kind)).cloned().unwrap_or_else(|| kind.to_string())
    }

    /// Opens a block of declarations, in a function.
    fn open_block(&mut self) {
        if let Some(scope) = &mut self.scope {
            scope.blocks.push(HashMap::new());
        }
    }

    fn close_block(&mut self) {
        if let Some(scope) = &mut self.scope {
            scope.blocks.pop();
        }
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

/// An integer literal's value: hexadecimal after 0x, octal after a leading 0, as in C.
fn int_value(text: &str) -> Parsed<i128> {
    let hexadecimal = text.starts_with("0x") || text.starts_with("0X");
    if !hexadecimal && text.len() > 1 && text.starts_with('0') && text.chars().all(|c| c.is_ascii_digit()) {
        return python_int(&text[1..], 8);
    }
    python_int(text, if hexadecimal { 16 } else { 10 })
}

/// An unsigned int literal, held as the VM's 32 bits.
fn unsigned_literal(value: i128) -> Parsed<Node> {
    Ok(int_literal(value as u32 as i32 as i128)?.with("unsigned", true))
}

fn literal(state: &State, token: &Token) -> Parsed<Node> {
    state.leaf(2);
    if token.kind == Kind::Int {
        let value = int_value(token.text())?;
        // C89: a literal too large for int (and long) is unsigned; one too large for that has no type
        let unsigned = token.words.first().is_some_and(|suffix| suffix.contains(['u', 'U']));
        if value > u32::MAX as i128 {
            return state.fail(format!("{} does not fit in 32 bits", token.text()));
        }
        if unsigned || value > i32::MAX as i128 {
            return unsigned_literal(value);
        }
        return int_literal(value);
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
            Kind::Int | Kind::Float if state.at("[", 0) => {
                // 1[a] is a[1]
                let number = literal(state, &token)?;
                return postfix_operators(state, number);
            }
            Kind::Int | Kind::Float => return literal(state, &token),
            Kind::Str => {
                // adjacent string literals are one
                let mut text = token.text().to_string();
                while state.peek(0).kind == Kind::Str {
                    let next = state.take();
                    if next.wide != token.wide {
                        return state.fail("a wide string literal cannot be joined to a plain one");
                    }
                    text.push_str(next.text());
                }
                if token.wide {
                    return postfix_operators(state, wide_string(state, &text)?);
                }
                return Ok(node!({"kind": "string", "text": text}));
            }
            Kind::Op if token.text() == "(" => {
                let node = expression(state)?;
                state.expect(")")?;
                return postfix_operators(state, node);
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
            match name {
                "__offsetof" => return offsetof(state),
                "__va_arg" => return va_arg(state),
                "__va_start" => return va_start(state),
                _ => {}
            }
            let storage = state.storage_name(name);
            if state.variable(&storage).is_some() {
                // a call through a function pointer
                let callee = node!({"kind": "var", "name": storage});
                let call = node!({"kind": "call_ptr", "callee": callee, "args": arguments(state)?});
                return postfix_operators(state, call);
            }
            let call = node!({"kind": "call", "name": name, "args": arguments(state)?});
            return postfix_operators(state, call);
        }
        let node = named(state, name)?;
        postfix_operators(state, node)
    })
}

/// The postfix operators after a primary expression: s.m, p->m, a[i] (of any expression), a call
/// through a function pointer, x++ and x--.
fn postfix_operators(state: &mut State, mut node: Node) -> Parsed<Node> {
    loop {
        if state.at(".", 0) || state.at("->", 0) {
            let arrow = state.take().text() == "->";
            let name = state.name()?;
            node = node!({"kind": "member", "object": node, "name": name, "arrow": arrow});
        } else if state.at("[", 0) {
            state.take();
            let index = expression(state)?;
            state.expect("]")?;
            node = node!({"kind": "subscript", "array": node, "index": index});
        } else if state.at("(", 0) {
            let args = arguments(state)?;
            node = node!({"kind": "call_ptr", "callee": node, "args": args});
        } else if state.at("++", 0) || state.at("--", 0) {
            if !is_target(&node) {
                return state.fail("++ and -- need a variable");
            }
            let op = state.take();
            writable(state, &node)?;
            node = postfix(state, node, op.text());
        } else {
            return Ok(node);
        }
    }
}

/// L"...": the characters' Shift-JIS codes, then 0, as an array of wchar_t (composite.rs gives it
/// storage).
fn wide_string(state: &State, text: &str) -> Parsed<Node> {
    let mut codes = Vec::new();
    for character in text.chars() {
        let code = if (character as u32) < 0x80 {
            character as u32
        } else {
            match c2_disc::text::encode(&character.to_string()) {
                Some(bytes) => bytes.iter().fold(0u32, |code, byte| code << 8 | u32::from(*byte)),
                // an escape's own value
                None if (character as u32) < 0x100 => character as u32,
                None => return state.fail(format!("{character:?} has no Shift-JIS code")),
            }
        };
        codes.push(Node::from(code as i64));
    }
    codes.push(Node::from(0));
    Ok(node!({"kind": "wide", "codes": codes}))
}

/// offsetof(type, member) from <stddef.h>: the member's offset in C's bytes, a size_t constant. The
/// member may be a path (a.b[2].c).
fn offsetof(state: &mut State) -> Parsed<Node> {
    state.expect("(")?;
    let mut ty = abstract_type(state)?;
    state.expect(",")?;
    let mut bytes = 0;
    loop {
        let name = state.name()?;
        let Ty::Struct(tag) = &ty else {
            return state.fail(format!("offsetof needs a struct or union, not {}", ty.shown()));
        };
        let Some((_, offset, member)) = ctype::member(&state.structs, tag, &name) else {
            return state.fail(format!("{} has no member {name}", ty.shown()));
        };
        bytes += offset;
        ty = member;
        while state.at("[", 0) {
            state.take();
            let index = constant_expression(state)?;
            state.expect("]")?;
            let Ty::Array(of, _) = &ty else {
                return state.fail(format!("{name} is not an array"));
            };
            bytes += index * of.bytes(&state.structs);
            ty = (**of).clone();
        }
        if !state.at(".", 0) {
            break;
        }
        state.take();
    }
    state.expect(")")?;
    Ok(size_literal(bytes))
}

/// va_arg(ap, type) from <stdarg.h>: the next of a variadic function's other arguments.
fn va_arg(state: &mut State) -> Parsed<Node> {
    state.expect("(")?;
    let list = assignment(state)?;
    state.expect(",")?;
    let ty = abstract_type(state)?;
    state.expect(")")?;
    Ok(node!({"kind": "va_arg", "list": list, "ctype": ty.to_node()}))
}

/// va_start(ap, last) from <stdarg.h>: ap at the first of the arguments after last, which must be
/// the function's last parameter.
fn va_start(state: &mut State) -> Parsed<Node> {
    state.expect("(")?;
    let list = assignment(state)?;
    state.expect(",")?;
    let last = state.name()?;
    state.expect(")")?;
    let params = state.scope.as_ref().map_or(&[][..], |scope| scope.params.as_slice());
    if params.last().is_none_or(|param| param["name"] != *last) {
        return state.fail(format!("va_start names the last parameter before the ..., not {last}"));
    }
    Ok(node!({"kind": "va_start", "list": list}))
}

/// A variable, or an element of an array, by the name code wrote: x, a[i], m[i][j].
#[inline(never)]
fn named(state: &mut State, name: &str) -> Parsed<Node> {
    let name = state.storage_name(name);
    if !state.at("[", 0) {
        return Ok(node!({"kind": "var", "name": name}));
    }
    if state.variable(&name).is_some_and(|entry| entry.get("ctype").is_some()) {
        // a variable of C's other types: each [] subscripts what the one before gave
        let mut node = node!({"kind": "var", "name": name});
        while state.at("[", 0) {
            state.take();
            let index = expression(state)?;
            state.expect("]")?;
            node = node!({"kind": "subscript", "array": node, "index": index});
        }
        return Ok(node);
    }
    let mut indices = Vec::new();
    while state.at("[", 0) {
        state.expect("[")?;
        indices.push(expression(state)?);
        state.expect("]")?;
    }
    if indices.len() == 1 && state.variable(&name).is_none_or(|entry| entry.get("dims").is_none()) {
        return Ok(node!({"kind": "index", "name": name, "index": indices.pop().unwrap()}));
    }
    element(state, &name, indices)
}

/// The inner sizes of a multi-dimensional array: all of a local's after the first, all of a
/// parameter's (whose first is the pointer's).
fn inner_sizes(entry: &Node) -> Vec<i64> {
    let dims: Vec<i64> =
        entry["dims"].as_array().map_or(Vec::new(), |dims| dims.iter().filter_map(Node::as_i64).collect());
    if entry.get("array").is_some() {
        dims
    } else {
        dims.into_iter().skip(1).collect()
    }
}

/// a[i][j] of a multi-dimensional array: the element at i * columns + j, or, with fewer indices
/// than the array has sizes, the address of that row (a[i] passes a row).
fn element(state: &State, name: &str, indices: Vec<Node>) -> Parsed<Node> {
    let Some(entry) = state.variable(name) else {
        return state.fail(format!("unknown variable {name}"));
    };
    let inner = inner_sizes(&entry);
    if indices.len() > inner.len() + 1 {
        return state.fail(format!("{name} has {} sizes, not {}", inner.len() + 1, indices.len()));
    }
    let mut total = Node::Null;
    for (at, index) in indices.iter().enumerate() {
        let stride: i64 = inner[at..].iter().product();
        let term = if stride == 1 {
            index.clone()
        } else {
            node!({"kind": "binary", "op": "*", "left": index.clone(), "right": {"kind": "int", "value": stride}})
        };
        total = if total.is_null() { term } else { node!({"kind": "binary", "op": "+", "left": total, "right": term}) };
    }
    let cell = node!({"kind": "index", "name": name, "index": total});
    if indices.len() == inner.len() + 1 {
        return Ok(cell);
    }
    let row: i64 = inner[indices.len() - 1..].iter().product();
    Ok(node!({"kind": "addr", "target": cell, "row": row}))
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
    matches!(node.kind(), "var" | "index" | "deref" | "member" | "subscript")
}

/// The variable an assignment target writes: x, a[i] or *p.
fn target_name(target: &Node) -> &str {
    match target.kind() {
        "deref" => field(&target["operand"], "name"),
        _ => field(target, "name"),
    }
}

/// Refuses writing a const object: a const variable, an element of a const array, or what a
/// pointer to const points at.
fn writable(state: &State, target: &Node) -> Parsed<()> {
    let name = target_name(target);
    let Some(entry) = state.variable(name) else {
        return Ok(());
    };
    let pointer = entry.get("pointer").is_some() || entry.get("array").is_some_and(|array| *array == Node::Bool(true));
    let constant = if target.kind() == "var" || !pointer {
        entry.get("const").is_some()
    } else {
        entry.get("pointee_const").is_some()
    };
    if constant {
        let shown = if target.kind() == "var" { name.to_string() } else { format!("what {name} points at") };
        return state.fail(format!("{shown} is const"));
    }
    Ok(())
}

// ---- types ---------------------------------------------------------------------------------

const STORAGE: [&str; 5] = ["static", "extern", "auto", "register", "typedef"];
const QUALIFIERS: [&str; 2] = ["const", "volatile"];
const INTEGER_WORDS: [&str; 6] = ["char", "short", "int", "long", "signed", "unsigned"];
/// The integer types besides int, as declarations name them (char is signed; long is int).
pub const INTEGER_TYPES: [&str; 5] = ["unsigned int", "short", "unsigned short", "char", "unsigned char"];

/// Whether the token offset ahead starts a type: a keyword of one, or a typedef of this file.
fn starts_type(state: &State, offset: usize) -> bool {
    let token = state.token(offset);
    token.kind == Kind::Name && type_word(state, token.text())
}

fn type_word(state: &State, text: &str) -> bool {
    STORAGE.contains(&text)
        || QUALIFIERS.contains(&text)
        || INTEGER_WORDS.contains(&text)
        || ["float", "string", "void", "enum", "struct", "union"].contains(&text)
        || state.kinds.contains_key(text)
        || state.types.contains_key(text)
}

/// What a declaration's specifiers say: the type, the storage class, and the qualifiers.
#[derive(Clone, Debug, Default)]
struct Spec {
    kind: String,
    storage: Option<String>,
    constant: bool,
    volatile: bool,
    /// A struct or union, or a typedef of a type beyond the scalars.
    ty: Option<Ty>,
}

/// The type the integer keywords of a declaration name, as C 3.5.2 lists their combinations.
fn integer_type(words: &[String]) -> std::result::Result<&'static str, String> {
    let count = |word: &str| words.iter().filter(|known| *known == word).count();
    let shown = || words.join(" ");
    if count("long") > 1 {
        return Err("long long is C99, and the VM's integers are 32 bits".into());
    }
    if ["char", "short", "int", "signed", "unsigned"].iter().any(|word| count(word) > 1)
        || (count("signed") > 0 && count("unsigned") > 0)
    {
        return Err(format!("{} is not a type", shown()));
    }
    let unsigned = count("unsigned") > 0;
    if count("char") > 0 {
        if count("short") + count("long") + count("int") > 0 {
            return Err(format!("{} is not a type", shown()));
        }
        return Ok(if unsigned { "unsigned char" } else { "char" });
    }
    if count("short") > 0 {
        if count("long") > 0 {
            return Err(format!("{} is not a type", shown()));
        }
        return Ok(if unsigned { "unsigned short" } else { "short" });
    }
    Ok(if unsigned { "unsigned int" } else { "int" })
}

/// A declaration's specifiers. header_kinds: an unknown name followed by a name or * is a value
/// kind from a header (bgm_file track;), which sdk.resolve reads as its base type.
fn specifiers(state: &mut State, header_kinds: bool) -> Parsed<Spec> {
    let mut spec = Spec::default();
    let mut words: Vec<String> = Vec::new();
    let mut base: Option<String> = None;
    let mut any = false;
    while state.token(0).kind == Kind::Name {
        let text = state.token(0).text().to_string();
        if STORAGE.contains(&text.as_str()) {
            if spec.storage.is_some() {
                return state.fail("a declaration has one storage class");
            }
            spec.storage = Some(text);
        } else if text == "const" {
            spec.constant = true;
        } else if text == "volatile" {
            spec.volatile = true;
        } else if INTEGER_WORDS.contains(&text.as_str()) && base.is_none() {
            words.push(text);
        } else if base.is_none() && words.is_empty() {
            if ["float", "string", "void"].contains(&text.as_str()) || state.kinds.contains_key(&text) {
                base = Some(text);
            } else if text == "struct" || text == "union" {
                state.take();
                let tag = struct_specifier(state, text == "union")?;
                base = Some(format!("{text} {tag}"));
                spec.ty = Some(Ty::Struct(tag));
                any = true;
                continue;
            } else if let Some(ty) = state.types.get(&text).cloned() {
                spec.ty = Some(ty);
                base = Some(text);
            } else if text == "enum" {
                state.take();
                let name = if state.at("{", 0) { None } else { Some(state.name()?) };
                if state.at("{", 0) {
                    // the enum is defined here; its members are the file's constants
                    let members = enumerators(state, name.as_deref())?;
                    state.constants_found.extend(members);
                }
                base = Some(name.map_or("int".to_string(), |name| format!("enum {name}")));
                any = true;
                continue;
            } else if header_kinds
                && (state.token(1).kind == Kind::Name
                    || (state.token(1).kind == Kind::Op && state.token(1).text() == "*"))
            {
                base = Some(text);
            } else {
                break;
            }
        } else {
            break;
        }
        state.take();
        any = true;
    }
    if !any {
        let found = state.shown(state.token(0))?;
        return state.fail(format!("expected a type, found {found}"));
    }
    spec.kind = match base {
        Some(base) if words.is_empty() => base,
        Some(base) => return state.fail(format!("{} {base} is not a type", words.join(" "))),
        // C89's implicit int: static x; const y = 1;
        None => integer_type(&words).or_else(|reason| state.fail(reason))?.to_string(),
    };
    Ok(spec)
}

/// struct TAG { members } or union TAG { ... } after its keyword (or TAG alone, naming one): the
/// tag, a made-up one for an unnamed struct. A member takes whole slots, so bit-fields are refused.
fn struct_specifier(state: &mut State, union: bool) -> Parsed<String> {
    let word = if union { "union" } else { "struct" };
    let tag = if state.peek(0).kind == Kind::Name {
        state.name()?
    } else if state.at("{", 0) {
        state.anonymous += 1;
        format!("__anonymous_{}", state.anonymous)
    } else {
        let found = state.shown(state.peek(0))?;
        return state.fail(format!("expected the name of a {word}, found {found}"));
    };
    let other = if union { "struct" } else { "union" };
    if state.structs.get(&tag).is_some_and(|known| known.union != union) {
        return state.fail(format!("{tag} is a {other}, not a {word}"));
    }
    if !state.at("{", 0) {
        // declared here, defined (or not) elsewhere: an incomplete type until then
        state.structs.entry(tag.clone()).or_insert(Composite { union, members: Vec::new() });
        return Ok(tag);
    }
    state.take();
    let mut members: Vec<(String, Ty)> = Vec::new();
    while !state.at("}", 0) {
        let spec = specifiers(state, true)?;
        if spec.storage.is_some() {
            return state.fail(format!("a member of {word} {tag} has no storage class"));
        }
        let base = base_ty(state, &spec)?;
        loop {
            let (name, ty) = declarator(state, base.clone())?;
            if name.is_empty() {
                return state.fail(format!("a member of {word} {tag} needs a name"));
            }
            if state.at(":", 0) {
                return state.fail("bit-fields are not supported: every member takes a slot of its own");
            }
            if members.iter().any(|(known, _)| *known == name) {
                return state.fail(format!("{word} {tag} has two members named {name}"));
            }
            complete(state, &name, &ty)?;
            members.push((name, ty));
            if !state.at(",", 0) {
                break;
            }
            state.take();
        }
        state.expect(";")?;
    }
    state.expect("}")?;
    if members.is_empty() {
        return state.fail(format!("{word} {tag} needs a member"));
    }
    let composite = Composite { union, members };
    if state.structs.get(&tag).is_some_and(|known| !known.members.is_empty() && *known != composite) {
        return state.fail(format!("{word} {tag} is defined twice"));
    }
    state.structs.insert(tag.clone(), composite);
    Ok(tag)
}

/// Refuses an object of a type with no size: void, a function, a struct not defined, an array
/// whose size nothing gave.
fn complete(state: &State, name: &str, ty: &Ty) -> Parsed<()> {
    match ty {
        Ty::Void => state.fail(format!("{name} cannot be void")),
        Ty::Function(..) => state.fail(format!("{name} is a function; declare functions at file scope")),
        Ty::Struct(tag) if state.structs.get(tag).is_none_or(|found| found.members.is_empty()) => {
            state.fail(format!("{name} is of {}, which is not defined", ty.shown()))
        }
        Ty::Array(_, -1) => state.fail(format!("{name}[] needs a size or a list of values")),
        Ty::Array(of, _) => complete(state, name, of),
        _ => Ok(()),
    }
}

/// The type specifiers give: a struct or typedef'd type as it is, void, or a scalar by its base
/// type (a typedef's, int for an enum; a header's value kind as it is named).
fn base_ty(state: &State, spec: &Spec) -> Parsed<Ty> {
    if let Some(ty) = &spec.ty {
        return Ok(ty.clone());
    }
    Ok(match spec.kind.as_str() {
        "void" => Ty::Void,
        kind if kind.starts_with("enum ") => ctype::int(),
        kind => Ty::Scalar(state.base_kind(kind)),
    })
}

/// A declarator, as C 3.5.4 reads one: *s (with their qualifiers), a name or a declarator in
/// parentheses, then [N]s and (parameters). The name ("" in a type name) and the type it gives
/// the name, built from base inside out: int *a[3] is an array of pointers, int (*f)(int) a pointer
/// to a function.
fn declarator(state: &mut State, base: Ty) -> Parsed<(String, Ty)> {
    let mut ty = base;
    while state.at("*", 0) {
        state.take();
        while state.at("const", 0) || state.at("volatile", 0) {
            state.take();
        }
        ty = ctype::pointer(ty);
    }
    let nested = state.at("(", 0)
        && (state.at("*", 1)
            || state.at("(", 1)
            || state.at("[", 1)
            || (state.token(1).kind == Kind::Name && !type_word(state, state.token(1).text())));
    if nested {
        // what follows the parentheses applies first: read it, then the declarator inside
        state.take();
        let inner = state.pos;
        let mut depth = 1;
        while depth > 0 {
            if state.peek(0).kind == Kind::Eof {
                return state.fail("expected ')'");
            }
            let token = state.take();
            if token.kind == Kind::Op {
                depth += match token.text() {
                    "(" => 1,
                    ")" => -1,
                    _ => 0,
                };
            }
        }
        let ty = suffixes(state, ty)?;
        let end = state.pos;
        state.pos = inner;
        let (name, ty) = declarator(state, ty)?;
        state.expect(")")?;
        state.pos = end;
        return Ok((name, ty));
    }
    let named = state.peek(0).kind == Kind::Name && !type_word(state, state.peek(0).text());
    let name = if named { state.name()? } else { String::new() };
    Ok((name, suffixes(state, ty)?))
}

/// The [N]s and (parameters) after a declarator's name, applied to ty from the last in.
fn suffixes(state: &mut State, ty: Ty) -> Parsed<Ty> {
    let mut found: Vec<std::result::Result<i64, (Vec<Ty>, bool)>> = Vec::new();
    loop {
        if state.at("[", 0) {
            state.take();
            let size = if state.at("]", 0) { -1 } else { constant_expression(state)? };
            state.expect("]")?;
            if size == 0 || size < -1 || (size == -1 && !found.is_empty()) {
                return state.fail("an array needs a size of at least 1");
            }
            found.push(Ok(size));
        } else if state.at("(", 0) {
            state.take();
            found.push(Err(parameter_types(state)?));
        } else {
            break;
        }
    }
    let mut ty = ty;
    for suffix in found.into_iter().rev() {
        ty = match suffix {
            Ok(size) => Ty::Array(Box::new(ty), size),
            Err((params, variadic)) => Ty::Function(Box::new(ty), params, variadic),
        };
    }
    Ok(ty)
}

/// A parameter's type as the function sees it: an array is a pointer to its first element, a
/// function a pointer to it.
fn adjusted(ty: Ty) -> Ty {
    match ty {
        Ty::Array(of, _) => Ty::Pointer(of),
        function @ Ty::Function(..) => ctype::pointer(function),
        other => other,
    }
}

/// The parameter types of a function type, after its (: names allowed and ignored.
fn parameter_types(state: &mut State) -> Parsed<(Vec<Ty>, bool)> {
    let mut params = Vec::new();
    let mut variadic = false;
    if !no_parameters(state) {
        loop {
            if state.at("...", 0) {
                state.take();
                variadic = true;
                break;
            }
            let spec = specifiers(state, true)?;
            let base = base_ty(state, &spec)?;
            let (_, ty) = declarator(state, base)?;
            params.push(adjusted(ty));
            if !state.at(",", 0) {
                break;
            }
            state.take();
        }
    }
    state.expect(")")?;
    Ok((params, variadic))
}

/// Whether the declarator ahead needs C's full declarators (declarator), not the forms the
/// decompiler writes (x, *x, x[N]...): a struct or typedef'd type, void *, pointers to pointers,
/// parentheses, an array of pointers.
fn complex_ahead(state: &State, spec: &Spec) -> bool {
    if spec.ty.is_some() {
        return true;
    }
    let mut offset = 0;
    let mut stars = 0;
    while state.at("*", offset) || state.at("const", offset) || state.at("volatile", offset) {
        stars += usize::from(state.at("*", offset));
        offset += 1;
    }
    (spec.kind == "void" && stars > 0)
        || stars > 1
        || state.at("(", offset)
        || (stars == 1 && state.token(offset).kind == Kind::Name && state.at("[", offset + 1))
}

/// A type name in a cast, sizeof or offsetof: specifiers and an abstract declarator.
fn abstract_type(state: &mut State) -> Parsed<Ty> {
    let spec = specifiers(state, false)?;
    if spec.storage.is_some() {
        return state.fail("a type name has no storage class");
    }
    let base = base_ty(state, &spec)?;
    let (name, ty) = declarator(state, base)?;
    if !name.is_empty() {
        return state.fail(format!("a type name has no name, found {name}"));
    }
    Ok(ty)
}

/// An integer constant expression (an array size): numbers, this file's integer #defines and enum
/// members, sizeof, and C's operators.
fn constant_expression(state: &mut State) -> Parsed<i64> {
    let node = ternary(state)?;
    let found =
        |name: &str| state.constants.get(name).or_else(|| state.enumerators.get(name)).map(|value| *value as i64);
    match folded(&node, &found) {
        Some((value, _)) => Ok(value),
        None => state.fail("expected an integer constant"),
    }
}

/// The bytes a value of a type (or a typedef of one) takes: the VM's slot holds 32 bits, and char and
/// short are C's 1 and 2.
fn type_size(state: &State, kind: &str) -> i64 {
    let kind = state.base_kind(kind);
    match kind.as_str() {
        "char" | "unsigned char" => 1,
        "short" | "unsigned short" => 2,
        _ => 4,
    }
}

/// The number of elements a local's or global's size gives (1 for a scalar).
fn elements(state: &State, entry: &Node) -> Parsed<i64> {
    match &entry["size"] {
        Node::Null => Ok(1),
        Node::Str(name) => match state.constants.get(name) {
            Some(value) => Ok(*value as i64),
            None => state.fail(format!("sizeof needs the value of {name}, which this file does not define")),
        },
        size => Ok(size.as_i64().unwrap_or(1)),
    }
}

/// The type of a variable's declaration: its own C type, or one its plain fields give.
fn entry_type(state: &State, entry: &Node) -> Ty {
    if let Some(ctype) = entry.get("ctype") {
        return Ty::from_node(ctype);
    }
    let kind = field(entry, "type");
    let scalar = Ty::Scalar(state.kinds.get(kind).cloned().unwrap_or_else(|| kind.to_string()));
    if entry.get("pointer").is_some() || entry["array"] == Node::Bool(true) {
        return ctype::pointer(scalar);
    }
    match elements(state, entry) {
        Ok(count) if !entry["size"].is_null() => Ty::Array(Box::new(scalar), count),
        _ => scalar,
    }
}

/// The type of an expression of C's other types that sizeof is given (None for the forms the
/// compiler had before, whose sizes size_of works out as it did).
fn static_type(state: &State, node: &Node) -> Option<Ty> {
    let typed = |name: &str| state.variable(name).filter(|entry| entry.get("ctype").is_some());
    match node.kind() {
        "var" => typed(field(node, "name")).map(|entry| entry_type(state, &entry)),
        "deref" => static_type(state, &node["operand"]).and_then(|ty| ty.target().cloned()),
        "subscript" => {
            let array = static_type(state, &node["array"])
                .or_else(|| state.variable(field(&node["array"], "name")).map(|entry| entry_type(state, &entry)))?;
            array.target().cloned()
        }
        "member" => {
            let object = static_type(state, &node["object"])
                .or_else(|| state.variable(field(&node["object"], "name")).map(|entry| entry_type(state, &entry)))?;
            let object = if node["arrow"] == Node::Bool(true) { object.target()?.clone() } else { object };
            let Ty::Struct(tag) = object else {
                return None;
            };
            ctype::member(&state.structs, &tag, field(node, "name")).map(|(_, _, ty)| ty)
        }
        "addr" => static_type(state, &node["target"]).map(ctype::pointer),
        "wide" => {
            let count = node["codes"].as_array().map_or(1, Vec::len) as i64;
            Some(Ty::Array(Box::new(Ty::Scalar("unsigned short".into())), count))
        }
        "cast" => node.get("ctype").map(Ty::from_node),
        _ => None,
    }
}

/// What sizeof gives for an expression, which it does not evaluate.
fn size_of(state: &State, node: &Node) -> Parsed<i64> {
    if let Some(ty) = static_type(state, node) {
        return Ok(ty.bytes(&state.structs));
    }
    let entry = |name: &str| state.variable(name);
    Ok(match node.kind() {
        "string" => match stb::encode_string(field(node, "text")) {
            Ok(bytes) => bytes.len() as i64,
            Err(error) => return state.fail(error),
        },
        "var" => match entry(field(node, "name")) {
            Some(found) if found.get("pointer").is_some() || found["array"] == Node::Bool(true) => 4,
            Some(found) => elements(state, &found)? * type_size(state, field(&found, "type")),
            // sizeof(wchar_t): a header's typedef, which reads as a name here
            None if state.header_kinds.contains_key(field(node, "name")) => type_size(state, field(node, "name")),
            None => 4,
        },
        "index" => entry(field(node, "name")).map_or(4, |found| type_size(state, field(&found, "type"))),
        "deref" => entry(field(&node["operand"], "name")).map_or(4, |found| type_size(state, field(&found, "type"))),
        "addr" if node.get("row").is_some() => {
            let row = node["row"].as_i64().unwrap_or(1);
            row * entry(field(&node["target"], "name")).map_or(4, |found| type_size(state, field(&found, "type")))
        }
        "cast" if field(node, "type") == "void" => return state.fail("sizeof a void expression"),
        "cast" if field(node, "type").ends_with('*') => 4,
        "cast" => type_size(state, field(node, "type")),
        "assign" | "compound" => size_of(state, &node["target"])?,
        "comma" => size_of(state, &node["right"])?,
        "binary" if node.get("postfix").is_some() => size_of(state, &node["left"])?,
        _ => 4,
    })
}

/// A sizeof's value: an int constant of size_t, which is unsigned int.
fn size_literal(size: i64) -> Node {
    node!({"kind": "int", "value": size, "unsigned": true})
}

fn unary(state: &mut State) -> Parsed<Node> {
    state.framed(|state| {
        if state.at("-", 0) || state.at("!", 0) {
            let op = state.take();
            return Ok(node!({"kind": "unary", "op": op.text(), "operand": unary(state)?}));
        }
        if state.at("+", 0) {
            state.take();
            return Ok(node!({"kind": "unary", "op": "+", "operand": unary(state)?}));
        }
        if state.at("sizeof", 0) {
            return sizeof(state);
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
            writable(state, &target)?;
            return Ok(stepped(state, target, op));
        }
        if state.at("&", 0) {
            state.take();
            let target = unary(state)?;
            if !is_target(&target) {
                return state.fail("& needs a variable");
            }
            return address(state, target);
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
            return Ok(node!({"kind": "cast", "type": format!("{kind} *"), "operand": operand}));
        }
        if state.at("(", 0) && starts_type(state, 1) {
            return cast(state);
        }
        primary(state)
    })
}

/// sizeof(type) or sizeof expression.
#[inline(never)]
fn sizeof(state: &mut State) -> Parsed<Node> {
    state.take();
    if state.at("(", 0) && starts_type(state, 1) {
        state.take();
        let ty = abstract_type(state)?;
        state.expect(")")?;
        match &ty {
            Ty::Void => return state.fail("sizeof(void)"),
            Ty::Function(..) => return state.fail("sizeof a function"),
            Ty::Struct(_) => complete(state, "sizeof", &ty)?,
            _ => {}
        }
        return Ok(size_literal(ty.bytes(&state.structs)));
    }
    let operand = unary(state)?;
    Ok(size_literal(size_of(state, &operand)?))
}

/// Any cast but the decompiler's: (unsigned char)x, (void)f(), (long)y, (const int)z.
#[inline(never)]
fn cast(state: &mut State) -> Parsed<Node> {
    state.take();
    let ty = abstract_type(state)?;
    state.expect(")")?;
    let operand = unary(state)?;
    match &ty {
        Ty::Array(..) => state.fail("a cast cannot give an array"),
        Ty::Struct(_) => state.fail("a cast cannot give a struct"),
        Ty::Function(..) => state.fail("a cast cannot give a function"),
        // the VM's pointers are values of their own: the cast passes the value as it is, and its
        // type is the compiler's (composite.rs)
        Ty::Pointer(to) => {
            Ok(node!({"kind": "cast", "type": format!("{} *", to.shown()), "operand": operand, "ctype": ty.to_node()}))
        }
        Ty::Void => Ok(node!({"kind": "cast", "type": "void", "operand": operand})),
        Ty::Scalar(kind) => Ok(node!({"kind": "cast", "type": kind.as_str(), "operand": operand})),
    }
}

/// &x; a register variable has no address.
#[inline(never)]
fn address(state: &State, target: Node) -> Parsed<Node> {
    if target.kind() == "var"
        && state.variable(field(&target, "name")).is_some_and(|entry| entry.get("register").is_some())
    {
        return state.fail(format!("{} is a register variable, which has no address", field(&target, "name")));
    }
    Ok(node!({"kind": "addr", "target": target}))
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
        writable(state, &left)?;
        let op = state.take();
        let value = assignment(state)?;
        if let Some(op) = compound_op(op.text()) {
            return Ok(compound(state, left, op, value));
        }
        Ok(node!({"kind": "assign", "target": left, "value": value}))
    })
}

/// An expression with C's comma operator: a, b works out a, then gives b.
fn expression(state: &mut State) -> Parsed<Node> {
    let mut node = assignment(state)?;
    while state.at(",", 0) {
        state.take();
        let right = assignment(state)?;
        node = node!({"kind": "comma", "left": discarded(node), "right": right});
    }
    Ok(node)
}

/// An expression whose value is not used: x++ is only its step.
fn discarded(node: Node) -> Node {
    if node["postfix"] == Node::Bool(true) {
        node["left"].clone()
    } else {
        node
    }
}

fn expression_statement(state: &State, node: Node) -> Node {
    state.leaf(1);
    node!({"kind": "expr", "expr": discarded(node)})
}

/// The statements of an expression statement: one for each operand of a comma at its top.
fn expression_statements(state: &State, node: Node) -> Vec<Node> {
    if node.kind() == "comma" {
        let mut found = expression_statements(state, node["left"].clone());
        found.extend(expression_statements(state, node["right"].clone()));
        return found;
    }
    vec![expression_statement(state, node)]
}

fn braced(state: &mut State) -> Parsed<Vec<Node>> {
    state.framed(|state| {
        state.expect("{")?;
        state.open_block();
        let mut statements = Vec::new();
        while !state.at("}", 0) {
            statements.extend(statement(state)?);
        }
        state.close_block();
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
        let cond = expression(state)?;
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
        // a declaration in the first part belongs to the loop (as C99 has it)
        state.open_block();
        let mut init = if declaration_ahead(state) { declaration(state)? } else { expression_list(state, ";")? };
        let cond = if state.at(";", 0) { one() } else { expression(state)? };
        state.expect(";")?;
        let step = expression_list(state, ")")?;
        let mut loop_body = before_continue(&body(state)?, &step);
        state.close_block();
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
        let cond = expression(state)?;
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
        let value = expression(state)?;
        state.expect(")")?;
        state.expect("{")?;
        let mut cases: Vec<Node> = Vec::new();
        // the case values known here, which C allows once each
        let mut numbers: Vec<i64> = Vec::new();
        while !state.at("}", 0) {
            if state.at("case", 0) {
                state.take();
                let value = ternary(state)?;
                let found = |name: &str| {
                    state.constants.get(name).or_else(|| state.enumerators.get(name)).map(|value| *value as i64)
                };
                if let Some((number, _)) = folded(&value, &found) {
                    if numbers.contains(&number) {
                        return state.fail(format!("case {number} is in the switch twice"));
                    }
                    numbers.push(number);
                }
                cases.push(node!({"value": value, "default": false, "body": []}));
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
        if state.token(0).kind == Kind::Name
            && !STATEMENT_WORDS.contains(&word.as_str())
            && state.token(1).kind == Kind::Op
            && state.token(1).text() == ":"
        {
            // a label: the statement after it follows it in the list
            state.take();
            state.take();
            let Some(scope) = state.scope.as_mut() else {
                return state.fail("labels belong in functions");
            };
            if scope.labels.contains(&word) {
                return state.fail(format!("label {word} is defined twice"));
            }
            scope.labels.push(word.clone());
            let mut found = vec![node!({"kind": "label", "name": &word})];
            if !state.at("}", 0) {
                found.extend(statement(state)?);
            }
            return Ok(found);
        }
        match word.as_str() {
            "if" => return Ok(vec![conditional(state)?]),
            "while" => {
                state.take();
                state.expect("(")?;
                let cond = expression(state)?;
                state.expect(")")?;
                return Ok(vec![node!({"kind": "while", "cond": cond, "body": body(state)?})]);
            }
            "for" => return for_loop(state),
            "do" => return do_loop(state),
            "switch" => return Ok(vec![switch(state)?]),
            "goto" => {
                let line = state.take().line;
                let label = state.name()?;
                state.expect(";")?;
                let Some(scope) = state.scope.as_mut() else {
                    return state.fail("goto belongs in functions");
                };
                scope.gotos.push((label.clone(), line));
                return Ok(vec![node!({"kind": "goto", "name": label})]);
            }
            "return" => {
                state.take();
                if state.at(";", 0) {
                    state.take();
                    return Ok(vec![node!({"kind": "return", "value": null})]);
                }
                let value = expression(state)?;
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
        let node = expression(state)?;
        state.expect(";")?;
        Ok(expression_statements(state, node))
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
/// Any other integer constant expression is worked out here.
fn array_size(state: &mut State) -> Parsed<Size> {
    state.leaf(3);
    let token = state.token(0);
    let alone = state.token(1).kind == Kind::Op && state.token(1).text() == "]";
    if alone && matches!(token.kind, Kind::Int | Kind::Name) {
        let token = state.take();
        return Ok(match token.kind {
            Kind::Int => Size::Number(int_value(token.text())?),
            _ => match state.constants.get(token.text()) {
                Some(value) => Size::Number(*value),
                None => Size::Named(token.text().to_string()),
            },
        });
    }
    if matches!(token.kind, Kind::Int | Kind::Name | Kind::Op) {
        return Ok(Size::Number(constant_expression(state)? as i128));
    }
    state.fail("an array size is a number or a constant")
}

/// An initializer: an expression, or a braced list of initializers.
#[derive(Clone, Debug)]
enum Init {
    Value(Node),
    List(Vec<Init>),
}

fn initializer(state: &mut State) -> Parsed<Init> {
    state.framed(|state| {
        if !state.at("{", 0) {
            return Ok(Init::Value(assignment(state)?));
        }
        state.expect("{")?;
        let mut values = Vec::new();
        while !state.at("}", 0) {
            values.push(initializer(state)?);
            if !state.at("}", 0) {
                state.expect(",")?;
            }
        }
        state.expect("}")?;
        Ok(Init::List(values))
    })
}

struct Declared {
    name: String,
    /// The array's sizes, the first Number(-1) when the initializer gives it.
    dims: Vec<Size>,
    init: Option<Init>,
    line: usize,
    pointer: bool,
    /// int *const p: the pointer itself is const.
    pointer_const: bool,
    /// The type, for a declarator of C's other types (complex_ahead); the fields above are the
    /// plain forms'.
    ty: Option<Ty>,
}

impl Declared {
    fn size(&self) -> &Size {
        self.dims.first().unwrap_or(&Size::None)
    }
}

/// The *, and const after it, before a declarator's name.
fn pointer_part(state: &mut State) -> Parsed<(bool, bool)> {
    let pointer = state.at("*", 0);
    let mut constant = false;
    if pointer {
        state.take();
        while state.at("const", 0) || state.at("volatile", 0) {
            constant |= state.take().text() == "const";
        }
        if state.at("*", 0) {
            return state.fail("pointers to pointers are not supported yet: the VM has one level of indirection");
        }
    }
    Ok((pointer, constant))
}

fn declarators(state: &mut State, spec: &Spec) -> Parsed<Vec<Declared>> {
    state.framed(|state| {
        let mut found = Vec::new();
        loop {
            let line = state.line();
            if complex_ahead(state, spec) {
                let base = base_ty(state, spec)?;
                let (name, ty) = declarator(state, base)?;
                if name.is_empty() {
                    let found = state.shown(state.peek(0))?;
                    return state.fail(format!("expected a name, found {found}"));
                }
                let mut init = None;
                if state.at("=", 0) {
                    state.take();
                    init = Some(initializer(state)?);
                }
                let (pointer, pointer_const, dims) = (false, false, Vec::new());
                found.push(Declared { name, dims, init, line, pointer, pointer_const, ty: Some(ty) });
                if !state.at(",", 0) {
                    break;
                }
                state.take();
                continue;
            }
            let (pointer, pointer_const) = pointer_part(state)?;
            let name = state.name()?;
            let mut dims = Vec::new();
            while state.at("[", 0) {
                state.take();
                let open = dims.is_empty() && state.at("]", 0);
                dims.push(if open { Size::Number(-1) } else { array_size(state)? });
                state.expect("]")?;
            }
            let mut init = None;
            if state.at("=", 0) {
                state.take();
                init = Some(initializer(state)?);
            }
            if dims.first() == Some(&Size::Number(-1)) && init.is_none() {
                return state.fail(format!("{name}[] needs a size or a list of values"));
            }
            if matches!(init, Some(Init::List(_))) && dims.is_empty() && pointer {
                return state.fail(format!("too many values for {name}"));
            }
            found.push(Declared { name, dims, init, line, pointer, pointer_const, ty: None });
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
        let defined = |offset: usize| state.token(offset).kind == Kind::Op && state.token(offset).text() == "{";
        return defined(1) || (state.peek(1).kind == Kind::Name && (state.peek(2).kind == Kind::Name || defined(2)));
    }
    let following = state.peek(1);
    if TYPES.contains(&token.text()) {
        // int (*f)(int): a declarator in parentheses
        return following.kind == Kind::Name || (following.kind == Kind::Op && ["*", "("].contains(&following.text()));
    }
    if starts_type(state, 0) {
        return true;
    }
    if STATEMENT_WORDS.contains(&token.text()) {
        return false;
    }
    // a value kind or typedef from a header (bgm_file track;), which sdk.resolve reads as its base type
    following.kind == Kind::Name
        && state.peek(2).text.as_deref().is_some_and(|text| [";", "=", "[", ","].contains(&text))
}

/// The zero an element of a type starts at when its array's initializer leaves it out.
fn zero(kind: &str) -> Node {
    if kind == "float" {
        node!({"kind": "float", "bits": 0})
    } else {
        node!({"kind": "int", "value": 0})
    }
}

/// A string's bytes as the elements of a char array (with its 0).
fn string_elements(state: &State, text: &str, kind: &str) -> Parsed<Vec<Node>> {
    let bytes = stb::encode_string(text).or_else(|error| state.fail(error))?;
    Ok(bytes
        .into_iter()
        .map(|byte| node!({"kind": "int", "value": if kind == "char" { byte as i8 as i64 } else { byte as i64 }}))
        .collect())
}

fn is_string(init: &Init) -> Option<&str> {
    match init {
        Init::Value(node) if node.kind() == "string" => node["text"].as_str(),
        _ => None,
    }
}

/// Puts an initializer list's values into an array of the sizes given, from base, reading the
/// list from at with C's brace elision (a list for each row, or the values in a row). Gives how
/// many of the first size's rows it reached; a first size of -1 takes as many as the list has.
#[allow(clippy::too_many_arguments)]
fn fill(
    state: &State,
    items: &[Init],
    at: &mut usize,
    dims: &[i64],
    base: i64,
    kind: &str,
    out: &mut std::collections::BTreeMap<i64, Node>,
) -> Parsed<i64> {
    let stride: i64 = dims[1..].iter().product();
    let chars = matches!(kind, "char" | "unsigned char");
    let mut row = 0;
    while (dims[0] < 0 || row < dims[0]) && *at < items.len() {
        let item = &items[*at];
        if dims.len() == 1 {
            let value = match item {
                Init::Value(node) => node.clone(),
                Init::List(inner) => match inner.as_slice() {
                    [Init::Value(node)] => node.clone(),
                    _ => return state.fail("a scalar takes one value"),
                },
            };
            out.insert(base + row, value);
            *at += 1;
        } else if let (true, 2, Some(text)) = (chars, dims.len(), is_string(item)) {
            put_string(state, text, dims[1], base + row * stride, kind, out)?;
            *at += 1;
        } else if let Init::List(inner) = item {
            let mut inner_at = 0;
            fill(state, inner, &mut inner_at, &dims[1..], base + row * stride, kind, out)?;
            if inner_at < inner.len() {
                return state.fail("too many values in a row");
            }
            *at += 1;
        } else {
            fill(state, items, at, &dims[1..], base + row * stride, kind, out)?;
        }
        row += 1;
    }
    Ok(row)
}

/// A string into a char array of length places (its 0 left out when the array is exactly its
/// length, as C allows); gives the elements it took.
fn put_string(
    state: &State,
    text: &str,
    length: i64,
    base: i64,
    kind: &str,
    out: &mut std::collections::BTreeMap<i64, Node>,
) -> Parsed<i64> {
    let mut elements = string_elements(state, text, kind)?;
    if length >= 0 && elements.len() as i64 == length + 1 {
        elements.pop();
    }
    if length >= 0 && elements.len() as i64 > length {
        return state.fail("the string is longer than the array");
    }
    let count = elements.len() as i64;
    for (offset, value) in elements.into_iter().enumerate() {
        out.insert(base + offset as i64, value);
    }
    Ok(count)
}

/// The values an array's initializer gives its elements, by offset.
type Elements = Vec<(i64, Node)>;

/// An array's sizes as numbers, and the assignments its initializer makes: every element, the
/// ones the list leaves out set to 0 as C sets them.
fn array_values(state: &State, declared: &Declared, kind: &str) -> Parsed<(Vec<i64>, Elements)> {
    let mut dims = Vec::new();
    for (at, size) in declared.dims.iter().enumerate() {
        match size {
            Size::Number(value) => dims.push(*value as i64),
            Size::Named(name) if state.enumerators.contains_key(name) => dims.push(state.enumerators[name] as i64),
            Size::Named(_) if declared.dims.len() == 1 && declared.init.is_none() => {
                // resolved with the headers' constants later
                return Ok((Vec::new(), Vec::new()));
            }
            Size::Named(name) => {
                return state.fail(format!(
                    "{}: size {} of {} must be a constant this file defines",
                    declared.name,
                    at + 1,
                    name
                ))
            }
            Size::None => {}
        }
    }
    let Some(init) = &declared.init else {
        if dims.iter().any(|size| *size <= 0) {
            return state.fail(format!("{} needs a size of at least 1", declared.name));
        }
        return Ok((dims, Vec::new()));
    };
    let mut out = std::collections::BTreeMap::new();
    let chars = matches!(kind, "char" | "unsigned char");
    let rows = match (init, is_string(init)) {
        (_, Some(text)) if chars && dims.len() == 1 => put_string(state, text, dims[0], 0, kind, &mut out)?,
        (Init::List(items), _) => {
            let mut at = 0;
            let rows = fill(state, items, &mut at, &dims, 0, kind, &mut out)?;
            if at < items.len() {
                return state.fail(format!("too many values for {}", declared.name));
            }
            rows
        }
        (Init::Value(_), _) => return state.fail(format!("{} is an array; give it a list of values", declared.name)),
    };
    if dims[0] < 0 {
        dims[0] = rows;
    }
    if dims.iter().any(|size| *size <= 0) {
        return state.fail(format!("{} needs a size of at least 1", declared.name));
    }
    let total: i64 = dims.iter().product();
    let values = (0..total).map(|offset| (offset, out.remove(&offset).unwrap_or_else(|| zero(kind)))).collect();
    Ok((dims, values))
}

/// A local declaration anywhere in a function. Every local gets a slot in the function's frame, so a
/// name declared again in another block reuses the slot when the type matches. Initializers become
/// assignments where the declaration stands. A static local is a hidden global, set once before
/// the script runs (see lower.rs).
fn declaration(state: &mut State) -> Parsed<Vec<Node>> {
    state.framed(|state| {
        if state.scope.is_none() {
            return state.fail("declarations belong inside functions");
        }
        let spec = specifiers(state, true)?;
        if spec.storage.as_deref() == Some("typedef") {
            return local_typedef(state, &spec);
        }
        // a typedef of this function's stands for its type
        let kind = state.kinds.get(&spec.kind).filter(|_| local_kind(state, &spec.kind)).cloned().unwrap_or(spec.kind.clone());
        if kind == "any" || (kind == "void" && !complex_ahead(state, &spec)) {
            return state.fail(format!("unknown type {kind}"));
        }
        let mut statements = Vec::new();
        for declared in declarators(state, &spec)? {
            if spec.storage.as_deref() == Some("extern") {
                // names a global this file declares
                continue;
            }
            let static_aggregate = spec.storage.as_deref() == Some("static") && (declared.pointer || !declared.dims.is_empty());
            let wide = matches!(&declared.init, Some(Init::Value(value)) if value.kind() == "wide");
            if declared.ty.is_some() || static_aggregate || wide {
                statements.extend(typed_local(state, &spec, declared, &kind)?);
                continue;
            }
            let line = declared.line;
            let (dims, values) = if declared.dims.is_empty() { (Vec::new(), Vec::new()) } else { array_values(state, &declared, &kind)? };
            let size = match dims.as_slice() {
                [] => declared.size().node()?,
                dims => int_node(dims.iter().product::<i64>() as i128)?,
            };
            let is_static = spec.storage.as_deref() == Some("static");
            let storage = storage_for(state, &declared.name, &kind, &size, &Node::Null, is_static)?;
            if spec.storage.as_deref() == Some("static") {
                static_local(state, &declared, &storage, &kind, &spec)?;
                continue;
            }
            let mut entry = node!({"name": &storage, "type": &kind, "size": size});
            if declared.pointer {
                entry["pointer"] = Node::Bool(true);
            }
            if dims.len() > 1 {
                entry["dims"] = Node::List(dims.iter().map(|size| Node::from(*size)).collect());
            }
            if (spec.constant && !declared.pointer) || declared.pointer_const {
                entry["const"] = Node::Bool(true);
            }
            if spec.constant && declared.pointer {
                entry["pointee_const"] = Node::Bool(true);
            }
            if spec.storage.as_deref() == Some("register") {
                entry["register"] = Node::Bool(true);
            }
            let scope = state.scope.as_mut().unwrap();
            if !scope.locals.contains(&entry) {
                scope.locals.push(entry);
            }
            let assign = |target: Node, value: Node| {
                node!({"kind": "expr", "expr": {"kind": "assign", "target": target, "value": value}, "line": line})
            };
            match declared.init {
                Some(Init::Value(value)) if declared.dims.is_empty() => {
                    statements.push(assign(node!({"kind": "var", "name": &storage}), value));
                }
                Some(Init::List(items)) if declared.dims.is_empty() => match items.as_slice() {
                    // int x = {5};
                    [Init::Value(value)] => statements.push(assign(node!({"kind": "var", "name": &storage}), value.clone())),
                    _ => return state.fail(format!("too many values for {}", declared.name)),
                },
                Some(_) => {
                    for (offset, value) in values {
                        let cell = node!({"kind": "index", "name": &storage, "index": {"kind": "int", "value": offset}});
                        statements.push(assign(cell, value));
                    }
                }
                None => {}
            }
        }
        Ok(statements)
    })
}

/// A declaration's type when its declarator has the plain fields only (a static or global array
/// or pointer, an array a wide string fills).
fn plain_type(state: &State, declared: &Declared, kind: &str) -> Parsed<Ty> {
    let kind = state.base_kind(kind);
    let scalar = if kind.starts_with("enum ") { ctype::int() } else { Ty::Scalar(kind) };
    if declared.pointer {
        return Ok(ctype::pointer(scalar));
    }
    let mut ty = scalar;
    for size in declared.dims.iter().rev() {
        let size = match size {
            Size::Number(size) => *size as i64,
            Size::Named(name) => match state.constants.get(name).or_else(|| state.enumerators.get(name)) {
                Some(size) => *size as i64,
                None => {
                    return state.fail(format!("{}: size {name} must be a constant this file defines", declared.name))
                }
            },
            Size::None => -1,
        };
        ty = Ty::Array(Box::new(ty), size);
    }
    Ok(ty)
}

/// An array's open size ([]) given by its initializer: as many elements as the list fills, or the
/// string's characters and its 0.
fn sized_by(state: &State, ty: Ty, init: &Option<Init>) -> Parsed<Ty> {
    let Ty::Array(of, -1) = &ty else {
        return Ok(ty);
    };
    let count = match init {
        Some(Init::List(items)) => {
            let mut values = std::collections::BTreeMap::new();
            fill_typed(state, &ty, items, &mut 0, 0, &mut values)?
        }
        Some(Init::Value(node)) if node.kind() == "wide" => node["codes"].as_array().map_or(1, Vec::len) as i64,
        Some(Init::Value(node)) if node.kind() == "string" => {
            stb::encode_string(field(node, "text")).or_else(|error| state.fail(error))?.len() as i64 + 1
        }
        _ => return Ok(ty),
    };
    Ok(Ty::Array(of.clone(), count))
}

/// Puts an initializer list's values into a value of a type, from slot base, reading items from at
/// with C's brace elision; gives how many elements of an array it filled (-1 size: all it can).
fn fill_typed(
    state: &State,
    ty: &Ty,
    items: &[Init],
    at: &mut usize,
    base: i64,
    out: &mut std::collections::BTreeMap<i64, Node>,
) -> Parsed<i64> {
    let structs = &state.structs;
    let one = |item: &Init| -> Parsed<Node> {
        match item {
            Init::Value(node) => Ok(node.clone()),
            Init::List(inner) => match inner.as_slice() {
                [Init::Value(node)] => Ok(node.clone()),
                _ => state.fail("a scalar takes one value"),
            },
        }
    };
    // what one element (or member) of ty is, and where it starts
    let parts: Vec<(Ty, i64)> = match ty {
        Ty::Array(of, size) => {
            let step = of.slots(structs);
            // an open size takes at most an element per value
            let count = if *size < 0 { items.len() as i64 } else { *size };
            (0..count).map(|index| ((**of).clone(), base + index * step)).collect()
        }
        Ty::Struct(tag) => {
            let Some(found) = structs.get(tag) else {
                return state.fail(format!("{} is not defined", ty.shown()));
            };
            let mut offset = base;
            let mut parts = Vec::new();
            for (_, member) in &found.members {
                parts.push((member.clone(), offset));
                if found.union {
                    // a union's initializer sets its first member
                    break;
                }
                offset += member.slots(structs);
            }
            parts
        }
        _ => {
            let Some(item) = items.get(*at) else {
                return Ok(0);
            };
            out.insert(base, one(item)?);
            *at += 1;
            return Ok(1);
        }
    };
    let characters =
        |of: &Ty| matches!(of, Ty::Scalar(kind) if kind.contains("char") || kind.contains("short") || kind == "int");
    let mut filled = 0;
    for (part, offset) in parts {
        if *at >= items.len() {
            break;
        }
        match (&part, &items[*at]) {
            // a string or wide string for an array of characters
            (Ty::Array(of, length), Init::Value(node))
                if matches!(node.kind(), "string" | "wide") && characters(of) =>
            {
                let codes: Vec<Node> = if node.kind() == "wide" {
                    let codes = node["codes"].as_array().map_or(&[][..], Vec::as_slice);
                    codes.iter().map(|code| node!({"kind": "int", "value": code.clone()})).collect()
                } else {
                    string_elements(state, field(node, "text"), &of.slot_kind())?
                };
                let fits = if *length >= 0 && codes.len() as i64 == length + 1 { codes.len() - 1 } else { codes.len() };
                if *length >= 0 && fits as i64 > *length {
                    return state.fail("the string is longer than the array");
                }
                for (index, code) in codes.into_iter().take(fits).enumerate() {
                    out.insert(offset + index as i64, code);
                }
                *at += 1;
            }
            (part, Init::List(inner)) if part.is_aggregate() => {
                let mut inner_at = 0;
                fill_typed(state, part, inner, &mut inner_at, offset, out)?;
                if inner_at < inner.len() {
                    return state.fail("too many values in a brace");
                }
                *at += 1;
            }
            (part, _) if part.is_aggregate() => {
                fill_typed(state, part, items, at, offset, out)?;
            }
            (_, item) => {
                out.insert(offset, one(item)?);
                *at += 1;
            }
        }
        filled += 1;
    }
    Ok(filled)
}

/// The values an initializer gives an object of a type, by slot: every scalar slot, those it
/// leaves out 0, as C sets them (only the given ones for a global, whose slots start at 0).
fn initial_values(state: &State, name: &str, ty: &Ty, init: &Init, every: bool) -> Parsed<Vec<(i64, Ty, Node)>> {
    let mut values = std::collections::BTreeMap::new();
    match init {
        Init::List(items) => {
            let mut at = 0;
            fill_typed(state, ty, items, &mut at, 0, &mut values)?;
            if at < items.len() {
                return state.fail(format!("too many values for {name}"));
            }
        }
        Init::Value(node) if matches!(node.kind(), "string" | "wide") && matches!(ty, Ty::Array(..)) => {
            // char s[] = "...": the array is the string's one place
            let whole = Ty::Array(Box::new(ty.clone()), 1);
            fill_typed(state, &whole, std::slice::from_ref(init), &mut 0, 0, &mut values)?;
        }
        Init::Value(_) => return state.fail(format!("{name} takes a list of values")),
    }
    let mut slots = Vec::new();
    ctype::scalar_slots(ty, &state.structs, 0, &mut slots);
    let mut found = Vec::new();
    for (slot, scalar) in slots {
        match values.remove(&slot) {
            Some(value) => found.push((slot, scalar, value)),
            None if every => {
                let zero = zero(&scalar.slot_kind());
                found.push((slot, scalar, zero));
            }
            None => {}
        }
    }
    Ok(found)
}

/// A slot of an aggregate's storage at a constant offset (composite.rs reads it), with its type.
fn slot_field(storage: &str, slot: i64, ty: &Ty) -> Node {
    node!({"kind": "field", "name": storage, "offset": slot, "type": ty.slot_kind(), "ctype": ty.to_node()})
}

/// The fields a local's or global's declaration has for a type of C's: its storage (slots, and
/// whether the slot holds a pointer the VM indexes through) for compile.rs, and the type itself.
fn typed_entry(state: &State, name: &str, ty: &Ty) -> Node {
    let ctype = ty.to_node();
    match ty {
        Ty::Array(..) | Ty::Struct(_) => {
            node!({"name": name, "type": "int", "size": ty.slots(&state.structs), "ctype": ctype})
        }
        Ty::Pointer(to) if ty.is_pointer() => {
            let kind = if matches!(**to, Ty::Scalar(_)) { to.slot_kind() } else { "int".into() };
            node!({"name": name, "type": kind, "size": null, "pointer": true, "ctype": ctype})
        }
        _ => node!({"name": name, "type": ty.slot_kind(), "size": null, "ctype": ctype}),
    }
}

/// A local of C's other types (a struct, a pointer to a pointer, a function pointer...) or a
/// static array or pointer: its slots, and assignments for its initializer. A static one is a
/// hidden global, as for static scalars.
fn typed_local(state: &mut State, spec: &Spec, declared: Declared, kind: &str) -> Parsed<Vec<Node>> {
    let ty = match &declared.ty {
        Some(ty) => ty.clone(),
        None => plain_type(state, &declared, kind)?,
    };
    let ty = sized_by(state, ty, &declared.init)?;
    complete(state, &declared.name, &ty)?;
    let is_static = spec.storage.as_deref() == Some("static");
    let entry = typed_entry(state, "", &ty);
    let storage =
        storage_for(state, &declared.name, field(&entry, "type"), &entry["size"], &entry["ctype"], is_static)?;
    let entry = entry.with("name", storage.as_str());
    if is_static {
        let mut global = entry.clone();
        if let Some(map) = global.as_object_mut() {
            map.remove("pointer");
        }
        let global = global_values(state, global, &declared, &ty)?;
        state.statics.push(global.clone());
        state.scope.as_mut().unwrap().statics.insert(storage, global);
        return Ok(Vec::new());
    }
    let scope = state.scope.as_mut().unwrap();
    if !scope.locals.contains(&entry) {
        scope.locals.push(entry);
    }
    let line = declared.line;
    let assign = |target: Node, value: Node| node!({"kind": "expr", "expr": {"kind": "assign", "target": target, "value": value}, "line": line});
    let Some(init) = &declared.init else {
        return Ok(Vec::new());
    };
    let variable = node!({"kind": "var", "name": &storage});
    match (&ty, init) {
        (Ty::Array(..), _) | (Ty::Struct(_), Init::List(_)) => {
            let values = initial_values(state, &declared.name, &ty, init, true)?;
            Ok(values.into_iter().map(|(slot, ty, value)| assign(slot_field(&storage, slot, &ty), value)).collect())
        }
        // a scalar or pointer, or a struct from another (copied, composite.rs)
        (_, Init::Value(value)) => Ok(vec![assign(variable, value.clone())]),
        (_, Init::List(items)) => match items.as_slice() {
            [Init::Value(value)] => Ok(vec![assign(variable, value.clone())]),
            _ => state.fail(format!("too many values for {}", declared.name)),
        },
    }
}

/// A global of C's other types: an array (its slots one after another), a pointer, a struct, a
/// function pointer. Its initializer is set once per load, as a scalar's.
fn typed_global(state: &mut State, program: &mut Node, spec: &Spec, declared: Declared) -> Parsed<()> {
    let ty = match &declared.ty {
        Some(ty) => ty.clone(),
        None => plain_type(state, &declared, &spec.kind)?,
    };
    if let Ty::Function(..) = ty {
        // a function's prototype written as a declarator
        return Ok(());
    }
    let ty = sized_by(state, ty, &declared.init)?;
    complete(state, &declared.name, &ty)?;
    let mut entry = typed_entry(state, &declared.name, &ty);
    if let Some(map) = entry.as_object_mut() {
        map.remove("pointer");
    }
    let mut entry = global_values(state, entry, &declared, &ty)?;
    if spec.constant {
        entry["const"] = Node::Bool(true);
    }
    let globals = program["globals"].as_array_mut().unwrap();
    match globals.iter_mut().find(|global| global["name"] == *declared.name) {
        Some(existing) if existing["ctype"] == entry["ctype"] => {
            let given = |entry: &Node| entry.get("init").is_some() || entry.get("inits").is_some();
            if given(&entry) {
                if given(existing) {
                    return state.fail(format!("{} is given a value twice", declared.name));
                }
                *existing = entry.clone();
            }
        }
        Some(_) => return state.fail(format!("{} is declared again with another type", declared.name)),
        None => globals.push(entry.clone()),
    }
    state.globals.insert(declared.name.clone(), entry);
    Ok(())
}

/// A global's (or static local's hidden global's) initializer, as its "init" (a scalar's value) or
/// "inits" (each slot an aggregate's list gives), set once per load (lower.rs).
fn global_values(state: &State, mut entry: Node, declared: &Declared, ty: &Ty) -> Parsed<Node> {
    let Some(init) = &declared.init else {
        return Ok(entry);
    };
    if ty.is_aggregate() {
        let values = initial_values(state, &declared.name, ty, init, false)?;
        let inits: Vec<Node> = values
            .into_iter()
            .map(|(slot, ty, value)| node!({"offset": slot, "type": ty.slot_kind(), "ctype": ty.to_node(), "value": value}))
            .collect();
        entry["inits"] = Node::List(inits);
    } else {
        entry["init"] = match init {
            Init::Value(value) => value.clone(),
            Init::List(items) => match items.as_slice() {
                [Init::Value(value)] => value.clone(),
                _ => return state.fail(format!("too many values for {}", declared.name)),
            },
        };
    }
    if state.strict {
        entry["std"] = Node::from("c89");
    }
    Ok(entry)
}

/// Whether a typedef name is one a function declared, which a declaration reads as its base type
/// (a file's typedefs reach sdk.resolve through the program's kinds).
fn local_kind(state: &State, name: &str) -> bool {
    state.scope.as_ref().is_some_and(|scope| scope.typedefs.iter().any(|known| known == name))
}

/// typedef of a type beyond the scalars (a struct, a pointer, an array, a function): the name
/// stands for the type from here on. false, reading nothing, for a scalar's typedef.
fn typed_typedef(state: &mut State, spec: &Spec) -> Parsed<bool> {
    let plain = spec.ty.is_none() && state.peek(0).kind == Kind::Name && state.at(";", 1);
    if plain {
        return Ok(false);
    }
    let base = base_ty(state, spec)?;
    let (name, ty) = declarator(state, base)?;
    if name.is_empty() {
        return state.fail("a typedef needs a name");
    }
    state.expect(";")?;
    state.types.insert(name, ty);
    Ok(true)
}

/// typedef inside a function: the name stands for the type in the declarations after it.
fn local_typedef(state: &mut State, spec: &Spec) -> Parsed<Vec<Node>> {
    if typed_typedef(state, spec)? {
        return Ok(Vec::new());
    }
    let base = base_type(state, &spec.kind)?;
    let name = state.name()?;
    state.expect(";")?;
    state.kinds.insert(name.clone(), base);
    state.scope.as_mut().unwrap().typedefs.push(name);
    Ok(Vec::new())
}

/// The type a typedef stands for: int, float, string, an integer type, or another typedef's.
fn base_type(state: &State, kind: &str) -> Parsed<String> {
    let base = state.kinds.get(kind).cloned().unwrap_or_else(|| kind.to_string());
    let base = if base.starts_with("enum ") { "int".to_string() } else { base };
    if TYPES.contains(&base.as_str()) || INTEGER_TYPES.contains(&base.as_str()) || base == "any" {
        return Ok(base);
    }
    state.fail(format!("cannot typedef {kind}"))
}

/// The name a local declared in the innermost block is held under: its own, or, where that would
/// share a slot with a variable it must not (one it hides, one of another type, a static), its name
/// numbered. A static local's is the hidden global's. Refuses a name declared twice in a block.
fn storage_for(
    state: &mut State,
    name: &str,
    kind: &str,
    size: &Node,
    ctype: &Node,
    is_static: bool,
) -> Parsed<String> {
    let scope = state.scope.as_ref().unwrap();
    let innermost = scope.blocks.len() - 1;
    if scope.blocks[innermost].contains_key(name) {
        return state.fail(format!("{name} is declared twice in one block"));
    }
    if innermost == 1 && scope.blocks[0].contains_key(name) {
        return state.fail(format!("{name} is already a parameter"));
    }
    let visible = scope.blocks.iter().any(|block| block.values().any(|storage| storage == name));
    let taken = |candidate: &str| {
        scope.params.iter().chain(&scope.locals).any(|entry| entry["name"] == *candidate)
            || scope.statics.contains_key(candidate)
            || scope.blocks.iter().any(|block| block.values().any(|storage| storage == candidate))
    };
    let reusable = !is_static
        && !visible
        && !scope.params.iter().any(|param| param["name"] == *name)
        && scope.locals.iter().any(|local| {
            local["name"] == *name && local["type"] == *kind && local["size"] == *size && local["ctype"] == *ctype
        });
    let storage = if is_static {
        let mut hidden = format!("__static_{}_{}", scope.function, name);
        let mut count = 2;
        while scope.statics.contains_key(&hidden) {
            hidden = format!("__static_{}_{}_{}", scope.function, name, count);
            count += 1;
        }
        hidden
    } else if reusable || !taken(name) {
        name.to_string()
    } else {
        let mut count = 2;
        while taken(&format!("{name}__{count}")) {
            count += 1;
        }
        format!("{name}__{count}")
    };
    state.scope.as_mut().unwrap().blocks[innermost].insert(name.to_string(), storage.clone());
    Ok(storage)
}

/// A static local: a hidden global that keeps its value between calls, which code in the function
/// reaches by the local's name.
fn static_local(state: &mut State, declared: &Declared, hidden: &str, kind: &str, spec: &Spec) -> Parsed<()> {
    if !declared.dims.is_empty() {
        return state
            .fail(format!("static arrays are not supported: the script VM cannot index a global ({})", declared.name));
    }
    if declared.pointer {
        return state.fail(format!(
            "static pointers are not supported: the script VM cannot index through a global ({})",
            declared.name
        ));
    }
    let mut entry = node!({"name": hidden, "type": kind});
    match &declared.init {
        Some(Init::Value(value)) => entry["init"] = value.clone(),
        Some(Init::List(items)) => match items.as_slice() {
            [Init::Value(value)] => entry["init"] = value.clone(),
            _ => return state.fail(format!("too many values for {}", declared.name)),
        },
        None => {}
    }
    if spec.constant {
        entry["const"] = Node::Bool(true);
    }
    if state.strict && entry.get("init").is_some() {
        entry["std"] = Node::from("c89");
    }
    state.statics.push(entry.clone());
    state.scope.as_mut().unwrap().statics.insert(hidden.to_string(), entry);
    Ok(())
}

fn no_parameters(state: &mut State) -> bool {
    if state.at("void", 0) && state.at(")", 1) {
        state.take();
        return true;
    }
    state.at(")", 0)
}

/// A function's parameters, and whether they are a K&R identifier list (int f(a, b) int a; ...),
/// whose types the declarations after the ) give. A prototype's parameters may go unnamed.
fn parameters(state: &mut State) -> Parsed<(Vec<Node>, bool, bool)> {
    state.expect("(")?;
    let mut params = Vec::new();
    let mut variadic = false;
    let identifiers = state.token(0).kind == Kind::Name
        && !starts_type(state, 0)
        && state.token(1).kind == Kind::Op
        && [",", ")"].contains(&state.token(1).text());
    if identifiers {
        loop {
            params.push(node!({"name": state.name()?, "type": "int", "array": false}));
            if !state.at(",", 0) {
                break;
            }
            state.take();
        }
        state.expect(")")?;
        return Ok((params, true, false));
    }
    if !no_parameters(state) {
        loop {
            if state.at("...", 0) && !params.is_empty() {
                // the arguments after the named ones, which <stdarg.h> reads
                state.take();
                variadic = true;
                break;
            }
            let typed = state.at("void", 0) && state.at("*", 1) && state.at("*", 2);
            let void_pointer = !typed && state.at("void", 0) && state.at("*", 1);
            let spec = if typed {
                specifiers(state, true)?
            } else if void_pointer {
                // void *p: a pointer parameter the function never reads, so of no element type
                state.take();
                Spec { kind: "void".to_string(), ..Spec::default() }
            } else {
                specifiers(state, true)?
            };
            if spec.storage.as_ref().is_some_and(|storage| storage != "register") {
                return state.fail("a parameter's only storage class is register");
            }
            let pointer_ahead = state.at("*", 0);
            if spec.kind == "any" || (spec.kind == "void" && !pointer_ahead) {
                return state.fail(format!("unknown type {}", spec.kind));
            }
            if typed || (!void_pointer && complex_ahead(state, &spec)) {
                params.push(typed_parameter_entry(state, &spec)?);
                if !state.at(",", 0) {
                    break;
                }
                state.take();
                continue;
            }
            let (pointer, pointer_const) = if spec.kind == "void" && pointer_ahead {
                state.take();
                (true, false)
            } else {
                pointer_part(state)?
            };
            let name = if state.peek(0).kind == Kind::Name { state.name()? } else { String::new() };
            let array = state.at("[", 0);
            let mut inner = Vec::new();
            if array {
                state.take();
                if !state.at("]", 0) {
                    array_size(state)?;
                }
                state.expect("]")?;
                while state.at("[", 0) {
                    state.take();
                    match array_size(state)? {
                        Size::Number(size) => inner.push(size as i64),
                        _ => return state.fail("a parameter's inner array sizes must be constants this file defines"),
                    }
                    state.expect("]")?;
                }
            }
            let kind = if local_kind(state, &spec.kind) { state.kinds[&spec.kind].clone() } else { spec.kind.clone() };
            let mut param = node!({"name": name, "type": kind, "array": array || pointer});
            if !inner.is_empty() {
                param["dims"] = Node::List(inner.into_iter().map(Node::from).collect());
            }
            if (spec.constant && !pointer && !array) || pointer_const {
                param["const"] = Node::Bool(true);
            }
            if spec.constant && (pointer || array) {
                param["pointee_const"] = Node::Bool(true);
            }
            if spec.storage.is_some() {
                param["register"] = Node::Bool(true);
            }
            params.push(param);
            if !state.at(",", 0) {
                break;
            }
            state.take();
        }
    }
    state.expect(")")?;
    Ok((params, false, variadic))
}

/// A parameter of C's other types: a pointer to a pointer or a struct, a struct (passed by value,
/// composite.rs), a function pointer; an array or function declared as one is its pointer.
fn typed_parameter_entry(state: &mut State, spec: &Spec) -> Parsed<Node> {
    let base = base_ty(state, spec)?;
    let (name, ty) = declarator(state, base)?;
    let ty = adjusted(ty);
    if let Ty::Struct(_) = &ty {
        complete(state, &name, &ty)?;
    }
    let entry = typed_entry(state, &name, &ty);
    let pointer = entry.get("pointer").is_some();
    Ok(node!({"name": name, "type": entry["type"].clone(), "array": pointer, "ctype": entry["ctype"].clone()}))
}

/// The declarations of a K&R definition's parameters, between its ) and its {.
fn identifier_types(state: &mut State, params: &mut [Node]) -> Parsed<()> {
    while !state.at("{", 0) {
        let spec = specifiers(state, true)?;
        if spec.storage.as_ref().is_some_and(|storage| storage != "register") {
            return state.fail("a parameter's only storage class is register");
        }
        for declared in declarators(state, &spec)? {
            if declared.ty.is_some() {
                return state.fail(format!("{}: an old-style parameter list takes plain types", declared.name));
            }
            let Some(param) = params.iter_mut().find(|param| param["name"] == *declared.name) else {
                return state.fail(format!("{} is not a parameter", declared.name));
            };
            if declared.init.is_some() {
                return state.fail(format!("parameter {} cannot have a value", declared.name));
            }
            let array = declared.pointer || !declared.dims.is_empty();
            *param = node!({"name": &declared.name, "type": &spec.kind, "array": array});
            if spec.constant && !array {
                param["const"] = Node::Bool(true);
            }
        }
    }
    Ok(())
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
    returns_ty: Option<Ty>,
) -> Parsed<Option<Node>> {
    if name.starts_with('_') {
        state.pos -= 1;
        return state.fail("function names cannot start with _, which marks natives");
    }
    let (mut params, identifiers, variadic) = parameters(state)?;
    if state.at(";", 0) {
        // a prototype: calls are checked against the definition
        state.take();
        return Ok(None);
    }
    if identifiers {
        identifier_types(state, &mut params)?;
    }
    if params.iter().any(|param| param["name"] == "") {
        return state.fail(format!("{name}: every parameter of a definition needs a name"));
    }
    state.scope = Some(Scope::new(name, params.clone()));
    let body = braced(state)?;
    let scope = state.scope.take().unwrap();
    if let Some((label, line)) = scope.gotos.iter().find(|(label, _)| !scope.labels.contains(label)) {
        return lex_fail(*line, format!("goto {label}: no such label in {name}"));
    }
    let locals = scope.locals;
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
    if variadic {
        function["variadic"] = Node::Bool(true);
    }
    if let Some(returns) = returns_ty {
        function["ctype_returns"] = returns.to_node();
    }
    if state.strict {
        function["std"] = Node::from("c89");
    }
    Ok(Some(function))
}

/// Whether an extern declares a native (extern int f(...) NATIVE(n);) rather than a C object or
/// function: NATIVE comes before the ;.
fn native_ahead(state: &State) -> bool {
    (0..)
        .map(|offset| state.token(offset))
        .take_while(|token| token.kind != Kind::Eof && token.text() != ";")
        .any(|token| token.kind == Kind::Name && token.text() == "NATIVE")
}

/// A declaration at file scope: a function's definition or prototype, or globals. A global's value
/// is set once before the script runs (see lower.rs). Ok(true) for a function's definition.
fn top_declaration(state: &mut State, program: &mut Node, pending: &Pending, doc: Option<&str>) -> Parsed<bool> {
    let spec = specifiers(state, false)?;
    if matches!(spec.storage.as_deref(), Some("auto" | "register")) {
        return state.fail("auto and register belong inside functions");
    }
    if state.at(";", 0) {
        state.take();
        return Ok(false);
    }
    // a function returning a pointer: its *s, then its name and (
    let stars = (0..).take_while(|offset| state.at("*", *offset)).count();
    if state.peek(stars).kind == Kind::Name && state.at("(", stars + 1) {
        let mut returns_ty = spec.ty.clone();
        if stars > 0 {
            let mut ty = base_ty(state, &spec)?;
            for _ in 0..stars {
                state.take();
                ty = ctype::pointer(ty);
            }
            returns_ty = Some(ty);
        }
        let returns = if returns_ty.is_some() { "int".to_string() } else { spec.kind.clone() };
        let name = state.name()?;
        let function =
            state.framed(|state| function_definition(state, pending, &returns, &name, doc, returns_ty.clone()))?;
        let Some(function) = function else {
            return Ok(false);
        };
        program["functions"].as_array_mut().unwrap().push(function);
        return Ok(true);
    }
    if spec.kind == "void" && !complex_ahead(state, &spec) {
        return state.fail("variables cannot be void");
    }
    for declared in declarators(state, &spec)? {
        if declared.ty.is_some() || declared.size() != &Size::None || declared.pointer {
            typed_global(state, program, &spec, declared)?;
            continue;
        }
        let mut entry = node!({"name": &declared.name, "type": &spec.kind});
        match declared.init {
            Some(Init::Value(value)) => entry["init"] = value,
            Some(Init::List(items)) => match items.as_slice() {
                [Init::Value(value)] => entry["init"] = value.clone(),
                _ => return state.fail(format!("too many values for {}", declared.name)),
            },
            None => {}
        }
        if spec.constant {
            entry["const"] = Node::Bool(true);
        }
        if state.strict && entry.get("init").is_some() {
            entry["std"] = Node::from("c89");
        }
        let globals = program["globals"].as_array_mut().unwrap();
        match globals.iter_mut().find(|global| global["name"] == *declared.name) {
            // a tentative definition, or an extern declaration, of a global declared before
            Some(existing) if existing["type"] == entry["type"] => {
                if entry.get("init").is_some() {
                    if existing.get("init").is_some() {
                        return state.fail(format!("{} is given a value twice", declared.name));
                    }
                    *existing = entry.clone();
                }
            }
            Some(_) => return state.fail(format!("{} is declared again with another type", declared.name)),
            None => globals.push(entry.clone()),
        }
        state.globals.insert(declared.name.clone(), entry);
    }
    Ok(false)
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

/// An enumerator's value: a number, as headers write them, or any integer constant expression of
/// numbers, this file's integer #defines and the enumerators before it.
fn enum_value(state: &mut State) -> Parsed<i128> {
    let negative = state.at("-", 0);
    let offset = usize::from(negative);
    let plain = state.token(offset).kind == Kind::Int
        && state.token(offset + 1).kind == Kind::Op
        && [",", "}"].contains(&state.token(offset + 1).text());
    if !plain {
        return Ok(constant_expression(state)? as i128);
    }
    if negative {
        state.take();
    }
    let value = int_value(state.expect_kind(Kind::Int)?.text())?;
    Ok(if negative { -value } else { value })
}

fn declarations<'a>(program: &'a mut Node, key: &str) -> &'a mut Vec<Node> {
    program["declarations"][key].as_array_mut().unwrap()
}

/// enum NAME { ... }; or an anonymous enum { ... };.
fn enumeration(state: &mut State, program: &mut Node) -> Parsed<()> {
    let enumeration = if state.at("{", 0) { None } else { Some(state.name()?) };
    let members = enumerators(state, enumeration.as_deref())?;
    let key = if enumeration.as_deref() == Some("program") { "programs" } else { "constants" };
    declarations(program, key).extend(members);
    state.expect(";")?;
    Ok(())
}

/// An enum's { ... }: its members as constants (as program names for enum program).
fn enumerators(state: &mut State, enumeration: Option<&str>) -> Parsed<Vec<Node>> {
    state.expect("{")?;
    let mut members = Vec::new();
    let mut value = 0i128;
    while !state.at("}", 0) {
        let name = state.name()?;
        if state.at("=", 0) {
            state.take();
            value = enum_value(state)?;
        }
        if enumeration == Some("program") {
            members.push(node!({"name": name, "no": int_node(value)?}));
        } else {
            state.enumerators.insert(name.clone(), value);
            members.push(node!({"name": name, "value": int_literal(value)?, "enum": enumeration}));
        }
        value += 1;
        if !state.at(",", 0) {
            break;
        }
        state.take();
    }
    state.expect("}")?;
    Ok(members)
}

/// Whether an enum definition at file scope stands alone (enum e { ... };), as headers write them,
/// rather than starting a declaration (enum e { ... } x;).
fn enum_alone(state: &State) -> bool {
    let close = (0..)
        .map(|offset| (offset, state.token(offset)))
        .find(|(_, token)| token.kind == Kind::Eof || (token.kind == Kind::Op && token.text() == "}"));
    close.is_some_and(|(offset, _)| state.at(";", offset + 1))
}

/// The constant a #define's value stands for, when the compiler keeps it as a name: a number or a
/// string, a negative number, or a unary expression of integer constants (a parenthesised one, as
/// (-2147483647 - 1)). Any other value is text the tokenizer puts where the name is used.
/// frames: Python frames in use, this function's included.
fn constant_of(value: &str, frames: usize) -> Parsed<Option<Node>> {
    let chars: Vec<char> = value.chars().collect();
    let mut inner = State::new(tokenize(&chars, Macros::default(), "", frames + 1)?, frames);
    let mut value = match unary(&mut inner) {
        Ok(value) => value,
        Err(Fault::Value(_)) => return Ok(None),
        Err(crash) => return Err(crash),
    };
    if inner.peek(0).kind != Kind::Eof {
        return Ok(None);
    }
    if value.kind() == "unary" && value["op"] == "-" && value["operand"].kind() == "int" {
        let negated = -(value["operand"]["value"].as_i64().unwrap() as i128);
        let unsigned = value["operand"].get("unsigned").is_some();
        value = if unsigned { unsigned_literal(negated as i32 as i128)? } else { int_literal(negated)? };
    }
    if matches!(value.kind(), "int" | "float" | "string") {
        return Ok(Some(value));
    }
    Ok(folded(&value, &|_| None).map(|(value, unsigned)| {
        let found = node!({"kind": "int", "value": value});
        if unsigned {
            found.with("unsigned", true)
        } else {
            found
        }
    }))
}

/// A #define whose value is a number or a string: a constant, and an integer one sizes arrays.
fn definition(state: &mut State, token: &Token, program: &mut Node) -> Parsed<()> {
    let [name, value] = token.words.as_slice() else {
        return Ok(());
    };
    // definition's own frame, then tokenize's
    if let Some(value) = constant_of(value, state.depth + 1)? {
        let integer = value["value"].as_i64().filter(|_| value.kind() == "int");
        declarations(program, "constants").push(node!({"name": name, "value": value, "enum": null}));
        if let Some(integer) = integer {
            state.constants.insert(name.clone(), integer as i128);
        }
    }
    Ok(())
}

/// An integer constant expression's value, wrapped to 32 bits, and whether it is unsigned; None
/// when it is not one (names give their value through names). Unsigned division, remainder,
/// comparison and right shifts are left to the compiler.
fn folded(node: &Node, names: &dyn Fn(&str) -> Option<i64>) -> Option<(i64, bool)> {
    let wrap = |value: i64| value as i32 as i64;
    match node.kind() {
        "int" => Some((wrap(node["value"].as_i64()?), node.get("unsigned").is_some())),
        "var" => names(field(node, "name")).map(|value| (wrap(value), false)),
        "cast" if matches!(field(node, "type"), "int" | "long") => folded(&node["operand"], names),
        "unary" => {
            let (value, unsigned) = folded(&node["operand"], names)?;
            Some(match field(node, "op") {
                "-" => (wrap(value.wrapping_neg()), unsigned),
                "+" => (value, unsigned),
                "!" => (i64::from(value == 0), false),
                _ => return None,
            })
        }
        "ternary" => {
            let (cond, _) = folded(&node["cond"], names)?;
            let (then, then_unsigned) = folded(&node["then"], names)?;
            let (otherwise, otherwise_unsigned) = folded(&node["else"], names)?;
            Some((if cond != 0 { then } else { otherwise }, then_unsigned || otherwise_unsigned))
        }
        "logical" => {
            let (left, _) = folded(&node["left"], names)?;
            let (right, _) = folded(&node["right"], names)?;
            let truth = if node["op"] == "&&" { left != 0 && right != 0 } else { left != 0 || right != 0 };
            Some((i64::from(truth), false))
        }
        "binary" => {
            let (left, left_unsigned) = folded(&node["left"], names)?;
            let (right, right_unsigned) = folded(&node["right"], names)?;
            let unsigned = left_unsigned || right_unsigned;
            let op = field(node, "op");
            if unsigned && matches!(op, "/" | "%" | "<" | "<=" | ">" | ">=" | ">>") {
                return None;
            }
            let (a, b) = (left as i32, right as i32);
            let value = match op {
                "+" => a.wrapping_add(b),
                "-" => a.wrapping_sub(b),
                "*" => a.wrapping_mul(b),
                "/" if b != 0 => a.wrapping_div(b),
                "%" if b != 0 => a.wrapping_rem(b),
                "&" => a & b,
                "|" => a | b,
                "^" => a ^ b,
                "<<" if (0..32).contains(&b) => a.wrapping_shl(b as u32),
                ">>" if (0..32).contains(&b) => a >> b,
                "==" => i32::from(a == b),
                "!=" => i32::from(a != b),
                "<" => i32::from(a < b),
                "<=" => i32::from(a <= b),
                ">" => i32::from(a > b),
                ">=" => i32::from(a >= b),
                _ => return None,
            };
            let compared = matches!(op, "==" | "!=" | "<" | "<=" | ">" | ">=");
            Some((value as i64, unsigned && !compared))
        }
        _ => None,
    }
}

/// A node's text field, or "".
fn field<'a>(node: &'a Node, key: &str) -> &'a str {
    node[key].as_str().unwrap_or("")
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
        // #pragma alone, which C ignores
        return Ok(());
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
        "std" => match first()?.as_str() {
            // C's semantics where the VM's differ, for the code after it (docs/c-standard.md)
            Some("c89" | "c90") => state.strict = true,
            _ => return state.fail("#pragma std takes c89"),
        },
        "once" => {}
        // C ignores a pragma it does not know
        _ => {}
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

/// The name __FILE__ gives a file parsed without one.
pub const UNNAMED_FILE: &str = "<script>";

/// parse.parse(text): the program a script's source holds.
pub fn parse(text: &str) -> Result<Node> {
    parse_with(text, &HashMap::new())
}

/// parse.parse(text, defines): with #defines in force before the first line (a mod patch's
/// LANGUAGE).
pub fn parse_with(text: &str, defines: &HashMap<String, String>) -> Result<Node> {
    Ok(parse_program(text, defines, UNNAMED_FILE, None)?)
}

/// The program a file's source holds, with its name for __FILE__.
pub fn parse_file(text: &str, file: &str) -> Result<Node> {
    Ok(parse_program(text, &HashMap::new(), file, None)?)
}

/// A source file as a compiler reads it: its #include <...> headers (from those given) read as text
/// for their macros, so a header's #defines reach the file's #if as C has it.
pub fn parse_source(text: &str, file: &str, headers: &IndexMap<String, String>) -> Result<Node> {
    Ok(parse_program(text, &HashMap::new(), file, Some(headers))?)
}

fn parse_program(text: &str, defines: &HashMap<String, String>, file: &str, headers: Headers) -> Parsed<Node> {
    Ok(parse_state(text, defines, file, headers)?.0)
}

thread_local! {
    /// The typedefs a header (and those it includes) declares, by its path and text.
    static HEADER_KINDS: RefCell<HashMap<u64, HashMap<String, String>>> = RefCell::new(HashMap::new());
}

/// The typedefs of a header and of those it includes, for State::header_kinds.
fn header_kinds(path: &str, headers: &IndexMap<String, String>) -> Parsed<HashMap<String, String>> {
    let Some(text) = headers.get(path).filter(|_| path != crate::sdk::PRELUDE) else {
        return Ok(HashMap::new());
    };
    let key = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (path, text).hash(&mut hasher);
        hasher.finish()
    };
    if let Some(found) = HEADER_KINDS.with(|cache| cache.borrow().get(&key).cloned()) {
        return Ok(found);
    }
    let (_, found) = parse_state(text, &HashMap::new(), path, Some(headers))
        .map_err(|fault| Fault::Value(format!("in <{path}>: {}", Error::from(fault))))?;
    HEADER_KINDS.with(|cache| cache.borrow_mut().insert(key, found.clone()));
    Ok(found)
}

/// A file's program, and the typedefs it knows at its end (its own and its headers').
fn parse_state(
    text: &str,
    defines: &HashMap<String, String>,
    file: &str,
    headers: Headers,
) -> Parsed<(Node, HashMap<String, String>)> {
    let chars: Vec<char> = text.chars().collect();
    let mut macros = Macros::default();
    for (name, value) in defines {
        macros.define(name, cpp::Macro { params: None, body: value.clone() }, true);
    }
    let tokens = tokenize_with(&chars, &mut macros, file, CALLER_FRAMES + 2, headers, 0)?;
    let mut state = State::new(tokens, CALLER_FRAMES + 1);
    if state.tokens[0].kind == Kind::Assembly {
        return Ok((node!({"kind": "program", "asm": state.tokens[0].text()}), HashMap::new()));
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
            Kind::Include => {
                if let Some(headers) = headers.filter(|_| token.system) {
                    let found = header_kinds(&token.path, headers)?;
                    state.header_kinds.extend(found);
                }
                program["functions"].as_array_mut().unwrap().push(node!({
                    "kind": "include",
                    "path": &token.path,
                    "system": token.system,
                }))
            }
            Kind::Assembly => return state.fail("#pragma assembly must start the file"),
            Kind::Define => definition(&mut state, &token, &mut program)?,
            _ if word == "enum" && (state.at("{", 0) || state.at("{", 1)) && enum_alone(&state) => {
                enumeration(&mut state, &mut program)?
            }
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
                let base = if state.at("any", 0) {
                    type_name(&mut state, &NATIVE_TYPES)?
                } else {
                    let spec = specifiers(&mut state, false)?;
                    if typed_typedef(&mut state, &spec)? {
                        continue;
                    }
                    base_type(&state, &spec.kind)?
                };
                let name = state.name()?;
                state.expect(";")?;
                program["declarations"]["kinds"][name.as_str()] = Node::from(&base);
                state.kinds.insert(name, base);
            }
            _ if word == "extern" && native_ahead(&state) => {
                let native = native_declaration(&mut state)?;
                declarations(&mut program, "natives").push(native);
            }
            _ if word == "static" && state.at("inline", 0) => {
                state.expect("inline")?;
                let inline = state.framed(inline_definition)?;
                declarations(&mut program, "inlines").push(inline);
            }
            _ if token.kind == Kind::Name && type_word(&state, word) => {
                let doc = token.doc.clone();
                state.pos -= 1;
                if top_declaration(&mut state, &mut program, &pending, doc.as_deref())? {
                    pending = Pending::default();
                }
            }
            _ => {
                state.pos -= 1;
                let found = state.shown(&token)?;
                return state.fail(format!("expected a declaration, found {found}"));
            }
        }
        let found = std::mem::take(&mut state.constants_found);
        declarations(&mut program, "constants").extend(found);
    }
    // static locals' hidden globals
    program["globals"].as_array_mut().unwrap().extend(std::mem::take(&mut state.statics));
    if program["global_num"].is_null() {
        let slots: i64 =
            program["globals"].as_array().unwrap().iter().map(|global| global["size"].as_i64().unwrap_or(1)).sum();
        program["global_num"] = Node::from(slots);
    }
    if !state.structs.is_empty() {
        program["structs"] = ctype::structs_node(&state.structs);
    }
    let mut kinds = std::mem::take(&mut state.header_kinds);
    kinds.extend(state.kinds.clone());
    Ok((with_own_padding(program), kinds))
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
        let want = r#""globals":[],"global_num":0,"main":null,"programs":[],"functions":[{"kind":"function","name":"f","returns":"void","params":[],"locals":[{"name":"t","type":"int","size":3}],"body":[{"kind":"expr","expr":{"kind":"assign","target":{"kind":"index","name":"t","index":{"kind":"int","value":0}},"value":{"kind":"int","value":1}},"line":3},{"kind":"expr","expr":{"kind":"assign","target":{"kind":"index","name":"t","index":{"kind":"int","value":1}},"value":{"kind":"int","value":2}},"line":3},{"kind":"expr","expr":{"kind":"assign","target":{"kind":"index","name":"t","index":{"kind":"int","value":2}},"value":{"kind":"int","value":0}},"line":3},{"kind":"expr","expr":{"kind":"assign","target":{"kind":"var","name":"x"},"value":{"kind":"binary","op":"+","left":{"kind":"binary","op":"+","left":{"kind":"int","value":97},"right":{"kind":"int","value":16}},"right":{"kind":"float","bits":1069547520}}},"line":3}],"unused_strings":[],"padding":null}],"string_pool":false,"padding_of":{},"declarations":{"natives":[],"programs":[],"inlines":[],"constants":[{"name":"N","value":{"kind":"int","value":3},"enum":null},{"name":"A","value":{"kind":"int","value":0},"enum":"e"},{"name":"B","value":{"kind":"int","value":-2},"enum":"e"}],"kinds":{}}}"#;
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
    fn conditions_read_as_c_reads_them() {
        let text = "#if LANGUAGE == LANGUAGE_FRENCH\nint fr;\n#elif LANGUAGE\nint other;\n#else\nint none;\n#endif\n";
        let defines: HashMap<String, String> =
            [("LANGUAGE", "2"), ("LANGUAGE_FRENCH", "2")].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let program = parse_with(text, &defines).unwrap();
        assert_eq!(program["globals"].to_string(), r#"[{"name":"fr","type":"int"}]"#);
        // C's precedence: 3 & (1 == 1), (!2) == 1; / truncates
        for (condition, value) in [("3 & 1 == 1", true), ("!2 == 1", false), ("-7 / 2 == -3", true), ("1 && 0", false)]
        {
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
            ("#if (1\n#endif\n", "line 1: cannot read the condition '(1': ( without )"),
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
