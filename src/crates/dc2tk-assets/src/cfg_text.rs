//! A text cfg script (`TAG arg, arg, ...;`, read by the engine's CScriptInterpreter,
//! scriptinterpreter.cpp) kept whole: every statement with the spaces, line breaks and comments
//! before it, and every argument with the spaces around it, so that an argument can be changed and
//! the rest of the file written back byte for byte.
//!
//! The engine's reading, which [`Piece::value`] follows: comments (`//` to the line's end, `/* */`)
//! are blanked first; a tag is the text up to a space or `;`; the arguments run to the `;`,
//! split at commas outside quotes. Inside quotes a Shift-JIS lead byte takes the next byte with
//! it and `\"` is a quote. An argument in quotes is a string; otherwise its text up to the first
//! space is a float when it has a `.`, an int (C's `atoi`) when it is digits and `-`, and invalid
//! (read as 0 or a null string) when it has anything else or is empty. An empty last argument,
//! right before the `;`, is not counted.
//!
//! Binary scripts (`BIN` first) and comments inside a statement are refused: the engine blanks a
//! comment wherever it is, even inside quotes, which a kept file could not show.

use crate::{bail, Result};

/// A script: its statements in order and what follows the last one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Script {
    pub statements: Vec<Statement>,
    /// Spaces, line breaks and comments after the last statement.
    pub tail: Vec<u8>,
}

/// One statement: `TAG` and its arguments up to the `;`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Statement {
    /// Spaces, line breaks and comments before the tag.
    pub before: Vec<u8>,
    pub tag: String,
    /// The text between the tag and the `;`, split at the commas outside quotes: one piece for a
    /// statement without arguments (empty, or the spaces before the `;`).
    pub pieces: Vec<Piece>,
}

/// The text of one argument, with the spaces and line breaks around it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Piece {
    pub before: Vec<u8>,
    pub text: Vec<u8>,
    pub after: Vec<u8>,
}

/// An argument as the engine reads it.
#[derive(Clone, Debug, PartialEq)]
pub enum Arg {
    Int(i32),
    Float(f32),
    /// The bytes inside the quotes, `\"` read as `"`.
    Str(Vec<u8>),
    /// Text that is not a number (or empty): 0 to spiGetStackInt, null to spiGetStackString.
    Invalid,
}

impl Arg {
    /// The value as spiGetStackInt gives it.
    pub fn int(&self) -> i32 {
        match self {
            Arg::Int(value) => *value,
            Arg::Float(value) => *value as i32,
            _ => 0,
        }
    }

    /// The value as spiGetStackFloat gives it.
    pub fn float(&self) -> f32 {
        match self {
            Arg::Int(value) => *value as f32,
            Arg::Float(value) => *value,
            _ => 0.0,
        }
    }
}

fn is_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

fn is_lead(byte: u8) -> bool {
    byte & 0x80 != 0 && !(0xA1..=0xDF).contains(&byte)
}

fn comment_at(data: &[u8], at: usize) -> bool {
    data.get(at) == Some(&b'/') && matches!(data.get(at + 1), Some(b'/' | b'*'))
}

