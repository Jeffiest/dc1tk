//! Message files (.mes, and the fish race menu's fconv<N>.bin): the menu, item, map and shop
//! names, NPC lines and system messages the game draws from tables of 16-bit codes, as text a
//! person edits (see docs/assets/text.md).
//!
//! ```text
//! u16 count, u16 size                      size: bytes of the header and messages, cut to 16 bits
//! count x (u16 id, u16 offset)             offset in words from word 1 + count
//! u16 FF01 FF00                            then each message: its codes, FF01, FF00
//! ```
//!
//! A code below 0xF000 is a glyph of the language's font; the rest are spaces, line and page
//! breaks, icons, number, item and name slots, colours and waits (ClsMes::MakeMesWinTbl,
//! nd_meswin.cpp). The text form writes each message after an `@id` line, glyphs as the characters
//! the font draws, the codes as the tags event text uses (`(O)`, `[Number1]`, `<page>`) or in
//! braces (`{wait 12}`), and whatever has no name as `{XXXX}`, so every file reads back to its
//! own bytes. A message file is read whole (Mes keeps every word between messages); a mod's text
//! changes the messages whose text differs from the disc's and adds new ids, leaving the rest as
//! they are.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::tree::{self, Place};
use crate::{bail, Error, Result};

/// The English font's table: the Shift-JIS code of each glyph, in glyph order (LoadFontTblBin,
/// font.cpp). The menus' English glyphs follow it.
pub const ENGLISH_FONT: &str = "meswin/fonttbl_1.bin";
/// Glyphs the English font's texture holds (meswin/FontTex_1_0.tm2: 9 rows of 32), of the table's
/// 985.
pub const ENGLISH_GLYPHS: usize = 288;

pub const NEWLINE: u16 = 0xFF00;
pub const END: u16 = 0xFF01;
pub const SPACE: u16 = 0xFF02;
pub const PAGE: u16 = 0xFF03;
/// Codes from here up are not glyphs.
pub const CODES: u16 = 0xF000;

/// The languages, by the game's LanguageCode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Language {
    Japanese,
    English,
    French,
    German,
    Italian,
    Spanish,
    Korean,
}

pub const LANGUAGES: [Language; 7] = [
    Language::Japanese,
    Language::English,
    Language::French,
    Language::German,
    Language::Italian,
    Language::Spanish,
    Language::Korean,
];

impl Language {
    pub fn from_code(code: u32) -> Option<Language> {
        LANGUAGES.get(code as usize).copied()
    }

    /// The short name ids use (text:en/allmenu).
    pub fn short(self) -> &'static str {
        ["ja", "en", "fr", "de", "it", "es", "ko"][self as usize]
    }

    pub fn from_short(short: &str) -> Option<Language> {
        LANGUAGES.into_iter().find(|language| language.short().eq_ignore_ascii_case(short))
    }

    pub fn name(self) -> &'static str {
        ["Japanese", "English", "French", "German", "Italian", "Spanish", "Korean"][self as usize]
    }

    fn european(self) -> bool {
        matches!(self, Language::French | Language::German | Language::Italian | Language::Spanish)
    }
}

/// Where a message file sits says its language and the name its id goes by: menu/<N>/ packs and a
/// language number in the file name (system_1.mes, help1.mes) are the game's own (rank 0), a file
/// without one is Japanese (rank 1), and the old language folders (meswin/usa, meswin/eu/fra,
/// meswin/kor, meswin/try), which the game does not read, come last (rank 2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Located {
    pub language: Language,
    pub name: String,
    pub rank: u8,
}

/// The file names the game makes with a language number straight after them (help%d.mes).
const NUMBERED: [&str; 4] = ["help", "hddchk", "gyore", "fconv"];

fn digit_language(digit: char) -> Option<Language> {
    Language::from_code(digit.to_digit(10)?)
}

/// Whether a file name is a message file: .mes, or fconv.bin and the fish race menu's
/// fconv<N>.bin (menu/aqua/mes), which are laid out as one.
pub fn is_message_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    let file = lower.rsplit('/').next().unwrap_or(&lower);
    let fconv = file.strip_prefix("fconv").and_then(|rest| rest.strip_suffix(".bin"));
    let numbered = |digit: &str| digit.is_empty() || (digit.len() == 1 && digit.as_bytes()[0].is_ascii_digit());
    file.ends_with(".mes") || fconv.is_some_and(numbered)
}

/// The language and name of the message file at a path as extract lays it out
/// (menu/1/shop.pac.d/shopname.mes); None for a file that is not one.
pub fn locate(path: &str) -> Option<Located> {
    if !is_message_file(path) {
        return None;
    }
    let lower = path.replace('\\', "/").to_lowercase();
    let (folder, file) = lower.rsplit_once('/').unwrap_or(("", &lower));
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    let located = |language: Language, name: &str, rank: u8| Some(Located { language, name: name.to_string(), rank });
    let parts: Vec<&str> = lower.split('/').collect();
    if parts.len() > 2 && parts[0] == "menu" && parts[1].len() == 1 {
        if let Some(language) = parts[1].chars().next().and_then(digit_language) {
            return located(language, stem, 0);
        }
    }
    let folders = [
        ("meswin/eu/fra", Language::French),
        ("meswin/eu/ger", Language::German),
        ("meswin/eu/ita", Language::Italian),
        ("meswin/eu/spn", Language::Spanish),
        ("meswin/usa", Language::English),
        ("meswin/kor", Language::Korean),
        ("meswin/try", Language::Japanese),
    ];
    if let Some((_, language)) = folders.iter().find(|(prefix, _)| folder == *prefix) {
        return located(*language, stem, 2);
    }
    if let Some((name, digit)) = stem.rsplit_once('_') {
        let mut chars = digit.chars();
        if let (Some(digit), None) = (chars.next(), chars.next()) {
            if let Some(language) = digit_language(digit).filter(|_| !name.is_empty()) {
                return located(language, name, 0);
            }
        }
    }
    for numbered in NUMBERED {
        if let Some(digit) = stem.strip_prefix(numbered) {
            let mut chars = digit.chars();
            if let (Some(digit), None) = (chars.next(), chars.next()) {
                if let Some(language) = digit_language(digit) {
                    return located(language, numbered, 0);
                }
            }
        }
    }
    located(Language::Japanese, stem, 1)
}

