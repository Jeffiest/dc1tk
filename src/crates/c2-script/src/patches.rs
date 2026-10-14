//! Patches: C files in a mod's folder that change scripts without copying them. A patch says which
//! scripts it changes with #pragma target lines and holds only what it adds or replaces:
//!
//!   #pragma target map s51                    every language of map s51's script (s51_1.c to s51_5.c)
//!   #pragma target script dungeon/monster/e33a.c   scripts by path without readable names, * and ? allowed
//!   #pragma target monster golem              the AI script of a monster kind (MONSTER_GOLEM in disc.h)
//!   #pragma target library lib/map/x.c        every script that includes a library
//!
//! build merges the patch into each of those scripts after the script's libraries are included, so
//! a patch reaches a function the script gets from a library without forking the library:
//!
//! - A function after #pragma override replaces the script's function of that name for that script
//!   alone; original(...) inside it calls the function it replaced. Several mods can override one
//!   function when the later ones call original(): the last mod's runs first.
//! - Other functions and globals are added. A name the script already has is refused (the patch
//!   most likely meant to override it); a name another mod adds as well is renamed for this one, so
//!   mods never need to agree on names. A patch's libraries can declare globals of their own.
//! - PROGRAM lines add programs. A program the script already has is changed by overriding its
//!   function instead, so that several mods can change it.
//! - A function after #pragma every_frame is called before every wait; in the script, so it runs
//!   once a frame whatever the script is waiting in (a monster's AI keeping a timer, say). It takes
//!   no parameters and must not wait.
//! - NEW_FLAG(name), NEW_PROGRAM(name) and NEW_MONSTER_VAR(name), in any file of the mod, are
//!   numbers build gives the mod (numbers); every patch of the mod sees them as constants.
//! - LANGUAGE is the language of the script being patched (s51_2.c is LANGUAGE_FRENCH), for #if
//!   and in code, so one patch can carry each language's text. Scripts that are not language
//!   variants (monster AI, say) have LANGUAGE -1.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use indexmap::IndexMap;
use regex::Regex;

use crate::asm::{self, Token};
use crate::index;
use crate::node;
use crate::node::{Map, Node};
use crate::source::py_str;
use crate::{additions, library, names, numbers, parse, pathnames, sdk, Error, Result};

pub const LANGUAGES: [(&str, i64); 6] = [
    ("LANGUAGE_JAPANESE", 0),
    ("LANGUAGE_ENGLISH", 1),
    ("LANGUAGE_FRENCH", 2),
    ("LANGUAGE_GERMAN", 3),
    ("LANGUAGE_ITALIAN", 4),
    ("LANGUAGE_SPANISH", 5),
];
pub const NO_LANGUAGE: i64 = -1;
const ORIGINAL: &str = "original";

fn regex(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).unwrap())
}

fn variant() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    regex(&CELL, r"_([1-5])\.c$")
}

fn target_line() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    regex(&CELL, r"(?m)^[ \t]*#[ \t]*pragma[ \t]+target\b(.*)$")
}

fn list<'a>(node: &'a Node, key: &str) -> &'a [Node] {
    node[key].as_array().map(Vec::as_slice).unwrap_or(&[])
}

fn text<'a>(node: &'a Node, key: &str) -> &'a str {
    node[key].as_str().unwrap_or("")
}

fn truthy(node: &Node) -> bool {
    crate::source::truthy(node)
}

pub fn language_of(plain: &str) -> i64 {
    if let Some(found) = variant().captures(plain) {
        return found[1].parse().unwrap();
    }
    if plain.starts_with("map/") {
        0
    } else {
        NO_LANGUAGE
    }
}

#[derive(Clone, Debug)]
pub struct Target {
    pub kind: String,
    pub values: Vec<String>,
}

pub fn targets_in(text: &str) -> Result<Vec<Target>> {
    let mut found = Vec::new();
    for capture in target_line().captures_iter(&index::without_comments(text)) {
        let line = &capture[1];
        let words: Vec<String> = asm::tokenize(line)?
            .into_iter()
            .map(|token| match token {
                Token::Word(word) | Token::Text(word) => word,
            })
            .collect();
        if words.len() < 2 || !parse::TARGET_KINDS.contains(&words[0].as_str()) {
            return Err(Error::invalid(format!(
                "#pragma target takes {}, then what to pick: {}",
                parse::TARGET_KINDS.join(", "),
                c2_py::strip(line)
            )));
        }
        found.push(Target { kind: words[0].clone(), values: words[1..].to_vec() });
    }
    Ok(found)
}

pub fn is_patch(text: &str) -> bool {
    target_line().is_match(&index::without_comments(text))
}

/// {name in upper case: AI script code} from the tree's disc.h, with the kind's number as a name
/// too.
pub fn monster_scripts(tree: &Path) -> Result<HashMap<String, String>> {
    static CELL: OnceLock<Regex> = OnceLock::new();
    let line = regex(&CELL, r"(?m)^\s*MONSTER_(\w+)\s*=\s*(\d+),\s*//\s*model\s+\S+,\s*AI\s+(\w+)");
    let path = tree.join("include").join("disc.h");
    let mut found = HashMap::new();
    if path.is_file() {
        for capture in line.captures_iter(&c2_py::read_text(&path)?) {
            found.insert(capture[1].to_string(), capture[3].to_string());
            found.entry(capture[2].to_string()).or_insert_with(|| capture[3].to_string());
        }
    }
    Ok(found)
}