impl Script {
    pub fn parse(data: &[u8]) -> Result<Script> {
        if data.starts_with(b"BIN") {
            bail!("a binary script (BIN), not text");
        }
        let n = data.len();
        let mut statements = Vec::new();
        let mut at = 0;
        loop {
            let start = at;
            // spaces and comments before the tag
            loop {
                if at < n && is_space(data[at]) {
                    at += 1;
                } else if comment_at(data, at) && data[at + 1] == b'/' {
                    while at < n && data[at] != b'\n' && data[at] != b'\r' {
                        at += 1;
                    }
                } else if comment_at(data, at) {
                    at = (at + 2..n.saturating_sub(1)).find(|&i| &data[i..i + 2] == b"*/").map_or(n, |i| i + 2);
                } else {
                    break;
                }
            }
            if at >= n {
                return Ok(Script { statements, tail: data[start..].to_vec() });
            }
            let before = data[start..at].to_vec();
            let tag_start = at;
            while at < n && !is_space(data[at]) && data[at] != b';' {
                if comment_at(data, at) {
                    bail!("byte {at}: a comment inside a statement");
                }
                at += 1;
            }
            let Ok(tag) = String::from_utf8(data[tag_start..at].to_vec()) else {
                bail!("byte {tag_start}: a tag that is not plain text");
            };
            let mut pieces = Vec::new();
            let mut piece_start = at;
            let mut quoted = false;
            loop {
                if at >= n {
                    bail!("byte {tag_start}: {tag} has no ';'");
                }
                let byte = data[at];
                if quoted && is_lead(byte) {
                    at += 2;
                    continue;
                }
                if quoted && byte == b'\\' && data.get(at + 1) == Some(&b'"') {
                    at += 2;
                    continue;
                }
                if byte == b'"' {
                    quoted = !quoted;
                } else if comment_at(data, at) {
                    bail!("byte {at}: a comment inside a statement ({tag})");
                } else if !quoted && (byte == b',' || byte == b';') {
                    pieces.push(Piece::split(&data[piece_start..at]));
                    at += 1;
                    piece_start = at;
                    if byte == b';' {
                        break;
                    }
                    continue;
                }
                at += 1;
            }
            statements.push(Statement { before, tag, pieces });
        }
    }

    pub fn write(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for statement in &self.statements {
            statement.write_into(&mut out);
        }
        out.extend_from_slice(&self.tail);
        out
    }
}

impl Statement {
    fn write_into(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.before);
        out.extend_from_slice(self.tag.as_bytes());
        for (number, piece) in self.pieces.iter().enumerate() {
            if number > 0 {
                out.push(b',');
            }
            out.extend_from_slice(&piece.before);
            out.extend_from_slice(&piece.text);
            out.extend_from_slice(&piece.after);
        }
        out.push(b';');
    }

    /// The arguments the engine counts: every piece but an empty last one.
    pub fn args(&self) -> &[Piece] {
        match self.pieces.split_last() {
            Some((last, rest)) if last.text.is_empty() => rest,
            _ => &self.pieces,
        }
    }

    /// The argument values the engine reads.
    pub fn values(&self) -> Vec<Arg> {
        self.args().iter().map(Piece::value).collect()
    }
}

impl Piece {
    fn split(raw: &[u8]) -> Piece {
        let start = raw.iter().position(|&byte| !is_space(byte)).unwrap_or(raw.len());
        let end = raw.iter().rposition(|&byte| !is_space(byte)).map_or(start, |at| at + 1);
        Piece { before: raw[..start].to_vec(), text: raw[start..end].to_vec(), after: raw[end..].to_vec() }
    }

    /// The value the engine reads from the text (GetArg).
    pub fn value(&self) -> Arg {
        let text = &self.text;
        if text.len() >= 2 && text[0] == b'"' && text[text.len() - 1] == b'"' {
            let inner = &text[1..text.len() - 1];
            let mut out = Vec::with_capacity(inner.len());
            let mut at = 0;
            while at < inner.len() {
                if is_lead(inner[at]) && at + 1 < inner.len() {
                    out.extend_from_slice(&inner[at..at + 2]);
                    at += 2;
                } else if inner[at] == b'\\' && inner.get(at + 1) == Some(&b'"') {
                    out.push(b'"');
                    at += 2;
                } else {
                    out.push(inner[at]);
                    at += 1;
                }
            }
            return Arg::Str(out);
        }
        // the text up to the first space; a quote at either end alone is part of it
        let word: &[u8] = &text[..text.iter().position(|&byte| is_space(byte)).unwrap_or(text.len())];
        let word = word.strip_suffix(b"\"").unwrap_or(word);
        if word.is_empty() || word.iter().any(|&byte| byte != b'-' && byte != b'.' && !byte.is_ascii_digit()) {
            return Arg::Invalid;
        }
        if word.contains(&b'.') {
            return Arg::Float(atof(word));
        }
        Arg::Int(atoi(word))
    }

    /// The piece with its text replaced and its spaces kept.
    pub fn with_text(&self, text: Vec<u8>) -> Piece {
        Piece { before: self.before.clone(), text, after: self.after.clone() }
    }
}

