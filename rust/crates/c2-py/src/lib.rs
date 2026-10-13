//! The Python tools' output depends on how CPython formats and parses numbers and splits text.
//! These do the same, so the port writes the same bytes: repr() of a float, "%.*g", int(text, 0),
//! float(text) and str.split()/str.isspace().

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
    let (digits, point) = digits(&format!("{:e}", value.abs()));
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

/// Digits with single underscores between them, as Python number literals allow; None otherwise.
fn without_underscores(digits: &str) -> Option<String> {
    if digits.starts_with('_') || digits.ends_with('_') || digits.contains("__") {
        return None;
    }
    Some(digits.replace('_', ""))
}

/// int(text, 0): an integer literal with an optional sign and 0x/0o/0b prefix.
pub fn parse_int(text: &str) -> Option<i64> {
    let text = strip(text);
    let (negative, body) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let lower = body.to_ascii_lowercase();
    let (radix, rest) = if let Some(rest) = lower.strip_prefix("0x") {
        (16, rest)
    } else if let Some(rest) = lower.strip_prefix("0o") {
        (8, rest)
    } else if let Some(rest) = lower.strip_prefix("0b") {
        (2, rest)
    } else {
        (10, lower.as_str())
    };
    // after a prefix an underscore may come first ("0x_1f")
    let rest = if radix != 10 { rest.strip_prefix('_').unwrap_or(rest) } else { rest };
    let digits = without_underscores(rest)?;
    if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
        return None;
    }
    if radix == 10 && digits.len() > 1 && digits.starts_with('0') && digits.chars().any(|c| c != '0') {
        return None;
    }
    let magnitude = i128::from_str_radix(&digits, radix).ok()?;
    let value = if negative { -magnitude } else { magnitude };
    i64::try_from(value).ok()
}

/// float(text).
pub fn parse_float(text: &str) -> Option<f64> {
    let text = strip(text);
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
    }

    #[test]
    fn float_like_python() {
        assert_eq!(parse_float("1.5"), Some(1.5));
        assert_eq!(parse_float("-1e-3"), Some(-0.001));
        assert_eq!(parse_float("1_0.5"), Some(10.5));
        assert_eq!(parse_float("1._5"), None);
        assert!(parse_float("inf").unwrap().is_infinite());
        assert_eq!(parse_float("0x10"), None);
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
}
