//! Ideas (docs/assets/data.md, section 5): the names and notebook order of the photo subjects
//! below 1000, `PIC_NAME id, "name", sort;` in the seven copies of `neta2.lst`, the `ideas` of
//! `data:inventions` (merged into the invention table by `c2 data`, as spectrumising is into the
//! item table).
//!
//! LoadFilePictureName (inventmn.cpp) reads `menu/<LanguageCode>/neta2.lst` (LoadFileMenu; the
//! file in `menu/` itself, a copy of the Japanese, is kept in step) into a 0x5000-byte stack
//! buffer aligned to 64 bytes, and gives its names and table one heap, `pic_name_text_buff_1660`,
//! of 0x248 16-byte units: `_PIC_INFO n` takes ceil(n × 8 / 16) + 2 units for the n 8-byte
//! PIC_NAME_INFO, and `_PIC_NAME` each name ceil((length + 1) / 16), the length as stored:
//! ConvertFontCode turns each `[UNIxxxx]` code into one byte only when LanguageCode is 5 or
//! more (Spanish), so the French, German and Italian names keep their codes. mgCMemory::Alloc
//! refuses the allocation that reaches the end (a name then reads as NULL). The notebook
//! (CMenuInvent::UpdataNetaMemoStr) sorts the known ideas by the third argument in a stack array
//! of 0x184 (388) keys, and MenuInventNetaMemoDraw prints `[bulb2]` and the name into a
//! 0x20-byte buffer. The ideas come first in the file: the notebook sorts its first entries,
//! as many as the known ideas below 1000, by the key, the scoops after them by their own order.
//!
//! The sort key is the name's first letter's place times 100 plus its place among the names of
//! that letter: in English, French, German and Italian A is 0 (accents and Œ folded away), in
//! Spanish A is 1 and a name opening with `¡` 0; in Japanese the kana row of the reading (あ 0,
//! か 5, ... や 35, ゆ 36, よ 37, ら 40, わ 45), Latin letters 50.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::cfg_text::{self, Arg, Edit, Script, To};
use crate::item_data::{self, LANGUAGE_NAMES};
use crate::{bail, Result};

/// Every copy, the one in `menu/` (Japanese, read by nothing) first.
pub fn paths() -> Vec<String> {
    let mut out = vec!["menu/neta2.lst".to_string()];
    out.extend((0..6).map(|language| format!("menu/{language}/neta2.lst")));
    out
}

/// Each copy's LanguageCode (ja, en, fr, de, it, es; Korean, 6, reads the English).
pub const FILE_LANGUAGES: [usize; 7] = [0, 0, 1, 2, 3, 4, 5];
/// The English copy, the one the table shows.
const ENGLISH: usize = 2;
/// pic_name_text_buff_1660's 16-byte units.
pub const UNITS: usize = 0x248;
const UNITS_WHY: &str =
    "pic_name_text_buff_1660, the 0x248 16-byte units LoadFilePictureName gives the table and names";
/// UpdataNetaMemoStr's sort_keys[0x184].
pub const ENTRIES_MOST: usize = 0x184;
/// The longest name MenuInventNetaMemoDraw's text[0x20] holds after "[bulb2]" and before the NUL
/// (the game's own French, German and Spanish names are longer: the most a copy takes is the
/// greater of this and its longest idea name).
pub const NAME_MOST: usize = 0x20 - 7 - 1;
/// LoadFilePictureName's 0x5000-byte buffer, less the 64-byte alignment.
pub const FILE_MOST: usize = 0x5000 - 0x40;
/// Ideas are 1 to 999; from 1000 are scoops.
pub const SCOOP_FIRST: i64 = 1000;

/// One PIC_NAME: its statement (None for a new one), id, name as stored in the file, sort key.
#[derive(Clone, Debug)]
struct Entry {
    statement: Option<usize>,
    id: i64,
    name: Vec<u8>,
    sort: i64,
}

