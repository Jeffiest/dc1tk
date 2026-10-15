//! Text assembly for .stb scripts: every byte of a file as a readable line, and back. Instructions
//! the assembly has no shorthand for are written as `.op OP ARG1 ARG2`, raw bytes as `.bytes`, so
//! any file disassembles and assembles back to itself.

use std::collections::{BTreeMap, HashMap, HashSet};

use dc2tk_py::{float_repr, format_g, parse_float, parse_int};

use crate::natives::Table;
use crate::stack;
use crate::stb::{self, Compare, Function, Header, Instruction, Mode, Op, ProgramEntry, Region, Script};
use crate::stb::{CONST_FLOAT, CONST_INT, CONST_STR};
use crate::{Error, Result};

/// The instructions written as their mnemonic alone (Op::mnemonic), their operands 0.
const PLAIN: [Op; 19] = [
    Op::Pop,
    Op::Store,
    Op::Add,
    Op::Sub,
    Op::Mul,
    Op::Div,
    Op::Mod,
    Op::Neg,
    Op::Itof,
    Op::Ftoi,
    Op::Ret,
    Op::Wait,
    Op::And,
    Op::Or,
    Op::Not,
    Op::End,
    Op::SkipEnd,
    Op::Sin,
    Op::Cos,
];

/// The instructions written as their mnemonic and a count (arg1).
const COUNTED: [Op; 3] = [Op::Print, Op::Ext, Op::Nop];

/// (opcode, keep) for each jump mnemonic: keep is arg2, 1 when the tested value stays pushed.
const JUMPS: [((Op, i32), &str); 5] = [
    ((Op::Jmp, 0), "jmp"),
    ((Op::JmpFalse, 0), "jf"),
    ((Op::JmpFalse, 1), "jf_push"),
    ((Op::JmpTrue, 0), "jt"),
    ((Op::JmpTrue, 1), "jt_push"),
];

fn default_header_padding() -> Vec<u8> {
    vec![0; stb::HEADER_SIZE - stb::HEADER_FIELDS_SIZE]
}

/// The instruction a mnemonic of PLAIN or COUNTED is.
fn named(table: &[Op], mnemonic: &str) -> Option<Op> {
    table.iter().copied().find(|op| op.mnemonic() == mnemonic)
}

/// A string as the assembly (and the decompiler's C) quotes it.
pub fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c as u32 == 0x7F => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The shortest decimal text (from 1 to 9 significant digits) that reads back as the float the
/// bits hold, as Python's repr writes it; None for NaN and infinities.
pub fn float_text(bits: i32) -> Option<String> {
    let value = f32::from_bits(bits as u32) as f64;
    if !value.is_finite() {
        return None;
    }
    for precision in 1..10 {
        let text = format_g(value, precision);
        let parsed: f64 = text.parse().expect("%g text is a float");
        if (parsed as f32).to_bits() == bits as u32 {
            return Some(float_repr(parsed));
        }
    }
    None
}

pub fn hex_text(value: i64) -> String {
    if value < 0 {
        format!("-0x{:x}", -value)
    } else {
        format!("0x{value:x}")
    }
}

pub fn function_label(offset: i64) -> String {
    format!("f_{offset:04x}")
}

pub fn jump_label(offset: i64) -> String {
    format!("L_{offset:04x}")
}

fn raw_op(item: &Instruction) -> String {
    format!(".op {} {} {}", item.op, item.arg1, item.arg2)
}

fn encoded_len(text: &str) -> usize {
    dc2tk_disc::text::encode(text).map_or(0, |bytes| bytes.len())
}

/// Where each of a function's strings starts in the file.
pub fn string_offsets(function: &Function) -> Vec<i64> {
    let mut position = (function.offset + stb::FUNCDATA_SIZE + function.instructions.len() * stb::VMCODE_SIZE) as i64;
    let mut offsets = Vec::with_capacity(function.strings.len());
    for text in &function.strings {
        offsets.push(position);
        position += encoded_len(text) as i64 + 1;
    }
    offsets
}