/// The buffer the game loads a message file into, where it has one of a fixed size: system_N
/// 0xD000 bytes (SystemMesBuffer), sysmes_N 0x13880 (SysMesBuffer), helpN 0x1000 (HelpMesBuff,
/// which LoadHelpMes refuses to overrun). Others load into the menu's or the dungeon's heap with
/// the rest of their pack.
pub fn buffer_size(path: &str) -> Option<usize> {
    let lower = path.replace('\\', "/").to_lowercase();
    let file = lower.rsplit('/').next().unwrap_or(&lower);
    if lower.starts_with("meswin/") && !lower["meswin/".len()..].contains('/') {
        if file.starts_with("system") {
            return Some(0xD000);
        }
        if file.starts_with("sysmes") {
            return Some(0x13880);
        }
    }
    if lower.starts_with("etc/help") {
        return Some(0x1000);
    }
    None
}

/// One run of the message words: a message's codes up to its FF01, and the words after them up
/// to the next message (FF01 FF00 in all but a few files).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub text: Vec<u16>,
    pub rest: Vec<u16>,
}

/// Where a message's offset points: a span, or outside the messages (the offset kept as it is;
/// five ids of an old French file the game does not read).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum At {
    Span(usize),
    Outside(u16),
}

/// A message file, read whole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mes {
    /// Each message's id, in the table's order, and where its text is; two ids can share a span.
    pub entries: Vec<(u16, At)>,
    /// The messages' words in file order.
    pub spans: Vec<Span>,
    /// The words between the table and the first message (FF01 FF00).
    pub lead: Vec<u16>,
    /// Bytes after the messages, when `size` says they end before the file does (four old
    /// Japanese files); the game never reads them.
    pub tail: Vec<u8>,
}

fn word(data: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([data[2 * at], data[2 * at + 1]])
}

impl Mes {
    /// Reads a message file. `size` is the file's length cut to 16 bits (allmenu.mes is 80 KB), so
    /// the messages run to the end of the file when it matches, and to `size` when it is less.
    pub fn parse(data: &[u8]) -> Result<Mes> {
        if data.len() < 4 || !data.len().is_multiple_of(2) {
            bail!("a message file is an even number of bytes, at least 4, not {}", data.len());
        }
        let count = word(data, 0) as usize;
        let size = word(data, 1) as usize;
        let header = 2 + 2 * count;
        let end = if size == data.len() & 0xFFFF { data.len() } else { size };
        if !end.is_multiple_of(2) || end > data.len() || 2 * header > end {
            bail!("a message file of {} bytes with {count} messages cannot end at byte {end}", data.len());
        }
        let words: Vec<u16> = (0..end / 2).map(|at| word(data, at)).collect();
        let base = 1 + count;
        let inside = |offset: u16| (header..=words.len()).contains(&(base + offset as usize));
        let table: Vec<(u16, u16)> = (0..count).map(|i| (words[2 + 2 * i], words[3 + 2 * i])).collect();
        let mut starts: Vec<usize> =
            table.iter().filter(|(_, offset)| inside(*offset)).map(|(_, offset)| base + *offset as usize).collect();
        starts.sort_unstable();
        starts.dedup();
        let lead_end = starts.first().copied().unwrap_or(words.len());
        let spans: Vec<Span> = starts
            .iter()
            .enumerate()
            .map(|(i, &start)| {
                let end = starts.get(i + 1).copied().unwrap_or(words.len());
                let stop = words[start..end].iter().position(|&w| w == END).map_or(end, |at| start + at);
                Span { text: words[start..stop].to_vec(), rest: words[stop..end].to_vec() }
            })
            .collect();
        let entries = table
            .iter()
            .map(|&(id, offset)| {
                if inside(offset) {
                    (id, At::Span(starts.binary_search(&(base + offset as usize)).expect("a span's start")))
                } else {
                    (id, At::Outside(offset))
                }
            })
            .collect();
        Ok(Mes { entries, spans, lead: words[header..lead_end].to_vec(), tail: data[end..].to_vec() })
    }

    pub fn write(&self) -> Result<Vec<u8>> {
        let count = self.entries.len();
        if count > 0xFFFF {
            bail!("{count} messages; a message file holds at most 65535");
        }
        let header = 2 + 2 * count;
        let base = 1 + count;
        let mut starts = Vec::with_capacity(self.spans.len());
        let mut at = header + self.lead.len();
        for span in &self.spans {
            starts.push(at);
            at += span.text.len() + span.rest.len();
        }
        if let Some(start) = starts.last().filter(|&&start| start - base > 0xFFFF) {
            bail!("the last message starts at word {start}, past the 16-bit offsets' reach");
        }
        if !self.tail.is_empty() && 2 * at > 0xFFFF {
            bail!("the messages take {} bytes, too many for size to say where the bytes after them start", 2 * at);
        }
        let mut words: Vec<u16> = Vec::with_capacity(at);
        words.push(count as u16);
        // the bytes of the header and messages, cut to 16 bits as the game's files have them
        words.push((2 * at) as u16);
        for &(id, target) in &self.entries {
            words.push(id);
            words.push(match target {
                At::Span(span) => (starts[span] - base) as u16,
                At::Outside(offset) => offset,
            });
        }
        words.extend(&self.lead);
        for span in &self.spans {
            words.extend(&span.text);
            words.extend(&span.rest);
        }
        let mut out: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        out.extend(&self.tail);
        Ok(out)
    }

    /// A message's codes, up to its FF01.
    pub fn get(&self, id: u16) -> Option<&[u16]> {
        match self.entries.iter().find(|(known, _)| *known == id)?.1 {
            At::Span(span) => Some(&self.spans[span].text),
            At::Outside(_) => None,
        }
    }

    /// The messages with text, in the table's order: (id, codes).
    pub fn messages(&self) -> impl Iterator<Item = (u16, &[u16])> {
        self.entries.iter().filter_map(|&(id, at)| match at {
            At::Span(span) => Some((id, &self.spans[span].text[..])),
            At::Outside(_) => None,
        })
    }

    /// Gives a message new codes, or adds it at the end. A message sharing its span with another
    /// gets one of its own; a message whose span had no FF01 (its text ran on into the next) gets
    /// FF01 FF00.
    pub fn set(&mut self, id: u16, text: Vec<u16>) {
        let found = self.entries.iter().position(|(known, _)| *known == id);
        if let Some(At::Span(span)) = found.map(|at| self.entries[at].1) {
            if self.entries.iter().filter(|(_, other)| *other == At::Span(span)).count() == 1 {
                let span = &mut self.spans[span];
                span.text = text;
                if span.rest.is_empty() {
                    span.rest = vec![END, NEWLINE];
                }
                return;
            }
        }
        if let Some(last) = self.spans.last_mut().filter(|last| last.rest.is_empty()) {
            // its text ran to the end of the messages; it ends where it did
            last.rest = vec![END];
        }
        self.spans.push(Span { text, rest: vec![END, NEWLINE] });
        let span = At::Span(self.spans.len() - 1);
        match found {
            Some(at) => self.entries[at].1 = span,
            None => self.entries.push((id, span)),
        }
    }
}