/// The seven copies of neta2.lst.
#[derive(Clone, Debug)]
pub struct Ideas {
    pub scripts: Vec<Script>,
    entries: Vec<Vec<Entry>>,
    /// Each copy's PIC_INFO statement.
    counts: Vec<usize>,
}

/// What the mods give an idea in one language: (value, mod).
#[derive(Default)]
struct Given {
    name: BTreeMap<usize, (String, String)>,
    sort: BTreeMap<usize, (i64, String)>,
}

/// The bytes a name takes in the heap: ConvertFontCode's for Spanish, the file's otherwise.
pub fn stored_len(name: &[u8], language: usize) -> usize {
    if language >= 5 {
        item_data::converted_len(name, language)
    } else {
        name.len()
    }
}

/// The 16-byte units a copy's table and names take.
fn units(entries: &[Entry], language: usize) -> usize {
    (entries.len() * 8).div_ceil(16)
        + 2
        + entries.iter().map(|entry| stored_len(&entry.name, language) / 16 + 1).sum::<usize>()
}

/// A name's text: Shift-JIS in Japanese, `[UNIxxxx]` codes read as their characters otherwise.
pub fn name_text(bytes: &[u8], language: usize) -> String {
    if language == 0 {
        return c2_disc::text::decode_lossy(bytes);
    }
    let mut out = String::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'[' && bytes.get(at + 1..at + 4) == Some(b"UNI") && bytes.get(at + 8) == Some(&b']') {
            let code =
                std::str::from_utf8(&bytes[at + 4..at + 8]).ok().and_then(|hex| u32::from_str_radix(hex, 16).ok());
            if let Some(c) = code.and_then(char::from_u32) {
                out.push(c);
                at += 9;
                continue;
            }
        }
        out.push(char::from(bytes[at]));
        at += 1;
    }
    out
}

/// A Latin letter with its accent folded away, upper case (Œ as O, Æ as A, ß as S).
fn fold(c: char) -> char {
    match c {
        'À'..='Å' | 'à'..='å' | 'Æ' | 'æ' => 'A',
        'Ç' | 'ç' => 'C',
        'È'..='Ë' | 'è'..='ë' => 'E',
        'Ì'..='Ï' | 'ì'..='ï' => 'I',
        'Ñ' | 'ñ' => 'N',
        'Ò'..='Ö' | 'Ø' | 'ò'..='ö' | 'ø' | 'Œ' | 'œ' => 'O',
        'Ù'..='Ü' | 'ù'..='ü' => 'U',
        'Ý' | 'ý' | 'ÿ' => 'Y',
        'ß' => 'S',
        c => c.to_ascii_uppercase(),
    }
}

/// Hiragana for katakana, the plain kana for a voiced, half-voiced or small one.
fn plain_kana(c: char) -> char {
    let c = if ('ァ'..='ヶ').contains(&c) { char::from_u32(c as u32 - 0x60).unwrap_or(c) } else { c };
    const VOICED: [(&str, &str); 2] = [
        ("がぎぐげござじずぜぞだぢづでどばびぶべぼ", "かきくけこさしすせそたちつてとはひふへほ"),
        ("ぱぴぷぺぽぁぃぅぇぉっゃゅょゎゔ", "はひふへほあいうえおつやゆよわう"),
    ];
    for (from, to) in VOICED {
        if let Some(at) = from.chars().position(|known| known == c) {
            return to.chars().nth(at).unwrap_or(c);
        }
    }
    c
}

/// The text names are compared by within a letter.
fn order_text(text: &str, language: usize) -> String {
    if language == 0 {
        text.chars().map(plain_kana).collect()
    } else {
        text.chars().map(fold).collect()
    }
}