/// The tree's scripts, and what #pragma target lines pick among them.
pub struct Scripts {
    tree: PathBuf,
    pub paths: Vec<String>,
    pub plain: HashMap<String, String>,
    monsters: OnceLock<HashMap<String, String>>,
    includers: OnceLock<BTreeMap<String, Vec<String>>>,
}

impl Scripts {
    pub fn new(tree: &Path) -> Result<Scripts> {
        let mut paths: Vec<String> =
            index::source_files(tree).into_iter().filter(|path| !index::is_library(path)).collect();
        paths.sort();
        let plain =
            paths.iter().map(|path| Ok((path.clone(), pathnames::unnamed(tree, path)?))).collect::<Result<_>>()?;
        Ok(Scripts { tree: tree.to_path_buf(), paths, plain, monsters: OnceLock::new(), includers: OnceLock::new() })
    }

    pub fn in_map(&self, code: &str) -> Vec<String> {
        let found: Vec<&String> =
            self.paths.iter().filter(|path| additions::in_map(&self.plain[*path], code)).collect();
        // the PAL game loads the numbered language variants; the unnumbered Japanese script only
        // when a map has nothing else
        let variants: Vec<&String> =
            found.iter().copied().filter(|path| variant().is_match(&self.plain[*path])).collect();
        if variants.is_empty() { found } else { variants }.into_iter().cloned().collect()
    }

    fn monster(&self, value: &str) -> Result<Vec<String>> {
        if self.monsters.get().is_none() {
            let _ = self.monsters.set(monster_scripts(&self.tree)?);
        }
        let key = value.to_uppercase();
        let key = key.strip_prefix("MONSTER_").unwrap_or(&key);
        let Some(script) = self.monsters.get().unwrap().get(key) else {
            return Ok(Vec::new());
        };
        let wanted = format!("dungeon/monster/{script}.c");
        Ok(self.paths.iter().filter(|path| self.plain[*path] == wanted).cloned().collect())
    }

    fn users(&self, path: &str) -> Result<Vec<String>> {
        if self.includers.get().is_none() {
            let _ = self.includers.set(index::includers(&index::outlines(&self.tree)?));
        }
        let includers = self.includers.get().unwrap();
        let direct = includers.get(path).filter(|users| !users.is_empty());
        Ok(match direct {
            Some(users) => users.clone(),
            None => includers.get(&pathnames::named(&self.tree, path)?).cloned().unwrap_or_default(),
        })
    }

    pub fn picked(&self, target: &Target) -> Result<BTreeSet<String>> {
        let mut found = BTreeSet::new();
        for value in &target.values {
            match target.kind.as_str() {
                "map" => found.extend(self.in_map(value)),
                "script" => found.extend(
                    self.paths
                        .iter()
                        .filter(|path| additions::fnmatch(&self.plain[*path], value) || additions::fnmatch(path, value))
                        .cloned(),
                ),
                "monster" => found.extend(self.monster(value)?),
                "library" => found.extend(self.users(value)?),
                _ => {}
            }
        }
        Ok(found)
    }
}

/// A mod's files for planning: its name, {path: text} of its own source files (in path order),
/// texts of its other files, and the mods it requires (mods::chosen), whose numbers it sees.
pub struct ModTexts {
    pub name: String,
    pub texts: Vec<(PathBuf, String)>,
    pub written: Vec<String>,
    pub requires: Vec<String>,
}

pub struct Patch {
    pub module: String,
    pub path: PathBuf,
    pub text: String,
    pub scripts: Vec<String>,
    pub plain: HashMap<String, String>,
    pub numbers: IndexMap<String, i64>,
}

impl Patch {
    pub fn file_name(&self) -> String {
        self.path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
    }
}

/// Which scripts each patch changes (indexes into patches, in mod order), the patches, and the
/// numbers the mods were given ({mod: {name: number}}).
pub struct Plan {
    pub scripts: BTreeMap<String, Vec<usize>>,
    pub patches: Vec<Patch>,
    pub numbers: BTreeMap<String, IndexMap<String, i64>>,
}

impl Plan {
    pub fn for_script(&self, script: &str) -> Option<Vec<&Patch>> {
        self.scripts.get(script).map(|found| found.iter().map(|at| &self.patches[*at]).collect())
    }
}