/// The accented letters the European fonts draw, as the codes FF00 + their Latin-1 byte the
/// menus write them with (GetAlphabeticalFontNo_us, font.cpp; 0xBD and 0xBE are Œ and œ).
const LATIN: [u8; 63] = [
    0xA1, 0xAA, 0xB0, 0xBA, 0xBF, 0xC0, 0xC1, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xCB, 0xCC, 0xCD,
    0xCE, 0xCF, 0xD1, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6, 0xD9, 0xDA, 0xDB, 0xDC, 0xDD, 0xDF, 0xE0, 0xE1, 0xE2, 0xE3, 0xE4,
    0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA, 0xEB, 0xEC, 0xED, 0xEE, 0xEF, 0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF9, 0xFA,
    0xFB, 0xFC, 0xFD, 0xFF, 0xBD, 0xBE,
];

fn latin_char(byte: u8) -> char {
    match byte {
        0xBD => 'Œ',
        0xBE => 'œ',
        byte => byte as char,
    }
}

/// The full-width symbols the codes FDE0 to FDF7 draw in the European fonts
/// (FontGaijiConvTbl, font.cpp).
const FONT_GAIJI: &str = "○●↑↓←→０１２３４５６７８９：＋－・“”‘’";

/// The icons FD00 to FD31 and the tags event text writes them as (FconvCodeTbl, font.cpp).
const ICONS: [(u16, &str); 44] = [
    (0xFD00, "[select]"),
    (0xFD01, "[start]"),
    (0xFD02, "[l1]"),
    (0xFD03, "[r1]"),
    (0xFD04, "[l2]"),
    (0xFD05, "[r2]"),
    (0xFD06, "(O)"),
    (0xFD07, "(A)"),
    (0xFD08, "(X)"),
    (0xFD09, "(#)"),
    (0xFD0A, "[+]"),
    (0xFD0B, "[-]"),
    (0xFD0C, "[|]"),
    (0xFD0D, "[!]"),
    (0xFD0E, "[heart]"),
    (0xFD0F, "[clef]"),
    (0xFD10, "[dame]"),
    (0xFD11, "[sita]"),
    (0xFD12, "[weapon]"),
    (0xFD13, "[protector]"),
    (0xFD14, "[material]"),
    (0xFD15, "[tool]"),
    (0xFD16, "[parts]"),
    (0xFD17, "[hari]"),
    (0xFD18, "[button]"),
    (0xFD19, "[fish]"),
    (0xFD1A, "[(!)]"),
    (0xFD21, "[bulb]"),
    (0xFD22, "[*]"),
    (0xFD23, "[|~]"),
    (0xFD24, "[cross]"),
    (0xFD25, "[|>]"),
    (0xFD26, "[hp]"),
    (0xFD27, "[mp]"),
    (0xFD28, "[lv]"),
    (0xFD29, "(L)"),
    (0xFD2A, "(R)"),
    (0xFD2B, "[bulb2]"),
    (0xFD2C, "[bulb3]"),
    (0xFD2D, "[tuck]"),
    (0xFD2E, "[spectrum]"),
    (0xFD2F, "(L3)"),
    (0xFD30, "(R3)"),
    (0xFD31, "(regi)"),
];

/// The slots MakeMesWinTbl fills in: numbers (set_message_value), text (set_message_string),
/// items' names (set_message_item) and the names the player gave (the name registry).
fn slot_code(kind: &str, slot: usize) -> Option<u16> {
    let slot = slot as u16;
    match kind {
        "Number" if slot == 0 => Some(0xFBFF),
        "Number" if (1..=8).contains(&slot) => Some(0xFBFA - (slot - 1)),
        "Number" if (9..=16).contains(&slot) => Some(0xFBE6 - (slot - 9)),
        "Item" if (1..=4).contains(&slot) => Some(0xFBFE - (slot - 1)),
        "Item" if (5..=16).contains(&slot) => Some(0xFBF2 - (slot - 5)),
        "String" if (1..=16).contains(&slot) => Some(0xFAF9 - (slot - 1)),
        "Name" if (1..=6).contains(&slot) => Some(0xFAFA + (slot - 1)),
        _ => None,
    }
}

fn slot_of(code: u16) -> Option<(&'static str, usize)> {
    let slot = match code {
        0xFBFF => ("Number", 0),
        0xFBF3..=0xFBFA => ("Number", (0xFBFA - code) as usize + 1),
        0xFBDF..=0xFBE6 => ("Number", (0xFBE6 - code) as usize + 9),
        0xFBFB..=0xFBFE => ("Item", (0xFBFE - code) as usize + 1),
        0xFBE7..=0xFBF2 => ("Item", (0xFBF2 - code) as usize + 5),
        0xFAEA..=0xFAF9 => ("String", (0xFAF9 - code) as usize + 1),
        0xFAFA..=0xFAFF => ("Name", (code - 0xFAFA) as usize + 1),
        _ => return None,
    };
    Some(slot)
}

/// The tag of a slot: [Number1], [Number] for the window's single number.
fn slot_tag(kind: &str, slot: usize) -> String {
    if slot == 0 {
        format!("[{kind}]")
    } else {
        format!("[{kind}{slot}]")
    }
}

/// The Japanese spellings event text uses: [数値１], [文字列１], [アイテム１].
fn japanese_slot_tag(kind: &str, slot: usize) -> Option<String> {
    let word = match kind {
        "Number" => "数値",
        "String" => "文字列",
        "Item" => "アイテム",
        _ => return None,
    };
    let digits: String = if slot == 0 {
        String::new()
    } else {
        slot.to_string().chars().map(|c| char::from_u32(c as u32 - '0' as u32 + '０' as u32).unwrap()).collect()
    };
    Some(format!("[{word}{digits}]"))
}

/// Codes with a byte argument, written {name N}.
const BYTE_CODES: [(&str, u16); 9] = [
    ("alpha", 0xF200),
    ("blue", 0xF300),
    ("green", 0xF400),
    ("red", 0xF500),
    ("justify", 0xF700),
    ("gap", 0xF800),
    ("move", 0xF900),
    ("colour", 0xFC00),
    ("wait", 0xFE00),
];