/// C's atoi over digits and '-': an optional sign, then digits up to the first other byte,
/// wrapping as the engine's 32-bit int does.
fn atoi(word: &[u8]) -> i32 {
    let (negative, digits) = match word.split_first() {
        Some((b'-', rest)) => (true, rest),
        _ => (false, word),
    };
    let mut value: i32 = 0;
    for &byte in digits.iter().take_while(|byte| byte.is_ascii_digit()) {
        value = value.wrapping_mul(10).wrapping_add((byte - b'0') as i32);
    }
    if negative {
        value.wrapping_neg()
    } else {
        value
    }
}

/// C's atof over digits, '-' and '.': the longest prefix that is a number.
fn atof(word: &[u8]) -> f32 {
    let text = String::from_utf8_lossy(word);
    let mut end = text.len();
    while end > 0 {
        if let Ok(value) = text[..end].parse::<f64>() {
            return value as f32;
        }
        end -= 1;
    }
    0.0
}

/// The text an int argument is written as.
pub fn int_text(value: i64) -> Vec<u8> {
    value.to_string().into_bytes()
}

/// The text a string argument is written as: in quotes, a quote inside as `\"`.
pub fn str_text(value: &[u8]) -> Vec<u8> {
    let mut out = vec![b'"'];
    let mut at = 0;
    while at < value.len() {
        if is_lead(value[at]) && at + 1 < value.len() {
            out.extend_from_slice(&value[at..at + 2]);
            at += 2;
            continue;
        }
        if value[at] == b'"' {
            out.push(b'\\');
        }
        out.push(value[at]);
        at += 1;
    }
    out.push(b'"');
    out
}

/// An edit of one argument, or of every argument from one on, of a statement of one of a set of
/// scripts: what a mod changes in a table kept in cfg scripts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub file: usize,
    pub statement: usize,
    pub arg: usize,
    pub to: To,
    /// What the edit changes, for messages (`item 1 (Battle Wrench) weapon.attack`).
    pub what: String,
    pub module: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum To {
    /// The argument's new text; its spaces are kept.
    Arg(Vec<u8>),
    /// The new texts of the arguments from `arg` on, written without spaces; the statement keeps
    /// the spaces before its `;`.
    Rest(Vec<Vec<u8>>),
    /// The new texts of the arguments from `arg` up to the last this many (a list inside a
    /// statement: DATASET's materials), written without spaces; the arguments after keep theirs.
    Span(Vec<Vec<u8>>, usize),
    /// New statements, written as they are (each with the line break before it), after the
    /// statement: a record a mod adds. Several after one statement keep the order given; the
    /// edit's `what` names the record, and two giving one record different texts are refused.
    Insert(Vec<u8>),
}

/// SPI_STACK_SIZE: the arguments one statement may have; PushStack drops the rest.
pub const ARGS_MOST: usize = 64;

/// The text of a new statement: a line break, the tag and the arguments' texts.
pub fn statement_text(tag: &str, args: &[Vec<u8>]) -> Vec<u8> {
    let mut out = b"\r\n".to_vec();
    out.extend_from_slice(tag.as_bytes());
    for (number, arg) in args.iter().enumerate() {
        out.push(if number == 0 { b' ' } else { b',' });
        out.extend_from_slice(arg);
    }
    out.push(b';');
    out
}

/// The text of a run of statements of a script, `from` to `to` (inclusive), as new statements:
/// the line break before the first stands for whatever was before it (a comment naming the
/// record it was copied from), the rest kept as they are.
pub fn block_text(statements: &[Statement]) -> Vec<u8> {
    let mut out = Vec::new();
    for (number, statement) in statements.iter().enumerate() {
        let mut copy = statement.clone();
        if number == 0 {
            copy.before = b"\r\n".to_vec();
        }
        copy.write_into(&mut out);
    }
    out
}