/// Which scripts each patch of the chosen mods changes, and the numbers the mods ask for. written
/// holds texts of mods left out, whose hand-written numbers are avoided too. lock is the numbers
/// file (numbers::LOCK), None to give numbers without keeping them.
pub fn plan(
    mod_list: &[ModTexts],
    tree: &Path,
    consts: &IndexMap<String, i64>,
    lock: Option<&Path>,
    written: &[String],
) -> Result<Plan> {
    let (scripts, mut patches) = located(mod_list, tree)?;
    let mut wanted = numbers::Wanted::new();
    for module in mod_list {
        let asked = numbers::requests(module.texts.iter().map(|(_, text)| text));
        if !asked.is_empty() {
            wanted.insert(module.name.clone(), asked);
        }
    }
    let mut given = BTreeMap::new();
    if !wanted.is_empty() {
        let kinds: BTreeSet<String> = wanted.values().flat_map(|asked| asked.keys().cloned()).collect();
        let mut used = numbers::used_by_tree(tree, consts, &kinds)?;
        let hand: Vec<&String> = mod_list
            .iter()
            .flat_map(|module| module.texts.iter().map(|(_, text)| text).chain(module.written.iter()))
            .chain(written.iter())
            .collect();
        let by_hand = numbers::written(hand);
        for kind in numbers::KINDS {
            used.get_mut(kind).unwrap().extend(by_hand[kind].iter().copied());
        }
        let mut held = numbers::read_lock(lock)?;
        let (found, changed) = numbers::assign(&wanted, &mut held, &used)?;
        if changed {
            if let Some(lock) = lock {
                numbers::write_lock(lock, &held)?;
            }
        }
        given = found;
    }
    let requires: HashMap<&str, &[String]> =
        mod_list.iter().map(|module| (module.name.as_str(), module.requires.as_slice())).collect();
    for patch in &mut patches {
        // the numbers of the mods it requires, then its own, which win where a name is in both
        let mut numbers = IndexMap::new();
        for module in requires.get(patch.module.as_str()).copied().unwrap_or_default().iter().rev() {
            numbers.extend(given.get(module).into_iter().flatten().map(|(name, value)| (name.clone(), *value)));
        }
        numbers.extend(given.get(&patch.module).into_iter().flatten().map(|(name, value)| (name.clone(), *value)));
        patch.numbers = numbers;
    }
    Ok(Plan { scripts, patches, numbers: given })
}

/// {script: indexes of the patches that change it}.
pub type ByScript = BTreeMap<String, Vec<usize>>;

/// The chosen mods' patches and the scripts each changes.
pub fn located(mod_list: &[ModTexts], tree: &Path) -> Result<(ByScript, Vec<Patch>)> {
    let mut scripts: Option<Scripts> = None;
    let mut by_script: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut patches = Vec::new();
    for module in mod_list {
        for (path, text) in &module.texts {
            let targets = targets_in(text)?;
            if targets.is_empty() {
                continue;
            }
            if scripts.is_none() {
                scripts = Some(Scripts::new(tree)?);
            }
            let scripts = scripts.as_ref().unwrap();
            let mut chosen = BTreeSet::new();
            for target in &targets {
                let found = scripts.picked(target)?;
                if found.is_empty() {
                    return Err(Error::invalid(format!(
                        "{}: #pragma target {} {} picks no script in {}",
                        path.display(),
                        target.kind,
                        target.values.join(" "),
                        tree.display()
                    )));
                }
                chosen.extend(found);
            }
            for script in &chosen {
                by_script.entry(script.clone()).or_default().push(patches.len());
            }
            patches.push(Patch {
                module: module.name.clone(),
                path: path.clone(),
                text: text.clone(),
                plain: chosen.iter().map(|script| (script.clone(), scripts.plain[script].clone())).collect(),
                scripts: chosen.into_iter().collect(),
                numbers: IndexMap::new(),
            });
        }
    }
    Ok((by_script, patches))
}

fn int_node(value: i64) -> Node {
    node!({"kind": "int", "value": value})
}

/// The patch as merged into one script: parsed with that script's LANGUAGE, its libraries
/// included (looked for from the patch's folder up, then in the tree, then in the SDK), with the
/// constants it alone knows: its own #defines and enums, LANGUAGE and the mod's numbers.
pub fn prepared(
    patch: &Patch,
    script: &str,
    tree: &Path,
    headers: &IndexMap<String, String>,
    overlay: Option<&IndexMap<String, String>>,
    sdk_dir: &Path,
) -> Result<(Node, IndexMap<String, Node>)> {
    let language = language_of(patch.plain.get(script).map_or(script, String::as_str));
    let mut defines: HashMap<String, String> =
        LANGUAGES.iter().map(|(name, value)| (name.to_string(), value.to_string())).collect();
    defines.insert("LANGUAGE".into(), language.to_string());
    let parsed = parse::parse_with(&patch.text, &defines)?;
    if parsed.get("asm").is_some() {
        return Err(Error::invalid("a patch cannot be assembly"));
    }
    if !parsed["main"].is_null() {
        return Err(Error::invalid("a patch has no MAIN; the script keeps its own"));
    }
    let texts = library::read_includes(&parsed, &patch.path, overlay, sdk_dir, &[tree.to_path_buf()])?;
    // the script's own libraries never declare globals; a mod's can
    let mut own_globals: Vec<Node> =
        list(&parsed, "globals").iter().map(|declared| declared.clone().with("source", Node::Null)).collect();
    for (path, text) in &texts {
        if let Ok(library_program) = parse::parse_with(text, &defines) {
            own_globals
                .extend(list(&library_program, "globals").iter().map(|declared| declared.clone().with("source", path)));
        }
    }
    let expanded = library::expand(parsed, &texts, headers)?;
    let mut consts: IndexMap<String, Node> = list(&expanded["declarations"], "constants")
        .iter()
        .map(|declared| (text(declared, "name").to_string(), declared["value"].clone()))
        .collect();
    for declared in list(&expanded["declarations"], "programs") {
        consts
            .entry(text(declared, "name").to_string())
            .or_insert_with(|| int_node(declared["no"].as_i64().unwrap_or(0)));
    }
    for (name, value) in LANGUAGES {
        consts.insert(name.to_string(), int_node(value));
    }
    consts.insert("LANGUAGE".into(), int_node(language));
    for (name, value) in &patch.numbers {
        consts.insert(name.clone(), int_node(*value));
    }
    Ok((expanded.with("globals", own_globals), consts))
}

