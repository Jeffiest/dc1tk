//! A character or pack info.cfg read as the engine's CScriptInterpreter reads it: Shift-JIS text,
//! comments blanked, statements `TAG arg, arg, ...;`, an argument a quoted string, a float when it
//! has a '.', otherwise an int.

use dc2tk_disc::text;

/// The characters of cp932 text, each with the offset of its first byte, decoded as
/// `text::decode_lossy` decodes.
fn chars_at(data: &[u8]) -> Vec<(char, usize)> {
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        let lead = matches!(data[i], 0x81..=0x9F | 0xE0..=0xFC);
        let width = if lead && data.get(i..i + 2).and_then(text::decode_strict).is_some() { 2 } else { 1 };
        out.extend(text::decode_lossy(&data[i..i + width]).chars().map(|c| (c, i)));
        i += width;
    }
    out
}

/// The text with its comments taken out (a block comment leaves a space), each character keeping
/// its byte offset.
fn strip_comments(t: &[(char, usize)]) -> Vec<(char, usize)> {
    let mut out = Vec::with_capacity(t.len());
    let n = t.len();
    let mut i = 0;
    let starts = |i: usize, a: char, b: char| i + 1 < n && t[i].0 == a && t[i + 1].0 == b;
    while i < n {
        if starts(i, '/', '/') {
            i = (i..n).find(|&j| t[j].0 == '\n').unwrap_or(n);
        } else if starts(i, '/', '*') {
            out.push((' ', t[i].1));
            i = (i + 2..n).find(|&j| starts(j, '*', '/')).map_or(n, |j| j + 2);
        } else {
            out.push(t[i]);
            i += 1;
        }
    }
    out
}

/// An argument of a statement: a quoted string, a float when it has a '.', otherwise an int; Null
/// for one that reads as none of them. A glTF's extras hold arguments as JSON: null, a number or
/// a string.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(untagged)]
pub enum Arg {
    Null,
    Int(i64),
    Float(f64),
    Str(String),
}

impl Arg {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Arg::Str(text) => Some(text),
            _ => None,
        }
    }

    /// A number's value, an int's as a float.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Arg::Int(value) => Some(*value as f64),
            Arg::Float(value) => Some(*value),
            _ => None,
        }
    }

    /// Whether the argument gives something: it is not Null, zero or an empty string.
    pub fn is_set(&self) -> bool {
        match self {
            Arg::Null => false,
            Arg::Int(value) => *value != 0,
            Arg::Float(value) => *value != 0.0,
            Arg::Str(text) => !text.is_empty(),
        }
    }

    /// The argument as names write it: a string as it is, a number in decimal (a float as the
    /// shortest text that reads back as it), Null as nothing.
    pub fn text(&self) -> String {
        match self {
            Arg::Null => String::new(),
            Arg::Int(value) => value.to_string(),
            Arg::Float(value) => format!("{value:?}"),
            Arg::Str(text) => text.clone(),
        }
    }
}

/// An argument's value. The game's files write numbers plainly (an optional sign, digits, and for
/// a float a point), which is what str::parse reads.
fn value(arg: &str) -> Arg {
    let a = arg.trim();
    let chars: Vec<char> = a.chars().collect();
    if chars.len() >= 2 && chars[0] == '"' && chars[chars.len() - 1] == '"' {
        let inner: String = chars[1..chars.len() - 1].iter().collect();
        return Arg::Str(inner.replace("\\\"", "\""));
    }
    if a.contains('.') {
        return a.parse().map_or(Arg::Null, Arg::Float);
    }
    a.parse().map_or(Arg::Null, Arg::Int)
}

/// A statement with where its parts are in the file's bytes.
#[derive(Clone, Debug)]
pub struct Statement {
    pub tag: String,
    /// Each argument with the bytes of its text, surrounding spaces left out.
    pub args: Vec<(Arg, std::ops::Range<usize>)>,
    /// Where the tag starts.
    pub start: usize,
    /// Just after the statement's ';' (the end of the file when it has none).
    pub end: usize,
}

/// Every statement, in file order, with where it is.
pub fn statements(data: &[u8]) -> Vec<Statement> {
    let t = strip_comments(&chars_at(data));
    let n = t.len();
    let width = |c: char| text::encode(&c.to_string()).map_or(1, |bytes| bytes.len());
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        while i < n && matches!(t[i].0, ' ' | '\t' | '\r' | '\n') {
            i += 1;
        }
        if i >= n {
            break;
        }
        let mut j = i;
        while j < n && (t[j].0.is_alphanumeric() || t[j].0 == '_') {
            j += 1;
        }
        let tag: String = t[i..j].iter().map(|&(c, _)| c).collect();
        let mut args = Vec::new();
        let mut current: Vec<(char, usize)> = Vec::new();
        let mut quoted = false;
        let mut k = j;
        while k < n {
            let c = t[k].0;
            if c == '"' {
                quoted = !quoted;
            }
            if !quoted && (c == ',' || c == ';') {
                let text: String = current.iter().map(|&(c, _)| c).collect();
                if !text.trim().is_empty() {
                    let first = current.iter().position(|&(c, _)| !c.is_whitespace()).unwrap_or(0);
                    let last = current.iter().rposition(|&(c, _)| !c.is_whitespace()).unwrap_or(0);
                    let (c, offset) = current[last];
                    args.push((value(&text), current[first].1..offset + width(c)));
                }
                current.clear();
                if c == ';' {
                    break;
                }
            } else {
                current.push(t[k]);
            }
            k += 1;
        }
        let empty = tag.is_empty();
        if !empty {
            let end = if k < n { t[k].1 + 1 } else { data.len() };
            out.push(Statement { tag, args, start: t[i].1, end });
        }
        i = k + 1;
        if empty {
            i = i.max(j + 1);
        }
    }
    out
}