/// Whether the function pushes its strings once each, in the order they are stored.
pub fn strings_in_order(function: &Function, code: i64) -> bool {
    let refs: Vec<i64> = function
        .instructions
        .iter()
        .filter(|item| item.opcode() == Some(Op::PushConst) && item.arg1 == CONST_STR)
        .map(|item| code + item.arg2 as i64)
        .collect();
    refs == string_offsets(function)
}

pub fn native_positions(instructions: &[Instruction], args: &HashMap<i64, i32>) -> HashSet<usize> {
    let mut positions = HashSet::new();
    for (index, item) in instructions.iter().enumerate() {
        if item.opcode() == Some(Op::Ext) {
            if let Some(position) = stack::native_push(instructions, index, args) {
                if instructions[position].is_int() {
                    positions.insert(position);
                }
            }
        }
    }
    positions
}

struct Context<'a> {
    bounds: HashSet<i64>,
    functions: &'a HashSet<i64>,
    natives: &'a BTreeMap<i32, &'a crate::natives::Native>,
    ordered: bool,
    strings: std::collections::VecDeque<String>,
    string_index: HashMap<i64, usize>,
    native: bool,
}

fn instruction_text(item: &Instruction, context: &mut Context) -> String {
    let Some(op) = item.opcode() else {
        return raw_op(item);
    };
    let (arg1, arg2) = (item.arg1, item.arg2);
    if PLAIN.contains(&op) {
        return if arg1 == 0 && arg2 == 0 { op.mnemonic() } else { raw_op(item) };
    }
    if COUNTED.contains(&op) {
        return if arg2 == 0 { format!("{} {arg1}", op.mnemonic()) } else { raw_op(item) };
    }
    match op {
        Op::Load | Op::LoadAddr => match Mode::from_bit(arg2) {
            Some(mode) => format!("{} {} {}", if op == Op::Load { "load" } else { "addr" }, mode.name(), arg1),
            None => raw_op(item),
        },
        Op::Cmp => match Compare::from_number(arg1) {
            Some(compare) if arg2 == 0 => format!("cmp {}", compare.name()),
            _ => raw_op(item),
        },
        Op::Jmp | Op::JmpFalse | Op::JmpTrue => match JUMPS.iter().find(|(jump, _)| *jump == (op, arg2)) {
            Some((_, mnemonic)) if context.bounds.contains(&(arg1 as i64)) => {
                format!("{} {}", mnemonic, jump_label(arg1 as i64))
            }
            _ => raw_op(item),
        },
        Op::Call => {
            if arg1 != 0 || !context.functions.contains(&(arg2 as i64)) {
                raw_op(item)
            } else {
                format!("call {}", function_label(arg2 as i64))
            }
        }
        Op::PushConst => {
            if arg1 == CONST_INT && context.native {
                return match context.natives.get(&arg2) {
                    Some(known) => format!("native {}", known.name),
                    None => format!("native {arg2}"),
                };
            }
            if arg1 == CONST_INT {
                return format!("push {arg2}");
            }
            if arg1 == CONST_FLOAT {
                return match float_text(arg2) {
                    Some(text) => format!("push {text}"),
                    None => raw_op(item),
                };
            }
            if arg1 == CONST_STR && context.ordered {
                let text = context.strings.pop_front().unwrap_or_default();
                return format!("push {}", quote(&text));
            }
            if arg1 == CONST_STR {
                if let Some(index) = context.string_index.get(&(arg2 as i64)) {
                    return format!("push string {index}");
                }
            }
            raw_op(item)
        }
        _ => raw_op(item),
    }
}