/// The tags, longest first, and the code each stands for.
fn tags() -> &'static [(String, u16)] {
    static TAGS: std::sync::OnceLock<Vec<(String, u16)>> = std::sync::OnceLock::new();
    TAGS.get_or_init(|| {
        let mut found: Vec<(String, u16)> = ICONS.iter().map(|&(code, tag)| (tag.to_string(), code)).collect();
        found.push(("<page>".to_string(), PAGE));
        for (kind, slots) in [("Number", 0..=16), ("Item", 1..=16), ("String", 1..=16), ("Name", 1..=6)] {
            for slot in slots {
                let code = slot_code(kind, slot).expect("a slot");
                found.push((slot_tag(kind, slot), code));
                if let Some(japanese) = japanese_slot_tag(kind, slot) {
                    found.push((japanese, code));
                }
            }
        }
        found.sort_by_key(|(tag, _)| std::cmp::Reverse(tag.chars().count()));
        found
    })
}

fn tag_at(text: &str) -> Option<(&'static str, u16)> {
    tags().iter().find(|(tag, _)| text.starts_with(tag.as_str())).map(|(tag, code)| (tag.as_str(), *code))
}

/// A language's glyphs and the characters they draw, for the menus' message files.
#[derive(Clone, Debug)]
pub struct Font {
    pub language: Language,
    /// The character of each glyph.
    glyphs: Vec<Option<char>>,
    /// The code each character is written with.
    codes: HashMap<char, u16>,
}

impl Font {
    /// The font of a language. The English glyphs past ASCII are the English font table's
    /// (ENGLISH_FONT, from the disc); without it English has ASCII only, as Japanese and Korean
    /// do: the disc has no table of their menus' glyphs.
    pub fn new(language: Language, english_table: Option<&[u8]>) -> Font {
        // glyphs 0 to 93: printable ASCII without the backslash, in every font
        let mut glyphs: Vec<Option<char>> = (0x20u8..0x7F).filter(|&b| b != b'\\').map(|b| Some(b as char)).collect();
        if language == Language::English {
            if let Some(table) = english_table.filter(|table| table.len() >= 8) {
                let count = ((table.len() - 8) / 2).min(ENGLISH_GLYPHS);
                glyphs = (0..count)
                    .map(|at| {
                        let code = [table[8 + 2 * at], table[9 + 2 * at]];
                        if code[1] == b' ' && code[0] < 0x80 {
                            return Some(code[0] as char);
                        }
                        let text = c2_disc::text::decode_strict(&code)?;
                        let mut chars = text.chars();
                        match (chars.next(), chars.next()) {
                            (Some(c), None) => Some(c),
                            _ => None,
                        }
                    })
                    .collect();
            }
        }
        let mut codes = HashMap::new();
        for (glyph, c) in glyphs.iter().enumerate() {
            if let Some(c) = c {
                codes.entry(*c).or_insert(glyph as u16);
            }
        }
        if language.european() {
            for byte in LATIN {
                codes.insert(latin_char(byte), 0xFF00 | byte as u16);
            }
            for (at, c) in FONT_GAIJI.chars().enumerate() {
                codes.insert(c, 0xFDE0 + at as u16);
            }
        }
        codes.insert(' ', SPACE);
        Font { language, glyphs, codes }
    }

    /// The character a code draws, when it is the one that character is written with.
    fn char_of(&self, code: u16) -> Option<char> {
        let c = if code < CODES {
            self.glyphs.get(code as usize).copied().flatten()?
        } else if self.language.european() && code >= 0xFF00 && LATIN.contains(&(code as u8)) {
            latin_char(code as u8)
        } else if self.language.european() && (0xFDE0..0xFDF8).contains(&code) {
            FONT_GAIJI.chars().nth((code - 0xFDE0) as usize)?
        } else if code == SPACE {
            ' '
        } else {
            return None;
        };
        (self.codes.get(&c) == Some(&code)).then_some(c)
    }

    /// The characters the font has, in code order.
    pub fn characters(&self) -> String {
        let mut found: Vec<(u16, char)> = self.codes.iter().map(|(&c, &code)| (code, c)).collect();
        found.sort();
        found.into_iter().map(|(_, c)| c).collect()
    }
}

/// How a code reads in the text: a character the font draws, or a tag.
enum Token {
    Char(char),
    Text(String),
}

fn token(code: u16, font: &Font) -> Token {
    if let Some(c) = font.char_of(code) {
        return Token::Char(c);
    }
    if code == NEWLINE {
        return Token::Text("\n".into());
    }
    if code == PAGE {
        return Token::Text("<page>".into());
    }
    if let Some(&(_, tag)) = ICONS.iter().find(|(known, _)| *known == code) {
        return Token::Text(tag.into());
    }
    if let Some((kind, slot)) = slot_of(code) {
        return Token::Text(slot_tag(kind, slot));
    }
    match code {
        0xFF04 => return Token::Text("{voice 1}".into()),
        0xFF05 => return Token::Text("{voice 0}".into()),
        0xFF06 => return Token::Text("{voice 2}".into()),
        _ => {}
    }
    if let Some((name, base)) = BYTE_CODES.iter().find(|(_, base)| code & 0xFF00 == *base) {
        return Token::Text(format!("{{{name} {}}}", code - base));
    }
    Token::Text(format!("{{{code:04X}}}"))
}

/// A message's codes as text: characters, tags, and braces for the rest. A character that would
/// read back as something else is escaped as {c}: a brace always, a bracket that starts a tag, an
/// @ at the start of a line.
pub fn decode(codes: &[u16], font: &Font) -> String {
    let tokens: Vec<Token> = codes.iter().map(|&code| token(code, font)).collect();
    let shown: Vec<String> = tokens
        .iter()
        .map(|token| match token {
            Token::Char(c) => c.to_string(),
            Token::Text(text) => text.clone(),
        })
        .collect();
    let mut out = String::new();
    for (at, token) in tokens.iter().enumerate() {
        match token {
            Token::Char(c) => {
                let line_start = out.is_empty() || out.ends_with('\n');
                let escape = match c {
                    '{' => true,
                    '@' => line_start,
                    '[' | '(' | '<' => {
                        let ahead: String = shown[at..].iter().take(12).map(String::as_str).collect();
                        tag_at(&ahead).is_some()
                    }
                    _ => false,
                };
                if escape {
                    out.push('{');
                    out.push(*c);
                    out.push('}');
                } else {
                    out.push(*c);
                }
            }
            Token::Text(text) => out.push_str(text),
        }
    }
    out
}

