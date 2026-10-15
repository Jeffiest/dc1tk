//! The C-like script language: the tokenizer, which is also the preprocessor (comments, trigraphs,
//! #if/#ifdef/#ifndef/#elif/#else/#endif, #define/#undef and macros, #include, #pragma, #line,
//! #error, asm blocks; see cpp.rs), and the recursive-descent parser that turns a file into a
//! program tree. The forms the decompiler writes parse to the tree it was written from, key for
//! key, so a decompiled script compiles back to its own bytes. C89's other forms parse to trees of
//! forms the compiler has, or to the ones it lowers (lower.rs, and composite.rs for C's pointers
//! and composite types, which declarations give their C types, ctype.rs); see docs/c-standard.md.
//!
//! A #pragma's words are read as the assembly reads a line (asm::tokenize): a plain word is a
//! string and a quoted one {"string": text}.
//!
//! An error names the line it is on. An integer literal past 32 bits is an error, as C has no type
//! for it here, and so is nesting deeper than NESTING.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use dc2tk_py::{ascii_digits, int_base, is_space, repr, split, strip, IntError};
use indexmap::IndexMap;

use crate::cpp::{self, Macros};
use crate::ctype::{self, Composite, Structs, Ty};
use crate::node::Node;
use crate::{asm, stb, Error, Result};

/// How deep expressions, statements, initializers and declarators may nest, counted where each
/// recursion of the parser passes (State::nested), and for each link of a chain the parser reads in
/// a loop but which nests in the tree (a + b + c is (a + b) + c; State::chain). Past it a file is
/// refused ("nested too deeply") rather than overflowing the stack: within it the parser keeps to
/// a 1 MB stack (a Windows main thread's) in a debug build, whose frames are the largest; the
/// functions on its way down are kept small for that. Parentheses take three of it each (an
/// assignment, a ternary and a unary expression), a block, if, loop or switch two (the statement
/// and itself), so parentheses nest 64 deep, statements 97, and an operator chain is 194 long.
pub const NESTING: usize = 200;

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

/// A binary operator's precedence, as C's grammar orders them (higher binds tighter); the parser
/// reads and the source writer (source.rs) parenthesises by it. Assignment is below them all, and
/// unary operators above.
pub fn precedence(op: &str) -> Option<u32> {
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
        "NEW_COUNTER" => "counter",
        "NEW_PROGRAM" => "program",
        "NEW_MONSTER_VAR" => "monster_var",
        _ => return None,
    })
}

fn lex_fail<T>(line: usize, reason: impl std::fmt::Display) -> Result<T> {
    Err(Error::invalid(format!("line {line}: {reason}")))
}

/// A text quoted for an error message, its first 200 characters.
fn short_repr(text: &str) -> String {
    repr(text).chars().take(200).collect()
}

/// A whole number in a base (0: the base its prefix gives, as 0x).
fn int_in_base(text: &str, base: u32) -> Result<i128> {
    int_base(text, base).map_err(|error| match error {
        IntError::Invalid => {
            let kind = match base {
                8 => "an octal",
                10 => "a decimal",
                16 => "a hexadecimal",
                _ => "a",
            };
            Error::invalid(format!("{} is not {kind} number", short_repr(text)))
        }
        IntError::TooLarge => Error::invalid(format!("{text} is too large a number")),
    })
}

/// The character a \x escape's two hexadecimal digits give.
fn hex_escape(digits: &[char], line: usize) -> Result<char> {
    let digits: String = digits.iter().collect();
    match int_base(&digits, 16).ok().and_then(|value| u32::try_from(value).ok()).and_then(char::from_u32) {
        Some(c) => Ok(c),
        None => lex_fail(line, format!("\\x takes two hexadecimal digits, not {}", short_repr(&digits))),
    }
}

/// A 64-bit integer node value.
fn int_node(value: i128) -> Result<Node> {
    i64::try_from(value).map(Node::Int).map_err(|_| Error::invalid(format!("{value} does not fit in 64 bits")))
}

fn int_literal(value: i128) -> Result<Node> {
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
fn condition_value(text: &str, macros: &Macros, line: usize) -> Result<bool> {
    cpp::condition(text, macros)
        .or_else(|reason| lex_fail(line, format!("cannot read the condition {}: {reason}", repr(strip(text)))))
}

fn conditional_directive(
    word: &str,
    rest: &str,
    conditions: &mut Vec<Block>,
    macros: &Macros,
    line: usize,
) -> Result<()> {
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
    /// The token's text; #define, #include, #pragma and asm tokens have none.
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
    /// The integer constants an #include's file #defines (or changes), with their values.
    constants: Vec<(String, i128)>,
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
            constants: Vec::new(),
            lines: Vec::new(),
            wide: false,
        }
    }

    fn text(&self) -> &str {
        self.text.as_deref().unwrap_or("")
    }
}

/// A string literal from the opening quote: its text and the position after it.
fn read_string(text: &[char], position: usize, line: usize) -> Result<(String, usize)> {
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
            out.push(hex_escape(&text[(position + 2).min(text.len())..(position + 4).min(text.len())], line)?);
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
fn read_char(text: &[char], position: usize, line: usize) -> Result<(i64, usize)> {
    match cpp::character_constant(&text[position..]) {
        Ok((value, length)) if text[position + 1] == '\\' && value < 256 => {
            Ok((value as u8 as i8 as i64, position + length))
        }
        Ok((value, length)) => Ok((value as i64, position + length)),
        Err(reason) => lex_fail(line, reason),
    }
}

/// A directive's words as the assembly reads a line (asm::tokenize): a plain word as a string, a
/// quoted one as {"string": text}.
fn directive_words(rest: &str, line: usize) -> Result<Vec<Node>> {
    let tokens = asm::tokenize(rest).or_else(|error| lex_fail(line, error))?;
    let words = tokens.into_iter().map(|token| match token {
        asm::Token::Word(word) => Node::from(word),
        asm::Token::Text(text) => node!({"string": text}),
    });
    Ok(words.collect())
}

/// An asm block from after its "asm": the token and the position after its closing }.
fn read_asm(text: &[char], position: usize, line: usize) -> Result<(Token, usize)> {
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
            && line.chars().next().is_some_and(dc2tk_py::is_alpha);
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
    let decimal = |i: usize| dc2tk_py::is_decimal(at(i));
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

/// The files #include reads as text (parse_source): the SDK's and definitions' headers, and the
/// libraries #include "..." names, each by its path.
#[derive(Clone, Copy)]
pub struct Files<'a> {
    pub headers: &'a IndexMap<String, String>,
    pub libraries: Option<&'a IndexMap<String, String>>,
}

impl<'a> Files<'a> {
    /// An included file's text: a header's by its path, a library's by its path in quotes.
    fn get(&self, key: &str) -> Option<&'a String> {
        match key.strip_prefix('"').and_then(|key| key.strip_suffix('"')) {
            Some(path) => self.libraries?.get(path),
            None => self.headers.get(key),
        }
    }
}

type Headers<'a> = Option<Files<'a>>;

