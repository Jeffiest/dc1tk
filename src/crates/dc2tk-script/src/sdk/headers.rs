//! Loading the SDK's headers: their texts, the declarations they make, and the context a
//! native table's scripts are compiled and presented with.

use super::*;

/// Files under a folder with an extension, sorted by their parts (lowercased on Windows, whose
/// names ignore case), as paths from the folder with "/".
pub fn files_under(base: &Path, extension: &str) -> Result<Vec<String>> {
    let mut found =
        paths::walk(base, |entry| entry.is_dir || entry.path.extension().is_some_and(|ext| ext == extension))?;
    paths::sort_by_parts(&mut found);
    Ok(found)
}

/// A text file's text (dc2tk_disc::textfile::read_text): UTF-8, its \r\n and \r line ends read as
/// \n.
pub fn read_text(path: &Path) -> Result<String> {
    Ok(dc2tk_disc::textfile::read_text(path)?)
}

/// Every header under a folder, by path from it.
pub fn load_texts(directory: &Path) -> Result<IndexMap<String, String>> {
    let mut texts = IndexMap::new();
    for relative in files_under(directory, "h")? {
        let text = read_text(&directory.join(&relative))?;
        texts.insert(relative, text);
    }
    Ok(texts)
}

/// The SDK's headers and each definition set's, a later one's winning.
pub fn header_texts(defs: &Definitions, sdk_dir: &Path) -> Result<IndexMap<String, String>> {
    let mut texts = load_texts(sdk_dir)?;
    for directory in &defs.sets {
        let include = directory.join("include");
        if include.is_dir() {
            texts.extend(load_texts(&include)?);
        }
    }
    Ok(texts)
}

pub(super) fn ext_number(name: &str) -> Option<i64> {
    let digits = name.strip_prefix("_EXT_")?;
    (!digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())).then(|| digits.parse().ok()).flatten()
}

/// The declarations headers make: lists of natives, programs, inlines and constants, and the
/// value kinds.
#[derive(Clone, Debug, Default)]
pub struct Declarations {
    pub natives: Vec<Node>,
    pub programs: Vec<Node>,
    pub inlines: Vec<Node>,
    pub constants: Vec<Node>,
    pub kinds: Map,
}

impl Declarations {
    /// A program's declarations ({"natives": [...], ...}).
    pub fn of(node: &Node) -> Declarations {
        Declarations {
            natives: node.list("natives").to_vec(),
            programs: node.list("programs").to_vec(),
            inlines: node.list("inlines").to_vec(),
            constants: node.list("constants").to_vec(),
            kinds: node["kinds"].as_object().cloned().unwrap_or_default(),
        }
    }

    /// The declarations as a program tree holds them.
    pub fn to_node(&self) -> Node {
        node!({
            "natives": self.natives.clone(),
            "programs": self.programs.clone(),
            "inlines": self.inlines.clone(),
            "constants": self.constants.clone(),
            "kinds": Node::Map(self.kinds.clone()),
        })
    }

    /// Adds another file's declarations: lists after this one's, kinds over it.
    pub fn add_from(&mut self, other: &Node) {
        self.add(other);
    }

    fn add(&mut self, other: &Node) {
        for key in DECLARATION_KEYS {
            match key {
                "kinds" => {
                    if let Some(kinds) = other.get(key).and_then(Node::as_object) {
                        for (name, value) in kinds.iter() {
                            self.kinds.insert(name.to_string(), value.clone());
                        }
                    }
                }
                _ => {
                    let into = match key {
                        "natives" => &mut self.natives,
                        "programs" => &mut self.programs,
                        "inlines" => &mut self.inlines,
                        _ => &mut self.constants,
                    };
                    into.extend(other.list(key).iter().cloned());
                }
            }
        }
    }
}

/// Reads a header's declarations, and those of the SDK headers it includes; the native table the
/// headers name.
pub fn header_declarations(
    path: &str,
    headers: &IndexMap<String, String>,
    declarations: &mut Declarations,
    seen: &mut HashSet<String>,
    parse: &dyn Fn(&str) -> Result<Node>,
) -> Result<Option<String>> {
    if path == PRELUDE || seen.contains(path) {
        return Ok(None);
    }
    let Some(text) = headers.get(path) else {
        return Err(Error::invalid(format!("cannot find SDK header <{path}>")));
    };
    seen.insert(path.to_string());
    let header = parse(text)?;
    let mut table = header["natives"].as_str().map(str::to_string);
    declarations.add(&header["declarations"]);
    for item in header.list("functions") {
        if item.kind() == kinds::INCLUDE && item["system"].is_set() {
            let nested = header_declarations(item.text("path"), headers, declarations, seen, parse)?;
            if table.is_none() {
                table = nested;
            }
        }
    }
    Ok(table)
}

