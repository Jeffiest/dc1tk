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

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::font;
use crate::tree::{self, Place};
use crate::{bail, Context, Error, Result};

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
        crate::lang::LANGUAGE_NAMES[self as usize]
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
const LATIN: [u8; 63] = font::EUROPEAN_LETTERS;

fn latin_char(byte: u8) -> char {
    font::latin_char(byte)
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
    /// A second code that draws a character, written `{c}`: the European fonts' own glyphs for the
    /// accented letters and symbols the menus mostly write as FFxx and FDEx codes.
    alternates: HashMap<char, u16>,
}

impl Font {
    /// The font of a language: ASCII in every one; English past it from the English font's
    /// table (ENGLISH_FONT, from the disc) and the letters a build adds to its texture (glyphs 288
    /// on: Fonts::english_extra, else font::ENGLISH_EXTRA); Japanese from font::JAPANESE; Korean
    /// from the Korean font's table (font::KOREAN_TABLE, from the disc). Without its table a
    /// language has ASCII only.
    pub fn new(language: Language, fonts: &Fonts) -> Font {
        // glyphs 0 to 93: printable ASCII without the backslash, in every font
        let mut glyphs: Vec<Option<char>> = (0x20u8..0x7F).filter(|&b| b != b'\\').map(|b| Some(b as char)).collect();
        match language {
            Language::English => {
                if let Some(table) = fonts.english_table.as_deref().filter(|table| table.len() >= 8) {
                    glyphs = font::table_chars(table, ENGLISH_GLYPHS);
                    if glyphs.len() == ENGLISH_GLYPHS {
                        match &fonts.english_extra {
                            Some(letters) => glyphs.extend(letters.iter().copied().map(Some)),
                            None => glyphs.extend(font::ENGLISH_EXTRA.chars().map(Some)),
                        }
                    }
                }
            }
            Language::Japanese => glyphs.extend(font::JAPANESE.chars().map(Some)),
            Language::Korean => {
                if let Some(table) = fonts.korean_table.as_deref().filter(|table| table.len() >= 8) {
                    glyphs = font::table_chars(table, usize::MAX);
                }
            }
            _ => {}
        }
        let mut codes = HashMap::new();
        for (glyph, c) in glyphs.iter().enumerate() {
            if let Some(c) = c {
                codes.entry(*c).or_insert(glyph as u16);
            }
        }
        let mut alternates = HashMap::new();
        if language.european() {
            for (at, byte) in LATIN.into_iter().enumerate() {
                codes.insert(latin_char(byte), 0xFF00 | byte as u16);
                alternates.insert(latin_char(byte), 0x5E + at as u16);
            }
            for (at, c) in FONT_GAIJI.chars().enumerate() {
                codes.insert(c, 0xFDE0 + at as u16);
                alternates.insert(c, 0x9D + at as u16);
            }
        }
        codes.insert(' ', SPACE);
        Font { language, glyphs, codes, alternates }
    }

