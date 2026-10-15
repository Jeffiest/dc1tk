//! JSON as Python's json module reads and writes it: objects keep their keys' order, numbers stay
//! ints or floats, and NaN and Infinity are read and written.

use crate::{bail, Result};

/// A JSON value with Python's types.
#[derive(Clone, Debug)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    List(Vec<Value>),
    Object(Vec<(String, Value)>),
}

impl Value {
    /// dict.get(key).
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(items) => items.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// key in dict.
    pub fn has(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    /// d[key] = value: replaced in place, or added at the end.
    pub fn set(&mut self, key: &str, value: Value) {
        if let Value::Object(items) = self {
            match items.iter_mut().find(|(k, _)| k == key) {
                Some(slot) => slot.1 = value,
                None => items.push((key.to_string(), value)),
            }
        }
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        match self {
            Value::Object(items) => items.iter_mut().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&Vec<Value>> {
        match self {
            Value::List(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(text) => Some(text),
            _ => None,
        }
    }

    /// The value as a Python int (bools are ints).
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(v) => Some(*v),
            Value::Bool(b) => Some(*b as i64),
            _ => None,
        }
    }

    /// The value as a number (an int or a float; bools are ints).
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(v) => Some(*v as f64),
            Value::Float(v) => Some(*v),
            Value::Bool(b) => Some(*b as i64 as f64),
            _ => None,
        }
    }

    /// Python truthiness.
    pub fn truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Int(v) => *v != 0,
            Value::Float(v) => *v != 0.0,
            Value::Str(s) => !s.is_empty(),
            Value::List(items) => !items.is_empty(),
            Value::Object(items) => !items.is_empty(),
        }
    }

    pub fn object() -> Value {
        Value::Object(Vec::new())
    }

    pub fn floats(values: &[f64]) -> Value {
        Value::List(values.iter().map(|&v| Value::Float(v)).collect())
    }

    /// json.dumps(value, sort_keys=True) with the default separators: compared to tell values
    /// apart as the Python's text does.
    pub fn sorted_text(&self) -> String {
        let mut out = String::new();
        self.write_sorted(&mut out);
        out
    }

    fn write_sorted(&self, out: &mut String) {
        match self {
            Value::Null => out.push_str("null"),
            Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Value::Int(v) => out.push_str(&v.to_string()),
            Value::Float(v) => out.push_str(&float_text(*v)),
            Value::Str(s) => out.push_str(&dc2tk_py::json::dumps(s, &dc2tk_py::json::Options::plain())),
            Value::List(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    item.write_sorted(out);
                }
                out.push(']');
            }
            Value::Object(items) => {
                let mut sorted: Vec<&(String, Value)> = items.iter().collect();
                sorted.sort_by(|a, b| a.0.cmp(&b.0));
                out.push('{');
                for (i, (k, v)) in sorted.into_iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(&dc2tk_py::json::dumps(k, &dc2tk_py::json::Options::plain()));
                    out.push_str(": ");
                    v.write_sorted(out);
                }
                out.push('}');
            }
        }
    }
}

/// A float as json.dumps writes it.
fn float_text(v: f64) -> String {
    if v.is_nan() {
        "NaN".into()
    } else if v.is_infinite() {
        if v > 0.0 { "Infinity" } else { "-Infinity" }.into()
    } else {
        dc2tk_py::float_repr(v)
    }
}

/// Builds an object from (key, value) pairs.
#[macro_export]
macro_rules! object {
    ($($key:expr => $value:expr),* $(,)?) => {
        $crate::json::Value::Object(vec![$(($key.to_string(), $crate::json::Value::from($value))),*])
    };
}

impl From<i64> for Value {
    fn from(v: i64) -> Value {
        Value::Int(v)
    }
}

impl From<usize> for Value {
    fn from(v: usize) -> Value {
        Value::Int(v as i64)
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Value {
        Value::Float(v)
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Value {
        Value::Bool(v)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Value {
        Value::Str(v.to_string())
    }
}

impl From<String> for Value {
    fn from(v: String) -> Value {
        Value::Str(v)
    }
}

impl From<Vec<Value>> for Value {
    fn from(v: Vec<Value>) -> Value {
        Value::List(v)
    }
}

/// json.dumps(value, ensure_ascii=False, separators=(",", ":")).
pub fn dumps_compact(value: &Value) -> String {
    let mut out = String::new();
    write_compact(value, &mut out);
    out
}

fn write_compact(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(v) => out.push_str(&v.to_string()),
        Value::Float(v) => out.push_str(&float_text(*v)),
        Value::Str(s) => write_string(s, out),
        Value::List(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_compact(item, out);
            }
            out.push(']');
        }
        Value::Object(items) => {
            out.push('{');
            for (i, (k, v)) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_string(k, out);
                out.push(':');
                write_compact(v, out);
            }
            out.push('}');
        }
    }
}

/// A string as json.dumps writes it with ensure_ascii=False.
fn write_string(text: &str, out: &mut String) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// json.loads(text).
pub fn loads(text: &str) -> Result<Value> {
    let mut parser = Parser { chars: text.as_bytes(), text, pos: 0 };
    parser.space();
    let value = parser.value()?;
    parser.space();
    if parser.pos != parser.chars.len() {
        bail!("Extra data: char {}", parser.pos);
    }
    Ok(value)
}

struct Parser<'a> {
    chars: &'a [u8],
    text: &'a str,
    pos: usize,
}

