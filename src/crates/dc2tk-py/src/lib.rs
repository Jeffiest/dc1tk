//! The Python tools' output depends on how CPython formats and parses numbers and splits text.
//! These do the same, so the port writes the same bytes: repr() of a float, "%.*g", int(text, 0),
//! float(text) and str.split()/str.isspace(); and, for text, the Unicode-aware str methods,
//! repr() of a str and int(text, base).

mod unicode_tables;

use unicode_tables::{ALPHA, DECIMAL, DECIMAL_ZEROS, DIGIT, NUMERIC, PRINTABLE};

pub mod difflib;
pub mod json;

/// Decimal digits and the exponent of a finite, non-negative value: value = 0.DIGITS × 10^point.
/// Rust's {:e} gives the shortest digits that read back as the same double, as Python's repr does;
/// {:.*e} gives them correctly rounded (ties to even), as Python's formatting does.
fn digits(text: &str) -> (String, i32) {
    let (mantissa, exponent) = text.split_once('e').expect("{:e} always has an exponent");
    let digits: String = mantissa.chars().filter(|c| c.is_ascii_digit()).collect();
    (digits, exponent.parse::<i32>().unwrap() + 1)
}

fn exponent_text(exponent: i32) -> String {
    format!("e{}{:02}", if exponent < 0 { '-' } else { '+' }, exponent.abs())
}

fn special(value: f64) -> Option<String> {
    if value.is_nan() {
        return Some("nan".into());
    }
    if value.is_infinite() {
        return Some(if value < 0.0 { "-inf" } else { "inf" }.into());
    }
    None
}

/// repr(value): the shortest text that reads back as the value, fixed between 1e-4 and 1e16 (with
/// ".0" for a whole number), exponential outside.
pub fn float_repr(value: f64) -> String {
    if let Some(text) = special(value) {
        return text;
    }
    let sign = if value.is_sign_negative() { "-" } else { "" };
    // Rust's shortest digits can end a tie the other way from CPython's (which takes the correctly
    // rounded digits whenever they round-trip)
    let shortest = format!("{:e}", value.abs());
    let length =
        shortest.split_once('e').map_or(0, |(mantissa, _)| mantissa.chars().filter(char::is_ascii_digit).count());
    let nearest = format!("{:.*e}", length.max(1) - 1, value.abs());
    let chosen = if nearest.parse::<f64>() == Ok(value.abs()) { nearest } else { shortest };
    let (digits, point) = digits(&chosen);
    let body = if point <= -4 || point > 16 {
        let mut mantissa = digits[..1].to_string();
        if digits.len() > 1 {
            mantissa.push('.');
            mantissa.push_str(&digits[1..]);
        }
        mantissa + &exponent_text(point - 1)
    } else if point <= 0 {
        format!("0.{}{}", "0".repeat((-point) as usize), digits)
    } else if point as usize >= digits.len() {
        format!("{}{}.0", digits, "0".repeat(point as usize - digits.len()))
    } else {
        format!("{}.{}", &digits[..point as usize], &digits[point as usize..])
    };
    format!("{sign}{body}")
}

/// "%.*f" % (precision, value): Rust's fixed notation rounds correctly, as Python's does.
pub fn format_f(value: f64, precision: usize) -> String {
    if let Some(text) = special(value) {
        return text;
    }
    format!("{value:.precision$}")
}

/// sum() of floats: the start value is the int 0, so the first float is added to 0.0 plainly,
/// then Neumaier's compensated summation (CPython 3.12), the compensation added at the end when
/// finite.
pub fn sum_floats(items: impl IntoIterator<Item = f64>) -> f64 {
    let mut items = items.into_iter();
    let Some(first) = items.next() else {
        return 0.0;
    };
    let mut total = 0.0 + first;
    let mut compensation = 0.0;
    for x in items {
        let t = total + x;
        if total.abs() >= x.abs() {
            compensation += (total - t) + x;
        } else {
            compensation += (x - t) + total;
        }
        total = t;
    }
    if compensation != 0.0 && compensation.is_finite() {
        total += compensation;
    }
    total
}