    /// The character whose second code (written `{c}`) a code is.
    fn alternate_of(&self, code: u16) -> Option<char> {
        self.alternates.iter().find(|(_, &known)| known == code).map(|(&c, _)| c)
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
    if let Some(c) = font.alternate_of(code) {
        return Token::Text(format!("{{{c}}}"));
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
        if font.language == Language::English && font.glyphs.len() > ENGLISH_GLYPHS {
            // the letters a build draws into the English font
            return Error::invalid(if font::drawable(c) {
                format!(
                    "the English font has no {c:?} (U+{:04X}) here: build draws letters past its own for text:en/... lines and data:items' names and descriptions",
                    c as u32
                )
            } else {
                format!("the English font has no {c:?} (U+{:04X}): {}", c as u32, font::why_not_drawable(c))
            });
        }
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
                    // a character's second code where the font has one (the European fonts' own
                    // glyphs for the letters the menus write as FFxx)
                    Some(c) => match font.alternates.get(&c) {
                        Some(&code) => (code, 2 + c.len_utf8()),
                        None => (char_code(c, font)?, 2 + c.len_utf8()),
                    },
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

/// The fonts of every language, with the English and Korean fonts' tables as the disc has them.
#[derive(Clone, Debug, Default)]
pub struct Fonts {
    pub english_table: Option<Vec<u8>>,
    pub korean_table: Option<Vec<u8>>,
    /// The letters a build draws as the English font's glyphs 288 on (font::english_slots); None
    /// for font::ENGLISH_EXTRA, which an exported text reads them as.
    pub english_extra: Option<Vec<char>>,
}

impl Fonts {
    /// The fonts from the disc's files, by path as extract lays them out.
    pub fn from_files<'a>(files: impl IntoIterator<Item = (&'a str, &'a [u8])>) -> Fonts {
        let mut fonts = Fonts::default();
        for (path, data) in files {
            if path.eq_ignore_ascii_case(ENGLISH_FONT) {
                fonts.english_table = Some(data.to_vec());
            } else if path.eq_ignore_ascii_case(font::KOREAN_TABLE) {
                fonts.korean_table = Some(data.to_vec());
            }
        }
        fonts
    }

    /// Whether the fonts read a disc file.
    pub fn reads(path: &str) -> bool {
        path.eq_ignore_ascii_case(ENGLISH_FONT) || path.eq_ignore_ascii_case(font::KOREAN_TABLE)
    }

    pub fn font(&self, language: Language) -> Font {
        Font::new(language, self)
    }

    /// The fonts with `letters` as the English font's glyphs 288 on.
    pub fn with_english_extra(&self, letters: &[char]) -> Fonts {
        Fonts { english_extra: Some(letters.to_vec()), ..self.clone() }
    }

    /// The fonts as an exported text reads them (font::ENGLISH_EXTRA at glyphs 288 on).
    pub fn exported(&self) -> Fonts {
        Fonts { english_extra: None, ..self.clone() }
    }

    /// The letters past the English font's own table (glyphs 288 on) a message's text uses, every
    /// one the build can draw; a character it cannot is refused.
    pub fn english_letters(&self, text: &str) -> Result<BTreeSet<char>> {
        let any = Fonts { english_extra: Some(font::drawable_letters()), ..self.clone() }.font(Language::English);
        let codes = encode(text, &any)?;
        Ok(codes
            .into_iter()
            .filter(|&code| (ENGLISH_GLYPHS..CODES as usize).contains(&(code as usize)))
            .filter_map(|code| any.glyphs.get(code as usize).copied().flatten())
            .collect())
    }
}

/// The letters past the English font's own table that the messages of a mod's text (for the
/// message file at `path`) whose text differs from the reference's use: the ones the build must
/// draw into the English font. Nothing for another language's file.
pub fn changed_letters(text: &str, reference: &[u8], path: &str, fonts: &Fonts) -> Result<BTreeSet<char>> {
    let mut out = BTreeSet::new();
    if locate(path).map(|found| found.language) != Some(Language::English) {
        return Ok(out);
    }
    let exported = fonts.exported().font(Language::English);
    let reference = Mes::parse(reference)?;
    for (id, body) in parse_text(text)? {
        if encode(&body, &exported).is_ok_and(|codes| reference.get(id) == Some(&codes[..])) {
            continue;
        }
        out.extend(fonts.english_letters(&body).with_context(|| format!("@{id}"))?);
    }
    Ok(out)
}

/// A message's codes when they differ from what `keep` keeps: read first as an exported text
/// reads it, so a message left as it was keeps its codes whatever letters the build's English font
/// gains (a debug menu's leftovers in English allmenu.mes use glyph numbers past the table).
fn changed_codes(
    body: &str,
    font: &Font,
    exported: Option<&Font>,
    keep: impl Fn(&[u16]) -> bool,
) -> Result<Option<Vec<u16>>> {
    if exported.is_some_and(|exported| encode(body, exported).is_ok_and(|codes| keep(&codes))) {
        return Ok(None);
    }
    let codes = encode(body, font)?;
    Ok((!keep(&codes)).then_some(codes))
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
    let exported = fonts.english_extra.is_some().then(|| fonts.exported().font(located.language));
    let reference = Mes::parse(reference)?;
    let mut mes = Mes::parse(template)?;
    let mut changed = false;
    for (id, body) in parse_text(text)? {
        let keep = |codes: &[u16]| reference.get(id) == Some(codes) || mes.get(id) == Some(codes);
        let Some(codes) = changed_codes(&body, &font, exported.as_ref(), keep).with_context(|| format!("@{id}"))?
        else {
            continue;
        };
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

/// The width of a glyph cell's pen step in the menus' windows (ClsMes::font_w; CFont's clearance
/// and the windows' presets give 15).
pub const FONT_W: i32 = 15;

/// The widths of the icons FD00 to FD31 (GaijiDataTbl, font.cpp's data).
const ICON_WIDTHS: [i32; 50] = [
    86, 110, 32, 32, 32, 32, 22, 22, 22, 22, 22, 22, 22, 22, 22, 22, 22, 22, 26, 26, 26, 26, 26, 66, 56, 40, 22, 0, 0,
    0, 0, 0, 0, 12, 16, 8, 10, 10, 26, 26, 26, 28, 28, 16, 16, 16, 26, 28, 28, 16,
];

/// How far each line of a message moves the pen, in pixels, as ClsMes::MakeMesWinTbl lays it out
/// with a cell step of `font_w`: a half-width glyph (ASCII, and the European fonts' letters at
/// 0x5E to 0x9C, and the FFxx letters) font_w × 0.55 (half_font_w_percent) cut to a whole pixel,
/// the space font_w / 2 (or the {gap} width), any other glyph and the FDEx symbols a whole cell,
/// an icon its own width, {move N} N. Slots (numbers, items, names, text) count nothing: what
/// fills them is not known here. A page break starts a line.
pub fn line_widths(codes: &[u16], language: Language, font_w: i32) -> Vec<i32> {
    let half = (font_w as f32 * 0.55) as i32;
    let mut lines = vec![0];
    let mut gap = None;
    for &code in codes {
        let width = match code {
            NEWLINE | PAGE => {
                lines.push(0);
                gap = None;
                continue;
            }
            SPACE => gap.unwrap_or(font_w / 2),
            0 => font_w / 2,
            1..=93 => half,
            0x5E..=0x9C if language.european() => half,
            94..=0xEFFF => font_w,
            0xFFA0..=0xFFFF => half,
            0xFDE0..=0xFDF7 => font_w,
            0xFD00..=0xFD31 => ICON_WIDTHS[(code - 0xFD00) as usize],
            0xF900..=0xF9FF => (code & 0xFF) as i32,
            0xF800..=0xF8FF => {
                gap = Some((code & 0xFF) as i32);
                0
            }
            _ => 0,
        };
        *lines.last_mut().expect("a line") += width;
    }
    lines
}

/// The message files whose windows grow with their text (dialogue, NPC lines, help pages), where
/// only the widest line any of them has is a limit; the rest are menus, item and map names drawn
/// into windows and columns of a fixed width.
fn grows_with_text(name: &str) -> bool {
    ["sysmes", "npcmsg", "help"].iter().any(|kind| name.starts_with(kind))
}

/// Warnings for a mod's text whose lines are wider than the game's windows take: wider than any
/// line of the file on the disc (the reference), or, in a menu's file, wider than the message's own
/// widest line on the disc by more than a cell. A line past its window is cut off or drawn over
/// the window's edge, which shows only in the game.
pub fn width_warnings(text: &str, reference: &[u8], path: &str, fonts: &Fonts) -> Result<Vec<String>> {
    let Some(located) = locate(path) else {
        bail!("{path} is not a message file");
    };
    let font = fonts.font(located.language);
    let exported = fonts.english_extra.is_some().then(|| fonts.exported().font(located.language));
    let reference = Mes::parse(reference)?;
    let widest = |codes: &[u16]| line_widths(codes, located.language, FONT_W).into_iter().max().unwrap_or(0);
    let file_widest = reference.messages().map(|(_, codes)| widest(codes)).max().unwrap_or(0);
    let mut out = Vec::new();
    for (id, body) in parse_text(text)? {
        let own = reference.get(id);
        let Some(codes) = changed_codes(&body, &font, exported.as_ref(), |codes| own == Some(codes))
            .with_context(|| format!("@{id}"))?
        else {
            continue;
        };
        for (line, width) in line_widths(&codes, located.language, FONT_W).into_iter().enumerate() {
            let what = format!("{path} @{id} line {}: {width} pixels wide", line + 1);
            if width > file_widest {
                out.push(format!("{what}, wider than any line of the file on the disc ({file_widest}); the game cuts it off or draws it past its window"));
            } else if let Some(own) =
                own.map(widest).filter(|&own| !grows_with_text(&located.name) && width > own + FONT_W)
            {
                out.push(format!("{what}, the disc's text {own}; a menu's window or column may cut it off"));
            }
        }
    }
    Ok(out)
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
                out.extend(dc2tk_disc::text::encode(&c.to_string()).unwrap());
            }
        }
        out
    }

    #[test]
    fn text_reads_back_to_the_same_codes() {
        let english: String = (0x20u8..0x7F).filter(|&b| b != b'\\').map(char::from).collect::<String>() + "　、。・";
        let fonts = Fonts { english_table: Some(table(&english)), ..Default::default() };
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
        assert_eq!(decode(&accented, &fr), "éŒ・{¡}b");
        assert_eq!(encode("éŒ・{¡}b", &fr).unwrap(), accented);
        // the European fonts' own glyphs for a letter or symbol are its second code, in braces
        assert_eq!(decode(&[0x5E + 42, 0x9D + 19], &fr), "{é}{・}");
        assert_eq!(encode("{é}{・}{{}", &fr).unwrap(), vec![0x5E + 42, 0x9D + 19, 0x5A]);
        // Japanese glyph 94 is the full-width space
        assert_eq!(decode(&accented, &ja), "{FFE9}{FFBD}{FDF3}\u{3000}b");
        assert_eq!(decode(&[95, 300, 1943, 1944], &ja), "、ガ騙{0798}");
        assert_eq!(encode("、ガ騙", &ja).unwrap(), vec![95, 300, 1943]);
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

    /// A font table of the English one's length (288 glyphs): ASCII, then full-width spaces.
    fn english_sized() -> Fonts {
        let ascii: String = (0x20u8..0x7F).filter(|&b| b != b'\\').map(char::from).collect();
        let mut data = table(&ascii);
        for _ in 94..ENGLISH_GLYPHS {
            data.extend([0x81, 0x40]);
        }
        Fonts { english_table: Some(data), ..Default::default() }
    }

    #[test]
    fn english_gains_accented_letters_after_its_table() {
        let en = english_sized().font(Language::English);
        assert_eq!(encode("Café ¡ü", &en).unwrap(), [ascii("Caf"), vec![288 + 18, SPACE, 288, 288 + 31]].concat());
        assert_eq!(decode(&[288 + 18, 319], &en), "éü");
        // a letter the build draws only when a text: line or an item uses it, and one it cannot
        let error = encode("ì", &en).unwrap_err().to_string();
        assert!(error.contains("English font has no 'ì' (U+00EC) here: build draws letters"), "{error}");
        let error = encode("š", &en).unwrap_err().to_string();
        assert!(error.contains("English font has no 'š' (U+0161): š is s with a caron"), "{error}");
    }

    #[test]
    fn english_glyphs_past_its_table_are_the_letters_the_text_uses() {
        let fonts = english_sized();
        // what a text uses past the table, any letter the build can draw
        let used = fonts.english_letters("gŷros, Café (O) {é}").unwrap();
        assert_eq!(used, ['é', 'ŷ'].into_iter().collect());
        let error = fonts.english_letters("ǎ").unwrap_err().to_string();
        assert!(error.contains("is neither"), "{error}");
        // the build's letters: ŷ takes ¡'s glyph, é keeps its own; a composed ŷ reads back
        let slots = font::english_slots(&used).unwrap();
        let built = fonts.with_english_extra(&slots);
        let en = built.font(Language::English);
        assert_eq!(encode("gŷé", &en).unwrap(), [ascii("g"), vec![288, 288 + 18]].concat());
        assert_eq!(decode(&[288, 288 + 18], &en), "ŷé");
        assert!(encode("¡", &en).unwrap_err().to_string().contains("here: build draws letters"));
        // any 32 letters
        let many: Vec<char> = "ŷŶĉĈĝĜĥĤĵĴŝŜŵŴĩĨũŨůŮĺĹńŃŕŔśŚźŹćĆ".chars().collect();
        let slots = font::english_slots(&many.iter().copied().collect()).unwrap();
        let en = fonts.with_english_extra(&slots).font(Language::English);
        let text: String = many.iter().collect();
        let codes = encode(&text, &en).unwrap();
        assert!(codes.iter().all(|&code| (288..320).contains(&code)));
        assert_eq!(decode(&codes, &en), text);
    }

    #[test]
    fn unchanged_messages_keep_their_glyphs_whatever_letters_the_build_adds() {
        let fonts = english_sized();
        let path = "menu/1/shop.pac.d/allmenu.mes";
        // a debug menu's leftover reads as ¡é (glyphs 288 and 306), as the export numbers them
        let data = sample(&[(0, &[288, 306]), (1, &ascii("Iron Pot"))]);
        let text = export(&data, path, "x", &fonts).unwrap();
        assert!(text.contains("@0\n¡é\n"), "{text}");
        let edited = text.replace("Iron Pot", "Gŷros");
        assert_eq!(changed_letters(&edited, &data, path, &fonts).unwrap(), ['ŷ'].into_iter().collect());
        assert!(changed_letters(&edited, &data, "meswin/system_2.mes", &fonts).unwrap().is_empty());
        let slots = font::english_slots(&['ŷ'].into_iter().collect()).unwrap();
        let built = fonts.with_english_extra(&slots);
        let out = build(&edited, &data, &data, path, &built).unwrap();
        let mes = Mes::parse(&out).unwrap();
        // the leftover keeps glyph 288, which now draws ŷ; Gŷros takes it too
        assert_eq!(mes.get(0), Some(&[288, 306][..]));
        assert_eq!(mes.get(1), Some(&[ascii("G"), vec![288], ascii("ros")].concat()[..]));
        assert!(width_warnings(&edited, &data, path, &built).unwrap().is_empty());
        // unedited, it builds the disc's bytes with any letters
        assert_eq!(build(&text, &data, &data, path, &built).unwrap(), data);
        // a changed message with the leftover's letters writes them as the build draws them
        let edited = text.replace("¡é", "¡é!");
        assert_eq!(changed_letters(&edited, &data, path, &fonts).unwrap(), ['¡', 'é'].into_iter().collect());
    }

    #[test]
    fn lines_are_measured_as_the_windows_lay_them_out() {
        // ASCII 8 pixels, the space 7, a full-width glyph 15, an icon its width, {move} its pixels
        let mut codes = ascii("Ab c");
        codes.extend([100, 0xFD06, 0xF905, NEWLINE, 0xFFE9, 0xF803, SPACE, 0x5E]);
        assert_eq!(line_widths(&codes, Language::English, FONT_W), vec![8 + 8 + 7 + 8 + 15 + 22 + 5, 8 + 3 + 15]);
        // the European fonts' letters at 0x5E on are half-width
        assert_eq!(line_widths(&[0xFFE9, 0x5E, PAGE, 0x9D], Language::French, FONT_W), vec![16, 15]);
    }

    #[test]
    fn lines_too_wide_for_the_game_are_warned_of() {
        let fonts = Fonts::default();
        let path = "menu/1/shop.pac.d/shopname.mes";
        let data = sample(&[(0, &ascii("Iron Pot")), (1, &ascii("A Much Longer Shop Name"))]);
        let text = export(&data, path, "x", &fonts).unwrap();
        assert!(width_warnings(&text, &data, path, &fonts).unwrap().is_empty());
        // a little longer is fine, past a cell more than the disc's line is warned of, past the
        // file's widest line too
        let edited = text.replace("Iron Pot", "Iron Pots");
        assert!(width_warnings(&edited, &data, path, &fonts).unwrap().is_empty());
        let edited = text.replace("Iron Pot", "Iron Pot Shop");
        let warnings = width_warnings(&edited, &data, path, &fonts).unwrap();
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("@0 line 1: 102 pixels wide, the disc's text 63"), "{}", warnings[0]);
        let edited = text.replace("Iron Pot", "Iron Pot, Pans, Kettles and Cups") + "@9\nNew\n";
        let warnings = width_warnings(&edited, &data, path, &fonts).unwrap();
        assert!(warnings[0].contains("wider than any line of the file on the disc (180)"), "{warnings:?}");
        // a window that grows with its text takes anything up to the file's widest line
        let path = "meswin/sysmes_1.mes";
        let edited = text.replace("Iron Pot", "Iron Pot Shop");
        assert!(width_warnings(&edited, &data, path, &fonts).unwrap().is_empty());
    }

    /// Japanese and Korean message files of an extracted disc (C2S_CORPUS) read as text: every
    /// glyph the game's own files use has its character, and the Japanese font pages have ink in
    /// exactly the cells font::JAPANESE names.
    #[test]
    fn corpus_japanese_and_korean_read_as_text() {
        let Some(corpus) = std::env::var_os("C2S_CORPUS") else {
            return;
        };
        let root = std::path::Path::new(&corpus);
        let fonts = Fonts {
            english_table: std::fs::read(root.join(ENGLISH_FONT)).ok(),
            korean_table: std::fs::read(root.join(font::KOREAN_TABLE)).ok(),
            english_extra: None,
        };
        let mut pages = Vec::new();
        for page in 0..4 {
            pages.push(
                font::Page::read(&std::fs::read(root.join(format!("meswin/FontTex_{page}.tm2"))).unwrap()).unwrap(),
            );
        }
        for glyph in 94..2432 {
            let (page, at) = (glyph / 0x260, glyph % 0x260);
            let inked = at < pages[page].glyphs() && pages[page].inked(at);
            assert_eq!(inked, (95..font::JAPANESE_GLYPHS).contains(&glyph), "Japanese glyph {glyph}");
        }
        let mut glyphs = BTreeMap::new();
        let mut unread = BTreeMap::new();
        for path in crate::files_under(root).unwrap() {
            let relative = crate::relative_posix(&path, root);
            let Some(located) = locate(&relative) else {
                continue;
            };
            let old = located.rank > 1 || ["sysmes0", "sysmes1", "sysmes2", "sysmes3"].contains(&located.name.as_str());
            if !matches!(located.language, Language::Japanese | Language::Korean) || old {
                continue;
            }
            let font = fonts.font(located.language);
            let mes = Mes::parse(&std::fs::read(&path).unwrap()).unwrap();
            for (_, codes) in mes.messages() {
                for &code in codes.iter().filter(|&&code| code < CODES) {
                    *glyphs.entry(located.language.short()).or_insert(0) += 1;
                    if font.char_of(code).is_none() {
                        *unread.entry(format!("{relative} {code}")).or_insert(0) += 1;
                    }
                }
            }
        }
        eprintln!("glyphs read: {glyphs:?}");
        assert!(glyphs["ja"] > 100_000 && glyphs["ko"] > 10_000, "{glyphs:?}");
        assert!(unread.is_empty(), "{unread:?}");
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
        let fonts = Fonts {
            english_table: std::fs::read(root.join(ENGLISH_FONT)).ok(),
            korean_table: std::fs::read(root.join(font::KOREAN_TABLE)).ok(),
            english_extra: None,
        };
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