fn fresh_name(base: &str, taken: &mut HashSet<String>) -> String {
    let mut name = base.to_string();
    let mut number = 2;
    while taken.contains(&name) {
        name = format!("{base}_{number}");
        number += 1;
    }
    taken.insert(name.clone());
    name
}

fn mod_identifier(module: &str) -> String {
    module.chars().map(|c| if c2_py::is_word(c) { c } else { '_' }).collect()
}

fn renamed(
    node: &Node,
    calls: &IndexMap<String, String>,
    variables: &IndexMap<String, String>,
    shadowed: &HashSet<String>,
) -> Node {
    match node {
        Node::List(items) => Node::List(items.iter().map(|item| renamed(item, calls, variables, shadowed)).collect()),
        Node::Map(map) => {
            let mut out = Node::Map(
                map.entries().map(|(key, value)| (key.clone(), renamed(value, calls, variables, shadowed))).collect(),
            );
            let name = text(node, "name");
            if node.kind() == "call" {
                if let Some(to) = calls.get(name) {
                    out["name"] = to.as_str().into();
                }
            }
            if matches!(node.kind(), "var" | "index") && !shadowed.contains(name) {
                if let Some(to) = variables.get(name) {
                    out["name"] = to.as_str().into();
                }
            }
            out
        }
        other => other.clone(),
    }
}

fn own_names(function: &Node) -> HashSet<String> {
    list(function, "params")
        .iter()
        .chain(list(function, "locals"))
        .map(|declared| text(declared, "name").to_string())
        .collect()
}

/// A patch function ready to join the script: the patch's constants filled in, its renamed
/// functions and globals followed, and original() pointed at before when it is an override.
fn joined(
    function: &Node,
    consts: &IndexMap<String, Node>,
    calls: &IndexMap<String, String>,
    variables: &IndexMap<String, String>,
    global_names: &HashSet<String>,
    module: &str,
    before: Option<&str>,
) -> Result<Node> {
    let shadowed = own_names(function);
    let hidden: HashSet<String> = shadowed.union(global_names).cloned().collect();
    let body = sdk::with_constants(&function["body"], consts, &hidden);
    let mut calls = calls.clone();
    if let Some(before) = before {
        calls.insert(ORIGINAL.into(), before.into());
    } else if names::callees(&body).iter().any(|name| name == ORIGINAL) {
        return Err(Error::invalid(format!(
            "{} calls original(), which only a #pragma override function can",
            text(function, "name")
        )));
    }
    let locals: Vec<Node> = list(function, "locals")
        .iter()
        .map(|declared| match declared["size"].as_str().and_then(|size| consts.get(size)) {
            Some(value) if value.kind() == "int" => declared.clone().with("size", value["value"].clone()),
            _ => declared.clone(),
        })
        .collect();
    let name = calls.get(text(function, "name")).cloned().unwrap_or_else(|| text(function, "name").to_string());
    let mut out = function
        .clone()
        .with("name", name)
        .with("locals", locals)
        .with("body", renamed(&body, &calls, variables, &shadowed))
        .with("padding", Node::Null)
        .with("unused_strings", Node::List(Vec::new()))
        .with("mod", module);
    if let Some(map) = out.as_object_mut() {
        for key in ["override", "every_frame", "library"] {
            map.remove(key);
        }
    }
    Ok(out)
}

fn program_number(number: &Node, numbering: &HashMap<String, i64>, consts: &IndexMap<String, Node>) -> Option<i64> {
    if let Node::Int(value) = number {
        return Some(*value);
    }
    let name = number.as_str()?;
    if let Some(value) = numbering.get(name) {
        return Some(*value);
    }
    consts.get(name).filter(|value| value.kind() == "int").and_then(|value| value["value"].as_i64())
}

fn parameter_count(function: &Node) -> usize {
    if function.kind() == "asm" {
        function["arg"].as_i64().unwrap_or(0) as usize
    } else {
        list(function, "params").len()
    }
}

/// What earlier patches did to a script: the names they added, the libraries they brought (under
/// the names they were given) and the functions they override.
#[derive(Clone, Debug, Default)]
pub struct Done {
    added: HashSet<String>,
    libraries: HashMap<String, Brought>,
    overrides: HashMap<String, Vec<String>>,
    every_frame: Vec<String>,
}

