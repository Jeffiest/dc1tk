//! The C preprocessor's parts beyond what the tokenizer does inline (parse/lex.rs): macros, expanded
//! as C expands them, and #if conditions, worked out as C works them out.
//!
//! A #define whose value is a constant (a literal, a negative one, or a parenthesised integer
//! expression) stays a named constant that the compiler resolves (sdk::resolve), so decompiled
//! scripts keep their constants' names and compile to their own bytes. Every other macro is text:
//! function-like macros (with # and ##) and object-like macros with any other value are replaced
//! where they are used, their arguments expanded first, and the result read again with the macro
//! itself turned off, as C 3.8.3 says. A macro works in the file that defines it, from its
//! #define on.
//!
//! #if conditions are integer constant expressions of C's long and unsigned long, which are 32
//! bits here, with every operator C allows, character constants, defined(NAME), and macros
//! expanded; any name left over is 0.

use std::collections::HashMap;

/// Whether a character may be in a name.
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// A macro: its parameters (None for an object-like macro) and its replacement text.
#[derive(Clone, Debug, PartialEq)]
pub struct Macro {
    pub params: Option<Vec<String>>,
    pub body: String,
}

/// The macros in force: every #define, and which of them stay named constants in code.
#[derive(Clone, Debug, Default)]
pub struct Macros {
    pub table: HashMap<String, Macro>,
    /// Object-like macros the compiler resolves as constants, which code does not expand.
    pub constants: std::collections::HashSet<String>,
    /// The integer constants' values, for the constant expressions (array sizes, case labels,
    /// enumerators) of a file that includes the header defining them.
    pub values: HashMap<String, i128>,
}

impl Macros {
    pub fn define(&mut self, name: &str, definition: Macro, constant: bool) {
        if constant {
            self.constants.insert(name.to_string());
        } else {
            self.constants.remove(name);
        }
        self.values.remove(name);
        self.table.insert(name.to_string(), definition);
    }

    pub fn undefine(&mut self, name: &str) {
        self.table.remove(name);
        self.constants.remove(name);
        self.values.remove(name);
    }

    pub fn defined(&self, name: &str) -> bool {
        self.table.contains_key(name) || PREDEFINED.contains(&name)
    }
}

/// The macros C defines itself (__STDC_VERSION__ is not one: it came with C95).
pub const PREDEFINED: [&str; 5] = ["__FILE__", "__LINE__", "__DATE__", "__TIME__", "__STDC__"];

fn is_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

/// The end of the identifier at position.
pub fn word_end(text: &[char], position: usize) -> usize {
    let mut end = position;
    while end < text.len() && (text[end].is_ascii_alphanumeric() || text[end] == '_') {
        end += 1;
    }
    end
}

/// The end of a string or character literal that opens at position (its quote).
fn literal_end(text: &[char], position: usize) -> usize {
    let quote = text[position];
    let mut at = position + 1;
    while at < text.len() && text[at] != quote && text[at] != '\n' {
        if text[at] == '\\' {
            at += 1;
        }
        at += 1;
    }
    (at + 1).min(text.len())
}

/// The end of a preprocessing number (a digit, then letters, digits, _, . and e+ e-).
fn number_end(text: &[char], position: usize) -> usize {
    let mut at = position + 1;
    while at < text.len() {
        let c = text[at];
        let exponent =
            matches!(c, '+' | '-') && matches!(text[at - 1], 'e' | 'E') && !text[position..at].contains(&'x');
        if !(exponent || is_word(c) || c == '.') {
            break;
        }
        at += 1;
    }
    at
}

/// The character a trigraph (??= and its kin) stands for.
fn trigraph(c: char) -> Option<char> {
    Some(match c {
        '=' => '#',
        '(' => '[',
        '/' => '\\',
        ')' => ']',
        '\'' => '^',
        '<' => '{',
        '!' => '|',
        '>' => '}',
        '-' => '~',
        _ => return None,
    })
}