pub fn function_text(
    function: &Function,
    code: i64,
    functions: &HashSet<i64>,
    natives: &BTreeMap<i32, &crate::natives::Native>,
    args: &HashMap<i64, i32>,
) -> Vec<String> {
    let start = (function.offset + stb::FUNCDATA_SIZE) as i64 - code;
    let instructions = &function.instructions;
    let at = |index: usize| start + (index * stb::VMCODE_SIZE) as i64;
    let bounds: HashSet<i64> = (0..instructions.len()).map(at).collect();
    let targets: HashSet<i64> = instructions
        .iter()
        .filter(|item| {
            matches!(item.opcode(), Some(Op::Jmp | Op::JmpFalse | Op::JmpTrue)) && bounds.contains(&(item.arg1 as i64))
        })
        .map(|item| item.arg1 as i64)
        .collect();
    let mut header = format!(
        ".function {} local {} arg {}",
        function_label(function.offset as i64 - code),
        function.local,
        function.arg
    );
    let mut types = function.arg_types.clone();
    while types.last() == Some(&0) {
        types.pop();
    }
    if !types.is_empty() {
        header += &format!(" types {}", types.iter().map(i32::to_string).collect::<Vec<_>>().join(" "));
    }
    if function.name != 0 {
        header += &format!(" name {}", function.name);
    }
    let mut lines = vec![header];
    let mut context = Context {
        bounds,
        functions,
        natives,
        ordered: strings_in_order(function, code),
        strings: function.strings.iter().cloned().collect(),
        string_index: string_offsets(function)
            .into_iter()
            .enumerate()
            .map(|(index, offset)| (offset - code, index))
            .collect(),
        native: false,
    };
    let pushes = native_positions(instructions, args);
    let last = instructions.len().wrapping_sub(1);
    for (index, item) in instructions.iter().enumerate() {
        if targets.contains(&at(index)) {
            lines.push(format!("{}:", jump_label(at(index))));
        }
        if index == last && item.arg1 == 0 && item.arg2 == 0 {
            continue;
        }
        context.native = pushes.contains(&index);
        lines.push(format!("    {}", instruction_text(item, &mut context)));
    }
    for text in &function.strings {
        lines.push(format!(".string {}", quote(text)));
    }
    if !function.padding.is_empty() {
        lines.push(format!(".padding {}", hex(&function.padding)));
    }
    lines
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn magic_text(magic: &[u8; 4]) -> String {
    if magic[3] == 0 && magic[..3].iter().all(u8::is_ascii_alphanumeric) {
        return String::from_utf8_lossy(&magic[..3]).into_owned();
    }
    format!("0x{}", hex(magic))
}

fn header_text(header: &Header, size: usize, functions: &HashSet<i64>, has_programs: bool) -> String {
    let code = header.code as i64;
    let mut line = format!(".header {}", magic_text(&header.magic));
    if functions.contains(&(header.main as i64 - code)) {
        line += &format!(" main {}", function_label(header.main as i64 - code));
    } else {
        line += &format!(" main {}", hex_text(header.main as i64));
    }
    line += &format!(" globals {}", header.global_num);
    if !(stb::HEADER_SIZE as i64 <= code && code <= size as i64) {
        line += &format!(" code {}", hex_text(code));
    }
    if !has_programs {
        line += &format!(" prog {} prog_num {}", hex_text(header.prog as i64), header.prog_num);
    }
    if header.unk_14 != 0 {
        line += &format!(" unk_14 {}", header.unk_14);
    }
    if header.padding != default_header_padding() {
        line += &format!(" padding {}", hex(&header.padding));
    }
    line
}

/// The assembly text of a script, calling natives by their names in the table.
pub fn disassemble(script: &Script, table: &str, tables: &BTreeMap<String, Table>) -> String {
    let Some(header) = &script.header else {
        let lines: Vec<String> = script
            .regions
            .iter()
            .map(|region| match region {
                Region::Raw { bytes, .. } => format!(".bytes {}", hex(bytes)),
                _ => unreachable!("a file without a header is all raw bytes"),
            })
            .collect();
        return format!("{}\n", lines.join("\n"));
    };
    let code = header.code as i64;
    let code_position =
        if stb::HEADER_SIZE as i64 <= code && code <= script.size as i64 { code as usize } else { script.size };
    let functions: HashSet<i64> = script.functions().map(|function| function.offset as i64 - code).collect();
    let has_programs = script.regions.iter().any(|region| matches!(region, Region::Programs { .. }));
    let numbered = tables.get(table).map(Table::numbered).unwrap_or_default();
    let args = stack::function_args(script);
    let mut lines = vec![format!(".natives {table}")];
    let mut code_marked = false;
    for region in &script.regions {
        if !code_marked && region.offset() >= code_position {
            lines.push(".code".into());
            code_marked = true;
        }
        match region {
            Region::Header(_) => lines.push(header_text(header, script.size, &functions, has_programs)),
            Region::Programs { entries, .. } => {
                for entry in entries {
                    let target = entry.func as i64 - code;
                    let to =
                        if functions.contains(&target) { function_label(target) } else { hex_text(entry.func as i64) };
                    lines.push(format!(".program {} {}", entry.no, to));
                }
            }
            Region::Raw { bytes, .. } => lines.push(format!(".bytes {}", hex(bytes))),
            Region::Function(function) => {
                lines.push(String::new());
                lines.extend(function_text(function, code, &functions, &numbered, &args));
            }
        }
    }
    if !code_marked {
        lines.push(".code".into());
    }
    format!("{}\n", lines.join("\n"))
}

// ---- assembling ----------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    Word(String),
    Text(String),
}