fn char_code(c: char, font: &Font) -> Result<u16> {
    font.codes.get(&c).copied().ok_or_else(|| {
        Error::invalid(format!(
            "the {} font has no {c:?} (U+{:04X}); it has {}",
            font.language.name(),
            c as u32,
            font.characters()
        ))
    })
}

fn braced(inside: &str, font: &Font) -> Result<u16> {
    let mut chars = inside.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return char_code(c, font);
    }
    if inside.len() == 4 && inside.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Ok(u16::from_str_radix(inside, 16).expect("hex digits"));
    }
    let (name, value) = inside.split_once(' ').unwrap_or((inside, ""));
    let value: Option<u16> = value.trim().parse().ok().filter(|&value| value <= 255);
    if name == "voice" {
        return match value {
            Some(1) => Ok(0xFF04),
            Some(0) => Ok(0xFF05),
            Some(2) => Ok(0xFF06),
            _ => bail!("{{voice N}} takes 0, 1 or 2"),
        };
    }
    match (BYTE_CODES.iter().find(|(known, _)| *known == name), value) {
        (Some((_, base)), Some(value)) => Ok(base + value),
        (Some(_), None) => bail!("{{{name} N}} takes a number from 0 to 255"),
        _ => bail!(
            "{{{inside}}} is not a code: braces hold one character, four hex digits, or wait, colour, red, green, blue, alpha, justify, gap, move or voice and a number"
        ),
    }
}

/// A message's text as codes: the reverse of decode.
pub fn encode(text: &str, font: &Font) -> Result<Vec<u16>> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        match c {
            '\n' => {
                out.push(NEWLINE);
                rest = &rest[1..];
            }
            '{' => {
                // {c} is the character itself, even a brace
                let inside = rest[1..].chars().next().filter(|&c| rest[1 + c.len_utf8()..].starts_with('}'));
                let (code, length) = match inside {
                    Some(c) => (char_code(c, font)?, 2 + c.len_utf8()),
                    None => {
                        let Some(end) = rest.find('}') else {
                            bail!("a {{ without its }}: {}", rest.chars().take(20).collect::<String>());
                        };
                        (braced(&rest[1..end], font)?, end + 1)
                    }
                };
                out.push(code);
                rest = &rest[length..];
            }
            '[' | '(' | '<' if tag_at(rest).is_some() => {
                let (tag, code) = tag_at(rest).expect("a tag");
                out.push(code);
                rest = &rest[tag.len()..];
            }
            c => {
                out.push(char_code(c, font)?);
                rest = &rest[c.len_utf8()..];
            }
        }
    }
    Ok(out)
}

/// The messages of a text file, in order: (id, text).
pub fn parse_text(text: &str) -> Result<Vec<(u16, String)>> {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text).replace("\r\n", "\n");
    let mut found: Vec<(u16, String)> = Vec::new();
    let mut current: Option<(u16, Vec<&str>)> = None;
    let mut seen = HashSet::new();
    let lines: Vec<&str> = text.split('\n').collect();
    let last = lines.len() - 1;
    for (number, line) in lines.iter().enumerate() {
        let id = line
            .strip_prefix('@')
            .map(str::trim_end)
            .filter(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()));
        if let Some(digits) = id {
            let Some(id) = digits.parse::<u16>().ok() else {
                bail!("line {}: @{digits}: message ids go from 0 to 65535", number + 1);
            };
            if !seen.insert(id) {
                bail!("line {}: message {id} is given twice", number + 1);
            }
            if let Some((id, body)) = current.take() {
                found.push((id, body.join("\n")));
            }
            current = Some((id, Vec::new()));
            continue;
        }
        match &mut current {
            // the line break after a file's last message ends the file
            Some(_) if number == last && line.is_empty() => {}
            Some((_, body)) => body.push(line),
            None if line.trim().is_empty() || line.starts_with('#') => {}
            None => bail!("line {}: text before the first @id line (comments start with #)", number + 1),
        }
    }
    if let Some((id, body)) = current {
        found.push((id, body.join("\n")));
    }
    Ok(found)
}

/// A message file as text: a comment line or two (`# ...`), then each message after its `@id`
/// line, in the file's order.
pub fn to_text(mes: &Mes, font: &Font, comment: &str) -> String {
    let mut out = String::new();
    for line in comment.lines() {
        out.push_str("# ");
        out.push_str(line);
        out.push('\n');
    }
    for (id, codes) in mes.messages() {
        out.push_str(&format!("@{id}\n{}\n", decode(codes, font)));
    }
    out
}

/// The English font table from the disc's files, for Fonts.
pub fn english_table<'a>(files: impl IntoIterator<Item = (&'a str, &'a [u8])>) -> Option<Vec<u8>> {
    files.into_iter().find(|(path, _)| path.eq_ignore_ascii_case(ENGLISH_FONT)).map(|(_, data)| data.to_vec())
}

/// The fonts of every language, with the English font's table as the disc has it.
#[derive(Clone, Debug, Default)]
pub struct Fonts {
    pub english_table: Option<Vec<u8>>,
}

impl Fonts {
    pub fn font(&self, language: Language) -> Font {
        Font::new(language, self.english_table.as_deref())
    }
}

/// The comment a text file starts with.
pub fn comment(id: &str, language: Language, path: &str) -> String {
    format!("{id}: {} text from {path}. Each message follows its @id line; see docs/assets/text.md.", language.name())
}

/// The message file at a path as text.
pub fn export(data: &[u8], path: &str, id: &str, fonts: &Fonts) -> Result<String> {
    let Some(located) = locate(path) else {
        bail!("{path} is not a message file");
    };
    let mes = Mes::parse(data)?;
    Ok(to_text(&mes, &fonts.font(located.language), &comment(id, located.language, path)))
}

/// The message file at a path with a mod's text in it: each message whose text differs from the
/// reference's (the file the text was exported from) replaces the template's, and ids the
/// template lacks are added; the rest of the template stays as it is. Refused when it outgrows
/// the buffer the game loads it into.
pub fn build(text: &str, template: &[u8], reference: &[u8], path: &str, fonts: &Fonts) -> Result<Vec<u8>> {
    let Some(located) = locate(path) else {
        bail!("{path} is not a message file");
    };
    let font = fonts.font(located.language);
    let reference = Mes::parse(reference)?;
    let mut mes = Mes::parse(template)?;
    let mut changed = false;
    for (id, body) in parse_text(text)? {
        let codes = encode(&body, &font).map_err(|error| Error::invalid(format!("@{id}: {error}")))?;
        if reference.get(id) == Some(&codes[..]) || mes.get(id) == Some(&codes[..]) {
            continue;
        }
        mes.set(id, codes);
        changed = true;
    }
    if !changed {
        return Ok(template.to_vec());
    }
    let built = mes.write()?;
    if let Some(limit) = buffer_size(path) {
        if built.len() > limit {
            bail!("{} bytes; the game loads {path} into a buffer of {limit}", built.len());
        }
    }
    Ok(built)
}