#[derive(Clone, Debug, Default)]
struct Brought {
    calls: IndexMap<String, String>,
    variables: IndexMap<String, String>,
}

/// The script program with one patch merged in. program has had its libraries included; patch and
/// consts come from prepared.
pub fn merged(
    program: &Node,
    done: &mut Done,
    patch: &Node,
    consts: &IndexMap<String, Node>,
    module: &str,
) -> Result<Node> {
    if truthy(&patch["natives"]) && truthy(&program["natives"]) && !patch["natives"].py_eq(&program["natives"]) {
        return Err(Error::invalid(format!(
            "the patch uses {} natives, but the script is an {} script",
            py_str(&patch["natives"]),
            py_str(&program["natives"])
        )));
    }
    let mut functions: Vec<Node> = list(program, "functions").to_vec();
    let script_libraries: HashSet<&str> =
        functions.iter().filter_map(|function| function["library"].as_str()).collect();
    let function_names: HashSet<String> = functions.iter().map(|function| text(function, "name").to_string()).collect();
    let global_names: HashSet<String> =
        list(program, "globals").iter().map(|declared| text(declared, "name").to_string()).collect();
    let mut taken: HashSet<String> = function_names.union(&global_names).cloned().collect();
    let mut calls: IndexMap<String, String> = IndexMap::new();
    let mut variables: IndexMap<String, String> = IndexMap::new();
    let mut mine: Vec<&Node> = Vec::new();
    let mut libraries: IndexMap<String, Brought> = IndexMap::new();
    for function in list(patch, "functions") {
        if function.kind() != "function" {
            return Err(Error::invalid(format!("a patch cannot hold asm functions ({})", text(function, "name"))));
        }
        let source = function["library"].as_str();
        if source.is_some_and(|source| script_libraries.contains(source)) {
            continue;
        }
        if let Some(brought) = source.and_then(|source| done.libraries.get(source)) {
            // an earlier mod brought this library in; use its copy, under the names it was given
            calls.extend(brought.calls.iter().map(|(from, to)| (from.clone(), to.clone())));
            continue;
        }
        mine.push(function);
        if let Some(source) = source {
            libraries.entry(source.to_string()).or_default();
        }
    }
    let is_override = |function: &Node| truthy(&function["override"]);
    for function in &mine {
        let name = text(function, "name");
        if is_override(function) || !taken.contains(name) {
            continue;
        }
        if !done.added.contains(name) {
            return Err(Error::invalid(format!(
                "the script already has {name}; put #pragma override before it to replace it, or rename it"
            )));
        }
        let fresh = fresh_name(&format!("{}_{}", name, mod_identifier(module)), &mut taken);
        calls.insert(name.to_string(), fresh);
    }
    for function in &mine {
        if !is_override(function) {
            let name = text(function, "name");
            taken.insert(calls.get(name).cloned().unwrap_or_else(|| name.to_string()));
        }
    }
    let mut added_globals = Vec::new();
    for declared in list(patch, "globals") {
        let name = text(declared, "name").to_string();
        let source = declared["source"].as_str();
        if let Some(brought) = source.and_then(|source| done.libraries.get(source)) {
            variables.extend(brought.variables.iter().map(|(from, to)| (from.clone(), to.clone())));
            continue;
        }
        if global_names.contains(&name) && !done.added.contains(&name) {
            return Err(Error::invalid(format!("the script already has a global {name}; rename the patch's")));
        }
        if taken.contains(&name) {
            let fresh = fresh_name(&format!("{}_{}", name, mod_identifier(module)), &mut taken);
            variables.insert(name.clone(), fresh);
        } else {
            taken.insert(name.clone());
        }
        let given = variables.get(&name).cloned().unwrap_or_else(|| name.clone());
        if let Some(source) = source {
            libraries.entry(source.to_string()).or_default().variables.insert(name.clone(), given.clone());
        }
        let mut out = declared.clone().with("name", given);
        if let Some(map) = out.as_object_mut() {
            map.remove("source");
        }
        added_globals.push(out);
    }
    let mut all_globals = global_names.clone();
    all_globals.extend(list(patch, "globals").iter().map(|declared| text(declared, "name").to_string()));
    for function in &mine {
        if let Some(source) = function["library"].as_str().filter(|_| !is_override(function)) {
            let name = text(function, "name");
            let given = calls.get(name).cloned().unwrap_or_else(|| name.to_string());
            libraries.get_mut(source).unwrap().calls.insert(name.to_string(), given);
        }
    }
    let mut position: HashMap<String, usize> =
        functions.iter().enumerate().map(|(number, function)| (text(function, "name").to_string(), number)).collect();
    let mut appended = Vec::new();
    for function in &mine {
        let name = text(function, "name");
        if truthy(&function["every_frame"]) {
            if !list(function, "params").is_empty() || is_override(function) {
                return Err(Error::invalid(format!(
                    "#pragma every_frame {name}: an every-frame function takes no parameters and overrides nothing"
                )));
            }
            done.every_frame.push(calls.get(name).cloned().unwrap_or_else(|| name.to_string()));
        }
        if !is_override(function) {
            appended.push(joined(function, consts, &calls, &variables, &all_globals, module, None)?);
            done.added.insert(calls.get(name).cloned().unwrap_or_else(|| name.to_string()));
            continue;
        }
        let Some(&at) = position.get(name) else {
            let mut known: Vec<String> = position.keys().cloned().collect();
            known.sort();
            let close = c2_py::difflib::get_close_matches(name, &known, 3, 0.6);
            let hint = if close.is_empty() {
                String::new()
            } else {
                format!("; did the definitions rename it? close: {}", close.join(", "))
            };
            return Err(Error::invalid(format!("#pragma override {name}: the script has no function {name}{hint}")));
        };
        let old = &functions[at];
        if parameter_count(old) != list(function, "params").len() {
            return Err(Error::invalid(format!(
                "override {} takes {} parameters; the script's {} takes {}",
                name,
                list(function, "params").len(),
                name,
                parameter_count(old)
            )));
        }
        let earlier = done.overrides.get(name).cloned().unwrap_or_default();
        let chains = names::callees(&function["body"]).iter().any(|callee| callee == ORIGINAL);
        if !earlier.is_empty() && !chains {
            return Err(Error::invalid(format!(
                "{} replaces {}, which mod {} overrides too; call original() in one of them to run both",
                module,
                name,
                earlier.join(", ")
            )));
        }
        if chains {
            let before = fresh_name(&format!("{}_before_{}", name, mod_identifier(module)), &mut taken);
            functions[at] = old.clone().with("name", before.as_str());
            position.remove(name);
            position.insert(before.clone(), at);
            appended.push(joined(function, consts, &calls, &variables, &all_globals, module, Some(&before))?);
        } else {
            functions[at] = joined(function, consts, &calls, &variables, &all_globals, module, None)?;
        }
        let mut owners = earlier;
        owners.push(module.to_string());
        done.overrides.insert(name.to_string(), owners);
    }
    let mut numbering: HashMap<String, i64> = HashMap::new();
    for declared in list(&program["declarations"], "programs").iter().chain(list(&patch["declarations"], "programs")) {
        numbering.insert(text(declared, "name").to_string(), declared["no"].as_i64().unwrap_or(0));
    }
    let mut present: HashMap<Option<i64>, String> = HashMap::new();
    for entry in list(program, "programs") {
        present.insert(program_number(&entry["no"], &numbering, &IndexMap::new()), py_str(&entry["function"]));
    }
    let mut programs: Vec<Node> = list(program, "programs").to_vec();
    for entry in list(patch, "programs") {
        let (written, function) = (py_str(&entry["no"]), py_str(&entry["function"]));
        let Some(number) = program_number(&entry["no"], &numbering, consts) else {
            return Err(Error::invalid(format!(
                "PROGRAM({written}, {function}): {written} is not a program number or name"
            )));
        };
        if let Some(runner) = present.get(&Some(number)) {
            return Err(Error::invalid(format!(
                "PROGRAM({written}, {function}): the script already has program {written}, run by {runner}; override {runner} instead"
            )));
        }
        present.insert(Some(number), function.clone());
        let given = calls.get(&function).cloned().unwrap_or(function);
        programs.push(node!({"no": number, "function": given}));
    }
    let mut globals: Vec<Node> = list(program, "globals").to_vec();
    let global_num = program["global_num"].as_i64().unwrap_or(0).max(0) as usize;
    while globals.len() < global_num {
        // slots the script counts but never names: keep them clear of the patch's
        globals.push(node!({"name": format!("unnamed_global_{}", globals.len()), "type": "int"}));
    }
    done.added.extend(added_globals.iter().map(|declared| text(declared, "name").to_string()));
    globals.extend(added_globals);
    done.libraries.extend(libraries);
    let count = globals.len();
    functions.extend(appended);
    let natives = if truthy(&program["natives"]) { program["natives"].clone() } else { patch["natives"].clone() };
    Ok(program
        .clone()
        .with("functions", functions)
        .with("programs", programs)
        .with("globals", globals)
        .with("global_num", count as i64)
        .with("declarations", merged_declarations(&program["declarations"], &patch["declarations"]))
        .with("natives", natives))
}