pub fn header_name(table: &str) -> String {
    format!("{table}.h")
}

/// What a script's headers declare, as the decompiler and compiler use it.
#[derive(Clone, Debug, Default)]
pub struct Context {
    pub table: String,
    /// Natives by their SDK name, each with the canonical (engine) name it stands for.
    pub natives: IndexMap<String, Node>,
    /// The same natives by canonical name.
    pub by_original: HashMap<String, Node>,
    /// The engine's names of the table's natives.
    pub originals: HashSet<String>,
    pub programs: IndexMap<String, i64>,
    pub program_names: HashMap<i64, String>,
    pub inlines: IndexMap<String, Node>,
    /// Constants by name: their value as a tree node.
    pub constants: IndexMap<String, Node>,
    /// Each enum's members: {value: name}.
    pub enums: IndexMap<String, IndexMap<i64, String>>,
    /// Float constants by their bits.
    pub float_names: IndexMap<i64, String>,
    pub old_groups: HashMap<i64, i64>,
    pub old_direct: HashMap<i64, i64>,
    pub old_table_groups: HashMap<i64, i64>,
    pub old_table: bool,
    pub raw_table: bool,
    pub by_number: HashMap<i64, String>,
    /// The headers a decompiled script includes.
    pub includes: Vec<String>,
    /// The event bank of each sound set, by number.
    pub sound_set_banks: HashMap<i64, String>,
    /// Names for the game's asset strings: {(prefix, text): name}.
    pub asset_names: IndexMap<(String, String), String>,
    /// The constant a slot takes while a load has put a pack in it: {(slot, pack): name}, for the
    /// folder's scripts (slots::loaded_names).
    pub slot_packs: HashMap<(i64, String), String>,
    /// The function being resolved was written under #pragma std c89: a native in an operand C
    /// may not evaluate runs only when C would evaluate it.
    pub strict: bool,
    pub(super) reserved: HashSet<String>,
}

fn numbered_groups(found: &IndexMap<String, i64>) -> HashMap<i64, i64> {
    found.iter().filter_map(|(key, value)| Some((key.parse::<i64>().ok()?, *value))).collect()
}

/// Refuses a constant declared twice: an enum member whose name another enum member or a #define
/// already has (C rejects it, and a number could print as either), or a #define with two values.
pub fn check_constants(constants: &[Node]) -> Result<()> {
    let shown = |declared: &Node| {
        let value = &declared["value"];
        let value = match value.kind() {
            kinds::INT => value["value"].as_i64().map_or_else(String::new, |number| number.to_string()),
            kinds::STRING => format!("{:?}", value["value"].as_str().unwrap_or_default()),
            kind => kind.to_string(),
        };
        match declared["enum"].as_str() {
            Some(enumeration) => format!("= {value} in enum {enumeration}"),
            None => format!("#define {value}"),
        }
    };
    let mut first: HashMap<&str, &Node> = HashMap::new();
    for declared in constants {
        let name = declared.text("name");
        let Some(earlier) = first.insert(name, declared) else {
            continue;
        };
        let member = |node: &Node| node["enum"].as_str().is_some();
        if member(earlier) || member(declared) || earlier["value"] != declared["value"] {
            return Err(Error::invalid(format!(
                "{name} is declared twice ({}, then {}); give one of them another name",
                shown(earlier),
                shown(declared)
            )));
        }
    }
    Ok(())
}