/// The key of an included file among Files: a header's path, a library's path in quotes.
fn included_key(path: &str, system: bool) -> String {
    if system {
        path.to_string()
    } else {
        format!("\"{path}\"")
    }
}

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
fn header_macros(path: &str, headers: Files, macros: Macros, depth: usize) -> Result<Macros> {
    let Some(text) = headers.get(path).filter(|_| path != crate::sdk::PRELUDE) else {
        return Ok(macros);
    };
    if depth > INCLUDE_DEPTH {
        return Err(Error::invalid(format!("#include {path}: files include each other too deeply")));
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
    tokenize_with(&chars, &mut found, path, Some(headers), depth + 1)
        .map_err(|fault| Error::invalid(format!("in <{path}>: {}", fault)))?;
    HEADER_MACROS.with(|cache| cache.borrow_mut().insert(key, found.clone()));
    Ok(found)
}

/// The tokens of a file. macros are the #defines in force before the first line (a mod patch's
/// LANGUAGE), which stay constants in code; file is the name __FILE__ gives until a #line names
/// another.
fn tokenize(source: &[char], mut macros: Macros, file: &str) -> Result<Vec<Token>> {
    tokenize_with(source, &mut macros, file, None, 0)
}

/// tokenize, reading the headers given for their macros; macros ends as the file leaves them.
fn tokenize_with(
    source: &[char],
    macros: &mut Macros,
    file: &str,
    headers: Headers,
    depth: usize,
) -> Result<Vec<Token>> {
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
                            Some(value) => constant_of(value)?,
                            None => None,
                        };
                        let integer = constant.as_ref().filter(|value| value.kind() == "int");
                        let integer = integer.and_then(|value| value["value"].as_i64());
                        macros.define(&name, definition, constant.is_some());
                        if let Some(integer) = integer {
                            macros.values.insert(name.clone(), integer as i128);
                        }
                        let mut token = Token::new(Kind::Define, line, None);
                        token.words = words;
                        tokens.push(token);
                    }
                }
                "include" => {
                    let rest = strip(rest);
                    let mut token = Token::new(Kind::Include, line, None);
                    let before = headers.map(|_| macros.values.clone());
                    if rest.starts_with('<') && rest.ends_with('>') {
                        token.path = strip(&rest[1..rest.len() - 1]).to_string();
                        token.system = true;
                        if let Some(headers) = headers {
                            *macros = header_macros(&token.path, headers, std::mem::take(macros), depth)?;
                        }
                    } else {
                        let words = directive_words(rest, line)?;
                        match words.as_slice() {
                            [word] if word.as_object().is_some() => {
                                token.path = word["string"].as_str().unwrap().to_string()
                            }
                            _ => return lex_fail(line, "expected #include \"path\" or #include <path>"),
                        }
                        // a library's macros reach the file that includes it, as C includes it
                        if let Some(files) = headers {
                            let key = included_key(&token.path, false);
                            *macros = header_macros(&key, files, std::mem::take(macros), depth)?;
                        }
                    }
                    // the integer constants the file (and what it includes) defines or changes,
                    // which the including file's constant expressions use as C expands them
                    if let Some(before) = before {
                        let changed = macros.values.iter().filter(|(name, value)| before.get(*name) != Some(value));
                        token.constants = changed.map(|(name, value)| (name.clone(), *value)).collect();
                        token.constants.sort();
                    }
                    tokens.push(token);
                }
                "pragma" => {
                    let words = directive_words(rest, line)?;
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
                    let words = directive_words(&expanded.replace(cpp::PAINT, " "), line)?;
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
        } else if dc2tk_py::is_digit(c)
            || (c == '.' && text.get(position + 1).is_some_and(|next| next.is_ascii_digit()))
        {
            let Some((mut word, suffix, end)) = number_at(&text, position) else {
                return Err(Error::invalid(format!("line {line}: cannot read the number at {}", repr(&c.to_string()))));
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
                Some(character) => dc2tk_disc::text::encode(&character.to_string())
                    .ok_or_else(|| Error::invalid(format!("line {line}: {character:?} has no Shift-JIS code")))?
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
    /// The struct and union tags each open block declares, beside blocks: a tag declared in a
    /// block names a struct of its own there (another tag, tag#key_n), hiding one outside it until
    /// the block ends, as C scopes tags.
    tags: Vec<HashMap<String, String>>,
}

impl Scope {
    fn new(function: &str, params: Vec<Node>) -> Scope {
        let names = params.iter().map(|param| (param.text("name").to_string(), param.text("name").to_string()));
        Scope {
            function: function.to_string(),
            blocks: vec![names.collect()],
            params,
            locals: Vec::new(),
            statics: HashMap::new(),
            labels: Vec::new(),
            gotos: Vec::new(),
            typedefs: Vec::new(),
            tags: vec![HashMap::new()],
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
    /// The integer #defines and the enumerators of the headers and libraries included so far,
    /// for constant expressions as C reads them. They stay names in code (and a size a name alone
    /// gives stays the name), which sdk::resolve gives the same values.
    included_constants: HashMap<String, i128>,
    included_enumerators: HashMap<String, i128>,
    /// Enumerators an enum defined in a declaration gave, for the program's constants.
    constants_found: Vec<Node>,
    /// typedef'd value kinds and their base types.
    kinds: HashMap<String, String>,
    scope: Option<Scope>,
    /// The globals declared so far, by name, and the hidden globals of static locals.
    globals: HashMap<String, Node>,
    statics: Vec<Node>,
    /// How deep the parser is nested, against NESTING.
    depth: usize,
    /// #pragma std c89 has been read: the functions and globals after it get C's semantics.
    strict: bool,
    /// The structs and unions declared so far, by tag.
    structs: Structs,
    /// typedefs of types beyond the scalars (pointers, structs, arrays, functions).
    types: HashMap<String, Ty>,
    /// Unnamed structs so far, for their tags.
    anonymous: usize,
    /// What makes the file's unnamed structs' tags its own: a hash of its text, so that two files
    /// (two mods' patches, a script and its library) never give two different structs one tag, and
    /// a file read twice (as an include and as a library, under two names) gives the same tags.
    file_key: String,
    /// The typedefs of the headers and libraries included so far, read when they are given
    /// (parse_source): what a type's size and slots need. They are not type words here, as a
    /// script's names may be the same (sdk::resolve reads the headers' kinds for the rest).
    header_kinds: HashMap<String, String>,
}

impl State {
    fn new(tokens: Vec<Token>) -> State {
        let (constants, kinds) = (HashMap::new(), HashMap::new());
        State {
            tokens: tokens.into(),
            pos: 0,
            constants,
            enumerators: HashMap::new(),
            included_constants: HashMap::new(),
            included_enumerators: HashMap::new(),
            constants_found: Vec::new(),
            kinds,
            scope: None,
            globals: HashMap::new(),
            statics: Vec::new(),
            depth: 0,
            strict: false,
            structs: Structs::new(),
            // what <stdarg.h>'s va_list stands for: a pointer into a variadic call's arguments
            types: HashMap::from([("__va_list".to_string(), ctype::pointer(ctype::int()))]),
            anonymous: 0,
            file_key: String::new(),
            header_kinds: HashMap::new(),
        }
    }

    /// An integer constant's value by its name: this file's #define or enumerator, or one of a
    /// header or library it includes.
    fn constant(&self, name: &str) -> Option<i128> {
        let own = self.constants.get(name).or_else(|| self.enumerators.get(name));
        let included = || self.included_constants.get(name).or_else(|| self.included_enumerators.get(name));
        own.or_else(included).copied()
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
            scope.tags.push(HashMap::new());
        }
    }

    fn close_block(&mut self) {
        if let Some(scope) = &mut self.scope {
            scope.blocks.pop();
            scope.tags.pop();
        }
    }

    /// The struct a tag names where the parser is: the innermost block's that declares it, or the
    /// file's.
    fn tag(&self, name: &str) -> String {
        let blocks = self.scope.as_ref().map_or(&[][..], |scope| scope.tags.as_slice());
        blocks.iter().rev().find_map(|block| block.get(name)).cloned().unwrap_or_else(|| name.to_string())
    }

    /// A tag the innermost block declares (struct T { ... } or struct T;): a struct of its own,
    /// unless the block declared the tag already. At file scope, the tag itself.
    fn declared_tag(&mut self, name: &str) -> String {
        let key = self.file_key.clone();
        let Some(scope) = self.scope.as_mut() else {
            return name.to_string();
        };
        let blocks = scope.tags.len();
        let innermost = scope.tags.last_mut().expect("a function has a block");
        if let Some(known) = innermost.get(name) {
            return known.clone();
        }
        self.anonymous += 1;
        let tag = format!("{name}#{key}_{}_{blocks}", self.anonymous);
        innermost.insert(name.to_string(), tag.clone());
        tag
    }

    fn token(&self, offset: usize) -> &Token {
        &self.tokens[(self.pos + offset).min(self.tokens.len() - 1)]
    }

    fn at(&self, text: &str, offset: usize) -> bool {
        let token = self.token(offset);
        matches!(token.kind, Kind::Op | Kind::Name) && token.text() == text
    }

    fn take(&mut self) -> Taken {
        let index = (self.pos).min(self.tokens.len() - 1);
        self.pos += 1;
        Taken { tokens: Rc::clone(&self.tokens), index }
    }

    fn line(&self) -> usize {
        self.token(0).line
    }

    fn fail<T>(&self, reason: impl std::fmt::Display) -> Result<T> {
        lex_fail(self.token(0).line, reason)
    }

    /// repr() of a token's text for a message; a directive's or asm block's token has none, and
    /// is unexpected wherever a message would quote one.
    fn shown(&self, token: &Token) -> Result<String> {
        match &token.text {
            Some(text) => Ok(repr(text)),
            None => Err(Error::invalid(format!("line {}: unexpected {} token", token.line, token.kind.name()))),
        }
    }

    fn expect(&mut self, text: &str) -> Result<Taken> {
        if !self.at(text, 0) {
            let found = self.shown(self.token(0))?;
            return self.fail(format!("expected {}, found {found}", repr(text)));
        }
        Ok(self.take())
    }

    fn expect_kind(&mut self, kind: Kind) -> Result<Taken> {
        if self.token(0).kind != kind {
            let found = self.shown(self.token(0))?;
            return self.fail(format!("expected {}, found {found}", kind.name()));
        }
        Ok(self.take())
    }

    fn name(&mut self) -> Result<String> {
        Ok(self.expect_kind(Kind::Name)?.text().to_string())
    }

    /// Parses one level deeper, refusing to go past NESTING.
    fn nested<T>(&mut self, parse: impl FnOnce(&mut State) -> Result<T>) -> Result<T> {
        self.link()?;
        let result = parse(self);
        self.depth -= 1;
        result
    }

    /// Parses a chain whose every link nests what came before it in the tree (a + b + c is
    /// (a + b) + c, and a.b.c is (a.b).c), though the parser reads it in a loop: each State::link
    /// is a level of NESTING, given back when the chain ends.
    fn chain<T>(&mut self, parse: impl FnOnce(&mut State) -> Result<T>) -> Result<T> {
        let depth = self.depth;
        let result = parse(self);
        self.depth = depth;
        result
    }

    /// One more link of a chain (State::chain): a level deeper, refused past NESTING.
    fn link(&mut self) -> Result<()> {
        if self.depth >= NESTING {
            return self.fail("nested too deeply");
        }
        self.depth += 1;
        Ok(())
    }
}

/// An integer literal's value: hexadecimal after 0x, octal after a leading 0, as in C.
fn int_value(text: &str) -> Result<i128> {
    let hexadecimal = text.starts_with("0x") || text.starts_with("0X");
    if !hexadecimal && text.len() > 1 && text.starts_with('0') && text.chars().all(|c| c.is_ascii_digit()) {
        return int_in_base(&text[1..], 8);
    }
    int_in_base(text, if hexadecimal { 16 } else { 10 })
}

/// An unsigned int literal, held as the VM's 32 bits.
fn unsigned_literal(value: i128) -> Result<Node> {
    Ok(int_literal(value as u32 as i32 as i128)?.with("unsigned", true))
}

fn literal(state: &State, token: &Token) -> Result<Node> {
    if token.kind == Kind::Int {
        let value = int_value(token.text()).or_else(|error| lex_fail(token.line, error))?;
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
    let bits = asm::float_bits(&ascii_digits(token.text())).or_else(|error| lex_fail(token.line, error))?;
    Ok(node!({"kind": "float", "bits": bits}))
}

fn arguments(state: &mut State) -> Result<Vec<Node>> {
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
}

// The expression parser recurses once for each level of nesting, so the functions on its way
// down are kept small (a debug build gives a function's every temporary a place of its own in its
// frame): each form is read by a function of its own, and the nodes are built apart from the
// recursion where they are large.

/// A primary expression, and the postfix operators after it.
fn primary(state: &mut State) -> Result<Node> {
    let token = state.take();
    match token.kind {
        Kind::Int | Kind::Float => {
            let number = literal(state, &token)?;
            // 1[a] is a[1]
            subscripted(state, number)
        }
        Kind::Str if token.wide => {
            let wide = string_literal(state, &token)?;
            postfix_operators(state, wide)
        }
        Kind::Str => {
            // "abc"[1]: the literal as an array
            let string = string_literal(state, &token)?;
            subscripted(state, string)
        }
        Kind::Op if token.text() == "(" => parenthesised(state),
        Kind::Name => named_primary(state, token.text()),
        _ => not_an_expression(state, &token),
    }
}

fn not_an_expression(state: &mut State, token: &Token) -> Result<Node> {
    state.pos -= 1;
    let found = state.shown(token)?;
    state.fail(format!("expected an expression, found {found}"))
}

/// A literal followed by [, which subscripts it; else the literal alone.
fn subscripted(state: &mut State, node: Node) -> Result<Node> {
    if state.at("[", 0) {
        return postfix_operators(state, node);
    }
    Ok(node)
}

/// A string literal and those right after it, which are one: plain, or wide (L"...").
fn string_literal(state: &mut State, token: &Token) -> Result<Node> {
    let mut text = token.text().to_string();
    while state.token(0).kind == Kind::Str {
        let next = state.take();
        if next.wide != token.wide {
            return state.fail("a wide string literal cannot be joined to a plain one");
        }
        text.push_str(next.text());
    }
    if token.wide {
        return wide_string(state, &text);
    }
    Ok(node!({"kind": "string", "text": text}))
}

/// ( expression ) after its (, and the postfix operators after it.
fn parenthesised(state: &mut State) -> Result<Node> {
    let node = expression(state)?;
    state.expect(")")?;
    postfix_operators(state, node)
}

/// What a name starts: sin(x) or cos(x), a call (of a function, or through a function pointer
/// variable), offsetof, va_arg or va_start, or a variable; and the postfix operators after it.
fn named_primary(state: &mut State, name: &str) -> Result<Node> {
    if !state.at("(", 0) {
        let node = named(state, name)?;
        return postfix_operators(state, node);
    }
    if BUILTINS.contains(&name) {
        return builtin(state, name);
    }
    match name {
        "__offsetof" => return offsetof(state),
        "__va_arg" => return va_arg(state),
        "__va_start" => return va_start(state),
        _ => {}
    }
    let call = call(state, name)?;
    postfix_operators(state, call)
}

/// sin(x) or cos(x), which the VM has instructions for.
fn builtin(state: &mut State, name: &str) -> Result<Node> {
    state.expect("(")?;
    let operand = assignment(state)?;
    state.expect(")")?;
    Ok(node!({"kind": "builtin", "name": name, "operand": operand}))
}

/// name(args): a call of a function, or through a function pointer the name is a variable of.
fn call(state: &mut State, name: &str) -> Result<Node> {
    let storage = state.storage_name(name);
    let pointer = state.variable(&storage).is_some();
    let args = arguments(state)?;
    Ok(if pointer {
        node!({"kind": "call_ptr", "callee": {"kind": "var", "name": storage}, "args": args})
    } else {
        node!({"kind": "call", "name": name, "args": args})
    })
}

/// The postfix operators after a primary expression: s.m, p->m, a[i] (of any expression), a call
/// through a function pointer, x++ and x--. Each nests what comes before it.
fn postfix_operators(state: &mut State, node: Node) -> Result<Node> {
    state.chain(|state| {
        let mut node = node;
        loop {
            let Some(op) = [".", "->", "[", "(", "++", "--"].into_iter().find(|op| state.at(op, 0)) else {
                return Ok(node);
            };
            state.link()?;
            node = postfix_operator(state, op, node)?;
        }
    })
}

/// One postfix operator applied to what comes before it.
fn postfix_operator(state: &mut State, op: &str, node: Node) -> Result<Node> {
    match op {
        "." | "->" => {
            state.take();
            let name = state.name()?;
            Ok(node!({"kind": "member", "object": node, "name": name, "arrow": op == "->"}))
        }
        "[" => {
            state.take();
            let index = expression(state)?;
            state.expect("]")?;
            Ok(node!({"kind": "subscript", "array": node, "index": index}))
        }
        "(" => {
            let args = arguments(state)?;
            Ok(node!({"kind": "call_ptr", "callee": node, "args": args}))
        }
        _ => {
            if !is_target(&node) {
                return state.fail("++ and -- need a variable");
            }
            state.take();
            writable(state, &node)?;
            Ok(postfix(node, op))
        }
    }
}

/// L"...": the characters' Shift-JIS codes, then 0, as an array of wchar_t (composite.rs gives it
/// storage).
fn wide_string(state: &State, text: &str) -> Result<Node> {
    let mut codes = Vec::new();
    for character in text.chars() {
        let code = if (character as u32) < 0x80 {
            character as u32
        } else {
            match dc2tk_disc::text::encode(&character.to_string()) {
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
fn offsetof(state: &mut State) -> Result<Node> {
    state.expect("(")?;
    let mut ty = abstract_type(state)?;
    state.expect(",")?;
    let mut bytes = 0;
    loop {
        let name = state.name()?;
        let Ty::Struct(tag) = &ty else {
            return state.fail(format!("offsetof needs a struct or union, not {}", ty.shown()));
        };
        let Some(ctype::Member { byte: offset, ty: member, bits, .. }) = ctype::member(&state.structs, tag, &name)
        else {
            return state.fail(format!("{} has no member {name}", ty.shown()));
        };
        if bits.is_some() {
            return state.fail(format!("offsetof of bit-field {name}, which has no byte of its own"));
        }
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
fn va_arg(state: &mut State) -> Result<Node> {
    state.expect("(")?;
    let list = assignment(state)?;
    state.expect(",")?;
    let ty = abstract_type(state)?;
    state.expect(")")?;
    Ok(node!({"kind": "va_arg", "list": list, "ctype": ty.to_node()}))
}

/// va_start(ap, last) from <stdarg.h>: ap at the first of the arguments after last, which must be
/// the function's last parameter.
fn va_start(state: &mut State) -> Result<Node> {
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
fn named(state: &mut State, name: &str) -> Result<Node> {
    let name = state.storage_name(name);
    if !state.at("[", 0) {
        return Ok(node!({"kind": "var", "name": name}));
    }
    if state.variable(&name).is_some_and(|entry| entry.get("ctype").is_some()) {
        // a variable of C's other types: each [] subscripts what the one before gave
        return state.chain(|state| {
            let mut node = node!({"kind": "var", "name": name});
            while state.at("[", 0) {
                state.link()?;
                state.take();
                let index = expression(state)?;
                state.expect("]")?;
                node = node!({"kind": "subscript", "array": node, "index": index});
            }
            Ok(node)
        });
    }
    // element() nests a term for each index
    let mut indices = state.chain(|state| {
        let mut indices = Vec::new();
        while state.at("[", 0) {
            state.link()?;
            state.expect("[")?;
            indices.push(expression(state)?);
            state.expect("]")?;
        }
        Ok(indices)
    })?;
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
fn element(state: &State, name: &str, indices: Vec<Node>) -> Result<Node> {
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
fn compound(target: Node, op: &str, value: Node) -> Node {
    node!({"kind": "compound", "op": op, "target": target, "value": value})
}

fn stepped(target: Node, op: &str) -> Node {
    compound(target, op, one())
}

/// x++ and x--: the variable steps, and the expression's value is the old one, worked back from the
/// new so no temporary is needed. As a statement only the step remains.
fn postfix(target: Node, text: &str) -> Node {
    let (op, undo) = if text == "++" { ("+", "-") } else { ("-", "+") };
    node!({"kind": "binary", "op": undo, "left": stepped(target, op), "right": one(), "postfix": true})
}

fn cast_ahead(state: &State) -> bool {
    state.at("(", 0)
        && state.token(1).kind == Kind::Name
        && CAST_TYPES.contains(&state.token(1).text())
        && state.at(")", 2)
}

fn pointer_cast_ahead(state: &State) -> bool {
    state.at("(", 0)
        && state.token(1).kind == Kind::Name
        && CAST_TYPES.contains(&state.token(1).text())
        && state.at("*", 2)
        && state.at(")", 3)
}

fn is_target(node: &Node) -> bool {
    matches!(node.kind(), "var" | "index" | "deref" | "member" | "subscript")
}

/// The variable an assignment target writes: x, a[i] or *p.
fn target_name(target: &Node) -> &str {
    match target.kind() {
        "deref" => target["operand"].text("name"),
        _ => target.text("name"),
    }
}

/// Refuses writing a const object: a const variable, an element of a const array, or what a
/// pointer to const points at.
fn writable(state: &State, target: &Node) -> Result<()> {
    let name = target_name(target);
    let Some(entry) = state.variable(name) else {
        return Ok(());
    };
    let pointer = entry.flag("pointer") || entry.flag("array");
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

/// Whether the token offset ahead starts a type: a keyword of one, a typedef of this file, or a
/// header's typedef followed by what only a declaration or a cast has there.
fn starts_type(state: &State, offset: usize) -> bool {
    let token = state.token(offset);
    token.kind == Kind::Name && (type_word(state, token.text()) || header_type_ahead(state, offset))
}

/// A typedef of an included header (size_t, a value kind) used as a type word: one the file has
/// not declared as anything else (a variable, an enumerator or a constant hides it, as in C). The
/// SDK's value kinds have names scripts use for variables too, so the name counts as a type only
/// where it is followed by a name or * (a declaration) or ) (a cast, sizeof).
fn header_type_ahead(state: &State, offset: usize) -> bool {
    header_type(state, state.token(offset).text(), state.token(offset + 1))
}

/// header_type_ahead of a name and the token after it.
fn header_type(state: &State, text: &str, next: &Token) -> bool {
    if !state.header_kinds.contains_key(text)
        || state.variable(&state.storage_name(text)).is_some()
        || state.enumerators.contains_key(text)
        || state.constants.contains_key(text)
    {
        return false;
    }
    next.kind == Kind::Name || (next.kind == Kind::Op && ["*", ")"].contains(&next.text()))
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
/// kind from a header (bgm_file track;), which sdk::resolve reads as its base type.
fn specifiers(state: &mut State, header_kinds: bool) -> Result<Spec> {
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
            } else if header_type_ahead(state, 0)
                || (header_kinds
                    && (state.token(1).kind == Kind::Name
                        || (state.token(1).kind == Kind::Op && state.token(1).text() == "*")))
                // an unnamed parameter of a header's type: int f(size_t, size_t);
                || (header_kinds
                    && state.header_kinds.contains_key(&text)
                    && state.token(1).kind == Kind::Op
                    && [",", "["].contains(&state.token(1).text()))
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
fn struct_specifier(state: &mut State, union: bool) -> Result<String> {
    state.nested(|state| {
        let word = if union { "union" } else { "struct" };
        let tag = if state.token(0).kind == Kind::Name {
            let name = state.name()?;
            if state.at("{", 0) || state.at(";", 0) {
                // defined or declared alone here: in a block, a struct of the block's
                state.declared_tag(&name)
            } else {
                state.tag(&name)
            }
        } else if state.at("{", 0) {
            state.anonymous += 1;
            format!("__anonymous_{}_{}", state.file_key, state.anonymous)
        } else {
            let found = state.shown(state.token(0))?;
            return state.fail(format!("expected the name of a {word}, found {found}"));
        };
        let shown = ctype::tag_shown(&tag);
        let other = if union { "struct" } else { "union" };
        if state.structs.get(&tag).is_some_and(|known| known.union != union) {
            return state.fail(format!("{shown} is a {other}, not a {word}"));
        }
        if !state.at("{", 0) {
            // declared here, defined (or not) elsewhere: an incomplete type until then
            state.structs.entry(tag.clone()).or_insert(Composite::new(union, Vec::new()));
            return Ok(tag);
        }
        state.take();
        let mut members: Vec<(String, Ty)> = Vec::new();
        let mut bits: Vec<Option<ctype::Bits>> = Vec::new();
        // the bits the slot bit-fields are filling has used, while one is
        let mut used: Option<i64> = None;
        while !state.at("}", 0) {
            let spec = specifiers(state, true)?;
            if spec.storage.is_some() {
                return state.fail(format!("a member of {word} {shown} has no storage class"));
            }
            let base = base_ty(state, &spec)?;
            loop {
                let (name, ty) = declarator(state, base.clone())?;
                if !name.is_empty() && members.iter().any(|(known, _)| *known == name) {
                    return state.fail(format!("{word} {shown} has two members named {name}"));
                }
                if state.at(":", 0) {
                    state.take();
                    let width = constant_expression(state)?;
                    let kind = match &ty {
                        Ty::Scalar(kind) => state.base_kind(kind),
                        _ => String::new(),
                    };
                    if kind != "int" && kind != "unsigned int" {
                        return state.fail(format!(
                            "bit-field {name} is of {}: a bit-field is an int or an unsigned int",
                            ty.shown()
                        ));
                    }
                    if !(0..=32).contains(&width) {
                        return state.fail(format!("bit-field {name} has {width} bits; an int has 32"));
                    }
                    match width {
                        0 if !name.is_empty() => {
                            return state.fail(format!("bit-field {name} has 0 bits; only an unnamed one may"))
                        }
                        // :0 ends the slot: the next bit-field starts another
                        0 => used = None,
                        32 => {
                            // the whole slot, an int of its own
                            members.push((name, ty));
                            bits.push(None);
                            used = None;
                        }
                        _ => {
                            let offset = match used {
                                Some(taken) if !union && taken + width <= 32 => taken,
                                _ => 0,
                            };
                            members.push((name, ty));
                            bits.push(Some(ctype::Bits { offset, width }));
                            used = Some(offset + width);
                        }
                    }
                } else {
                    if name.is_empty() {
                        return state.fail(format!("a member of {word} {shown} needs a name"));
                    }
                    complete(state, &name, &ty)?;
                    members.push((name, ty));
                    bits.push(None);
                    used = None;
                }
                if !state.at(",", 0) {
                    break;
                }
                state.take();
            }
            state.expect(";")?;
        }
        state.expect("}")?;
        if members.is_empty() {
            return state.fail(format!("{word} {shown} needs a member"));
        }
        let bits = if bits.iter().any(Option::is_some) { bits } else { Vec::new() };
        let composite = Composite { union, members, bits };
        if state.structs.get(&tag).is_some_and(|known| !known.members.is_empty() && *known != composite) {
            return state.fail(format!("{word} {shown} is defined twice"));
        }
        state.structs.insert(tag.clone(), composite);
        Ok(tag)
    })
}

/// Refuses an object of a type with no size: void, a function, a struct not defined, an array
/// whose size nothing gave.
fn complete(state: &State, name: &str, ty: &Ty) -> Result<()> {
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
fn base_ty(state: &State, spec: &Spec) -> Result<Ty> {
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
fn declarator(state: &mut State, base: Ty) -> Result<(String, Ty)> {
    state.nested(|state| {
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
                if state.token(0).kind == Kind::Eof {
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
        let named = state.token(0).kind == Kind::Name && !type_word(state, state.token(0).text());
        let name = if named { state.name()? } else { String::new() };
        Ok((name, suffixes(state, ty)?))
    })
}

/// The [N]s and (parameters) after a declarator's name, applied to ty from the last in.
fn suffixes(state: &mut State, ty: Ty) -> Result<Ty> {
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
fn parameter_types(state: &mut State) -> Result<(Vec<Ty>, bool)> {
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
fn abstract_type(state: &mut State) -> Result<Ty> {
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

/// An integer constant expression (an array size): numbers, the integer #defines and enum members
/// of this file and the files it includes, sizeof, and C's operators.
fn constant_expression(state: &mut State) -> Result<i64> {
    let node = ternary(state)?;
    let found = |name: &str| state.constant(name).map(|value| value as i64);
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
fn elements(state: &State, entry: &Node) -> Result<i64> {
    match &entry["size"] {
        Node::Null => Ok(1),
        Node::Str(name) => match state.constant(name) {
            Some(value) => Ok(value as i64),
            None => state.fail(format!("sizeof needs the value of {name}, which this file does not define or include")),
        },
        size => Ok(size.as_i64().unwrap_or(1)),
    }
}

/// The type of a variable's declaration: its own C type, or one its plain fields give.
fn entry_type(state: &State, entry: &Node) -> Ty {
    if let Some(ctype) = entry.get("ctype") {
        return Ty::from_node(ctype);
    }
    let kind = entry.text("type");
    let scalar = Ty::Scalar(state.kinds.get(kind).cloned().unwrap_or_else(|| kind.to_string()));
    if entry.flag("pointer") || entry.flag("array") {
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
        "var" => typed(node.text("name")).map(|entry| entry_type(state, &entry)),
        "deref" => static_type(state, &node["operand"]).and_then(|ty| ty.target().cloned()),
        "subscript" => {
            let array = static_type(state, &node["array"])
                .or_else(|| state.variable(node["array"].text("name")).map(|entry| entry_type(state, &entry)))?;
            array.target().cloned()
        }
        "member" => member_of(state, node).map(|(ty, _)| ty),
        "addr" => static_type(state, &node["target"]).map(ctype::pointer),
        "wide" => {
            let count = node["codes"].as_array().map_or(1, Vec::len) as i64;
            Some(Ty::Array(Box::new(Ty::Scalar("unsigned short".into())), count))
        }
        "cast" => node.get("ctype").map(Ty::from_node),
        _ => None,
    }
}

/// A member expression's type, and its bits when it is a bit-field.
fn member_of(state: &State, node: &Node) -> Option<(Ty, Option<ctype::Bits>)> {
    let object = static_type(state, &node["object"])
        .or_else(|| state.variable(node["object"].text("name")).map(|entry| entry_type(state, &entry)))?;
    let object = if node["arrow"] == Node::Bool(true) { object.target()?.clone() } else { object };
    let Ty::Struct(tag) = object else {
        return None;
    };
    ctype::member(&state.structs, &tag, node.text("name")).map(|found| (found.ty, found.bits))
}

/// What sizeof gives for an expression, which it does not evaluate.
fn size_of(state: &State, node: &Node) -> Result<i64> {
    if node.kind() == "member" && member_of(state, node).is_some_and(|(_, bits)| bits.is_some()) {
        return state.fail(format!("sizeof of bit-field {}, which has no size of its own", node.text("name")));
    }
    if let Some(ty) = static_type(state, node) {
        return Ok(ty.bytes(&state.structs));
    }
    let entry = |name: &str| state.variable(name);
    Ok(match node.kind() {
        "string" => match stb::encode_string(node.text("text")) {
            Ok(bytes) => bytes.len() as i64,
            Err(error) => return state.fail(error),
        },
        "var" => match entry(node.text("name")) {
            Some(found) if found.flag("pointer") || found.flag("array") => 4,
            Some(found) => elements(state, &found)? * type_size(state, found.text("type")),
            // sizeof(wchar_t): a header's typedef, which reads as a name here
            None if state.header_kinds.contains_key(node.text("name")) => type_size(state, node.text("name")),
            None => 4,
        },
        "index" => entry(node.text("name")).map_or(4, |found| type_size(state, found.text("type"))),
        "deref" => entry(node["operand"].text("name")).map_or(4, |found| type_size(state, found.text("type"))),
        "addr" if node.get("row").is_some() => {
            let row = node["row"].as_i64().unwrap_or(1);
            row * entry(node["target"].text("name")).map_or(4, |found| type_size(state, found.text("type")))
        }
        "cast" if node.text("type") == "void" => return state.fail("sizeof a void expression"),
        "cast" if node.text("type").ends_with('*') => 4,
        "cast" => type_size(state, node.text("type")),
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

fn unary(state: &mut State) -> Result<Node> {
    state.nested(prefixed)
}

/// A unary expression: an operator and its operand, sizeof, a cast, or a primary expression.
fn prefixed(state: &mut State) -> Result<Node> {
    if ["-", "!", "+", "~", "*"].iter().any(|op| state.at(op, 0)) {
        return prefix_operator(state);
    }
    if state.at("sizeof", 0) {
        return sizeof(state);
    }
    if state.at("++", 0) || state.at("--", 0) {
        return increment(state);
    }
    if state.at("&", 0) {
        return address(state);
    }
    if cast_ahead(state) {
        return plain_cast(state);
    }
    if state.at("(", 0) && state.at("enum", 1) && state.token(2).kind == Kind::Name && state.at(")", 3) {
        return enum_cast(state);
    }
    if pointer_cast_ahead(state) {
        return pointer_cast(state);
    }
    if state.at("(", 0) && starts_type(state, 1) {
        return cast(state);
    }
    primary(state)
}

/// -x, !x, +x, ~x (-x - 1, the VM having no instruction for it) and *p.
fn prefix_operator(state: &mut State) -> Result<Node> {
    let op = state.take();
    let operand = unary(state)?;
    Ok(prefix_node(op.text(), operand))
}

fn prefix_node(op: &str, operand: Node) -> Node {
    match op {
        "~" => {
            node!({"kind": "binary", "op": "-", "left": {"kind": "unary", "op": "-", "operand": operand}, "right": one()})
        }
        "*" => node!({"kind": "deref", "operand": operand}),
        _ => node!({"kind": "unary", "op": op, "operand": operand}),
    }
}

/// ++x and --x: the variable steps, and the expression's value is the new one.
fn increment(state: &mut State) -> Result<Node> {
    let op = if state.take().text() == "++" { "+" } else { "-" };
    let target = unary(state)?;
    if !is_target(&target) {
        return state.fail("++ and -- need a variable");
    }
    writable(state, &target)?;
    Ok(stepped(target, op))
}

/// The decompiler's casts, (int)x and (float)x, which convert between the VM's ints and floats.
fn plain_cast(state: &mut State) -> Result<Node> {
    state.take();
    let kind = state.take();
    state.take();
    let operand = unary(state)?;
    Ok(node!({"kind": "cast", "type": kind.text(), "operand": operand}))
}

/// (enum flags)&x passes x's address where a native takes an enum, as it is.
fn enum_cast(state: &mut State) -> Result<Node> {
    state.take();
    state.take();
    let kind = format!("enum {}", state.take().text());
    state.take();
    let operand = unary(state)?;
    if operand.kind() != "addr" {
        return state.fail("a cast to an enum takes an address (&x)");
    }
    Ok(node!({"kind": "cast", "type": kind, "operand": operand}))
}

/// (int *)&x passes x's address as it is: C's view of a pointer of another type.
fn pointer_cast(state: &mut State) -> Result<Node> {
    state.take();
    let kind = state.take().text().to_string();
    state.take();
    state.take();
    let operand = unary(state)?;
    Ok(node!({"kind": "cast", "type": format!("{kind} *"), "operand": operand}))
}

/// sizeof(type) or sizeof expression.
#[inline(never)]
fn sizeof(state: &mut State) -> Result<Node> {
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
fn cast(state: &mut State) -> Result<Node> {
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
fn address(state: &mut State) -> Result<Node> {
    state.take();
    let target = unary(state)?;
    if !is_target(&target) {
        return state.fail("& needs a variable");
    }
    if target.kind() == "var"
        && state.variable(target.text("name")).is_some_and(|entry| entry.get("register").is_some())
    {
        return state.fail(format!("{} is a register variable, which has no address", target.text("name")));
    }
    Ok(node!({"kind": "addr", "target": target}))
}

fn binary(state: &mut State, minimum: u32) -> Result<Node> {
    state.chain(|state| {
        let mut left = unary(state)?;
        while state.token(0).kind == Kind::Op && precedence(state.token(0).text()).unwrap_or(0) >= minimum {
            state.link()?;
            let op = state.take();
            let right = binary(state, precedence(op.text()).unwrap() + 1)?;
            left = binary_node(op.text(), left, right);
        }
        Ok(left)
    })
}

fn binary_node(op: &str, left: Node, right: Node) -> Node {
    let kind = if op == "&&" || op == "||" { "logical" } else { "binary" };
    node!({"kind": kind, "op": op, "left": left, "right": right})
}

fn ternary(state: &mut State) -> Result<Node> {
    state.nested(|state| {
        let cond = binary(state, 2)?;
        if !state.at("?", 0) {
            return Ok(cond);
        }
        choice(state, cond)
    })
}

/// cond ? then : else, after cond.
fn choice(state: &mut State, cond: Node) -> Result<Node> {
    state.take();
    let then = assignment(state)?;
    state.expect(":")?;
    let otherwise = ternary(state)?;
    Ok(node!({"kind": "ternary", "cond": cond, "then": then, "else": otherwise}))
}

fn assignment(state: &mut State) -> Result<Node> {
    state.nested(|state| {
        let left = ternary(state)?;
        let token = state.token(0);
        if token.kind != Kind::Op || (token.text() != "=" && compound_op(token.text()).is_none()) {
            return Ok(left);
        }
        assigned(state, left)
    })
}

/// target = value or target op= value, after the target.
fn assigned(state: &mut State, target: Node) -> Result<Node> {
    if !is_target(&target) {
        return state.fail("cannot assign to this expression");
    }
    writable(state, &target)?;
    let op = state.take();
    let value = assignment(state)?;
    Ok(match compound_op(op.text()) {
        Some(op) => compound(target, op, value),
        None => node!({"kind": "assign", "target": target, "value": value}),
    })
}

/// An expression with C's comma operator: a, b works out a, then gives b.
fn expression(state: &mut State) -> Result<Node> {
    state.chain(|state| {
        let mut node = assignment(state)?;
        while state.at(",", 0) {
            state.link()?;
            state.take();
            let right = assignment(state)?;
            node = comma(node, right);
        }
        Ok(node)
    })
}

fn comma(left: Node, right: Node) -> Node {
    node!({"kind": "comma", "left": discarded(left), "right": right})
}

/// An expression whose value is not used: x++ is only its step.
fn discarded(node: Node) -> Node {
    if node["postfix"] == Node::Bool(true) {
        node["left"].clone()
    } else {
        node
    }
}

fn expression_statement(node: Node) -> Node {
    node!({"kind": "expr", "expr": discarded(node)})
}

/// The statements of an expression statement: one for each operand of a comma at its top.
fn expression_statements(mut node: Node) -> Vec<Node> {
    if node.kind() == "comma" {
        let mut found = expression_statements(std::mem::take(&mut node["left"]));
        found.extend(expression_statements(std::mem::take(&mut node["right"])));
        return found;
    }
    vec![expression_statement(node)]
}

// The statements, as the expressions, keep the functions on the parser's way down small: each
// form is read by a function of its own.

fn braced(state: &mut State) -> Result<Vec<Node>> {
    state.nested(|state| {
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
fn body(state: &mut State) -> Result<Vec<Node>> {
    if state.at("{", 0) {
        braced(state)
    } else {
        statement(state)
    }
}

fn conditional(state: &mut State) -> Result<Node> {
    state.nested(|state| {
        let line = state.expect("if")?.line;
        state.expect("(")?;
        let cond = expression(state)?;
        state.expect(")")?;
        let then = body(state)?;
        let otherwise = if state.at("else", 0) { otherwise(state)? } else { Node::Null };
        Ok(node!({"kind": "if", "cond": cond, "then": then, "else": otherwise, "line": line}))
    })
}

/// An if's else, after the else: another if, or a body.
fn otherwise(state: &mut State) -> Result<Node> {
    state.take();
    if state.at("if", 0) {
        return Ok(Node::List(vec![conditional(state)?]));
    }
    Ok(Node::List(body(state)?))
}

/// Puts extra statements in front of every continue of a loop body (not of loops inside it), so a
/// for loop's step and a do loop's test run when an iteration is cut short.
fn before_continue(nodes: &[Node], extra: &[Node]) -> Vec<Node> {
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
                    Node::List(before_continue(node.list("else"), extra))
                };
                node.clone().with("then", before_continue(node.list("then"), extra)).with("else", otherwise)
            }
            "switch" => {
                let cases: Vec<Node> = node
                    .list("cases")
                    .iter()
                    .map(|case| {
                        let body = before_continue(case.list("body"), extra);
                        case.clone().with("body", body)
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

fn expression_list(state: &mut State, closing: &str) -> Result<Vec<Node>> {
    let mut found = Vec::new();
    while !state.at(closing, 0) {
        let node = assignment(state)?;
        found.push(expression_statement(node));
        if !state.at(closing, 0) {
            state.expect(",")?;
        }
    }
    state.expect(closing)?;
    Ok(found)
}

fn while_loop(state: &mut State) -> Result<Vec<Node>> {
    state.nested(|state| {
        state.take();
        state.expect("(")?;
        let cond = expression(state)?;
        state.expect(")")?;
        let body = body(state)?;
        Ok(vec![node!({"kind": "while", "cond": cond, "body": body})])
    })
}

/// for (init; cond; step) body becomes init; while (cond) { body step }, with the step also run
/// before each continue.
fn for_loop(state: &mut State) -> Result<Vec<Node>> {
    state.nested(|state| {
        let line = state.expect("for")?.line;
        state.expect("(")?;
        // a declaration in the first part belongs to the loop (as C99 has it)
        state.open_block();
        let init = if declaration_ahead(state) { declaration(state)? } else { expression_list(state, ";")? };
        let cond = if state.at(";", 0) { one() } else { expression(state)? };
        state.expect(";")?;
        let step = expression_list(state, ")")?;
        let body = body(state)?;
        state.close_block();
        Ok(for_nodes(init, cond, step, &body, line))
    })
}

fn for_nodes(mut init: Vec<Node>, cond: Node, step: Vec<Node>, body: &[Node], line: usize) -> Vec<Node> {
    let mut body = before_continue(body, &step);
    body.extend(step);
    init.push(node!({"kind": "while", "cond": cond, "body": body, "line": line}));
    init
}

/// do body while (cond); becomes while (1) { body if (!cond) break; }, with the test also run before
/// each continue.
fn do_loop(state: &mut State) -> Result<Vec<Node>> {
    state.nested(|state| {
        let line = state.expect("do")?.line;
        let body = body(state)?;
        state.expect("while")?;
        state.expect("(")?;
        let cond = expression(state)?;
        state.expect(")")?;
        state.expect(";")?;
        Ok(do_nodes(&body, cond, line))
    })
}

fn do_nodes(body: &[Node], cond: Node, line: usize) -> Vec<Node> {
    let leave = node!({
        "kind": "if",
        "cond": {"kind": "unary", "op": "!", "operand": cond},
        "then": [{"kind": "break"}],
        "else": null,
    });
    let mut body = before_continue(body, std::slice::from_ref(&leave));
    body.push(leave);
    vec![node!({"kind": "while", "cond": one(), "body": body, "line": line})]
}

fn switch(state: &mut State) -> Result<Node> {
    state.nested(|state| {
        let line = state.expect("switch")?.line;
        state.expect("(")?;
        let value = expression(state)?;
        state.expect(")")?;
        state.expect("{")?;
        let mut cases: Vec<Node> = Vec::new();
        // the case values known here, which C allows once each
        let mut numbers: Vec<i64> = Vec::new();
        while !state.at("}", 0) {
            if state.at("case", 0) || state.at("default", 0) {
                let case = case_label(state, &cases, &mut numbers)?;
                cases.push(case);
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

/// case value: or default:, as a case of the switch with no statements yet.
fn case_label(state: &mut State, cases: &[Node], numbers: &mut Vec<i64>) -> Result<Node> {
    if state.take().text() == "default" {
        state.expect(":")?;
        if cases.iter().any(|case| case.flag("default")) {
            return state.fail("a switch has one default");
        }
        return Ok(node!({"value": null, "default": true, "body": []}));
    }
    let value = ternary(state)?;
    let found = |name: &str| state.constant(name).map(|value| value as i64);
    if let Some((number, _)) = folded(&value, &found) {
        if numbers.contains(&number) {
            return state.fail(format!("case {number} is in the switch twice"));
        }
        numbers.push(number);
    }
    state.expect(":")?;
    Ok(node!({"value": value, "default": false, "body": []}))
}

fn statement(state: &mut State) -> Result<Vec<Node>> {
    state.nested(|state| {
        let line = state.line();
        // a "// note: ..." comment right above a statement is a line note the definitions keep
        let doc = state.token(0).doc.clone();
        let nodes = statement_nodes(state)?;
        Ok(placed(nodes, line, doc.as_deref()))
    })
}

/// A statement's nodes with its line, where they have none, and the first with its note: the last
/// paragraph of the comment above it that starts "note:".
fn placed(nodes: Vec<Node>, line: usize, doc: Option<&str>) -> Vec<Node> {
    let mut nodes: Vec<Node> =
        nodes.into_iter().map(|node| if node.get("line").is_some() { node } else { node.with("line", line) }).collect();
    let note = doc
        .unwrap_or_default()
        .split('\n')
        .rfind(|paragraph| paragraph.get(..NOTE.len()).is_some_and(|start| start.eq_ignore_ascii_case(NOTE)));
    if let (Some(first), Some(note)) = (nodes.first_mut(), note) {
        *first = std::mem::take(first).with("note", strip(&note[NOTE.len()..]));
    }
    nodes
}

fn statement_nodes(state: &mut State) -> Result<Vec<Node>> {
    let token = state.token(0);
    let word = if token.kind == Kind::Name { token.text().to_string() } else { String::new() };
    if word == "AT" || word == "DIRECT" {
        return placed_at(state);
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
        return label(state, word);
    }
    match word.as_str() {
        "if" => Ok(vec![conditional(state)?]),
        "while" => while_loop(state),
        "for" => for_loop(state),
        "do" => do_loop(state),
        "switch" => Ok(vec![switch(state)?]),
        "goto" => goto(state),
        "return" => return_statement(state),
        "print" => print(state),
        word if SIMPLE.contains(&word) => {
            state.take();
            state.expect(";")?;
            Ok(vec![node!({"kind": word})])
        }
        _ => {
            let node = expression(state)?;
            state.expect(";")?;
            Ok(expression_statements(node))
        }
    }
}

/// AT(base) statement or DIRECT statement: the native call the statement makes, at a base or
/// direct (sdk::resolve).
fn placed_at(state: &mut State) -> Result<Vec<Node>> {
    let at = if state.take().text() == "DIRECT" {
        Node::from("direct")
    } else {
        state.expect("(")?;
        let base = int_node(int_value(state.expect_kind(Kind::Int)?.text())?)?;
        state.expect(")")?;
        base
    };
    Ok(statement(state)?.into_iter().map(|node| node.with("at", at.clone())).collect())
}

/// A label: the statement after it follows it in the list.
fn label(state: &mut State, word: String) -> Result<Vec<Node>> {
    state.take();
    state.take();
    let Some(scope) = state.scope.as_mut() else {
        return state.fail("labels belong in functions");
    };
    if scope.labels.contains(&word) {
        return state.fail(format!("label {word} is defined twice"));
    }
    scope.labels.push(word.clone());
    let mut found = vec![node!({"kind": "label", "name": word})];
    if !state.at("}", 0) {
        found.extend(statement(state)?);
    }
    Ok(found)
}

fn goto(state: &mut State) -> Result<Vec<Node>> {
    let line = state.take().line;
    let label = state.name()?;
    state.expect(";")?;
    let Some(scope) = state.scope.as_mut() else {
        return state.fail("goto belongs in functions");
    };
    scope.gotos.push((label.clone(), line));
    Ok(vec![node!({"kind": "goto", "name": label})])
}

fn return_statement(state: &mut State) -> Result<Vec<Node>> {
    state.take();
    if state.at(";", 0) {
        state.take();
        return Ok(vec![node!({"kind": "return", "value": null})]);
    }
    let value = expression(state)?;
    state.expect(";")?;
    Ok(vec![node!({"kind": "return", "value": value})])
}

fn print(state: &mut State) -> Result<Vec<Node>> {
    state.take();
    let args = arguments(state)?;
    state.expect(";")?;
    Ok(vec![node!({"kind": "print", "args": args})])
}

fn type_name(state: &mut State, allowed: &[&str]) -> Result<String> {
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
    fn node(&self) -> Result<Node> {
        match self {
            Size::None => Ok(Node::Null),
            Size::Number(value) => int_node(*value),
            Size::Named(name) => Ok(Node::from(name)),
        }
    }
}

/// An array size: a number, or a constant's name. A name this file does not define is kept and
/// resolved with the constants of the headers and libraries the script includes (sdk::resolve).
/// Any other integer constant expression is worked out here.
fn array_size(state: &mut State) -> Result<Size> {
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

fn initializer(state: &mut State) -> Result<Init> {
    state.nested(|state| {
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
fn pointer_part(state: &mut State) -> Result<(bool, bool)> {
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

fn declarators(state: &mut State, spec: &Spec) -> Result<Vec<Declared>> {
    let mut found = Vec::new();
    loop {
        let line = state.line();
        if complex_ahead(state, spec) {
            let base = base_ty(state, spec)?;
            let (name, ty) = declarator(state, base)?;
            if name.is_empty() {
                let found = state.shown(state.token(0))?;
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
        if dims.first() == Some(&Size::Number(-1)) && init.is_none() && spec.storage.as_deref() != Some("extern") {
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
}

fn declaration_ahead(state: &State) -> bool {
    let token = state.token(0);
    if token.kind != Kind::Name {
        return false;
    }
    if token.text() == "enum" {
        let defined = |offset: usize| state.token(offset).kind == Kind::Op && state.token(offset).text() == "{";
        return defined(1) || (state.token(1).kind == Kind::Name && (state.token(2).kind == Kind::Name || defined(2)));
    }
    let following = state.token(1);
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
    // a value kind or typedef from a header (bgm_file track;), which sdk::resolve reads as its base type
    following.kind == Kind::Name
        && state.token(2).text.as_deref().is_some_and(|text| [";", "=", "[", ","].contains(&text))
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
fn string_elements(state: &State, text: &str, kind: &str) -> Result<Vec<Node>> {
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
) -> Result<i64> {
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
) -> Result<i64> {
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
fn array_values(state: &State, declared: &Declared, kind: &str) -> Result<(Vec<i64>, Elements)> {
    let mut dims = Vec::new();
    for (at, size) in declared.dims.iter().enumerate() {
        match size {
            Size::Number(value) => dims.push(*value as i64),
            Size::Named(name) if state.enumerators.contains_key(name) => dims.push(state.enumerators[name] as i64),
            Size::Named(_) if declared.dims.len() == 1 && declared.init.is_none() => {
                // resolved with the headers' constants later
                return Ok((Vec::new(), Vec::new()));
            }
            Size::Named(name) if state.constant(name).is_some() => dims.push(state.constant(name).unwrap() as i64),
            Size::Named(name) => {
                return state.fail(format!(
                    "{}: size {} of {} must be a constant this file defines or includes",
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
fn declaration(state: &mut State) -> Result<Vec<Node>> {
    if state.scope.is_none() {
        return state.fail("declarations belong inside functions");
    }
    let spec = specifiers(state, true)?;
    if spec.storage.as_deref() == Some("typedef") {
        return local_typedef(state, &spec);
    }
    if state.at(";", 0) && (matches!(spec.ty, Some(Ty::Struct(_))) || spec.kind.starts_with("enum ")) {
        // struct T { ... }; or struct T;: declares the tag, in this block
        state.take();
        return Ok(Vec::new());
    }
    // a typedef of this function's stands for its type
    let kind =
        state.kinds.get(&spec.kind).filter(|_| local_kind(state, &spec.kind)).cloned().unwrap_or(spec.kind.clone());
    if kind == "any" || (kind == "void" && !complex_ahead(state, &spec)) {
        return state.fail(format!("unknown type {kind}"));
    }
    let mut statements = Vec::new();
    for declared in declarators(state, &spec)? {
        if spec.storage.as_deref() == Some("extern") {
            // names a global this file declares
            continue;
        }
        let static_aggregate =
            spec.storage.as_deref() == Some("static") && (declared.pointer || !declared.dims.is_empty());
        let wide = matches!(&declared.init, Some(Init::Value(value)) if value.kind() == "wide");
        if declared.ty.is_some() || static_aggregate || wide {
            statements.extend(typed_local(state, &spec, declared, &kind)?);
            continue;
        }
        let line = declared.line;
        let (dims, values) =
            if declared.dims.is_empty() { (Vec::new(), Vec::new()) } else { array_values(state, &declared, &kind)? };
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
        let assign = |target: Node, value: Node| node!({"kind": "expr", "expr": {"kind": "assign", "target": target, "value": value}, "line": line});
        match declared.init {
            Some(Init::Value(value)) if declared.dims.is_empty() => {
                statements.push(assign(node!({"kind": "var", "name": &storage}), value));
            }
            Some(Init::List(items)) if declared.dims.is_empty() => match items.as_slice() {
                // int x = {5};
                [Init::Value(value)] => {
                    statements.push(assign(node!({"kind": "var", "name": &storage}), value.clone()))
                }
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
}

/// A declaration's type when its declarator has the plain fields only (a static or global array
/// or pointer, an array a wide string fills).
fn plain_type(state: &State, declared: &Declared, kind: &str) -> Result<Ty> {
    let kind = state.base_kind(kind);
    let scalar = if kind.starts_with("enum ") { ctype::int() } else { Ty::Scalar(kind) };
    if declared.pointer {
        return Ok(ctype::pointer(scalar));
    }
    let mut ty = scalar;
    for size in declared.dims.iter().rev() {
        let size = match size {
            Size::Number(size) => *size as i64,
            Size::Named(name) => match state.constant(name) {
                Some(size) => size as i64,
                None => {
                    return state.fail(format!(
                        "{}: size {name} must be a constant this file defines or includes",
                        declared.name
                    ))
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
fn sized_by(state: &State, ty: Ty, init: &Option<Init>) -> Result<Ty> {
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
            stb::encode_string(node.text("text")).or_else(|error| state.fail(error))?.len() as i64 + 1
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
) -> Result<i64> {
    let structs = &state.structs;
    let one = |item: &Init| -> Result<Node> {
        match item {
            Init::Value(node) => Ok(node.clone()),
            Init::List(inner) => match inner.as_slice() {
                [Init::Value(node)] => Ok(node.clone()),
                _ => state.fail("a scalar takes one value"),
            },
        }
    };
    // what one element (or member) of ty is, where it starts, and its bits for a bit-field
    let parts: Vec<(Ty, i64, Option<ctype::Bits>)> = match ty {
        Ty::Array(of, size) => {
            let step = of.slots(structs);
            // an open size takes at most an element per value
            let count = if *size < 0 { items.len() as i64 } else { *size };
            (0..count).map(|index| ((**of).clone(), base + index * step, None)).collect()
        }
        Ty::Struct(tag) => {
            let Some(found) = structs.get(tag) else {
                return state.fail(format!("{} is not defined", ty.shown()));
            };
            let mut offset = base;
            let mut parts = Vec::new();
            for (at, (name, member)) in found.members.iter().enumerate() {
                let bits = found.bits_of(at);
                // a bit-field that shares a slot is in the one the last started
                let start = if bits.is_some_and(|bits| bits.offset > 0) { offset - 1 } else { offset };
                if !name.is_empty() {
                    // an unnamed bit-field takes no value
                    parts.push((member.clone(), start, bits));
                    if found.union {
                        // a union's initializer sets its first member
                        break;
                    }
                }
                offset += found.slots_of(at, structs);
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
    for (part, offset, bits) in parts {
        if *at >= items.len() {
            break;
        }
        if let Some(bits) = bits {
            // the fields that share a slot are put together in its one value
            let packed = bits.packed(one(&items[*at])?);
            let value = match out.remove(&offset) {
                Some(known) => ctype::Bits::joined(known, packed),
                None => packed,
            };
            out.insert(offset, value);
            *at += 1;
            filled += 1;
            continue;
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
                    string_elements(state, node.text("text"), &of.slot_kind())?
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
fn initial_values(state: &State, name: &str, ty: &Ty, init: &Init, every: bool) -> Result<Vec<(i64, Ty, Node)>> {
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
fn typed_local(state: &mut State, spec: &Spec, declared: Declared, kind: &str) -> Result<Vec<Node>> {
    let ty = match &declared.ty {
        Some(ty) => ty.clone(),
        None => plain_type(state, &declared, kind)?,
    };
    let ty = sized_by(state, ty, &declared.init)?;
    complete(state, &declared.name, &ty)?;
    let is_static = spec.storage.as_deref() == Some("static");
    let entry = typed_entry(state, "", &ty);
    let storage = storage_for(state, &declared.name, entry.text("type"), &entry["size"], &entry["ctype"], is_static)?;
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
fn typed_global(state: &mut State, program: &mut Node, spec: &Spec, declared: Declared) -> Result<()> {
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
fn global_values(state: &State, mut entry: Node, declared: &Declared, ty: &Ty) -> Result<Node> {
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
/// (a file's typedefs reach sdk::resolve through the program's kinds).
fn local_kind(state: &State, name: &str) -> bool {
    state.scope.as_ref().is_some_and(|scope| scope.typedefs.iter().any(|known| known == name))
}

/// typedef of a type beyond the scalars (a struct, a pointer, an array, a function): the name
/// stands for the type from here on. false, reading nothing, for a scalar's typedef.
fn typed_typedef(state: &mut State, spec: &Spec) -> Result<bool> {
    let plain = spec.ty.is_none() && state.token(0).kind == Kind::Name && state.at(";", 1);
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
fn local_typedef(state: &mut State, spec: &Spec) -> Result<Vec<Node>> {
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
fn base_type(state: &State, kind: &str) -> Result<String> {
    let base =
        state.kinds.get(kind).or_else(|| state.header_kinds.get(kind)).cloned().unwrap_or_else(|| kind.to_string());
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
) -> Result<String> {
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
fn static_local(state: &mut State, declared: &Declared, hidden: &str, kind: &str, spec: &Spec) -> Result<()> {
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

/// A function's parameter list.
struct Parameters {
    params: Vec<Node>,
    /// A K&R identifier list (int f(a, b) int a; ...), whose types the declarations after the )
    /// give.
    identifiers: bool,
    /// ... after the named parameters.
    variadic: bool,
}

/// A function's parameters. A prototype's parameters may go unnamed.
fn parameters(state: &mut State) -> Result<Parameters> {
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
        return Ok(Parameters { params, identifiers: true, variadic: false });
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
            let name = if state.token(0).kind == Kind::Name { state.name()? } else { String::new() };
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
                        Size::Named(name) if state.constant(&name).is_some() => {
                            inner.push(state.constant(&name).unwrap() as i64)
                        }
                        _ => {
                            return state.fail(
                                "a parameter's inner array sizes must be constants this file defines or includes",
                            )
                        }
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
    Ok(Parameters { params, identifiers: false, variadic })
}

/// A parameter of C's other types: a pointer to a pointer or a struct, a struct (passed by value,
/// composite.rs), a function pointer; an array or function declared as one is its pointer.
fn typed_parameter_entry(state: &mut State, spec: &Spec) -> Result<Node> {
    let base = base_ty(state, spec)?;
    let (name, ty) = declarator(state, base)?;
    let ty = adjusted(ty);
    if let Ty::Struct(_) = &ty {
        complete(state, &name, &ty)?;
    }
    let entry = typed_entry(state, &name, &ty);
    let pointer = entry.flag("pointer");
    Ok(node!({"name": name, "type": entry["type"].clone(), "array": pointer, "ctype": entry["ctype"].clone()}))
}

/// The declarations of a K&R definition's parameters, between its ) and its {.
fn identifier_types(state: &mut State, params: &mut [Node]) -> Result<()> {
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
) -> Result<Option<Node>> {
    if name.starts_with('_') {
        state.pos -= 1;
        return state.fail("function names cannot start with _, which marks natives");
    }
    let Parameters { mut params, identifiers, variadic } = parameters(state)?;
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
fn top_declaration(state: &mut State, program: &mut Node, pending: &Pending, doc: Option<&str>) -> Result<bool> {
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
    if state.token(stars).kind == Kind::Name && state.at("(", stars + 1) {
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
        let function = function_definition(state, pending, &returns, &name, doc, returns_ty.clone())?;
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
            let open = declared.size() == &Size::Number(-1) || matches!(declared.ty, Some(Ty::Array(_, -1)));
            if spec.storage.as_deref() == Some("extern") && (open || state.globals.contains_key(&declared.name)) {
                // extern int table[];: declares a global another declaration defines
                continue;
            }
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

fn typed_parameter(state: &mut State) -> Result<Node> {
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
    } else if state.token(0).kind == Kind::Name && state.kinds.contains_key(state.token(0).text()) {
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

fn native_declaration(state: &mut State) -> Result<Node> {
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

fn inline_definition(state: &mut State) -> Result<Node> {
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
/// numbers, the integer #defines (this file's and its includes') and the enumerators before it.
fn enum_value(state: &mut State) -> Result<i128> {
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
fn enumeration(state: &mut State, program: &mut Node) -> Result<()> {
    let enumeration = if state.at("{", 0) { None } else { Some(state.name()?) };
    let members = enumerators(state, enumeration.as_deref())?;
    let key = if enumeration.as_deref() == Some("program") { "programs" } else { "constants" };
    declarations(program, key).extend(members);
    state.expect(";")?;
    Ok(())
}

/// An enum's { ... }: its members as constants (as program names for enum program).
fn enumerators(state: &mut State, enumeration: Option<&str>) -> Result<Vec<Node>> {
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
/// (-2147483647 - 1)). Any other value, one that does not parse as such an expression included,
/// is text the tokenizer puts where the name is used (and fails there, if it fails).
fn constant_of(value: &str) -> Result<Option<Node>> {
    let chars: Vec<char> = value.chars().collect();
    let mut inner = State::new(tokenize(&chars, Macros::default(), "")?);
    let Ok(mut value) = unary(&mut inner) else {
        return Ok(None);
    };
    if inner.token(0).kind != Kind::Eof {
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
fn definition(state: &mut State, token: &Token, program: &mut Node) -> Result<()> {
    let [name, value] = token.words.as_slice() else {
        return Ok(());
    };
    if let Some(value) = constant_of(value)? {
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
        "var" => names(node.text("name")).map(|value| (wrap(value), false)),
        "cast" if matches!(node.text("type"), "int" | "long") => folded(&node["operand"], names),
        "unary" => {
            let (value, unsigned) = folded(&node["operand"], names)?;
            Some(match node.text("op") {
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
            let op = node.text("op");
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

fn program_entry(state: &mut State, program: &mut Node) -> Result<()> {
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

fn asm_function(state: &mut State, token: &Token) -> Result<Node> {
    let words = &token.words;
    if words.len() != 3 || words[1] != "arg" {
        state.pos -= 1;
        return state.fail("asm block header must be: asm NAME arg COUNT {");
    }
    let arg = int_node(int_in_base(&words[2], 10).or_else(|error| lex_fail(token.line, error))?)?;
    Ok(node!({"kind": "asm", "name": &words[0], "arg": arg, "reason": "", "lines": token.lines.clone()}))
}

/// #pragma magic's four bytes, as the assembly's .header writes them (asm::magic_bytes): 0x and
/// hexadecimal digits, or ASCII text and a 0 byte; as a text of those bytes, a character each.
fn magic(token: &str, line: usize) -> Result<Node> {
    let bytes = if let Some(digits) = token.strip_prefix("0x") {
        match from_hex(digits) {
            Some(bytes) => bytes,
            None => {
                return lex_fail(line, format!("#pragma magic 0x{digits}: the digits after 0x are not hexadecimal"))
            }
        }
    } else if !token.is_ascii() {
        return lex_fail(line, format!("#pragma magic {token}: the magic is ASCII text, or 0x and hexadecimal digits"));
    } else {
        let mut bytes = token.as_bytes().to_vec();
        bytes.push(0);
        bytes
    };
    Ok(Node::from(bytes.iter().map(|&byte| byte as char).collect::<String>()))
}

/// Pairs of hexadecimal digits, with whitespace allowed between pairs; None when they are not.
fn from_hex(text: &str) -> Option<Vec<u8>> {
    let chars: Vec<char> = text.chars().collect();
    let digit = |i: usize| chars.get(i).and_then(|c| c.to_digit(16));
    let space = |c: char| matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c');
    let mut bytes = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if space(chars[i]) {
            while i < chars.len() && space(chars[i]) {
                i += 1;
            }
            if i >= chars.len() {
                break;
            }
        }
        bytes.push((digit(i)? * 16 + digit(i + 1)?) as u8);
        i += 2;
    }
    Some(bytes)
}

fn pragma(state: &mut State, token: &Token, program: &mut Node, pending: &mut Pending) -> Result<()> {
    let words = &token.pragma;
    if words.is_empty() {
        // #pragma alone, which C ignores
        return Ok(());
    }
    let line = token.line;
    let refused = |what: &str| Error::invalid(format!("line {line}: #pragma {what}"));
    let rest = &words[1..];
    let first = || rest.first().ok_or_else(|| refused("is missing its value"));
    let word = |node: &Node| node.as_str().map(String::from).ok_or_else(|| refused("takes a word, not a string"));
    let integer = |node: &Node| -> Result<Node> {
        int_node(int_in_base(&word(node)?, 0).or_else(|error| lex_fail(line, error))?)
    };
    let Some(key) = words[0].as_str() else {
        return Err(refused("name is a string"));
    };
    match key {
        "natives" => program["natives"] = first()?.clone(),
        "magic" => program["magic"] = magic(&word(first()?)?, line)?,
        "unk_14" => program["unk_14"] = integer(first()?)?,
        "header_padding" => program["header_padding"] = first()?.clone(),
        "globals" => program["global_num"] = integer(first()?)?,
        "strings" => {
            let strings =
                rest.iter().map(|word| word.get("string").cloned().ok_or_else(|| refused("strings are quoted")));
            pending.strings = Some(strings.collect::<Result<Vec<Node>>>()?);
        }
        "padding" => pending.padding = Some(rest.first().cloned().unwrap_or_else(|| Node::from(""))),
        "padding_of" => {
            let name = word(first()?)?;
            program["padding_of"][name.as_str()] = rest.get(1).cloned().unwrap_or_else(|| Node::from(""));
        }
        "string_pool" => program["string_pool"] = Node::Bool(true),
        "target" => {
            // the scripts a mod patch changes: #pragma target map s51 (see patches.rs)
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
    parse_program(text, defines, UNNAMED_FILE, None)
}

/// The program a file's source holds, with its name for __FILE__.
pub fn parse_file(text: &str, file: &str) -> Result<Node> {
    parse_program(text, &HashMap::new(), file, None)
}

/// A source file as a compiler reads it: its #include <...> headers (from those given) read as text
/// for their macros, so a header's #defines reach the file's #if as C has it.
pub fn parse_source(text: &str, file: &str, headers: &IndexMap<String, String>) -> Result<Node> {
    parse_program(text, &HashMap::new(), file, Some(Files { headers, libraries: None }))
}

/// parse_source with #defines in force before the first line (a mod patch's LANGUAGE), and the
/// libraries #include "..." reads (library::include_texts), whose macros, typedefs and structs
/// reach the file as C includes them.
pub fn parse_source_with(
    text: &str,
    file: &str,
    defines: &HashMap<String, String>,
    headers: &IndexMap<String, String>,
    libraries: &IndexMap<String, String>,
) -> Result<Node> {
    parse_program(text, defines, file, Some(Files { headers, libraries: Some(libraries) }))
}

fn parse_program(text: &str, defines: &HashMap<String, String>, file: &str, headers: Headers) -> Result<Node> {
    Ok(parse_state(text, defines, file, headers)?.0)
}

/// What an included file declares that the file including it needs while it is parsed: typedefs
/// of scalars (for sizes), of other types, structs and unions, and enumerators (for constant
/// expressions), its own and those of the files it includes. Its #defines reach the including file
/// through the tokens (Token::constants), read with the macros in force.
#[derive(Clone, Debug, Default)]
struct Included {
    kinds: HashMap<String, String>,
    types: HashMap<String, Ty>,
    structs: Structs,
    enumerators: HashMap<String, i128>,
}

thread_local! {
    /// What a header or library (and what it includes) declares, by its key and text.
    static INCLUDED: RefCell<HashMap<u64, Included>> = RefCell::new(HashMap::new());
}

/// A file's own part of its unnamed structs' tags: 8 hexadecimal digits of a hash of its text that
/// stays the same from run to run (FNV-1a), as the program's bytes must. Two files with the same
/// text give the same tags to the same structs, which is harmless.
fn file_key(text: &str) -> String {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in text.bytes() {
        hash = (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193);
    }
    format!("{hash:08x}")
}

/// What an included file declares, for the file including it.
fn included(path: &str, headers: Files) -> Result<Included> {
    let Some(text) = headers.get(path).filter(|_| path != crate::sdk::PRELUDE) else {
        return Ok(Included::default());
    };
    let key = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (path, text).hash(&mut hasher);
        hasher.finish()
    };
    if let Some(found) = INCLUDED.with(|cache| cache.borrow().get(&key).cloned()) {
        return Ok(found);
    }
    let (_, found) = parse_state(text, &HashMap::new(), path, Some(headers))
        .map_err(|fault| Error::invalid(format!("in {path}: {}", fault)))?;
    INCLUDED.with(|cache| cache.borrow_mut().insert(key, found.clone()));
    Ok(found)
}

/// A file's program, and what it declares that a file including it needs.
fn parse_state(
    text: &str,
    defines: &HashMap<String, String>,
    file: &str,
    headers: Headers,
) -> Result<(Node, Included)> {
    let chars: Vec<char> = text.chars().collect();
    let mut macros = Macros::default();
    for (name, value) in defines {
        macros.define(name, cpp::Macro { params: None, body: value.clone() }, true);
        if let Ok(value) = value.trim().parse::<i128>() {
            macros.values.insert(name.clone(), value);
        }
    }
    let predefined = macros.values.clone();
    let tokens = tokenize_with(&chars, &mut macros, file, headers, 0)?;
    let mut state = State::new(tokens);
    // the constants in force before the first line (LANGUAGE), as a header's are
    state.included_constants = predefined;
    state.file_key = file_key(text);
    if state.tokens[0].kind == Kind::Assembly {
        return Ok((node!({"kind": "program", "asm": state.tokens[0].text()}), Included::default()));
    }
    let mut program = crate::node::program_skeleton();
    let doc = file_doc(text);
    if !doc.is_empty() {
        program["doc"] = Node::from(doc);
    }
    let mut pending = Pending::default();
    while state.token(0).kind != Kind::Eof {
        let token = state.take();
        let word = if token.kind == Kind::Name { token.text() } else { "" };
        match token.kind {
            Kind::Pragma => pragma(&mut state, &token, &mut program, &mut pending)?,
            Kind::Asm => {
                let function = asm_function(&mut state, &token)?;
                program["functions"].as_array_mut().unwrap().push(function);
            }
            Kind::Include => {
                if let Some(files) = headers {
                    // what the file declares that this one's declarations need (a library's
                    // structs, a header's typedefs), as C includes it
                    let found = included(&included_key(&token.path, token.system), files)?;
                    state.header_kinds.extend(found.kinds);
                    state.types.extend(found.types);
                    state.included_enumerators.extend(found.enumerators);
                    for (tag, composite) in found.structs {
                        if state.structs.get(&tag).is_none_or(|known| known.members.is_empty()) {
                            state.structs.insert(tag, composite);
                        }
                    }
                }
                // the integer #defines the file leaves, as C includes it: they replace this
                // file's own of the same names
                for (name, value) in &token.constants {
                    state.constants.remove(name);
                    state.included_constants.insert(name.clone(), *value);
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
                // NEW_FLAG(name): a number build gives the mod (numbers.rs)
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
                let inline = inline_definition(&mut state)?;
                declarations(&mut program, "inlines").push(inline);
            }
            _ if token.kind == Kind::Name && (type_word(&state, word) || header_type(&state, word, state.token(0))) => {
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
    let mut enumerators = std::mem::take(&mut state.included_enumerators);
    enumerators.extend(std::mem::take(&mut state.enumerators));
    let included = Included {
        kinds,
        types: std::mem::take(&mut state.types),
        structs: std::mem::take(&mut state.structs),
        enumerators,
    };
    Ok((with_own_padding(program), included))
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
        // starts on it
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
    fn errors_name_the_line_and_what_is_wrong() {
        let cases = [
            ("void f() { x = 1 }", "line 1: expected ';', found '}'"),
            (r#"void f() { s = "\q"; }"#, r"line 1: unknown escape \q"),
            (r#"void f() { s = "\xzz"; }"#, r"line 1: \x takes two hexadecimal digits, not 'zz'"),
            ("void f() { x = 08; }", "line 1: '8' is not an octal number"),
            ("#if 1\nint a;\n", "line 1: #if without #endif"),
            ("/* open", "line 1: unterminated comment"),
            ("#if (1\n#endif\n", "line 1: cannot read the condition '(1': ( without )"),
            (
                "#pragma magic S\u{e9}\n",
                "line 1: #pragma magic S\u{e9}: the magic is ASCII text, or 0x and hexadecimal digits",
            ),
            ("#pragma magic 0x5342zz\n", "line 1: #pragma magic 0x5342zz: the digits after 0x are not hexadecimal"),
            ("#pragma globals x\n", "line 1: 'x' is not a number"),
            ("#pragma target map \"s51\n", r#"line 1: a string in "target map \"s51" is not ended"#),
            ("void f() { x = \u{3042}; }", "line 1: unexpected character '\u{3042}'"),
        ];
        for (text, message) in cases {
            assert_eq!(error(text), message, "{text:?}");
        }
    }

    #[test]
    fn nesting_deeper_than_the_limit_is_an_error_not_a_crash() {
        // on a thread with a 1 MB stack, as Windows gives a main thread, in a debug build too
        let thread = std::thread::Builder::new().stack_size(1 << 20);
        thread
            .spawn(|| {
                // the deepest each form nests within NESTING, and one deeper
                let deepest = |form: &dyn Fn(usize) -> String, count: usize| {
                    assert!(parse(&form(count)).is_ok(), "{}", form(count));
                    assert_eq!(error(&form(count + 1)), "line 1: nested too deeply");
                };
                let r = |text: &str, count: usize| text.repeat(count);
                deepest(&|count| format!("void f() {{ x = {}1; }}", r("!", count)), 194);
                deepest(&|count| format!("void f() {{ x = 1{}; }}", r(" + 1", count)), 194);
                deepest(&|count| format!("void f() {{ x = {}1{}; }}", r("(", count), r(")", count)), 64);
                deepest(&|count| format!("void f() {{ x = {}1{}; }}", r("f(", count), r(")", count)), 64);
                deepest(&|count| format!("void f() {{ {}x = 1;{} }}", r("{ ", count), r(" }", count)), 97);
                deepest(&|count| format!("void f() {{ {}x = 1; }}", r("if (x) ", count)), 97);
                deepest(&|count| format!("void f() {{ {}x = 1; }}", r("while (x) ", count)), 97);
                deepest(
                    &|count| format!("void f() {{ {}x = 1;{} }}", r("switch (x) { case 0: ", count), r(" }", count)),
                    97,
                );
                deepest(&|count| format!("int {}x{};", r("(", count), r(")", count)), 199);
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
