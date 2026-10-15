//! The .stb script format: a header, the program table (event numbers to functions), and the code
//! section of functions, each a FUNCDATA record, VM instructions and the strings they push. What
//! the reader cannot place is kept as raw bytes, so every file reads and writes back unchanged.

use dc2tk_disc::text;

use crate::{Error, Result};

pub const HEADER_SIZE: usize = 0x40;
pub const HEADER_FIELDS_SIZE: usize = 0x1C;
pub const PROGDATA_SIZE: usize = 8;
pub const FUNCDATA_SIZE: usize = 0x38;
pub const FUNCDATA_ARGS: usize = 10;
pub const VMCODE_SIZE: usize = 12;

/// The VM's instructions, by opcode (runscript.cpp's names, which OPCODES lists in order).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Op {
    FuncEnd,
    Load,
    LoadAddr,
    PushConst,
    Pop,
    Store,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Neg,
    Itof,
    Ftoi,
    Cmp,
    Ret,
    Jmp,
    JmpFalse,
    JmpTrue,
    Call,
    Print,
    Ext,
    Nop,
    Wait,
    And,
    Or,
    Not,
    End,
    SkipEnd,
    Sin,
    Cos,
}

/// The opcodes' names, by number.
pub const OPCODES: [&str; 31] = [
    "FUNC_END",
    "LOAD",
    "LOAD_ADDR",
    "PUSH_CONST",
    "POP",
    "STORE",
    "ADD",
    "SUB",
    "MUL",
    "DIV",
    "MOD",
    "NEG",
    "ITOF",
    "FTOI",
    "CMP",
    "RET",
    "JMP",
    "JMP_FALSE",
    "JMP_TRUE",
    "CALL",
    "PRINT",
    "EXT",
    "NOP",
    "WAIT",
    "AND",
    "OR",
    "NOT",
    "END",
    "SKIP_END",
    "SIN",
    "COS",
];

/// The binary operators' instructions, with the C operators they are.
const BINARY: [(Op, &str); 7] =
    [(Op::Add, "+"), (Op::Sub, "-"), (Op::Mul, "*"), (Op::Div, "/"), (Op::Mod, "%"), (Op::And, "&"), (Op::Or, "|")];

impl Op {
    /// Every opcode, by number.
    pub const ALL: [Op; 31] = [
        Op::FuncEnd,
        Op::Load,
        Op::LoadAddr,
        Op::PushConst,
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
        Op::Cmp,
        Op::Ret,
        Op::Jmp,
        Op::JmpFalse,
        Op::JmpTrue,
        Op::Call,
        Op::Print,
        Op::Ext,
        Op::Nop,
        Op::Wait,
        Op::And,
        Op::Or,
        Op::Not,
        Op::End,
        Op::SkipEnd,
        Op::Sin,
        Op::Cos,
    ];

    /// The opcode a number is, if the VM has it.
    pub fn from_number(number: i32) -> Option<Op> {
        usize::try_from(number).ok().and_then(|number| Op::ALL.get(number)).copied()
    }

    pub fn number(self) -> i32 {
        self as i32
    }

    pub fn name(self) -> &'static str {
        OPCODES[self as usize]
    }

    pub fn from_name(name: &str) -> Option<Op> {
        OPCODES.iter().position(|op| *op == name).map(|number| Op::ALL[number])
    }

    /// The assembly's mnemonic of an instruction with no operands or a count (pop, add, nop...):
    /// its name in lower case.
    pub fn mnemonic(self) -> String {
        self.name().to_lowercase()
    }

    /// The instruction of a binary C operator the VM has one for (+ - * / % & |).
    pub fn binary(operator: &str) -> Option<Op> {
        BINARY.iter().find(|(_, known)| *known == operator).map(|(op, _)| *op)
    }

    /// The C operator a binary instruction is.
    pub fn binary_operator(self) -> Option<&'static str> {
        BINARY.iter().find(|(op, _)| *op == self).map(|(_, operator)| *operator)
    }
}

/// CMP's comparisons (its arg1), as the assembly names them and C writes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compare {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// Each comparison's number, the assembly's name and C's operator.
const COMPARES: [(Compare, i32, &str, &str); 6] = [
    (Compare::Eq, 40, "eq", "=="),
    (Compare::Ne, 41, "ne", "!="),
    (Compare::Lt, 42, "lt", "<"),
    (Compare::Le, 43, "le", "<="),
    (Compare::Gt, 44, "gt", ">"),
    (Compare::Ge, 45, "ge", ">="),
];