fn merged_declarations(script: &Node, patch: &Node) -> Node {
    let mut out = script.clone();
    if out.is_null() {
        out = Node::Map(Map::new());
    }
    for key in ["natives", "inlines", "programs"] {
        let have: HashSet<&str> = list(script, key).iter().map(|declared| text(declared, "name")).collect();
        let mut joined: Vec<Node> = list(script, key).to_vec();
        joined.extend(list(patch, key).iter().filter(|declared| !have.contains(text(declared, "name"))).cloned());
        out[key] = Node::List(joined);
    }
    let mut kinds = patch["kinds"].as_object().cloned().unwrap_or_default();
    if let Some(own) = script["kinds"].as_object() {
        for (key, value) in own.iter() {
            kinds.insert(key.to_string(), value.clone());
        }
    }
    out["kinds"] = Node::Map(kinds);
    out
}

/// The script's program with every patch that targets it merged in, in mod order.
pub fn applied(
    program: Node,
    patches: &[&Patch],
    script: &str,
    tree: &Path,
    headers: &IndexMap<String, String>,
    overlay: Option<&IndexMap<String, String>>,
    sdk_dir: &Path,
) -> Result<Node> {
    if program.get("asm").is_some() {
        return Err(Error::invalid("a patch cannot change a script kept as assembly"));
    }
    let mut program = program;
    let mut done = Done::default();
    for patch in patches {
        let step = prepared(patch, script, tree, headers, overlay, sdk_dir)
            .and_then(|(parsed, consts)| merged(&program, &mut done, &parsed, &consts, &patch.module));
        program =
            step.map_err(|error| Error::invalid(format!("{} (mod {}): {}", patch.file_name(), patch.module, error)))?;
    }
    with_frame_calls(program, &done.every_frame)
}