/// Text with its trigraphs replaced, as C's first phase of translation replaces them, except in
/// comments, whose text the definitions keep as it is written.
pub fn trigraphs(text: &[char]) -> Vec<char> {
    if !text.windows(2).any(|pair| pair == ['?', '?']) {
        return text.to_vec();
    }
    let mut out = Vec::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        let c = text[at];
        if c == '/' && text.get(at + 1) == Some(&'/') {
            let end = (at..text.len()).find(|&k| text[k] == '\n').unwrap_or(text.len());
            out.extend(&text[at..end]);
            at = end;
        } else if c == '/' && text.get(at + 1) == Some(&'*') {
            let end = (at + 2..text.len())
                .find(|&k| text[k] == '*' && text.get(k + 1) == Some(&'/'))
                .map_or(text.len(), |k| k + 2);
            out.extend(&text[at..end]);
            at = end;
        } else if let (Some('?'), Some('?'), Some(replaced)) =
            (Some(c), text.get(at + 1).copied(), text.get(at + 2).copied().and_then(trigraph))
        {
            out.push(replaced);
            at += 3;
        } else if c == '"' || c == '\'' {
            // a literal, its trigraphs replaced too; a comment opener inside it is text
            out.push(c);
            at += 1;
            while at < text.len() && text[at] != c && text[at] != '\n' {
                if text[at] == '?' && text.get(at + 1) == Some(&'?') {
                    if let Some(replaced) = text.get(at + 2).copied().and_then(trigraph) {
                        out.push(replaced);
                        at += 3;
                        continue;
                    }
                }
                if text[at] == '\\' && at + 1 < text.len() {
                    out.push('\\');
                    at += 1;
                }
                out.push(text[at]);
                at += 1;
            }
        } else {
            out.push(c);
            at += 1;
        }
    }
    out
}

/// Text with its comments replaced by a space each, string and character literals kept whole.
pub fn without_comments(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        if c == '"' || c == '\'' {
            let end = literal_end(&chars, at);
            out.extend(&chars[at..end]);
            at = end;
        } else if c == '/' && chars.get(at + 1) == Some(&'*') {
            let mut end = at + 2;
            while end < chars.len() && !(chars[end] == '*' && chars.get(end + 1) == Some(&'/')) {
                end += 1;
            }
            out.push(' ');
            at = (end + 2).min(chars.len());
        } else if c == '/' && chars.get(at + 1) == Some(&'/') {
            break;
        } else {
            out.push(c);
            at += 1;
        }
    }
    out
}

/// What follows #define: the macro's name and the macro.
pub fn parse_define(rest: &str) -> Result<(String, Macro), String> {
    let chars: Vec<char> = rest.trim_start().chars().collect();
    if chars.first().is_none_or(|c| !is_start(*c)) {
        return Err("#define needs a name".into());
    }
    let end = word_end(&chars, 0);
    let name: String = chars[..end].iter().collect();
    if chars.get(end) != Some(&'(') {
        let body = without_comments(&chars[end..].iter().collect::<String>()).trim().to_string();
        return Ok((name, Macro { params: None, body }));
    }
    let close = chars[end..].iter().position(|c| *c == ')').map(|at| end + at);
    let Some(close) = close else {
        return Err(format!("#define {name}( has no )"));
    };
    let inside: String = chars[end + 1..close].iter().collect();
    let mut params = Vec::new();
    if !inside.trim().is_empty() {
        for param in inside.split(',') {
            let param = param.trim();
            let valid = param.chars().next().is_some_and(is_start) && param.chars().all(is_word);
            if !valid {
                let shown = if param == "..." { "macros with ... (C99)" } else { param };
                return Err(format!("#define {name}: {shown} is not a parameter name"));
            }
            if params.iter().any(|known| known == param) {
                return Err(format!("#define {name}: parameter {param} is named twice"));
            }
            params.push(param.to_string());
        }
    }
    let body = without_comments(&chars[close + 1..].iter().collect::<String>()).trim().to_string();
    Ok((name, Macro { params: Some(params), body }))
}

/// A piece of a macro's replacement text.
#[derive(Clone, Debug, PartialEq)]
enum Piece {
    Word(String),
    Text(String),
    Space,
    Hash,
    Paste,
}

fn pieces(body: &str) -> Vec<Piece> {
    let chars: Vec<char> = body.chars().collect();
    let mut out = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        if c.is_whitespace() {
            while at < chars.len() && chars[at].is_whitespace() {
                at += 1;
            }
            out.push(Piece::Space);
        } else if c == '#' && chars.get(at + 1) == Some(&'#') {
            out.push(Piece::Paste);
            at += 2;
        } else if c == '#' {
            out.push(Piece::Hash);
            at += 1;
        } else if is_start(c) {
            let end = word_end(&chars, at);
            out.push(Piece::Word(chars[at..end].iter().collect()));
            at = end;
        } else if c == '"' || c == '\'' {
            let end = literal_end(&chars, at);
            out.push(Piece::Text(chars[at..end].iter().collect()));
            at = end;
        } else if c.is_ascii_digit() || (c == '.' && chars.get(at + 1).is_some_and(char::is_ascii_digit)) {
            let end = number_end(&chars, at);
            out.push(Piece::Text(chars[at..end].iter().collect()));
            at = end;
        } else {
            out.push(Piece::Text(c.to_string()));
            at += 1;
        }
    }
    out
}