impl Compare {
    fn entry(self) -> (Compare, i32, &'static str, &'static str) {
        *COMPARES.iter().find(|entry| entry.0 == self).expect("every comparison is listed")
    }

    pub fn number(self) -> i32 {
        self.entry().1
    }

    pub fn name(self) -> &'static str {
        self.entry().2
    }

    pub fn operator(self) -> &'static str {
        self.entry().3
    }

    pub fn from_number(number: i32) -> Option<Compare> {
        COMPARES.iter().find(|entry| entry.1 == number).map(|entry| entry.0)
    }

    pub fn from_name(name: &str) -> Option<Compare> {
        COMPARES.iter().find(|entry| entry.2 == name).map(|entry| entry.0)
    }

    pub fn from_operator(operator: &str) -> Option<Compare> {
        COMPARES.iter().find(|entry| entry.3 == operator).map(|entry| entry.0)
    }
}

/// Where a LOAD or LOAD_ADDR's slot is (its addressing mode, arg2, says, with the value's type).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Place {
    /// A local's slot (a parameter is a local).
    Local,
    /// An element of a local array: the index popped, added to the slot.
    Array,
    /// An element through the pointer a local holds: the index popped, added to the pointer.
    Pointer,
    /// A global's slot.
    Global,
}

/// A LOAD or LOAD_ADDR's addressing mode: where the slot is, and whether its value is read as a
/// float (an int there is converted). The assembly names it local, local_idx, ptr_idx or global,
/// with _f for a float.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Mode {
    pub place: Place,
    pub float: bool,
}

impl Mode {
    /// Every mode, with its bit.
    pub const ALL: [(Mode, i32); 8] = [
        (Mode { place: Place::Local, float: false }, 0x1),
        (Mode { place: Place::Array, float: false }, 0x2),
        (Mode { place: Place::Pointer, float: false }, 0x4),
        (Mode { place: Place::Local, float: true }, 0x8),
        (Mode { place: Place::Array, float: true }, 0x10),
        (Mode { place: Place::Pointer, float: true }, 0x20),
        (Mode { place: Place::Global, float: false }, 0x40),
        (Mode { place: Place::Global, float: true }, 0x200),
    ];

    pub fn new(place: Place, float: bool) -> Mode {
        Mode { place, float }
    }

    /// A slot of a value of a type: float for "float", int for any other.
    pub fn of(place: Place, kind: &str) -> Mode {
        Mode::new(place, kind == "float")
    }

    pub fn bit(self) -> i32 {
        Mode::ALL.iter().find(|(mode, _)| *mode == self).map(|(_, bit)| *bit).expect("every mode has a bit")
    }

    pub fn from_bit(bit: i32) -> Option<Mode> {
        Mode::ALL.iter().find(|(_, known)| *known == bit).map(|(mode, _)| *mode)
    }

    pub fn name(self) -> &'static str {
        match (self.place, self.float) {
            (Place::Local, false) => "local",
            (Place::Array, false) => "local_idx",
            (Place::Pointer, false) => "ptr_idx",
            (Place::Global, false) => "global",
            (Place::Local, true) => "local_f",
            (Place::Array, true) => "local_idx_f",
            (Place::Pointer, true) => "ptr_idx_f",
            (Place::Global, true) => "global_f",
        }
    }

    pub fn from_name(name: &str) -> Option<Mode> {
        Mode::ALL.iter().map(|(mode, _)| *mode).find(|mode| mode.name() == name)
    }

    pub fn is_float(self) -> bool {
        self.float
    }

    /// Whether the instruction pops an index (an element of an array or through a pointer).
    pub fn is_indexed(self) -> bool {
        matches!(self.place, Place::Array | Place::Pointer)
    }

    /// The type of the value the slot is read as.
    pub fn kind(self) -> &'static str {
        if self.float {
            "float"
        } else {
            "int"
        }
    }
}

/// PUSH_CONST's kinds of constant (its arg1): an int, a float's bits, or a string by its offset
/// from the code section.
pub const CONST_INT: i32 = 1;
pub const CONST_FLOAT: i32 = 2;
pub const CONST_STR: i32 = 3;