impl Token {
    fn word(&self) -> Option<&str> {
        match self {
            Token::Word(word) => Some(word),
            Token::Text(_) => None,
        }
    }
}

fn unescape(c: char) -> Option<char> {
    match c {
        '\\' => Some('\\'),
        '"' => Some('"'),
        'n' => Some('\n'),
        'r' => Some('\r'),
        't' => Some('\t'),
        _ => None,
    }
}

pub fn tokenize(line: &str) -> Result<Vec<Token>> {
    if !line.contains('"') {
        return Ok(dc2tk_py::split(line).into_iter().map(|word| Token::Word(word.to_string())).collect());
    }
    let chars: Vec<char> = line.chars().collect();
    let mut tokens = Vec::new();
    let mut position = 0;
    let bad = || Error::invalid(format!("a string in {line:?} is not ended"));
    while position < chars.len() {
        let c = chars[position];
        if dc2tk_py::is_space(c) {
            position += 1;
            continue;
        }
        if c != '"' {
            let start = position;
            while position < chars.len() && !dc2tk_py::is_space(chars[position]) {
                position += 1;
            }
            tokens.push(Token::Word(chars[start..position].iter().collect()));
            continue;
        }
        let mut text = String::new();
        position += 1;
        loop {
            let c = *chars.get(position).ok_or_else(bad)?;
            if c == '"' {
                break;
            }
            if c == '\\' {
                let escape = *chars.get(position + 1).ok_or_else(bad)?;
                if escape == 'x' {
                    let digits: String = chars.get(position + 2..position + 4).ok_or_else(bad)?.iter().collect();
                    let value = u32::from_str_radix(&digits, 16)
                        .map_err(|_| Error::invalid(format!("bad \\x escape in {line:?}")))?;
                    text.push(char::from_u32(value).unwrap());
                    position += 4;
                    continue;
                }
                text.push(
                    unescape(escape).ok_or_else(|| Error::invalid(format!("bad escape \\{escape} in {line:?}")))?,
                );
                position += 2;
                continue;
            }
            text.push(c);
            position += 1;
        }
        tokens.push(Token::Text(text));
        position += 1;
    }
    Ok(tokens)
}

pub fn number(token: &str) -> Result<i64> {
    parse_int(token).ok_or_else(|| Error::invalid(format!("{token:?} is not a number")))
}

fn number32(token: &str) -> Result<i32> {
    i32::try_from(number(token)?).map_err(|_| Error::invalid(format!("{token} does not fit in 32 bits")))
}

pub fn is_float(token: &str) -> bool {
    !token.starts_with("0x") && !token.starts_with("-0x") && (token.contains('.') || token.contains(['e', 'E']))
}

pub fn float_bits(token: &str) -> Result<i32> {
    let value = parse_float(token).ok_or_else(|| Error::invalid(format!("{token:?} is not a number")))?;
    let single = value as f32;
    if single.is_infinite() && value.is_finite() {
        return Err(Error::invalid(format!("{token} is too large for a float")));
    }
    Ok(single.to_bits() as i32)
}

#[derive(Clone, Debug, Default)]
struct Options {
    pairs: HashMap<String, String>,
    types: Option<Vec<i32>>,
}

impl Options {
    fn get(&self, key: &str) -> Option<&str> {
        self.pairs.get(key).map(String::as_str)
    }