/// The context of a set of declarations for a native table.
pub fn context(declarations: &Declarations, table: &str, tables: &BTreeMap<String, Table>) -> Result<Context> {
    check_constants(&declarations.constants)?;
    let rows = tables.get(table);
    let original_by_number: IndexMap<i64, String> =
        rows.map(|rows| rows.natives.iter().map(|row| (row.no as i64, row.name.clone())).collect()).unwrap_or_default();
    let mut found = Context { table: table.to_string(), ..Context::default() };
    for declared in &declarations.natives {
        let no = declared["no"].as_i64().unwrap_or(0);
        let canonical = original_by_number.get(&no).cloned().unwrap_or_else(|| format!("_EXT_{no}"));
        let with = declared.clone().with("canonical", canonical.clone());
        found.natives.insert(declared.text("name").to_string(), with.clone());
        found.by_original.insert(canonical, with);
    }
    for declared in &declarations.programs {
        found.programs.insert(declared.text("name").to_string(), declared["no"].as_i64().unwrap_or(0));
    }
    for declared in &declarations.constants {
        found.constants.insert(declared.text("name").to_string(), declared["value"].clone());
    }
    for declared in &declarations.programs {
        let (name, no) = (declared.text("name").to_string(), declared["no"].as_i64().unwrap_or(0));
        if !found.constants.contains_key(&name) {
            found.constants.insert(name.clone(), node!({"kind": "int", "value": no}));
        }
        found.enums.entry("program".into()).or_default().entry(no).or_insert(name);
    }
    for declared in &declarations.constants {
        let value = &declared["value"];
        let name = declared.text("name").to_string();
        if let Some(enum_name) = declared["enum"].as_str() {
            if let Some(member) = value["value"].as_i64() {
                found.enums.entry(enum_name.to_string()).or_default().entry(member).or_insert(name);
            }
        } else if value.kind() == kinds::FLOAT {
            if let Some(bits) = value["bits"].as_i64() {
                found.float_names.entry(bits).or_insert(name);
            }
        }
    }
    found.originals = original_by_number.values().cloned().collect();
    found.program_names = found.programs.iter().map(|(name, number)| (*number, name.clone())).collect();
    if let Some(rows) = rows {
        found.old_groups = numbered_groups(&rows.old.old_groups);
        found.old_direct = numbered_groups(&rows.old.old_direct);
        found.old_table_groups = numbered_groups(&rows.old.old_table_groups);
    }
    found.by_number = declarations
        .natives
        .iter()
        .map(|declared| (declared["no"].as_i64().unwrap_or(0), declared.text("name").to_string()))
        .collect();
    found.refresh_reserved();
    for inline in &declarations.inlines {
        let shadowed: HashSet<String> =
            inline.list("params").iter().map(|param| param.text("name").to_string()).collect();
        let body = with_constants(&inline["body"], &found.constants, &shadowed);
        let resolved = statements(body.as_array().map(Vec::as_slice).unwrap_or(&[]), &found, &mut Vec::new())?;
        found.inlines.insert(inline.text("name").to_string(), inline.clone().with("body", resolved));
        found.refresh_reserved();
    }
    Ok(found)
}

impl Context {
    fn refresh_reserved(&mut self) {
        let mut names: HashSet<String> = KEYWORDS.iter().map(|word| word.to_string()).collect();
        names.extend(self.natives.keys().cloned());
        names.extend(self.inlines.keys().cloned());
        names.extend(self.programs.keys().cloned());
        names.extend(self.constants.keys().cloned());
        self.reserved = names;
    }
}

/// The context for a native table's header and any extra includes; None when the SDK has no
/// header for the table.
pub fn header_context(
    table: &str,
    headers: &IndexMap<String, String>,
    tables: &BTreeMap<String, Table>,
    extra: &[String],
    parse: &dyn Fn(&str) -> Result<Node>,
) -> Result<Option<Context>> {
    let name = header_name(table);
    if !headers.contains_key(&name) {
        return Ok(None);
    }
    let mut declarations = Declarations::default();
    let mut seen = HashSet::new();
    header_declarations(&name, headers, &mut declarations, &mut seen, parse)?;
    for include in extra {
        header_declarations(include, headers, &mut declarations, &mut seen, parse)?;
    }
    let mut found = context(&declarations, table, tables)?;
    found.includes = std::iter::once(name).chain(extra.iter().cloned()).collect();
    Ok(Some(found))
}

/// The names a script's own names must not take.
pub fn reserved(found: Option<&Context>) -> HashSet<String> {
    match found {
        None => KEYWORDS.iter().map(|word| word.to_string()).collect(),
        Some(found) => found.reserved.clone(),
    }
}