/// The hundreds of a name's sort key in a language (module documentation), None for a Japanese
/// name that opens with a kanji, whose reading the name does not give.
pub fn letter(text: &str, language: usize) -> Option<i64> {
    let first = text.chars().next()?;
    if language == 0 {
        const ROWS: &str = "あいうえおかきくけこさしすせそたちつてとなにぬねのはひふへほまみむめもやゆよ";
        if let Some(at) = ROWS.chars().position(|known| known == plain_kana(first)) {
            return Some(at as i64);
        }
        if let Some(at) = "らりるれろわをん".chars().position(|known| known == plain_kana(first)) {
            return Some(40 + at as i64);
        }
        return (first.is_ascii_alphanumeric() || ('！'..='～').contains(&first)).then_some(50);
    }
    let spanish = language == 5;
    for c in text.chars() {
        let folded = fold(c);
        if folded.is_ascii_uppercase() {
            return Some(folded as i64 - 'A' as i64 + i64::from(spanish));
        }
        if spanish {
            return Some(0);
        }
    }
    Some(0)
}

impl Ideas {
    pub fn read(files: &[Vec<u8>]) -> Result<Ideas> {
        let names = paths();
        if files.len() != names.len() {
            bail!("the ideas are read from {} files, not {}", names.len(), files.len());
        }
        let mut scripts = Vec::new();
        let mut entries = Vec::new();
        let mut counts = Vec::new();
        for (file, data) in files.iter().enumerate() {
            let script =
                Script::parse(data).map_err(|error| crate::Error::invalid(format!("{}: {error}", names[file])))?;
            let Some(count) = script.statements.iter().position(|statement| statement.tag == "PIC_INFO") else {
                bail!("{} has no PIC_INFO", names[file]);
            };
            let mut list = Vec::new();
            for (number, statement) in script.statements.iter().enumerate() {
                if statement.tag != "PIC_NAME" {
                    continue;
                }
                let values = statement.values();
                let (Some(id), Some(Arg::Str(name)), Some(sort)) = (values.first(), values.get(1), values.get(2))
                else {
                    bail!("{}: PIC_NAME statement {number} is not id, \"name\", sort", names[file]);
                };
                list.push(Entry {
                    statement: Some(number),
                    id: id.int() as i64,
                    name: name.clone(),
                    sort: sort.int() as i64,
                });
            }
            scripts.push(script);
            entries.push(list);
            counts.push(count);
        }
        Ok(Ideas { scripts, entries, counts })
    }

    /// `[{"idea": 1, "name": ..., "sort": ...}]`: the ideas' English names and sort keys.
    pub fn to_json(&self) -> Value {
        let ideas = self.entries[ENGLISH]
            .iter()
            .filter(|entry| entry.id < SCOOP_FIRST)
            .map(|entry| {
                let mut object = Map::new();
                object.insert("idea".into(), Value::from(entry.id));
                object.insert("name".into(), Value::from(name_text(&entry.name, FILE_LANGUAGES[ENGLISH])));
                object.insert("sort".into(), Value::from(entry.sort));
                Value::Object(object)
            })
            .collect();
        Value::Array(ideas)
    }

    pub fn write(&self) -> Vec<Vec<u8>> {
        self.scripts.iter().map(Script::write).collect()
    }

    fn known(&self, id: i64) -> bool {
        self.entries.iter().any(|list| list.iter().any(|entry| entry.id == id))
    }

    /// The edits the mods' `ideas` lists make, (mod, list) in order. An idea the game has is
    /// renamed (`name` English, `names` by language) or re-sorted (`sort` English, `sorts` by
    /// language); an id it lacks (1 to 999) is a new idea, `name` in every language and `names`
    /// over it, put after the last idea of every copy with PIC_INFO raised. A name given without
    /// its language's sort key is put in its place in that language's order: the key of the
    /// first name of its letter that comes after it, those from there on moved up one.
    pub fn changes_all(&self, lists: &[(String, Value)]) -> Result<Vec<Edit>> {
        let mut given: BTreeMap<i64, Given> = BTreeMap::new();
        for (module, list) in lists {
            self.given(list, module, &mut given)
                .map_err(|error| crate::Error::invalid(format!("mod {module}: {error}")))?;
        }
        let mut edits = Vec::new();
        for (file, entries) in self.entries.iter().enumerate() {
            self.file_edits(file, entries, &given, &mut edits)?;
        }
        Ok(edits)
    }