/// The scripts with the edits made. Edits that agree are made once; two that give one argument
/// (or one statement's list) different texts are refused, naming both mods, as is an argument a
/// list of its statement writes over with other text. Single arguments are changed first, by their
/// places in the disc's statements, then lists, which may change how many arguments a statement
/// has. An edit of a file, statement or argument the scripts lack is refused.
pub fn apply_edits(scripts: &[Script], edits: &[Edit]) -> Result<Vec<Vec<u8>>> {
    let mut scripts = scripts.to_vec();
    let mut done: std::collections::BTreeMap<(usize, usize, usize), &Edit> = std::collections::BTreeMap::new();
    let mut lists = Vec::new();
    // new statements by (file, statement after), each record once
    let mut inserted: std::collections::BTreeMap<(usize, usize), Vec<&Edit>> = std::collections::BTreeMap::new();
    let mut records: std::collections::BTreeMap<(usize, &str), &Edit> = std::collections::BTreeMap::new();
    for edit in edits {
        let Some(script) = scripts.get(edit.file) else {
            bail!("{}: there is no file {} (of {})", edit.what, edit.file, scripts.len());
        };
        if edit.statement >= script.statements.len() {
            bail!("{}: the file has no statement {}", edit.what, edit.statement);
        }
        if let To::Insert(_) = edit.to {
            match records.get(&(edit.file, edit.what.as_str())) {
                Some(other) if other.to != edit.to || other.statement != edit.statement => {
                    bail!("{} is given two ways, by mod {} and mod {}", edit.what, other.module, edit.module)
                }
                Some(_) => {}
                None => {
                    if let To::Insert(text) = &edit.to {
                        for statement in Script::parse(text)?.statements {
                            if statement.args().len() > ARGS_MOST {
                                bail!(
                                    "{}: {} arguments; the engine reads at most {ARGS_MOST} a statement",
                                    edit.what,
                                    statement.args().len()
                                );
                            }
                        }
                    }
                    records.insert((edit.file, edit.what.as_str()), edit);
                    inserted.entry((edit.file, edit.statement)).or_default().push(edit);
                }
            }
            continue;
        }
        let slot = match edit.to {
            To::Arg(_) => edit.arg,
            To::Rest(_) | To::Span(..) | To::Insert(_) => usize::MAX,
        };
        if let Some(other) = done.get(&(edit.file, edit.statement, slot)) {
            if other.to != edit.to || other.arg != edit.arg {
                bail!("{} is given two values, by mod {} and mod {}", edit.what, other.module, edit.module);
            }
            continue;
        }
        done.insert((edit.file, edit.statement, slot), edit);
        match &edit.to {
            To::Arg(text) => {
                let statement = &mut scripts[edit.file].statements[edit.statement];
                let Some(piece) = statement.pieces.get_mut(edit.arg) else {
                    bail!("{}: {} has no argument {}", edit.what, statement.tag, edit.arg);
                };
                *piece = piece.with_text(text.clone());
            }
            _ => lists.push(edit),
        }
    }
    for list in &lists {
        overwritten_args(&scripts, &done, list)?;
    }
    let pieces = |texts: &[Vec<u8>]| -> Vec<Piece> {
        texts.iter().map(|text| Piece { before: Vec::new(), text: text.clone(), after: Vec::new() }).collect()
    };
    for edit in lists {
        let statement = &mut scripts[edit.file].statements[edit.statement];
        let counted = statement.args().len();
        match &edit.to {
            To::Rest(texts) => {
                let end = statement.pieces.last().map(|last| last.after.clone()).unwrap_or_default();
                // an empty last piece holds only the spaces before the ';'
                let trailing = statement.pieces.len() > counted;
                // the spaces between the tag and its first argument stay
                let before =
                    statement.pieces.get(edit.arg).filter(|_| edit.arg < counted).map(|piece| piece.before.clone());
                statement.pieces.truncate(edit.arg.min(counted));
                let mut new = pieces(texts);
                if let (Some(first), Some(before)) = (new.first_mut(), before) {
                    first.before = before;
                }
                statement.pieces.extend(new);
                if trailing || statement.pieces.is_empty() {
                    statement.pieces.push(Piece { before: Vec::new(), text: Vec::new(), after: end });
                } else if let Some(last) = statement.pieces.last_mut() {
                    last.after = end;
                }
            }
            To::Span(texts, tail) => {
                let Some(end) = counted.checked_sub(*tail).filter(|&end| end >= edit.arg) else {
                    bail!("{}: the statement has only {counted} arguments", edit.what);
                };
                let before = statement.pieces.get(edit.arg).map(|piece| piece.before.clone());
                let kept = statement.pieces.split_off(end);
                statement.pieces.truncate(edit.arg);
                let mut new = pieces(texts);
                if let (Some(first), Some(before)) = (new.first_mut(), before) {
                    first.before = before;
                }
                statement.pieces.extend(new);
                statement.pieces.extend(kept);
            }
            To::Arg(_) | To::Insert(_) => unreachable!(),
        }
        // a first argument written right after the tag would join it
        if let Some(first) = statement.pieces.first_mut() {
            if first.before.is_empty() && !first.text.is_empty() {
                first.before = b" ".to_vec();
            }
        }
        if statement.args().len() > ARGS_MOST {
            bail!(
                "{}: {} arguments; the engine reads at most {ARGS_MOST} a statement",
                edit.what,
                statement.args().len()
            );
        }
    }
    let mut out = Vec::new();
    for (file, script) in scripts.iter().enumerate() {
        let mut data = Vec::new();
        for (at, statement) in script.statements.iter().enumerate() {
            statement.write_into(&mut data);
            for edit in inserted.get(&(file, at)).into_iter().flatten() {
                if let To::Insert(text) = &edit.to {
                    data.extend_from_slice(text);
                }
            }
        }
        data.extend_from_slice(&script.tail);
        out.push(data);
    }
    Ok(out)
}