    fn need(&self, key: &str, line: usize) -> Result<&str> {
        self.get(key).ok_or_else(|| Error::invalid(format!("line {line}: no {key} given")))
    }
}

fn options(tokens: &[Token], line: usize) -> Result<Options> {
    let mut found = Options::default();
    let mut position = 0;
    let word = |token: &Token| {
        token
            .word()
            .map(str::to_string)
            .ok_or_else(|| Error::invalid(format!("line {line}: a string where a word goes")))
    };
    while position < tokens.len() {
        let key = word(&tokens[position])?;
        if key == "types" {
            let mut values = Vec::new();
            position += 1;
            while let Some(Some(token)) = tokens.get(position).map(Token::word) {
                let digits = token.trim_start_matches('-');
                if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
                    break;
                }
                values.push(number32(token)?);
                position += 1;
            }
            found.types = Some(values);
            continue;
        }
        let value =
            tokens.get(position + 1).ok_or_else(|| Error::invalid(format!("line {line}: {key} has no value")))?;
        found.pairs.insert(key, word(value)?);
        position += 2;
    }
    Ok(found)
}

fn magic_bytes(token: &str) -> Result<[u8; 4]> {
    let bytes = if let Some(digits) = token.strip_prefix("0x") {
        (0..digits.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(digits.get(at..at + 2).unwrap_or("x"), 16))
            .collect::<std::result::Result<Vec<u8>, _>>()
            .map_err(|_| Error::invalid(format!("bad magic {token}")))?
    } else {
        let mut bytes = token.as_bytes().to_vec();
        bytes.push(0);
        bytes
    };
    bytes.try_into().map_err(|_| Error::invalid(format!("the magic {token} is not 4 bytes")))
}

#[derive(Clone, Debug)]
struct Line {
    mnemonic: String,
    operands: Vec<Token>,
    line: usize,
}

#[derive(Clone, Debug)]
struct FunctionItem {
    label: String,
    settings: Options,
    instructions: Vec<Line>,
    labels: Vec<(String, usize)>,
    strings: Vec<String>,
    padding: Vec<u8>,
    offset: usize,
}

#[derive(Clone, Debug)]
enum Item {
    Header { offset: usize },
    Program { no: i64, target: String, offset: usize },
    Raw { bytes: Vec<u8>, offset: usize },
    Code { offset: usize },
    Function(FunctionItem),
}

struct Program {
    table: Option<String>,
    header: Option<(String, Options)>,
    tail: bool,
    items: Vec<Item>,
}

fn unhex(text: &str, line: usize) -> Result<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return Err(Error::invalid(format!("line {line}: odd number of hex digits")));
    }
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16))
        .collect::<std::result::Result<Vec<u8>, _>>()
        .map_err(|_| Error::invalid(format!("line {line}: not hex")))
}