    fn given(&self, list: &Value, module: &str, out: &mut BTreeMap<i64, Given>) -> Result<()> {
        let Some(list) = list.as_array() else {
            bail!("\"ideas\" is a list of {{\"idea\": id, \"name\": ...}}");
        };
        for record in list {
            let Some(object) = record.as_object() else {
                bail!("each of \"ideas\" is an object");
            };
            let Some(id) = object.get("idea").and_then(Value::as_i64) else {
                bail!("each of \"ideas\" names its idea (\"idea\": a number)");
            };
            let label = format!("idea {id}");
            if id == 30000 || id == 30001 {
                bail!(
                    "{label}: _PIC_NAME drops 30000 and a map's 30001 photographs townsperson 260; neither is an idea"
                );
            }
            if id >= SCOOP_FIRST {
                bail!("{label}: ideas are 1 to 999 (from 1000 are scoops, named in scoop.cfg's table)");
            }
            if id < 1 {
                bail!("{label}: ideas are 1 to 999 (0 is no subject)");
            }
            for key in object.keys() {
                if !["idea", "name", "names", "sort", "sorts"].contains(&key.as_str()) {
                    if key == "like" {
                        bail!("{label}: a new idea copies nothing; it gives its \"name\"");
                    }
                    bail!("{label}: an idea has \"name\", \"names\", \"sort\" and \"sorts\", not \"{key}\"");
                }
            }
            let new = !self.known(id);
            let mut names: BTreeMap<usize, String> = BTreeMap::new();
            if let Some(value) = object.get("name") {
                let Some(text) = value.as_str() else {
                    bail!("{label}: \"name\" is a string");
                };
                let languages: Vec<usize> = if new { (0..6).collect() } else { vec![1] };
                for language in languages {
                    names.insert(language, text.to_string());
                }
            }
            for (language, value) in by_language(object, "names", &label)? {
                let Some(text) = value.as_str() else {
                    bail!("{label}: \"names\".{} is a string", LANGUAGE_NAMES[language]);
                };
                names.insert(language, text.to_string());
            }
            if new {
                let Some(english) = names.get(&1).cloned() else {
                    bail!("{label}: a new idea gives its \"name\" (or \"names\" with en)");
                };
                for language in 0..6 {
                    names.entry(language).or_insert_with(|| english.clone());
                }
            }
            let mut sorts: BTreeMap<usize, i64> = BTreeMap::new();
            if let Some(value) = object.get("sort") {
                sorts.insert(1, sort_value(value, &label)?);
            }
            for (language, value) in by_language(object, "sorts", &label)? {
                sorts.insert(language, sort_value(value, &label)?);
            }
            let known = out.entry(id).or_default();
            for (language, text) in names {
                match known.name.get(&language) {
                    Some((other, by)) if *other != text => bail!(
                        "{label}: its {} name is given two ways, by mod {by} and mod {module}",
                        LANGUAGE_NAMES[language]
                    ),
                    Some(_) => {}
                    None => {
                        known.name.insert(language, (text, module.to_string()));
                    }
                }
            }
            for (language, sort) in sorts {
                match known.sort.get(&language) {
                    Some((other, by)) if *other != sort => bail!(
                        "{label}: its {} sort is given two ways, by mod {by} and mod {module}",
                        LANGUAGE_NAMES[language]
                    ),
                    Some(_) => {}
                    None => {
                        known.sort.insert(language, (sort, module.to_string()));
                    }
                }
            }
        }
        Ok(())
    }

