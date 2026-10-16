//! The tokenizer, which is also the preprocessor: comments, trigraphs, the conditionals,
//! #define and macros (cpp.rs), #include, #pragma, #line, #error, and asm blocks.

use super::*;

// ---- characters ----------------------------------------------------------------------------

pub(super) fn text_of(chars: &[char]) -> String {
    chars.iter().collect()
}

pub(super) fn starts_with(chars: &[char], position: usize, prefix: &str) -> bool {
    (position..).zip(prefix.chars()).all(|(at, c)| chars.get(at) == Some(&c))
}

pub(super) fn find(chars: &[char], from: usize, pattern: &str) -> Option<usize> {
    (from..chars.len()).find(|&at| starts_with(chars, at, pattern))
}

pub(super) fn line_end(chars: &[char], position: usize) -> usize {
    (position..chars.len()).find(|&at| chars[at] == '\n').unwrap_or(chars.len())
}

pub(super) fn count_lines(chars: &[char], from: usize, to: usize) -> usize {
    chars[from.min(to)..to].iter().filter(|&&c| c == '\n').count()
}

/// The text before a // comment.
pub(super) fn before_comment(text: &str) -> &str {
    text.split("//").next().unwrap_or("")
}

/// The first word and the rest (its trailing whitespace kept).
pub(super) fn split_first_word(text: &str) -> Vec<&str> {
    let rest = text.trim_start();
    if rest.is_empty() {
        return Vec::new();
    }
    match rest.find(char::is_whitespace) {
        None => vec![rest],
        Some(at) => {
            let remainder = rest[at..].trim_start();
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
pub(super) struct Block {
    pub(super) parent: bool,
    pub(super) on: bool,
    pub(super) taken: bool,
    pub(super) line: usize,
    /// whether its #else has been read, after which only #endif may come
    pub(super) otherwise: bool,
}

/// The value of an #if or #elif condition, as C works it out (cpp::condition).
pub(super) fn condition_value(text: &str, macros: &Macros, line: usize) -> Result<bool> {
    cpp::condition(text, macros)
        .or_else(|reason| lex_fail(line, format!("cannot read the condition {}: {reason}", quoted(text.trim()))))
}

pub(super) fn conditional_directive(
    word: &str,
    rest: &str,
    conditions: &mut Vec<Block>,
    macros: &Macros,
    line: usize,
) -> Result<()> {
    let compiling = conditions.last().is_none_or(|block| block.on);
    if matches!(word, "if" | "ifdef" | "ifndef") {
        let name = before_comment(rest).trim();
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

/// What a token is, with what only a token of its kind has.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Kind {
    /// #define NAME VALUE of a value that is not a macro's: its words, the name and the rest.
    Define(Vec<String>),
    /// #include: the file's path, whether it was <path>, and the integer constants the file
    /// #defines (or changes), with their values.
    Include {
        path: String,
        system: bool,
        constants: Vec<(String, i128)>,
    },
    /// #pragma's words.
    Pragma(Vec<Node>),
    /// #pragma assembly: the rest of the file, the token's text, is assembly.
    Assembly,
    /// An asm block: its header's words and its lines.
    Asm {
        words: Vec<String>,
        lines: Vec<String>,
    },
    /// A string literal, wide when written L"...".
    Str {
        wide: bool,
    },
    /// An integer literal, unsigned when its suffix says so (1u).
    Int {
        unsigned: bool,
    },
    Float,
    Name,
    Op,
    Eof,
}

/// An integer token of a number written plainly (a character, __LINE__).
pub(super) const INT: Kind = Kind::Int { unsigned: false };
pub(super) const STR: Kind = Kind::Str { wide: false };

impl Kind {
    pub(super) fn name(&self) -> &'static str {
        match self {
            Kind::Define(_) => "define",
            Kind::Include { .. } => "include",
            Kind::Pragma(_) => "pragma",
            Kind::Assembly => "assembly",
            Kind::Asm { .. } => "asm",
            Kind::Str { .. } => "string",
            Kind::Int { .. } => "int",
            Kind::Float => "float",
            Kind::Name => "name",
            Kind::Op => "op",
            Kind::Eof => "eof",
        }
    }

    pub(super) fn is_int(&self) -> bool {
        matches!(self, Kind::Int { .. })
    }

    pub(super) fn is_string(&self) -> bool {
        matches!(self, Kind::Str { .. })
    }
}

#[derive(Clone, Debug)]
pub(super) struct Token {
    pub(super) kind: Kind,
    pub(super) line: usize,
    /// The token's text; #define, #include, #pragma and asm tokens have none.
    pub(super) text: Option<String>,
    /// The comment block right above a name.
    pub(super) doc: Option<String>,
}

impl Token {
    pub(super) fn new(kind: Kind, line: usize, text: Option<String>) -> Token {
        Token { kind, line, text, doc: None }
    }

    pub(super) fn text(&self) -> &str {
        self.text.as_deref().unwrap_or("")
    }

    /// Whether a string literal is wide (L"...").
    pub(super) fn wide(&self) -> bool {
        matches!(self.kind, Kind::Str { wide: true })
    }
}

/// A string literal from the opening quote: its text and the position after it.
pub(super) fn read_string(text: &[char], position: usize, line: usize) -> Result<(String, usize)> {
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
pub(super) fn read_char(text: &[char], position: usize, line: usize) -> Result<(i64, usize)> {
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
pub(super) fn directive_words(rest: &str, line: usize) -> Result<Vec<Node>> {
    let tokens = asm::tokenize(rest).or_else(|error| lex_fail(line, error))?;
    let words = tokens.into_iter().map(|token| match token {
        asm::Token::Word(word) => Node::from(word),
        asm::Token::Text(text) => node!({"string": text}),
    });
    Ok(words.collect())
}

/// An asm block from after its "asm": the token and the position after its closing }.
pub(super) fn read_asm(text: &[char], position: usize, line: usize) -> Result<(Token, usize)> {
    let Some(opening) = find(text, position, "{") else {
        return lex_fail(line, "asm block without {");
    };
    let words: Vec<String> = text_of(&text[position..opening]).split_whitespace().map(String::from).collect();
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
    Ok((Token::new(Kind::Asm { words, lines }, line, None), end))
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
pub(super) fn joined(lines: &[&str]) -> String {
    let mut text = String::new();
    for line in lines {
        let mut end = text.chars().rev();
        let hyphen = end.next() == Some('-')
            && end.next().is_some_and(|c| c.is_ascii_alphabetic())
            && line.chars().next().is_some_and(char::is_alphabetic);
        if !hyphen && !text.is_empty() {
            text.push(' ');
        }
        text.push_str(line);
    }
    text
}

/// The end of a directive's line, read on past a backslash that ends a line (C's line splicing),
/// and how many lines it joins.
pub(super) fn directive_end(text: &[char], position: usize) -> (usize, usize) {
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
pub(super) fn directive_at(text: &[char], position: usize) -> Option<(String, String)> {
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
/// then [fFuUlL]*, in ASCII digits. The word, the suffix and the end.
pub(super) fn number_at(text: &[char], position: usize) -> Option<(String, String, usize)> {
    let at = |i: usize| text.get(i).copied().unwrap_or('\0');
    let decimal = |i: usize| at(i).is_ascii_digit();
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
pub(super) const NO_DOUBLE: &str = "double is not supported: the VM has 32-bit floats only; use float";

/// The files #include reads as text (ParseOptions::headers and libraries): the SDK's and definitions' headers, and the
/// libraries #include "..." names, each by its path.
#[derive(Clone, Copy)]
pub struct Files<'a> {
    pub headers: &'a IndexMap<String, String>,
    pub libraries: Option<&'a IndexMap<String, String>>,
}

impl<'a> Files<'a> {
    /// An included file's text: a header's by its path, a library's by its path in quotes.
    pub(super) fn get(&self, key: &str) -> Option<&'a String> {
        match key.strip_prefix('"').and_then(|key| key.strip_suffix('"')) {
            Some(path) => self.libraries?.get(path),
            None => self.headers.get(key),
        }
    }
}

pub(super) type Headers<'a> = Option<Files<'a>>;

/// The key of an included file among Files: a header's path, a library's path in quotes.
pub(super) fn included_key(path: &str, system: bool) -> String {
    if system {
        path.to_string()
    } else {
        format!("\"{path}\"")
    }
}

/// How deep headers may include headers.
pub(super) const INCLUDE_DEPTH: usize = 32;

/// The macros in force, as a cache key: each by its name, sorted, with its parameters, its body
/// and whether it stays a constant.
pub(super) type MacrosKey = Vec<(String, Option<Vec<String>>, String, bool)>;

thread_local! {
    /// The macros a header leaves defined, by its path, its text and the macros before it.
    static HEADER_MACROS: RefCell<HashMap<(String, String, MacrosKey), Macros>> = RefCell::new(HashMap::new());
}

/// The macros after #include <path>: the header read as text, as C reads it, from the macros
/// before it, so its #defines reach the including file's #if. Its declarations are read apart, as
/// before (library::expand), and its constants stay names in code, so nothing else changes. The
/// prelude (c2s.h, for C tools only) and a header the headers do not have (an error later) add
/// none.
pub(super) fn header_macros(path: &str, headers: Files, macros: Macros, depth: usize) -> Result<Macros> {
    let Some(text) = headers.get(path).filter(|_| path != crate::sdk::PRELUDE) else {
        return Ok(macros);
    };
    if depth > INCLUDE_DEPTH {
        return Err(Error::invalid(format!("#include {path}: files include each other too deeply")));
    }
    let mut defined: MacrosKey = macros
        .table
        .iter()
        .map(|(name, definition)| {
            let constant = macros.constants.contains(name);
            (name.clone(), definition.params.clone(), definition.body.clone(), constant)
        })
        .collect();
    defined.sort_by(|a, b| a.0.cmp(&b.0));
    let key = (path.to_string(), text.clone(), defined);
    if let Some(found) = HEADER_MACROS.with(|cache| cache.borrow().get(&key).cloned()) {
        return Ok(found);
    }
    let chars: Vec<char> = text.chars().collect();
    let mut found = macros;
    tokenize_with(&chars, &mut found, path, Some(headers), depth + 1)
        .map_err(|fault| Error::invalid(format!("in <{path}>: {fault}")))?;
    HEADER_MACROS.with(|cache| cache.borrow_mut().insert(key, found.clone()));
    Ok(found)
}

/// The tokens of a file. macros are the #defines in force before the first line (a mod patch's
/// LANGUAGE), which stay constants in code; file is the name __FILE__ gives until a #line names
/// another.
pub(super) fn tokenize(source: &[char], mut macros: Macros, file: &str) -> Result<Vec<Token>> {
    tokenize_with(source, &mut macros, file, None, 0)
}

/// tokenize, reading the headers given for their macros; macros ends as the file leaves them.
pub(super) fn tokenize_with(
    source: &[char],
    macros: &mut Macros,
    file: &str,
    headers: Headers,
    depth: usize,
) -> Result<Vec<Token>> {
    let lexer = Lexer {
        // the text grows and shrinks as macros are replaced in it
        text: cpp::trigraphs(source),
        macros,
        file: file.to_string(),
        headers,
        depth,
        regions: Vec::new(),
        when: None,
        tokens: Vec::new(),
        comments: Vec::new(),
        conditions: Vec::new(),
        position: 0,
        line: 1,
    };
    lexer.run()
}

/// The tokenizer's place in a file, and what it has read so far.
pub(super) struct Lexer<'a, 'm> {
    pub(super) text: Vec<char>,
    pub(super) macros: &'m mut Macros,
    /// The name __FILE__ gives, which #line changes.
    pub(super) file: String,
    pub(super) headers: Headers<'a>,
    /// How deep headers include headers here.
    pub(super) depth: usize,
    /// Where macros' replacements are in the text, which they do not expand again.
    pub(super) regions: Vec<cpp::Region>,
    /// __DATE__ and __TIME__, read from the clock when first used.
    pub(super) when: Option<(String, String)>,
    pub(super) tokens: Vec<Token>,
    /// The // comments that are a line of their own, by line: what comment_block reads.
    pub(super) comments: Vec<(usize, String)>,
    /// The #if blocks open.
    pub(super) conditions: Vec<Block>,
    pub(super) position: usize,
    pub(super) line: usize,
}

/// Whether the tokenizer goes on after a directive: #pragma assembly ends the file's tokens.
pub(super) enum After {
    More,
    Done,
}

impl Lexer<'_, '_> {
    pub(super) fn run(mut self) -> Result<Vec<Token>> {
        while self.position < self.text.len() {
            let c = self.text[self.position];
            if c == '\n' {
                self.line += 1;
                self.position += 1;
                continue;
            }
            let directive = if c == '#' { directive_at(&self.text, self.position) } else { None };
            if let Some((word, rest)) = &directive {
                if CONDITIONALS.contains(&word.as_str()) {
                    conditional_directive(word, rest, &mut self.conditions, self.macros, self.line)?;
                    self.skip_directive();
                    continue;
                }
            }
            if self.conditions.last().is_some_and(|block| !block.on) {
                self.position = line_end(&self.text, self.position);
                continue;
            }
            if let Some((word, rest)) = &directive {
                if let After::Done = self.directive(word, rest)? {
                    return Ok(self.tokens);
                }
                continue;
            }
            if c == '#'
                && self.text[self.position + 1..line_end(&self.text, self.position)].iter().all(|c| c.is_whitespace())
            {
                // the null directive
                self.position = line_end(&self.text, self.position);
                continue;
            }
            self.token(c)?;
        }
        if let Some(block) = self.conditions.last() {
            return lex_fail(block.line, "#if without #endif");
        }
        self.tokens.push(Token::new(Kind::Eof, self.line, Some(String::new())));
        Ok(self.tokens)
    }

    /// Past a directive's line (and the lines it splices).
    pub(super) fn skip_directive(&mut self) {
        let (end, joined) = directive_end(&self.text, self.position);
        self.line += joined;
        self.position = end;
    }

    pub(super) fn push(&mut self, kind: Kind, text: Option<String>) {
        self.tokens.push(Token::new(kind, self.line, text));
    }

    /// A directive but the conditionals: #undef, #define, #include, #pragma, #error and #line.
    pub(super) fn directive(&mut self, word: &str, rest: &str) -> Result<After> {
        let line = self.line;
        let (end, joined) = directive_end(&self.text, self.position);
        let mut next_line = line + joined;
        match word {
            "undef" => self.macros.undefine(before_comment(rest).trim()),
            "define" => self.define(rest)?,
            "include" => self.include(rest)?,
            "pragma" => {
                let words = directive_words(rest, line)?;
                if words.first().is_some_and(|word| *word == "assembly") {
                    let rest = text_of(&self.text[(end + 1).min(self.text.len())..]);
                    self.push(Kind::Assembly, Some(rest));
                    return Ok(After::Done);
                }
                self.push(Kind::Pragma(words), None);
            }
            "error" => return lex_fail(line, format!("#error {}", rest.trim())),
            "line" => next_line = self.line_directive(rest)?.unwrap_or(next_line),
            other => return lex_fail(line, format!("unknown directive #{other}")),
        }
        self.line = next_line;
        self.position = end;
        Ok(After::More)
    }

    /// #define: a macro, which is a constant too when its value is one (a number or a string).
    pub(super) fn define(&mut self, rest: &str) -> Result<()> {
        let line = self.line;
        let (name, definition) = cpp::parse_define(rest).or_else(|reason| lex_fail(line, reason))?;
        if cpp::PREDEFINED.contains(&name.as_str()) || name == "defined" {
            return lex_fail(line, format!("{name} cannot be #defined"));
        }
        let same = |old: &cpp::Macro| {
            let spaced = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
            old.params == definition.params && spaced(&old.body) == spaced(&definition.body)
        };
        if self.macros.table.get(&name).is_some_and(|old| !same(old)) {
            return lex_fail(line, format!("{name} is #defined again differently; #undef it first"));
        }
        if definition.params.is_some() {
            self.macros.define(&name, definition, false);
            return Ok(());
        }
        let words: Vec<String> = split_first_word(rest).into_iter().map(String::from).collect();
        let constant = match words.get(1) {
            Some(value) => constant_of(value)?,
            None => None,
        };
        let integer = constant.as_ref().filter(|value| value.kind() == "int");
        let integer = integer.and_then(|value| value["value"].as_i64());
        self.macros.define(&name, definition, constant.is_some());
        if let Some(integer) = integer {
            self.macros.values.insert(name.clone(), integer as i128);
        }
        self.push(Kind::Define(words), None);
        Ok(())
    }

    /// #include <path> or #include "path": the file's macros reach this one (with the headers
    /// given), and the token keeps the integer constants it defines or changes.
    pub(super) fn include(&mut self, rest: &str) -> Result<()> {
        let line = self.line;
        let rest = rest.trim();
        let before = self.headers.map(|_| self.macros.values.clone());
        let system = rest.starts_with('<') && rest.ends_with('>');
        let path = if system {
            rest[1..rest.len() - 1].trim().to_string()
        } else {
            match directive_words(rest, line)?.as_slice() {
                [word] if word.as_object().is_some() => word.text("string").to_string(),
                _ => return lex_fail(line, "expected #include \"path\" or #include <path>"),
            }
        };
        // a header's macros, and a library's, reach the file that includes it, as C includes it
        if let Some(files) = self.headers {
            let key = included_key(&path, system);
            *self.macros = header_macros(&key, files, std::mem::take(self.macros), self.depth)?;
        }
        // the integer constants the file (and what it includes) defines or changes, which the
        // including file's constant expressions use as C expands them
        let mut constants = Vec::new();
        if let Some(before) = before {
            let changed = self.macros.values.iter().filter(|(name, value)| before.get(*name) != Some(value));
            constants = changed.map(|(name, value)| (name.clone(), *value)).collect();
            constants.sort();
        }
        self.push(Kind::Include { path, system, constants }, None);
        Ok(())
    }

    /// #line N or #line N "file": the next line's number, and the file's name from there.
    pub(super) fn line_directive(&mut self, rest: &str) -> Result<Option<usize>> {
        let line = self.line;
        let expanded = cpp::expand_text(rest, self.macros, true, &[]).or_else(|reason| lex_fail(line, reason))?;
        let words = directive_words(&expanded.replace(cpp::PAINT, " "), line)?;
        let number = words.first().and_then(Node::as_str).and_then(|word| word.parse::<usize>().ok());
        let name = words.get(1).map(|word| word.get("string").and_then(Node::as_str));
        match (number, name, words.len()) {
            (Some(number), None, 1) => Ok(Some(number.saturating_sub(1))),
            (Some(number), Some(Some(name)), 2) => {
                self.file = name.to_string();
                Ok(Some(number.saturating_sub(1)))
            }
            _ => lex_fail(line, "#line takes a line number and, after it, a file name in quotes"),
        }
    }

    /// The token at c (or the space or comment there).
    pub(super) fn token(&mut self, c: char) -> Result<()> {
        let (text, position) = (&self.text, self.position);
        if c.is_whitespace() {
            self.position += 1;
        } else if starts_with(text, position, "//") {
            self.line_comment();
        } else if starts_with(text, position, "/*") {
            let Some(end) = find(text, position + 2, "*/") else {
                return lex_fail(self.line, "unterminated comment");
            };
            self.line += count_lines(text, position, end);
            self.position = end + 2;
        } else if c == '"' {
            let (value, end) = read_string(text, position, self.line)?;
            self.push(STR, Some(value));
            self.position = end;
        } else if c.is_ascii_digit() || (c == '.' && text.get(position + 1).is_some_and(|next| next.is_ascii_digit())) {
            self.number(c)?;
        } else if c == '\'' {
            let (value, end) = read_char(text, position, self.line)?;
            self.push(INT, Some(value.to_string()));
            self.position = end;
        } else if c == 'L' && text.get(position + 1) == Some(&'\'') {
            self.wide_character()?;
        } else if c == 'L' && text.get(position + 1) == Some(&'"') {
            // a wide string: an array of wchar_t, each a character's Shift-JIS code
            let (value, end) = read_string(text, position + 1, self.line)?;
            self.push(Kind::Str { wide: true }, Some(value));
            self.position = end;
        } else if c == cpp::PAINT || c.is_ascii_alphabetic() || c == '_' {
            self.name(c)?;
        } else {
            self.operator(c)?;
        }
        Ok(())
    }

    /// A // comment: one that is a line of its own is kept, for the doc of a name below it.
    pub(super) fn line_comment(&mut self) {
        let (text, position) = (&self.text, self.position);
        let end = line_end(text, position);
        let start = (0..position).rev().find(|&at| text[at] == '\n').map_or(0, |at| at + 1);
        if text_of(&text[start..position]).trim().is_empty() {
            self.comments.push((self.line, text_of(&text[position + 2..end]).trim().to_string()));
        }
        self.position = end;
    }

    /// A number: an int, or a float when it has a point, an exponent or an f.
    pub(super) fn number(&mut self, c: char) -> Result<()> {
        let line = self.line;
        let Some((mut word, suffix, end)) = number_at(&self.text, self.position) else {
            return Err(Error::invalid(format!("line {line}: cannot read the number at {}", quoted(&c.to_string()))));
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
        let kind = if floating { Kind::Float } else { Kind::Int { unsigned: suffix.contains(['u', 'U']) } };
        self.push(kind, Some(word));
        self.position = end;
        Ok(())
    }

    /// L'x': a wide character, its Shift-JIS code, which wchar_t holds.
    pub(super) fn wide_character(&mut self) -> Result<()> {
        let (text, position, line) = (&self.text, self.position, self.line);
        let (value, end) = cpp::character_constant(&text[position + 1..]).or_else(|reason| lex_fail(line, reason))?;
        let value = match char::from_u32(value).filter(|_| text[position + 2] != '\\') {
            Some(character) => dc2tk_disc::text::encode(&character.to_string())
                .ok_or_else(|| Error::invalid(format!("line {line}: {character:?} has no Shift-JIS code")))?
                .iter()
                .fold(0u32, |code, byte| code << 8 | u32::from(*byte)),
            None => value,
        };
        self.push(INT, Some(value.to_string()));
        self.position += 1 + end;
        Ok(())
    }

    /// A name: an asm block, a macro expanded in the text, a name C predefines, or a name (with the
    /// comment block above it). A painted name is one a macro's own replacement held, which is
    /// never expanded.
    pub(super) fn name(&mut self, c: char) -> Result<()> {
        let line = self.line;
        let painted = c == cpp::PAINT;
        let start = self.position + usize::from(painted);
        let end = cpp::word_end(&self.text, start);
        let name = text_of(&self.text[start..end]);
        if name == "asm" && !painted {
            let (token, after) = read_asm(&self.text, end, line)?;
            self.tokens.push(token);
            self.line += count_lines(&self.text, self.position, after);
            self.position = after;
            return Ok(());
        }
        if !painted && cpp::expandable(self.macros, &name, false).is_some() {
            let position = self.position;
            self.regions.retain(|region| region.end > position);
            if !cpp::disabled(&self.regions, &name, position)
                && cpp::expand_at(&mut self.text, position, end, self.macros, false, &mut self.regions)
                    .or_else(|reason| lex_fail(line, reason))?
            {
                return Ok(());
            }
        }
        self.position = end;
        let token = match name.as_str() {
            "" => return Ok(()),
            "__LINE__" => Token::new(INT, line, Some(line.to_string())),
            "__FILE__" => Token::new(STR, line, Some(self.file.clone())),
            "__DATE__" | "__TIME__" => {
                let (date, time) = self.when.get_or_insert_with(cpp::translation_time);
                Token::new(STR, line, Some(if name == "__DATE__" { date.clone() } else { time.clone() }))
            }
            "__STDC__" => Token::new(INT, line, Some("1".into())),
            "double" => return lex_fail(line, NO_DOUBLE),
            _ => {
                let mut token = Token::new(Kind::Name, line, Some(name));
                let doc = comment_block(&self.comments, line);
                if !doc.is_empty() {
                    token.doc = Some(doc);
                }
                token
            }
        };
        self.tokens.push(token);
        Ok(())
    }

    /// An operator: the longest of three characters, two or one.
    pub(super) fn operator(&mut self, c: char) -> Result<()> {
        let (text, position) = (&self.text, self.position);
        let (op, length) = if let Some(op) = LONG_OPERATORS.iter().find(|op| starts_with(text, position, op)) {
            (op.to_string(), 3)
        } else if let Some(op) = OPERATORS.iter().find(|op| starts_with(text, position, op)) {
            (op.to_string(), 2)
        } else if SINGLES.contains(c) {
            (c.to_string(), 1)
        } else {
            return lex_fail(self.line, format!("unexpected character {}", quoted(&c.to_string())));
        };
        self.push(Kind::Op, Some(op));
        self.position += length;
        Ok(())
    }
}