/// A call to each #pragma every_frame function before every wait; in the script, so it runs once
/// a frame whatever the script is waiting in. The functions themselves, and what they call, are
/// left alone, and must not wait.
fn with_frame_calls(program: Node, hooks: &[String]) -> Result<Node> {
    if hooks.is_empty() {
        return Ok(program);
    }
    let by_name: HashMap<&str, &Node> =
        list(&program, "functions").iter().map(|function| (text(function, "name"), function)).collect();
    let mut exempt: HashSet<String> = HashSet::new();
    let mut pending: Vec<String> = hooks.to_vec();
    while let Some(name) = pending.pop() {
        let Some(function) = by_name.get(name.as_str()).filter(|_| !exempt.contains(&name)) else {
            continue;
        };
        exempt.insert(name.clone());
        if function.kind() == "function" {
            if waits(&function["body"]) {
                let what =
                    if hooks.contains(&name) { "an every_frame function" } else { "an every_frame function's callee" };
                return Err(Error::invalid(format!(
                    "{name} waits, so it cannot run every frame (it is {what} or called by one)"
                )));
            }
            pending.extend(names::callees(&function["body"]));
        }
    }
    let calls: Vec<Node> = hooks
        .iter()
        .map(|hook| node!({"kind": "expr", "expr": {"kind": "call", "name": hook.as_str(), "args": []}}))
        .collect();
    let functions: Vec<Node> = list(&program, "functions")
        .iter()
        .map(|function| {
            if function.kind() == "function" && !exempt.contains(text(function, "name")) {
                function.clone().with("body", before_waits(&function["body"], &calls))
            } else {
                function.clone()
            }
        })
        .collect();
    Ok(program.with("functions", functions))
}

fn waits(node: &Node) -> bool {
    match node {
        Node::List(items) => items.iter().any(waits),
        Node::Map(map) => node.kind() == "wait" || map.values().any(waits),
        _ => false,
    }
}

fn before_waits(node: &Node, calls: &[Node]) -> Node {
    match node {
        Node::List(items) => {
            let mut out = Vec::new();
            for item in items {
                if item.kind() == "wait" {
                    out.extend(calls.iter().map(|call| call.clone().with("line", item["line"].clone())));
                }
                out.push(before_waits(item, calls));
            }
            Node::List(out)
        }
        Node::Map(map) => {
            Node::Map(map.entries().map(|(key, value)| (key.clone(), before_waits(value, calls))).collect())
        }
        other => other.clone(),
    }
}

/// The source text of a function, with the doc comment above it, or None.
pub fn function_text(text: &str, name: &str) -> Option<String> {
    let lines: Vec<&str> = text.split('\n').collect();
    let head = Regex::new(&format!(r"^(?:void|int|float|string)\s+\**{}\s*\(", regex::escape(name))).unwrap();
    let number = lines.iter().position(|line| head.is_match(line))?;
    let mut start = number;
    while start > 0 && lines[start - 1].starts_with("//") {
        start -= 1;
    }
    let end = (number..lines.len()).find(|later| lines[*later] == "}")?;
    Some(lines[start..=end].join("\n") + "\n")
}

/// {name: source text} of the wanted functions a tree script defines or gets from its libraries.
pub fn function_texts(tree: &Path, script: &str, wanted: &[String]) -> Result<IndexMap<String, String>> {
    let mut found = IndexMap::new();
    let mut seen = HashSet::new();
    let mut pending = std::collections::VecDeque::from([script.to_string()]);
    while found.len() < wanted.len() {
        let Some(path) = pending.pop_front() else {
            break;
        };
        if seen.contains(&path) || !tree.join(&path).is_file() {
            continue;
        }
        seen.insert(path.clone());
        let text = c2_py::read_text(&tree.join(&path))?;
        for name in wanted {
            if !found.contains_key(name) {
                if let Some(piece) = function_text(&text, name) {
                    found.insert(name.clone(), piece);
                }
            }
        }
        pending.extend(index::include_pattern().captures_iter(&text).map(|capture| capture[1].to_string()));
    }
    Ok(found)
}