/// "%.*g" % (precision, value): precision significant digits, trailing zeros dropped, exponential
/// when the exponent is below -4 or not below the precision.
pub fn format_g(value: f64, precision: usize) -> String {
    if let Some(text) = special(value) {
        return text;
    }
    let precision = precision.max(1);
    let sign = if value.is_sign_negative() { "-" } else { "" };
    let (digits, point) = digits(&format!("{:.*e}", precision - 1, value.abs()));
    let exponent = point - 1;
    let body = if exponent < -4 || exponent >= precision as i32 {
        let mut mantissa = digits[..1].to_string();
        let rest = digits[1..].trim_end_matches('0');
        if !rest.is_empty() {
            mantissa.push('.');
            mantissa.push_str(rest);
        }
        mantissa + &exponent_text(exponent)
    } else {
        let text = if point <= 0 {
            format!("0.{}{}", "0".repeat((-point) as usize), digits)
        } else if point as usize >= digits.len() {
            format!("{}{}", digits, "0".repeat(point as usize - digits.len()))
        } else {
            format!("{}.{}", &digits[..point as usize], &digits[point as usize..])
        };
        if text.contains('.') {
            text.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            text
        }
    };
    format!("{sign}{body}")
}

/// str.isspace() for one character.
pub fn is_space(c: char) -> bool {
    c.is_whitespace() || ('\x1c'..='\x1f').contains(&c)
}

/// str.split() with no arguments.
pub fn split(text: &str) -> Vec<&str> {
    text.split(is_space).filter(|part| !part.is_empty()).collect()
}

/// str.strip() with no arguments.
pub fn strip(text: &str) -> &str {
    text.trim_matches(is_space)
}

/// int(text, 0): an integer literal with an optional sign and 0x/0o/0b prefix, in any script's
/// decimal digits; None when it is not one, or does not fit in an i64.
pub fn parse_int(text: &str) -> Option<i64> {
    int_base(text, 0).ok().and_then(|value| i64::try_from(value).ok())
}

/// float(text), with any script's decimal digits: whitespace around it, single underscores between
/// digits, and inf, infinity and nan in any case.
pub fn parse_float(text: &str) -> Option<f64> {
    let ascii = ascii_digits(text);
    let text = ascii.trim_matches(|c: char| c.is_ascii() && is_c_space(c as u8));
    let lower = text.to_ascii_lowercase();
    let unsigned = lower.trim_start_matches(['+', '-']);
    if matches!(unsigned, "inf" | "infinity" | "nan") {
        return lower.parse().ok();
    }
    if text.contains('_') {
        // underscores only between digits
        let bytes = text.as_bytes();
        for (at, &byte) in bytes.iter().enumerate() {
            if byte == b'_'
                && !(at > 0 && bytes[at - 1].is_ascii_digit() && bytes.get(at + 1).is_some_and(u8::is_ascii_digit))
            {
                return None;
            }
        }
    }
    let plain = text.replace('_', "");
    if !plain.bytes().all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'e' | b'E' | b'+' | b'-')) {
        return None;
    }
    plain.parse().ok()
}

/// The whitespace textwrap knows: ASCII only.
fn wrap_space(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\x0b' | '\x0c' | '\r' | ' ')
}

/// str.expandtabs(8).
pub fn expand_tabs(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut column = 0;
    for c in text.chars() {
        match c {
            '\t' => {
                let spaces = 8 - column % 8;
                out.extend(std::iter::repeat_n(' ', spaces));
                column += spaces;
            }
            '\n' | '\r' => {
                out.push(c);
                column = 0;
            }
            c => {
                out.push(c);
                column += 1;
            }
        }
    }
    out
}

/// textwrap.wrap(text, width, break_on_hyphens=False) with the other options at their defaults:
/// lines of at most width characters, broken at whitespace, long words split.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let text: String = expand_tabs(text).chars().map(|c| if wrap_space(c) { ' ' } else { c }).collect();
    // chunks: runs of spaces and runs of anything else
    let mut chunks: Vec<Vec<char>> = Vec::new();
    for c in text.chars() {
        match chunks.last_mut() {
            Some(last) if (last[0] == ' ') == (c == ' ') => last.push(c),
            _ => chunks.push(vec![c]),
        }
    }
    let blank = |chunk: &[char]| chunk.iter().all(|&c| is_space(c));
    chunks.reverse();
    let mut lines: Vec<String> = Vec::new();
    while !chunks.is_empty() {
        let mut line: Vec<Vec<char>> = Vec::new();
        let mut length = 0;
        if blank(chunks.last().unwrap()) && !lines.is_empty() {
            chunks.pop();
        }
        while let Some(next) = chunks.last() {
            if length + next.len() <= width {
                length += next.len();
                line.push(chunks.pop().unwrap());
            } else {
                break;
            }
        }
        // a word longer than a line: as much of it as fits
        let space_left = if width < 1 { 1 } else { width - length };
        if space_left > 0 {
            if let Some(chunk) = chunks.pop_if(|next| next.len() > width) {
                line.push(chunk[..space_left].to_vec());
                chunks.push(chunk[space_left..].to_vec());
            }
        }
        if line.last().is_some_and(|last| blank(last)) {
            line.pop();
        }
        if !line.is_empty() {
            lines.push(line.into_iter().flatten().collect());
        }
    }
    lines
}