/// The opcode's name, or "UNKNOWN" for a number the VM does not have.
pub fn op_name(op: i32) -> &'static str {
    Op::from_number(op).map_or("UNKNOWN", Op::name)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub magic: [u8; 4],
    pub main: i32,
    pub code: i32,
    pub prog: i32,
    pub prog_num: i32,
    pub unk_14: i32,
    pub global_num: i32,
    /// The 36 bytes after the fields.
    pub padding: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProgramEntry {
    pub no: i32,
    pub func: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Instruction {
    pub op: i32,
    pub arg1: i32,
    pub arg2: i32,
}

impl Instruction {
    pub fn new(op: Op, arg1: i32, arg2: i32) -> Instruction {
        Instruction { op: op.number(), arg1, arg2 }
    }

    /// The instruction's opcode; None for a number the VM does not have (which only an assembly
    /// .op line can give).
    pub fn opcode(&self) -> Option<Op> {
        Op::from_number(self.op)
    }

    pub fn name(&self) -> &'static str {
        op_name(self.op)
    }

    /// The addressing mode of a LOAD or LOAD_ADDR (None for another instruction, or a mode the VM
    /// does not have).
    pub fn mode(&self) -> Option<Mode> {
        matches!(self.opcode(), Some(Op::Load | Op::LoadAddr)).then(|| Mode::from_bit(self.arg2)).flatten()
    }

    /// Whether the instruction pushes an int constant.
    pub fn is_int(&self) -> bool {
        self.opcode() == Some(Op::PushConst) && self.arg1 == CONST_INT
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Function {
    pub offset: usize,
    pub size: usize,
    pub addr: i32,
    pub name: i32,
    pub local: i32,
    pub arg: i32,
    pub arg_types: Vec<i32>,
    pub instructions: Vec<Instruction>,
    pub strings: Vec<String>,
    pub padding: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Region {
    Header(Header),
    Programs { offset: usize, size: usize, entries: Vec<ProgramEntry> },
    Function(Function),
    Raw { offset: usize, bytes: Vec<u8> },
}

impl Region {
    pub fn offset(&self) -> usize {
        match self {
            Region::Header(_) => 0,
            Region::Programs { offset, .. } | Region::Raw { offset, .. } => *offset,
            Region::Function(function) => function.offset,
        }
    }

    pub fn size(&self) -> usize {
        match self {
            Region::Header(_) => HEADER_SIZE,
            Region::Programs { size, .. } => *size,
            Region::Function(function) => function.size,
            Region::Raw { bytes, .. } => bytes.len(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Script {
    pub size: usize,
    pub header: Option<Header>,
    pub regions: Vec<Region>,
}

impl Script {
    pub fn functions(&self) -> impl Iterator<Item = &Function> {
        self.regions.iter().filter_map(|region| match region {
            Region::Function(function) => Some(function),
            _ => None,
        })
    }
}

pub fn word(data: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
}

fn read_header(data: &[u8]) -> Header {
    Header {
        magic: data[0..4].try_into().unwrap(),
        main: word(data, 4),
        code: word(data, 8),
        prog: word(data, 12),
        prog_num: word(data, 16),
        unk_14: word(data, 20),
        global_num: word(data, 24),
        padding: data[HEADER_FIELDS_SIZE..HEADER_SIZE].to_vec(),
    }
}

fn read_programs(data: &[u8], header: &Header) -> Option<Region> {
    let start = header.prog as i64;
    let count = header.prog_num as i64;
    let end = start + count * PROGDATA_SIZE as i64;
    if count <= 0 || start < HEADER_SIZE as i64 || end > header.code as i64 || end > data.len() as i64 {
        return None;
    }
    let start = start as usize;
    let entries = (0..count as usize)
        .map(|i| {
            let offset = start + i * PROGDATA_SIZE;
            ProgramEntry { no: word(data, offset), func: word(data, offset + 4) }
        })
        .collect();
    Some(Region::Programs { offset: start, size: end as usize - start, entries })
}

fn is_function(data: &[u8], code: usize, offset: usize) -> bool {
    if !offset.is_multiple_of(4) || offset + FUNCDATA_SIZE > data.len() {
        return false;
    }
    word(data, offset) as i64 == offset as i64 - code as i64 + FUNCDATA_SIZE as i64
}

fn next_function(data: &[u8], code: usize, offset: usize) -> usize {
    let mut position = offset.next_multiple_of(4);
    while position < data.len() && !is_function(data, code, position) {
        position += 4;
    }
    position.min(data.len())
}

fn read_instructions(data: &[u8], start: usize) -> Option<Vec<Instruction>> {
    let mut instructions = Vec::new();
    let mut position = start;
    while position + VMCODE_SIZE <= data.len() {
        let item =
            Instruction { op: word(data, position), arg1: word(data, position + 4), arg2: word(data, position + 8) };
        let op = Op::from_number(item.op)?;
        instructions.push(item);
        position += VMCODE_SIZE;
        if op == Op::FuncEnd {
            return Some(instructions);
        }
    }
    None
}

/// Where the strings a function's instructions push are, from the start of the file.
pub fn string_refs(code: usize, instructions: &[Instruction]) -> Vec<i64> {
    instructions
        .iter()
        .filter(|item| item.opcode() == Some(Op::PushConst) && item.arg1 == CONST_STR)
        .map(|item| code as i64 + item.arg2 as i64)
        .collect()
}

fn find_zero(data: &[u8], start: usize, end: usize) -> Option<usize> {
    data.get(start..end)?.iter().position(|&b| b == 0).map(|at| start + at)
}

fn read_strings(data: &[u8], start: usize, end: usize, refs: &[i64]) -> Option<(Vec<String>, Vec<u8>)> {
    let mut used = start;
    for &reference in refs {
        if start as i64 <= reference && reference < end as i64 {
            let terminator = find_zero(data, reference as usize, end)?;
            used = used.max(terminator + 1);
        }
    }
    let mut strings = Vec::new();
    let mut position = start;
    while position < used {
        let terminator = find_zero(data, position, used).expect("a string ends before the last one used");
        let raw = &data[position..terminator];
        let text = text::decode_strict(raw)?;
        if text::encode(&text).as_deref() != Some(raw) {
            return None;
        }
        strings.push(text);
        position = terminator + 1;
    }
    Some((strings, data[used..end].to_vec()))
}

fn read_function(data: &[u8], code: usize, offset: usize) -> Option<Function> {
    let instructions = read_instructions(data, offset + FUNCDATA_SIZE)?;
    let strings_start = offset + FUNCDATA_SIZE + instructions.len() * VMCODE_SIZE;
    let end = next_function(data, code, strings_start);
    let (strings, padding) = read_strings(data, strings_start, end, &string_refs(code, &instructions))?;
    Some(Function {
        offset,
        size: end - offset,
        addr: word(data, offset),
        name: word(data, offset + 4),
        local: word(data, offset + 8),
        arg: word(data, offset + 12),
        arg_types: (0..FUNCDATA_ARGS).map(|i| word(data, offset + 16 + i * 4)).collect(),
        instructions,
        strings,
        padding,
    })
}

fn raw(data: &[u8], start: usize, end: usize) -> Region {
    Region::Raw { offset: start, bytes: data[start..end].to_vec() }
}

fn read_code(data: &[u8], code: usize) -> Vec<Region> {
    let mut regions = Vec::new();
    let mut position = code;
    while position < data.len() {
        let end = if is_function(data, code, position) {
            if let Some(function) = read_function(data, code, position) {
                position += function.size;
                regions.push(Region::Function(function));
                continue;
            }
            next_function(data, code, position + 1)
        } else {
            next_function(data, code, position)
        };
        regions.push(raw(data, position, end));
        position = end;
    }
    regions
}

pub fn read(data: &[u8]) -> Script {
    if data.len() < HEADER_SIZE {
        let regions = if data.is_empty() { vec![] } else { vec![raw(data, 0, data.len())] };
        return Script { size: data.len(), header: None, regions };
    }
    let header = read_header(data);
    let mut regions = vec![Region::Header(header.clone())];
    let mut position = HEADER_SIZE;
    let code = if header.code < HEADER_SIZE as i32 || header.code as i64 > data.len() as i64 {
        data.len()
    } else {
        header.code as usize
    };
    if let Some(programs) = read_programs(data, &header) {
        if programs.offset() > position {
            regions.push(raw(data, position, programs.offset()));
        }
        position = programs.offset() + programs.size();
        regions.push(programs);
    }
    if code > position {
        regions.push(raw(data, position, code));
    }
    regions.extend(read_code(data, code));
    Script { size: data.len(), header: Some(header), regions }
}

pub fn encode_string(text: &str) -> Result<Vec<u8>> {
    let mut out = text::encode(text)
        .ok_or_else(|| Error::invalid(format!("{text:?} has a character the game's text cannot hold")))?;
    out.push(0);
    Ok(out)
}

fn put(out: &mut Vec<u8>, values: &[i32]) {
    for value in values {
        out.extend(value.to_le_bytes());
    }
}

pub fn encode(region: &Region) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    match region {
        Region::Header(header) => {
            out.extend(header.magic);
            put(&mut out, &[header.main, header.code, header.prog, header.prog_num, header.unk_14, header.global_num]);
            out.extend(&header.padding);
        }
        Region::Programs { entries, .. } => {
            for entry in entries {
                put(&mut out, &[entry.no, entry.func]);
            }
        }
        Region::Function(function) => {
            put(&mut out, &[function.addr, function.name, function.local, function.arg]);
            put(&mut out, &function.arg_types);
            for item in &function.instructions {
                put(&mut out, &[item.op, item.arg1, item.arg2]);
            }
            for text in &function.strings {
                out.extend(encode_string(text)?);
            }
            out.extend(&function.padding);
        }
        Region::Raw { bytes, .. } => out.extend(bytes),
    }
    Ok(out)
}

pub fn write(script: &Script) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    for region in &script.regions {
        if region.offset() != out.len() {
            return Err(Error::invalid(format!("region at 0x{:x} does not follow 0x{:x}", region.offset(), out.len())));
        }
        out.extend(encode(region)?);
    }
    Ok(out)
}

/// The share of the file read as something other than raw bytes.
pub fn coverage(script: &Script) -> f64 {
    if script.size == 0 {
        return 1.0;
    }
    let unknown: usize =
        script.regions.iter().filter(|region| matches!(region, Region::Raw { .. })).map(Region::size).sum();
    1.0 - unknown as f64 / script.size as f64
}

/// Zero bytes after the last function, filling the code section to a multiple of 16.
pub fn tail_padding(end: usize, code: usize) -> usize {
    (16 - (end - code) % 16) % 16
}

fn round16(size: usize) -> usize {
    size.next_multiple_of(16)
}

fn encoded_strings(strings: &[String]) -> Result<Vec<u8>> {
    let mut raw = Vec::new();
    for text in strings {
        raw.extend(encode_string(text)?);
    }
    Ok(raw)
}

// The original compiler kept one string buffer per file, zero-filled at the start. A function wrote
// its pushed strings into it, rounded the write offset up to 16 and wrote the buffer up to there
// after FUNC_END. Most files reset the offset for each function (block_padding: the strings, then
// whatever the buffer held up to the next 16); some never reset it, so every function writes the
// whole pool of the file's strings so far with its own after them (pool_block). After the last
// function the code section is zero-filled to a multiple of 16 (tail_padding).

/// The padding after a function's strings, and the buffer after writing them.
pub fn block_padding(previous: &[u8], strings: &[String]) -> Result<(Vec<u8>, Vec<u8>)> {
    let raw = encoded_strings(strings)?;
    if raw.is_empty() {
        return Ok((Vec::new(), previous.to_vec()));
    }
    let end = round16(raw.len());
    let mut buffer = raw.clone();
    if previous.len() > raw.len() {
        buffer.extend_from_slice(&previous[raw.len()..]);
    }
    if buffer.len() < end {
        buffer.resize(end, 0);
    }
    Ok((buffer[raw.len()..end].to_vec(), buffer))
}

/// A file's strings split at their zero ends.
pub fn split_strings(data: &[u8]) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut position = 0;
    while position < data.len() {
        let terminator =
            find_zero(data, position, data.len()).ok_or_else(|| Error::invalid("a string is not ended"))?;
        let raw = &data[position..terminator];
        out.push(text::decode_strict(raw).ok_or_else(|| Error::invalid("a string is not cp932"))?);
        position = terminator + 1;
    }
    Ok(out)
}

/// A function's block in a file whose string pool is never reset: the pool's strings, which come
/// before its own in the block; the padding after them; the pool after the function. A function
/// that pushes nothing writes the pool as padding, as the reader splits a block.
pub fn pool_block(pool: &[u8], strings: &[String]) -> Result<(Vec<String>, Vec<u8>, Vec<u8>)> {
    let raw = encoded_strings(strings)?;
    if raw.is_empty() {
        return Ok((Vec::new(), pool.to_vec(), pool.to_vec()));
    }
    let mut block = pool.to_vec();
    block.extend(raw);
    let fill = round16(block.len()) - block.len();
    let prefix = split_strings(pool)?;
    block.resize(block.len() + fill, 0);
    Ok((prefix, vec![0; fill], block))
}