fn language_name(language: i64) -> &'static str {
    LANGUAGES.iter().find(|(_, value)| *value == language).map_or("", |(name, _)| name)
}

/// One text for a function whose copies differ between a map's languages ({language: text},
/// English first): where the copies line up, the lines that differ get an #if LANGUAGE branch
/// each; where they do not, each language gets its whole copy.
pub fn language_text(copies: &BTreeMap<i64, String>) -> String {
    let mut order: Vec<i64> = copies.keys().copied().collect();
    order.sort_by_key(|language| (*language != 1, *language));
    if order.iter().map(|language| &copies[language]).collect::<HashSet<_>>().len() == 1 {
        return copies[&order[0]].clone();
    }
    let split: HashMap<i64, Vec<&str>> = order
        .iter()
        .map(|language| (*language, copies[language].trim_end_matches('\n').split('\n').collect()))
        .collect();
    let branches = |pieces: &HashMap<i64, Vec<&str>>| -> Vec<String> {
        let mut lines = Vec::new();
        for (number, language) in order[1..].iter().enumerate() {
            lines.push(format!(
                "#{} LANGUAGE == {}",
                if number == 0 { "if" } else { "elif" },
                language_name(*language)
            ));
            lines.extend(pieces[language].iter().map(|line| line.to_string()));
        }
        lines.push("#else".into());
        lines.extend(pieces[&order[0]].iter().map(|line| line.to_string()));
        lines.push("#endif".into());
        lines
    };
    if split.values().map(Vec::len).collect::<HashSet<_>>().len() != 1 {
        return branches(&split).join("\n") + "\n";
    }
    let mut out: Vec<String> = Vec::new();
    for (number, _) in split[&order[0]].iter().enumerate() {
        let found: HashMap<i64, Vec<&str>> =
            order.iter().map(|language| (*language, vec![split[language][number]])).collect();
        if found.values().collect::<HashSet<_>>().len() == 1 {
            out.push(found[&order[0]][0].to_string());
        } else {
            out.extend(branches(&found));
        }
    }
    out.join("\n") + "\n"
}

/// The start of a new patch file: what it is, its target and the headers the script includes.
pub fn patch_header(module: &str, kind: &str, value: &str, script_text: &str) -> String {
    static CELL: OnceLock<Regex> = OnceLock::new();
    let system = regex(&CELL, r"^#include\s*<");
    let mut lines = vec![
        format!("// Mod {module}: a patch, merged into the scripts below when the mod is built. A function after"),
        "// #pragma override replaces the script's own, and original() inside it runs the one it replaced;".into(),
        "// other functions, globals and PROGRAM lines are added.".into(),
        format!("#pragma target {kind} {value}"),
        String::new(),
    ];
    lines.extend(script_text.split('\n').filter(|line| system.is_match(line)).map(str::to_string));
    lines.join("\n") + "\n"
}

/// One line per patch for mod list and build: what it targets, overrides, adds and asks for.
pub fn summary(patch: &Patch) -> String {
    static OVERRIDES: OnceLock<Regex> = OnceLock::new();
    static EVERY_FRAME: OnceLock<Regex> = OnceLock::new();
    static PROGRAMS: OnceLock<Regex> = OnceLock::new();
    let overrides = regex(&OVERRIDES, r"#[ \t]*pragma[ \t]+override\s+(?:\w+\s+)*?(\w+)\s*\(");
    let every_frame = regex(&EVERY_FRAME, r"#[ \t]*pragma[ \t]+every_frame\s+(?:\w+\s+)*?(\w+)\s*\(");
    let programs = regex(&PROGRAMS, r"(?m)^PROGRAM\(\s*([^,]+?)\s*,\s*(\w+)\s*\);");
    let text = index::without_comments(&patch.text);
    let first =
        |pattern: &Regex| -> Vec<String> { pattern.captures_iter(&text).map(|found| found[1].to_string()).collect() };
    let (overridden, every, added) = (first(overrides), first(every_frame), first(programs));
    let count = patch.scripts.len();
    let mut parts = vec![format!("{} script{}", count, if count == 1 { "" } else { "s" })];
    if !overridden.is_empty() {
        parts.push(format!("overrides {}", overridden.join(", ")));
    }
    if !every.is_empty() {
        parts.push(format!("runs {} every frame", every.join(", ")));
    }
    if !added.is_empty() {
        parts.push(format!("adds program{} {}", if added.len() == 1 { "" } else { "s" }, added.join(", ")));
    }
    format!("{}: {}", patch.file_name(), parts.join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages() {
        assert_eq!(language_of("map/s/s51/s51_2.c"), 2);
        assert_eq!(language_of("map/s/s51/s51.c"), 0);
        assert_eq!(language_of("dungeon/monster/e33a.c"), -1);
    }

    #[test]
    fn language_branches() {
        let copies = BTreeMap::from([(1, "a\nb\n".to_string()), (2, "a\nc\n".to_string())]);
        assert_eq!(language_text(&copies), "a\n#if LANGUAGE == LANGUAGE_FRENCH\nc\n#else\nb\n#endif\n");
    }
}
