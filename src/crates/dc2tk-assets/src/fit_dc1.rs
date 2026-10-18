//! Making a built Dark Cloud 1 file fit where the game's went (MoonMod's game.resources.override
//! takes a replacement only when it is no larger than the retail file): the slack the game's
//! files carry, taken out without changing what the game reads.
//!
//! - Tag scripts (.cfg, loose or in a pack): comments (`//`, `/* */`), trailing spaces and blank
//!   lines go; the interpreter skips them (scriptinterpreter.cpp).
//! - Models (.mds, loose or in a pack): every mesh written in the packed layout (no zero gaps
//!   between sections; mds::Mdt style 'B'); the loader follows the header's offsets.
//!
//! Each is tried only while the file is still too big, cheapest first.

use dc2tk_disc::archive::Pack;
use dc2tk_disc::text;

use crate::{mds, Result};

/// A tag script without its comments, trailing spaces and blank lines, or None when it is not
/// Shift-JIS text. Quoted strings are kept whole.
pub fn strip_cfg(data: &[u8]) -> Option<Vec<u8>> {
    let source = text::decode_strict(data)?;
    let newline = if source.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out = String::new();
    let mut in_block = false;
    for line in source.lines() {
        let mut kept = String::new();
        let mut chars = line.chars().peekable();
        let mut quoted = false;
        while let Some(c) = chars.next() {
            if in_block {
                if c == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    in_block = false;
                    kept.push(' ');
                }
                continue;
            }
            if c == '"' {
                quoted = !quoted;
            }
            if !quoted && c == '/' && chars.peek() == Some(&'/') {
                break;
            }
            if !quoted && c == '/' && chars.peek() == Some(&'*') {
                chars.next();
                in_block = true;
                continue;
            }
            kept.push(c);
        }
        let kept = kept.trim_end();
        if !kept.trim().is_empty() {
            out.push_str(kept);
            out.push_str(newline);
        }
    }
    text::encode(&out)
}

/// An MDS file with every mesh in the packed layout, or None when it does not read.
pub fn pack_mds(data: &[u8]) -> Option<Vec<u8>> {
    let mut model = mds::parse(data).ok()?;
    for node in &mut model.nodes {
        if let Some(mdt) = node.mdt.as_mut() {
            mdt.layout = None;
            mdt.faces_pad = None;
            mdt.style = 'B';
        }
    }
    mds::build(&model).ok()
}

/// The file made to fit `limit` bytes if it can be, by the steps above in turn, with what each
/// saved; None when it still does not fit.
pub fn fit(path: &str, data: &[u8], limit: usize) -> Result<(Option<Vec<u8>>, Vec<String>)> {
    let lower = path.to_lowercase();
    let mut notes = Vec::new();
    let mut current = data.to_vec();
    let done = |current: &Vec<u8>| current.len() <= limit;
    if lower.ends_with(".cfg") {
        if let Some(stripped) = strip_cfg(&current).filter(|s| s.len() < current.len()) {
            notes.push(format!("comments taken out: {} bytes", current.len() - stripped.len()));
            current = stripped;
        }
    } else if lower.ends_with(".mds") {
        if let Some(packed) = pack_mds(&current).filter(|p| p.len() < current.len()) {
            notes.push(format!("meshes packed: {} bytes", current.len() - packed.len()));
            current = packed;
        }
    } else if let Some(mut pack) = Pack::read(&current) {
        // a pack: its cfgs first, then its models, largest saving first, until it fits
        for kind in [".cfg", ".mds"] {
            if done(&pack.write()?) {
                break;
            }
            let mut savings: Vec<(usize, usize, Vec<u8>)> = Vec::new();
            for (at, member) in pack.members.iter().enumerate() {
                if !member.name.to_lowercase().ends_with(kind) {
                    continue;
                }
                let smaller = if kind == ".cfg" { strip_cfg(&member.data) } else { pack_mds(&member.data) };
                if let Some(smaller) = smaller.filter(|s| s.len() < member.data.len()) {
                    savings.push((member.data.len() - smaller.len(), at, smaller));
                }
            }
            savings.sort_by(|a, b| b.0.cmp(&a.0));
            for (saved, at, smaller) in savings {
                if done(&pack.write()?) {
                    break;
                }
                notes.push(format!("{}: {saved} bytes", pack.members[at].name));
                pack.members[at].data = smaller;
            }
        }
        current = pack.write()?;
    }
    Ok((done(&current).then_some(current), notes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_go_and_statements_stay() {
        let cfg = b"// head\r\n\tFOG 1, 2 // near\r\n\r\n/* a\r\nb */ SET_PATH \"a//b\"\r\n";
        let stripped = strip_cfg(cfg).unwrap();
        assert_eq!(String::from_utf8(stripped.clone()).unwrap(), "\tFOG 1, 2\r\n  SET_PATH \"a//b\"\r\n");
        assert_eq!(crate::info_cfg::parse(&stripped), crate::info_cfg::parse(cfg));
    }
}