    /// One copy's edits: names, keys (given, or moved to make room), new PIC_NAMEs and the count.
    fn file_edits(
        &self,
        file: usize,
        disc: &[Entry],
        given: &BTreeMap<i64, Given>,
        edits: &mut Vec<Edit>,
    ) -> Result<()> {
        let language = FILE_LANGUAGES[file];
        let short = LANGUAGE_NAMES[language];
        let path = &paths()[file];
        let mut entries = disc.to_vec();
        let mut fixed = vec![false; entries.len()];
        let mut owner: Vec<Option<String>> = vec![None; entries.len()];
        let mut to_place = Vec::new();
        let longest =
            disc.iter().filter(|entry| entry.id < SCOOP_FIRST).map(|entry| stored_len(&entry.name, language)).max();
        let most = longest.unwrap_or(0).max(NAME_MOST);
        for (&id, wanted) in given {
            let label = format!("idea {id}");
            let at = match entries.iter().position(|entry| entry.id == id) {
                Some(at) => at,
                None => {
                    entries.push(Entry { statement: None, id, name: Vec::new(), sort: 0 });
                    fixed.push(false);
                    owner.push(None);
                    entries.len() - 1
                }
            };
            if let Some((text, module)) = wanted.name.get(&language) {
                let bytes = name_bytes(text, language, &label)?;
                if bytes != entries[at].name {
                    let length = stored_len(&bytes, language);
                    if length > most {
                        bail!(
                            "mod {module}: {label}: its {short} name {text:?} takes {length} bytes; the notebook prints at most {most} \
                             (MenuInventNetaMemoDraw's text[0x20] after \"[bulb2]\", or the game's own longest)"
                        );
                    }
                    entries[at].name = bytes;
                    owner[at] = Some(module.clone());
                    if !wanted.sort.contains_key(&language) {
                        to_place.push(at);
                    }
                }
            }
            if let Some((sort, module)) = wanted.sort.get(&language) {
                entries[at].sort = *sort;
                fixed[at] = true;
                owner[at] = Some(module.clone());
            } else if entries[at].statement.is_none() && !to_place.contains(&at) {
                to_place.push(at);
            }
        }
        for at in to_place {
            place(&mut entries, &fixed, at, language)?;
        }
        if entries.len() > ENTRIES_MOST {
            bail!("{path}: {} ideas and scoops; the notebook sorts at most {ENTRIES_MOST} (UpdataNetaMemoStr's sort_keys)", entries.len());
        }
        let taken = units(&entries, language);
        if taken >= UNITS {
            bail!(
                "{path}: the {short} ideas and scoops take {taken} 16-byte units, and {} is the most ({UNITS_WHY}): \
                 shorter names, or fewer new ideas",
                UNITS - 1
            );
        }
        // the changes, by the mods that make them (moved keys by build)
        let module_of = |at: usize| owner[at].clone().unwrap_or_else(|| "build".to_string());
        for (at, entry) in entries.iter().enumerate() {
            let Some(statement) = entry.statement else {
                continue;
            };
            let old = &disc[at];
            if entry.name != old.name {
                edits.push(Edit {
                    file,
                    statement,
                    arg: 1,
                    to: To::Arg(cfg_text::str_text(&entry.name)),
                    what: format!("idea {} name ({short})", entry.id),
                    module: module_of(at),
                });
            }
            if entry.sort != old.sort {
                edits.push(Edit {
                    file,
                    statement,
                    arg: 2,
                    to: To::Arg(cfg_text::int_text(entry.sort)),
                    what: format!("idea {} sort ({short})", entry.id),
                    module: if fixed[at] { module_of(at) } else { "build".to_string() },
                });
            }
        }
        let added: Vec<usize> = (disc.len()..entries.len()).collect();
        if added.is_empty() {
            return Ok(());
        }
        let Some(after) = disc.iter().filter(|entry| entry.id < SCOOP_FIRST).filter_map(|entry| entry.statement).max()
        else {
            bail!("{path} has no idea to put new ones after");
        };
        for at in added {
            let entry = &entries[at];
            let args = [cfg_text::int_text(entry.id), cfg_text::str_text(&entry.name), cfg_text::int_text(entry.sort)];
            edits.push(Edit {
                file,
                statement: after,
                arg: 0,
                to: To::Insert(cfg_text::statement_text("PIC_NAME", &args)),
                what: format!("idea {} (new)", entry.id),
                module: module_of(at),
            });
        }
        let count = self.counts[file];
        let had = self.scripts[file].statements[count].values().first().map_or(0, Arg::int) as i64;
        edits.push(Edit {
            file,
            statement: count,
            arg: 0,
            to: To::Arg(cfg_text::int_text(had + (entries.len() - disc.len()) as i64)),
            what: format!("ideas count ({short})"),
            module: "build".to_string(),
        });
        Ok(())
    }

