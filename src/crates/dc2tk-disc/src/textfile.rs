//! The text files the tools write: UTF-8 with \n line endings on every system, and JSON as
//! serde_json writes it pretty (two-space indent, text as it is rather than \u-escaped) with a
//! final newline.
//! Reading takes \r\n as \n, so a file an editor saved with Windows line endings, or one an older
//! version of the tools wrote, reads the same. Game files are never read or written through here:
//! they keep their bytes.

use std::path::Path;

use serde::Serialize;

use crate::{Result, WithPath};

/// A text file's contents, with \r\n read as \n. An error names the file.
pub fn read_text(path: &Path) -> Result<String> {
    let text = std::fs::read_to_string(path).with_path("read", path)?;
    Ok(if text.contains('\r') { text.replace("\r\n", "\n") } else { text })
}

/// Writes text as it is: its \n line endings stay \n. An error names the file.
pub fn write_text(path: &Path, text: &str) -> Result<()> {
    std::fs::write(path, text).with_path("write", path)
}

/// A value as the JSON files the tools write hold it: pretty, with a final newline.
pub fn json_text<T: Serialize + ?Sized>(value: &T) -> String {
    serde_json::to_string_pretty(value).expect("the tools' values serialize to JSON") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_line_endings_read_as_newlines() {
        let path = std::env::temp_dir().join(format!("dc2tk-textfile-{}.txt", std::process::id()));
        std::fs::write(&path, "a\r\nb\nc\r\n").unwrap();
        assert_eq!(read_text(&path).unwrap(), "a\nb\nc\n");
        write_text(&path, "a\nb\n").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"a\nb\n");
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn a_file_that_cannot_be_read_or_written_is_named() {
        let folder = std::env::temp_dir().join(format!("dc2tk-textfile-missing-{}", std::process::id()));
        let missing = folder.join("none.txt");
        let error = read_text(&missing).unwrap_err().to_string();
        assert!(error.starts_with(&format!("cannot read {}: ", missing.display())), "{error}");
        let error = write_text(&missing, "a\n").unwrap_err().to_string();
        assert!(error.starts_with(&format!("cannot write {}: ", missing.display())), "{error}");
    }

    #[test]
    fn json_is_pretty_with_a_final_newline_and_text_unescaped() {
        let value = serde_json::json!({"name": "モ", "list": [1, 2.5], "empty": {}});
        assert_eq!(
            json_text(&value),
            "{\n  \"name\": \"モ\",\n  \"list\": [\n    1,\n    2.5\n  ],\n  \"empty\": {}\n}\n"
        );
    }

    /// Floats are written as the shortest text that reads back as the same bits.
    #[test]
    fn json_floats_read_back_as_the_same_bits() {
        let values = [0.1, -0.0, 1e16, 1.5e-7, 87.76602172851562, f64::MAX, f64::MIN_POSITIVE, 5e-324];
        let text = json_text(&values);
        let back: Vec<f64> = serde_json::from_str(&text).unwrap();
        assert!(values.iter().zip(&back).all(|(a, b)| a.to_bits() == b.to_bits()), "{text}");
    }
}