fn parse(text: &str) -> Result<Program> {
    let mut program = Program { table: None, header: None, tail: false, items: Vec::new() };
    let mut function: Option<usize> = None;
    for (index, line) in text.split('\n').enumerate() {
        let number_line = index + 1;
        let tokens = tokenize(line)?;
        let Some(head) = tokens.first() else {
            continue;
        };
        let Some(head) = head.word() else {
            return Err(Error::invalid(format!("line {number_line}: a line cannot start with a string")));
        };
        let rest = &tokens[1..];
        let first_word = || {
            rest.first()
                .and_then(Token::word)
                .ok_or_else(|| Error::invalid(format!("line {number_line}: {head} needs an operand")))
        };
        let current = |function: Option<usize>| {
            function.ok_or_else(|| Error::invalid(format!("line {number_line}: {head} outside a function")))
        };
        match head {
            ".natives" => program.table = Some(first_word()?.to_string()),
            ".header" => {
                program.header = Some((first_word()?.to_string(), options(&rest[1..], number_line)?));
                program.items.push(Item::Header { offset: 0 });
            }
            ".program" => {
                let target = rest
                    .get(1)
                    .and_then(Token::word)
                    .ok_or_else(|| Error::invalid(format!("line {number_line}: .program needs a target")))?;
                program.items.push(Item::Program { no: number(first_word()?)?, target: target.to_string(), offset: 0 });
            }
            ".bytes" => program.items.push(Item::Raw { bytes: unhex(first_word()?, number_line)?, offset: 0 }),
            ".tail" => program.tail = true,
            ".code" => program.items.push(Item::Code { offset: 0 }),
            ".function" => {
                program.items.push(Item::Function(FunctionItem {
                    label: first_word()?.to_string(),
                    settings: options(&rest[1..], number_line)?,
                    instructions: Vec::new(),
                    labels: Vec::new(),
                    strings: Vec::new(),
                    padding: Vec::new(),
                    offset: 0,
                }));
                function = Some(program.items.len() - 1);
            }
            ".string" => {
                let text = match rest.first() {
                    Some(Token::Text(text)) => text.clone(),
                    _ => return Err(Error::invalid(format!("line {number_line}: .string needs a quoted string"))),
                };
                function_at(&mut program, current(function)?).strings.push(text);
            }
            ".padding" => {
                let bytes = unhex(first_word()?, number_line)?;
                function_at(&mut program, current(function)?).padding = bytes;
            }
            _ if head.ends_with(':') && tokens.len() == 1 => {
                let item = function_at(&mut program, current(function)?);
                let at = item.instructions.len();
                let label = head[..head.len() - 1].to_string();
                match item.labels.iter_mut().find(|(name, _)| *name == label) {
                    Some(existing) => existing.1 = at,
                    None => item.labels.push((label, at)),
                }
            }
            _ => {
                let line = Line { mnemonic: head.to_string(), operands: rest.to_vec(), line: number_line };
                function_at(&mut program, current(function)?).instructions.push(line);
            }
        }
    }
    Ok(program)
}

fn function_at(program: &mut Program, index: usize) -> &mut FunctionItem {
    match &mut program.items[index] {
        Item::Function(function) => function,
        _ => unreachable!("the index of a function item"),
    }
}

fn needs_end(function: &FunctionItem) -> bool {
    match function.instructions.last() {
        None => true,
        Some(last) => {
            !(last.mnemonic == ".op"
                && last.operands.first().and_then(Token::word).and_then(parse_int) == Some(Op::FuncEnd.number() as i64))
        }
    }
}

fn instruction_count(function: &FunctionItem) -> usize {
    function.instructions.len() + usize::from(needs_end(function))
}

fn function_size(function: &FunctionItem) -> usize {
    let strings: usize = function.strings.iter().map(|text| encoded_len(text) + 1).sum();
    stb::FUNCDATA_SIZE + instruction_count(function) * stb::VMCODE_SIZE + strings + function.padding.len()
}

struct Places {
    code: usize,
    symbols: HashMap<String, usize>,
    size: usize,
}

fn layout(program: &mut Program) -> Places {
    let mut offset = 0;
    let mut code = None;
    let mut symbols = HashMap::new();
    for item in &mut program.items {
        match item {
            Item::Header { offset: at } => {
                *at = offset;
                offset += stb::HEADER_SIZE;
            }
            Item::Program { offset: at, .. } => {
                *at = offset;
                offset += stb::PROGDATA_SIZE;
            }
            Item::Raw { bytes, offset: at } => {
                *at = offset;
                offset += bytes.len();
            }
            Item::Code { offset: at } => {
                *at = offset;
                code = Some(offset);
            }
            Item::Function(function) => {
                function.offset = offset;
                symbols.insert(function.label.clone(), offset);
                for (label, index) in &function.labels {
                    symbols.insert(label.clone(), offset + stb::FUNCDATA_SIZE + index * stb::VMCODE_SIZE);
                }
                offset += function_size(function);
            }
        }
    }
    if program.tail && matches!(program.items.last(), Some(Item::Function(_))) {
        let fill = stb::tail_padding(offset, code.unwrap_or(0));
        if let Some(Item::Function(last)) = program.items.last_mut() {
            last.padding.extend(std::iter::repeat_n(0, fill));
        }
        offset += fill;
    }
    Places { code: code.unwrap_or(offset), symbols, size: offset }
}

