//! A small interpreter for the assembly the compiler writes, so code can be tried without the game:
//! the tests and c2 mod test use it. It follows the game's VM (runscript.cpp) in what script logic
//! needs: values keep their type (int, float, string, pointer), locals, arrays and pointers,
//! arithmetic with C's truncating division, comparisons, jumps and calls. Natives are not run: a
//! native call only takes its arguments off the stack, so a result it would give stays 0. print
//! records its values.

use std::collections::{BTreeMap, HashMap};

use crate::asm::{self, Token};
use crate::natives::Table;
use crate::node::Node;
use crate::{compile, sdk, Error, Result};

/// A value on the VM's stack or in a variable.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Int(i128),
    Float(f64),
    Text(String),
    /// A variable's address: its frame (None for the globals) and slot.
    Pointer(Option<usize>, usize),
}

impl Value {
    fn number(&self) -> Result<f64> {
        match self {
            Value::Int(value) => Ok(*value as f64),
            Value::Float(value) => Ok(*value),
            other => Err(Error::invalid(format!("not a number: {other:?}"))),
        }
    }

    fn int(&self) -> Result<i128> {
        match self {
            Value::Int(value) => Ok(*value),
            Value::Float(value) => Ok(*value as i128),
            other => Err(Error::invalid(format!("not a number: {other:?}"))),
        }
    }

    pub fn truthy(&self) -> bool {
        match self {
            Value::Int(value) => *value != 0,
            Value::Float(value) => *value != 0.0,
            Value::Text(text) => !text.is_empty(),
            Value::Pointer(..) => true,
        }
    }

