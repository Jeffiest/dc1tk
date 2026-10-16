//! The C-like script language: the tokenizer, which is also the preprocessor (comments, trigraphs,
//! #if/#ifdef/#ifndef/#elif/#else/#endif, #define/#undef and macros, #include, #pragma, #line,
//! #error, asm blocks; see cpp.rs), and the recursive-descent parser that turns a file into a
//! program tree. The forms the decompiler writes parse to the tree it was written from, key for
//! key, so a decompiled script compiles back to its own bytes. C89's other forms parse to trees of
//! forms the compiler has, or to the ones it lowers (lower.rs, and composite.rs for C's pointers
//! and composite types, which declarations give their C types, ctype.rs); see docs/coding/c-standard.md.
//!
//! A #pragma's words are read as the assembly reads a line (asm::tokenize): a plain word is a
//! string and a quoted one {"string": text}.
//!
//! An error names the line it is on. An integer literal past 32 bits is an error, as C has no type
//! for it here, and so is nesting deeper than NESTING.
//!
//! The parts: lex (the tokens and the preprocessor), program (file scope: declarations of natives
//! and helpers, enums, constants, pragmas, and parse_state, which reads a file), declarations
//! (locals, globals, typedefs, parameters, function definitions), types (specifiers, structs,
//! declarators), statements and expressions. This module has the parser's State and the entry
//! points, parse and parse_with.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use indexmap::IndexMap;

use crate::cpp::{self, Macros};
use crate::ctype::{self, Composite, Structs, Ty};
use crate::node::Node;
use crate::{asm, stb, Error, Result};

mod declarations;
mod expressions;
mod lex;
mod program;
mod statements;
mod types;

use declarations::*;
use expressions::*;
use lex::*;
use program::*;
use statements::*;
use types::*;

pub use lex::{comment_block, Files};
pub use types::INTEGER_TYPES;

/// How deep expressions, statements, initializers and declarators may nest, counted where each
/// recursion of the parser passes (State::nested), and for each link of a chain the parser reads in
/// a loop but which nests in the tree (a + b + c is (a + b) + c; State::chain). Past it a file is
/// refused ("nested too deeply") rather than overflowing the stack: within it the parser keeps to
/// a 1 MB stack (a Windows main thread's) in a debug build, whose frames are the largest; the
/// functions on its way down are kept small for that. Parentheses take three of it each (an
/// assignment, a ternary and a unary expression), a block, if, loop or switch two (the statement
/// and itself), so parentheses nest 64 deep, statements 97, and an operator chain is 194 long;
/// a struct defined in a struct's members takes two (the struct and its members' declaration).
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

/// A text quoted for an error message: in single quotes, what cannot be shown escaped.
fn quoted(text: &str) -> String {
    format!("'{}'", text.escape_debug())
}

/// quoted, of the text's first 200 characters.
fn short_quoted(text: &str) -> String {
    quoted(&text.chars().take(200).collect::<String>())
}

/// A whole number in a base, with an optional sign: 16 takes an 0x before its digits, and 0
/// reads the base the prefix gives (0x, else decimal).
fn int_in_base(text: &str, base: u32) -> Result<i128> {
    let (negative, unsigned) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let hexadecimal = unsigned.strip_prefix("0x").or_else(|| unsigned.strip_prefix("0X"));
    let (radix, digits) = match (base, hexadecimal) {
        (0 | 16, Some(digits)) => (16, digits),
        (0, None) => (10, unsigned),
        _ => (base, unsigned),
    };
    if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
        let kind = match base {
            8 => "an octal",
            10 => "a decimal",
            16 => "a hexadecimal",
            _ => "a",
        };
        return Err(Error::invalid(format!("{} is not {kind} number", short_quoted(text))));
    }
    let value =
        i128::from_str_radix(digits, radix).map_err(|_| Error::invalid(format!("{text} is too large a number")))?;
    Ok(if negative { -value } else { value })
}

/// The character a \x escape's two hexadecimal digits give.
fn hex_escape(digits: &[char], line: usize) -> Result<char> {
    let text: String = digits.iter().collect();
    match digits {
        [high, low] if high.is_ascii_hexdigit() && low.is_ascii_hexdigit() => {
            Ok(char::from(u8::from_str_radix(&text, 16).expect("two hexadecimal digits")))
        }
        _ => lex_fail(line, format!("\\x takes two hexadecimal digits, not {}", short_quoted(&text))),
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
    /// (ParseOptions::headers): what a type's size and slots need. They are not type words here, as a
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

    /// A token's text quoted for a message; a directive's or asm block's token has none, and is
    /// unexpected wherever a message would quote one.
    fn shown(&self, token: &Token) -> Result<String> {
        match &token.text {
            Some(text) => Ok(quoted(text)),
            None => Err(Error::invalid(format!("line {}: unexpected {} token", token.line, token.kind.name()))),
        }
    }

    fn expect(&mut self, text: &str) -> Result<Taken> {
        if !self.at(text, 0) {
            let found = self.shown(self.token(0))?;
            return self.fail(format!("expected {}, found {found}", quoted(text)));
        }
        Ok(self.take())
    }

    /// The next token, which must be of a kind (its data aside).
    fn expect_kind(&mut self, kind: &Kind) -> Result<Taken> {
        if std::mem::discriminant(&self.token(0).kind) != std::mem::discriminant(kind) {
            let found = self.shown(self.token(0))?;
            return self.fail(format!("expected {}, found {found}", kind.name()));
        }
        Ok(self.take())
    }

    fn expect_int(&mut self) -> Result<Taken> {
        self.expect_kind(&INT)
    }

    fn name(&mut self) -> Result<String> {
        Ok(self.expect_kind(&Kind::Name)?.text().to_string())
    }

    /// Parses one level deeper, refusing to go past NESTING.
    #[inline(always)]
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

/// The name __FILE__ gives a file parsed without one.
pub const UNNAMED_FILE: &str = "<script>";

/// How a file is parsed (parse_with); the defaults are parse's.
#[derive(Clone, Copy)]
pub struct ParseOptions<'a> {
    /// The name __FILE__ gives until a #line names another (UNNAMED_FILE).
    pub file: &'a str,
    /// #defines in force before the first line (a mod patch's LANGUAGE); none.
    pub defines: Option<&'a HashMap<String, String>>,
    /// The headers #include <...> reads as text, by path, as a compiler reads them: their macros
    /// reach the file's #if, and their typedefs, structs and constants its declarations. None
    /// reads no file an #include names.
    pub headers: Option<&'a IndexMap<String, String>>,
    /// The libraries #include "..." reads (library::include_texts), whose macros, typedefs and
    /// structs reach the file as C includes them. They are read with the headers.
    pub libraries: Option<&'a IndexMap<String, String>>,
}

impl Default for ParseOptions<'_> {
    fn default() -> Self {
        ParseOptions { file: UNNAMED_FILE, defines: None, headers: None, libraries: None }
    }
}