impl Parser<'_> {
    fn space(&mut self) {
        while self.pos < self.chars.len() && matches!(self.chars[self.pos], b' ' | b'\t' | b'\n' | b'\r') {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.chars.get(self.pos).copied()
    }

    fn literal(&mut self, word: &str, value: Value) -> Result<Value> {
        if self.text[self.pos..].starts_with(word) {
            self.pos += word.len();
            Ok(value)
        } else {
            bail!("Expecting value: char {}", self.pos)
        }
    }

    fn value(&mut self) -> Result<Value> {
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.list(),
            Some(b'"') => Ok(Value::Str(self.string()?)),
            Some(b'n') => self.literal("null", Value::Null),
            Some(b't') => self.literal("true", Value::Bool(true)),
            Some(b'f') => self.literal("false", Value::Bool(false)),
            Some(b'N') => self.literal("NaN", Value::Float(f64::NAN)),
            Some(b'I') => self.literal("Infinity", Value::Float(f64::INFINITY)),
            Some(b'-') if self.text[self.pos..].starts_with("-Infinity") => {
                self.literal("-Infinity", Value::Float(f64::NEG_INFINITY))
            }
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => bail!("Expecting value: char {}", self.pos),
        }
    }

    fn number(&mut self) -> Result<Value> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        match self.peek() {
            Some(b'0') => self.pos += 1,
            Some(b'1'..=b'9') => {
                while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.pos += 1;
                }
            }
            _ => bail!("Expecting value: char {start}"),
        }
        let mut float = false;
        if self.peek() == Some(b'.') && self.chars.get(self.pos + 1).is_some_and(u8::is_ascii_digit) {
            float = true;
            self.pos += 1;
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            let mut at = self.pos + 1;
            if matches!(self.chars.get(at), Some(b'+' | b'-')) {
                at += 1;
            }
            if self.chars.get(at).is_some_and(u8::is_ascii_digit) {
                float = true;
                self.pos = at;
                while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.pos += 1;
                }
            }
        }
        let text = &self.text[start..self.pos];
        if float {
            Ok(Value::Float(text.parse().expect("a JSON number parses")))
        } else {
            match text.parse::<i64>() {
                Ok(v) => Ok(Value::Int(v)),
                Err(_) => bail!("an integer too large: {text}"),
            }
        }
    }

    fn string(&mut self) -> Result<String> {
        self.pos += 1;
        let mut out = String::new();
        loop {
            let Some(c) = self.peek() else {
                bail!("Unterminated string starting at: char {}", self.pos);
            };
            match c {
                b'"' => {
                    self.pos += 1;
                    return Ok(out);
                }
                b'\\' => {
                    let Some(e) = self.chars.get(self.pos + 1).copied() else {
                        bail!("Unterminated string starting at: char {}", self.pos);
                    };
                    self.pos += 2;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let unit = self.hex4()?;
                            if (0xD800..0xDC00).contains(&unit) && self.text[self.pos..].starts_with("\\u") {
                                let save = self.pos;
                                self.pos += 2;
                                let low = self.hex4()?;
                                if (0xDC00..0xE000).contains(&low) {
                                    let code = 0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00);
                                    out.push(char::from_u32(code).unwrap());
                                    continue;
                                }
                                self.pos = save;
                            }
                            out.push(char::from_u32(unit).unwrap_or('\u{FFFD}'));
                        }
                        _ => bail!("Invalid \\escape: char {}", self.pos - 2),
                    }
                }
                c if c < 0x20 => bail!("Invalid control character at: char {}", self.pos),
                _ => {
                    let rest = &self.text[self.pos..];
                    let ch = rest.chars().next().unwrap();
                    out.push(ch);
                    self.pos += ch.len_utf8();
                }
            }
        }
    }

    fn hex4(&mut self) -> Result<u32> {
        let Some(digits) = self.text.get(self.pos..self.pos + 4) else {
            bail!("Invalid \\uXXXX escape: char {}", self.pos);
        };
        let Ok(unit) = u32::from_str_radix(digits, 16) else {
            bail!("Invalid \\uXXXX escape: char {}", self.pos);
        };
        self.pos += 4;
        Ok(unit)
    }

    fn list(&mut self) -> Result<Value> {
        self.pos += 1;
        let mut items = Vec::new();
        self.space();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Value::List(items));
        }
        loop {
            self.space();
            items.push(self.value()?);
            self.space();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Value::List(items));
                }
                _ => bail!("Expecting ',' delimiter: char {}", self.pos),
            }
        }
    }

    fn object(&mut self) -> Result<Value> {
        self.pos += 1;
        let mut value = Value::object();
        self.space();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(value);
        }
        loop {
            self.space();
            if self.peek() != Some(b'"') {
                bail!("Expecting property name enclosed in double quotes: char {}", self.pos);
            }
            let key = self.string()?;
            self.space();
            if self.peek() != Some(b':') {
                bail!("Expecting ':' delimiter: char {}", self.pos);
            }
            self.pos += 1;
            self.space();
            let item = self.value()?;
            value.set(&key, item);
            self.space();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(value);
                }
                _ => bail!("Expecting ',' delimiter: char {}", self.pos),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_writes_like_python() {
        let text = r#"{"a":[1,2.5,-0.0,1e+16,NaN,-Infinity],"b":{"c":null,"d":true},"e":"é😀"}"#;
        let value = loads(text).unwrap();
        assert_eq!(
            dumps_compact(&value),
            "{\"a\":[1,2.5,-0.0,1e+16,NaN,-Infinity],\"b\":{\"c\":null,\"d\":true},\"e\":\"é😀\"}"
        );
        assert_eq!(loads(r#"{"z":1,"a":2,"z":3}"#).unwrap().sorted_text(), r#"{"a": 2, "z": 3}"#);
        assert!(loads("[1,]").is_err());
        assert!(loads("{} x").is_err());
    }
}