/// An argument as # writes it: a string literal of its text, spaces between its tokens made one,
/// with " and \ escaped inside its string and character literals.
fn stringified(argument: &str) -> String {
    let chars: Vec<char> = argument.trim().chars().collect();
    let mut out = String::from("\"");
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        if c == '"' || c == '\'' {
            let end = literal_end(&chars, at);
            for &inner in &chars[at..end] {
                if inner == '"' || inner == '\\' {
                    out.push('\\');
                }
                out.push(inner);
            }
            at = end;
        } else if c.is_whitespace() {
            while at < chars.len() && chars[at].is_whitespace() {
                at += 1;
            }
            out.push(' ');
        } else {
            out.push(c);
            at += 1;
        }
    }
    out.push('"');
    out
}

/// A macro's replacement for its arguments: # and ## take an argument as written, other uses take
/// it expanded.
fn substituted(definition: &Macro, raw: &[String], expanded: &[String]) -> Result<String, String> {
    let params = definition.params.clone().unwrap_or_default();
    let argument = |word: &str| params.iter().position(|param| param == word);
    let written = |index: usize| unpainted(raw[index].trim());
    let pieces = pieces(&definition.body);
    let next_solid = |from: usize| (from..pieces.len()).find(|&k| pieces[k] != Piece::Space);
    let mut out: Vec<String> = Vec::new();
    // whether the piece follows a ##
    let mut pasting = false;
    let mut at = 0;
    while at < pieces.len() {
        let pasted_after = next_solid(at + 1).is_some_and(|k| pieces[k] == Piece::Paste);
        match &pieces[at] {
            Piece::Hash if definition.params.is_some() => {
                let operand = next_solid(at + 1);
                let index = operand.and_then(|k| match &pieces[k] {
                    Piece::Word(word) => argument(word),
                    _ => None,
                });
                let (Some(k), Some(index)) = (operand, index) else {
                    return Err("# in a macro must come before a parameter".into());
                };
                out.push(stringified(&written(index)));
                at = k + 1;
                pasting = false;
                continue;
            }
            Piece::Paste => {
                let next = next_solid(at + 1).filter(|_| out.iter().any(|text| !text.trim().is_empty()));
                let Some(next) = next else {
                    return Err("## cannot start or end a macro".into());
                };
                while out.last().is_some_and(|text| text.trim().is_empty()) {
                    out.pop();
                }
                pasting = true;
                at = next;
                continue;
            }
            Piece::Word(word) => match argument(word) {
                Some(index) if pasted_after || pasting => out.push(written(index)),
                Some(index) => out.push(format!(" {} ", expanded[index])),
                None => out.push(word.clone()),
            },
            Piece::Space => out.push(" ".into()),
            Piece::Hash => out.push("#".into()),
            Piece::Text(text) => out.push(text.clone()),
        }
        pasting = false;
        at += 1;
    }
    Ok(out.concat())
}

/// Marks a name the expander met while its macro was off, so it is never expanded later (C's
/// "painted blue" tokens); the tokenizer reads it as a space.
pub const PAINT: char = '\u{1}';

fn unpainted(text: &str) -> String {
    text.replace(PAINT, "")
}

/// A macro use the expander turns off while it reads that macro's replacement.
#[derive(Clone, Debug)]
pub struct Region {
    pub end: usize,
    pub name: String,
}

/// Whether a macro is turned off at position.
pub fn disabled(regions: &[Region], name: &str, position: usize) -> bool {
    regions.iter().any(|region| region.name == name && position < region.end)
}