/// A message file's asset: its id's name (en/allmenu, without text:), language and the game's
/// name, and its places (indexes into what text_assets was given), the reference's first: the
/// content found at most places, which the tree exports. An id covers every version of a file of
/// one name and language (allmenu.mes is in 20 English menu packs, in three versions), since a
/// mod's text changes messages, not whole files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextAsset {
    pub id: String,
    pub language: Language,
    pub name: String,
    pub places: Vec<usize>,
    pub versions: usize,
    pub game_hash: String,
}

/// The message files' assets, from (place, hash of its bytes) in the disc's order. The game's own
/// files of a name and language (rank 0, else 1) take the bare id (text:en/allmenu); the others
/// take @ and their folder (text:en/system@meswin-usa), as other assets' later versions do.
pub fn text_assets(found: &[(&Place, &str)]) -> Vec<TextAsset> {
    let mut groups: BTreeMap<(Language, String), Vec<(usize, Located)>> = BTreeMap::new();
    let mut order: Vec<(Language, String)> = Vec::new();
    for (at, (place, _)) in found.iter().enumerate() {
        let Some(located) = locate(&place.display()) else {
            continue;
        };
        let key = (located.language, located.name.clone());
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push((at, located));
    }
    let mut taken = HashSet::new();
    let mut out = Vec::new();
    for key in order {
        let members = &groups[&key];
        let best = members.iter().map(|(_, located)| located.rank).min().unwrap_or(0);
        // the bare id's places, then each other folder's
        let mut parts: Vec<(String, Vec<usize>)> = vec![(String::new(), Vec::new())];
        for (at, located) in members {
            if located.rank == best {
                parts[0].1.push(*at);
                continue;
            }
            let label = found[*at].0.label();
            match parts.iter_mut().find(|(known, _)| *known == label) {
                Some((_, places)) => places.push(*at),
                None => parts.push((label, vec![*at])),
            }
        }
        for (label, places) in parts {
            let base = format!("{}/{}", key.0.short(), key.1);
            let mut id = if label.is_empty() { base.clone() } else { format!("{base}@{label}") };
            let mut n = 2;
            while !taken.insert(id.to_lowercase()) {
                id = format!("{base}@{label}.{n}");
                n += 1;
            }
            // the content found at most places, the first found of those
            let mut counts: Vec<(&str, usize, usize)> = Vec::new();
            for &at in &places {
                let hash = found[at].1;
                match counts.iter_mut().find(|(known, _, _)| *known == hash) {
                    Some((_, count, _)) => *count += 1,
                    None => counts.push((hash, 1, at)),
                }
            }
            let &(game_hash, _, first) =
                counts.iter().max_by_key(|(_, count, first)| (*count, std::cmp::Reverse(*first))).expect("a place");
            let mut ordered = vec![first];
            ordered.extend(places.iter().copied().filter(|&at| at != first));
            out.push(TextAsset {
                id,
                language: key.0,
                name: key.1.clone(),
                places: ordered,
                versions: counts.len(),
                game_hash: game_hash.to_string(),
            });
        }
    }
    out
}

/// The file an id's text is written to in the tree and in a mod: text/en/allmenu.txt.
pub fn text_file(id_name: &str) -> String {
    let (language, name) = id_name.split_once('/').unwrap_or(("", id_name));
    format!("{}/{language}/{}.txt", TEXT, tree::file_stem(name))
}