/// (tag, args) of every statement, in file order.
pub fn parse(data: &[u8]) -> Vec<(String, Vec<Arg>)> {
    statements(data).into_iter().map(|s| (s.tag, s.args.into_iter().map(|(value, _)| value).collect())).collect()
}

/// A motion set: its .mot, .bbp and .wgt and the named clips (KEY lines).
#[derive(Clone, Debug)]
pub struct Motion {
    pub mot: Arg,
    pub bbp: Arg,
    pub wgt: Arg,
    pub keys: Vec<Key>,
}

/// A KEY line: a clip's name and frames.
#[derive(Clone, Debug)]
pub struct Key {
    pub name: Arg,
    pub start: Arg,
    pub end: Arg,
    pub step: Arg,
}

/// Whether two arguments are the same: numbers by value (an int equals the float of the same
/// value), strings by their text.
pub fn same_value(a: &Arg, b: &Arg) -> bool {
    match (a, b) {
        (Arg::Int(x), Arg::Float(y)) | (Arg::Float(y), Arg::Int(x)) => *x as f64 == *y,
        (Arg::Int(x), Arg::Int(y)) => x == y,
        (Arg::Float(x), Arg::Float(y)) => x == y,
        (Arg::Str(x), Arg::Str(y)) => x == y,
        (Arg::Null, Arg::Null) => true,
        _ => false,
    }
}

/// The parts of a character info.cfg that matter for its model.
#[derive(Clone, Debug, Default)]
pub struct CharInfo {
    pub name: Option<Arg>,
    pub model: Option<Arg>,
    pub shadow_model: Option<Arg>,
    pub imgs: Vec<(Arg, Arg)>,
    pub vertex_anime: Vec<String>,
    pub shape_anime: Option<Arg>,
    /// Motion sets by id, in the order the ids were first given.
    pub motions: Vec<(Arg, Motion)>,
    pub shadow_motion: Option<(Arg, Arg, Arg)>,
    pub lod: Vec<Vec<Arg>>,
}

impl CharInfo {
    pub fn new(statements: &[(String, Vec<Arg>)]) -> CharInfo {
        let mut info = CharInfo::default();
        let mut current: Option<usize> = None;
        for (tag, a) in statements {
            match tag.as_str() {
                "NAME" if !a.is_empty() => info.name = Some(a[0].clone()),
                "MODEL" if !a.is_empty() => info.model = Some(a[0].clone()),
                "SHADOW_MODEL" if !a.is_empty() => info.shadow_model = Some(a[0].clone()),
                "IMG" if a.len() >= 2 => info.imgs.push((a[0].clone(), a[1].clone())),
                "VERTEX_ANIME" => {
                    info.vertex_anime.extend(a.iter().filter_map(|x| x.as_str().map(str::to_string)));
                }
                "SHAPE_ANIME" if !a.is_empty() => info.shape_anime = Some(a[0].clone()),
                "MOTION" if a.len() >= 4 => {
                    let motion = Motion { mot: a[1].clone(), bbp: a[2].clone(), wgt: a[3].clone(), keys: Vec::new() };
                    // a dict assignment: an id given again keeps its place and takes the new set
                    match info.motions.iter().position(|(id, _)| same_value(id, &a[0])) {
                        Some(at) => {
                            info.motions[at].1 = motion;
                            current = Some(at);
                        }
                        None => {
                            info.motions.push((a[0].clone(), motion));
                            current = Some(info.motions.len() - 1);
                        }
                    }
                }
                "SHADOW_MOTION" if a.len() >= 3 => {
                    info.shadow_motion = Some((a[0].clone(), a[1].clone(), a[2].clone()))
                }
                "KEY" if a.len() >= 4 => {
                    if let Some(at) = current {
                        info.motions[at].1.keys.push(Key {
                            name: a[0].clone(),
                            start: a[1].clone(),
                            end: a[2].clone(),
                            step: a[3].clone(),
                        });
                    }
                }
                "LOD_MODEL" if a.len() >= 5 => info.lod.push(a[..5].to_vec()),
                _ => {}
            }
        }
        info
    }

    /// The motion set with id 0 (an int or a float of that value).
    pub fn motion0(&self) -> Option<&Motion> {
        self.motions.iter().find(|(id, _)| same_value(id, &Arg::Int(0))).map(|(_, m)| m)
    }
}

pub fn read(data: &[u8]) -> CharInfo {
    CharInfo::new(&parse(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statements_parse_as_the_engine_reads_them() {
        let text = b"v2;\n// comment\nMODEL \"c.mds\";\nMOTION 0, \"a.mot\", \"a.bbp\", \"a.wgt\";\n/* x */KEY \"walk\", 0, 10, 1.0;\nSCALE +5, x;";
        let statements = parse(text);
        let tags: Vec<&str> = statements.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(tags, ["v2", "MODEL", "MOTION", "KEY", "SCALE"]);
        assert!(matches!(statements[4].1[..], [Arg::Int(5), Arg::Null]));
        let info = CharInfo::new(&statements);
        assert_eq!(info.model.as_ref().and_then(Arg::as_str), Some("c.mds"));
        let motion = info.motion0().unwrap();
        assert_eq!(motion.keys.len(), 1);
        assert!(matches!(motion.keys[0].step, Arg::Float(x) if x == 1.0));
    }
}