/// A function-like macro's arguments after its name: the raw text of each, the position after
/// the closing ), and the newlines read past; None when no ( follows (the name is then a plain
/// name).
fn arguments(text: &[char], from: usize) -> Result<Option<(Vec<String>, usize, usize)>, String> {
    let mut at = from;
    let mut newlines = 0;
    loop {
        match text.get(at) {
            Some('\n') => newlines += 1,
            Some(c) if c.is_whitespace() => {}
            Some('/') if text.get(at + 1) == Some(&'*') => {
                at += 2;
                while at < text.len() && !(text[at] == '*' && text.get(at + 1) == Some(&'/')) {
                    newlines += usize::from(text[at] == '\n');
                    at += 1;
                }
                at += 1;
            }
            Some('(') => break,
            _ => return Ok(None),
        }
        at += 1;
    }
    at += 1;
    let mut args = vec![String::new()];
    let mut depth = 0;
    while at < text.len() {
        let c = text[at];
        match c {
            '"' | '\'' => {
                let end = literal_end(text, at);
                args.last_mut().unwrap().extend(&text[at..end]);
                at = end;
                continue;
            }
            '/' if text.get(at + 1) == Some(&'*') => {
                at += 2;
                while at < text.len() && !(text[at] == '*' && text.get(at + 1) == Some(&'/')) {
                    newlines += usize::from(text[at] == '\n');
                    at += 1;
                }
                args.last_mut().unwrap().push(' ');
                at += 2;
                continue;
            }
            '/' if text.get(at + 1) == Some(&'/') => {
                while at < text.len() && text[at] != '\n' {
                    at += 1;
                }
                continue;
            }
            '\n' => {
                newlines += 1;
                args.last_mut().unwrap().push(' ');
            }
            '(' => {
                depth += 1;
                args.last_mut().unwrap().push(c);
            }
            ')' if depth == 0 => return Ok(Some((args, at + 1, newlines))),
            ')' => {
                depth -= 1;
                args.last_mut().unwrap().push(c);
            }
            ',' if depth == 0 => args.push(String::new()),
            _ => args.last_mut().unwrap().push(c),
        }
        at += 1;
    }
    Err("a macro's arguments have no closing )".into())
}

/// The macro a name in code or in a condition expands to, if any: in code, constants stay names.
pub fn expandable<'a>(macros: &'a Macros, name: &str, expand_constants: bool) -> Option<&'a Macro> {
    macros.table.get(name).filter(|_| expand_constants || !macros.constants.contains(name))
}

/// Expands the macro named by the identifier at [start, end) of the buffer in place, when it is
/// a macro (and, for a function-like one, is followed by its arguments); the caller has checked
/// that it is on. The replacement goes where the use was, followed by the newlines the use
/// spanned, and is turned off for its own name until the expander reads past it. Ok(true) when it
/// expanded.
pub fn expand_at(
    text: &mut Vec<char>,
    start: usize,
    end: usize,
    macros: &Macros,
    expand_constants: bool,
    regions: &mut Vec<Region>,
) -> Result<bool, String> {
    let name: String = text[start..end].iter().collect();
    let Some(definition) = expandable(macros, &name, expand_constants) else {
        return Ok(false);
    };
    let (replacement, stop, newlines) = match &definition.params {
        None => (substituted(definition, &[], &[])?, end, 0),
        Some(params) => {
            let Some((mut raw, stop, newlines)) = arguments(text, end)? else {
                return Ok(false);
            };
            if params.is_empty() && raw.len() == 1 && raw[0].trim().is_empty() {
                raw.clear();
            }
            if raw.len() != params.len() {
                let plural = if params.len() == 1 { "" } else { "s" };
                return Err(format!("macro {name} takes {} argument{plural}, given {}", params.len(), raw.len()));
            }
            let active: Vec<Region> = regions.iter().filter(|region| start < region.end).cloned().collect();
            let mut expanded = Vec::new();
            for argument in &raw {
                // an argument is expanded in full, constants too, so # and ## of a macro that
                // passes it on see its value, as in C
                expanded.push(expand_text(argument, macros, true, &active)?);
            }
            (substituted(definition, &raw, &expanded)?, stop, newlines)
        }
    };
    let mut spliced: Vec<char> = format!(" {replacement} ").chars().collect();
    let length = spliced.len();
    spliced.extend(std::iter::repeat_n('\n', newlines));
    let removed = stop - start;
    let grown = spliced.len() as isize - removed as isize;
    text.splice(start..stop, spliced);
    for region in regions.iter_mut() {
        if region.end >= stop {
            region.end = (region.end as isize + grown) as usize;
        } else if region.end > start {
            // a use whose arguments run past the replacement it stands in ends that macro's turn
            // off where it starts, so f(2)(9) with f(a) a*g and g(a) f(a) reads as 2*9*g
            region.end = start;
        }
    }
    regions.push(Region { end: start + length, name });
    Ok(true)
}