/// The program a script's source holds, read alone.
pub fn parse(text: &str) -> Result<Node> {
    parse_with(text, &ParseOptions::default())
}

/// The program a source file holds, with its name, #defines and included files.
pub fn parse_with(text: &str, options: &ParseOptions) -> Result<Node> {
    let none = HashMap::new();
    let defines = options.defines.unwrap_or(&none);
    let headers = options.headers.map(|headers| Files { headers, libraries: options.libraries });
    Ok(parse_state(text, defines, options.file, headers)?.0)
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
        let program = parse_with(text, &ParseOptions { defines: Some(&defines), ..ParseOptions::default() }).unwrap();
        assert_eq!(program["globals"].to_string(), r#"[{"name":"fr","type":"int"}]"#);
        // C's precedence: 3 & (1 == 1), (!2) == 1; / truncates
        for (condition, value) in [("3 & 1 == 1", true), ("!2 == 1", false), ("-7 / 2 == -3", true), ("1 && 0", false)]
        {
            let globals = dump(&format!("#if {condition}\nint yes;\n#endif\n"));
            assert_eq!(globals.contains("\"yes\""), value, "{condition}");
        }
    }

    #[test]
    fn a_headers_macros_are_cached_by_its_whole_text() {
        // the cache is keyed by the header's path and text (and the macros before it), compared
        // whole: another text under the same path is read again
        let text = "#include <h.h>
#if FLAG
int yes;
#endif
";
        let globals = |header: &str| {
            let headers: IndexMap<String, String> = [("h.h".to_string(), header.to_string())].into();
            let program = parse_with(text, &ParseOptions { headers: Some(&headers), ..ParseOptions::default() });
            program.unwrap()["globals"].to_string()
        };
        assert_eq!(
            globals(
                "#define FLAG 1
"
            ),
            r#"[{"name":"yes","type":"int"}]"#
        );
        assert_eq!(
            globals(
                "#define FLAG 0
"
            ),
            "[]"
        );
        assert_eq!(
            globals(
                "#define FLAG 1
"
            ),
            r#"[{"name":"yes","type":"int"}]"#
        );
    }

    #[test]
    fn a_comment_after_a_directive_is_a_space() {
        // C reads comments before directives: one after #include or #pragma is not part of what
        // the directive takes, and a /* comment may run on over lines
        let headers: IndexMap<String, String> = [("h.h".to_string(), "#define FLAG 1\n".to_string())].into();
        let parsed = |text: &str| {
            parse_with(text, &ParseOptions { headers: Some(&headers), ..ParseOptions::default() }).unwrap().to_string()
        };
        let body = "#pragma std c89\n#define N 2\n#ifdef N\n#pragma override\nint f() { return FLAG + N; }\n#endif\n";
        let plain = parsed(&format!("#include <h.h>\n\n{body}"));
        for text in [
            "#include <h.h> // FLAG\n\n#pragma std c89 // strict\n#define N 2 // two\n#ifdef N // c\n\
             #pragma override   // Replace f\nint f() { return FLAG + N; }\n#endif // N\n",
            "#include <h.h> /* FLAG */\n\n#pragma std /* c */ c89\n#define N /* c */ 2\n#ifdef N /* c */\n\
             #pragma override /* Replace f */\nint f() { return FLAG + N; }\n#endif\n",
            &format!("#include <h.h> /* FLAG,\n   over two lines */\n{body}"),
            &format!("#include <h.h> // FLAG, and a backslash \\\n   splices this line to the comment\n{body}"),
        ] {
            assert_eq!(parsed(text), plain, "{text:?}");
        }
        let program =
            parse("#pragma target map s51 // the castle\n#pragma override place // why\nvoid f() {}\n").unwrap();
        assert_eq!(program["targets"].to_string(), r#"[{"kind":"map","values":["s51"]}]"#);
        assert_eq!(program["functions"][0]["override_of"], "place");
        assert_eq!(error("#include <h.h> /* open\nint a;\n"), "line 1: unterminated comment");
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
                let structs = |count: usize| {
                    let opened: String = (0..count).map(|level| format!("struct s{level} {{ ")).collect();
                    let closed: String = (0..count).rev().map(|level| format!("}} m{level}; ")).collect();
                    format!("struct t {{ {opened}int z; {closed}}};")
                };
                deepest(&structs, 98);
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
