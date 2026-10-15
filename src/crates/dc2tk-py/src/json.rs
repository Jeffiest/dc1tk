//! Python's json.dumps: the same text for the same value, with its indent, separators and
//! ensure_ascii options, and floats as repr() writes them.

use std::io::{self, Write};

use serde::Serialize;
use serde_json::ser::{CharEscape, Formatter, Serializer};

use crate::float_repr;

/// json.dumps's options.
#[derive(Clone, Debug)]
pub struct Options {
    /// Spaces per level; None writes everything on one line.
    pub indent: Option<usize>,
    /// Between items and between a key and its value.
    pub separators: (&'static str, &'static str),
    /// Non-ASCII as \uXXXX escapes.
    pub ensure_ascii: bool,
}

impl Options {
    /// json.dumps(value) with its defaults.
    pub fn plain() -> Options {
        Options { indent: None, separators: (", ", ": "), ensure_ascii: true }
    }

    /// json.dumps(value, indent=n): with an indent the item separator has no trailing space.
    pub fn indented(indent: usize) -> Options {
        Options { indent: Some(indent), separators: (",", ": "), ensure_ascii: true }
    }

    /// separators=(",", ":"), ensure_ascii=False: the compact form the tools hash.
    pub fn compact() -> Options {
        Options { indent: None, separators: (",", ":"), ensure_ascii: false }
    }

    pub fn unicode(mut self) -> Options {
        self.ensure_ascii = false;
        self
    }
}

struct PyFormatter {
    options: Options,
    level: usize,
    has_value: bool,
}

impl PyFormatter {
    fn newline<W: ?Sized + Write>(&self, writer: &mut W) -> io::Result<()> {
        if let Some(indent) = self.options.indent {
            writer.write_all(b"\n")?;
            for _ in 0..self.level * indent {
                writer.write_all(b" ")?;
            }
        }
        Ok(())
    }

    fn begin<W: ?Sized + Write>(&mut self, writer: &mut W, open: &[u8]) -> io::Result<()> {
        self.level += 1;
        self.has_value = false;
        writer.write_all(open)
    }

    fn end<W: ?Sized + Write>(&mut self, writer: &mut W, close: &[u8]) -> io::Result<()> {
        self.level -= 1;
        if self.has_value {
            self.newline(writer)?;
        }
        writer.write_all(close)
    }

    fn item<W: ?Sized + Write>(&mut self, writer: &mut W, first: bool) -> io::Result<()> {
        if !first {
            writer.write_all(self.options.separators.0.as_bytes())?;
        }
        self.newline(writer)
    }
}

impl Formatter for PyFormatter {
    fn write_f64<W: ?Sized + Write>(&mut self, writer: &mut W, value: f64) -> io::Result<()> {
        let text = if value.is_nan() {
            "NaN".to_string()
        } else if value.is_infinite() {
            if value > 0.0 { "Infinity" } else { "-Infinity" }.to_string()
        } else {
            float_repr(value)
        };
        writer.write_all(text.as_bytes())
    }

    fn write_f32<W: ?Sized + Write>(&mut self, writer: &mut W, value: f32) -> io::Result<()> {
        self.write_f64(writer, value as f64)
    }

    fn write_string_fragment<W: ?Sized + Write>(&mut self, writer: &mut W, fragment: &str) -> io::Result<()> {
        if !self.options.ensure_ascii {
            return writer.write_all(fragment.as_bytes());
        }
        for c in fragment.chars() {
            if (' '..='~').contains(&c) {
                writer.write_all(&[c as u8])?;
            } else {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    write!(writer, "\\u{unit:04x}")?;
                }
            }
        }
        Ok(())
    }

    fn write_char_escape<W: ?Sized + Write>(&mut self, writer: &mut W, escape: CharEscape) -> io::Result<()> {
        let text: String = match escape {
            CharEscape::Quote => "\\\"".into(),
            CharEscape::ReverseSolidus => "\\\\".into(),
            CharEscape::Solidus => "/".into(),
            CharEscape::Backspace => "\\b".into(),
            CharEscape::FormFeed => "\\f".into(),
            CharEscape::LineFeed => "\\n".into(),
            CharEscape::CarriageReturn => "\\r".into(),
            CharEscape::Tab => "\\t".into(),
            CharEscape::AsciiControl(byte) => format!("\\u{byte:04x}"),
        };
        writer.write_all(text.as_bytes())
    }

    fn begin_array<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.begin(writer, b"[")
    }

    fn end_array<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.end(writer, b"]")
    }

    fn begin_array_value<W: ?Sized + Write>(&mut self, writer: &mut W, first: bool) -> io::Result<()> {
        self.item(writer, first)
    }

    fn end_array_value<W: ?Sized + Write>(&mut self, _writer: &mut W) -> io::Result<()> {
        self.has_value = true;
        Ok(())
    }

    fn begin_object<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.begin(writer, b"{")
    }

    fn end_object<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.end(writer, b"}")
    }

    fn begin_object_key<W: ?Sized + Write>(&mut self, writer: &mut W, first: bool) -> io::Result<()> {
        self.item(writer, first)
    }

    fn begin_object_value<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()> {
        writer.write_all(self.options.separators.1.as_bytes())
    }

    fn end_object_value<W: ?Sized + Write>(&mut self, _writer: &mut W) -> io::Result<()> {
        self.has_value = true;
        Ok(())
    }
}

/// json.dumps(value, ...) with the options.
pub fn dumps<T: Serialize + ?Sized>(value: &T, options: &Options) -> String {
    let mut out = Vec::new();
    let formatter = PyFormatter { options: options.clone(), level: 0, has_value: false };
    let mut serializer = Serializer::with_formatter(&mut out, formatter);
    value.serialize(&mut serializer).expect("a value serializes to JSON");
    String::from_utf8(out).expect("JSON is UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn like_python() {
        let value =
            json!({"a": [1, 2.5, 1e16, 1e-5], "b": {}, "c": [], "d": "é\u{7f}\u{1F600}\n", "e": null, "f": true});
        assert_eq!(
            dumps(&value, &Options::plain()),
            r#"{"a": [1, 2.5, 1e+16, 1e-05], "b": {}, "c": [], "d": "\u00e9\u007f\ud83d\ude00\n", "e": null, "f": true}"#
        );
        assert_eq!(
            dumps(&value, &Options::indented(1)),
            "{\n \"a\": [\n  1,\n  2.5,\n  1e+16,\n  1e-05\n ],\n \"b\": {},\n \"c\": [],\n \"d\": \"\\u00e9\\u007f\\ud83d\\ude00\\n\",\n \"e\": null,\n \"f\": true\n}"
        );
        assert_eq!(
            dumps(&value, &Options::compact()),
            "{\"a\":[1,2.5,1e+16,1e-05],\"b\":{},\"c\":[],\"d\":\"é\u{7f}\u{1F600}\\n\",\"e\":null,\"f\":true}"
        );
    }
}