/// Refuses an argument a list of its statement writes over with other text, which would be lost:
/// a Rest from its argument on, a Span up to its tail.
fn overwritten_args(
    scripts: &[Script],
    done: &std::collections::BTreeMap<(usize, usize, usize), &Edit>,
    list: &Edit,
) -> Result<()> {
    let statement = &scripts[list.file].statements[list.statement];
    let (texts, end) = match &list.to {
        To::Rest(texts) => (texts, usize::MAX),
        To::Span(texts, tail) => (texts, statement.args().len().saturating_sub(*tail)),
        To::Arg(_) | To::Insert(_) => return Ok(()),
    };
    // the single arguments' slots, below the lists' (usize::MAX)
    let singles =
        done.range((list.file, list.statement, list.arg)..(list.file, list.statement, end.min(usize::MAX - 1)));
    for (&(.., arg), single) in singles {
        let To::Arg(text) = &single.to else {
            continue;
        };
        if texts.get(arg - list.arg) != Some(text) {
            bail!(
                "{} and {} both change {} argument {arg}, by mod {} and mod {}",
                single.what,
                list.what,
                statement.tag,
                single.module,
                list.module
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &[u8] =
        b"// items\r\nWEPNUM 2;\r\nWEP_ST2 2,0, 5 ,0;\r\nGRD 261,,,;\r\n/* end */ NAME \"a,b;\\\"c\";\r\nEMPTY;\r\n";

    #[test]
    fn scripts_round_trip_and_read_as_the_engine_does() {
        let script = Script::parse(SAMPLE).unwrap();
        assert_eq!(script.write(), SAMPLE);
        let tags: Vec<&str> = script.statements.iter().map(|statement| statement.tag.as_str()).collect();
        assert_eq!(tags, ["WEPNUM", "WEP_ST2", "GRD", "NAME", "EMPTY"]);
        assert_eq!(script.statements[0].before, b"// items\r\n");
        assert_eq!(script.statements[1].values(), [Arg::Int(2), Arg::Int(0), Arg::Int(5), Arg::Int(0)]);
        assert_eq!(script.statements[1].pieces[2].before, b" ");
        // empty arguments count, but not the last before the ';'
        assert_eq!(script.statements[2].values(), [Arg::Int(261), Arg::Invalid, Arg::Invalid]);
        assert_eq!(script.statements[3].values(), [Arg::Str(b"a,b;\"c".to_vec())]);
        assert_eq!(script.statements[4].values(), []);
        assert_eq!(script.tail, b"\r\n");
    }

    #[test]
    fn values_follow_get_arg() {
        let value = |text: &[u8]| Piece::split(text).value();
        assert_eq!(value(b" -12 "), Arg::Int(-12));
        assert_eq!(value(b"1.5"), Arg::Float(1.5));
        assert_eq!(value(b"12 34"), Arg::Int(12));
        assert_eq!(value(b"1x"), Arg::Invalid);
        assert_eq!(value(b""), Arg::Invalid);
        assert_eq!(value(b"\"\""), Arg::Str(Vec::new()));
        assert_eq!(value(b"\"\x83\\\""), Arg::Str(b"\x83\\".to_vec()));
        assert_eq!(Arg::Float(2.9).int(), 2);
    }

    #[test]
    fn a_changed_argument_keeps_its_spaces() {
        let mut script = Script::parse(SAMPLE).unwrap();
        let piece = &script.statements[1].pieces[2];
        script.statements[1].pieces[2] = piece.with_text(int_text(-7));
        let written = script.write();
        let wanted = String::from_utf8_lossy(SAMPLE).replacen(" 5 ,", " -7 ,", 1);
        assert_eq!(written, wanted.as_bytes());
        assert_eq!(Script::parse(&written).unwrap().statements[1].values()[2], Arg::Int(-7));
        assert_eq!(str_text(b"a\"b"), b"\"a\\\"b\"");
    }

    #[test]
    fn what_cannot_be_kept_is_refused() {
        assert!(Script::parse(b"BIN\0").is_err());
        assert!(Script::parse(b"TAG 1 // two\r\n,2;").is_err());
        assert!(Script::parse(b"TAG \"//x\";").is_err());
        assert!(Script::parse(b"TAG 1,2").is_err());
    }

    #[test]
    fn edits_change_arguments_and_lists() {
        let script = Script::parse(b"SHOP 0,1,22;\r\nSHOP 33;\r\nPRICE 11, 450 ,225;\r\n").unwrap();
        let edit = |statement: usize, arg: usize, to: To, module: &str| Edit {
            file: 0,
            statement,
            arg,
            to,
            what: format!("statement {statement}"),
            module: module.into(),
        };
        let list = |items: &[&str]| To::Rest(items.iter().map(|item| item.as_bytes().to_vec()).collect());
        let edits = [
            edit(0, 1, list(&["5"]), "a"),
            edit(1, 1, list(&["7", "8"]), "a"),
            edit(2, 1, To::Arg(b"500".to_vec()), "a"),
            edit(2, 1, To::Arg(b"500".to_vec()), "b"),
        ];
        let written = apply_edits(std::slice::from_ref(&script), &edits).unwrap();
        assert_eq!(written[0], b"SHOP 0,5;\r\nSHOP 33,7,8;\r\nPRICE 11, 500 ,225;\r\n");
        let emptied = apply_edits(std::slice::from_ref(&script), &[edit(0, 1, list(&[]), "a")]).unwrap();
        assert!(emptied[0].starts_with(b"SHOP 0;\r\n"));
        let error =
            apply_edits(&[script], &[edit(2, 1, To::Arg(b"1".to_vec()), "a"), edit(2, 1, To::Arg(b"2".to_vec()), "b")])
                .unwrap_err()
                .to_string();
        assert!(error.contains("mod a and mod b"), "{error}");
    }

    #[test]
    fn inserts_add_statements_once_and_clash_by_record() {
        let script = Script::parse(b"WEPNUM 1;\r\nWEP 1,2;\r\nWEP_BUILD 0;\r\n// end\r\n").unwrap();
        let edit = |statement: usize, to: To, what: &str, module: &str| Edit {
            file: 0,
            statement,
            arg: 0,
            to,
            what: what.into(),
            module: module.into(),
        };
        let block = block_text(&script.statements[1..3]);
        assert_eq!(block, b"\r\nWEP 1,2;\r\nWEP_BUILD 0;");
        let new = To::Insert(statement_text("WEP", &[b"3".to_vec(), b"4".to_vec()]));
        let edits = [
            edit(2, new.clone(), "weapon 1", "a"),
            edit(2, new.clone(), "weapon 1", "b"),
            edit(2, To::Insert(block.clone()), "weapon 2", "a"),
            edit(0, To::Arg(b"3".to_vec()), "count", "a"),
        ];
        let written = apply_edits(std::slice::from_ref(&script), &edits).unwrap();
        assert_eq!(
            String::from_utf8_lossy(&written[0]),
            "WEPNUM 3;\r\nWEP 1,2;\r\nWEP_BUILD 0;\r\nWEP 3,4;\r\nWEP 1,2;\r\nWEP_BUILD 0;\r\n// end\r\n"
        );
        let clash = [edit(2, new, "weapon 1", "a"), edit(2, To::Insert(block), "weapon 1", "b")];
        let error = apply_edits(&[script], &clash).unwrap_err().to_string();
        assert!(error.contains("mod a and mod b"), "{error}");
    }

    #[test]
    fn an_edit_of_what_the_scripts_lack_is_refused() {
        let script = Script::parse(b"MES_SYS 5;\r\n").unwrap();
        let edit = |file: usize, statement: usize, arg: usize| Edit {
            file,
            statement,
            arg,
            to: To::Arg(b"\"Wrench\"".to_vec()),
            what: "item 5 name".into(),
            module: "m".into(),
        };
        let refused = |edit: Edit| apply_edits(std::slice::from_ref(&script), &[edit]).unwrap_err().to_string();
        assert_eq!(refused(edit(0, 0, 1)), "item 5 name: MES_SYS has no argument 1");
        assert_eq!(refused(edit(0, 3, 0)), "item 5 name: the file has no statement 3");
        assert_eq!(refused(edit(2, 0, 0)), "item 5 name: there is no file 2 (of 1)");
    }

    #[test]
    fn an_argument_a_list_writes_over_is_refused_unless_the_list_agrees() {
        let script = Script::parse(b"SHOP 0,1,22;\r\nDATASET 309,4,11,12,210,5,0,0.99;\r\n").unwrap();
        let edit = |statement: usize, arg: usize, to: To, module: &str| Edit {
            file: 0,
            statement,
            arg,
            to,
            what: format!("statement {statement} arg {arg}"),
            module: module.into(),
        };
        let texts = |items: &[&str]| items.iter().map(|item| item.as_bytes().to_vec()).collect::<Vec<_>>();
        let apply = |edits: &[Edit]| apply_edits(std::slice::from_ref(&script), edits);
        // the list from argument 1 puts 7 at argument 2, where the single edit puts 8
        let rest = edit(0, 1, To::Rest(texts(&["5", "7"])), "a");
        let error = apply(&[edit(0, 2, To::Arg(b"8".to_vec()), "b"), rest.clone()]).unwrap_err().to_string();
        assert!(error.contains("argument 2") && error.contains("mod b and mod a"), "{error}");
        // the same text, or an argument before the list, is kept
        assert!(apply(&[edit(0, 2, To::Arg(b"7".to_vec()), "b"), rest.clone()]).is_ok());
        let written = apply(&[edit(0, 0, To::Arg(b"3".to_vec()), "b"), rest]).unwrap();
        assert!(written[0].starts_with(b"SHOP 3,5,7;"));
        // inside a span, refused; in the arguments after it, kept
        let span = edit(1, 4, To::Span(texts(&["1", "2"]), 2), "a");
        assert!(apply(&[edit(1, 5, To::Arg(b"9".to_vec()), "b"), span.clone()]).is_err());
        let written = apply(&[edit(1, 7, To::Arg(b"2.5".to_vec()), "b"), span]).unwrap();
        assert!(written[0].ends_with(b"DATASET 309,4,11,12,1,2,0,2.5;\r\n"));
    }

    #[test]
    fn a_span_replaces_a_list_inside_a_statement() {
        let script = Script::parse(b"DATASET 309,4,11,12,210,5,216,4,0,0.99,-10.4,0.72,0;\r\n").unwrap();
        let edit =
            |to: To, arg: usize| Edit { file: 0, statement: 0, arg, to, what: "recipe".into(), module: "m".into() };
        let span = To::Span(vec![b"1".to_vec(), b"2".to_vec()], 5);
        let edits = [edit(span, 4), edit(To::Arg(b"7".to_vec()), 9)];
        let written = apply_edits(std::slice::from_ref(&script), &edits).unwrap();
        assert_eq!(written[0], b"DATASET 309,4,11,12,1,2,0,7,-10.4,0.72,0;\r\n");
    }
}
