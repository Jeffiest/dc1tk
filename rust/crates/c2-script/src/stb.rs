//! The .stb script format: a header, the program table (event numbers to functions), and the code
//! section of functions, each a FUNCDATA record, VM instructions and the strings they push. What
//! the reader cannot place is kept as raw bytes, so every file reads and writes back unchanged.

use c2_disc::text;

use crate::{Error, Result};

pub const HEADER_SIZE: usize = 0x40;
pub const HEADER_FIELDS_SIZE: usize = 0x1C;
pub const PROGDATA_SIZE: usize = 8;
pub const FUNCDATA_SIZE: usize = 0x38;
pub const FUNCDATA_ARGS: usize = 10;
pub const VMCODE_SIZE: usize = 12;

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

pub const OP_FUNC_END: i32 = 0;
pub const OP_PUSH_CONST: i32 = 3;
pub const CONST_STR: i32 = 3;

/// The opcode's name, or "UNKNOWN" for a number the VM does not have.
pub fn op_name(op: i32) -> &'static str {
    usize::try_from(op).ok().and_then(|op| OPCODES.get(op)).copied().unwrap_or("UNKNOWN")
}

pub fn op_number(name: &str) -> Option<i32> {
    OPCODES.iter().position(|op| *op == name).map(|op| op as i32)
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
    pub fn new(name: &str, arg1: i32, arg2: i32) -> Instruction {
        Instruction { op: op_number(name).expect("a known opcode name"), arg1, arg2 }
    }

    pub fn name(&self) -> &'static str {
        op_name(self.op)
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
        if !(0..OPCODES.len() as i32).contains(&item.op) {
            return None;
        }
        instructions.push(item);
        position += VMCODE_SIZE;
        if item.op == OP_FUNC_END {
            return Some(instructions);
        }
    }
    None
}

/// Where the strings a function's instructions push are, from the start of the file.
pub fn string_refs(code: usize, instructions: &[Instruction]) -> Vec<i64> {
    instructions
        .iter()
        .filter(|item| item.op == OP_PUSH_CONST && item.arg1 == CONST_STR)
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