    /// The copies with the edits made; a copy past LoadFilePictureName's buffer is refused.
    pub fn apply(&self, edits: &[Edit]) -> Result<Vec<Vec<u8>>> {
        let written = cfg_text::apply_edits(&self.scripts, edits)?;
        for (file, data) in written.iter().enumerate() {
            if data.len() > FILE_MOST {
                bail!("{} would be {} bytes; LoadFilePictureName reads it into {FILE_MOST}", paths()[file], data.len());
            }
        }
        Ok(written)
    }
}

/// A `names` or `sorts` object by language: {"en": ..., "fr": ...}.
fn by_language<'a>(object: &'a Map<String, Value>, key: &str, label: &str) -> Result<Vec<(usize, &'a Value)>> {
    let Some(value) = object.get(key) else {
        return Ok(Vec::new());
    };
    let Some(map) = value.as_object() else {
        bail!("{label}: \"{key}\" is {{\"en\": ..., \"fr\": ...}}, by language");
    };
    let mut out = Vec::new();
    for (short, value) in map {
        match LANGUAGE_NAMES[..6].iter().position(|name| name == short) {
            Some(language) => out.push((language, value)),
            None => bail!(
                "{label}: \"{key}\" has {short}; the languages are {} (Korean reads the English file)",
                LANGUAGE_NAMES[..6].join(", ")
            ),
        }
    }
    Ok(out)
}

fn sort_value(value: &Value, label: &str) -> Result<i64> {
    match value.as_i64() {
        Some(sort) if (-32768..=32767).contains(&sort) => Ok(sort),
        _ => bail!("{label}: a sort key is a whole number from -32768 to 32767 (PIC_NAME_INFO's s16)"),
    }
}

/// A name as a language's file keeps it.
fn name_bytes(text: &str, language: usize, label: &str) -> Result<Vec<u8>> {
    let short = LANGUAGE_NAMES[language];
    if text.is_empty() {
        bail!("{label}: its {short} name is empty");
    }
    if text.chars().any(|c| c.is_control() || c == '\\') {
        bail!("{label}: its {short} name {text:?} has a control character or a backslash");
    }
    match item_data::name_bytes(text, language) {
        Some(bytes) => Ok(bytes),
        None => bail!("{label}: its {short} name {text:?} has characters the game's {short} text cannot hold"),
    }
}