    /// The value as Python's str() writes it.
    pub fn text(&self) -> String {
        match self {
            Value::Int(value) => value.to_string(),
            Value::Float(value) => c2_py::float_repr(*value),
            Value::Text(text) => text.clone(),
            Value::Pointer(frame, slot) => {
                format!("({}, {})", frame.map_or("globals".into(), |frame| frame.to_string()), slot)
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Function {
    pub locals: usize,
    pub args: usize,
    pub code: Vec<Vec<String>>,
    pub labels: HashMap<String, usize>,
    pub strings: Vec<String>,
}

pub type Functions = HashMap<String, Function>;

/// A program as the table of its functions.
pub fn table_of(program: &Node, tables: &BTreeMap<String, Table>) -> Result<Functions> {
    lines_table(&compile::to_asm(&sdk::resolve(program, tables)?, tables, None)?)
}

fn is_label(line: &str) -> Option<&str> {
    let label = line.strip_suffix(':')?;
    (label.starts_with("L_") && label.len() > 2 && label[2..].chars().all(c2_py::is_word)).then_some(label)
}

pub fn lines_table(text: &str) -> Result<Functions> {
    let mut found = Functions::new();
    let mut current: Option<String> = None;
    for line in text.split('\n') {
        let line = c2_py::strip(line);
        if line.starts_with(".function ") {
            let words = c2_py::split(line);
            let after = |key: &str| -> usize {
                words
                    .iter()
                    .position(|word| *word == key)
                    .and_then(|at| words.get(at + 1))
                    .and_then(|word| word.parse().ok())
                    .unwrap_or(0)
            };
            found.insert(
                words[1].to_string(),
                Function { locals: after("local"), args: after("arg"), ..Default::default() },
            );
            current = Some(words[1].to_string());
            continue;
        }
        let Some(name) = current.as_ref() else {
            continue;
        };
        if line.is_empty()
            || [".padding", ".natives", ".header", ".program", ".code"].iter().any(|start| line.starts_with(start))
        {
            continue;
        }
        let function = found.get_mut(name).unwrap();
        if line.starts_with(".string ") {
            if let Some(Token::Text(text)) = asm::tokenize(line)?.into_iter().nth(1) {
                function.strings.push(text);
            }
        } else if let Some(label) = is_label(line) {
            function.labels.insert(label.to_string(), function.code.len());
        } else {
            function.code.push(c2_py::split(line).into_iter().map(str::to_string).collect());
        }
    }
    Ok(found)
}

/// An int as the VM's 32 bits hold it.
fn wrapped(value: i128) -> i128 {
    value as i32 as i128
}

fn truncated(left: i128, right: i128) -> i128 {
    let quotient = left.abs() / right.abs();
    if (left < 0) == (right < 0) {
        quotient
    } else {
        -quotient
    }
}

fn arithmetic(op: &str, left: &Value, right: &Value) -> Result<Value> {
    if matches!(op, "div" | "mod") && right.number()? == 0.0 {
        return Err(Error::invalid("division by zero, which stops the game's VM"));
    }
    if let (Value::Int(a), Value::Int(b)) = (left, right) {
        // the game's ints are 32 bits, and its arithmetic wraps
        return Ok(Value::Int(wrapped(match op {
            "add" => a + b,
            "sub" => a - b,
            "mul" => a * b,
            "div" => truncated(*a, *b),
            "mod" => a - b * truncated(*a, *b),
            "and" => a & b,
            _ => a | b,
        })));
    }
    let (a, b) = (left.number()?, right.number()?);
    Ok(match op {
        "add" => Value::Float(a + b),
        "sub" => Value::Float(a - b),
        "mul" => Value::Float(a * b),
        "div" => Value::Float(a / b),
        "mod" => Value::Float(a - b * (a.abs() / b.abs()).floor() * if (a < 0.0) == (b < 0.0) { 1.0 } else { -1.0 }),
        "and" => Value::Int(left.int()? & right.int()?),
        _ => Value::Int(left.int()? | right.int()?),
    })
}

fn compare(op: &str, left: &Value, right: &Value) -> Result<bool> {
    let ordering = match (left, right) {
        (Value::Text(a), Value::Text(b)) => a.partial_cmp(b),
        _ => left.number()?.partial_cmp(&right.number()?),
    };
    use std::cmp::Ordering::*;
    Ok(match op {
        "eq" => ordering == Some(Equal),
        "ne" => ordering != Some(Equal),
        "lt" => ordering == Some(Less),
        "le" => matches!(ordering, Some(Less | Equal)),
        "gt" => ordering == Some(Greater),
        _ => matches!(ordering, Some(Greater | Equal)),
    })
}

/// The VM's variables: the globals and a frame of locals per running function.
pub struct Machine<'a> {
    pub table: &'a Functions,
    pub globals: Vec<Value>,
    frames: Vec<Vec<Value>>,
    pub printed: Vec<String>,
}

impl<'a> Machine<'a> {
    pub fn new(table: &'a Functions, globals: Vec<Value>) -> Machine<'a> {
        Machine { table, globals, frames: Vec::new(), printed: Vec::new() }
    }

    fn slot(&mut self, place: Option<usize>, index: usize) -> Result<&mut Value> {
        let cells = match place {
            None => &mut self.globals,
            Some(frame) => &mut self.frames[frame],
        };
        cells.get_mut(index).ok_or_else(|| Error::invalid("list index out of range"))
    }

    fn cell(&self, mode: &str, slot: usize, stack: &mut Vec<Value>) -> Result<(Option<usize>, usize)> {
        let frame = self.frames.len() - 1;
        if mode.starts_with("global") {
            return Ok((None, slot));
        }
        if mode.starts_with("local_idx") {
            let index = pop(stack)?.int()?;
            return Ok((Some(frame), (slot as i128 + index) as usize));
        }
        if mode.starts_with("ptr_idx") {
            let index = pop(stack)?.int()?;
            let Some(Value::Pointer(place, base)) = self.frames[frame].get(slot) else {
                return Err(Error::invalid("not a pointer"));
            };
            return Ok((*place, (*base as i128 + index) as usize));
        }
        Ok((Some(frame), slot))
    }

    /// Runs a function and returns its result.
    pub fn run(&mut self, name: &str, args: Vec<Value>) -> Result<Value> {
        let function = self.table.get(name).ok_or_else(|| Error::invalid(format!("'{name}'")))?;
        let mut frame = args;
        while frame.len() < function.locals {
            frame.push(Value::Int(0));
        }
        self.frames.push(frame);
        let result = self.execute(function);
        self.frames.pop();
        result
    }

    fn execute(&mut self, function: &Function) -> Result<Value> {
        let mut stack: Vec<Value> = Vec::new();
        let mut pc = 0;
        let label = |name: &str| function.labels.get(name).copied().ok_or_else(|| Error::invalid(format!("'{name}'")));
        loop {
            let words = function.code.get(pc).ok_or_else(|| Error::invalid("list index out of range"))?;
            pc += 1;
            let word = |at: usize| words.get(at).map(String::as_str).unwrap_or("");
            let number = |at: usize| -> Result<usize> {
                word(at).parse().map_err(|_| Error::invalid(format!("bad operand {}", word(at))))
            };
            match word(0) {
                "push" => {
                    let value = if word(1) == "string" {
                        Value::Text(
                            function
                                .strings
                                .get(number(2)?)
                                .cloned()
                                .ok_or_else(|| Error::invalid("list index out of range"))?,
                        )
                    } else if let Ok(value) = word(1).parse::<i128>() {
                        Value::Int(value)
                    } else {
                        // a constant is a 32-bit float, as the assembler writes it
                        let value = c2_py::parse_float(word(1)).ok_or_else(|| {
                            Error::invalid(format!("could not convert string to float: '{}'", word(1)))
                        })?;
                        Value::Float(value as f32 as f64)
                    };
                    stack.push(value);
                }
                op @ ("load" | "addr") => {
                    let (place, index) = self.cell(word(1), number(2)?, &mut stack)?;
                    if op == "addr" {
                        stack.push(Value::Pointer(place, index));
                    } else {
                        let mut value = self.slot(place, index)?.clone();
                        if word(1).ends_with("_f") {
                            if let Value::Int(int) = value {
                                value = Value::Float(int as f64);
                            }
                        }
                        stack.push(value);
                    }
                }
                "store" => {
                    let value = pop(&mut stack)?;
                    let Value::Pointer(place, index) = pop(&mut stack)? else {
                        return Err(Error::invalid("store to something that is not an address"));
                    };
                    *self.slot(place, index)? = value.clone();
                    stack.push(value);
                }
                "pop" => {
                    pop(&mut stack)?;
                }
                op @ ("add" | "sub" | "mul" | "div" | "mod" | "and" | "or") => {
                    let right = pop(&mut stack)?;
                    let left = pop(&mut stack)?;
                    stack.push(arithmetic(op, &left, &right)?);
                }
                "neg" => {
                    let value = match pop(&mut stack)? {
                        Value::Int(value) => Value::Int(wrapped(-value)),
                        Value::Float(value) => Value::Float(-value),
                        other => return Err(Error::invalid(format!("bad operand type for unary -: {other:?}"))),
                    };
                    stack.push(value);
                }
                "not" => {
                    let value = pop(&mut stack)?;
                    stack.push(Value::Int(!value.truthy() as i128));
                }
                "itof" => {
                    let value = pop(&mut stack)?.number()?;
                    stack.push(Value::Float(value));
                }
                "ftoi" => {
                    let value = pop(&mut stack)?.int()?;
                    stack.push(Value::Int(value));
                }
                "cmp" => {
                    let right = pop(&mut stack)?;
                    let left = pop(&mut stack)?;
                    stack.push(Value::Int(compare(word(1), &left, &right)? as i128));
                }
                "jmp" => pc = label(word(1))?,
                op @ ("jf" | "jt" | "jf_push" | "jt_push") => {
                    let truth = pop(&mut stack)?.truthy();
                    if truth == op.starts_with("jt") {
                        if op.ends_with("_push") {
                            stack.push(Value::Int(truth as i128));
                        }
                        pc = label(word(1))?;
                    }
                }
                "call" => {
                    let callee = self.table.get(word(1)).ok_or_else(|| Error::invalid(format!("'{}'", word(1))))?;
                    let at = stack.len().saturating_sub(callee.args);
                    let args = stack.split_off(at);
                    let result = self.run(word(1), args)?;
                    stack.push(result);
                }
                "ret" => return pop(&mut stack),
                op @ ("ext" | "print") => {
                    let at = stack.len().saturating_sub(number(1)?);
                    let values = stack.split_off(at);
                    if op == "print" {
                        self.printed.push(values.iter().map(Value::text).collect::<Vec<_>>().join(" "));
                    }
                }
                "nop" | "wait" | "skip_end" => {}
                "end" => return Ok(Value::Int(0)),
                op => return Err(Error::invalid(format!("the test VM does not run {op}"))),
            }
        }
    }
}

fn pop(stack: &mut Vec<Value>) -> Result<Value> {
    stack.pop().ok_or_else(|| Error::invalid("pop from empty list"))
}