/// Text with every macro in it expanded (a macro's arguments, and #if conditions); outer are the
/// macros already off where the text stands. A name met while its macro is off is painted.
pub fn expand_text(text: &str, macros: &Macros, expand_constants: bool, outer: &[Region]) -> Result<String, String> {
    let mut buffer: Vec<char> = text.chars().collect();
    let mut regions: Vec<Region> = Vec::new();
    let outer_names: Vec<String> = outer.iter().map(|region| region.name.clone()).collect();
    let mut at = 0;
    while at < buffer.len() {
        let c = buffer[at];
        if c == '"' || c == '\'' {
            at = literal_end(&buffer, at);
        } else if c.is_ascii_digit() {
            at = number_end(&buffer, at);
        } else if c == PAINT {
            at = word_end(&buffer, at + 1);
        } else if is_start(c) {
            let end = word_end(&buffer, at);
            let name: String = buffer[at..end].iter().collect();
            let off = outer_names.contains(&name) || disabled(&regions, &name, at);
            if off && expandable(macros, &name, expand_constants).is_some() {
                buffer.insert(at, PAINT);
                for region in regions.iter_mut().filter(|region| region.end > at) {
                    region.end += 1;
                }
                at = end + 1;
            } else if off || !expand_at(&mut buffer, at, end, macros, expand_constants, &mut regions)? {
                at = end;
            }
        } else {
            at += 1;
        }
    }
    Ok(buffer.into_iter().collect())
}

// ---- #if -----------------------------------------------------------------------------------

/// defined(NAME) and defined NAME as 1 or 0, before macros are expanded.
fn replace_defined(text: &str, macros: &Macros) -> Result<String, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        if c == '"' || c == '\'' {
            let end = literal_end(&chars, at);
            out.extend(&chars[at..end]);
            at = end;
            continue;
        }
        if !is_start(c) || (at > 0 && is_word(chars[at - 1])) {
            out.push(c);
            at += 1;
            continue;
        }
        let end = word_end(&chars, at);
        if chars[at..end].iter().collect::<String>() != "defined" {
            out.extend(&chars[at..end]);
            at = end;
            continue;
        }
        let skip = |mut k: usize| {
            while k < chars.len() && chars[k].is_whitespace() {
                k += 1;
            }
            k
        };
        let mut k = skip(end);
        let parenthesised = chars.get(k) == Some(&'(');
        if parenthesised {
            k = skip(k + 1);
        }
        if chars.get(k).is_none_or(|c| !is_start(*c)) {
            return Err("defined needs a name".into());
        }
        let name_end = word_end(&chars, k);
        let name: String = chars[k..name_end].iter().collect();
        k = name_end;
        if parenthesised {
            k = skip(k);
            if chars.get(k) != Some(&')') {
                return Err("defined( needs a )".into());
            }
            k += 1;
        }
        out.push_str(if macros.defined(&name) { " 1 " } else { " 0 " });
        at = k;
    }
    Ok(out)
}

/// A value of an #if expression: long or unsigned long, 32 bits.
#[derive(Clone, Copy, Debug)]
struct Value {
    bits: u32,
    unsigned: bool,
}

impl Value {
    fn signed(value: i64) -> Value {
        Value { bits: value as u32, unsigned: false }
    }

    fn truth(self) -> bool {
        self.bits != 0
    }