fn address(token: &str, places: &Places, relative: usize) -> Result<i32> {
    match places.symbols.get(token) {
        Some(&at) => Ok((at as i64 - relative as i64) as i32),
        None => number32(token),
    }
}

fn word_operand(item: &Line, index: usize) -> Result<&str> {
    item.operands
        .get(index)
        .and_then(Token::word)
        .ok_or_else(|| Error::invalid(format!("line {}: {} is missing an operand", item.line, item.mnemonic)))
}

fn encode_instruction(
    item: &Line,
    places: &Places,
    names: &HashMap<&str, i32>,
    strings: &[usize],
    queue: &mut std::collections::VecDeque<usize>,
) -> Result<Instruction> {
    let mnemonic = item.mnemonic.as_str();
    let code = places.code as i64;
    let unknown = |what: &str| Error::invalid(format!("line {}: unknown {} {}", item.line, what, mnemonic));
    if mnemonic == ".op" {
        return Ok(Instruction {
            op: number32(word_operand(item, 0)?)?,
            arg1: number32(word_operand(item, 1)?)?,
            arg2: number32(word_operand(item, 2)?)?,
        });
    }
    if let Some(op) = named(&PLAIN, mnemonic) {
        return Ok(Instruction::new(op, 0, 0));
    }
    if let Some(op) = named(&COUNTED, mnemonic) {
        return Ok(Instruction::new(op, number32(word_operand(item, 0)?)?, 0));
    }
    match mnemonic {
        "load" | "addr" => {
            let mode = Mode::from_name(word_operand(item, 0)?).ok_or_else(|| unknown("mode for"))?;
            let op = if mnemonic == "load" { Op::Load } else { Op::LoadAddr };
            Ok(Instruction::new(op, number32(word_operand(item, 1)?)?, mode.bit()))
        }
        "cmp" => {
            let compare = Compare::from_name(word_operand(item, 0)?).ok_or_else(|| unknown("compare"))?;
            Ok(Instruction::new(Op::Cmp, compare.number(), 0))
        }
        "call" => Ok(Instruction::new(Op::Call, 0, address(word_operand(item, 0)?, places, places.code)?)),
        "native" => {
            let token = word_operand(item, 0)?;
            let no = match names.get(token) {
                Some(&no) => no,
                None => number32(token)?,
            };
            Ok(Instruction::new(Op::PushConst, CONST_INT, no))
        }
        "push" => match item.operands.first() {
            Some(Token::Text(_)) => {
                let at = queue.pop_front().ok_or_else(|| unknown("string pushed by"))?;
                Ok(Instruction::new(Op::PushConst, CONST_STR, (at as i64 - code) as i32))
            }
            Some(Token::Word(token)) if token == "string" => {
                let index = number(word_operand(item, 1)?)?;
                let at = usize::try_from(index)
                    .ok()
                    .and_then(|index| strings.get(index))
                    .ok_or_else(|| Error::invalid(format!("line {}: no string {}", item.line, index)))?;
                Ok(Instruction::new(Op::PushConst, CONST_STR, (*at as i64 - code) as i32))
            }
            Some(Token::Word(token)) if is_float(token) => {
                Ok(Instruction::new(Op::PushConst, CONST_FLOAT, float_bits(token)?))
            }
            Some(Token::Word(token)) => Ok(Instruction::new(Op::PushConst, CONST_INT, number32(token)?)),
            None => Err(unknown("operand for")),
        },
        _ => match JUMPS.iter().find(|(_, name)| *name == mnemonic) {
            Some(((op, keep), _)) => {
                Ok(Instruction::new(*op, address(word_operand(item, 0)?, places, places.code)?, *keep))
            }
            None => Err(Error::invalid(format!("line {}: unknown instruction {}", item.line, mnemonic))),
        },
    }
}