/// Path.read_text(): UTF-8, with \r\n and \r read as \n.
pub fn read_text(path: &std::path::Path) -> std::io::Result<String> {
    Ok(std::fs::read_to_string(path)?.replace("\r\n", "\n").replace('\r', "\n"))
}

/// Path.write_text(): UTF-8, with \n written as this system's line ending.
pub fn write_text(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    if cfg!(windows) {
        std::fs::write(path, text.replace('\n', "\r\n"))
    } else {
        std::fs::write(path, text)
    }
}

/// Whether c falls in one of a table's (first, last) pairs.
fn in_table(table: &[u32], c: char) -> bool {
    let c = c as u32;
    let pairs = table.len() / 2;
    let (mut low, mut high) = (0, pairs);
    while low < high {
        let middle = (low + high) / 2;
        if c < table[2 * middle] {
            high = middle;
        } else if c > table[2 * middle + 1] {
            low = middle + 1;
        } else {
            return true;
        }
    }
    false
}

/// str.isalpha() for one character.
pub fn is_alpha(c: char) -> bool {
    in_table(&ALPHA, c)
}

/// str.isdecimal() for one character: what the re module's \d matches.
pub fn is_decimal(c: char) -> bool {
    in_table(&DECIMAL, c)
}

/// The value of a decimal digit (any script's), as unicodedata.decimal() gives it.
pub fn decimal_value(c: char) -> Option<u32> {
    if !is_decimal(c) {
        return None;
    }
    let at = DECIMAL_ZEROS.partition_point(|&zero| zero <= c as u32);
    Some(c as u32 - DECIMAL_ZEROS[at - 1])
}

/// str.isdigit() for one character.
pub fn is_digit(c: char) -> bool {
    in_table(&DIGIT, c)
}

/// str.isnumeric() for one character.
pub fn is_numeric(c: char) -> bool {
    in_table(&NUMERIC, c)
}

/// str.isalnum() for one character.
pub fn is_alnum(c: char) -> bool {
    is_alpha(c) || is_numeric(c)
}

/// What the re module's \w matches in a str pattern.
pub fn is_word(c: char) -> bool {
    c == '_' || is_alnum(c)
}

/// str.isprintable() for one character.
pub fn is_printable(c: char) -> bool {
    in_table(&PRINTABLE, c)
}

/// repr() of a str.
pub fn repr(text: &str) -> String {
    let quote = if text.contains('\'') && !text.contains('"') { '"' } else { '\'' };
    let mut out = String::with_capacity(text.len() + 2);
    out.push(quote);
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if c < ' ' || c == '\x7f' => out.push_str(&format!("\\x{:02x}", c as u32)),
            c if c.is_ascii() || is_printable(c) => out.push(c),
            c if (c as u32) <= 0xff => out.push_str(&format!("\\x{:02x}", c as u32)),
            c if (c as u32) <= 0xffff => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push_str(&format!("\\U{:08x}", c as u32)),
        }
    }
    out.push(quote);
    out
}

/// What int() and float() parse instead of a str (_PyUnicode_TransformDecimalAndSpaceToASCII):
/// ASCII stays, other whitespace becomes a space and other decimal digits their ASCII digit;
/// anything else ends the text with '?'.
pub fn ascii_digits(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if (c as u32) < 127 {
            out.push(c);
        } else if is_space(c) {
            out.push(' ');
        } else if let Some(value) = decimal_value(c) {
            out.push(char::from_digit(value, 10).unwrap());
        } else {
            out.push('?');
            break;
        }
    }
    out
}