    fn int(self) -> i64 {
        if self.unsigned {
            self.bits as i64
        } else {
            self.bits as i32 as i64
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Number(u32, bool),
    Op(&'static str),
}

const CONDITION_OPERATORS: [&str; 27] = [
    "<<", ">>", "<=", ">=", "==", "!=", "&&", "||", "(", ")", "+", "-", "*", "/", "%", "&", "|", "^", "~", "!", "<",
    ">", "?", ":", ",", "#", "=",
];

/// An integer constant's value and whether it is unsigned (a u suffix, or too large for long).
pub fn integer_constant(text: &str) -> Result<(u32, bool), String> {
    let lower = text.to_ascii_lowercase();
    let digits = lower.trim_end_matches(['u', 'l']);
    let suffix = &lower[digits.len()..];
    if suffix.matches('u').count() > 1 || suffix.matches('l').count() > 1 {
        return Err(format!("bad suffix on {text}"));
    }
    let (radix, body) = if let Some(hex) = digits.strip_prefix("0x") {
        (16, hex)
    } else if digits.len() > 1 && digits.starts_with('0') {
        (8, &digits[1..])
    } else {
        (10, digits)
    };
    let value = u64::from_str_radix(body, radix).map_err(|_| format!("bad number {text}"))?;
    if value > u32::MAX as u64 {
        return Err(format!("{text} does not fit in 32 bits"));
    }
    let unsigned = suffix.contains('u') || value > i32::MAX as u64;
    Ok((value as u32, unsigned))
}

/// A character constant's value from its quotes on: 'a', '\n', '\101', '\x41'.
pub fn character_constant(text: &[char]) -> Result<(u32, usize), String> {
    let bad = || "a character constant holds one character, as in 'a'".to_string();
    if text.get(1) == Some(&'\\') {
        let (value, length) = escape_value(&text[2..]).ok_or_else(bad)?;
        if text.get(2 + length) != Some(&'\'') {
            return Err(bad());
        }
        return Ok((value, 3 + length));
    }
    match (text.get(1), text.get(2)) {
        (Some(c), Some('\'')) if *c != '\'' => Ok((*c as u32, 3)),
        _ => Err(bad()),
    }
}

/// An escape sequence's value from after its backslash, and how many characters it takes: C's
/// simple escapes, up to three octal digits, or \x and two hexadecimal digits.
pub fn escape_value(text: &[char]) -> Option<(u32, usize)> {
    let c = *text.first()?;
    let simple = match c {
        'n' => Some(10),
        't' => Some(9),
        'r' => Some(13),
        'a' => Some(7),
        'b' => Some(8),
        'f' => Some(12),
        'v' => Some(11),
        '\\' | '\'' | '"' | '?' => Some(c as u32),
        _ => None,
    };
    if let Some(value) = simple {
        return Some((value, 1));
    }
    if ('0'..='7').contains(&c) {
        let length = text.iter().take(3).take_while(|c| ('0'..='7').contains(*c)).count();
        let digits: String = text[..length].iter().collect();
        return Some((u32::from_str_radix(&digits, 8).ok()?, length));
    }
    if c == 'x' {
        let digits: String = text.iter().skip(1).take(2).collect();
        if digits.len() == 2 && digits.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some((u32::from_str_radix(&digits, 16).ok()?, 3));
        }
    }
    None
}

fn condition_tokens(text: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        if c.is_whitespace() || c == PAINT {
            at += 1;
        } else if c.is_ascii_digit() {
            let end = number_end(&chars, at);
            let word: String = chars[at..end].iter().collect();
            if word.contains('.') || (!word.to_ascii_lowercase().starts_with("0x") && word.contains(['e', 'E'])) {
                return Err(format!("{word} is not an integer"));
            }
            let (value, unsigned) = integer_constant(&word)?;
            tokens.push(Token::Number(value, unsigned));
            at = end;
        } else if is_start(c) {
            // a name no macro gives a value is 0
            at = word_end(&chars, at);
            tokens.push(Token::Number(0, false));
        } else if c == '\'' {
            let (value, length) = character_constant(&chars[at..])?;
            // char is signed: '\377' is -1
            tokens.push(Token::Number(value as u8 as i8 as i32 as u32, false));
            at += length;
        } else if let Some(op) =
            CONDITION_OPERATORS.iter().find(|op| chars[at..].starts_with(&op.chars().collect::<Vec<_>>()))
        {
            if matches!(*op, "#" | "=" | ",") {
                return Err(format!("{op} cannot stand in an #if condition"));
            }
            tokens.push(Token::Op(op));
            at += op.len();
        } else {
            return Err(format!("unexpected {c:?}"));
        }
    }
    Ok(tokens)
}

struct Condition {
    tokens: Vec<Token>,
    at: usize,
    depth: usize,
}

impl Condition {
    fn eat(&mut self, op: &str) -> bool {
        let found = matches!(self.tokens.get(self.at), Some(Token::Op(found)) if *found == op);
        if found {
            self.at += 1;
        }
        found
    }