pub const TEXT: &str = "text";

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A made-up message file: the table, FF01 FF00, then each message and FF01 FF00.
    pub(crate) fn sample(messages: &[(u16, &[u16])]) -> Vec<u8> {
        let count = messages.len();
        let mut body = vec![END, NEWLINE];
        let mut table = Vec::new();
        for (id, text) in messages {
            table.push(*id);
            table.push((2 + 2 * count + body.len() - (1 + count)) as u16);
            body.extend(*text);
            body.extend([END, NEWLINE]);
        }
        let size = 2 * (2 + table.len() + body.len());
        let words: Vec<u16> = [vec![count as u16, (size & 0xFFFF) as u16], table, body].concat();
        words.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    /// Glyphs of ASCII text in every font: printable ASCII from the space, without the backslash.
    pub(crate) fn ascii(text: &str) -> Vec<u16> {
        text.bytes()
            .map(|b| match b {
                b' ' => SPACE,
                b'\n' => NEWLINE,
                b if b > b'\\' => (b - 0x21) as u16,
                b => (b - 0x20) as u16,
            })
            .collect()
    }

    #[test]
    fn a_file_reads_and_writes_back() {
        let data = sample(&[(5, &ascii("Iron Pot")), (2, &ascii("Two\nlines")), (9, &[])]);
        let mes = Mes::parse(&data).unwrap();
        assert_eq!(mes.entries, vec![(5, At::Span(0)), (2, At::Span(1)), (9, At::Span(2))]);
        assert_eq!(mes.get(2), Some(&ascii("Two\nlines")[..]));
        assert_eq!(mes.lead, vec![END, NEWLINE]);
        assert_eq!(mes.write().unwrap(), data);
        // bytes after size, a span without its FF01, two ids sharing a span
        let mut words: Vec<u16> = data.chunks(2).map(|w| u16::from_le_bytes([w[0], w[1]])).collect();
        words[5] = words[3];
        words.truncate(words.len() - 2);
        words[1] = (2 * words.len()) as u16;
        let mut odd: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        odd.extend([0xAB, 0xCD, 0xEF, 0x01]);
        let mes = Mes::parse(&odd).unwrap();
        assert_eq!(mes.entries, vec![(5, At::Span(0)), (2, At::Span(0)), (9, At::Span(1))]);
        assert_eq!(mes.spans[1].rest, Vec::<u16>::new());
        assert_eq!(mes.tail, vec![0xAB, 0xCD, 0xEF, 0x01]);
        assert_eq!(mes.write().unwrap(), odd);
        // an offset past the end
        words[7] = 900;
        let odd: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        let mes = Mes::parse(&odd).unwrap();
        assert_eq!((mes.entries[2], mes.get(9), mes.messages().count()), ((9, At::Outside(900)), None, 2));
        assert_eq!(mes.write().unwrap(), odd);
        // a file past 64 KB, whose size is its length cut to 16 bits
        let long = vec![0x21; 40000];
        let data = sample(&[(1, &long), (2, &ascii("after"))]);
        assert_eq!(word(&data, 1) as usize, data.len() - 0x10000);
        let mes = Mes::parse(&data).unwrap();
        assert_eq!((mes.get(2), mes.tail.len()), (Some(&ascii("after")[..]), 0));
        assert_eq!(mes.write().unwrap(), data);
    }

    #[test]
    fn broken_files_are_refused() {
        assert!(Mes::parse(&[1, 0]).is_err());
        let mut data = sample(&[(1, &ascii("a"))]);
        data[2] = 200;
        assert!(Mes::parse(&data).is_err());
        let mut data = sample(&[(1, &ascii("a"))]);
        data[0] = 50;
        assert!(Mes::parse(&data).is_err());
        assert!(Mes::parse(&data[..7]).is_err());
    }

    #[test]
    fn set_changes_a_message_and_adds_one() {
        let data = sample(&[(5, &ascii("Iron Pot")), (2, &ascii("Two"))]);
        let mut mes = Mes::parse(&data).unwrap();
        mes.set(5, ascii("Gold Pot!"));
        mes.set(70, ascii("New"));
        let written = mes.write().unwrap();
        assert_eq!(written, sample(&[(5, &ascii("Gold Pot!")), (2, &ascii("Two")), (70, &ascii("New"))]));
        // a shared span is split, so the other id keeps its text
        let mut mes = Mes::parse(&data).unwrap();
        mes.entries[1].1 = At::Span(0);
        mes.set(2, ascii("Mine"));
        let back = Mes::parse(&mes.write().unwrap()).unwrap();
        assert_eq!((back.get(5), back.get(2)), (Some(&ascii("Iron Pot")[..]), Some(&ascii("Mine")[..])));
        // a last message without FF01 keeps its text when another follows it
        let mut mes = Mes::parse(&data).unwrap();
        mes.spans[1].rest.clear();
        mes.set(3, ascii("x"));
        let back = Mes::parse(&mes.write().unwrap()).unwrap();
        assert_eq!((back.get(2), back.get(3)), (Some(&ascii("Two")[..]), Some(&ascii("x")[..])));
    }

    #[test]
    fn languages_and_names_come_from_the_path() {
        let at = |path: &str| locate(path).map(|found| (found.language.short(), found.name, found.rank));
        assert_eq!(at("menu/1/shop.pac.d/shopname.mes"), Some(("en", "shopname".into(), 0)));
        assert_eq!(at("meswin/system_3.mes"), Some(("de", "system".into(), 0)));
        assert_eq!(at("etc/help5.mes"), Some(("es", "help".into(), 0)));
        assert_eq!(at("dungeon/msg/d01_2.mes"), Some(("fr", "d01".into(), 0)));
        assert_eq!(at("menu/aqua/mes/fconv4.bin"), Some(("it", "fconv".into(), 0)));
        assert_eq!(at("meswin/sysmes.mes"), Some(("ja", "sysmes".into(), 1)));
        assert_eq!(at("meswin/sysmes2.mes"), Some(("ja", "sysmes2".into(), 1)));
        assert_eq!(at("meswin/eu/ger/npcmsg.mes"), Some(("de", "npcmsg".into(), 2)));
        assert_eq!(at("meswin/kor/npcmsg.mes"), Some(("ko", "npcmsg".into(), 2)));
        assert_eq!(at("event/fconv.bin"), Some(("ja", "fconv".into(), 1)));
        assert_eq!(at("menu/aqua/mes/other.bin"), None);
        assert_eq!(buffer_size("meswin/system_1.mes"), Some(0xD000));
        assert_eq!(buffer_size("meswin/sysmes_5.mes"), Some(0x13880));
        assert_eq!(buffer_size("etc/help1.mes"), Some(0x1000));
        assert_eq!(buffer_size("meswin/usa/system.mes"), None);
    }

    #[test]
    fn ids_take_the_games_own_files_and_every_version() {
        let place = |path: &str| {
            let (path, members) = match path.split_once(".pac.d/") {
                Some((top, member)) => (format!("{top}.pac"), vec![member.to_string()]),
                None => (path.to_string(), Vec::new()),
            };
            Place { path, members }
        };
        let places = [
            place("menu/1/a.pac.d/allmenu.mes"),
            place("menu/1/b.pac.d/allmenu.mes"),
            place("menu/1/c.pac.d/allmenu.mes"),
            place("meswin/system_1.mes"),
            place("meswin/usa/system.mes"),
            place("menu/allmenu.mes"),
            place("menu/0/a.pac.d/allmenu.mes"),
        ];
        let hashes = ["x", "y", "y", "s", "t", "j", "k"];
        let found: Vec<(&Place, &str)> = places.iter().zip(hashes).collect();
        let assets = text_assets(&found);
        let shown: Vec<(&str, &[usize], usize)> =
            assets.iter().map(|asset| (asset.id.as_str(), &asset.places[..], asset.versions)).collect();
        assert_eq!(
            shown,
            [
                ("en/allmenu", &[1, 0, 2][..], 2),
                ("en/system", &[3][..], 1),
                ("en/system@meswin-usa", &[4][..], 1),
                ("ja/allmenu", &[6][..], 1),
                ("ja/allmenu@menu", &[5][..], 1),
            ]
        );
        assert_eq!(assets[0].game_hash, "y");
        assert_eq!(text_file("en/system@meswin-usa"), "text/en/system@meswin-usa.txt");
    }

    fn table(chars: &str) -> Vec<u8> {
        let mut out = vec![0u8; 8];
        for c in chars.chars() {
            if c.is_ascii() {
                out.extend([c as u8, b' ']);
            } else {
                out.extend(c2_disc::text::encode(&c.to_string()).unwrap());
            }
        }
        out
    }

    #[test]
    fn text_reads_back_to_the_same_codes() {
        let english: String = (0x20u8..0x7F).filter(|&b| b != b'\\').map(char::from).collect::<String>() + "　、。・";
        let fonts = Fonts { english_table: Some(table(&english)) };
        let en = fonts.font(Language::English);
        let fr = fonts.font(Language::French);
        let ja = fonts.font(Language::Japanese);
        let mut codes = ascii("Press ");
        codes.extend([0xFD06, SPACE, 0xFBFA, 0xFAF9, 0xFBFE, 0xFAFA, 0xFBFF, PAGE, 0xFE0C, 0xFC01, 0xF805, 0xF936]);
        codes.extend([0xFF04, 0xFD1C, 0xF100, 0x0400, 94, 0, NEWLINE]);
        codes.extend(ascii("@1 [l1] {x} (O) <page> [Number1] [z]"));
        let text = decode(&codes, &en);
        assert_eq!(
            text,
            "Press (O) [Number1][String1][Item1][Name1][Number]<page>{wait 12}{colour 1}{gap 5}{move 54}{voice 1}{FD1C}{F100}{0400}　{0000}\n{@}1 {[}l1] {{}x} {(}O) {<}page> {[}Number1] [z]"
        );
        assert_eq!(encode(&text, &en).unwrap(), codes);
        // European accents are FF00 and their Latin-1 byte, the full-width symbols FDE0 on
        let accented = vec![0xFFE9, 0xFFBD, 0xFDF3, 0x5E, 0x41];
        assert_eq!(decode(&accented, &fr), "éŒ・{005E}b");
        assert_eq!(encode("éŒ・{005E}b", &fr).unwrap(), accented);
        assert_eq!(decode(&accented, &ja), "{FFE9}{FFBD}{FDF3}{005E}b");
        // Japanese spellings of the slots, and characters a font lacks
        assert_eq!(encode("[数値１６][文字列２][アイテム５]", &ja).unwrap(), vec![0xFBDF, 0xFAF8, 0xFBF2]);
        assert!(encode("é", &en).unwrap_err().to_string().contains("English font has no 'é'"));
        assert!(encode("{wait 300}", &en).is_err());
        assert!(encode("{nothing}", &en).is_err());
        assert!(encode("{FD06", &en).is_err());
    }

    #[test]
    fn an_edited_line_builds_and_reads_back() {
        let fonts = Fonts::default();
        let path = "menu/1/shop.pac.d/shopname.mes";
        let data = sample(&[(0, &ascii("Iron Pot")), (1, &ascii("Two\nlines")), (7, &[NEWLINE])]);
        let text = export(&data, path, "text:en/shopname", &fonts).unwrap();
        assert!(text.starts_with("# text:en/shopname: English text from menu/1/shop.pac.d/shopname.mes."));
        assert!(text.ends_with("@0\nIron Pot\n@1\nTwo\nlines\n@7\n\n\n"));
        assert_eq!(parse_text(&text).unwrap()[2], (7, "\n".to_string()));
        // unedited, and with Windows line ends, it builds the same bytes
        assert_eq!(build(&text, &data, &data, path, &fonts).unwrap(), data);
        assert_eq!(build(&text.replace('\n', "\r\n"), &data, &data, path, &fonts).unwrap(), data);
        // only the changed message changes, here in another version of the file; a new id is added
        let edited = text.replace("Iron Pot", "Gold Pot") + "@300\nAdded {wait 3}\n";
        let other = sample(&[(1, &ascii("Other")), (0, &ascii("Iron Pot"))]);
        let built = build(&edited, &other, &data, path, &fonts).unwrap();
        let back = Mes::parse(&built).unwrap();
        assert_eq!(back.get(0), Some(&ascii("Gold Pot")[..]));
        assert_eq!(back.get(1), Some(&ascii("Other")[..]));
        let mut added = ascii("Added ");
        added.push(0xFE03);
        assert_eq!(back.get(300), Some(&added[..]));
        let again = export(&built, path, "x", &fonts).unwrap();
        assert!(again.ends_with("@1\nOther\n@0\nGold Pot\n@300\nAdded {wait 3}\n"), "{again}");
        // mistakes are named by message
        let error = build(&text.replace("Iron Pot", "Iron {wait}"), &data, &data, path, &fonts).unwrap_err();
        assert!(error.to_string().starts_with("@0: {wait N} takes a number"), "{error}");
        assert!(parse_text("stray\n@1\nx\n").is_err());
        assert!(parse_text("@1\nx\n@1\ny\n").is_err());
        assert!(parse_text("@70000\nx\n").is_err());
        // a file the game loads into a buffer of fixed size may not outgrow it
        let big = format!("@1\n{}\n", "x".repeat(3000));
        let error = build(&big, &data, &data, "etc/help1.mes", &fonts).unwrap_err();
        assert!(error.to_string().contains("buffer of 4096"), "{error}");
    }

    /// Every message file of an extracted disc (C2S_CORPUS): read and written back, and through
    /// its text, byte for byte.
    #[test]
    fn corpus_message_files_round_trip() {
        let Some(corpus) = std::env::var_os("C2S_CORPUS") else {
            return;
        };
        let root = std::path::Path::new(&corpus);
        let paths = crate::files_under(root).unwrap();
        let fonts = Fonts { english_table: std::fs::read(root.join(ENGLISH_FONT)).ok() };
        assert!(fonts.english_table.is_some(), "no {ENGLISH_FONT} under {corpus:?}");
        let mut checked = 0;
        let mut failed = Vec::new();
        for path in &paths {
            let relative = crate::relative_posix(path, root);
            if !is_message_file(&relative) {
                continue;
            }
            checked += 1;
            let data = std::fs::read(path).unwrap();
            let outcome = Mes::parse(&data).and_then(|mes| {
                if mes.write()? != data {
                    bail!("written differently");
                }
                let text = export(&data, &relative, "text:x", &fonts)?;
                let font = fonts.font(locate(&relative).unwrap().language);
                for (id, body) in parse_text(&text)? {
                    if encode(&body, &font)? != mes.get(id).unwrap() {
                        bail!("@{id} reads back differently");
                    }
                }
                if build(&text, &data, &data, &relative, &fonts)? != data {
                    bail!("built differently");
                }
                // and edited: a word added to the first message builds and reads back
                let Some((first, _)) = mes.messages().next() else {
                    return Ok(());
                };
                let mut wanted = mes.clone();
                wanted.set(first, [mes.get(first).unwrap(), &encode(" Edited", &font)?].concat());
                let edited = to_text(&wanted, &font, &comment("text:x", font.language, &relative));
                let built = build(&edited, &data, &data, &relative, &fonts)?;
                if Mes::parse(&built)?.get(first) != wanted.get(first) {
                    bail!("an edited message builds differently");
                }
                if export(&built, &relative, "text:x", &fonts)? != edited {
                    bail!("an edited message reads back differently");
                }
                Ok(())
            });
            if let Err(error) = outcome {
                failed.push(format!("{relative}: {error}"));
            }
        }
        eprintln!("{} of {checked} message files round-trip", checked - failed.len());
        assert!(checked >= 310, "only {checked} message files under {corpus:?}");
        assert!(failed.is_empty(), "{}", failed.join("\n"));
    }
}
