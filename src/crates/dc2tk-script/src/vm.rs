//! A small interpreter for the assembly the compiler writes, so code can be tried without the game:
//! the tests and dc2tk mod test use it. It follows the game's VM (runscript.cpp) in what script logic
//! needs: values keep their type (int, float, string, pointer), locals, arrays and pointers,
//! arithmetic with C's truncating division, comparisons, jumps and calls. Natives are not run: a
//! native call only takes its arguments off the stack, so a result it would give stays 0. print
//! records its values.
//!
//! A run stops with an error past vm::STEPS instructions (a test that loops for ever) or past
//! vm::CALLS nested calls (one that calls itself without end); calls are the machine's own, so a
//! deep one never overflows Rust's stack. An error names the function it stopped in.

use std::collections::{BTreeMap, HashMap};

use crate::asm::{self, Token};
use crate::natives::Table;
use crate::node::Node;
use crate::stb::{Compare, Mode, Op, Place};
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
    fn number(&self) -> std::result::Result<f64, String> {
        match self {
            Value::Int(value) => Ok(*value as f64),
            Value::Float(value) => Ok(*value),
            other => Err(format!("{} is not a number", other.shown())),
        }
    }

    fn int(&self) -> std::result::Result<i128, String> {
        match self {
            Value::Int(value) => Ok(*value),
            Value::Float(value) => Ok(*value as i128),
            other => Err(format!("{} is not a number", other.shown())),
        }
    }

    /// The value for an error message: a string quoted, a pointer as one.
    fn shown(&self) -> String {
        match self {
            Value::Text(text) => format!("{text:?}"),
            Value::Pointer(..) => "a pointer".into(),
            other => other.text(),
        }
    }

    pub fn is_true(&self) -> bool {
        match self {
            Value::Int(value) => *value != 0,
            Value::Float(value) => *value != 0.0,
            Value::Text(text) => !text.is_empty(),
            Value::Pointer(..) => true,
        }
    }

    /// The value as print shows it: a float as the shortest text that reads back as it, a pointer
    /// as the variable it points at (&globals[3], or &frame1[2] for a local of the second frame).
    pub fn text(&self) -> String {
        match self {
            Value::Int(value) => value.to_string(),
            Value::Float(value) => format!("{value:?}"),
            Value::Text(text) => text.clone(),
            Value::Pointer(frame, slot) => match frame {
                Some(frame) => format!("&frame{frame}[{slot}]"),
                None => format!("&globals[{slot}]"),
            },
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
    let word = |c: char| c.is_alphanumeric() || c == '_';
    (label.starts_with("L_") && label.len() > 2 && label[2..].chars().all(word)).then_some(label)
}

pub fn lines_table(text: &str) -> Result<Functions> {
    let mut found = Functions::new();
    let mut current: Option<String> = None;
    for line in text.split('\n') {
        let line = line.trim();
        if line.starts_with(".function ") {
            let words: Vec<&str> = line.split_whitespace().collect();
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
            function.code.push(line.split_whitespace().map(str::to_string).collect());
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

/// add, sub, mul, div, mod, and, or: ints wrap to 32 bits, and a float operand makes the others
/// floats (but for and and or, which take ints).
fn arithmetic(op: Op, left: &Value, right: &Value) -> std::result::Result<Value, String> {
    if matches!(op, Op::Div | Op::Mod) && right.number()? == 0.0 {
        return Err("division by zero, which stops the game's VM".into());
    }
    if let (Value::Int(a), Value::Int(b)) = (left, right) {
        // the game's ints are 32 bits, and its arithmetic wraps
        return Ok(Value::Int(wrapped(match op {
            Op::Add => a + b,
            Op::Sub => a - b,
            Op::Mul => a * b,
            Op::Div => truncated(*a, *b),
            Op::Mod => a - b * truncated(*a, *b),
            Op::And => a & b,
            _ => a | b,
        })));
    }
    let (a, b) = (left.number()?, right.number()?);
    Ok(match op {
        Op::Add => Value::Float(a + b),
        Op::Sub => Value::Float(a - b),
        Op::Mul => Value::Float(a * b),
        Op::Div => Value::Float(a / b),
        Op::Mod => Value::Float(a - b * (a.abs() / b.abs()).floor() * if (a < 0.0) == (b < 0.0) { 1.0 } else { -1.0 }),
        Op::And => Value::Int(left.int()? & right.int()?),
        _ => Value::Int(left.int()? | right.int()?),
    })
}

fn compare(compare: Compare, left: &Value, right: &Value) -> std::result::Result<bool, String> {
    use std::cmp::Ordering::*;
    let ordering = match (left, right) {
        (Value::Text(a), Value::Text(b)) => a.partial_cmp(b),
        _ => left.number()?.partial_cmp(&right.number()?),
    };
    Ok(match compare {
        Compare::Eq => ordering == Some(Equal),
        Compare::Ne => ordering != Some(Equal),
        Compare::Lt => ordering == Some(Less),
        Compare::Le => matches!(ordering, Some(Less | Equal)),
        Compare::Gt => ordering == Some(Greater),
        Compare::Ge => matches!(ordering, Some(Greater | Equal)),
    })
}

/// How many instructions a run may take: a test that loops for ever stops here.
pub const STEPS: u64 = 10_000_000;

/// How deep calls may nest: a function that calls itself without end stops here. Calls are the
/// machine's own (a call does not recurse in Rust), so this is about the VM's memory, not the
/// stack's.
pub const CALLS: usize = 10_000;

/// A function running: where it is and its values on the stack. Its locals are the machine's last
/// frame.
struct Call<'a> {
    name: &'a str,
    function: &'a Function,
    pc: usize,
    stack: Vec<Value>,
}

/// What an instruction asks of the machine after it.
enum Flow<'a> {
    Next,
    Call(&'a str, Vec<Value>),
    Return(Value),
}

/// The VM's variables: the globals and a frame of locals per running function.
pub struct Machine<'a> {
    pub table: &'a Functions,
    pub globals: Vec<Value>,
    frames: Vec<Vec<Value>>,
    pub printed: Vec<String>,
    /// How many instructions a run may take (STEPS), and how deep its calls may nest (CALLS).
    pub max_steps: u64,
    pub max_calls: usize,
    /// The instructions the current run has taken.
    steps: u64,
}

impl<'a> Machine<'a> {
    pub fn new(table: &'a Functions, globals: Vec<Value>) -> Machine<'a> {
        Machine {
            table,
            globals,
            frames: Vec::new(),
            printed: Vec::new(),
            max_steps: STEPS,
            max_calls: CALLS,
            steps: 0,
        }
    }

    /// Runs a function and returns its result. A run that takes more than max_steps instructions,
    /// or nests calls deeper than max_calls, is stopped with an error.
    pub fn run(&mut self, name: &str, args: Vec<Value>) -> Result<Value> {
        self.steps = 0;
        let base = self.frames.len();
        let result = self.run_calls(name, args);
        self.frames.truncate(base);
        result
    }

    fn run_calls(&mut self, name: &str, args: Vec<Value>) -> Result<Value> {
        let table: &'a Functions = self.table;
        let (entry, _) = table.get_key_value(name).ok_or_else(|| Error::invalid(format!("unknown function {name}")))?;
        let mut calls = vec![self.call(entry, args)?];
        loop {
            self.steps += 1;
            if self.steps > self.max_steps {
                let steps = self.max_steps;
                return Err(Error::invalid(format!("{entry} ran more than {steps} steps; does it loop for ever?")));
            }
            let call = calls.last_mut().expect("a call is running");
            let name = call.name;
            let flow = self.step(call).map_err(|reason| Error::invalid(format!("{name}: {reason}")))?;
            match flow {
                Flow::Next => {}
                Flow::Call(callee, args) => {
                    if calls.len() >= self.max_calls {
                        let deepest = self.max_calls;
                        return Err(Error::invalid(format!("calls nested deeper than {deepest} (in {callee})")));
                    }
                    let call = self.call(callee, args)?;
                    calls.push(call);
                }
                Flow::Return(value) => {
                    calls.pop();
                    self.frames.pop();
                    match calls.last_mut() {
                        Some(caller) => caller.stack.push(value),
                        None => return Ok(value),
                    }
                }
            }
        }
    }

    /// A call of a function the table has, its frame made: the arguments, then 0 in the rest of
    /// its locals.
    fn call(&mut self, name: &'a str, args: Vec<Value>) -> Result<Call<'a>> {
        let table: &'a Functions = self.table;
        let function = table.get(name).ok_or_else(|| Error::invalid(format!("unknown function {name}")))?;
        let mut frame = args;
        while frame.len() < function.locals {
            frame.push(Value::Int(0));
        }
        self.frames.push(frame);
        Ok(Call { name, function, pc: 0, stack: Vec::new() })
    }

    fn slot(&mut self, place: Option<usize>, index: usize) -> std::result::Result<&mut Value, String> {
        let (cells, what) = match place {
            None => (&mut self.globals, "the globals"),
            Some(frame) => (&mut self.frames[frame], "its frame"),
        };
        let count = cells.len();
        cells.get_mut(index).ok_or_else(|| format!("slot {index} is outside {what} ({count} slots)"))
    }

    /// The variable an addressing mode reaches: its frame (None for the globals) and slot.
    fn cell(
        &self,
        mode: Mode,
        slot: usize,
        stack: &mut Vec<Value>,
    ) -> std::result::Result<(Option<usize>, usize), String> {
        let frame = self.frames.len() - 1;
        match mode.place {
            Place::Global => Ok((None, slot)),
            Place::Local => Ok((Some(frame), slot)),
            Place::Array => {
                let index = pop(stack)?.int()?;
                Ok((Some(frame), (slot as i128 + index) as usize))
            }
            Place::Pointer => {
                let index = pop(stack)?.int()?;
                let Some(Value::Pointer(place, base)) = self.frames[frame].get(slot) else {
                    return Err(format!("slot {slot} holds no pointer to index through"));
                };
                Ok((*place, (*base as i128 + index) as usize))
            }
        }
    }

    /// Runs the call's next instruction.
    fn step(&mut self, call: &mut Call<'a>) -> std::result::Result<Flow<'a>, String> {
        let function = call.function;
        let words = function.code.get(call.pc).ok_or("it ran past its last instruction")?;
        call.pc += 1;
        let word = |at: usize| words.get(at).map_or("", String::as_str);
        let number = |at: usize| -> std::result::Result<usize, String> {
            word(at).parse().map_err(|_| format!("bad operand {}", word(at)))
        };
        let label = |name: &str| function.labels.get(name).copied().ok_or_else(|| format!("no label {name}"));
        let stack = &mut call.stack;
        match word(0) {
            "push" => stack.push(constant(function, word(1), word(2))?),
            op @ ("load" | "addr") => {
                let mode = Mode::from_name(word(1)).ok_or_else(|| format!("unknown mode {}", word(1)))?;
                let (place, index) = self.cell(mode, number(2)?, stack)?;
                if op == "addr" {
                    stack.push(Value::Pointer(place, index));
                } else {
                    let mut value = self.slot(place, index)?.clone();
                    if let (true, Value::Int(int)) = (mode.is_float(), &value) {
                        value = Value::Float(*int as f64);
                    }
                    stack.push(value);
                }
            }
            "store" => {
                let value = pop(stack)?;
                let Value::Pointer(place, index) = pop(stack)? else {
                    return Err("store to something that is not an address".into());
                };
                *self.slot(place, index)? = value.clone();
                stack.push(value);
            }
            "pop" => {
                pop(stack)?;
            }
            "cmp" => {
                let compare_as =
                    Compare::from_name(word(1)).ok_or_else(|| format!("unknown comparison {}", word(1)))?;
                let right = pop(stack)?;
                let left = pop(stack)?;
                stack.push(Value::Int(compare(compare_as, &left, &right)? as i128));
            }
            "jmp" => call.pc = label(word(1))?,
            op @ ("jf" | "jt" | "jf_push" | "jt_push") => {
                let truth = pop(stack)?.is_true();
                if truth == op.starts_with("jt") {
                    if op.ends_with("_push") {
                        stack.push(Value::Int(truth as i128));
                    }
                    call.pc = label(word(1))?;
                }
            }
            "call" => {
                let table: &'a Functions = self.table;
                let (callee, found) =
                    table.get_key_value(word(1)).ok_or_else(|| format!("unknown function {}", word(1)))?;
                let at =
                    stack.len().checked_sub(found.args).ok_or_else(|| format!("stack underflow calling {callee}"))?;
                return Ok(Flow::Call(callee, stack.split_off(at)));
            }
            "ret" => return Ok(Flow::Return(pop(stack)?)),
            op @ ("ext" | "print") => {
                let count = number(1)?;
                let at = stack.len().checked_sub(count).ok_or("stack underflow")?;
                let values = stack.split_off(at);
                if op == "print" {
                    self.printed.push(values.iter().map(Value::text).collect::<Vec<_>>().join(" "));
                }
            }
            "nop" | "wait" | "skip_end" => {}
            "end" => return Ok(Flow::Return(Value::Int(0))),
            mnemonic => match Op::ALL.into_iter().find(|op| op.mnemonic() == mnemonic) {
                Some(op) => unary_or_arithmetic(op, stack)?,
                None => return Err(format!("the test VM does not run {mnemonic}")),
            },
        }
        Ok(Flow::Next)
    }
}

/// The value push gives: an int, a float (a 32-bit one, as the assembler writes it), or one of the
/// function's strings (push string N).
fn constant(function: &Function, word: &str, index: &str) -> std::result::Result<Value, String> {
    if word == "string" {
        let at: usize = index.parse().map_err(|_| format!("bad operand {index}"))?;
        return function.strings.get(at).cloned().map(Value::Text).ok_or_else(|| format!("no string {at}"));
    }
    if let Ok(value) = word.parse::<i128>() {
        return Ok(Value::Int(value));
    }
    let value: f64 = word.parse().map_err(|_| format!("push {word}: not a number"))?;
    Ok(Value::Float(value as f32 as f64))
}

/// The instructions of one or two values: neg, not, itof, ftoi, and the arithmetic.
fn unary_or_arithmetic(op: Op, stack: &mut Vec<Value>) -> std::result::Result<(), String> {
    let value = match op {
        Op::Neg => match pop(stack)? {
            Value::Int(value) => Value::Int(wrapped(-value)),
            Value::Float(value) => Value::Float(-value),
            other => return Err(format!("neg takes a number, not {}", other.shown())),
        },
        Op::Not => Value::Int(!pop(stack)?.is_true() as i128),
        Op::Itof => Value::Float(pop(stack)?.number()?),
        Op::Ftoi => Value::Int(pop(stack)?.int()?),
        Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod | Op::And | Op::Or => {
            let right = pop(stack)?;
            let left = pop(stack)?;
            arithmetic(op, &left, &right)?
        }
        other => return Err(format!("the test VM does not run {}", other.mnemonic())),
    };
    stack.push(value);
    Ok(())
}

fn pop(stack: &mut Vec<Value>) -> std::result::Result<Value, String> {
    stack.pop().ok_or_else(|| "stack underflow".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_shows_floats_shortest_and_pointers_as_their_variable() {
        assert_eq!(Value::Float(0.1).text(), "0.1");
        assert_eq!(Value::Float(2.0).text(), "2.0");
        assert_eq!(Value::Float(1.5e-7).text(), "1.5e-7");
        assert_eq!(Value::Pointer(None, 3).text(), "&globals[3]");
        assert_eq!(Value::Pointer(Some(1), 2).text(), "&frame1[2]");
    }

    fn run(text: &str, name: &str, limits: impl FnOnce(&mut Machine)) -> Result<Value> {
        let table = lines_table(text).unwrap();
        let mut machine = Machine::new(&table, vec![Value::Int(0); 4]);
        limits(&mut machine);
        machine.run(name, Vec::new())
    }

    #[test]
    fn a_loop_without_end_stops_after_its_steps() {
        let endless = ".function test_loop local 0 arg 0\nL_0:\n    nop 0\n    jmp L_0\n";
        let error = run(endless, "test_loop", |machine| machine.max_steps = 1000).unwrap_err();
        assert_eq!(error.to_string(), "test_loop ran more than 1000 steps; does it loop for ever?");
    }

    #[test]
    fn calls_nested_past_the_limit_are_an_error_not_a_crash() {
        // the machine's calls do not recurse in Rust: the limit holds on a small stack too
        let thread = std::thread::Builder::new().stack_size(256 << 10);
        let error = thread
            .spawn(|| run(".function f local 0 arg 0\n    call f\n    ret\n", "f", |_| {}).unwrap_err().to_string())
            .unwrap()
            .join()
            .unwrap();
        assert_eq!(error, format!("calls nested deeper than {CALLS} (in f)"));
        let countdown = ".function f local 1 arg 1\n    load local 0\n    jf L_0\n    load local 0\n    push 1\n    \
                         sub\n    call f\n    ret\nL_0:\n    push 7\n    ret\n.function test local 0 arg 0\n    push 50\n    \
                         call f\n    ret\n";
        assert_eq!(run(countdown, "test", |machine| machine.max_calls = 52).unwrap(), Value::Int(7));
        assert!(run(countdown, "test", |machine| machine.max_calls = 51).is_err());
    }

    #[test]
    fn errors_say_what_went_wrong_in_the_script() {
        let cases = [
            (".function test local 0 arg 0\n    call missing\n    ret\n", "test: unknown function missing"),
            (".function test local 0 arg 0\n    pop\n", "test: stack underflow"),
            (".function test local 0 arg 0\n    push 1\n", "test: it ran past its last instruction"),
            (".function test local 0 arg 0\n    push string 2\n    ret\n", "test: no string 2"),
            (".function test local 0 arg 0\n    push abc\n    ret\n", "test: push abc: not a number"),
            (
                ".function test local 0 arg 0\n    load global 9\n    ret\n",
                "test: slot 9 is outside the globals (4 slots)",
            ),
            (
                ".function test local 1 arg 0\n    load local 3\n    ret\n",
                "test: slot 3 is outside its frame (1 slots)",
            ),
            (".function test local 0 arg 0\n    jmp L_9\n", "test: no label L_9"),
            (
                ".function test local 0 arg 0\n    push 1\n    push string 0\n    add\n    ret\n.string \"a\"\n",
                "test: \"a\" is not a number",
            ),
            (
                ".function test local 0 arg 0\n    push 1\n    push 0\n    div\n    ret\n",
                "test: division by zero, which stops the game's VM",
            ),
        ];
        for (text, message) in cases {
            assert_eq!(run(text, "test", |_| {}).unwrap_err().to_string(), message, "{text}");
        }
        assert_eq!(run("", "absent", |_| {}).unwrap_err().to_string(), "unknown function absent");
    }
}