    fn next_op(&self, ops: &[&'static str]) -> Option<&'static str> {
        match self.tokens.get(self.at) {
            Some(Token::Op(op)) => ops.iter().find(|candidate| *candidate == op).copied(),
            _ => None,
        }
    }

    fn conditional(&mut self) -> Result<Value, String> {
        self.depth += 1;
        if self.depth > 200 {
            return Err("the condition nests too deeply".into());
        }
        let cond = self.binary(0)?;
        let value = if self.eat("?") {
            let then = self.conditional()?;
            if !self.eat(":") {
                return Err("? without :".into());
            }
            let otherwise = self.conditional()?;
            let unsigned = then.unsigned || otherwise.unsigned;
            let chosen = if cond.truth() { then } else { otherwise };
            Value { bits: chosen.bits, unsigned }
        } else {
            cond
        };
        self.depth -= 1;
        Ok(value)
    }

    fn binary(&mut self, level: usize) -> Result<Value, String> {
        const LEVELS: [&[&str]; 10] = [
            &["||"],
            &["&&"],
            &["|"],
            &["^"],
            &["&"],
            &["==", "!="],
            &["<", ">", "<=", ">="],
            &["<<", ">>"],
            &["+", "-"],
            &["*", "/", "%"],
        ];
        if level == LEVELS.len() {
            return self.unary();
        }
        let mut left = self.binary(level + 1)?;
        while let Some(op) = self.next_op(LEVELS[level]) {
            self.at += 1;
            let right = self.binary(level + 1)?;
            left = apply(op, left, right)?;
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Value, String> {
        if let Some(op) = self.next_op(&["+", "-", "~", "!"]) {
            self.at += 1;
            self.depth += 1;
            if self.depth > 200 {
                return Err("the condition nests too deeply".into());
            }
            let value = self.unary()?;
            self.depth -= 1;
            return Ok(match op {
                "+" => value,
                "-" => Value { bits: value.bits.wrapping_neg(), unsigned: value.unsigned },
                "~" => Value { bits: !value.bits, unsigned: value.unsigned },
                _ => Value::signed(i64::from(!value.truth())),
            });
        }
        match self.tokens.get(self.at).cloned() {
            Some(Token::Number(bits, unsigned)) => {
                self.at += 1;
                Ok(Value { bits, unsigned })
            }
            Some(Token::Op("(")) => {
                self.at += 1;
                let value = self.conditional()?;
                if !self.eat(")") {
                    return Err("( without )".into());
                }
                Ok(value)
            }
            _ => Err("expected a value".into()),
        }
    }
}

/// A binary operator on two values, with C's usual arithmetic conversions.
fn apply(op: &str, left: Value, right: Value) -> Result<Value, String> {
    if op == "&&" || op == "||" {
        let truth = if op == "&&" { left.truth() && right.truth() } else { left.truth() || right.truth() };
        return Ok(Value::signed(i64::from(truth)));
    }
    if op == "<<" || op == ">>" {
        let count = right.int();
        if !(0..32).contains(&count) {
            return Err(format!("shift count {count} is out of range"));
        }
        let bits = if op == "<<" {
            left.bits << count
        } else if left.unsigned {
            left.bits >> count
        } else {
            ((left.bits as i32) >> count) as u32
        };
        return Ok(Value { bits, unsigned: left.unsigned });
    }
    let unsigned = left.unsigned || right.unsigned;
    let (a, b) = (left.bits, right.bits);
    let compare = |ordering: std::cmp::Ordering| {
        let truth = match op {
            "<" => ordering.is_lt(),
            ">" => ordering.is_gt(),
            "<=" => ordering.is_le(),
            _ => ordering.is_ge(),
        };
        Value::signed(i64::from(truth))
    };
    Ok(match op {
        "==" => Value::signed(i64::from(a == b)),
        "!=" => Value::signed(i64::from(a != b)),
        "<" | ">" | "<=" | ">=" if unsigned => compare(a.cmp(&b)),
        "<" | ">" | "<=" | ">=" => compare((a as i32).cmp(&(b as i32))),
        "+" => Value { bits: a.wrapping_add(b), unsigned },
        "-" => Value { bits: a.wrapping_sub(b), unsigned },
        "*" => Value { bits: a.wrapping_mul(b), unsigned },
        "&" => Value { bits: a & b, unsigned },
        "|" => Value { bits: a | b, unsigned },
        "^" => Value { bits: a ^ b, unsigned },
        "/" | "%" if b == 0 => return Err("division by zero in the condition".into()),
        "/" if unsigned => Value { bits: a / b, unsigned },
        "%" if unsigned => Value { bits: a % b, unsigned },
        "/" => Value { bits: (a as i32).wrapping_div(b as i32) as u32, unsigned },
        _ => Value { bits: (a as i32).wrapping_rem(b as i32) as u32, unsigned },
    })
}

/// The truth of an #if or #elif condition.
pub fn condition(text: &str, macros: &Macros) -> Result<bool, String> {
    let replaced = replace_defined(&without_comments(text), macros)?;
    let expanded = expand_text(&replaced, macros, true, &[])?;
    let tokens = condition_tokens(&expanded)?;
    if tokens.is_empty() {
        return Err("#if needs a condition".into());
    }
    let mut parser = Condition { tokens, at: 0, depth: 0 };
    let value = parser.conditional()?;
    if parser.at != parser.tokens.len() {
        return Err("the condition does not end where it should".into());
    }
    Ok(value.truth())
}

// ---- __DATE__ and __TIME__ -------------------------------------------------------------------

/// The time of translation as __DATE__ ("Mmm dd yyyy") and __TIME__ ("hh:mm:ss") give it, in UTC;
/// SOURCE_DATE_EPOCH, when set, stands in for the clock so builds can repeat.
pub fn translation_time() -> (String, String) {
    let seconds =
        std::env::var("SOURCE_DATE_EPOCH").ok().and_then(|value| value.parse::<i64>().ok()).unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_secs() as i64)
        });
    date_and_time(seconds)
}

/// A time in seconds since 1970 as __DATE__ and __TIME__ write it.
fn date_and_time(seconds: i64) -> (String, String) {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let (days, rest) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));
    // the civil date of a day count (Howard Hinnant's days_from_civil, inverted)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted + 2) / 5 + 1;
    let month = if shifted < 10 { shifted + 3 } else { shifted - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    let date = format!("{} {:>2} {}", MONTHS[(month - 1) as usize], day, year);
    let time = format!("{:02}:{:02}:{:02}", rest / 3600, rest / 60 % 60, rest % 60);
    (date, time)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn macros(defines: &[&str]) -> Macros {
        let mut found = Macros::default();
        for define in defines {
            let (name, definition) = parse_define(define).unwrap();
            found.define(&name, definition, false);
        }
        found
    }

    /// The expansion without its spaces and paint.
    fn expanded(defines: &[&str], text: &str) -> String {
        let out = expand_text(text, &macros(defines), true, &[]).unwrap();
        out.chars().filter(|c| !c.is_whitespace() && *c != PAINT).collect()
    }

    #[test]
    fn macros_expand_as_c_expands_them() {
        assert_eq!(expanded(&["SQ(x) ((x) * (x))"], "SQ(a + 1)"), "((a+1)*(a+1))");
        assert_eq!(expanded(&["STR(x) #x"], "STR( a  \"b\\n\" )"), "\"a\\\"b\\\\n\\\"\"");
        assert_eq!(expanded(&["CAT(a, b) a ## b", "xy 5"], "CAT(x, y)"), "5");
        assert_eq!(expanded(&["CAT(a, b) a ## b"], "CAT(1, CAT(2, 3))"), "1CAT(2,3)");
        // C 3.8.3.5's examples of a macro read again with itself turned off
        assert_eq!(expanded(&["f(a) a*g", "g(a) f(a)"], "f(2)(9)"), "2*9*g");
        assert_eq!(expanded(&["x 3", "f(a) f(x * (a))", "z z[0]"], "f(f(z))"), "f(3*(f(3*(z[0]))))");
        assert_eq!(expanded(&["F(a) a"], "F (1) F"), "1F");
        assert_eq!(expanded(&["NONE() 7"], "NONE()"), "7");
    }

    #[test]
    fn conditions_are_c_constant_expressions() {
        let found = macros(&["TWO 2", "ADD(a, b) ((a) + (b))"]);
        for (text, truth) in [
            ("!2 == 1", false),
            ("3 & 1 == 1", true),
            ("-7 / 2 == -3 && -7 % 2 == -1", true),
            ("ADD(TWO, 3) == 5 && defined TWO && !defined(THREE)", true),
            ("-1 < 0u", false),
            ("0xFFFFFFFF == -1", true),
            ("2147483647 + 1 < 0", true),
            ("1 ? 2 : 0", true),
            ("(1 << 4) + 010 == 24 && '\\n' == 10 && UNKNOWN == 0", true),
            ("1 > 2 ? 1 : 1 > 0 ? 3 : 0", true),
        ] {
            assert_eq!(condition(text, &found), Ok(truth), "{text}");
        }
        assert!(condition("1 / 0", &found).is_err());
        assert!(condition("1.5", &found).is_err());
        assert!(condition("", &found).is_err());
    }

    #[test]
    fn dates_read_as_c_writes_them() {
        assert_eq!(date_and_time(951_782_400), ("Feb 29 2000".to_string(), "00:00:00".to_string()));
        assert_eq!(date_and_time(1_791_975_845), ("Oct 14 2026".to_string(), "11:04:05".to_string()));
    }
}