fn build_function(function: &FunctionItem, places: &Places, names: &HashMap<&str, i32>) -> Result<Function> {
    let settings = &function.settings;
    let offset = function.offset;
    let mut arg_types = settings.types.clone().unwrap_or_default();
    if arg_types.len() > stb::FUNCDATA_ARGS {
        return Err(Error::invalid(format!("{} has more than {} types", function.label, stb::FUNCDATA_ARGS)));
    }
    arg_types.resize(stb::FUNCDATA_ARGS, 0);
    let code_start = offset + stb::FUNCDATA_SIZE;
    let strings_start = code_start + instruction_count(function) * stb::VMCODE_SIZE;
    let mut strings = Vec::with_capacity(function.strings.len());
    let mut position = strings_start;
    for text in &function.strings {
        strings.push(position);
        position += encoded_len(text) + 1;
    }
    let mut queue: std::collections::VecDeque<usize> = strings.iter().copied().collect();
    let mut instructions = function
        .instructions
        .iter()
        .map(|item| encode_instruction(item, places, names, &strings, &mut queue))
        .collect::<Result<Vec<_>>>()?;
    if needs_end(function) {
        instructions.push(Instruction::new(Op::FuncEnd, 0, 0));
    }
    let line = 0;
    Ok(Function {
        offset,
        size: function_size(function),
        addr: (code_start as i64 - places.code as i64) as i32,
        name: number32(settings.get("name").unwrap_or("0"))?,
        local: number32(settings.need("local", line)?)?,
        arg: number32(settings.need("arg", line)?)?,
        arg_types,
        instructions,
        strings: function.strings.clone(),
        padding: function.padding.clone(),
    })
}

fn build_header(program: &Program, places: &Places) -> Result<Header> {
    let (magic, settings) = program.header.as_ref().expect("a header item has header settings");
    let programs: Vec<usize> = program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Program { offset, .. } => Some(*offset),
            _ => None,
        })
        .collect();
    let given = |key: &str, otherwise: i64| -> Result<i32> {
        match settings.get(key) {
            Some(token) => number32(token),
            None => Ok(otherwise as i32),
        }
    };
    Ok(Header {
        magic: magic_bytes(magic)?,
        main: address(settings.need("main", 0)?, places, 0)?,
        code: given("code", places.code as i64)?,
        prog: given("prog", programs.first().copied().unwrap_or(0) as i64)?,
        prog_num: given("prog_num", programs.len() as i64)?,
        unk_14: given("unk_14", 0)?,
        global_num: number32(settings.need("globals", 0)?)?,
        padding: match settings.get("padding") {
            Some(text) => unhex(text, 0)?,
            None => default_header_padding(),
        },
    })
}

pub fn to_script(text: &str, tables: &BTreeMap<String, Table>) -> Result<Script> {
    let mut program = parse(text)?;
    let places = layout(&mut program);
    let names: HashMap<&str, i32> = program
        .table
        .as_ref()
        .and_then(|table| tables.get(table))
        .map(|table| table.natives.iter().map(|native| (native.name.as_str(), native.no)).collect())
        .unwrap_or_default();
    let mut regions: Vec<Region> = Vec::new();
    // every .program line goes into the one table, made at the first
    let mut programs: Option<usize> = None;
    for item in &program.items {
        match item {
            Item::Header { .. } => regions.push(Region::Header(build_header(&program, &places)?)),
            Item::Program { no, target, offset } => {
                let entry = ProgramEntry { no: *no as i32, func: address(target, &places, 0)? };
                match programs {
                    Some(at) => {
                        if let Region::Programs { entries, size, .. } = &mut regions[at] {
                            entries.push(entry);
                            *size += stb::PROGDATA_SIZE;
                        }
                    }
                    None => {
                        programs = Some(regions.len());
                        regions.push(Region::Programs {
                            offset: *offset,
                            size: stb::PROGDATA_SIZE,
                            entries: vec![entry],
                        });
                    }
                }
            }
            Item::Raw { bytes, offset } => regions.push(Region::Raw { offset: *offset, bytes: bytes.clone() }),
            Item::Code { .. } => {}
            Item::Function(function) => regions.push(Region::Function(build_function(function, &places, &names)?)),
        }
    }
    let header = match regions.first() {
        Some(Region::Header(header)) => Some(header.clone()),
        _ => None,
    };
    Ok(Script { size: places.size, header, regions })
}

pub fn assemble(text: &str, tables: &BTreeMap<String, Table>) -> Result<Vec<u8>> {
    stb::write(&to_script(text, tables)?)
}