/// Gives entry `at` the key of its name's place in its language's order: the key of the first
/// name of its letter after it (or one past the letter's last), those from there on moved up
/// one unless a mod gave them their keys.
fn place(entries: &mut [Entry], fixed: &[bool], at: usize, language: usize) -> Result<()> {
    let text = name_text(&entries[at].name, language);
    let letter = letter(&text, language).unwrap_or(50);
    let wanted = order_text(&text, language);
    let mut same: Vec<usize> =
        (0..entries.len()).filter(|&other| other != at && entries[other].sort.div_euclid(100) == letter).collect();
    same.sort_by_key(|&other| entries[other].sort);
    let next =
        same.iter().copied().find(|&other| order_text(&name_text(&entries[other].name, language), language) > wanted);
    let key = match next {
        Some(other) => entries[other].sort,
        None => same.last().map_or(letter * 100, |&last| entries[last].sort + 1),
    };
    for &other in &same {
        if entries[other].sort >= key && !fixed[other] {
            entries[other].sort += 1;
        }
    }
    if same.iter().map(|&other| entries[other].sort).chain([key]).any(|sort| sort >= letter * 100 + 100) {
        bail!(
            "idea {}: the {} letter {letter} has no room for another place (100 keys a letter); give its \"sorts\"",
            entries[at].id,
            LANGUAGE_NAMES[language]
        );
    }
    entries[at].sort = key;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn letters_follow_each_language() {
        assert_eq!(letter("Barrel", 1), Some(1));
        assert_eq!(letter("Œuf", 2), Some(14));
        assert_eq!(letter("Écran", 2), Some(4));
        assert_eq!(letter("Banco", 5), Some(2));
        assert_eq!(letter("¡Hurra por Roca!", 5), Some(0));
        assert_eq!(letter("ランプ", 0), Some(40));
        assert_eq!(letter("ゆうひ", 0), Some(36));
        assert_eq!(letter("バス", 0), Some(25));
        assert_eq!(letter("木", 0), None);
        assert_eq!(name_text(b"Cr[UNI00e9]puscule", 2), "Crépuscule");
        assert_eq!(stored_len(b"Cr[UNI00e9]puscule", 5), 10);
        assert_eq!(stored_len(b"Cr[UNI00e9]puscule", 2), 18);
    }

    /// Every copy of neta2.lst of an extracted disc (C2S_CORPUS) reads, writes back unchanged,
    /// and its JSON changes nothing; the sort keys follow the letters; an idea is added and
    /// renamed in every copy, in its place; the limits refuse.
    #[test]
    fn corpus_ideas() {
        let Some(corpus) = std::env::var_os("C2S_CORPUS") else {
            return;
        };
        let root = std::path::Path::new(&corpus);
        let files: Vec<Vec<u8>> = paths().iter().map(|path| std::fs::read(root.join(path)).unwrap()).collect();
        let ideas = Ideas::read(&files).unwrap();
        assert_eq!(ideas.write(), files);
        let json = ideas.to_json();
        assert_eq!(json.as_array().unwrap().len(), 241);
        assert_eq!(json[0], json!({"idea": 1, "name": "Trashcan", "sort": 1911}));
        assert!(ideas.changes_all(&[("m".into(), json.clone())]).unwrap().is_empty());
        // the keys' hundreds are the letters (Japanese names opening with kanji aside)
        for (file, entries) in ideas.entries.iter().enumerate() {
            let language = FILE_LANGUAGES[file];
            for entry in entries {
                let text = name_text(&entry.name, language);
                if let Some(letter) = letter(&text, language) {
                    assert_eq!(letter, entry.sort / 100, "{} {text}", paths()[file]);
                }
            }
            assert!(units(entries, language) < UNITS, "{}", paths()[file]);
        }
        // a new idea, in its place in every language
        let new =
            json!([{"idea": 251, "name": "Endless Turning", "names": {"fr": "Rotation sans fin", "ja": "カイテン"}}]);
        let edits = ideas.changes_all(&[("m".into(), new)]).unwrap();
        let written = ideas.apply(&edits).unwrap();
        let again = Ideas::read(&written).unwrap();
        for (file, entries) in again.entries.iter().enumerate() {
            let language = FILE_LANGUAGES[file];
            let text = String::from_utf8_lossy(&written[file]);
            assert!(text.contains("PIC_INFO 295;"), "{}", paths()[file]);
            let at = entries.iter().position(|entry| entry.id == 251).unwrap();
            assert_eq!(entries[at - 1].id, 250, "after the last idea, before the scoops");
            assert!(entries[at + 1].id >= SCOOP_FIRST);
            let mut order: Vec<&Entry> = entries.iter().filter(|entry| entry.id < SCOOP_FIRST).collect();
            order.sort_by_key(|entry| entry.sort);
            let place = order.iter().position(|entry| entry.id == 251).unwrap();
            let name = |entry: &Entry| order_text(&name_text(&entry.name, language), language);
            // after the names of its letter before it, before those after it
            let letter_of = |entry: &Entry| entry.sort.div_euclid(100);
            let before = order[place - 1];
            assert!(
                letter_of(before) != letter_of(order[place]) || name(before) < name(order[place]),
                "{}",
                paths()[file]
            );
            let after = order[place + 1];
            assert!(
                letter_of(after) != letter_of(order[place]) || name(after) > name(order[place]),
                "{}",
                paths()[file]
            );
            let twice = |entries: &[Entry]| {
                let mut keys: Vec<i64> = entries.iter().map(|entry| entry.sort).collect();
                keys.sort();
                keys.windows(2).filter(|pair| pair[0] == pair[1]).count()
            };
            assert_eq!(twice(entries), twice(&ideas.entries[file]), "no key twice more ({})", paths()[file]);
        }
        let english = again.to_json();
        let find =
            |json: &Value, id: i64| json.as_array().unwrap().iter().find(|idea| idea["idea"] == id).cloned().unwrap();
        assert_eq!(find(&english, 251), json!({"idea": 251, "name": "Endless Turning", "sort": 411}));
        assert_eq!(find(&english, 160)["sort"], 412, "Energy Pipe moved up");
        assert_eq!(find(&english, 118)["sort"], 409, "Elevator stays");
        // a rename moves the idea to its letter; French alone
        let rename = json!([{"idea": 5, "name": "Zebra Cask", "names": {"fr": "Tonneau zébré"}}]);
        let written = ideas.apply(&ideas.changes_all(&[("m".into(), rename)]).unwrap()).unwrap();
        let again = Ideas::read(&written).unwrap();
        assert_eq!(find(&again.to_json(), 5)["sort"].as_i64().unwrap() / 100, 25);
        assert!(String::from_utf8_lossy(&written[3]).contains("\"Tonneau z[UNI00e9]br[UNI00e9]\""));
        assert_eq!(written[4], files[4], "German untouched");
        // a sort given stays as given
        let sorted = json!([{"idea": 252, "name": "Zz", "sort": 2599, "sorts": {"fr": 7}}]);
        let written = ideas.apply(&ideas.changes_all(&[("m".into(), sorted)]).unwrap()).unwrap();
        assert!(String::from_utf8_lossy(&written[2]).contains("PIC_NAME 252,\"Zz\",2599;"));
        assert!(String::from_utf8_lossy(&written[3]).contains("PIC_NAME 252,\"Zz\",7;"));
        // refusals
        let refused = |list: Value| ideas.changes_all(&[("m".into(), list)]).unwrap_err().to_string();
        assert!(refused(json!([{"idea": 1000, "name": "x"}])).contains("scoops"));
        assert!(refused(json!([{"idea": 30001, "name": "x"}])).contains("260"));
        assert!(refused(json!([{"idea": 0, "name": "x"}])).contains("1 to 999"));
        assert!(refused(json!([{"idea": 251}])).contains("gives its \"name\""));
        assert!(refused(json!([{"idea": 251, "like": 1, "name": "x"}])).contains("copies nothing"));
        assert!(refused(json!([{"idea": 251, "name": "A name far longer than the notebook"}])).contains("text[0x20]"));
        let two = ideas
            .changes_all(&[
                ("a".into(), json!([{"idea": 3, "name": "Lamp"}])),
                ("b".into(), json!([{"idea": 3, "name": "Light"}])),
            ])
            .unwrap_err()
            .to_string();
        assert!(two.contains("mod a") && two.contains("mod b"), "{two}");
        // the French units run out first: 17 a disc's French copy has left
        let many: Vec<Value> = (251..270).map(|id| json!({"idea": id, "name": format!("Idea {id}")})).collect();
        let error = refused(Value::Array(many));
        assert!(error.contains("menu/2/neta2.lst") && error.contains("pic_name_text_buff_1660"), "{error}");
        let fit: Vec<Value> = (251..262).map(|id| json!({"idea": id, "name": format!("Idea {id}")})).collect();
        ideas.apply(&ideas.changes_all(&[("m".into(), Value::Array(fit))]).unwrap()).unwrap();
        eprintln!("241 ideas in {} copies round-trip", files.len());
    }
}