/// Why int(text, base) fails: text that is not an integer, or one too large to hold here (Python's
/// ints have no limit).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntError {
    Invalid,
    TooLarge,
}

/// The whitespace CPython skips around a number: ASCII space, \t, \n, \v, \f, \r.
fn is_c_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r')
}

/// int(text, base) for base 0 or 2 to 36, with any script's decimal digits.
pub fn int_base(text: &str, base: u32) -> Result<i128, IntError> {
    let ascii = ascii_digits(text);
    let bytes = ascii.as_bytes();
    let at = |i: usize| bytes.get(i).copied().unwrap_or(0);
    let mut i = 0;
    while is_c_space(at(i)) {
        i += 1;
    }
    let negative = at(i) == b'-';
    if matches!(at(i), b'+' | b'-') {
        i += 1;
    }
    let mut base = base;
    let mut only_zero = false;
    if base == 0 {
        base = match (at(i), at(i + 1)) {
            (b'0', b'x' | b'X') => 16,
            (b'0', b'o' | b'O') => 8,
            (b'0', b'b' | b'B') => 2,
            (b'0', _) => {
                only_zero = true;
                10
            }
            _ => 10,
        };
    }
    if at(i) == b'0' && matches!((base, at(i + 1)), (16, b'x' | b'X') | (8, b'o' | b'O') | (2, b'b' | b'B')) {
        i += 2;
        if at(i) == b'_' {
            i += 1;
        }
    }
    if at(i) == b'_' {
        return Err(IntError::Invalid);
    }
    let mut value: Option<i128> = Some(0);
    let mut digits = 0;
    let mut previous = 0;
    loop {
        let byte = at(i);
        let digit = (byte as char).to_digit(36).filter(|&digit| digit < base);
        if byte == b'_' {
            if previous == b'_' {
                return Err(IntError::Invalid);
            }
        } else if let Some(digit) = digit {
            digits += 1;
            value = value.and_then(|value| value.checked_mul(base as i128)?.checked_add(digit as i128));
        } else {
            break;
        }
        previous = byte;
        i += 1;
    }
    if previous == b'_' || digits == 0 {
        return Err(IntError::Invalid);
    }
    while is_c_space(at(i)) {
        i += 1;
    }
    if i != bytes.len() {
        return Err(IntError::Invalid);
    }
    let value = value.ok_or(IntError::TooLarge)?;
    if only_zero && value != 0 {
        return Err(IntError::Invalid);
    }
    Ok(if negative { -value } else { value })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repr_like_python() {
        let cases = [
            (0.0, "0.0"),
            (-0.0, "-0.0"),
            (1.0, "1.0"),
            (0.1, "0.1"),
            (1.5e-5, "1.5e-05"),
            (0.0001, "0.0001"),
            (1e16, "1e+16"),
            (1e15, "1000000000000000.0"),
            (123.456, "123.456"),
            (-2.5, "-2.5"),
            (3.4028234663852886e38, "3.4028234663852886e+38"),
            (-1.0728836059570312e-06, "-1.0728836059570312e-06"),
        ];
        for (value, text) in cases {
            assert_eq!(float_repr(value), text, "{value}");
        }
    }

    #[test]
    fn format_g_like_python() {
        let cases = [
            (0.1f32 as f64, 1, "0.1"),
            (0.1f32 as f64, 9, "0.100000001"),
            (100.0, 1, "1e+02"),
            (100.0, 3, "100"),
            (0.0001, 2, "0.0001"),
            (0.00001234, 3, "1.23e-05"),
            (0.0, 1, "0"),
            (-0.0, 3, "-0"),
            (2.5, 1, "2"),
            (3.5, 1, "4"),
            (123456.0, 4, "1.235e+05"),
        ];
        for (value, precision, text) in cases {
            assert_eq!(format_g(value, precision), text, "{value} {precision}");
        }
    }

    #[test]
    fn int_like_python() {
        assert_eq!(parse_int("0x1f"), Some(31));
        assert_eq!(parse_int("-0x10"), Some(-16));
        assert_eq!(parse_int(" 12 "), Some(12));
        assert_eq!(parse_int("0"), Some(0));
        assert_eq!(parse_int("00"), Some(0));
        assert_eq!(parse_int("010"), None);
        assert_eq!(parse_int("1_000"), Some(1000));
        assert_eq!(parse_int("0x_ff"), Some(255));
        assert_eq!(parse_int("1__0"), None);
        assert_eq!(parse_int("abc"), None);
        // as int(text, 0) reads them: any script's digits, only ASCII whitespace around
        assert_eq!(parse_int("\u{661}\u{662}"), Some(12));
        assert_eq!(parse_int("\u{3000}12"), Some(12));
        assert_eq!(parse_int("\x1c12"), None);
        assert_eq!(parse_int("9223372036854775808"), None);
    }

    #[test]
    fn float_like_python() {
        assert_eq!(parse_float("1.5"), Some(1.5));
        assert_eq!(parse_float("-1e-3"), Some(-0.001));
        assert_eq!(parse_float("1_0.5"), Some(10.5));
        assert_eq!(parse_float("1._5"), None);
        assert!(parse_float("inf").unwrap().is_infinite());
        assert_eq!(parse_float("0x10"), None);
        assert_eq!(parse_float("\u{661}.5"), Some(1.5));
        assert_eq!(parse_float("\u{a0}1.5\u{3000}"), Some(1.5));
        assert_eq!(parse_float("\x1c1.5"), None);
    }

    #[test]
    fn wrap_like_python() {
        assert_eq!(wrap("note: a b c", 7), vec!["note: a", "b c"]);
        assert_eq!(wrap("", 10), Vec::<String>::new());
        assert_eq!(wrap("  lead and  double", 8), vec!["  lead", "and", "double"]);
        assert_eq!(wrap("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
        assert_eq!(wrap("mid-motion text", 6), vec!["mid-mo", "tion", "text"]);
        assert_eq!(wrap("a\tb", 20), vec!["a       b"]);
    }

    #[test]
    fn whitespace_like_python() {
        assert_eq!(split(" a\tb\x1cc  "), vec!["a", "b", "c"]);
    }

    #[test]
    fn int_base_like_python() {
        assert_eq!(int_base("010", 10), Ok(10));
        assert_eq!(int_base("010", 0), Err(IntError::Invalid));
        assert_eq!(int_base("00_0", 0), Ok(0));
        assert_eq!(int_base("0x10", 10), Err(IntError::Invalid));
        assert_eq!(int_base("0x_1f", 16), Ok(31));
        assert_eq!(int_base("+f", 16), Ok(15));
        assert_eq!(int_base(" -1\n", 16), Ok(-1));
        assert_eq!(int_base("_1", 16), Err(IntError::Invalid));
        assert_eq!(int_base("\u{661}\u{661}", 16), Ok(17));
        assert_eq!(int_base("\u{ff11}", 10), Ok(1));
        assert_eq!(int_base("1__0", 10), Err(IntError::Invalid));
        assert_eq!(int_base("", 10), Err(IntError::Invalid));
        assert_eq!(int_base(&"9".repeat(40), 10), Err(IntError::TooLarge));
    }

    /// in_table and decimal_value search the tables, so each must be sorted runs.
    #[test]
    fn unicode_tables_are_sorted_runs() {
        for table in [&ALPHA[..], &DECIMAL, &DIGIT, &NUMERIC, &PRINTABLE] {
            assert!(table.len() % 2 == 0);
            assert!(table.chunks(2).all(|pair| pair[0] <= pair[1]));
            // a run ends before the next one starts, with a gap (touching runs would be one)
            assert!(table[1..].chunks(2).filter(|gap| gap.len() == 2).all(|gap| gap[0] + 1 < gap[1]));
        }
        assert!(DECIMAL_ZEROS.windows(2).all(|pair| pair[0] + 10 <= pair[1]));
    }

    #[test]
    fn str_like_python() {
        assert_eq!(repr("a'b"), "\"a'b\"");
        assert_eq!(repr("a'\"b"), "'a\\'\"b'");
        assert_eq!(
            repr("\u{feff}\x7f\u{80}\u{200b}\u{e0001}\u{a0}\u{ad}\\\0\n\t\r"),
            "'\\ufeff\\x7f\\x80\\u200b\\U000e0001\\xa0\\xad\\\\\\x00\\n\\t\\r'"
        );
        assert_eq!(repr("あ"), "'あ'");
        assert!(!is_alpha('\u{3007}') && !is_alpha('\u{903}') && is_alpha('あ'));
        assert!(is_digit('²') && !is_decimal('²') && decimal_value('\u{661}') == Some(1));
    }
}
